//! Kapla — живая капля-остров для Windows.
//! Rust-ядро: окно без фокуса, провайдеры состояния (музыка, система, буфер) и команды для UI.

mod audio;
mod browser;
mod clipboard;
mod connectivity;
mod converter;
mod log;
mod media;
mod shelf_dnd;
mod system;
mod tools;
mod vpn;
mod weather;
mod window;

use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{DragDropEvent, Emitter, Manager, WindowEvent};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

const MODS: Modifiers = Modifiers::CONTROL.union(Modifiers::ALT);

pub fn run() {
    tauri::Builder::default()
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| {
                    if event.state() != ShortcutState::Pressed {
                        return;
                    }
                    if shortcut.matches(MODS, Code::KeyI) {
                        let _ = app.emit("panel", "toggle");
                    } else if shortcut.matches(MODS, Code::KeyV) {
                        let _ = app.emit("panel", "clipboard");
                    } else if shortcut.matches(MODS, Code::KeyM) {
                        if let Ok(muted) = audio::toggle_mic() {
                            let _ = app.emit("mic_toggled", muted);
                        }
                    }
                })
                .build(),
        )
        .manage(window::WinState::default())
        .manage(clipboard::ClipState::default())
        .manage(vpn::VpnState::default())
        .manage(browser::BrowserState::default())
        .invoke_handler(tauri::generate_handler![
            window::set_hit_rect,
            window::set_through,
            window::set_capture_visible,
            window::begin_input,
            window::end_input,
            media::media_control,
            media::media_now,
            system::sys_now,
            system::toggle_windows_theme,
            audio::set_volume,
            audio::toggle_mute,
            audio::toggle_mic,
            clipboard::clip_list,
            clipboard::clip_set,
            clipboard::clip_paste,
            clipboard::clip_pin,
            clipboard::clip_delete,
            clipboard::clip_clear,
            clipboard::clip_image_data,
            clipboard::clip_save_edited,
            tools::open_snip,
            tools::lock_pc,
            tools::capture_screen,
            tools::pick_color,
            tools::open_path,
            tools::reveal_path,
            tools::shelf_meta,
            tools::pick_files,
            tools::default_screenshots_dir,
            tools::pick_save_image_path,
            tools::pick_folder,
            shelf_dnd::shelf_prepare_drop,
            shelf_dnd::shelf_copy_files,
            shelf_dnd::shelf_start_drag,
            converter::convert_probe,
            converter::convert_file,
            converter::convert_read_b64,
            converter::convert_save_canvas,
            converter::convert_zip_bundle,
            vpn::vpn_status,
            vpn::vpn_discover,
            vpn::vpn_pick_app,
            vpn::vpn_add_profile,
            vpn::vpn_select,
            vpn::vpn_remove,
            vpn::vpn_connect,
            vpn::vpn_disconnect,
            vpn::vpn_toggle,
            vpn::vpn_open_app,
            browser::browser_open,
            browser::browser_focus,
            browser::browser_hide,
            browser::browser_go,
            browser::browser_nav,
            browser::browser_open_external,
            weather::weather_now,
            connectivity::connectivity_status,
            connectivity::wifi_toggle,
            connectivity::wifi_connect,
            connectivity::wifi_disconnect,
            connectivity::bt_toggle,
            connectivity::bt_connect,
            connectivity::power_set,
            connectivity::open_win_settings,
        ])
        .setup(|app| {
            if let Ok(dir) = app.path().app_data_dir() {
                log::init(dir);
            }
            let win = app
                .get_webview_window("island")
                .expect("island window is missing");
            window::setup(&win)?;

            let handle = app.handle().clone();
            window::spawn_cursor_loop(handle.clone());
            media::spawn(handle.clone());
            system::spawn(handle.clone());
            clipboard::spawn(handle.clone());
            vpn::spawn(handle.clone());

            let gs = app.global_shortcut();
            for code in [Code::KeyI, Code::KeyV, Code::KeyM] {
                match gs.register(Shortcut::new(Some(MODS), code)) {
                    Ok(()) => log::line(format!("registered hotkey Ctrl+Alt+{code:?}")),
                    Err(e) => log::line(format!("hotkey {code:?} busy: {e}")),
                }
            }

            let quit = MenuItem::with_id(app, "quit", "Выйти из Kapla", true, None::<&str>)?;
            let open = MenuItem::with_id(app, "open", "Открыть остров", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&open, &quit])?;
            TrayIconBuilder::with_id("kapla")
                .icon(app.default_window_icon().cloned().expect("icon"))
                .tooltip("Kapla — Ctrl+Alt+I")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let _ = tray.app_handle().emit("panel", "toggle");
                    }
                })
                .on_menu_event(|app, e| match e.id.as_ref() {
                    "quit" => app.exit(0),
                    "open" => {
                        let _ = app.emit("panel", "open");
                    }
                    _ => {}
                })
                .build(app)?;

            win.show()?;
            window::apply_styles(&win)?;
            shelf_dnd::attach_drop_targets(&handle);
            let dnd_app = handle.clone();
            std::thread::spawn(move || {
                for delay in [500, 1500, 3500] {
                    std::thread::sleep(std::time::Duration::from_millis(delay));
                    shelf_dnd::attach_drop_targets(&dnd_app);
                }
            });
            Ok(())
        })
        .on_window_event(|win, event| {
            if win.label() != "island" {
                return;
            }
            if let WindowEvent::DragDrop(ev) = event {
                let app = win.app_handle();
                match ev {
                    DragDropEvent::Enter { .. } | DragDropEvent::Over { .. } => {
                        let _ = app.emit("shelf_drag", "enter");
                    }
                    DragDropEvent::Drop { paths, .. } => {
                        let strs: Vec<String> = paths
                            .iter()
                            .map(|p| p.to_string_lossy().into_owned())
                            .collect();
                        log::line(format!("window drag-drop: {} files", strs.len()));
                        if !strs.is_empty() {
                            let _ = app.emit("shelf_drop", strs);
                        }
                    }
                    DragDropEvent::Leave => {
                        let _ = app.emit("shelf_drag", "leave");
                    }
                    _ => {}
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running Kapla");
}
