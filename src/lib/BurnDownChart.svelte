<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import Chart from "chart.js/auto";
  import type { UsageWindow, UsageSample, TokenDay, Forecast } from "./types";

  let {
    win,
    samples,
    tokenHistory,
    fetchedAt,
    forecast,
    safetyBuffer,
  }: {
    win: UsageWindow;
    samples: UsageSample[];
    tokenHistory: TokenDay[];
    fetchedAt: number;
    forecast: Forecast;
    safetyBuffer: number;
  } = $props();

  let canvas = $state<HTMLCanvasElement | undefined>(undefined);
  let chart: Chart | null = null;
  let themeMedia: MediaQueryList | null = null;
  let themeVersion = $state(0);
  let colorProbe: HTMLSpanElement | null = null;

  function cssToken(name: string, fallback: string): string {
    if (typeof document === "undefined") return fallback;
    const raw = getComputedStyle(document.documentElement).getPropertyValue(name).trim();
    if (!raw) return fallback;

    if (!colorProbe) {
      colorProbe = document.createElement("span");
      colorProbe.setAttribute("aria-hidden", "true");
      colorProbe.style.position = "absolute";
      colorProbe.style.inlineSize = "0";
      colorProbe.style.blockSize = "0";
      colorProbe.style.pointerEvents = "none";
      document.documentElement.append(colorProbe);
    }

    // Custom properties can contain system colors (for example AccentColor).
    // Resolve them through the browser before handing them to Canvas/Chart.js.
    colorProbe.style.color = `var(${name})`;
    const resolved = getComputedStyle(colorProbe).color.trim();
    return resolved || fallback;
  }

  function withAlpha(color: string, alpha: number): string {
    const hex = color.match(/^#([0-9a-f]{3}|[0-9a-f]{6})$/i);
    if (hex) {
      const value = hex[1].length === 3
        ? hex[1].split("").map((part) => part + part).join("")
        : hex[1];
      const channels = [0, 2, 4].map((offset) => Number.parseInt(value.slice(offset, offset + 2), 16));
      return `rgb(${channels.join(" ")} / ${alpha})`;
    }
    const rgb = color.match(/^rgba?\(\s*([\d.]+)[,\s]+([\d.]+)[,\s]+([\d.]+)/i);
    if (rgb) return `rgb(${rgb[1]} ${rgb[2]} ${rgb[3]} / ${alpha})`;
    return color;
  }

  function handleThemeChange() {
    themeVersion += 1;
  }

  function getStartsAt() {
    return forecast.window_started_at;
  }

  function buildObserved(): { x: number; y: number }[] {
    const startsAt = getStartsAt();
    const current = { x: fetchedAt, y: win.remaining_percent };
    const local = samples
      .filter((s) => s.observed_at > startsAt && s.observed_at < fetchedAt)
      .map((s) => ({ x: s.observed_at, y: s.remaining_percent }))
      .sort((a, b) => a.x - b.x);

    const firstKnown = local.length > 0 ? local[0] : current;
    const buckets = tokenHistory
      .filter((td) => {
        const ts = new Date(td.date + "T00:00:00Z").getTime() / 1000;
        return ts + 86400 > startsAt && ts < firstKnown.x;
      })
      .sort((a, b) => a.date.localeCompare(b.date));

    const totalTokens = buckets.reduce((sum, b) => sum + b.tokens, 0);
    const bootstrapped: { x: number; y: number }[] = [];

    if (totalTokens > 0) {
      let cumulative = 0;
      for (const bucket of buckets) {
        cumulative += bucket.tokens;
        const ts = new Date(bucket.date + "T00:00:00Z").getTime() / 1000;
        const date = Math.min(Math.max(ts + 86400, startsAt), firstKnown.x);
        const used = ((100 - firstKnown.y) * cumulative) / totalTokens;
        bootstrapped.push({ x: date, y: 100 - used });
      }
    }

    const all = [{ x: startsAt, y: 100 }, ...bootstrapped, ...local, current];
    const deduped: { x: number; y: number }[] = [];
    for (const pt of all.sort((a, b) => a.x - b.x)) {
      if (deduped.length > 0 && deduped[deduped.length - 1].x === pt.x) {
        deduped[deduped.length - 1] = pt;
      } else {
        deduped.push(pt);
      }
    }
    return deduped;
  }

  function buildProjection(rate: number, remainingAtReset: number): { x: number; y: number }[] {
    const current = { x: fetchedAt, y: win.remaining_percent };
    if (rate <= 0) {
      return [current, { x: win.resets_at, y: win.remaining_percent }];
    }
    const exhaustion = fetchedAt + (win.remaining_percent / rate) * 86400;
    const endpoint =
      exhaustion < win.resets_at
        ? { x: exhaustion, y: 0 }
        : { x: win.resets_at, y: remainingAtReset };
    return [current, endpoint];
  }

  function renderChart() {
    if (!canvas) return;
    if (chart) chart.destroy();

    const startsAt = getStartsAt();
    const showEveryDay = win.duration_minutes > 1440 && win.duration_minutes <= 8 * 1440;
    const targetColor = cssToken("--system-success", "#107c10");
    const actualColor = cssToken("--accent-default", "#0f6cbd");
    const criticalColor = cssToken("--system-critical", "#c42b1c");
    const secondaryAccent = cssToken("--accent-secondary", "#115ea3");
    const secondaryText = cssToken("--text-secondary", "#5f5f5f");
    const gridColor = withAlpha(cssToken("--control-stroke-default", "#d1d1d1"), 0.3);
    const surfaceColor = cssToken("--solid-background-tertiary", "#dedede");
    const primaryText = cssToken("--text-primary", "#1a1a1a");
    const cardStroke = cssToken("--card-stroke", "#e2e2e2");
    const currentColor = forecast.current_percent_per_day > forecast.historical_percent_per_day
      ? criticalColor
      : secondaryAccent;

    const target = [
      { x: startsAt, y: 100 },
      { x: win.resets_at, y: safetyBuffer },
    ];

    const observed = buildObserved();
    const currentProj = buildProjection(forecast.current_percent_per_day, forecast.expected_remaining_at_reset);
    const historicalProj = buildProjection(forecast.historical_percent_per_day, forecast.historical_remaining_at_reset);

    chart = new Chart(canvas, {
      type: "line",
      data: {
        datasets: [
          {
            label: "Target",
            data: target,
            borderColor: targetColor,
            borderWidth: 1.5,
            borderDash: [3, 3],
            pointRadius: 0,
            tension: 0,
          },
          {
            label: "Actual",
            data: observed,
            borderColor: actualColor,
            borderWidth: 2,
            pointRadius: 0,
            stepped: "after" as const,
          },
          {
            label: "Current",
            data: currentProj,
            borderColor: currentColor,
            borderWidth: 2.5,
            borderDash: [7, 3],
            pointRadius: 0,
            tension: 0,
          },
          {
            label: "Historical",
            data: historicalProj,
            borderColor: secondaryText,
            borderWidth: 1.5,
            borderDash: [2, 3],
            pointRadius: 0,
            tension: 0,
          },
        ],
      },
      options: {
        responsive: true,
        maintainAspectRatio: false,
        animation: false,
        interaction: { mode: "nearest", intersect: false },
        plugins: {
          legend: { display: false },
          tooltip: {
            enabled: true,
            backgroundColor: surfaceColor,
            titleColor: primaryText,
            bodyColor: primaryText,
            borderColor: cardStroke,
            borderWidth: 1,
          },
        },
        scales: {
          x: {
            type: "linear",
            min: startsAt,
            max: win.resets_at,
            ticks: {
              color: secondaryText,
              font: { size: 10 },
              autoSkip: !showEveryDay,
              stepSize: showEveryDay ? 86400 : undefined,
              callback: (value) => {
                const d = new Date((value as number) * 1000);
                if (win.duration_minutes <= 1440) {
                  return d.toLocaleTimeString("en-US", { hour: "numeric" });
                }
                return d.toLocaleDateString("en-US", { weekday: "short" });
              },
              maxTicksLimit: showEveryDay ? 9 : 8,
            },
            grid: { color: gridColor },
          },
          y: {
            min: 0,
            max: 100,
            ticks: {
              color: secondaryText,
              font: { size: 10 },
              callback: (value) => `${value}%`,
              stepSize: 25,
            },
            grid: { color: gridColor },
          },
        },
      },
    });
  }

  onMount(() => {
    themeMedia = window.matchMedia("(prefers-color-scheme: dark)");
    themeMedia.addEventListener("change", handleThemeChange);
  });
  $effect(() => {
    void win; void samples; void forecast; void safetyBuffer; void fetchedAt; void tokenHistory; void themeVersion;
    renderChart();
  });

  onDestroy(() => {
    themeMedia?.removeEventListener("change", handleThemeChange);
    if (chart) chart.destroy();
    colorProbe?.remove();
    colorProbe = null;
  });
</script>

<div class="chart-container">
  <div class="legend">
    <span class="legend-item"><span class="line target dashed" aria-hidden="true"></span> Target</span>
    <span class="legend-item"><span class="line actual" aria-hidden="true"></span> Actual</span>
    <span class="legend-item"><span class="line current" class:critical={forecast.current_percent_per_day > forecast.historical_percent_per_day} aria-hidden="true"></span> Current</span>
    <span class="legend-item"><span class="line historical dotted" aria-hidden="true"></span> Historical</span>
  </div>
  <div class="canvas-wrap">
    <canvas bind:this={canvas} aria-label="Usage burn-down chart"></canvas>
  </div>
</div>

<style>
  .chart-container { display: flex; flex-direction: column; gap: var(--space-2); }
  .legend {
    display: flex;
    flex-wrap: wrap;
    align-content: flex-start;
    height: var(--space-4);
    min-height: var(--space-4);
    gap: 6px 12px;
    color: var(--text-secondary);
    font-size: 11px;
    line-height: var(--space-4);
    flex-shrink: 0;
  }
  .legend-item { display: flex; align-items: center; gap: 4px; }
  .line { display: inline-block; width: 18px; height: 0; border-top: 2px solid; }
  .line.target { border-color: var(--system-success); }
  .line.actual { border-color: var(--accent-default); }
  .line.current { border-color: var(--accent-secondary); border-top-style: dashed; }
  .line.current.critical { border-color: var(--system-critical); }
  .line.historical { border-color: var(--text-secondary); border-top-style: dotted; }
  .line.dashed { border-top-style: dashed; }
  .line.dotted { border-top-style: dotted; }
  .canvas-wrap { height: 140px; position: relative; }
  :global(.app.compact .canvas-wrap) { height: 110px; }
  @media (forced-colors: active) {
    .legend { color: CanvasText; }
    .line.target { border-color: GrayText; }
    .line.actual { border-color: Highlight; }
    .line.current { border-color: CanvasText; }
    .line.current.critical { border-color: Mark; }
    .line.historical { border-color: GrayText; }
  }
</style>
