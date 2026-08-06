<script>
  import { onMount, onDestroy, createEventDispatcher } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { midiLearn } from '../lib/midiLearn.js';
  import { t, tx } from '../lib/i18n.js';

  // Dispatcher voor acties die de parent afhandelt (externalAction: tremulant/EQ/
  // crescendo/afsluiten). Zonder deze dispatcher gaf handleMidiAction een stille
  // ReferenceError in pollMidiTriggers en deden die MIDI-acties niets.
  const dispatch = createEventDispatcher();

  export let organInfo = null;
  export let initialPresets = null; // Presets from library restore

  // Setzer state (-1 = geen preset actief)
  let currentPreset = -1;
  let setMode = false;
  let memoryLevel = 1; // 1-8 memory levels
  let presets = {}; // { [presetNum]: [stopId1, stopId2, ...] }

  // Export functions for App.svelte to read/write presets
  export function getPresets() {
    return presets;
  }

  export function setPresets(data) {
    if (data && typeof data === 'object') {
      presets = data;
      savePresetsToStorage();
    }
  }
  let midiBindings = {}; // { [actionCode]: count }

  // Context menu state
  let contextMenu = null; // { x, y, action }

  // Alleen het hoofdvenster consumeert MIDI-preset-triggers: het kanaal is
  // consumerend (try_recv), dus een tweede poller (extra registerpaneel) zou
  // triggers willekeurig "stelen" (auditbevinding 5/10).
  export let consumeMidiTriggers = true;

  // MIDI polling interval
  let pollInterval;

  // Action code mapping
  const ACTION_SET = 10;
  const ACTION_GC = 11;
  const ACTION_MINUS1 = 12;
  const ACTION_PLUS1 = 13;
  const ACTION_MINUS10 = 14;
  const ACTION_PLUS10 = 15;
  // Memory level direct-keuze knoppen M1..M8 → action codes 16..23
  const ACTION_M_BASE = 16;

  $: organId = organInfo?.id || '';
  $: storageKey = organId ? `jm-orgue-presets-${hashCode(organId)}-m${memoryLevel}` : '';
  $: currentBank = currentPreset >= 0 ? Math.floor(currentPreset / 10) : 0;

  function keyForLevel(level) {
    return organId ? `jm-orgue-presets-${hashCode(organId)}-m${level}` : '';
  }

  function switchMemoryLevel(level) {
    // Save current presets first
    savePresetsToStorage();
    memoryLevel = level;
    // NIET via de reactive storageKey laden: die is binnen deze handler nog
    // niet herberekend, waardoor het OUDE niveau geladen werd (audit 8).
    loadPresetsFromKey(keyForLevel(level));
  }

  function loadPresetsFromKey(key) {
    if (!key) return;
    try {
      const saved = localStorage.getItem(key);
      // Leeg niveau = echt leeg — niet de presets van het vorige niveau laten staan.
      presets = saved ? JSON.parse(saved) : {};
    } catch (e) {
      console.warn('Failed to load presets:', e);
      presets = {};
    }
  }

  function hashCode(str) {
    let hash = 0;
    for (let i = 0; i < str.length; i++) {
      const char = str.charCodeAt(i);
      hash = ((hash << 5) - hash) + char;
      hash |= 0;
    }
    return Math.abs(hash).toString(36);
  }

  // Crescendo live state
  let crescEnabled = false;
  let crescStage = 0;
  let crescTotal = 0;
  let crescPollInterval;

  async function pollCrescendoState() {
    try {
      const result = await invoke('get_crescendo_state');
      if (Array.isArray(result) && result.length === 3) {
        crescEnabled = result[0];
        crescStage = result[1];
        crescTotal = result[2];
      }
    } catch (e) {}
  }

  // Crescendo-pedaal (continue CC) inleren — direct vanaf de indicator (rechtermuis/long-press).
  let crescLearning = false;
  async function learnCrescendoPedal() {
    if (crescLearning) return;
    crescLearning = true;
    try {
      await invoke('learn_crescendo_pedal');
    } catch (e) {
      console.error('Crescendo-pedaal inleren mislukt:', e);
    }
    crescLearning = false;
  }

  onMount(() => {
    loadPresets();
    loadMidiBindingCounts();
    if (consumeMidiTriggers) {
      // Eerst opgespaarde triggers van vóór het mounten WEGGOOIEN: dat waren
      // oude piston-drukken (bv. tijdens een orgel-load) die anders nu in één
      // keer afgespeeld werden (auditbevinding 9).
      (async () => {
        try {
          for (let i = 0; i < 20; i++) {
            const t = await invoke('poll_preset_trigger');
            if (t === null || t === undefined) break;
          }
        } catch (e) {}
      })();
      // Poll for MIDI triggers every 100ms
      pollInterval = setInterval(pollMidiTriggers, 100);
    }
    crescPollInterval = setInterval(pollCrescendoState, 100);
    // Close context menu on click elsewhere
    window.addEventListener('click', closeContextMenu);
  });

  onDestroy(() => {
    if (pollInterval) clearInterval(pollInterval);
    if (crescPollInterval) clearInterval(crescPollInterval);
    window.removeEventListener('click', closeContextMenu);
  });

  // Load presets from localStorage (or from initialPresets if provided)
  function loadPresets() {
    if (initialPresets && Object.keys(initialPresets).length > 0) {
      presets = initialPresets;
      savePresetsToStorage();
      return;
    }
    loadPresetsFromKey(storageKey);
  }

  // Save presets to localStorage
  function savePresetsToStorage() {
    if (!storageKey) return;
    localStorage.setItem(storageKey, JSON.stringify(presets));
  }

  // Load MIDI binding counts from backend
  async function loadMidiBindingCounts() {
    try {
      const bindings = await invoke('get_preset_bindings');
      midiBindings = {};
      for (const b of bindings) {
        midiBindings[b.preset_num] = (midiBindings[b.preset_num] || 0) + 1;
      }
    } catch (e) {
      console.warn('Failed to load MIDI bindings:', e);
    }
  }

  // Get current drawn stop IDs from organInfo
  function getDrawnStopIds() {
    if (!organInfo?.divisions) return [];
    const ids = [];
    for (const div of organInfo.divisions) {
      for (const stop of div.stops) {
        if (stop.drawn) ids.push(stop.id);
      }
    }
    return ids;
  }

  // Get current active coupler IDs from organInfo
  function getActiveCouplerIds() {
    if (!organInfo?.couplers) return [];
    return organInfo.couplers.filter(c => c.active).map(c => c.id);
  }

  // Save current registration to a preset slot
  function savePreset(num) {
    const stopIds = getDrawnStopIds();
    const couplerIds = getActiveCouplerIds();
    presets[num] = { stops: stopIds, couplers: couplerIds };
    presets = presets; // trigger reactivity
    savePresetsToStorage();
    currentPreset = num;
    setMode = false;
  }

  // Load a preset (recall registration)
  async function loadPreset(num) {
    currentPreset = num;
    const preset = presets[num];
    // Backward compatible: old format is array, new format is { stops, couplers }
    let stopIds = [];
    let couplerIds = [];
    if (Array.isArray(preset)) {
      stopIds = preset;
    } else if (preset) {
      stopIds = preset.stops || [];
      couplerIds = preset.couplers || [];
    }
    try {
      await invoke('set_drawn_stops', { stopIds });
      await invoke('set_active_couplers', { couplerIds });
    } catch (e) {
      console.error('Failed to load preset:', e);
    }
  }

  // Digit button pressed (0-9)
  function digitPressed(d) {
    const presetNum = currentBank * 10 + d;
    if (setMode) {
      savePreset(presetNum);
    } else {
      loadPreset(presetNum);
    }
  }

  // Step through presets by delta
  function stepPreset(delta) {
    const base = currentPreset >= 0 ? currentPreset : 0;
    const next = Math.max(0, Math.min(999, base + delta));
    loadPreset(next);
  }

  // General Cancel - all stops off, all couplers off, reset
  async function generalCancel() {
    try {
      await invoke('set_drawn_stops', { stopIds: [] });
      await invoke('set_active_couplers', { couplerIds: [] });
      currentPreset = -1;
    } catch (e) {
      console.error('Failed to general cancel:', e);
    }
  }

  // Toggle SET mode
  function toggleSetMode() {
    setMode = !setMode;
  }

  // Reactive: which digit (0-9) is active in the current bank (-1 = geen)
  $: activeDigit = currentPreset >= 0 ? currentPreset % 10 : -1;

  // Reactive: has-data flags for each digit in current bank
  $: dataFlags = [0,1,2,3,4,5,6,7,8,9].map(d => {
    const num = currentBank * 10 + d;
    const p = presets[num];
    if (!p) return false;
    // Old format: array of stop IDs
    if (Array.isArray(p)) return p.length > 0;
    // New format: { stops, couplers }
    return (p.stops && p.stops.length > 0) || (p.couplers && p.couplers.length > 0);
  });

  // Context menu handling
  function showContextMenu(e, actionCode) {
    e.preventDefault();
    contextMenu = {
      x: e.clientX,
      y: e.clientY,
      action: actionCode,
    };
  }

  function showContextMenuAt(actionCode) {
    // Positie midden op scherm — werkt voor zowel rechtermuis als touch long-press
    contextMenu = {
      x: Math.round(window.innerWidth / 2 - 80),
      y: Math.round(window.innerHeight / 2 - 40),
      action: actionCode,
    };
  }

  function closeContextMenu() {
    contextMenu = null;
  }

  // MIDI Learn
  async function startMidiLearn(actionCode) {
    contextMenu = null;
    const count = midiBindings[actionCode] || 0;
    if (count >= 4) {
      alert(tx('midi.max_signals_alert'));
      return;
    }
    try {
      const result = await invoke('learn_preset_binding', { presetNum: actionCode });
      if (result) {
        midiBindings[actionCode] = (midiBindings[actionCode] || 0) + 1;
        midiBindings = midiBindings; // trigger reactivity
      }
    } catch (e) {
      console.error('MIDI learn failed:', e);
    }
  }

  // Clear MIDI bindings for an action
  async function clearMidiBindings(actionCode) {
    contextMenu = null;
    try {
      await invoke('clear_preset_binding', { presetNum: actionCode });
      midiBindings[actionCode] = 0;
      midiBindings = midiBindings;
    } catch (e) {
      console.error('Failed to clear MIDI bindings:', e);
    }
  }

  // Poll for incoming MIDI preset triggers
  async function pollMidiTriggers() {
    try {
      const triggered = await invoke('poll_preset_trigger');
      if (triggered !== null && triggered !== undefined) {
        handleMidiAction(triggered);
      }
    } catch (e) {
      // Silently ignore poll errors
    }
  }

  // Handle a triggered action (from MIDI or UI)
  function handleMidiAction(actionCode) {
    if (actionCode >= 0 && actionCode <= 9) {
      digitPressed(actionCode);
    } else if (actionCode === ACTION_SET) {
      toggleSetMode();
    } else if (actionCode === ACTION_GC) {
      generalCancel();
    } else if (actionCode === ACTION_MINUS1) {
      stepPreset(-1);
    } else if (actionCode === ACTION_PLUS1) {
      stepPreset(1);
    } else if (actionCode === ACTION_MINUS10) {
      stepPreset(-10);
    } else if (actionCode === ACTION_PLUS10) {
      stepPreset(10);
    } else if (actionCode >= ACTION_M_BASE && actionCode < ACTION_M_BASE + 8) {
      // Direct memory level kiezen: M1..M8
      switchMemoryLevel(actionCode - ACTION_M_BASE + 1);
    } else if (actionCode === 42) {
      // Computer afsluiten (MIDI-leerbaar) — door naar App.svelte voor bevestiging + afsluiten
      dispatch('externalAction', { actionCode });
    } else if (actionCode >= 24 && actionCode < 100) {
      // Tremulant LFO (24..39) + algemene toggles (40..99) — laat parent (Console) het afhandelen
      dispatch('externalAction', { actionCode });
    } else if (actionCode >= 150) {
      // Stop MIDI trigger - find matching stop and toggle
      if (organInfo?.divisions) {
        for (const div of organInfo.divisions) {
          const stop = div.stops.find(s => s.midi_action_code === actionCode);
          if (stop) {
            invoke('toggle_stop', { stopId: stop.id }).catch(console.error);
            break;
          }
        }
      }
    } else if (actionCode >= 100) {
      // Coupler MIDI trigger - find matching coupler and toggle
      if (organInfo?.couplers) {
        const coupler = organInfo.couplers.find(c => c.midi_action_code === actionCode);
        if (coupler) {
          invoke('toggle_coupler', { couplerId: coupler.id }).catch(console.error);
        }
      }
    }
  }

  // Reload presets when organ changes
  let prevOrganId = '';
  $: {
    if (organId && organId !== prevOrganId) {
      prevOrganId = organId;
      loadPresets();
      loadMidiBindingCounts();
      currentPreset = -1;
      setMode = false;
    }
  }
