use crate::codex_client;
use crate::forecast_engine;
use crate::models::*;
use crate::opencode_client;
use crate::usage_history::UsageHistory;
use std::path::PathBuf;

const RESET_NOTIFICATION_12_HOURS: i64 = 12 * 60 * 60;
const RESET_NOTIFICATION_6_HOURS: i64 = 6 * 60 * 60;
const RESET_NOTIFICATION_12_MASK: u8 = 1;
const RESET_NOTIFICATION_6_MASK: u8 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResetNotification {
    pub threshold_hours: u8,
}

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
struct ResetNotificationState {
    expires_at: i64,
    sent_mask: u8,
}

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
    pub codex_enabled: bool,
    pub reset_notifications_enabled: bool,
    pub opencode_go: Option<OpenCodeGoSnapshot>,
    pub opencode_go_error: Option<String>,
    pub opencode_go_forecasts: Vec<Forecast>,
    pub opencode_go_samples: Vec<UsageSample>,
    pub opencode_go_enabled: bool,
    opencode_api_key: Option<String>,
    opencode_go_previous_statuses: Vec<PaceStatus>,
    previous_status: Option<PaceStatus>,
    history: UsageHistory,
    config_path: PathBuf,
    reset_notification_state: Option<ResetNotificationState>,
}

#[derive(serde::Serialize, serde::Deserialize, Default)]
struct PersistedConfig {
    safety_buffer: Option<f64>,
    launch_at_login: Option<bool>,
    sync_folder: Option<String>,
    codex_enabled: Option<bool>,
    previous_status: Option<PaceStatus>,
    opencode_api_key: Option<String>,
    opencode_go_enabled: Option<bool>,
    reset_notifications_enabled: Option<bool>,
    reset_notification_state: Option<ResetNotificationState>,
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

        let config_exists = config_path.exists();
        let config = load_config(&config_path);
        // A brand-new install starts unconfigured. Existing installs that
        // predate the explicit provider setting retain the historical Codex
        let codex_enabled = default_codex_enabled(config_exists, config.codex_enabled);
        // The first launch may have neither provider configured.
        let opencode_go_enabled = config.opencode_go_enabled.unwrap_or(false);

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
            reset_notifications_enabled: config.reset_notifications_enabled.unwrap_or(true),
            codex_enabled,
            opencode_go: None,
            opencode_go_error: None,
            opencode_go_forecasts: Vec::new(),
            opencode_go_samples: Vec::new(),
            opencode_go_enabled,
            opencode_api_key: config.opencode_api_key.clone(),
            opencode_go_previous_statuses: Vec::new(),
            previous_status: config.previous_status,
            history,
            config_path,
            reset_notification_state: config.reset_notification_state,
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

