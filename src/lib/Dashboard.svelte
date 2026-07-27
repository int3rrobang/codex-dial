<script lang="ts">
  import type { UiState, PaceStatus, Forecast, UsageWindow, UsageSample } from "./types";
  import { refresh, view } from "./store";
  import BurnDownChart from "./BurnDownChart.svelte";


  let { data }: { data: UiState } = $props();

  let ocgWindow = $state(1);
  let showOtherLimits = $state(false);
  let tab = $state<"codex" | "opencode">("codex");
  let showOtherWindows = $state(false);
  let showBankedResets = $state(false);
  let bankedResetStat = $state<HTMLDivElement>();

  function handleWindowClick(event: MouseEvent) {
    if (
      showBankedResets
      && bankedResetStat
      && !bankedResetStat.contains(event.target as Node)
    ) {
      showBankedResets = false;
    }
  }

  function handleKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") showBankedResets = false;
  }


  $effect(() => {
    if (!data.codex_enabled && tab === "codex") tab = "opencode";
    if (!data.opencode_go_enabled && tab === "opencode") tab = "codex";
  });

  function statusTitle(status: PaceStatus): string {
    switch (status) {
      case "slowDown": return "Slow down";
      case "onTrack": return "On track";
      case "roomToUseMore": return "Room to use more";
    }
  }

  function statusColor(status: PaceStatus): string {
    switch (status) {
      case "slowDown": return "var(--accent-red)";
      case "onTrack": return "var(--accent-green)";
      case "roomToUseMore": return "var(--accent-blue)";
    }
  }

  function genericStatusMessage(f: Forecast, remaining: number, resetsAt: number, fetchedAt: number): string {
    switch (f.status) {
      case "slowDown": {
        const timeLeft = resetsAt - fetchedAt;
        const timeToEmpty = (remaining / Math.max(f.safety_percent_per_day, 0.01)) * 86400;
        const early = Math.max(timeLeft - timeToEmpty, 0);
        if (early > 0) {
          const hrs = Math.max(Math.round(early / 3600), 1);
          return `At this pace, your limit may run out ${hrs} ${hrs === 1 ? "hour" : "hours"} early.`;
        }
        return "Your current pace is too close to the limit.";
      }
      case "onTrack":
        return `You're on track to have ${Math.round(f.expected_remaining_at_reset)}% left at reset.`;
      case "roomToUseMore": {
        const room = Math.max(f.expected_remaining_at_reset - data.safety_buffer, 0);
        return `You can use about ${Math.round(room)}% more before the reset.`;
      }
    }
  }

  function genericPaceText(f: Forecast, resetsAt: number): string {
    const now = Date.now() / 1000;
    if (resetsAt - now <= 86400) {
      return `Up to ${(f.recommended_percent_per_day / 24).toFixed(1)}% an hour`;
    }
    return `Up to ${f.recommended_percent_per_day.toFixed(1)}% a day`;
  }

  function formatResetTime(ts: number): string {
    return new Date(ts * 1000).toLocaleString("en-US", {
      month: "short", day: "numeric", hour: "numeric", minute: "2-digit",
    });
  }

  function formatResetDeadline(ts: number): string {
    return new Date(ts * 1000).toLocaleString("en-US", {
      year: "numeric", month: "short", day: "numeric",
      hour: "numeric", minute: "2-digit",
    });
  }

  function relativeTime(ts: number): string {
    const seconds = Math.max(ts - Date.now() / 1000, 0);
    const days = Math.floor(seconds / 86400);
    const hours = Math.floor((seconds % 86400) / 3600);
    const mins = Math.floor((seconds % 3600) / 60);
    if (days > 0) return `${days}d ${hours}h`;
    if (hours > 0) return `${hours}h ${mins}m`;
    return `${mins}m`;
  }
  function resetExpiryText(ts: number): string {
    const remaining = ts - Date.now() / 1000;
    return remaining <= 0 ? "expired" : `in ${relativeTime(ts)}`;
  }


  function currentWindowSamples() {
    if (!data.snapshot) return [];
    const reset = data.snapshot.main_limit.window.resets_at;
    return data.samples
      .filter((s) => s.resets_at === reset)
      .sort((a, b) => a.observed_at - b.observed_at);
  }

  function ocgWindowSamples(): UsageSample[] {
    if (!data.opencode_go || data.opencode_go.windows.length === 0) return [];
    const idx = Math.min(ocgWindow, data.opencode_go.windows.length - 1);
    const reset = data.opencode_go.windows[idx].resets_at;
    return data.opencode_go_samples
      .filter((s) => s.resets_at === reset)
      .sort((a, b) => a.observed_at - b.observed_at);
  }

  function ocgPrimaryWindow(): UsageWindow | null {
    if (!data.opencode_go || data.opencode_go.windows.length === 0) return null;
    const idx = Math.min(ocgWindow, data.opencode_go.windows.length - 1);
    const w = data.opencode_go.windows[idx];
    return { remaining_percent: w.remaining_percent, resets_at: w.resets_at, duration_minutes: w.duration_minutes };
  }
