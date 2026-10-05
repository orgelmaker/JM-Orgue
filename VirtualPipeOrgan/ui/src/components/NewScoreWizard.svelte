<script>
  // Wizard "Nieuw stuk" (0.7.83). Stap 1 is het stuk: titel, componist,
  // ondertitel, maatsoort, toonsoort, tempo, raster en het aantal maten
  // waarmee het blad begint. Stap 2 zijn de balken (sinds 0.7.14): naam,
  // sleutel, divisie-routering en in klavar de hand. mode 'new' maakt een
  // vers stuk; mode 'edit' past het lopende stuk aan, waarbij de balken
  // alleen te wijzigen zijn zolang er nog geen noten staan.
  //
  // De ouder maakt per opening een nieuwe instantie ({#if wizard}), dus
  // `initial` wordt één keer overgenomen bij het aanmaken — geen $:-blok dat
  // toestand zet (Svelte-val: eerder gedraaide $:-blokken blijven dan oud).
  import { createEventDispatcher } from 'svelte';
  import { t, tx } from '../lib/i18n.js';
  import { MAATSOORTEN, splitsMaatsoort } from '../lib/notatieKeuzes.js';

  export let mode = 'new';          // 'new' | 'edit'
  export let initial = null;        // { title, composer, subtitle, beats, unit, keyFifths, minor, bpm, quantize, minMeasures, staves }
  export let divisions = [];        // divisienamen van het geladen orgel
  export let hasNotes = false;      // in 'edit': balken op slot
  export let showHand = false;      // klavar-weergave: hand per balk
  export let splitOpties = [];      // [[midi, naam]] voor R+L
  export let keyChoices = [];       // [{ v, label }]
  export let gridChoices = [];      // [{ v, label }]
  const dispatch = createEventDispatcher();

  let stap = 1;
  let stuk = { title: '', composer: '', subtitle: '', beats: 4, unit: 4, keyFifths: 0, minor: false, bpm: 90, quantize: 4, minMeasures: 8 };
  let staves = [];
  let maatsoort = '4/4';
  if (initial) {
    const { staves: st, ...rest } = initial;
    stuk = { ...stuk, ...rest };
    staves = (st || []).map(s => ({ ...s, divisions: [...(s.divisions || [])] }));
    maatsoort = `${stuk.beats}/${stuk.unit}`;
  }
  if (staves.length === 0) staves = [nieuweBalk(1)];

  $: lockStaves = mode === 'edit' && hasNotes;
  $: maatsoortKeuzes = MAATSOORTEN.includes(maatsoort) ? MAATSOORTEN : [...MAATSOORTEN, maatsoort];
  // Bij /8 past een kwart niet op het raster: minstens achtsten.
  $: rasterKeuzes = stuk.unit === 8 ? gridChoices.filter(g => g.v >= 2) : gridChoices;

  function nieuweBalk(n) {
    return { name: tx('notation.staff_n').replace('{n}', n), bass: false, divisions: [], hand: 'auto', split: 60 };
  }
  function kiesMaatsoort(v) {
    maatsoort = v;
    const [b, u] = splitsMaatsoort(v);
    stuk.beats = b; stuk.unit = u;
    if (u === 8 && Number(stuk.quantize) < 2) stuk.quantize = 2;
  }
  function addStaff() { staves = [...staves, nieuweBalk(staves.length + 1)]; }
  function removeStaff(i) { staves = staves.filter((_, idx) => idx !== i); }
  function toggleDivision(i, div) {
    const st = staves[i];
    st.divisions = st.divisions.includes(div) ? st.divisions.filter(d => d !== div) : [...st.divisions, div];
    staves = staves;
  }
  function apply() {
    if (staves.length === 0) return;
    const klem = (v, lo, hi, std) => Math.min(hi, Math.max(lo, Number(v) || std));
    const out = {
      title: String(stuk.title ?? '').trim(), composer: String(stuk.composer ?? '').trim(), subtitle: String(stuk.subtitle ?? '').trim(),
      beats: klem(stuk.beats, 1, 12, 4), unit: [2, 4, 8].includes(Number(stuk.unit)) ? Number(stuk.unit) : 4,
      keyFifths: klem(stuk.keyFifths, -7, 7, 0), minor: !!stuk.minor,
      bpm: klem(stuk.bpm, 20, 300, 90), quantize: klem(stuk.quantize, 1, 8, 4),
      minMeasures: Math.round(klem(stuk.minMeasures, 0, 10000, 0)),
    };
    if (out.unit === 8 && out.quantize < 2) out.quantize = 2;
    dispatch('apply', { stuk: out, staves });
  }
  function onKey(e) {
    if (e.key === 'Escape') { dispatch('close'); e.stopPropagation(); }
  }
  // Eerste veld meteen focussen: Escape en typen werken dan zonder klik.
  function focusEerste(node) {
    const el = node.querySelector('input, select, button');
    if (el) el.focus();
  }