</script>

<div class="setzer-bar">
  <!-- SET button -->
  <button
    class="setzer-btn set-btn"
    class:active={setMode}
    on:click={toggleSetMode}
    use:midiLearn={{ onTrigger: () => showContextMenuAt(ACTION_SET) }}
    title="Registratie opslaan (SET) · rechtermuis of 3s ingedrukt = MIDI inleren"
  >SET</button>

  <!-- Digit buttons 0-9 -->
  <div class="setzer-digits">
    {#each [0,1,2,3,4,5,6,7,8,9] as d}
      <button
        class="setzer-btn digit-btn"
        class:active={d === activeDigit && !setMode}
        class:has-data={dataFlags[d]}
        class:set-mode={setMode}
        class:has-midi={midiBindings[d] > 0}
        on:click={() => digitPressed(d)}
        use:midiLearn={{ onTrigger: () => showContextMenuAt(d) }}
        title="Preset {currentBank * 10 + d} · rechtermuis of 3s ingedrukt = MIDI inleren"
      >{d}</button>
    {/each}
  </div>

  <!-- Navigation -1/+1 -->
  <div class="setzer-nav">
    <button
      class="setzer-btn nav-btn"
      class:has-midi={midiBindings[ACTION_MINUS1] > 0}
      on:click={() => stepPreset(-1)}
      use:midiLearn={{ onTrigger: () => showContextMenuAt(ACTION_MINUS1) }}
      title={$t('setzer.prev_preset')}
    >-1</button>
    <button
      class="setzer-btn nav-btn"
      class:has-midi={midiBindings[ACTION_PLUS1] > 0}
      on:click={() => stepPreset(1)}
      use:midiLearn={{ onTrigger: () => showContextMenuAt(ACTION_PLUS1) }}
      title={$t('setzer.next_preset')}
    >+1</button>
  </div>

  <!-- Bank navigation -10/+10 -->
  <div class="setzer-bank">
    <button
      class="setzer-btn nav-btn"
      class:has-midi={midiBindings[ACTION_MINUS10] > 0}
      on:click={() => stepPreset(-10)}
      use:midiLearn={{ onTrigger: () => showContextMenuAt(ACTION_MINUS10) }}
      title={$t('setzer.prev_bank')}
    >-10</button>
    <button
      class="setzer-btn nav-btn"
      class:has-midi={midiBindings[ACTION_PLUS10] > 0}
      on:click={() => stepPreset(10)}
      use:midiLearn={{ onTrigger: () => showContextMenuAt(ACTION_PLUS10) }}
      title={$t('setzer.next_bank')}
    >+10</button>
  </div>

  <!-- Digital display -->
  <div class="setzer-display" title="Preset nummer">
    {currentPreset >= 0 ? String(currentPreset).padStart(3, '0') : '---'}
  </div>

  <!-- General Cancel -->
  <button
    class="setzer-btn gc-btn"
    class:has-midi={midiBindings[ACTION_GC] > 0}
    on:click={generalCancel}
    use:midiLearn={{ onTrigger: () => showContextMenuAt(ACTION_GC) }}
    title={$t('setzer.general_cancel')}
  >G.C.</button>

  <!-- Memory Level selector -->
  <div class="setzer-memory">
    <span class="memory-label">M</span>
    {#each [1,2,3,4,5,6,7,8] as level}
      <button
        class="setzer-btn memory-btn"
        class:active={memoryLevel === level}
        class:has-midi={midiBindings[ACTION_M_BASE + level - 1] > 0}
        on:click={() => switchMemoryLevel(level)}
        use:midiLearn={{ onTrigger: () => showContextMenuAt(ACTION_M_BASE + level - 1) }}
        title={$t('setzer.memory').replace('{n}', level)}
      >{level}</button>
    {/each}
  </div>

  <!-- Crescendo indicator: zichtbaar zodra ingesteld (stages > 0), gedimd indien uit.
       Rechtermuis / 3s ingedrukt = crescendo-pedaal (CC) inleren. -->
  {#if crescTotal > 0}
    <div
      class="cresc-indicator"
      class:disabled={!crescEnabled}
      class:learning={crescLearning}
      role="button"
      tabindex="0"
      use:midiLearn={{ onTrigger: learnCrescendoPedal }}
      title="{$t('settings.crescendo')}: {$t('settings.crescendo_stage')} {crescStage}/{crescTotal}{crescEnabled ? '' : ' (uit)'} · rechtermuis of 3s ingedrukt = pedaal inleren"
    >
      <span class="cresc-label">Cresc</span>
      <div class="cresc-leds">
        {#each Array(crescTotal) as _, i}
          <span class="cresc-led" class:active={crescEnabled && i < crescStage}></span>
        {/each}
      </div>
      <span class="cresc-value">{crescStage}/{crescTotal}</span>
    </div>
  {/if}
</div>

<!-- Context menu for MIDI learn -->
{#if contextMenu}
  <div class="setzer-context-menu" style="left: {contextMenu.x}px; top: {contextMenu.y}px;">
    <button on:click={() => startMidiLearn(contextMenu.action)}>
      {$t('midi.learn_title')} ({midiBindings[contextMenu.action] || 0}/4)
    </button>
    {#if midiBindings[contextMenu.action] > 0}
      <button on:click={() => clearMidiBindings(contextMenu.action)}>
        {$t('midi.learn_clear')}
      </button>
    {/if}
  </div>
{/if}

<style>
  .setzer-bar {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.4rem 0.75rem;
    background: var(--setzer-bg);
    border-top: 1px solid var(--setzer-border);
    flex-shrink: 0;
    min-height: 44px;
  }

  .setzer-btn {
    height: 34px;
    min-width: 34px;
    padding: 0 0.5rem;
    border: 1px solid var(--setzer-border-strong);
    border-radius: 3px;
    background: var(--setzer-bg-elevated);
    color: var(--setzer-text);
    font-size: 0.8rem;
    font-weight: 600;
    cursor: pointer;
    transition: all 0.15s ease;
    position: relative;
    font-family: inherit;
  }

  .setzer-btn:hover {
    background: var(--setzer-bg-active);
    color: var(--setzer-text-strong);
    border-color: var(--setzer-border-hover);
  }

  .setzer-btn:active {
    background: var(--setzer-border-strong);
    transform: scale(0.95);
  }

  /* SET button */
  .set-btn {
    background: var(--setzer-set-bg);
    border-color: var(--primary);
    color: var(--setzer-set-text);
    font-weight: 700;
  }

  .set-btn.active {
    background: var(--setzer-set-bg-active);
    color: var(--setzer-set-text-active);
    animation: pulse-gold 1s infinite;
  }

  @keyframes pulse-gold {
    0%, 100% { box-shadow: 0 0 0 0 var(--gold-shadow); }
    50% { box-shadow: 0 0 8px 2px var(--gold-shadow); }
  }

  /* Digit buttons */
  .setzer-digits {
    display: flex;
    gap: 2px;
  }

  .digit-btn {
    min-width: 30px;
    padding: 0;
  }

  .digit-btn.active {
    background: var(--primary-hover);
    color: var(--setzer-set-text-active);
    border-color: var(--primary);
  }

  .digit-btn.has-data::after {
    content: '';
    position: absolute;
    top: 3px;
    right: 3px;
    width: 5px;
    height: 5px;
    background: var(--primary-hover);
    border-radius: 50%;
  }

  .digit-btn.active.has-data::after {
    background: var(--setzer-set-text-active);
  }

  .digit-btn.set-mode {
    border-color: var(--primary);
    animation: pulse-gold 1.2s infinite;
  }

  /* MIDI indicator */
  .has-midi {
    border-color: var(--midi-indicator) !important;
  }

  .has-midi::before {
    content: '';
    position: absolute;
    bottom: 2px;
    left: 50%;
    transform: translateX(-50%);
    width: 4px;
    height: 4px;
    background: var(--midi-indicator);
    border-radius: 50%;
  }

  /* Navigation buttons */
  .setzer-nav, .setzer-bank {
    display: flex;
    gap: 2px;
  }

  .nav-btn {
    font-size: 0.75rem;
    min-width: 32px;
  }

  /* Digital display */
  .setzer-display {
    min-width: 52px;
    height: 34px;
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--setzer-display-bg);
    border: 1px solid var(--setzer-border);
    border-radius: 3px;
    color: var(--setzer-display-fg);
    font-family: 'Courier New', monospace;
    font-size: 1.2rem;
    font-weight: 700;
    letter-spacing: 2px;
    text-shadow: 0 0 6px rgba(232, 160, 32, 0.5);
    padding: 0 0.25rem;
  }

  /* General Cancel button */
  .gc-btn {
    background: var(--setzer-gc-bg);
    border-color: var(--setzer-gc-border);
    color: var(--setzer-gc-text);
    font-weight: 700;
  }

  .gc-btn:hover {
    background: var(--setzer-gc-border);
    color: var(--text-on-primary);
  }

  /* Memory level selector */
  .setzer-memory {
    display: flex;
    align-items: center;
    gap: 2px;
    margin-left: 0.3rem;
  }
  .memory-label {
    color: var(--setzer-text-muted);
    font-size: 0.65rem;
    font-weight: 700;
    margin-right: 2px;
  }
  .memory-btn {
    min-width: 24px !important;
    height: 28px !important;
    padding: 0 !important;
    font-size: 0.7rem;
  }
  .memory-btn.active {
    background: var(--primary);
    border-color: var(--primary-hover);
    color: var(--text-on-primary);
  }

  /* Crescendo indicator */
  .cresc-indicator {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    margin-left: 0.6rem;
    padding: 0 0.5rem;
    height: 28px;
    background: var(--setzer-display-bg);
    border: 1px solid var(--setzer-border-strong);
    border-radius: 4px;
    cursor: pointer;
  }
  /* Ingesteld maar uitgeschakeld: gedimd tonen zodat zichtbaar is dat het er is. */
  .cresc-indicator.disabled {
    opacity: 0.45;
  }
  /* Bezig met crescendo-pedaal inleren: pulserende rand. */
  .cresc-indicator.learning {
    border-color: #ff5050;
    box-shadow: 0 0 6px rgba(255, 80, 80, 0.7);
    animation: cresc-learn-pulse 0.8s ease-in-out infinite;
  }
  @keyframes cresc-learn-pulse {
    0%, 100% { box-shadow: 0 0 4px rgba(255, 80, 80, 0.5); }
    50% { box-shadow: 0 0 10px rgba(255, 80, 80, 0.9); }
  }
  .cresc-label {
    color: var(--setzer-cresc-label);
    font-size: 0.7rem;
    font-weight: 700;
    letter-spacing: 0.05em;
  }
  .cresc-leds {
    display: flex;
    gap: 1px;
  }
  .cresc-led {
    width: 6px;
    height: 14px;
    background: var(--setzer-cresc-led-bg);
    border-radius: 1px;
    transition: background 0.05s ease;
  }
  .cresc-led.active {
    background: linear-gradient(to top, var(--setzer-cresc-led-on-start) 0%, var(--setzer-cresc-led-on-end) 100%);
    box-shadow: 0 0 4px rgba(255, 80, 80, 0.6);
  }
  .cresc-value {
    color: var(--setzer-cresc-value);
    font-family: 'Courier New', monospace;
    font-size: 0.72rem;
    font-weight: 700;
    min-width: 32px;
    text-align: right;
  }

  /* Context menu */
  .setzer-context-menu {
    position: fixed;
    z-index: 10000;
    background: var(--menu-bg);
    border: 1px solid var(--menu-border);
    border-radius: 4px;
    box-shadow: 0 4px 12px rgba(0,0,0,0.5);
    overflow: hidden;
  }

  .setzer-context-menu button {
    display: block;
    width: 100%;
    padding: 0.5rem 1rem;
    border: none;
    background: transparent;
    color: var(--menu-text);
    font-size: 0.8rem;
    cursor: pointer;
    text-align: left;
    white-space: nowrap;
  }

  .setzer-context-menu button:hover {
    background: var(--menu-hover-bg);
    color: var(--menu-hover-text);
  }
</style>