    pub async fn refresh(&mut self) -> Vec<ResetNotification> {
        if self.is_refreshing {
            return Vec::new();
        }
        self.is_refreshing = true;
        let mut notifications = Vec::new();

        let sync_state = self.history.synchronize();
        self.samples = sync_state.samples;
        self.sync_folder_name = sync_state.folder_name;
        self.sync_error_message = sync_state.error_message;

        let codex_enabled = self.codex_enabled;
        let api_key = self.opencode_api_key.clone();
        let ocg_enabled = self.opencode_go_enabled;

        if !codex_enabled {
            self.snapshot = None;
            self.forecast = None;
            self.error_message = None;
        }
        if !ocg_enabled {
            self.opencode_go = None;
            self.opencode_go_error = None;
            self.opencode_go_forecasts.clear();
            self.opencode_go_samples.clear();
            self.opencode_go_previous_statuses.clear();
        }

        let (codex_result, opencode_result) = tokio::join!(
            async {
                if codex_enabled {
                    Some(codex_client::fetch().await)
                } else {
                    None
                }
            },
            async {
                if ocg_enabled {
                    Some(opencode_client::fetch(&api_key).await)
                } else {
                    None
                }
            }
        );

        if let Some(result) = codex_result {
            match result {
                Ok(snapshot) => {
                    let window = &snapshot.main_limit.window;
                    let previous_reset = self
                        .snapshot
                        .as_ref()
                        .map(|s| s.main_limit.window.resets_at)
                        .or_else(|| {
                            self.samples
                                .iter()
                                .max_by_key(|s| s.observed_at)
                                .map(|s| s.resets_at)
                        });
                    let reset_detected = previous_reset != Some(window.resets_at)
                        || self.snapshot.as_ref().is_some_and(|previous| {
                            previous.main_limit.window.resets_at == window.resets_at
                                && window.remaining_percent
                                    >= previous.main_limit.window.remaining_percent + 10.0
                        });
                    if reset_detected {
                        // Hysteresis belongs to one reset window. Carrying a
                        // prior window's status into an early reset can keep
                        // the plain-language prediction stale.
                        self.previous_status = None;
                    }

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

                    notifications.extend(self.collect_reset_notifications(&snapshot));
                    self.snapshot = Some(snapshot);
                    self.error_message = None;
                    self.recalculate();
                }
                Err(e) => {
                    self.error_message = Some(e.to_string());
                }
            }
        }

        if let Some(result) = opencode_result {
            match result {
                Ok(snapshot) => {
                    for w in &snapshot.windows {
                        let sample = UsageSample {
                            observed_at: snapshot.fetched_at,
                            remaining_percent: w.remaining_percent.round() as u32,
                            resets_at: w.resets_at,
                        };
                        self.opencode_go_samples
                            .retain(|s| s.resets_at != w.resets_at);
                        self.opencode_go_samples.push(sample);
                    }

                    self.opencode_go = Some(snapshot);
                    self.recalculate_opencode_go();
                    self.opencode_go_error = None;
                }
                Err(error @ opencode_client::OpenCodeError::NotConfigured) => {
                    self.opencode_go = None;
                    self.opencode_go_error = Some(error.to_string());
                    self.opencode_go_forecasts.clear();
                    self.opencode_go_samples.clear();
                    self.opencode_go_previous_statuses.clear();
                }
                Err(e) => {
                    self.opencode_go_error = Some(e.to_string());
                }
            }
        }

        self.is_refreshing = false;
        notifications
    }

