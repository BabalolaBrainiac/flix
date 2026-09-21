<script lang="ts">
  import { onDestroy, onMount, tick } from 'svelte';
  import { getEpisodes, getStreams, play } from '../lib/api';
  import type { CatalogItemSummary, EpisodeSummary, StreamSummary, PlayCommand } from '../lib/types';
  import {
    membership,
    refreshMembership,
    findStatus,
    idsOf,
    imdbIdOf,
    episodeLabel,
    listsContaining,
    setStatus,
    clearStatus,
    titleRefOf,
    STATUS_LABELS,
  } from '../lib/library';
  import { Film, Tv, Play, ArrowLeft, Loader2, ChevronRight, ChevronDown, Bookmark, X, Clock, PlayCircle, CheckCircle2, User, Users, HardDrive } from 'lucide-svelte';
  import AddToListPopover from './AddToListPopover.svelte';

  export let item: CatalogItemSummary;
  export let activeProfileId: string | null = null;
  export let onPlayStarted: () => void;
  export let onOpenProfilePicker: () => void = () => {};
  export let onBack: () => void;

  const STATUS_OPTIONS = [
    { key: 'to_watch', icon: Clock },
    { key: 'in_progress', icon: PlayCircle },
    { key: 'watched', icon: CheckCircle2 },
  ] as const;

  let selectedItem: CatalogItemSummary = item;
  let showPopover = false;
  let statusError = '';
  let episodes: EpisodeSummary[] = [];
  let activeSeason = 1;
  let selectedEpisode: EpisodeSummary | null = null;
  let streams: StreamSummary[] = [];
  let showAllSources = false;
  let detailRequest = 0;
  let streamRequest = 0;
  let isLoadingEpisodes = false;
  let isLoadingStreams = false;
  let actionMessage = '';
  let isPreparingPlayback = false;
  let streamError = '';
  let streamAbort: AbortController | null = null;
  let sourcePanel: HTMLElement;
  let failedPoster = false;
  let restoredLastEpisode = false;

  // An anime with a `kitsu:` id also has an IMDb id inside its episode ids.
  // Store both, so the same title never shows twice in a list or a status.
  $: canonicalId = selectedItem.id.startsWith('kitsu:')
    ? imdbIdOf(episodes.find((episode) => imdbIdOf(episode.imdb_id))?.imdb_id)
    : null;
  $: titleRef = titleRefOf(selectedItem, canonicalId);
  $: ids = idsOf(titleRef);
  $: status = findStatus($membership, ids);
  $: lastPlayedEpisode = status?.episode_id ?? null;
  $: inLists = listsContaining($membership, ids);

  async function chooseStatus(next: string) {
    if (!activeProfileId || status?.status === next) return;
    statusError = '';
    try {
      await setStatus(activeProfileId, titleRef, next);
    } catch (e: any) {
      statusError = e.message || 'The status did not save. Try again.';
    }
  }

  async function removeStatus() {
    if (!activeProfileId) return;
    statusError = '';
    try {
      await clearStatus(activeProfileId, ids);
    } catch (e: any) {
      statusError = e.message || 'The status was not removed. Try again.';
    }
  }

  function clearStreams() {
    streamRequest += 1;
    streamAbort?.abort();
    streamAbort = null;
    streams = [];
    streamError = '';
    isLoadingStreams = false;
    showAllSources = false;
  }

  onMount(() => {
    if (activeProfileId) void refreshMembership(activeProfileId);
    void selectItem(item);
  });

  onDestroy(() => {
    detailRequest += 1;
    clearStreams();
  });

  $: seasons = Array.from(new Set(episodes.map((e) => e.season))).sort((a, b) => a - b);
  $: seasonEpisodes = episodes.filter((e) => e.season === activeSeason);
  $: needsEpisode = selectedItem.media_type !== 'movie';
  $: showStreams = !needsEpisode || selectedEpisode !== null;
  $: if (!restoredLastEpisode && lastPlayedEpisode && episodes.length > 0) {
    const savedEpisode = episodes.find((episode) => episode.id === lastPlayedEpisode);
    if (savedEpisode) activeSeason = savedEpisode.season;
    restoredLastEpisode = true;
  }

  async function selectItem(target: CatalogItemSummary) {
    const request = ++detailRequest;
    clearStreams();
    selectedItem = target;
    episodes = [];
    selectedEpisode = null;
    restoredLastEpisode = false;
    isLoadingEpisodes = false;
    actionMessage = '';

    try {
      if (target.media_type !== 'movie') {
        isLoadingEpisodes = true;
        const epRes = await getEpisodes(target.id, target.is_anime);
        if (request !== detailRequest) return;
        episodes = epRes.episodes;
        if (epRes.is_anime) selectedItem = { ...target, is_anime: true };
        if (episodes.length > 0) activeSeason = episodes[0].season;
      } else {
        isLoadingStreams = true;
        streamAbort = new AbortController();
        const streamRes = await getStreams('movie', target.id, false, streamAbort.signal);
        if (request !== detailRequest) return;
        streams = streamRes.streams;
        if (streamRes.is_anime) selectedItem = { ...target, is_anime: true };
      }
    } catch (e: any) {
      if (request !== detailRequest) return;
      actionMessage = `Failed to load details: ${e.message}`;
    } finally {
      if (request === detailRequest) {
        isLoadingEpisodes = false;
        if (target.media_type === 'movie') isLoadingStreams = false;
      }
    }
  }

  async function selectEpisode(episode: EpisodeSummary) {
    if (isPreparingPlayback) return;
    if (selectedEpisode?.id === episode.id && (isLoadingStreams || streams.length > 0)) return;
    clearStreams();
    const request = streamRequest;
    streamAbort = new AbortController();
    selectedEpisode = episode;
    isLoadingStreams = true;
    actionMessage = '';
    const signal = streamAbort.signal;
    void revealSources(request);
    try {
      const streamRes = await getStreams('series', episode.stream_id || episode.id, selectedItem.is_anime, signal);
      if (request !== streamRequest) return;
      streams = streamRes.streams;
      if (streamRes.is_anime) selectedItem = { ...selectedItem, is_anime: true };
    } catch (e: any) {
      if (request !== streamRequest) return;
      streamError = e.message || 'Sources could not load. Try again.';
    } finally {
      if (request === streamRequest) isLoadingStreams = false;
    }
  }

  async function revealSources(request: number) {
    await tick();
    if (request !== streamRequest || !window.matchMedia('(max-width: 800px)').matches) return;
    sourcePanel?.scrollIntoView({
      block: 'nearest',
      behavior: window.matchMedia('(prefers-reduced-motion: reduce)').matches ? 'instant' : 'smooth',
    });
  }

  function selectSeason(season: number) {
    if (season === activeSeason || isPreparingPlayback) return;
    activeSeason = season;
    selectedEpisode = null;
    actionMessage = '';
    clearStreams();
  }

  $: recommendedStream = streams.find((s) => s.is_recommended) || streams[0];
  $: otherStreams = streams.filter((s) => s !== recommendedStream);

  let playbackTarget: 'external' | 'browser' = 'external';
  $: if (selectedItem.is_anime && playbackTarget === 'browser') playbackTarget = 'external';

  function mediaRefFor(stream: StreamSummary): PlayCommand['media_ref'] {
    if (selectedItem.media_type === 'movie') {
      return { type: 'movie', catalog_id: selectedItem.id, title: selectedItem.name };
    }
    if (!selectedEpisode) return undefined;
    return {
      type: 'episode',
      catalog_id: selectedItem.id,
      stream_id: selectedEpisode.stream_id || selectedEpisode.id,
      imdb_id: selectedEpisode.imdb_id,
      season: selectedEpisode.season,
      episode: selectedEpisode.episode,
      title: selectedEpisode.title,
    };
  }

  async function handlePlayStream(stream: StreamSummary) {
    if (isPreparingPlayback || (needsEpisode && !selectedEpisode)) return;
    const request = streamRequest;
    isPreparingPlayback = true;
    actionMessage = 'Preparing playback…';
    try {
      const command: PlayCommand = {
        target: playbackTarget,
        source_id: stream.source_id,
        file_index: stream.file_index,
        media_ref: mediaRefFor(stream),
        queue_seed:
          episodes.length > 0 && selectedEpisode
            ? { catalog_item: selectedItem, episodes, current_episode_id: selectedEpisode.id }
            : undefined,
      };
      await play(command);
      if (request === streamRequest) actionMessage = '';
      onPlayStarted();
    } catch (e: any) {
      if (request === streamRequest) actionMessage = `Playback failed: ${e.message}`;
    } finally {
      isPreparingPlayback = false;
    }
  }

  function formatEpisodeTitle(ep: EpisodeSummary): string {
    const code = `S${String(ep.season).padStart(2, '0')}E${String(ep.episode).padStart(2, '0')}`;
    const t = ep.title?.trim();
    return t ? `${code} · ${t}` : code;
  }

  function sourceName(stream: StreamSummary): string {
    return stream.name.split('\n')
      .map(line => line.trim())
      .filter(line => line && line.toLowerCase() !== stream.quality.toLowerCase())
      .join(' · ') || stream.name;
  }
