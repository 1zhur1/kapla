//! Надёжный Drag-and-Drop для Полки: приём файлов поверх дочерних HWND WebView2,
//! вытягивание файлов с Полки (OLE DoDragDrop) и копирование файлов в буфер (CF_HDROP).

use crate::log;
use parking_lot::Mutex;
use std::cell::UnsafeCell;
use std::collections::HashSet;
use std::ffi::OsString;
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::path::PathBuf;
use std::ptr;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use tauri::{AppHandle, Emitter, Manager};
use windows::Win32::Foundation::{
    DRAGDROP_S_CANCEL, DRAGDROP_S_DROP, DRAGDROP_S_USEDEFAULTCURSORS, E_FAIL, GlobalFree, HANDLE,
    HGLOBAL, HWND, LPARAM, POINT, POINTL, S_OK,
};
use windows::Win32::Graphics::Gdi::ScreenToClient;
use windows::Win32::System::Com::{
    CoTaskMemFree, DVASPECT_CONTENT, FORMATETC, IDataObject, TYMED_HGLOBAL,
};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
};
use windows::Win32::System::Memory::{
    GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE, GMEM_ZEROINIT,
};
use windows::Win32::System::Ole::{
    DoDragDrop, IDropSource, IDropSource_Impl, IDropTarget, IDropTarget_Impl, RegisterDragDrop,
    RevokeDragDrop, CF_HDROP, DROPEFFECT, DROPEFFECT_COPY, DROPEFFECT_LINK, DROPEFFECT_MOVE,
    DROPEFFECT_NONE,
};
use windows::Win32::System::SystemServices::{MK_LBUTTON, MODIFIERKEYS_FLAGS};
use windows::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture;
use windows::Win32::UI::Shell::{
    BHID_DataObject, DragFinish, DragQueryFileW, IShellItem, IShellItemArray,
    SHCreateItemFromParsingName, SHCreateShellItemArrayFromDataObject, DROPFILES, HDROP,
    SIGDN_FILESYSPATH,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumChildWindows, GetClassNameW, GetParent, GetPropW, IsChild,
};
use windows::core::{implement, w, BOOL, HRESULT, HSTRING, Ref, Result as WinResult};

static OUT_DRAGGING: AtomicBool = AtomicBool::new(false);
static MAIN_WRY_HWND: AtomicIsize = AtomicIsize::new(0);
static HOOKED_HWNDS: Mutex<Option<HashSet<isize>>> = Mutex::new(None);

pub fn is_out_dragging() -> bool {
    OUT_DRAGGING.load(Ordering::Relaxed)
}

#[implement(IDropSource)]
struct ShelfDropSource;

#[allow(non_snake_case)]
impl IDropSource_Impl for ShelfDropSource_Impl {
    fn QueryContinueDrag(
        &self,
        fescapepressed: BOOL,
        grfkeystate: MODIFIERKEYS_FLAGS,
    ) -> HRESULT {
        if fescapepressed.as_bool() {
            return DRAGDROP_S_CANCEL;
        }
        if (grfkeystate & MK_LBUTTON) == MODIFIERKEYS_FLAGS(0) {
            return DRAGDROP_S_DROP;
        }
        S_OK
    }

    fn GiveFeedback(&self, _dweffect: DROPEFFECT) -> HRESULT {
        DRAGDROP_S_USEDEFAULTCURSORS
    }
}

enum DropSrc {
    HDrop(HDROP),
    ShellItems,
}

#[implement(IDropTarget)]
struct ShelfDropTarget {
    hwnd: HWND,
    app: AppHandle,
    cursor_effect: UnsafeCell<DROPEFFECT>,
    valid_enter: UnsafeCell<bool>,
}

impl ShelfDropTarget {
    fn new(hwnd: HWND, app: AppHandle) -> Self {
        Self {
            hwnd,
            app,
            cursor_effect: UnsafeCell::new(DROPEFFECT_NONE),
            valid_enter: UnsafeCell::new(false),
        }
    }

