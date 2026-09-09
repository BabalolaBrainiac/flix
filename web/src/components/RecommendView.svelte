<script lang="ts">
  import { getRecommendations } from '../lib/api';
  import type { RecommendItem } from '../lib/types';
  import { Compass, Film, Tv, Sparkles, Loader2, AlertCircle } from 'lucide-svelte';

  let keywords = '';
  let activeFilter: 'all' | 'movie' | 'series' | 'anime' = 'all';
  let isLoading = false;
  let errorMsg = '';
  let items: RecommendItem[] = [];
  let hasSearched = false;

  let failedPosters = new Set<string>();
  function markPosterFailed(url: string | null | undefined) {
    if (!url) return;
    failedPosters = new Set(failedPosters).add(url);
  }

  async function handleDiscover() {
    if (!keywords.trim()) return;
    isLoading = true;
    errorMsg = '';
    hasSearched = true;
    try {
      const command: Parameters<typeof getRecommendations>[0] = {
        keywords: keywords.trim(),
        limit: 20,
      };
      if (activeFilter === 'movie') command.movie = true;
      else if (activeFilter === 'series') command.show = true;
      else if (activeFilter === 'anime') command.anime = true;

      const res = await getRecommendations(command);
      items = res.items;
    } catch (e: any) {
      errorMsg = e.message || 'Failed to get recommendations';
      items = [];
    } finally {
      isLoading = false;
    }
  }

  function handleCardClick(item: RecommendItem) {
    // Full play integration comes later; for now just log.
    console.log('Selected recommendation:', item.name, item.id);
  }
</script>

<div class="recommend-view">
  <div class="discover-bar-section">
    <form class="discover-form" on:submit|preventDefault={handleDiscover}>
      <div class="discover-input-wrapper">
        <Compass size={18} class="discover-icon" />
        <input
          type="text"
          bind:value={keywords}
          placeholder="Describe what you want to watch..."
          class="discover-input"
        />
        {#if isLoading}
          <Loader2 size={18} class="spinner-icon" />
        {/if}
      </div>
      <button type="submit" class="discover-btn" disabled={isLoading || !keywords.trim()}>
        Discover
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
        <Tv size={14} /> Shows
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

  {#if items.length > 0}
    <div class="poster-grid">
      {#each items as item (item.id)}
        <button class="grid-card" on:click={() => handleCardClick(item)}>
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
            <div class="score-badge">{item.score.toFixed(1)}</div>
          </div>
          <div class="card-info">
            <div class="card-title" title={item.name}>{item.name}</div>
            <div class="card-meta-row">
              {#if item.release_info}
                <span class="card-meta">{item.release_info}</span>
              {/if}
              {#if item.imdb_rating}
                <span class="card-rating">★ {item.imdb_rating}</span>
              {/if}
            </div>
            {#if item.genres && item.genres.length > 0}
              <div class="card-genres">{item.genres.slice(0, 3).join(', ')}</div>
            {/if}
          </div>
        </button>
      {/each}
    </div>
  {:else if !isLoading && hasSearched}
    <div class="empty-state">
      <p>No recommendations found. Try different keywords.</p>
    </div>
  {/if}
</div>

<style>
  .recommend-view {
    max-width: 1280px;
    margin: 0 auto;
    padding: 24px;
  }

  .discover-bar-section {
    display: flex;
    flex-direction: column;
    gap: 16px;
    margin-bottom: 28px;
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

  .discover-input {
    width: 100%;
    height: 48px;
    padding: 0 44px;
    background: var(--bg-surface);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    color: var(--text-primary);
    font-size: 0.95rem;
  }

  .discover-input:focus {
    border-color: var(--border-focus);
  }

  .discover-btn {
    padding: 0 24px;
    height: 48px;
    background: var(--accent-primary);
    color: #fff;
    font-weight: 600;
    border-radius: var(--radius-md);
  }

  .discover-btn:hover:not(:disabled) {
    background: var(--accent-primary-hover);
  }

  .discover-btn:disabled {
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

  .score-badge {
    position: absolute;
    bottom: 8px;
    right: 8px;
    background: rgba(0, 0, 0, 0.75);
    color: var(--accent-primary);
    padding: 2px 8px;
    border-radius: var(--radius-sm);
    font-size: 0.75rem;
    font-weight: 700;
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

  .card-meta-row {
    display: flex;
    gap: 8px;
    align-items: center;
    margin-top: 2px;
  }

  .card-meta {
    font-size: 0.75rem;
    color: var(--text-muted);
  }

  .card-rating {
    font-size: 0.75rem;
    color: var(--status-amber);
  }

  .card-genres {
    font-size: 0.7rem;
    color: var(--text-muted);
    margin-top: 2px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .empty-state {
    text-align: center;
    padding: 48px;
    color: var(--text-muted);
  }
</style>
