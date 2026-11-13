//! Управление Wi-Fi, Bluetooth и режимом питания Windows для пульта Kapla.

use crate::log;
use serde::Serialize;
use std::collections::HashSet;
use std::process::Command;
use windows::core::GUID;
use windows::Devices::Radios::{Radio, RadioAccessStatus, RadioKind, RadioState};
use windows::Win32::Devices::Bluetooth::{
    BluetoothFindDeviceClose, BluetoothFindFirstDevice, BluetoothFindNextDevice,
    BluetoothSetServiceState, BLUETOOTH_DEVICE_INFO, BLUETOOTH_DEVICE_SEARCH_PARAMS,
    BLUETOOTH_SERVICE_DISABLE, BLUETOOTH_SERVICE_ENABLE,
};
use windows::Win32::Foundation::HANDLE;
use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};

#[cfg(windows)]
use std::os::windows::process::CommandExt;
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;

const AUDIO_SINK_UUID: GUID = GUID::from_u128(0x0000110b_0000_1000_8000_00805f9b34fb);
const HANDSFREE_UUID: GUID = GUID::from_u128(0x0000111e_0000_1000_8000_00805f9b34fb);

const GUID_SAVER: &str = "a1841308-3541-4fab-bc81-f71556f20b4a";
const GUID_BALANCED: &str = "381b4222-f694-41f0-9685-ff5bb260df2e";
const GUID_PERF: &str = "8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c";

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct WifiNet {
    pub ssid: String,
    pub signal: u8,
    pub secured: bool,
    pub saved: bool,
    pub connected: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct BtDev {
    pub name: String,
    pub addr: String,
    pub connected: bool,
    pub is_audio: bool,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct ConnectivityState {
    pub wifi_has_radio: bool,
    pub wifi_on: bool,
    pub wifi_ssid: Option<String>,
    pub wifi_signal: Option<u8>,
    pub wifi_nets: Vec<WifiNet>,
    pub bt_has_radio: bool,
    pub bt_on: bool,
    pub bt_devices: Vec<BtDev>,
    pub power_plan: String,
}

fn run_cmd(prog: &str, args: &[&str]) -> String {
    let mut cmd = Command::new(prog);
    cmd.args(args);
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd.output()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default()
}

fn read_radios() -> ((bool, bool), (bool, bool)) {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
    let mut wifi = (false, false);
    let mut bt = (false, false);
    let Ok(op) = Radio::GetRadiosAsync() else {
        return (wifi, bt);
    };
    let Ok(list) = op.get() else {
        return (wifi, bt);
    };
    for r in list {
        let Ok(kind) = r.Kind() else { continue };
        let on = r.State().map(|s| s == RadioState::On).unwrap_or(false);
        if kind == RadioKind::WiFi {
            wifi = (true, on);
        } else if kind == RadioKind::Bluetooth {
            bt = (true, on);
        }
    }
    (wifi, bt)
}

fn set_radio(kind: RadioKind, on: bool) -> Result<(), String> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
    let access = Radio::RequestAccessAsync()
        .map_err(|e| format!("RequestAccess: {e}"))?
        .get()
        .map_err(|e| format!("RequestAccess wait: {e}"))?;
    if access != RadioAccessStatus::Allowed {
        return Err("Windows запретил переключение радиомодуля".into());
    }
    let radios = Radio::GetRadiosAsync()
        .map_err(|e| format!("GetRadios: {e}"))?
        .get()
        .map_err(|e| format!("GetRadios wait: {e}"))?;
    let target = if on { RadioState::On } else { RadioState::Off };
    let mut found = false;
    for r in radios {
        if r.Kind().ok() == Some(kind) {
            found = true;
            let res = r
                .SetStateAsync(target)
                .map_err(|e| format!("SetState: {e}"))?
                .get()
                .map_err(|e| format!("SetState wait: {e}"))?;
            if res != RadioAccessStatus::Allowed {
                return Err("Нет прав на изменение состояния".into());
            }
        }
    }
    if !found {
        return Err("Адаптер не найден".into());
    }
    Ok(())
}

pub fn parse_netsh_iface(raw: &str) -> (Option<String>, Option<u8>, bool) {
    let mut ssid: Option<String> = None;
    let mut sig: Option<u8> = None;
    let mut connected = false;
    for line in raw.lines() {
        let Some((k, v)) = line.split_once(':') else { continue };
        let kl = k.trim().to_ascii_lowercase();
        let val = v.trim();
        if kl == "state" || kl == "состояние" {
            let vl = val.to_ascii_lowercase();
            connected = vl.contains("connected") || vl.contains("подключ");
            if connected && (vl.contains("dis") || vl.contains("отключ")) {
                connected = false;
            }
        } else if kl == "ssid" && !val.is_empty() {
            ssid = Some(val.to_string());
        } else if (kl == "signal" || kl == "сигнал") && val.ends_with('%') {
            if let Ok(n) = val.trim_end_matches('%').trim().parse::<u8>() {
                sig = Some(n);
            }
        }
    }
    if !connected {
        (None, None, false)
    } else {
        (ssid, sig, true)
    }
}

pub fn parse_netsh_profiles(raw: &str) -> HashSet<String> {
    let mut set = HashSet::new();
    for line in raw.lines() {
        if let Some((k, v)) = line.split_once(':') {
            let kl = k.trim().to_ascii_lowercase();
            if (kl.contains("profile") || kl.contains("профил")) && !v.trim().is_empty() {
                set.insert(v.trim().to_string());
            }
        }
    }
    set
}

pub fn parse_netsh_networks(
    raw: &str,
    saved: &HashSet<String>,
    active_ssid: Option<&str>,
) -> Vec<WifiNet> {
    let mut nets: Vec<WifiNet> = Vec::new();
    let mut cur_ssid: Option<String> = None;
    let mut cur_sig: u8 = 0;
    let mut cur_sec = true;

    let flush = |nets: &mut Vec<WifiNet>, ssid: Option<String>, sig: u8, sec: bool| {
        let Some(name) = ssid.filter(|s| !s.trim().is_empty()) else {
            return;
        };
        if let Some(existing) = nets.iter_mut().find(|n| n.ssid == name) {
            if sig > existing.signal {
                existing.signal = sig;
            }
            return;
        }
        let connected = active_ssid.is_some_and(|a| a == name);
        let is_saved = connected || saved.contains(&name);
        nets.push(WifiNet {
            ssid: name,
            signal: sig,
            secured: sec,
            saved: is_saved,
            connected,
        });
    };

    for line in raw.lines() {
        let trimmed = line.trim();
        let Some((k, v)) = trimmed.split_once(':') else {
            continue;
        };
        let kl = k.trim().to_ascii_lowercase();
        let val = v.trim();
        if kl.starts_with("ssid ") && !kl.starts_with("bssid") {
            flush(&mut nets, cur_ssid.take(), cur_sig, cur_sec);
            cur_ssid = Some(val.to_string());
            cur_sig = 0;
            cur_sec = true;
        } else if (kl == "authentication" || kl.contains("проверка подлинности"))
            && (val.eq_ignore_ascii_case("open") || val.contains("Открыт"))
        {
            cur_sec = false;
        } else if (kl == "signal" || kl == "сигнал") && val.ends_with('%') {
            if let Ok(n) = val.trim_end_matches('%').trim().parse::<u8>() {
                if n > cur_sig {
                    cur_sig = n;
                }
            }
        }
    }
    flush(&mut nets, cur_ssid, cur_sig, cur_sec);

    nets.sort_by(|a, b| {
        b.connected
            .cmp(&a.connected)
            .then(b.saved.cmp(&a.saved))
            .then(b.signal.cmp(&a.signal))
    });
    nets
}

fn list_bt_devices() -> Vec<BtDev> {
    let mut out = Vec::new();
    let params = BLUETOOTH_DEVICE_SEARCH_PARAMS {
        dwSize: std::mem::size_of::<BLUETOOTH_DEVICE_SEARCH_PARAMS>() as u32,
        fReturnAuthenticated: true.into(),
        fReturnRemembered: true.into(),
        fReturnUnknown: false.into(),
        fReturnConnected: true.into(),
        fIssueInquiry: false.into(),
        cTimeoutMultiplier: 0,
        hRadio: HANDLE::default(),
    };
    let mut info = BLUETOOTH_DEVICE_INFO {
        dwSize: std::mem::size_of::<BLUETOOTH_DEVICE_INFO>() as u32,
        ..Default::default()
    };

    let Ok(hfind) = (unsafe { BluetoothFindFirstDevice(&params, &mut info) }) else {
        return out;
    };

    loop {
        let raw_name = &info.szName;
        let len = raw_name.iter().position(|&c| c == 0).unwrap_or(raw_name.len());
        let name = String::from_utf16_lossy(&raw_name[..len]).trim().to_string();
        if !name.is_empty() {
            let addr_u64 = unsafe { info.Address.Anonymous.ullLong };
            let major_class = (info.ulClassofDevice >> 8) & 0x1F;
            let is_audio = major_class == 4;
            out.push(BtDev {
                name,
                addr: format!("{addr_u64:012X}"),
                connected: info.fConnected.as_bool(),
                is_audio,
            });
        }
        info = BLUETOOTH_DEVICE_INFO {
            dwSize: std::mem::size_of::<BLUETOOTH_DEVICE_INFO>() as u32,
            ..Default::default()
        };
        if unsafe { BluetoothFindNextDevice(hfind, &mut info) }.is_err() {
            break;
        }
    }
    let _ = unsafe { BluetoothFindDeviceClose(hfind) };

    out.sort_by(|a, b| {
        b.connected
            .cmp(&a.connected)
            .then(b.is_audio.cmp(&a.is_audio))
            .then(a.name.cmp(&b.name))
    });
    out
}

fn active_power_plan() -> String {
    let raw = run_cmd("powercfg", &["/getactivescheme"]).to_ascii_lowercase();
    if raw.contains(GUID_SAVER) {
        "saver".into()
    } else if raw.contains(GUID_PERF) {
        "performance".into()
    } else {
        "balanced".into()
    }
}

fn collect_state(scan_wifi: bool) -> ConnectivityState {
    let ((wifi_has_radio, wifi_on), (bt_has_radio, bt_on)) = read_radios();
    let iface = if wifi_on || !wifi_has_radio {
        run_cmd("netsh", &["wlan", "show", "interfaces"])
    } else {
        String::new()
    };
    let (wifi_ssid, wifi_signal, iface_ok) = parse_netsh_iface(&iface);
    let wifi_on_final = if wifi_has_radio { wifi_on } else { iface_ok };

    let wifi_nets = if scan_wifi && wifi_on_final {
        let saved = parse_netsh_profiles(&run_cmd("netsh", &["wlan", "show", "profiles"]));
        let nets_raw = run_cmd("netsh", &["wlan", "show", "networks", "mode=bssid"]);
        parse_netsh_networks(&nets_raw, &saved, wifi_ssid.as_deref())
    } else {
        Vec::new()
    };

    let bt_devices = if bt_on { list_bt_devices() } else { Vec::new() };

    ConnectivityState {
        wifi_has_radio: wifi_has_radio || !iface.is_empty(),
        wifi_on: wifi_on_final,
        wifi_ssid,
        wifi_signal,
        wifi_nets,
        bt_has_radio: bt_has_radio || !bt_devices.is_empty(),
        bt_on,
        bt_devices,
        power_plan: active_power_plan(),
    }
}

#[tauri::command]
pub async fn connectivity_status(scan: Option<bool>) -> Result<ConnectivityState, String> {
    let do_scan = scan.unwrap_or(false);
    tauri::async_runtime::spawn_blocking(move || Ok(collect_state(do_scan)))
        .await
        .map_err(|e| format!("join: {e}"))?
}

#[tauri::command]
pub async fn wifi_toggle(on: bool) -> Result<ConnectivityState, String> {
    tauri::async_runtime::spawn_blocking(move || {
        set_radio(RadioKind::WiFi, on)?;
        std::thread::sleep(std::time::Duration::from_millis(350));
        Ok(collect_state(on))
    })
    .await
    .map_err(|e| format!("join: {e}"))?
}

#[tauri::command]
pub async fn wifi_connect(ssid: String) -> Result<ConnectivityState, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let mut cmd = Command::new("netsh");
        cmd.args(["wlan", "connect", &format!("name={ssid}")]);
        #[cfg(windows)]
        cmd.creation_flags(CREATE_NO_WINDOW);
        let out = cmd.output().map_err(|e| e.to_string())?;
        if !out.status.success() {
            return Err("Сеть требует пароль — подключи её один раз через настройки Windows".into());
        }
        std::thread::sleep(std::time::Duration::from_millis(800));
        Ok(collect_state(true))
    })
    .await
    .map_err(|e| format!("join: {e}"))?
}

