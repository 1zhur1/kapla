//! Встроенный мини-браузер (дочерний WebView2 внутри капли) для ChatGPT, DeepSeek и быстрых запросов.

use crate::log;
use crate::window;
use parking_lot::Mutex;
use serde::Serialize;
use std::process::Command;
use tauri::webview::PageLoadEvent;
use tauri::{
    AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, Rect, State, Url, WebviewBuilder,
    WebviewUrl,
};

#[derive(Default)]
pub struct BrowserState {
    last_url: Mutex<String>,
}

#[derive(Clone, Serialize)]
struct BrowserNavEvt {
    url: String,
    loading: bool,
}

fn encode_query(q: &str) -> String {
    let mut out = String::with_capacity(q.len() * 2);
    for b in q.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*b as char)
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

pub fn normalize_target(raw: &str) -> String {
    let t = raw.trim();
    if t.is_empty() {
        return "https://chatgpt.com/".to_string();
    }
    if t.starts_with("http://") || t.starts_with("https://") {
        return t.to_string();
    }
    let looks_like_host = !t.contains(' ')
        && t.contains('.')
        && t.is_ascii()
        && !t.starts_with('.');
    if looks_like_host {
        format!("https://{t}")
    } else {
        format!("https://www.google.com/search?q={}", encode_query(t))
    }
}

#[tauri::command]
pub async fn browser_open(
    app: AppHandle,
    state: State<'_, BrowserState>,
    url: Option<String>,
    fallback_url: Option<String>,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
) -> Result<String, String> {
    window::set_browser_active(&app, true);

    let bounds = Rect {
        position: LogicalPosition::new(x, y).into(),
        size: LogicalSize::new(w.max(200.0), h.max(180.0)).into(),
    };

    if let Some(wv) = app.get_webview("mini_browser") {
        let _ = wv.set_bounds(bounds);
        let _ = wv.show();
        if let Some(explicit) = url.filter(|u| !u.trim().is_empty()) {
            let norm = normalize_target(&explicit);
            let parsed = Url::parse(&norm).map_err(|e| format!("неверный адрес: {e}"))?;
            *state.last_url.lock() = parsed.to_string();
            let cur = wv.url().map(|u| u.to_string()).unwrap_or_default();
            if cur != parsed.as_str() {
                let _ = wv.navigate(parsed.clone());
            }
            let _ = wv.set_focus();
            return Ok(parsed.to_string());
        }
        let _ = wv.set_focus();
        let cur = wv
            .url()
            .map(|u| u.to_string())
            .ok()
            .filter(|s| s.starts_with("http"))
            .unwrap_or_else(|| state.last_url.lock().clone());
        return Ok(cur);
    }

    let initial = url
        .filter(|u| !u.trim().is_empty())
        .or_else(|| fallback_url.filter(|u| !u.trim().is_empty()))
        .unwrap_or_else(|| "https://chatgpt.com/".to_string());

    let target = {
        let mut last = state.last_url.lock();
        let norm = normalize_target(&initial);
        *last = norm.clone();
        norm
    };

    let parsed = Url::parse(&target).map_err(|e| format!("неверный адрес: {e}"))?;

    let win = app
        .get_window("island")
        .ok_or("окно island не найдено")?;

    let app_ev = app.clone();
    let builder = WebviewBuilder::new("mini_browser", WebviewUrl::External(parsed.clone()))
        .transparent(false)
        .on_page_load(move |_wv, payload| {
            let u = payload.url().to_string();
            let loading = matches!(payload.event(), PageLoadEvent::Started);
            let _ = app_ev.emit("browser_nav", BrowserNavEvt { url: u, loading });
        });

    let wv = win
        .add_child(
            builder,
            LogicalPosition::new(x, y),
            LogicalSize::new(w.max(200.0), h.max(180.0)),
        )
        .map_err(|e| format!("не удалось создать мини-браузер: {e}"))?;

    let _ = wv.set_focus();
    log::line(format!("mini_browser created at ({x:.0},{y:.0}) {w:.0}x{h:.0} -> {parsed}"));
    Ok(parsed.to_string())
}

#[tauri::command]
pub fn browser_focus(app: AppHandle) {
    window::set_browser_active(&app, true);
}

#[tauri::command]
pub fn browser_hide(app: AppHandle) {
    if let Some(wv) = app.get_webview("mini_browser") {
        let _ = wv.hide();
    }
    window::set_browser_active(&app, false);
}

#[tauri::command]
pub fn browser_go(
    app: AppHandle,
    state: State<BrowserState>,
    input: String,
) -> Result<String, String> {
    let norm = normalize_target(&input);
    let parsed = Url::parse(&norm).map_err(|e| format!("неверный адрес: {e}"))?;
    *state.last_url.lock() = parsed.to_string();
    if let Some(wv) = app.get_webview("mini_browser") {
        wv.navigate(parsed.clone()).map_err(|e| e.to_string())?;
        let _ = wv.set_focus();
    }
    Ok(parsed.to_string())
}

#[tauri::command]
pub fn browser_nav(app: AppHandle, action: String) -> Result<(), String> {
    let Some(wv) = app.get_webview("mini_browser") else {
        return Ok(());
    };
    let js = match action.as_str() {
        "back" => "history.back()",
        "forward" => "history.forward()",
        "reload" => "location.reload()",
        _ => return Err("неизвестная команда навигации".into()),
    };
    wv.eval(js).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn browser_open_external(url: String) -> Result<(), String> {
    let norm = normalize_target(&url);
    Command::new("rundll32.exe")
        .args(["url.dll,FileProtocolHandler", &norm])
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_urls_and_queries() {
        assert_eq!(normalize_target(""), "https://chatgpt.com/");
        assert_eq!(normalize_target("https://chat.deepseek.com"), "https://chat.deepseek.com");
        assert_eq!(normalize_target("chatgpt.com"), "https://chatgpt.com");
        assert_eq!(
            normalize_target("привет мир"),
            "https://www.google.com/search?q=%D0%BF%D1%80%D0%B8%D0%B2%D0%B5%D1%82+%D0%BC%D0%B8%D1%80"
        );
    }
}
