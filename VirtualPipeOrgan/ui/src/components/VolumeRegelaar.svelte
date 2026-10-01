<script>
  // Hoofdvolume in de setzerbalk (0.7.65). Bedient hetzelfde volume als de
  // schuif in Orgel-instellingen > Master en die op de afstandsbediening
  // (master_volume_db, per orgel, -40..+6 dB). De ouder (Console) stuurt het
  // naar de backend; die meldt het aan alle vensters.
  //
  // compact: past de volle schuif niet in de balk (smal scherm, aanraakscherm,
  // veel crescendotrappen), dan een knop "-6 dB" die de schuif erboven opent.
  // Dat beslist SetzerBar door te METEN, niet door te rekenen.
  import { createEventDispatcher, onDestroy } from 'svelte';
  import { t } from '../lib/i18n.js';

  export let volume = -6;
  export let compact = false;

  const dispatch = createEventDispatcher();
  const MIN = -40;
  const MAX = 6;
  const STANDAARD = -6;

  let waarde = volume;
  let slepen = false;
  // Een waarde van buiten (ander venster, afstandsbediening, laden van een
  // orgel) volgen, behalve tijdens het slepen: anders springt de duim terug
  // naar een waarde die net al achterhaald is.
  $: if (!slepen) waarde = volume;

  function zet(db) {
    db = Math.max(MIN, Math.min(MAX, Math.round(db)));
    if (db === waarde) return;
    waarde = db;
    dispatch('change', db);
  }

  function opInvoer() {
    dispatch('change', Number(waarde));
  }

  // Muiswiel: 1 dB per klik. Optellen tot een hele klik (deltaY ~100), zodat
  // een trackpad met kleine stapjes niet in één veeg naar het uiterste schiet.
  let wielRest = 0;
  function opWiel(e) {
    wielRest += e.deltaY;
    while (wielRest <= -100) { wielRest += 100; zet(waarde + 1); }
    while (wielRest >= 100) { wielRest -= 100; zet(waarde - 1); }
  }

  // Dubbelklik of dubbeltik = terug naar de standaard. Zelf herkend op
  // pointerup: een aanraakscherm geeft niet betrouwbaar een dblclick.
  let laatsteTik = 0;
  function tik() {
    const nu = performance.now();
    if (nu - laatsteTik < 350) {
      laatsteTik = 0;
      zet(STANDAARD);
    } else {
      laatsteTik = nu;
    }
  }

  function begin() { slepen = true; }
  function einde() { slepen = false; tik(); }

  // ---- Compacte stand: knop met uitklapschuif ----
  let open = false;
  let knopEl;
  let popEl;
  let popStijl = '';
  function wisselOpen() {
    open = !open;
    if (open && knopEl) {
      // position: fixed, boven de knop en rechts uitgelijnd: zo valt hij
      // niet buiten het scherm en telt hij niet mee in de opbouw (Passend).
      const r = knopEl.getBoundingClientRect();
      popStijl = `right:${Math.max(4, window.innerWidth - r.right)}px;bottom:${window.innerHeight - r.top + 6}px;`;
    }
  }
  function klikBuiten(e) {
    if (!open) return;
    if (popEl?.contains(e.target) || knopEl?.contains(e.target)) return;
    open = false;
  }
  $: if (!compact) open = false;

  onDestroy(() => { open = false; });
</script>

<!-- pointerup ook op het venster: laat je de muis buiten de schuif los, dan
     komt die niet altijd bij de schuif aan, en dan bleef "slepen" hangen. -->
<svelte:window
  on:pointerdown={klikBuiten}
  on:pointerup={() => { if (slepen) slepen = false; }}
  on:pointercancel={() => { if (slepen) slepen = false; }} />