#[tauri::command]
pub async fn wifi_disconnect() -> Result<ConnectivityState, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let _ = run_cmd("netsh", &["wlan", "disconnect"]);
        std::thread::sleep(std::time::Duration::from_millis(300));
        Ok(collect_state(true))
    })
    .await
    .map_err(|e| format!("join: {e}"))?
}

#[tauri::command]
pub async fn bt_toggle(on: bool) -> Result<ConnectivityState, String> {
    tauri::async_runtime::spawn_blocking(move || {
        set_radio(RadioKind::Bluetooth, on)?;
        std::thread::sleep(std::time::Duration::from_millis(350));
        Ok(collect_state(false))
    })
    .await
    .map_err(|e| format!("join: {e}"))?
}

#[tauri::command]
pub async fn bt_connect(addr: String, connect: bool) -> Result<ConnectivityState, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let target = u64::from_str_radix(addr.trim(), 16).map_err(|_| "Неверный MAC")?;
        let params = BLUETOOTH_DEVICE_SEARCH_PARAMS {
            dwSize: std::mem::size_of::<BLUETOOTH_DEVICE_SEARCH_PARAMS>() as u32,
            fReturnAuthenticated: true.into(),
            fReturnRemembered: true.into(),
            fReturnUnknown: false.into(),
            fReturnConnected: true.into(),
            fIssueInquiry: false.into(),
            cTimeoutMultiplier: 0,
            hRadio: HANDLE::default(),
        };
        let mut info = BLUETOOTH_DEVICE_INFO {
            dwSize: std::mem::size_of::<BLUETOOTH_DEVICE_INFO>() as u32,
            ..Default::default()
        };
        let hfind = unsafe { BluetoothFindFirstDevice(&params, &mut info) }
            .map_err(|_| "Устройство Bluetooth не найдено")?;

        let mut matched: Option<BLUETOOTH_DEVICE_INFO> = None;
        loop {
            if unsafe { info.Address.Anonymous.ullLong } == target {
                matched = Some(info);
                break;
            }
            info = BLUETOOTH_DEVICE_INFO {
                dwSize: std::mem::size_of::<BLUETOOTH_DEVICE_INFO>() as u32,
                ..Default::default()
            };
            if unsafe { BluetoothFindNextDevice(hfind, &mut info) }.is_err() {
                break;
            }
        }
        let _ = unsafe { BluetoothFindDeviceClose(hfind) };

        let dev = matched.ok_or("Устройство не найдено среди сопряжённых")?;
        let flag = if connect {
            BLUETOOTH_SERVICE_ENABLE
        } else {
            BLUETOOTH_SERVICE_DISABLE
        };
        let r1 = unsafe { BluetoothSetServiceState(None, &dev, &AUDIO_SINK_UUID, flag) };
        let r2 = unsafe { BluetoothSetServiceState(None, &dev, &HANDSFREE_UUID, flag) };
        log::line(format!("bt_connect {addr} -> {connect}: sink={r1} hfp={r2}"));
        std::thread::sleep(std::time::Duration::from_millis(600));
        Ok(collect_state(false))
    })
    .await
    .map_err(|e| format!("join: {e}"))?
}

