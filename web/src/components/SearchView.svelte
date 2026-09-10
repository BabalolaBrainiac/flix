<script lang="ts">
  import { onDestroy, tick } from 'svelte';
  import { getEpisodes, getStreams, play } from '../lib/api';
  import { searchProgressively } from '../lib/search';
  import type {
    CatalogItemSummary,
    EpisodeSummary,
    StreamSummary,
    PlayCommand,
  } from '../lib/types';
  import { Search, Film, Tv, Sparkles, Play, ArrowLeft, Loader2, AlertCircle, ChevronRight, ChevronDown } from 'lucide-svelte';

  export let onPlayStarted: () => void;

  let query = '';
  let activeFilter: 'all' | 'movie' | 'series' | 'anime' = 'all';
  let isLoading = false;
  let searchRequest = 0;
  let searchAbort: AbortController | null = null;
  let pendingCatalogs: string[] = [];
  let hasSearched = false;
  let errorMsg = '';
  let items: CatalogItemSummary[] = [];
  let notes: string[] = [];

  // Selected item full-width detail view state
  let selectedItem: CatalogItemSummary | null = null;
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

  function clearStreams() {
    streamRequest += 1;
    streamAbort?.abort();
    streamAbort = null;
    streams = [];
    streamError = '';
    isLoadingStreams = false;
    showAllSources = false;
  }

  onDestroy(() => {
    detailRequest += 1;
    searchRequest += 1;
    searchAbort?.abort();
    clearStreams();
  });

  async function handleSearch() {
    if (!query.trim()) return;
    const request = ++searchRequest;
    searchAbort?.abort();
    searchAbort = new AbortController();
    isLoading = true;
    hasSearched = true;
    items = [];
    failedPosters = new Set();
    pendingCatalogs = ['Movies', 'Series', 'Anime'];
    errorMsg = '';
    notes = [];
    returnToSearch();
    try {
      await searchProgressively(query.trim(), searchAbort.signal, (res, pending) => {
        if (request !== searchRequest) return;
        items = res.items;
        notes = res.notes;
        pendingCatalogs = pending;
      });
    } catch (e: any) {
      if (request !== searchRequest) return;
      errorMsg = e.message || 'Search failed';
    } finally {
      if (request === searchRequest) isLoading = false;
    }
  }

  $: filteredItems = items.filter((item) => {
    if (activeFilter === 'all') return true;
    if (activeFilter === 'anime') return item.is_anime;
    if (activeFilter === 'movie') return item.media_type === 'movie' && !item.is_anime;
    if (activeFilter === 'series') return item.media_type === 'series' && !item.is_anime;
    return true;
  });

  $: seasons = Array.from(new Set(episodes.map((e) => e.season))).sort((a, b) => a - b);
  $: seasonEpisodes = episodes.filter((e) => e.season === activeSeason);
  $: needsEpisode = selectedItem && selectedItem.media_type !== 'movie';
  $: showStreams = selectedItem && (!needsEpisode || selectedEpisode !== null);

  async function selectItem(item: CatalogItemSummary) {
    const request = ++detailRequest;
    clearStreams();
    selectedItem = item;
    episodes = [];
    selectedEpisode = null;
    streams = [];
    showAllSources = false;
    isLoadingEpisodes = false;
    isLoadingStreams = false;
    actionMessage = '';

    try {
      if (item.media_type !== 'movie') {
        isLoadingEpisodes = true;
        const epRes = await getEpisodes(item.id, item.is_anime);
        if (request !== detailRequest) return;
        episodes = epRes.episodes;
        if (epRes.is_anime) selectedItem = { ...item, is_anime: true };
        if (episodes.length > 0) {
          activeSeason = episodes[0].season;
        }
      } else {
        isLoadingStreams = true;
        streamAbort = new AbortController();
        const streamRes = await getStreams('movie', item.id, false, streamAbort.signal);
        if (request !== detailRequest) return;
        streams = streamRes.streams;
        if (streamRes.is_anime) selectedItem = { ...item, is_anime: true };
      }
    } catch (e: any) {
      if (request !== detailRequest) return;
      actionMessage = `Failed to load details: ${e.message}`;
    } finally {
      if (request !== detailRequest) return;
      isLoadingEpisodes = false;
      if (item.media_type === 'movie') isLoadingStreams = false;
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
      const streamRes = await getStreams('series', episode.stream_id || episode.id, selectedItem?.is_anime ?? false, signal);
      if (request !== streamRequest) return;
      streams = streamRes.streams;
      if (streamRes.is_anime && selectedItem) selectedItem = { ...selectedItem, is_anime: true };
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

  function returnToSearch() {
    detailRequest += 1;
    clearStreams();
    selectedItem = null;
    selectedEpisode = null;
    actionMessage = '';
    episodes = [];
    streams = [];
    isLoadingEpisodes = false;
    isLoadingStreams = false;
  }

  $: recommendedStream = streams.find((s) => s.is_recommended) || streams[0];
  $: otherStreams = streams.filter((s) => s !== recommendedStream);

  let playbackTarget: 'external' | 'browser' = 'external';
  $: if (selectedItem?.is_anime && playbackTarget === 'browser') playbackTarget = 'external';

  async function handlePlayStream(stream: StreamSummary) {
    if (isPreparingPlayback || !selectedItem || (needsEpisode && !selectedEpisode)) return;
    const request = streamRequest;
    isPreparingPlayback = true;
    actionMessage = 'Preparing playback…';
    try {
      const command: PlayCommand = {
        target: playbackTarget,
        source_id: stream.source_id,
        file_index: stream.file_index,
        media_ref:
          selectedItem?.media_type === 'movie'
            ? {
                type: 'movie',
                catalog_id: selectedItem.id,
                title: selectedItem.name,
              }
            : selectedItem && selectedEpisode
              ? {
                  type: 'episode',
                  catalog_id: selectedItem.id,
                  stream_id: selectedEpisode.stream_id || selectedEpisode.id,
                  imdb_id: selectedEpisode.imdb_id,
                  season: selectedEpisode.season,
                  episode: selectedEpisode.episode,
                  title: selectedEpisode.title,
                }
              : undefined,
        queue_seed:
          selectedItem && episodes.length > 0 && selectedEpisode
            ? {
                catalog_item: selectedItem,
                episodes,
                current_episode_id: selectedEpisode.id,
              }
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

  // Poster URLs that failed to load. A broken image falls back to the film
  // placeholder instead of showing the browser's broken-image icon.
  let failedPosters = new Set<string>();
  function markPosterFailed(url: string | undefined) {
    if (!url) return;
    failedPosters = new Set(failedPosters).add(url);
  }

  function formatEpisodeTitle(ep: EpisodeSummary): string {
    const code = `S${String(ep.season).padStart(2, '0')}E${String(ep.episode).padStart(2, '0')}`;
    const t = ep.title?.trim();
    // Show only the episode code when there is no title. A "Title unavailable"
    // label adds noise without adding information.
    return t ? `${code} · ${t}` : code;
  }

  function sourceName(stream: StreamSummary): string {
    return stream.name.split('\n')
      .map(line => line.trim())
      .filter(line => line && line.toLowerCase() !== stream.quality.toLowerCase())
      .join(' · ') || stream.name;
  }
</script>

<div class="search-view">
  {#if !selectedItem}
    <div class="search-bar-section">
      <form class="search-form" on:submit|preventDefault={handleSearch}>
        <div class="search-input-wrapper">
          <Search size={18} class="search-icon" />
          <input
            type="text"
            bind:value={query}
            maxlength={200}
            placeholder="Search movies, TV shows, and anime..."
            class="search-input"
          />
          {#if isLoading}
            <Loader2 size={18} class="spinner-icon" />
          {/if}
        </div>
        <button type="submit" class="search-btn" disabled={!query.trim()}>
          Search
        </button>
      </form>

      <div class="filter-pills">
        <button
          class="pill {activeFilter === 'all' ? 'active' : ''}"
          on:click={() => { activeFilter = 'all'; }}
        >
          All
        </button>
        <button
          class="pill {activeFilter === 'movie' ? 'active' : ''}"
          on:click={() => { activeFilter = 'movie'; }}
        >
          <Film size={14} /> Movies
        </button>
        <button
          class="pill {activeFilter === 'series' ? 'active' : ''}"
          on:click={() => { activeFilter = 'series'; }}
        >
          <Tv size={14} /> Series
        </button>
        <button
          class="pill {activeFilter === 'anime' ? 'active' : ''}"
          on:click={() => { activeFilter = 'anime'; }}
        >
          <Sparkles size={14} /> Anime
        </button>
      </div>
    </div>

    {#if errorMsg}
      <div class="error-banner">
        <AlertCircle size={16} />
        <span>{errorMsg}</span>
      </div>
    {/if}

    {#if isLoading || notes.length > 0}
      <div class="search-status" role="status" aria-live="polite">
        {#if isLoading}<p>Searching {pendingCatalogs.join(', ')}… You can open a result now.</p>{/if}
        {#each notes as note}<p>{note}</p>{/each}
      </div>
    {/if}

    {#if filteredItems.length > 0}
      <div class="poster-grid">
        {#each filteredItems as item (item.id)}
          <button class="grid-card" on:click={() => selectItem(item)}>
            <div class="poster-card">
              {#if item.poster && !failedPosters.has(item.poster)}
                <img
                  src={item.poster}
                  alt={item.name}
                  loading="lazy"
                  on:error={() => markPosterFailed(item.poster)}
                />
              {:else}
                <div class="poster-placeholder">
                  <Film size={32} />
                </div>
              {/if}
              <div class="poster-badge-top">
                {#if item.is_anime}
                  <span class="badge badge-recommended">Anime</span>
                {:else if item.media_type === 'movie'}
                  <span class="badge">Movie</span>
                {:else}
                  <span class="badge">Series</span>
                {/if}
              </div>
            </div>
            <div class="card-info">
              <div class="card-title" title={item.name}>{item.name}</div>
              {#if item.release_info}
                <div class="card-meta">{item.release_info}</div>
              {/if}
            </div>
          </button>
        {/each}
      </div>
    {:else if !isLoading && hasSearched && query}
      <div class="empty-state">
        <p>No results found for "{query}".</p>
      </div>
    {/if}
  {:else}
    <!-- Full-Width Detail View -->
    <div class="detail-view">
      <button class="back-btn" on:click={returnToSearch} disabled={isPreparingPlayback}>
        <ArrowLeft size={16} /> Back to results
      </button>

      <div class="detail-header">
        <div class="detail-poster">
          {#if selectedItem.poster && !failedPosters.has(selectedItem.poster)}
            <img
              src={selectedItem.poster}
              alt={selectedItem.name}
              on:error={() => markPosterFailed(selectedItem?.poster)}
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
            <p>Select an episode to see its sources.</p>
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
                class="episode-row {selectedEpisode?.id === ep.id ? 'active' : ''}"
                on:click={() => selectEpisode(ep)}
                aria-pressed={selectedEpisode?.id === ep.id}
                aria-controls="selected-sources"
                disabled={isPreparingPlayback}
              >
                <span class="ep-number">{ep.episode}</span>
                <span class="ep-title">{ep.title?.trim() || `Episode ${ep.episode}`}</span>
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
          <h3 id="sources-heading">{selectedEpisode ? formatEpisodeTitle(selectedEpisode) : 'Choose a source'}</h3>
          <label class="player-choice">Play with
            <select bind:value={playbackTarget} disabled={isPreparingPlayback}>
              <option value="external">External player</option>
              <option value="browser" disabled={selectedItem?.is_anime}>Browser preview</option>
            </select>
          </label>
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
                  <span class="badge badge-recommended">Recommended</span>
                  <span class="badge {recommendedStream.quality === '4K' ? 'badge-4k' : 'badge-1080p'}">
                    {recommendedStream.quality}
                  </span>
                  <span class="stream-name" title={recommendedStream.name}>{sourceName(recommendedStream)}</span>
                </div>
                <div class="stream-stats">
                  {#if recommendedStream.seeders !== undefined}
                    <span>Seeds: {recommendedStream.seeders}</span>
                  {/if}
                  {#if recommendedStream.size}
                    <span>Size: {recommendedStream.size}</span>
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
                          <span class="stream-seeders">{s.seeders} seeds</span>
                        {/if}
                        {#if s.size}
                          <span class="stream-size">{s.size}</span>
                        {/if}
                      </div>
                      <div class="stream-row-actions">
                        <button
                          class="btn-play-sm"
                          on:click={() => handlePlayStream(s)}
                          disabled={isPreparingPlayback}
                        >
                          <Play size={14} /> Play
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
  {/if}
</div>

<style>
  .search-status { color: var(--text-secondary); font-size: 0.875rem; margin-bottom: 1rem; }
  .search-status p { margin: 0.4rem 0; }
  .player-choice { display: flex; align-items: center; flex-wrap: wrap; gap: 8px; margin: 8px 0; color: var(--text-secondary); font-size: .85rem; }
  .player-choice select { padding: 8px; background: var(--bg-surface); color: var(--text-primary); border: 1px solid var(--border-subtle); border-radius: var(--radius-sm); }

  .search-view {
    max-width: 1280px;
    margin: 0 auto;
    padding: 24px;
  }

  .search-bar-section {
    display: flex;
    flex-direction: column;
    gap: 16px;
    margin-bottom: 28px;
  }

  .search-form {
    display: flex;
    gap: 12px;
  }

  .search-input-wrapper {
    flex: 1;
    position: relative;
    display: flex;
    align-items: center;
  }

  :global(.search-icon) {
    position: absolute;
    left: 14px;
    color: var(--text-muted);
  }

  :global(.spinner-icon) {
    animation: spin 1s linear infinite;
  }

  @keyframes spin {
    from { transform: rotate(0deg); }
    to { transform: rotate(360deg); }
  }

  .search-input {
    width: 100%;
    height: 48px;
    padding: 0 44px;
    background: var(--bg-surface);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    color: var(--text-primary);
    font-size: 0.95rem;
  }

  .search-input:focus {
    border-color: var(--border-focus);
  }

  .search-btn {
    padding: 0 24px;
    height: 48px;
    background: var(--accent-primary);
    color: #fff;
    font-weight: 600;
    border-radius: var(--radius-md);
  }

  .search-btn:hover:not(:disabled) {
    background: var(--accent-primary-hover);
  }

  .search-btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .filter-pills {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }

  .pill {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 14px;
    background: var(--bg-surface);
    color: var(--text-secondary);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    font-size: 0.85rem;
    font-weight: 500;
  }

  .pill.active {
    background: var(--bg-surface-active);
    color: var(--text-primary);
    border-color: var(--border-focus);
  }

  .error-banner {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 12px 16px;
    background: rgba(239, 68, 68, 0.15);
    border: 1px solid rgba(239, 68, 68, 0.3);
    border-radius: var(--radius-md);
    color: #fca5a5;
    margin-bottom: 20px;
  }

  .grid-card {
    background: transparent;
    display: flex;
    flex-direction: column;
    text-align: left;
    cursor: pointer;
  }

  .poster-placeholder {
    width: 100%;
    height: 100%;
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--bg-secondary);
    color: var(--text-muted);
  }

  .poster-badge-top {
    position: absolute;
    top: 8px;
    left: 8px;
  }

  .card-info {
    margin-top: 8px;
  }

  .card-title {
    font-size: 0.9rem;
    font-weight: 600;
    color: var(--text-primary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .card-meta {
    font-size: 0.75rem;
    color: var(--text-muted);
    margin-top: 2px;
  }

  .empty-state {
    text-align: center;
    padding: 48px;
    color: var(--text-muted);
  }

  /* Full-width detail view */
  .detail-view {
    display: flex;
    flex-direction: column;
    gap: 24px;
  }

  .back-btn {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    background: transparent;
    color: var(--text-secondary);
    font-size: 0.9rem;
    padding: 8px 0;
    align-self: flex-start;
  }

  .back-btn:hover {
    color: var(--text-primary);
  }

  .detail-header {
    display: flex;
    gap: 24px;
    align-items: center;
    padding-bottom: 8px;
  }

  .detail-poster {
    width: 112px;
    aspect-ratio: 2 / 3;
    flex-shrink: 0;
    border-radius: var(--radius-sm);
    overflow: hidden;
    background: var(--bg-secondary);
  }

  .detail-poster img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }

  .detail-meta h2 {
    font-size: 1.5rem;
    margin-bottom: 8px;
  }

  .meta-row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    margin-bottom: 12px;
  }

  .meta-tag {
    font-size: 0.85rem;
    color: var(--text-muted);
  }

  .action-alert {
    padding: 8px 12px;
    background: var(--bg-surface-active);
    border-radius: var(--radius-sm);
    font-size: 0.85rem;
    color: var(--status-amber);
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
    border-radius: 12px;
    padding: 20px;
    scroll-margin: 24px;
  }

  .episodes-section h3, .streams-section h3 {
    font-size: 1.1rem;
    overflow-wrap: anywhere;
  }

  .section-heading { margin-bottom: 18px; }
  .section-heading p, .no-streams, .source-empty {
    color: var(--text-secondary);
    font-size: 0.85rem;
    margin-top: 6px;
  }

  .selection-hint {
    align-self: center;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    padding: 40px 20px;
    color: var(--text-secondary);
    text-align: center;
  }

  .selection-hint p { color: var(--text-primary); margin-top: 8px; }
  .selection-hint span { font-size: 0.85rem; }
  .loading-box { display: flex; align-items: center; gap: 12px; min-height: 96px; color: var(--text-secondary); }

  button:focus-visible { outline: 2px solid var(--text-primary); outline-offset: 3px; }
  button:disabled { opacity: 0.55; cursor: wait; }

  .retry-btn {
    margin-top: 12px;
    padding: 8px 14px;
    color: var(--text-primary);
    background: var(--bg-surface-hover);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
  }

  .season-tabs {
    display: flex;
    gap: 8px;
    overflow-x: auto;
    margin-bottom: 14px;
    border-bottom: 1px solid var(--border-subtle);
    padding-bottom: 8px;
  }

  .season-tab {
    flex-shrink: 0;
    padding: 6px 12px;
    background: transparent;
    color: var(--text-secondary);
    border-radius: var(--radius-sm);
    font-size: 0.85rem;
  }

  .season-tab.active {
    background: var(--bg-surface-active);
    color: var(--text-primary);
  }

  .episode-list {
    display: flex;
    flex-direction: column;
    gap: 2px;
    max-height: 460px;
    overflow-y: auto;
    padding: 4px;
  }

  .episode-row {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 12px;
    background: transparent;
    border: 1px solid transparent;
    border-radius: var(--radius-md);
    color: var(--text-secondary);
    text-align: left;
    transition: background var(--transition-fast), color var(--transition-fast);
  }

  .episode-row:hover {
    background: var(--bg-surface-hover);
    color: var(--text-primary);
  }

  .episode-row.active {
    background: var(--bg-surface-active);
    border-color: var(--border-subtle);
    color: var(--text-primary);
  }

  /* Fixed-size chip showing only the episode number. The season is already
     set by the season tab, so the row does not repeat it. */
  .ep-number {
    flex-shrink: 0;
    width: 30px;
    height: 30px;
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
    background: var(--bg-surface-hover);
    color: var(--text-primary);
  }

  .ep-title {
    flex: 1;
    min-width: 0;
    font-size: 0.9rem;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
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
    padding: 16px;
    background: var(--bg-secondary);
    border-radius: var(--radius-md);
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
    gap: 16px;
    flex-wrap: wrap;
    font-size: 0.8rem;
    color: var(--text-secondary);
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
    padding: 8px 18px;
    background: var(--accent-primary);
    color: #fff;
    font-weight: 600;
    border-radius: var(--radius-sm);
  }

  .btn-play:hover:not(:disabled) {
    background: var(--accent-primary-hover);
  }

  .toggle-sources-btn {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    margin-top: 14px;
    padding: 8px 0;
    background: transparent;
    color: var(--text-secondary);
    font-size: 0.85rem;
  }

  .sources-list {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin-top: 10px;
  }

  .stream-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 12px;
    flex-wrap: wrap;
    padding: 10px 14px;
    background: var(--bg-secondary);
    border-radius: var(--radius-sm);
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
    color: var(--text-secondary);
  }

  .stream-title { overflow-wrap: anywhere; white-space: pre-line; }
  .stream-row-actions { margin-left: auto; }

  .btn-play-sm {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 8px 12px;
    background: var(--bg-surface-active);
    color: var(--text-primary);
    font-size: 0.8rem;
    border-radius: var(--radius-sm);
  }

  @media (max-width: 800px) {
    .selection-layout.has-episodes { grid-template-columns: minmax(0, 1fr); }
    .selection-hint { display: none; }
    .search-view { padding: 20px 16px; }
    .detail-header { gap: 16px; }
    .detail-poster { width: 80px; }
    .detail-meta { min-width: 0; }
    .detail-meta h2 { font-size: 1.25rem; overflow-wrap: anywhere; }
    .streams-section { padding: 16px; }
  }

  @media (prefers-reduced-motion: reduce) {
    :global(.spinner-icon) { animation: none; }
    button { transition: none; }
  }
</style>