    unsafe fn collect_paths(
        data_obj: Ref<'_, IDataObject>,
        out: &mut Vec<String>,
    ) -> Option<DropSrc> {
        let Some(obj) = data_obj.as_ref() else {
            return None;
        };
        let fmt = FORMATETC {
            cfFormat: CF_HDROP.0,
            ptd: ptr::null_mut(),
            dwAspect: DVASPECT_CONTENT.0,
            lindex: -1,
            tymed: TYMED_HGLOBAL.0 as u32,
        };
        if let Ok(medium) = unsafe { obj.GetData(&fmt) } {
            let hdrop = HDROP(unsafe { medium.u.hGlobal.0 } as _);
            let count = unsafe { DragQueryFileW(hdrop, 0xFFFFFFFF, None) };
            for i in 0..count {
                let len = unsafe { DragQueryFileW(hdrop, i, None) } as usize;
                let mut buf = vec![0u16; len + 1];
                unsafe { DragQueryFileW(hdrop, i, Some(&mut buf)) };
                let p: PathBuf = OsString::from_wide(&buf[..len]).into();
                out.push(p.to_string_lossy().into_owned());
            }
            return if out.is_empty() {
                unsafe { DragFinish(hdrop) };
                None
            } else {
                Some(DropSrc::HDrop(hdrop))
            };
        }

        if let Ok(items) = unsafe { SHCreateShellItemArrayFromDataObject::<_, IShellItemArray>(obj) }
        {
            let count = unsafe { items.GetCount() }.unwrap_or(0);
            for i in 0..count {
                let Ok(item) = (unsafe { items.GetItemAt(i) }) else {
                    continue;
                };
                let Ok(name) = (unsafe { item.GetDisplayName(SIGDN_FILESYSPATH) }) else {
                    continue;
                };
                let p: PathBuf = OsString::from_wide(unsafe { name.as_wide() }).into();
                unsafe { CoTaskMemFree(Some(name.0 as _)) };
                out.push(p.to_string_lossy().into_owned());
            }
            if !out.is_empty() {
                return Some(DropSrc::ShellItems);
            }
        }
        None
    }
}

#[allow(non_snake_case)]
impl IDropTarget_Impl for ShelfDropTarget_Impl {
    fn DragEnter(
        &self,
        pDataObj: Ref<'_, IDataObject>,
        _grfKeyState: MODIFIERKEYS_FLAGS,
        pt: &POINTL,
        pdwEffect: *mut DROPEFFECT,
    ) -> WinResult<()> {
        if is_out_dragging() {
            unsafe {
                *self.valid_enter.get() = false;
                *self.cursor_effect.get() = DROPEFFECT_NONE;
                if !pdwEffect.is_null() {
                    *pdwEffect = DROPEFFECT_NONE;
                }
            }
            return Ok(());
        }

        let mut cpt = POINT { x: pt.x, y: pt.y };
        let _ = unsafe { ScreenToClient(self.hwnd, &mut cpt) };

        let mut paths = Vec::new();
        let src = unsafe { ShelfDropTarget::collect_paths(pDataObj, &mut paths) };
        let valid = src.is_some();
        if let Some(DropSrc::HDrop(h)) = src {
            unsafe { DragFinish(h) };
        }

        let eff = if valid {
            DROPEFFECT_COPY
        } else {
            DROPEFFECT_NONE
        };
        unsafe {
            *self.valid_enter.get() = valid;
            *self.cursor_effect.get() = eff;
            if !pdwEffect.is_null() {
                *pdwEffect = eff;
            }
        }
        if valid {
            let _ = self.app.emit("shelf_drag", "enter");
        }
        Ok(())
    }

    fn DragOver(
        &self,
        _grfKeyState: MODIFIERKEYS_FLAGS,
        _pt: &POINTL,
        pdwEffect: *mut DROPEFFECT,
    ) -> WinResult<()> {
        unsafe {
            if !pdwEffect.is_null() {
                *pdwEffect = *self.cursor_effect.get();
            }
        }
        Ok(())
    }

    fn DragLeave(&self) -> WinResult<()> {
        if unsafe { *self.valid_enter.get() } {
            unsafe { *self.valid_enter.get() = false };
            let _ = self.app.emit("shelf_drag", "leave");
        }
        Ok(())
    }

    fn Drop(
        &self,
        pDataObj: Ref<'_, IDataObject>,
        _grfKeyState: MODIFIERKEYS_FLAGS,
        _pt: &POINTL,
        pdwEffect: *mut DROPEFFECT,
    ) -> WinResult<()> {
        let valid = unsafe { *self.valid_enter.get() };
        unsafe { *self.valid_enter.get() = false };
        if !valid || is_out_dragging() {
            unsafe {
                if !pdwEffect.is_null() {
                    *pdwEffect = DROPEFFECT_NONE;
                }
            }
            return Ok(());
        }

        let mut paths = Vec::new();
        let src = unsafe { ShelfDropTarget::collect_paths(pDataObj, &mut paths) };
        if let Some(DropSrc::HDrop(h)) = src {
            unsafe { DragFinish(h) };
        }

        let _ = self.app.emit("shelf_drag", "leave");
        if !paths.is_empty() {
            log::line(format!("shelf_dnd drop: {} file(s)", paths.len()));
            let _ = self.app.emit("shelf_drop", paths);
            unsafe {
                if !pdwEffect.is_null() {
                    *pdwEffect = DROPEFFECT_COPY;
                }
            }
        }
        Ok(())
    }
}

