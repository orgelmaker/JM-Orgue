<script>
  // Tabblad Teksten & tekens (0.7.88): kop van het stuk, liedtekst typen,
  // aanwijzingen (met presets en de getrokken registratie) en de lijst van
  // alle teksten om ze te bewerken, te verplaatsen of te verwijderen.
  import { t, tx } from '../../lib/i18n.js';
  export let title = '';
  export let composer = '';
  export let subtitle = '';
  export let lyricMode = false;
  export let strofe = 1;
  export let texts = [];            // Score.texts
  export let laagNaam = () => '';   // layer_id → naam
  export let tijdTekst = () => '';  // start_us → "maat m, tel b"
  export let acties = {};
  // Tekens (0.7.89)
  export let tekenSticky = null;    // teken dat op de muis zit (buiten stapinvoer)
  export let stepTekens = new Set(); // kleverige articulaties in stapinvoer
  export let stepMode = false;
  export let selectieAantal = 0;
  function doe(naam, ...args) { const f = acties[naam]; if (typeof f === 'function') return f(...args); }
  const TEKENS = [
    { id: 'staccato', glyph: '•', toets: 'S' }, { id: 'tenuto', glyph: '–', toets: 'E' }, { id: 'accent', glyph: '>', toets: 'A' },
    { id: 'fermata', glyph: '𝄐', toets: 'F' }, { id: 'breath', glyph: '’', toets: '' }, { id: 'spelling', glyph: '♯↔♭', toets: '9' },
  ];
  const BALKTEKENS = [
    { id: 'repeat_start', glyph: '𝄆' }, { id: 'repeat_end', glyph: '𝄇' }, { id: 'ending1', glyph: '1.' },
    { id: 'ending2', glyph: '2.' }, { id: 'double', glyph: '𝄁' }, { id: 'final', glyph: '𝄂' },
  ];
  $: tekenActief = (id) => tekenSticky === id || (stepMode && stepTekens.has(id));

  const SOORTEN = ['tempo', 'expressive', 'technique', 'registration', 'dynamic', 'rehearsal', 'free'];
  const PRESETS = {
    tempo: ['Adagio', 'Andante', 'Moderato', 'Allegro', 'rit.', 'a tempo', '♩ = 72'],
    expressive: ['dolce', 'cantabile', 'legato', 'marcato', 'espressivo'],
    technique: ['man.', 'ped.', 'Ped. 16\'', 'Zw. dicht', 'Zw. open', 'HW + Zw.'],
    registration: [],
    dynamic: ['pp', 'p', 'mp', 'mf', 'f', 'ff', 'cresc.', 'dim.'],
    rehearsal: ['A', 'B', 'C', 'D', 'E'],
    free: [],
  };
  let soort = 'tempo';
  let tekst = '';
  let plaats = 'above';
  $: presets = PRESETS[soort] || [];
  $: if (soort === 'dynamic') plaats = 'below';
  let bewerkId = null;
  let bewerkTekst = '';
  async function registratie() {
    const r = await doe('vulRegistratie');
    if (typeof r === 'string' && r) { soort = 'registration'; tekst = r; }
  }
  function plaatsen() {
    if (!tekst.trim()) return;
    doe('addText', soort, tekst.trim(), plaats);
  }
  function startBewerken(m) { bewerkId = m.id; bewerkTekst = m.text; }
  function commitBewerken() {
    if (bewerkId == null) return;
    const id = bewerkId; bewerkId = null;
    if (bewerkTekst.trim()) doe('editText', id, bewerkTekst.trim());
  }
  function focusSelect(node) { node.focus(); node.select(); }
</script>

<div class="tab-rij">
  <span class="kopje">{$t('notation.header_section')}</span>
  <label>{$t('notation.title')}<input type="text" value={title} on:change={(e) => doe('setHeader', e.currentTarget.value, composer, subtitle)} /></label>
  <label>{$t('notation.composer')}<input type="text" value={composer} on:change={(e) => doe('setHeader', title, e.currentTarget.value, subtitle)} /></label>
  <label>{$t('notation.subtitle')}<input type="text" value={subtitle} on:change={(e) => doe('setHeader', title, composer, e.currentTarget.value)} /></label>
</div>
<div class="tab-rij">
  <span class="kopje">{$t('notation.lyrics_section')}</span>
  <button class="btn btn-sm" class:btn-primary={lyricMode} class:btn-ghost={!lyricMode} on:click={() => doe('toggleLyricMode')} title={$t('notation.lyric_mode_title')}>{$t('notation.lyric_mode')}</button>
  <label>{$t('notation.strofe')}
    <select value={strofe} on:change={(e) => doe('setStrofe', Number(e.currentTarget.value))}>
      <option value={1}>1</option><option value={2}>2</option><option value={3}>3</option>
    </select>
  </label>
  <span class="hint">{$t('notation.lyric_hint')}</span>
