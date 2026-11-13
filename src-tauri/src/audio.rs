//! Звук: громкость вывода, выключение микрофона и пиковый уровень звука (WASAPI).

use windows::core::Result;
use windows::Win32::Media::Audio::Endpoints::{IAudioEndpointVolume, IAudioMeterInformation};
use windows::Win32::Media::Audio::{
    eCapture, eConsole, eRender, EDataFlow, IMMDeviceEnumerator, MMDeviceEnumerator,
};
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED};

fn default_device(flow: EDataFlow) -> Result<windows::Win32::Media::Audio::IMMDevice> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let en: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        en.GetDefaultAudioEndpoint(flow, eConsole)
    }
}

pub fn endpoint() -> Result<IAudioEndpointVolume> {
    unsafe { default_device(eRender)?.Activate::<IAudioEndpointVolume>(CLSCTX_ALL, None) }
}

pub fn meter() -> Result<IAudioMeterInformation> {
    unsafe { default_device(eRender)?.Activate::<IAudioMeterInformation>(CLSCTX_ALL, None) }
}

pub fn mic_endpoint() -> Result<IAudioEndpointVolume> {
    unsafe { default_device(eCapture)?.Activate::<IAudioEndpointVolume>(CLSCTX_ALL, None) }
}

/// (громкость 0..1, выключен ли звук)
pub fn read(ep: &IAudioEndpointVolume) -> Result<(f32, bool)> {
    unsafe { Ok((ep.GetMasterVolumeLevelScalar()?, ep.GetMute()?.as_bool())) }
}

pub fn read_mic(ep: &IAudioEndpointVolume) -> bool {
    unsafe { ep.GetMute().map(|b| b.as_bool()).unwrap_or(false) }
}

pub fn read_peak(m: &IAudioMeterInformation) -> f32 {
    unsafe { m.GetPeakValue().unwrap_or(0.0).clamp(0.0, 1.0) }
}

#[tauri::command]
pub fn set_volume(value: f32) -> std::result::Result<(), String> {
    let ep = endpoint().map_err(|e| e.to_string())?;
    unsafe {
        ep.SetMasterVolumeLevelScalar(value.clamp(0.0, 1.0), std::ptr::null())
            .map_err(|e| e.to_string())?;
        if value > 0.0 {
            let _ = ep.SetMute(false, std::ptr::null());
        }
    }
    Ok(())
}

#[tauri::command]
pub fn toggle_mute() -> std::result::Result<(), String> {
    let ep = endpoint().map_err(|e| e.to_string())?;
    unsafe {
        let m = ep.GetMute().map_err(|e| e.to_string())?.as_bool();
        ep.SetMute(!m, std::ptr::null()).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub fn toggle_mic() -> std::result::Result<bool, String> {
    let ep = mic_endpoint().map_err(|e| e.to_string())?;
    unsafe {
        let next = !ep.GetMute().map_err(|e| e.to_string())?.as_bool();
        ep.SetMute(next, std::ptr::null()).map_err(|e| e.to_string())?;
        Ok(next)
    }
}
