//! Музыка: системные медиа-сессии Windows (GSMTC) — Spotify, браузеры, Яндекс Музыка и т. д.

use crate::log;
use base64::Engine;
use serde::Serialize;
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSession as Session,
    GlobalSystemMediaTransportControlsSessionManager as Manager,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus as Status,
};
use windows::Storage::Streams::DataReader;
use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};
use windows::Win32::System::SystemInformation::GetSystemTimeAsFileTime;

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct MediaInfo {
    pub active: bool,
    pub title: String,
    pub artist: String,
    pub app: String,
    pub playing: bool,
    /// Секунды.
    pub position: f64,
    pub duration: f64,
    /// data:-URL обложки.
    pub cover: Option<String>,
}

fn pretty_app(id: &str) -> String {
    let base = id.rsplit(['\\', '!']).next().unwrap_or(id);
    let base = base.trim_end_matches(".exe");
    let base = base.split('.').last().unwrap_or(base);
    let lower = base.to_lowercase();
    match lower.as_str() {
        "spotify" => "Spotify".into(),
        "chrome" => "Chrome".into(),
        "msedge" => "Edge".into(),
        "firefox" => "Firefox".into(),
        "yandex" | "browser" => "Яндекс".into(),
        "zunemusic" | "media" => "Медиаплеер".into(),
        _ => {
            let mut c = base.chars();
            match c.next() {
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                None => String::new(),
            }
        }
    }
}

fn now_filetime() -> i64 {
    let ft = unsafe { GetSystemTimeAsFileTime() };
    ((ft.dwHighDateTime as i64) << 32) | ft.dwLowDateTime as i64
}

fn read_cover(session: &Session) -> windows::core::Result<Option<String>> {
    let props = session.TryGetMediaPropertiesAsync()?.get()?;
    let Ok(thumb) = props.Thumbnail() else { return Ok(None) };
    let stream = thumb.OpenReadAsync()?.get()?;
    let size = stream.Size()? as u32;
    if size == 0 || size > 8_000_000 {
        return Ok(None);
    }
    let mime = stream.ContentType().map(|s| s.to_string()).unwrap_or_default();
    let reader = DataReader::CreateDataReader(&stream)?;
    reader.LoadAsync(size)?.get()?;
    let mut buf = vec![0u8; size as usize];
    reader.ReadBytes(&mut buf)?;
    let mime = if mime.is_empty() { "image/png".to_string() } else { mime };
    Ok(Some(format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(buf)
    )))
}

/// Выбирает лучшую медиа-сессию: сначала играющую, затем текущую, затем любую с названием.
fn pick_session(mgr: &Manager) -> Option<Session> {
    if let Ok(list) = mgr.GetSessions() {
        let mut fallback: Option<Session> = None;
        for s in list {
            let status = s.GetPlaybackInfo().and_then(|p| p.PlaybackStatus()).ok();
            let has_title = s
                .TryGetMediaPropertiesAsync()
                .and_then(|op| op.get())
                .and_then(|p| p.Title())
                .map(|t| !t.is_empty())
                .unwrap_or(false);
            if status == Some(Status::Playing) && has_title {
                return Some(s);
            }
            if fallback.is_none() && has_title {
                fallback = Some(s);
            }
        }
        if let Ok(cur) = mgr.GetCurrentSession() {
            return Some(cur);
        }
        return fallback;
    }
    mgr.GetCurrentSession().ok()
}

fn read(mgr: &Manager, prev: &MediaInfo) -> MediaInfo {
    let Some(session) = pick_session(mgr) else {
        return MediaInfo::default();
    };
    let Ok(props) = session.TryGetMediaPropertiesAsync().and_then(|op| op.get()) else {
        return MediaInfo::default();
    };
    let title = props.Title().map(|s| s.to_string()).unwrap_or_default();
    if title.is_empty() {
        return MediaInfo::default();
    }
    let artist = props.Artist().map(|s| s.to_string()).unwrap_or_default();
    let playing = session
        .GetPlaybackInfo()
        .and_then(|p| p.PlaybackStatus())
        .map(|s| s == Status::Playing)
        .unwrap_or(false);

    let (mut position, duration) = match session.GetTimelineProperties() {
        Ok(tl) => {
            let mut pos = tl.Position().map(|t| t.Duration as f64 / 1e7).unwrap_or(0.0);
            let start = tl.StartTime().map(|t| t.Duration as f64 / 1e7).unwrap_or(0.0);
            let end = tl.EndTime().map(|t| t.Duration as f64 / 1e7).unwrap_or(0.0);
            let dur = (end - start).max(0.0);
            if playing {
                if let Ok(ut) = tl.LastUpdatedTime() {
                    if ut.UniversalTime > 0 {
                        pos += ((now_filetime() - ut.UniversalTime) as f64 / 1e7).max(0.0);
                    }
                }
            }
            if dur > 0.0 {
                pos = pos.min(dur);
            }
            (pos.max(0.0), dur)
        }
        Err(_) => (0.0, 0.0),
    };
    if !position.is_finite() {
        position = 0.0;
    }

    let same_track = prev.title == title && prev.artist == artist;
    let cover = if same_track && prev.cover.is_some() {
        prev.cover.clone()
    } else {
        read_cover(&session).ok().flatten()
    };
    let app = session
        .SourceAppUserModelId()
        .map(|s| pretty_app(&s.to_string()))
        .unwrap_or_default();

    MediaInfo {
        active: true,
        title,
        artist,
        app,
        playing,
        position,
        duration,
        cover,
    }
}

static LAST: parking_lot::Mutex<Option<MediaInfo>> = parking_lot::Mutex::new(None);

pub fn spawn(app: AppHandle) {
    std::thread::spawn(move || {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        }
        loop {
            let mgr = match Manager::RequestAsync().and_then(|op| op.get()) {
                Ok(m) => {
                    log::line("GSMTC media manager ready");
                    m
                }
                Err(e) => {
                    log::line(format!("media manager error: {e}"));
                    std::thread::sleep(Duration::from_secs(5));
                    continue;
                }
            };
            let mut prev = MediaInfo::default();
            let mut ticks = 0u32;
            loop {
                let info = read(&mgr, &prev);
                if info.active != prev.active || info.title != prev.title {
                    log::line(format!(
                        "media state: active={} app='{}' title='{}' playing={}",
                        info.active, info.app, info.title, info.playing
                    ));
                }
                // Позиция меняется каждый тик — шлём её не чаще раза в 2 с, остальное сразу.
                let mut cmp = info.clone();
                cmp.position = prev.position;
                if cmp != prev || ticks % 4 == 0 {
                    *LAST.lock() = Some(info.clone());
                    let _ = app.emit("media", &info);
                }
                prev = info;
                ticks = ticks.wrapping_add(1);
                std::thread::sleep(Duration::from_millis(500));
            }
        }
    });
}

#[tauri::command]
pub fn media_now() -> MediaInfo {
    LAST.lock().clone().unwrap_or_default()
}

#[tauri::command]
pub async fn media_control(action: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || -> windows::core::Result<()> {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        }
        let mgr = Manager::RequestAsync()?.get()?;
        let Some(s) = pick_session(&mgr) else {
            return Ok(());
        };
        match action.as_str() {
            "toggle" => {
                let _ = s.TryTogglePlayPauseAsync()?.get()?;
            }
            "next" => {
                let _ = s.TrySkipNextAsync()?.get()?;
            }
            "prev" => {
                let _ = s.TrySkipPreviousAsync()?.get()?;
            }
            _ => {}
        }
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())
}
