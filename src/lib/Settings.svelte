<script lang="ts">
  import type { UiState } from "./types";
  import {
    chooseSyncFolder,
    setCodexEnabled,
    setLaunchAtLogin,
    setOpenCodeApiKey,
    setOpenCodeGoEnabled,
    setResetNotificationsEnabled,
    setSafetyBuffer,
    stopSync,
    view,
  } from "./store";
  import FluentIcon from "./FluentIcon.svelte";
  import ToggleSwitch from "./ToggleSwitch.svelte";

  let { data }: { data: UiState } = $props();

  let apiKeyInput = $state("");
  let isSavingApiKey = $state(false);
  let savedMsg = $state("");
  let saveError = $state("");

  function inputChecked(event: Event): boolean {
    return (event.currentTarget as HTMLInputElement).checked;
  }

  function clearSaveStatus() {
    savedMsg = "";
    saveError = "";
  }

  async function saveOpenCodeApiKey() {
    clearSaveStatus();
    isSavingApiKey = true;

    try {
      await setOpenCodeApiKey(apiKeyInput.trim() || null);
      apiKeyInput = "";
      savedMsg = "Saved.";
    } catch {
      saveError = "Could not save the OpenCode API key. Check the key and try again.";
    } finally {
      isSavingApiKey = false;
    }
  }
</script>

