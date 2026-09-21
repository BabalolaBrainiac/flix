<script lang="ts">
  import type { PlaybackSnapshot, ProfileSummary } from '../lib/types';
  import type { Route, RouteName } from '../lib/router';
  import { navigate } from '../lib/router';
  import { Compass, Bookmark, Download, BookOpen, ChevronDown, User, Activity, Power } from 'lucide-svelte';
  import { parseAvatarKey, iconFor } from '../lib/avatars';
  import { quitApp } from '../lib/api';

  export let route: Route;
  export let playbackState: PlaybackSnapshot = { state: 'idle' };
  export let activeProfile: ProfileSummary | null = null;
  export let onOpenProfilePicker: () => void = () => {};

  $: isPlaying = playbackState.state === 'playing' || (playbackState.state === 'browser' && playbackState.phase === 'playing');

  const TABS: { name: RouteName; label: string; icon: typeof Compass }[] = [
    { name: 'discover', label: 'Discover', icon: Compass },
    { name: 'library', label: 'Library', icon: Bookmark },
    { name: 'downloads', label: 'Downloads', icon: Download },
    { name: 'reader', label: 'Reader', icon: BookOpen },
  ];

  function getInitials(name: string): string {
    const parts = name.trim().split(/\s+/);
    if (parts.length >= 2) return (parts[0][0] + parts[1][0]).toUpperCase();
    return name.slice(0, 2).toUpperCase();
  }

  let menuOpen = false;
  let menuEl: HTMLElement;
  let menuButtonEl: HTMLButtonElement;
  const menuItemSelector = '[role="menuitem"]';

  function closeMenu() {
    menuOpen = false;
    menuButtonEl?.focus();
  }

  function toggleMenu() {
    menuOpen = !menuOpen;
  }

  function focusFirstMenuItem() {
    (menuEl?.querySelector(menuItemSelector) as HTMLElement | null)?.focus();
  }

  // A simple focus trap: Tab past the last item wraps to the first, and
  // Shift+Tab before the first wraps to the last, so focus never escapes an
  // open menu into the page behind it.
  function trapFocus(event: KeyboardEvent) {
    if (event.key === 'Escape') {
      event.preventDefault();
      closeMenu();
      return;
    }
    if (event.key !== 'Tab' || !menuEl) return;
    const items = Array.from(menuEl.querySelectorAll<HTMLElement>(menuItemSelector));
    if (items.length === 0) return;
    const [first, last] = [items[0], items[items.length - 1]];
    if (event.shiftKey && document.activeElement === first) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && document.activeElement === last) {
      event.preventDefault();
      first.focus();
    }
  }

  function handleMenuButtonKeydown(event: KeyboardEvent) {
    if (event.key === 'ArrowDown' || event.key === 'Enter' || event.key === ' ') {
      event.preventDefault();
      menuOpen = true;
    }
  }

  $: if (menuOpen) {
    requestAnimationFrame(focusFirstMenuItem);
  }

  function handleOutsideClick(event: MouseEvent) {
    if (menuOpen && menuEl && !menuEl.contains(event.target as Node) && !menuButtonEl.contains(event.target as Node)) {
      menuOpen = false;
    }
  }

  function openDiagnostics() {
    menuOpen = false;
    navigate({ name: 'diagnostics' });
  }

  function openProfilePicker(action: () => void) {
    menuOpen = false;
    action();
  }

  let isQuitting = false;

  async function handleQuit() {
    menuOpen = false;
    isQuitting = true;
    try {
      await quitApp();
    } catch {
      // The process may already be stopping when this returns - either way
      // there is nothing more useful to show the user here.
    }
  }
</script>

<svelte:window on:click={handleOutsideClick} />

