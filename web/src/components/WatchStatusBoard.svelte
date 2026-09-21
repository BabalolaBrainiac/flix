<script lang="ts">
  import {
    membership,
    refreshMembership,
    setStatus,
    clearStatus,
    idsOf,
    episodeLabel,
    backfillTitleIfMissing,
  } from '../lib/library';
  import type { WatchStatusSummary } from '../lib/types';
  import { openTitle, catalogItemOf } from '../lib/titles';
  import { CheckCircle2, Clock, PlayCircle, X, AlertCircle, User } from 'lucide-svelte';

  export let activeProfileId: string | null = null;
  export let onOpenProfilePicker: () => void = () => {};

  let errorMessage = '';
  let pendingId: string | null = null;

  $: allStatuses = $membership.statuses;
  $: isLoading = !$membership.loaded && !!activeProfileId;
  $: toWatchItems = allStatuses.filter((s) => s.status === 'to_watch');
  $: inProgressItems = allStatuses.filter((s) => s.status === 'in_progress');
  $: watchedItems = allStatuses.filter((s) => s.status === 'watched');

  // A status set before this app stored titles shows only its catalog id.
  // Look each one up once and fix it in place.
  $: for (const entry of allStatuses) {
    if (!entry.title) void backfillTitleIfMissing(activeProfileId, entry.catalog_id, entry.media_type || 'movie');
  }

  function formatTime(seconds: number | null): string {
    if (!seconds || seconds <= 0) return '';
    const mins = Math.floor(seconds / 60);
    const secs = seconds % 60;
    const hrs = Math.floor(mins / 60);
    if (hrs > 0) return `${hrs}h ${mins % 60}m`;
    return `${mins}m ${secs}s`;
  }

  async function handleMoveStatus(item: WatchStatusSummary, newStatus: string) {
    if (!activeProfileId) return;
    errorMessage = '';
    pendingId = item.catalog_id;
    try {
      await setStatus(activeProfileId, {
        catalogId: item.catalog_id,
        mediaType: item.media_type || 'movie',
        title: item.title,
        poster: item.poster,
        year: item.year,
        canonicalId: item.canonical_id,
      }, newStatus);
    } catch (e: any) {
      errorMessage = e.message || 'Failed to update watch status';
    } finally {
      pendingId = null;
    }
  }

  async function handleRemove(item: WatchStatusSummary) {
    if (!activeProfileId) return;
    errorMessage = '';
    pendingId = item.catalog_id;
    try {
      await clearStatus(activeProfileId, idsOf({ catalogId: item.catalog_id, canonicalId: item.canonical_id }));
    } catch (e: any) {
      errorMessage = e.message || 'Failed to remove watch status';
    } finally {
      pendingId = null;
    }
  }

  $: if (activeProfileId) {
    void refreshMembership(activeProfileId);
  }
</script>

