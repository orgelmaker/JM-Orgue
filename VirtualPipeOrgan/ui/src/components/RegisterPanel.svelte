<script>
  import { onMount, onDestroy } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import SetzerBar from './SetzerBar.svelte';
  import { t, tx } from '../lib/i18n.js';

  let organInfo = null;
  let selectedDivisions = [];
  let layout = 'horizontal';
  let pollInterval = null;
  let tremulantState = {};
  let couplerContextMenu = null;
  let couplerMidiBindings = {};
  let stopContextMenu = null;
  let stopMidiBindings = {};
  let divisionVolumes = {};
  let swellPollInterval = null;
  let visibleCouplers = {};
  let swellEnabled = {};
  let stopSize = 100; // min-breedte registerknop in px (gedeeld met hoofdvenster)

  // Stop order (shared with main Console)
  let stopOrder = {};
  function loadStopOrder() {
    if (!organInfo) return;
    try {
      const key = `jm-orgue-stop-order-${organInfo.id || 'default'}`;
      const saved = localStorage.getItem(key);
      if (saved) stopOrder = JSON.parse(saved);
      else stopOrder = {};
    } catch (e) { stopOrder = {}; }
  }
  function sortStops(stops, divName) {
    const order = stopOrder[divName];
    if (!order || order.length === 0) return stops;
    const indexed = new Map(stops.map(s => [s.id, s]));
    const sorted = [];
    for (const id of order) {
      if (indexed.has(id)) { sorted.push(indexed.get(id)); indexed.delete(id); }
    }
    for (const s of indexed.values()) sorted.push(s);
    return sorted;
  }

  // Per-orgel localStorage-sleutels (zelfde hash als het hoofdvenster) zodat zwel- en
  // koppel-zichtbaarheid + knopgrootte niet meer tussen orgels lekken.
  function hashCode(str) {
    let h = 0;
    for (let i = 0; i < (str || '').length; i++) { h = ((h << 5) - h) + str.charCodeAt(i); h |= 0; }
    return Math.abs(h).toString(36);
  }
  function organUiKey(base) {
    return organInfo && organInfo.id ? base + '-' + hashCode(organInfo.id) : base;
  }
  function readOrganUiPref(base) {
    const v = localStorage.getItem(organUiKey(base));
    return v !== null ? v : localStorage.getItem(base);
  }

  function isCouplerVisible(couplerId) {
    // Default: hidden — koppels worden pas getoond als de gebruiker ze in de
    // instellingen expliciet aanzet (visibleCouplers[id] === true).
    return visibleCouplers[couplerId] === true;
  }

  function isSwellEnabled(divisionName) {
    return swellEnabled[divisionName] === true;
  }

  // Refresh settings from localStorage periodically — per orgel (zelfde keys als Console).
  function refreshSettings() {
    try {
      const vc = readOrganUiPref('jm-orgue-visible-couplers');
      if (vc) visibleCouplers = JSON.parse(vc);
    } catch (e) {}
    try {
      const se = readOrganUiPref('jm-orgue-swell-enabled');
      if (se) swellEnabled = JSON.parse(se);
    } catch (e) {}
    // Synchroniseer registerknop-grootte met het hoofdvenster (per orgel).
    const ss = parseInt(readOrganUiPref('jm-orgue-stop-size'), 10);
    if (ss >= 70 && ss <= 220 && ss !== stopSize) stopSize = ss;
  }

  let settingsInterval = null;

  onMount(async () => {
    // Layout + knopgrootte per orgel (met fallback op de oude globale key).
    // Vóór refreshOrgan() is organInfo nog null → dan leest dit de globale key;
    // zodra het orgel bekend is laadt refreshOrgan() de per-orgel-waarden.
    loadPanelUiPrefs();
    await refreshOrgan();
    pollInterval = setInterval(refreshOrgan, 300);
    swellPollInterval = setInterval(pollDivisionVolumes, 100);
    settingsInterval = setInterval(refreshSettings, 1000);
    window.addEventListener('click', () => { closeCouplerContextMenu(); closeStopContextMenu(); });
    loadCouplerMidiCounts();
  });

  onDestroy(() => {
    if (pollInterval) clearInterval(pollInterval);
    if (swellPollInterval) clearInterval(swellPollInterval);
    if (settingsInterval) clearInterval(settingsInterval);
  });

  // Laad layout + knopgrootte uit de per-orgel keys (fallback: oude globale key).
  function loadPanelUiPrefs() {
    const savedLayout = readOrganUiPref('jm-orgue-panel-layout');
    if (savedLayout === 'horizontal' || savedLayout === 'vertical') layout = savedLayout;
    const ss = parseInt(readOrganUiPref('jm-orgue-stop-size'), 10);
    if (ss >= 70 && ss <= 220) stopSize = ss;
  }

  let prevPanelOrganId = '';
  async function refreshOrgan() {
    try {
      organInfo = await invoke('get_organ_info');
      // Bij een echte orgelwissel (id verandert): reset de lokale tremulant-stand,
      // anders toont een gelijknamige divisie ten onrechte een "actieve" tremulant.
      if (organInfo && organInfo.id !== prevPanelOrganId) {
        prevPanelOrganId = organInfo.id;
        tremulantState = {};
        // Per-orgel layout/knopgrootte van dít orgel laden.
        loadPanelUiPrefs();
      }
      loadStopOrder();
      if (organInfo && organInfo.divisions && selectedDivisions.length === 0) {
        selectedDivisions = organInfo.divisions.map(d => d.name);
      }
    } catch (e) {}
  }

  async function toggleStop(stopId) {
    try {
      await invoke('toggle_stop', { stopId });
      organInfo = await invoke('get_organ_info');
    } catch (e) {
      console.error('Toggle stop failed:', e);
    }
  }

  function toggleTremulant(divisionName) {
    const newState = !tremulantState[divisionName];
    tremulantState[divisionName] = newState;
    tremulantState = tremulantState;
    invoke('set_tremulant', { divisionName, active: newState }).catch(console.error);
  }

  async function toggleCoupler(couplerId) {
    try {
      await invoke('toggle_coupler', { couplerId });
      organInfo = await invoke('get_organ_info');
    } catch (e) {
      console.error('Toggle coupler failed:', e);
    }
  }

  function showCouplerContextMenu(e, actionCode) {
    e.preventDefault();
    couplerContextMenu = { x: e.clientX, y: e.clientY, action: actionCode };
  }

  function closeCouplerContextMenu() {
    couplerContextMenu = null;
  }

  async function learnCouplerMidi(actionCode) {
    couplerContextMenu = null;
    const count = couplerMidiBindings[actionCode] || 0;
    if (count >= 4) {
      alert(tx('alerts.max_midi_signals'));
      return;
    }
    try {
      const result = await invoke('learn_preset_binding', { presetNum: actionCode });
      if (result) {
        couplerMidiBindings[actionCode] = (couplerMidiBindings[actionCode] || 0) + 1;
        couplerMidiBindings = couplerMidiBindings;
      }
    } catch (e) {
      console.error('Coupler MIDI learn failed:', e);
    }
  }

  async function clearCouplerMidi(actionCode) {
    couplerContextMenu = null;
    try {
      await invoke('clear_preset_binding', { presetNum: actionCode });
      couplerMidiBindings[actionCode] = 0;
      couplerMidiBindings = couplerMidiBindings;
    } catch (e) {
      console.error('Failed to clear coupler MIDI:', e);
    }
  }

  async function pollDivisionVolumes() {
    try {
      const volumes = await invoke('get_division_volumes');
      for (const v of volumes) {
        divisionVolumes[v.division] = v.volume;
      }
      divisionVolumes = divisionVolumes;
    } catch (e) {}
  }

  function getSwellLevel(divisionName) {
    return divisionVolumes[divisionName] ?? 1.0;
  }

  async function setSwellLevel(divisionName, value) {
    divisionVolumes[divisionName] = value;
    divisionVolumes = divisionVolumes;
    try {
      await invoke('set_division_volume', { division: divisionName, volume: value });
    } catch (e) {
      console.error('Failed to set division volume:', e);
    }
  }

  async function loadCouplerMidiCounts() {
    try {
      const bindings = await invoke('get_preset_bindings');
      couplerMidiBindings = {};
      stopMidiBindings = {};
      for (const b of bindings) {
        if (b.preset_num >= 150) {
          stopMidiBindings[b.preset_num] = (stopMidiBindings[b.preset_num] || 0) + 1;
        } else if (b.preset_num >= 100) {
          couplerMidiBindings[b.preset_num] = (couplerMidiBindings[b.preset_num] || 0) + 1;
        }
      }
    } catch (e) { /* ignore */ }
  }

  // Stop MIDI learn functions
  function showStopContextMenu(e, actionCode) {
    e.preventDefault();
    stopContextMenu = { x: e.clientX, y: e.clientY, action: actionCode };
  }

  function closeStopContextMenu() {
    stopContextMenu = null;
  }

  async function learnStopMidi(actionCode) {
    stopContextMenu = null;
    const count = stopMidiBindings[actionCode] || 0;
    if (count >= 4) {
      alert(tx('alerts.max_midi_signals'));
      return;
    }
    try {
      const result = await invoke('learn_preset_binding', { presetNum: actionCode });
      if (result) {
        stopMidiBindings[actionCode] = (stopMidiBindings[actionCode] || 0) + 1;
        stopMidiBindings = stopMidiBindings;
      }
    } catch (e) {
      console.error('Stop MIDI learn failed:', e);
    }
  }

  async function clearStopMidi(actionCode) {
    stopContextMenu = null;
    try {
      await invoke('clear_preset_binding', { presetNum: actionCode });
      stopMidiBindings[actionCode] = 0;
      stopMidiBindings = stopMidiBindings;
    } catch (e) {
      console.error('Failed to clear stop MIDI:', e);
    }
  }

  function toggleLayout() {
    layout = layout === 'horizontal' ? 'vertical' : 'horizontal';
    // Per-orgel key — de globale key zou door refreshSettings/loadPanelUiPrefs
    // (die de per-orgel key voorrang geven) direct weer overschreven worden.
    localStorage.setItem(organUiKey('jm-orgue-panel-layout'), layout);
  }

  function adjustStopSize(delta) {
    stopSize = Math.max(70, Math.min(220, stopSize + delta));
    // Per-orgel key — zelfde key als Console.adjustStopSize, anders springt de
    // grootte binnen een seconde terug via de refreshSettings-poll.
    localStorage.setItem(organUiKey('jm-orgue-stop-size'), String(stopSize));
  }

  function toggleDivision(name) {
    if (selectedDivisions.includes(name)) {
      if (selectedDivisions.length > 1) {
        selectedDivisions = selectedDivisions.filter(d => d !== name);
      }
    } else {
      selectedDivisions = [...selectedDivisions, name];
    }
  }

  function getStopClass(stop) {
    const name = stop.name.toLowerCase();
    if (name.includes('trompet') || name.includes('hobo') || name.includes('bazuin') || name.includes('schalmei')) return 'reed';
    if (name.includes('fluit') || name.includes('gedekt') || name.includes('bourdon') || name.includes('gedackt')) return 'flute';
    if (name.includes('viola') || name.includes('gamba') || name.includes('celeste') || name.includes('salicional') || name.includes('voix')) return 'string';
    if (name.includes('mixtuur') || name.includes('cymbel')) return 'mixture';
    if (name.includes('koppel') || name.includes('coupler')) return 'coupler';
    if (name.includes('tremulant')) return 'tremulant';
    return '';
  }

  function formatPitch(feet) {
    if (!feet || feet === 0) return '';
    if (feet === Math.floor(feet)) return `${feet}'`;
    if (Math.abs(feet - 2.666) < 0.01) return "2\u2154'";
    if (Math.abs(feet - 5.333) < 0.01) return "5\u2153'";
    if (Math.abs(feet - 1.333) < 0.01) return "1\u2153'";
    if (Math.abs(feet - 1.6) < 0.01) return "1\u2157'";
    return `${feet}'`;
  }

  function cleanStopName(stop) {
    let name = stop.name;
    if (stop.pitch_feet && stop.pitch_feet > 0) {
      const pitchStr = formatPitch(stop.pitch_feet);
      if (pitchStr) {
        if (name.endsWith(' ' + pitchStr)) name = name.slice(0, -(pitchStr.length + 1)).trim();
        else if (name.endsWith(pitchStr)) name = name.slice(0, -pitchStr.length).trim();
      }
    }
    if (stop.pitch) {
      if (name.endsWith(' ' + stop.pitch)) name = name.slice(0, -(stop.pitch.length + 1)).trim();
      else if (name.endsWith(stop.pitch)) name = name.slice(0, -stop.pitch.length).trim();
    }
    return name || stop.name;
  }

  // Auto-shrink font for long names
  function stopNameStyle(name) {
    const len = name.length;
    if (len <= 12) return '';
    if (len <= 16) return 'font-size: 0.68rem;';
    if (len <= 20) return 'font-size: 0.6rem;';
    return 'font-size: 0.52rem;';
  }
