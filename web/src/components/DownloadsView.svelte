<script lang="ts">
  import { onMount } from 'svelte';
  import { getDownloads, addDownload } from '../lib/api';
  import type { DownloadEntrySummary } from '../lib/types';
  import { Download, RefreshCw, Plus, HardDrive, AlertCircle } from 'lucide-svelte';

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
      message = 'Torrent added to download library.';
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
      <h1 class="page-title">Downloads</h1>
      <p class="page-subtitle">Manage downloaded torrents and background library files.</p>
    </div>
    <button class="refresh-btn" on:click={loadDownloads} disabled={isLoading}>
      <RefreshCw size={14} class={isLoading ? 'spinning' : ''} /> Refresh
    </button>
  </div>

  <form class="add-magnet-card panel" on:submit|preventDefault={handleAddMagnet}>
    <h3 class="card-heading">Add Magnet Link or Info Hash</h3>
    <div class="input-row">
      <input
        type="text"
        bind:value={magnetInput}
        placeholder="magnet:?xt=urn:btih:... or 40-character info hash"
        class="magnet-input"
      />
      <button type="submit" class="add-btn" disabled={isAdding || !magnetInput.trim()}>
        <Plus size={16} /> Add Download
      </button>
    </div>
  </form>

  {#if message}
    <div class="alert-banner {isError ? 'error' : 'success'}">
      <span>{message}</span>
    </div>
  {/if}

  <div class="entries-section">
    <h2 class="section-title">Saved Items ({entries.length})</h2>

    {#if isLoading}
      <div class="loading-box">
        <span>Loading library items...</span>
      </div>
    {:else if entries.length > 0}
      <div class="entries-list">
        {#each entries as entry}
          <div class="entry-card panel">
            <HardDrive size={24} class="entry-icon" />
            <div class="entry-main">
              <div class="entry-title">{entry.display_name}</div>
              <div class="entry-sub">
                {#if entry.title}
                  <span class="badge badge-recommended">{entry.title}</span>
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
      <div class="empty-downloads panel">
        <Download size={36} class="empty-icon" />
        <p class="empty-text">No downloads saved in the library yet.</p>
        <p class="empty-help">Add a magnet link above or select "Download" from search results.</p>
      </div>
    {/if}
  </div>
</div>

<style>
  .downloads-view {
    max-width: 960px;
    margin: 0 auto;
    padding: 32px 24px 80px;
  }

  .downloads-header {
    display: flex;
    justify-content: space-between;
    align-items: flex-end;
    margin-bottom: 24px;
  }

  .page-title {
    font-size: 1.8rem;
    font-weight: 700;
    margin-bottom: 4px;
  }

  .page-subtitle {
    color: var(--text-secondary);
    font-size: 0.9rem;
  }

  .refresh-btn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    background: var(--bg-surface);
    border: 1px solid var(--border-subtle);
    color: var(--text-secondary);
    padding: 6px 14px;
    border-radius: var(--radius-sm);
    font-size: 0.85rem;
  }

  .refresh-btn:hover:not(:disabled) {
    color: var(--text-primary);
    background: var(--bg-surface-active);
  }

  .add-magnet-card {
    padding: 16px 20px;
    margin-bottom: 24px;
  }

  .card-heading {
    font-size: 0.9rem;
    font-weight: 600;
    margin-bottom: 10px;
    color: var(--text-secondary);
  }

  .input-row {
    display: flex;
    gap: 10px;
  }

  .magnet-input {
    flex: 1;
    background: var(--bg-secondary);
    border: 1px solid var(--border-subtle);
    padding: 8px 14px;
    border-radius: var(--radius-sm);
    color: var(--text-primary);
    font-size: 0.85rem;
  }

  .magnet-input:focus {
    border-color: var(--border-focus);
  }

  .add-btn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    background: var(--accent-primary);
    color: #fff;
    padding: 8px 16px;
    border-radius: var(--radius-sm);
    font-weight: 600;
    font-size: 0.85rem;
  }

  .add-btn:hover:not(:disabled) {
    background: var(--accent-primary-hover);
  }

  .add-btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .alert-banner {
    padding: 10px 16px;
    border-radius: var(--radius-sm);
    margin-bottom: 20px;
    font-size: 0.85rem;
  }

  .alert-banner.success {
    background: rgba(34, 197, 94, 0.15);
    border: 1px solid rgba(34, 197, 94, 0.3);
    color: var(--status-green);
  }

  .alert-banner.error {
    background: rgba(239, 68, 68, 0.15);
    border: 1px solid rgba(239, 68, 68, 0.3);
    color: #fca5a5;
  }

  .section-title {
    font-size: 1.1rem;
    font-weight: 600;
    margin-bottom: 14px;
  }

  .entries-list {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .entry-card {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 14px 18px;
  }

  :global(.entry-icon) {
    color: var(--text-muted);
    flex-shrink: 0;
  }

  .entry-main {
    flex: 1;
    min-width: 0;
  }

  .entry-title {
    font-size: 0.9rem;
    font-weight: 600;
    margin-bottom: 4px;
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
    background: var(--bg-secondary);
    padding: 2px 6px;
    border-radius: var(--radius-sm);
  }

  .empty-downloads {
    text-align: center;
    padding: 48px 24px;
  }

  :global(.empty-icon) {
    color: var(--text-muted);
    margin-bottom: 10px;
  }

  .empty-text {
    font-weight: 600;
    font-size: 0.95rem;
    margin-bottom: 4px;
  }

  .empty-help {
    font-size: 0.85rem;
    color: var(--text-muted);
  }

  .loading-box {
    padding: 36px;
    text-align: center;
    color: var(--text-secondary);
  }

  :global(.spinning) {
    animation: spin 1s linear infinite;
  }

  @keyframes spin {
    to { transform: rotate(360deg); }
  }
</style>
