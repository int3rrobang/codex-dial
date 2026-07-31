import { writable } from "svelte/store";
import { invoke } from "@tauri-apps/api/core";
import type { AccentPalette, UiState, WorkArea } from "./types";

export const uiState = writable<UiState>({
  snapshot: null,
  forecast: null,
  samples: [],
  is_refreshing: false,
  error_message: null,
  sync_folder_name: null,
  sync_error_message: null,
  codex_enabled: true,
  safety_buffer: 3,
  launch_at_login: true,
  opencode_go: null,
  opencode_go_error: null,
  opencode_go_forecasts: [],
  opencode_go_samples: [],
  opencode_go_enabled: false,
  reset_notifications_enabled: true,
});

export const view = writable<"dashboard" | "settings">("dashboard");

export async function getAccentPalette(): Promise<AccentPalette> {
  return invoke<AccentPalette>("get_accent_palette");
}

export async function getWorkArea(): Promise<WorkArea> {
  return invoke<WorkArea>("get_work_area");
}

export async function setFlyoutBounds(x: number, y: number, width: number, height: number) {
  await invoke("set_flyout_bounds", { x, y, width, height });
}

export async function fetchState() {
  const state = await invoke<UiState>("get_state");
  uiState.set(state);
}

export async function refresh() {
  uiState.update((s) => ({ ...s, is_refreshing: true }));
  const state = await invoke<UiState>("refresh");
  uiState.set(state);
}

export async function setSafetyBuffer(value: number) {
  const state = await invoke<UiState>("set_safety_buffer", { value });
  uiState.set(state);
}

export async function setLaunchAtLogin(enabled: boolean) {
  const state = await invoke<UiState>("set_launch_at_login", { enabled });
  uiState.set(state);
}

export async function setResetNotificationsEnabled(enabled: boolean) {
  const state = await invoke<UiState>("set_reset_notifications_enabled", { enabled });
  uiState.set(state);
}

export async function setCodexEnabled(enabled: boolean) {
  const state = await invoke<UiState>("set_codex_enabled", { enabled });
  uiState.set(state);
  if (enabled) await refresh();
}

export async function chooseSyncFolder() {
  const state = await invoke<UiState>("choose_sync_folder");
  uiState.set(state);
}

export async function stopSync() {
  const state = await invoke<UiState>("stop_sync");
  uiState.set(state);
}

export async function setOpenCodeCookie(cookie: string | null) {
  await invoke<UiState>("set_opencode_cookie", { cookie });
  await refresh();
}

export async function setOpenCodeWorkspaceId(id: string | null) {
  await invoke<UiState>("set_opencode_workspace_id", { id });
  await refresh();
}

export async function setOpenCodeGoEnabled(enabled: boolean) {
  await invoke<UiState>("set_opencode_go_enabled", { enabled });
  if (enabled) await refresh();
  else await fetchState();
}
