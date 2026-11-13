//! Подключение VPN: выбор установленного VPN-приложения (.exe/.lnk) или профиля Windows,
//! мониторинг процесса и активного сетевого TUN/TAP/WireGuard-туннеля.

use crate::log;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, State};
use windows::core::{BOOL, PCWSTR, PWSTR};
use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM, WPARAM};
use windows::Win32::NetworkManagement::IpHelper::{
    GetAdaptersAddresses, GAA_FLAG_SKIP_ANYCAST, GAA_FLAG_SKIP_MULTICAST, IP_ADAPTER_ADDRESSES_LH,
};
use windows::Win32::NetworkManagement::Ndis::IfOperStatusUp;
use windows::Win32::Networking::WinSock::AF_UNSPEC;
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::Threading::{OpenProcess, TerminateProcess, PROCESS_TERMINATE};
use windows::Win32::UI::Controls::Dialogs::{
    GetOpenFileNameW, OFN_FILEMUSTEXIST, OFN_NOCHANGEDIR, OFN_PATHMUSTEXIST, OPENFILENAMEW,
};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindowThreadProcessId, PostMessageW, SW_SHOWNORMAL, WM_CLOSE,
};

const CREATE_NO_WINDOW: u32 = 0x08000000;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VpnKind {
    #[default]
    App,
    System,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VpnProfile {
    pub id: String,
    pub name: String,
    pub kind: VpnKind,
    pub path: String,
    #[serde(default)]
    pub args: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct VpnConfig {
    selected_id: Option<String>,
    profiles: Vec<VpnProfile>,
}

#[derive(Default)]
pub struct VpnState {
    cfg: Mutex<VpnConfig>,
    file: Mutex<Option<PathBuf>>,
}

#[derive(Clone, Debug, Serialize)]
pub struct VpnStatus {
    pub configured: bool,
    pub selected: Option<VpnProfile>,
    pub profiles: Vec<VpnProfile>,
    pub app_running: bool,
    pub tunnel_up: bool,
    pub tunnel_name: Option<String>,
    pub connected: bool,
}

fn to_wide(s: &str) -> Vec<u16> {
    std::ffi::OsStr::new(s)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

unsafe fn pwstr_to_string(p: PWSTR) -> String {
    if p.is_null() {
        return String::new();
    }
    p.to_string().unwrap_or_default()
}

fn make_id(kind: &VpnKind, path: &str) -> String {
    let tag = match kind {
        VpnKind::App => "app",
        VpnKind::System => "sys",
    };
    format!("{tag}:{}", path.to_lowercase())
}

fn display_name_for_path(path: &str) -> String {
    Path::new(path)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| path.to_string())
}

impl VpnState {
    fn save(&self) {
        let Some(p) = self.file.lock().as_ref().cloned() else {
            return;
        };
        let cfg = self.cfg.lock().clone();
        if let Ok(json) = serde_json::to_string_pretty(&cfg) {
            if let Err(e) = std::fs::write(p, json) {
                log::line(format!("vpn config save error: {e}"));
            }
        }
    }

    fn upsert(&self, profile: VpnProfile) -> VpnProfile {
        let mut cfg = self.cfg.lock();
        if let Some(existing) = cfg.profiles.iter_mut().find(|p| p.id == profile.id) {
            *existing = profile.clone();
        } else {
            cfg.profiles.push(profile.clone());
        }
        cfg.selected_id = Some(profile.id.clone());
        drop(cfg);
        self.save();
        profile
    }

    fn select(&self, id: &str) {
        let mut cfg = self.cfg.lock();
        if cfg.profiles.iter().any(|p| p.id == id) {
            cfg.selected_id = Some(id.to_string());
        }
        drop(cfg);
        self.save();
    }

    fn remove(&self, id: &str) {
        let mut cfg = self.cfg.lock();
        cfg.profiles.retain(|p| p.id != id);
        if cfg.selected_id.as_deref() == Some(id) {
            cfg.selected_id = cfg.profiles.first().map(|p| p.id.clone());
        }
        drop(cfg);
        self.save();
    }

    fn selected(&self) -> Option<VpnProfile> {
        let cfg = self.cfg.lock();
        cfg.selected_id
            .as_ref()
            .and_then(|id| cfg.profiles.iter().find(|p| &p.id == id).cloned())
            .or_else(|| cfg.profiles.first().cloned())
    }

    fn snapshot(&self) -> VpnStatus {
        let cfg = self.cfg.lock().clone();
        let selected = cfg
            .selected_id
            .as_ref()
            .and_then(|id| cfg.profiles.iter().find(|p| &p.id == id).cloned())
            .or_else(|| cfg.profiles.first().cloned());

        let tunnel_name = detect_tunnel();
        let tunnel_up = tunnel_name.is_some();
        let app_running = selected
            .as_ref()
            .is_some_and(|p| p.kind == VpnKind::App && is_exe_running(&p.path));

        let connected = match selected.as_ref() {
            Some(p) if p.kind == VpnKind::System => tunnel_up || is_ras_connected(&p.path),
            Some(_) => tunnel_up || app_running,
            None => tunnel_up,
        };

        VpnStatus {
            configured: !cfg.profiles.is_empty(),
            selected,
            profiles: cfg.profiles,
            app_running,
            tunnel_up,
            tunnel_name,
            connected,
        }
    }
}

/// Проверяет, поднят ли в Windows активный VPN/TUN/TAP/WireGuard-адаптер.
fn detect_tunnel() -> Option<String> {
    unsafe {
        let flags = GAA_FLAG_SKIP_ANYCAST | GAA_FLAG_SKIP_MULTICAST;
        let mut size: u32 = 16 * 1024;
        let mut buf = vec![0u8; size as usize];
        let mut err = GetAdaptersAddresses(
            AF_UNSPEC.0 as u32,
            flags,
            None,
            Some(buf.as_mut_ptr().cast::<IP_ADAPTER_ADDRESSES_LH>()),
            &mut size,
        );
        if err == 111 /* ERROR_BUFFER_OVERFLOW */ {
            buf.resize(size as usize, 0);
            err = GetAdaptersAddresses(
                AF_UNSPEC.0 as u32,
                flags,
                None,
                Some(buf.as_mut_ptr().cast::<IP_ADAPTER_ADDRESSES_LH>()),
                &mut size,
            );
        }
        if err != 0 {
            return None;
        }

        let mut cur = buf.as_ptr().cast::<IP_ADAPTER_ADDRESSES_LH>();
        while !cur.is_null() {
            let a = &*cur;
            if a.OperStatus == IfOperStatusUp {
                let desc = pwstr_to_string(a.Description);
                let friendly = pwstr_to_string(a.FriendlyName);
                let combined = format!("{desc} {friendly}").to_lowercase();

                let ignored = combined.contains("teredo")
                    || combined.contains("isatap")
                    || combined.contains("6to4")
                    || combined.contains("kernel debug")
                    || combined.contains("loopback")
                    || combined.contains("hyper-v")
                    || combined.contains("vmware")
                    || combined.contains("virtualbox");

                if !ignored {
                    let is_vpn_kw = [
                        "wintun",
                        "wireguard",
                        "tap-windows",
                        "openvpn",
                        "amnezia",
                        "outline",
                        "clash",
                        "sing-box",
                        "xray",
                        "nekobox",
                        "tun2socks",
                        "tailscale",
                        "zerotier",
                        "warp",
                        "cloudflare",
                        "mullvad",
                        "proton",
                        "nordlynx",
                        "windscribe",
                        "adguard",
                        "radmin",
                        "vpn",
                    ]
                    .iter()
                    .any(|kw| combined.contains(kw));

                    // IF_TYPE_PPP (23) или IF_TYPE_TUNNEL (131)
                    if is_vpn_kw || a.IfType == 23 || a.IfType == 131 {
                        return Some(if !friendly.is_empty() { friendly } else { desc });
                    }
                }
            }
            cur = a.Next;
        }
    }
    None
}

fn find_pids_by_exe(exe_path: &str) -> Vec<u32> {
    let Some(target) = Path::new(exe_path)
        .file_name()
        .map(|s| s.to_string_lossy().to_lowercase())
    else {
        return Vec::new();
    };
    if !target.ends_with(".exe") {
        return Vec::new();
    }

    let mut pids = Vec::new();
    unsafe {
        let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else {
            return pids;
        };
        let mut pe = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        if Process32FirstW(snap, &mut pe).is_ok() {
            loop {
                let len = pe
                    .szExeFile
                    .iter()
                    .position(|&c| c == 0)
                    .unwrap_or(pe.szExeFile.len());
                let name = String::from_utf16_lossy(&pe.szExeFile[..len]).to_lowercase();
                if name == target {
                    pids.push(pe.th32ProcessID);
                }
                if Process32NextW(snap, &mut pe).is_err() {
                    break;
                }
            }
        }
        let _ = CloseHandle(snap);
    }
    pids
}

fn is_exe_running(exe_path: &str) -> bool {
    !find_pids_by_exe(exe_path).is_empty()
}

fn is_ras_connected(name: &str) -> bool {
    let Ok(out) = Command::new("rasdial")
        .creation_flags(CREATE_NO_WINDOW)
        .output()
    else {
        return false;
    };
    let text = String::from_utf8_lossy(&out.stdout).to_lowercase();
    text.contains(&name.to_lowercase())
}

/// Читает настроенные профили Windows VPN из `rasphone.pbk`.
fn parse_ras_pbk(content: &str) -> Vec<String> {
    content
        .lines()
        .map(str::trim)
        .filter(|l| l.starts_with('[') && l.ends_with(']') && l.len() > 2)
        .map(|l| l[1..l.len() - 1].to_string())
        .collect()
}

fn system_vpn_names() -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for var in ["APPDATA", "ProgramData"] {
        if let Ok(base) = std::env::var(var) {
            let pbk = PathBuf::from(base)
                .join("Microsoft")
                .join("Network")
                .join("Connections")
                .join("Pbk")
                .join("rasphone.pbk");
            if let Ok(raw) = std::fs::read_to_string(pbk) {
                for name in parse_ras_pbk(&raw) {
                    if seen.insert(name.to_lowercase()) {
                        out.push(name);
                    }
                }
            }
        }
    }
    out
}

fn discover_profiles() -> Vec<VpnProfile> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();

    for name in system_vpn_names() {
        let id = make_id(&VpnKind::System, &name);
        if seen.insert(id.clone()) {
            out.push(VpnProfile {
                id,
                name,
                kind: VpnKind::System,
                path: String::new(),
                args: String::new(),
            });
        }
    }
    for p in &mut out {
        if p.kind == VpnKind::System {
            p.path = p.name.clone();
        }
    }

    let pf = std::env::var("ProgramFiles").unwrap_or_default();
    let pf86 = std::env::var("ProgramFiles(x86)").unwrap_or_default();
    let local = std::env::var("LOCALAPPDATA").unwrap_or_default();

    let candidates: &[(&str, &str)] = &[
        ("AmneziaVPN", r"AmneziaVPN\AmneziaVPN.exe"),
        ("WireGuard", r"WireGuard\wireguard.exe"),
        ("OpenVPN GUI", r"OpenVPN\bin\openvpn-gui.exe"),
        ("Cloudflare WARP", r"Cloudflare\Cloudflare WARP\Cloudflare WARP.exe"),
        ("Mullvad VPN", r"Mullvad VPN\Mullvad VPN.exe"),
        ("Proton VPN", r"Proton\VPN\ProtonVPN.Launcher.exe"),
        ("AdGuard VPN", r"AdGuardVpn\AdGuardVpn.exe"),
        ("Tailscale", r"Tailscale\tailscale-ipn.exe"),
        ("ZeroTier", r"ZeroTier\One\ZeroTier One.exe"),
        ("Radmin VPN", r"Radmin VPN\RvControlGui.exe"),
        ("Outline", r"Outline\Outline.exe"),
        ("Hiddify", r"Hiddify\Hiddify.exe"),
        ("Clash Verge", r"Clash Verge\clash-verge.exe"),
        ("v2rayN", r"v2rayN\v2rayN.exe"),
    ];

    for (label, rel) in candidates {
        for base in [&pf, &pf86, &local] {
            if base.is_empty() {
                continue;
            }
            let dirs = [PathBuf::from(base), PathBuf::from(base).join("Programs")];
            for d in dirs {
                let full = d.join(rel);
                if full.is_file() {
                    let path_str = full.to_string_lossy().into_owned();
                    let id = make_id(&VpnKind::App, &path_str);
                    if seen.insert(id.clone()) {
                        out.push(VpnProfile {
                            id,
                            name: (*label).to_string(),
                            kind: VpnKind::App,
                            path: path_str,
                            args: String::new(),
                        });
                    }
                }
            }
        }
    }

    out
}