#[tauri::command]
pub async fn power_set(mode: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let guid = match mode.as_str() {
            "saver" => GUID_SAVER,
            "performance" => GUID_PERF,
            _ => GUID_BALANCED,
        };
        let _ = run_cmd("powercfg", &["/setactive", guid]);
        Ok(active_power_plan())
    })
    .await
    .map_err(|e| format!("join: {e}"))?
}

#[tauri::command]
pub fn open_win_settings(uri: String) -> Result<(), String> {
    Command::new("explorer.exe")
        .arg(&uri)
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_netsh_wifi_output() {
        let iface = "\
    Name : Беспроводная сеть
    State : connected
    SSID : TP-Link_AF03_5G
    Signal : 83%";
        let (ssid, sig, ok) = parse_netsh_iface(iface);
        assert!(ok);
        assert_eq!(ssid.as_deref(), Some("TP-Link_AF03_5G"));
        assert_eq!(sig, Some(83));

        let saved = parse_netsh_profiles("    All User Profile : TP-Link_AF03_5G\n");
        let nets = parse_netsh_networks(
            "SSID 1 : Neighbor\n    Signal : 90%\nSSID 2 : TP-Link_AF03_5G\n    Signal : 70%\n",
            &saved,
            Some("TP-Link_AF03_5G"),
        );
        assert_eq!(nets.len(), 2);
        assert!(nets[0].connected);
        assert!(nets[0].saved);
        assert_eq!(nets[0].ssid, "TP-Link_AF03_5G");
    }
}
