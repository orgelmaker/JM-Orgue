<script>
  // Invoerpalet (0.7.85): notenwaarden (toetsen 7..2), punt, rust, herhaal,
  // kleverig voorteken (♯ ♭ ♮), kleverige overbinding en gum. Altijd
  // zichtbaar op het tabblad Invoer; knoppen ≥ 40 px voor aanraking.
  import { t } from '../../lib/i18n.js';
  export let stepQuarters = 1;
  export let stepDotted = false;
  export let stepTriplet = false;
  export let trioolMogelijk = true;
  export let acc = null;        // 1 | -1 | 'nat' | null
  export let tie = false;
  export let kanHerhaal = false;
  export let kanAlterLast = false;
  export let gum = false;
  export let stepMode = false;
  export let acties = {};
  const WAARDEN = [[4, '𝅝', '7'], [2, '𝅗𝅥', '6'], [1, '♩', '5'], [0.5, '♪', '4'], [0.25, '𝅘𝅥𝅯', '3'], [0.125, '𝅘𝅥𝅰', '2']];
  function doe(naam, ...args) { const f = acties[naam]; if (typeof f === 'function') f(...args); }
</script>

<div class="palet" role="group" aria-label={$t('notation.palette_title')}>
  {#each WAARDEN as [q, sym, key]}
    <button class="pk waarde" class:active={stepQuarters === q} on:click={() => doe('setQuarters', q)}
      title={$t('notation.note_value_key').replace('{key}', key)}>{sym}</button>
  {/each}
  <button class="pk" class:active={stepDotted} on:click={() => doe('toggleDot')} title={$t('notation.dotted_title')}>•</button>
  <button class="pk" class:active={stepTriplet} disabled={!trioolMogelijk} on:click={() => doe('toggleTriplet')}
    title={trioolMogelijk ? $t('notation.triplet_title') : $t('notation.triplet_unavailable')}>3</button>
  <span class="scheiding"></span>
  <button class="pk" on:click={() => doe('rest')} disabled={!stepMode} title={$t('notation.rest_title')}>𝄽</button>
  <button class="pk" on:click={() => doe('repeat')} disabled={!stepMode || !kanHerhaal} title={$t('notation.repeat_last_title')}>R</button>
  <button class="pk laatste" on:click={() => doe('alterLast', 1)} disabled={!stepMode || !kanAlterLast} title={$t('notation.step_sharp_title')}>♯<small>←</small></button>
  <button class="pk laatste" on:click={() => doe('alterLast', -1)} disabled={!stepMode || !kanAlterLast} title={$t('notation.step_flat_title')}>♭<small>←</small></button>
  <span class="scheiding"></span>
  <button class="pk" class:active={acc === 1} on:click={() => doe('setAcc', acc === 1 ? null : 1)} title={$t('notation.acc_sharp_title')}>♯</button>
  <button class="pk" class:active={acc === -1} on:click={() => doe('setAcc', acc === -1 ? null : -1)} title={$t('notation.acc_flat_title')}>♭</button>
  <button class="pk" class:active={acc === 'nat'} on:click={() => doe('setAcc', acc === 'nat' ? null : 'nat')} title={$t('notation.acc_natural_title')}>♮</button>
  <button class="pk" class:active={tie} on:click={() => doe('toggleTie')} title={$t('notation.tie_title')}>{$t('notation.tie')}</button>
  <span class="scheiding"></span>
  <button class="pk gum" class:active={gum} on:click={() => doe('gum')} title={$t('notation.eraser_title')}>{$t('notation.eraser')}</button>
</div>

<style>
  .palet { display: flex; align-items: center; flex-wrap: wrap; gap: 0.25rem; }
  .pk {
    min-width: 2.5rem; min-height: 2.5rem; padding: 0 0.5rem;
    border: 1px solid var(--text-muted, #555); border-radius: 6px;
    background: var(--bg-panel, #2a2a2a); color: var(--text, #eee);
    font-size: 1.1rem; line-height: 1; cursor: pointer;
  }
  .pk.waarde { font-size: 1.35rem; }
  .pk:hover:not(:disabled) { border-color: var(--gold-border, #d4af37); }
  .pk.active { background: var(--accent, #6a8); color: #fff; border-color: var(--accent, #6a8); }
  .pk:disabled { opacity: 0.35; cursor: default; }
  .gum { font-size: 0.85rem; }
  .laatste small { font-size: 0.6rem; margin-left: 0.1rem; }
  .scheiding { width: 1px; height: 1.8rem; background: var(--text-muted, #555); margin: 0 0.2rem; }
</style>
