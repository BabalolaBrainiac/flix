<script lang="ts">
  import { onMount } from 'svelte';
  import { AlertTriangle, X } from 'lucide-svelte';

  export let title: string;
  export let message: string;
  export let confirmLabel: string;
  export let onCancel: () => void;
  export let onConfirm: () => void;

  let dialog: HTMLDivElement;
  let cancelButton: HTMLButtonElement;

  onMount(() => {
    const previousFocus = document.activeElement;
    cancelButton?.focus();
    return () => {
      if (previousFocus instanceof HTMLElement && previousFocus.isConnected) previousFocus.focus();
    };
  });

  function handleKeydown(event: KeyboardEvent) {
    if (event.key === 'Escape') {
      event.preventDefault();
      event.stopPropagation();
      onCancel();
      return;
    }
    if (event.key !== 'Tab') return;
    const buttons = Array.from(dialog.querySelectorAll<HTMLButtonElement>('button'));
    const first = buttons[0];
    const last = buttons[buttons.length - 1];
    if (event.shiftKey && document.activeElement === first) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && document.activeElement === last) {
      event.preventDefault();
      first.focus();
    }
  }
</script>

<div class="confirmation-backdrop" role="presentation" on:click|self={onCancel}>
  <div
    class="confirmation-dialog"
    role="alertdialog"
    aria-modal="true"
    aria-labelledby="confirmation-title"
    aria-describedby="confirmation-message"
    tabindex="-1"
    bind:this={dialog}
    on:keydown={handleKeydown}
  >
    <div class="confirmation-icon"><AlertTriangle size={20} /></div>
    <button class="confirmation-close" type="button" aria-label="Close" on:click={onCancel}><X size={18} /></button>
    <h2 id="confirmation-title">{title}</h2>
    <p id="confirmation-message">{message}</p>
    <div class="confirmation-actions">
      <button class="confirmation-cancel" type="button" bind:this={cancelButton} on:click={onCancel}>Cancel</button>
      <button class="confirmation-destructive" type="button" on:click={onConfirm}>{confirmLabel}</button>
    </div>
  </div>
</div>

<style>
  .confirmation-backdrop {
    position: fixed;
    inset: 0;
    z-index: 400;
    display: grid;
    place-items: center;
    padding: 24px;
    background: rgba(0, 0, 0, 0.52);
    animation: backdrop-in 180ms ease-out;
  }

  .confirmation-dialog {
    position: relative;
    width: min(420px, 100%);
    padding: 28px;
    overflow: hidden;
    border: 1px solid var(--modal-border);
    border-radius: var(--radius-xl);
    background: var(--modal-surface);
    box-shadow: var(--shadow-modal);
    animation: dialog-in 240ms cubic-bezier(0.16, 1, 0.3, 1);
  }

  .confirmation-icon {
    display: grid;
    width: 40px;
    height: 40px;
    margin-bottom: 16px;
    place-items: center;
    border: 1px solid rgba(255, 69, 58, 0.32);
    border-radius: 50%;
    background: rgba(255, 69, 58, 0.14);
    color: var(--status-red);
  }

  .confirmation-close {
    position: absolute;
    top: 16px;
    right: 16px;
    display: grid;
    width: 32px;
    height: 32px;
    place-items: center;
    border-radius: 50%;
    background: var(--control-fill);
    color: var(--text-secondary);
  }

  .confirmation-close:hover { background: var(--control-fill-hover); color: var(--text-primary); }
  h2 { font-size: 1.2rem; letter-spacing: -0.02em; }
  p { margin-top: 8px; color: var(--text-secondary); font-size: 0.9rem; line-height: 1.45; }

  .confirmation-actions { display: flex; gap: 10px; margin-top: 24px; }
  .confirmation-actions button { min-height: 42px; flex: 1; border-radius: 11px; font-size: 0.88rem; font-weight: 650; }
  .confirmation-cancel { border: 1px solid var(--border-subtle); background: var(--control-fill); color: var(--text-primary); }
  .confirmation-cancel:hover { background: var(--control-fill-hover); }
  .confirmation-destructive { background: var(--danger-fill); color: white; }
  .confirmation-destructive:hover { background: var(--danger-fill-hover); }

  @keyframes backdrop-in { from { opacity: 0; } to { opacity: 1; } }
  @keyframes dialog-in { from { opacity: 0; transform: scale(0.96) translateY(10px); } to { opacity: 1; transform: scale(1) translateY(0); } }

  @media (max-width: 480px) {
    .confirmation-dialog { padding: 24px 20px 20px; }
    .confirmation-actions { flex-direction: column-reverse; }
  }
</style>
