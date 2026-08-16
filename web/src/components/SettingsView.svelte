<script lang="ts">
  import { onMount } from 'svelte';
  import { getSettings, quitApp } from '../lib/api';
  import type { SettingsResponse } from '../lib/types';

  let settings: SettingsResponse | null = null;
  let isLoading = false;
  let errorMsg = '';
  let isQuitting = false;
  let quitSuccess = false;

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
    if (!confirm('Are you sure you want to stop Flix Desktop and quit the background server?')) {
      return;
    }
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

  onMount(() => {
    loadSettings();
  });
</script>

<div class="settings-view">
  <div class="settings-header">
    <div>
      <h1 class="page-title">Settings & Diagnostics</h1>
      <p class="page-subtitle">Inspect local directories, media players, and system status.</p>
    </div>
    <button class="refresh-btn" on:click={loadSettings} disabled={isLoading}>
      🔄 Refresh
    </button>
  </div>

  {#if quitSuccess}
    <div class="quit-card glass-panel">
      <div class="quit-icon">👋</div>
      <h2 class="quit-title">Flix Desktop Stopped</h2>
      <p class="quit-desc">The local service and media players have shut down safely. You may now close this tab.</p>
    </div>
  {:else if errorMsg}
    <div class="error-banner glass-panel">
      {errorMsg}
    </div>
  {:else if settings}
    <div class="settings-grid">
      <!-- Storage Paths -->
      <div class="settings-card glass-panel">
        <h3 class="card-heading">📁 Storage Directories</h3>
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
      <div class="settings-card glass-panel">
        <h3 class="card-heading">🎬 Detected Media Players</h3>
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
            <span>⚠️ No supported player found. Please install VLC or mpv.</span>
          </div>
        {/if}
      </div>

      <!-- OpenSubtitles Config -->
      <div class="settings-card glass-panel">
        <h3 class="card-heading">💬 Subtitle Providers</h3>
        <div class="provider-row">
          <div class="provider-info">
            <div class="provider-title">OpenSubtitles Integration</div>
            <div class="provider-desc">Fetches English subtitles for movies and series.</div>
          </div>
          {#if settings.opensubtitles_configured}
            <span class="badge badge-4k">Configured</span>
          {:else}
            <span class="badge badge-accent">Default / Guest</span>
          {/if}
        </div>
      </div>

      <!-- Application Lifecycle -->
      <div class="settings-card glass-panel quit-section">
        <h3 class="card-heading">🛑 Application Lifecycle</h3>
        <p class="quit-info">
          Stop active torrent playback sessions, release local network ports, and exit the Flix desktop process.
        </p>
        <button class="quit-btn" on:click={handleQuit} disabled={isQuitting}>
          {#if isQuitting}
            <span class="spinner"></span>
            <span>Shutting down...</span>
          {:else}
            <span>🛑 Quit Flix Desktop</span>
          {/if}
        </button>
      </div>
    </div>
  {:else}
    <div class="loading-box">
      <span class="spinner large"></span>
      <span>Loading settings...</span>
    </div>
  {/if}
</div>

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
    margin-bottom: 28px;
  }

  .page-title {
    font-size: 2.25rem;
    font-weight: 800;
    margin-bottom: 6px;
    background: var(--accent-gradient);
    -webkit-background-clip: text;
    background-clip: text;
    -webkit-text-fill-color: transparent;

  }

  .page-subtitle {
    color: var(--text-secondary);
    font-size: 0.95rem;
  }

  .refresh-btn {
    background: rgba(23, 31, 48, 0.6);
    border: 1px solid var(--border-subtle);
    color: var(--text-secondary);
    padding: 8px 16px;
    border-radius: var(--radius-sm);
    font-size: 0.85rem;
  }

  .refresh-btn:hover:not(:disabled) {
    color: var(--text-primary);
    background: rgba(255, 255, 255, 0.08);
  }

  .settings-grid {
    display: flex;
    flex-direction: column;
    gap: 20px;
  }

  .settings-card {
    padding: 24px 28px;
    border-radius: var(--radius-md);
  }

  .card-heading {
    font-size: 1.1rem;
    font-weight: 700;
    margin-bottom: 16px;
  }

  .info-group {
    display: flex;
    flex-direction: column;
    gap: 16px;
  }

  .info-item {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .info-title {
    font-size: 0.8rem;
    color: var(--text-muted);
    font-weight: 600;
    text-transform: uppercase;
  }

  .path-value {
    font-family: monospace;
    font-size: 0.85rem;
    color: #cbd5e1;
    background: rgba(8, 11, 17, 0.6);
    padding: 6px 12px;
    border-radius: 6px;
    border: 1px solid var(--border-subtle);
    word-break: break-all;
  }

  .players-list {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .player-item {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .player-name-row {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .active-tag {
    font-size: 0.75rem;
    color: #34d399;
  }

  .warning-box {
    padding: 12px 16px;
    border-radius: var(--radius-sm);
    background: rgba(245, 158, 11, 0.15);
    border: 1px solid rgba(245, 158, 11, 0.3);
    color: #fcd34d;
    font-size: 0.9rem;
  }

  .provider-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }

  .provider-title {
    font-weight: 600;
    font-size: 0.95rem;
  }

  .provider-desc {
    font-size: 0.85rem;
    color: var(--text-muted);
  }

  .quit-section {
    border-color: rgba(239, 68, 68, 0.25);
  }

  .quit-info {
    font-size: 0.9rem;
    color: var(--text-secondary);
    margin-bottom: 16px;
  }

  .quit-btn {
    background: rgba(239, 68, 68, 0.2);
    border: 1px solid rgba(239, 68, 68, 0.4);
    color: #fca5a5;
    padding: 10px 22px;
    border-radius: var(--radius-md);
    font-weight: 600;
    font-size: 0.9rem;
  }

  .quit-btn:hover:not(:disabled) {
    background: rgba(239, 68, 68, 0.3);
  }

  .quit-card {
    text-align: center;
    padding: 60px 24px;
    border-radius: var(--radius-lg);
  }

  .quit-icon {
    font-size: 3rem;
    margin-bottom: 12px;
  }

  .quit-title {
    font-size: 1.5rem;
    font-weight: 700;
    margin-bottom: 8px;
  }

  .quit-desc {
    color: var(--text-secondary);
    max-width: 440px;
    margin: 0 auto;
  }

  .loading-box {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 12px;
    padding: 60px;
    color: var(--text-secondary);
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

  .spinner.large {
    width: 28px;
    height: 28px;
  }

  @keyframes spin {
    to { transform: rotate(360deg); }
  }
</style>