<header class="nav-header">
  <div class="nav-container">
    <button type="button" class="brand" on:click={() => navigate({ name: 'discover' })}>
      <div class="brand-text">
        <span class="brand-name">FLIX</span>
        <span class="brand-sub">DESKTOP</span>
      </div>
    </button>

    <nav class="nav-tabs">
      {#each TABS as tab (tab.name)}
        <button
          class="nav-tab {route.name === tab.name ? 'active' : ''}"
          aria-current={route.name === tab.name ? 'page' : undefined}
          on:click={() => navigate({ name: tab.name })}
        >
          {#if tab.name === 'library' && isPlaying}
            <span class="live-dot"></span>
          {:else}
          <svelte:component this={tab.icon} size={17} strokeWidth={1.8} />
          {/if}
          <span>{tab.label}</span>
        </button>
      {/each}
    </nav>

    <div class="nav-right">
      <button
        type="button"
        class="profile-nav-btn"
        bind:this={menuButtonEl}
        aria-haspopup="menu"
        aria-expanded={menuOpen}
        on:click={toggleMenu}
        on:keydown={handleMenuButtonKeydown}
      >
        {#if activeProfile}
          {@const parsed = parseAvatarKey(activeProfile.avatar_key)}
          {@const AvatarIcon = iconFor(parsed.iconKey)}
          <div class="nav-avatar-chip" style="background-color: {parsed.color}">
            {#if AvatarIcon}
              <svelte:component this={AvatarIcon} size={15} strokeWidth={1.8} />
            {:else}
              {getInitials(activeProfile.name)}
            {/if}
          </div>
          <span class="nav-profile-name">{activeProfile.name}</span>
        {:else}
          <User size={17} strokeWidth={1.8} />
          <span>Profiles</span>
        {/if}
        <ChevronDown size={13} />
      </button>

      {#if menuOpen}
        <div
          class="avatar-menu"
          role="menu"
          tabindex="-1"
          aria-label="Profile menu"
          bind:this={menuEl}
          on:keydown={trapFocus}
        >
          <button type="button" role="menuitem" on:click={() => openProfilePicker(onOpenProfilePicker)}>
            <User size={16} strokeWidth={1.8} /> Profiles
          </button>
          <div class="menu-divider"></div>
          <button type="button" role="menuitem" on:click={openDiagnostics}>
            <Activity size={16} strokeWidth={1.8} /> Diagnostics
          </button>
          <button type="button" role="menuitem" on:click={handleQuit} disabled={isQuitting}>
            <Power size={16} strokeWidth={1.8} /> {isQuitting ? 'Quitting…' : 'Quit Flix'}
          </button>
        </div>
      {/if}
    </div>
  </div>
</header>

<style>
  .nav-right {
    position: relative;
    display: flex;
    align-items: center;
  }

  .profile-nav-btn {
    display: flex;
    align-items: center;
    gap: 10px;
    min-height: 38px;
    padding: 5px 11px;
    border-radius: var(--radius-md);
    background: var(--control-fill);
    border: 1px solid var(--border-subtle);
    color: var(--text-primary);
    font-size: 0.84rem;
    font-weight: 600;
    transition: background-color var(--transition-fast), border-color var(--transition-fast);
  }

  .profile-nav-btn:hover {
    background: var(--control-fill-hover);
    border-color: var(--border-muted);
  }

  .nav-avatar-chip {
    width: 24px;
    height: 24px;
    border-radius: 50%;
    display: flex;
    align-items: center;
    justify-content: center;
    color: #111;
    font-size: 0.72rem;
    font-weight: 800;
    font-family: var(--font-sans);
    box-shadow: 0 0 0 1px rgba(255, 255, 255, 0.12);
  }

  .nav-profile-name {
    max-width: 100px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .avatar-menu {
    position: absolute;
    top: calc(100% + 8px);
    right: 0;
    min-width: 206px;
    background: var(--modal-surface);
    border: 1px solid var(--modal-border);
    border-radius: var(--radius-lg);
    box-shadow: var(--shadow-modal);
    padding: 7px;
    display: flex;
    flex-direction: column;
    gap: 2px;
    z-index: 60;
    animation: menu-enter 160ms ease-out;
  }

  .avatar-menu button {
    display: flex;
    align-items: center;
    gap: 10px;
    text-align: left;
    min-height: 38px;
    padding: 8px 11px;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--text-primary);
    font-size: 0.86rem;
    font-weight: 500;
  }

  .avatar-menu button:hover,
  .avatar-menu button:focus-visible {
    background: var(--control-fill);
    color: var(--text-primary);
  }

  .menu-divider {
    height: 1px;
    background: var(--border-subtle);
    margin: 4px 2px;
  }

  .nav-header {
    position: sticky;
    top: 0;
    z-index: 50;
    background: var(--bg-secondary);
    border-bottom: 1px solid var(--border-subtle);
    transition: background-color var(--transition-fast);
    box-shadow: 0 1px 0 rgba(0, 0, 0, 0.24);
  }

  .nav-container {
    max-width: 1320px;
    margin: 0 auto;
    padding: 0 28px;
    height: 64px;
    display: flex;
    align-items: center;
    justify-content: space-between;
  }

  .brand {
    display: flex;
    align-items: center;
    gap: 10px;
    background: transparent;
    cursor: pointer;
    text-decoration: none;
    padding: 6px 10px;
    border-radius: var(--radius-sm);
    transition: opacity var(--transition-fast);
  }

  .brand:hover {
    opacity: 0.72;
  }

  .brand-text {
    display: flex;
    align-items: baseline;
    gap: 8px;
  }

  .brand-name {
    font-size: 1.34rem;
    font-weight: 800;
    letter-spacing: -0.07em;
    color: var(--text-primary);
  }

  .brand-sub {
    font-size: 0.62rem;
    font-weight: 650;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--text-muted);
    background: transparent;
    padding: 2px 0;
    border-radius: var(--radius-xs);
    font-family: var(--font-sans);
  }

  .nav-tabs {
    display: flex;
    gap: 2px;
    background: var(--control-fill);
    padding: 3px;
    border-radius: var(--radius-md);
    border: 1px solid var(--border-subtle);
  }

  .nav-tab {
    flex-shrink: 0;
    display: flex;
    align-items: center;
    gap: 8px;
    min-height: 36px;
    padding: 7px 14px;
    border-radius: var(--radius-sm);
    background: transparent;
    border: 1px solid transparent;
    color: var(--text-secondary);
    font-size: 0.85rem;
    font-weight: 500;
    transition: background-color var(--transition-fast), color var(--transition-fast), box-shadow var(--transition-fast);
    position: relative;
  }

  .nav-tab:hover {
    color: var(--text-primary);
    background: var(--control-fill-hover);
  }

  .nav-tab.active {
    background: var(--bg-surface-hover);
    color: var(--text-primary);
    border-color: var(--border-muted);
    font-weight: 620;
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.22);
  }

  .nav-tab.active :global(svg) { color: var(--accent-primary); }

  @keyframes menu-enter {
    from { opacity: 0; transform: translateY(-5px) scale(0.98); }
    to { opacity: 1; transform: translateY(0) scale(1); }
  }

  .live-dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--status-green);
    box-shadow: 0 0 10px var(--status-green);
    animation: live-pulse 2s cubic-bezier(0.4, 0, 0.6, 1) infinite;
  }

  @keyframes live-pulse {
    0%, 100% {
      opacity: 1;
      transform: scale(1);
    }
    50% {
      opacity: 0.4;
      transform: scale(0.85);
    }
  }

  .nav-tab:focus-visible, .brand:focus-visible {
    outline: 2px solid var(--border-focus);
    outline-offset: 2px;
  }

  @media (max-width: 960px) {
    .nav-container {
      height: auto;
      min-width: 0;
      flex-wrap: wrap;
      gap: 8px;
      padding: 10px 16px;
    }
    .nav-tabs { order: 3; width: 100%; min-width: 0; overflow-x: auto; }
    .nav-tab { flex: 1; justify-content: center; padding: 8px 12px; }
  }

  @media (max-width: 520px) {
    .brand-sub, .nav-profile-name { display: none; }
    .nav-tabs { justify-content: space-between; }
    .nav-tab { flex-direction: column; gap: 3px; min-width: 68px; font-size: 0.69rem; }
    .profile-nav-btn { gap: 6px; }
  }
</style>
