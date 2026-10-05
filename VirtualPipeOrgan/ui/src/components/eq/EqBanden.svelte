<script>
  // Equalizerpaneel (0.7.77, grafiek sinds 0.7.79): voorversterking, de
  // grafiek met sleepbare punten, een strook met bandknoppen en één
  // detailkaart voor de gekozen band. Gedeeld door "Klankkleur van dit orgel"
  // en de uitgangscorrectie per uitvoerprofiel. De component muteert de
  // bandobjecten in `bands` en meldt dat met `change` (detail { live }:
  // true = tijdens slepen, alleen naar de audio; false = definitief);
  // de eigenaar doet dan zelf `bands = bands` en stuurt alles naar de
  // backend. Voorversterking en eenheid komen als props binnen en gaan als
  // events (`preamp`, `unit`) terug; `add` draagt optioneel { freq, gain_db }.
  import { createEventDispatcher } from 'svelte';
  import { t } from '../../lib/i18n.js';
  import EqGrafiek from './EqGrafiek.svelte';
  import { qToBw, bwToQ, EQ_MAX_BANDEN } from '../../lib/eqCurve.js';

  export let bands = [];
  export let channelCount = 2;
  export let unit = 'oct';            // 'oct' (bandbreedte) | 'q'
  export let preampDb = 0;
  export let preampAuto = true;
  export let preampEffectief = 0;
  export let preampAutoWaarde = 0;
  export let sampleRate = 48000;
  export let enabled = true;

  const dispatch = createEventDispatcher();

  let selected = 0;
  let channelView = null;
  $: if (bands && selected >= bands.length) selected = Math.max(0, bands.length - 1);
  $: band = bands && bands.length ? bands[selected] : null;
  $: heeftKanaalbanden = (bands || []).some((b) => b.channel != null);
  $: if (!heeftKanaalbanden && channelView != null) channelView = null;

  const bandQ = (b) => (b.q != null ? Number(b.q) : bwToQ(Number(b.bandwidth) || 1));
  const isShelf = (b) => b.band_type === 'lowshelf' || b.band_type === 'highshelf';
  const heeftGain = (b) => b.band_type === 'peak' || isShelf(b);
  const freqToSlider = (f) => Math.round(100 * Math.log10(Math.max(20, Math.min(20000, f)) / 20));
  const sliderToFreq = (v) => Math.round(20 * Math.pow(10, v / 100));
  const fmtF = (f) => (f >= 1000 ? `${(f / 1000).toFixed(f >= 10000 ? 0 : 1)}k` : `${Math.round(f)}`);

  $: bandTypes = [
    { value: 'peak', label: $t('eq.band_peak') },
    { value: 'lowpass', label: $t('eq.band_lowpass') },
    { value: 'highpass', label: $t('eq.band_highpass') },
    { value: 'bandpass', label: $t('eq.band_bandpass') },
    { value: 'lowshelf', label: $t('eq.band_lowshelf') },
    { value: 'highshelf', label: $t('eq.band_highshelf') },
  ];

  const gewijzigd = (live = false) => { bands = bands; dispatch('change', { live }); };

  function setBandQ(b, q) {
    // Shelves: 0,3–2 (daarbuiten resoneert de kantel); overige: 0,05–20.
    const lo = isShelf(b) ? 0.3 : 0.05;
    const hi = isShelf(b) ? 2 : 20;
    const qq = Math.max(lo, Math.min(hi, Number(q) || 0.7));
    b.q = qq;
    b.bandwidth = qToBw(qq);
    gewijzigd();
  }
  function setBandBw(b, bw) {
    const v = Math.max(0.05, Math.min(8, Number(bw) || 1));
    b.bandwidth = v;
    b.q = bwToQ(v);
    gewijzigd();
  }
  function typeGewijzigd(b, value) {
    b.band_type = value;
    if (isShelf(b) && b.q != null) setBandQ(b, b.q);
    else gewijzigd();
  }
  function setPreampDb(v) {
    const db = Math.max(-24, Math.min(6, Number(v) || 0));
    dispatch('preamp', { db, auto: false });
    return db;
  }
  function kies(i) { selected = i; }
  function voegToe(detail) {
    if ((bands || []).length >= EQ_MAX_BANDEN) return;
    dispatch('add', detail || null);
    // De nieuwe band komt achteraan; selecteer hem zodra de eigenaar hem heeft toegevoegd.
    selected = (bands || []).length;
  }
  $: preampToon = preampAuto ? preampEffectief : preampDb;
  $: preampWaarschuwing = !preampAuto && preampDb > preampAutoWaarde + 0.05 && (bands || []).some((b) => b.enabled && b.gain_db > 0);
