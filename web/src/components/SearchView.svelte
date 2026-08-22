<script lang="ts">
  import { search, getEpisodes, getStreams, play } from '../lib/api';
  import type {
    CatalogItemSummary,
    EpisodeSummary,
    StreamSummary,
    PlayCommand,
  } from '../lib/types';
  import { Search, Film, Tv, Sparkles, Play, ArrowLeft, Loader2, AlertCircle } from 'lucide-svelte';

  export let onPlayStarted: () => void;

  let query = '';
  let activeFilter: 'all' | 'movie' | 'series' | 'anime' = 'all';
  let isLoading = false;
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
  let isLoadingDetails = false;
  let actionMessage = '';
  let isPreparingPlayback = false;

  async function handleSearch() {
    if (!query.trim()) return;
    isLoading = true;
    errorMsg = '';
    notes = [];
    selectedItem = null;
    try {
      const isAnimeOnly = activeFilter === 'anime';
      const res = await search(query.trim(), isAnimeOnly);
      items = res.items;
      notes = res.notes;
    } catch (e: any) {
      errorMsg = e.message || 'Search failed';
    } finally {
      isLoading = false;
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

  async function selectItem(item: CatalogItemSummary) {
    selectedItem = item;
    episodes = [];
    selectedEpisode = null;
    streams = [];
    showAllSources = false;
    isLoadingDetails = true;
    actionMessage = '';

    try {
      if (item.media_type === 'series' || item.is_anime) {
        const epRes = await getEpisodes(item.id, item.is_anime);
        episodes = epRes.episodes;
        if (episodes.length > 0) {
          activeSeason = episodes[0].season;
          await selectEpisode(episodes[0]);
        }
      } else {
        const streamRes = await getStreams('movie', item.id);
        streams = streamRes.streams;
      }
    } catch (e: any) {
      actionMessage = `Failed to load details: ${e.message}`;
    } finally {
      isLoadingDetails = false;
    }
  }

  async function selectEpisode(episode: EpisodeSummary) {
    selectedEpisode = episode;
    isLoadingDetails = true;
    streams = [];
    showAllSources = false;
    try {
      const streamRes = await getStreams('series', episode.stream_id || episode.id);
      streams = streamRes.streams;
    } catch (e: any) {
      actionMessage = `Failed to load streams: ${e.message}`;
    } finally {
      isLoadingDetails = false;
    }
  }

  $: recommendedStream = streams.find((s) => s.is_recommended) || streams[0];
  $: otherStreams = streams.filter((s) => s !== recommendedStream);

  async function handlePlayStream(stream: StreamSummary) {
    isPreparingPlayback = true;
    actionMessage = 'Starting playback pipeline...';
    try {
      const command: PlayCommand = {
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
      onPlayStarted();
    } catch (e: any) {
      actionMessage = `Playback failed: ${e.message}`;
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
            placeholder="Search movies, TV shows, and anime..."
            class="search-input"
          />
          {#if isLoading}
            <Loader2 size={18} class="spinner-icon" />
          {/if}
        </div>
        <button type="submit" class="search-btn" disabled={isLoading || !query.trim()}>
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
    {:else if !isLoading && query}
      <div class="empty-state">
        <p>No results found for "{query}".</p>
      </div>
    {/if}
  {:else}
    <!-- Full-Width Detail View -->
    <div class="detail-view">
      <button class="back-btn" on:click={() => { selectedItem = null; }}>
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

      {#if episodes.length > 0}
        <div class="episodes-section">
          <h3>Episodes</h3>
          {#if seasons.length > 1}
            <div class="season-tabs">
              {#each seasons as s}
                <button
                  class="season-tab {activeSeason === s ? 'active' : ''}"
                  on:click={() => { activeSeason = s; }}
                >
                  Season {s}
                </button>
              {/each}
            </div>
          {/if}

          <div class="episode-list">
            {#each seasonEpisodes as ep (ep.id)}
              <button
                class="episode-row {selectedEpisode?.id === ep.id ? 'active' : ''}"
                on:click={() => selectEpisode(ep)}
              >
                <span class="ep-number">{ep.episode}</span>
                <span class="ep-title">{ep.title?.trim() || `Episode ${ep.episode}`}</span>
                <Play size={14} class="ep-play-icon" />
              </button>
            {/each}
          </div>
        </div>
      {/if}

      <div class="streams-section">
        <h3>Available Streams</h3>
        {#if isLoadingDetails}
          <div class="loading-box">
            <Loader2 size={24} class="spinner-icon" />
            <span>Finding verified streams...</span>
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
                  <span class="stream-name" title={recommendedStream.name}>{recommendedStream.name}</span>
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
                  <Play size={16} /> Play
                </button>
              </div>
            </div>
          {/if}

          {#if otherStreams.length > 0}
            <div class="other-sources">
              <button
                class="toggle-sources-btn"
                on:click={() => { showAllSources = !showAllSources; }}
              >
                {showAllSources ? 'Hide Alternative Sources' : `Show ${otherStreams.length} Alternative Sources`}
              </button>

              {#if showAllSources}
                <div class="sources-list">
                  {#each otherStreams as s (s.source_id)}
                    <div class="stream-row">
                      <div class="stream-details">
                        <span class="badge {s.quality === '4K' ? 'badge-4k' : 'badge-1080p'}">{s.quality}</span>
                        <span class="stream-title" title={s.name}>{s.name}</span>
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
          <p class="no-streams">No streams found for this selection.</p>
        {/if}
      </div>
    </div>
  {/if}
</div>

<style>
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
    background: var(--bg-surface);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    padding: 24px;
  }

  .detail-poster {
    width: 160px;
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

  .episodes-section, .streams-section {
    background: var(--bg-surface);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    padding: 20px;
  }

  .episodes-section h3, .streams-section h3 {
    font-size: 1.1rem;
    margin-bottom: 14px;
  }

  .season-tabs {
    display: flex;
    gap: 8px;
    margin-bottom: 14px;
    border-bottom: 1px solid var(--border-subtle);
    padding-bottom: 8px;
  }

  .season-tab {
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
    max-height: 380px;
    overflow-y: auto;
  }

  .episode-row {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 9px 12px;
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
    background: var(--accent-primary);
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

  /* lucide renders an svg in a child component, so Svelte's scoped class does
     not reach it. Target it globally, the same way .spinner-icon is handled. */
  :global(.ep-play-icon) {
    flex-shrink: 0;
    color: var(--text-muted);
    opacity: 0;
    transition: opacity var(--transition-fast);
  }

  .episode-row:hover :global(.ep-play-icon),
  .episode-row.active :global(.ep-play-icon) {
    opacity: 1;
  }

  .recommended-banner {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 16px;
    background: var(--bg-surface-active);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
  }

  .stream-title-row {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 6px;
  }

  .stream-name {
    font-weight: 600;
    font-size: 0.95rem;
  }

  .stream-stats {
    display: flex;
    gap: 16px;
    font-size: 0.8rem;
    color: var(--text-muted);
  }

  .stream-actions {
    display: flex;
    gap: 8px;
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
    margin-top: 14px;
    padding: 8px 12px;
    background: transparent;
    color: var(--text-secondary);
    font-size: 0.85rem;
    text-decoration: underline;
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
    padding: 10px 14px;
    background: var(--bg-secondary);
    border-radius: var(--radius-sm);
  }

  .stream-details {
    display: flex;
    align-items: center;
    gap: 10px;
    font-size: 0.85rem;
  }

  .stream-seeders, .stream-size {
    color: var(--text-muted);
  }

  .btn-play-sm {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 4px 10px;
    background: var(--bg-surface-active);
    color: var(--text-primary);
    font-size: 0.8rem;
    border-radius: var(--radius-sm);
  }
</style>
