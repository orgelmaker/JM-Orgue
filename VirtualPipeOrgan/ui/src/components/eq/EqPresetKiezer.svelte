<script>
  // Presetkiezer (0.7.77): knop met de huidige presetnaam, een uitklapper met
  // zoekveld, de algemene profielen en de gemeten hoofdtelefoons (AutoEq,
  // oratory1990), plus "Importeren…" voor AutoEq/Equalizer APO-tekst,
  // Sweelinq .swes en eigen JSON. Meldt `apply` met { id, naam, banden,
  // preamp, bron, melding } en `fout` met een vertaalde tekst.
  import { createEventDispatcher, tick } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { open as openDialog } from '@tauri-apps/plugin-dialog';
  import { t } from '../../lib/i18n.js';
  import { zoekPresets, presetsPerMerk, presetNaarBanden, presetSamenvatting, presetBron } from '../../lib/eqPresets.js';
  import { herkenEnParse } from '../../lib/eqImport.js';

  export let naam = null;          // huidige presetnaam (of null)
  export let gewijzigd = false;    // banden wijken af van de preset
  export let algemeen = [];        // [{ id, label, banden, preamp }] — niet-gemeten profielen

  const dispatch = createEventDispatcher();
  let open = false;
  let zoektekst = '';
  let zoekveld;
  let bezig = false;

  $: resultaten = zoektekst.trim().length >= 2 ? zoekPresets(zoektekst, 60) : null;
  const merken = presetsPerMerk();

  async function toggle() {
    open = !open;
    if (open) { zoektekst = ''; await tick(); zoekveld?.focus?.(); }
  }
  function sluit() { open = false; }
  function buitenKlik(e) {
    if (!open) return;
    if (!e.target.closest?.('.eq-preset-kiezer')) open = false;
  }

  function kiesGemeten(p) {
    dispatch('apply', { id: p.id, naam: p.model, banden: presetNaarBanden(p), preamp: p.preamp, bron: 'preset', melding: '' });
    sluit();
  }
  function kiesAlgemeen(a) {
    dispatch('apply', { id: a.id, naam: a.label, banden: a.banden.map((b) => ({ ...b })), preamp: a.preamp ?? null, bron: 'algemeen', melding: '' });
    sluit();
  }

  function foutTekst(code) {
    const sleutel = `eq.import_err_${code}`;
    const vertaald = $t(sleutel);
    return vertaald === sleutel ? code : vertaald;
  }

  async function importeer() {
    if (bezig) return;
    bezig = true;
    try {
      const pad = await openDialog({
        multiple: false, directory: false,
        filters: [{ name: $t('eq.import_filter'), extensions: ['txt', 'swes', 'json'] }],
      });
      if (!pad) return;
      const bytes = await invoke('read_eq_file', { path: pad });
      const r = herkenEnParse(pad, bytes);
      const extra = [];
      if (r.kanaalregels) extra.push($t('eq.import_channels_skipped').replace('{n}', r.kanaalregels));
      if (r.overgeslagen) extra.push($t('eq.import_skipped').replace('{n}', r.overgeslagen));
      const melding = $t('eq.imported').replace('{n}', r.banden.length) + (extra.length ? ` (${extra.join(', ')})` : '');
      dispatch('apply', { id: null, naam: r.naam, banden: r.banden, preamp: r.preamp, bron: 'import', melding });
      sluit();
    } catch (e) {
      const msg = e instanceof Error ? foutTekst(e.message) : String(e);
      dispatch('fout', $t('eq.import_failed').replace('{error}', msg));
    } finally {
      bezig = false;
    }
  }
</script>

<svelte:window on:click={buitenKlik} />