fn launch_shell(path: &str, args: &str) -> Result<(), String> {
    let op = to_wide("open");
    let file = to_wide(path);
    let params = if args.trim().is_empty() {
        None
    } else {
        Some(to_wide(args))
    };
    let dir = Path::new(path)
        .parent()
        .map(|p| to_wide(&p.to_string_lossy()));

    let res = unsafe {
        ShellExecuteW(
            None,
            PCWSTR(op.as_ptr()),
            PCWSTR(file.as_ptr()),
            params.as_ref().map_or(PCWSTR::null(), |v| PCWSTR(v.as_ptr())),
            dir.as_ref().map_or(PCWSTR::null(), |v| PCWSTR(v.as_ptr())),
            SW_SHOWNORMAL,
        )
    };
    if (res.0 as isize) <= 32 {
        return Err(format!("не удалось запустить файл (код {})", res.0 as isize));
    }
    Ok(())
}

/// Пробует использовать встроенный CLI для известных VPN (если он есть рядом),
/// возвращая Some(Ok/Err) если команда сработала, или None чтобы упасть на обычный запуск/остановку.
fn try_known_cli(exe_path: &str, connect: bool) -> Option<Result<(), String>> {
    let stem = Path::new(exe_path)
        .file_stem()
        .map(|s| s.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    let dir = Path::new(exe_path).parent()?;

    let (cli, args): (PathBuf, &[&str]) = if stem.contains("warp") {
        (
            dir.join("warp-cli.exe"),
            if connect { &["connect"] } else { &["disconnect"] },
        )
    } else if stem.contains("tailscale") {
        (
            dir.join("tailscale.exe"),
            if connect { &["up"] } else { &["down"] },
        )
    } else if stem.contains("mullvad") {
        (
            dir.join("resources").join("mullvad.exe"),
            if connect { &["connect"] } else { &["disconnect"] },
        )
    } else {
        return None;
    };

    if !cli.is_file() {
        return None;
    }

    let out = Command::new(&cli)
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .output();
    match out {
        Ok(o) if o.status.success() => Some(Ok(())),
        Ok(_) => None,
        Err(_) => None,
    }
}

struct CloseCtx {
    pids: Vec<u32>,
}

unsafe extern "system" fn enum_close_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let ctx = &*(lparam.0 as *const CloseCtx);
    let mut pid = 0u32;
    GetWindowThreadProcessId(hwnd, Some(&mut pid));
    if ctx.pids.contains(&pid) {
        let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
    }
    BOOL(1)
}

