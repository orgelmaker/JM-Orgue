<script>
  // Presetkiezer (0.7.77, eigen presets en exporteren sinds 0.7.80): knop met
  // de huidige presetnaam, een uitklapper met zoekveld, de algemene profielen,
  // de eigen presets en de gemeten hoofdtelefoons (AutoEq, oratory1990), plus
  // "Importeren…" (AutoEq/Equalizer APO-tekst, Sweelinq .swes, eigen JSON) en
  // "Exporteren…". Meldt `apply` met { id, naam, banden, preamp, bron,
  // melding }, `fout` met een vertaalde tekst, `saveown` met de gekozen naam,
  // `deleteown` met een id en `export` zonder detail. Kent Tauri alleen voor
  // de bestandsdialoog en het lezen van het bestand.
  import { createEventDispatcher, tick } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { open as openDialog } from '@tauri-apps/plugin-dialog';
  import { t } from '../../lib/i18n.js';
  import { zoekPresets, presetsPerMerk, presetNaarBanden, presetSamenvatting, presetBron } from '../../lib/eqPresets.js';
  import { zoekLuidsprekers, luidsprekersPerMerk, luidsprekerBron } from '../../lib/eqLuidsprekers.js';
  import { herkenEnParse } from '../../lib/eqImport.js';

  export let naam = null;          // huidige presetnaam (of null)
  export let gewijzigd = false;    // banden wijken af van de preset
  export let algemeen = [];        // [{ id, label, banden, preamp }] — niet-gemeten profielen
  export let eigen = [];           // [{ id, naam, banden, preamp }] — eigen presets (0.7.80)
  export let soort = 'headphones'; // 'headphones' = AutoEq-hoofdtelefoons; 'speakers' = spinorama-luidsprekers (0.7.81)

  const dispatch = createEventDispatcher();
  let open = false;
  let zoektekst = '';
  let zoekveld;
  let bezig = false;
  let nieuweNaam = '';
  let naamVeldOpen = false;
  let naamVeld;
  let teVerwijderen = null;        // id waarvoor "Zeker?" staat

  $: luidsprekers = soort === 'speakers';
  $: resultaten = zoektekst.trim().length >= 2 ? (luidsprekers ? zoekLuidsprekers(zoektekst, 60) : zoekPresets(zoektekst, 60)) : null;
  const merkenHoofdtelefoon = presetsPerMerk();
  const merkenLuidsprekers = luidsprekersPerMerk();
  $: merken = luidsprekers ? merkenLuidsprekers : merkenHoofdtelefoon;
  // Een luidsprekerpreset wordt niet uit de bundel gelezen maar door de
  // eigenaar opgehaald (fetch_speaker_eq) en geparseerd.
  function kiesGemetenOfLuidspreker(p) {
    if (luidsprekers) { dispatch('fetchspeaker', { id: p.id, naam: p.model, pad: p.pad }); sluit(); }
    else kiesGemeten(p);
  }

  async function toggle() {
    open = !open;
    if (open) { zoektekst = ''; naamVeldOpen = false; teVerwijderen = null; await tick(); zoekveld?.focus?.(); }
  }
  function sluit() { open = false; naamVeldOpen = false; teVerwijderen = null; }
  function buitenKlik(e) {
    if (!open) return;
    if (!e.target.closest?.('.eq-preset-kiezer')) sluit();
  }

  function kiesGemeten(p) {
    dispatch('apply', { id: p.id, naam: p.model, banden: presetNaarBanden(p), preamp: p.preamp, bron: 'preset', melding: '' });
    sluit();
  }
  function kiesAlgemeen(a) {
    dispatch('apply', { id: a.id, naam: a.label, banden: a.banden.map((b) => ({ ...b })), preamp: a.preamp ?? null, bron: 'algemeen', melding: '' });
    sluit();
  }
  function kiesEigen(p) {
    dispatch('apply', { id: p.id, naam: p.naam, banden: (p.banden || []).map((b) => ({ ...b })), preamp: p.preamp ?? null, preampAuto: p.preampAuto !== false, bron: 'eigen', melding: '' });
    sluit();
  }
  async function openNaamVeld() {
    naamVeldOpen = true;
    nieuweNaam = naam && !/^\(/.test(naam) ? naam : '';
    await tick();
    naamVeld?.focus?.();
  }
  function bewaar() {
    const n = nieuweNaam.trim();
    if (!n) return;
    dispatch('saveown', n);
    sluit();
  }
  function verwijder(p) {
    if (teVerwijderen !== p.id) { teVerwijderen = p.id; return; }
    teVerwijderen = null;
    dispatch('deleteown', p.id);
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

<span class="eq-preset-kiezer" style="position:relative; display:inline-flex; gap:0.3rem; align-items:center; flex-wrap:wrap;">
  <button class="btn btn-secondary btn-sm" style="font-size:0.72rem; padding:0.15rem 0.5rem; max-width:16rem; overflow:hidden; text-overflow:ellipsis; white-space:nowrap;"
    on:click|stopPropagation={toggle} title={naam || $t('eq.preset_none')}>
    {$t('eq.preset')}: {naam || $t('eq.preset_none')}{gewijzigd ? ' ' + $t('eq.preset_modified') : ''} ▾
  </button>
  <button class="btn btn-ghost btn-sm" style="font-size:0.72rem; padding:0.15rem 0.5rem;" on:click|stopPropagation={importeer} disabled={bezig}>{$t('eq.import')}</button>
  <button class="btn btn-ghost btn-sm" style="font-size:0.72rem; padding:0.15rem 0.5rem;" on:click|stopPropagation={() => dispatch('export')}>{$t('eq.export')}</button>
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
            <button class="eq-preset-regel" on:click={() => kiesGemetenOfLuidspreker(p)}>
              <span>{p.model}</span><span class="eq-preset-info">{luidsprekers ? '' : presetSamenvatting(p)}</span>
            </button>
          {/each}
        {/if}
      {:else}
        <!-- Eigen presets (0.7.80) -->
        <div class="eq-preset-kop">{$t('eq.preset_own')}</div>
        {#if !eigen.length}
          <div class="settings-hint" style="margin:0 0 0.3rem;">{$t('eq.preset_none_own')}</div>
        {/if}
        {#each eigen as p (p.id)}
          <div style="display:flex; align-items:center; gap:0.2rem;">
            <button class="eq-preset-regel" on:click={() => kiesEigen(p)}>
              <span>{p.naam}</span><span class="eq-preset-info">{(p.banden || []).length}</span>
            </button>
            <button class="btn btn-ghost btn-sm" style="font-size:0.68rem; padding:0.05rem 0.4rem; white-space:nowrap;" on:click={() => verwijder(p)}
              title={$t('eq.preset_delete')}>{teVerwijderen === p.id ? $t('eq.preset_delete_confirm') : '✕'}</button>
          </div>
        {/each}
        {#if naamVeldOpen}
          <form style="display:flex; gap:0.3rem; margin:0.3rem 0;" on:submit|preventDefault={bewaar}>
            <input type="text" bind:this={naamVeld} bind:value={nieuweNaam} placeholder={$t('eq.preset_name')} maxlength="60"
              style="flex:1; font-size:0.78rem; padding:0.25rem 0.4rem; background:var(--bg-darkest); border:1px solid var(--accent-soft-2); border-radius:var(--radius-sm); color:var(--text);" />
            <button class="btn btn-secondary btn-sm" type="submit" style="font-size:0.72rem;" disabled={!nieuweNaam.trim()}>{$t('eq.preset_save_own').replace('…', '')}</button>
          </form>
        {:else}
          <button class="btn btn-ghost btn-sm" style="font-size:0.72rem; margin:0.2rem 0 0.4rem;" on:click={openNaamVeld}>{$t('eq.preset_save_own')}</button>
        {/if}
        {#if algemeen.length}
          <div class="eq-preset-kop">{$t('eq.preset_general')}</div>
          {#each algemeen as a (a.id)}
            <button class="eq-preset-regel" on:click={() => kiesAlgemeen(a)}>
              <span>{a.label}</span><span class="eq-preset-info">{a.banden.length}</span>
            </button>
          {/each}
        {/if}
        <div class="eq-preset-kop">{luidsprekers ? $t('eq.preset_measured_speakers') : $t('eq.preset_measured')}</div>
        <div class="settings-hint" style="margin:0 0 0.3rem;">{$t('eq.preset_refine')}</div>
        {#each merken as m (m.merk)}
          <details>
            <summary style="cursor:pointer; padding:0.15rem 0.2rem;">{m.merk} <span class="eq-preset-info">({m.presets.length})</span></summary>
            {#each m.presets as p (p.id)}
              <button class="eq-preset-regel" style="padding-left:1.2rem;" on:click={() => kiesGemetenOfLuidspreker(p)}>
                <span>{p.model}</span><span class="eq-preset-info">{luidsprekers ? '' : presetSamenvatting(p)}</span>
              </button>
            {/each}
          </details>
        {/each}
      {/if}
      <div class="settings-hint" style="margin-top:0.5rem; border-top:1px solid var(--accent-soft-2); padding-top:0.35rem;">
        {#if luidsprekers}
          {$t('eq.preset_credit_speakers')}{luidsprekerBron.gegenereerd ? ` (${luidsprekerBron.gegenereerd})` : ''}
        {:else}
          {$t('eq.preset_credit')}{presetBron.gegenereerd ? ` (${presetBron.gegenereerd})` : ''}
        {/if}
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