</script>

<!-- Voorversterking: Auto = −(grootste opgetelde versterking). -->
<div class="swell-config-sliders" style="border:1px solid var(--accent-soft-2); border-radius:var(--radius-sm); padding:0.4rem 0.5rem;">
  <div class="swell-config-row">
    <span class="swell-config-label">{$t('eq.preamp')}</span>
    <input type="range" min="-24" max="6" step="0.1" value={preampToon}
      disabled={preampAuto} style="opacity:{preampAuto ? 0.55 : 1};"
      on:input={(e) => setPreampDb(e.target.value)} />
    <input type="number" min="-24" max="6" step="0.1"
      style="width:4.2rem; font-size:0.75rem; padding:0.1rem 0.25rem; background:var(--bg-elevated); border:1px solid var(--accent-soft-2); border-radius:var(--radius-sm); color:{preampAuto ? 'var(--text-muted)' : 'var(--text)'};"
      value={Number(preampToon).toFixed(1)} disabled={preampAuto}
      on:change={(e) => { const db = setPreampDb(e.target.value); e.target.value = db.toFixed(1); }} />
    <span class="swell-config-value" style="width:1.4rem;">dB</span>
    <label class="swell-toggle" style="margin:0 0 0 0.4rem;" title={$t('eq.preamp_auto_title')}>
      <!-- Uitvinken geeft de getoonde (Auto-)waarde mee, zodat de eigenaar die als
           handmatige startwaarde houdt (reviewbevinding: anders sprong hij naar 0 dB). -->
      <input type="checkbox" checked={preampAuto} on:change={(e) => dispatch('preamp', { db: preampToon, auto: e.target.checked })} />
      <span class="swell-toggle-label">{$t('eq.preamp_auto')}</span>
    </label>
    <span class="eq-unit-switch" style="margin-left:auto; display:inline-flex; gap:0.15rem;" title={$t('eq.unit_title')}>
      <button class="btn btn-ghost btn-sm" class:active={unit === 'oct'} style="font-size:0.7rem; padding:0.05rem 0.4rem;" on:click={() => dispatch('unit', 'oct')}>{$t('eq.unit_oct')}</button>
      <button class="btn btn-ghost btn-sm" class:active={unit === 'q'} style="font-size:0.7rem; padding:0.05rem 0.4rem;" on:click={() => dispatch('unit', 'q')}>{$t('eq.unit_q')}</button>
    </span>
  </div>
  {#if preampWaarschuwing}
    <div class="settings-hint" style="color:var(--warning, #b8860b);">{$t('eq.preamp_warn')}</div>
  {/if}
</div>

<!-- Grafiek (0.7.79) -->
<EqGrafiek {bands} {selected} {sampleRate} {unit} {enabled} {channelView}
  preampDb={preampAuto ? preampEffectief : preampDb}
  on:select={(e) => kies(e.detail)}
  on:change={(e) => gewijzigd(!!e.detail?.live)}
  on:add={(e) => voegToe(e.detail)}
  on:remove={(e) => dispatch('remove', e.detail)} />
<div style="display:flex; align-items:center; gap:0.5rem; flex-wrap:wrap;">
  <span style="margin:0; flex:1; min-width:12rem; font-size:0.68rem; color:var(--text-muted); line-height:1.35;">{$t('eq.graph_help')}</span>
  {#if heeftKanaalbanden}
    <label style="display:inline-flex; align-items:center; gap:0.3rem; font-size:0.72rem; color:var(--text-muted);">
      {$t('eq.view_channel')}
      <select style="font-size:0.72rem; padding:0.1rem 0.3rem; background:var(--bg-elevated); border:1px solid var(--accent-soft-2); border-radius:var(--radius-sm); color:var(--text);"
        value={channelView == null ? 'all' : String(channelView)}
        on:change={(e) => { channelView = e.target.value === 'all' ? null : parseInt(e.target.value, 10); }}>
        <option value="all">{$t('eq.view_all')}</option>
        {#each Array(Math.max(2, channelCount)) as _, ch}
          <option value={String(ch)}>{$t('settings.channel_n').replace('{n}', ch + 1)}</option>
        {/each}
      </select>
    </label>
  {/if}
</div>

<!-- Bandstrook -->
<div class="eq-chips">
  {#each bands as b, i (i)}
    <button class="eq-chip" class:gekozen={i === selected} class:bandUit={!b.enabled} on:click={() => kies(i)} title={$t('eq.band_n').replace('{n}', i + 1)}>
      <span class="eq-chip-nr">{i + 1}</span>
      <span class="eq-chip-f">{fmtF(Number(b.freq) || 0)}</span>
    </button>
  {/each}
  <button class="eq-chip eq-chip-plus" on:click={() => voegToe(null)} disabled={(bands || []).length >= EQ_MAX_BANDEN}
    title={(bands || []).length >= EQ_MAX_BANDEN ? $t('eq.max_bands') : $t('eq.band_add')}>+</button>
</div>

<!-- Detailkaart van de gekozen band -->
{#if band}
  <div style="border:1px solid var(--accent-soft-2); border-radius:var(--radius-sm); padding:0.4rem 0.5rem; opacity:{band.enabled ? 1 : 0.55};">
    <div style="display:flex; align-items:center; gap:0.45rem; flex-wrap:wrap; margin-bottom:0.35rem;">
      <label class="swell-toggle" style="margin:0;" title={$t('eq.band_toggle_title')}>
        <input type="checkbox" checked={band.enabled} on:change={(e) => { band.enabled = e.target.checked; gewijzigd(); }} />
        <span class="swell-toggle-label">{$t('eq.band_n').replace('{n}', selected + 1)}</span>
      </label>
      <select
        style="font-size:0.75rem; padding:0.15rem 0.3rem; background:var(--bg-elevated); border:1px solid var(--accent-soft-2); border-radius:var(--radius-sm); color:var(--text);"
        value={band.band_type}
        on:change={(e) => typeGewijzigd(band, e.target.value)}
        title={$t('eq.filter_type')}
      >
        {#each bandTypes as bt}
          <option value={bt.value}>{bt.label}</option>
        {/each}
      </select>
      <select
        style="font-size:0.75rem; padding:0.15rem 0.3rem; background:var(--bg-elevated); border:1px solid var(--accent-soft-2); border-radius:var(--radius-sm); color:var(--text);"
        value={band.channel === null || band.channel === undefined ? 'all' : String(band.channel)}
        on:change={(e) => { band.channel = e.target.value === 'all' ? null : parseInt(e.target.value, 10); gewijzigd(); }}
        title={$t('settings.eq_channel_hint')}
      >
        <option value="all">{$t('settings.eq_all_channels')}</option>
        {#each Array(Math.max(2, channelCount)) as _, ch}
          <option value={String(ch)}>{$t('settings.channel_n').replace('{n}', ch + 1)}</option>
        {/each}
      </select>
      <button
        class="btn btn-ghost btn-sm"
        style="margin-left:auto; font-size:0.75rem; padding:0.1rem 0.45rem;"
        on:click={() => dispatch('remove', selected)}
        title={$t('eq.band_remove')}
      >✕</button>
    </div>
    <div class="swell-config-sliders">
      <div class="swell-config-row">
        <span class="swell-config-label">{$t('eq.frequency')}</span>
        <input type="range" min="0" max="300" step="1"
          value={freqToSlider(band.freq)}
          on:input={(e) => { band.freq = sliderToFreq(parseInt(e.target.value, 10)); gewijzigd(); }}
        />
        <input type="number" min="20" max="20000" step="1"
          style="width:4.6rem; font-size:0.75rem; padding:0.1rem 0.25rem; background:var(--bg-elevated); border:1px solid var(--accent-soft-2); border-radius:var(--radius-sm); color:var(--text);"
          value={Math.round(band.freq)}
          on:change={(e) => { band.freq = Math.max(20, Math.min(20000, parseFloat(e.target.value) || 1000)); e.target.value = Math.round(band.freq); gewijzigd(); }}
        />
        <span class="swell-config-value" style="width:1.4rem;">Hz</span>
      </div>
      {#if heeftGain(band)}
        <div class="swell-config-row">
          <span class="swell-config-label">{$t('eq.gain')}</span>
          <input type="range" min="-24" max="24" step="0.1" value={band.gain_db} on:input={(e) => { band.gain_db = Number(e.target.value); gewijzigd(); }} />
          <span class="swell-config-value">{band.gain_db > 0 ? '+' : ''}{Number(band.gain_db).toFixed(1)} dB</span>
        </div>
      {/if}
      {#if isShelf(band) && band.q == null}
        <div class="swell-config-row">
          <span class="swell-config-label">{$t('eq.shelf_q')}</span>
          <button class="btn btn-ghost btn-sm" style="font-size:0.72rem; padding:0.05rem 0.45rem;" on:click={() => setBandQ(band, 0.7071)} title={$t('eq.fixed_slope_title')}>{$t('eq.fixed_slope')}</button>
        </div>
      {:else if unit === 'q' || isShelf(band)}
        <div class="swell-config-row">
          <span class="swell-config-label">{isShelf(band) ? $t('eq.shelf_q') : $t('eq.unit_q')}</span>
          <input type="range" min={isShelf(band) ? 0.3 : 0.1} max={isShelf(band) ? 2 : 10} step="0.01" value={bandQ(band)} on:input={(e) => setBandQ(band, e.target.value)} />
          <input type="number" min="0.05" max="20" step="0.01"
            style="width:4.2rem; font-size:0.75rem; padding:0.1rem 0.25rem; background:var(--bg-elevated); border:1px solid var(--accent-soft-2); border-radius:var(--radius-sm); color:var(--text);"
            value={bandQ(band).toFixed(2)} on:change={(e) => { setBandQ(band, e.target.value); e.target.value = bandQ(band).toFixed(2); }} />
          <span class="swell-config-value" style="width:1.4rem;">Q</span>
        </div>
      {:else}
        <div class="swell-config-row">
          <span class="swell-config-label">{$t('eq.bandwidth')}</span>
          <input type="range" min="0.1" max="4" step="0.05" value={Number(band.bandwidth)} on:input={(e) => setBandBw(band, e.target.value)} />
          <span class="swell-config-value">{Number(band.bandwidth).toFixed(2)} oct</span>
        </div>
      {/if}
    </div>
  </div>
{/if}
<span style="font-size:0.7rem; color:var(--text-muted);">{$t('eq.channel_note')}</span>

<style>
  .eq-chips { display: flex; gap: 0.3rem; overflow-x: auto; padding: 0.1rem 0; }
  .eq-chip {
    flex: 0 0 auto; min-width: 2.2rem; min-height: 2.2rem; padding: 0.15rem 0.35rem;
    display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 0;
    border: 1px solid var(--accent-soft-2); border-radius: var(--radius-sm); background: var(--bg-elevated); color: var(--text);
    font: inherit; font-size: 0.72rem; line-height: 1.1; cursor: pointer;
  }
  .eq-chip.gekozen { background: var(--primary); color: var(--bg-darkest); border-color: var(--gold-border); }
  .eq-chip.bandUit { border-style: dashed; opacity: 0.6; }
  .eq-chip:disabled { opacity: 0.4; cursor: default; }
  .eq-chip-nr { font-weight: 700; }
  .eq-chip-f { font-size: 0.62rem; opacity: 0.8; }
  .eq-chip-plus { font-size: 1rem; font-weight: 700; min-width: 2rem; }
  @media (pointer: coarse) {
    .eq-chip { min-width: 2.75rem; min-height: 2.75rem; }
  }
</style>
