<script lang="ts">
  import type { PlaybackSnapshot } from '../lib/types';
  import { Search, Compass, Film, Download, BookOpen, Settings, Activity } from 'lucide-svelte';

  export let activeTab: 'search' | 'discover' | 'continue' | 'downloads' | 'reader' | 'settings' | 'diagnostics' = 'search';
  export let playbackState: PlaybackSnapshot = { state: 'idle' };
  export let onSelectTab: (tab: 'search' | 'discover' | 'continue' | 'downloads' | 'reader' | 'settings' | 'diagnostics') => void;

  $: isPlaying = playbackState.state === 'playing';
</script>

<header class="nav-header">
  <div class="nav-container">
    <button type="button" class="brand" on:click={() => onSelectTab('search')}>
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
        <Search size={15} />
        <span>Search</span>
      </button>

      <button
        class="nav-tab {activeTab === 'discover' ? 'active' : ''}"
        on:click={() => onSelectTab('discover')}
      >
        <Compass size={15} />
        <span>Discover</span>
      </button>

      <button
        class="nav-tab {activeTab === 'continue' ? 'active' : ''}"
        on:click={() => onSelectTab('continue')}
      >
        {#if isPlaying}
          <span class="live-dot"></span>
        {:else}
          <Film size={15} />
        {/if}
        <span>Library</span>
      </button>

      <button
        class="nav-tab {activeTab === 'downloads' ? 'active' : ''}"
        on:click={() => onSelectTab('downloads')}
      >
        <Download size={15} />
        <span>Downloads</span>
      </button>

      <button
        class="nav-tab {activeTab === 'reader' ? 'active' : ''}"
        on:click={() => onSelectTab('reader')}
      >
        <BookOpen size={15} />
        <span>Reader</span>
      </button>

      <button
        class="nav-tab {activeTab === 'settings' ? 'active' : ''}"
        on:click={() => onSelectTab('settings')}
      >
        <Settings size={15} />
        <span>Settings</span>
      </button>

      <button
        class="nav-tab {activeTab === 'diagnostics' ? 'active' : ''}"
        on:click={() => onSelectTab('diagnostics')}
      >
        <Activity size={15} />
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
    background: var(--bg-primary);
    border-bottom: 1px solid var(--border-subtle);
  }

  .nav-container {
    max-width: 1280px;
    margin: 0 auto;
    padding: 0 24px;
    height: 64px;
    display: flex;
    align-items: center;
    justify-content: space-between;
  }

  .brand {
    display: flex;
    align-items: center;
    background: transparent;
    cursor: pointer;
  }

  .brand-text {
    display: flex;
    align-items: baseline;
    gap: 6px;
  }

  .brand-name {
    font-size: 1.2rem;
    font-weight: 800;
    letter-spacing: 0.05em;
    color: var(--accent-primary);
  }

  .brand-sub {
    font-size: 0.65rem;
    font-weight: 700;
    letter-spacing: 0.12em;
    color: var(--text-muted);
  }

  .nav-tabs {
    display: flex;
    gap: 4px;
    background: var(--bg-secondary);
    padding: 4px;
    border-radius: var(--radius-md);
    border: 1px solid var(--border-subtle);
  }

  .nav-tab {
    flex-shrink: 0;
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 14px;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--text-secondary);
    font-size: 0.85rem;
    font-weight: 500;
  }

  .nav-tab:hover {
    color: var(--text-primary);
    background: var(--bg-surface);
  }

  .nav-tab.active {
    background: var(--bg-surface-active);
    color: var(--text-primary);
    box-shadow: inset 0 -2px 0 var(--accent-primary);
  }

  .live-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--status-green);
  }

  .nav-tab:focus-visible, .brand:focus-visible {
    outline: 2px solid var(--text-primary);
    outline-offset: 2px;
  }

  @media (max-width: 960px) {
    .nav-container {
      height: auto;
      min-width: 0;
      flex-direction: column;
      align-items: stretch;
      gap: 12px;
      padding: 14px 16px 10px;
    }
    .brand { align-self: flex-start; }
    .nav-tabs { min-width: 0; overflow-x: auto; }
    .nav-tab { padding: 8px 12px; }
  }
</style>
