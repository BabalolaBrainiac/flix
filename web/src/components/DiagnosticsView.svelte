<script lang="ts">
  import { onMount } from 'svelte';
  import { getDiagnostics, resetActivation } from '../lib/api';
  import type { DiagnosticsReport } from '../lib/types';

  export let onTriggerActivation: () => void;

  let report: DiagnosticsReport | null = null;
  let isLoading = true;
  let errorMsg = '';
  let actionMessage = '';

  async function loadReport() {
    isLoading = true;
    errorMsg = '';
    try {
      report = await getDiagnostics();
    } catch (e: any) {
      errorMsg = e.message || 'Failed to load diagnostics';
    } finally {
      isLoading = false;
    }
  }

  async function handleResetActivation() {
    if (!confirm('Are you sure you want to remove your device token from the OS credential store? You will need an invite code to re-activate.')) {
      return;
    }
    try {
      await resetActivation();
      actionMessage = 'Device token removed from OS credential store.';
      await loadReport();
      onTriggerActivation();
    } catch (e: any) {
      actionMessage = `Failed to reset token: ${e.message}`;
    }
  }

  onMount(() => {
    loadReport();
  });
</script>

<div class="diagnostics-view">
  <div class="header-section">
    <h1 class="view-title">System Diagnostics</h1>
    <p class="view-subtitle">
      Inspect local runtime environment, OS credential storage, VLC integration, and session health without exposing secrets.
    </p>
  </div>

  {#if actionMessage}
    <div class="alert-box glass-panel">{actionMessage}</div>
  {/if}

  {#if errorMsg}
    <div class="error-banner glass-panel">{errorMsg}</div>
  {/if}

  {#if isLoading}
    <div class="loading-panel glass-panel">
      <span class="spinner"></span>
      <span>Generating diagnostic report...</span>
    </div>
  {:else if report}
    <div class="diagnostics-grid">
      <!-- Section 1: Application & Activation -->
      <div class="diag-card glass-panel">
        <h3 class="card-title">🔐 Activation & Credential Store</h3>
        <div class="item-row">
          <span class="item-label">Flix App Version:</span>
          <span class="item-value highlight">{report.app_version}</span>
        </div>
        <div class="item-row">
          <span class="item-label">Device Activation:</span>
          <span class="item-value {report.is_activated ? 'status-ok' : 'status-warn'}">
            {report.is_activated ? '✓ Activated (Stored in OS Keyring)' : '✕ Not Activated'}
          </span>
        </div>
        <div class="item-row">
          <span class="item-label">Gateway Status:</span>
          <span class="item-value {report.gateway_reachable ? 'status-ok' : 'status-muted'}">
            {report.gateway_reachable ? '✓ Gateway Configured' : 'Offline / Standalone Mode'}
          </span>
        </div>
        <div class="card-actions">
          <button class="action-btn danger" on:click={handleResetActivation}>
            🗑 Remove Device Token from Keyring
          </button>
        </div>
      </div>

      <!-- Section 2: Player & VLC Detection -->
      <div class="diag-card glass-panel">
        <h3 class="card-title">🎮 Media Player Detection</h3>
        <div class="item-row">
          <span class="item-label">VLC Player Status:</span>
          <span class="item-value {report.vlc_status === 'Installed' ? 'status-ok' : 'status-warn'}">
            {report.vlc_status}
          </span>
        </div>
        <div class="item-row">
          <span class="item-label">Detected Player Binary:</span>
          <span class="item-value path-text" title={report.player_path || 'None'}>
            {report.player_path || 'None'}
          </span>
        </div>
        <div class="item-row">
          <span class="item-label">Streaming Protocol:</span>
          <span class="item-value">Direct localhost HTTP to player (Zero browser overhead)</span>
        </div>
      </div>

      <!-- Section 3: Storage & Session -->
      <div class="diag-card glass-panel full-width">
        <h3 class="card-title">💾 Local Storage & Session State</h3>
        <div class="item-row">
          <span class="item-label">Downloads Directory:</span>
          <span class="item-value path-text">{report.download_dir}</span>
        </div>
        <div class="item-row">
          <span class="item-label">Data & Library Directory:</span>
          <span class="item-value path-text">{report.data_dir}</span>
        </div>
        <div class="item-row">
          <span class="item-label">Active Torrent Stream Session:</span>
          <span class="item-value {report.active_session ? 'status-ok' : 'status-muted'}">
            {report.active_session ? '▶ Session Running' : 'Idle'}
          </span>
        </div>
      </div>
    </div>
  {/if}
</div>

<style>
  .diagnostics-view {
    max-width: 1000px;
    margin: 0 auto;
    padding: 32px 24px 80px;
  }

  .header-section {
    margin-bottom: 32px;
  }

  .view-title {
    font-size: 2.25rem;
    font-weight: 800;
    margin-bottom: 8px;
  }

  .view-subtitle {
    color: var(--text-secondary);
    font-size: 1rem;
    max-width: 680px;
  }

  .alert-box {
    padding: 12px 18px;
    background: rgba(99, 102, 241, 0.15);
    border: 1px solid rgba(99, 102, 241, 0.3);
    color: #c7d2fe;
    border-radius: var(--radius-sm);
    margin-bottom: 24px;
  }

  .error-banner {
    padding: 14px 20px;
    background: rgba(239, 68, 68, 0.15);
    border: 1px solid rgba(239, 68, 68, 0.3);
    color: #fca5a5;
    margin-bottom: 24px;
    border-radius: var(--radius-sm);
  }

  .loading-panel {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 32px;
    color: var(--text-muted);
  }

  .diagnostics-grid {
    display: grid;
    grid-template-columns: repeat(2, 1fr);
    gap: 20px;
  }

  .diag-card {
    padding: 24px;
    border-radius: var(--radius-md);
    background: var(--bg-surface);
    border: 1px solid var(--border-subtle);
  }

  .diag-card.full-width {
    grid-column: 1 / -1;
  }

  .card-title {
    font-size: 1.15rem;
    font-weight: 700;
    margin-bottom: 20px;
    padding-bottom: 12px;
    border-bottom: 1px solid var(--border-subtle);
  }

  .item-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 14px;
    gap: 12px;
    font-size: 0.9rem;
  }

  .item-label {
    color: var(--text-muted);
    font-weight: 500;
  }

  .item-value {
    color: var(--text-primary);
    font-weight: 600;
    text-align: right;
  }

  .item-value.highlight {
    color: #a5b4fc;
    font-family: monospace;
  }

  .item-value.status-ok {
    color: #34d399;
  }

  .item-value.status-warn {
    color: #fca5a5;
  }

  .item-value.status-muted {
    color: var(--text-muted);
  }

  .item-value.path-text {
    font-family: monospace;
    font-size: 0.8rem;
    max-width: 380px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .card-actions {
    margin-top: 24px;
    padding-top: 16px;
    border-top: 1px solid var(--border-subtle);
  }

  .action-btn {
    padding: 8px 16px;
    border-radius: var(--radius-sm);
    font-size: 0.85rem;
    font-weight: 600;
  }

  .action-btn.danger {
    background: rgba(239, 68, 68, 0.15);
    border: 1px solid rgba(239, 68, 68, 0.3);
    color: #fca5a5;
  }

  .action-btn.danger:hover {
    background: rgba(239, 68, 68, 0.25);
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

  @media (max-width: 768px) {
    .diagnostics-grid {
      grid-template-columns: 1fr;
    }
  }
</style>
