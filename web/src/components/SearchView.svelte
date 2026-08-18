<script lang="ts">
  import { search, getEpisodes, getStreams, play, addDownload } from '../lib/api';
  import type {
    CatalogItemSummary,
    EpisodeSummary,
    StreamSummary,
    PlayCommand,
  } from '../lib/types';

  export let onPlayStarted: () => void;

  let query = '';
  let activeFilter: 'all' | 'movie' | 'series' | 'anime' = 'all';
  let isLoading = false;
  let errorMsg = '';
  let items: CatalogItemSummary[] = [];
  let notes: string[] = [];

  // Selected item modal state
  let selectedItem: CatalogItemSummary | null = null;
  let episodes: EpisodeSummary[] = [];
  let selectedEpisode: EpisodeSummary | null = null;
  let streams: StreamSummary[] = [];
  let streamQualityFilter: 'all' | '4K' | '1080p' = 'all';
  let showMoreSources = false;
  let isLoadingDetails = false;
  let actionMessage = '';
  let isPreparingPlayback = false;

  async function handleSearch() {
    if (!query.trim()) return;
    isLoading = true;
    errorMsg = '';
    notes = [];
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

  async function selectItem(item: CatalogItemSummary) {
    selectedItem = item;
    episodes = [];
    selectedEpisode = null;
    streams = [];
    streamQualityFilter = 'all';
    showMoreSources = false;
    isLoadingDetails = true;
    actionMessage = '';

    try {
      if (item.media_type === 'series' || item.is_anime) {
        const epRes = await getEpisodes(item.id, item.is_anime);
        episodes = epRes.episodes;
        if (episodes.length > 0) {
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
    showMoreSources = false;
    try {
      const mediaType = selectedItem?.is_anime ? 'anime' : 'series';
      const streamRes = await getStreams(mediaType, episode.stream_id || episode.id);
      streams = streamRes.streams;
    } catch (e: any) {
      actionMessage = `Failed to load streams: ${e.message}`;
    } finally {
      isLoadingDetails = false;
    }
  }

  $: recommendedStream = streams.find((s) => s.is_recommended) || streams[0];
  $: otherStreams = streams.filter((s) => s !== recommendedStream);
  $: filteredOtherStreams = otherStreams.filter((s) => {
    if (streamQualityFilter === 'all') return true;
    if (streamQualityFilter === '4K') return s.quality === '4K';
    if (streamQualityFilter === '1080p') return s.quality === '1080p';
    return true;
  });

  async function handlePlayStream(stream: StreamSummary) {
    isPreparingPlayback = true;
    actionMessage = 'Preparing video stream, warm-up buffers, and English subtitles...';
    try {
      const command: PlayCommand = {
        magnet: stream.magnet,
        file_index: stream.file_index,
        queue_seed:
          selectedItem && episodes.length > 0 && selectedEpisode
            ? {
                catalog_item: selectedItem,
                episodes,
                current_episode_id: selectedEpisode.id,
              }
            : undefined,
        alternatives: streams
          .filter((s) => s.info_hash !== stream.info_hash)
          .slice(0, 3)
          .map((s) => ({
            torrent: s.magnet,
            file_index: s.file_index,
            quality: s.quality,
          })),
      };

      await play(command);
      actionMessage = 'Media player launched on your device!';
      onPlayStarted();
    } catch (e: any) {
      actionMessage = `Playback failed: ${e.message}`;
    } finally {
      isPreparingPlayback = false;
    }
  }

  async function handleDownload(stream: StreamSummary) {
    try {
      await addDownload(stream.magnet, stream.file_index);
      actionMessage = 'Added torrent to download library!';
    } catch (e: any) {
      actionMessage = `Download error: ${e.message}`;
    }
  }

  function handleCopyMagnet(magnet: string) {
    navigator.clipboard.writeText(magnet);
    actionMessage = 'Magnet link copied to clipboard!';
  }

  function closeModal() {
    selectedItem = null;
    episodes = [];
    selectedEpisode = null;
    streams = [];
    actionMessage = '';
  }
</script>

<div class="search-view">
  <!-- Search Control Section -->
  <div class="search-hero">
    <h1 class="hero-title">Search & Stream</h1>
    <p class="hero-subtitle">
      Explore movies, series, and anime with automatic 4K ranking and local playback.
    </p>

    <form class="search-bar glass-panel" on:submit|preventDefault={handleSearch}>
      <svg class="search-icon" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
        <circle cx="11" cy="11" r="8"></circle>
        <line x1="21" y1="21" x2="16.65" y2="16.65"></line>
      </svg>
      <input
        type="text"
        bind:value={query}
        placeholder="Search titles, movies, shows, anime..."
        class="search-input"
      />
      {#if query}
        <button type="button" class="clear-btn" on:click={() => (query = '')}>✕</button>
      {/if}
      <button type="submit" class="search-btn" disabled={isLoading || !query.trim()}>
        {#if isLoading}
          <span class="spinner"></span>
        {:else}
          <span>Search</span>
        {/if}
      </button>
    </form>

    <div class="filter-pills">
      <button
        class="pill {activeFilter === 'all' ? 'active' : ''}"
        on:click={() => (activeFilter = 'all')}
      >
        All
      </button>
      <button
        class="pill {activeFilter === 'movie' ? 'active' : ''}"
        on:click={() => (activeFilter = 'movie')}
      >
        Movies
      </button>
      <button
        class="pill {activeFilter === 'series' ? 'active' : ''}"
        on:click={() => (activeFilter = 'series')}
      >
        Series
      </button>
      <button
        class="pill {activeFilter === 'anime' ? 'active' : ''}"
        on:click={() => (activeFilter = 'anime')}
      >
        Anime
      </button>
    </div>
  </div>

  <!-- Messages / Notes -->
  {#if errorMsg}
    <div class="error-banner glass-panel">
      <span>{errorMsg}</span>
    </div>
  {/if}

  {#if notes.length > 0}
    <div class="notes-banner">
      {#each notes as note}
        <p class="note-item">{note}</p>
      {/each}
    </div>
  {/if}

  <!-- Results Grid -->
  {#if items.length > 0}
    <div class="results-section">
      <h2 class="section-heading">Catalog Results ({filteredItems.length})</h2>
      <div class="results-grid">
        {#each filteredItems as item}
          <button
            type="button"
            class="media-card glass-panel"
            on:click={() => selectItem(item)}
          >
            <div class="card-cover">
              <div class="cover-placeholder">
                <svg width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
                  <polygon points="5 3 19 12 5 21 5 3"></polygon>
                </svg>
              </div>
              <div class="badges-overlay">
                {#if item.is_anime}
                  <span class="badge badge-accent">Anime</span>
                {:else}
                  <span class="badge badge-1080p">{item.media_type}</span>
                {/if}
                {#if item.release_info}
                  <span class="badge">{item.release_info}</span>
                {/if}
              </div>
            </div>

            <div class="card-info">
              <h3 class="card-title" title={item.name}>{item.name}</h3>
              <div class="card-meta">
                <span>{item.is_anime ? 'Anime Kitsu' : 'Cinemeta'}</span>
                <span class="view-action">Select →</span>
              </div>
            </div>
          </button>
        {/each}
      </div>
    </div>
  {:else if !isLoading}
    <div class="empty-state">
      <div class="empty-icon">🎬</div>
      <p class="empty-title">Ready to stream</p>
      <p class="empty-desc">
        Type a title above to search across movie, TV, and anime catalogs with automatic native 4K selection.
      </p>
    </div>
  {/if}

  <!-- Detail & Stream Selection Modal -->
  {#if selectedItem}
    <div
      class="modal-backdrop"
      on:click={(e) => e.target === e.currentTarget && closeModal()}
      on:keydown={(e) => e.key === 'Escape' && closeModal()}
      role="dialog"
      aria-modal="true"
      tabindex="-1"
    >
      <div class="modal-content glass-panel" role="document">
        <div class="modal-header">
          <div>
            <h2 class="modal-title">{selectedItem.name}</h2>
            <div class="modal-subtitle">
              <span class="badge badge-accent">
                {selectedItem.is_anime ? 'Anime' : selectedItem.media_type}
              </span>
              {#if selectedItem.release_info}
                <span class="badge">{selectedItem.release_info}</span>
              {/if}
            </div>
          </div>
          <button class="close-btn" on:click={closeModal}>✕</button>
        </div>

        {#if actionMessage}
          <div class="action-alert">{actionMessage}</div>
        {/if}

        <!-- Episodes selector if series -->
        {#if episodes.length > 0}
          <div class="episodes-section">
            <h4 class="sub-heading">Episodes ({episodes.length}):</h4>
            <div class="episodes-list">
              {#each episodes as ep}
                <button
                  class="episode-chip {selectedEpisode?.id === ep.id ? 'selected' : ''}"
                  on:click={() => selectEpisode(ep)}
                >
                  S{ep.season} E{ep.episode} {ep.title ? `· ${ep.title}` : ''}
                </button>
              {/each}
            </div>
          </div>
        {/if}

        <!-- Streams list -->
        <div class="streams-section">
          {#if isLoadingDetails}
            <div class="modal-loading">
              <span class="spinner large"></span>
              <span>Searching torrent streams with 4K ranking...</span>
            </div>
          {:else if streams.length > 0}
            <!-- Highlighted Recommended Stream -->
            {#if recommendedStream}
              <div class="recommended-hero glass-panel">
                <div class="rec-badge-row">
                  <span class="badge badge-accent">⭐ Recommended Default</span>
                  <span class="stream-quality {recommendedStream.quality === '4K' ? 'badge-4k' : 'badge-1080p'}">
                    {recommendedStream.quality}
                  </span>
                  {#if recommendedStream.seeders !== undefined}
                    <span class="stream-seeds">🌱 {recommendedStream.seeders} seeds</span>
                  {/if}
                  {#if recommendedStream.size}
                    <span class="stream-size">💾 {recommendedStream.size}</span>
                  {/if}
                </div>

                <div class="rec-title">{recommendedStream.file_name || recommendedStream.name}</div>

                <div class="rec-actions">
                  <button
                    class="play-btn primary large"
                    on:click={() => handlePlayStream(recommendedStream)}
                    disabled={isPreparingPlayback}
                  >
                    {#if isPreparingPlayback}
                      <span class="spinner"></span>
                      <span>Starting Stream & Player...</span>
                    {:else}
                      <span>▶ Stream in Media Player</span>
                    {/if}
                  </button>
                  <button class="download-btn" on:click={() => handleDownload(recommendedStream)}>
                    ⬇ Download
                  </button>
                  <button class="copy-btn" on:click={() => handleCopyMagnet(recommendedStream.magnet)}>
                    🧲 Copy Magnet
                  </button>
                </div>
              </div>
            {/if}

            <!-- More Sources Drawer Toggle -->
            {#if otherStreams.length > 0}
              <div class="more-sources-header">
                <button
                  class="toggle-more-btn"
                  on:click={() => (showMoreSources = !showMoreSources)}
                >
                  <span>{showMoreSources ? '▼ Hide Alternative Sources' : `▶ More Sources (${otherStreams.length})`}</span>
                </button>

                {#if showMoreSources}
                  <div class="stream-filter-pills">
                    <button
                      class="stream-filter-btn {streamQualityFilter === 'all' ? 'active' : ''}"
                      on:click={() => (streamQualityFilter = 'all')}
                    >
                      All
                    </button>
                    <button
                      class="stream-filter-btn {streamQualityFilter === '4K' ? 'active' : ''}"
                      on:click={() => (streamQualityFilter = '4K')}
                    >
                      4K Only
                    </button>
                    <button
                      class="stream-filter-btn {streamQualityFilter === '1080p' ? 'active' : ''}"
                      on:click={() => (streamQualityFilter = '1080p')}
                    >
                      1080p Only
                    </button>
                  </div>
                {/if}
              </div>

              {#if showMoreSources}
                <div class="streams-list">
                  {#each filteredOtherStreams as stream}
                    <div class="stream-card">
                      <div class="stream-main">
                        <div class="stream-header-row">
                          <span class="stream-quality {stream.quality === '4K' ? 'badge-4k' : 'badge-1080p'}">
                            {stream.quality}
                          </span>
                          {#if stream.seeders !== undefined}
                            <span class="stream-seeds">🌱 {stream.seeders} seeds</span>
                          {/if}
                          {#if stream.size}
                            <span class="stream-size">💾 {stream.size}</span>
                          {/if}
                        </div>
                        <div class="stream-name" title={stream.file_name || stream.name}>
                          {stream.file_name || stream.name}
                        </div>
                      </div>

                      <div class="stream-actions">
                        <button
                          class="play-btn secondary"
                          on:click={() => handlePlayStream(stream)}
                          disabled={isPreparingPlayback}
                        >
                          ▶ Play
                        </button>
                        <button class="download-btn" on:click={() => handleDownload(stream)}>
                          ⬇
                        </button>
                        <button class="copy-btn" on:click={() => handleCopyMagnet(stream.magnet)}>
                          🧲
                        </button>
                      </div>
                    </div>
                  {/each}
                </div>
              {/if}
            {/if}
          {:else}
            <p class="no-streams">No torrent streams found for this selection.</p>
          {/if}
        </div>
      </div>
    </div>
  {/if}
</div>

<style>
  .search-view {
    max-width: 1280px;
    margin: 0 auto;
    padding: 32px 24px 80px;
  }

  .search-hero {
    text-align: center;
    max-width: 720px;
    margin: 0 auto 40px;
  }

  .hero-title {
    font-size: 2.75rem;
    font-weight: 800;
    margin-bottom: 8px;
    background: var(--accent-gradient);
    -webkit-background-clip: text;
    background-clip: text;
    -webkit-text-fill-color: transparent;
  }

  .hero-subtitle {
    color: var(--text-secondary);
    font-size: 1.05rem;
    margin-bottom: 28px;
  }

  .search-bar {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 8px 12px 8px 18px;
    background: var(--bg-surface);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-full);
    box-shadow: 0 8px 32px rgba(0, 0, 0, 0.3);
    margin-bottom: 20px;
    transition: all var(--transition-smooth);
  }

  .search-bar:focus-within {
    border-color: var(--border-focus);
    box-shadow: var(--border-glow);
  }

  .search-icon {
    color: var(--text-muted);
  }

  .search-input {
    flex: 1;
    background: transparent;
    border: none;
    color: var(--text-primary);
    font-size: 1rem;
  }

  .clear-btn {
    background: transparent;
    color: var(--text-muted);
    font-size: 1rem;
    padding: 4px 8px;
  }

  .clear-btn:hover {
    color: var(--text-primary);
  }

  .search-btn {
    background: var(--accent-primary);
    color: #fff;
    padding: 10px 22px;
    border-radius: var(--radius-full);
    font-weight: 600;
    font-size: 0.95rem;
    box-shadow: var(--accent-glow);
  }

  .search-btn:hover:not(:disabled) {
    background: var(--accent-primary-hover);
    transform: translateY(-1px);
  }

  .search-btn:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }

  .filter-pills {
    display: flex;
    justify-content: center;
    gap: 10px;
  }

  .pill {
    padding: 6px 18px;
    border-radius: var(--radius-full);
    background: rgba(23, 31, 48, 0.6);
    color: var(--text-secondary);
    font-size: 0.875rem;
    font-weight: 500;
    border: 1px solid var(--border-subtle);
  }

  .pill:hover {
    background: rgba(255, 255, 255, 0.08);
    color: var(--text-primary);
  }

  .pill.active {
    background: rgba(99, 102, 241, 0.2);
    color: #fff;
    border-color: rgba(99, 102, 241, 0.5);
  }

  .error-banner {
    padding: 14px 20px;
    background: rgba(239, 68, 68, 0.15);
    border: 1px solid rgba(239, 68, 68, 0.3);
    color: #fca5a5;
    margin-bottom: 24px;
  }

  .notes-banner {
    margin-bottom: 20px;
  }

  .note-item {
    font-size: 0.85rem;
    color: var(--text-muted);
  }

  .section-heading {
    font-size: 1.5rem;
    font-weight: 700;
    margin-bottom: 20px;
  }

  .results-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(220px, 1fr));
    gap: 20px;
  }

  .media-card {
    cursor: pointer;
    overflow: hidden;
    transition: all var(--transition-smooth);
    border-radius: var(--radius-md);
    background: var(--bg-surface);
    text-align: left;
    color: inherit;
    border: 1px solid var(--border-subtle);
  }

  .media-card:hover {
    transform: translateY(-4px);
    border-color: rgba(99, 102, 241, 0.4);
    box-shadow: 0 12px 30px rgba(0, 0, 0, 0.4);
  }

  .card-cover {
    height: 140px;
    background: linear-gradient(135deg, rgba(30, 41, 64, 0.8), rgba(15, 20, 34, 0.95));
    position: relative;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .cover-placeholder {
    color: rgba(255, 255, 255, 0.15);
  }

  .badges-overlay {
    position: absolute;
    bottom: 10px;
    left: 10px;
    display: flex;
    gap: 6px;
  }

  .card-info {
    padding: 14px;
  }

  .card-title {
    font-size: 0.95rem;
    font-weight: 600;
    margin-bottom: 8px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .card-meta {
    display: flex;
    justify-content: space-between;
    align-items: center;
    font-size: 0.75rem;
    color: var(--text-muted);
  }

  .view-action {
    color: var(--accent-primary);
    font-weight: 600;
  }

  .empty-state {
    text-align: center;
    padding: 60px 20px;
  }

  .empty-icon {
    font-size: 3rem;
    margin-bottom: 12px;
  }

  .empty-title {
    font-size: 1.25rem;
    font-weight: 700;
    margin-bottom: 6px;
  }

  .empty-desc {
    color: var(--text-secondary);
    max-width: 440px;
    margin: 0 auto;
    font-size: 0.9rem;
  }

  /* Modal */
  .modal-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.75);
    backdrop-filter: blur(8px);
    z-index: 100;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 24px;
  }

  .modal-content {
    background: var(--bg-secondary);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-lg);
    max-width: 800px;
    width: 100%;
    max-height: 88vh;
    overflow-y: auto;
    padding: 28px;
    box-shadow: 0 24px 60px rgba(0, 0, 0, 0.6);
  }

  .modal-header {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    margin-bottom: 20px;
  }

  .modal-title {
    font-size: 1.5rem;
    font-weight: 700;
    margin-bottom: 6px;
  }

  .modal-subtitle {
    display: flex;
    gap: 8px;
  }

  .close-btn {
    background: transparent;
    color: var(--text-muted);
    font-size: 1.25rem;
    padding: 6px;
  }

  .close-btn:hover {
    color: var(--text-primary);
  }

  .action-alert {
    padding: 12px 18px;
    border-radius: var(--radius-sm);
    background: rgba(99, 102, 241, 0.15);
    border: 1px solid rgba(99, 102, 241, 0.3);
    color: #c7d2fe;
    margin-bottom: 20px;
    font-size: 0.9rem;
  }

  .sub-heading {
    font-size: 0.95rem;
    font-weight: 600;
    margin-bottom: 12px;
    color: var(--text-secondary);
  }

  .episodes-section {
    margin-bottom: 24px;
  }

  .episodes-list {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    max-height: 140px;
    overflow-y: auto;
  }

  .episode-chip {
    padding: 6px 14px;
    border-radius: var(--radius-sm);
    background: var(--bg-surface);
    color: var(--text-secondary);
    font-size: 0.8rem;
    border: 1px solid var(--border-subtle);
  }

  .episode-chip:hover {
    background: rgba(255, 255, 255, 0.08);
    color: var(--text-primary);
  }

  .episode-chip.selected {
    background: rgba(99, 102, 241, 0.25);
    border-color: var(--accent-primary);
    color: #fff;
    font-weight: 600;
  }

  .recommended-hero {
    padding: 20px 24px;
    border-radius: var(--radius-md);
    border: 1px solid rgba(99, 102, 241, 0.4);
    background: linear-gradient(135deg, rgba(30, 41, 64, 0.8), rgba(23, 31, 48, 0.9));
    margin-bottom: 24px;
    box-shadow: 0 8px 24px rgba(99, 102, 241, 0.15);
  }

  .rec-badge-row {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-bottom: 10px;
  }

  .rec-title {
    font-size: 1.05rem;
    font-weight: 700;
    margin-bottom: 16px;
    color: var(--text-primary);
  }

  .rec-actions {
    display: flex;
    gap: 10px;
  }

  .more-sources-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 14px;
  }

  .toggle-more-btn {
    background: transparent;
    color: var(--accent-primary);
    font-weight: 600;
    font-size: 0.9rem;
    padding: 4px 0;
  }

  .toggle-more-btn:hover {
    color: #a5b4fc;
  }

  .stream-filter-pills {
    display: flex;
    gap: 6px;
  }

  .stream-filter-btn {
    padding: 4px 12px;
    border-radius: var(--radius-full);
    background: rgba(23, 31, 48, 0.6);
    border: 1px solid var(--border-subtle);
    color: var(--text-muted);
    font-size: 0.75rem;
  }

  .stream-filter-btn.active {
    background: rgba(99, 102, 241, 0.2);
    color: #fff;
    border-color: rgba(99, 102, 241, 0.4);
  }

  .streams-list {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .stream-card {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 16px;
    padding: 12px 16px;
    border-radius: var(--radius-md);
    background: var(--bg-surface);
    border: 1px solid var(--border-subtle);
  }

  .stream-main {
    flex: 1;
    min-width: 0;
  }

  .stream-header-row {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-bottom: 4px;
    font-size: 0.8rem;
  }

  .stream-seeds {
    color: #34d399;
  }

  .stream-size {
    color: var(--text-muted);
  }

  .stream-name {
    font-size: 0.85rem;
    color: var(--text-secondary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .stream-actions {
    display: flex;
    gap: 8px;
  }

  .play-btn {
    padding: 8px 16px;
    border-radius: var(--radius-sm);
    font-size: 0.85rem;
    font-weight: 600;
  }

  .play-btn.large {
    padding: 10px 22px;
    font-size: 0.95rem;
  }

  .play-btn.primary {
    background: var(--accent-primary);
    color: #fff;
    box-shadow: var(--accent-glow);
  }

  .play-btn.primary:hover:not(:disabled) {
    background: var(--accent-primary-hover);
  }

  .play-btn.secondary {
    background: rgba(255, 255, 255, 0.08);
    color: var(--text-primary);
  }

  .play-btn:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }

  .download-btn, .copy-btn {
    background: transparent;
    border: 1px solid var(--border-subtle);
    color: var(--text-secondary);
    padding: 8px 12px;
    border-radius: var(--radius-sm);
    font-size: 0.85rem;
  }

  .download-btn:hover, .copy-btn:hover {
    background: rgba(255, 255, 255, 0.05);
    color: var(--text-primary);
  }

  .spinner {
    display: inline-block;
    width: 16px;
    height: 16px;
    border: 2px solid rgba(255, 255, 255, 0.3);
    border-top-color: #fff;
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }

  .spinner.large {
    width: 24px;
    height: 24px;
  }

  @keyframes spin {
    to { transform: rotate(360deg); }
  }

  .modal-loading {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 24px;
    color: var(--text-secondary);
  }

  .no-streams {
    color: var(--text-muted);
    font-size: 0.9rem;
    padding: 12px 0;
  }
</style>
