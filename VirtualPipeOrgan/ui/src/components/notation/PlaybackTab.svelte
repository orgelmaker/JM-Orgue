<script>
  // Tabblad Afspelen (0.7.85): metronoom, count-in, tempotest en het
  // klavar-bereik. Opnemen/afspelen zelf staan op de kopbalk.
  import { t } from '../../lib/i18n.js';
  export let metroOn = false;
  export let countInBeats = 0;
  export let viewMode = 'staff';
  export let klavarBereik = 'auto';
  export let acties = {};
  function doe(naam, ...args) { const f = acties[naam]; if (typeof f === 'function') f(...args); }
</script>

<div class="tab-rij">
  <label class="schakelaar" title={$t('notation.metronome_title')}>
    <input type="checkbox" checked={metroOn} on:change={(e) => doe('setMetronome', e.currentTarget.checked, countInBeats)} />
    {$t('notation.metronome')}
  </label>
  <label title={$t('notation.count_in_title')}>{$t('notation.count_in')}
    <select value={countInBeats} on:change={(e) => doe('setMetronome', metroOn, Number(e.currentTarget.value))}>
      <option value={0}>{$t('notation.count_in_off')}</option>
      <option value={1}>1</option>
      <option value={2}>2</option>
      <option value={4}>4</option>
    </select>
  </label>
  <button class="btn btn-ghost btn-sm" on:click={() => doe('previewTempo')} title={$t('notation.tempo_test_title')}>♪ {$t('notation.tempo_test')}</button>
  {#if viewMode === 'klavar'}
    <label title={$t('notation.klavar_range_title')}>{$t('notation.klavar_range')}
      <select value={klavarBereik} on:change={(e) => doe('setKlavarBereik', e.currentTarget.value)}>
        <option value="auto">{$t('notation.klavar_range_auto')}</option>
        <option value="klavier">{$t('notation.klavar_range_keyboard')}</option>
      </select>
    </label>
  {/if}
</div>

<style>
  .tab-rij { display: flex; align-items: center; flex-wrap: wrap; gap: 0.6rem; min-width: 0; }
  .tab-rij label { display: flex; align-items: center; gap: 0.3rem; font-size: 0.8rem; white-space: nowrap; }
  .schakelaar { cursor: pointer; }
</style>
