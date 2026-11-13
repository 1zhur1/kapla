//! Системные мини-инструменты: скриншот, ножницы, пипетка, блокировка, файлы полки.

use crate::clipboard;
use crate::log;
use arboard::ImageData;
use image::{ImageFormat, RgbaImage};
use serde::Serialize;
use std::borrow::Cow;
use std::os::windows::ffi::OsStringExt;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Manager};
use windows::core::{PCWSTR, PWSTR};
use windows::Win32::Foundation::POINT;
use windows::Win32::Graphics::Gdi::{
    BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC, GetDIBits,
    GetPixel, ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
    HGDIOBJ, SRCCOPY,
};
use windows::Win32::System::Shutdown::LockWorkStation;
use windows::Win32::UI::Controls::Dialogs::{
    GetOpenFileNameW, GetSaveFileNameW, OFN_ALLOWMULTISELECT, OFN_EXPLORER, OFN_FILEMUSTEXIST,
    OFN_NOCHANGEDIR, OFN_OVERWRITEPROMPT, OFN_PATHMUSTEXIST, OPENFILENAMEW,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_ALL,
    COINIT_APARTMENTTHREADED,
};
use windows::Win32::UI::Shell::{
    FileOpenDialog, IFileOpenDialog, FOS_FORCEFILESYSTEM, FOS_PICKFOLDERS, SIGDN_FILESYSPATH,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_ESCAPE, VK_LBUTTON};
use windows::Win32::UI::WindowsAndMessaging::{
    GetCursorPos, GetSystemMetrics, SM_CXSCREEN, SM_CXVIRTUALSCREEN, SM_CYSCREEN,
    SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
};

#[derive(Clone, Debug, Serialize)]
pub struct ShelfItem {
    pub path: String,
    pub name: String,
    pub size: u64,
    pub is_dir: bool,
}

#[tauri::command]
pub fn open_snip() -> Result<(), String> {
    Command::new("explorer.exe")
        .arg("ms-screenclip:")
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn lock_pc() -> Result<(), String> {
    unsafe { LockWorkStation().map_err(|e| e.to_string()) }
}

#[tauri::command]
pub async fn capture_screen(app: AppHandle) -> Result<String, String> {
    let app_bg = app.clone();
    let res = tauri::async_runtime::spawn_blocking(move || -> Result<(PathBuf, usize, usize, Vec<u8>), String> {
        unsafe {
            let mut vx = GetSystemMetrics(SM_XVIRTUALSCREEN);
            let mut vy = GetSystemMetrics(SM_YVIRTUALSCREEN);
            let mut vw = GetSystemMetrics(SM_CXVIRTUALSCREEN);
            let mut vh = GetSystemMetrics(SM_CYVIRTUALSCREEN);
            if vw <= 0 || vh <= 0 {
                vx = 0;
                vy = 0;
                vw = GetSystemMetrics(SM_CXSCREEN);
                vh = GetSystemMetrics(SM_CYSCREEN);
            }
            if vw <= 0 || vh <= 0 {
                return Err("не удалось определить размер экрана".into());
            }

            let screen_dc = GetDC(None);
            if screen_dc.is_invalid() {
                return Err("GetDC failed".into());
            }
            let mem_dc = CreateCompatibleDC(Some(screen_dc));
            if mem_dc.is_invalid() {
                ReleaseDC(None, screen_dc);
                return Err("CreateCompatibleDC failed".into());
            }
            let bmp = CreateCompatibleBitmap(screen_dc, vw, vh);
            if bmp.is_invalid() {
                let _ = DeleteDC(mem_dc);
                ReleaseDC(None, screen_dc);
                return Err("CreateCompatibleBitmap failed".into());
            }
            let old = SelectObject(mem_dc, HGDIOBJ(bmp.0));
            let ok = BitBlt(mem_dc, 0, 0, vw, vh, Some(screen_dc), vx, vy, SRCCOPY).is_ok();

            let mut bi = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: vw,
                    biHeight: -vh, // top-down DIB
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };

            let mut pixels = vec![0u8; (vw as usize) * (vh as usize) * 4];
            let lines = if ok {
                GetDIBits(
                    mem_dc,
                    bmp,
                    0,
                    vh as u32,
                    Some(pixels.as_mut_ptr().cast()),
                    &mut bi,
                    DIB_RGB_COLORS,
                )
            } else {
                0
            };

            SelectObject(mem_dc, old);
            let _ = DeleteObject(HGDIOBJ(bmp.0));
            let _ = DeleteDC(mem_dc);
            ReleaseDC(None, screen_dc);

            if !ok || lines == 0 {
                return Err("BitBlt / GetDIBits failed".into());
            }

            // BGRA -> RGBA (alpha = 255)
            for px in pixels.chunks_exact_mut(4) {
                px.swap(0, 2);
                px[3] = 255;
            }

            let dir = app_bg
                .path()
                .picture_dir()
                .or_else(|_| app_bg.path().app_data_dir())
                .map_err(|e| e.to_string())?
                .join("Kapla");
            std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            let secs = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let file = dir.join(format!("kapla-{secs}.png"));

            let img = RgbaImage::from_raw(vw as u32, vh as u32, pixels.clone())
                .ok_or("неверный буфер изображения")?;
            img.save_with_format(&file, ImageFormat::Png)
                .map_err(|e| e.to_string())?;

            let _ = arboard::Clipboard::new().and_then(|mut c| {
                c.set_image(ImageData {
                    width: vw as usize,
                    height: vh as usize,
                    bytes: Cow::Owned(pixels.clone()),
                })
            });

            Ok((file, vw as usize, vh as usize, pixels))
        }
    })
    .await
    .map_err(|e| e.to_string())??;

    clipboard::capture(&app, false);
    let path_str = res.0.to_string_lossy().into_owned();
    log::line(format!("screen captured -> {path_str} ({}x{})", res.1, res.2));
    Ok(path_str)
}

