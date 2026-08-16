<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { getStatus, getActivationStatus } from './lib/api';
  import type { PlaybackState, ActivationStatus } from './lib/types';
  import Navigation from './components/Navigation.svelte';
  import SearchView from './components/SearchView.svelte';
  import ContinueWatchingView from './components/ContinueWatchingView.svelte';
  import DownloadsView from './components/DownloadsView.svelte';
  import SettingsView from './components/SettingsView.svelte';
  import DiagnosticsView from './components/DiagnosticsView.svelte';
  import ActivationModal from './components/ActivationModal.svelte';
  import VlcGuidanceModal from './components/VlcGuidanceModal.svelte';

  let activeTab: 'search' | 'continue' | 'downloads' | 'settings' | 'diagnostics' = 'search';
  let playbackState: PlaybackState = { status: 'idle' };
  let activationStatus: ActivationStatus | null = null;
  let offlineMode = false;
  let showVlcGuidance = false;
  let pollInterval: any = null;

  async function checkActivation() {
    try {
      activationStatus = await getActivationStatus();
      if (!activationStatus.vlc_installed && !activationStatus.is_activated) {
        showVlcGuidance = true;
      }
    } catch {
      // Ignore initial activation check failure
    }
  }

  async function refreshStatus() {
    try {
      const res = await getStatus();
      playbackState = res.state;
    } catch {
      // Ignore background poll errors
    }
  }

  function handlePlayStarted() {
    refreshStatus();
    activeTab = 'continue';
  }

  function handleActivationComplete() {
    checkActivation();
  }

  function handleUseOfflineMode() {
    offlineMode = true;
  }

  onMount(() => {
    checkActivation();
    refreshStatus();
    pollInterval = setInterval(refreshStatus, 2000);
  });

  onDestroy(() => {
    if (pollInterval) clearInterval(pollInterval);
  });
</script>

<div class="app-root">
  <Navigation
    {activeTab}
    {playbackState}
    onSelectTab={(tab) => (activeTab = tab)}
  />

  <!-- First-Run Activation Gate (skipped if offline mode chosen) -->
  {#if activationStatus && !activationStatus.is_activated && !offlineMode}
    <ActivationModal
      onActivated={handleActivationComplete}
      onUseOfflineMode={handleUseOfflineMode}
    />
  {/if}

  <!-- Missing VLC guidance modal if triggered -->
  {#if showVlcGuidance}
    <VlcGuidanceModal onClose={() => (showVlcGuidance = false)} />
  {/if}

  <main class="main-content">
    {#if activeTab === 'search'}
      <SearchView onPlayStarted={handlePlayStarted} />
    {:else if activeTab === 'continue'}
      <ContinueWatchingView
        {playbackState}
        onNavigateToSearch={() => (activeTab = 'search')}
        onStateChanged={refreshStatus}
      />
    {:else if activeTab === 'downloads'}
      <DownloadsView />
    {:else if activeTab === 'settings'}
      <SettingsView />
    {:else if activeTab === 'diagnostics'}
      <DiagnosticsView onTriggerActivation={() => { offlineMode = false; checkActivation(); }} />
    {/if}
  </main>
</div>

<style>
  .app-root {
    min-height: 100vh;
    display: flex;
    flex-direction: column;
  }

  .main-content {
    flex: 1;
  }
</style>
