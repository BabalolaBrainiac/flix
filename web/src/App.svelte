<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { getActivationStatus, getStatus, getProfiles, subscribeEvents } from './lib/api';
  import type { ActivationStatus, PlaybackSnapshot, ProfileSummary } from './lib/types';
  import { currentRoute, initRouter, navigate, goBack, DEFAULT_ROUTE } from './lib/router';
  import { refreshMembership } from './lib/library';
  import { recallTitle } from './lib/titles';
  import ActivationModal from './components/ActivationModal.svelte';
  import Navigation from './components/Navigation.svelte';
  import DiscoverView from './components/DiscoverView.svelte';
  import LibraryView from './components/LibraryView.svelte';
  import TitleView from './components/TitleView.svelte';
  import ProfilePicker from './components/ProfilePicker.svelte';
  import DownloadsView from './components/DownloadsView.svelte';
  import ReaderView from './components/ReaderView.svelte';
  import DiagnosticsView from './components/DiagnosticsView.svelte';
  import BrowserPlayer from './components/BrowserPlayer.svelte';
  import NowPlayingBar from './components/NowPlayingBar.svelte';

  let playbackState: PlaybackSnapshot = { state: 'idle' };
  let activationStatus: ActivationStatus | null = null;
  let activeProfile: ProfileSummary | null = null;
  let showProfilePicker = false;

  let unsubscribeEvents: (() => void) | null = null;
  let stopRouter: (() => void) | null = null;
  let pollInterval: any = null;
  let statusRevision = 0;

  $: route = $currentRoute;
  $: titleItem = route.name === 'title' && route.titleId ? recallTitle(route.titleId) : null;

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

  async function refreshProfiles() {
    try {
      const res = await getProfiles();
      if (res.active_profile_id) {
        activeProfile = res.profiles.find((p) => p.id === res.active_profile_id) || null;
      } else if (res.profiles.length === 1) {
        activeProfile = res.profiles[0];
      } else if (res.profiles.length > 1) {
        // Multiple profiles with none chosen yet: prompt picker
        showProfilePicker = true;
      }
    } catch {
      activeProfile = null;
    }
  }

  async function refreshActivation() {
    try {
      activationStatus = await getActivationStatus();
    } catch {
      activationStatus = null;
    }
  }

  // Every list, status, and badge on screen reads from this one store. It
  // reloads whenever the active profile changes, so switching a profile
  // never shows a stale profile's lists or statuses.
  $: void refreshMembership(activeProfile?.id ?? null);

  function handleTitleBack() {
    goBack({ name: 'discover' });
  }

  onMount(() => {
    refreshActivation();
    refreshProfiles();
    refreshStatus();
    stopRouter = initRouter();
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
    if (stopRouter) stopRouter();
  });
</script>

<div class="app-root">
  {#if activationStatus && !activationStatus.is_activated}
    <ActivationModal onActivated={refreshActivation} />
  {/if}

  {#if showProfilePicker}
    <ProfilePicker
      currentProfileId={activeProfile?.id || null}
      onProfileChanged={(p) => (activeProfile = p)}
      onClose={() => (showProfilePicker = false)}
    />
  {/if}

  <Navigation
    {route}
    {playbackState}
    {activeProfile}
    onOpenProfilePicker={() => (showProfilePicker = true)}
  />

  <main class="main-content">
    {#if playbackState.state === 'browser'}
      {#key playbackState.playback_id}
        <BrowserPlayer snapshot={playbackState} onStateChange={refreshStatus} />
      {/key}
    {/if}

    <!-- Every primary view stays mounted once visited, hidden rather than
         torn down, so its results, scroll position, and in-flight state
         survive a tab switch or a visit to a title page. -->
    <div class:hidden={route.name !== 'discover'}>
      <DiscoverView
        activeProfileId={activeProfile?.id || null}
        onOpenProfilePicker={() => (showProfilePicker = true)}
      />
    </div>
    <div class:hidden={route.name !== 'library'}>
      <LibraryView
        tab={route.name === 'library' ? route.library ?? 'lists' : 'lists'}
        activeProfileId={activeProfile?.id || null}
        onOpenProfilePicker={() => (showProfilePicker = true)}
        {playbackState}
        onNavigateToDiscover={() => navigate({ name: 'discover' })}
        onStateChanged={refreshStatus}
      />
    </div>
    <div class:hidden={route.name !== 'downloads'}>
      <DownloadsView />
    </div>
    <div class:hidden={route.name !== 'reader'}>
      <ReaderView />
    </div>
    <div class:hidden={route.name !== 'diagnostics'}>
      <DiagnosticsView />
    </div>

    {#if route.name === 'title'}
      {#if titleItem}
        {#key titleItem.id}
          <div class="title-overlay">
            <TitleView
              item={titleItem}
              activeProfileId={activeProfile?.id || null}
              onPlayStarted={handlePlayStarted}
              onOpenProfilePicker={() => (showProfilePicker = true)}
              onBack={handleTitleBack}
            />
          </div>
        {/key}
      {:else}
        <div class="title-overlay title-missing">
          <p>This title is no longer available. Go back and open it again.</p>
          <button type="button" on:click={() => navigate(DEFAULT_ROUTE, { replace: true })}>Back to Discover</button>
        </div>
      {/if}
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
    position: relative;
  }

  .hidden {
    display: none;
  }

  .title-overlay {
    background: var(--bg-primary);
  }

  .title-missing {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 12px;
    padding: 64px 20px;
    color: var(--text-secondary);
    text-align: center;
  }

  .title-missing button {
    padding: 8px 16px;
    background: var(--accent-primary);
    color: #0c0e14;
    font-weight: 700;
    border-radius: var(--radius-sm);
  }
</style>
