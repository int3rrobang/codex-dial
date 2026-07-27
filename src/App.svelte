<script lang="ts">
  import { onMount, tick } from "svelte";
  import { exit } from "@tauri-apps/plugin-process";
  import type { UiState } from "./lib/types";
  import { uiState, view, fetchState, getWorkArea, refresh } from "./lib/store";
  import Dashboard from "./lib/Dashboard.svelte";
  import Settings from "./lib/Settings.svelte";
  import { listen } from "@tauri-apps/api/event";
  import { getCurrentWindow, LogicalSize, PhysicalPosition } from "@tauri-apps/api/window";
  import { updateLimitTray } from "./lib/tray-icon";

  let contentEl: HTMLDivElement;
  let footerEl: HTMLDivElement;
  let compactLayout = $state(false);
  let didInitialFit = false;
  let fitVersion = 0;

  const TITLEBAR_HEIGHT = 28;
  const PADDING = 12;
  const HORIZONTAL_MARGIN = 16;
  const VERTICAL_MARGIN = 0;
  const MIN_HEIGHT = 380;
  const MAX_HEIGHT = 900;

  onMount(async () => {
    listen("trigger-refresh", () => refresh());
    listen("navigate-settings", () => view.set("settings"));
    await fitToContent();
    didInitialFit = true;
    await fetchState();
  });

  async function measureContentHeight() {
    await tick();
    await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));

    const contentStyles = {
      flex: contentEl.style.flex,
      height: contentEl.style.height,
      minHeight: contentEl.style.minHeight,
      overflowY: contentEl.style.overflowY,
    };

    contentEl.style.flex = "none";
    contentEl.style.height = "auto";
    contentEl.style.minHeight = "0";
    contentEl.style.overflowY = "visible";

    const contentHeight = contentEl.scrollHeight;

    contentEl.style.flex = contentStyles.flex;
    contentEl.style.height = contentStyles.height;
    contentEl.style.minHeight = contentStyles.minHeight;
    contentEl.style.overflowY = contentStyles.overflowY;

    return contentHeight;
  }

  async function fitToContent() {
    if (!contentEl) return;
    const version = ++fitVersion;

    const win = getCurrentWindow();
    const scale = await win.scaleFactor();
    const size = await win.outerSize();
    const workArea = await getWorkArea().catch(() => null);
    const availableHeight = workArea
      ? workArea.height / scale - VERTICAL_MARGIN * 2
      : MAX_HEIGHT;
    const maxHeight = Math.min(MAX_HEIGHT, availableHeight);
    const fittedHeight = (naturalHeight: number) =>
      Math.min(Math.max(naturalHeight, MIN_HEIGHT), maxHeight);

    compactLayout = Boolean(contentEl.querySelector(".dashboard"));
    let contentHeight = await measureContentHeight();
    let naturalHeight = contentHeight + TITLEBAR_HEIGHT + PADDING + (footerEl?.offsetHeight ?? 0);
    let totalHeight = fittedHeight(naturalHeight);

    await win.setSize(new LogicalSize(size.width / scale, totalHeight));
    await tick();
    await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));
    if (version !== fitVersion) return;

    // If a non-dashboard view still overflows after receiving its real
    // WebView height, use the compact density before falling back to scrolling.
    if (!compactLayout && (naturalHeight > maxHeight || contentEl.scrollHeight > contentEl.clientHeight + 1)) {
      compactLayout = true;
      contentHeight = await measureContentHeight();
      naturalHeight = contentHeight + TITLEBAR_HEIGHT + PADDING + (footerEl?.offsetHeight ?? 0);
      totalHeight = fittedHeight(naturalHeight);
      await win.setSize(new LogicalSize(size.width / scale, totalHeight));
    }

    if (version !== fitVersion) return;
    if (workArea) {
      const physicalHeight = Math.round(totalHeight * scale);
      const horizontalMargin = Math.round(HORIZONTAL_MARGIN * scale);
      const verticalMargin = Math.round(VERTICAL_MARGIN * scale);
      await win.setPosition(new PhysicalPosition(
        workArea.x + workArea.width - size.width - horizontalMargin,
        workArea.y + workArea.height - physicalHeight - verticalMargin
      ));
    }
  }
  function updateAgoText(state: UiState) {
    const fetchedAt = [state.snapshot?.fetched_at, state.opencode_go?.fetched_at]
      .filter((timestamp): timestamp is number => timestamp !== undefined)
      .reduce((latest, timestamp) => Math.max(latest, timestamp), 0);
    if (!fetchedAt) return;

    const seconds = Math.max(Date.now() / 1000 - fetchedAt, 0);
    if (seconds < 60) updatedAgo = "Updated just now";
    else if (seconds < 3600) updatedAgo = `Updated ${Math.floor(seconds / 60)} min ago`;
    else if (seconds < 86400) {
      const hours = Math.floor(seconds / 3600);
      updatedAgo = `Updated ${hours} ${hours === 1 ? "hr" : "hrs"} ago`;
    } else {
      const days = Math.floor(seconds / 86400);
      updatedAgo = `Updated ${days} ${days === 1 ? "day" : "days"} ago`;
    }
  }

  let updatedAgo = $state("Updated just now");

  $effect(() => {
    const state = $uiState;
    updateAgoText(state);
    const interval = setInterval(() => updateAgoText(state), 60000);
    return () => clearInterval(interval);
  });

  function hideWindow() {
    getCurrentWindow().hide();
  }

  // Refit after switching views or receiving state with newly rendered content.
  $effect(() => {
    void $view;
    void $uiState;
    if (didInitialFit) {
      void fitToContent();
    }
  });

  // Update tray icon when state changes
  $effect(() => {
    const s = $uiState;
    let backendName: string | null = null;
    let remaining = 0;
    let currentPace = 0;
    let suggestedPace = 0;
    let resetsAt = 0;

    if (s.codex_enabled && s.snapshot && s.forecast) {
      backendName = "Codex";
      remaining = s.snapshot.main_limit.window.remaining_percent;
      currentPace = s.forecast.current_percent_per_day;
      suggestedPace = s.forecast.recommended_percent_per_day;
      resetsAt = s.snapshot.main_limit.window.resets_at;
    } else if (
      s.opencode_go_enabled
      && s.opencode_go
      && s.opencode_go.windows.length > 0
      && s.opencode_go_forecasts.length > 0
    ) {
      backendName = "OpenCode Go";
      remaining = s.opencode_go.windows[0].remaining_percent;
      currentPace = s.opencode_go_forecasts[0].current_percent_per_day;
      suggestedPace = s.opencode_go_forecasts[0].recommended_percent_per_day;
      resetsAt = s.opencode_go.windows[0].resets_at;
    }

    if (!backendName) return;

    const seconds = Math.max(resetsAt - Date.now() / 1000, 0);
    const days = Math.floor(seconds / 86400);
    const hours = Math.floor((seconds % 86400) / 3600);
    const resetIn = days > 0 ? `${days}d ${hours}h` : `${hours}h ${Math.floor((seconds % 3600) / 60)}m`;

    updateLimitTray({
      backendName,
      remaining,
      currentPace,
      suggestedPace,
      resetIn,
    }).catch(() => {});
  });
</script>

<div class="app" class:compact={compactLayout}>
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
  <div class="footer" bind:this={footerEl}>
    <span class="updated">{updatedAgo}</span>
    <span class="spacer"></span>
    <button onclick={() => view.set("settings")} title="Settings">&#9881;</button>
    <button onclick={() => exit(0)}>Quit</button>
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
    min-height: 0;
    display: flex;
    flex-direction: column;
    overflow-y: auto;
    padding: 0 16px;
  }
  .footer {
    display: flex;
    align-items: center;
    gap: 4px;
    flex-shrink: 0;
    padding: 0 16px 12px;
  }
  .updated {
    font-size: 11px;
    color: var(--text-secondary);
  }
</style>
