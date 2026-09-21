<script lang="ts">
  import {
    membership,
    refreshMembership,
    listNameTaken,
    createList,
    deleteList,
    removeFromList,
    idsOf,
    backfillTitleIfMissing,
  } from '../lib/library';
  import { openTitle, catalogItemOf } from '../lib/titles';
  import { Bookmark, Plus, Trash2, X, Film, Tv, Sparkles, AlertCircle, User } from 'lucide-svelte';
  import ConfirmationDialog from './ConfirmationDialog.svelte';
  import { confirmationFor } from '../lib/confirmation';

  export let activeProfileId: string | null = null;
  export let onOpenProfilePicker: () => void = () => {};

  let selectedListId: string | null = null;
  let errorMessage = '';
  let newListName = '';
  let isCreating = false;
  let isSaving = false;
  let pendingDeleteList: { id: string; name: string } | null = null;

  $: lists = $membership.lists;
  $: isLoading = !$membership.loaded && !!activeProfileId;
  $: activeList = lists.find((l) => l.id === selectedListId) || lists[0] || null;
  $: nameTaken = newListName.trim() !== '' && listNameTaken($membership, newListName);

  $: if (activeProfileId) {
    void refreshMembership(activeProfileId);
  }

  // A title added before this app stored titles shows only its catalog id.
  // Look each one up once and fix it in place.
  $: for (const item of activeList?.items ?? []) {
    if (!item.title) void backfillTitleIfMissing(activeProfileId, item.catalog_id, item.media_type);
  }

  async function handleCreateList() {
    if (!activeProfileId || !newListName.trim() || nameTaken) return;
    errorMessage = '';
    isSaving = true;
    try {
      const created = await createList(activeProfileId, newListName);
      newListName = '';
      isCreating = false;
      if (created) selectedListId = created.id;
    } catch (e: any) {
      errorMessage = e.message || 'Failed to create list';
    } finally {
      isSaving = false;
    }
  }

  async function handleDeleteList(listId: string) {
    if (!activeProfileId) return;
    errorMessage = '';
    try {
      await deleteList(activeProfileId, listId);
      if (selectedListId === listId) selectedListId = null;
    } catch (e: any) {
      errorMessage = e.message || 'Failed to delete list';
    }
  }

  function confirmDeleteList() {
    if (!pendingDeleteList) return;
    const { id } = pendingDeleteList;
    pendingDeleteList = null;
    void handleDeleteList(id);
  }

  async function handleRemoveItem(listId: string, item: { catalog_id: string; canonical_id?: string | null }) {
    errorMessage = '';
    try {
      await removeFromList(listId, idsOf({ catalogId: item.catalog_id, canonicalId: item.canonical_id }));
    } catch (e: any) {
      errorMessage = e.message || 'Failed to remove item';
    }
  }
</script>

