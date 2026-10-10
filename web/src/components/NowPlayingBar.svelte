<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import type { PlaybackSnapshot, QueueItem } from '../lib/types';
  import { nextEpisode, stopPlayback } from '../lib/api';
  import { playQueuedItem, queueItems, refreshQueue, removeQueuedItem } from '../lib/queue';
  import { Square, SkipForward, AlertTriangle, Loader2, Play, X, ListVideo, CheckCircle2, ChevronDown, ChevronUp } from 'lucide-svelte';

  export let snapshot: PlaybackSnapshot;
  export let onStateChange: () => void;

  let isStopping = false;
  let controlError = '';
  let queueActionError = '';
  let queuePollHandle: ReturnType<typeof setInterval> | undefined;

  const QUEUE_COLLAPSED_KEY = 'flix.queueCollapsed';
  let queueCollapsed = readQueueCollapsed();

  // Storage can throw in a private window. The section then opens expanded.
  function readQueueCollapsed(): boolean {
    try {
      return localStorage.getItem(QUEUE_COLLAPSED_KEY) === '1';
    } catch {
      return false;
    }
  }

  function toggleQueueCollapsed() {
    queueCollapsed = !queueCollapsed;
    try {
      localStorage.setItem(QUEUE_COLLAPSED_KEY, queueCollapsed ? '1' : '0');
    } catch {
      // The choice then applies to this page load only.
    }
  }

  onMount(() => {
    refreshQueue();
    queuePollHandle = setInterval(refreshQueue, 3000);
  });

  onDestroy(() => {
    if (queuePollHandle) clearInterval(queuePollHandle);
  });

  function episodeCode(media: QueueItem['media']): string {
    if (media.type !== 'episode') return '';
    return `S${String(media.season).padStart(2, '0')}E${String(media.episode).padStart(2, '0')}`;
  }

  function mediaTitle(media: QueueItem['media']): string {
    if (media.type === 'movie') return media.title;
    return media.show_title ?? episodeCode(media);
  }

  function mediaSubtitle(media: QueueItem['media']): string {
    if (media.type !== 'episode') return '';
    const code = episodeCode(media);
    return media.show_title ? (media.title ? `${code} · ${media.title}` : code) : (media.title ?? '');
  }

  function bufferPercent(item: QueueItem): number {
    if (item.target_bytes <= 0) return 0;
    return Math.min(100, Math.round((item.buffered_bytes / item.target_bytes) * 100));
  }

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

  async function handlePlayQueued(id: string) {
    queueActionError = '';
    try {
      await playQueuedItem(id);
      onStateChange();
    } catch (error) {
      queueActionError = error instanceof Error ? error.message : 'That item could not be played.';
    }
  }

  async function handleRemoveQueued(id: string) {
    queueActionError = '';
    try {
      await removeQueuedItem(id);
    } catch (error) {
      queueActionError = error instanceof Error ? error.message : 'That item could not be removed.';
    }
  }

  // The browser player has its own controls, so the playback row hides for it.
  $: showPlayback = snapshot.state !== 'idle' && snapshot.state !== 'browser';
  $: hasQueue = $queueItems.length > 0;
  $: isBusy = snapshot.state !== 'idle' && snapshot.state !== 'playing' && snapshot.state !== 'failed' && snapshot.state !== 'stopping';
</script>

