<script lang="ts">
  import { onMount, tick } from "svelte";
  import type { UnlistenFn } from "@tauri-apps/api/event";
  import type { AccentPalette, UiState, WorkArea } from "./lib/types";
  import { uiState, view, fetchState, getAccentPalette, getWorkArea, setFlyoutBounds, refresh } from "./lib/store";
  import Dashboard from "./lib/Dashboard.svelte";
  import Settings from "./lib/Settings.svelte";
  import { emit, listen } from "@tauri-apps/api/event";
  import { getCurrentWindow, LogicalSize } from "@tauri-apps/api/window";
  import { updateLimitTray } from "./lib/tray-icon";
  import FluentIcon from "./lib/FluentIcon.svelte";

  let contentEl: HTMLDivElement;
  let footerEl: HTMLElement;
  let compactLayout = $state(false);
  let didInitialFit = false;
  let fitVersion = 0;
  let showVersion = 0;
  let entranceActive = $state(false);
  let entranceFrame: number | undefined;
  let entranceVersion = 0;

  const FLYOUT_WIDTH = 380;
  const FOOTER_HEIGHT = 48;
  const HORIZONTAL_MARGIN = 12;
  const VERTICAL_MARGIN = 12;
  const MIN_HEIGHT = 380;
  const MAX_HEIGHT = 900;
  const currentWindow = getCurrentWindow();

  function preferredTextColor(color: string): string {
    const channels = [1, 3, 5].map((offset) => {
      const channel = Number.parseInt(color.slice(offset, offset + 2), 16) / 255;
      return channel <= 0.04045
        ? channel / 12.92
        : ((channel + 0.055) / 1.055) ** 2.4;
    });
    const luminance = 0.2126 * channels[0] + 0.7152 * channels[1] + 0.0722 * channels[2];
    return (luminance + 0.05) / 0.05 >= 1.05 / (luminance + 0.05)
      ? "#000000"
      : "#ffffff";
  }

  function applyAccentPalette(palette: AccentPalette) {
    const root = document.documentElement.style;
    for (const [name, value] of Object.entries(palette)) {
      root.setProperty(`--windows-accent-${name.replaceAll("_", "-")}`, value);
    }
    root.setProperty("--windows-accent-dark-1-text", preferredTextColor(palette.dark_1));
    root.setProperty("--windows-accent-light-1-text", preferredTextColor(palette.light_1));
  }

  async function syncWindowsAccent() {
    const palette = await getAccentPalette();
    applyAccentPalette(palette);
  }

  onMount(() => {
    let disposed = false;
    let cleanupFns: UnlistenFn[] = [];
    let listenersDisposed = false;

    const disposeListeners = (listeners: UnlistenFn[]) => {
      if (listenersDisposed) return;
      listenersDisposed = true;
      listeners.forEach((unlisten) => unlisten());
    };

    // Start every registration before the initial fetch so an update cannot
    // arrive between the first fetch and its listener being attached.
    const listenerRegistration = Promise.all([
      listen("trigger-refresh", () => {
        void refresh();
      }),
      listen("state-updated", () => {
        void fetchState();
      }),
      listen("window-shown", () => {
        void handleWindowShown();
      }),
      listen("navigate-settings", () => {
        handleNavigateSettings();
      }),
      currentWindow.onFocusChanged(({ payload: focused }) => {
        if (focused) void syncWindowsAccent().catch(() => {});
      }),
    ]);

    const handleKeydown = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      event.preventDefault();
      if ($view === "dashboard") hideWindow();
      else view.set("dashboard");
    };
    window.addEventListener("keydown", handleKeydown);

    void listenerRegistration.then(async (resolved) => {
      if (disposed) {
        disposeListeners(resolved);
        return;
      }
      cleanupFns = resolved;
      await Promise.all([fetchState(), syncWindowsAccent().catch(() => {})]);
      if (disposed) return;
      await fitToContent();
      if (disposed) return;
      didInitialFit = true;
      replayEntrance();
    });

    return () => {
      disposed = true;
      window.removeEventListener("keydown", handleKeydown);
      if (entranceFrame !== undefined) cancelAnimationFrame(entranceFrame);
      if (cleanupFns.length > 0) disposeListeners(cleanupFns);
      // If unmount races listener registration, clean up as soon as the
      // asynchronous Tauri registrations resolve.
      void listenerRegistration.then(disposeListeners);
    };
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

  async function applyFlyoutHeight(totalHeight: number, scale: number, workArea: WorkArea | null) {
    if (!workArea) {
      await currentWindow.setSize(new LogicalSize(FLYOUT_WIDTH, totalHeight));
      return;
    }

    const physicalWidth = Math.round(FLYOUT_WIDTH * scale);
    const physicalHeight = Math.round(totalHeight * scale);
    const horizontalMargin = Math.round(HORIZONTAL_MARGIN * scale);
    const verticalMargin = Math.round(VERTICAL_MARGIN * scale);
    await setFlyoutBounds(
      workArea.x + workArea.width - physicalWidth - horizontalMargin,
      workArea.y + workArea.height - physicalHeight - verticalMargin,
      physicalWidth,
      physicalHeight,
    );
  }

  async function fitToContent() {
    if (!contentEl) return;
    const version = ++fitVersion;

    const scale = await currentWindow.scaleFactor();
    const workArea = await getWorkArea().catch(() => null);
    const availableHeight = workArea
      ? workArea.height / scale - VERTICAL_MARGIN * 2
      : MAX_HEIGHT;
    const maxHeight = Math.min(MAX_HEIGHT, availableHeight);
    const fittedHeight = (naturalHeight: number) =>
      Math.min(Math.max(naturalHeight, MIN_HEIGHT), maxHeight);

    compactLayout = Boolean(contentEl.querySelector(".dashboard"));
    let contentHeight = await measureContentHeight();
    let naturalHeight = contentHeight + (footerEl?.offsetHeight ?? FOOTER_HEIGHT);
    let totalHeight = fittedHeight(naturalHeight);

    if (version !== fitVersion) return;
    await applyFlyoutHeight(totalHeight, scale, workArea);
    await tick();
    await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));
    if (version !== fitVersion) return;

    // If a non-dashboard view still overflows after receiving its real
    // WebView height, use the compact density before falling back to scrolling.
    if (!compactLayout && (naturalHeight > maxHeight || contentEl.scrollHeight > contentEl.clientHeight + 1)) {
      compactLayout = true;
      contentHeight = await measureContentHeight();
      naturalHeight = contentHeight + (footerEl?.offsetHeight ?? FOOTER_HEIGHT);
      if (version !== fitVersion) return;
      const compactHeight = fittedHeight(naturalHeight);
      if (compactHeight !== totalHeight) {
        totalHeight = compactHeight;
        await applyFlyoutHeight(totalHeight, scale, workArea);
      }
    }
  }

  async function handleWindowShown() {
    const version = ++showVersion;
    view.set("dashboard");
    replayEntrance();
    await Promise.all([fetchState(), syncWindowsAccent().catch(() => {})]);
    if (version !== showVersion) return;
    await fitToContent();
  }

  function handleNavigateSettings() {
    showVersion += 1;
    view.set("settings");
  }
  async function handleDashboardLayoutChange() {
    await fitToContent();
  }

  function replayEntrance() {
    const version = ++entranceVersion;
    entranceActive = false;
    if (entranceFrame !== undefined) cancelAnimationFrame(entranceFrame);
    void tick().then(() => {
      if (version !== entranceVersion) return;
      entranceFrame = requestAnimationFrame(() => {
        if (version === entranceVersion) entranceActive = true;
      });
    });
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
    void currentWindow.hide();
  }

  // Refit after switching views or receiving state with newly rendered content.
  $effect(() => {
    void $view;
    void $uiState;
    if (didInitialFit) {
      void fitToContent();
    }
  });

  $effect(() => {
    void emit("light-dismiss-enabled", $view === "dashboard").catch(() => {});
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

<div class="app" class:entering={entranceActive} class:compact={compactLayout}>
  <div class="content" bind:this={contentEl}>
    {#if $view === "dashboard"}
      <Dashboard data={$uiState} onlayoutchange={() => void handleDashboardLayoutChange()} />
    {:else}
      <Settings data={$uiState} />
    {/if}
  </div>
  <footer class="footer" bind:this={footerEl}>
    <span class="updated">{updatedAgo}</span>
    <span class="spacer" aria-hidden="true"></span>
    {#if $view === "dashboard"}
      <button class="subtle-button icon-button" onclick={() => view.set("settings")} aria-label="Open Settings" title="Settings">
        <FluentIcon name="settings" size={16} />
      </button>
    {/if}
  </footer>
</div>

<style>
  .app {
    display: flex;
    flex-direction: column;
    width: 380px;
    max-width: 380px;
    height: 100vh;
    overflow: hidden;
    background: var(--flyout-background);
    box-shadow: inset 0 0 0 1px var(--card-stroke);
    border-radius: var(--overlay-radius);
    isolation: isolate;
  }

  .app.entering {
    animation: flyout-enter 180ms cubic-bezier(0.16, 1, 0.3, 1);
  }

  @keyframes flyout-enter {
    from {
      opacity: 0;
      transform: translateY(8px);
    }
    to {
      opacity: 1;
      transform: translateY(0);
    }
  }

  .content {
    display: flex;
    flex: 1;
    flex-direction: column;
    min-height: 0;
    overflow-x: hidden;
    overflow-y: auto;
    padding: var(--space-3) var(--space-3) 0;
    scrollbar-gutter: auto;
    scrollbar-color: var(--scrollbar-thumb) transparent;
    scrollbar-width: thin;
  }
  .content:not(:hover):not(:focus-within) {
    scrollbar-color: transparent transparent;
  }

  .content:not(:hover):not(:focus-within)::-webkit-scrollbar-thumb {
    background: transparent;
  }


  .content::-webkit-scrollbar {
    width: 8px;
  }

  .content::-webkit-scrollbar-track {
    background: transparent;
  }

  .content::-webkit-scrollbar-thumb {
    min-height: 32px;
    border: 2px solid transparent;
    border-radius: 999px;
    background: var(--scrollbar-thumb);
    background-clip: padding-box;
  }

  .content::-webkit-scrollbar-thumb:hover {
    background: var(--scrollbar-thumb-hover);
    background-clip: padding-box;
  }

  .footer {
    display: flex;
    align-items: center;
    flex-shrink: 0;
    gap: var(--space-1);
    height: 48px;
    min-height: 48px;
    padding: 7px var(--space-3) 8px;
    background: var(--flyout-footer-background);
    border-top: 1px solid color-mix(in srgb, var(--card-stroke) 68%, transparent);
  }

  .updated {
    color: var(--text-secondary);
    font-size: 11px;
    font-variant-numeric: tabular-nums;
  }

  .spacer {
    flex: 1;
  }

  .footer .icon-button {
    flex: 0 0 32px;
    margin-inline-end: var(--space-1);
    color: var(--text-secondary);
  }

  .footer .icon-button:hover {
    color: var(--text-primary);
  }

  @media (prefers-reduced-motion: reduce) {
    .app.entering {
      animation: none;
    }
  }
</style>
