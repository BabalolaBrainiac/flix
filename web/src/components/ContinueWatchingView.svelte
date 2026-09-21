<script lang="ts">
  import { nextEpisode, stopPlayback } from '../lib/api';
  import type { PlaybackSnapshot } from '../lib/types';
  import { Square, SkipForward, HardDrive, Tv, Film } from 'lucide-svelte';

  export let playbackState: PlaybackSnapshot;
  export let onNavigateToDiscover: () => void;
  export let onStateChanged: () => void;

  let isAdvancing = false;
  let isStopping = false;
  let actionMessage = '';

  async function handleNext() {
    isAdvancing = true;
    try {
      await nextEpisode();
      onStateChanged();
    } catch (e: any) {
      actionMessage = `Next episode failed: ${e.message}`;
    } finally {
      isAdvancing = false;
    }
  }

  async function handleStop() {
    isStopping = true;
    try {
      await stopPlayback();
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
  {#if playbackState.state === 'playing'}
    <div class="hero-player-card panel">
      <div class="card-status-bar">
        <div class="status-indicator">
          <span class="live-dot"></span>
          <span class="status-label">ACTIVE PLAYBACK SESSION</span>
        </div>
        <div class="quality-tags">
          <span class="badge {playbackState.quality === '4K' ? 'badge-4k' : 'badge-1080p'}">
            {playbackState.quality}
          </span>
          {#if playbackState.subtitle_ready}
            <span class="badge badge-recommended">English Subtitles</span>
          {/if}
        </div>
      </div>

      <div class="media-details">
        <h1 class="media-title">{playbackState.title}</h1>
        <div class="meta-row">
          <span class="meta-item"><HardDrive size={14} /> {formatBytes(playbackState.file_length)}</span>
          <span class="meta-item"><Tv size={14} /> {playbackState.player_name}</span>
          {#if playbackState.has_next}
            <span class="meta-item queue-tag">Next Episode Queued</span>
          {/if}
        </div>
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
            <SkipForward size={16} /> Next Episode
          </button>
        {/if}

        <button
          class="control-btn danger"
          on:click={handleStop}
          disabled={isStopping || isAdvancing}
        >
          <Square size={14} /> Stop Playback
        </button>
      </div>
    </div>
  {:else if playbackState.state === 'browser'}
    <p>The browser player is above. Use its controls to continue playback.</p>
  {:else}
    <div class="idle-card panel">
      <Film size={48} class="idle-icon" />
      <h2 class="idle-title">No Active Session</h2>
      <p class="idle-desc">
        Find a movie, series, or anime on Discover to start playback.
      </p>
      <button class="jump-btn" on:click={onNavigateToDiscover}>
        Find Something to Watch
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
    padding: 32px;
  }

  .card-status-bar {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 20px;
    padding-bottom: 12px;
    border-bottom: 1px solid var(--border-subtle);
  }

  .status-indicator {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .live-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--status-green);
  }

  .status-label {
    font-size: 0.75rem;
    font-weight: 700;
    letter-spacing: 0.05em;
    color: var(--status-green);
  }

  .quality-tags {
    display: flex;
    gap: 6px;
  }

  .media-title {
    font-size: 1.6rem;
    font-weight: 700;
    margin-bottom: 8px;
  }

  .meta-row {
    display: flex;
    align-items: center;
    gap: 16px;
    font-size: 0.85rem;
    color: var(--text-secondary);
    margin-bottom: 20px;
  }

  .meta-item {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .queue-tag {
    color: #93c5fd;
  }

  .action-alert {
    padding: 10px 14px;
    border-radius: var(--radius-sm);
    background: var(--bg-surface-active);
    color: var(--status-amber);
    margin-bottom: 20px;
    font-size: 0.85rem;
  }

  .playback-controls {
    display: flex;
    gap: 12px;
  }

  .control-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    padding: 10px 20px;
    border-radius: var(--radius-sm);
    font-weight: 600;
    font-size: 0.9rem;
  }

  .control-btn.primary {
    background: var(--action-fill);
    color: #fff;
  }

  .control-btn.primary:hover:not(:disabled) {
    background: var(--action-fill-hover);
  }

  .control-btn.danger {
    background: rgba(239, 68, 68, 0.15);
    border: 1px solid rgba(239, 68, 68, 0.3);
    color: #fca5a5;
  }

  .control-btn.danger:hover:not(:disabled) {
    background: rgba(239, 68, 68, 0.25);
  }

  .idle-card {
    text-align: center;
    padding: 56px 24px;
  }

  :global(.idle-icon) {
    margin-bottom: 14px;
    color: var(--text-muted);
  }

  .idle-title {
    font-size: 1.3rem;
    font-weight: 700;
    margin-bottom: 6px;
  }

  .idle-desc {
    color: var(--text-secondary);
    max-width: 440px;
    margin: 0 auto 20px;
    font-size: 0.9rem;
  }

  .jump-btn {
    background: var(--action-fill);
    color: #fff;
    padding: 8px 18px;
    border-radius: var(--radius-sm);
    font-weight: 600;
    font-size: 0.85rem;
  }

  .jump-btn:hover {
    background: var(--action-fill-hover);
  }
</style>
