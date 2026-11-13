//! Окно острова: поверх всех, не крадёт фокус (`WS_EX_NOACTIVATE` + `WM_MOUSEACTIVATE`),
//! плавный click-through без пересоздания стилей `tao` и с возвратом фокуса для ввода текста.

use crate::log;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, State, WebviewWindow};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentProcessId, GetCurrentThreadId};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON};
use windows::Win32::UI::WindowsAndMessaging::{
    BringWindowToTop, CallWindowProcW, GetCursorPos, GetForegroundWindow, GetWindowLongPtrW,
    GetWindowThreadProcessId, SetForegroundWindow, SetWindowDisplayAffinity, SetWindowLongPtrW,
    SetWindowPos, GWL_EXSTYLE, GWLP_WNDPROC, HWND_TOPMOST, MA_NOACTIVATE, SWP_FRAMECHANGED,
    SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, WDA_EXCLUDEFROMCAPTURE, WDA_NONE, WM_MOUSEACTIVATE,
    WS_EX_APPWINDOW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TRANSPARENT,
    WNDPROC,
};

pub const WIN_W: f64 = 860.0;
pub const WIN_H: f64 = 540.0;

#[derive(Clone, Copy, Debug, Default, Deserialize)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Rect {
    fn contains(&self, x: f64, y: f64) -> bool {
        x >= self.x && x <= self.x + self.w && y >= self.y && y <= self.y + self.h
    }
}

pub struct WinState {
    hit: Mutex<Option<Rect>>,
    prev_fg: Mutex<isize>,
    pub input_mode: AtomicBool,
    pub through_mode: AtomicBool,
}

impl Default for WinState {
    fn default() -> Self {
        Self {
            hit: Mutex::new(Some(Rect { x: WIN_W / 2.0 - 110.0, y: 0.0, w: 220.0, h: 46.0 })),
            prev_fg: Mutex::new(0),
            input_mode: AtomicBool::new(false),
            through_mode: AtomicBool::new(false),
        }
    }
}

#[derive(Clone, Serialize)]
struct Cursor {
    x: f64,
    y: f64,
    edge: bool,
    near: bool,
    dragging: bool,
}

static ORIG_WNDPROC: AtomicIsize = AtomicIsize::new(0);
static INPUT_ACTIVE: AtomicBool = AtomicBool::new(false);
static BROWSER_ACTIVE: AtomicBool = AtomicBool::new(false);

unsafe extern "system" fn island_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if msg == WM_MOUSEACTIVATE
        && !INPUT_ACTIVE.load(Ordering::Relaxed)
        && !BROWSER_ACTIVE.load(Ordering::Relaxed)
    {
        return LRESULT(MA_NOACTIVATE as isize);
    }
    let orig = ORIG_WNDPROC.load(Ordering::Relaxed);
    if orig != 0 {
        let f: WNDPROC = std::mem::transmute(orig);
        return CallWindowProcW(f, hwnd, msg, wparam, lparam);
    }
    LRESULT(0)
}

fn hwnd_of(win: &WebviewWindow) -> tauri::Result<HWND> {
    Ok(HWND(win.hwnd()?.0 as _))
}

/// Ставит/убирает только бит `WS_EX_TRANSPARENT` без `ShowWindow` и без сброса флагов.
fn set_transparent_click(hwnd: HWND, ignore: bool) {
    unsafe {
        let mut ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        ex &= !(WS_EX_APPWINDOW.0 as isize);
        ex |= WS_EX_TOOLWINDOW.0 as isize | WS_EX_LAYERED.0 as isize;
        if !INPUT_ACTIVE.load(Ordering::Relaxed) && !BROWSER_ACTIVE.load(Ordering::Relaxed) {
            ex |= WS_EX_NOACTIVATE.0 as isize;
        } else {
            ex &= !(WS_EX_NOACTIVATE.0 as isize);
        }
        if ignore {
            ex |= WS_EX_TRANSPARENT.0 as isize;
        } else {
            ex &= !(WS_EX_TRANSPARENT.0 as isize);
        }
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex);
    }
}

pub fn apply_styles(win: &WebviewWindow) -> tauri::Result<()> {
    let hwnd = hwnd_of(win)?;
    unsafe {
        if ORIG_WNDPROC.load(Ordering::Relaxed) == 0 {
            let prev = SetWindowLongPtrW(hwnd, GWLP_WNDPROC, island_wnd_proc as *const () as isize);
            ORIG_WNDPROC.store(prev, Ordering::Relaxed);
        }
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) & !(WS_EX_APPWINDOW.0 as isize);
        SetWindowLongPtrW(
            hwnd,
            GWL_EXSTYLE,
            ex | WS_EX_TOOLWINDOW.0 as isize | WS_EX_NOACTIVATE.0 as isize | WS_EX_LAYERED.0 as isize,
        );
        let _ = SetWindowPos(
            hwnd,
            Some(HWND_TOPMOST),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_FRAMECHANGED,
        );
        if std::env::var_os("KAPLA_SHOW_IN_CAPTURE").is_none() {
            let _ = SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE);
        }
    }
    Ok(())
}