</script>

<div class="detail-view">
  <button class="back-btn" on:click={onBack} disabled={isPreparingPlayback}>
    <ArrowLeft size={16} /> Back
  </button>

  <div class="detail-header">
    <div class="detail-poster">
      {#if selectedItem.poster && !failedPoster}
        <img
          src={selectedItem.poster}
          alt={selectedItem.name}
          decoding="async"
          on:error={() => (failedPoster = true)}
        />
      {:else}
        <div class="poster-placeholder">
          <Film size={48} />
        </div>
      {/if}
    </div>
    <div class="detail-meta">
      <h2>{selectedItem.name}</h2>
      <div class="meta-row">
        {#if selectedItem.release_info}
          <span class="meta-tag">{selectedItem.release_info}</span>
        {/if}
        <span class="badge badge-recommended">
          {selectedItem.is_anime ? 'Anime' : selectedItem.media_type.toUpperCase()}
        </span>
      </div>

      {#if activeProfileId}
        <div class="status-row" role="group" aria-label="Watch status">
          {#each STATUS_OPTIONS as option (option.key)}
            <button
              type="button"
              class="status-chip {option.key}"
              class:active={status?.status === option.key}
              aria-pressed={status?.status === option.key}
              on:click={() => chooseStatus(option.key)}
            >
              <svelte:component this={option.icon} size={14} />
              <span>{STATUS_LABELS[option.key]}</span>
            </button>
          {/each}
          {#if status}
            <button type="button" class="status-clear" on:click={removeStatus} title="Remove status" aria-label="Remove status">
              <X size={14} />
            </button>
          {/if}
        </div>

        <div class="lists-row">
          {#each inLists as list (list.id)}
            <span class="list-chip">{list.name}</span>
          {/each}
          <button
            type="button"
            class="btn-list-action"
            on:click={() => (showPopover = true)}
            title={inLists.length > 0 ? 'Manage lists' : 'Add to a list'}
            aria-label={inLists.length > 0 ? 'Manage lists' : 'Add to a list'}
          >
            <Bookmark size={15} />
            {#if inLists.length === 0}<span>Add to a list</span>{/if}
          </button>
        </div>
        {#if statusError}
          <div class="action-alert" role="alert">{statusError}</div>
        {/if}
      {:else}
        <div class="detail-actions-row">
          <button type="button" class="btn-list-action" on:click={onOpenProfilePicker}>
            <User size={15} />
            <span>Choose a profile to save this title</span>
          </button>
        </div>
      {/if}

      {#if actionMessage}
        <div class="action-alert">{actionMessage}</div>
      {/if}
    </div>
  </div>

  <div class="selection-layout" class:has-episodes={needsEpisode}>
  {#if needsEpisode}
    <div class="episodes-section">
      <div class="section-heading">
        <h3>Choose an episode</h3>
        <p>
          {#if episodeLabel(lastPlayedEpisode)}
            Last played {episodeLabel(lastPlayedEpisode)}. Select an episode to see its sources.
          {:else}
            Select an episode to see its sources.
          {/if}
        </p>
      </div>
      {#if seasons.length > 1}
        <div class="season-tabs">
          {#each seasons as s}
            <button
              class="season-tab {activeSeason === s ? 'active' : ''}"
              on:click={() => selectSeason(s)}
              aria-pressed={activeSeason === s}
              disabled={isPreparingPlayback}
            >
              Season {s}
            </button>
          {/each}
        </div>
      {/if}

      {#if isLoadingEpisodes}
        <div class="loading-box" role="status"><Loader2 size={20} class="spinner-icon" /> Loading episodes…</div>
      {:else if episodes.length === 0}
        <p class="no-streams">No episodes are available for this title.</p>
      {:else}
      <div class="episode-list" aria-label="Episodes">
        {#each seasonEpisodes as ep (ep.id)}
          <button
            class="episode-row {selectedEpisode?.id === ep.id ? 'active' : ''} {lastPlayedEpisode === ep.id ? 'last-played' : ''}"
            on:click={() => selectEpisode(ep)}
            aria-pressed={selectedEpisode?.id === ep.id}
            aria-controls="selected-sources"
            disabled={isPreparingPlayback}
          >
            <span class="ep-number">{ep.episode}</span>
            <span class="ep-title">{ep.title?.trim() || `Episode ${ep.episode}`}</span>
            {#if lastPlayedEpisode === ep.id}<span class="last-played-label">Last played</span>{/if}
            <ChevronRight size={16} class="ep-select-icon" />
          </button>
        {/each}
      </div>
      {/if}
    </div>
  {/if}

  {#if showStreams}
  <section class="streams-section" id="selected-sources" aria-labelledby="sources-heading" bind:this={sourcePanel}>
    <div class="section-heading">
      <div class="sources-heading-row">
        <h3 id="sources-heading">{selectedEpisode ? formatEpisodeTitle(selectedEpisode) : 'Choose a source'}</h3>
        <div class="player-toggle" role="group" aria-label="Play with">
          <button
            type="button"
            class="player-toggle-btn"
            class:active={playbackTarget === 'external'}
            disabled={isPreparingPlayback}
            on:click={() => (playbackTarget = 'external')}
          >
            External
          </button>
          <button
            type="button"
            class="player-toggle-btn"
            class:active={playbackTarget === 'browser'}
            disabled={isPreparingPlayback || selectedItem?.is_anime}
            title={selectedItem?.is_anime ? 'Anime needs the external player' : 'Browser preview'}
            on:click={() => (playbackTarget = 'browser')}
          >
            Browser
          </button>
        </div>
      </div>
      {#if playbackTarget === 'browser'}
        <p>MP4 and WebM only. Anime needs the external player until verified track selection is available.</p>
      {:else if selectedItem?.is_anime}
        <p>Original audio and English subtitles. Anime currently needs an external player.</p>
      {/if}
    </div>
    {#if isLoadingStreams}
      <div class="loading-box" role="status">
        <Loader2 size={24} class="spinner-icon" />
        <span>Finding sources…</span>
      </div>
    {:else if streams.length > 0}
      {#if recommendedStream}
        <div class="recommended-banner">
          <div class="stream-info-main">
            <div class="stream-title-row">
              <span class="badge {recommendedStream.quality === '4K' ? 'badge-4k' : 'badge-1080p'}">
                {recommendedStream.quality}
              </span>
              <span class="stream-name" title={recommendedStream.name}>{sourceName(recommendedStream)}</span>
            </div>
            <div class="stream-stats">
              {#if recommendedStream.seeders !== undefined}
                <span><Users size={12} /> {recommendedStream.seeders}</span>
              {/if}
              {#if recommendedStream.size}
                <span><HardDrive size={12} /> {recommendedStream.size}</span>
              {/if}
            </div>
          </div>
          <div class="stream-actions">
            <button
              class="btn-play"
              on:click={() => handlePlayStream(recommendedStream)}
              disabled={isPreparingPlayback}
            >
              {#if isPreparingPlayback}<Loader2 size={16} class="spinner-icon" /> Preparing…{:else}<Play size={16} /> Play{/if}
            </button>
          </div>
        </div>
      {/if}

      {#if otherStreams.length > 0}
        <div class="other-sources">
          <button
            class="toggle-sources-btn"
            on:click={() => { showAllSources = !showAllSources; }}
            aria-expanded={showAllSources}
            aria-controls="alternative-sources"
          >
            <ChevronDown size={16} />
            {showAllSources ? 'Hide other sources' : `${otherStreams.length} other sources`}
          </button>

          {#if showAllSources}
            <div class="sources-list" id="alternative-sources">
              {#each otherStreams as s (s.source_id)}
                <div class="stream-row">
                  <div class="stream-details">
                    <span class="badge {s.quality === '4K' ? 'badge-4k' : 'badge-1080p'}">{s.quality}</span>
                    <span class="stream-title" title={s.name}>{sourceName(s)}</span>
                    {#if s.seeders !== undefined}
                      <span class="stream-seeders"><Users size={11} /> {s.seeders}</span>
                    {/if}
                    {#if s.size}
                      <span class="stream-size"><HardDrive size={11} /> {s.size}</span>
                    {/if}
                  </div>
                  <div class="stream-row-actions">
                    <button
                      class="btn-play-sm"
                      on:click={() => handlePlayStream(s)}
                      disabled={isPreparingPlayback}
                      title="Play this source"
                      aria-label="Play this source"
                    >
                      <Play size={14} />
                    </button>
                  </div>
                </div>
              {/each}
            </div>
          {/if}
        </div>
      {/if}
    {:else}
      <div class="source-empty" role="status">
        <p>{streamError || 'No sources are available for this selection.'}</p>
        {#if selectedEpisode}
          <button class="retry-btn" on:click={() => selectedEpisode && selectEpisode(selectedEpisode)}>Try again</button>
        {:else}
          <button class="retry-btn" on:click={() => selectedItem && selectItem(selectedItem)}>Try again</button>
        {/if}
      </div>
    {/if}
  </section>
  {:else if episodes.length > 0}
    <div class="selection-hint">
      <Tv size={28} />
      <p>Your next episode starts here.</p>
      <span>Choose an episode from the list.</span>
    </div>
  {/if}
  </div>
</div>

{#if showPopover}
  <AddToListPopover
    {titleRef}
    {activeProfileId}
    onClose={() => (showPopover = false)}
    {onOpenProfilePicker}
  />
{/if}

<style>
  .detail-actions-row {
    margin-top: 14px;
    display: flex;
    gap: 10px;
  }

  .btn-list-action {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    padding: 8px 14px;
    background: var(--bg-surface-elevated);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    color: var(--text-primary);
    font-size: 0.84rem;
    font-weight: 600;
    transition: all var(--transition-fast);
  }

  .btn-list-action:hover {
    border-color: var(--accent-primary);
    color: var(--accent-primary);
  }

  .search-status { color: var(--text-secondary); font-size: 0.875rem; margin-bottom: 1rem; }
  .sources-heading-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    flex-wrap: wrap;
    gap: 12px;
  }

  .player-toggle {
    display: flex;
    gap: 2px;
    padding: 3px;
    background: var(--bg-surface);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
  }

  .player-toggle-btn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 6px 12px;
    border-radius: var(--radius-xs);
    background: transparent;
    color: var(--text-secondary);
    font-size: 0.78rem;
    font-weight: 600;
  }

  .player-toggle-btn.active {
    background: var(--bg-surface-elevated);
    color: var(--accent-primary);
  }

  .player-toggle-btn:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }
  /* Full-width detail view */
  .detail-view {
    display: flex;
    flex-direction: column;
    gap: 28px;
  }

  .back-btn {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    background: transparent;
    color: var(--text-secondary);
    font-size: 0.9rem;
    padding: 8px 12px;
    border-radius: var(--radius-sm);
    align-self: flex-start;
  }

  .back-btn:hover {
    color: var(--text-primary);
    background: var(--bg-surface);
  }

  .detail-header {
    display: flex;
    gap: 28px;
    align-items: center;
    padding: 24px;
    background: linear-gradient(135deg, var(--bg-surface-elevated), var(--bg-surface) 65%);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-lg);
    box-shadow: var(--shadow-sm);
  }

  .detail-poster {
    width: 120px;
    aspect-ratio: 2 / 3;
    flex-shrink: 0;
    border-radius: var(--radius-md);
    overflow: hidden;
    background: var(--bg-secondary);
    box-shadow: var(--shadow-card);
  }

  .detail-poster img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }

  .detail-meta h2 {
    font-size: clamp(1.5rem, 2.5vw, 2.2rem);
    margin-bottom: 10px;
    letter-spacing: -0.02em;
  }

  .meta-row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 10px;
    margin-bottom: 14px;
  }

  .meta-tag {
    font-size: 0.85rem;
    color: var(--text-secondary);
    background: var(--bg-surface-elevated);
    padding: 3px 10px;
    border-radius: var(--radius-sm);
    border: 1px solid var(--border-subtle);
  }

  .action-alert {
    padding: 10px 14px;
    background: var(--accent-muted);
    border: 1px solid var(--accent-border);
    border-radius: var(--radius-sm);
    font-size: 0.85rem;
    color: var(--accent-primary-hover);
  }

  .selection-layout {
    display: grid;
    gap: 28px;
    align-items: start;
  }

  .selection-layout.has-episodes {
    grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
  }

  .episodes-section, .streams-section {
    min-width: 0;
  }

  .streams-section {
    background: var(--bg-surface);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-lg);
    padding: 24px;
    box-shadow: none;
    scroll-margin: 24px;
  }

  .episodes-section h3, .streams-section h3 {
    font-size: 1.15rem;
    overflow-wrap: anywhere;
  }

  .section-heading { margin-bottom: 20px; }
  .section-heading p, .no-streams, .source-empty {
    color: var(--text-secondary);
    font-size: 0.86rem;
    margin-top: 6px;
  }

  .selection-hint {
    align-self: center;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 10px;
    padding: 48px 24px;
    color: var(--text-secondary);
    text-align: center;
    background: var(--bg-surface);
    border: 1px dashed var(--border-subtle);
    border-radius: var(--radius-lg);
  }

  .selection-hint p { color: var(--text-primary); font-weight: 600; margin-top: 8px; }
  .selection-hint span { font-size: 0.85rem; }

  .loading-box { display: flex; align-items: center; gap: 12px; min-height: 96px; color: var(--text-secondary); }

  button:focus-visible { outline: 2px solid var(--accent-primary); outline-offset: 2px; }
  button:disabled { opacity: 0.5; cursor: wait; }

  .retry-btn {
    margin-top: 12px;
    padding: 8px 16px;
    color: var(--text-primary);
    background: var(--bg-surface-hover);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
  }

  .season-tabs {
    display: flex;
    gap: 8px;
    overflow-x: auto;
    margin-bottom: 16px;
    border-bottom: 1px solid var(--border-subtle);
    padding-bottom: 10px;
  }

  .season-tab {
    flex-shrink: 0;
    padding: 6px 14px;
    background: transparent;
    color: var(--text-secondary);
    border-radius: var(--radius-sm);
    font-size: 0.85rem;
    font-weight: 500;
  }

  .season-tab.active {
    background: var(--bg-surface-active);
    color: var(--text-primary);
    font-weight: 600;
  }

  .episode-list {
    display: flex;
    flex-direction: column;
    gap: 4px;
    max-height: 480px;
    overflow-y: auto;
    padding: 4px;
  }

  .episode-row {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 12px 14px;
    background: transparent;
    border: 1px solid transparent;
    border-radius: var(--radius-md);
    color: var(--text-secondary);
    text-align: left;
    transition: background-color var(--transition-fast), border-color var(--transition-fast), color var(--transition-fast);
  }

  .episode-row:hover {
    background: var(--bg-surface-hover);
    color: var(--text-primary);
    border-color: var(--border-subtle);
  }

  .episode-row.active {
    background: var(--bg-surface-active);
    border-color: var(--border-focus);
    color: var(--text-primary);
  }

  .episode-row.last-played {
    border-color: color-mix(in srgb, var(--accent-primary) 55%, transparent);
  }

  .ep-number {
    flex-shrink: 0;
    width: 32px;
    height: 32px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    background: var(--bg-secondary);
    border-radius: var(--radius-sm);
    font-size: 0.82rem;
    font-weight: 700;
    color: var(--text-secondary);
    font-variant-numeric: tabular-nums;
  }

  .episode-row.active .ep-number {
    background: var(--action-fill);
    color: #fff;
  }

  .ep-title {
    flex: 1;
    min-width: 0;
    font-size: 0.9rem;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .last-played-label {
    flex-shrink: 0;
    color: var(--accent-primary);
    font-size: 0.7rem;
    font-weight: 700;
  }

  :global(.ep-select-icon) {
    flex-shrink: 0;
    color: var(--text-muted);
    opacity: 0.65;
    transition: opacity var(--transition-fast);
  }

  .episode-row:hover :global(.ep-select-icon),
  .episode-row.active :global(.ep-select-icon) {
    opacity: 1;
  }

  .recommended-banner {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 16px;
    flex-wrap: wrap;
    padding: 18px 20px;
    background: linear-gradient(125deg, rgba(10, 132, 255, 0.16), var(--bg-surface) 62%);
    border: 1px solid var(--accent-border);
    border-radius: var(--radius-md);
    box-shadow: none;
  }

  .stream-title-row {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
    margin-bottom: 6px;
  }

  .stream-name {
    overflow-wrap: anywhere;
    white-space: pre-line;
    font-weight: 600;
    font-size: 0.95rem;
  }

  .stream-stats {
    display: flex;
    gap: 14px;
    flex-wrap: wrap;
    font-size: 0.8rem;
    color: var(--text-secondary);
  }

  .stream-stats span {
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }

  .stream-actions {
    display: flex;
    gap: 8px;
    margin-left: auto;
  }

  .btn-play {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 10px 22px;
    background: var(--action-fill);
    color: #fff;
    font-weight: 650;
    font-size: 0.92rem;
    border-radius: var(--radius-sm);
    box-shadow: 0 4px 12px rgba(10, 132, 255, 0.18);
  }

  .btn-play:hover:not(:disabled) {
    background: var(--action-fill-hover);
    box-shadow: 0 6px 16px rgba(10, 132, 255, 0.22);
    transform: translateY(-1px);
  }

  .toggle-sources-btn {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    margin-top: 16px;
    padding: 8px 12px;
    background: var(--bg-surface-elevated);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    color: var(--text-secondary);
    font-size: 0.85rem;
    font-weight: 500;
  }

  .toggle-sources-btn:hover {
    color: var(--text-primary);
    background: var(--bg-surface-hover);
  }

  .sources-list {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin-top: 12px;
  }

  .stream-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 12px;
    flex-wrap: wrap;
    padding: 12px 16px;
    background: var(--bg-secondary);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    transition: background-color var(--transition-fast), border-color var(--transition-fast);
  }

  .stream-row:hover {
    background: var(--bg-surface-elevated);
    border-color: rgba(255, 255, 255, 0.14);
  }

  .stream-details {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-wrap: wrap;
    min-width: 0;
    font-size: 0.85rem;
  }

  .stream-seeders, .stream-size {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    color: var(--text-secondary);
    font-size: 0.8rem;
  }

  .stream-title { overflow-wrap: anywhere; white-space: pre-line; }
  .stream-row-actions { margin-left: auto; }

  .btn-play-sm {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 32px;
    height: 32px;
    background: var(--bg-surface-active);
    color: var(--text-primary);
    border-radius: var(--radius-full);
  }

  .btn-play-sm:hover:not(:disabled) {
    background: var(--action-fill);
    color: #fff;
  }


  :global(.spinner-icon) {
    animation: spin 1s linear infinite;
  }

  @keyframes spin {
    from { transform: rotate(0deg); }
    to { transform: rotate(360deg); }
  }

  .poster-placeholder {
    width: 100%;
    height: 100%;
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--bg-surface);
    color: var(--text-muted);
  }

  .status-row,
  .lists-row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    margin-top: 14px;
  }

  .status-clear {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 30px;
    height: 30px;
    background: transparent;
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    color: var(--text-muted);
    transition: all var(--transition-fast);
  }

  .status-chip {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    padding: 7px 13px;
    background: var(--bg-surface-elevated);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    color: var(--text-secondary);
    font-size: 0.82rem;
    font-weight: 600;
    transition: all var(--transition-fast);
  }

  .status-chip:hover:not(.active),
  .status-clear:hover {
    color: var(--text-primary);
    border-color: var(--border-muted);
  }

  .status-chip.active {
    color: #0c0e14;
    border-color: transparent;
    box-shadow: 0 0 14px -2px var(--accent-glow);
  }

  .status-chip.active.to_watch { background: var(--status-blue, #60a5fa); }
  .status-chip.active.in_progress { background: var(--accent-primary); }
  .status-chip.active.watched { background: var(--status-green, #34d399); }

  .list-chip {
    padding: 4px 10px;
    border-radius: var(--radius-sm);
    background: var(--accent-muted);
    border: 1px solid var(--accent-border);
    color: var(--accent-primary);
    font-size: 0.78rem;
    font-weight: 600;
  }

  @media (max-width: 800px) {
    .selection-layout.has-episodes { grid-template-columns: minmax(0, 1fr); }
    .selection-hint { display: none; }
    .detail-header { gap: 16px; flex-direction: column; align-items: flex-start; }
    .detail-poster { width: 90px; }
    .detail-meta { min-width: 0; }
    .detail-meta h2 { font-size: 1.35rem; overflow-wrap: anywhere; }
    .streams-section { padding: 18px; }
  }

  @media (prefers-reduced-motion: reduce) {
    :global(.spinner-icon) { animation: none; }
    button { transition: none; }
  }
</style>