fn hwnd_class(hwnd: HWND) -> String {
    let mut buf = [0u16; 96];
    let n = unsafe { GetClassNameW(hwnd, &mut buf) };
    if n <= 0 {
        return String::new();
    }
    String::from_utf16_lossy(&buf[..n as usize])
}

/// Вешает `IDropTarget` на главное окно `island` и дочерние HWND его основного WebView2
/// (`WRY_WEBVIEW`, `Chrome_WidgetWin_0`, `Chrome_WidgetWin_1`, `Chrome_RenderWidgetHostHWND`),
/// не затрагивая встроенный `mini_browser`.
pub fn attach_drop_targets(app: &AppHandle) {
    let app_cb = app.clone();
    let _ = app.run_on_main_thread(move || {
        let Some(win) = app_cb.get_webview_window("island") else {
            return;
        };
        let Ok(raw) = win.hwnd() else {
            return;
        };
        let top = HWND(raw.0 as _);

        let mut all_children: Vec<HWND> = Vec::new();
        unsafe extern "system" fn enum_cb(hwnd: HWND, lparam: LPARAM) -> BOOL {
            let list = unsafe { &mut *(lparam.0 as *mut Vec<HWND>) };
            list.push(hwnd);
            BOOL(1)
        }
        let _ = unsafe {
            EnumChildWindows(
                Some(top),
                Some(enum_cb),
                LPARAM(&mut all_children as *mut Vec<HWND> as isize),
            )
        };

        let mut main_wry_raw = MAIN_WRY_HWND.load(Ordering::Relaxed);
        if main_wry_raw == 0 {
            for &ch in &all_children {
                if unsafe { GetParent(ch) }.ok() == Some(top) && hwnd_class(ch) == "WRY_WEBVIEW" {
                    main_wry_raw = ch.0 as isize;
                    MAIN_WRY_HWND.store(main_wry_raw, Ordering::Relaxed);
                    break;
                }
            }
        }
        let main_wry = if main_wry_raw != 0 {
            Some(HWND(main_wry_raw as _))
        } else {
            None
        };

        let mut targets = vec![top];
        for ch in all_children {
            let belongs = match main_wry {
                Some(mw) => ch == mw || unsafe { IsChild(mw, ch) }.as_bool(),
                None => true,
            };
            if belongs {
                targets.push(ch);
            }
        }

        let mut guard = HOOKED_HWNDS.lock();
        let hooked = guard.get_or_insert_with(HashSet::new);
        let mut newly = 0usize;

        for h in targets {
            let key = h.0 as isize;
            let has_prop = !unsafe { GetPropW(h, w!("OleDropTargetInterface")) }
                .0
                .is_null();
            if hooked.contains(&key) && has_prop {
                continue;
            }
            let _ = unsafe { RevokeDragDrop(h) };
            let target: IDropTarget = ShelfDropTarget::new(h, app_cb.clone()).into();
            if unsafe { RegisterDragDrop(h, &target) }.is_ok() {
                hooked.insert(key);
                newly += 1;
            }
        }

        if newly > 0 {
            log::line(format!(
                "shelf_dnd: hooked {newly} hwnd(s), total {}",
                hooked.len()
            ));
        }
    });
}

#[tauri::command]
pub fn shelf_prepare_drop(app: AppHandle) {
    attach_drop_targets(&app);
}

/// Собирает бинарный буфер Windows `DROPFILES` + UTF-16 пути для `CF_HDROP`.
pub fn build_dropfiles_bytes(paths: &[String]) -> Vec<u8> {
    let hdr_size = std::mem::size_of::<DROPFILES>();
    let mut out = Vec::with_capacity(hdr_size + 128);
    let hdr = DROPFILES {
        pFiles: hdr_size as u32,
        pt: POINT { x: 0, y: 0 },
        fNC: BOOL(0),
        fWide: BOOL(1),
    };
    let hdr_bytes = unsafe {
        std::slice::from_raw_parts(&hdr as *const DROPFILES as *const u8, hdr_size)
    };
    out.extend_from_slice(hdr_bytes);
    for p in paths {
        let trimmed = p.trim();
        if trimmed.is_empty() {
            continue;
        }
        for u in std::ffi::OsStr::new(trimmed).encode_wide() {
            out.extend_from_slice(&u.to_le_bytes());
        }
        out.extend_from_slice(&0u16.to_le_bytes());
    }
    out.extend_from_slice(&0u16.to_le_bytes());
    out
}

