<script>
  // Rechtsklikmenu op een noot (0.7.87): een lijst knoppen op de muispositie;
  // sluit bij een klik erbuiten of Escape. `items`: [{ label, on, disabled,
  // scheiding, kop }].
  import { createEventDispatcher } from 'svelte';
  export let x = 0;
  export let y = 0;
  export let items = [];
  const dispatch = createEventDispatcher();
  let el;
  function buiten(e) { if (el && !el.contains(e.target)) dispatch('close'); }
  function onKey(e) { if (e.key === 'Escape') { dispatch('close'); e.stopPropagation(); } }
  function kies(it) { if (it.disabled) return; dispatch('close'); if (typeof it.on === 'function') it.on(); }
  // Binnen het venster houden.
  function positie(node) {
    const r = node.getBoundingClientRect();
    const px = Math.min(x, window.innerWidth - r.width - 8);
    const py = Math.min(y, window.innerHeight - r.height - 8);
    node.style.left = Math.max(0, px) + 'px';
    node.style.top = Math.max(0, py) + 'px';
    const b = node.querySelector('button:not(:disabled)');
    if (b) b.focus();
  }
</script>

<svelte:window on:mousedown={buiten} on:keydown={onKey} on:blur={() => dispatch('close')} />

<div class="menu" role="menu" bind:this={el} use:positie>
  {#each items as it}
    {#if it.scheiding}
      <div class="scheiding"></div>
    {:else if it.kop}
      <div class="kop">{it.label}</div>
    {:else}
      <button role="menuitem" disabled={it.disabled} on:click={() => kies(it)}>{it.label}</button>
    {/if}
  {/each}
</div>

<style>
  .menu {
    position: fixed; z-index: 70; min-width: 13rem;
    display: flex; flex-direction: column; padding: 0.3rem 0;
    background: var(--bg-elevated, #333); color: var(--text, #eee);
    border: 1px solid var(--text-muted, #555); border-radius: 6px;
    box-shadow: 0 6px 18px rgba(0, 0, 0, 0.4);
  }
  .menu button {
    border: none; background: transparent; color: inherit; text-align: left;
    padding: 0.35rem 0.9rem; font-size: 0.85rem; cursor: pointer; min-height: 2rem;
  }
  .menu button:hover:not(:disabled), .menu button:focus-visible { background: var(--bg-panel, #2a2a2a); outline: none; }
  .menu button:disabled { opacity: 0.4; cursor: default; }
  .kop { padding: 0.25rem 0.9rem 0.1rem; font-size: 0.72rem; color: var(--text-muted, #aaa); text-transform: uppercase; letter-spacing: 0.04em; }
  .scheiding { height: 1px; margin: 0.25rem 0; background: var(--text-muted, #555); }
</style>