</div>
<div class="tab-rij">
  <span class="kopje">{$t('notation.mark_section')}</span>
  <select bind:value={soort} title={$t('notation.mark_kind')}>
    {#each SOORTEN as s}<option value={s}>{$t('notation.kind_' + s)}</option>{/each}
  </select>
  <input class="tekst" type="text" list="jm-tekst-presets" bind:value={tekst} placeholder={$t('notation.mark_text')} on:keydown={(e) => { if (e.key === 'Enter') plaatsen(); }} />
  <datalist id="jm-tekst-presets">{#each presets as p}<option value={p}></option>{/each}</datalist>
  <select bind:value={plaats} title={$t('notation.placement')}>
    <option value="above">{$t('notation.placement_above')}</option>
    <option value="below">{$t('notation.placement_below')}</option>
  </select>
  <button class="btn btn-primary btn-sm" on:click={plaatsen} disabled={!tekst.trim()} title={$t('notation.place_at_cursor_title')}>{$t('notation.place_at_cursor')}</button>
  <button class="btn btn-ghost btn-sm" on:click={registratie} title={$t('notation.take_registration_title')}>{$t('notation.take_registration')}</button>
</div>
<div class="tab-rij">
  <span class="kopje">{$t('notation.signs_section')}</span>
  {#each TEKENS as tk}
    <button class="btn btn-sm teken" class:btn-primary={tekenActief(tk.id)} class:btn-ghost={!tekenActief(tk.id)} on:click={() => doe('teken', tk.id)}
      title={$t('notation.sign_' + tk.id + '_title') + (tk.toets ? ' (' + tk.toets + ')' : '')}>{tk.glyph}</button>
  {/each}
  <span class="sep"></span>
  <button class="btn btn-ghost btn-sm" on:click={() => doe('span', 'slur')} disabled={selectieAantal < 2} title={$t('notation.slur_title')}>{$t('notation.slur')}</button>
  <button class="btn btn-ghost btn-sm" on:click={() => doe('span', 'crescendo')} disabled={selectieAantal < 2} title={$t('notation.cresc_title')}>cresc.</button>
  <button class="btn btn-ghost btn-sm" on:click={() => doe('span', 'diminuendo')} disabled={selectieAantal < 2} title={$t('notation.dim_title')}>dim.</button>
  <button class="btn btn-ghost btn-sm" on:click={() => doe('span', 'octaveup')} disabled={selectieAantal < 2} title={$t('notation.octave_line_up_title')}>8va</button>
  <button class="btn btn-ghost btn-sm" on:click={() => doe('span', 'octavedown')} disabled={selectieAantal < 2} title={$t('notation.octave_line_down_title')}>8vb</button>
  <button class="btn btn-ghost btn-sm" on:click={() => doe('spansWeg')} title={$t('notation.spans_remove_title')}>{$t('notation.spans_remove')}</button>
  <span class="sep"></span>
  {#each BALKTEKENS as bt}
    <button class="btn btn-ghost btn-sm teken" on:click={() => doe('maatteken', bt.id)} title={$t('notation.bar_' + bt.id + '_title')}>{bt.glyph}</button>
  {/each}
  <span class="hint">{$t('notation.signs_hint')}</span>
</div>
<div class="lijst">
  <span class="kopje">{$t('notation.texts_list')}</span>
  {#if !texts.length}<span class="hint">{$t('notation.no_texts')}</span>{/if}
  {#each texts as m (m.id)}
    <div class="tekst-rij">
      <span class="soort">{$t('notation.kind_' + (m.kind || 'free'))}</span>
      {#if bewerkId === m.id}
        <input class="tekst" type="text" bind:value={bewerkTekst} use:focusSelect
          on:keydown={(e) => { if (e.key === 'Enter') commitBewerken(); else if (e.key === 'Escape') bewerkId = null; }} on:blur={commitBewerken} />
      {:else}
        <span class="inhoud" role="button" tabindex="0" on:dblclick={() => startBewerken(m)} on:keydown={(e) => { if (e.key === 'F2') startBewerken(m); }} title={$t('notation.edit_text_title')}>{m.text}</span>
      {/if}
      <span class="waar">{laagNaam(m.layer_id)} · {tijdTekst(m.start_us)}</span>
      <button class="tool" on:click={() => doe('moveText', m.id, -1)} title={$t('notation.move_text_left')}>◄</button>
      <button class="tool" on:click={() => doe('moveText', m.id, +1)} title={$t('notation.move_text_right')}>►</button>
      <button class="tool" on:click={() => startBewerken(m)} title={$t('notation.edit_text_title')}>✎</button>
      <button class="tool verwijder" on:click={() => doe('removeText', m.id)} title={$t('actions.delete')}>✕</button>
    </div>
  {/each}
</div>

<style>
  .tab-rij { display: flex; align-items: center; flex-wrap: wrap; gap: 0.5rem; min-width: 0; }
  .tab-rij + .tab-rij, .lijst { margin-top: 0.4rem; }
  .tab-rij label { display: flex; align-items: center; gap: 0.3rem; font-size: 0.8rem; white-space: nowrap; }
  .tab-rij label input[type="text"] { width: 10rem; }
  .teken { min-width: 2rem; font-family: 'Segoe UI Symbol', 'Noto Music', inherit; }
  .sep { width: 0.6rem; }
  .kopje { font-size: 0.75rem; color: var(--text-muted, #aaa); min-width: 5.5rem; }
  .hint { font-size: 0.72rem; color: var(--text-muted, #aaa); }
  .tekst { width: 14rem; }
  .lijst { display: flex; flex-direction: column; gap: 0.2rem; max-height: 9rem; overflow-y: auto; }
  .tekst-rij { display: flex; align-items: center; gap: 0.4rem; font-size: 0.78rem; }
  .soort { color: var(--text-muted, #aaa); min-width: 6rem; }
  .inhoud { font-weight: 600; cursor: text; min-width: 8rem; }
  .waar { color: var(--text-muted, #aaa); }
  .tool { border: none; background: transparent; color: var(--text-muted, #aaa); font-size: 0.75rem; padding: 0 0.2rem; cursor: pointer; }
  .tool:hover { color: var(--text, #eee); }
  .verwijder:hover { color: #ff7070; }
</style>
