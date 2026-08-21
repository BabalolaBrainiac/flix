<script lang="ts">
  import { onMount } from 'svelte';
  import { getDiagnostics } from '../lib/api';
  import type { DiagnosticsReport } from '../lib/types';
  import { Activity, ShieldCheck, HardDrive, Tv, AlertCircle } from 'lucide-svelte';

  let report: DiagnosticsReport | null = null;
  let isLoading = true;
  let errorMsg = '';

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

  onMount(() => {
    loadReport();
  });
</script>

<div class="diagnostics-view">
  <div class="header-section">
    <h1 class="view-title">System Diagnostics</h1>
    <p class="view-subtitle">
      Inspect local runtime environment, player detection, and session health.
    </p>
  </div>

  {#if errorMsg}
    <div class="error-banner">
      <AlertCircle size={16} />
      <span>{errorMsg}</span>
    </div>
  {/if}

  {#if isLoading}
    <div class="loading-panel panel">
      <span>Generating diagnostic report...</span>
    </div>
  {:else if report}
    <div class="diagnostics-grid">
      <!-- Section 1: Application -->
      <div class="diag-card panel">
        <h3 class="card-title"><ShieldCheck size={18} /> Application Info</h3>
        <div class="item-row">
          <span class="item-label">Flix App Version:</span>
          <span class="item-value highlight">{report.app_version}</span>
        </div>
      </div>

      <!-- Section 2: Player & VLC Detection -->
      <div class="diag-card panel">
        <h3 class="card-title"><Tv size={18} /> Media Player Detection</h3>
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
      <div class="diag-card panel full-width">
        <h3 class="card-title"><HardDrive size={18} /> Local Storage & Session State</h3>
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
            {report.active_session ? 'Active' : 'Idle'}
          </span>
        </div>
      </div>
    </div>
  {/if}
</div>

<style>
  .diagnostics-view {
    max-width: 960px;
    margin: 0 auto;
    padding: 32px 24px 80px;
  }

  .header-section {
    margin-bottom: 24px;
  }

  .view-title {
    font-size: 1.8rem;
    font-weight: 700;
    margin-bottom: 4px;
  }

  .view-subtitle {
    color: var(--text-secondary);
    font-size: 0.9rem;
  }

  .error-banner {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 12px 16px;
    background: rgba(239, 68, 68, 0.15);
    border: 1px solid rgba(239, 68, 68, 0.3);
    color: #fca5a5;
    margin-bottom: 20px;
    border-radius: var(--radius-sm);
  }

  .loading-panel {
    padding: 32px;
    color: var(--text-muted);
    text-align: center;
  }

  .diagnostics-grid {
    display: grid;
    grid-template-columns: repeat(2, 1fr);
    gap: 16px;
  }

  .diag-card {
    padding: 20px 24px;
  }

  .diag-card.full-width {
    grid-column: 1 / -1;
  }

  .card-title {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 1rem;
    font-weight: 600;
    margin-bottom: 16px;
    padding-bottom: 10px;
    border-bottom: 1px solid var(--border-subtle);
  }

  .item-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 12px;
    gap: 12px;
    font-size: 0.85rem;
  }

  .item-label {
    color: var(--text-muted);
  }

  .item-value {
    color: var(--text-primary);
    font-weight: 500;
    text-align: right;
  }

  .item-value.highlight {
    color: #93c5fd;
    font-family: monospace;
  }

  .item-value.status-ok {
    color: var(--status-green);
  }

  .item-value.status-warn {
    color: var(--status-amber);
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

  @media (max-width: 768px) {
    .diagnostics-grid {
      grid-template-columns: 1fr;
    }
  }
</style>
