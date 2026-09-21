<script lang="ts">
  import { onMount } from 'svelte';
  import { getSettings, quitApp } from '../lib/api';
  import type { SettingsResponse } from '../lib/types';
  import { RefreshCw, Folder, Tv, MessageSquare, Power, AlertTriangle, CheckCircle, ExternalLink } from 'lucide-svelte';
  import ConfirmationDialog from './ConfirmationDialog.svelte';
  import { confirmationFor } from '../lib/confirmation';

  let settings: SettingsResponse | null = null;
  let isLoading = false;
  let errorMsg = '';
  let isQuitting = false;
  let quitSuccess = false;
  let confirmQuit = false;

  async function loadSettings() {
    isLoading = true;
    errorMsg = '';
    try {
      settings = await getSettings();
    } catch (e: any) {
      errorMsg = e.message || 'Failed to load settings';
    } finally {
      isLoading = false;
    }
  }

  async function handleQuit() {
    isQuitting = true;
    try {
      await quitApp();
      quitSuccess = true;
    } catch (e: any) {
      errorMsg = `Failed to quit: ${e.message}`;
    } finally {
      isQuitting = false;
    }
  }

  function confirmQuitApp() {
    confirmQuit = false;
    void handleQuit();
  }

  onMount(() => {
    loadSettings();
  });
</script>

