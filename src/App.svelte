<script lang="ts">
  import { onMount, tick } from "svelte";
  import { uiState, view, fetchState, refresh } from "./lib/store";
  import Dashboard from "./lib/Dashboard.svelte";
  import Settings from "./lib/Settings.svelte";
  import { listen } from "@tauri-apps/api/event";
  import { getCurrentWindow, LogicalSize } from "@tauri-apps/api/window";
  import { updateLimitTray } from "./lib/tray-icon";

  let contentEl: HTMLDivElement;
  let didInitialFit = false;

  const TITLEBAR_HEIGHT = 28;
  const PADDING = 12;
  const MIN_HEIGHT = 380;
  const MAX_HEIGHT = 900;

  onMount(async () => {
    await fetchState();
    listen("trigger-refresh", () => refresh());
    listen("navigate-settings", () => view.set("settings"));
    await tick();
    fitToContent();
    didInitialFit = true;
  });

  function fitToContent() {
    if (!contentEl) return;
    const contentHeight = contentEl.scrollHeight;
    const totalHeight = Math.min(
      Math.max(contentHeight + TITLEBAR_HEIGHT + PADDING, MIN_HEIGHT),
      MAX_HEIGHT
    );
    const win = getCurrentWindow();
    win.outerSize().then((size) => {
      win.setSize(new LogicalSize(size.width, totalHeight));
    });
  }

  function hideWindow() {
    getCurrentWindow().hide();
  }

  // Only auto-fit when switching views (not on every state update)
  $effect(() => {
    void $view;
    if (didInitialFit) {
      tick().then(() => fitToContent());
    }
  });

  // Update tray icon when state changes
  $effect(() => {
    const s = $uiState;
    if (s.snapshot && s.forecast) {
      const seconds = Math.max(s.snapshot.main_limit.window.resets_at - Date.now() / 1000, 0);
      const days = Math.floor(seconds / 86400);
      const hours = Math.floor((seconds % 86400) / 3600);
      const resetIn = days > 0 ? `${days}d ${hours}h` : `${hours}h ${Math.floor((seconds % 3600) / 60)}m`;

      updateLimitTray({
        remaining: s.snapshot.main_limit.window.remaining_percent,
        currentPace: s.forecast.current_percent_per_day,
        suggestedPace: s.forecast.recommended_percent_per_day,
        resetIn,
      }).catch(() => {});
    }
  });
</script>

<div class="app">
  <div class="titlebar" data-tauri-drag-region>
    <span class="titlebar-spacer" data-tauri-drag-region></span>
    <button class="hide-btn" onclick={hideWindow} title="Hide">&#x2715;</button>
  </div>
  <div class="content" bind:this={contentEl}>
    {#if $view === "dashboard"}
      <Dashboard data={$uiState} />
    {:else}
      <Settings data={$uiState} />
    {/if}
  </div>
</div>

<style>
  .app {
    display: flex;
    flex-direction: column;
    height: 100vh;
    overflow: hidden;
  }
  .titlebar {
    display: flex;
    align-items: center;
    height: 28px;
    flex-shrink: 0;
    cursor: grab;
  }
  .titlebar:active {
    cursor: grabbing;
  }
  .titlebar-spacer {
    flex: 1;
    height: 100%;
  }
  .hide-btn {
    width: 28px;
    height: 28px;
    display: flex;
    align-items: center;
    justify-content: center;
    border: none;
    background: none;
    color: var(--text-secondary);
    font-size: 12px;
    cursor: pointer;
    border-radius: 4px;
  }
  .hide-btn:hover {
    background: rgba(255, 107, 107, 0.15);
    color: var(--accent-red);
  }
  .content {
    flex: 1;
    display: flex;
    flex-direction: column;
    overflow: hidden;
    padding: 0 16px 16px;
  }
</style>
