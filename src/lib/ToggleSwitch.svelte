<script lang="ts">
  type ToggleChangeEvent = Event & { currentTarget: HTMLInputElement };

  type Props = {
    checked: boolean;
    disabled?: boolean;
    label: string;
    onchange: (event: ToggleChangeEvent) => void;
    showLabel?: boolean;
  };

  let {
    checked,
    disabled = false,
    label,
    onchange,
    showLabel = true,
  }: Props = $props();
</script>

<label class="toggle" class:is-disabled={disabled}>
  <input
    class="toggle-input"
    type="checkbox"
    {checked}
    {disabled}
    aria-label={label}
    onchange={onchange}
  />
  <span class="toggle-track" aria-hidden="true">
    <span class="toggle-thumb"></span>
  </span>
  <span class:visually-hidden={!showLabel} class="toggle-label">{label}</span>
</label>

<style>
  .toggle {
    position: relative;
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    min-height: 20px;
    color: var(--text-primary);
    cursor: pointer;
    flex-shrink: 0;
  }

  .toggle.is-disabled {
    color: var(--text-disabled);
    cursor: default;
  }

  .toggle-input {
    position: absolute;
    width: 1px;
    height: 1px;
    margin: -1px;
    overflow: hidden;
    clip: rect(0 0 0 0);
    clip-path: inset(50%);
    white-space: nowrap;
    accent-color: var(--accent-default);
  }

  .toggle-track {
    position: relative;
    display: inline-flex;
    align-items: center;
    width: 40px;
    height: 20px;
    flex: 0 0 40px;
    border: 1px solid var(--control-strong-stroke-default);
    border-radius: 999px;
    background: var(--control-fill-secondary);
    forced-color-adjust: none;
    transition: background-color var(--duration-fast) ease-out, border-color var(--duration-fast) ease-out;
  }

  .toggle:hover .toggle-track {
    border-color: var(--accent-default);
    background: var(--accent-subtle-fill);
  }


  .toggle-thumb {
    width: 16px;
    height: 16px;
    margin-inline-start: 1px;
    border-radius: 50%;
    background: var(--text-primary);
    box-shadow: 0 1px 2px color-mix(in srgb, var(--solid-background-base) 36%, transparent);
    transition: transform var(--duration-fast) ease-out, background-color var(--duration-fast) ease-out;
  }

  .toggle-input:checked + .toggle-track {
    border-color: var(--accent-default);
    background: var(--accent-default);
  }

  .toggle-input:checked + .toggle-track .toggle-thumb {
    transform: translateX(20px);
    background: var(--accent-text);
  }


  .toggle-input:disabled + .toggle-track {
    border-color: var(--control-stroke-default);
    background: var(--control-fill-tertiary);
  }

  .toggle-input:disabled + .toggle-track .toggle-thumb {
    background: var(--text-disabled);
  }

  .toggle-input:checked:disabled + .toggle-track {
    border-color: var(--accent-tertiary);
    background: var(--accent-tertiary);
  }

  .toggle-input:checked:disabled + .toggle-track .toggle-thumb {
    background: var(--solid-background-secondary);
  }

  .toggle-input:focus-visible + .toggle-track {
    outline: 2px solid var(--focus-stroke);
    outline-offset: 2px;
  }

  @media (forced-colors: active) {
    .toggle-track {
      border-color: ButtonText;
      background: Canvas;
    }

    .toggle:hover .toggle-track {
      border-color: ButtonText;
      background: Canvas;
    }

    .toggle-thumb {
      background: ButtonText;
      box-shadow: none;
    }

    .toggle-input:checked + .toggle-track {
      border-color: Highlight;
      background: Highlight;
    }

    .toggle-input:checked + .toggle-track .toggle-thumb {
      background: HighlightText;
    }

    .toggle-input:disabled + .toggle-track,
    .toggle-input:checked:disabled + .toggle-track {
      border-color: GrayText;
      background: Canvas;
    }

    .toggle-input:disabled + .toggle-track .toggle-thumb,
    .toggle-input:checked:disabled + .toggle-track .toggle-thumb {
      background: GrayText;
    }

    .toggle-input:focus-visible + .toggle-track {
      outline-color: Highlight;
    }

    .toggle.is-disabled {
      color: GrayText;
    }
  }

  .toggle-label {
    font-size: 12px;
    line-height: 1.25;
  }

  .visually-hidden {
    position: absolute;
    width: 1px;
    height: 1px;
    padding: 0;
    margin: -1px;
    overflow: hidden;
    clip: rect(0 0 0 0);
    clip-path: inset(50%);
    white-space: nowrap;
    border: 0;
  }

  @media (prefers-reduced-motion: reduce) {
    .toggle-track,
    .toggle-thumb {
      transition: none;
    }
  }
</style>