fn stop_exe(exe_path: &str) -> Result<(), String> {
    let pids = find_pids_by_exe(exe_path);
    if pids.is_empty() {
        return Ok(());
    }
    let ctx = CloseCtx { pids: pids.clone() };
    unsafe {
        let _ = EnumWindows(Some(enum_close_proc), LPARAM(&ctx as *const CloseCtx as isize));
    }
    for _ in 0..5 {
        std::thread::sleep(Duration::from_millis(80));
        if find_pids_by_exe(exe_path).is_empty() {
            return Ok(());
        }
    }

    let remaining = find_pids_by_exe(exe_path);
    let mut access_denied = false;
    for pid in remaining {
        unsafe {
            match OpenProcess(PROCESS_TERMINATE, false, pid) {
                Ok(h) => {
                    let _ = TerminateProcess(h, 0);
                    let _ = CloseHandle(h);
                }
                Err(_) => access_denied = true,
            }
        }
    }
    std::thread::sleep(Duration::from_millis(120));
    if !find_pids_by_exe(exe_path).is_empty() && access_denied {
        return Err("VPN запущен от администратора — закройте в его окне".into());
    }
    Ok(())
}

pub fn spawn(app: AppHandle) {
    let state = app.state::<VpnState>();
    if let Ok(dir) = app.path().app_data_dir() {
        let _ = std::fs::create_dir_all(&dir);
        let p = dir.join("vpn.json");
        if let Ok(raw) = std::fs::read_to_string(&p) {
            if let Ok(cfg) = serde_json::from_str::<VpnConfig>(&raw) {
                *state.cfg.lock() = cfg;
            }
        }
        *state.file.lock() = Some(p);
    }

    std::thread::spawn(move || {
        let mut last_key = String::new();
        loop {
            std::thread::sleep(Duration::from_secs(2));
            let st = app.state::<VpnState>().snapshot();
            let key = format!(
                "{}:{}:{}:{}",
                st.connected,
                st.app_running,
                st.tunnel_up,
                st.selected.as_ref().map(|p| p.id.as_str()).unwrap_or("")
            );
            if key != last_key {
                last_key = key;
                let _ = app.emit("vpn", st);
            }
        }
    });
}

