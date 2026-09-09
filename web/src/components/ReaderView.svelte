<script lang="ts">
  import { readerSearch, readerChapters } from '../lib/api';
  import type { ReaderPublication, ReaderChapter } from '../lib/types';
  import { Search, BookOpen, ArrowLeft, Loader2, AlertCircle } from 'lucide-svelte';
  import ReaderPane from './ReaderPane.svelte';

  let query = '';
  let isLoading = false;
  let searchRequest = 0;
  let errorMsg = '';
  let publications: ReaderPublication[] = [];
  let hasSearched = false;

  // Chapter list state
  let selectedPublication: ReaderPublication | null = null;
  let chapters: ReaderChapter[] = [];
  let availableLanguages: string[] = [];
  let selectedLang = 'en';
  let isLoadingChapters = false;
  let chapterRequest = 0;

  // Reader pane state
  let selectedChapter: ReaderChapter | null = null;

  let failedCovers = new Set<string>();
  function markCoverFailed(url: string | null | undefined) {
    if (!url) return;
    failedCovers = new Set(failedCovers).add(url);
  }

  async function handleSearch() {
    if (!query.trim()) return;
    const request = ++searchRequest;
    isLoading = true;
    errorMsg = '';
    hasSearched = true;
    selectedPublication = null;
    selectedChapter = null;
    try {
      const res = await readerSearch(query.trim());
      if (request !== searchRequest) return;
      publications = res.publications;
    } catch (e: any) {
      if (request !== searchRequest) return;
      errorMsg = e.message || 'Search failed';
      publications = [];
    } finally {
      if (request === searchRequest) isLoading = false;
    }
  }

  async function selectPublication(pub: ReaderPublication) {
    const request = ++chapterRequest;
    selectedPublication = pub;
    selectedChapter = null;
    chapters = [];
    isLoadingChapters = true;
    errorMsg = '';
    try {
      const res = await readerChapters(pub.id, selectedLang);
      if (request !== chapterRequest) return;
      chapters = res.chapters;
      availableLanguages = res.available_languages;
    } catch (e: any) {
      if (request !== chapterRequest) return;
      errorMsg = e.message || 'Failed to load chapters';
    } finally {
      if (request === chapterRequest) isLoadingChapters = false;
    }
  }

  async function changeLanguage(lang: string) {
    selectedLang = lang;
    if (selectedPublication) {
      await selectPublication(selectedPublication);
    }
  }

  function selectChapter(chapter: ReaderChapter) {
    selectedChapter = chapter;
  }

  function returnToSearch() {
    chapterRequest += 1;
    selectedPublication = null;
    selectedChapter = null;
    chapters = [];
    isLoadingChapters = false;
  }

  function returnToChapters() {
    selectedChapter = null;
  }

  function formatChapterLabel(ch: ReaderChapter): string {
    const parts: string[] = [];
    if (ch.volume) parts.push(`Vol. ${ch.volume}`);
    if (ch.chapter) parts.push(`Ch. ${ch.chapter}`);
    if (ch.title) parts.push(ch.title);
    return parts.length > 0 ? parts.join(' · ') : 'Untitled';
  }
</script>

