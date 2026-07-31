use crate::models::UiState;
use crate::monitor::{MonitorState, ResetNotification};
use serde::Serialize;
use std::path::PathBuf;
use tauri::{AppHandle, State};

#[derive(Debug, Serialize)]
pub struct WorkArea {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

#[derive(Debug, Serialize)]
pub struct AccentPalette {
    pub accent: String,
    pub dark_1: String,
    pub dark_2: String,
    pub dark_3: String,
    pub light_1: String,
    pub light_2: String,
    pub light_3: String,
}

#[cfg(target_os = "windows")]
fn color_to_css(color: windows::UI::Color) -> String {
    format!("#{:02x}{:02x}{:02x}", color.R, color.G, color.B)
}

#[tauri::command]
pub fn get_accent_palette() -> Result<AccentPalette, String> {
    #[cfg(target_os = "windows")]
    {
        use windows::UI::ViewManagement::{UIColorType, UISettings};

        let settings = UISettings::new()
            .map_err(|_| "Could not access Windows color settings.".to_string())?;
        let color = |kind| {
            settings
                .GetColorValue(kind)
                .map(color_to_css)
                .map_err(|_| "Could not read the Windows accent palette.".to_string())
        };

        return Ok(AccentPalette {
            accent: color(UIColorType::Accent)?,
            dark_1: color(UIColorType::AccentDark1)?,
            dark_2: color(UIColorType::AccentDark2)?,
            dark_3: color(UIColorType::AccentDark3)?,
            light_1: color(UIColorType::AccentLight1)?,
            light_2: color(UIColorType::AccentLight2)?,
            light_3: color(UIColorType::AccentLight3)?,
        });
    }

    #[cfg(not(target_os = "windows"))]
    Err("Accent palette detection is only supported on Windows.".to_string())
}

#[tauri::command]
pub fn get_work_area() -> Result<WorkArea, String> {
    #[cfg(target_os = "windows")]
    {
        use std::mem::size_of;
        use windows_sys::Win32::Foundation::POINT;
        use windows_sys::Win32::Graphics::Gdi::{
            GetMonitorInfoW, MonitorFromPoint, MONITORINFO, MONITOR_DEFAULTTOPRIMARY,
        };
        use windows_sys::Win32::UI::Shell::{
            SHAppBarMessage, ABE_BOTTOM, ABE_LEFT, ABE_RIGHT, ABE_TOP, ABM_GETTASKBARPOS,
            APPBARDATA,
        };

        let monitor = unsafe { MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY) };
        if monitor == std::ptr::null_mut() {
            return Err("Could not find the primary monitor.".to_string());
        }

        let mut info = MONITORINFO {
            cbSize: size_of::<MONITORINFO>() as u32,
            rcMonitor: unsafe { std::mem::zeroed() },
            rcWork: unsafe { std::mem::zeroed() },
            dwFlags: 0,
        };
        if unsafe { GetMonitorInfoW(monitor, &mut info) } == 0 {
            return Err("Could not read the primary monitor work area.".to_string());
        }

        // Windows reports the full monitor as rcWork when the taskbar auto-hides.
        // Reserve its revealed rectangle as well so the popup never sits underneath it.
        let mut work = info.rcWork;
        let mut taskbar = APPBARDATA {
            cbSize: size_of::<APPBARDATA>() as u32,
            hWnd: std::ptr::null_mut(),
            uCallbackMessage: 0,
            uEdge: 0,
            rc: unsafe { std::mem::zeroed() },
            lParam: 0,
        };
        if unsafe { SHAppBarMessage(ABM_GETTASKBARPOS, &mut taskbar) } != 0 {
            match taskbar.uEdge {
                ABE_LEFT => work.left = work.left.max(taskbar.rc.right),
                ABE_TOP => work.top = work.top.max(taskbar.rc.bottom),
                ABE_RIGHT => work.right = work.right.min(taskbar.rc.left),
                ABE_BOTTOM => work.bottom = work.bottom.min(taskbar.rc.top),
                _ => {}
            }
        }

        return Ok(WorkArea {
            x: work.left,
            y: work.top,
            width: work.right - work.left,
            height: work.bottom - work.top,
        });
    }

    #[cfg(not(target_os = "windows"))]
    Err("Work area detection is only supported on Windows.".to_string())
}

#[tauri::command]
pub fn set_flyout_bounds(
    window: tauri::WebviewWindow,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
) -> Result<(), String> {
    if width <= 0 || height <= 0 {
        return Err("Flyout width and height must be positive.".to_string());
    }

    #[cfg(target_os = "windows")]
    {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            SetWindowPos, SWP_NOACTIVATE, SWP_NOZORDER,
        };

        let hwnd = window
            .hwnd()
            .map_err(|_| "Could not access the flyout window handle.".to_string())?;
        if hwnd.0.is_null() {
            return Err("Could not access the flyout window handle.".to_string());
        }

        let moved = unsafe {
            SetWindowPos(
                hwnd.0,
                std::ptr::null_mut(),
                x,
                y,
                width,
                height,
                SWP_NOACTIVATE | SWP_NOZORDER,
            )
        };
        if moved == 0 {
            return Err("Could not set the flyout window bounds.".to_string());
        }
        return Ok(());
    }

    #[cfg(not(target_os = "windows"))]
    {
        window
            .set_size(tauri::PhysicalSize::new(width as u32, height as u32))
            .map_err(|_| "Could not set the flyout window size.".to_string())?;
        window
            .set_position(tauri::PhysicalPosition::new(x, y))
            .map_err(|_| "Could not set the flyout window position.".to_string())?;
        Ok(())
    }
}

