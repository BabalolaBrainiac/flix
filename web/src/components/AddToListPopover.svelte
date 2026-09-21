<script lang="ts">
  import { onMount } from 'svelte';
  import {
    membership,
    refreshMembership,
    findStatus,
    idsOf,
    listsContaining,
    listNameTaken,
    setStatus,
    clearStatus,
    addToList,
    removeFromList,
    createList,
  } from '../lib/library';
  import type { ListSummary, TitleRef } from '../lib/types';
  import { Bookmark, Check, Plus, X, Clock, PlayCircle, CheckCircle2, User } from 'lucide-svelte';

  export let titleRef: TitleRef;
  export let activeProfileId: string | null = null;
  export let onClose: () => void;
  export let onOpenProfilePicker: () => void = () => {};

  let isSaving = false;
  let errorMessage = '';
  let showCreateInput = false;
  let newListName = '';
  let panel: HTMLElement;

  $: ids = idsOf(titleRef);
  $: currentStatus = findStatus($membership, ids)?.status ?? null;
  $: memberLists = new Set(listsContaining($membership, ids).map((list) => list.id));
  $: nameTaken = newListName.trim() !== '' && listNameTaken($membership, newListName);
  $: lists = $membership.lists;

  // Runs one change. The store shows it at once and restores the old state
  // when the change fails, so this function only reports the failure.
  async function run(action: () => Promise<unknown>, failure: string) {
    if (!activeProfileId || isSaving) return;
    isSaving = true;
    errorMessage = '';
    try {
      await action();
    } catch (e: any) {
      errorMessage = e.message || failure;
    } finally {
      isSaving = false;
    }
  }

  function handleToggleList(list: ListSummary) {
    return run(
      () => (memberLists.has(list.id) ? removeFromList(list.id, ids) : addToList(list.id, titleRef)),
      'The list did not change. Try again.',
    );
  }

  function handleSetStatus(status: string) {
    if (currentStatus === status) return Promise.resolve();
    return run(() => setStatus(activeProfileId!, titleRef, status), 'The status did not save. Try again.');
  }

  function handleClearStatus() {
    return run(() => clearStatus(activeProfileId!, ids), 'The status was not removed. Try again.');
  }

  async function handleCreateAndAdd() {
    if (!newListName.trim()) return;
    if (nameTaken) {
      errorMessage = 'A list with that name already exists.';
      return;
    }
    await run(async () => {
      const created = await createList(activeProfileId!, newListName);
      newListName = '';
      showCreateInput = false;
      if (created) await addToList(created.id, titleRef);
    }, 'The list was not created. Try again.');
  }

  function handleKeydown(event: KeyboardEvent) {
    if (event.key === 'Escape') onClose();
  }

  onMount(() => {
    if (activeProfileId && !$membership.loaded) void refreshMembership(activeProfileId);
    panel?.focus();
  });
</script>

<svelte:window on:keydown={handleKeydown} />

<div
  class="popover-backdrop"
  role="presentation"
  on:click|self={onClose}
