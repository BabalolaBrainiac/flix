<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { getActivationStatus, getStatus, subscribeEvents } from './lib/api';
  import type { ActivationStatus, PlaybackSnapshot } from './lib/types';
  import ActivationModal from './components/ActivationModal.svelte';
  import Navigation from './components/Navigation.svelte';
  import SearchView from './components/SearchView.svelte';
  import RecommendView from './components/RecommendView.svelte';
  import ContinueWatchingView from './components/ContinueWatchingView.svelte';
  import DownloadsView from './components/DownloadsView.svelte';
  import ReaderView from './components/ReaderView.svelte';
  import SettingsView from './components/SettingsView.svelte';
  import DiagnosticsView from './components/DiagnosticsView.svelte';
  import NowPlayingBar from './components/NowPlayingBar.svelte';

  let activeTab: 'search' | 'discover' | 'continue' | 'downloads' | 'reader' | 'settings' | 'diagnostics' = 'search';
  let playbackState: PlaybackSnapshot = { state: 'idle' };
  let activationStatus: ActivationStatus | null = null;
  let unsubscribeEvents: (() => void) | null = null;
  let pollInterval: any = null;
  let statusRevision = 0;

  async function refreshStatus() {
    const revision = ++statusRevision;
    try {
      const res = await getStatus();
      if (revision === statusRevision) playbackState = res.state;
    } catch {
      // Ignore background poll errors
    }
  }

  function refreshWhenVisible() {
    if (!document.hidden) void refreshStatus();
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
      statusRevision += 1;
      playbackState = snapshot;
    });
    // Fallback poll every 5 seconds for status synchronization
    pollInterval = setInterval(refreshWhenVisible, 5000);
    document.addEventListener('visibilitychange', refreshWhenVisible);
  });

  onDestroy(() => {
    statusRevision += 1;
    document.removeEventListener('visibilitychange', refreshWhenVisible);
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
    {:else if activeTab === 'discover'}
      <RecommendView />
    {:else if activeTab === 'continue'}
      <ContinueWatchingView
        {playbackState}
        onNavigateToSearch={() => (activeTab = 'search')}
        onStateChanged={refreshStatus}
      />
    {:else if activeTab === 'downloads'}
      <DownloadsView />
    {:else if activeTab === 'reader'}
      <ReaderView />
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
