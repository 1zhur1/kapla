//! История буфера обмена (текст + изображения) и вставка кликом в активное окно.

use crate::log;
use crate::window::{restore_focus, WinState};
use arboard::ImageData;
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use image::{imageops::FilterType, ImageFormat, RgbaImage};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::io::Cursor;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager, State};
use windows::core::w;
use windows::Win32::System::DataExchange::{
    GetClipboardSequenceNumber, IsClipboardFormatAvailable, RegisterClipboardFormatW,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP,
    VIRTUAL_KEY, VK_CONTROL, VK_V,
};

const MAX_UNPINNED: usize = 200;
const MAX_UNPINNED_IMAGES: usize = 25;
const MAX_LEN: usize = 100_000;
const MAX_IMAGE_PIXELS: usize = 32_000_000;
const CF_UNICODETEXT: u32 = 13;
const CF_DIB: u32 = 8;
const CF_DIBV5: u32 = 17;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ClipKind {
    #[default]
    Text,
    Image,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Clip {
    pub id: u64,
    #[serde(default)]
    pub kind: ClipKind,
    pub text: String,
    #[serde(default)]
    pub preview: Option<String>,
    #[serde(default)]
    pub img_path: Option<String>,
    #[serde(default)]
    pub hash: Option<u64>,
    pub ts: u64,
    pub pinned: bool,
}

#[derive(Default)]
pub struct ClipState {
    items: Mutex<Vec<Clip>>,
    next_id: Mutex<u64>,
    path: Mutex<Option<PathBuf>>,
    img_dir: Mutex<Option<PathBuf>>,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn fnv1a(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

fn remove_img_file(path: &Option<String>) {
    if let Some(p) = path {
        let _ = std::fs::remove_file(p);
    }
}

impl ClipState {
    fn save(&self) {
        if let Some(p) = self.path.lock().as_ref() {
            if let Ok(json) = serde_json::to_string(&*self.items.lock()) {
                if let Err(e) = std::fs::write(p, json) {
                    log::line(format!("clipboard save error: {e}"));
                }
            }
        }
    }

    fn trim_locked(items: &mut Vec<Clip>) {
        let mut unpinned_text = 0usize;
        let mut unpinned_img = 0usize;
        let mut removed_paths: Vec<Option<String>> = Vec::new();
        items.retain(|c| {
            if c.pinned {
                return true;
            }
            let keep = match c.kind {
                ClipKind::Text => {
                    unpinned_text += 1;
                    unpinned_text <= MAX_UNPINNED
                }
                ClipKind::Image => {
                    unpinned_img += 1;
                    unpinned_img <= MAX_UNPINNED_IMAGES
                }
            };
            if !keep {
                removed_paths.push(c.img_path.clone());
            }
            keep
        });
        for p in removed_paths {
            remove_img_file(&p);
        }
    }

    /// Возвращает true, если список реально изменился.
    fn push_text(&self, text: String) -> bool {
        let mut items = self.items.lock();
        if items
            .first()
            .is_some_and(|c| c.kind == ClipKind::Text && c.text == text)
        {
            return false;
        }
        let ts = now_ms();
        if let Some(i) = items
            .iter()
            .position(|c| c.kind == ClipKind::Text && c.text == text)
        {
            let mut c = items.remove(i);
            c.ts = ts;
            items.insert(0, c);
        } else {
            let mut id = self.next_id.lock();
            *id += 1;
            items.insert(
                0,
                Clip {
                    id: *id,
                    kind: ClipKind::Text,
                    text,
                    preview: None,
                    img_path: None,
                    hash: None,
                    ts,
                    pinned: false,
                },
            );
        }
        Self::trim_locked(&mut items);
        true
    }

    fn push_image(&self, width: usize, height: usize, rgba: &[u8]) -> bool {
        if width == 0 || height == 0 || width.saturating_mul(height) > MAX_IMAGE_PIXELS {
            return false;
        }
        let hash = fnv1a(rgba);
        {
            let mut items = self.items.lock();
            if items
                .first()
                .is_some_and(|c| c.kind == ClipKind::Image && c.hash == Some(hash))
            {
                return false;
            }
            if let Some(i) = items
                .iter()
                .position(|c| c.kind == ClipKind::Image && c.hash == Some(hash))
            {
                let mut c = items.remove(i);
                c.ts = now_ms();
                items.insert(0, c);
                return true;
            }
        }

        let Some(img) = RgbaImage::from_raw(width as u32, height as u32, rgba.to_vec()) else {
            return false;
        };

        let id = {
            let mut n = self.next_id.lock();
            *n += 1;
            *n
        };

        let img_path = if let Some(dir) = self.img_dir.lock().as_ref() {
            let _ = std::fs::create_dir_all(dir);
            let p = dir.join(format!("{id}.png"));
            if let Err(e) = img.save_with_format(&p, ImageFormat::Png) {
                log::line(format!("clipboard image save error: {e}"));
                return false;
            }
            Some(p.to_string_lossy().into_owned())
        } else {
            return false;
        };

        let thumb = if width > 220 {
            let th = ((height as u64 * 220) / width as u64).max(1) as u32;
            image::imageops::resize(&img, 220, th, FilterType::Triangle)
        } else {
            img
        };
        let mut buf = Cursor::new(Vec::new());
        let preview = if thumb.write_to(&mut buf, ImageFormat::Png).is_ok() {
            Some(format!("data:image/png;base64,{}", B64.encode(buf.into_inner())))
        } else {
            None
        };

        let mut items = self.items.lock();
        items.insert(
            0,
            Clip {
                id,
                kind: ClipKind::Image,
                text: format!("{width}×{height} · PNG"),
                preview,
                img_path,
                hash: Some(hash),
                ts: now_ms(),
                pinned: false,
            },
        );
        Self::trim_locked(&mut items);
        true
    }
}

/// Менеджеры паролей помечают приватные копии этими форматами — такие записи не храним.
fn is_private() -> bool {
    unsafe {
        let f1 = RegisterClipboardFormatW(w!("ExcludeClipboardContentFromMonitorProcessing"));
        let f2 = RegisterClipboardFormatW(w!("Clipboard Viewer Ignore"));
        IsClipboardFormatAvailable(f1).is_ok() || IsClipboardFormatAvailable(f2).is_ok()
    }
}

fn has_format(fmt: u32) -> bool {
    unsafe { IsClipboardFormatAvailable(fmt).is_ok() }
}

fn read_text_retry() -> Option<String> {
    if !has_format(CF_UNICODETEXT) {
        return None;
    }
    for attempt in 0..5 {
        match arboard::Clipboard::new().and_then(|mut c| c.get_text()) {
            Ok(t) => return Some(t),
            Err(e) => {
                if attempt == 4 {
                    log::line(format!("clipboard text read skipped: {e}"));
                }
                std::thread::sleep(Duration::from_millis(30));
            }
        }
    }
    None
}

fn read_image_retry() -> Option<(usize, usize, Vec<u8>)> {
    if !has_format(CF_DIB) && !has_format(CF_DIBV5) {
        return None;
    }
    for attempt in 0..4 {
        match arboard::Clipboard::new().and_then(|mut c| c.get_image()) {
            Ok(img) => return Some((img.width, img.height, img.bytes.into_owned())),
            Err(e) => {
                if attempt == 3 {
                    log::line(format!("clipboard image read skipped: {e}"));
                }
                std::thread::sleep(Duration::from_millis(35));
            }
        }
    }
    None
}

static SUPPRESS_NOTIFY_UNTIL: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Serialize)]
struct ClipCapturedEvt {
    kind: ClipKind,
    text: String,
}

fn suppress_notify(ms: u64) {
    SUPPRESS_NOTIFY_UNTIL.store(now_ms().saturating_add(ms), Ordering::Relaxed);
}

pub fn capture(app: &AppHandle, notify: bool) {
    if is_private() {
        return;
    }
    let allow_evt = notify && now_ms() >= SUPPRESS_NOTIFY_UNTIL.load(Ordering::Relaxed);
    let st = app.state::<ClipState>();
    if let Some(text) = read_text_retry() {
        if !text.trim().is_empty() && text.len() <= MAX_LEN && st.push_text(text.clone()) {
            st.save();
            let list = st.items.lock().clone();
            log::line(format!("clipboard text captured, total={}", list.len()));
            let _ = app.emit("clips", list);
            if allow_evt {
                let _ = app.emit(
                    "clip_captured",
                    ClipCapturedEvt {
                        kind: ClipKind::Text,
                        text,
                    },
                );
            }
        }
        return;
    }
    if let Some((w, h, rgba)) = read_image_retry() {
        if st.push_image(w, h, &rgba) {
            st.save();
            let list = st.items.lock().clone();
            log::line(format!("clipboard image captured ({w}x{h}), total={}", list.len()));
            let _ = app.emit("clips", list);
            if allow_evt {
                let _ = app.emit(
                    "clip_captured",
                    ClipCapturedEvt {
                        kind: ClipKind::Image,
                        text: format!("{w}×{h}"),
                    },
                );
            }
        }
    }
}

pub fn spawn(app: AppHandle) {
    let state = app.state::<ClipState>();
    if let Ok(dir) = app.path().app_data_dir() {
        let _ = std::fs::create_dir_all(&dir);
        let img_dir = dir.join("clip-img");
        let _ = std::fs::create_dir_all(&img_dir);
        *state.img_dir.lock() = Some(img_dir);

        let p = dir.join("clipboard.json");
        if let Ok(raw) = std::fs::read_to_string(&p) {
            if let Ok(items) = serde_json::from_str::<Vec<Clip>>(&raw) {
                *state.next_id.lock() = items.iter().map(|c| c.id).max().unwrap_or(0);
                *state.items.lock() = items;
            }
        }
        *state.path.lock() = Some(p);
    }

    // Подхватываем то, что уже было в буфере при запуске программы (без вспышки).
    capture(&app, false);

    std::thread::spawn(move || {
        let mut seq = unsafe { GetClipboardSequenceNumber() };
        loop {
            std::thread::sleep(Duration::from_millis(250));
            let cur = unsafe { GetClipboardSequenceNumber() };
            if cur == seq {
                continue;
            }
            seq = cur;
            capture(&app, true);
        }
    });
}

fn key(vk: VIRTUAL_KEY, up: bool) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: 0,
                dwFlags: if up { KEYEVENTF_KEYUP } else { KEYBD_EVENT_FLAGS(0) },
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

pub fn send_ctrl_v() {
    let inputs = [
        key(VK_CONTROL, false),
        key(VK_V, false),
        key(VK_V, true),
        key(VK_CONTROL, true),
    ];
    unsafe {
        SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
    }
}

#[tauri::command]
pub fn clip_list(state: State<ClipState>) -> Vec<Clip> {
    state.items.lock().clone()
}

#[tauri::command]
pub fn clip_set(app: AppHandle, state: State<ClipState>, text: String) -> Result<(), String> {
    suppress_notify(900);
    arboard::Clipboard::new()
        .and_then(|mut c| c.set_text(text.clone()))
        .map_err(|e| e.to_string())?;
    if !text.trim().is_empty() && text.len() <= MAX_LEN && state.push_text(text) {
        state.save();
        let _ = app.emit("clips", state.items.lock().clone());
    }
    Ok(())
}

#[tauri::command]
pub async fn clip_paste(app: AppHandle, id: u64) -> Result<(), String> {
    suppress_notify(1100);
    let clip = {
        let st = app.state::<ClipState>();
        let items = st.items.lock();
        items.iter().find(|c| c.id == id).cloned()
    }
    .ok_or("запись не найдена")?;

    let ws = app.state::<WinState>();
    let had_focus = ws.input_mode.load(std::sync::atomic::Ordering::SeqCst);
    restore_focus(&app, &ws);

    tauri::async_runtime::spawn_blocking(move || {
        if had_focus {
            std::thread::sleep(Duration::from_millis(85));
        }
        match clip.kind {
            ClipKind::Text => {
                arboard::Clipboard::new()
                    .and_then(|mut c| c.set_text(clip.text))
                    .map_err(|e| e.to_string())?;
            }
            ClipKind::Image => {
                let p = clip.img_path.ok_or("файл изображения не найден")?;
                let dyn_img = image::open(&p).map_err(|e| e.to_string())?;
                let rgba = dyn_img.to_rgba8();
                let (w, h) = (rgba.width() as usize, rgba.height() as usize);
                let data = ImageData {
                    width: w,
                    height: h,
                    bytes: Cow::Owned(rgba.into_raw()),
                };
                arboard::Clipboard::new()
                    .and_then(|mut c| c.set_image(data))
                    .map_err(|e| e.to_string())?;
            }
        }
        std::thread::sleep(Duration::from_millis(45));
        send_ctrl_v();
        Ok::<_, String>(())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn clip_pin(app: AppHandle, state: State<ClipState>, id: u64) {
    if let Some(c) = state.items.lock().iter_mut().find(|c| c.id == id) {
        c.pinned = !c.pinned;
    }
    state.save();
    let _ = app.emit("clips", state.items.lock().clone());
}

#[tauri::command]
pub fn clip_delete(app: AppHandle, state: State<ClipState>, id: u64) {
    let mut removed = None;
    {
        let mut items = state.items.lock();
        if let Some(pos) = items.iter().position(|c| c.id == id) {
            removed = items.remove(pos).img_path;
        }
    }
    remove_img_file(&removed);
    state.save();
    let _ = app.emit("clips", state.items.lock().clone());
}

#[tauri::command]
pub fn clip_clear(app: AppHandle, state: State<ClipState>) {
    let mut removed = Vec::new();
    {
        let mut items = state.items.lock();
        items.retain(|c| {
            if c.pinned {
                true
            } else {
                removed.push(c.img_path.clone());
                false
            }
        });
    }
    for p in removed {
        remove_img_file(&p);
    }
    state.save();
    let _ = app.emit("clips", state.items.lock().clone());
}

#[tauri::command]
pub fn clip_image_data(state: State<ClipState>, id: u64) -> Result<String, String> {
    let img_path = {
        let items = state.items.lock();
        items
            .iter()
            .find(|c| c.id == id)
            .and_then(|c| c.img_path.clone())
    }
    .ok_or_else(|| "оригинал изображения не найден".to_string())?;
    let bytes = std::fs::read(&img_path).map_err(|e| format!("не удалось прочитать файл: {e}"))?;
    Ok(format!("data:image/png;base64,{}", B64.encode(&bytes)))
}

#[derive(Clone, Serialize)]
pub struct EditedImageResult {
    pub saved_path: Option<String>,
    pub clip_id: Option<u64>,
}

#[tauri::command]
pub async fn clip_save_edited(
    app: AppHandle,
    png_bytes: Vec<u8>,
    copy_clipboard: bool,
    save_path: Option<String>,
    save_dir: Option<String>,
) -> Result<EditedImageResult, String> {
    let app_bg = app.clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<EditedImageResult, String> {
        let dyn_img = image::load_from_memory(&png_bytes)
            .map_err(|e| format!("некорректное изображение: {e}"))?;
        let rgba = dyn_img.to_rgba8();
        let (w, h) = (rgba.width() as usize, rgba.height() as usize);

        let mut saved_path: Option<String> = None;
        let target_path = if let Some(p) = save_path.filter(|s| !s.trim().is_empty()) {
            Some(PathBuf::from(p))
        } else if let Some(dir) = save_dir.filter(|s| !s.trim().is_empty()) {
            let secs = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            Some(PathBuf::from(dir).join(format!("kapla_edit_{secs}.png")))
        } else {
            None
        };

        if let Some(ref dest) = target_path {
            if let Some(parent) = dest.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let is_jpg = dest
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| matches!(e.to_ascii_lowercase().as_str(), "jpg" | "jpeg"))
                .unwrap_or(false);
            if is_jpg {
                dyn_img
                    .to_rgb8()
                    .save_with_format(dest, ImageFormat::Jpeg)
                    .map_err(|e| format!("ошибка сохранения JPG: {e}"))?;
            } else {
                std::fs::write(dest, &png_bytes)
                    .map_err(|e| format!("ошибка сохранения PNG: {e}"))?;
            }
            saved_path = Some(dest.to_string_lossy().into_owned());
        }

        let mut clip_id: Option<u64> = None;
        if copy_clipboard {
            suppress_notify(1200);
            let raw = rgba.into_raw();
            let data = ImageData {
                width: w,
                height: h,
                bytes: Cow::Borrowed(&raw),
            };
            arboard::Clipboard::new()
                .and_then(|mut c| c.set_image(data))
                .map_err(|e| format!("не удалось записать в буфер: {e}"))?;

            let st = app_bg.state::<ClipState>();
            if st.push_image(w, h, &raw) {
                st.save();
                let list = st.items.lock().clone();
                clip_id = list.first().map(|c| c.id);
                let _ = app_bg.emit("clips", list);
            }
        }

        Ok(EditedImageResult { saved_path, clip_id })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_clipboard_json_deserializes() {
        let raw = r#"[{"id":1,"text":"hello","ts":1234,"pinned":true}]"#;
        let parsed: Vec<Clip> = serde_json::from_str(raw).expect("deserialize legacy");
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].kind, ClipKind::Text);
        assert!(parsed[0].preview.is_none());
        assert!(parsed[0].pinned);
    }

    #[test]
    fn push_text_dedups_and_preserves_pinned() {
        let st = ClipState::default();
        assert!(st.push_text("alpha".into()));
        assert!(!st.push_text("alpha".into()));
        assert!(st.push_text("beta".into()));
        assert!(st.push_text("alpha".into()));
        let items = st.items.lock();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].text, "alpha");
        assert_eq!(items[1].text, "beta");
    }

    #[test]
    fn rejects_empty_or_oversized_images() {
        let st = ClipState::default();
        assert!(!st.push_image(0, 100, &[]));
        assert!(!st.push_image(100, 0, &[]));
        assert!(!st.push_image(100_000, 100_000, &[]));
    }
}