/// Копирует файлы в системный буфер обмена Windows как `CF_HDROP`
/// (их можно вставить по Ctrl+V в Проводник, на рабочий стол или в мессенджер).
#[tauri::command]
pub fn shelf_copy_files(paths: Vec<String>) -> Result<(), String> {
    let existing: Vec<String> = paths
        .into_iter()
        .filter(|p| std::path::Path::new(p).exists())
        .collect();
    if existing.is_empty() {
        return Err("Файл не найден на диске".into());
    }
    let bytes = build_dropfiles_bytes(&existing);
    unsafe {
        let hglob: HGLOBAL = GlobalAlloc(GMEM_MOVEABLE | GMEM_ZEROINIT, bytes.len())
            .map_err(|e| format!("GlobalAlloc: {e}"))?;
        let ptr = GlobalLock(hglob) as *mut u8;
        if ptr.is_null() {
            let _ = GlobalFree(Some(hglob));
            return Err("GlobalLock failed".into());
        }
        ptr::copy_nonoverlapping(bytes.as_ptr(), ptr, bytes.len());
        let _ = GlobalUnlock(hglob);

        if OpenClipboard(None).is_err() {
            let _ = GlobalFree(Some(hglob));
            return Err("Не удалось открыть буфер обмена".into());
        }
        let _ = EmptyClipboard();
        let res = SetClipboardData(CF_HDROP.0 as u32, Some(HANDLE(hglob.0)));
        let _ = CloseClipboard();
        if res.is_err() {
            let _ = GlobalFree(Some(hglob));
            return Err("SetClipboardData failed".into());
        }
    }
    log::line(format!("shelf copied {} file(s) as CF_HDROP", existing.len()));
    Ok(())
}

/// Запускает нативный Windows OLE `DoDragDrop` для вытягивания файла с Полки
/// на рабочий стол, в Проводник или любое приложение.
#[tauri::command]
pub async fn shelf_start_drag(app: AppHandle, path: String) -> Result<bool, String> {
    if !std::path::Path::new(&path).exists() {
        return Err("Файл не найден на диске".into());
    }
    let (tx, rx) = std::sync::mpsc::channel::<Result<bool, String>>();
    app.run_on_main_thread(move || {
        struct ResetFlag;
        impl Drop for ResetFlag {
            fn drop(&mut self) {
                OUT_DRAGGING.store(false, Ordering::SeqCst);
            }
        }
        OUT_DRAGGING.store(true, Ordering::SeqCst);
        let _reset = ResetFlag;

        let res = unsafe {
            let _ = ReleaseCapture();
            let item: IShellItem =
                match SHCreateItemFromParsingName(&HSTRING::from(&path), None) {
                    Ok(it) => it,
                    Err(e) => {
                        let _ = tx.send(Err(format!("SHCreateItem: {e}")));
                        return;
                    }
                };
            let data_obj: IDataObject = match item.BindToHandler(None, &BHID_DataObject) {
                Ok(d) => d,
                Err(e) => {
                    let _ = tx.send(Err(format!("BindToHandler: {e}")));
                    return;
                }
            };
            let drop_src: IDropSource = ShelfDropSource.into();
            let mut effect = DROPEFFECT_NONE;
            let hr = DoDragDrop(
                &data_obj,
                &drop_src,
                DROPEFFECT_COPY | DROPEFFECT_MOVE | DROPEFFECT_LINK,
                &mut effect,
            );
            if hr == E_FAIL {
                Err("DoDragDrop failed".into())
            } else {
                Ok(hr == DRAGDROP_S_DROP)
            }
        };
        let _ = tx.send(res);
    })
    .map_err(|e| format!("run_on_main_thread: {e}"))?;

    tauri::async_runtime::spawn_blocking(move || rx.recv().map_err(|_| "drag cancelled".to_string())?)
        .await
        .map_err(|e| format!("join: {e}"))?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dropfiles_layout_is_valid_utf16_double_null() {
        let buf = build_dropfiles_bytes(&[r"C:\a.txt".into(), r"D:\b.png".into()]);
        assert_eq!(std::mem::size_of::<DROPFILES>(), 20);
        // pFiles = 20, fWide = 1
        assert_eq!(u32::from_le_bytes(buf[0..4].try_into().unwrap()), 20);
        assert_eq!(u32::from_le_bytes(buf[16..20].try_into().unwrap()), 1);

        let u16s: Vec<u16> = buf[20..]
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        let expected: Vec<u16> = "C:\\a.txt\0D:\\b.png\0\0".encode_utf16().collect();
        assert_eq!(u16s, expected);
    }
}
