#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use codex_dial::commands;
use codex_dial::monitor::{Monitor, MonitorState};
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Emitter, Listener, Manager, Runtime, Theme, WindowEvent,
};

#[cfg(windows)]
use tauri::utils::config::Color;

#[cfg(windows)]
use std::{ffi::c_void, mem::size_of};

#[cfg(windows)]
use windows_sys::Win32::Graphics::Dwm::{
    DwmSetWindowAttribute, DWMSBT_TRANSIENTWINDOW, DWMWA_SYSTEMBACKDROP_TYPE,
    DWMWA_USE_IMMERSIVE_DARK_MODE, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND,
};
#[cfg(windows)]
use windows_sys::Win32::UI::WindowsAndMessaging::GetForegroundWindow;

struct FlyoutBehavior {
    light_dismiss: AtomicBool,
}

impl Default for FlyoutBehavior {
    fn default() -> Self {
        Self {
            light_dismiss: AtomicBool::new(true),
        }
    }
}

#[cfg(windows)]
fn set_dwm_attribute<T>(hwnd: *mut c_void, attribute: i32, value: &T) -> bool {
    unsafe {
        DwmSetWindowAttribute(
            hwnd,
            attribute as u32,
            value as *const T as *const c_void,
            size_of::<T>() as u32,
        ) >= 0
    }
}

#[cfg(windows)]
fn apply_native_window_attributes<R: Runtime>(
    window: &tauri::WebviewWindow<R>,
    theme: Option<Theme>,
) -> bool {
    let fallback_color = if matches!(theme, Some(Theme::Dark)) {
        Color(32, 32, 32, 255)
    } else {
        Color(243, 243, 243, 255)
    };

    let hwnd = match window.hwnd() {
        Ok(hwnd) if !hwnd.0.is_null() => hwnd.0,
        _ => {
            let _ = window.set_background_color(Some(fallback_color));
            return false;
        }
    };

    let backdrop_applied =
        set_dwm_attribute(hwnd, DWMWA_SYSTEMBACKDROP_TYPE, &DWMSBT_TRANSIENTWINDOW);
    if !backdrop_applied {
        let _ = window.set_background_color(Some(fallback_color));
    }

    let rounded = DWMWCP_ROUND;
    let _ = set_dwm_attribute(hwnd, DWMWA_WINDOW_CORNER_PREFERENCE, &rounded);

    let dark_mode: i32 = if matches!(theme, Some(Theme::Dark)) {
        1
    } else {
        0
    };
    let _ = set_dwm_attribute(hwnd, DWMWA_USE_IMMERSIVE_DARK_MODE, &dark_mode);

    backdrop_applied
}

#[cfg(not(windows))]
fn apply_native_window_attributes<R: Runtime>(
    _window: &tauri::WebviewWindow<R>,
    _theme: Option<Theme>,
) -> bool {
    true
}

#[cfg(windows)]
fn is_flyout_foreground<R: Runtime>(window: &tauri::WebviewWindow<R>) -> bool {
    let Ok(hwnd) = window.hwnd() else {
        return true;
    };
    let foreground = unsafe { GetForegroundWindow() };
    foreground.is_null() || foreground == hwnd.0
}

#[cfg(not(windows))]
fn is_flyout_foreground<R: Runtime>(_window: &tauri::WebviewWindow<R>) -> bool {
    true
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_process::init())
        .manage(FlyoutBehavior::default())
        .manage(MonitorState::new(Monitor::new()))
        .invoke_handler(tauri::generate_handler![
            commands::get_work_area,
            commands::get_accent_palette,
            commands::set_flyout_bounds,
            commands::get_state,
            commands::refresh,
            commands::set_safety_buffer,
            commands::set_launch_at_login,
            commands::choose_sync_folder,
            commands::set_codex_enabled,
            commands::stop_sync,
            commands::set_opencode_cookie,
            commands::set_opencode_workspace_id,
            commands::set_reset_notifications_enabled,
            commands::set_opencode_go_enabled,
        ])
        .setup(|app| {
            let dismiss_events = app.handle().clone();
            app.listen("light-dismiss-enabled", move |event| {
                if let Ok(enabled) = serde_json::from_str::<bool>(event.payload()) {
                    dismiss_events
                        .state::<FlyoutBehavior>()
                        .light_dismiss
                        .store(enabled, Ordering::Relaxed);
                }
            });

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
                            let _ = window.emit("window-shown", ());
                            let _ = window.emit("trigger-refresh", ());
                        }
                    }
                    "settings" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                            let _ = window.emit("window-shown", ());
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
                                let _ = window.emit("window-shown", ());
                            }
                        }
                    }
                })
                .build(app)?;

            // Start at the taskbar edge; the frontend refines the size once
            // its content has rendered.
            if let Some(window) = app.get_webview_window("main") {
                let _ = apply_native_window_attributes(&window, window.theme().ok());
                let focus_app = app.handle().clone();
                let native_window = window.clone();
                window.on_window_event(move |event| match event {
                    WindowEvent::CloseRequested { api, .. } => {
                        api.prevent_close();
                        let _ = native_window.hide();
                    }
                    WindowEvent::Focused(false) => {
                        let dismiss_window = native_window.clone();
                        let dismiss_app = focus_app.clone();
                        tauri::async_runtime::spawn(async move {
                            tokio::time::sleep(std::time::Duration::from_millis(220)).await;
                            let dismiss_enabled = dismiss_app
                                .state::<FlyoutBehavior>()
                                .light_dismiss
                                .load(Ordering::Relaxed);
                            if dismiss_enabled
                                && dismiss_window.is_visible().unwrap_or(false)
                                && !is_flyout_foreground(&dismiss_window)
                            {
                                let _ = dismiss_window.hide();
                            }
                        });
                    }
                    WindowEvent::ThemeChanged(theme) => {
                        let _ = apply_native_window_attributes(&native_window, Some(*theme));
                    }
                    _ => {}
                });
                if let Ok(Some(monitor)) = window.primary_monitor() {
                    let scale = monitor.scale_factor();
                    let work_area = monitor.work_area();
                    let win_width = 380.0 * scale;
                    let win_height = 480.0 * scale;
                    let x = work_area.position.x as f64 + work_area.size.width as f64
                        - win_width
                        - 12.0 * scale;
                    let y = work_area.position.y as f64 + work_area.size.height as f64
                        - win_height
                        - 12.0 * scale;
                    let _ = window.set_position(tauri::PhysicalPosition::new(x as i32, y as i32));
                }
            }

            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                // Initial refresh on launch
                refresh_and_notify(&app_handle).await;

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
                        refresh_and_notify(&app_handle).await;
                        last_refresh = tokio::time::Instant::now();
                    }
                }
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Codex Dial");
}

async fn refresh_and_notify(app_handle: &tauri::AppHandle) {
    let state = app_handle.state::<MonitorState>();
    let notifications = {
        let mut monitor = state.lock().await;
        monitor.refresh().await
    };
    commands::notify_reset_notifications(app_handle, notifications);
    let _ = app_handle.emit("state-updated", ());
}
