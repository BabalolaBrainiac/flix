<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { getRecommendations } from '../lib/api';
  import { searchProgressively } from '../lib/search';
  import { openTitle } from '../lib/titles';
  import { membership, buildBadgeIndex, badgeFor, titleRefOf } from '../lib/library';
  import type { CatalogItemSummary, RecommendItem, TitleRef } from '../lib/types';
  import { Search, Film, Tv, Sparkles, Loader2, AlertCircle, X, Bookmark, Star } from 'lucide-svelte';
  import AddToListPopover from './AddToListPopover.svelte';
  import PosterMarks from './PosterMarks.svelte';

  export let activeProfileId: string | null = null;
  export let onOpenProfilePicker: () => void = () => {};

  const TRENDING_PAGE_SIZE = 20;
  const SEARCH_PAGE_SIZE = 24;

  const promptSuggestions = [
    'Cyberpunk Anime',
    'Sci-Fi Space Thriller',
    'Mind-Bending Mystery',
    'Post-Apocalyptic Survival',
    'Dark Crime Drama',
  ];

  // One search box drives two engines. `searchMode` says which results are
  // showing: a typed, submitted query always runs the real ranked catalog
  // search (below); the "Try:" mood chips always run the recommend engine
  // and never leave trending mode. Both fill the same box and the same grid
  // area, so this reads as one tab, not two.
  let keywords = '';
  let searchMode = false;
  let activeFilter: 'all' | 'movie' | 'series' | 'anime' = 'all';
  let errorMsg = '';
  let popoverRef: TitleRef | null = null;
  let searchInputEl: HTMLInputElement | null = null;
  let failedPosters = new Set<string>();

  $: badgeIndex = buildBadgeIndex($membership);

  function markPosterFailed(url: string | null | undefined) {
    if (!url) return;
    failedPosters = new Set(failedPosters).add(url);
  }

  // ---- Trending / mood recommend engine ----

  let trendingItems: RecommendItem[] = [];
  let hasMoodQuery = false;
  let isLoadingTrending = false;
  let isLoadingMoreTrending = false;
  let hasMoreTrending = true;

  function recommendCommand(): Parameters<typeof getRecommendations>[0] {
    const command: Parameters<typeof getRecommendations>[0] = {
      keywords: hasMoodQuery ? keywords.trim() : '',
      limit: TRENDING_PAGE_SIZE,
    };
    if (activeFilter === 'movie') command.movie = true;
    else if (activeFilter === 'series') command.show = true;
    else if (activeFilter === 'anime') command.anime = true;
    return command;
  }

  async function loadTrending() {
    isLoadingTrending = true;
    errorMsg = '';
    trendingItems = [];
    hasMoreTrending = true;
    try {
      const res = await getRecommendations(recommendCommand());
      trendingItems = res.items;
      hasMoreTrending = res.items.length === TRENDING_PAGE_SIZE;
    } catch (e: any) {
      errorMsg = e.message || 'Failed to load recommendations';
      trendingItems = [];
    } finally {
      isLoadingTrending = false;
    }
  }

  function pickSuggestion(text: string) {
    searchMode = false;
    keywords = text;
    hasMoodQuery = true;
    void loadTrending();
  }

  async function loadMoreTrending() {
    if (isLoadingMoreTrending || !hasMoreTrending) return;
    isLoadingMoreTrending = true;
    try {
      const known = new Set(trendingItems.map((item) => item.id));
      const res = await getRecommendations({ ...recommendCommand(), offset: trendingItems.length });
      const fresh = res.items.filter((item) => !known.has(item.id));
      trendingItems = [...trendingItems, ...fresh];
      hasMoreTrending = res.items.length === TRENDING_PAGE_SIZE && fresh.length > 0;
    } catch (e: any) {
      errorMsg = e.message || 'Failed to load more recommendations';
    } finally {
      isLoadingMoreTrending = false;
    }
  }

  function handleTrendingCardClick(item: RecommendItem) {
    openTitle({
      id: item.id,
      name: item.name,
      media_type: item.media_type,
      release_info: item.release_info ?? undefined,
      poster: item.poster ?? undefined,
      is_anime: item.is_anime,
    });
  }

  // ---- Real title search ----

  let searchQuery = '';
  let searchItems: CatalogItemSummary[] = [];
  let searchNotes: string[] = [];
  let isSearching = false;
  let searchRequest = 0;
  let searchAbort: AbortController | null = null;
  let pendingCatalogs: string[] = [];
  let searchVisibleCount = SEARCH_PAGE_SIZE;

  $: searchAllCount = searchItems.length;
  $: searchMovieCount = searchItems.filter((i) => i.media_type === 'movie' && !i.is_anime).length;
  $: searchSeriesCount = searchItems.filter((i) => i.media_type === 'series' && !i.is_anime).length;
  $: searchAnimeCount = searchItems.filter((i) => i.is_anime).length;
  $: filteredSearchItems = searchItems.filter((item) => {
    if (activeFilter === 'all') return true;
    if (activeFilter === 'anime') return item.is_anime;
    if (activeFilter === 'movie') return item.media_type === 'movie' && !item.is_anime;
    return item.media_type === 'series' && !item.is_anime;
  });
  $: visibleSearchItems = filteredSearchItems.slice(0, searchVisibleCount);
  $: remainingSearchItems = filteredSearchItems.length - visibleSearchItems.length;

  async function runSearch() {
    const trimmed = searchQuery.trim();
    if (!trimmed) return;
    const request = ++searchRequest;
    searchAbort?.abort();
    searchAbort = new AbortController();
    isSearching = true;
    searchItems = [];
    searchVisibleCount = SEARCH_PAGE_SIZE;
    failedPosters = new Set();
    pendingCatalogs = ['Movies', 'Series', 'Anime'];
    errorMsg = '';
    searchNotes = [];
    try {
      await searchProgressively(trimmed, searchAbort.signal, (res, pending) => {
        if (request !== searchRequest) return;
        searchItems = res.items;
        searchNotes = res.notes;
        pendingCatalogs = pending;
      });
    } catch (e: any) {
      if (request !== searchRequest) return;
      errorMsg = e.message || 'Search failed';
    } finally {
      if (request === searchRequest) isSearching = false;
    }
  }

  function showMoreSearch() {
    searchVisibleCount += SEARCH_PAGE_SIZE;
  }

  // ---- The shared box, filter pills, and mode switching ----

  function handleSearchSubmit() {
    const trimmed = keywords.trim();
    if (!trimmed) {
      clearSearch();
      return;
    }
    searchMode = true;
    searchQuery = trimmed;
    activeFilter = 'all';
    void runSearch();
  }

  function clearSearch() {
    searchMode = false;
    keywords = '';
    searchQuery = '';
    hasMoodQuery = false;
    activeFilter = 'all';
    void loadTrending();
    searchInputEl?.focus();
  }

  function selectFilter(filter: typeof activeFilter) {
    if (activeFilter === filter) return;
    activeFilter = filter;
    if (searchMode) {
      searchVisibleCount = SEARCH_PAGE_SIZE;
      return;
    }
    void loadTrending();
  }

  onMount(() => {
    if (trendingItems.length === 0 && !hasMoodQuery && !searchMode) {
      void loadTrending();
    }
    function handleKeydown(event: KeyboardEvent) {
      const typing = document.activeElement instanceof HTMLInputElement || document.activeElement instanceof HTMLTextAreaElement;
      if (event.key === '/' && !typing && !popoverRef && searchInputEl?.offsetParent) {
        event.preventDefault();
        searchInputEl.focus();
      }
    }
    window.addEventListener('keydown', handleKeydown);
    return () => window.removeEventListener('keydown', handleKeydown);
  });

  onDestroy(() => {
    searchRequest += 1;
    searchAbort?.abort();
  });