#[tauri::command]
pub async fn get_state(monitor: State<'_, MonitorState>) -> Result<UiState, String> {
    let m = monitor.lock().await;
    Ok(m.ui_state())
}

#[tauri::command]
pub async fn refresh(app: AppHandle, monitor: State<'_, MonitorState>) -> Result<UiState, String> {
    let (state, notifications) = {
        let mut m = monitor.lock().await;
        let notifications = m.refresh().await;
        (m.ui_state(), notifications)
    };
    notify_reset_notifications(&app, notifications);
    Ok(state)
}

pub fn notify_reset_notifications<R: tauri::Runtime>(
    app: &AppHandle<R>,
    notifications: Vec<ResetNotification>,
) {
    use tauri_plugin_notification::{NotificationExt, PermissionState};

    let permission_granted = match app.notification().permission_state() {
        Ok(PermissionState::Granted) => true,
        Ok(PermissionState::Prompt | PermissionState::PromptWithRationale) => {
            matches!(
                app.notification().request_permission(),
                Ok(PermissionState::Granted)
            )
        }
        Ok(PermissionState::Denied) => false,
        Err(_) => true,
    };
    if !permission_granted {
        return;
    }

    for notification in notifications {
        let hours = notification.threshold_hours;
        let body = format!("Your next banked reset expires in less than {hours} hours.");
        let _ = app
            .notification()
            .builder()
            .title("Banked reset expiring soon")
            .body(body)
            .show();
    }
}

#[tauri::command]
pub async fn set_safety_buffer(
    monitor: State<'_, MonitorState>,
    value: f64,
) -> Result<UiState, String> {
    let mut m = monitor.lock().await;
    m.set_safety_buffer(value);
    Ok(m.ui_state())
}

#[tauri::command]
pub async fn set_launch_at_login(
    monitor: State<'_, MonitorState>,
    enabled: bool,
) -> Result<UiState, String> {
    let mut m = monitor.lock().await;
    m.set_launch_at_login(enabled);
    Ok(m.ui_state())
}

#[tauri::command]
pub async fn set_reset_notifications_enabled(
    monitor: State<'_, MonitorState>,
    enabled: bool,
) -> Result<UiState, String> {
    let mut m = monitor.lock().await;
    m.set_reset_notifications_enabled(enabled);
    Ok(m.ui_state())
}

#[tauri::command]
pub async fn set_codex_enabled(
    monitor: State<'_, MonitorState>,
    enabled: bool,
) -> Result<UiState, String> {
    let mut m = monitor.lock().await;
    m.set_codex_enabled(enabled)?;
    Ok(m.ui_state())
}
#[tauri::command]
pub async fn choose_sync_folder(
    monitor: State<'_, MonitorState>,
    app: tauri::AppHandle,
) -> Result<UiState, String> {
    let folder = tauri_plugin_dialog::DialogExt::dialog(&app)
        .file()
        .blocking_pick_folder();

    match folder {
        Some(path) => {
            let path_buf = PathBuf::from(path.to_string());
            let mut m = monitor.lock().await;
            m.connect_sync_folder(path_buf);
            Ok(m.ui_state())
        }
        None => {
            let m = monitor.lock().await;
            Ok(m.ui_state())
        }
    }
}

#[tauri::command]
pub async fn stop_sync(monitor: State<'_, MonitorState>) -> Result<UiState, String> {
    let mut m = monitor.lock().await;
    m.stop_sync();
    Ok(m.ui_state())
}

#[tauri::command]
pub async fn set_opencode_cookie(
    monitor: State<'_, MonitorState>,
    cookie: Option<String>,
) -> Result<UiState, String> {
    let mut m = monitor.lock().await;
    m.set_opencode_cookie(cookie);
    Ok(m.ui_state())
}

#[tauri::command]
pub async fn set_opencode_workspace_id(
    monitor: State<'_, MonitorState>,
    id: Option<String>,
) -> Result<UiState, String> {
    let mut m = monitor.lock().await;
    m.set_opencode_workspace_id(id);
    Ok(m.ui_state())
}

#[tauri::command]
pub async fn set_opencode_go_enabled(
    monitor: State<'_, MonitorState>,
    enabled: bool,
) -> Result<UiState, String> {
    let mut m = monitor.lock().await;
    m.set_opencode_go_enabled(enabled)?;
    Ok(m.ui_state())
}
