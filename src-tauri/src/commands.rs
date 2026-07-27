use crate::models::UiState;
use crate::monitor::MonitorState;
use serde::Serialize;
use std::path::PathBuf;
use tauri::State;

#[derive(Debug, Serialize)]
pub struct WorkArea {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
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
pub async fn get_state(monitor: State<'_, MonitorState>) -> Result<UiState, String> {
    let m = monitor.lock().await;
    Ok(m.ui_state())
}

#[tauri::command]
pub async fn refresh(monitor: State<'_, MonitorState>) -> Result<UiState, String> {
    let mut m = monitor.lock().await;
    m.refresh().await;
    Ok(m.ui_state())
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
