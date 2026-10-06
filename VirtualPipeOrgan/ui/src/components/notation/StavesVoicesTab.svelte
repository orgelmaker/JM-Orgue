<script>
  // Tabblad Balken & stemmen (0.7.85): per balk de divisie-routering en de
  // hand (klavar), plus maatsoort, toonsoort, raster en akkoord-speling.
  // Stemmen per balk komen in een volgende versie.
  import { t } from '../../lib/i18n.js';
  export let layers = [];
  export let divisions = [];
  export let viewMode = 'staff';
  export let klavarModel = null;
  export let splitOpties = [];
  export let layerHandKeuze = () => 'auto';
  export let autoHandLabel = () => '';
  export let maatsoort = '4/4';
  export let maatsoortKeuzes = [];
  export let keyFifths = 0;
  export let minor = false;
  export let quantize = 4;
  export let tolerancePct = 80;
  export let keyChoices = [];
  export let gridChoices = [];
  export let onlyActive = false;
  export let acties = {};
  function doe(naam, ...args) { const f = acties[naam]; if (typeof f === 'function') f(...args); }
  // Schuif lokaal binden: het etiket loopt mee tijdens het slepen, de score
  // krijgt de waarde bij het loslaten.
  let speling = tolerancePct;
  $: speling = tolerancePct;
</script>

<div class="tab-rij">
  <label>{$t('notation.meter')}
    <select value={maatsoort} on:change={(e) => doe('setMeter', e.currentTarget.value)}>
      {#each maatsoortKeuzes as m}<option value={m}>{m}</option>{/each}
    </select>
  </label>
  <label>{$t('notation.key_signature')}
    <select value={keyFifths} on:change={(e) => doe('setKey', e.currentTarget.value)}>
      {#each keyChoices as k}<option value={k.v}>{k.label}</option>{/each}
    </select>
    <select value={minor ? 'minor' : 'major'} on:change={(e) => doe('setMode', e.currentTarget.value === 'minor')} title={$t('notation.key_mode')}>
      <option value="major">{$t('notation.key_major')}</option>
      <option value="minor">{$t('notation.key_minor')}</option>
    </select>
  </label>
  <label title={$t('notation.grid_title')}>{$t('notation.grid')}
    <select value={quantize} on:change={(e) => doe('setQuantize', Number(e.currentTarget.value))}>
      {#each gridChoices as g}<option value={g.v}>{g.label}</option>{/each}
    </select>
  </label>
  <!-- De ritmeschuif staat sinds 0.7.93 weer in de kopbalk (NotationHeader). -->
  <span class="notation-spacer"></span>
  <button class="btn btn-ghost btn-sm" on:click={() => doe('addLayer')} title={$t('notation.new_staff_title')}>{$t('notation.add_staff')}</button>
  <button class="btn btn-ghost btn-sm" on:click={() => doe('openWizard')} title={$t('notation.layout_title')}>{$t('notation.layout')}</button>
</div>
<div class="balken">
  {#each layers as layer (layer.id)}
    <div class="balk">
      <span class="naam">{layer.name}</span>
      <span class="divs" title={$t('notation.layer_divisions_title')}>
        {#each divisions as div}
          <label class="div">
            <input type="checkbox" checked={(layer.divisions || []).includes(div)} on:change={() => doe('toggleLayerDivision', layer, div)} />
            {div}
          </label>
        {/each}
        {#if divisions.length === 0}<span class="hint">{$t('notation.no_organ_routing_later')}</span>{/if}
      </span>
      {#if viewMode === 'klavar'}
        <select class="layer-hand" value={layerHandKeuze(layer)} on:change={(e) => doe('setLayerHand', layer, e.currentTarget.value)} title={$t('notation.hand_title')}>
          <option value="auto">{$t('notation.hand')}: {$t('notation.hand_auto')} ({autoHandLabel(layer, klavarModel, $t)})</option>
          <option value="right">{$t('notation.hand_right')}</option>
          <option value="left">{$t('notation.hand_left')}</option>
          <option value="pedal">{$t('notation.hand_pedal')}</option>
          <option value="rl">{$t('notation.hand_split')}</option>
        </select>
        {#if layerHandKeuze(layer) === 'rl'}
          <select class="layer-split" value={layer.klavar_split ?? 60} on:change={(e) => doe('setLayerHand', layer, 'rl', Number(e.currentTarget.value))} title={$t('notation.hand_split_title')}>
            {#each splitOpties as [m, naam]}<option value={m}>{naam}</option>{/each}
          </select>
        {/if}
      {/if}
    </div>
  {/each}
</div>
<div class="tab-rij stemmen-rij">
  <span class="kopje">{$t('notation.voices')}:</span>
  <span class="kopje">{$t('notation.to_voice')}</span>
  {#each [1, 2, 3, 4] as v}
    <button class="btn btn-ghost btn-sm" on:click={() => doe('toVoice', v)} title={$t('notation.to_voice_title').replace('{n}', v)}>{v}</button>
  {/each}
  <button class="btn btn-ghost btn-sm" on:click={() => doe('splitChord')} title={$t('notation.split_chord_title')}>{$t('notation.split_chord')}</button>
  <label class="schakelaar" title={$t('notation.only_active_voice_title')}>
    <input type="checkbox" checked={onlyActive} on:change={() => doe('toggleOnlyActive')} />
    {$t('notation.only_active_voice')}
  </label>
  <span class="hint">{$t('notation.voices_hint')}</span>
</div>

<style>
  .tab-rij { display: flex; align-items: center; flex-wrap: wrap; gap: 0.6rem; min-width: 0; }
  .tab-rij label { display: flex; align-items: center; gap: 0.3rem; font-size: 0.8rem; white-space: nowrap; }
  .notation-spacer { flex: 1; }
  .tolerance-slider input[type="range"] { width: 8rem; }
  .tolerance-value { font-size: 0.72rem; color: var(--text-muted, #aaa); min-width: 3rem; text-align: center; }
  .balken { display: flex; flex-wrap: wrap; gap: 0.4rem; margin-top: 0.4rem; align-items: center; }
  .balk {
    display: flex; align-items: center; flex-wrap: wrap; gap: 0.4rem;
    padding: 0.2rem 0.5rem; border: 1px solid var(--text-muted, #555); border-radius: 6px;
    background: var(--bg-panel, #2a2a2a); font-size: 0.78rem;
  }
  .naam { font-weight: 600; }
  .divs { display: flex; align-items: center; flex-wrap: wrap; gap: 0.4rem; }
  .div { display: flex; align-items: center; gap: 0.2rem; white-space: nowrap; cursor: pointer; }
  .hint { font-size: 0.72rem; color: var(--text-muted, #aaa); }
  .stemmen-rij { margin-top: 0.4rem; }
  .kopje { font-size: 0.75rem; color: var(--text-muted, #aaa); }
  .schakelaar { cursor: pointer; }
  .layer-hand, .layer-split { font-size: 0.75rem; }
</style>
