<script lang="ts">
  import { onMount } from 'svelte';
  import { getVlcGuidance } from '../lib/api';
  import type { VlcGuidance } from '../lib/types';

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
  <div class="guidance-card glass-panel" role="document">

    <div class="card-header">
      <div>
        <span class="badge badge-accent">PLAYER SETUP</span>
        <h2 class="card-title">VLC Media Player Required</h2>
      </div>
      <button class="close-btn" on:click={onClose}>✕</button>
    </div>

    <p class="card-desc">
      Flix streams original quality video directly to VLC on your computer without transcoding or browser lag.
    </p>

    {#if guidance}
      {#if guidance.command}
        <div class="command-box">
          <div class="command-header">One-Click Install Command ({guidance.platform.toUpperCase()}):</div>
          <div class="command-row">
            <code class="command-text">{guidance.command}</code>
            <button class="copy-cmd-btn" on:click={() => copyCommand(guidance?.command || '')}>
              {copiedCommand ? '✓ Copied' : '📋 Copy'}
            </button>
          </div>
        </div>
      {/if}

      <div class="steps-section">
        <h4 class="steps-title">Installation Steps:</h4>
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
          🌐 Open Official VLC Download Page
        </a>
        <button class="done-btn" on:click={onClose}>
          I've Installed VLC
        </button>
      </div>
    {:else}
      <div class="loading-box">
        <span class="spinner"></span>
        <span>Loading installation steps...</span>
      </div>
    {/if}
  </div>
</div>

<style>
  .modal-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.8);
    backdrop-filter: blur(10px);
    z-index: 200;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 24px;
  }

  .guidance-card {
    max-width: 540px;
    width: 100%;
    padding: 32px;
    border-radius: var(--radius-lg);
    background: linear-gradient(135deg, rgba(23, 31, 48, 0.95), rgba(15, 20, 34, 0.98));
    border: 1px solid rgba(99, 102, 241, 0.4);
    box-shadow: 0 24px 60px rgba(0, 0, 0, 0.6);
  }

  .card-header {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    margin-bottom: 12px;
  }

  .card-title {
    font-size: 1.5rem;
    font-weight: 800;
    margin-top: 6px;
    color: var(--text-primary);
  }

  .close-btn {
    background: transparent;
    color: var(--text-muted);
    font-size: 1.25rem;
    padding: 4px;
  }

  .close-btn:hover {
    color: var(--text-primary);
  }

  .card-desc {
    color: var(--text-secondary);
    font-size: 0.95rem;
    margin-bottom: 24px;
    line-height: 1.5;
  }

  .command-box {
    background: rgba(8, 11, 17, 0.8);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    padding: 14px 18px;
    margin-bottom: 24px;
  }

  .command-header {
    font-size: 0.75rem;
    font-weight: 700;
    color: var(--text-muted);
    text-transform: uppercase;
    margin-bottom: 8px;
  }

  .command-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 12px;
  }

  .command-text {
    font-family: monospace;
    font-size: 0.9rem;
    color: #a5b4fc;
  }

  .copy-cmd-btn {
    padding: 6px 12px;
    border-radius: var(--radius-sm);
    background: rgba(99, 102, 241, 0.2);
    color: #fff;
    font-size: 0.8rem;
    font-weight: 600;
    border: 1px solid rgba(99, 102, 241, 0.4);
  }

  .copy-cmd-btn:hover {
    background: rgba(99, 102, 241, 0.35);
  }

  .steps-section {
    margin-bottom: 28px;
  }

  .steps-title {
    font-size: 0.9rem;
    font-weight: 700;
    color: var(--text-secondary);
    margin-bottom: 10px;
  }

  .steps-list {
    margin: 0;
    padding-left: 20px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .step-item {
    font-size: 0.9rem;
    color: var(--text-secondary);
    line-height: 1.4;
  }

  .modal-actions {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .download-link-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 12px 24px;
    border-radius: var(--radius-md);
    font-size: 0.95rem;
    font-weight: 600;
    text-decoration: none;
    background: var(--accent-primary);
    color: #fff;
    box-shadow: var(--accent-glow);
  }

  .download-link-btn:hover {
    background: var(--accent-primary-hover);
  }

  .done-btn {
    padding: 10px 20px;
    border-radius: var(--radius-md);
    background: rgba(255, 255, 255, 0.05);
    border: 1px solid var(--border-subtle);
    color: var(--text-secondary);
    font-size: 0.9rem;
  }

  .done-btn:hover {
    background: rgba(255, 255, 255, 0.1);
    color: var(--text-primary);
  }

  .loading-box {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 24px 0;
    color: var(--text-muted);
  }

  .spinner {
    display: inline-block;
    width: 16px;
    height: 16px;
    border: 2px solid rgba(255, 255, 255, 0.3);
    border-top-color: #fff;
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }

  @keyframes spin {
    to { transform: rotate(360deg); }
  }
</style>
