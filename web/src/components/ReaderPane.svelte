<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { readerPages, readerGetProgress, readerSaveProgress, getToken } from '../lib/api';
  import type { ReaderChapter, ReaderPublication } from '../lib/types';
  import { ArrowLeft, Loader2, AlertCircle, ChevronLeft, ChevronRight } from 'lucide-svelte';

  export let chapter: ReaderChapter;
  export let publication: ReaderPublication;
  export let onBack: () => void;

  let pages: string[] = [];
  let currentPage = 0;
  let isLoading = true;
  let errorMsg = '';
  let pageCount = 0;

  // Build an authenticated image URL by appending the token as a query param
  // (the page endpoint accepts token via query or header).
  function pageUrl(url: string): string {
    const token = getToken();
    if (token) {
      const sep = url.includes('?') ? '&' : '?';
      return `${url}${sep}token=${encodeURIComponent(token)}`;
    }
    return url;
  }

  async function loadPages() {
    isLoading = true;
    errorMsg = '';
    try {
      const res = await readerPages(chapter.id, publication.id);
      pages = res.pages;
      pageCount = res.page_count;

      // Restore saved progress
      try {
        const progress = await readerGetProgress(publication.id);
        if (progress.position && progress.position.chapter_id === chapter.id) {
          currentPage = Math.min(progress.position.page, pageCount - 1);
        }
      } catch {
        // Ignore progress load errors
      }
    } catch (e: any) {
      errorMsg = e.message || 'Failed to load pages';
    } finally {
      isLoading = false;
    }
  }

  function saveProgress() {
    readerSaveProgress(publication.id, chapter.id, currentPage).catch(() => {
      // Silently ignore progress save errors
    });
  }

  function goToPage(index: number) {
    if (index < 0 || index >= pageCount) return;
    currentPage = index;
    saveProgress();
    preloadUpcoming();
  }

  function nextPage() {
    goToPage(currentPage + 1);
  }

  function prevPage() {
    goToPage(currentPage - 1);
  }

  // Preload the next 3 pages
  function preloadUpcoming() {
    for (let i = 1; i <= 3; i++) {
      const idx = currentPage + i;
      if (idx < pageCount && pages[idx]) {
        const img = new Image();
        img.src = pageUrl(pages[idx]);
      }
    }
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'ArrowRight' || e.key === 'ArrowDown' || e.key === ' ') {
      e.preventDefault();
      nextPage();
    } else if (e.key === 'ArrowLeft' || e.key === 'ArrowUp') {
      e.preventDefault();
      prevPage();
    }
  }

  onMount(() => {
    loadPages().then(() => preloadUpcoming());
    window.addEventListener('keydown', handleKeydown);
  });

  onDestroy(() => {
    window.removeEventListener('keydown', handleKeydown);
  });
</script>

<div class="reader-pane">
  <div class="reader-toolbar">
    <button class="back-btn" on:click={onBack}>
      <ArrowLeft size={16} /> Chapters
    </button>
    <div class="reader-title">
      <span class="reader-pub-title">{publication.title}</span>
      {#if chapter.chapter}
        <span class="reader-ch-label">Ch. {chapter.chapter}</span>
      {/if}
    </div>
    {#if pageCount > 0}
      <span class="page-counter">{currentPage + 1} / {pageCount}</span>
    {/if}
  </div>

  {#if errorMsg}
    <div class="error-banner">
      <AlertCircle size={16} />
      <span>{errorMsg}</span>
    </div>
  {/if}

  {#if isLoading}
    <div class="loading-box">
      <Loader2 size={32} class="spinner-icon" />
      <span>Loading pages...</span>
    </div>
  {:else if pages.length > 0}
    <div class="page-container">
      <button
        class="nav-zone nav-left"
        on:click={prevPage}
        disabled={currentPage === 0}
        aria-label="Previous page"
      >
        <ChevronLeft size={24} />
      </button>

      <div class="page-image-wrapper">
        {#if pages[currentPage]}
          <img
            class="page-image"
            src={pageUrl(pages[currentPage])}
            alt="Page {currentPage + 1}"
          />
        {/if}
      </div>

      <button
        class="nav-zone nav-right"
        on:click={nextPage}
        disabled={currentPage >= pageCount - 1}
        aria-label="Next page"
      >
        <ChevronRight size={24} />
      </button>
    </div>

    <div class="page-overlay">
      <span>{currentPage + 1} / {pageCount}</span>
    </div>
  {/if}
</div>

<style>
  .reader-pane {
    display: flex;
    flex-direction: column;
    min-height: calc(100vh - 140px);
    position: relative;
  }

  .reader-toolbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    padding: 8px 0;
    margin-bottom: 12px;
  }

  .back-btn {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    background: transparent;
    color: var(--text-secondary);
    font-size: 0.9rem;
    padding: 8px 0;
  }

  .back-btn:hover {
    color: var(--text-primary);
  }

  .reader-title {
    flex: 1;
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
    overflow: hidden;
  }

  .reader-pub-title {
    font-size: 0.9rem;
    font-weight: 600;
    color: var(--text-primary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .reader-ch-label {
    font-size: 0.8rem;
    color: var(--text-muted);
    flex-shrink: 0;
  }

  .page-counter {
    font-size: 0.85rem;
    color: var(--text-muted);
    font-variant-numeric: tabular-nums;
    flex-shrink: 0;
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

  .loading-box {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 12px;
    padding: 64px;
    color: var(--text-muted);
  }

  :global(.spinner-icon) {
    animation: spin 1s linear infinite;
  }

  @keyframes spin {
    from { transform: rotate(0deg); }
    to { transform: rotate(360deg); }
  }

  .page-container {
    display: flex;
    align-items: center;
    flex: 1;
    position: relative;
  }

  .nav-zone {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 48px;
    height: 100%;
    min-height: 200px;
    background: transparent;
    color: var(--text-muted);
    opacity: 0;
    transition: opacity var(--transition-fast);
    cursor: pointer;
    flex-shrink: 0;
  }

  .page-container:hover .nav-zone {
    opacity: 1;
  }

  .nav-zone:hover:not(:disabled) {
    color: var(--text-primary);
    background: var(--bg-surface);
  }

  .nav-zone:disabled {
    cursor: default;
    color: var(--text-muted);
    opacity: 0 !important;
  }

  .page-image-wrapper {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    min-height: 400px;
  }

  .page-image {
    max-width: 100%;
    max-height: calc(100vh - 200px);
    object-fit: contain;
    border-radius: var(--radius-sm);
  }

  .page-overlay {
    position: fixed;
    bottom: 90px;
    left: 50%;
    transform: translateX(-50%);
    background: rgba(0, 0, 0, 0.7);
    color: #fff;
    padding: 4px 12px;
    border-radius: var(--radius-sm);
    font-size: 0.8rem;
    font-variant-numeric: tabular-nums;
    pointer-events: none;
  }
</style>