<span class="eq-preset-kiezer" style="position:relative; display:inline-flex; gap:0.3rem; align-items:center;">
  <button class="btn btn-secondary btn-sm" style="font-size:0.72rem; padding:0.15rem 0.5rem; max-width:16rem; overflow:hidden; text-overflow:ellipsis; white-space:nowrap;"
    on:click|stopPropagation={toggle} title={naam || $t('eq.preset_none')}>
    {$t('eq.preset')}: {naam || $t('eq.preset_none')}{gewijzigd ? ' ' + $t('eq.preset_modified') : ''} ▾
  </button>
  <button class="btn btn-ghost btn-sm" style="font-size:0.72rem; padding:0.15rem 0.5rem;" on:click|stopPropagation={importeer} disabled={bezig}>{$t('eq.import')}</button>
  {#if open}
    <div class="eq-preset-popover" on:click|stopPropagation on:keydown={(e) => { if (e.key === 'Escape') sluit(); }} role="dialog" tabindex="-1"
      style="position:absolute; top:calc(100% + 0.3rem); left:0; z-index:40; width:min(28rem, 92vw); max-height:24rem; overflow:auto; background:var(--bg-elevated); border:1px solid var(--accent-soft-2); border-radius:var(--radius-sm); box-shadow:0 6px 24px rgba(0,0,0,0.25); padding:0.5rem; font-size:0.78rem;">
      <input type="search" bind:this={zoekveld} bind:value={zoektekst} placeholder={$t('eq.preset_search')}
        style="width:100%; box-sizing:border-box; font-size:0.8rem; padding:0.3rem 0.45rem; background:var(--bg-darkest); border:1px solid var(--accent-soft-2); border-radius:var(--radius-sm); color:var(--text); margin-bottom:0.4rem;" />
      {#if resultaten}
        {#if resultaten.length === 0}
          <div class="settings-hint">{$t('eq.preset_none_found')}</div>
        {:else}
          {#each resultaten as p (p.id)}
            <button class="eq-preset-regel" on:click={() => kiesGemeten(p)}>
              <span>{p.model}</span><span class="eq-preset-info">{presetSamenvatting(p)}</span>
            </button>
          {/each}
        {/if}
      {:else}
        {#if algemeen.length}
          <div class="eq-preset-kop">{$t('eq.preset_general')}</div>
          {#each algemeen as a (a.id)}
            <button class="eq-preset-regel" on:click={() => kiesAlgemeen(a)}>
              <span>{a.label}</span><span class="eq-preset-info">{a.banden.length}</span>
            </button>
          {/each}
        {/if}
        <div class="eq-preset-kop">{$t('eq.preset_measured')}</div>
        <div class="settings-hint" style="margin:0 0 0.3rem;">{$t('eq.preset_refine')}</div>
        {#each merken as m (m.merk)}
          <details>
            <summary style="cursor:pointer; padding:0.15rem 0.2rem;">{m.merk} <span class="eq-preset-info">({m.presets.length})</span></summary>
            {#each m.presets as p (p.id)}
              <button class="eq-preset-regel" style="padding-left:1.2rem;" on:click={() => kiesGemeten(p)}>
                <span>{p.model}</span><span class="eq-preset-info">{presetSamenvatting(p)}</span>
              </button>
            {/each}
          </details>
        {/each}
      {/if}
      <div class="settings-hint" style="margin-top:0.5rem; border-top:1px solid var(--accent-soft-2); padding-top:0.35rem;">
        {$t('eq.preset_credit')}{presetBron.gegenereerd ? ` (${presetBron.gegenereerd})` : ''}
      </div>
    </div>
  {/if}
</span>

<style>
  .eq-preset-regel {
    display: flex; justify-content: space-between; gap: 0.5rem; width: 100%;
    padding: 0.28rem 0.4rem; min-height: 1.9rem; border: 0; border-radius: var(--radius-sm);
    background: transparent; color: var(--text); text-align: left; cursor: pointer; font: inherit;
  }
  .eq-preset-regel:hover, .eq-preset-regel:focus-visible { background: var(--bg-darkest); outline: none; }
  .eq-preset-info { color: var(--text-muted); font-size: 0.7rem; white-space: nowrap; }
  .eq-preset-kop { font-weight: 600; margin: 0.4rem 0 0.2rem; color: var(--text-muted); font-size: 0.7rem; text-transform: uppercase; letter-spacing: 0.04em; }
</style>
