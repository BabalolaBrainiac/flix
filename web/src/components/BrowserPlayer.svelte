<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { browserEvent, nextEpisode } from '../lib/api';
  import type { BrowserEvent, BrowserPlayback } from '../lib/types';

  export let snapshot: BrowserPlayback;
  export let onStateChange: () => void;

  const initialPosition = snapshot.position_ms;
  const initiallyPaused = snapshot.phase === 'paused';

  let video: HTMLVideoElement;
  let notice = '';
  let phase = snapshot.phase;
  let disposed = false;
  let stopping = false;
  let paused = true;
  let sending = false;
  let queued: { id: string; event: BrowserEvent; position: number } | null = null;
  let heartbeat: ReturnType<typeof setInterval>;
  let pending = Promise.resolve();

  function report(event: BrowserEvent) {
    if (disposed || stopping) return;
    if (event === 'heartbeat' && (sending || queued)) return;
    const position = video?.readyState ? Math.max(0, Math.round(video.currentTime * 1000)) : initialPosition;
    const id = snapshot.playback_id;
    if (event === 'playing' || event === 'paused' || event === 'buffering') phase = event;
    if (queued?.event === 'failed' || queued?.event === 'ended') return;
    queued = { id, event, position };
    if (!sending) pending = sendEvents();
  }

  async function sendEvents() {
    sending = true;
    try {
      while (queued && !disposed) {
        const update = queued;
        queued = null;
        try {
          await browserEvent(update.id, update.event, update.position);
        } catch {
          if (!disposed) notice = 'Flix could not update the session. Check the connection or try Stop.';
        }
      }
    } finally {
      sending = false;
    }
  }

  async function resume() {
    notice = '';
    try {
      await video.play();
    } catch (error) {
      if (disposed || stopping) return;
      if (error instanceof DOMException && error.name === 'NotAllowedError') {
        notice = 'Ready. Select Play to start the video.';
        report('paused');
      } else if (!(error instanceof DOMException && error.name === 'AbortError')) {
        report('failed');
      }
    }
  }

  function jump(seconds: number) {
    if (!Number.isFinite(video.duration)) return;
    video.currentTime = Math.max(0, Math.min(video.duration, video.currentTime + seconds));
  }

  function loaded() {
    if (initialPosition > 0 && Number.isFinite(video.duration)) {
      video.currentTime = Math.min(initialPosition / 1000, video.duration);
    }
    if (!initiallyPaused) void resume();
  }

  async function stop() {
    stopping = true;
    queued = null;
    video.pause();
    try {
      await pending;
      await browserEvent(snapshot.playback_id, 'stop');
      onStateChange();
    } catch {
      notice = 'Playback could not stop. Try Stop again.';
      stopping = false;
    }
  }

  async function next() {
    stopping = true;
    queued = null;
    video.pause();
    try {
      await pending;
      await nextEpisode();
      onStateChange();
    } catch {
      notice = 'The next episode could not start. Try again.';
      stopping = false;
    }
  }

  function visible() {
    if (!document.hidden) {
      report('heartbeat');
      onStateChange();
    }
  }

  onMount(() => {
    report('heartbeat');
    heartbeat = setInterval(() => report('heartbeat'), 30_000);
    document.addEventListener('visibilitychange', visible);
    if (!video.canPlayType(snapshot.mime_type)) report('failed');
  });

  onDestroy(() => {
    disposed = true;
    clearInterval(heartbeat);
    document.removeEventListener('visibilitychange', visible);
    if (video) {
      video.pause();
      video.removeAttribute('src');
      video.load();
    }
  });
</script>

<section class="browser-player panel" aria-label="Browser player">
  <div class="player-heading">
    <div><h2>{snapshot.title}</h2><p>{snapshot.quality} · Browser · {phase}</p></div>
    <button on:click={stop} disabled={stopping}>Stop</button>
  </div>
  <!-- svelte-ignore a11y_media_has_caption (The provider supplies subtitles when available.) -->
  <video
    bind:this={video}
    bind:paused
    src={snapshot.stream_url}
    controls
    playsinline
    preload="metadata"
    crossorigin="anonymous"
    on:loadedmetadata={loaded}
    on:playing={() => report('playing')}
    on:pause={() => { if (!video.ended) report('paused'); }}
    on:waiting={() => report('buffering')}
    on:seeking={() => report('buffering')}
    on:seeked={() => { if (video.paused) report('paused'); }}
    on:ended={() => report('ended')}
    on:error={() => report('failed')}
  >
    {#if snapshot.subtitle_url}
      <track kind="subtitles" src={snapshot.subtitle_url} srclang="en" label="English" default />
    {/if}
  </video>
  <div class="player-controls">
    <button on:click={() => jump(-10)} disabled={stopping} aria-label="Back 10 seconds">−10 seconds</button>
    <button on:click={() => video.paused ? resume() : video.pause()} disabled={stopping}>
      {paused ? 'Play' : 'Pause'}
    </button>
    <button on:click={() => jump(10)} disabled={stopping} aria-label="Forward 10 seconds">+10 seconds</button>
    {#if snapshot.has_next}<button on:click={next} disabled={stopping}>Next episode</button>{/if}
  </div>
  {#if notice}<p class="notice" role="status">{notice}</p>{/if}
  {#if !snapshot.subtitle_url}<p class="subtitle-note">No external English subtitles are available for this source.</p>{/if}
</section>

<style>
  .browser-player { max-width: 1000px; margin: 24px auto; padding: 16px; scroll-margin-top: 20px; }
  .player-heading { display: flex; align-items: center; justify-content: space-between; gap: 16px; margin-bottom: 12px; }
  h2 { font-size: 1.1rem; margin: 0 0 4px; overflow-wrap: anywhere; }
  p { color: var(--text-secondary); margin: 0; font-size: .85rem; }
  video { display: block; width: 100%; max-height: 70vh; aspect-ratio: 16 / 9; background: #000; border-radius: var(--radius-sm); }
  .player-controls { display: flex; flex-wrap: wrap; gap: 8px; margin-top: 12px; }
  button { padding: 9px 14px; background: var(--bg-surface-active); color: var(--text-primary); border: 1px solid var(--border-subtle); border-radius: var(--radius-sm); }
  button:hover { border-color: var(--text-muted); }
  button:focus-visible { outline: 2px solid var(--accent-primary); outline-offset: 3px; }
  .notice, .subtitle-note { margin-top: 12px; }
  @media (max-width: 640px) { .browser-player { margin: 12px; padding: 12px; } .player-controls button { flex: 1; } }
</style>
