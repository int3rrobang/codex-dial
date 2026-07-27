#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use codex_dial::commands;
use codex_dial::monitor::{Monitor, MonitorState};
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Emitter, Manager,
};

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_process::init())
        .manage(MonitorState::new(Monitor::new()))
        .invoke_handler(tauri::generate_handler![
            commands::get_work_area,
            commands::get_state,
            commands::refresh,
            commands::set_safety_buffer,
            commands::set_launch_at_login,
            commands::choose_sync_folder,
            commands::set_codex_enabled,
            commands::stop_sync,
            commands::set_opencode_cookie,
            commands::set_opencode_workspace_id,
            commands::set_opencode_go_enabled,
        ])
        .setup(|app| {
            let refresh_item = MenuItem::with_id(app, "refresh", "Refresh", true, None::<&str>)?;
            let settings_item = MenuItem::with_id(app, "settings", "Settings", true, None::<&str>)?;
            let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;

            let menu = Menu::with_items(app, &[&refresh_item, &settings_item, &quit_item])?;

            let _tray = TrayIconBuilder::with_id("codex-limit")
                .icon(app.default_window_icon().cloned().unwrap())
                .menu(&menu)
                .tooltip("Codex Dial")
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "refresh" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                            let _ = window.emit("trigger-refresh", ());
                        }
                    }
                    "settings" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                            let _ = window.emit("navigate-settings", ());
                        }
                    }
                    "quit" => {
                        app.exit(0);
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        if let Some(window) = app.get_webview_window("main") {
                            if window.is_visible().unwrap_or(false) {
                                let _ = window.hide();
                            } else {
                                let _ = window.show();
                                let _ = window.set_focus();
                            }
                        }
                    }
                })
                .build(app)?;

            // Start at the taskbar edge; the frontend refines the size once
            // its content has rendered.
            if let Some(window) = app.get_webview_window("main") {
                if let Ok(Some(monitor)) = window.primary_monitor() {
                    let scale = monitor.scale_factor();
                    let work_area = monitor.work_area();
                    let win_width = 380.0 * scale;
                    let win_height = 480.0 * scale;
                    let x = work_area.position.x as f64 + work_area.size.width as f64
                        - win_width
                        - 16.0 * scale;
                    let y = work_area.position.y as f64 + work_area.size.height as f64 - win_height;
                    let _ = window.set_position(tauri::PhysicalPosition::new(x as i32, y as i32));
                }
            }

            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                // Initial refresh on launch
                {
                    let state = app_handle.state::<MonitorState>();
                    let mut monitor = state.lock().await;
                    monitor.refresh().await;
                }

                // Heartbeat: refresh every 10 min, or immediately on wake-from-sleep
                let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
                let mut last_refresh = tokio::time::Instant::now();
                let mut last_tick = tokio::time::Instant::now();

                loop {
                    interval.tick().await;
                    let now = tokio::time::Instant::now();
                    let gap = now.duration_since(last_tick);
                    last_tick = now;

                    let time_since_refresh = now.duration_since(last_refresh);
                    let woke_from_sleep = gap > std::time::Duration::from_secs(60);
                    let interval_elapsed =
                        time_since_refresh >= std::time::Duration::from_secs(600);

                    if woke_from_sleep || interval_elapsed {
                        let state = app_handle.state::<MonitorState>();
                        let mut monitor = state.lock().await;
                        monitor.refresh().await;
                        last_refresh = tokio::time::Instant::now();
                    }
                }
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Codex Dial");
}
