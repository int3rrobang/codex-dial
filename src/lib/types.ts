export interface UsageWindow {
  remaining_percent: number;
  resets_at: number;
  duration_minutes: number;
}

export interface UsageSample {
  observed_at: number;
  remaining_percent: number;
  resets_at: number;
}

export interface TokenDay {
  date: string;
  tokens: number;
}

export interface LimitReading {
  limit_id: string;
  name: string;
  window: UsageWindow;
}

export interface UsageSnapshot {
  main_limit: LimitReading;
  other_limits: LimitReading[];
  token_history: TokenDay[];
  emergency_reset_count: number;
  fetched_at: number;
}

export type PaceStatus = "slowDown" | "onTrack" | "roomToUseMore";

export interface Forecast {
  status: PaceStatus;
  expected_remaining_at_reset: number;
  safety_remaining_at_reset: number;
  historical_remaining_at_reset: number;
  recommended_percent_per_day: number;
  current_percent_per_day: number;
  historical_percent_per_day: number;
  safety_percent_per_day: number;
}

export interface UiState {
  snapshot: UsageSnapshot | null;
  forecast: Forecast | null;
  samples: UsageSample[];
  is_refreshing: boolean;
  error_message: string | null;
  sync_folder_name: string | null;
  sync_error_message: string | null;
  safety_buffer: number;
  launch_at_login: boolean;
  opencode_go: OpenCodeGoSnapshot | null;
  opencode_go_error: string | null;
  opencode_go_forecasts: Forecast[];
  opencode_go_samples: UsageSample[];
  opencode_go_enabled: boolean;
}

export interface OpenCodeGoWindow {
  name: string;
  remaining_percent: number;
  resets_at: number;
  duration_minutes: number;
}

export interface OpenCodeGoSnapshot {
  windows: OpenCodeGoWindow[];
  fetched_at: number;
}
