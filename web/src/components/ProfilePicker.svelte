<script lang="ts">
  import { onMount } from 'svelte';
  import {
    getProfiles,
    createProfile,
    updateProfile,
    deleteProfile,
    setActiveProfile,
  } from '../lib/api';
  import type { ProfileSummary } from '../lib/types';
  import AvatarPicker from './AvatarPicker.svelte';
  import { User, Plus, Check, Trash2, Edit2, X, AlertCircle } from 'lucide-svelte';
  import { parseAvatarKey, iconFor } from '../lib/avatars';
  import ConfirmationDialog from './ConfirmationDialog.svelte';
  import { confirmationFor } from '../lib/confirmation';

  export let currentProfileId: string | null = null;
  export let onProfileChanged: (profile: ProfileSummary) => void;
  export let onClose: () => void;

  let profiles: ProfileSummary[] = [];
  let isLoading = true;
  let errorMessage = '';
  let isCreating = false;
  let isEditingId: string | null = null;
  let pendingDeleteProfile: ProfileSummary | null = null;

  // New/Edit form state
  let formName = '';
  let formAvatarKey = 'amber';

  function getInitials(name: string): string {
    const parts = name.trim().split(/\s+/);
    if (parts.length >= 2) {
      return (parts[0][0] + parts[1][0]).toUpperCase();
    }
    return name.slice(0, 2).toUpperCase();
  }

  async function loadProfiles() {
    isLoading = true;
    errorMessage = '';
    try {
      const res = await getProfiles();
      profiles = res.profiles;
      if (res.active_profile_id) {
        currentProfileId = res.active_profile_id;
      }
    } catch (e: any) {
      errorMessage = e.message || 'Failed to load profiles';
    } finally {
      isLoading = false;
    }
  }

  async function handleSelectProfile(profile: ProfileSummary) {
    if (profile.id === currentProfileId) {
      onClose();
      return;
    }
    errorMessage = '';
    try {
      await setActiveProfile(profile.id);
      currentProfileId = profile.id;
      onProfileChanged(profile);
      onClose();
    } catch (e: any) {
      if (e.message?.includes('ACTIVE_PROFILE_ERROR') || e.message?.includes('409') || e.message?.includes('idle')) {
        errorMessage = 'Cannot switch profiles during active playback. Stop playback first.';
      } else {
        errorMessage = e.message || 'Failed to switch profile';
      }
    }
  }

  async function handleCreate() {
    if (!formName.trim()) return;
    errorMessage = '';
    try {
      const created = await createProfile(formName.trim(), formAvatarKey);
      profiles = [...profiles, created];
      isCreating = false;
      formName = '';
      // If this is the only profile, auto-select it
      if (profiles.length === 1) {
        await handleSelectProfile(created);
      }
    } catch (e: any) {
      errorMessage = e.message || 'Failed to create profile';
    }
  }

  function startEdit(profile: ProfileSummary) {
    isEditingId = profile.id;
    formName = profile.name;
    formAvatarKey = profile.avatar_key;
    isCreating = false;
  }

  async function handleSaveEdit(profileId: string) {
    if (!formName.trim()) return;
    errorMessage = '';
    try {
      await updateProfile(profileId, formName.trim(), formAvatarKey);
      let updated: ProfileSummary | undefined;
      profiles = profiles.map((p) => {
        if (p.id !== profileId) return p;
        updated = { ...p, name: formName.trim(), avatar_key: formAvatarKey };
        return updated;
      });
      // The name/avatar shown outside this dialog (the nav bar chip) comes
      // from the profile App.svelte already loaded, which this edit does
      // not otherwise reach - only the active profile's own edit needs to
      // push its new data out.
      if (updated && profileId === currentProfileId) {
        onProfileChanged(updated);
      }
      isEditingId = null;
      formName = '';
    } catch (e: any) {
      errorMessage = e.message || 'Failed to update profile';
    }
  }

  async function handleDelete(profileId: string) {
    errorMessage = '';
    try {
      await deleteProfile(profileId);
      profiles = profiles.filter((p) => p.id !== profileId);
      if (currentProfileId === profileId) {
        currentProfileId = profiles[0]?.id || null;
        if (profiles[0]) {
          onProfileChanged(profiles[0]);
        }
      }
    } catch (e: any) {
      errorMessage = e.message || 'Failed to delete profile';
    }
  }

  function confirmDeleteProfile() {
    if (!pendingDeleteProfile) return;
    const profileId = pendingDeleteProfile.id;
    pendingDeleteProfile = null;
    void handleDelete(profileId);
  }

  let modalBackdrop: HTMLDivElement;

  onMount(() => {
    loadProfiles();
    modalBackdrop?.focus();
  });
</script>

