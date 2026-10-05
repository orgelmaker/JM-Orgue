<script>
  // Lagenbalk (0.7.85, uit NotationWindow gelicht): per balk de armed-bol,
  // de naam (dubbelklik = hernoemen), ▲▼✕ en de takes. Divisies en hand
  // staan op het tabblad Balken & stemmen.
  import { t } from '../../lib/i18n.js';
  export let layers = [];
  export let armedLayer = null;
  export let recording = false;
  export let acties = {};
  let renameLayerId = null;
  function doe(naam, ...args) { const f = acties[naam]; if (typeof f === 'function') f(...args); }
  function focusSelect(node) { node.focus(); node.select(); }
  // Stemmen met noten in de zichtbare takes (gevulde knop).
  function stemmenMetNoten(layer) {
    const s = new Set();
    for (const t of layer.takes || []) { if (!t.visible) continue; for (const e of t.events) s.add(Number(e.voice) || 1); }
    return s;
  }
  function commitRename(layerId, naam) {
    if (renameLayerId !== layerId) return;
    renameLayerId = null;
    doe('rename', layerId, naam);
  }
</script>

<div class="lagen">
  <span class="label">{$t('notation.staves_label')}</span>
  {#each layers as layer, i (layer.id)}
    <div class="laag">
      <button class="arm" class:armed={armedLayer === layer.id} on:click={() => doe('arm', layer.id)}
        title={armedLayer === layer.id ? $t('notation.layer_armed_title') : $t('notation.layer_arm_title')}>●</button>
      {#if renameLayerId === layer.id}
        <input class="hernoem" type="text" value={layer.name} use:focusSelect
          on:keydown={(e) => { if (e.key === 'Enter') commitRename(layer.id, e.currentTarget.value); else if (e.key === 'Escape') renameLayerId = null; }}
          on:blur={(e) => commitRename(layer.id, e.currentTarget.value)} />
      {:else}
        <span class="naam" role="button" tabindex="0" on:dblclick={() => renameLayerId = layer.id}
          on:keydown={(e) => { if (e.key === 'F2') renameLayerId = layer.id; }} title={$t('notation.rename_staff_title')}>{layer.name}</span>
      {/if}
      <span class="stemmen" role="group" aria-label={$t('notation.voices')}>
        {#each [1, 2, 3, 4] as v}
          <button class="stem stem-{v}" class:actief={(Number(layer.active_voice) || 1) === v} class:gevuld={stemmenMetNoten(layer).has(v)}
            on:click={() => doe('setActiveVoice', layer.id, v)} title={$t('notation.voice_button_title').replaceAll('{n}', String(v))}>{v}</button>
        {/each}
      </span>
      <span class="tools">
        <button class="tool" on:click={() => doe('move', layer.id, -1)} disabled={i === 0} title={$t('notation.move_up')}>▲</button>
        <button class="tool" on:click={() => doe('move', layer.id, +1)} disabled={i === layers.length - 1} title={$t('notation.move_down')}>▼</button>
        <button class="tool verwijder" on:click={() => doe('remove', layer)} disabled={layers.length <= 1} title={$t('notation.remove_staff')}>✕</button>
      </span>
      <span class="takes">
        {#each layer.takes as take (take.id)}
          <label class="take" title={$t('notation.take_visible_title')}>
            <input type="checkbox" checked={take.visible} on:change={(e) => doe('toggleTakeVisible', layer.id, take.id, e.currentTarget.checked)} />
            {take.name}
            {#if layer.armed_take === take.id && armedLayer === layer.id && recording}<span class="live">•REC</span>{/if}
          </label>
        {/each}
        <button class="btn btn-ghost btn-sm take-add" on:click={() => doe('addTake', layer.id)} title={$t('notation.new_take_title')}>+</button>
      </span>
    </div>
  {/each}
  <button class="btn btn-ghost btn-sm toevoegen" on:click={() => doe('addLayer')} title={$t('notation.new_staff_title')}>{$t('notation.add_staff')}</button>
</div>

<style>
  .lagen {
    display: flex; align-items: center; flex-wrap: wrap; gap: 0.5rem; min-width: 0;
    padding: 0.35rem 0.75rem;
    background: var(--bg-elevated, #333); color: var(--text, #eee);
    border-bottom: 1px solid var(--text-muted, #555);
    font-size: 0.78rem; flex-shrink: 0;
  }
  .label { font-weight: 600; color: var(--text-muted, #aaa); }
  .laag {
    display: flex; align-items: center; gap: 0.45rem;
    padding: 0.2rem 0.5rem; border: 1px solid var(--text-muted, #555); border-radius: 6px;
    background: var(--bg-panel, #2a2a2a);
  }
  .arm {
    width: 1.1rem; height: 1.1rem; border-radius: 50%; border: 1px solid var(--text-muted, #666);
    background: transparent; color: var(--text-muted, #666); cursor: pointer; font-size: 0.9rem;
    display: inline-flex; align-items: center; justify-content: center; padding: 0;
  }
  .arm.armed { background: #cc3030; color: #fff; border-color: #ff5050; }
  .naam { font-weight: 600; cursor: text; }
  .hernoem { width: 8rem; font-size: 0.78rem; }
  .stemmen { display: inline-flex; gap: 0.1rem; }
  .stem {
    width: 1.35rem; height: 1.35rem; padding: 0; border-radius: 4px; cursor: pointer;
    border: 1px solid var(--text-muted, #666); background: transparent; color: var(--text-muted, #999);
    font-size: 0.7rem; line-height: 1;
  }
  .stem.gevuld { color: var(--text, #eee); font-weight: 700; }
  .stem.actief { background: var(--text, #eee); color: var(--bg-panel, #2a2a2a); border-color: var(--text, #eee); }
  .stem-2.actief { background: #2a6fdb; color: #fff; border-color: #2a6fdb; }
  .stem-3.actief { background: #2e8b57; color: #fff; border-color: #2e8b57; }
  .stem-4.actief { background: #e07b00; color: #fff; border-color: #e07b00; }
  .tools { display: inline-flex; gap: 0.1rem; }
  .tool { border: none; background: transparent; color: var(--text-muted, #aaa); font-size: 0.7rem; padding: 0 0.2rem; cursor: pointer; line-height: 1.4; }
  .tool:hover:not(:disabled) { color: var(--text, #eee); }
  .tool:disabled { opacity: 0.3; cursor: default; }
  .verwijder:hover:not(:disabled) { color: #ff7070; }
  .takes { display: flex; align-items: center; gap: 0.35rem; }
  .take { display: flex; align-items: center; gap: 0.2rem; }
  .live { color: #ff5050; font-weight: 700; font-size: 0.7rem; margin-left: 0.2rem; }
  .take-add { padding: 0 0.35rem; }
  .toevoegen { margin-left: auto; }
  @media print { .lagen { display: none !important; } }
</style>