>
  <div class="popover-panel panel" role="dialog" aria-modal="true" aria-label="Lists and watch status" tabindex="-1" bind:this={panel}>
    <div class="popover-header">
      <div class="header-title">
        <Bookmark size={16} class="title-icon" />
        <h3>Add to List / Status</h3>
      </div>
      <button class="close-btn" type="button" aria-label="Close lists and status" on:click={onClose}><X size={16} /></button>
    </div>

    {#if errorMessage}
      <div class="error-text">{errorMessage}</div>
    {/if}

    {#if !$membership.loaded && activeProfileId}
      <div class="popover-loading">Loading...</div>
    {:else if !activeProfileId}
      <div class="no-profile-cta">
        <div class="no-profile-icon-wrap">
          <User size={24} />
        </div>
        <h4>No Profile Selected</h4>
        <p>You need a profile to save titles to custom lists or track watch status.</p>
        <button
          type="button"
          class="btn-create-profile"
          on:click={() => {
            onClose();
            onOpenProfilePicker();
          }}
        >
          <Plus size={15} />
          <span>Create or Select Profile</span>
        </button>
      </div>
    {:else}
      <!-- Status Buttons Section -->
      <div class="section-block">
        <div class="section-title">Watch Status</div>
        <div class="status-btn-group">
          <button
            type="button"
            class="status-btn {currentStatus === 'to_watch' ? 'active to-watch' : ''}"
            on:click={() => handleSetStatus('to_watch')}
            disabled={isSaving}
          >
            <Clock size={14} />
            <span>To Watch</span>
          </button>
          <button
            type="button"
            class="status-btn {currentStatus === 'in_progress' ? 'active in-progress' : ''}"
            on:click={() => handleSetStatus('in_progress')}
            disabled={isSaving}
          >
            <PlayCircle size={14} />
            <span>In Progress</span>
          </button>
          <button
            type="button"
            class="status-btn {currentStatus === 'watched' ? 'active watched' : ''}"
            on:click={() => handleSetStatus('watched')}
            disabled={isSaving}
          >
            <CheckCircle2 size={14} />
            <span>Watched</span>
          </button>
        </div>
        {#if currentStatus}
          <button type="button" class="clear-status-btn" on:click={handleClearStatus} disabled={isSaving}>
            <X size={13} />
            <span>Remove status</span>
          </button>
        {/if}
      </div>

      <!-- Custom Lists Section -->
      <div class="section-block">
        <div class="section-title">Custom Lists</div>
        <div class="lists-select-box">
          {#if lists.length === 0}
            <div class="no-lists-note">No custom lists found.</div>
          {:else}
            {#each lists as list}
              <button
                type="button"
                class="list-item-btn {memberLists.has(list.id) ? 'selected' : ''}"
                on:click={() => handleToggleList(list)}
                disabled={isSaving}
              >
                <div class="checkbox-indicator">
                  {#if memberLists.has(list.id)}
                    <Check size={13} />
                  {/if}
                </div>
                <span class="list-label">{list.name}</span>
                <span class="item-count">({list.items.length})</span>
              </button>
            {/each}
          {/if}
        </div>

        {#if showCreateInput}
          <div class="new-list-row">
            <input
              type="text"
              bind:value={newListName}
              placeholder="List name..."
              class="new-list-input"
              on:keydown={(e) => e.key === 'Enter' && handleCreateAndAdd()}
            />
            <button
              type="button"
              class="btn-add-list"
              on:click={handleCreateAndAdd}
              disabled={!newListName.trim() || nameTaken || isSaving}
              aria-describedby={nameTaken ? 'list-name-hint' : undefined}
            >
              Add
            </button>
          </div>
          {#if nameTaken}
            <div class="error-text" id="list-name-hint">A list with that name already exists.</div>
          {/if}
        {:else}
          <button
            type="button"
            class="toggle-create-btn"
            on:click={() => (showCreateInput = true)}
          >
            <Plus size={14} />
            <span>Create new list</span>
          </button>
        {/if}
      </div>
    {/if}
  </div>
</div>

<style>
  .popover-backdrop {
    position: fixed;
    inset: 0;
    z-index: 250;
    background: rgba(0, 0, 0, 0.52);
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 16px;
  }

  .popover-panel {
    width: 100%;
    max-width: 380px;
    background: var(--modal-surface);
    border: 1px solid var(--modal-border);
    border-radius: var(--radius-lg);
    padding: 20px;
    box-shadow: var(--shadow-modal);
    animation: popover-enter 240ms cubic-bezier(0.16, 1, 0.3, 1);
    max-height: min(760px, calc(100dvh - 32px));
    overflow-y: auto;
  }

  .popover-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 16px;
    padding-bottom: 10px;
    border-bottom: 1px solid var(--border-subtle);
  }

  .header-title {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .header-title h3 {
    font-size: 0.95rem;
    font-weight: 700;
  }

  :global(.title-icon) {
    color: var(--accent-primary);
  }

  .close-btn {
    background: var(--control-fill);
    color: var(--text-muted);
    padding: 4px;
    border-radius: 50%;
  }

  .close-btn:hover {
    background: var(--control-fill-hover);
    color: var(--text-primary);
  }

  @keyframes popover-enter {
    from { opacity: 0; transform: scale(0.96) translateY(8px); }
    to { opacity: 1; transform: scale(1) translateY(0); }
  }

  .error-text {
    font-size: 0.8rem;
    color: var(--status-red);
    margin-bottom: 12px;
  }

  .clear-status-btn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    margin-top: 8px;
    padding: 5px 9px;
    background: transparent;
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    color: var(--text-muted);
    font-size: 0.78rem;
    font-weight: 600;
  }

  .clear-status-btn:hover:not(:disabled) {
    color: var(--text-primary);
    border-color: var(--border-muted);
  }

  .popover-loading {
    padding: 24px;
    text-align: center;
    color: var(--text-muted);
    font-size: 0.88rem;
  }

  .no-profile-cta {
    display: flex;
    flex-direction: column;
    align-items: center;
    text-align: center;
    padding: 20px 8px 12px;
  }

  .no-profile-icon-wrap {
    width: 44px;
    height: 44px;
    border-radius: var(--radius-full);
    background: var(--bg-surface-elevated);
    border: 1px solid var(--border-muted);
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--accent-primary);
    margin-bottom: 12px;
    box-shadow: 0 0 16px -4px var(--accent-glow);
  }

  .no-profile-cta h4 {
    font-size: 0.96rem;
    font-weight: 700;
    margin-bottom: 6px;
    color: var(--text-primary);
  }

  .no-profile-cta p {
    font-size: 0.8rem;
    color: var(--text-secondary);
    line-height: 1.4;
    margin-bottom: 16px;
  }

  .btn-create-profile {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    padding: 8px 16px;
    background: var(--action-fill);
    color: #fff;
    font-weight: 700;
    font-size: 0.82rem;
    border-radius: var(--radius-sm);
    transition: background-color var(--transition-fast), transform var(--transition-fast);
  }

  .btn-create-profile:hover {
    background: var(--action-fill-hover);
    transform: translateY(-1px);
  }

  .section-block {
    margin-bottom: 16px;
  }

  .section-title {
    font-size: 0.72rem;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--text-muted);
    margin-bottom: 8px;
    font-family: var(--font-sans);
  }

  .status-btn-group {
    display: flex;
    gap: 6px;
  }

  .status-btn {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 5px;
    padding: 7px 6px;
    background: var(--bg-surface);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    color: var(--text-secondary);
    font-size: 0.76rem;
    font-weight: 600;
    transition: background-color var(--transition-fast), border-color var(--transition-fast), color var(--transition-fast);
  }

  .status-btn:hover {
    border-color: var(--border-muted);
    color: var(--text-primary);
  }

  .status-btn.active.to-watch {
    background: var(--bg-surface-elevated);
    color: var(--text-primary);
    border-color: var(--border-muted);
  }

  .status-btn.active.in-progress {
    background: var(--accent-muted);
    color: var(--accent-primary);
    border-color: var(--accent-border);
  }

  .status-btn.active.watched {
    background: rgba(16, 185, 129, 0.12);
    color: var(--status-green);
    border-color: rgba(16, 185, 129, 0.4);
  }

  .lists-select-box {
    display: flex;
    flex-direction: column;
    gap: 4px;
    max-height: 180px;
    overflow-y: auto;
    margin-bottom: 10px;
  }

  .no-lists-note {
    font-size: 0.8rem;
    color: var(--text-muted);
    padding: 8px 4px;
  }

  .list-item-btn {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 7px 8px;
    background: var(--bg-surface);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    color: var(--text-secondary);
    font-size: 0.82rem;
    font-weight: 500;
    text-align: left;
    transition: background-color var(--transition-fast), border-color var(--transition-fast), color var(--transition-fast);
  }

  .list-item-btn:hover {
    border-color: var(--border-muted);
    color: var(--text-primary);
  }

  .list-item-btn.selected {
    background: var(--bg-surface-elevated);
    border-color: var(--accent-border);
    color: var(--text-primary);
  }

  .checkbox-indicator {
    width: 16px;
    height: 16px;
    border-radius: 3px;
    border: 1px solid var(--border-muted);
    background: var(--bg-primary);
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--accent-primary);
    flex-shrink: 0;
  }

  .list-item-btn.selected .checkbox-indicator {
    border-color: var(--accent-primary);
    background: var(--accent-muted);
  }

  .list-label {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .item-count {
    font-size: 0.72rem;
    color: var(--text-muted);
    font-family: var(--font-mono);
  }

  .toggle-create-btn {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 10px;
    background: transparent;
    color: var(--accent-primary);
    font-size: 0.8rem;
    font-weight: 600;
  }

  .toggle-create-btn:hover {
    text-decoration: underline;
  }

  .new-list-row {
    display: flex;
    gap: 6px;
  }

  .new-list-input {
    flex: 1;
    height: 34px;
    padding: 0 10px;
    background: var(--bg-surface);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    color: var(--text-primary);
    font-size: 0.82rem;
  }

  .new-list-input:focus {
    border-color: var(--accent-primary);
  }

  .btn-add-list {
    padding: 0 12px;
    background: var(--action-fill);
    color: #fff;
    font-size: 0.8rem;
    font-weight: 700;
    border-radius: var(--radius-sm);
  }

  .btn-add-list:disabled {
    opacity: 0.5;
  }
</style>