pub fn setup(win: &WebviewWindow) -> tauri::Result<()> {
    place(win)?;
    let _ = win.set_focusable(false);
    win.set_ignore_cursor_events(true)?;
    apply_styles(win)?;
    Ok(())
}

fn place(win: &WebviewWindow) -> tauri::Result<()> {
    let mon = match win.primary_monitor()? {
        Some(m) => Some(m),
        None => win.current_monitor()?,
    };
    if let Some(m) = mon {
        let s = m.scale_factor();
        let w = (WIN_W * s).round() as i32;
        let h = (WIN_H * s).round() as u32;
        win.set_size(PhysicalSize::new(w as u32, h))?;
        let pos = m.position();
        let size = m.size();
        let x = pos.x + (size.width as i32 - w) / 2;
        win.set_position(PhysicalPosition::new(x, pos.y))?;
        log::line(format!("placed window at ({x}, {}) size=({w}x{h}) scale={s}", pos.y));
    }
    Ok(())
}

fn is_own_window(hwnd: HWND, own_pid: u32) -> bool {
    if hwnd.0.is_null() {
        return false;
    }
    let mut pid = 0u32;
    unsafe {
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
    }
    pid == own_pid
}

pub fn spawn_cursor_loop(app: AppHandle) {
    std::thread::spawn(move || {
        let Some(win) = app.get_webview_window("island") else { return };
        let Ok(hwnd) = hwnd_of(&win) else { return };
        let own_pid = unsafe { GetCurrentProcessId() };
        let mut ignoring = true;
        let mut last = (i32::MIN, i32::MIN);
        let mut far_sent = false;
        let mut tick = 0u32;
        let mut cached_wp = win.outer_position().ok();
        let mut cached_scale = win.scale_factor().ok();
        loop {
            std::thread::sleep(Duration::from_millis(16));
            tick = tick.wrapping_add(1);

            // Всегда помним последнее внешнее активное окно — чтобы клик по буферу вставлял точно туда.
            let fg = unsafe { GetForegroundWindow() };
            if !fg.0.is_null() && !is_own_window(fg, own_pid) {
                *app.state::<WinState>().prev_fg.lock() = fg.0 as isize;
            }

            let mut p = POINT::default();
            if unsafe { GetCursorPos(&mut p) }.is_err() {
                continue;
            }
            if tick % 20 == 0 || cached_wp.is_none() || cached_scale.is_none() {
                cached_wp = win.outer_position().ok();
                cached_scale = win.scale_factor().ok();
            }
            let (Some(wp), Some(scale)) = (cached_wp, cached_scale) else {
                continue;
            };
            let lx = (p.x - wp.x) as f64 / scale;
            let ly = (p.y - wp.y) as f64 / scale;

            let state = app.state::<WinState>();
            let through = state.through_mode.load(Ordering::Relaxed);
            let dragging = !crate::shelf_dnd::is_out_dragging()
                && unsafe { (GetAsyncKeyState(VK_LBUTTON.0 as i32) as u16 & 0x8000) != 0 };
            let drop_zone = dragging
                && ly >= -4.0
                && ly <= 85.0
                && (lx - WIN_W / 2.0).abs() <= 260.0;
            if drop_zone {
                crate::shelf_dnd::attach_drop_targets(&app);
            }
            let (inside, hit_h) = state
                .hit
                .lock()
                .map(|r| ((!through || r.w >= 400.0) && r.contains(lx, ly), r.h))
                .unwrap_or((false, 0.0));
            let want_ignore = !inside
                && !drop_zone
                && !state.input_mode.load(Ordering::Relaxed);
            if want_ignore != ignoring {
                ignoring = want_ignore;
                set_transparent_click(hwnd, ignoring);
            }

            let max_y = if hit_h > 100.0 { hit_h + 90.0 } else { 180.0 };
            let near = lx > -200.0 && lx < WIN_W + 200.0 && ly < max_y;
            if near {
                if (p.x, p.y) != last {
                    last = (p.x, p.y);
                    far_sent = false;
                    let _ = app.emit(
                        "cursor",
                        Cursor { x: lx, y: ly, edge: p.y <= wp.y + 1, near: true, dragging },
                    );
                }
            } else if !far_sent {
                far_sent = true;
                last = (i32::MIN, i32::MIN);
                let _ = app.emit(
                    "cursor",
                    Cursor { x: lx, y: ly, edge: false, near: false, dragging },
                );
            }
        }
    });
}

#[tauri::command]
pub fn set_hit_rect(state: State<WinState>, rect: Option<Rect>) {
    static FIRST: AtomicBool = AtomicBool::new(false);
    if !FIRST.swap(true, Ordering::Relaxed) {
        log::line(format!("first hit_rect from UI: {rect:?}"));
    }
    *state.hit.lock() = rect;
}

#[tauri::command]
pub fn set_through(state: State<WinState>, on: bool) {
    state.through_mode.store(on, Ordering::Relaxed);
}

