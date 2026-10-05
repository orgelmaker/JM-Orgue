<script>
  // Hulpvenster (0.7.85): de uitleg die eerst als lange regel onder het
  // blad stond, per onderwerp, plus de sneltoetsen.
  import { createEventDispatcher } from 'svelte';
  import { t } from '../../lib/i18n.js';
  const dispatch = createEventDispatcher();
  function onKey(e) { if (e.key === 'Escape') { dispatch('close'); e.stopPropagation(); } }
  function focusEerste(node) { const el = node.querySelector('button'); if (el) el.focus(); }
</script>

<!-- svelte-ignore a11y-no-noninteractive-element-interactions -->
<div class="overlay" role="dialog" aria-modal="true" tabindex="-1" on:keydown={onKey} use:focusEerste>
  <div class="modal">
    <h3>{$t('notation.help_title')}</h3>
    <h4>{$t('notation.view_staff')}</h4>
    <p>{$t('notation.hint_live')}</p>
    <h4>{$t('notation.view_klavar')}</h4>
    <p>{$t('notation.hint_klavar')}</p>
    <h4>{$t('notation.help_shortcuts_title')}</h4>
    <p class="toetsen">{$t('notation.help_shortcuts')}</p>
    <div class="acties">
      <span class="spacer"></span>
      <button class="btn btn-primary btn-sm" on:click={() => dispatch('close')}>{$t('actions.close')}</button>
    </div>
  </div>
</div>

<style>
  .overlay { position: fixed; inset: 0; z-index: 50; background: rgba(0, 0, 0, 0.55); display: flex; align-items: center; justify-content: center; }
  .modal {
    background: var(--bg-panel, #2a2a2a); color: var(--text, #eee);
    border: 1px solid var(--text-muted, #555); border-radius: 8px;
    padding: 1rem 1.2rem; max-width: 44rem; width: calc(100% - 3rem); max-height: 85vh; overflow-y: auto;
    font-size: 0.85rem; line-height: 1.45;
  }
  .modal h3 { margin: 0 0 0.5rem; }
  .modal h4 { margin: 0.8rem 0 0.2rem; font-size: 0.85rem; color: var(--gold-border, #d4af37); }
  .modal p { margin: 0; }
  .toetsen { white-space: pre-line; }
  .acties { display: flex; margin-top: 0.8rem; }
  .spacer { flex: 1; }
</style>
