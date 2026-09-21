<script lang="ts">
  import type { LibraryTab } from '../lib/router';
  import { navigate } from '../lib/router';
  import type { PlaybackSnapshot } from '../lib/types';
  import { Bookmark, PlayCircle, Film } from 'lucide-svelte';
  import MyListsView from './MyListsView.svelte';
  import WatchStatusBoard from './WatchStatusBoard.svelte';
  import ContinueWatchingView from './ContinueWatchingView.svelte';

  export let tab: LibraryTab;
  export let activeProfileId: string | null = null;
  export let onOpenProfilePicker: () => void = () => {};
  export let playbackState: PlaybackSnapshot;
  export let onNavigateToDiscover: () => void;
  export let onStateChanged: () => void;

  const TABS: { key: LibraryTab; label: string; icon: typeof Bookmark }[] = [
    { key: 'lists', label: 'Lists', icon: Bookmark },
    { key: 'status', label: 'Status', icon: PlayCircle },
    { key: 'now-playing', label: 'Now Playing', icon: Film },
  ];

  function select(next: LibraryTab) {
    navigate({ name: 'library', library: next });
  }
</script>

<div class="library-view">
  <nav class="library-tabs" aria-label="Library sections">
    {#each TABS as option (option.key)}
      <button
        type="button"
        class="library-tab {tab === option.key ? 'active' : ''}"
        aria-pressed={tab === option.key}
        on:click={() => select(option.key)}
      >
        <svelte:component this={option.icon} size={15} />
        <span>{option.label}</span>
      </button>
    {/each}
  </nav>

  <div class="library-body" class:hidden={tab !== 'lists'}>
    <MyListsView {activeProfileId} {onOpenProfilePicker} />
  </div>
  <div class="library-body" class:hidden={tab !== 'status'}>
    <WatchStatusBoard {activeProfileId} {onOpenProfilePicker} />
  </div>
  <div class="library-body" class:hidden={tab !== 'now-playing'}>
    <ContinueWatchingView {playbackState} {onNavigateToDiscover} {onStateChanged} />
  </div>
</div>

<style>
  .library-view {
    max-width: 1320px;
    margin: 0 auto;
    padding: 20px 28px 0;
  }

  .library-tabs {
    display: flex;
    gap: 4px;
    margin-bottom: 8px;
    border-bottom: 1px solid var(--border-subtle);
  }

  .library-tab {
    display: flex;
    align-items: center;
    gap: 7px;
    padding: 10px 16px;
    background: transparent;
    border: none;
    border-bottom: 2px solid transparent;
    color: var(--text-secondary);
    font-size: 0.85rem;
    font-weight: 600;
  }

  .library-tab:hover {
    color: var(--text-primary);
  }

  .library-tab.active {
    color: var(--accent-primary);
    border-bottom-color: var(--accent-primary);
  }

  .library-body :global(.lists-view),
  .library-body :global(.watch-board-view) {
    padding: 20px 0;
  }

  .hidden {
    display: none;
  }

  @media (max-width: 800px) {
    .library-view {
      padding: 16px 16px 0;
    }
  }
</style>
