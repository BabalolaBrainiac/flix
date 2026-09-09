<script lang="ts">
  import { onMount } from 'svelte';
  import { getDiagnostics, exportDebugReport } from '../lib/api';
  import type { DiagnosticsReport } from '../lib/types';
  import { ShieldCheck, HardDrive, Tv, AlertCircle, Download, ExternalLink, Loader2, Check } from 'lucide-svelte';

  let report: DiagnosticsReport | null = null;
  let isLoading = true;
  let errorMsg = '';
  let isExporting = false;
  let exportMessage = '';
  let exportError = '';

  async function downloadReport() {
    if (isExporting) return;
    isExporting = true;
    exportMessage = '';
    exportError = '';
    try {
      const report = await exportDebugReport();
      const url = URL.createObjectURL(new Blob([JSON.stringify(report, null, 2)], { type: 'application/json' }));
      const link = document.createElement('a');
      link.href = url;
      link.download = 'flix-debug-report.json';
      link.click();
      setTimeout(() => URL.revokeObjectURL(url), 1000);
      exportMessage = 'Report download started. You can attach it to your GitHub issue.';
    } catch (error) {
      exportError = error instanceof Error ? error.message : 'The debug report could not be exported.';
    } finally {
      isExporting = false;
    }
  }

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
    <h1 class="view-title">Help and diagnostics</h1>
    <p class="view-subtitle">
      Get help with playback or check your app.
    </p>
  </div>

  <div class="help-card panel">
    <div class="help-row">
      <div class="help-copy">
        <h2>Have a problem or a feature request?</h2>
        <p>Open an issue to share it with us.</p>
      </div>
      <a class="help-action" href="https://github.com/BabalolaBrainiac/flix/issues/new/choose" target="_blank" rel="noopener noreferrer">
        Open an issue <ExternalLink size={16} />
      </a>
    </div>
    <div class="help-row report-row">
      <div class="help-copy">
        <h3>Share details about a playback problem</h3>
        <p>Download a debug report to attach to your issue. Flix does not upload it.</p>
        <details class="report-contents">
          <summary>What does the report contain?</summary>
          <p>App version, session ID, playback stages, timings, player exit status, and error codes.</p>
          <p>It excludes titles, file paths, source links, and account credentials.</p>
        </details>
      </div>
      <button class="help-action" on:click={downloadReport} disabled={isExporting} aria-busy={isExporting}>
        {#if isExporting}<Loader2 size={16} class="export-spinner" />{:else}<Download size={16} />{/if}
        {isExporting ? 'Exporting…' : 'Export debug report'}
      </button>
    </div>
    {#if exportMessage}<p class="export-status" role="status"><Check size={16} />{exportMessage}</p>{/if}
    {#if exportError}<p class="export-error" role="alert"><AlertCircle size={16} />{exportError}</p>{/if}
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
          <span class="item-value">Local stream to your media player</span>
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
  .help-card { margin-bottom: 28px; padding: 0 24px; border-radius: 12px; }
  .help-row { display: flex; align-items: center; justify-content: space-between; gap: 24px; padding: 24px 0; }
  .report-row { border-top: 1px solid var(--border-subtle); }
  .help-copy { min-width: 0; }
  .help-copy h2 { font-size: 1.1rem; }
  .help-copy h3 { font-size: 0.95rem; }
  .help-copy p { margin-top: 6px; color: var(--text-secondary); font-size: 0.85rem; }
  .help-action {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
    gap: 8px;
    min-height: 42px;
    padding: 10px 16px;
    border: 1px solid #454c5a;
    border-radius: var(--radius-md);
    background: transparent;
    color: var(--text-primary);
    font-size: 0.85rem;
    font-weight: 500;
    text-decoration: none;
  }
  .help-action:hover:not(:disabled) { background: var(--bg-surface-hover); border-color: var(--text-secondary); }
  .help-action:focus-visible, summary:focus-visible { outline: 2px solid var(--text-primary); outline-offset: 4px; }
  .help-action:disabled { opacity: 0.6; cursor: wait; }
  .report-contents { margin-top: 12px; font-size: 0.8rem; color: var(--text-secondary); }
  .report-contents summary { cursor: pointer; width: fit-content; }
  .export-status, .export-error { display: flex; align-items: center; gap: 8px; padding-bottom: 20px; font-size: 0.85rem; }
  .export-status { color: var(--text-secondary); }
  .export-error { color: #fca5a5; }
  :global(.export-spinner) { animation: spin 1s linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
  @media (prefers-reduced-motion: reduce) { :global(.export-spinner) { animation: none; } }
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
    .diagnostics-view { padding: 24px 16px 80px; }
    .help-card { padding: 0 18px; }
    .help-row { align-items: flex-start; flex-direction: column; gap: 16px; }
    .help-action { width: 100%; }
    .item-row { align-items: flex-start; flex-direction: column; gap: 4px; }
    .item-value { text-align: left; }
    .item-value.path-text { max-width: 100%; white-space: normal; overflow-wrap: anywhere; }
    .diagnostics-grid {
      grid-template-columns: 1fr;
    }
  }
</style>
