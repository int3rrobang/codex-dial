use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UsageWindow {
    pub remaining_percent: f64,
    pub resets_at: i64,
    pub duration_minutes: i64,
}

impl UsageWindow {
    pub fn starts_at(&self) -> i64 {
        self.resets_at - self.duration_minutes * 60
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct UsageSample {
    pub observed_at: i64,
    pub remaining_percent: u32,
    pub resets_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TokenDay {
    pub date: String,
    pub tokens: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LimitReading {
    pub limit_id: String,
    pub name: String,
    pub window: UsageWindow,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BankedResetCredit {
    pub title: String,
    pub description: Option<String>,
    pub expires_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UsageSnapshot {
    pub main_limit: LimitReading,
    pub other_limits: Vec<LimitReading>,
    pub token_history: Vec<TokenDay>,
    pub banked_reset_count: i32,
    pub banked_reset_credits: Vec<BankedResetCredit>,
    pub fetched_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OpenCodeGoWindow {
    pub name: String,
    pub remaining_percent: f64,
    pub resets_at: i64,
    pub duration_minutes: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OpenCodeGoSnapshot {
    pub windows: Vec<OpenCodeGoWindow>,
    pub fetched_at: i64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum PaceStatus {
    SlowDown,
    OnTrack,
    RoomToUseMore,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Forecast {
    pub window_started_at: i64,
    pub status: PaceStatus,
    pub expected_remaining_at_reset: f64,
    pub safety_remaining_at_reset: f64,
    pub historical_remaining_at_reset: f64,
    pub recommended_percent_per_day: f64,
    pub current_percent_per_day: f64,
    pub historical_percent_per_day: f64,
    pub safety_percent_per_day: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiState {
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
    pub opencode_go: Option<OpenCodeGoSnapshot>,
    pub opencode_go_error: Option<String>,
    pub opencode_go_forecasts: Vec<Forecast>,
    pub opencode_go_samples: Vec<UsageSample>,
    pub opencode_go_enabled: bool,
}