</script>

<!-- svelte-ignore a11y-no-noninteractive-element-interactions -->
<div class="wizard-overlay" role="dialog" aria-modal="true" tabindex="-1" on:keydown={onKey} use:focusEerste>
  <div class="wizard-modal">
    <h3>{mode === 'new' ? $t('notation.wizard_new_title') : $t('notation.wizard_title')}</h3>
    <div class="wizard-stappen" role="tablist">
      <button class="wizard-stap" class:active={stap === 1} role="tab" aria-selected={stap === 1} on:click={() => stap = 1}>1 · {$t('notation.wizard_step_piece')}</button>
      <button class="wizard-stap" class:active={stap === 2} role="tab" aria-selected={stap === 2} on:click={() => stap = 2}>2 · {$t('notation.wizard_step_staves')}</button>
    </div>

    {#if stap === 1}
      <div class="wizard-velden">
        <label class="veld breed">{$t('notation.title')}
          <input type="text" bind:value={stuk.title} />
        </label>
        <label class="veld breed">{$t('notation.composer')}
          <input type="text" bind:value={stuk.composer} />
        </label>
        <label class="veld breed">{$t('notation.subtitle')}
          <input type="text" bind:value={stuk.subtitle} />
        </label>
        <label class="veld">{$t('notation.meter')}
          <select value={maatsoort} on:change={(e) => kiesMaatsoort(e.currentTarget.value)}>
            {#each maatsoortKeuzes as m}<option value={m}>{m}</option>{/each}
          </select>
        </label>
        <label class="veld">{$t('notation.key_signature')}
          <select bind:value={stuk.keyFifths}>
            {#each keyChoices as k}<option value={k.v}>{k.label}</option>{/each}
          </select>
          <select bind:value={stuk.minor} title={$t('notation.key_mode')}>
            <option value={false}>{$t('notation.key_major')}</option>
            <option value={true}>{$t('notation.key_minor')}</option>
          </select>
        </label>
        <label class="veld">{$t('notation.tempo')}
          <input type="number" min="20" max="300" bind:value={stuk.bpm} />
          <button type="button" class="btn btn-ghost btn-sm" title={$t('notation.tempo_test_title')}
            on:click={() => dispatch('previewTempo', { bpm: Number(stuk.bpm) || 90, beats: Number(stuk.beats) || 4, unit: Number(stuk.unit) || 4 })}>♪ {$t('notation.tempo_test')}</button>
        </label>
        <label class="veld">{$t('notation.grid')}
          <select bind:value={stuk.quantize}>
            {#each rasterKeuzes as g}<option value={g.v}>{g.label}</option>{/each}
          </select>
        </label>
        <label class="veld">{$t('notation.measures_min')}
          <input type="number" min="0" max="10000" bind:value={stuk.minMeasures} />
          <span class="wizard-hint inline">{$t('notation.measures_grow')}</span>
        </label>
      </div>
    {:else}
      <p class="wizard-hint">{$t('notation.wizard_hint')}</p>
      {#if lockStaves}<p class="wizard-hint">{$t('notation.wizard_has_notes')}</p>{/if}
      {#each staves as st, i}
        <div class="wizard-staff">
          <input class="wizard-staff-name" type="text" bind:value={st.name} disabled={lockStaves} title={$t('notation.staff_name_title')} />
          <select bind:value={st.bass} disabled={lockStaves} title={$t('notation.clef')}>
            <option value={false}>𝄞 {$t('notation.clef_treble')}</option>
            <option value={true}>𝄢 {$t('notation.clef_bass')}</option>
          </select>
          {#if showHand}
            <select class="layer-hand" bind:value={st.hand} disabled={lockStaves} title={$t('notation.hand_title')}>
              <option value="auto">{$t('notation.hand')}: {$t('notation.hand_auto')}</option>
              <option value="right">{$t('notation.hand_right')}</option>
              <option value="left">{$t('notation.hand_left')}</option>
              <option value="pedal">{$t('notation.hand_pedal')}</option>
              <option value="rl">{$t('notation.hand_split')}</option>
            </select>
            {#if st.hand === 'rl'}
              <select class="layer-split" bind:value={st.split} disabled={lockStaves} title={$t('notation.hand_split_title')}>
                {#each splitOpties as [m, naam]}<option value={m}>{naam}</option>{/each}
              </select>
            {/if}
          {/if}
          <span class="wizard-divs">
            {#each divisions as div}
              <label class="wizard-div">
                <input type="checkbox" disabled={lockStaves} checked={st.divisions.includes(div)} on:change={() => toggleDivision(i, div)} />
                {div}
              </label>
            {/each}
            {#if divisions.length === 0}<span class="wizard-hint inline">{$t('notation.no_organ_routing_later')}</span>{/if}
          </span>
          {#if staves.length > 1 && !lockStaves}
            <button class="wizard-remove" on:click={() => removeStaff(i)} title={$t('notation.remove_staff')} aria-label={$t('notation.remove_staff')}>×</button>
          {/if}
        </div>
      {/each}
      {#if !lockStaves}
        <button class="btn btn-ghost btn-sm" on:click={addStaff}>{$t('notation.add_staff')}</button>
      {/if}
    {/if}

    <div class="wizard-actions">
      {#if stap === 2}
        <button class="btn btn-ghost btn-sm" on:click={() => stap = 1}>‹ {$t('notation.wizard_back')}</button>
      {/if}
      <span class="notation-spacer"></span>
      <button class="btn btn-secondary btn-sm" on:click={() => dispatch('close')}>{$t('actions.cancel')}</button>
      {#if stap === 1}
        <button class="btn btn-ghost btn-sm" on:click={() => stap = 2}>{$t('notation.wizard_next')} ›</button>
      {/if}
      <button class="btn btn-primary btn-sm" on:click={apply}>{mode === 'new' ? $t('notation.wizard_start') : $t('notation.wizard_apply')}</button>
    </div>
  </div>
</div>

<style>
  .wizard-overlay {
    position: fixed; inset: 0; z-index: 50;
    background: rgba(0, 0, 0, 0.55);
    display: flex; align-items: center; justify-content: center;
  }
  .wizard-modal {
    background: var(--bg-panel, #2a2a2a); color: var(--text, #eee);
    border: 1px solid var(--text-muted, #555); border-radius: 8px;
    padding: 1rem 1.2rem; max-width: 46rem; width: calc(100% - 3rem);
    max-height: 85vh; overflow-y: auto;
  }
  .wizard-modal h3 { margin: 0 0 0.5rem; }
  .wizard-stappen { display: flex; gap: 0.3rem; margin-bottom: 0.7rem; border-bottom: 1px solid var(--text-muted, #555); }
  .wizard-stap {
    border: none; background: transparent; color: var(--text-muted, #aaa);
    padding: 0.3rem 0.7rem; cursor: pointer; font-size: 0.85rem;
    border-bottom: 2px solid transparent; margin-bottom: -1px;
  }
  .wizard-stap.active { color: var(--text, #eee); border-bottom-color: var(--gold-border, #d4af37); }
  .wizard-velden { display: grid; grid-template-columns: repeat(auto-fill, minmax(14rem, 1fr)); gap: 0.5rem 1rem; }
  .veld { display: flex; align-items: center; gap: 0.4rem; font-size: 0.82rem; white-space: nowrap; }
  .veld.breed { grid-column: 1 / -1; }
  .veld input[type="text"] { flex: 1; min-width: 6rem; }
  .veld input[type="number"] { width: 4.5rem; }
  .wizard-hint { font-size: 0.78rem; color: var(--text-muted, #aaa); margin: 0 0 0.6rem; }
  .wizard-hint.inline { margin: 0; white-space: normal; }
  .wizard-staff {
    display: flex; align-items: center; flex-wrap: wrap; gap: 0.5rem;
    padding: 0.35rem 0.5rem; margin-bottom: 0.4rem;
    border: 1px solid var(--text-muted, #555); border-radius: 6px;
  }
  .wizard-staff-name { width: 9rem; }
  .wizard-divs { display: flex; align-items: center; flex-wrap: wrap; gap: 0.5rem; }
  .wizard-div { display: flex; align-items: center; gap: 0.25rem; font-size: 0.8rem; white-space: nowrap; cursor: pointer; }
  .wizard-remove { margin-left: auto; border: none; background: none; color: #cc6666; font-size: 1.1rem; cursor: pointer; }
  .wizard-actions { display: flex; align-items: center; gap: 0.5rem; margin-top: 0.8rem; }
  .notation-spacer { flex: 1; }
  .layer-hand, .layer-split { font-size: 0.75rem; }
</style>
