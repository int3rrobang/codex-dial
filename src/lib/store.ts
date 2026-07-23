import { writable } from "svelte/store";
import { invoke } from "@tauri-apps/api/core";
import type { UiState } from "./types";

export const uiState = writable<UiState>({
  snapshot: null,
  forecast: null,
  samples: [],
  is_refreshing: false,
  error_message: null,
  sync_folder_name: null,
  sync_error_message: null,
  safety_buffer: 3,
  launch_at_login: true,
});

export const view = writable<"dashboard" | "settings">("dashboard");

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

export async function chooseSyncFolder() {
  const state = await invoke<UiState>("choose_sync_folder");
  uiState.set(state);
}

export async function stopSync() {
  const state = await invoke<UiState>("stop_sync");
  uiState.set(state);
}