<div class="settings-view">
  <div class="settings-header">
    <div>
      <h1 class="page-title">Settings</h1>
      <p class="page-subtitle">Inspect local storage paths, detected media players, and system controls.</p>
    </div>
    <button class="refresh-btn" on:click={loadSettings} disabled={isLoading}>
      <RefreshCw size={14} class={isLoading ? 'spinning' : ''} /> Refresh
    </button>
  </div>

  {#if quitSuccess}
    <div class="quit-card panel">
      <CheckCircle size={40} class="status-success" />
      <h2 class="quit-title">Flix Desktop Stopped</h2>
      <p class="quit-desc">The local service and media players have shut down safely. You may now close this tab.</p>
    </div>
  {:else if errorMsg}
    <div class="error-banner">
      <AlertTriangle size={16} />
      <span>{errorMsg}</span>
    </div>
  {:else if settings}
    <div class="settings-grid">
      <!-- Storage Paths -->
      <div class="settings-card panel">
        <h3 class="card-heading"><Folder size={18} /> Storage Directories</h3>
        <div class="info-group">
          <div class="info-item">
            <span class="info-title">Download Directory</span>
            <code class="path-value">{settings.download_dir}</code>
          </div>
          <div class="info-item">
            <span class="info-title">Application Data Directory</span>
            <code class="path-value">{settings.data_dir}</code>
          </div>
        </div>
      </div>

      <!-- Media Players -->
      <div class="settings-card panel">
        <h3 class="card-heading"><Tv size={18} /> Detected Media Players</h3>
        {#if settings.players.length > 0}
          <div class="players-list">
            {#each settings.players as player}
              <div class="player-item">
                <div class="player-name-row">
                  <span class="badge badge-4k">{player.name}</span>
                  <span class="active-tag">Detected</span>
                </div>
                <code class="path-value">{player.path}</code>
              </div>
            {/each}
          </div>
        {:else}
          <div class="warning-box">
            <AlertTriangle size={16} />
            <div>
              <p>No supported media player found on your system.</p>
              <p class="warning-sub">Flix requires mpv or VLC to stream torrents directly.</p>
              <div class="install-links">
                <a href="https://mpv.io/installation/" target="_blank" rel="noreferrer" class="install-link">
                  Install mpv <ExternalLink size={12} />
                </a>
                <a href="https://www.videolan.org/vlc/" target="_blank" rel="noreferrer" class="install-link">
                  Install VLC <ExternalLink size={12} />
                </a>
              </div>
            </div>
          </div>
        {/if}
      </div>

      <!-- Subtitle Integration -->
      <div class="settings-card panel">
        <h3 class="card-heading"><MessageSquare size={18} /> Subtitle Providers</h3>
        <div class="provider-row">
          <div class="provider-info">
            <div class="provider-title">OpenSubtitles Gateway</div>
            <div class="provider-desc">Automatic English subtitle resolution via Cloudflare gateway.</div>
          </div>
          {#if settings.opensubtitles_configured}
            <span class="badge badge-4k">Active</span>
          {:else}
            <span class="badge">Guest Mode</span>
          {/if}
        </div>
      </div>

      <!-- Application Lifecycle -->
      <div class="settings-card panel quit-section">
        <h3 class="card-heading"><Power size={18} /> Application Lifecycle</h3>
        <p class="quit-info">
          Stop active torrent sessions, release local network ports, and exit the Flix desktop process.
        </p>
        <button class="quit-btn" on:click={() => (confirmQuit = true)} disabled={isQuitting}>
          <Power size={15} /> Quit Flix Desktop
        </button>
      </div>
    </div>
  {:else}
    <div class="loading-box">
      <span>Loading settings...</span>
    </div>
  {/if}
</div>

{#if confirmQuit}
  {@const copy = confirmationFor('quit')}
  <ConfirmationDialog {...copy} onCancel={() => (confirmQuit = false)} onConfirm={confirmQuitApp} />
{/if}

<style>
  .settings-view {
    max-width: 960px;
    margin: 0 auto;
    padding: 32px 24px 80px;
  }

  .settings-header {
    display: flex;
    justify-content: space-between;
    align-items: flex-end;
    margin-bottom: 24px;
  }

  .page-title {
    font-size: 1.8rem;
    font-weight: 700;
    margin-bottom: 4px;
  }

  .page-subtitle {
    color: var(--text-secondary);
    font-size: 0.9rem;
  }

  .refresh-btn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    background: var(--bg-surface);
    border: 1px solid var(--border-subtle);
    color: var(--text-secondary);
    padding: 6px 14px;
    border-radius: var(--radius-sm);
    font-size: 0.85rem;
  }

  .refresh-btn:hover:not(:disabled) {
    color: var(--text-primary);
    background: var(--bg-surface-active);
  }

  .settings-grid {
    display: flex;
    flex-direction: column;
    gap: 16px;
  }

  .settings-card {
    padding: 20px 24px;
  }

  .card-heading {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 1rem;
    font-weight: 600;
    margin-bottom: 16px;
  }

  .info-group {
    display: flex;
    flex-direction: column;
    gap: 14px;
  }

  .info-item {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .info-title {
    font-size: 0.75rem;
    color: var(--text-muted);
    font-weight: 600;
    text-transform: uppercase;
  }

  .path-value {
    font-family: monospace;
    font-size: 0.8rem;
    color: #cbd5e1;
    background: var(--bg-secondary);
    padding: 6px 12px;
    border-radius: var(--radius-sm);
    border: 1px solid var(--border-subtle);
    word-break: break-all;
  }

  .players-list {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .player-item {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .player-name-row {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .active-tag {
    font-size: 0.75rem;
    color: var(--status-green);
  }

  .warning-box {
    display: flex;
    gap: 12px;
    padding: 12px 16px;
    border-radius: var(--radius-sm);
    background: rgba(245, 158, 11, 0.12);
    border: 1px solid rgba(245, 158, 11, 0.25);
    color: #fcd34d;
    font-size: 0.85rem;
  }

  .warning-sub {
    color: var(--text-muted);
    font-size: 0.8rem;
    margin-top: 2px;
  }

  .install-links {
    display: flex;
    gap: 12px;
    margin-top: 8px;
  }

  .install-link {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    color: #93c5fd;
    font-size: 0.8rem;
    text-decoration: underline;
  }

  .provider-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }

  .provider-title {
    font-weight: 600;
    font-size: 0.9rem;
  }

  .provider-desc {
    font-size: 0.8rem;
    color: var(--text-muted);
  }

  .quit-section {
    border-color: rgba(239, 68, 68, 0.2);
  }

  .quit-info {
    font-size: 0.85rem;
    color: var(--text-secondary);
    margin-bottom: 14px;
  }

  .quit-btn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    background: rgba(239, 68, 68, 0.15);
    border: 1px solid rgba(239, 68, 68, 0.3);
    color: #fca5a5;
    padding: 8px 16px;
    border-radius: var(--radius-sm);
    font-weight: 600;
    font-size: 0.85rem;
  }

  .quit-btn:hover:not(:disabled) {
    background: rgba(239, 68, 68, 0.25);
  }

  .quit-card {
    text-align: center;
    padding: 48px 24px;
  }

  .quit-title {
    font-size: 1.3rem;
    font-weight: 700;
    margin-top: 10px;
    margin-bottom: 6px;
  }

  .quit-desc {
    color: var(--text-secondary);
    max-width: 440px;
    margin: 0 auto;
    font-size: 0.85rem;
  }

  :global(.status-success) {
    color: var(--status-green);
  }

  .error-banner {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 12px 16px;
    background: rgba(239, 68, 68, 0.15);
    border: 1px solid rgba(239, 68, 68, 0.3);
    border-radius: var(--radius-sm);
    color: #fca5a5;
    margin-bottom: 16px;
  }

  .loading-box {
    padding: 48px;
    text-align: center;
    color: var(--text-secondary);
  }

  :global(.spinning) {
    animation: spin 1s linear infinite;
  }

  @keyframes spin {
    to { transform: rotate(360deg); }
  }
</style>
