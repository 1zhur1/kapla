//! Простой файл-лог в `%APPDATA%\app.kapla.island\kapla.log` (перезаписывается при старте).

use parking_lot::Mutex;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

static LOG_PATH: Mutex<Option<PathBuf>> = Mutex::new(None);

pub fn init(dir: PathBuf) {
    let _ = fs::create_dir_all(&dir);
    let p = dir.join("kapla.log");
    let _ = fs::write(&p, "");
    *LOG_PATH.lock() = Some(p);
    line("kapla started");
}

pub fn line(msg: impl AsRef<str>) {
    let s = msg.as_ref();
    eprintln!("[kapla] {s}");
    let guard = LOG_PATH.lock();
    let Some(p) = guard.as_ref() else { return };
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(p) {
        let secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() % 86_400)
            .unwrap_or(0);
        let h = secs / 3600;
        let m = (secs % 3600) / 60;
        let sec = secs % 60;
        let _ = writeln!(f, "[{h:02}:{m:02}:{sec:02}] {s}");
    }
}
