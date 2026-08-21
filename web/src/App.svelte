<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { getActivationStatus, getStatus, subscribeEvents } from './lib/api';
  import type { ActivationStatus, PlaybackSnapshot } from './lib/types';
  import ActivationModal from './components/ActivationModal.svelte';
  import Navigation from './components/Navigation.svelte';
  import SearchView from './components/SearchView.svelte';
  import ContinueWatchingView from './components/ContinueWatchingView.svelte';
  import DownloadsView from './components/DownloadsView.svelte';
  import SettingsView from './components/SettingsView.svelte';
  import DiagnosticsView from './components/DiagnosticsView.svelte';
  import NowPlayingBar from './components/NowPlayingBar.svelte';

  let activeTab: 'search' | 'continue' | 'downloads' | 'settings' | 'diagnostics' = 'search';
  let playbackState: PlaybackSnapshot = { state: 'idle' };
  let activationStatus: ActivationStatus | null = null;
  let unsubscribeEvents: (() => void) | null = null;
  let pollInterval: any = null;

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
  }

  async function refreshActivation() {
    try {
      activationStatus = await getActivationStatus();
    } catch {
      activationStatus = null;
    }
  }

  onMount(() => {
    refreshActivation();
    refreshStatus();
    unsubscribeEvents = subscribeEvents((snapshot) => {
      playbackState = snapshot;
    });
    // Fallback poll every 5 seconds for status synchronization
    pollInterval = setInterval(refreshStatus, 5000);
  });

  onDestroy(() => {
    if (unsubscribeEvents) unsubscribeEvents();
    if (pollInterval) clearInterval(pollInterval);
  });
</script>

<div class="app-root">
  {#if activationStatus && !activationStatus.is_activated}
    <ActivationModal onActivated={refreshActivation} />
  {/if}

  <Navigation
    {activeTab}
    {playbackState}
    onSelectTab={(tab) => (activeTab = tab)}
  />

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
      <DiagnosticsView />
    {/if}
  </main>

  <NowPlayingBar snapshot={playbackState} onStateChange={refreshStatus} />
</div>

<style>
  .app-root {
    min-height: 100vh;
    display: flex;
    flex-direction: column;
    padding-bottom: 80px;
  }

  .main-content {
    flex: 1;
  }
</style>