<div
  class="modal-backdrop"
  role="dialog"
  aria-modal="true"
  aria-labelledby="profiles-title"
  tabindex="-1"
  bind:this={modalBackdrop}
  on:click|self={onClose}
  on:keydown={(e) => e.key === 'Escape' && onClose()}
>
  <div class="profile-modal panel">
    <div class="modal-header">
      <div class="title-group">
        <User size={18} class="title-icon" />
        <h2 id="profiles-title">Manage Profiles</h2>
      </div>
      <button class="close-btn" type="button" aria-label="Close profiles" on:click={onClose}><X size={18} /></button>
    </div>

    {#if errorMessage}
      <div class="error-banner">
        <AlertCircle size={16} />
        <span>{errorMessage}</span>
      </div>
    {/if}

    {#if isLoading}
      <div class="loading-state">Loading profiles...</div>
    {:else}
      <div class="profiles-list">
        {#each profiles as profile}
          <div class="profile-row {profile.id === currentProfileId ? 'active-row' : ''}">
            {#if isEditingId === profile.id}
              <div class="edit-form">
                <input
                  type="text"
                  bind:value={formName}
                  placeholder="Profile Name"
                  class="edit-input"
                />
                <AvatarPicker
                  selectedKey={formAvatarKey}
                  onSelect={(k) => (formAvatarKey = k)}
                />
                <div class="edit-actions">
                  <button
                    type="button"
                    class="btn-save"
                    on:click={() => handleSaveEdit(profile.id)}
                  >
                    Save
                  </button>
                  <button
                    type="button"
                    class="btn-cancel"
                    on:click={() => (isEditingId = null)}
                  >
                    Cancel
                  </button>
                </div>
              </div>
            {:else}
              {@const parsed = parseAvatarKey(profile.avatar_key)}
              {@const AvatarIcon = iconFor(parsed.iconKey)}
              <button
                type="button"
                class="profile-select-btn"
                on:click={() => handleSelectProfile(profile)}
              >
                <div class="avatar-chip" style="background-color: {parsed.color}">
                  {#if AvatarIcon}
                    <svelte:component this={AvatarIcon} size={18} />
                  {:else}
                    {getInitials(profile.name)}
                  {/if}
                </div>
                <div class="profile-meta">
                  <div class="profile-name">{profile.name}</div>
                  {#if profile.id === currentProfileId}
                    <div class="profile-badge">Active</div>
                  {/if}
                </div>
              </button>

              <div class="row-controls">
                <button
                  type="button"
                  class="icon-action-btn"
                  title="Rename profile"
                  on:click={() => startEdit(profile)}
                >
                  <Edit2 size={15} />
                </button>
                {#if profiles.length > 1}
                  <button
                    type="button"
                    class="icon-action-btn delete-btn"
                    title="Delete profile"
                    on:click={() => (pendingDeleteProfile = profile)}
                  >
                    <Trash2 size={15} />
                  </button>
                {/if}
              </div>
            {/if}
          </div>
        {/each}
      </div>

      {#if isCreating}
        <div class="create-box">
          <h3>Create Profile</h3>
          <input
            type="text"
            bind:value={formName}
            placeholder="Profile Name"
            class="create-input"
          />
          <AvatarPicker
            selectedKey={formAvatarKey}
            onSelect={(k) => (formAvatarKey = k)}
          />
          <div class="create-actions">
            <button
              type="button"
              class="btn-save"
              on:click={handleCreate}
              disabled={!formName.trim()}
            >
              Create
            </button>
            <button
              type="button"
              class="btn-cancel"
              on:click={() => {
                isCreating = false;
                formName = '';
              }}
            >
              Cancel
            </button>
          </div>
        </div>
      {:else}
        <button
          type="button"
          class="add-profile-btn"
          on:click={() => {
            isCreating = true;
            formName = '';
            formAvatarKey = 'amber';
          }}
        >
          <Plus size={16} />
          <span>Add Profile</span>
        </button>
      {/if}
    {/if}
  </div>
</div>

{#if pendingDeleteProfile}
  {@const copy = confirmationFor('profile', pendingDeleteProfile.name)}
  <ConfirmationDialog {...copy} onCancel={() => (pendingDeleteProfile = null)} onConfirm={confirmDeleteProfile} />
{/if}

<style>
  .modal-backdrop {
    position: fixed;
    inset: 0;
    z-index: 200;
    background: rgba(0, 0, 0, 0.52);
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 20px;
  }

  .profile-modal {
    width: 100%;
    max-width: 500px;
    background: var(--modal-surface);
    border: 1px solid var(--modal-border);
    border-radius: var(--radius-lg);
    padding: 28px;
    box-shadow: var(--shadow-modal);
    animation: modal-enter 200ms cubic-bezier(0.16, 1, 0.3, 1);
    max-height: min(820px, calc(100dvh - 40px));
    overflow-y: auto;
  }

  @keyframes modal-enter {
    from {
      opacity: 0;
      transform: scale(0.96) translateY(8px);
    }
    to {
      opacity: 1;
      transform: scale(1) translateY(0);
    }
  }

  .modal-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 22px;
    padding-bottom: 14px;
    border-bottom: 1px solid var(--border-subtle);
  }

  .title-group {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .title-group h2 {
    font-size: 1.25rem;
    font-weight: 700;
  }

  :global(.title-icon) {
    color: var(--accent-primary);
  }

  .close-btn {
    background: var(--control-fill);
    color: var(--text-muted);
    padding: 6px;
    border-radius: 50%;
    transition: all var(--transition-fast);
  }

  .close-btn:hover {
    color: var(--text-primary);
    background: var(--control-fill-hover);
  }

  .error-banner {
    display: flex;
    align-items: center;
    gap: 8px;
    background: rgba(244, 63, 94, 0.12);
    border: 1px solid rgba(244, 63, 94, 0.35);
    color: #fda4af;
    padding: 10px 14px;
    border-radius: var(--radius-sm);
    font-size: 0.84rem;
    margin-bottom: 16px;
  }

  .loading-state {
    padding: 24px;
    text-align: center;
    color: var(--text-muted);
  }

  .profiles-list {
    display: flex;
    flex-direction: column;
    gap: 10px;
    margin-bottom: 20px;
  }

  .profile-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    background: var(--bg-surface);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    padding: 10px 14px;
    transition: background-color var(--transition-fast), border-color var(--transition-fast);
  }

  .profile-row:hover {
    border-color: var(--border-muted);
  }

  .profile-row.active-row {
    border-color: var(--accent-border);
    background: var(--bg-surface-elevated);
  }

  .profile-select-btn {
    display: flex;
    align-items: center;
    gap: 12px;
    background: transparent;
    flex: 1;
    text-align: left;
    cursor: pointer;
  }

  .avatar-chip {
    width: 34px;
    height: 34px;
    border-radius: var(--radius-sm);
    display: flex;
    align-items: center;
    justify-content: center;
    color: #0c0e14;
    font-weight: 800;
    font-size: 0.82rem;
    font-family: var(--font-mono);
  }

  .profile-meta {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .profile-name {
    font-size: 0.92rem;
    font-weight: 600;
    color: var(--text-primary);
  }

  .profile-badge {
    font-size: 0.65rem;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--accent-primary);
    background: var(--accent-muted);
    padding: 2px 6px;
    border-radius: var(--radius-sm);
    font-family: var(--font-sans);
  }

  .row-controls {
    display: flex;
    gap: 6px;
  }

  .icon-action-btn {
    padding: 6px;
    background: transparent;
    color: var(--text-muted);
    border-radius: var(--radius-sm);
  }

  .icon-action-btn:hover {
    color: var(--text-primary);
    background: var(--bg-surface-hover);
  }

  .icon-action-btn.delete-btn:hover {
    color: var(--status-red);
  }

  .add-profile-btn {
    width: 100%;
    padding: 10px;
    border: 1px dashed var(--border-muted);
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--text-secondary);
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    font-size: 0.86rem;
    font-weight: 600;
    cursor: pointer;
    transition: all var(--transition-fast);
  }

  .add-profile-btn:hover {
    border-color: var(--accent-primary);
    color: var(--accent-primary);
    background: var(--accent-muted);
  }

  .create-box, .edit-form {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 14px;
    background: var(--bg-surface-elevated);
    border-radius: var(--radius-sm);
    width: 100%;
  }

  .create-box h3 {
    font-size: 0.92rem;
    font-weight: 600;
  }

  .create-input, .edit-input {
    height: 40px;
    padding: 0 12px;
    background: var(--bg-surface);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    color: var(--text-primary);
    font-size: 0.88rem;
  }

  .create-input:focus, .edit-input:focus {
    border-color: var(--accent-primary);
  }

  .create-actions, .edit-actions {
    display: flex;
    gap: 8px;
    justify-content: flex-end;
  }

  .btn-save {
    padding: 6px 14px;
    background: var(--action-fill);
    color: #fff;
    font-weight: 700;
    font-size: 0.82rem;
    border-radius: var(--radius-sm);
  }

  .btn-save:hover:not(:disabled) {
    background: var(--action-fill-hover);
  }

  .btn-save:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .btn-cancel {
    padding: 6px 14px;
    background: var(--bg-surface);
    color: var(--text-secondary);
    font-size: 0.82rem;
    font-weight: 600;
    border-radius: var(--radius-sm);
  }

  .btn-cancel:hover {
    color: var(--text-primary);
  }
</style>
