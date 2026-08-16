<script lang="ts">
  import { tick } from "svelte";
  import type { UiState, PaceStatus, Forecast, UsageWindow, UsageSample } from "./types";
  import { applyCodexResetCredit, refresh, view } from "./store";
  import FluentIcon from "./FluentIcon.svelte";
  import BurnDownChart from "./BurnDownChart.svelte";


  let { data, onlayoutchange }: { data: UiState; onlayoutchange?: () => void } = $props();

  let ocgWindow = $state(1);
  let showOtherLimits = $state(false);
  let tab = $state<"codex" | "opencode">("codex");
  let showOtherWindows = $state(false);
  let showBankedResets = $state(false);
  let bankedResetStat = $state<HTMLDivElement>();
  let selectedBankedResetId = $state<string | null>(null);
  let bankedResetIdempotencyKey = $state<string | null>(null);
  let isApplyingBankedReset = $state(false);
  let bankedResetError = $state<string | null>(null);
  let now = $state(Date.now());



  $effect(() => {
    const interval = window.setInterval(() => { now = Date.now(); }, 60000);
    return () => window.clearInterval(interval);
  });

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

  function selectBankedResetCredit(credit: NonNullable<UiState["snapshot"]>["banked_reset_credits"][number]) {
    if (isApplyingBankedReset || selectedBankedResetId === credit.id) return;
    selectedBankedResetId = credit.id;
    bankedResetIdempotencyKey = crypto.randomUUID();
    bankedResetError = null;
  }

  function cancelBankedResetSelection() {
    if (isApplyingBankedReset) return;
    selectedBankedResetId = null;
    bankedResetIdempotencyKey = null;
    bankedResetError = null;
  }

  async function applySelectedBankedReset() {
    const creditId = selectedBankedResetId;
    const idempotencyKey = bankedResetIdempotencyKey;
    const credit = data.snapshot?.banked_reset_credits.find((item) => item.id === creditId);
    if (isApplyingBankedReset || !creditId || !idempotencyKey || !credit) return;

    isApplyingBankedReset = true;
    bankedResetError = null;
    try {
      await applyCodexResetCredit(creditId, idempotencyKey);
      showBankedResets = false;
      selectedBankedResetId = null;
      bankedResetIdempotencyKey = null;
    } catch {
      bankedResetError = "We couldn't apply this reset. Try again.";
    } finally {
      isApplyingBankedReset = false;
    }
  }
  function handleTabNavigation(
    event: KeyboardEvent,
    currentIndex: number,
    count: number,
    select: (index: number) => void,
  ) {
    if (count < 2) return;
    let nextIndex = currentIndex;
    if (event.key === "ArrowRight" || event.key === "ArrowDown") {
      nextIndex = (currentIndex + 1) % count;
    } else if (event.key === "ArrowLeft" || event.key === "ArrowUp") {
      nextIndex = (currentIndex - 1 + count) % count;
    } else if (event.key === "Home") {
      nextIndex = 0;
    } else if (event.key === "End") {
      nextIndex = count - 1;
    } else {
      return;
    }

    event.preventDefault();
    const tabList = (event.currentTarget as HTMLElement).parentElement;
    select(nextIndex);
    requestAnimationFrame(() => {
      const tabs = tabList?.querySelectorAll<HTMLElement>('[role="tab"]');
      tabs?.[nextIndex]?.focus();
    });
  }


  async function selectProvider(provider: "codex" | "opencode") {
    tab = provider;
    await tick();
    onlayoutchange?.();
  }

  async function toggleOtherLimits() {
    showOtherLimits = !showOtherLimits;
    await tick();
    onlayoutchange?.();
  }

  async function toggleOtherWindows() {
    showOtherWindows = !showOtherWindows;
    await tick();
    onlayoutchange?.();
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
      case "slowDown": return "var(--system-critical)";
      case "onTrack": return "var(--system-success)";
      case "roomToUseMore": return "var(--accent-secondary)";
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

  function bankedResetIsImminent(ts: number): boolean {
    const remaining = ts - now / 1000;
    return remaining > 0 && remaining < 86400;
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
    <button class="accent-button retry" onclick={() => view.set("settings")}>Open Settings</button>
  </div>
{:else}
{#if data.codex_enabled && data.opencode_go_enabled}
  <div class="tabs selector-group" role="tablist" aria-label="Usage provider">
    <button
      class="tab"
      id="codex-tab"
      class:active={tab === "codex"}
      role="tab"
      aria-selected={tab === "codex"}
      aria-controls="codex-panel"
      tabindex={tab === "codex" ? 0 : -1}
      onclick={() => void selectProvider("codex")}
      onkeydown={(event) => handleTabNavigation(event, tab === "codex" ? 0 : 1, 2, (next) => void selectProvider(next === 0 ? "codex" : "opencode"))}
    >Codex</button>
    <button
      class="tab"
      id="opencode-tab"
      class:active={tab === "opencode"}
      role="tab"
      aria-selected={tab === "opencode"}
      aria-controls="opencode-panel"
      tabindex={tab === "opencode" ? 0 : -1}
      onclick={() => void selectProvider("opencode")}
      onkeydown={(event) => handleTabNavigation(event, tab === "codex" ? 0 : 1, 2, (next) => void selectProvider(next === 0 ? "codex" : "opencode"))}
    >OpenCode Go</button>
  </div>
{/if}

<div class="tab-panels">
  <div
    id="codex-panel"
    class="tab-panel"
    class:inactive={tab !== "codex"}
    role="tabpanel"
    aria-labelledby="codex-tab"
    aria-hidden={tab !== "codex"}
  >
  {#if data.snapshot && data.forecast}
    {@const snapshot = data.snapshot}
    {@const forecast = data.forecast}
    <div class="dashboard">
      <div class="header">
        <span class="remaining">{Math.round(snapshot.main_limit.window.remaining_percent)}%</span>
        <span class="remaining-label">remaining</span>
        <span class="spacer"></span>
        <button class="icon-button refresh-button" onclick={() => refresh()} aria-label="Refresh Codex usage">
          {#if data.is_refreshing}
            <span class="spinner" aria-hidden="true"></span>
          {:else}
            <FluentIcon name="refresh" size={15} />
          {/if}
        </button>
      </div>

      <div class="status-section" role="status" aria-live="polite">
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
                class="subtle-button banked-reset-trigger"
                aria-expanded={showBankedResets}
                aria-controls="banked-reset-details"
                aria-label={`${snapshot.banked_reset_count} banked reset${snapshot.banked_reset_count === 1 ? "" : "s"}${snapshot.banked_reset_credits[0] ? `; next expires ${formatResetDeadline(snapshot.banked_reset_credits[0].expires_at)}` : ""}`}
                onclick={() => showBankedResets = !showBankedResets}
              >
                <span class="stat-value">{snapshot.banked_reset_count}</span>
                {#if snapshot.banked_reset_credits.length > 0}
                  <span class="banked-next-deadline" class:imminent={bankedResetIsImminent(snapshot.banked_reset_credits[0].expires_at)}>Next {formatResetTime(snapshot.banked_reset_credits[0].expires_at)}</span>
                {/if}
                <span class="chevron" class:open={showBankedResets}><FluentIcon name="chevron-right" size={12} /></span>
              </button>
            {:else}
              <span class="stat-value">0</span>
            {/if}

            {#if showBankedResets && snapshot.banked_reset_count > 0}
              <div id="banked-reset-details" class="reset-popover" role="dialog" aria-label="Banked reset deadlines">
                <div class="reset-popover-title">Banked reset deadlines</div>
                {#if snapshot.banked_reset_credits.length > 0}
                  {@const selectedCredit = selectedBankedResetId
                    ? snapshot.banked_reset_credits.find((credit) => credit.id === selectedBankedResetId) ?? null
                    : null}
                  <div class="reset-list">
                    {#each snapshot.banked_reset_credits as credit}
                      {#if credit.id}
                        <button
                          type="button"
                          class="reset-credit"
                          class:selected={selectedBankedResetId === credit.id}
                          aria-pressed={selectedBankedResetId === credit.id}
                          aria-label={`Select ${credit.title}; expires ${formatResetDeadline(credit.expires_at)}`}
                          disabled={isApplyingBankedReset}
                          onclick={() => selectBankedResetCredit(credit)}
                        >
                          <span class="reset-credit-title">{credit.title}</span>
                          <span class="reset-credit-deadline">Expires {formatResetDeadline(credit.expires_at)}</span>
                          <span class="reset-credit-relative">{resetExpiryText(credit.expires_at)}</span>
                          {#if credit.description}
                            <span class="reset-credit-description">{credit.description}</span>
                          {/if}
                        </button>
                      {:else}
                        <div class="reset-credit reset-credit-unavailable">
                          <div class="reset-credit-title">{credit.title}</div>
                          <div class="reset-credit-deadline">Expiry details unavailable</div>
                          <div class="reset-credit-description">Refresh later or update the Codex CLI before applying this reset.</div>
                        </div>
                      {/if}
                    {/each}
                  </div>
                  {#if snapshot.banked_reset_credits.length < snapshot.banked_reset_count}
                    <p class="reset-note">
                      Showing {snapshot.banked_reset_credits.length} of {snapshot.banked_reset_count} deadlines returned by Codex. The missing resets cannot be applied here.
                    </p>
                  {/if}
                  {#if selectedCredit}
                    <div class="reset-confirmation" role="group" aria-labelledby="banked-reset-confirmation-title">
                      <div id="banked-reset-confirmation-title" class="reset-confirmation-title">Apply {selectedCredit.title}?</div>
                      <div class="reset-confirmation-expiry">Expires {formatResetDeadline(selectedCredit.expires_at)}</div>
                      <p class="reset-confirmation-warning">Applying this reset consumes one reset and cannot be undone.</p>
                      {#if bankedResetError}
                        <div class="reset-confirmation-error" role="alert">{bankedResetError}</div>
                      {/if}
                      <div class="reset-confirmation-actions">
                        <button
                          type="button"
                          class="subtle-button"
                          disabled={isApplyingBankedReset}
                          onclick={cancelBankedResetSelection}
                        >Cancel</button>
                        <button
                          type="button"
                          class="accent-button"
                          disabled={isApplyingBankedReset}
                          aria-busy={isApplyingBankedReset}
                          onclick={() => void applySelectedBankedReset()}
                        >
                          {#if isApplyingBankedReset}
                            <span class="spinner" aria-hidden="true"></span>
                            Applying…
                          {:else}
                            Apply
                          {/if}
                        </button>
                      </div>
                    </div>
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
        <div class="other-limits">
          <button
            class="subtle-button section-toggle"
            aria-expanded={showOtherLimits}
            aria-controls="codex-other-limits"
            onclick={toggleOtherLimits}
          >
            <span class="section-label">Other limits ({snapshot.other_limits.length})</span>
            <span class="chevron" class:open={showOtherLimits}><FluentIcon name="chevron-right" size={12} /></span>
          </button>
          {#if showOtherLimits}
            <div id="codex-other-limits" class="limit-list">
              {#each snapshot.other_limits as limit}
                <div class="limit-row">
                  <span class="limit-name">{limit.name}</span>
                  <span class="limit-pct">{Math.round(limit.window.remaining_percent)}%</span>
                  <span class="limit-reset">{formatResetTime(limit.window.resets_at)}</span>
                </div>
              {/each}
            </div>
          {/if}
        </div>
      {/if}

      {#if data.error_message}
        <div class="error" role="alert">
          <FluentIcon name="warning" size={14} />
          <span>{data.error_message}</span>
        </div>
      {/if}

    </div>
  {:else}
    <div class="empty-state">
      {#if data.is_refreshing}
        <span class="spinner large" aria-hidden="true"></span>
        <p>Reading Codex usage...</p>
      {:else}
        <FluentIcon name="warning" size={20} />
        <p>{data.error_message ?? "Codex usage is not available."}</p>
        <button class="accent-button retry" onclick={() => refresh()}>Try Again</button>
      {/if}
    </div>
  {/if}
  </div>
  <div
    id="opencode-panel"
    class="tab-panel"
    class:inactive={tab !== "opencode"}
    role="tabpanel"
    aria-labelledby="opencode-tab"
    aria-hidden={tab !== "opencode"}
  >
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
        <div class="window-tabs" role="tablist" aria-label="OpenCode Go usage window">
          {#each ocg.windows as w, i}
            <button
              class="window-tab"
              class:active={i === idx}
              role="tab"
              aria-selected={i === idx}
              aria-label={`${w.name} window`}
              tabindex={i === idx ? 0 : -1}
              onclick={() => ocgWindow = i}
              onkeydown={(event) => handleTabNavigation(event, i, ocg.windows.length, (next) => ocgWindow = next)}
            >{w.name}</button>
          {/each}
        </div>
        <span class="spacer"></span>
        <button class="icon-button refresh-button" onclick={() => refresh()} aria-label="Refresh OpenCode Go usage">
          {#if data.is_refreshing}
            <span class="spinner" aria-hidden="true"></span>
          {:else}
            <FluentIcon name="refresh" size={15} />
          {/if}
        </button>
      </div>

      <div class="status-section" role="status" aria-live="polite">
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
        <div class="other-limits">
          <button
            class="subtle-button section-toggle"
            aria-expanded={showOtherWindows}
            aria-controls="opencode-other-windows"
            onclick={toggleOtherWindows}
          >
            <span class="section-label">Other windows ({ocg.windows.length - 1})</span>
            <span class="chevron" class:open={showOtherWindows}><FluentIcon name="chevron-right" size={12} /></span>
          </button>
          {#if showOtherWindows}
            <div id="opencode-other-windows" class="limit-list">
              {#each ocg.windows.filter((_, i) => i !== idx) as w}
                <div class="limit-row">
                  <span class="limit-name">{w.name}</span>
                  <span class="limit-pct">{Math.round(w.remaining_percent)}%</span>
                  <span class="limit-reset">{relativeTime(w.resets_at)}</span>
                </div>
              {/each}
            </div>
          {/if}
        </div>
      {/if}

      {#if data.opencode_go_error}
        <div class="error" role="alert">
          <FluentIcon name="warning" size={14} />
          <span>{data.opencode_go_error}</span>
        </div>
      {/if}

    </div>
  {:else}
    <div class="empty-state">
      {#if data.is_refreshing}
        <span class="spinner large" aria-hidden="true"></span>
        <p>Reading OpenCode Go usage...</p>
      {:else}
        <FluentIcon name="warning" size={20} />
        <p>{data.opencode_go_error ?? "OpenCode Go usage is not available."}</p>
        <button class="accent-button retry" onclick={() => refresh()}>Try Again</button>
      {/if}
    </div>
  {/if}
  </div>
</div>
{/if}

<style>
  .selector-group {
    display: flex;
    align-items: center;
    gap: 2px;
    min-width: 0;
    padding: 2px;
    border-radius: var(--control-radius);
    background: color-mix(in srgb, var(--flyout-card-background, var(--control-fill-secondary)) 76%, transparent);
    margin-inline-end: var(--space-1);
  }
  .tabs { margin-bottom: 10px; }
  .tab-panels {
    display: grid;
    padding: 0 var(--space-1) var(--space-3);
  }
  .tab-panel { grid-area: 1 / 1; min-width: 0; }
  .tab-panel.inactive { visibility: hidden; pointer-events: none; }
  .tab {
    position: relative;
    flex: 1;
    min-width: 0;
    min-height: 30px;
    padding: 5px 10px;
    border: 0;
    border-radius: calc(var(--control-radius) - 1px);
    background: transparent;
    color: var(--text-secondary);
    font-size: 13px;
    font-weight: 600;
    cursor: pointer;
    transition: background-color var(--duration-fast) ease-out, color var(--duration-fast) ease-out;
  }
  .tab:hover,
  .window-tab:hover {
    background: color-mix(in srgb, var(--accent-default) 8%, transparent);
    color: var(--text-primary);
  }
  .tab.active,
  .window-tab.active {
    background: color-mix(in srgb, var(--accent-default) 14%, transparent);
    color: var(--accent-secondary);
  }
  .tab.active::after,
  .window-tab.active::after {
    position: absolute;
    right: 8px;
    bottom: 2px;
    left: 8px;
    height: 2px;
    border-radius: 2px;
    background: var(--accent-default);
    content: "";
  }
  .window-tabs {
    display: flex;
    align-items: center;
    gap: 2px;
    max-width: 100%;
    min-width: 0;
    overflow-x: auto;
    white-space: nowrap;
    scrollbar-width: none;
    width: max-content;
    justify-self: start;
  }
  .window-tabs::-webkit-scrollbar { display: none; }
  .window-tab {
    position: relative;
    flex: 0 0 auto;
    min-height: 26px;
    padding: 4px 8px;
    border: 0;
    border-radius: var(--control-radius);
    background: transparent;
    color: var(--text-secondary);
    font-size: 11px;
    cursor: pointer;
    white-space: nowrap;
    transition: background-color var(--duration-fast) ease-out, color var(--duration-fast) ease-out;
  }
  .dashboard { display: flex; flex-direction: column; gap: 12px; flex: 1; min-height: 0; }
  .header {
    display: grid;
    grid-template-columns: max-content max-content minmax(0, auto) minmax(0, 1fr) auto;
    align-items: center;
    column-gap: 6px;
    width: 100%;
    height: 36px;
    min-height: 36px;
  }
  .remaining { font-size: 32px; font-weight: 600; font-variant-numeric: tabular-nums; letter-spacing: -0.02em; line-height: 1; }
  .remaining-label { color: var(--text-secondary); white-space: nowrap; }
  .spacer { grid-column: 4; min-width: 0; }
  .refresh-button { grid-column: 5; justify-self: end; }
  .status-section {
    display: flex;
    flex-direction: column;
    justify-content: center;
    gap: 4px;
    height: 64px;
    min-height: 64px;
  }
  .status-title { font-weight: 600; font-size: 14px; }
  .status-message { color: var(--text-secondary); font-size: 14px; line-height: 1.35; }
  .stats-grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    grid-template-rows: repeat(3, calc(var(--space-4) * 2 + var(--space-1)));
    grid-auto-flow: column;
    column-gap: 20px;
    row-gap: 8px;
  }
  .stats-col { display: contents; }
  .stat-item { display: flex; flex-direction: column; gap: 0; min-width: 0; min-height: 0; height: 100%; }
  .banked-reset-stat { position: relative; }
  .banked-reset-trigger {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
    min-height: calc(var(--space-4) + var(--space-1));
    padding: 0 4px;
    border-radius: var(--control-radius);
    color: var(--text-primary);
    text-align: left;
  }
  .banked-reset-trigger:hover { background: var(--control-fill-secondary); }
  .banked-next-deadline {
    min-width: 0;
    overflow: hidden;
    color: var(--text-secondary);
    font-size: 11px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .banked-next-deadline.imminent {
    color: var(--system-critical);
    text-shadow: 0 0 6px color-mix(in srgb, var(--system-critical) 80%, transparent);
    animation: banked-reset-glow 1.8s ease-in-out infinite alternate;
  }
  @keyframes banked-reset-glow {
    from { text-shadow: 0 0 3px color-mix(in srgb, var(--system-critical) 55%, transparent); }
    to { text-shadow: 0 0 8px color-mix(in srgb, var(--system-critical) 90%, transparent); }
  }
  .reset-popover {
    position: absolute;
    right: 0;
    bottom: calc(100% + 8px);
    z-index: 20;
    width: min(300px, calc(100vw - 32px));
    max-height: 240px;
    overflow-y: auto;
    padding: 10px;
    border: 1px solid color-mix(in srgb, var(--card-stroke) 65%, transparent);
    border-radius: var(--overlay-radius);
    background: var(--flyout-card-background, var(--solid-background-secondary));
    box-shadow: 0 8px 24px color-mix(in srgb, var(--solid-background-base) 38%, transparent);
  }
  .reset-popover-title { margin-bottom: 8px; color: var(--text-primary); font-size: 12px; font-weight: 600; }
  .reset-list { display: flex; flex-direction: column; gap: 9px; }
  .reset-credit {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    width: 100%;
    min-width: 0;
    padding: 0 0 var(--space-2);
    border: 0;
    border-bottom: 1px solid var(--control-stroke-default);
    background: transparent;
    color: inherit;
    font: inherit;
    font-size: 11px;
    text-align: left;
    cursor: pointer;
  }
  .reset-credit:hover,
  .reset-credit.selected { background: color-mix(in srgb, var(--accent-default) 8%, transparent); }
  .reset-credit:disabled { cursor: wait; opacity: 0.65; }
  .reset-credit-unavailable { cursor: default; }
  .reset-credit:last-child { padding-bottom: 0; border-bottom: none; }
  .reset-credit-title { display: block; color: var(--text-primary); font-weight: 600; }
  .reset-credit-deadline { display: block; margin-top: 2px; color: var(--text-secondary); }
  .reset-credit-relative { display: block; color: var(--accent-secondary); font-variant-numeric: tabular-nums; }
  .reset-credit-description { display: block; margin-top: 2px; color: var(--text-secondary); }
  .reset-note { color: var(--text-secondary); font-size: 11px; line-height: 1.4; }
  .reset-confirmation {
    margin-top: var(--space-2);
    padding-top: var(--space-2);
    border-top: 1px solid var(--control-stroke-default);
  }
  .reset-confirmation-title { color: var(--text-primary); font-size: 12px; font-weight: 600; }
  .reset-confirmation-expiry { margin-top: 2px; color: var(--text-secondary); font-size: 11px; }
  .reset-confirmation-warning {
    margin: var(--space-2) 0 0;
    color: var(--system-critical);
    font-size: 11px;
    line-height: 1.4;
  }
  .reset-confirmation-error {
    margin-top: var(--space-2);
    padding: var(--space-1) var(--space-2);
    border-radius: var(--control-radius);
    background: var(--system-background-critical);
    color: var(--text-primary);
    font-size: 11px;
    line-height: 1.4;
  }
  .reset-confirmation-actions {
    display: flex;
    justify-content: flex-end;
    gap: var(--space-2);
    margin-top: var(--space-2);
  }
  .reset-confirmation-actions button {
    min-height: 28px;
    padding-inline: var(--space-3);
  }
  .reset-confirmation-actions button:disabled { cursor: wait; opacity: 0.7; }
  .stat-label { font-size: 11px; line-height: var(--space-4); color: var(--text-secondary); }
  .stat-value { font-size: 13px; line-height: var(--space-4); font-variant-numeric: tabular-nums; }
  .section-label { font-size: 11px; color: var(--text-secondary); }
  .section-toggle {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
    padding: 2px 4px;
    border-radius: var(--control-radius);
    text-align: left;
  }
  .section-toggle:hover { background: var(--control-fill-secondary); }
  .limit-list { display: flex; flex-direction: column; gap: 4px; margin-top: 4px; }
  .chevron {
    display: inline-flex;
    flex: 0 0 auto;
    color: var(--text-secondary);
    transition: transform var(--duration-fast) ease-out;
  }
  .chevron.open { transform: rotate(90deg); }
  .limit-row { display: flex; align-items: center; gap: 8px; min-height: 22px; font-size: 12px; }
  .limit-name { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .limit-pct { font-variant-numeric: tabular-nums; }
  .limit-reset { color: var(--text-secondary); }
  .error {
    display: flex;
    align-items: flex-start;
    gap: 6px;
    padding: 7px 8px;
    border-radius: var(--control-radius);
    background: var(--system-background-critical);
    box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--system-critical) 55%, transparent);
    color: var(--text-primary);
    font-size: 12px;
    line-height: 1.35;
  }
  .empty-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 10px;
    min-height: 200px;
    color: var(--text-secondary);
    text-align: center;
  }
  .empty-state p { font-size: 14px; }
  .provider-title { color: var(--text-primary); font-weight: 600; }
  .retry { padding: 6px 14px; }
  .spinner {
    display: inline-block;
    width: 14px;
    height: 14px;
    border: 2px solid var(--control-strong-stroke-default);
    border-top-color: var(--accent-default);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }
  .spinner.large { width: 24px; height: 24px; }
  @keyframes spin { to { transform: rotate(360deg); } }
  :global(.app.compact .tabs) { margin-bottom: 6px; }
  :global(.app.compact .dashboard) { gap: 9px; }
  :global(.app.compact .status-section) { height: 56px; min-height: 56px; }
  :global(.app.compact .stats-col) { gap: 5px; }
  @media (prefers-reduced-motion: reduce) {
    .spinner { animation: none; }
    .tab, .window-tab, .chevron { transition: none; }
    .banked-next-deadline.imminent {
      animation: none;
      text-shadow: 0 0 6px color-mix(in srgb, var(--system-critical) 80%, transparent);
    }
  }
  @media (forced-colors: active) {
    .selector-group { background: Canvas; }
    .tab:hover,
    .window-tab:hover { background: ButtonFace; color: ButtonText; }
    .tab.active,
    .window-tab.active { background: Highlight; color: HighlightText; }
    .tab.active::after,
    .window-tab.active::after { background: HighlightText; }
    .banked-next-deadline.imminent {
      color: MarkText;
      text-shadow: none;
    }
  }
</style>
