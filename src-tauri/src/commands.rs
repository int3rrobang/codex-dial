use crate::models::UiState;
use crate::monitor::MonitorState;
use std::path::PathBuf;
use tauri::State;

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
pub async fn set_safety_buffer(monitor: State<'_, MonitorState>, value: f64) -> Result<UiState, String> {
    let mut m = monitor.lock().await;
    m.set_safety_buffer(value);
    Ok(m.ui_state())
}

#[tauri::command]
pub async fn set_launch_at_login(monitor: State<'_, MonitorState>, enabled: bool) -> Result<UiState, String> {
    let mut m = monitor.lock().await;
    m.set_launch_at_login(enabled);
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
    m.set_opencode_go_enabled(enabled);
    Ok(m.ui_state())
}
