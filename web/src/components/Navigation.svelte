<script lang="ts">
  import type { PlaybackState } from '../lib/types';

  export let activeTab: 'search' | 'continue' | 'downloads' | 'settings' | 'diagnostics' = 'search';
  export let playbackState: PlaybackState = { status: 'idle' };
  export let onSelectTab: (tab: 'search' | 'continue' | 'downloads' | 'settings' | 'diagnostics') => void;

  $: isPlaying = playbackState.status === 'playing';
</script>

<header class="nav-header">
  <div class="nav-container">
    <button type="button" class="brand" on:click={() => onSelectTab('search')}>
      <div class="logo-mark">

        <svg width="20" height="20" viewBox="0 0 24 24" fill="none">
          <path d="M5 3L19 12L5 21V3Z" fill="url(#brand-grad)" />
          <defs>
            <linearGradient id="brand-grad" x1="5" y1="3" x2="19" y2="21" gradientUnits="userSpaceOnUse">
              <stop stop-color="#6366f1" />
              <stop offset="0.5" stop-color="#a855f7" />
              <stop offset="1" stop-color="#ec4899" />
            </linearGradient>
          </defs>
        </svg>
      </div>
      <div class="brand-text">
        <span class="brand-name">FLIX</span>
        <span class="brand-sub">DESKTOP</span>
      </div>
    </button>


    <nav class="nav-tabs">
      <button
        class="nav-tab {activeTab === 'search' ? 'active' : ''}"
        on:click={() => onSelectTab('search')}
      >
        <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <circle cx="11" cy="11" r="8"></circle>
          <line x1="21" y1="21" x2="16.65" y2="16.65"></line>
        </svg>
        <span>Search</span>
      </button>

      <button
        class="nav-tab {activeTab === 'continue' ? 'active' : ''}"
        on:click={() => onSelectTab('continue')}
      >
        {#if isPlaying}
          <span class="live-dot pulsing-indicator"></span>
        {:else}
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <polygon points="5 3 19 12 5 21 5 3"></polygon>
          </svg>
        {/if}
        <span>Continue Watching</span>
      </button>

      <button
        class="nav-tab {activeTab === 'downloads' ? 'active' : ''}"
        on:click={() => onSelectTab('downloads')}
      >
        <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"></path>
          <polyline points="7 10 12 15 17 10"></polyline>
          <line x1="12" y1="15" x2="12" y2="3"></line>
        </svg>
        <span>Downloads</span>
      </button>

      <button
        class="nav-tab {activeTab === 'settings' ? 'active' : ''}"
        on:click={() => onSelectTab('settings')}
      >
        <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <circle cx="12" cy="12" r="3"></circle>
          <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1 0 2.83 2 2 0 0 1-2.83 0l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-2 2 2 2 0 0 1-2-2v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83 0 2 2 0 0 1 0-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1-2-2 2 2 0 0 1 2-2h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 0-2.83 2 2 0 0 1 2.83 0l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 2-2 2 2 0 0 1 2 2v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 0 2 2 0 0 1 0 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 2 2 2 2 0 0 1-2 2h-.09a1.65 1.65 0 0 0-1.51 1z"></path>
        </svg>
        <span>Settings</span>
      </button>

      <button
        class="nav-tab {activeTab === 'diagnostics' ? 'active' : ''}"
        on:click={() => onSelectTab('diagnostics')}
      >
        <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <path d="M22 12h-4l-3 9L9 3l-3 9H2"></path>
        </svg>
        <span>Diagnostics</span>
      </button>
    </nav>
  </div>
</header>


<style>
  .nav-header {
    position: sticky;
    top: 0;
    z-index: 50;
    backdrop-filter: blur(20px);
    -webkit-backdrop-filter: blur(20px);
    background: rgba(8, 11, 17, 0.85);
    border-bottom: 1px solid var(--border-subtle);
  }

  .nav-container {
    max-width: 1280px;
    margin: 0 auto;
    padding: 0 24px;
    height: 72px;
    display: flex;
    align-items: center;
    justify-content: space-between;
  }

  .brand {
    display: flex;
    align-items: center;
    gap: 12px;
    cursor: pointer;
    user-select: none;
  }

  .logo-mark {
    width: 38px;
    height: 38px;
    border-radius: var(--radius-sm);
    background: rgba(99, 102, 241, 0.12);
    border: 1px solid rgba(99, 102, 241, 0.3);
    display: flex;
    align-items: center;
    justify-content: center;
    box-shadow: 0 0 15px rgba(99, 102, 241, 0.2);
  }

  .brand-text {
    display: flex;
    flex-direction: column;
  }

  .brand-name {
    font-family: var(--font-display);
    font-size: 1.25rem;
    font-weight: 800;
    letter-spacing: 0.05em;
    background: var(--accent-gradient);
    -webkit-background-clip: text;
    background-clip: text;
    -webkit-text-fill-color: transparent;

  }

  .brand-sub {
    font-size: 0.65rem;
    font-weight: 700;
    letter-spacing: 0.15em;
    color: var(--text-muted);
  }

  .nav-tabs {
    display: flex;
    gap: 8px;
    background: rgba(23, 31, 48, 0.5);
    padding: 6px;
    border-radius: var(--radius-full);
    border: 1px solid var(--border-subtle);
  }

  .nav-tab {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 16px;
    border-radius: var(--radius-full);
    background: transparent;
    color: var(--text-secondary);
    font-size: 0.875rem;
    font-weight: 500;
  }

  .nav-tab:hover {
    color: var(--text-primary);
    background: rgba(255, 255, 255, 0.05);
  }

  .nav-tab.active {
    background: rgba(99, 102, 241, 0.2);
    color: #fff;
    border: 1px solid rgba(99, 102, 241, 0.4);
    box-shadow: 0 0 15px rgba(99, 102, 241, 0.2);
  }

  .live-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--status-playing);
    box-shadow: 0 0 8px var(--status-playing);
  }
</style>
