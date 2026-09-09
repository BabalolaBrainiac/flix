<script lang="ts">
  import type { PlaybackSnapshot } from '../lib/types';
  import { nextEpisode, stopPlayback } from '../lib/api';
  import { Square, SkipForward, AlertTriangle, Loader2 } from 'lucide-svelte';

  export let snapshot: PlaybackSnapshot;
  export let onStateChange: () => void;

  let isStopping = false;
  let controlError = '';

  async function handleStop() {
    isStopping = true;
    controlError = '';
    try {
      await stopPlayback();
      onStateChange();
    } catch (error) {
      controlError = error instanceof Error ? error.message : 'Playback could not stop. Try again.';
    } finally {
      isStopping = false;
    }
  }

  async function handleNext() {
    controlError = '';
    try {
      await nextEpisode();
      onStateChange();
    } catch (error) {
      controlError = error instanceof Error ? error.message : 'The next episode could not start.';
    }
  }

  $: isBusy = snapshot.state !== 'idle' && snapshot.state !== 'playing' && snapshot.state !== 'failed' && snapshot.state !== 'stopping';
</script>

{#if snapshot.state !== 'idle'}
  <aside
    class="now-playing-bar panel"
    class:is-playing={snapshot.state === 'playing'}
    class:is-failed={snapshot.state === 'failed'}
    class:is-busy={isBusy}
  >
    <div class="bar-content">
      <div class="info-section">
        {#if isBusy}
          <Loader2 size={18} class="spinner-icon status-busy" />
        {:else if snapshot.state === 'playing'}
          <span class="status-dot-playing"></span>
        {:else if snapshot.state === 'failed'}
          <AlertTriangle size={18} class="status-failed" />
        {/if}

        <div class="text-block">
          {#if controlError}
            <span class="now-sub status-failed-msg" role="alert">{controlError}</span>
          {/if}
          {#if snapshot.state === 'playing'}
            <span class="now-title">{snapshot.title}</span>
            <span class="now-sub">
              Open in {snapshot.player_name}{snapshot.subtitle_ready ? '' : ' · No subtitles'}
            </span>
          {:else if snapshot.state === 'resolving_source'}
            <span class="now-title">Resolving Source</span>
            <span class="now-sub">Connecting to torrent providers...</span>
          {:else if snapshot.state === 'loading_torrent'}
            <span class="now-title">Loading Torrent</span>
            <span class="now-sub">Fetching metadata and peers ({snapshot.quality})...</span>
          {:else if snapshot.state === 'buffering'}
            <span class="now-title">Buffering Video</span>
            <span class="now-sub">{snapshot.file_name}</span>
          {:else if snapshot.state === 'finding_subtitle'}
            <span class="now-title">Finding English Subtitles</span>
            <span class="now-sub">Searching cached sources and gateway...</span>
          {:else if snapshot.state === 'downloading_subtitle'}
            <span class="now-title">Downloading Subtitles</span>
            <span class="now-sub">{snapshot.subtitle_name}</span>
          {:else if snapshot.state === 'launching_player'}
            <span class="now-title">Launching Player</span>
            <span class="now-sub">Opening {snapshot.player_name}...</span>
          {:else if snapshot.state === 'stopping'}
            <span class="now-title">Stopping Playback</span>
            <span class="now-sub">Terminating player process...</span>
          {:else if snapshot.state === 'failed'}
            <span class="now-title">Playback Failed</span>
            <span class="now-sub status-failed-msg">{snapshot.error.message}</span>
          {/if}
        </div>
      </div>

      <div class="controls-section">
        {#if snapshot.state === 'playing' && snapshot.has_next}
          <button class="control-btn" on:click={handleNext} title="Next Episode">
            <SkipForward size={16} /> Next
          </button>
        {/if}

        <button
          class="control-btn btn-stop"
          on:click={handleStop}
          disabled={isStopping}
          title={snapshot.state === 'failed' ? 'Dismiss' : 'Stop Playback'}
        >
          <Square size={14} /> {snapshot.state === 'failed' ? 'Dismiss' : 'Stop'}
        </button>
      </div>
    </div>
  </aside>
{/if}

<style>
  .now-playing-bar {
    position: fixed;
    bottom: 16px;
    left: 50%;
    transform: translateX(-50%);
    width: calc(100% - 32px);
    max-width: 960px;
    z-index: 100;
    padding: 12px 20px;
    border-radius: var(--radius-lg);
    background: var(--bg-surface);
    border: 1px solid var(--border-subtle);
    /* A colored left edge shows the state at a glance. */
    border-left: 3px solid var(--text-muted);
    box-shadow: 0 10px 25px rgba(0, 0, 0, 0.5);
    transition: border-left-color var(--transition-smooth);
  }

  .now-playing-bar.is-busy {
    border-left-color: var(--accent-primary);
  }

  .now-playing-bar.is-playing {
    border-left-color: var(--status-green);
  }

  .now-playing-bar.is-failed {
    border-left-color: var(--status-red);
  }

  .bar-content {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }

  .info-section {
    display: flex;
    align-items: center;
    gap: 12px;
  }

  .text-block {
    display: flex;
    flex-direction: column;
  }

  .now-title {
    font-size: 0.9rem;
    font-weight: 600;
    color: var(--text-primary);
  }

  .now-sub {
    font-size: 0.75rem;
    color: var(--text-muted);
  }

  :global(.status-busy) {
    color: var(--status-amber);
    animation: spin 1s linear infinite;
  }

  :global(.status-failed) {
    color: var(--status-red);
  }

  .status-failed-msg {
    color: #fca5a5;
  }

  .status-dot-playing {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--status-green);
  }

  .controls-section {
    display: flex;
    gap: 8px;
  }

  .control-btn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 6px 12px;
    background: var(--bg-surface-active);
    color: var(--text-primary);
    border-radius: var(--radius-sm);
    font-size: 0.8rem;
  }

  .btn-stop {
    background: rgba(229, 9, 20, 0.15);
    color: #f87171;
    border: 1px solid rgba(229, 9, 20, 0.3);
  }

  .btn-stop:hover:not(:disabled) {
    background: var(--accent-primary);
    color: #fff;
  }

  @keyframes spin {
    from { transform: rotate(0deg); }
    to { transform: rotate(360deg); }
  }
</style>