</script>
<svelte:window onclick={handleWindowClick} onkeydown={handleKeydown} />

{#if !data.codex_enabled && !data.opencode_go_enabled}
  <div class="empty-state provider-empty">
    <p class="provider-title">No usage provider is enabled</p>
    <p>Enable Codex CLI or OpenCode Go in Settings to start monitoring usage.</p>
    <button class="retry" onclick={() => view.set("settings")}>Open Settings</button>
  </div>
{:else}
{#if data.codex_enabled && data.opencode_go_enabled}
  <div class="tabs">
    <button class="tab" class:active={tab === "codex"} onclick={() => tab = "codex"}>Codex</button>
    <button class="tab" class:active={tab === "opencode"} onclick={() => tab = "opencode"}>OpenCode Go</button>
  </div>
{/if}

<div class="tab-panels">
  <div class="tab-panel" class:inactive={tab !== "codex"} aria-hidden={tab !== "codex"}>
  {#if data.snapshot && data.forecast}
    {@const snapshot = data.snapshot}
    {@const forecast = data.forecast}
    <div class="dashboard">
      <div class="header">
        <span class="remaining">{Math.round(snapshot.main_limit.window.remaining_percent)}%</span>
        <span class="remaining-label">remaining</span>
        <span class="spacer"></span>
        <button onclick={() => refresh()} title="Refresh">
          {#if data.is_refreshing}
            <span class="spinner"></span>
          {:else}
            &#x21bb;
          {/if}
        </button>
      </div>

      <div class="status-section">
        <div class="status-title" style="color: {statusColor(forecast.status)}">
          {statusTitle(forecast.status)}
        </div>
        <div class="status-message">{genericStatusMessage(forecast, snapshot.main_limit.window.remaining_percent, snapshot.main_limit.window.resets_at, snapshot.fetched_at)}</div>
      </div>

      <BurnDownChart
        win={snapshot.main_limit.window}
        samples={currentWindowSamples()}
        tokenHistory={snapshot.token_history}
        fetchedAt={snapshot.fetched_at}
        {forecast}
        safetyBuffer={data.safety_buffer}
      />

      <div class="stats-grid">
        <div class="stats-col">
          <div class="stat-item">
            <span class="stat-label">Reset in</span>
            <span class="stat-value">{relativeTime(snapshot.main_limit.window.resets_at)}</span>
          </div>
          <div class="stat-item">
            <span class="stat-label">Suggested pace</span>
            <span class="stat-value">{genericPaceText(forecast, snapshot.main_limit.window.resets_at)}</span>
          </div>
          <div class="stat-item">
            <span class="stat-label">Current pace</span>
            <span class="stat-value">{forecast.current_percent_per_day.toFixed(1)}% / day</span>
          </div>
        </div>
        <div class="stats-col">
          <div class="stat-item banked-reset-stat" bind:this={bankedResetStat}>
            <span class="stat-label">Banked resets</span>
            {#if snapshot.banked_reset_count > 0}
              <button
                class="banked-reset-trigger"
                aria-expanded={showBankedResets}
                aria-controls="banked-reset-details"
                aria-label={`${snapshot.banked_reset_count} banked reset${snapshot.banked_reset_count === 1 ? "" : "s"}${snapshot.banked_reset_credits[0] ? `; next expires ${formatResetDeadline(snapshot.banked_reset_credits[0].expires_at)}` : ""}`}
                onclick={() => showBankedResets = !showBankedResets}
              >
                <span class="stat-value">{snapshot.banked_reset_count}</span>
                {#if snapshot.banked_reset_credits.length > 0}
                  <span class="banked-next-deadline">Next {formatResetTime(snapshot.banked_reset_credits[0].expires_at)}</span>
                {/if}
                <span class="chevron" class:open={showBankedResets}>&#9656;</span>
              </button>
            {:else}
              <span class="stat-value">0</span>
            {/if}

            {#if showBankedResets && snapshot.banked_reset_count > 0}
              <div id="banked-reset-details" class="reset-popover" role="dialog" aria-label="Banked reset deadlines">
                <div class="reset-popover-title">Banked reset deadlines</div>
                {#if snapshot.banked_reset_credits.length > 0}
                  <div class="reset-list">
                    {#each snapshot.banked_reset_credits as credit}
                      <div class="reset-credit">
                        <div class="reset-credit-title">{credit.title}</div>
                        <div class="reset-credit-deadline">Expires {formatResetDeadline(credit.expires_at)}</div>
                        <div class="reset-credit-relative">{resetExpiryText(credit.expires_at)}</div>
                        {#if credit.description}
                          <div class="reset-credit-description">{credit.description}</div>
                        {/if}
                      </div>
                    {/each}
                  </div>
                  {#if snapshot.banked_reset_credits.length < snapshot.banked_reset_count}
                    <p class="reset-note">
                      Showing {snapshot.banked_reset_credits.length} of {snapshot.banked_reset_count} deadlines returned by Codex.
                    </p>
                  {/if}
                {:else}
                  <p class="reset-note">
                    Codex reported {snapshot.banked_reset_count} banked reset{snapshot.banked_reset_count === 1 ? "" : "s"} but did not return their deadlines. Refresh later or update the Codex CLI.
                  </p>
                {/if}
              </div>
            {/if}
          </div>
          <div class="stat-item">
            <span class="stat-label">Nearest reset</span>
            <span class="stat-value">{relativeTime(Math.min(snapshot.main_limit.window.resets_at, ...snapshot.other_limits.map((l) => l.window.resets_at)))}</span>
          </div>
          <div class="stat-item">
            <span class="stat-label">Historical pace</span>
            <span class="stat-value">{forecast.historical_percent_per_day.toFixed(1)}% / day</span>
          </div>
        </div>
      </div>

      {#if snapshot.other_limits.length > 0}
        <hr />
        <div class="other-limits">
          <button class="section-toggle" onclick={() => showOtherLimits = !showOtherLimits}>
            <span class="section-label">Other limits ({snapshot.other_limits.length})</span>
            <span class="chevron" class:open={showOtherLimits}>&#9656;</span>
          </button>
          {#if showOtherLimits}
            {#each snapshot.other_limits as limit}
              <div class="limit-row">
                <span class="limit-name">{limit.name}</span>
                <span class="limit-pct">{Math.round(limit.window.remaining_percent)}%</span>
                <span class="limit-reset">{formatResetTime(limit.window.resets_at)}</span>
              </div>
            {/each}
          {/if}
        </div>
      {/if}

      {#if data.error_message}
        <div class="error">&#9888; {data.error_message}</div>
      {/if}

    </div>
  {:else}
    <div class="empty-state">
      {#if data.is_refreshing}
        <span class="spinner large"></span>
        <p>Reading Codex usage...</p>
      {:else}
        <p>&#9888;</p>
        <p>{data.error_message ?? "Codex usage is not available."}</p>
        <button class="retry" onclick={() => refresh()}>Try Again</button>
      {/if}
    </div>
  {/if}
  </div>
  <div class="tab-panel" class:inactive={tab !== "opencode"} aria-hidden={tab !== "opencode"}>
  {#if data.opencode_go && data.opencode_go_forecasts.length > 0}
    {@const ocg = data.opencode_go}
    {@const idx = Math.min(ocgWindow, ocg.windows.length - 1)}
    {@const selected = ocg.windows[idx]}
    {@const forecast = data.opencode_go_forecasts[idx]}
    {@const win = ocgPrimaryWindow()}
    <div class="dashboard">
      <div class="header">
        <span class="remaining">{Math.round(selected.remaining_percent)}%</span>
        <span class="remaining-label">remaining</span>
        <span class="window-tabs">
          {#each ocg.windows as w, i}
            <button class="window-tab" class:active={i === idx} onclick={() => ocgWindow = i}>{w.name}</button>
          {/each}
        </span>
        <span class="spacer"></span>
        <button onclick={() => refresh()} title="Refresh">
          {#if data.is_refreshing}
            <span class="spinner"></span>
          {:else}
            &#x21bb;
          {/if}
        </button>
      </div>

      <div class="status-section">
        <div class="status-title" style="color: {statusColor(forecast.status)}">
          {statusTitle(forecast.status)}
        </div>
        <div class="status-message">{genericStatusMessage(forecast, selected.remaining_percent, selected.resets_at, ocg.fetched_at)}</div>
      </div>

      {#if win}
        <BurnDownChart
          {win}
          samples={ocgWindowSamples()}
          tokenHistory={[]}
          fetchedAt={ocg.fetched_at}
          {forecast}
          safetyBuffer={data.safety_buffer}
        />
      {/if}

      <div class="stats-grid">
        <div class="stats-col">
          <div class="stat-item">
            <span class="stat-label">Reset in</span>
            <span class="stat-value">{relativeTime(selected.resets_at)}</span>
          </div>
          <div class="stat-item">
            <span class="stat-label">Suggested pace</span>
            <span class="stat-value">{genericPaceText(forecast, selected.resets_at)}</span>
          </div>
          <div class="stat-item">
            <span class="stat-label">Current pace</span>
            <span class="stat-value">{forecast.current_percent_per_day.toFixed(1)}% / day</span>
          </div>
        </div>
        <div class="stats-col">
          <div class="stat-item">
            <span class="stat-label">Window</span>
            <span class="stat-value">{selected.name}</span>
          </div>
          <div class="stat-item">
            <span class="stat-label">Nearest reset</span>
            <span class="stat-value">{relativeTime(Math.min(...ocg.windows.map((w) => w.resets_at)))}</span>
          </div>
          <div class="stat-item">
            <span class="stat-label">Historical pace</span>
            <span class="stat-value">{forecast.historical_percent_per_day.toFixed(1)}% / day</span>
          </div>
        </div>
      </div>

      {#if ocg.windows.length > 1}
        <hr />
        <div class="other-limits">
          <button class="section-toggle" onclick={() => showOtherWindows = !showOtherWindows}>
            <span class="section-label">Other windows ({ocg.windows.length - 1})</span>
            <span class="chevron" class:open={showOtherWindows}>&#9656;</span>
          </button>
          {#if showOtherWindows}
            {#each ocg.windows.filter((_, i) => i !== idx) as w}
              <div class="limit-row">
                <span class="limit-name">{w.name}</span>
                <span class="limit-pct">{Math.round(w.remaining_percent)}%</span>
                <span class="limit-reset">{relativeTime(w.resets_at)}</span>
              </div>
            {/each}
          {/if}
        </div>
      {/if}

      {#if data.opencode_go_error}
        <div class="error">&#9888; {data.opencode_go_error}</div>
      {/if}

    </div>
  {:else}
    <div class="empty-state">
      {#if data.is_refreshing}
        <span class="spinner large"></span>
        <p>Reading OpenCode Go usage...</p>
      {:else}
        <p>&#9888;</p>
        <p>{data.opencode_go_error ?? "OpenCode Go usage is not available."}</p>
        <button class="retry" onclick={() => refresh()}>Try Again</button>
      {/if}
    </div>
  {/if}
  </div>
</div>
{/if}

<style>
  .tabs { display: flex; gap: 4px; margin-bottom: 12px; }
  .tab-panels { display: grid; }
  .tab-panel { grid-area: 1 / 1; min-width: 0; }
  .tab-panel.inactive { visibility: hidden; pointer-events: none; }
  .tab { padding: 4px 12px; border-radius: 6px; font-size: 12px; font-weight: 500; background: none; color: var(--text-secondary); border: 1px solid var(--border); cursor: pointer; }
  .tab.active { background: var(--surface); color: var(--text-primary); border-color: var(--accent-blue); }
  .window-tabs { display: flex; gap: 2px; flex-wrap: nowrap; white-space: nowrap; }
  .window-tab { padding: 2px 7px; border-radius: 4px; font-size: 10px; background: none; color: var(--text-secondary); border: 1px solid transparent; cursor: pointer; white-space: nowrap; }
  .window-tab.active { color: var(--text-primary); border-color: var(--border); background: var(--surface); }
  .dashboard { display: flex; flex-direction: column; gap: 14px; flex: 1; min-height: 0; }
  .header { display: flex; align-items: baseline; gap: 6px; flex-wrap: nowrap; }
  .remaining { font-size: 34px; font-weight: 600; font-variant-numeric: tabular-nums; }
  .remaining-label { color: var(--text-secondary); white-space: nowrap; }
  .spacer { flex: 1; }
  .status-section { display: flex; flex-direction: column; gap: 4px; }
  .status-title { font-weight: 600; font-size: 14px; }
  .status-message { color: var(--text-secondary); }
  .stats-grid { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); grid-template-rows: repeat(3, auto); grid-auto-flow: column; column-gap: 24px; row-gap: 8px; }
  .stats-col { display: contents; }
  .stat-item { display: flex; flex-direction: column; gap: 2px; }
  .banked-reset-stat { position: relative; }
  .banked-reset-trigger { display: flex; align-items: center; gap: 6px; width: 100%; padding: 0; color: var(--text-primary); text-align: left; }
  .banked-reset-trigger:hover { background: none; }
  .banked-next-deadline { min-width: 0; overflow: hidden; color: var(--text-secondary); font-size: 11px; text-overflow: ellipsis; white-space: nowrap; }
  .reset-popover { position: absolute; right: 0; bottom: calc(100% + 8px); z-index: 20; width: min(300px, calc(100vw - 32px)); max-height: 240px; overflow-y: auto; padding: 10px; border: 1px solid var(--border); border-radius: 7px; background: var(--surface); box-shadow: 0 8px 24px rgba(0, 0, 0, 0.35); }
  .reset-popover-title { margin-bottom: 8px; color: var(--text-primary); font-size: 12px; font-weight: 600; }
  .reset-list { display: flex; flex-direction: column; gap: 9px; }
  .reset-credit { padding-bottom: 9px; border-bottom: 1px solid var(--border); font-size: 11px; }
  .reset-credit:last-child { padding-bottom: 0; border-bottom: none; }
  .reset-credit-title { color: var(--text-primary); font-weight: 600; }
  .reset-credit-deadline { margin-top: 2px; color: var(--text-secondary); }
  .reset-credit-relative { color: var(--accent-blue); font-variant-numeric: tabular-nums; }
  .reset-credit-description { margin-top: 2px; color: var(--text-secondary); }
  .reset-note { color: var(--text-secondary); font-size: 11px; line-height: 1.4; }

  .stat-label { font-size: 11px; color: var(--text-secondary); }
  .stat-value { font-size: 13px; font-variant-numeric: tabular-nums; }
  hr { border: none; border-top: 1px solid var(--border); }
  .section-label { font-size: 11px; color: var(--text-secondary); margin-bottom: 4px; }
  .section-toggle { display: flex; align-items: center; gap: 6px; background: none; border: none; cursor: pointer; padding: 0; width: 100%; }
  .section-toggle .section-label { margin-bottom: 0; }
  .chevron { font-size: 9px; color: var(--text-secondary); transition: transform 0.15s; }
  .chevron.open { transform: rotate(90deg); }
  .limit-row { display: flex; align-items: center; gap: 8px; font-size: 12px; padding: 2px 0; }
  .limit-name { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .limit-pct { font-variant-numeric: tabular-nums; }
  .limit-reset { color: var(--text-secondary); }
  .error { font-size: 12px; color: var(--text-secondary); }
  .empty-state { display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 12px; min-height: 200px; color: var(--text-secondary); text-align: center; }
  .provider-title { color: var(--text-primary); font-weight: 600; }
  .retry { background: var(--surface); padding: 6px 16px; border-radius: 6px; }
  .spinner { display: inline-block; width: 14px; height: 14px; border: 2px solid var(--border); border-top-color: var(--accent-blue); border-radius: 50%; animation: spin 0.8s linear infinite; }
  .spinner.large { width: 24px; height: 24px; }
  @keyframes spin { to { transform: rotate(360deg); } }
  :global(.app.compact .tabs) { margin-bottom: 8px; }
  :global(.app.compact .dashboard) { gap: 10px; }
  :global(.app.compact .stats-col) { gap: 5px; }
</style>