#[tauri::command]
pub async fn pick_color() -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(|| -> Result<Option<String>, String> {
        // Ждём отпускания кнопки после клика по кнопке пипетки в UI.
        for _ in 0..40 {
            let down = unsafe { (GetAsyncKeyState(VK_LBUTTON.0 as i32) as u16 & 0x8000) != 0 };
            if !down {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }

        // Ждём следующий клик левой кнопкой (или Esc для отмены), максимум 12 секунд.
        for _ in 0..800 {
            let esc = unsafe { (GetAsyncKeyState(VK_ESCAPE.0 as i32) as u16 & 0x8000) != 0 };
            if esc {
                return Ok(None);
            }
            let down = unsafe { (GetAsyncKeyState(VK_LBUTTON.0 as i32) as u16 & 0x8000) != 0 };
            if down {
                let mut pt = POINT::default();
                unsafe {
                    let _ = GetCursorPos(&mut pt);
                    let dc = GetDC(None);
                    if dc.is_invalid() {
                        return Err("GetDC failed".into());
                    }
                    let cr = GetPixel(dc, pt.x, pt.y).0;
                    ReleaseDC(None, dc);
                    if cr == u32::MAX {
                        return Err("точка вне экрана".into());
                    }
                    let r = (cr & 0xFF) as u8;
                    let g = ((cr >> 8) & 0xFF) as u8;
                    let b = ((cr >> 16) & 0xFF) as u8;
                    let hex = format!("#{r:02X}{g:02X}{b:02X}");
                    let _ = arboard::Clipboard::new().and_then(|mut c| c.set_text(hex.clone()));
                    return Ok(Some(hex));
                }
            }
            std::thread::sleep(Duration::from_millis(15));
        }
        Ok(None)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn open_path(path: String) -> Result<(), String> {
    Command::new("explorer.exe")
        .arg(&path)
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn reveal_path(path: String) -> Result<(), String> {
    let escaped = path.replace('"', "");
    Command::new("explorer.exe")
        .raw_arg(format!("/select,\"{escaped}\""))
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn shelf_meta(paths: Vec<String>) -> Vec<ShelfItem> {
    paths
        .into_iter()
        .filter_map(|p| {
            let pb = Path::new(&p);
            let meta = std::fs::metadata(pb).ok()?;
            let name = pb
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| p.clone());
            Some(ShelfItem {
                path: p,
                name,
                size: if meta.is_file() { meta.len() } else { 0 },
                is_dir: meta.is_dir(),
            })
        })
        .collect()
}

fn parse_ofn_multi_paths(buf: &[u16]) -> Vec<String> {
    let mut parts = Vec::new();
    let mut start = 0usize;
    for i in 0..buf.len() {
        if buf[i] == 0 {
            if i == start {
                break;
            }
            parts.push(
                std::ffi::OsString::from_wide(&buf[start..i])
                    .to_string_lossy()
                    .into_owned(),
            );
            start = i + 1;
        }
    }
    match parts.len() {
        0 => Vec::new(),
        1 => parts,
        _ => {
            let dir = Path::new(&parts[0]);
            parts[1..]
                .iter()
                .map(|f| dir.join(f).to_string_lossy().into_owned())
                .collect()
        }
    }
}

#[tauri::command]
pub async fn pick_files() -> Result<Vec<ShelfItem>, String> {
    let paths = tauri::async_runtime::spawn_blocking(|| -> Vec<String> {
        let mut buf = vec![0u16; 16 * 1024];
        let filter: Vec<u16> = "Все файлы (*.*)\0*.*\0\0".encode_utf16().collect();
        let title: Vec<u16> = "Добавить файлы на полку Kapla\0".encode_utf16().collect();
        let mut ofn = OPENFILENAMEW {
            lStructSize: std::mem::size_of::<OPENFILENAMEW>() as u32,
            lpstrFilter: PCWSTR(filter.as_ptr()),
            lpstrFile: PWSTR(buf.as_mut_ptr()),
            nMaxFile: buf.len() as u32,
            lpstrTitle: PCWSTR(title.as_ptr()),
            Flags: OFN_ALLOWMULTISELECT
                | OFN_EXPLORER
                | OFN_FILEMUSTEXIST
                | OFN_PATHMUSTEXIST
                | OFN_NOCHANGEDIR,
            ..Default::default()
        };
        if unsafe { GetOpenFileNameW(&mut ofn).as_bool() } {
            parse_ofn_multi_paths(&buf)
        } else {
            Vec::new()
        }
    })
    .await
    .map_err(|e| e.to_string())?;

    Ok(shelf_meta(paths))
}

#[tauri::command]
pub fn default_screenshots_dir(app: AppHandle) -> String {
    if let Ok(pic) = app.path().picture_dir() {
        return pic.join("Kapla").to_string_lossy().into_owned();
    }
    if let Ok(home) = app.path().home_dir() {
        return home.join("Pictures").join("Kapla").to_string_lossy().into_owned();
    }
    String::from("C:\\Kapla")
}

#[tauri::command]
pub async fn pick_save_image_path(
    default_name: String,
    initial_dir: Option<String>,
) -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(move || -> Option<String> {
        let mut buf = vec![0u16; 2048];
        let name_utf16: Vec<u16> = default_name.encode_utf16().collect();
        let copy_len = name_utf16.len().min(buf.len() - 1);
        buf[..copy_len].copy_from_slice(&name_utf16[..copy_len]);

        let filter: Vec<u16> = "PNG изображение (*.png)\0*.png\0JPEG изображение (*.jpg)\0*.jpg\0\0"
            .encode_utf16()
            .collect();
        let title: Vec<u16> = "Сохранить отредактированное изображение\0"
            .encode_utf16()
            .collect();
        let def_ext: Vec<u16> = "png\0".encode_utf16().collect();
        let init_dir_u16: Option<Vec<u16>> = initial_dir
            .filter(|s| !s.trim().is_empty())
            .map(|s| format!("{s}\0").encode_utf16().collect());

        let mut ofn = OPENFILENAMEW {
            lStructSize: std::mem::size_of::<OPENFILENAMEW>() as u32,
            lpstrFilter: PCWSTR(filter.as_ptr()),
            lpstrFile: PWSTR(buf.as_mut_ptr()),
            nMaxFile: buf.len() as u32,
            lpstrInitialDir: init_dir_u16
                .as_ref()
                .map(|v| PCWSTR(v.as_ptr()))
                .unwrap_or(PCWSTR::null()),
            lpstrTitle: PCWSTR(title.as_ptr()),
            lpstrDefExt: PCWSTR(def_ext.as_ptr()),
            Flags: OFN_EXPLORER | OFN_OVERWRITEPROMPT | OFN_PATHMUSTEXIST | OFN_NOCHANGEDIR,
            ..Default::default()
        };
        if unsafe { GetSaveFileNameW(&mut ofn).as_bool() } {
            let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
            Some(
                std::ffi::OsString::from_wide(&buf[..end])
                    .to_string_lossy()
                    .into_owned(),
            )
        } else {
            None
        }
    })
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn pick_folder() -> Result<Option<String>, String> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let res = unsafe {
            let hr = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            let out = (|| -> Option<String> {
                let dlg: IFileOpenDialog =
                    CoCreateInstance(&FileOpenDialog, None, CLSCTX_ALL).ok()?;
                let opts = dlg.GetOptions().ok()?;
                dlg.SetOptions(opts | FOS_PICKFOLDERS | FOS_FORCEFILESYSTEM).ok()?;
                let title = windows::core::HSTRING::from("Выбери папку для сохранения картинок Kapla");
                let _ = dlg.SetTitle(&title);
                if dlg.Show(None).is_err() {
                    return None;
                }
                let item = dlg.GetResult().ok()?;
                let raw = item.GetDisplayName(SIGDN_FILESYSPATH).ok()?;
                let s = raw.to_string().ok();
                CoTaskMemFree(Some(raw.0 as *const _));
                s
            })();
            if hr.is_ok() {
                CoUninitialize();
            }
            out
        };
        let _ = tx.send(res);
    });
    tauri::async_runtime::spawn_blocking(move || rx.recv().unwrap_or(None))
        .await
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_single_and_multi_ofn_paths() {
        let single: Vec<u16> = "C:\\Temp\\note.txt\0\0".encode_utf16().collect();
        assert_eq!(parse_ofn_multi_paths(&single), vec![r"C:\Temp\note.txt"]);

        let multi: Vec<u16> = "C:\\Temp\0a.png\0b.pdf\0\0".encode_utf16().collect();
        assert_eq!(
            parse_ofn_multi_paths(&multi),
            vec![r"C:\Temp\a.png", r"C:\Temp\b.pdf"]
        );
    }
}
