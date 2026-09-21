<script lang="ts">
  import { onMount } from 'svelte';
  import { getVlcGuidance } from '../lib/api';
  import type { VlcGuidance } from '../lib/types';
  import { Tv, Copy, Check, ExternalLink, X } from 'lucide-svelte';

  export let onClose: () => void;

  let guidance: VlcGuidance | null = null;
  let copiedCommand = false;

  onMount(async () => {
    try {
      guidance = await getVlcGuidance();
    } catch {
      guidance = {
        platform: 'other',
        is_installed: false,
        download_url: 'https://www.videolan.org/vlc/',
        instructions: ['Download and install VLC from videolan.org/vlc'],
      };
    }
  });

  function copyCommand(cmd: string) {
    navigator.clipboard.writeText(cmd);
    copiedCommand = true;
    setTimeout(() => {
      copiedCommand = false;
    }, 2000);
  }
</script>

<div
  class="modal-backdrop"
  role="dialog"
  aria-modal="true"
  tabindex="-1"
  on:click={(e) => e.target === e.currentTarget && onClose()}
  on:keydown={(e) => e.key === 'Escape' && onClose()}
>
  <div class="guidance-card panel" role="document">
    <div class="card-header">
      <div>
        <span class="badge badge-recommended">Media Player</span>
        <h2 class="card-title">Media Player Setup</h2>
      </div>
      <button class="close-btn" type="button" aria-label="Close media player setup" on:click={onClose}><X size={18} /></button>
    </div>

    <p class="card-desc">
      Flix streams native video directly to mpv or VLC on your computer without transcoding.
    </p>

    {#if guidance}
      {#if guidance.command}
        <div class="command-box">
          <div class="command-header">Install Command ({guidance.platform.toUpperCase()}):</div>
          <div class="command-row">
            <code class="command-text">{guidance.command}</code>
            <button class="copy-cmd-btn" on:click={() => copyCommand(guidance?.command || '')}>
              {#if copiedCommand}
                <Check size={13} /> Copied
              {:else}
                <Copy size={13} /> Copy
              {/if}
            </button>
          </div>
        </div>
      {/if}

      <div class="steps-section">
        <h4 class="steps-title">Installation Instructions:</h4>
        <ol class="steps-list">
          {#each guidance.instructions as step}
            <li class="step-item">{step}</li>
          {/each}
        </ol>
      </div>

      <div class="modal-actions">
        <a
          href={guidance.download_url}
          target="_blank"
          rel="noopener noreferrer"
          class="download-link-btn primary"
        >
          <ExternalLink size={15} /> Open Official VLC Download Page
        </a>
        <button class="done-btn" on:click={onClose}>
          Dismiss
        </button>
      </div>
    {:else}
      <div class="loading-box">
        <span>Loading installation steps...</span>
      </div>
    {/if}
  </div>
</div>

<style>
  .modal-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.52);
    z-index: 200;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 24px;
  }

  .guidance-card {
    max-width: 520px;
    width: 100%;
    padding: 28px;
    border-radius: var(--radius-lg);
    background: var(--modal-surface);
    border: 1px solid var(--modal-border);
    box-shadow: var(--shadow-modal);
    animation: guidance-enter 240ms cubic-bezier(0.16, 1, 0.3, 1);
    max-height: calc(100dvh - 48px);
    overflow-y: auto;
  }

  .card-header {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    margin-bottom: 12px;
  }

  .card-title {
    font-size: 1.3rem;
    font-weight: 700;
    margin-top: 6px;
    color: var(--text-primary);
  }

  .close-btn {
    background: var(--control-fill);
    color: var(--text-muted);
    padding: 6px;
    border-radius: 50%;
  }

  .close-btn:hover {
    background: var(--control-fill-hover);
    color: var(--text-primary);
  }

  @keyframes guidance-enter {
    from { opacity: 0; transform: scale(0.96) translateY(10px); }
    to { opacity: 1; transform: scale(1) translateY(0); }
  }

  .card-desc {
    color: var(--text-secondary);
    font-size: 0.85rem;
    margin-bottom: 20px;
    line-height: 1.4;
  }

  .command-box {
    background: var(--bg-secondary);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    padding: 12px 16px;
    margin-bottom: 20px;
  }

  .command-header {
    font-size: 0.75rem;
    font-weight: 600;
    color: var(--text-muted);
    text-transform: uppercase;
    margin-bottom: 6px;
  }

  .command-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 10px;
  }

  .command-text {
    font-family: monospace;
    font-size: 0.85rem;
    color: #93c5fd;
  }

  .copy-cmd-btn {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 4px 10px;
    border-radius: var(--radius-sm);
    background: var(--bg-surface-active);
    color: var(--text-primary);
    font-size: 0.8rem;
    font-weight: 600;
  }

  .steps-section {
    margin-bottom: 24px;
  }

  .steps-title {
    font-size: 0.85rem;
    font-weight: 600;
    color: var(--text-secondary);
    margin-bottom: 8px;
  }

  .steps-list {
    margin: 0;
    padding-left: 20px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .step-item {
    font-size: 0.85rem;
    color: var(--text-secondary);
    line-height: 1.4;
  }

  .modal-actions {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .download-link-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    padding: 10px 20px;
    border-radius: var(--radius-sm);
    font-size: 0.9rem;
    font-weight: 600;
    text-decoration: none;
    background: var(--action-fill);
    color: #fff;
  }

  .download-link-btn:hover {
    background: var(--action-fill-hover);
  }

  .done-btn {
    padding: 8px 16px;
    border-radius: var(--radius-sm);
    background: var(--bg-secondary);
    border: 1px solid var(--border-subtle);
    color: var(--text-secondary);
    font-size: 0.85rem;
  }

  .done-btn:hover {
    color: var(--text-primary);
  }

  .loading-box {
    padding: 20px 0;
    text-align: center;
    color: var(--text-muted);
  }
</style>