#[tauri::command]
pub fn set_capture_visible(app: AppHandle, visible: bool) -> Result<(), String> {
    let win = app.get_webview_window("island").ok_or("no window")?;
    let hwnd = hwnd_of(&win).map_err(|e| e.to_string())?;
    unsafe {
        let _ = SetWindowDisplayAffinity(
            hwnd,
            if visible { WDA_NONE } else { WDA_EXCLUDEFROMCAPTURE },
        );
    }
    Ok(())
}

/// Надёжно передаёт реальный Win32-фокус клавиатуры окну Kapla на главном UI-потоке через `AttachThreadInput`.
pub fn force_activate(app: &AppHandle, focus_mini_browser: bool) {
    let app_cb = app.clone();
    let _ = app.run_on_main_thread(move || {
        let Some(win) = app_cb.get_webview_window("island") else {
            return;
        };
        let Ok(hwnd) = hwnd_of(&win) else {
            return;
        };
        let _ = win.set_focusable(true);
        unsafe {
            let mut ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
            ex &= !(WS_EX_APPWINDOW.0 as isize
                | WS_EX_NOACTIVATE.0 as isize
                | WS_EX_TRANSPARENT.0 as isize);
            ex |= WS_EX_TOOLWINDOW.0 as isize | WS_EX_LAYERED.0 as isize;
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex);

            let fg = GetForegroundWindow();
            let my_tid = GetCurrentThreadId();
            let fg_tid = if !fg.0.is_null() {
                GetWindowThreadProcessId(fg, None)
            } else {
                0
            };
            if fg_tid != 0 && fg_tid != my_tid {
                let _ = AttachThreadInput(fg_tid, my_tid, true);
                let _ = BringWindowToTop(hwnd);
                let _ = SetForegroundWindow(hwnd);
                let _ = AttachThreadInput(fg_tid, my_tid, false);
            } else {
                let _ = BringWindowToTop(hwnd);
                let _ = SetForegroundWindow(hwnd);
            }
        }
        if focus_mini_browser {
            if let Some(wv) = app_cb.get_webview("mini_browser") {
                let _ = wv.set_focus();
                return;
            }
        }
        let _ = win.set_focus();
    });
}

#[tauri::command]
pub fn begin_input(app: AppHandle, state: State<WinState>) -> Result<(), String> {
    state.input_mode.store(true, Ordering::SeqCst);
    INPUT_ACTIVE.store(true, Ordering::SeqCst);
    let own_pid = unsafe { GetCurrentProcessId() };
    let fg = unsafe { GetForegroundWindow() };
    if !fg.0.is_null() && !is_own_window(fg, own_pid) {
        *state.prev_fg.lock() = fg.0 as isize;
    }
    if !is_own_window(fg, own_pid) {
        force_activate(&app, false);
    } else if let Some(win) = app.get_webview_window("island") {
        if let Ok(hwnd) = hwnd_of(&win) {
            set_transparent_click(hwnd, false);
        }
    }
    Ok(())
}

#[tauri::command]
pub fn end_input(app: AppHandle, state: State<WinState>) -> Result<(), String> {
    restore_focus(&app, &state);
    Ok(())
}

pub fn set_browser_active(app: &AppHandle, active: bool) {
    let prev = BROWSER_ACTIVE.swap(active, Ordering::SeqCst);
    let state = app.state::<WinState>();
    if active {
        let own_pid = unsafe { GetCurrentProcessId() };
        let fg = unsafe { GetForegroundWindow() };
        if !fg.0.is_null() && !is_own_window(fg, own_pid) {
            *state.prev_fg.lock() = fg.0 as isize;
        }
        if !prev || !is_own_window(fg, own_pid) {
            force_activate(app, true);
        }
        return;
    }
    if prev == active {
        return;
    }
    if !state.input_mode.load(Ordering::Relaxed) {
        restore_focus(app, &state);
    } else if let Some(win) = app.get_webview_window("island") {
        let _ = win.set_focusable(true);
    }
}

pub fn restore_focus(app: &AppHandle, state: &WinState) {
    let was_input = state.input_mode.swap(false, Ordering::SeqCst);
    INPUT_ACTIVE.store(false, Ordering::SeqCst);
    let browser_on = BROWSER_ACTIVE.load(Ordering::Relaxed);
    if !browser_on {
        if let Some(win) = app.get_webview_window("island") {
            let _ = win.set_focusable(false);
            if let Ok(hwnd) = hwnd_of(&win) {
                unsafe {
                    let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE)
                        | WS_EX_NOACTIVATE.0 as isize
                        | WS_EX_TOOLWINDOW.0 as isize;
                    SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex);
                }
            }
        }
        let own_pid = unsafe { GetCurrentProcessId() };
        let fg = unsafe { GetForegroundWindow() };
        if was_input || is_own_window(fg, own_pid) {
            let prev = *state.prev_fg.lock();
            if prev != 0 {
                unsafe {
                    let _ = SetForegroundWindow(HWND(prev as _));
                }
            }
        }
    }
}
