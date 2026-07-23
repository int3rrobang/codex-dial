import { isTauri } from "@tauri-apps/api/core";
import { Image as TauriImage } from "@tauri-apps/api/image";
import { TrayIcon } from "@tauri-apps/api/tray";

export interface LimitTrayState {
  remaining: number;
  currentPace: number;
  suggestedPace: number;
  resetIn?: string;
}

const TRAY_ID = "codex-limit";
const ICON_SIZE = 64;

let trayPromise: Promise<TrayIcon> | null = null;
let lastVisualKey = "";
let lastTooltip = "";

function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}

async function getTray(): Promise<TrayIcon | null> {
  if (!trayPromise) {
    trayPromise = (async () => {
      const existing = await TrayIcon.getById(TRAY_ID);
      if (existing) return existing;
      return TrayIcon.new({
        id: TRAY_ID,
        tooltip: "Codex Dial",
        showMenuOnLeftClick: false,
      });
    })();
  }
  return trayPromise;
}

function strokeArc(
  ctx: CanvasRenderingContext2D,
  start: number,
  end: number,
  width: number,
  color: string
): void {
  ctx.beginPath();
  ctx.arc(ICON_SIZE / 2, ICON_SIZE / 2, 21, start, end);
  ctx.lineWidth = width;
  ctx.lineCap = "round";
  ctx.strokeStyle = color;
  ctx.stroke();
}

async function renderIcon(remaining: number, warning: boolean): Promise<TauriImage> {
  const canvas = document.createElement("canvas");
  canvas.width = ICON_SIZE;
  canvas.height = ICON_SIZE;
  const ctx = canvas.getContext("2d")!;
  ctx.clearRect(0, 0, ICON_SIZE, ICON_SIZE);

  const fraction = clamp(remaining, 0, 100) / 100;
  const start = Math.PI / 4;
  const sweep = Math.PI * 1.5;
  const end = start + sweep;
  const progressEnd = start + sweep * fraction;
  const critical = remaining <= 10;

  const foreground = critical ? "#ff315f" : warning ? "#ff4f89" : "#f5f2ff";

  // Background track
  strokeArc(ctx, start, end, 12, "rgba(0, 0, 0, 0.82)");
  strokeArc(ctx, start, end, 8, "rgba(255, 255, 255, 0.24)");

  // Progress arc
  if (fraction > 0) {
    strokeArc(ctx, start, progressEnd, 12, "rgba(0, 0, 0, 0.92)");
    strokeArc(ctx, start, progressEnd, 8, foreground);
  }

  // Warning dot in center
  if (warning) {
    ctx.beginPath();
    ctx.arc(ICON_SIZE / 2, ICON_SIZE / 2, 6, 0, Math.PI * 2);
    ctx.fillStyle = "rgba(0, 0, 0, 0.9)";
    ctx.fill();
    ctx.beginPath();
    ctx.arc(ICON_SIZE / 2, ICON_SIZE / 2, 3.5, 0, Math.PI * 2);
    ctx.fillStyle = foreground;
    ctx.fill();
  }

  const imageData = ctx.getImageData(0, 0, ICON_SIZE, ICON_SIZE);
  const rgba = new Uint8Array(imageData.data);
  return TauriImage.new(rgba, ICON_SIZE, ICON_SIZE);
}

export async function updateLimitTray(state: LimitTrayState): Promise<void> {
  if (!isTauri()) return;

  const remaining = clamp(state.remaining, 0, 100);
  const warning = state.currentPace > state.suggestedPace;
  const visualKey = `${Math.round(remaining)}:${warning}`;

  const tooltipParts = [
    `Codex: ${Math.round(remaining)}% remaining`,
    `${state.currentPace.toFixed(1)}%/day`,
  ];
  if (warning) tooltipParts.push("slow down");
  if (state.resetIn) tooltipParts.push(`reset in ${state.resetIn}`);
  const tooltip = tooltipParts.join(" · ");

  const tray = await getTray();
  if (!tray) return;

  if (visualKey !== lastVisualKey) {
    const image = await renderIcon(remaining, warning);
    try {
      await tray.setIcon(image);
      lastVisualKey = visualKey;
    } finally {
      await image.close();
    }
  }

  if (tooltip !== lastTooltip) {
    await tray.setTooltip(tooltip);
    lastTooltip = tooltip;
  }
}