<div class="volume" class:compact title={$t('setzer.volume_title').replace('{db}', STANDAARD)}>
  {#if !compact}
    <svg class="vol-icoon" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true">
      <polygon points="11 5 6 9 2 9 2 15 6 15 11 19 11 5"/>
      <path d="M15.54 8.46a5 5 0 0 1 0 7.07M19.07 4.93a10 10 0 0 1 0 14.14"/>
    </svg>
    <input
      class="vol-schuif"
      type="range"
      min={MIN}
      max={MAX}
      step="1"
      bind:value={waarde}
      on:input={opInvoer}
      on:pointerdown={begin}
      on:pointerup={einde}
      on:pointercancel={() => { slepen = false; }}
      on:wheel|preventDefault={opWiel}
      aria-label={$t('setzer.volume')}
      aria-valuetext="{waarde} dB"
    />
    <span class="vol-waarde" on:pointerup={tik}>{waarde} dB</span>
  {:else}
    <button
      class="vol-knop"
      class:open
      bind:this={knopEl}
      on:click={wisselOpen}
      on:wheel|preventDefault={opWiel}
      aria-label={$t('setzer.volume_open')}
      aria-expanded={open}
    >
      <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true">
        <polygon points="11 5 6 9 2 9 2 15 6 15 11 19 11 5"/>
        <path d="M15.54 8.46a5 5 0 0 1 0 7.07"/>
      </svg>
      <span class="vol-waarde">{waarde} dB</span>
    </button>
    {#if open}
      <div class="vol-pop" bind:this={popEl} style={popStijl}>
        <input
          class="vol-schuif"
          type="range"
          min={MIN}
          max={MAX}
          step="1"
          bind:value={waarde}
          on:input={opInvoer}
          on:pointerdown={begin}
          on:pointerup={einde}
          on:pointercancel={() => { slepen = false; }}
          on:wheel|preventDefault={opWiel}
          aria-label={$t('setzer.volume')}
          aria-valuetext="{waarde} dB"
        />
        <span class="vol-waarde">{waarde} dB</span>
        <button class="vol-reset" on:click={() => zet(STANDAARD)}>
          {$t('setzer.volume_reset').replace('{db}', STANDAARD)}
        </button>
      </div>
    {/if}
  {/if}
</div>

<style>
  /* Helemaal rechts in de balk; het enige onderdeel dat mag krimpen, dus het
     vangt een tekort aan ruimte op tot zijn minimum. Niet hoger dan de
     setzerknoppen: dan blijft de balk even hoog en verandert Passend niet. */
  .volume {
    margin-left: auto;
    flex: 1 1 220px;
    min-width: 120px;
    max-width: 280px;
    display: flex;
    align-items: center;
    gap: 0.4rem;
    height: 34px;
    color: var(--setzer-text);
  }
  /* Compact: altijd rechts in beeld, ook als de balk te smal is en schuift;
     hij ligt dan over de knoppen eronder heen, met een eigen achtergrond. */
  .volume.compact {
    flex: 0 0 auto;
    min-width: 0;
    max-width: none;
    position: sticky;
    right: 0;
    z-index: 1;
    padding-left: 0.4rem;
    background: var(--setzer-bg);
    box-shadow: -10px 0 8px -6px var(--setzer-bg);
  }
  .vol-icoon {
    flex: 0 0 auto;
    color: var(--setzer-text-muted);
  }
  .vol-schuif {
    flex: 1 1 auto;
    min-width: 60px;
    width: 100%;
    margin: 0;
    cursor: pointer;
  }
  .vol-waarde {
    flex: 0 0 auto;
    min-width: 4.2em;
    text-align: right;
    font-family: 'Courier New', monospace;
    font-size: 0.78rem;
    font-weight: 700;
    font-variant-numeric: tabular-nums;
    color: var(--setzer-display-fg);
    user-select: none;
    cursor: default;
  }

  .vol-knop {
    display: inline-flex;
    align-items: center;
    gap: 0.35rem;
    height: 34px;
    padding: 0 0.55rem;
    border: 1px solid var(--setzer-border-strong);
    border-radius: 3px;
    background: var(--setzer-bg-elevated);
    color: var(--setzer-text);
    cursor: pointer;
    font-family: inherit;
  }
  .vol-knop:hover,
  .vol-knop.open {
    background: var(--setzer-bg-active);
    border-color: var(--setzer-border-hover);
  }
  .vol-knop .vol-waarde {
    min-width: 0;
  }

  /* Boven de speeltafelstrook (z-index 70) en de balken-laag (59/60). */
  .vol-pop {
    position: fixed;
    z-index: 200;
    display: flex;
    align-items: center;
    gap: 0.5rem;
    width: 300px;
    max-width: calc(100vw - 8px);
    padding: 0.55rem 0.7rem;
    background: var(--setzer-bg);
    border: 1px solid var(--setzer-border-strong);
    border-radius: 4px;
    box-shadow: 0 4px 14px rgba(0, 0, 0, 0.45);
  }
  .vol-reset {
    flex: 0 0 auto;
    height: 28px;
    padding: 0 0.5rem;
    border: 1px solid var(--setzer-border-strong);
    border-radius: 3px;
    background: var(--setzer-bg-elevated);
    color: var(--setzer-text);
    font-size: 0.72rem;
    cursor: pointer;
    font-family: inherit;
    white-space: nowrap;
  }

  @media (pointer: coarse) {
    .volume { height: 44px; min-width: 150px; }
    .vol-knop { height: 44px; }
    .vol-reset { height: 40px; }
  }
</style>