</script>

<div class="panel-window">
  {#if !organInfo}
    <div class="panel-empty">
      <p>{$t('register_panel.no_organ')}</p>
    </div>
  {:else}
    <div class="panel-header">
      <div class="panel-config">
        {#each organInfo.divisions as div}
          <button
            class="panel-div-btn"
            class:selected={selectedDivisions.includes(div.name)}
            on:click={() => toggleDivision(div.name)}
          >{div.name}</button>
        {/each}
      </div>
      <div class="panel-actions">
        <button class="panel-toggle-btn panel-size-btn" on:click={() => adjustStopSize(-15)} title="Registers kleiner" aria-label="Kleiner">−</button>
        <button class="panel-toggle-btn panel-size-btn" on:click={() => adjustStopSize(15)} title="Registers groter" aria-label="Groter">+</button>
        <button class="panel-toggle-btn" on:click={toggleLayout} title="{layout === 'horizontal' ? 'Verticaal' : 'Horizontaal'}">
          {#if layout === 'horizontal'}
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <rect x="3" y="3" width="7" height="18" rx="1"/>
              <rect x="14" y="3" width="7" height="18" rx="1"/>
            </svg>
          {:else}
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <rect x="3" y="3" width="18" height="7" rx="1"/>
              <rect x="3" y="14" width="18" height="7" rx="1"/>
            </svg>
          {/if}
        </button>
      </div>
    </div>

    <div class="panel-content" class:divisions-vertical={layout === 'vertical'}>
      {#each selectedDivisions.map(name => organInfo.divisions.find(d => d.name === name)).filter(Boolean) as division}
        {@const hasTrem = division.stops.some(s => s.has_tremulant)}
        <div class="division">
          <div class="division-header division-header-compact">
            <div class="division-name">{division.name}</div>
            {#if isSwellEnabled(division.name)}
              <div class="swell-leds">
                {#each Array(10) as _, i}
                  <span
                    class="swell-led"
                    class:active={i < Math.round(getSwellLevel(division.name) * 10)}
                    on:click|stopPropagation={() => setSwellLevel(division.name, (i + 1) / 10)}
                    on:keypress={(e) => e.key === 'Enter' && setSwellLevel(division.name, (i + 1) / 10)}
                    role="button"
                    tabindex="0"
                    title="{(i + 1) * 10}%"
                  ></span>
                {/each}
              </div>
            {/if}
          </div>
          <div class="stops-grid" class:stops-vertical={layout === 'vertical'} style="--stop-min-width: {stopSize}px">
            {#each sortStops(division.stops, division.name) as stop}
              {@const name = cleanStopName(stop)}
              <button
                class="stop-knob {getStopClass(stop)}"
                class:engaged={stop.drawn}
                class:has-midi={stopMidiBindings[stop.midi_action_code] > 0}
                on:click={() => toggleStop(stop.id)}
                on:contextmenu|preventDefault={(e) => showStopContextMenu(e, stop.midi_action_code)}
              >
                <span class="stop-name" style="{stopNameStyle(name)}">{name}</span>
                {#if stop.pitch || stop.pitch_feet}
                  <span class="stop-pitch">{stop.pitch || formatPitch(stop.pitch_feet)}</span>
                {/if}
              </button>
            {/each}
            {#if hasTrem}
              <button
                class="stop-knob tremulant"
                class:engaged={tremulantState[division.name]}
                on:click={() => toggleTremulant(division.name)}
              >
                <span class="stop-name">Tremulant</span>
              </button>
            {/if}
            <!-- Koppelknoppen (alleen zichtbare) -->
            {#if organInfo.couplers}
              {#each organInfo.couplers.filter(c => c.display_in_division === division.name && isCouplerVisible(c.id)) as coupler}
                <button
                  class="stop-knob coupler coupler-knob"
                  class:engaged={coupler.active}
                  class:has-midi={couplerMidiBindings[coupler.midi_action_code] > 0}
                  on:click={() => toggleCoupler(coupler.id)}
                  on:contextmenu|preventDefault={(e) => showCouplerContextMenu(e, coupler.midi_action_code)}
                  title={coupler.name.replace(/\n/g, ' ')}
                >
                  {#each coupler.name.split('\n') as line}
                    <span class="coupler-line">{line}</span>
                  {/each}
                </button>
              {/each}
            {/if}
          </div>
        </div>
      {/each}
    </div>
    <!-- Bewust GEEN on:externalAction-handler: tremulant/EQ/crescendo/afsluiten
         (actiecodes 24..99 en 42) horen bij het hoofdvenster (Console/App), dat
         zijn eigen SetzerBar met handler heeft. Dit extra venster negeert die
         codes expres — stop-/koppel-/preset-acties werken hier wél gewoon. -->
    <SetzerBar organInfo={organInfo} />
  {/if}

  <!-- Coupler MIDI context menu -->
  {#if couplerContextMenu}
    <div class="coupler-context-menu" style="left: {couplerContextMenu.x}px; top: {couplerContextMenu.y}px;">
      <button on:click={() => learnCouplerMidi(couplerContextMenu.action)}>
        {$t('midi.learn_title')} ({couplerMidiBindings[couplerContextMenu.action] || 0}/4)
      </button>
      {#if couplerMidiBindings[couplerContextMenu.action] > 0}
        <button on:click={() => clearCouplerMidi(couplerContextMenu.action)}>
          {$t('midi.learn_clear')}
        </button>
      {/if}
    </div>
  {/if}

  <!-- Stop MIDI context menu -->
  {#if stopContextMenu}
    <div class="coupler-context-menu" style="left: {stopContextMenu.x}px; top: {stopContextMenu.y}px;">
      <button on:click={() => learnStopMidi(stopContextMenu.action)}>
        {$t('midi.learn_title')} ({stopMidiBindings[stopContextMenu.action] || 0}/4)
      </button>
      {#if stopMidiBindings[stopContextMenu.action] > 0}
        <button on:click={() => clearStopMidi(stopContextMenu.action)}>
          {$t('midi.learn_clear')}
        </button>
      {/if}
    </div>
  {/if}
</div>

<style>
  .panel-window {
    display: flex;
    flex-direction: column;
    height: 100vh;
    background: var(--bg-dark);
  }
  .panel-empty {
    display: flex;
    align-items: center;
    justify-content: center;
    flex: 1;
    color: var(--text-muted);
  }
  .panel-content {
    flex: 1;
    overflow-y: auto;
    padding: 0.75rem;
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
  }
  .panel-content.divisions-vertical {
    flex-direction: row;
    align-items: flex-start;
    flex-wrap: wrap;
    overflow-x: auto;
  }
  .panel-content.divisions-vertical > :global(.division) {
    flex: 1 1 0;
    min-width: 0;
  }
  .panel-toggle-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 30px;
    height: 30px;
    border: 1px solid rgba(180, 150, 11, 0.3);
    border-radius: var(--radius-sm);
    background: var(--bg-panel);
    color: var(--text-secondary);
    cursor: pointer;
    transition: all 0.15s ease;
  }
  .panel-toggle-btn:hover {
    border-color: var(--gold-border);
    color: var(--text);
    background: var(--bg-elevated);
  }
  .panel-size-btn {
    font-size: 1.1rem;
    font-weight: 600;
    line-height: 1;
  }
</style>
