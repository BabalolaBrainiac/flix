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

  const promptSuggestions = [
    'Cyberpunk Anime',
    'Sci-Fi Space Thriller',
    'Mind-Bending Mystery',
    'Post-Apocalyptic Survival',
    'Dark Crime Drama',
  ];

  function pickSuggestion(text: string) {
    keywords = text;
    void handleDiscover();
  }

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
          placeholder="Describe what you want to watch (e.g. mind-bending space thriller)..."
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

    <div class="suggestion-chips">
      <span class="suggestion-label">Try:</span>
      {#each promptSuggestions as prompt}
        <button
          type="button"
          class="suggestion-chip"
          on:click={() => pickSuggestion(prompt)}
          disabled={isLoading}
        >
          {prompt}
        </button>
      {/each}
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
            <div class="poster-gradient-overlay"></div>
            <div class="poster-badge-top">
              {#if item.is_anime}
                <span class="badge badge-anime">Anime</span>
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
      <p>No recommendations found. Try different keywords or genres.</p>
    </div>
  {:else if !isLoading && !hasSearched}
    <div class="discover-welcome">
      <div class="welcome-icon-box">
        <Sparkles size={36} />
      </div>
      <h3>Discover Your Next Story</h3>
      <p>Tell Flix what mood, genre, or vibe you want to experience.</p>
      <div class="welcome-prompts">
        {#each promptSuggestions as prompt}
          <button
            type="button"
            class="welcome-prompt-card"
            on:click={() => pickSuggestion(prompt)}
          >
            <span>{prompt}</span>
          </button>
        {/each}
      </div>
    </div>
  {/if}
</div>

<style>
  .recommend-view {
    max-width: 1320px;
    margin: 0 auto;
    padding: 28px;
  }

  .discover-bar-section {
    display: flex;
    flex-direction: column;
    gap: 18px;
    margin-bottom: 32px;
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
    background: var(--bg-surface-elevated);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    color: var(--text-primary);
    font-size: 0.95rem;
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.25);
  }

  .discover-input:focus {
    border-color: var(--accent-primary);
    box-shadow: 0 0 0 2px var(--accent-glow), 0 8px 24px rgba(0, 0, 0, 0.4);
  }

  .discover-btn {
    padding: 0 28px;
    height: 52px;
    background: var(--accent-primary);
    color: #fff;
    font-weight: 600;
    font-size: 0.92rem;
    letter-spacing: 0.02em;
    border-radius: var(--radius-md);
    box-shadow: 0 4px 14px var(--accent-glow);
  }

  .discover-btn:hover:not(:disabled) {
    background: var(--accent-primary-hover);
    transform: translateY(-1px);
    box-shadow: 0 6px 18px var(--accent-glow);
  }

  .discover-btn:disabled {
    opacity: 0.45;
    cursor: not-allowed;
    box-shadow: none;
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
    padding: 7px 16px;
    background: var(--bg-surface);
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
    background: var(--accent-primary);
    color: #fff;
    border-color: var(--accent-primary);
    box-shadow: 0 2px 10px var(--accent-glow);
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
    padding: 4px 12px;
    background: var(--bg-surface-elevated);
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
    cursor: pointer;
    border-radius: var(--radius-md);
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

  .poster-gradient-overlay {
    position: absolute;
    inset: 0;
    background: linear-gradient(180deg, rgba(0,0,0,0) 60%, rgba(0,0,0,0.85) 100%);
    pointer-events: none;
  }

  .poster-badge-top {
    position: absolute;
    top: 8px;
    left: 8px;
    z-index: 2;
  }

  .score-badge {
    position: absolute;
    bottom: 8px;
    right: 8px;
    z-index: 2;
    background: rgba(0, 0, 0, 0.78);
    backdrop-filter: blur(8px);
    -webkit-backdrop-filter: blur(8px);
    border: 1px solid rgba(255, 255, 255, 0.1);
    color: #34d399;
    padding: 2px 8px;
    border-radius: var(--radius-sm);
    font-size: 0.75rem;
    font-weight: 700;
  }

  .card-info {
    margin-top: 10px;
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

  .grid-card:hover .card-title {
    color: var(--accent-primary-hover);
  }

  .card-meta-row {
    display: flex;
    gap: 8px;
    align-items: center;
    margin-top: 3px;
  }

  .card-meta {
    font-size: 0.78rem;
    color: var(--text-muted);
  }

  .card-rating {
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
    padding: 64px 20px;
    color: var(--text-muted);
    font-size: 0.95rem;
  }

  .discover-welcome {
    display: flex;
    flex-direction: column;
    align-items: center;
    text-align: center;
    padding: 64px 24px;
    background: var(--glass-surface);
    backdrop-filter: var(--glass-blur);
    -webkit-backdrop-filter: var(--glass-blur);
    border: 1px solid var(--glass-border);
    border-radius: var(--radius-lg);
    box-shadow: var(--shadow-md);
  }

  .welcome-icon-box {
    width: 68px;
    height: 68px;
    border-radius: 50%;
    background: rgba(229, 9, 20, 0.1);
    border: 1px solid rgba(229, 9, 20, 0.25);
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--accent-primary);
    margin-bottom: 20px;
    box-shadow: 0 0 24px var(--accent-glow);
  }

  .discover-welcome h3 {
    font-size: 1.4rem;
    margin-bottom: 8px;
    letter-spacing: -0.02em;
  }

  .discover-welcome p {
    color: var(--text-secondary);
    font-size: 0.92rem;
    max-width: 460px;
    margin-bottom: 28px;
  }

  .welcome-prompts {
    display: flex;
    flex-wrap: wrap;
    justify-content: center;
    gap: 10px;
    max-width: 600px;
  }

  .welcome-prompt-card {
    padding: 8px 16px;
    background: var(--bg-surface-elevated);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-full);
    color: var(--text-primary);
    font-size: 0.85rem;
    font-weight: 500;
  }

  .welcome-prompt-card:hover {
    background: var(--bg-surface-hover);
    border-color: rgba(229, 9, 20, 0.4);
    box-shadow: 0 4px 14px rgba(0, 0, 0, 0.4);
    transform: translateY(-2px);
  }
</style>
