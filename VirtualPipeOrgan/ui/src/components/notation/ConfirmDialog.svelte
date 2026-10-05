<script>
  // Eigen bevestigingsvraag (0.7.83/84, als onderdeel sinds 0.7.85):
  // window.confirm geeft in Tauri een Promise, dus een eigen modaal met
  // een vrije rij knoppen. `knoppen`: [{ label, stijl: 'primary'|'secondary'|'ghost', on }].
  import { createEventDispatcher } from 'svelte';
  export let tekst = '';
  export let knoppen = [];
  const dispatch = createEventDispatcher();
  function onKey(e) { if (e.key === 'Escape') { dispatch('cancel'); e.stopPropagation(); } }
  function focusEerste(node) { const el = node.querySelector('button.btn-primary') || node.querySelector('button'); if (el) el.focus(); }
</script>

<!-- svelte-ignore a11y-no-noninteractive-element-interactions -->
<div class="overlay" role="dialog" aria-modal="true" tabindex="-1" on:keydown={onKey} use:focusEerste>
  <div class="modal">
    <p>{tekst}</p>
    <div class="acties">
      <span class="spacer"></span>
      {#each knoppen as k}
        <button class="btn btn-sm {k.stijl === 'primary' ? 'btn-primary' : k.stijl === 'ghost' ? 'btn-ghost' : 'btn-secondary'}" on:click={() => k.on && k.on()}>{k.label}</button>
      {/each}
    </div>
  </div>
</div>

<style>
  .overlay { position: fixed; inset: 0; z-index: 55; background: rgba(0, 0, 0, 0.55); display: flex; align-items: center; justify-content: center; }
  .modal {
    background: var(--bg-panel, #2a2a2a); color: var(--text, #eee);
    border: 1px solid var(--text-muted, #555); border-radius: 8px;
    padding: 1rem 1.2rem; max-width: 28rem; width: calc(100% - 3rem);
  }
  .modal p { margin: 0.2rem 0 0.6rem; font-size: 0.9rem; }
  .acties { display: flex; align-items: center; gap: 0.5rem; margin-top: 0.6rem; }
  .spacer { flex: 1; }
</style>
