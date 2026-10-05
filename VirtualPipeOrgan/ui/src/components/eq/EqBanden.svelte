<script>
  // Bandenlijst van een equalizer (0.7.77): gedeeld door "Klankkleur van dit
  // orgel" en de uitgangscorrectie per uitvoerprofiel. De component muteert de
  // bandobjecten in `bands` en meldt dat met `change`; de eigenaar doet dan
  // zelf `bands = bands` en stuurt alles naar de backend. Voorversterking en
  // eenheid komen als props binnen en gaan als events (`preamp`, `unit`) terug.
  import { createEventDispatcher } from 'svelte';
  import { t } from '../../lib/i18n.js';

  export let bands = [];
  export let channelCount = 2;
  export let unit = 'oct';            // 'oct' (bandbreedte) | 'q'
  export let preampDb = 0;
  export let preampAuto = true;
  export let preampEffectief = 0;
  export let preampAutoWaarde = 0;

  const dispatch = createEventDispatcher();

  // Q ↔ bandbreedte (analoge RBJ-benadering; identiek aan vpo_audio::q_naar_bandbreedte).
  const qToBw = (q) => (2 / Math.LN2) * Math.asinh(1 / (2 * Math.max(0.05, q)));
  const bwToQ = (bw) => 1 / (2 * Math.sinh(0.5 * Math.LN2 * Math.max(0.01, bw)));
  const bandQ = (b) => (b.q != null ? Number(b.q) : bwToQ(Number(b.bandwidth) || 1));
  const isShelf = (b) => b.band_type === 'lowshelf' || b.band_type === 'highshelf';
  const heeftGain = (b) => b.band_type === 'peak' || isShelf(b);

  // Log-schaal voor de frequentie-slider: 0..300 ↔ 20 Hz .. 20 kHz.
  const freqToSlider = (f) => Math.round(100 * Math.log10(Math.max(20, Math.min(20000, f)) / 20));
  const sliderToFreq = (v) => Math.round(20 * Math.pow(10, v / 100));

  $: bandTypes = [
    { value: 'peak', label: $t('eq.band_peak') },
    { value: 'lowpass', label: $t('eq.band_lowpass') },
    { value: 'highpass', label: $t('eq.band_highpass') },
    { value: 'bandpass', label: $t('eq.band_bandpass') },
    { value: 'lowshelf', label: $t('eq.band_lowshelf') },
    { value: 'highshelf', label: $t('eq.band_highshelf') },
  ];

  const gewijzigd = () => dispatch('change');

  function setBandQ(band, q) {
    // Shelves: 0,3–2 (daarbuiten resoneert de kantel); overige: 0,05–20.
    const lo = isShelf(band) ? 0.3 : 0.05;
    const hi = isShelf(band) ? 2 : 20;
    const qq = Math.max(lo, Math.min(hi, Number(q) || 0.7));
    band.q = qq;
    band.bandwidth = qToBw(qq);
    gewijzigd();
  }
  function setBandBw(band, bw) {
    const b = Math.max(0.05, Math.min(8, Number(bw) || 1));
    band.bandwidth = b;
    band.q = bwToQ(b);
    gewijzigd();
  }
  function typeGewijzigd(band, value) {
    band.band_type = value;
    if (isShelf(band) && band.q != null) setBandQ(band, band.q);
    else gewijzigd();
  }
  function setPreampDb(v) {
    const db = Math.max(-24, Math.min(6, Number(v) || 0));
    dispatch('preamp', { db, auto: false });
    return db;
  }
  $: preampToon = preampAuto ? preampEffectief : preampDb;
  $: preampWaarschuwing = !preampAuto && preampDb > preampAutoWaarde + 0.05 && bands.some((b) => b.enabled && b.gain_db > 0);
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

{#each bands as band, bi (bi)}
  <div style="border:1px solid var(--accent-soft-2); border-radius:var(--radius-sm); padding:0.4rem 0.5rem; opacity:{band.enabled ? 1 : 0.55};">
    <div style="display:flex; align-items:center; gap:0.45rem; flex-wrap:wrap; margin-bottom:0.35rem;">
      <label class="swell-toggle" style="margin:0;" title={$t('eq.band_toggle_title')}>
        <input type="checkbox" checked={band.enabled} on:change={(e) => { band.enabled = e.target.checked; gewijzigd(); }} />
        <span class="swell-toggle-label">{$t('eq.band_n').replace('{n}', bi + 1)}</span>
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
        on:click={() => dispatch('remove', bi)}
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
{/each}
<div style="display:flex; gap:0.4rem;">
  <button class="btn btn-secondary btn-sm" on:click={() => dispatch('add')}>{$t('eq.band_add')}</button>
  <span style="font-size:0.7rem; color:var(--text-muted); align-self:center;">
    {$t('eq.channel_note')}
  </span>
</div>
