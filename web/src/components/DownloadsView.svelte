<script lang="ts">
  import { onMount } from 'svelte';
  import { getDownloads, addDownload } from '../lib/api';
  import type { DownloadEntrySummary } from '../lib/types';

  let entries: DownloadEntrySummary[] = [];
  let isLoading = false;
  let magnetInput = '';
  let isAdding = false;
  let message = '';
  let isError = false;

  async function loadDownloads() {
    isLoading = true;
    try {
      const res = await getDownloads();
      entries = res.entries;
    } catch (e: any) {
      message = `Failed to load downloads: ${e.message}`;
      isError = true;
    } finally {
      isLoading = false;
    }
  }

  async function handleAddMagnet() {
    if (!magnetInput.trim()) return;
    isAdding = true;
    message = '';
    isError = false;
    try {
      const res = await addDownload(magnetInput.trim());
      entries = res.entries;
      magnetInput = '';
      message = 'Torrent added to download library!';
    } catch (e: any) {
      message = `Failed to add torrent: ${e.message}`;
      isError = true;
    } finally {
      isAdding = false;
    }
  }

  onMount(() => {
    loadDownloads();
  });
</script>

<div class="downloads-view">
  <div class="downloads-header">
    <div>
      <h1 class="page-title">Downloads & Library</h1>
      <p class="page-subtitle">Manage downloaded torrents and background library items.</p>
    </div>
    <button class="refresh-btn" on:click={loadDownloads} disabled={isLoading}>
      🔄 Refresh
    </button>
  </div>

  <form class="add-magnet-card glass-panel" on:submit|preventDefault={handleAddMagnet}>
    <h3 class="card-heading">Add Magnet Link or Hash</h3>
    <div class="input-row">
      <input
        type="text"
        bind:value={magnetInput}
        placeholder="magnet:?xt=urn:btih:... or 40-character info hash"
        class="magnet-input"
      />
      <button type="submit" class="add-btn" disabled={isAdding || !magnetInput.trim()}>
        {#if isAdding}
          <span class="spinner"></span>
        {:else}
          <span>+ Add Download</span>
        {/if}
      </button>
    </div>
  </form>

  {#if message}
    <div class="alert-banner {isError ? 'error' : 'success'} glass-panel">
      {message}
    </div>
  {/if}

  <div class="entries-section">
    <h2 class="section-title">Saved Downloads ({entries.length})</h2>

    {#if isLoading}
      <div class="loading-box">
        <span class="spinner large"></span>
        <span>Loading library items...</span>
      </div>
    {:else if entries.length > 0}
      <div class="entries-list">
        {#each entries as entry}
          <div class="entry-card glass-panel">
            <div class="entry-icon">📦</div>
            <div class="entry-main">
              <div class="entry-title">{entry.display_name}</div>
              <div class="entry-sub">
                {#if entry.title}
                  <span class="badge badge-accent">{entry.title}</span>
                {/if}
                {#if entry.year}
                  <span class="badge">{entry.year}</span>
                {/if}
                <code class="hash-tag">{entry.info_hash}</code>
              </div>
            </div>
          </div>
        {/each}
      </div>
    {:else}
      <div class="empty-downloads glass-panel">
        <div class="empty-emoji">📂</div>
        <p class="empty-text">No downloads saved in the library yet.</p>
        <p class="empty-help">Add a magnet link above or select "Download" from search results.</p>
      </div>
    {/if}
  </div>
</div>

<style>
  .downloads-view {
    max-width: 1000px;
    margin: 0 auto;
    padding: 32px 24px 80px;
  }

  .downloads-header {
    display: flex;
    justify-content: space-between;
    align-items: flex-end;
    margin-bottom: 28px;
  }

  .page-title {
    font-size: 2.25rem;
    font-weight: 800;
    margin-bottom: 6px;
    background: var(--accent-gradient);
    -webkit-background-clip: text;
    background-clip: text;
    -webkit-text-fill-color: transparent;

  }

  .page-subtitle {
    color: var(--text-secondary);
    font-size: 0.95rem;
  }

  .refresh-btn {
    background: rgba(23, 31, 48, 0.6);
    border: 1px solid var(--border-subtle);
    color: var(--text-secondary);
    padding: 8px 16px;
    border-radius: var(--radius-sm);
    font-size: 0.85rem;
  }

  .refresh-btn:hover:not(:disabled) {
    color: var(--text-primary);
    background: rgba(255, 255, 255, 0.08);
  }

  .add-magnet-card {
    padding: 20px 24px;
    margin-bottom: 24px;
    border-radius: var(--radius-md);
  }

  .card-heading {
    font-size: 1rem;
    font-weight: 600;
    margin-bottom: 12px;
    color: var(--text-secondary);
  }

  .input-row {
    display: flex;
    gap: 12px;
  }

  .magnet-input {
    flex: 1;
    background: rgba(8, 11, 17, 0.7);
    border: 1px solid var(--border-subtle);
    padding: 10px 16px;
    border-radius: var(--radius-sm);
    color: var(--text-primary);
    font-size: 0.9rem;
  }

  .magnet-input:focus {
    border-color: var(--border-focus);
  }

  .add-btn {
    background: var(--accent-primary);
    color: #fff;
    padding: 10px 20px;
    border-radius: var(--radius-sm);
    font-weight: 600;
    font-size: 0.9rem;
    box-shadow: var(--accent-glow);
  }

  .add-btn:hover:not(:disabled) {
    background: var(--accent-primary-hover);
  }

  .add-btn:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }

  .alert-banner {
    padding: 12px 18px;
    border-radius: var(--radius-sm);
    margin-bottom: 24px;
    font-size: 0.9rem;
  }

  .alert-banner.success {
    background: rgba(16, 185, 129, 0.15);
    border: 1px solid rgba(16, 185, 129, 0.3);
    color: #6ee7b7;
  }

  .alert-banner.error {
    background: rgba(239, 68, 68, 0.15);
    border: 1px solid rgba(239, 68, 68, 0.3);
    color: #fca5a5;
  }

  .section-title {
    font-size: 1.25rem;
    font-weight: 700;
    margin-bottom: 16px;
  }

  .entries-list {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .entry-card {
    display: flex;
    align-items: center;
    gap: 16px;
    padding: 16px 20px;
    border-radius: var(--radius-md);
  }

  .entry-icon {
    font-size: 1.75rem;
  }

  .entry-main {
    flex: 1;
    min-width: 0;
  }

  .entry-title {
    font-size: 0.95rem;
    font-weight: 600;
    margin-bottom: 6px;
  }

  .entry-sub {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }

  .hash-tag {
    font-size: 0.75rem;
    color: var(--text-muted);
    background: rgba(0, 0, 0, 0.3);
    padding: 2px 6px;
    border-radius: 4px;
  }

  .empty-downloads {
    text-align: center;
    padding: 48px 24px;
    border-radius: var(--radius-md);
  }

  .empty-emoji {
    font-size: 2.5rem;
    margin-bottom: 12px;
  }

  .empty-text {
    font-weight: 600;
    margin-bottom: 4px;
  }

  .empty-help {
    font-size: 0.85rem;
    color: var(--text-muted);
  }

  .loading-box {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 12px;
    padding: 40px;
    color: var(--text-secondary);
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
</style>
