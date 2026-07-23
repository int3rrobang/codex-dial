use crate::codex_client;
use crate::forecast_engine;
use crate::models::*;
use crate::usage_history::UsageHistory;
use std::path::PathBuf;

pub struct Monitor {
    pub snapshot: Option<UsageSnapshot>,
    pub forecast: Option<Forecast>,
    pub samples: Vec<UsageSample>,
    pub is_refreshing: bool,
    pub error_message: Option<String>,
    pub sync_folder_name: Option<String>,
    pub sync_error_message: Option<String>,
    pub safety_buffer: f64,
    pub launch_at_login: bool,
    previous_status: Option<PaceStatus>,
    history: UsageHistory,
    config_path: PathBuf,
}

#[derive(serde::Serialize, serde::Deserialize, Default)]
struct PersistedConfig {
    safety_buffer: Option<f64>,
    launch_at_login: Option<bool>,
    sync_folder: Option<String>,
    previous_status: Option<PaceStatus>,
}

impl Monitor {
    pub fn new() -> Self {
        let app_dir = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("com.github.thrr87.CodexLimits");
        let history_dir = app_dir.join("History");
        let config_path = app_dir.join("config.json");

        let _ = std::fs::create_dir_all(&app_dir);

        let installation_id = get_or_create_installation_id(&app_dir);
        let config = load_config(&config_path);

        let mut history = UsageHistory::new(history_dir, installation_id);
        let state = history.load();

        let mut monitor = Self {
            snapshot: None,
            forecast: None,
            samples: state.samples,
            is_refreshing: false,
            error_message: None,
            sync_folder_name: state.folder_name,
            sync_error_message: state.error_message,
            safety_buffer: config.safety_buffer.unwrap_or(3.0),
            launch_at_login: config.launch_at_login.unwrap_or(true),
            previous_status: config.previous_status,
            history,
            config_path,
        };

        if let Some(sync_path) = config.sync_folder {
            let path = PathBuf::from(sync_path);
            if path.exists() {
                let state = monitor.history.connect(path);
                monitor.samples = state.samples;
                monitor.sync_folder_name = state.folder_name;
                monitor.sync_error_message = state.error_message;
            }
        }

        monitor
    }

    pub async fn refresh(&mut self) {
        if self.is_refreshing {
            return;
        }
        self.is_refreshing = true;

        let sync_state = self.history.synchronize();
        self.samples = sync_state.samples;
        self.sync_folder_name = sync_state.folder_name;
        self.sync_error_message = sync_state.error_message;

        match codex_client::fetch().await {
            Ok(snapshot) => {
                let window = &snapshot.main_limit.window;
                let sample = UsageSample {
                    observed_at: snapshot.fetched_at,
                    remaining_percent: window.remaining_percent.round() as u32,
                    resets_at: window.resets_at,
                };

                let state = self.history.record(sample);
                self.samples = state.samples;
                self.sync_folder_name = state.folder_name;
                if state.error_message.is_none() {
                    self.sync_error_message = None;
                }

                self.snapshot = Some(snapshot);
                self.error_message = None;
                self.recalculate();
            }
            Err(e) => {
                self.error_message = Some(e.to_string());
            }
        }

        self.is_refreshing = false;
    }

    pub fn recalculate(&mut self) {
        let snapshot = match &self.snapshot {
            Some(s) => s.clone(),
            None => return,
        };

        let forecast = forecast_engine::evaluate(
            &snapshot.main_limit.window,
            &self.samples,
            &snapshot.token_history,
            self.safety_buffer,
            snapshot.fetched_at,
            self.previous_status,
        );

        self.previous_status = Some(forecast.status);
        self.forecast = Some(forecast);
        self.persist_config();
    }

    pub fn set_safety_buffer(&mut self, value: f64) {
        self.safety_buffer = value;
        self.recalculate();
    }

    pub fn set_launch_at_login(&mut self, enabled: bool) {
        self.launch_at_login = enabled;
        set_registry_run_key(enabled);
        self.persist_config();
    }

    pub fn connect_sync_folder(&mut self, path: PathBuf) {
        let state = self.history.connect(path.clone());
        self.samples = state.samples;
        let connected = state.folder_name.is_some();
        self.sync_folder_name = state.folder_name;
        self.sync_error_message = state.error_message;

        if connected {
            self.update_config(|c| c.sync_folder = Some(path.to_string_lossy().to_string()));
        }
    }

    pub fn stop_sync(&mut self) {
        let state = self.history.disconnect();
        self.samples = state.samples;
        self.sync_folder_name = None;
        self.sync_error_message = None;
        self.update_config(|c| c.sync_folder = None);
    }

    pub fn ui_state(&self) -> UiState {
        UiState {
            snapshot: self.snapshot.clone(),
            forecast: self.forecast.clone(),
            samples: self.samples.clone(),
            is_refreshing: self.is_refreshing,
            error_message: self.error_message.clone(),
            sync_folder_name: self.sync_folder_name.clone(),
            sync_error_message: self.sync_error_message.clone(),
            safety_buffer: self.safety_buffer,
            launch_at_login: self.launch_at_login,
        }
    }

    fn persist_config(&self) {
        self.update_config(|c| {
            c.safety_buffer = Some(self.safety_buffer);
            c.launch_at_login = Some(self.launch_at_login);
            c.previous_status = self.previous_status;
        });
    }

    fn update_config(&self, f: impl FnOnce(&mut PersistedConfig)) {
        let mut config = load_config(&self.config_path);
        f(&mut config);
        if let Ok(json) = serde_json::to_string_pretty(&config) {
            let _ = std::fs::write(&self.config_path, json);
        }
    }
}

fn get_or_create_installation_id(app_dir: &PathBuf) -> String {
    let id_path = app_dir.join("installation_id");
    if let Ok(id) = std::fs::read_to_string(&id_path) {
        let trimmed = id.trim().to_lowercase();
        if !trimmed.is_empty() {
            return trimmed;
        }
    }
    let id = uuid::Uuid::new_v4().to_string().to_lowercase();
    let _ = std::fs::write(&id_path, &id);
    id
}

fn load_config(path: &PathBuf) -> PersistedConfig {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|data| serde_json::from_str(&data).ok())
        .unwrap_or_default()
}

fn set_registry_run_key(enabled: bool) {
    use winreg::enums::*;
    use winreg::RegKey;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let run_key = hkcu.open_subkey_with_flags(
        r"Software\Microsoft\Windows\CurrentVersion\Run",
        KEY_SET_VALUE,
    );

    match run_key {
        Ok(key) => {
            if enabled {
                let exe_path = std::env::current_exe()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_default();
                let _ = key.set_value("CodexDial", &exe_path);
            } else {
                let _ = key.delete_value("CodexDial");
            }
        }
        Err(_) => {}
    }
}

pub type MonitorState = tokio::sync::Mutex<Monitor>;