<div class="watch-board-view">
  <div class="board-header">
    <div class="title-group">
      <PlayCircle size={22} class="header-icon" />
      <h2>Watch Status</h2>
    </div>
    <div class="summary-pills">
      <span class="pill to-watch">{toWatchItems.length} To Watch</span>
      <span class="pill in-progress">{inProgressItems.length} In Progress</span>
      <span class="pill watched">{watchedItems.length} Watched</span>
    </div>
  </div>

  {#if errorMessage}
    <div class="error-banner">
      <AlertCircle size={16} />
      <span>{errorMessage}</span>
    </div>
  {/if}

  {#if isLoading}
    <div class="state-message">Loading watch status board...</div>
  {:else if !activeProfileId}
    <div class="empty-board panel">
      <User size={36} class="empty-icon" />
      <h3>No Profile Selected</h3>
      <p>Create or select a profile to track your watch status.</p>
      <button type="button" class="cta-btn" on:click={onOpenProfilePicker}>
        Create Profile
      </button>
    </div>
  {:else}
    <div class="board-columns">
      <div class="board-column panel">
        <div class="column-header to-watch-header">
          <div class="header-left">
            <Clock size={16} />
            <h4>To Watch</h4>
          </div>
          <span class="count-tag">{toWatchItems.length}</span>
        </div>

        <div class="column-content">
          {#if toWatchItems.length === 0}
            <div class="empty-col">No titles to watch</div>
          {:else}
            {#each toWatchItems as item (item.catalog_id)}
              <div class="card panel">
                <div class="card-body">
                  {#if item.poster}
                    <img class="card-poster" src={item.poster} alt="" loading="lazy" decoding="async" />
                  {/if}
                  <button
                    type="button"
                    class="card-title-btn"
                    on:click={() => openTitle(catalogItemOf(item))}
                  >
                    <span class="item-title" title={item.title || item.catalog_id}>{item.title || item.catalog_id}</span>
                    {#if item.year}<span class="item-year">{item.year}</span>{/if}
                  </button>
                  <div class="card-actions">
                    <button type="button" class="move-btn" title="Mark In Progress" disabled={pendingId === item.catalog_id} on:click={() => handleMoveStatus(item, 'in_progress')}>Start</button>
                    <button type="button" class="move-btn" title="Mark Watched" disabled={pendingId === item.catalog_id} on:click={() => handleMoveStatus(item, 'watched')}>Done</button>
                    <button type="button" class="remove-btn" title="Remove status" disabled={pendingId === item.catalog_id} on:click={() => handleRemove(item)}>
                      <X size={13} />
                    </button>
                  </div>
                </div>
              </div>
            {/each}
          {/if}
        </div>
      </div>

      <div class="board-column panel">
        <div class="column-header in-progress-header">
          <div class="header-left">
            <PlayCircle size={16} />
            <h4>In Progress</h4>
          </div>
          <span class="count-tag">{inProgressItems.length}</span>
        </div>

        <div class="column-content">
          {#if inProgressItems.length === 0}
            <div class="empty-col">No titles in progress</div>
          {:else}
            {#each inProgressItems as item (item.catalog_id)}
              <div class="card panel highlight">
                <div class="card-body">
                  {#if item.poster}
                    <img class="card-poster" src={item.poster} alt="" loading="lazy" decoding="async" />
                  {/if}
                  <button
                    type="button"
                    class="card-title-btn"
                    on:click={() => openTitle(catalogItemOf(item))}
                  >
                    <span class="item-title" title={item.title || item.catalog_id}>{item.title || item.catalog_id}</span>
                    {#if item.year}<span class="item-year">{item.year}</span>{/if}
                    {#if episodeLabel(item.episode_id)}
                      <span class="episode-tag">Last played {episodeLabel(item.episode_id)}</span>
                    {/if}
                    {#if item.resume_seconds}
                      <span class="resume-tag">Resume at {formatTime(item.resume_seconds)}</span>
                    {/if}
                  </button>
                  <div class="card-actions">
                    <button type="button" class="move-btn" title="Move back to To Watch" disabled={pendingId === item.catalog_id} on:click={() => handleMoveStatus(item, 'to_watch')}>Reset</button>
                    <button type="button" class="move-btn complete" title="Mark Watched" disabled={pendingId === item.catalog_id} on:click={() => handleMoveStatus(item, 'watched')}>Finish</button>
                    <button type="button" class="remove-btn" title="Remove status" disabled={pendingId === item.catalog_id} on:click={() => handleRemove(item)}>
                      <X size={13} />
                    </button>
                  </div>
                </div>
              </div>
            {/each}
          {/if}
        </div>
      </div>

      <div class="board-column panel">
        <div class="column-header watched-header">
          <div class="header-left">
            <CheckCircle2 size={16} />
            <h4>Watched</h4>
          </div>
          <span class="count-tag">{watchedItems.length}</span>
        </div>

        <div class="column-content">
          {#if watchedItems.length === 0}
            <div class="empty-col">No completed titles</div>
          {:else}
            {#each watchedItems as item (item.catalog_id)}
              <div class="card panel">
                <div class="card-body">
                  {#if item.poster}
                    <img class="card-poster" src={item.poster} alt="" loading="lazy" decoding="async" />
                  {/if}
                  <button
                    type="button"
                    class="card-title-btn"
                    on:click={() => openTitle(catalogItemOf(item))}
                  >
                    <span class="item-title completed" title={item.title || item.catalog_id}>{item.title || item.catalog_id}</span>
                    {#if item.year}<span class="item-year">{item.year}</span>{/if}
                  </button>
                  <div class="card-actions">
                    <button type="button" class="move-btn" title="Watch again" disabled={pendingId === item.catalog_id} on:click={() => handleMoveStatus(item, 'in_progress')}>Replay</button>
                    <button type="button" class="remove-btn" title="Remove status" disabled={pendingId === item.catalog_id} on:click={() => handleRemove(item)}>
                      <X size={13} />
                    </button>
                  </div>
                </div>
              </div>
            {/each}
          {/if}
        </div>
      </div>
    </div>
  {/if}
</div>

<style>
  .watch-board-view {
    max-width: 1320px;
    margin: 0 auto;
    padding: 28px;
  }

  .board-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 24px;
    flex-wrap: wrap;
    gap: 16px;
  }

  .title-group {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .title-group h2 {
    font-size: 1.4rem;
    font-weight: 700;
  }

  :global(.header-icon) {
    color: var(--accent-primary);
  }

  .summary-pills {
    display: flex;
    gap: 8px;
  }

  .pill {
    padding: 4px 10px;
    border-radius: var(--radius-sm);
    font-size: 0.75rem;
    font-weight: 600;
    font-family: var(--font-mono);
  }

  .pill.to-watch {
    background: var(--bg-surface-elevated);
    color: var(--text-secondary);
    border: 1px solid var(--border-subtle);
  }

  .pill.in-progress {
    background: var(--accent-muted);
    color: var(--accent-primary);
    border: 1px solid var(--accent-border);
  }

  .pill.watched {
    background: rgba(16, 185, 129, 0.12);
    color: var(--status-green);
    border: 1px solid rgba(16, 185, 129, 0.3);
  }

  .error-banner {
    display: flex;
    align-items: center;
    gap: 8px;
    background: rgba(239, 68, 68, 0.12);
    border: 1px solid rgba(239, 68, 68, 0.35);
    color: #fca5a5;
    padding: 10px 16px;
    border-radius: var(--radius-sm);
    font-size: 0.84rem;
    margin-bottom: 20px;
  }

  .state-message {
    padding: 64px 20px;
    text-align: center;
    color: var(--text-muted);
    font-size: 0.95rem;
  }

  .board-columns {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 20px;
    align-items: flex-start;
  }

  .board-column {
    background: var(--bg-secondary);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    overflow: hidden;
  }

  .column-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 14px 18px;
    border-bottom: 1px solid var(--border-subtle);
    background: var(--bg-surface);
  }

  .header-left {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .header-left h4 {
    font-size: 0.92rem;
    font-weight: 700;
  }

  .to-watch-header { color: var(--text-secondary); }
  .in-progress-header { color: var(--accent-primary); }
  .watched-header { color: var(--status-green); }

  .count-tag {
    font-size: 0.72rem;
    font-weight: 700;
    padding: 2px 7px;
    border-radius: var(--radius-sm);
    background: var(--bg-primary);
    color: var(--text-muted);
    font-family: var(--font-mono);
  }

  .column-content {
    padding: 14px;
    display: flex;
    flex-direction: column;
    gap: 10px;
    min-height: 280px;
  }

  .empty-col {
    padding: 36px 12px;
    text-align: center;
    color: var(--text-muted);
    font-size: 0.82rem;
  }

  .card {
    background: var(--bg-surface);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    padding: 12px 14px;
    transition: all var(--transition-fast);
  }

  .card:hover {
    border-color: var(--border-muted);
  }

  .card.highlight {
    border-left: 3px solid var(--accent-primary);
  }

  .card-body {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .card-poster {
    width: 34px;
    aspect-ratio: 2 / 3;
    object-fit: cover;
    border-radius: var(--radius-xs);
    flex-shrink: 0;
    background: var(--bg-primary);
  }

  .card-title-btn {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 4px;
    text-align: left;
    background: transparent;
    min-width: 0;
  }

  .item-title {
    font-size: 0.88rem;
    font-weight: 600;
    color: var(--text-primary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .item-title.completed {
    color: var(--text-secondary);
  }

  .item-year {
    font-size: 0.72rem;
    color: var(--text-muted);
    font-family: var(--font-mono);
  }

  .resume-tag {
    font-size: 0.72rem;
    color: var(--accent-primary);
    font-family: var(--font-mono);
  }

  .episode-tag {
    color: var(--text-secondary);
    font-family: var(--font-mono);
    font-size: 0.72rem;
    font-weight: 700;
  }

  .card-actions {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-shrink: 0;
  }

  .move-btn {
    padding: 4px 8px;
    border-radius: var(--radius-sm);
    font-size: 0.72rem;
    font-weight: 700;
    background: var(--bg-surface-elevated);
    color: var(--text-secondary);
    border: 1px solid var(--border-subtle);
  }

  .move-btn:hover:not(:disabled) {
    color: var(--text-primary);
    border-color: var(--border-muted);
  }

  .move-btn.complete {
    color: var(--status-green);
    border-color: rgba(16, 185, 129, 0.3);
    background: rgba(16, 185, 129, 0.1);
  }

  .remove-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 22px;
    height: 22px;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--text-muted);
    border: 1px solid transparent;
  }

  .remove-btn:hover:not(:disabled) {
    color: #fca5a5;
    border-color: rgba(239, 68, 68, 0.35);
  }

  button:disabled {
    opacity: 0.5;
    cursor: wait;
  }

  .empty-board {
    display: flex;
    flex-direction: column;
    align-items: center;
    text-align: center;
    padding: 64px 24px;
    margin: 0 auto;
    max-width: 460px;
  }

  .empty-board h3 {
    font-size: 1.25rem;
    margin-bottom: 6px;
  }

  .empty-board p {
    color: var(--text-secondary);
    font-size: 0.9rem;
    margin-bottom: 16px;
  }

  .cta-btn {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 8px 16px;
    background: var(--action-fill);
    color: #fff;
    font-weight: 700;
    font-size: 0.84rem;
    border-radius: var(--radius-sm);
  }

  .cta-btn:hover {
    background: var(--action-fill-hover);
  }

  @media (max-width: 900px) {
    .board-columns {
      grid-template-columns: 1fr;
    }
  }
</style>
