<script>
  // Tabbladen van het notatievenster (0.7.85): Invoer, Teksten & tekens,
  // Balken & stemmen, Afspelen (Ctrl+1..4).
  import { createEventDispatcher } from 'svelte';
  import { t } from '../../lib/i18n.js';
  export let tab = 'input';
  const dispatch = createEventDispatcher();
  const TABS = [
    ['input', 'tab_input', '✎'], ['texts', 'tab_texts', '𝄐'], ['staves', 'tab_staves', '𝄚'], ['playback', 'tab_playback', '▶'],
  ];
</script>

<div class="tabs" role="tablist">
  {#each TABS as [id, key, icoon], i}
    <button class="tab" class:active={tab === id} role="tab" aria-selected={tab === id}
      title={$t('notation.' + key) + ' (Ctrl+' + (i + 1) + ')'} on:click={() => dispatch('change', id)}>
      <span class="icoon" aria-hidden="true">{icoon}</span><span class="label">{$t('notation.' + key)}</span>
    </button>
  {/each}
</div>

<style>
  .tabs {
    display: flex; gap: 0.2rem; padding: 0.2rem 0.75rem 0; min-width: 0;
    background: var(--bg-panel, #2a2a2a); border-bottom: 1px solid var(--text-muted, #555);
    flex-shrink: 0;
  }
  .tab {
    border: 1px solid transparent; border-bottom: none; border-radius: 6px 6px 0 0;
    background: transparent; color: var(--text-muted, #aaa);
    padding: 0.3rem 0.8rem; cursor: pointer; font-size: 0.85rem; min-height: 2.1rem;
    display: inline-flex; align-items: center; gap: 0.35rem;
  }
  .tab.active {
    color: var(--text, #eee); background: var(--bg-elevated, #333);
    border-color: var(--text-muted, #555); margin-bottom: -1px; padding-bottom: calc(0.3rem + 1px);
  }
  .icoon { font-size: 0.95rem; }
  @media (max-width: 900px) { .label { display: none; } .tab { padding: 0.3rem 0.6rem; } }
  @media print { .tabs { display: none !important; } }
</style>
