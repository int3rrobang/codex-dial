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

  function getStartsAt() {
    return win.resets_at - win.duration_minutes * 60;
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
    const currentColor = forecast.current_percent_per_day > forecast.historical_percent_per_day
      ? "#ff6b6b"
      : "#6c9eff";

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
            borderColor: "rgba(92, 214, 133, 0.75)",
            borderWidth: 1.5,
            borderDash: [3, 3],
            pointRadius: 0,
            tension: 0,
          },
          {
            label: "Actual",
            data: observed,
            borderColor: "#6c9eff",
            borderWidth: 2,
            pointRadius: 0,
            stepped: "end" as const,
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
            borderColor: "#9090a8",
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
          tooltip: { enabled: true },
        },
        scales: {
          x: {
            type: "linear",
            min: startsAt,
            max: win.resets_at,
            ticks: {
              color: "#9090a8",
              font: { size: 10 },
              callback: (value) => {
                const d = new Date((value as number) * 1000);
                if (win.duration_minutes <= 1440) {
                  return d.toLocaleTimeString("en-US", { hour: "numeric" });
                }
                return d.toLocaleDateString("en-US", { weekday: "short" });
              },
              maxTicksLimit: 8,
            },
            grid: { color: "rgba(144,144,168,0.15)" },
          },
          y: {
            min: 0,
            max: 100,
            ticks: {
              color: "#9090a8",
              font: { size: 10 },
              callback: (value) => `${value}%`,
              stepSize: 25,
            },
            grid: { color: "rgba(144,144,168,0.15)" },
          },
        },
      },
    });
  }

  $effect(() => {
    void win; void samples; void forecast; void safetyBuffer; void fetchedAt; void tokenHistory;
    renderChart();
  });

  onDestroy(() => {
    if (chart) chart.destroy();
  });
</script>

<div class="chart-container">
  <div class="legend">
    <span class="legend-item"><span class="line green dashed"></span> Target</span>
    <span class="legend-item"><span class="line blue"></span> Actual</span>
    <span class="legend-item"><span class="line current" style="border-color: {forecast.current_percent_per_day > forecast.historical_percent_per_day ? '#ff6b6b' : '#6c9eff'}"></span> Current</span>
    <span class="legend-item"><span class="line gray dotted"></span> Historical</span>
  </div>
  <div class="canvas-wrap">
    <canvas bind:this={canvas}></canvas>
  </div>
</div>

<style>
  .chart-container { display: flex; flex-direction: column; gap: 8px; }
  .legend { display: flex; gap: 12px; font-size: 11px; color: var(--text-secondary); flex-shrink: 0; }
  .legend-item { display: flex; align-items: center; gap: 4px; }
  .line { display: inline-block; width: 18px; height: 0; border-top: 2px solid; }
  .line.green { border-color: rgba(92, 214, 133, 0.75); }
  .line.blue { border-color: #6c9eff; }
  .line.current { border-top-style: dashed; }
  .line.gray { border-color: #9090a8; border-top-style: dotted; }
  .line.dashed { border-top-style: dashed; }
  .canvas-wrap { height: 140px; position: relative; }
</style>
