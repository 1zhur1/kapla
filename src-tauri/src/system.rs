//! Системные показатели: батарея, раскладка, Caps Lock, громкость, микрофон, тема и уровень звука.

use crate::audio;
use serde::Serialize;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};
use windows::core::w;
use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
use windows::Win32::System::Registry::{
    RegGetValueW, RegOpenKeyExW, RegSetValueExW, HKEY_CURRENT_USER, KEY_WRITE, REG_DWORD,
    RRF_RT_REG_DWORD,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetKeyState, GetKeyboardLayout, VK_CAPITAL};
use windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetWindowThreadProcessId, SendMessageTimeoutW, HWND_BROADCAST,
    SMTO_ABORTIFHUNG, WM_SETTINGCHANGE,
};

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Sys {
    pub battery: Option<u8>,
    pub charging: bool,
    pub layout: String,
    pub caps: bool,
    pub volume: f32,
    pub muted: bool,
    pub mic_muted: bool,
    pub dark_theme: bool,
}

fn battery() -> (Option<u8>, bool) {
    let mut s = SYSTEM_POWER_STATUS::default();
    if unsafe { GetSystemPowerStatus(&mut s) }.is_err() {
        return (None, false);
    }
    let no_battery = s.BatteryFlag & 128 != 0 || s.BatteryLifePercent == 255;
    (
        if no_battery { None } else { Some(s.BatteryLifePercent) },
        s.ACLineStatus == 1,
    )
}

fn layout() -> String {
    unsafe {
        let fg = GetForegroundWindow();
        let tid = GetWindowThreadProcessId(fg, None);
        let hkl = GetKeyboardLayout(tid);
        let lang = (hkl.0 as usize & 0x3FF) as u16;
        match lang {
            0x19 => "RU",
            0x09 => "EN",
            0x22 => "UA",
            0x23 => "BY",
            0x3F => "KZ",
            0x07 => "DE",
            0x0C => "FR",
            0x0A => "ES",
            _ => "??",
        }
        .to_string()
    }
}

fn is_dark_theme() -> bool {
    unsafe {
        let mut val: u32 = 0;
        let mut size = std::mem::size_of::<u32>() as u32;
        let ok = RegGetValueW(
            HKEY_CURRENT_USER,
            w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"),
            w!("AppsUseLightTheme"),
            RRF_RT_REG_DWORD,
            None,
            Some(&mut val as *mut u32 as *mut _),
            Some(&mut size),
        );
        if ok.is_ok() {
            val == 0
        } else {
            true
        }
    }
}

#[tauri::command]
pub fn toggle_windows_theme() -> Result<bool, String> {
    let dark = !is_dark_theme();
    let light_val: u32 = if dark { 0 } else { 1 };
    unsafe {
        let mut hkey = Default::default();
        RegOpenKeyExW(
            HKEY_CURRENT_USER,
            w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"),
            Some(0),
            KEY_WRITE,
            &mut hkey,
        )
        .ok()
        .map_err(|e| e.to_string())?;
        let bytes = light_val.to_le_bytes();
        let _ = RegSetValueExW(hkey, w!("AppsUseLightTheme"), Some(0), REG_DWORD, Some(&bytes));
        let _ = RegSetValueExW(hkey, w!("SystemUsesLightTheme"), Some(0), REG_DWORD, Some(&bytes));
        let _ = SendMessageTimeoutW(
            HWND_BROADCAST,
            WM_SETTINGCHANGE,
            windows::Win32::Foundation::WPARAM(0),
            windows::Win32::Foundation::LPARAM(w!("ImmersiveColorSet").as_ptr() as isize),
            SMTO_ABORTIFHUNG,
            150,
            None,
        );
    }
    Ok(dark)
}

static LAST: parking_lot::Mutex<Option<Sys>> = parking_lot::Mutex::new(None);

pub fn spawn(app: AppHandle) {
    // Системное состояние (250 мс)
    let app_sys = app.clone();
    std::thread::spawn(move || {
        let mut ep = audio::endpoint().ok();
        let mut mic = audio::mic_endpoint().ok();
        let mut ep_born = Instant::now();
        let mut prev = Sys::default();
        loop {
            if ep.is_none() || ep_born.elapsed() > Duration::from_secs(5) {
                ep = audio::endpoint().ok();
                mic = audio::mic_endpoint().ok();
                ep_born = Instant::now();
            }
            let (volume, muted) = ep
                .as_ref()
                .and_then(|e| audio::read(e).ok())
                .unwrap_or((0.0, false));
            let mic_muted = mic.as_ref().map(audio::read_mic).unwrap_or(false);
            let (battery, charging) = battery();
            let cur = Sys {
                battery,
                charging,
                layout: layout(),
                caps: unsafe { GetKeyState(VK_CAPITAL.0 as i32) } & 1 != 0,
                volume: (volume * 100.0).round() / 100.0,
                muted,
                mic_muted,
                dark_theme: is_dark_theme(),
            };
            if cur != prev {
                *LAST.lock() = Some(cur.clone());
                let _ = app_sys.emit("sys", &cur);
                prev = cur;
            }
            std::thread::sleep(Duration::from_millis(250));
        }
    });

    // Пиковый уровень звука (~20 Гц, только когда реально играет звук)
    std::thread::spawn(move || {
        let mut meter = audio::meter().ok();
        let mut born = Instant::now();
        let mut silent_ticks = 0u32;
        loop {
            if meter.is_none() || born.elapsed() > Duration::from_secs(8) {
                meter = audio::meter().ok();
                born = Instant::now();
            }
            let level = meter.as_ref().map(audio::read_peak).unwrap_or(0.0);
            if level > 0.008 {
                silent_ticks = 0;
                let _ = app.emit("audio_level", level);
            } else if silent_ticks < 4 {
                silent_ticks += 1;
                let _ = app.emit("audio_level", 0.0f32);
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    });
}

#[tauri::command]
pub fn sys_now() -> Sys {
    LAST.lock().clone().unwrap_or_default()
}
