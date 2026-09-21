<script lang="ts">
  import { AVATAR_COLORS, AVATAR_ICONS, avatarKeyOf, parseAvatarKey } from '../lib/avatars';
  import { Check } from 'lucide-svelte';

  export let selectedKey: string = 'amber';
  export let onSelect: (key: string) => void;

  $: parsed = parseAvatarKey(selectedKey);

  function chooseColor(colorKey: string) {
    onSelect(avatarKeyOf(colorKey, parsed.iconKey));
  }

  function chooseIcon(iconKey: string) {
    // Picking the icon already showing turns it off, back to plain initials.
    onSelect(avatarKeyOf(parsed.colorKey, parsed.iconKey === iconKey ? null : iconKey));
  }
</script>

<div class="avatar-picker">
  <div class="preset-label">Color</div>
  <div class="color-grid">
    {#each AVATAR_COLORS as preset (preset.key)}
      <button
        type="button"
        class="preset-btn {parsed.colorKey === preset.key ? 'active' : ''}"
        style="--swatch-color: {preset.color}"
        on:click={() => chooseColor(preset.key)}
        title={preset.label}
        aria-label={preset.label}
        aria-pressed={parsed.colorKey === preset.key}
      >
        <span class="swatch"></span>
      </button>
    {/each}
  </div>

  <div class="preset-label">Icon (optional)</div>
  <div class="icon-grid" style="--swatch-color: {parsed.color}">
    {#each AVATAR_ICONS as preset (preset.key)}
      <button
        type="button"
        class="icon-btn {parsed.iconKey === preset.key ? 'active' : ''}"
        on:click={() => chooseIcon(preset.key)}
        title={preset.label}
        aria-label={preset.label}
        aria-pressed={parsed.iconKey === preset.key}
      >
        <svelte:component this={preset.icon} size={17} />
        {#if parsed.iconKey === preset.key}
          <span class="icon-check"><Check size={10} /></span>
        {/if}
      </button>
    {/each}
  </div>
</div>

<style>
  .avatar-picker {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .preset-label {
    font-size: 0.76rem;
    font-weight: 600;
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.06em;
  }

  .preset-label:not(:first-child) {
    margin-top: 6px;
  }

  .color-grid {
    display: flex;
    gap: 10px;
    flex-wrap: wrap;
  }

  .preset-btn {
    width: 36px;
    height: 36px;
    border-radius: var(--radius-sm);
    background: var(--bg-surface-elevated);
    border: 1px solid var(--border-subtle);
    display: flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
    transition: all var(--transition-fast);
  }

  .preset-btn:hover {
    border-color: var(--border-muted);
    transform: scale(1.05);
  }

  .preset-btn.active {
    border-color: var(--accent-primary);
    box-shadow: 0 0 0 2px var(--accent-muted);
  }

  .swatch {
    width: 18px;
    height: 18px;
    border-radius: 4px;
    background-color: var(--swatch-color);
  }

  .icon-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(36px, 1fr));
    gap: 8px;
    max-width: 340px;
  }

  .icon-btn {
    position: relative;
    width: 36px;
    height: 36px;
    border-radius: var(--radius-sm);
    background: var(--bg-surface-elevated);
    border: 1px solid var(--border-subtle);
    color: var(--text-secondary);
    display: flex;
    align-items: center;
    justify-content: center;
    transition: all var(--transition-fast);
  }

  .icon-btn:hover {
    color: var(--text-primary);
    border-color: var(--border-muted);
  }

  .icon-btn.active {
    color: var(--swatch-color);
    border-color: var(--swatch-color);
    background: color-mix(in srgb, var(--swatch-color) 16%, var(--bg-surface-elevated));
  }

  .icon-check {
    position: absolute;
    top: -4px;
    right: -4px;
    width: 13px;
    height: 13px;
    border-radius: 50%;
    background: var(--swatch-color);
    color: #07090e;
    display: flex;
    align-items: center;
    justify-content: center;
  }
</style>
