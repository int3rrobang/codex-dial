# Codex Dial

A Windows system-tray app that tracks your OpenAI Codex usage allowance and helps you pace yourself until the limit resets.

Ported from [thrr87/codex-limits](https://github.com/thrr87/codex-limits) (macOS) to Windows using Tauri 2, Rust, and Svelte.

![Codex Dial screenshot](docs/screenshot.png)

## What it does

- Reads your Codex rate limits by talking to the Codex CLI over JSON-RPC (stdio)
- Shows a burn-down chart with four projections: target, actual, current pace, and historical pace
- Gives a plain-language verdict: **Slow down**, **On track**, or **Room to use more**
- Dynamic tray icon — a C-shaped arc that depletes as your quota burns (turns pink/red when you're going too fast)
- Tracks 90 days of usage history in local JSON files
- Optional folder sync (OneDrive, Dropbox, etc.) to share history across machines
- Refreshes every 10 minutes, on wake from sleep, or manually

## OpenCode Go support

Codex Dial also includes an optional OpenCode Go backend. When enabled, the Rust backend fetches OpenCode Go usage from `opencode.ai`, resolves the workspace, and produces forecasts for each usage window alongside the normal Codex view.

OpenCode Go is disabled by default. To enable it:

1. Open **Settings** and enable **OpenCode Go**.
2. Copy your `auth` or `__Host-auth` session cookie from your browser's developer tools (**Application → Cookies**).
3. Paste the cookie into Codex Dial and select **Save**.
4. Refresh the dashboard. If automatic workspace discovery does not work, paste the `wrk_...` workspace ID as an override in Settings.

The session cookie is saved in the app's local configuration and acts like a password. Treat it as sensitive, do not share it, and remove or replace it if it is exposed. OpenCode Go depends on authenticated web responses from `opencode.ai`, so expired cookies or upstream website changes can cause the optional tab to show an error without affecting Codex monitoring.

## Stats at a glance

| Left | Right |
|------|-------|
| Reset in (relative time) | Banked resets (count) |
| Suggested pace (%/day) | Nearest reset deadline |
| Current pace (%/day) | Historical pace (%/day) |

## Requirements

- Windows 10/11
- [Codex CLI](https://github.com/openai/codex) installed and signed in (`codex` or `codex.cmd` on PATH)
- Optional: an OpenCode Go account and an active browser session cookie if you want to use the OpenCode Go tab

## Install

Grab the latest `.exe` installer from [Releases](../../releases).

## Build from source

### Prerequisites

- [Rust](https://rustup.rs/) (stable)
- [Node.js](https://nodejs.org/) 18+
- [Tauri 2 prerequisites](https://v2.tauri.app/start/prerequisites/) (WebView2, Visual Studio C++ Build Tools)
- A signed-in Codex CLI installation for live Codex data
- Optional: an OpenCode Go browser session cookie for live OpenCode Go data

### Steps

```bash
git clone https://github.com/int3rrobang/codex-dial.git
cd codex-dial
npm install

# Build the frontend only
npm run build

# Run the complete desktop app in development mode with hot reload
npm run tauri dev

# Build production Windows installers
npm run tauri build
```

Production installers are written under:

```text
src-tauri/target/release/bundle/nsis/
src-tauri/target/release/bundle/msi/
```

The Codex and OpenCode Go integrations are part of the Rust backend and are included automatically in both development and production Tauri builds. No separate backend service is required.

## Tech stack

| Layer | Technology |
|-------|-----------|
| Backend | Rust (Tauri 2) |
| Frontend | Svelte 5 + TypeScript |
| Charts | Chart.js |
| Tray icon | Canvas → Tauri Image API (dynamic, no static assets) |
| Persistence | JSON files in `%APPDATA%\com.github.thrr87.CodexLimits\History\` |
| IPC | Tauri commands (async, tokio) |

## How it polls

- **On launch** — immediate refresh
- **Every 10 minutes** — background timer
- **On wake from sleep** — 30s heartbeat detects time gaps
- **Manual** — tray menu or refresh button

## Credits

- Original macOS app: [thrr87/codex-limits](https://github.com/thrr87/codex-limits)
- Codex Dial is an independent, unofficial project. Not affiliated with or endorsed by OpenAI.

## License

[MIT](LICENSE)