#[tauri::command]
pub fn vpn_status(state: State<VpnState>) -> VpnStatus {
    state.snapshot()
}

#[tauri::command]
pub fn vpn_discover() -> Vec<VpnProfile> {
    discover_profiles()
}

#[tauri::command]
pub async fn vpn_pick_app(app: AppHandle) -> Result<VpnStatus, String> {
    let picked = tauri::async_runtime::spawn_blocking(|| -> Option<String> {
        let mut buf = vec![0u16; 1024];
        let filter: Vec<u16> = "VPN программы (*.exe;*.lnk)\0*.exe;*.lnk\0Все файлы (*.*)\0*.*\0\0"
            .encode_utf16()
            .collect();
        let title = to_wide("Выберите приложение для VPN");
        let mut ofn = OPENFILENAMEW {
            lStructSize: std::mem::size_of::<OPENFILENAMEW>() as u32,
            lpstrFilter: PCWSTR(filter.as_ptr()),
            lpstrFile: PWSTR(buf.as_mut_ptr()),
            nMaxFile: buf.len() as u32,
            lpstrTitle: PCWSTR(title.as_ptr()),
            Flags: OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST | OFN_NOCHANGEDIR,
            ..Default::default()
        };
        let ok = unsafe { GetOpenFileNameW(&mut ofn).as_bool() };
        if !ok {
            return None;
        }
        let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        let path = std::ffi::OsString::from_wide(&buf[..end])
            .to_string_lossy()
            .into_owned();
        if path.is_empty() { None } else { Some(path) }
    })
    .await
    .map_err(|e| e.to_string())?;

    let st = app.state::<VpnState>();
    if let Some(path) = picked {
        let profile = VpnProfile {
            id: make_id(&VpnKind::App, &path),
            name: display_name_for_path(&path),
            kind: VpnKind::App,
            path,
            args: String::new(),
        };
        st.upsert(profile);
        log::line("vpn app configured via picker");
    }
    let snap = st.snapshot();
    let _ = app.emit("vpn", snap.clone());
    Ok(snap)
}

