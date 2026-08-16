<script lang="ts">
  import { redeemInvite } from '../lib/api';

  export let onActivated: () => void;
  export let onUseOfflineMode: () => void;

  let inviteCode = '';
  let customGateway = '';
  let showAdvanced = false;
  let isSubmitting = false;
  let errorMessage = '';

  async function handleRedeem() {
    if (!inviteCode.trim()) return;
    isSubmitting = true;
    errorMessage = '';

    try {
      await redeemInvite(
        inviteCode.trim(),
        customGateway.trim() ? customGateway.trim() : undefined
      );
      onActivated();
    } catch (e: any) {
      errorMessage = e.message || 'Activation failed. Please check your invite code.';
    } finally {
      isSubmitting = false;
    }
  }
</script>

<div class="modal-backdrop" role="dialog" aria-modal="true" tabindex="-1">
  <div class="activation-card glass-panel" role="document">
    <div class="card-badge">✨ WELCOME TO FLIX</div>
    <h1 class="card-title">Activate Private Access</h1>
    <p class="card-desc">
      Enter your private invite code to access catalog search, 4K torrent streams, and subtitle downloads.
    </p>

    {#if errorMessage}
      <div class="error-box">{errorMessage}</div>
    {/if}

    <form on:submit|preventDefault={handleRedeem} class="activation-form">
      <div class="input-group">
        <label for="invite-input" class="input-label">Invite Code</label>
        <input
          id="invite-input"
          type="text"
          bind:value={inviteCode}
          placeholder="e.g. flix_a1b2c3d4..."
          class="text-input"
          disabled={isSubmitting}
          required
        />
      </div>

      {#if showAdvanced}
        <div class="input-group">
          <label for="gateway-input" class="input-label">Custom Gateway URL (Optional)</label>
          <input
            id="gateway-input"
            type="text"
            bind:value={customGateway}
            placeholder="http://127.0.0.1:8787"
            class="text-input"
            disabled={isSubmitting}
          />
        </div>
      {/if}

      <div class="form-actions">
        <button
          type="submit"
          class="submit-btn primary"
          disabled={isSubmitting || !inviteCode.trim()}
        >
          {#if isSubmitting}
            <span class="spinner"></span>
            <span>Activating Device...</span>
          {:else}
            <span>Activate Device</span>
          {/if}
        </button>

        <button
          type="button"
          class="toggle-advanced-btn"
          on:click={() => (showAdvanced = !showAdvanced)}
        >
          {showAdvanced ? 'Hide Custom Gateway' : 'Custom Gateway URL'}
        </button>
      </div>
    </form>

    <div class="divider">
      <span>OR</span>
    </div>

    <div class="offline-section">
      <button class="offline-btn" on:click={onUseOfflineMode}>
        🧲 Use Offline Magnet Mode Only
      </button>
      <p class="offline-note">
        You can paste magnet links and stream directly to local players without a private gateway.
      </p>
    </div>
  </div>
</div>

<style>
  .modal-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.85);
    backdrop-filter: blur(12px);
    z-index: 200;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 24px;
  }

  .activation-card {
    max-width: 480px;
    width: 100%;
    padding: 36px;
    border-radius: var(--radius-lg);
    background: linear-gradient(135deg, rgba(23, 31, 48, 0.95), rgba(15, 20, 34, 0.98));
    border: 1px solid rgba(99, 102, 241, 0.4);
    box-shadow: 0 24px 60px rgba(0, 0, 0, 0.6), 0 0 40px rgba(99, 102, 241, 0.2);
    text-align: center;
  }

  .card-badge {
    display: inline-block;
    padding: 4px 12px;
    border-radius: var(--radius-full);
    background: rgba(99, 102, 241, 0.2);
    color: #a5b4fc;
    font-size: 0.75rem;
    font-weight: 700;
    letter-spacing: 0.08em;
    margin-bottom: 16px;
    border: 1px solid rgba(99, 102, 241, 0.3);
  }

  .card-title {
    font-size: 1.75rem;
    font-weight: 800;
    margin-bottom: 10px;
    color: var(--text-primary);
  }

  .card-desc {
    font-size: 0.95rem;
    color: var(--text-secondary);
    margin-bottom: 28px;
    line-height: 1.5;
  }

  .error-box {
    padding: 12px 16px;
    border-radius: var(--radius-sm);
    background: rgba(239, 68, 68, 0.15);
    border: 1px solid rgba(239, 68, 68, 0.3);
    color: #fca5a5;
    font-size: 0.85rem;
    margin-bottom: 20px;
    text-align: left;
  }

  .activation-form {
    display: flex;
    flex-direction: column;
    gap: 16px;
    margin-bottom: 24px;
    text-align: left;
  }

  .input-group {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .input-label {
    font-size: 0.8rem;
    font-weight: 600;
    color: var(--text-secondary);
  }

  .text-input {
    width: 100%;
    padding: 12px 16px;
    border-radius: var(--radius-sm);
    background: rgba(8, 11, 17, 0.8);
    border: 1px solid var(--border-subtle);
    color: var(--text-primary);
    font-size: 0.95rem;
    box-sizing: border-box;
  }

  .text-input:focus {
    border-color: var(--accent-primary);
    box-shadow: 0 0 12px rgba(99, 102, 241, 0.3);
  }

  .form-actions {
    display: flex;
    flex-direction: column;
    gap: 10px;
    margin-top: 8px;
  }

  .submit-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 10px;
    padding: 12px 24px;
    border-radius: var(--radius-md);
    font-size: 1rem;
    font-weight: 600;
    background: var(--accent-primary);
    color: #fff;
    box-shadow: var(--accent-glow);
  }

  .submit-btn:hover:not(:disabled) {
    background: var(--accent-primary-hover);
  }

  .submit-btn:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }

  .toggle-advanced-btn {
    background: transparent;
    color: var(--text-muted);
    font-size: 0.8rem;
    padding: 4px;
  }

  .toggle-advanced-btn:hover {
    color: var(--text-secondary);
  }

  .divider {
    position: relative;
    text-align: center;
    margin: 20px 0;
  }

  .divider::before {
    content: '';
    position: absolute;
    top: 50%;
    left: 0;
    right: 0;
    height: 1px;
    background: var(--border-subtle);
  }

  .divider span {
    position: relative;
    padding: 0 12px;
    background: rgba(18, 25, 40, 0.95);
    font-size: 0.75rem;
    color: var(--text-muted);
    font-weight: 600;
  }

  .offline-section {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
  }

  .offline-btn {
    padding: 10px 18px;
    border-radius: var(--radius-md);
    background: rgba(255, 255, 255, 0.05);
    border: 1px solid var(--border-subtle);
    color: var(--text-secondary);
    font-size: 0.85rem;
    font-weight: 600;
  }

  .offline-btn:hover {
    background: rgba(255, 255, 255, 0.1);
    color: var(--text-primary);
  }

  .offline-note {
    font-size: 0.75rem;
    color: var(--text-muted);
    line-height: 1.4;
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

  @keyframes spin {
    to { transform: rotate(360deg); }
  }
</style>
