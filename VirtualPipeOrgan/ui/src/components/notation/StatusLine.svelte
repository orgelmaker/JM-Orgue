<script>
  // Statusregel (0.7.85): aftellen, selectie, en in stapinvoer de plaats
  // van de invoercursor (maat:tel, toon, balk).
  import { t } from '../../lib/i18n.js';
  export let armedWaiting = false;
  export let countInRemaining = 0;
  export let aantalNoten = 0;
  export let selectieAantal = 0;
  export let cursorNaam = '';
  export let cursorIndex = 0;
  export let cursorLaag = '';
  export let stepMode = false;
  export let caretTekst = '';
  export let debugTekst = '';
</script>

<div class="status" class:counting={armedWaiting}>
  {#if armedWaiting}
    <span>{$t('notation.count_in_prefix')} <b>{countInRemaining}</b> {countInRemaining === 1 ? $t('notation.count_in_suffix_one') : $t('notation.count_in_suffix_many')}</span>
  {:else if stepMode && caretTekst}
    <span><b>{$t('notation.caret_label')}</b> {caretTekst}</span>
  {:else if aantalNoten > 0}
    {#if selectieAantal > 1}
      <span><b>{selectieAantal}</b> {$t('notation.selected_many')}</span>
    {:else if cursorNaam}
      <span>{$t('notation.selection_label')} <b>{cursorNaam}</b> {$t('notation.selection_detail').replace('{i}', cursorIndex + 1).replace('{total}', aantalNoten).replace('{layer}', cursorLaag)}</span>
    {:else}
      <span>{$t('notation.click_to_select')}</span>
    {/if}
  {:else}
    <span class="leeg">{$t('notation.empty_score')}</span>
  {/if}
  {#if debugTekst}<span class="debug">{debugTekst}</span>{/if}
</div>

<style>
  .status {
    display: flex; align-items: center; gap: 0.6rem;
    padding: 0.3rem 0.75rem; min-width: 0;
    background: #f4ecd4; color: #6b5b1e;
    font-size: 0.78rem; border-bottom: 1px solid #ddd; flex-shrink: 0;
  }
  .status.counting { background: #7a2020; color: #fff; font-weight: 600; }
  .leeg { color: #999; font-style: italic; }
  .debug { margin-left: auto; font-size: 0.72rem; opacity: 0.7; }
  @media print { .status { display: none !important; } }
</style>