{#if showPlayback || hasQueue}
  <aside
    class="now-playing-bar panel"
    class:is-playing={snapshot.state === 'playing'}
    class:is-failed={snapshot.state === 'failed'}
    class:is-busy={isBusy}
  >
    {#if showPlayback}
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
    {/if}

    {#if hasQueue}
      <div class="queue-strip" class:has-playback={showPlayback}>
        <button
          class="queue-strip-header"
          on:click={toggleQueueCollapsed}
          aria-expanded={!queueCollapsed}
          title={queueCollapsed ? 'Show queue' : 'Hide queue'}
        >
          <ListVideo size={13} />
          <span>Up Next · {$queueItems.length}/2</span>
          {#if queueCollapsed}<ChevronUp size={13} />{:else}<ChevronDown size={13} />{/if}
        </button>
        {#if !queueCollapsed}
          {#if queueActionError}
            <span class="now-sub status-failed-msg" role="alert">{queueActionError}</span>
          {/if}
          {#each $queueItems as item (item.id)}
            <div class="queue-row">
              <div class="queue-row-status">
                {#if item.state === 'ready'}
                  <CheckCircle2 size={16} class="queue-status-ready" />
                {:else}
                  <Loader2 size={16} class="spinner-icon queue-status-buffering" />
                {/if}
              </div>
              <div class="queue-row-text">
                <span class="queue-row-title">{mediaTitle(item.media)}</span>
                {#if mediaSubtitle(item.media)}
                  <span class="queue-row-subtitle">{mediaSubtitle(item.media)}</span>
                {/if}
                <div class="queue-row-progress">
                  <div class="queue-row-progress-fill" class:is-ready={item.state === 'ready'} style="width: {bufferPercent(item)}%"></div>
                </div>
              </div>
              <div class="queue-row-actions">
                <button
                  class="queue-pill-btn"
                  on:click={() => handlePlayQueued(item.id)}
                  title="Play now"
                >
                  <Play size={13} />
                </button>
                <button
                  class="queue-pill-btn"
                  on:click={() => handleRemoveQueued(item.id)}
                  title="Remove from queue"
                >
                  <X size={13} />
                </button>
              </div>
            </div>
          {/each}
        {/if}
      </div>
    {/if}
  </aside>
{/if}

<style>
  .now-playing-bar {
    position: fixed;
    bottom: 24px;
    left: 50%;
    transform: translateX(-50%);
    width: calc(100% - 48px);
    max-width: 980px;
    z-index: 100;
    padding: 14px 24px;
    border-radius: var(--radius-lg);
    background: var(--modal-surface);
    border: 1px solid var(--border-muted);
    border-left: 3px solid var(--text-muted);
    box-shadow: var(--shadow-lg);
    transition: border-color var(--transition-smooth), background-color var(--transition-smooth);
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
    gap: 16px;
  }

  .info-section {
    display: flex;
    align-items: center;
    gap: 14px;
    min-width: 0;
  }

  .text-block {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }

  .now-title {
    font-size: 0.94rem;
    font-weight: 600;
    color: var(--text-primary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .now-sub {
    font-size: 0.78rem;
    color: var(--text-secondary);
    margin-top: 2px;
  }

  :global(.status-busy) {
    color: var(--status-amber);
    animation: spin 1s linear infinite;
    flex-shrink: 0;
  }

  :global(.status-failed) {
    color: var(--status-red);
    flex-shrink: 0;
  }

  .status-failed-msg {
    color: #fca5a5;
  }

  .status-dot-playing {
    width: 9px;
    height: 9px;
    border-radius: 50%;
    background: var(--status-green);
    box-shadow: 0 0 10px var(--status-green);
    animation: live-pulse 2s infinite;
    flex-shrink: 0;
  }

  @keyframes live-pulse {
    0%, 100% {
      opacity: 1;
      transform: scale(1);
    }
    50% {
      opacity: 0.45;
      transform: scale(0.85);
    }
  }

  .controls-section {
    display: flex;
    gap: 10px;
    flex-shrink: 0;
  }

  .control-btn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 7px 16px;
    background: var(--bg-surface-elevated);
    border: 1px solid var(--border-subtle);
    color: var(--text-primary);
    border-radius: var(--radius-sm);
    font-size: 0.82rem;
    font-weight: 600;
    transition: all var(--transition-fast);
  }

  .control-btn:hover:not(:disabled) {
    background: var(--bg-surface-hover);
    border-color: rgba(255, 255, 255, 0.15);
  }

  .btn-stop {
    background: rgba(239, 68, 68, 0.15);
    color: #f87171;
    border: 1px solid rgba(239, 68, 68, 0.35);
  }

  .btn-stop:hover:not(:disabled) {
    background: var(--status-red);
    color: #fff;
    border-color: var(--status-red);
  }

  @keyframes spin {
    from { transform: rotate(0deg); }
    to { transform: rotate(360deg); }
  }

  .queue-strip {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .queue-strip.has-playback {
    margin-top: 14px;
    padding-top: 12px;
    border-top: 1px solid var(--border-subtle);
  }

  .queue-strip-header {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 0.7rem;
    font-weight: 700;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--text-muted);
    background: transparent;
    padding: 0;
    align-self: flex-start;
  }

  .queue-strip-header:hover {
    color: var(--text-primary);
  }

  .queue-row {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 10px;
    border-radius: var(--radius-sm);
    background: var(--bg-surface-elevated);
    border: 1px solid var(--border-subtle);
  }

  .queue-row-status {
    display: flex;
    align-items: center;
    flex-shrink: 0;
  }

  :global(.queue-status-ready) {
    color: var(--status-green);
  }

  :global(.queue-status-buffering) {
    color: var(--text-secondary);
  }

  .queue-row-text {
    display: flex;
    flex-direction: column;
    gap: 3px;
    min-width: 0;
    flex: 1;
  }

  .queue-row-title {
    font-size: 0.82rem;
    font-weight: 600;
    color: var(--text-primary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .queue-row-subtitle {
    font-size: 0.72rem;
    color: var(--text-secondary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .queue-row-progress {
    height: 3px;
    border-radius: var(--radius-full);
    background: var(--border-subtle);
    overflow: hidden;
    margin-top: 2px;
  }

  .queue-row-progress-fill {
    height: 100%;
    background: var(--text-muted);
    transition: width var(--transition-smooth);
  }

  .queue-row-progress-fill.is-ready {
    background: var(--status-green);
  }

  .queue-row-actions {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-shrink: 0;
  }

  .queue-pill-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 26px;
    height: 26px;
    border-radius: 50%;
    background: var(--control-fill);
    color: var(--text-primary);
  }

  .queue-pill-btn:hover {
    background: var(--control-fill-hover);
  }
</style>