    fn collect_reset_notifications(&mut self, snapshot: &UsageSnapshot) -> Vec<ResetNotification> {
        let next_expiry = snapshot
            .banked_reset_credits
            .iter()
            .map(|credit| credit.expires_at)
            .min();
        let Some(expires_at) = next_expiry else {
            self.reset_notification_state = None;
            return Vec::new();
        };

        if self
            .reset_notification_state
            .is_none_or(|state| state.expires_at != expires_at)
        {
            self.reset_notification_state = Some(ResetNotificationState {
                expires_at,
                sent_mask: 0,
            });
        }

        pending_reset_notifications(
            self.reset_notifications_enabled,
            expires_at,
            snapshot.fetched_at,
            &mut self.reset_notification_state,
        )
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

    fn recalculate_opencode_go(&mut self) {
        let snapshot = match &self.opencode_go {
            Some(snapshot) => snapshot.clone(),
            None => {
                self.opencode_go_forecasts.clear();
                return;
            }
        };

        let mut forecasts = Vec::new();
        self.opencode_go_previous_statuses
            .resize(snapshot.windows.len(), PaceStatus::OnTrack);
        for (i, w) in snapshot.windows.iter().enumerate() {
            let window = UsageWindow {
                remaining_percent: w.remaining_percent,
                resets_at: w.resets_at,
                duration_minutes: w.duration_minutes,
            };
            let window_samples: Vec<UsageSample> = self
                .opencode_go_samples
                .iter()
                .filter(|s| s.resets_at == w.resets_at)
                .cloned()
                .collect();
            let forecast = forecast_engine::evaluate(
                &window,
                &window_samples,
                &[],
                self.safety_buffer,
                snapshot.fetched_at,
                Some(self.opencode_go_previous_statuses[i]),
            );
            self.opencode_go_previous_statuses[i] = forecast.status;
            forecasts.push(forecast);
        }
        self.opencode_go_forecasts = forecasts;
    }

    pub fn set_safety_buffer(&mut self, value: f64) {
        self.safety_buffer = value;
        self.recalculate();
        self.recalculate_opencode_go();
    }

    pub fn set_launch_at_login(&mut self, enabled: bool) {
        self.launch_at_login = enabled;
        set_registry_run_key(enabled);
        self.persist_config();
    }

    pub fn set_reset_notifications_enabled(&mut self, enabled: bool) {
        self.reset_notifications_enabled = enabled;
        self.update_config(|c| c.reset_notifications_enabled = Some(enabled));
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

    pub fn set_codex_enabled(&mut self, enabled: bool) -> Result<(), String> {
        self.codex_enabled = enabled;
        if !enabled {
            self.snapshot = None;
            self.forecast = None;
            self.error_message = None;
        }
        self.update_config(|c| c.codex_enabled = Some(enabled));
        Ok(())
    }

    pub async fn apply_codex_reset_credit(
        &mut self,
        credit_id: String,
        idempotency_key: String,
    ) -> Result<(), String> {
        if uuid::Uuid::parse_str(&idempotency_key).is_err() {
            return Err("The idempotency key must be a valid UUID.".to_string());
        }
        if credit_id.trim().is_empty() {
            return Err("That reset credit is not currently available.".to_string());
        }

        let available = self.snapshot.as_ref().is_some_and(|snapshot| {
            snapshot
                .banked_reset_credits
                .iter()
                .any(|credit| credit.id == credit_id)
        });
        if !available {
            return Err("That reset credit is not currently available.".to_string());
        }

        match codex_client::consume_reset_credit(&credit_id, &idempotency_key)
            .await
            .map_err(|error| error.to_string())?
        {
            codex_client::ConsumeOutcome::Reset
            | codex_client::ConsumeOutcome::AlreadyRedeemed => {}
        }

        self.refresh().await;
        Ok(())
    }
    pub fn set_opencode_api_key(&mut self, api_key: Option<String>) {
        let api_key = api_key
            .map(|key| key.trim().to_string())
            .filter(|key| !key.is_empty());
        self.opencode_api_key = api_key.clone();
        self.update_config(|c| c.opencode_api_key = api_key);
    }

    pub fn set_opencode_go_enabled(&mut self, enabled: bool) -> Result<(), String> {
        self.opencode_go_enabled = enabled;
        if !enabled {
            self.opencode_go = None;
            self.opencode_go_error = None;
            self.opencode_go_forecasts.clear();
            self.opencode_go_samples.clear();
            self.opencode_go_previous_statuses.clear();
        }
        self.update_config(|c| c.opencode_go_enabled = Some(enabled));
        Ok(())
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
            codex_enabled: self.codex_enabled,
            opencode_go: self.opencode_go.clone(),
            opencode_go_error: self.opencode_go_error.clone(),
            opencode_go_forecasts: self.opencode_go_forecasts.clone(),
            opencode_go_samples: self.opencode_go_samples.clone(),
            opencode_go_enabled: self.opencode_go_enabled,
            reset_notifications_enabled: self.reset_notifications_enabled,
        }
    }

    fn persist_config(&self) {
        self.update_config(|c| {
            c.safety_buffer = Some(self.safety_buffer);
            c.launch_at_login = Some(self.launch_at_login);
            c.previous_status = self.previous_status;
            c.reset_notifications_enabled = Some(self.reset_notifications_enabled);
            c.reset_notification_state = self.reset_notification_state;
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

fn pending_reset_notifications(
    enabled: bool,
    expires_at: i64,
    now: i64,
    state: &mut Option<ResetNotificationState>,
) -> Vec<ResetNotification> {
    if !enabled {
        return Vec::new();
    }

    let remaining = expires_at - now;
    if remaining <= 0 {
        return Vec::new();
    }

    let notification_state = state.get_or_insert(ResetNotificationState {
        expires_at,
        sent_mask: 0,
    });
    if notification_state.expires_at != expires_at {
        notification_state.expires_at = expires_at;
        notification_state.sent_mask = 0;
    }

    if remaining < RESET_NOTIFICATION_6_HOURS
        && notification_state.sent_mask & RESET_NOTIFICATION_6_MASK == 0
    {
        notification_state.sent_mask |= RESET_NOTIFICATION_12_MASK | RESET_NOTIFICATION_6_MASK;
        return vec![ResetNotification { threshold_hours: 6 }];
    }

    if remaining < RESET_NOTIFICATION_12_HOURS
        && notification_state.sent_mask & RESET_NOTIFICATION_12_MASK == 0
    {
        notification_state.sent_mask |= RESET_NOTIFICATION_12_MASK;
        return vec![ResetNotification {
            threshold_hours: 12,
        }];
    }

    Vec::new()
}
fn default_codex_enabled(config_exists: bool, configured: Option<bool>) -> bool {
    configured.unwrap_or(config_exists)
}

#[cfg(test)]
mod tests {
    use super::{default_codex_enabled, pending_reset_notifications, ResetNotificationState};

    #[test]
    fn new_install_starts_without_codex_provider() {
        assert!(!default_codex_enabled(false, None));
    }

    #[test]
    fn existing_and_explicit_provider_settings_are_preserved() {
        assert!(default_codex_enabled(true, None));
        assert!(default_codex_enabled(false, Some(true)));
        assert!(!default_codex_enabled(true, Some(false)));
    }

    #[test]
    fn sends_reset_notifications_once_at_each_threshold() {
        let expiry = 1_000_000;
        let mut state = None;

        assert_eq!(
            pending_reset_notifications(true, expiry, expiry - 12 * 60 * 60, &mut state),
            Vec::new()
        );
        assert_eq!(
            pending_reset_notifications(true, expiry, expiry - 11 * 60 * 60, &mut state),
            vec![super::ResetNotification {
                threshold_hours: 12
            }]
        );
        assert_eq!(
            pending_reset_notifications(true, expiry, expiry - 5 * 60 * 60, &mut state),
            vec![super::ResetNotification { threshold_hours: 6 }]
        );
        assert_eq!(
            pending_reset_notifications(true, expiry, expiry - 60, &mut state),
            Vec::new()
        );
    }

    #[test]
    fn first_refresh_under_six_hours_sends_only_urgent_notification() {
        let expiry = 1_000_000;
        let mut state = Some(ResetNotificationState {
            expires_at: expiry,
            sent_mask: 0,
        });

        assert_eq!(
            pending_reset_notifications(true, expiry, expiry - 5 * 60 * 60, &mut state),
            vec![super::ResetNotification { threshold_hours: 6 }]
        );
        assert_eq!(
            pending_reset_notifications(true, expiry, expiry - 11 * 60 * 60, &mut state),
            Vec::new()
        );
    }

    #[test]
    fn disabled_notifications_do_not_mark_thresholds_sent() {
        let expiry = 1_000_000;
        let mut state = None;

        assert_eq!(
            pending_reset_notifications(false, expiry, expiry - 5 * 60 * 60, &mut state),
            Vec::new()
        );
        assert!(state.is_none());
    }

    #[tokio::test]
    async fn reports_missing_opencode_key_when_provider_is_enabled() {
        let mut monitor = super::Monitor::new();
        monitor.codex_enabled = false;
        monitor.opencode_go_enabled = true;
        monitor.opencode_api_key = None;

        monitor.refresh().await;

        assert_eq!(
            monitor.opencode_go_error.as_deref(),
            Some("OpenCode Go: API key is not configured. Add an API key in Settings.")
        );
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
