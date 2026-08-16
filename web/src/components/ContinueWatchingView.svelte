<script lang="ts">
  import { nextEpisode, stopPlayback } from '../lib/api';
  import type { PlaybackState } from '../lib/types';

  export let playbackState: PlaybackState;
  export let onNavigateToSearch: () => void;
  export let onStateChanged: () => void;

  let isAdvancing = false;
  let isStopping = false;
  let actionMessage = '';

  async function handleNext() {
    isAdvancing = true;
    actionMessage = 'Preparing next episode in background while current stream continues...';
    try {
      await nextEpisode();
      actionMessage = 'Advanced to next episode!';
      onStateChanged();
    } catch (e: any) {
      actionMessage = `Next episode preparation failed: ${e.message}`;
    } finally {
      isAdvancing = false;
    }
  }

  async function handleStop() {
    isStopping = true;
    actionMessage = 'Stopping player and cleaning up session...';
    try {
      await stopPlayback();
      actionMessage = 'Playback stopped and session cleaned.';
      onStateChanged();
    } catch (e: any) {
      actionMessage = `Stop failed: ${e.message}`;
    } finally {
      isStopping = false;
    }
  }

  function formatBytes(bytes: number): string {
    if (!bytes || bytes === 0) return '0 B';
    const k = 1024;
    const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + ' ' + sizes[i];
  }
</script>