#[tauri::command]
pub fn vpn_add_profile(
    app: AppHandle,
    state: State<VpnState>,
    name: String,
    kind: VpnKind,
    path: String,
) -> VpnStatus {
    let clean_name = if name.trim().is_empty() {
        display_name_for_path(&path)
    } else {
        name.trim().to_string()
    };
    let profile = VpnProfile {
        id: make_id(&kind, &path),
        name: clean_name,
        kind,
        path,
        args: String::new(),
    };
    state.upsert(profile);
    let snap = state.snapshot();
    let _ = app.emit("vpn", snap.clone());
    snap
}

#[tauri::command]
pub fn vpn_select(app: AppHandle, state: State<VpnState>, id: String) -> VpnStatus {
    state.select(&id);
    let snap = state.snapshot();
    let _ = app.emit("vpn", snap.clone());
    snap
}

#[tauri::command]
pub fn vpn_remove(app: AppHandle, state: State<VpnState>, id: String) -> VpnStatus {
    state.remove(&id);
    let snap = state.snapshot();
    let _ = app.emit("vpn", snap.clone());
    snap
}

#[tauri::command]
pub async fn vpn_connect(app: AppHandle) -> Result<VpnStatus, String> {
    let profile = app
        .state::<VpnState>()
        .selected()
        .ok_or("VPN ещё не выбран")?;

    tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
        match profile.kind {
            VpnKind::System => {
                let out = Command::new("rasdial")
                    .arg(&profile.path)
                    .creation_flags(CREATE_NO_WINDOW)
                    .output()
                    .map_err(|e| e.to_string())?;
                if !out.status.success() {
                    return Err("Не удалось подключить системный VPN-профиль".into());
                }
            }
            VpnKind::App => {
                if let Some(res) = try_known_cli(&profile.path, true) {
                    res?;
                } else {
                    launch_shell(&profile.path, &profile.args)?;
                }
            }
        }
        std::thread::sleep(Duration::from_millis(350));
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())??;

    let snap = app.state::<VpnState>().snapshot();
    let _ = app.emit("vpn", snap.clone());
    log::line(format!("vpn connect triggered -> connected={}", snap.connected));
    Ok(snap)
}

