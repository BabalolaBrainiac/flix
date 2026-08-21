<script lang="ts">
  import { redeemInvite } from '../lib/api';

  export let onActivated: () => void;

  let inviteCode = '';
  let gatewayUrl = '';
  let showGatewayInput = false;
  let isSubmitting = false;
  let errorMessage = '';

  async function handleSubmit() {
    if (!inviteCode.trim()) {
      return;
    }
    isSubmitting = true;
    errorMessage = '';

    try {
      await redeemInvite(inviteCode.trim(), gatewayUrl.trim() || undefined);
      onActivated();
    } catch (error: any) {
      errorMessage = error.message || 'Device activation failed.';
    } finally {
      isSubmitting = false;
    }
  }
</script>

<div class="modal-backdrop" role="dialog" aria-modal="true" aria-labelledby="activation-title">
  <div class="activation-card panel">
    <div class="eyebrow">Private Access</div>
    <h1 id="activation-title">Enter your invite code</h1>
    <p class="description">
      Flix needs a one-time invite code before it can use the subtitle gateway.
    </p>

    {#if errorMessage}
      <div class="error-banner">{errorMessage}</div>
    {/if}

    <form class="activation-form" on:submit|preventDefault={handleSubmit}>
      <label class="field">
        <span>Invite code</span>
        <input
          bind:value={inviteCode}
          type="text"
          placeholder="Enter invite code"
          disabled={isSubmitting}
          autocomplete="one-time-code"
          required
        />
      </label>

      {#if showGatewayInput}
        <label class="field">
          <span>Gateway URL</span>
          <input
            bind:value={gatewayUrl}
            type="url"
            placeholder="http://127.0.0.1:8787"
            disabled={isSubmitting}
          />
        </label>
      {/if}

      <div class="actions">
        <button class="primary" type="submit" disabled={isSubmitting || !inviteCode.trim()}>
          {isSubmitting ? 'Activating...' : 'Activate device'}
        </button>
        <button class="secondary" type="button" on:click={() => (showGatewayInput = !showGatewayInput)}>
          {showGatewayInput ? 'Hide gateway URL' : 'Set gateway URL'}
        </button>
      </div>
    </form>
  </div>
</div>

<style>
  .modal-backdrop {
    position: fixed;
    inset: 0;
    z-index: 300;
    display: grid;
    place-items: center;
    padding: 24px;
    background: rgba(4, 6, 10, 0.84);
  }

  .activation-card {
    width: min(460px, 100%);
    padding: 28px;
    border-radius: var(--radius-md);
  }

  .eyebrow {
    margin-bottom: 10px;
    color: var(--status-amber);
    font-size: 0.75rem;
    font-weight: 700;
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }

  h1 {
    margin: 0 0 10px;
    font-size: 1.8rem;
  }

  .description {
    margin: 0 0 20px;
    color: var(--text-secondary);
  }

  .error-banner {
    margin-bottom: 16px;
    padding: 12px 14px;
    border: 1px solid rgba(239, 68, 68, 0.28);
    border-radius: var(--radius-sm);
    color: #fca5a5;
    background: rgba(239, 68, 68, 0.12);
  }

  .activation-form {
    display: grid;
    gap: 14px;
  }

  .field {
    display: grid;
    gap: 6px;
  }

  .field span {
    color: var(--text-secondary);
    font-size: 0.85rem;
  }

  .field input {
    height: 44px;
    padding: 0 14px;
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    background: var(--bg-secondary);
    color: var(--text-primary);
  }

  .actions {
    display: flex;
    gap: 10px;
    margin-top: 8px;
  }

  .primary,
  .secondary {
    height: 42px;
    padding: 0 16px;
    border-radius: var(--radius-sm);
  }

  .primary {
    background: var(--accent-primary);
    color: #fff;
  }

  .secondary {
    border: 1px solid var(--border-subtle);
    background: var(--bg-surface-active);
    color: var(--text-secondary);
  }
</style>