<div class="continue-view">
  {#if playbackState.status === 'playing'}
    <div class="hero-player-card glass-panel">
      <div class="card-status-bar">
        <div class="status-indicator">
          <span class="live-pulse pulsing-indicator"></span>
          <span class="status-label">STREAMING TO LOCAL PLAYER</span>
        </div>
        <div class="quality-tags">
          <span class="badge {playbackState.quality === '4K' ? 'badge-4k' : 'badge-1080p'}">
            {playbackState.quality}
          </span>
          {#if playbackState.subtitle_url}
            <span class="badge badge-accent">Subtitles Loaded</span>
          {/if}
        </div>
      </div>

      <div class="media-details">
        <h1 class="media-title">{playbackState.title}</h1>
        <div class="meta-row">
          <span class="meta-item">💾 {formatBytes(playbackState.file_length)}</span>
          <span class="meta-item">🎮 {playbackState.player_path.split('/').pop() || 'Media Player'}</span>
          {#if playbackState.has_next}
            <span class="meta-item queue-tag">✨ Next Episode in Queue</span>
          {/if}
        </div>
      </div>

      <div class="stream-info-box">
        <div class="info-label">Local Stream Endpoint (Original Quality, No Transcoding)</div>
        <code class="stream-url">{playbackState.url}</code>
      </div>

      {#if actionMessage}
        <div class="action-alert">{actionMessage}</div>
      {/if}

      <div class="playback-controls">
        {#if playbackState.has_next}
          <button
            class="control-btn primary"
            on:click={handleNext}
            disabled={isAdvancing || isStopping}
          >
            {#if isAdvancing}
              <span class="spinner"></span>
              <span>Preparing Next Episode...</span>
            {:else}
              <span>⏭ Next Episode</span>
            {/if}
          </button>
        {/if}

        <button
          class="control-btn danger"
          on:click={handleStop}
          disabled={isStopping || isAdvancing}
        >
          {#if isStopping}
            <span class="spinner"></span>
            <span>Stopping...</span>
          {:else}
            <span>⏹ Stop & Clean Session</span>
          {/if}
        </button>
      </div>
    </div>
  {:else if playbackState.status === 'preparing'}
    <div class="state-card glass-panel">
      <span class="spinner large"></span>
      <h2 class="state-title">Preparing Stream & Subtitles</h2>
      <p class="state-sub">Warming torrent buffers and downloading English subtitles for: {playbackState.title}</p>
    </div>
  {:else}
    <div class="idle-card glass-panel">
      <div class="idle-icon">🍿</div>
      <h2 class="idle-title">No Active Stream</h2>
      <p class="idle-desc">
        Start watching a movie, series, or anime from the search catalog to control playback here.
      </p>
      <button class="jump-btn" on:click={onNavigateToSearch}>
        🔍 Browse Catalogs
      </button>
    </div>
  {/if}
</div>

<style>
  .continue-view {
    max-width: 900px;
    margin: 0 auto;
    padding: 32px 24px 80px;
  }

  .hero-player-card {
    padding: 36px;
    background: linear-gradient(135deg, rgba(23, 31, 48, 0.9), rgba(15, 20, 34, 0.95));
    border: 1px solid rgba(99, 102, 241, 0.4);
    box-shadow: 0 16px 40px rgba(0, 0, 0, 0.5), 0 0 30px rgba(99, 102, 241, 0.15);
    border-radius: var(--radius-lg);
  }

  .card-status-bar {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 24px;
    padding-bottom: 16px;
    border-bottom: 1px solid var(--border-subtle);
  }

  .status-indicator {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .live-pulse {
    width: 10px;
    height: 10px;
    border-radius: 50%;
    background: var(--status-playing);
    box-shadow: 0 0 10px var(--status-playing);
  }

  .status-label {
    font-size: 0.75rem;
    font-weight: 700;
    letter-spacing: 0.1em;
    color: var(--status-playing);
  }

  .quality-tags {
    display: flex;
    gap: 8px;
  }

  .media-title {
    font-size: 1.85rem;
    font-weight: 800;
    margin-bottom: 10px;
    line-height: 1.25;
  }

  .meta-row {
    display: flex;
    align-items: center;
    gap: 16px;
    font-size: 0.9rem;
    color: var(--text-secondary);
    margin-bottom: 24px;
    flex-wrap: wrap;
  }

  .queue-tag {
    color: #a5b4fc;
    font-weight: 500;
  }

  .stream-info-box {
    background: rgba(8, 11, 17, 0.6);
    padding: 14px 18px;
    border-radius: var(--radius-sm);
    border: 1px solid var(--border-subtle);
    margin-bottom: 28px;
  }

  .info-label {
    font-size: 0.75rem;
    color: var(--text-muted);
    margin-bottom: 4px;
    text-transform: uppercase;
    font-weight: 600;
  }

  .stream-url {
    font-family: monospace;
    font-size: 0.85rem;
    color: #a5b4fc;
    word-break: break-all;
  }

  .action-alert {
    padding: 12px 18px;
    border-radius: var(--radius-sm);
    background: rgba(99, 102, 241, 0.15);
    border: 1px solid rgba(99, 102, 241, 0.3);
    color: #c7d2fe;
    margin-bottom: 24px;
    font-size: 0.9rem;
  }

  .playback-controls {
    display: flex;
    gap: 14px;
  }

  .control-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 10px;
    padding: 12px 24px;
    border-radius: var(--radius-md);
    font-weight: 600;
    font-size: 0.95rem;
  }

  .control-btn.primary {
    background: var(--accent-primary);
    color: #fff;
    box-shadow: var(--accent-glow);
  }

  .control-btn.primary:hover:not(:disabled) {
    background: var(--accent-primary-hover);
    transform: translateY(-1px);
  }

  .control-btn.danger {
    background: rgba(239, 68, 68, 0.15);
    border: 1px solid rgba(239, 68, 68, 0.3);
    color: #fca5a5;
  }

  .control-btn.danger:hover:not(:disabled) {
    background: rgba(239, 68, 68, 0.25);
  }

  .control-btn:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }

  .state-card, .idle-card {
    text-align: center;
    padding: 60px 24px;
    border-radius: var(--radius-lg);
  }

  .idle-icon {
    font-size: 3.5rem;
    margin-bottom: 16px;
  }

  .idle-title, .state-title {
    font-size: 1.5rem;
    font-weight: 700;
    margin-bottom: 8px;
  }

  .idle-desc, .state-sub {
    color: var(--text-secondary);
    max-width: 480px;
    margin: 0 auto 24px;
    font-size: 0.95rem;
  }

  .jump-btn {
    background: var(--accent-primary);
    color: #fff;
    padding: 10px 22px;
    border-radius: var(--radius-full);
    font-weight: 600;
    box-shadow: var(--accent-glow);
  }

  .jump-btn:hover {
    background: var(--accent-primary-hover);
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
    width: 32px;
    height: 32px;
    border-width: 3px;
    margin-bottom: 16px;
  }

  @keyframes spin {
    to { transform: rotate(360deg); }
  }
</style>