<div class="reader-view">
  {#if selectedChapter && selectedPublication}
    <ReaderPane
      chapter={selectedChapter}
      publication={selectedPublication}
      onBack={returnToChapters}
    />
  {:else if !selectedPublication}
    <div class="search-bar-section">
      <form class="search-form" on:submit|preventDefault={handleSearch}>
        <div class="search-input-wrapper">
          <Search size={18} class="search-icon" />
          <input
            type="text"
            bind:value={query}
            placeholder="Search manga..."
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
    </div>

    {#if errorMsg}
      <div class="error-banner">
        <AlertCircle size={16} />
        <span>{errorMsg}</span>
      </div>
    {/if}

    {#if publications.length > 0}
      <div class="poster-grid">
        {#each publications as pub (pub.id)}
          <button class="grid-card" on:click={() => selectPublication(pub)}>
            <div class="poster-card">
              {#if pub.cover_url && !failedCovers.has(pub.cover_url)}
                <img
                  src={pub.cover_url}
                  alt={pub.title}
                  loading="lazy"
                  on:error={() => markCoverFailed(pub.cover_url)}
                />
              {:else}
                <div class="poster-placeholder">
                  <BookOpen size={32} />
                </div>
              {/if}
              <div class="poster-badge-top">
                {#if pub.status}
                  <span class="badge">{pub.status}</span>
                {/if}
              </div>
            </div>
            <div class="card-info">
              <div class="card-title" title={pub.title}>{pub.title}</div>
              {#if pub.year}
                <div class="card-meta">{pub.year}</div>
              {/if}
            </div>
          </button>
        {/each}
      </div>
    {:else if !isLoading && hasSearched}
      <div class="empty-state">
        <p>No manga found for "{query}".</p>
      </div>
    {/if}
  {:else}
    <!-- Chapter list view -->
    <div class="detail-view">
      <button class="back-btn" on:click={returnToSearch}>
        <ArrowLeft size={16} /> Back to results
      </button>

      <div class="detail-header">
        <div class="detail-poster">
          {#if selectedPublication.cover_url && !failedCovers.has(selectedPublication.cover_url)}
            <img
              src={selectedPublication.cover_url}
              alt={selectedPublication.title}
              on:error={() => markCoverFailed(selectedPublication?.cover_url)}
            />
          {:else}
            <div class="poster-placeholder">
              <BookOpen size={48} />
            </div>
          {/if}
        </div>
        <div class="detail-meta">
          <h2>{selectedPublication.title}</h2>
          <div class="meta-row">
            {#if selectedPublication.year}
              <span class="meta-tag">{selectedPublication.year}</span>
            {/if}
            {#if selectedPublication.status}
              <span class="badge badge-recommended">{selectedPublication.status}</span>
            {/if}
          </div>
          {#if selectedPublication.tags.length > 0}
            <div class="meta-tags">{selectedPublication.tags.slice(0, 5).join(', ')}</div>
          {/if}
        </div>
      </div>

      {#if availableLanguages.length > 1}
        <div class="lang-selector">
          <span class="lang-label">Language:</span>
          {#each availableLanguages as lang}
            <button
              class="pill {selectedLang === lang ? 'active' : ''}"
              on:click={() => changeLanguage(lang)}
            >
              {lang.toUpperCase()}
            </button>
          {/each}
        </div>
      {/if}

      {#if errorMsg}
        <div class="error-banner">
          <AlertCircle size={16} />
          <span>{errorMsg}</span>
        </div>
      {/if}

      <div class="chapters-section">
        <h3>Chapters</h3>
        {#if isLoadingChapters}
          <div class="loading-box">
            <Loader2 size={24} class="spinner-icon" />
            <span>Loading chapters...</span>
          </div>
        {:else if chapters.length > 0}
          <div class="chapter-list">
            {#each chapters as ch (ch.id)}
              <button
                class="chapter-row"
                on:click={() => selectChapter(ch)}
              >
                <span class="ch-number">{ch.chapter || '—'}</span>
                <span class="ch-title">{formatChapterLabel(ch)}</span>
                <span class="ch-pages">{ch.pages} pages</span>
              </button>
            {/each}
          </div>
        {:else}
          <p class="no-chapters">No chapters found for this language.</p>
        {/if}
      </div>
    </div>
  {/if}
</div>

<style>
  .reader-view {
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

  /* Detail / chapter list view */
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

  .meta-tags {
    font-size: 0.8rem;
    color: var(--text-muted);
  }

  .lang-selector {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .lang-label {
    font-size: 0.85rem;
    color: var(--text-secondary);
    font-weight: 500;
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

  .chapters-section {
    background: var(--bg-surface);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    padding: 20px;
  }

  .chapters-section h3 {
    font-size: 1.1rem;
    margin-bottom: 14px;
  }

  .loading-box {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 24px;
    color: var(--text-muted);
  }

  .chapter-list {
    display: flex;
    flex-direction: column;
    gap: 2px;
    max-height: 480px;
    overflow-y: auto;
  }

  .chapter-row {
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

  .chapter-row:hover {
    background: var(--bg-surface-hover);
    color: var(--text-primary);
  }

  .ch-number {
    flex-shrink: 0;
    width: 40px;
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

  .ch-title {
    flex: 1;
    min-width: 0;
    font-size: 0.9rem;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .ch-pages {
    flex-shrink: 0;
    font-size: 0.8rem;
    color: var(--text-muted);
  }

  .no-chapters {
    color: var(--text-muted);
    padding: 24px;
    text-align: center;
  }
</style>