</script>

<div class="recommend-view">
  <div class="discover-bar-section">
    <form class="discover-form" on:submit|preventDefault={handleSearchSubmit}>
      <div class="discover-input-wrapper">
        <Search size={18} class="discover-icon" />
        <input
          bind:this={searchInputEl}
          type="text"
          bind:value={keywords}
          maxlength={200}
          placeholder="Search titles or moods"
          class="discover-input"
        />
        {#if searchMode}
          <button type="button" class="clear-btn" on:click={clearSearch} aria-label="Clear search">
            <X size={16} />
          </button>
        {/if}
        {#if isSearching || (isLoadingTrending && !searchMode)}
          <Loader2 size={18} class="spinner-icon" />
        {/if}
      </div>
      <button type="submit" class="discover-btn">
        Search
      </button>
    </form>

    <div class="filter-pills">
      <button
        type="button"
        class="pill {activeFilter === 'all' ? 'active' : ''}"
        on:click={() => selectFilter('all')}
      >
        All
        {#if searchMode && searchAllCount > 0}<span class="pill-count">{searchAllCount}</span>{/if}
      </button>
      <button
        type="button"
        class="pill {activeFilter === 'movie' ? 'active' : ''}"
        on:click={() => selectFilter('movie')}
      >
        <Film size={14} /> Movies
        {#if searchMode && searchMovieCount > 0}<span class="pill-count">{searchMovieCount}</span>{/if}
      </button>
      <button
        type="button"
        class="pill {activeFilter === 'series' ? 'active' : ''}"
        on:click={() => selectFilter('series')}
      >
        <Tv size={14} /> Shows
        {#if searchMode && searchSeriesCount > 0}<span class="pill-count">{searchSeriesCount}</span>{/if}
      </button>
      <button
        type="button"
        class="pill {activeFilter === 'anime' ? 'active' : ''}"
        on:click={() => selectFilter('anime')}
      >
        <Sparkles size={14} /> Anime
        {#if searchMode && searchAnimeCount > 0}<span class="pill-count">{searchAnimeCount}</span>{/if}
      </button>
    </div>

    {#if !searchMode}
      <div class="suggestion-chips">
        <span class="suggestion-label">Try:</span>
        {#each promptSuggestions as prompt}
          <button
            type="button"
            class="suggestion-chip"
            on:click={() => pickSuggestion(prompt)}
            disabled={isLoadingTrending}
          >
            {prompt}
          </button>
        {/each}
      </div>
    {/if}
  </div>

  {#if errorMsg}
    <div class="error-banner">
      <AlertCircle size={16} />
      <span>{errorMsg}</span>
    </div>
  {/if}

  {#if searchMode}
    {#if isSearching || searchNotes.length > 0}
      <div class="search-status" role="status" aria-live="polite">
        {#if isSearching}<p>Searching {pendingCatalogs.join(', ')}… You can open a result now.</p>{/if}
        {#each searchNotes as note}<p>{note}</p>{/each}
      </div>
    {/if}

    {#if visibleSearchItems.length > 0}
      <div class="poster-grid">
        {#each visibleSearchItems as item (item.id)}
          {@const badge = badgeFor(badgeIndex, item.id)}
          <div class="grid-card">
            <div class="poster-card">
              <button
                type="button"
                class="poster-click-area"
                on:click={() => openTitle(item)}
                aria-label={item.name}
              >
                {#if item.poster && !failedPosters.has(item.poster)}
                  <img
                    src={item.poster}
                    alt={item.name}
                    loading="lazy"
                    decoding="async"
                    on:error={() => markPosterFailed(item.poster)}
                  />
                {:else}
                  <div class="poster-placeholder">
                    {#if item.is_anime}
                      <Sparkles size={32} />
                    {:else if item.media_type === 'movie'}
                      <Film size={32} />
                    {:else}
                      <Tv size={32} />
                    {/if}
                  </div>
                {/if}
              </button>
              <div class="poster-badge-top">
                {#if item.is_anime}
                  <span class="badge badge-anime">Anime</span>
                {:else if item.media_type === 'movie'}
                  <span class="badge">Movie</span>
                {:else}
                  <span class="badge">Series</span>
                {/if}
              </div>
              <PosterMarks {badge} />
              <button
                type="button"
                class="poster-bookmark-btn"
                title="Add to list or status"
                aria-label="Add {item.name} to a list or status"
                on:click|stopPropagation={() => (popoverRef = titleRefOf(item))}
              >
                <Bookmark size={14} />
              </button>
            </div>
            <button type="button" class="card-info" on:click={() => openTitle(item)}>
              <div class="card-title" title={item.name}>{item.name}</div>
              {#if item.release_info}
                <div class="card-meta">{item.release_info}</div>
              {/if}
            </button>
          </div>
        {/each}
      </div>
      {#if remainingSearchItems > 0}
        <div class="more-row">
          <button type="button" class="more-btn" on:click={showMoreSearch}>Show more ({remainingSearchItems} left)</button>
        </div>
      {/if}
    {:else if !isSearching}
      <div class="empty-state">
        <p>No results found for "{searchQuery}".</p>
      </div>
    {/if}
  {:else}
    {#if isLoadingTrending && trendingItems.length === 0}
      <div class="loading-state">
        <Loader2 size={32} class="spinner-icon" />
        <p>{hasMoodQuery ? 'Finding recommendations…' : 'Loading trending and recently released titles…'}</p>
      </div>
    {/if}

    {#if trendingItems.length > 0}
      <div class="discover-section-header">
        {#if hasMoodQuery}
          <h3>Recommendations for "{keywords}"</h3>
        {:else}
          <h3>Trending & Recently Released</h3>
          <span class="discover-section-hint">Popular titles available across catalogs</span>
        {/if}
      </div>
      <div class="poster-grid">
        {#each trendingItems as item (item.id)}
          <div class="grid-card">
            <div class="poster-card">
              <button
                type="button"
                class="poster-click-area"
                on:click={() => handleTrendingCardClick(item)}
                aria-label={item.name}
              >
                {#if item.poster && !failedPosters.has(item.poster)}
                  <img
                    src={item.poster}
                    alt={item.name}
                    loading="lazy"
                    decoding="async"
                    on:error={() => markPosterFailed(item.poster)}
                  />
                {:else}
                  <div class="poster-placeholder">
                    <Film size={32} />
                  </div>
                {/if}
              </button>
              <div class="poster-badge-top">
                {#if item.is_anime}
                  <span class="badge badge-anime">Anime</span>
                {:else if item.media_type === 'movie'}
                  <span class="badge">Movie</span>
                {:else}
                  <span class="badge">Series</span>
                {/if}
              </div>
              <PosterMarks badge={badgeFor(badgeIndex, item.id)} />
              <button
                type="button"
                class="poster-bookmark-btn"
                title="Add to list or status"
                aria-label="Add {item.name} to a list or status"
                on:click|stopPropagation={() => (popoverRef = titleRefOf({ id: item.id, name: item.name, media_type: item.media_type, release_info: item.release_info, poster: item.poster }))}
              >
                <Bookmark size={14} />
              </button>
            </div>
            <button
              type="button"
              class="card-info"
              on:click={() => handleTrendingCardClick(item)}
            >
              <div class="card-title" title={item.name}>{item.name}</div>
              <div class="card-meta-row">
                {#if item.release_info}
                  <span class="card-meta">{item.release_info}</span>
                {/if}
                {#if item.imdb_rating}
                  <span class="card-rating"><Star size={12} fill="currentColor" strokeWidth={1.5} /> {item.imdb_rating}</span>
                {/if}
              </div>
              {#if item.genres && item.genres.length > 0}
                <div class="card-genres">{item.genres.slice(0, 3).join(', ')}</div>
              {/if}
            </button>
          </div>
        {/each}
      </div>
      {#if hasMoreTrending}
        <div class="more-row">
          <button type="button" class="more-btn" on:click={loadMoreTrending} disabled={isLoadingMoreTrending}>
            {#if isLoadingMoreTrending}<Loader2 size={16} class="spinner-icon" /> Loading…{:else}Show more{/if}
          </button>
        </div>
      {/if}
    {:else if !isLoadingTrending && hasMoodQuery}
      <div class="empty-state">
        <p>No recommendations found. Try different keywords or genres.</p>
      </div>
    {/if}
  {/if}

  {#if popoverRef}
    <AddToListPopover
      titleRef={popoverRef}
      {activeProfileId}
      onClose={() => (popoverRef = null)}
      {onOpenProfilePicker}
    />
  {/if}
</div>

<style>
  .poster-bookmark-btn {
    position: absolute;
    top: 8px;
    right: 8px;
    z-index: 3;
    width: 34px;
    height: 34px;
    border-radius: 50%;
    background: rgba(20, 20, 22, 0.9);
    border: 1px solid var(--border-muted);
    color: var(--text-secondary);
    display: flex;
    align-items: center;
    justify-content: center;
    opacity: 0.95;
    transition: background-color var(--transition-fast), color var(--transition-fast), transform var(--transition-fast);
  }

  .poster-bookmark-btn:hover {
    opacity: 1;
    color: var(--accent-primary);
    background: var(--bg-surface-elevated);
    transform: scale(1.06);
  }

  .recommend-view {
    max-width: 1320px;
    margin: 0 auto;
    padding: 32px 28px 80px;
  }

  .search-status { color: var(--text-secondary); font-size: 0.875rem; margin-bottom: 1rem; }
  .search-status p { margin: 0.4rem 0; }

  .discover-section-header {
    display: flex;
    align-items: baseline;
    gap: 12px;
    margin-bottom: 20px;
    padding: 0 4px;
  }

  .discover-section-header h3 {
    font-size: 1.38rem;
    font-weight: 680;
    color: var(--text-primary);
    margin: 0;
  }

  .discover-section-hint {
    font-size: 0.85rem;
    color: var(--text-muted);
  }

  .loading-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 16px;
    padding: 80px 20px;
    color: var(--text-muted);
    font-size: 0.95rem;
  }

  .discover-bar-section {
    display: flex;
    flex-direction: column;
    gap: 18px;
    margin-bottom: 36px;
  }

  .discover-form {
    display: flex;
    gap: 12px;
  }

  .discover-input-wrapper {
    flex: 1;
    position: relative;
    display: flex;
    align-items: center;
  }

  :global(.discover-icon) {
    position: absolute;
    left: 16px;
    color: var(--text-muted);
    pointer-events: none;
  }

  :global(.spinner-icon) {
    animation: spin 1s linear infinite;
  }

  @keyframes spin {
    from { transform: rotate(0deg); }
    to { transform: rotate(360deg); }
  }

  .discover-input {
    width: 100%;
    height: 52px;
    padding: 0 48px;
    background: var(--bg-surface);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-lg);
    color: var(--text-primary);
    font-size: 0.95rem;
    box-shadow: inset 0 1px 2px rgba(0, 0, 0, 0.18);
  }

  .discover-input:focus {
    border-color: var(--accent-primary);
    box-shadow: 0 0 0 3px var(--accent-muted);
  }

  .clear-btn {
    position: absolute;
    right: 14px;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 6px;
    background: transparent;
    color: var(--text-muted);
    border-radius: 50%;
  }

  .clear-btn:hover {
    color: var(--text-primary);
    background: rgba(255, 255, 255, 0.1);
  }

  .discover-btn {
    padding: 0 28px;
    height: 52px;
    background: var(--action-fill);
    color: #fff;
    font-weight: 640;
    font-size: 0.92rem;
    letter-spacing: -0.01em;
    border-radius: var(--radius-lg);
  }

  .discover-btn:hover:not(:disabled) {
    background: var(--action-fill-hover);
    transform: translateY(-1px);
    box-shadow: 0 5px 14px rgba(10, 132, 255, 0.18);
  }

  .filter-pills {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
  }

  .pill {
    display: flex;
    align-items: center;
    gap: 7px;
    min-height: 36px;
    padding: 7px 15px;
    background: var(--control-fill);
    color: var(--text-secondary);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-full);
    font-size: 0.84rem;
    font-weight: 500;
  }

  .pill:hover {
    color: var(--text-primary);
    background: var(--bg-surface-hover);
    border-color: rgba(255, 255, 255, 0.12);
  }

  .pill.active {
    background: var(--bg-surface-hover);
    color: var(--text-primary);
    border-color: var(--border-highlight);
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.2);
  }

  .pill.active :global(svg) { color: var(--accent-primary); }

  .pill-count {
    font-size: 0.7rem;
    padding: 1px 6px;
    border-radius: var(--radius-sm);
    background: var(--accent-muted);
    font-family: var(--font-mono);
    font-weight: 700;
  }

  .suggestion-chips {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
    padding-top: 4px;
  }

  .suggestion-label {
    font-size: 0.8rem;
    font-weight: 600;
    color: var(--text-muted);
  }

  .suggestion-chip {
    padding: 6px 12px;
    background: var(--bg-surface);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-full);
    color: var(--text-secondary);
    font-size: 0.78rem;
    font-weight: 500;
  }

  .suggestion-chip:hover:not(:disabled) {
    color: var(--text-primary);
    background: var(--bg-surface-hover);
    border-color: rgba(255, 255, 255, 0.2);
    transform: translateY(-1px);
  }

  .error-banner {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 14px 18px;
    background: rgba(239, 68, 68, 0.12);
    border: 1px solid rgba(239, 68, 68, 0.35);
    border-radius: var(--radius-md);
    color: #fca5a5;
    margin-bottom: 24px;
    font-size: 0.9rem;
  }

  .grid-card {
    background: transparent;
    display: flex;
    flex-direction: column;
    text-align: left;
    border-radius: var(--radius-md);
  }

  .poster-click-area {
    width: 100%;
    height: 100%;
    display: block;
    background: transparent;
    border: none;
    padding: 0;
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
    z-index: 2;
    pointer-events: none;
  }

  .more-row {
    display: flex;
    justify-content: center;
    margin-top: 28px;
  }

  .more-btn {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    padding: 10px 22px;
    background: var(--bg-surface-elevated);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    color: var(--text-primary);
    font-size: 0.88rem;
    font-weight: 600;
  }

  .more-btn:hover:not(:disabled) {
    border-color: var(--accent-primary);
    color: var(--accent-primary);
  }

  .card-info {
    margin-top: 10px;
    background: transparent;
    border: none;
    padding: 0;
    text-align: left;
    cursor: pointer;
    width: 100%;
    display: block;
  }

  .card-title {
    font-size: 0.92rem;
    font-weight: 600;
    color: var(--text-primary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    transition: color var(--transition-fast);
  }

  .grid-card:hover .poster-card {
    border-color: var(--border-highlight);
    transform: translateY(-3px);
    box-shadow: var(--shadow-card-hover);
  }

  .grid-card:hover .card-title {
    color: var(--text-primary);
  }

  .card-meta-row {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 3px;
  }

  .card-meta {
    font-size: 0.78rem;
    color: var(--text-muted);
  }

  .card-rating {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    font-size: 0.78rem;
    color: var(--status-amber);
    font-weight: 600;
  }

  .card-genres {
    font-size: 0.74rem;
    color: var(--text-secondary);
    margin-top: 3px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .empty-state {
    text-align: center;
    padding: 48px 24px;
    color: var(--text-muted);
  }

  .empty-state p {
    font-size: 0.95rem;
  }

  button:focus-visible { outline: 2px solid var(--accent-primary); outline-offset: 2px; }

  @media (max-width: 800px) {
    .recommend-view { padding: 20px 16px; }
  }

  @media (max-width: 520px) {
    :global(.poster-grid) { grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 20px 12px; }
    .discover-form { gap: 8px; }
    .discover-input { height: 48px; }
    .discover-btn { height: 48px; padding: 0 16px; }
    .discover-section-header { align-items: flex-start; flex-direction: column; gap: 2px; }
    .filter-pills, .suggestion-chips {
      flex-wrap: nowrap;
      overflow-x: auto;
      scrollbar-width: none;
      margin-inline: -16px;
      padding-inline: 16px;
      padding-bottom: 2px;
    }
    .filter-pills::-webkit-scrollbar, .suggestion-chips::-webkit-scrollbar { display: none; }
    .pill, .suggestion-chip, .suggestion-label { flex: none; }
  }

  @media (prefers-reduced-motion: reduce) {
    :global(.spinner-icon) { animation: none; }
    button { transition: none; }
  }
</style>