<div class="settings">
  <header class="settings-header">
    <button
      class="subtle-button back-button"
      aria-label="Back to dashboard"
      title="Back to dashboard"
      onclick={() => view.set("dashboard")}
    >
      <FluentIcon name="back" size={16} />
      <span>Back</span>
    </button>
    <div class="header-copy">
      <h2>Settings</h2>
      <p>Preferences for monitoring and syncing usage.</p>
    </div>
  </header>

  <section class="setting-group" aria-labelledby="general-settings-title">
    <div class="section-heading">
      <h3 id="general-settings-title">General</h3>
      <p>Set pacing and startup behavior for Codex Dial.</p>
    </div>

    <div class="setting-row safety-row">
      <div class="setting-copy">
        <div class="setting-label-row">
          <label for="safety-buffer">Safety buffer</label>
          <output for="safety-buffer">{Math.round(data.safety_buffer)}%</output>
        </div>
        <p class="setting-description">Keep this much usage in reserve before a reset.</p>
      </div>
      <div class="safety-control">
        <input
          id="safety-buffer"
          type="range"
          min="1"
          max="10"
          step="1"
          value={data.safety_buffer}
          aria-label="Safety buffer percentage"
          oninput={(event) => setSafetyBuffer(Number(event.currentTarget.value))}
        />
        <div class="range-scale" aria-hidden="true">
          <span>1%</span>
          <span>10%</span>
        </div>
      </div>
    </div>


    <div class="setting-row">
      <div class="setting-copy">
        <span class="setting-label">Launch at login</span>
        <p class="setting-description">Start Codex Dial when you sign in to Windows.</p>
      </div>
      <ToggleSwitch
        checked={data.launch_at_login}
        label="Launch at login"
        showLabel={false}
        onchange={(event) => setLaunchAtLogin(inputChecked(event))}
      />
    </div>

    <div class="setting-row">
      <div class="setting-copy">
        <span class="setting-label">Banked reset notifications</span>
        <p class="setting-description">Notify me when the next banked reset expires in under 12 hours and again under 6 hours.</p>
      </div>
      <ToggleSwitch
        checked={data.reset_notifications_enabled}
        label="Banked reset notifications"
        showLabel={false}
        onchange={(event) => setResetNotificationsEnabled(inputChecked(event))}
      />
    </div>
  </section>

  <section class="setting-group" aria-labelledby="usage-backends-title">
    <div class="section-heading">
      <h3 id="usage-backends-title">Usage backends</h3>
      <p>Enable either backend, or both to compare their usage. You can leave both disabled until you are ready to configure a provider.</p>
    </div>

    <div class="setting-list">
      <div class="setting-row">
        <div class="setting-copy">
          <span class="setting-label">Codex CLI</span>
          <p class="setting-description">Monitor usage from your local Codex account.</p>
        </div>
        <ToggleSwitch
          checked={data.codex_enabled}
          label="Codex CLI"
          showLabel={false}
          onchange={(event) => setCodexEnabled(inputChecked(event))}
        />
      </div>
      <div class="setting-row">
        <div class="setting-copy">
          <span class="setting-label">OpenCode Go</span>
          <p class="setting-description">Monitor usage from your OpenCode Go account.</p>
        </div>
        <ToggleSwitch
          checked={data.opencode_go_enabled}
          label="OpenCode Go"
          showLabel={false}
          onchange={(event) => setOpenCodeGoEnabled(inputChecked(event))}
        />
      </div>
    </div>
  </section>

  <section class="setting-group" aria-labelledby="history-sync-title">
    <div class="section-heading">
      <h3 id="history-sync-title">History sync</h3>
      <p>Keep usage history in a folder synced across your PCs (OneDrive, Dropbox, etc.).</p>
    </div>

    {#if data.sync_folder_name}
      <div class="sync-row">
        <div class="setting-copy">
          <span class="setting-label">Current folder</span>
          <strong class="folder-name">{data.sync_folder_name}</strong>
        </div>
        <button class="subtle-button" onclick={() => stopSync()}>Stop Syncing</button>
      </div>
    {:else}
      <button class="standard-button choose-button" onclick={() => chooseSyncFolder()}>
        <span>Choose Folder...</span>
        <FluentIcon name="chevron-right" size={14} />
      </button>
    {/if}

    <p class="setting-description sync-note">Use this folder only on PCs signed in to the same account.</p>

    {#if data.sync_error_message}
      <div class="info-bar error-bar" role="alert">
        <FluentIcon name="warning" size={14} />
        <span>{data.sync_error_message}</span>
      </div>
    {/if}
  </section>

  <section class="setting-group" aria-labelledby="opencode-setup-title">
    <div class="section-heading">
      <h3 id="opencode-setup-title">OpenCode Go setup</h3>
    </div>

    {#if data.opencode_go_enabled}
      <p id="opencode-setup-description" class="setting-description setup-description">
        Create an API key in OpenCode Zen, then enter it here to read your usage windows.
      </p>

      <div class="field-row">
        <div class="field-copy">
          <label class="setting-label" for="opencode-api-key">OpenCode API key</label>
          <span id="opencode-api-key-hint" class="field-hint">Not displayed after saving.</span>
        </div>
        <div class="field-control">
          <input
            id="opencode-api-key"
            class="fluent-field"
            type="password"
            placeholder="Enter API key"
            aria-describedby="opencode-setup-description opencode-api-key-hint"
            autocomplete="off"
            spellcheck="false"
            autocapitalize="off"
            bind:value={apiKeyInput}
            oninput={clearSaveStatus}
            disabled={isSavingApiKey}
          />
          <button
            class="accent-button save-button"
            type="button"
            onclick={() => void saveOpenCodeApiKey()}
            disabled={isSavingApiKey}
          >
            {isSavingApiKey ? "Saving…" : "Save"}
          </button>
        </div>
      </div>

      {#if savedMsg}
        <div class="info-bar success-bar" role="status" aria-live="polite">
          <span>{savedMsg}</span>
        </div>
      {/if}
      {#if saveError}
        <div class="info-bar error-bar" role="alert" aria-live="assertive">
          <FluentIcon name="warning" size={14} />
          <span>{saveError}</span>
        </div>
      {/if}
      {#if data.opencode_go_error}
        <div class="info-bar error-bar" role="alert">
          <FluentIcon name="warning" size={14} />
          <span>{data.opencode_go_error}</span>
        </div>
      {/if}
    {/if}
  </section>
</div>

<style>
  .settings {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    min-height: 100%;
    padding: var(--space-1) var(--space-2) var(--space-3);
  }

  .settings-header {
    display: flex;
    align-items: center;
    gap: 8px;
    min-height: 36px;
    padding-inline: 2px;
  }

  .back-button {
    display: inline-flex;
    align-items: center;
    gap: 5px;
  }

  .header-copy {
    min-width: 0;
  }

  .header-copy h2 {
    margin: 0;
    font-size: 15px;
    font-weight: 600;
    line-height: 1.2;
    color: var(--text-primary);
  }

  .header-copy p {
    margin: 2px 0 0;
    font-size: 11px;
    color: var(--text-secondary);
  }

  .setting-group {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    padding: var(--space-2) var(--space-1);
  }

  .section-heading {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    padding-inline-start: var(--space-2);
    border-inline-start: 2px solid var(--accent-default);
  }

  .section-heading h3 {
    margin: 0;
    font-size: 12px;
    font-weight: 600;
    line-height: 1.25;
    color: var(--text-primary);
  }

  .section-heading p,
  .setting-description,
  .field-hint {
    margin: 0;
    font-size: 11px;
    line-height: 1.35;
    color: var(--text-secondary);
  }


  .setting-row,
  .sync-row,
  .field-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    min-width: 0;
  }

  .setting-copy,
  .field-copy {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }

  .setting-label,
  .setting-label-row label {
    font-size: 12px;
    font-weight: 500;
    line-height: 1.25;
    color: var(--text-primary);
  }

  .setting-label-row {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 8px;
  }

  .setting-label-row output {
    flex-shrink: 0;
    font-size: 12px;
    font-weight: 600;
    color: var(--accent-default);
    font-variant-numeric: tabular-nums;
  }


  .safety-row {
    align-items: flex-start;
  }

  .safety-control {
    display: flex;
    flex: 0 0 122px;
    flex-direction: column;
    gap: 3px;
    padding-top: 2px;
  }

  input[type="range"] {
    width: 100%;
    height: 16px;
    margin: 0;
    accent-color: var(--accent-default);
    cursor: pointer;
  }

  .range-scale {
    display: flex;
    justify-content: space-between;
    font-size: 10px;
    color: var(--text-secondary);
  }

  .setting-list {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }

  .sync-row {
    align-items: flex-start;
  }

  .folder-name {
    overflow: hidden;
    max-width: 190px;
    font-size: 11px;
    font-weight: 500;
    line-height: 1.35;
    color: var(--text-primary);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .choose-button {
    align-self: flex-start;
  }

  .sync-note {
    margin-top: -2px;
  }

  .info-bar {
    display: flex;
    align-items: flex-start;
    gap: 7px;
    padding: 7px 8px;
    border: 1px solid var(--card-stroke);
    border-radius: var(--overlay-radius);
    font-size: 11px;
    line-height: 1.35;
  }

  .error-bar {
    color: var(--system-critical);
    background: var(--system-background-critical);
  }

  .success-bar {
    color: var(--system-success);
    background: var(--subtle-fill-tertiary);
  }

  .setup-description {
    margin-top: -2px;
  }

  .field-row {
    align-items: flex-start;
  }

  .field-copy {
    flex: 0 1 94px;
    padding-top: 7px;
  }

  .field-control {
    display: flex;
    flex: 1 1 210px;
    align-items: center;
    gap: var(--space-1);
    min-width: 0;
  }

  .fluent-field {
    width: 100%;
    min-width: 0;
    padding: 6px var(--space-2) 5px;
    border: 1px solid var(--control-stroke-default);
    border-bottom: 2px solid var(--control-strong-stroke-default);
    border-radius: var(--overlay-radius);
    background: var(--control-fill-input-active);
    color: var(--text-primary);
    caret-color: var(--accent-default);
    font: inherit;
    font-size: 11px;
    outline: none;
    transition: border-color var(--duration-fast) ease-out, box-shadow var(--duration-fast) ease-out;
  }

  .fluent-field::placeholder {
    color: var(--text-secondary);
    opacity: 1;
  }

  .fluent-field:hover {
    border-color: var(--control-strong-stroke-default);
  }

  .fluent-field:focus {
    border-color: var(--accent-default);
    border-bottom-color: var(--accent-default);
    box-shadow: 0 1px 0 var(--accent-default);
  }

  .save-button {
    flex: 0 0 auto;
    white-space: nowrap;
  }

  :is(button, input):focus-visible {
    outline: 2px solid var(--focus-stroke);
    outline-offset: 2px;
  }

  @media (max-width: 340px) {
    .setting-row,
    .sync-row,
    .field-row {
      align-items: flex-start;
      flex-wrap: wrap;
    }

    .setting-copy,
    .field-copy,
    .field-control {
      flex-basis: 100%;
    }

    .safety-control {
      flex-basis: 100%;
      padding-top: 0;
    }

    .field-copy {
      padding-top: 0;
    }

    .field-control {
      flex-basis: 100%;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .fluent-field {
      transition: none;
    }
  }

  .fluent-field:disabled,
  input[type="range"]:disabled {
    color: var(--text-disabled);
    cursor: default;
  }

  .fluent-field:disabled {
    border-color: var(--control-stroke-default);
    border-bottom-color: var(--control-stroke-default);
    background: var(--control-fill-tertiary);
  }

  input[type="range"]:disabled {
    opacity: 0.65;
  }

  @media (forced-colors: active) {
    .section-heading {
      border-inline-start-color: Highlight;
    }

    .setting-label-row output {
      color: Highlight;
    }

    .fluent-field {
      forced-color-adjust: auto;
      border-color: ButtonText;
      border-bottom-color: ButtonText;
      background: Field;
      color: FieldText;
    }

    .fluent-field:disabled {
      border-color: GrayText;
      border-bottom-color: GrayText;
      background: Canvas;
      color: GrayText;
    }

    .fluent-field::placeholder,
    .field-hint,
    .section-heading p,
    .setting-description,
    .range-scale,
    .header-copy p {
      color: GrayText;
    }

    .fluent-field:focus {
      border-color: Highlight;
      border-bottom-color: Highlight;
      box-shadow: 0 0 0 1px Highlight;
    }

    input[type="range"] {
      forced-color-adjust: auto;
      accent-color: Highlight;
    }

    input[type="range"]:disabled {
      opacity: 1;
      accent-color: GrayText;
    }

    .info-bar {
      border-color: CanvasText;
    }

    .error-bar,
    .success-bar {
      color: CanvasText;
      background: Canvas;
    }

    .standard-button {
      border-color: ButtonText;
      background: ButtonFace;
      color: ButtonText;
    }

    .accent-button {
      border-color: Highlight;
      background: Highlight;
      color: HighlightText;
    }

    .standard-button:hover,
    .standard-button:active {
      background: ButtonFace;
      color: ButtonText;
    }

    .accent-button:hover,
    .accent-button:active {
      border-color: Highlight;
      background: Highlight;
      color: HighlightText;
    }

    .subtle-button {
      color: ButtonText;
    }
  }
</style>
