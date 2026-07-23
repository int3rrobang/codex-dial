<script lang="ts">
  import type { UiState } from "./types";
  import { view, setSafetyBuffer, setLaunchAtLogin, chooseSyncFolder, stopSync } from "./store";

  let { data }: { data: UiState } = $props();
</script>

<div class="settings">
  <div class="settings-header">
    <button onclick={() => view.set("dashboard")}>&larr; Back</button>
    <h2>Settings</h2>
  </div>

  <div class="setting-group">
    <label>
      Safety buffer: {Math.round(data.safety_buffer)}%
      <input
        type="range"
        min="1"
        max="10"
        step="1"
        value={data.safety_buffer}
        oninput={(e) => setSafetyBuffer(Number(e.currentTarget.value))}
      />
    </label>
  </div>

  <div class="setting-group">
    <label class="toggle-label">
      <input
        type="checkbox"
        checked={data.launch_at_login}
        onchange={(e) => setLaunchAtLogin(e.currentTarget.checked)}
      />
      Launch at login
    </label>
  </div>

  <div class="setting-group">
    <div class="section-title">History sync</div>
    <p class="hint">Keep usage history in a folder synced across your PCs (OneDrive, Dropbox, etc.).</p>

    {#if data.sync_folder_name}
      <div class="sync-info">
        <span>Folder: <strong>{data.sync_folder_name}</strong></span>
        <button onclick={() => stopSync()}>Stop Syncing</button>
      </div>
    {:else}
      <button class="choose-btn" onclick={() => chooseSyncFolder()}>Choose Folder...</button>
    {/if}

    <p class="hint">Use this folder only on PCs signed in to the same Codex account.</p>

    {#if data.sync_error_message}
      <p class="error">&#9888; {data.sync_error_message}</p>
    {/if}
  </div>
</div>

<style>
  .settings { display: flex; flex-direction: column; gap: 16px; }
  .settings-header { display: flex; align-items: center; gap: 12px; }
  .settings-header h2 { font-size: 15px; font-weight: 600; }
  .setting-group { display: flex; flex-direction: column; gap: 8px; }
  .section-title { font-size: 12px; font-weight: 600; color: var(--text-secondary); text-transform: uppercase; letter-spacing: 0.5px; }
  .hint { font-size: 11px; color: var(--text-secondary); }
  .error { font-size: 11px; color: var(--accent-red); }
  .toggle-label { display: flex; align-items: center; gap: 8px; cursor: pointer; }
  .sync-info { display: flex; align-items: center; gap: 12px; }
  .choose-btn { background: var(--surface); padding: 6px 12px; border-radius: 6px; align-self: flex-start; }
  input[type="range"] { width: 100%; margin-top: 4px; accent-color: var(--accent-blue); }
  input[type="checkbox"] { accent-color: var(--accent-blue); }
</style>
