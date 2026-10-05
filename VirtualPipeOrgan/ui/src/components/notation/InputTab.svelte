<script>
  // Tabblad Invoer (0.7.85): stapinvoer aan/uit, het palet en de
  // bewerkknoppen voor de selectie.
  import { t } from '../../lib/i18n.js';
  import NotePalette from './NotePalette.svelte';
  export let stepMode = false;
  export let stepQuarters = 1;
  export let stepDotted = false;
  export let acc = null;
  export let tie = false;
  export let kanHerhaal = false;
  export let kanAlterLast = false;
  export let viewMode = 'staff';
  export let selectieAlleenPedaal = false;
  export let clipboardCount = 0;
  export let acties = {};
  function doe(naam, ...args) { const f = acties[naam]; if (typeof f === 'function') f(...args); }
</script>

<div class="tab-rij">
  <label class="schakelaar" title={$t('notation.step_input_title')}>
    <input type="checkbox" checked={stepMode} on:change={(e) => doe('setStepMode', e.currentTarget.checked)} />
    <span>{$t('notation.step_input')} <kbd>N</kbd></span>
  </label>
  <NotePalette {stepQuarters} {stepDotted} {acc} {tie} {kanHerhaal} {kanAlterLast} {stepMode} {acties} />
</div>
<div class="tab-rij">
  <span class="kopje">{$t('notation.selection_section')}</span>
  <button class="btn btn-ghost btn-sm" on:click={() => doe('moveCursor', -1)} title={$t('notation.prev_note_title')}>◄</button>
  <button class="btn btn-ghost btn-sm" on:click={() => doe('moveCursor', +1)} title={$t('notation.next_note_title')}>►</button>
  <button class="btn btn-ghost btn-sm" on:click={() => doe('deleteSelection')} title={$t('notation.delete_selection_title')}>{$t('actions.delete')}</button>
  <button class="btn btn-ghost btn-sm" on:click={() => doe('transpose', -1)} title={$t('notation.semitone_down_title')}>−½</button>
  <button class="btn btn-ghost btn-sm" on:click={() => doe('transpose', +1)} title={$t('notation.semitone_up_title')}>+½</button>
  <button class="btn btn-ghost btn-sm" on:click={() => doe('transpose', -12)} title={$t('notation.octave_down_title')}>−8va</button>
  <button class="btn btn-ghost btn-sm" on:click={() => doe('transpose', +12)} title={$t('notation.octave_up_title')}>+8va</button>
  {#if viewMode === 'klavar'}
    <button class="btn btn-ghost btn-sm" on:click={() => doe('setHands', 'left')} disabled={selectieAlleenPedaal} title={$t('notation.to_left_hand_title')}>{$t('notation.to_left_hand')}</button>
    <button class="btn btn-ghost btn-sm" on:click={() => doe('setHands', 'right')} disabled={selectieAlleenPedaal} title={$t('notation.to_right_hand_title')}>{$t('notation.to_right_hand')}</button>
  {/if}
  <button class="btn btn-ghost btn-sm" on:click={() => doe('changeDuration', 'halve')} title={$t('notation.halve_duration_title')}>÷2</button>
  <button class="btn btn-ghost btn-sm" on:click={() => doe('changeDuration', 'double')} title={$t('notation.double_duration_title')}>×2</button>
  <button class="btn btn-ghost btn-sm" on:click={() => doe('changeDuration', 'dot')} title={$t('notation.dot_toggle_title')}>• {$t('notation.dot')}</button>
  <button class="btn btn-ghost btn-sm" on:click={() => doe('copy')} title={$t('notation.copy_title')}>{$t('notation.copy')}</button>
  <button class="btn btn-ghost btn-sm" on:click={() => doe('paste')} disabled={clipboardCount === 0} title={$t('notation.paste_title')}>{$t('notation.paste')}{clipboardCount ? ` (${clipboardCount})` : ''}</button>
</div>

<style>
  .tab-rij { display: flex; align-items: center; flex-wrap: wrap; gap: 0.4rem; min-width: 0; }
  .tab-rij + .tab-rij { margin-top: 0.35rem; }
  .schakelaar { display: flex; align-items: center; gap: 0.3rem; font-size: 0.85rem; white-space: nowrap; cursor: pointer; margin-right: 0.4rem; }
  .schakelaar kbd { font-size: 0.7rem; color: var(--text-muted, #aaa); font-family: inherit; }
  .kopje { font-size: 0.75rem; color: var(--text-muted, #aaa); margin-right: 0.2rem; }
</style>
