<script lang="ts">
  import { STATUS_LABELS, type Badge } from '../lib/library';

  export let badge: Badge | null = null;
</script>

{#if badge}
  <div class="poster-marks">
    {#if badge.status}
      <span class="mark mark-{badge.status}">{STATUS_LABELS[badge.status] ?? badge.status}</span>
    {/if}
    {#if badge.lists.length > 0}
      <span class="mark mark-list" title={badge.lists.map((list) => list.name).join(', ')}>
        {badge.lists.length === 1 ? badge.lists[0].name : `${badge.lists.length} lists`}
      </span>
    {/if}
  </div>
{/if}

<style>
  .poster-marks {
    position: absolute;
    left: 8px;
    bottom: 8px;
    z-index: 2;
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    max-width: calc(100% - 16px);
    pointer-events: none;
  }

  .mark {
    max-width: 100%;
    padding: 3px 7px;
    border-radius: var(--radius-xs);
    border: 1px solid var(--border-subtle);
    background: rgba(8, 10, 16, 0.84);
    color: var(--text-secondary);
    font-size: 0.68rem;
    font-weight: 700;
    letter-spacing: 0.04em;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .mark-to_watch { color: #93c5fd; border-color: rgba(96, 165, 250, 0.5); }
  .mark-in_progress { color: var(--accent-primary); border-color: var(--accent-border); }
  .mark-watched { color: #34d399; border-color: rgba(52, 211, 153, 0.5); }
</style>