<div class="lists-view">
  <div class="lists-header">
    <div class="title-group">
      <Bookmark size={22} class="header-icon" />
      <h2>My Lists</h2>
    </div>

    {#if !isCreating}
      <button
        type="button"
        class="create-btn"
        on:click={() => (isCreating = true)}
        disabled={!activeProfileId}
      >
        <Plus size={16} />
        <span>New List</span>
      </button>
    {/if}
  </div>

  {#if isCreating}
    <div class="create-bar panel">
      <input
        type="text"
        bind:value={newListName}
        placeholder="Enter list title..."
        class="create-input"
        on:keydown={(e) => e.key === 'Enter' && handleCreateList()}
      />
      <div class="create-actions">
        <button
          type="button"
          class="btn-confirm"
          on:click={handleCreateList}
          disabled={!newListName.trim() || nameTaken || isSaving}
        >
          Create List
        </button>
        <button
          type="button"
          class="btn-cancel"
          on:click={() => {
            isCreating = false;
            newListName = '';
          }}
        >
          Cancel
        </button>
      </div>
      {#if nameTaken}
        <div class="name-hint">A list with that name already exists.</div>
      {/if}
    </div>
  {/if}

  {#if errorMessage}
    <div class="error-banner">
      <AlertCircle size={16} />
      <span>{errorMessage}</span>
    </div>
  {/if}

  {#if isLoading}
    <div class="state-message">Loading custom lists...</div>
  {:else if !activeProfileId}
    <div class="empty-lists panel">
      <User size={36} class="empty-icon" />
      <h3>No Profile Selected</h3>
      <p>Create or select a profile to manage your custom lists.</p>
      <button
        type="button"
        class="create-btn center"
        on:click={onOpenProfilePicker}
      >
        <Plus size={16} />
        <span>Create Profile</span>
      </button>
    </div>
  {:else if lists.length === 0}
    <div class="empty-lists panel">
      <Bookmark size={36} class="empty-icon" />
      <h3>No Lists Created Yet</h3>
      <p>Organize your favorite movies, series, and anime into custom collections.</p>
      <button
        type="button"
        class="create-btn center"
        on:click={() => (isCreating = true)}
      >
        <Plus size={16} />
        <span>Create First List</span>
      </button>
    </div>
  {:else}
    <div class="lists-layout">
      <!-- Sidebar List Tabs -->
      <aside class="lists-sidebar panel">
        <div class="sidebar-header">Lists ({lists.length})</div>
        <div class="sidebar-nav">
          {#each lists as list}
            <div class="sidebar-item-row {list.id === activeList?.id ? 'active' : ''}">
              <button
                type="button"
                class="list-tab-btn"
                on:click={() => (selectedListId = list.id)}
              >
                <span class="tab-name">{list.name}</span>
                <span class="tab-badge">{list.items.length}</span>
              </button>
              <button
                type="button"
                class="delete-list-btn"
                title="Delete list"
                on:click={() => (pendingDeleteList = { id: list.id, name: list.name })}
              >
                <Trash2 size={13} />
              </button>
            </div>
          {/each}
        </div>
      </aside>

      <!-- List Items Grid -->
      <section class="list-detail-section">
        {#if activeList}
          <div class="list-detail-header">
            <div class="list-title-wrap">
              <h3>{activeList.name}</h3>
              <span class="count-pill">{activeList.items.length} titles</span>
            </div>
          </div>

          {#if activeList.items.length === 0}
            <div class="empty-items-box panel">
              <p>This list is empty. Add titles from Search or Discover to populate it.</p>
            </div>
          {:else}
            <div class="items-grid">
              {#each activeList.items as item (item.catalog_id)}
                <div class="item-card panel">
                  <div class="item-card-inner">
                    <button
                      type="button"
                      class="card-click-area"
                      on:click={() => openTitle(catalogItemOf(item))}
                    >
                      <div class="item-icon-box">
                        {#if item.poster}
                          <img src={item.poster} alt="" class="item-poster" loading="lazy" decoding="async" />
                        {:else if item.media_type === 'movie'}
                          <Film size={20} />
                        {:else if item.media_type === 'series'}
                          <Tv size={20} />
                        {:else}
                          <Sparkles size={20} />
                        {/if}
                      </div>
                      <div class="item-info">
                        <div class="item-id" title={item.title || item.catalog_id}>{item.title || item.catalog_id}</div>
                        <div class="item-type">
                          {item.media_type}{#if item.year}<span class="item-year"> · {item.year}</span>{/if}
                        </div>
                      </div>
                    </button>
                    <button
                      type="button"
                      class="remove-item-btn"
                      title="Remove from list"
                      on:click={() => handleRemoveItem(activeList.id, item)}
                    >
                      <X size={15} />
                    </button>
                  </div>
                </div>
              {/each}
            </div>
          {/if}
        {/if}
      </section>
    </div>
  {/if}
</div>

{#if pendingDeleteList}
  {@const copy = confirmationFor('list', pendingDeleteList.name)}
  <ConfirmationDialog {...copy} onCancel={() => (pendingDeleteList = null)} onConfirm={confirmDeleteList} />
{/if}

<style>
  .lists-view {
    max-width: 1320px;
    margin: 0 auto;
    padding: 28px;
  }

  .lists-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 24px;
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

  .create-btn {
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

  .create-btn:hover:not(:disabled) {
    background: var(--action-fill-hover);
  }

  .create-btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .create-btn.center {
    margin-top: 16px;
  }

  .create-bar {
    display: flex;
    gap: 12px;
    padding: 14px 18px;
    margin-bottom: 24px;
    align-items: center;
  }

  .create-input {
    flex: 1;
    height: 40px;
    padding: 0 14px;
    background: var(--bg-surface-elevated);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    color: var(--text-primary);
    font-size: 0.9rem;
  }

  .create-input:focus {
    border-color: var(--accent-primary);
  }

  .create-actions {
    display: flex;
    gap: 8px;
  }

  .btn-confirm {
    padding: 8px 16px;
    background: var(--action-fill);
    color: #fff;
    font-weight: 700;
    font-size: 0.82rem;
    border-radius: var(--radius-sm);
  }

  .btn-confirm:hover:not(:disabled) {
    background: var(--action-fill-hover);
  }

  .btn-confirm:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .btn-cancel {
    padding: 8px 14px;
    background: var(--bg-surface-elevated);
    color: var(--text-secondary);
    font-size: 0.82rem;
    font-weight: 600;
    border-radius: var(--radius-sm);
  }

  .name-hint {
    margin-top: 8px;
    font-size: 0.78rem;
    color: var(--status-red);
  }

  .item-year {
    color: var(--text-muted);
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

  .empty-lists {
    display: flex;
    flex-direction: column;
    align-items: center;
    text-align: center;
    padding: 64px 24px;
  }

  :global(.empty-icon) {
    color: var(--text-muted);
    margin-bottom: 16px;
  }

  .empty-lists h3 {
    font-size: 1.25rem;
    margin-bottom: 6px;
  }

  .empty-lists p {
    color: var(--text-secondary);
    font-size: 0.9rem;
    max-width: 420px;
  }

  .lists-layout {
    display: grid;
    grid-template-columns: 260px 1fr;
    gap: 24px;
    align-items: flex-start;
  }

  .lists-sidebar {
    padding: 14px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .sidebar-header {
    font-size: 0.72rem;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--text-muted);
    padding: 4px 6px;
    font-family: var(--font-mono);
  }

  .sidebar-nav {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .sidebar-item-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    border-radius: var(--radius-sm);
    padding: 2px 4px;
    transition: all var(--transition-fast);
  }

  .sidebar-item-row:hover {
    background: var(--bg-surface-elevated);
  }

  .sidebar-item-row.active {
    background: var(--bg-surface-elevated);
    border-left: 3px solid var(--accent-primary);
  }

  .list-tab-btn {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 8px 10px;
    background: transparent;
    color: var(--text-secondary);
    font-size: 0.88rem;
    font-weight: 500;
    text-align: left;
    min-width: 0;
  }

  .sidebar-item-row.active .list-tab-btn {
    color: var(--text-primary);
    font-weight: 600;
  }

  .tab-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .tab-badge {
    font-size: 0.7rem;
    font-weight: 700;
    padding: 1px 6px;
    border-radius: var(--radius-sm);
    background: var(--bg-primary);
    color: var(--text-muted);
    font-family: var(--font-mono);
  }

  .delete-list-btn {
    padding: 6px;
    background: transparent;
    color: var(--text-muted);
    opacity: 0.4;
    border-radius: var(--radius-sm);
  }

  .delete-list-btn:hover {
    opacity: 1;
    color: var(--status-red);
  }

  .list-detail-section {
    display: flex;
    flex-direction: column;
    gap: 16px;
  }

  .list-detail-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }

  .list-title-wrap {
    display: flex;
    align-items: baseline;
    gap: 10px;
  }

  .list-title-wrap h3 {
    font-size: 1.2rem;
    font-weight: 700;
  }

  .count-pill {
    font-size: 0.75rem;
    color: var(--text-muted);
    font-family: var(--font-mono);
  }

  .empty-items-box {
    padding: 48px 20px;
    text-align: center;
    color: var(--text-muted);
    font-size: 0.88rem;
  }

  .items-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(260px, 1fr));
    gap: 12px;
  }

  .item-card {
    padding: 12px 14px;
    background: var(--bg-surface);
    transition: all var(--transition-fast);
  }

  .item-card:hover {
    border-color: var(--border-muted);
  }

  .item-card-inner {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }

  .card-click-area {
    display: flex;
    align-items: center;
    gap: 10px;
    background: transparent;
    flex: 1;
    text-align: left;
    min-width: 0;
  }

  .item-icon-box {
    width: 36px;
    height: 36px;
    border-radius: var(--radius-sm);
    background: var(--bg-surface-elevated);
    border: 1px solid var(--border-subtle);
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--accent-primary);
    flex-shrink: 0;
    overflow: hidden;
  }

  .item-poster {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }

  .item-info {
    min-width: 0;
  }

  .item-id {
    font-size: 0.86rem;
    font-weight: 600;
    color: var(--text-primary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .item-type {
    font-size: 0.72rem;
    color: var(--text-muted);
    text-transform: capitalize;
    font-family: var(--font-mono);
  }

  .remove-item-btn {
    padding: 6px;
    background: transparent;
    color: var(--text-muted);
    border-radius: var(--radius-sm);
  }

  .remove-item-btn:hover {
    color: var(--status-red);
    background: var(--bg-surface-hover);
  }

  @media (max-width: 768px) {
    .lists-layout {
      grid-template-columns: 1fr;
    }
  }
</style>