#[tauri::command]
pub async fn vpn_disconnect(app: AppHandle) -> Result<VpnStatus, String> {
    let profile = app
        .state::<VpnState>()
        .selected()
        .ok_or("VPN ещё не выбран")?;

    tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
        match profile.kind {
            VpnKind::System => {
                let out = Command::new("rasdial")
                    .args([&profile.path, "/disconnect"])
                    .creation_flags(CREATE_NO_WINDOW)
                    .output()
                    .map_err(|e| e.to_string())?;
                if !out.status.success() {
                    return Err("Не удалось отключить системный VPN".into());
                }
            }
            VpnKind::App => {
                if let Some(res) = try_known_cli(&profile.path, false) {
                    res?;
                } else {
                    stop_exe(&profile.path)?;
                }
            }
        }
        std::thread::sleep(Duration::from_millis(250));
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())??;

    let snap = app.state::<VpnState>().snapshot();
    let _ = app.emit("vpn", snap.clone());
    log::line(format!("vpn disconnect triggered -> connected={}", snap.connected));
    Ok(snap)
}

#[tauri::command]
pub async fn vpn_toggle(app: AppHandle) -> Result<VpnStatus, String> {
    let cur = app.state::<VpnState>().snapshot();
    if !cur.configured {
        return vpn_pick_app(app).await;
    }
    if cur.connected {
        vpn_disconnect(app).await
    } else {
        vpn_connect(app).await
    }
}

#[tauri::command]
pub fn vpn_open_app(state: State<VpnState>) -> Result<(), String> {
    let profile = state.selected().ok_or("VPN ещё не выбран")?;
    match profile.kind {
        VpnKind::App => launch_shell(&profile.path, &profile.args),
        VpnKind::System => launch_shell("ms-settings:network-vpn", ""),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_rasphone_sections() {
        let pbk = "[Work VPN]\nEncoding=1\n[Home WireGuard]\nType=2\n";
        assert_eq!(parse_ras_pbk(pbk), vec!["Work VPN", "Home WireGuard"]);
    }

    #[test]
    fn vpn_state_crud_and_selection() {
        let st = VpnState::default();
        let p1 = VpnProfile {
            id: "app:a".into(),
            name: "AmneziaVPN".into(),
            kind: VpnKind::App,
            path: r"C:\Program Files\AmneziaVPN\AmneziaVPN.exe".into(),
            args: String::new(),
        };
        let p2 = VpnProfile {
            id: "sys:work".into(),
            name: "Work".into(),
            kind: VpnKind::System,
            path: "Work".into(),
            args: String::new(),
        };
        st.upsert(p1);
        st.upsert(p2);
        assert_eq!(st.selected().unwrap().id, "sys:work");
        st.select("app:a");
        assert_eq!(st.selected().unwrap().name, "AmneziaVPN");
        st.remove("app:a");
        assert_eq!(st.selected().unwrap().id, "sys:work");
    }
}
