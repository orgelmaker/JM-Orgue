<script>
  import { createEventDispatcher, onMount, onDestroy } from 'svelte';
  import { open } from '@tauri-apps/plugin-dialog';
  import LayoutSettings from './LayoutSettings.svelte';
  import VoicingPanel from './VoicingPanel.svelte';
  import MidiPlayer from './MidiPlayer.svelte';
  import { t, tx } from '../lib/i18n.js';
  import SetzerBar from './SetzerBar.svelte';
  import { midiLearn } from '../lib/midiLearn.js';
  import { invoke } from '@tauri-apps/api/core';

  export let organInfo = null;
  export let loading = false;
  export let loadingProgress = 0;
  export let loadingMessage = '';
  export let showOrganBrowser = true;
  export let midiMappings = [];
  export let midiConnected = false;
  export let sampleRate = 0;
  export let autostartEnabled = false;
  export let restoreRegistration = false;
  let supportedSampleRates = [];

  async function loadSupportedSampleRates() {
    try {
      supportedSampleRates = await invoke('query_supported_sample_rates');
    } catch (e) {
      console.warn('Kon ondersteunde sample rates niet ophalen:', e);
      supportedSampleRates = [];
    }
  }

  // ===== Sampleset silence tools =====
  let samplesetThresholdDb = -60;
  let samplesetPrerollMs = 5;
  let samplesetScan = null;   // ScanReport from backend
  let samplesetTrim = null;   // TrimReport from backend
  let samplesetBusy = false;

  function samplesetDir() {
    return organInfo?.id || '';
  }
  function fmtMs(ms) {
    if (ms == null) return '—';
    return ms >= 1000 ? `${(ms / 1000).toFixed(2)} s` : `${Math.round(ms)} ms`;
  }
  async function scanSampleset() {
    if (!samplesetDir()) return;
    samplesetBusy = true; samplesetTrim = null; samplesetScan = null;
    try {
      samplesetScan = await invoke('scan_sampleset_silence', {
        directory: samplesetDir(), thresholdDb: samplesetThresholdDb,
      });
    } catch (e) { alert(tx('settings.sampleset_error') + ': ' + e); }
    samplesetBusy = false;
  }
  async function trimSampleset() {
    if (!samplesetDir()) return;
    if (!confirm(tx('settings.sampleset_confirm'))) return;
    samplesetBusy = true;
    try {
      samplesetTrim = await invoke('trim_sampleset_silence', {
        directory: samplesetDir(), thresholdDb: samplesetThresholdDb, prerollMs: samplesetPrerollMs,
      });
      // Re-scan so the user sees the new state.
      samplesetScan = await invoke('scan_sampleset_silence', {
        directory: samplesetDir(), thresholdDb: samplesetThresholdDb,
      });
    } catch (e) { alert(tx('settings.sampleset_error') + ': ' + e); }
    samplesetBusy = false;
  }

  // ===== Sampleset loop tool =====
  let loopScan = null;     // LoopScanReport
  let loopApply = null;    // LoopApplyReport
  let loopBusy = false;
  let loopOnlyMissing = true;
  async function scanLoops() {
    if (!samplesetDir()) return;
    loopBusy = true; loopApply = null; loopScan = null;
    try {
      loopScan = await invoke('scan_sampleset_loops', { directory: samplesetDir() });
    } catch (e) { alert(tx('settings.sampleset_error') + ': ' + e); }
    loopBusy = false;
  }
  async function applyLoops() {
    if (!samplesetDir()) return;
    if (!confirm('Loops detecteren en in de WAV-bestanden schrijven? De originelen worden eerst geback-upt naar een "_loopbackup"-map.')) return;
    loopBusy = true;
    try {
      loopApply = await invoke('apply_sampleset_loops', {
        directory: samplesetDir(), onlyMissing: loopOnlyMissing,
      });
      loopScan = await invoke('scan_sampleset_loops', { directory: samplesetDir() });
    } catch (e) { alert(tx('settings.sampleset_error') + ': ' + e); }
    loopBusy = false;
  }
  export let activeView = 'orgel';
  export let audioDevices = [];
  export let audioHosts = [];
  export let selectedAudioHost = null;
  export let selectedBufferFrames = null;
  export let midiDevices = [];
  export let selectedAudioDevice = null;
  export let selectedMidiDevice = null;

  const dispatch = createEventDispatcher();

  // --- Audio device UX helpers ---------------------------------------------
  // ASIO drivers (bv. ASIO4ALL) leveren maar één apparaat; de keuze van de
  // fysieke uitgang (koptelefoon/luidspreker) gebeurt in het ASIO-paneel, niet
  // hier. Dat detecteren we om een gerichte hint te tonen.
  $: isAsioHost = /asio/i.test(selectedAudioHost || '');

  // Begrijpelijker label voor de host-keuze (waarde blijft de echte cpal-naam).
  function prettyHost(name) {
    const n = (name || '').toLowerCase();
    if (/asio/.test(n)) return `${name} (lage latency)`;
    if (/wasapi/.test(n)) return `${name} (Windows — per uitgang)`;
    return name || '—';
  }

  // Raad het type uitgang uit de apparaatnaam zodat we het juiste icoon tonen.
  function deviceKind(name) {
    const n = (name || '').toLowerCase();
    if (/(hoofdtelefoon|koptelefoon|headphone|headset|kopfh|casque|écouteur|ecouteur|earphone)/.test(n)) return 'headphone';
    if (/(luidspreker|speaker|lautsprecher|haut-parleur|haut parleur)/.test(n)) return 'speaker';
    return 'generic';
  }

  // Splits "Hoofdtelefoon (Realtek(R) Audio)" → naam "Hoofdtelefoon", sub "Realtek(R) Audio".
  function deviceLabel(name) {
    const m = (name || '').match(/^(.*?)\s*\(([^)]*(?:\([^)]*\)[^)]*)*)\)\s*$/);
    if (m && m[1].trim()) return m[1].trim();
    return name || '';
  }
  function deviceSub(name) {
    const m = (name || '').match(/^(.*?)\s*\(([^)]*(?:\([^)]*\)[^)]*)*)\)\s*$/);
    return m && m[1].trim() ? m[2].trim() : '';
  }

  // State for MIDI learning
  let learningDivision = null;
  let learningStep = 0;
  let learningTremDivIdx = null; // index van de divisie wiens Tremulant LFO toggle wordt ingeleerd

  // Action codes voor MIDI-learnable knoppen die geen stop/coupler zijn (zie SetzerBar voor 0-23).
  // Tremulant LFO toggle per divisie → 24..39 (max 16 divisies).
  // Algemene toggles: 40 (EQ enable), 41 (Crescendo enable).
  const ACTION_TREM_LFO_BASE = 24;
  const ACTION_EQ_ENABLE = 40;
  const ACTION_CRESCENDO_ENABLE = 41;
  let tremLfoMidiBindings = {}; // { actionCode: count }
  let globalMidiBindings = {};  // { actionCode: count } voor algemene toggles
  let globalLearningAction = null;

  // Handle MIDI-trigger acties die SetzerBar niet zelf afhandelt
  function handleExternalAction(actionCode) {
    if (actionCode >= ACTION_TREM_LFO_BASE && actionCode < ACTION_TREM_LFO_BASE + 16) {
      const divIdx = actionCode - ACTION_TREM_LFO_BASE;
      const div = displayOrgan?.divisions?.[divIdx];
      if (div) {
        setTremActive(div, !getTremActive(div.name));
      }
    } else if (actionCode === ACTION_EQ_ENABLE) {
      eqEnabled = !eqEnabled;
      updateEq();
    } else if (actionCode === ACTION_CRESCENDO_ENABLE) {
      crescendoEnabled = !crescendoEnabled;
      saveCrescendo();
    } else if (actionCode === 42) {
      // Computer afsluiten — bubbel naar App.svelte zodat die bevestiging + opslag regelt
      dispatch('shutdownRequest');
    }
  }

  async function learnGlobalAction(actionCode) {
    globalLearningAction = actionCode;
    try {
      await invoke('learn_preset_binding', { presetNum: actionCode });
      const bindings = await invoke('get_preset_bindings');
      globalMidiBindings[actionCode] = bindings.filter(b => b.preset_num === actionCode).length;
      globalMidiBindings = globalMidiBindings;
    } catch (e) {
      console.error('Global MIDI learn failed:', e);
    } finally {
      globalLearningAction = null;
    }
  }

  async function learnTremulantLfo(divIdx) {
    const actionCode = ACTION_TREM_LFO_BASE + divIdx;
    learningTremDivIdx = divIdx;
    try {
      await invoke('learn_preset_binding', { presetNum: actionCode });
      // Hertel bindings na succesvol leren
      const bindings = await invoke('get_preset_bindings');
      tremLfoMidiBindings[actionCode] = bindings.filter(b => b.preset_num === actionCode).length;
      tremLfoMidiBindings = tremLfoMidiBindings;
    } catch (e) {
      console.error('Tremulant MIDI learn failed:', e);
    } finally {
      learningTremDivIdx = null;
    }
  }

  // Tel bindings bij organ load
  $: if (organInfo) {
    invoke('get_preset_bindings').then(bindings => {
      const tremCounts = {};
      const globCounts = {};
      for (const b of bindings) {
        if (b.preset_num >= ACTION_TREM_LFO_BASE && b.preset_num < ACTION_TREM_LFO_BASE + 16) {
          tremCounts[b.preset_num] = (tremCounts[b.preset_num] || 0) + 1;
        } else if (b.preset_num === ACTION_EQ_ENABLE || b.preset_num === ACTION_CRESCENDO_ENABLE) {
          globCounts[b.preset_num] = (globCounts[b.preset_num] || 0) + 1;
        }
      }
      tremLfoMidiBindings = tremCounts;
      globalMidiBindings = globCounts;
    }).catch(() => {});
  }

  // Layout state for main view
  let mainLayout = 'horizontal';
  let stopSize = 100; // min-breedte registerknop in px (instelbaar)

  // MP3-recorder status (poll vanuit backend).
  let recorderStatus = { recording: false, seconds: 0, path: null, frames_dropped: 0, error: null };
  let recorderPoll = null;
  function formatSeconds(s) {
    const t = Math.max(0, Math.floor(s));
    const m = Math.floor(t / 60).toString().padStart(2, '0');
    const r = (t % 60).toString().padStart(2, '0');
    return `${m}:${r}`;
  }
  async function pollRecorder() {
    try { recorderStatus = await invoke('get_recording_status'); } catch (e) {}
  }
  async function toggleRecording() {
    try {
      if (recorderStatus.recording) {
        const final = await invoke('stop_recording');
        recorderStatus = final;
        if (final && final.path) {
          alert(`Opname opgeslagen:\n${final.path}\n(${formatSeconds(final.seconds)})`);
        }
      } else {
        const path = await invoke('start_recording', { path: null });
        recorderStatus = { recording: true, seconds: 0, path, frames_dropped: 0, error: null };
      }
    } catch (e) {
      alert(`Opname-fout: ${e}`);
    }
  }

  // MIDI-opname status (poll vanuit backend — live teller events + tijd).
  // Aparte keten van de MP3-opname: dit legt vast wát je speelde (noten/CC), niet de audio.
  let midiRec = { recording: false, event_count: 0, seconds: 0 };
  let midiRecPoll = null;
  let lastMidiPath = null; // pad van de laatst opgeslagen MIDI-opname (voor "in speler laden")

  async function pollMidiRec() {
    try { midiRec = await invoke('midi_recording_status'); } catch (e) {}
  }

  async function toggleMidiRecording() {
    try {
      if (midiRec.recording) {
        // Stoppen → aantal opgenomen events ophalen.
        const count = await invoke('stop_midi_recording');
        if (midiRecPoll) { clearInterval(midiRecPoll); midiRecPoll = null; }
        midiRec = { recording: false, event_count: count, seconds: midiRec.seconds };
        if (!count) {
          await invoke('clear_midi_recording');
          alert('Geen noten opgenomen — speel op het klavier (via MIDI) tijdens de opname.');
          return;
        }
        // Opslagdialoog, standaard in Documenten\JM-Orgue-opnames met tijdgestempelde naam.
        const suggested = await invoke('suggest_midi_recording_path');
        const { save } = await import('@tauri-apps/plugin-dialog');
        const path = await save({
          defaultPath: suggested,
          filters: [{ name: 'MIDI-bestand', extensions: ['mid'] }],
        });
        if (path) {
          await invoke('save_midi_recording', { path });
          lastMidiPath = path;
          alert(`MIDI-opname opgeslagen:\n${path}\n(${count} events)`);
        } else {
          // Geannuleerd → opname verwerpen zodat de status schoon is.
          await invoke('clear_midi_recording');
        }
      } else {
        // Starten — alleen noten (geen registratie); poll voor live teller.
        await invoke('start_midi_recording');
        lastMidiPath = null;
        midiRec = { recording: true, event_count: 0, seconds: 0 };
        if (midiRecPoll) clearInterval(midiRecPoll);
        midiRecPoll = setInterval(pollMidiRec, 300);
      }
    } catch (e) {
      alert(`MIDI-opname-fout: ${e}`);
    }
  }

  // Zojuist opgenomen take direct in de MIDI-speler laden en afspelen.
  // Let op: trek zelf de registers, anders blijft het orgel stil (opname = alleen noten).
  async function playLastMidi() {
    if (!lastMidiPath) return;
    try {
      await invoke('midi_play_file', { path: lastMidiPath });
    } catch (e) {
      alert(`Afspelen mislukt: ${e}`);
    }
  }
  let selectedDivisions = [];
  let openPanels = [];

  // Volume / Reverb / Tuning
  let volume = -6;
  let reverb = 30;
  import { temperaments } from '../temperaments.js';
  let selectedTemperament = 0; // index into temperaments array (0 = equal)
  let showCustomTemperament = false;
  let customCents = [0,0,0,0,0,0,0,0,0,0,0,0];
  const noteNames = ['C','C#','D','D#','E','F','F#','G','G#','A','A#','B'];

  // Load custom temperaments from localStorage
  try {
    const saved = localStorage.getItem('jm-orgue-custom-temperaments');
    if (saved) {
      const customs = JSON.parse(saved);
      for (const ct of customs) {
        temperaments.push(ct);
      }
    }
  } catch (e) {}

  function saveCustomTemperament() {
    const name = prompt('Naam voor deze stemming:');
    if (!name) return;
    const custom = {
      name, nameDutch: name, category: 'custom', period: 'Eigen', year: 2026,
      modernUse: true, description: 'Eigen stemming',
      cents: [...customCents],
    };
    temperaments.push(custom);
    selectedTemperament = temperaments.length - 1;
    // Save customs to localStorage
    const customs = temperaments.filter(t => t.category === 'custom');
    localStorage.setItem('jm-orgue-custom-temperaments', JSON.stringify(customs));
    onTemperamentChange();
    showCustomTemperament = false;
  }

  function applyCustomCents() {
    const name = temperaments[selectedTemperament]?.name || 'Aangepast';
    invoke('set_temperament', { noteOffsets: customCents, fineTune, name }).catch(console.error);
  }
  let fineTune = 0; // cents offset from A440
  // Crescendo state
  let crescendoEnabled = false;
  let crescendoStages = [];
  let crescendoStage = 0;
  let crescendoNumStages = 15;
  let crescendoLearning = false;
  let crescendoBinding = null;

  try {
    const saved = localStorage.getItem('jm-orgue-crescendo');
    if (saved) {
      const cfg = JSON.parse(saved);
      crescendoEnabled = cfg.enabled || false;
      crescendoStages = cfg.stages || [];
      crescendoNumStages = cfg.numStages || 15;
    }
  } catch (e) {}

  function getAllStopIds() {
    if (!displayOrgan) return [];
    return displayOrgan.divisions.flatMap(d => d.stops.map(s => s.id));
  }

  // Voethoogte (in voet) uit een pitch-string halen, bv. "8'", "16'", "2 2/3'", "4st".
  function pitchFeet(s) {
    const p = (s.pitch || '').toString();
    const m = p.match(/(\d+)(?:\s+(\d+)\/(\d+))?/);
    if (!m) return 8; // onbekend → behandel als 8'
    let feet = parseInt(m[1], 10) || 8;
    if (m[2] && m[3]) feet += parseInt(m[2], 10) / parseInt(m[3], 10);
    return feet;
  }
  // Familie-rang voor een musicale opbouw: grondstemmen eerst, tongwerken laatst.
  function crescFamily(s) {
    const n = (s.name || '').toLowerCase();
    if (s.is_reed || n.includes('trompet') || n.includes('hobo') || n.includes('bazuin') || n.includes('fagot') || n.includes('schalmei') || n.includes('kromhoorn') || n.includes('trumpet') || n.includes('clairon') || n.includes('dulciaan')) return 4; // tongwerken
    if (n.includes('mixtuur') || n.includes('cymbel') || n.includes('scherp') || n.includes('sesquialter') || n.includes('cornet') || n.includes('mixture')) return 3; // vulstemmen
    if (n.includes('prestant') || n.includes('principaal') || n.includes('principal') || n.includes('octaaf') || n.includes('octave') || n.includes('diapason') || n.includes('quint') || n.includes('nasard') || n.includes('terts') || n.includes('tierce')) return 2; // principalen + mutaties
    return 1; // fluiten/gedekt/strijkers (grondtoon)
  }

  function autoFillCrescendo() {
    if (!displayOrgan) return;
    // Musicale opbouw: per familie (grondtoon → principalen → vulstemmen → tongwerken),
    // binnen elke familie van laag (grote voet) naar hoog. Zo komt het crescendo geleidelijk
    // op: eerst zachte grondstemmen, dan de principaalkoor, dan mixturen, tongwerken als laatst.
    const all = [];
    for (const div of displayOrgan.divisions) {
      for (const stop of div.stops) all.push(stop);
    }
    all.sort((a, b) => {
      const fa = crescFamily(a), fb = crescFamily(b);
      if (fa !== fb) return fa - fb;
      return pitchFeet(b) - pitchFeet(a); // grote voet (laag) eerst
    });
    const orderedIds = all.map(s => s.id);

    crescendoStages = [];
    for (let i = 0; i < crescendoNumStages; i++) {
      const count = Math.ceil(((i + 1) / crescendoNumStages) * orderedIds.length);
      crescendoStages.push(orderedIds.slice(0, count)); // cumulatief, monotoon
    }
    saveCrescendo();
  }

  // Crescendo wordt PER ORGEL bewaard (stop-IDs verschillen per orgel). Sleutel op
  // een hash van het orgel-id; valt terug op de globale sleutel als er geen orgel is.
  function hashCode(str) {
    let h = 0;
    for (let i = 0; i < (str || '').length; i++) { h = ((h << 5) - h) + str.charCodeAt(i); h |= 0; }
    return Math.abs(h).toString(36);
  }
  function crescKey() {
    return organInfo && organInfo.id ? 'jm-orgue-crescendo-' + hashCode(organInfo.id) : 'jm-orgue-crescendo';
  }
  // Per-orgel localStorage-sleutel voor pure UI-voorkeuren (zwel/koppel-zichtbaarheid,
  // layout, knopgrootte). Zo lekken ze niet meer tussen orgels.
  function organUiKey(base) {
    return organInfo && organInfo.id ? base + '-' + hashCode(organInfo.id) : base;
  }
  // Lees een per-orgel UI-key; valt eenmalig terug op de oude globale key (migratie).
  function readOrganUiPref(base) {
    const v = localStorage.getItem(organUiKey(base));
    if (v !== null) return v;
    return localStorage.getItem(base); // legacy globale waarde (migratie naar dit orgel)
  }

  async function saveCrescendo() {
    localStorage.setItem(crescKey(), JSON.stringify({
      enabled: crescendoEnabled, stages: crescendoStages, numStages: crescendoNumStages
    }));
    try {
      await invoke('set_crescendo_config', { stages: crescendoStages, enabled: crescendoEnabled });
    } catch (e) {
      console.error('Failed to save crescendo:', e);
    }
  }

  // Laad de crescendo-config voor het huidige orgel en stuur die naar de backend.
  async function loadCrescendoForOrgan() {
    try {
      // Per-orgel sleutel; val eenmalig terug op de oude globale sleutel als migratie.
      const saved = localStorage.getItem(crescKey()) || localStorage.getItem('jm-orgue-crescendo');
      if (saved) {
        const cfg = JSON.parse(saved);
        crescendoEnabled = cfg.enabled || false;
        crescendoStages = cfg.stages || [];
        crescendoNumStages = cfg.numStages || 15;
      } else {
        // Geen config voor dit orgel: begin leeg i.p.v. de vorige organ-config hergebruiken.
        crescendoStages = [];
      }
    } catch (e) { crescendoStages = []; }
    try {
      await invoke('set_crescendo_config', { stages: crescendoStages, enabled: crescendoEnabled });
    } catch (e) { /* backend nog niet klaar — UI-state blijft leidend */ }
  }

  // ---- Crescendo-editor (matrix: rijen = registers, kolommen = stappen) ----
  // Zorg dat crescendoStages exact `crescendoNumStages` stappen heeft.
  function ensureCrescStages() {
    const arr = Array.isArray(crescendoStages) ? crescendoStages.map(s => Array.isArray(s) ? s.slice() : []) : [];
    while (arr.length < crescendoNumStages) arr.push([]);
    arr.length = crescendoNumStages;
    return arr;
  }
  function isStopInStage(stepIdx, stopId) {
    return Array.isArray(crescendoStages[stepIdx]) && crescendoStages[stepIdx].includes(stopId);
  }
  // Zet één cel (stap × register) aan/uit.
  function toggleCrescCell(stepIdx, stopId) {
    const arr = ensureCrescStages();
    const set = new Set(arr[stepIdx]);
    if (set.has(stopId)) set.delete(stopId); else set.add(stopId);
    arr[stepIdx] = Array.from(set);
    crescendoStages = arr;
    saveCrescendo();
  }
  // Vul een register cumulatief: aan vanaf deze stap t/m het einde (klassiek crescendo-gedrag).
  function fillStopFrom(stepIdx, stopId) {
    const arr = ensureCrescStages();
    const turnOn = !isStopInStage(stepIdx, stopId); // toggle-richting o.b.v. de aangeklikte stap
    for (let i = 0; i < arr.length; i++) {
      const set = new Set(arr[i]);
      if (i >= stepIdx && turnOn) set.add(stopId);
      else if (i >= stepIdx && !turnOn) set.delete(stopId);
      arr[i] = Array.from(set);
    }
    crescendoStages = arr;
    saveCrescendo();
  }
  function clearCrescendo() {
    crescendoStages = Array.from({ length: crescendoNumStages }, () => []);
    saveCrescendo();
  }

  async function setCrescendoStage(stage) {
    crescendoStage = stage;
    try {
      const targetStops = await invoke('set_crescendo_stage', { stage });
      if (targetStops && targetStops.length > 0) {
        // De backend zet hier alleen de trap-teller en geeft de doelregisters
        // terug; App.svelte past ze toe (on:crescendoChange → set_drawn_stops).
        dispatch('crescendoChange', targetStops);
      }
    } catch (e) {
      console.error('Failed to set crescendo stage:', e);
    }
  }

  async function learnCrescendoPedal() {
    crescendoLearning = true;
    try {
      const result = await invoke('learn_crescendo_pedal');
      if (result) {
        crescendoBinding = { channel: result[0], cc: result[1], min: 0, max: 127, invert: false };
      }
    } catch (e) {
      console.error('Failed to learn crescendo pedal:', e);
    }
    crescendoLearning = false;
  }

  async function toggleCrescendoInvert(invert) {
    try {
      await invoke('set_crescendo_invert', { invert });
      if (crescendoBinding) { crescendoBinding.invert = invert; crescendoBinding = crescendoBinding; }
    } catch (e) {
      console.error('Failed to toggle crescendo invert:', e);
    }
  }

  async function setCrescendoRangeUI(minVal, maxVal) {
    const mn = Math.max(0, Math.min(127, minVal | 0));
    const mx = Math.max(0, Math.min(127, maxVal | 0));
    try {
      await invoke('set_crescendo_range', { minVal: mn, maxVal: mx });
      if (crescendoBinding) { crescendoBinding.min = mn; crescendoBinding.max = mx; crescendoBinding = crescendoBinding; }
    } catch (e) {
      console.error('Failed to set crescendo range:', e);
    }
  }

  // Bij organ-load: huidige crescendo-binding (incl. min/max/invert) ophalen voor de UI.
  $: if (organInfo) {
    invoke('get_crescendo_binding').then(r => {
      if (Array.isArray(r) && r.length === 5) {
        crescendoBinding = { channel: r[0], cc: r[1], min: r[2], max: r[3], invert: !!r[4] };
      } else {
        crescendoBinding = null;
      }
    }).catch(() => {});
  }

  // Parametric EQ state
  let eqEnabled = false;
  let eqLowFreq = 200, eqLowGain = 0;
  let eqMidFreq = 1000, eqMidGain = 0, eqMidQ = 10;
  let eqHighFreq = 4000, eqHighGain = 0;

  // EQ wordt per orgel opgeslagen in de backend (.jm-settings.json) en bij load geladen
  // via loadAudioSettingsForOrgan() — niet meer in een globale localStorage-key.

  async function updateEq() {
    try {
      await invoke('set_parametric_eq', {
        enabled: eqEnabled,
        lowFreq: eqLowFreq, lowGain: eqLowGain,
        midFreq: eqMidFreq, midGain: eqMidGain, midQ: eqMidQ / 10,
        highFreq: eqHighFreq, highGain: eqHighGain,
      });
    } catch (e) {
      console.error('Failed to set EQ:', e);
    }
  }

  let reverbIrLoaded = false;
  let reverbIrName = '';
  let reverbType = 'convolution'; // 'convolution' or 'algorithmic'
  let reverbPreset = 1; // 0=Kleine Kapel, 1=Dorpskerk, etc.
  let reverbRt60 = 2.0;
  let reverbPreDelay = 25;
  let reverbDamping = 50;
  let reverbRoomSize = 80;

  // Reverb wordt per orgel opgeslagen in de backend (.jm-settings.json) en bij load geladen
  // via loadAudioSettingsForOrgan() — niet meer in een globale localStorage-key.
  // Bewaart de volledige reverb-configuratie (incl. mix) naar de backend voor per-orgel opslag.
  function persistReverbConfig() {
    invoke('persist_reverb_config', {
      reverbType,
      preset: reverbPreset,
      rt60: reverbRt60,
      preDelayMs: reverbPreDelay,
      damping: reverbDamping,
      roomSize: reverbRoomSize,
      mix: reverb,
      irPath: null,
    }).catch(console.error);
  }

  const reverbPresets = [
    { name: 'Kleine Kapel', rt60: 1.2, preDelay: 10, damping: 60, roomSize: 50 },
    { name: 'Dorpskerk', rt60: 2.0, preDelay: 25, damping: 50, roomSize: 80 },
    { name: 'Grote Kerk', rt60: 3.5, preDelay: 40, damping: 40, roomSize: 120 },
    { name: 'Kathedraal', rt60: 6.0, preDelay: 60, damping: 30, roomSize: 180 },
    { name: 'Gotische Kathedraal', rt60: 10.0, preDelay: 80, damping: 20, roomSize: 250 },
    { name: 'Concertzaal', rt60: 2.2, preDelay: 30, damping: 50, roomSize: 100 },
  ];

  async function updateAlgorithmicReverb(usePreset = false) {
    persistReverbConfig();
    try {
      await invoke('set_algorithmic_reverb', {
        preset: usePreset ? reverbPreset : null,
        rt60: reverbRt60,
        preDelayMs: reverbPreDelay,
        damping: reverbDamping / 100,
        roomSize: reverbRoomSize / 100,
        mix: reverb / 100,
      });
    } catch (e) {
      console.error('Failed to set algorithmic reverb:', e);
    }
  }

  async function switchReverbType(type) {
    reverbType = type;
    persistReverbConfig();
    try {
      await invoke('set_reverb_type', { algorithmic: type === 'algorithmic' });
      if (type === 'algorithmic') {
        await updateAlgorithmicReverb(true);
      }
    } catch (e) {
      console.error('Failed to switch reverb type:', e);
    }
  }

  function applyReverbPreset(idx) {
    reverbPreset = idx;
    const p = reverbPresets[idx];
    reverbRt60 = p.rt60;
    reverbPreDelay = p.preDelay;
    reverbDamping = p.damping;
    reverbRoomSize = p.roomSize;
    updateAlgorithmicReverb(true);
  }

  // Stuur de (opgeslagen) reverb-instellingen naar de audio-engine. Nodig bij
  // opstarten en na elke organ-(her)load: een verse audio-thread start altijd op
  // standaard (convolutie zonder IR, mix 0) = geen galm. Zonder dit hoorde je
  // niets ondanks de UI-keuze "Algoritmisch + preset".
  async function applyReverbToBackend() {
    try {
      await invoke('set_reverb_type', { algorithmic: reverbType === 'algorithmic' });
      if (reverbType === 'algorithmic') {
        await invoke('set_algorithmic_reverb', {
          preset: null,
          rt60: reverbRt60,
          preDelayMs: reverbPreDelay,
          damping: reverbDamping / 100,
          roomSize: reverbRoomSize / 100,
          mix: reverb / 100,
        });
      } else {
        // Convolutie: mix toepassen (IR moet de gebruiker zelf opnieuw laden).
        await invoke('set_reverb_mix', { mix: reverb / 100 });
      }
    } catch (e) {
      console.error('Failed to apply reverb to backend:', e);
    }
  }

  // Pas de reverb toe bij een échte laad-/wisselgebeurtenis (orgelnaam of type),
  // NIET bij elke organInfo-poll (300ms) — anders zou set_algorithmic_reverb de
  // FDN-delaylijnen telkens wissen en de galm-staart kapotmaken.
  let lastReverbAppliedKey = null;
  $: {
    const key = organInfo ? `${organInfo.name}|${reverbType}` : null;
    if (key && key !== lastReverbAppliedKey) {
      lastReverbAppliedKey = key;
      applyReverbToBackend();
    }
  }

  // Swell box (zwelkast) state
  let divisionVolumes = {};
  let swellBindings = {};
  let swellEnabled = {};
  let swellMinDb = {};      // per division: min dB when closed
  let swellFilterCutoff = {}; // per division: filter cutoff when closed (Hz)
  let learningSwellDivision = null;
  let swellLearnStep = 0;
  let swellPollInterval = null;

  // Zwel-config (min-dB + filter) wordt per orgel in de backend opgeslagen en in
  // loadAudioSettingsForOrgan() geladen — niet meer in een globale localStorage-key.

  function getSwellMinDb(div) { return swellMinDb[div] ?? -20; }
  function getSwellFilterCutoff(div) { return swellFilterCutoff[div] ?? 800; }

  async function updateSwellConfig(divisionName) {
    const minDb = getSwellMinDb(divisionName);
    const cutoff = getSwellFilterCutoff(divisionName);
    try {
      // set_swell_config schrijft de per-orgel opslag-spiegel in de backend.
      await invoke('set_swell_config', { division: divisionName, minDb, filterCutoff: cutoff });
    } catch (e) {
      console.error('Failed to set swell config:', e);
    }
  }

  // (Verouderd per-divisie wind-model verwijderd — het actieve systeem is de wind-GROEP-config
  // hieronder. De oude globale key 'jm-orgue-wind-config' wordt niet meer gebruikt.)

  // Stereo pan per division (-100..100). Per orgel opgeslagen in de backend (.jm-settings.json);
  // geladen in loadAudioSettingsForOrgan() — niet meer in een globale localStorage-key.
  let divisionPans = {};

  function getDivisionPan(div) { return divisionPans[div] ?? 0; }
  function formatPan(v) {
    if (v === 0) return 'C';
    if (v < 0) return `L${Math.abs(v)}`;
    return `R${v}`;
  }
  function onPanChange(div, value) {
    divisionPans[div] = value;
    divisionPans = divisionPans;
    // set_division_pan schrijft de per-orgel opslag-spiegel in de backend (geen localStorage meer).
    invoke('set_division_pan', { division: div, pan: value / 100 }).catch(() => {});
  }

  // ============ Wind-groep toewijzing per divisie ============
  // Default: divisie i → groep i (eigen wind-reservoir). Twee divisies met dezelfde
  // groep delen één reservoir (gecombineerd voice-count).
  // Wind-groep toewijzing, aantal groepen en groep-config worden per orgel in de backend
  // bewaard en in loadAudioSettingsForOrgan() geladen — niet meer in globale localStorage-keys.
  let divisionWindGroups = {};  // { divisionName: groupIndex (0-based) }
  let numWindGroups = 8;
  let windGroupConfig = {};     // { groupIdx: { enabled, reservoir, damping, maxSag } }

  invoke('get_num_wind_groups').then(n => {
    if (typeof n === 'number' && n >= 1 && n <= 8) numWindGroups = n;
  }).catch(() => {});

  function getWindGroup(div, divIdx) {
    const v = divisionWindGroups[div];
    if (typeof v === 'number') return Math.min(v, numWindGroups - 1);
    return Math.min(divIdx, numWindGroups - 1);
  }
  function onWindGroupChange(div, value) {
    const v = parseInt(value, 10);
    divisionWindGroups[div] = v;
    divisionWindGroups = divisionWindGroups;
    // set_division_wind_group schrijft de per-orgel state in de backend (geen localStorage meer).
    invoke('set_division_wind_group', { division: div, group: v }).catch(console.error);
    // Reapply de wind-config van de NIEUWE groep op deze divisie (zodat de slider-waarden kloppen)
    pushWindGroupConfig(v);
  }

  function onNumWindGroupsChange(value) {
    const n = parseInt(value, 10);
    numWindGroups = n;
    invoke('set_num_wind_groups', { count: n }).catch(console.error);
  }

  function getWindGroupEnabled(g) { return windGroupConfig[g]?.enabled === true; }
  function getWindGroupReservoir(g) { return windGroupConfig[g]?.reservoir ?? 0.5; }
  function getWindGroupDamping(g) { return windGroupConfig[g]?.damping ?? 0.5; }
  function getWindGroupMaxSag(g) { return windGroupConfig[g]?.maxSag ?? 10; }

  function saveWindGroupConfig(groupIdx) {
    // Per orgel opslaan in de backend. Bewaar de hele config of (indien meegegeven) één groep.
    const groups = groupIdx === undefined ? Object.keys(windGroupConfig) : [groupIdx];
    for (const gk of groups) {
      const g = parseInt(gk, 10);
      const cfg = windGroupConfig[g];
      if (!cfg) continue;
      invoke('persist_wind_group_config', {
        group: g,
        enabled: cfg.enabled === true,
        reservoirSize: cfg.reservoir ?? 0.5,
        damping: cfg.damping ?? 0.5,
        maxSag: cfg.maxSag ?? 10,
      }).catch(console.error);
    }
  }

  /// Stuur de wind-config van een groep naar de audio thread.
  /// Backend `set_wind_model` neemt een divisie-naam aan, maar onder de motorkap stuurt het
  /// naar de groep waaraan die divisie is toegewezen. We pakken dus een willekeurige divisie
  /// uit deze groep en sturen daarmee.
  function pushWindGroupConfig(groupIdx) {
    if (!organInfo?.divisions) return;
    const divInGroup = organInfo.divisions.find((d, i) => getWindGroup(d.name, i) === groupIdx);
    if (!divInGroup) return;
    const cfg = windGroupConfig[groupIdx] ?? { enabled: false, reservoir: 0.5, damping: 0.5, maxSag: 10 };
    invoke('set_wind_model', {
      division: divInGroup.name,
      enabled: cfg.enabled === true,
      reservoirSize: cfg.reservoir,
      damping: cfg.damping,
      maxSag: cfg.maxSag / 100,
    }).catch(console.error);
  }

  function updateWindGroup(groupIdx, patch) {
    windGroupConfig[groupIdx] = { ...(windGroupConfig[groupIdx] ?? { enabled: false, reservoir: 0.5, damping: 0.5, maxSag: 10 }), ...patch };
    windGroupConfig = windGroupConfig;
    saveWindGroupConfig(groupIdx);
    pushWindGroupConfig(groupIdx);
  }

  // Pas de opgeslagen divisie→wind-groep toewijzing toe vanuit de backend (per orgel).
  // De groep-config + UI-waarden worden in loadAudioSettingsForOrgan() geladen en toegepast.
  $: if (organInfo) {
    invoke('apply_saved_wind_groups').catch(() => {});
  }

  // ============ Multi-channel surround + C/Cis-lade spreiding ============
  // Per divisie een vrije set fysieke output-kanalen (1 t/m alle). Leeg = standaard voorste paar.
  let divisionOutputChannels = {}; // { divName: [chIdx, ...] }
  let divisionCcis = {};           // { divName: bool } — C/Cis-spreiding aan/uit
  let ccisStrength = 70;           // 0..100 (UI) → 0..1 backend
  let ccisFalloff = 60;            // 0..100 (UI) → 0..1 backend
  let ccisSwap = false;
  let audioChannelCount = 2;       // wordt vanuit backend gelezen

  // Output-kanalen, C/Cis-aan/uit per divisie en C/Cis-spreiding worden per orgel in de backend
  // bewaard (apply_saved_output_channels) en in loadAudioSettingsForOrgan() voor de UI geladen —
  // niet meer in globale localStorage-keys.

  invoke('query_audio_channel_count').then(c => { audioChannelCount = c; }).catch(() => {});

  // Vriendelijke kanaalnamen volgens de gangbare WASAPI/7.1-ordening.
  function channelLabel(ch, total) {
    if (total <= 2) return ['Links', 'Rechts'][ch] ?? `Kanaal ${ch + 1}`;
    const names = ['Voor L', 'Voor R', 'Center', 'LFE', 'Achter L', 'Achter R', 'Zij L', 'Zij R'];
    return names[ch] ?? `Kanaal ${ch + 1}`;
  }

  function getDivisionChannels(div) {
    const c = divisionOutputChannels[div];
    return Array.isArray(c) ? c : [];
  }
  function isChannelSelected(div, ch) { return getDivisionChannels(div).includes(ch); }
  function toggleDivisionChannel(div, ch) {
    let list = getDivisionChannels(div).slice();
    if (list.includes(ch)) list = list.filter(x => x !== ch);
    else { list.push(ch); list.sort((a, b) => a - b); }
    divisionOutputChannels[div] = list;
    divisionOutputChannels = divisionOutputChannels;
    // set_division_output_channels schrijft de per-orgel state in de backend (geen localStorage meer).
    invoke('set_division_output_channels', { division: div, channels: list }).catch(() => {});
  }

  function getDivisionCcis(div) { return divisionCcis[div] === true; }
  function toggleDivisionCcis(div) {
    const v = !getDivisionCcis(div);
    divisionCcis[div] = v;
    divisionCcis = divisionCcis;
    invoke('set_division_ccis', { division: div, enabled: v }).catch(() => {});
  }
  function applyCcisSpread() {
    invoke('set_ccis_spread', { strength: ccisStrength / 100, falloff: ccisFalloff / 100, swap: ccisSwap }).catch(() => {});
  }
  // (Toepassen + laden bij orgelwissel gebeurt in loadAudioSettingsForOrgan via
  //  apply_saved_output_channels + get_division_output_channels/get_division_ccis/get_ccis_spread.)

  // Bluetooth LE MIDI state
  let bleDevices = [];
  let bleScanning = false;
  let bleScanComplete = false;
  let bleMessageCount = 0;
  let bleCountInterval = null;

  onMount(() => {
    // Poll BLE message counter every 250ms
    bleCountInterval = setInterval(async () => {
      try {
        bleMessageCount = await invoke('get_ble_midi_count');
      } catch (e) {}
    }, 250);
    return () => { if (bleCountInterval) clearInterval(bleCountInterval); };
  });

  async function scanBleMidi() {
    bleScanning = true;
    bleScanComplete = false;
    try {
      const devices = await invoke('scan_ble_midi');
      bleDevices = devices;
      bleScanComplete = true;
    } catch (e) {
      console.error('BLE scan failed:', e);
      alert('Bluetooth scan mislukt: ' + e);
    }
    bleScanning = false;
  }

  async function toggleBleDevice(device) {
    try {
      if (device.connected) {
        await invoke('disconnect_ble_midi', { deviceId: device.id });
      } else {
        await invoke('connect_ble_midi', { deviceId: device.id });
      }
      // Refresh list
      const connected = await invoke('get_connected_ble_midi');
      const connectedIds = new Set(connected.map(c => c[0]));
      bleDevices = bleDevices.map(d => ({ ...d, connected: connectedIds.has(d.id) }));
    } catch (e) {
      console.error('BLE toggle failed:', e);
      alert('Bluetooth verbinding mislukt: ' + e);
    }
  }

  // Tremulant state per division.
  //  - tremLfoEnabled: "tremulant beschikbaar op dit klavier" (ingesteld in
  //    Instellingen, persistent). Bepaalt of er een Tremulant-register verschijnt.
  //  - tremActive: live aan/uit (bediend via het Tremulant-register; niet persistent,
  //    start uit na laden — net als koppels).
  //  - rate/ampDepth/pitchDepth: LFO-parameters (persistent).
  // tremLfoEnabled/Rate/AmpDepth/PitchDepth worden per orgel in de backend bewaard en in
  // loadAudioSettingsForOrgan() geladen — niet meer in een globale localStorage-key.
  let tremLfoEnabled = {};
  let tremActive = {};
  let tremLfoRate = {};
  let tremLfoAmpDepth = {};
  let tremLfoPitchDepth = {};

  function getTremEnabled(div) { return tremLfoEnabled[div] === true; }
  function getTremActive(div) { return tremActive[div] === true; }
  function getTremRate(div) { return tremLfoRate[div] ?? 6.0; }
  function getTremAmpDepth(div) { return tremLfoAmpDepth[div] ?? 10; }
  function getTremPitchDepth(div) { return tremLfoPitchDepth[div] ?? 15; }

  // Toon een Tremulant-register als de divisie tremulant-samples heeft óf als
  // LFO-tremulant voor dit klavier is ingeschakeld in de instellingen.
  function tremAvailable(division) {
    return division.stops.some(s => s.has_tremulant) || getTremEnabled(division.name);
  }

  function persistTrem() {
    // Per orgel opslaan in de backend (per divisie). De live aan/uit-stand (tremActive)
    // wordt NIET bewaard. amp/pitch in UI-eenheden — set_tremulant_lfo doet de /100 voor audio.
    for (const div of Object.keys(tremLfoEnabled)) {
      invoke('persist_tremulant_config', {
        division: div,
        enabled: tremLfoEnabled[div] === true,
        rate: tremLfoRate[div] ?? 6.0,
        ampDepth: tremLfoAmpDepth[div] ?? 10,
        pitchDepth: tremLfoPitchDepth[div] ?? 15,
      }).catch(() => {});
    }
  }

  // Bij het laden van een orgel: maak de LFO-tremulant standaard beschikbaar voor
  // divisies die in de bron (ODF) een tremulant hebben (DivisionDto.has_tremulant),
  // tenzij de gebruiker hier al een keuze voor maakte. Zo "mist" een geïmporteerd
  // orgel zijn tremulant niet meer.
  $: if (organInfo) {
    let tremChanged = false;
    for (const div of (organInfo.divisions || [])) {
      if (div.has_tremulant && tremLfoEnabled[div.name] === undefined) {
        tremLfoEnabled[div.name] = true;
        tremChanged = true;
      }
    }
    if (tremChanged) { tremLfoEnabled = tremLfoEnabled; persistTrem(); }
  }

  // Zet de tremulant live aan/uit voor een divisie (vanuit het register of MIDI).
  // Stuurt sample-tremulant (als de divisie trem-samples heeft) én LFO-synthese
  // (als die is ingeschakeld) aan/uit.
  async function setTremActive(division, val) {
    tremActive[division.name] = val;
    tremActive = tremActive;
    if (division.stops.some(s => s.has_tremulant)) {
      dispatch('setTremulant', { division: division.name, active: val });
    }
    // Bij UITzetten altijd het stop-commando sturen, ook als de beschikbaarheid
    // net is uitgezet (anders blijft de LFO-tremulant in de backend doorlopen).
    // Bij AANzetten alleen als de tremulant beschikbaar is.
    if (getTremEnabled(division.name) || !val) {
      try {
        await invoke('set_tremulant_lfo', {
          division: division.name,
          active: val,
          rate: getTremRate(division.name),
          ampDepth: getTremAmpDepth(division.name) / 100.0,
          pitchDepth: getTremPitchDepth(division.name),
        });
      } catch (e) {
        console.error('Failed to set tremulant LFO:', e);
      }
    }
  }

  // Schakel LFO-tremulant beschikbaarheid in/uit (Instellingen). Bij uitzetten
  // ook de live tremulant stoppen.
  function setTremEnabled(division, val) {
    tremLfoEnabled[division.name] = val;
    tremLfoEnabled = tremLfoEnabled;
    persistTrem();
    if (!val && getTremActive(division.name)) {
      setTremActive(division, false);
    }
  }

  // Live bijwerken van LFO-parameters (sliders in Instellingen).
  async function updateTremLfoParams(divisionName) {
    persistTrem();
    if (getTremActive(divisionName)) {
      try {
        await invoke('set_tremulant_lfo', {
          division: divisionName,
          active: true,
          rate: getTremRate(divisionName),
          ampDepth: getTremAmpDepth(divisionName) / 100.0,
          pitchDepth: getTremPitchDepth(divisionName),
        });
      } catch (e) {
        console.error('Failed to update tremulant LFO:', e);
      }
    }
  }

  // Zwel- en koppel-zichtbaarheid worden per orgel bewaard (organUiKey) en in
  // loadUiPrefsForOrgan() geladen — niet meer in een globale localStorage-key.
  let visibleCouplers = {};

  function isCouplerVisible(couplerId) {
    // Default: hidden — koppels worden niet automatisch getoond bij een nieuw
    // orgel. De gebruiker zet zelf in de instellingen aan welke koppels zichtbaar
    // moeten zijn (visibleCouplers[id] === true).
    return visibleCouplers[couplerId] === true;
  }

  function toggleCouplerVisibility(couplerId) {
    visibleCouplers[couplerId] = !isCouplerVisible(couplerId);
    visibleCouplers = visibleCouplers;
    localStorage.setItem(organUiKey('jm-orgue-visible-couplers'), JSON.stringify(visibleCouplers));
  }

  function isSwellEnabled(divisionName) {
    return swellEnabled[divisionName] === true;
  }

  function toggleSwellEnabled(divisionName) {
    swellEnabled[divisionName] = !swellEnabled[divisionName];
    swellEnabled = swellEnabled;
    localStorage.setItem(organUiKey('jm-orgue-swell-enabled'), JSON.stringify(swellEnabled));
    // Reset volume to 100% when disabling
    if (!swellEnabled[divisionName]) {
      setSwellLevel(divisionName, 1.0);
      // Clear any existing binding
      clearSwellBinding(divisionName);
    }
  }

  // Laad per-orgel UI-voorkeuren (zwel/koppel-zichtbaarheid + layout/knopgrootte) en zet de
  // bron-defaults (zwelkast aan voor has_swell-divisies; echte koppels standaard zichtbaar).
  // Vervangt de oude globale localStorage-keys; valt eenmalig terug op de globale waarde (migratie).
  function loadUiPrefsForOrgan() {
    if (!organInfo) return;
    // Zwel-zichtbaarheid
    try { swellEnabled = JSON.parse(readOrganUiPref('jm-orgue-swell-enabled') || '{}'); } catch (e) { swellEnabled = {}; }
    for (const div of (organInfo.divisions || [])) {
      if (div.has_swell && swellEnabled[div.name] === undefined) swellEnabled[div.name] = true;
    }
    swellEnabled = swellEnabled;
    localStorage.setItem(organUiKey('jm-orgue-swell-enabled'), JSON.stringify(swellEnabled));
    // Koppel-zichtbaarheid
    try { visibleCouplers = JSON.parse(readOrganUiPref('jm-orgue-visible-couplers') || '{}'); } catch (e) { visibleCouplers = {}; }
    for (const c of (organInfo.couplers || [])) {
      if (c.id && c.id.startsWith('real_coupler_') && visibleCouplers[c.id] === undefined) visibleCouplers[c.id] = true;
    }
    visibleCouplers = visibleCouplers;
    localStorage.setItem(organUiKey('jm-orgue-visible-couplers'), JSON.stringify(visibleCouplers));
    // Layout + knopgrootte (puur UI, per orgel)
    const ml = readOrganUiPref('jm-orgue-main-layout');
    if (ml === 'horizontal' || ml === 'vertical') mainLayout = ml;
    const ss = parseInt(readOrganUiPref('jm-orgue-stop-size'), 10);
    if (ss >= 70 && ss <= 220) stopSize = ss;
  }

  async function pollDivisionVolumes() {
    if (!organInfo) return;
    try {
      const volumes = await invoke('get_division_volumes');
      for (const v of volumes) {
        divisionVolumes[v.division] = v.volume;
      }
      divisionVolumes = divisionVolumes;
    } catch (e) {}
  }

  async function loadSwellBindings() {
    try {
      const bindings = await invoke('get_swell_bindings');
      swellBindings = {};
      for (const b of bindings) {
        swellBindings[b.division] = b;
      }
      swellBindings = swellBindings;
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

  async function learnSwellPedal(divisionName) {
    if (!midiConnected) {
      alert('Verbind eerst een MIDI apparaat!');
      return;
    }
    learningSwellDivision = divisionName;
    swellLearnStep = 1;
    try {
      const result = await invoke('learn_swell_pedal', { division: divisionName });
      if (result) {
        await loadSwellBindings();
      }
    } catch (e) {
      console.error('Swell learn failed:', e);
    } finally {
      learningSwellDivision = null;
      swellLearnStep = 0;
    }
  }

  async function clearSwellBinding(divisionName) {
    try {
      await invoke('clear_swell_binding', { division: divisionName });
      delete swellBindings[divisionName];
      swellBindings = swellBindings;
    } catch (e) {
      console.error('Failed to clear swell binding:', e);
    }
  }

  async function toggleSwellInvert(divisionName, invert) {
    try {
      await invoke('set_swell_invert', { division: divisionName, invert });
      if (swellBindings[divisionName]) {
        swellBindings[divisionName].invert = invert;
        swellBindings = swellBindings;
      }
    } catch (e) {
      console.error('Failed to toggle swell invert:', e);
    }
  }

  async function setSwellRangeUI(divisionName, minVal, maxVal) {
    const mn = Math.max(0, Math.min(127, minVal | 0));
    const mx = Math.max(0, Math.min(127, maxVal | 0));
    try {
      await invoke('set_swell_range', { division: divisionName, minVal: mn, maxVal: mx });
      if (swellBindings[divisionName]) {
        swellBindings[divisionName].min_val = mn;
        swellBindings[divisionName].max_val = mx;
        swellBindings = swellBindings;
      }
    } catch (e) {
      console.error('Failed to set swell range:', e);
    }
  }

  function onVolumeChange() {
    dispatch('volumeChange', volume);
  }

  function onReverbChange() {
    dispatch('reverbChange', reverb / 100);
    persistReverbConfig(); // mix maakt deel uit van de per-orgel reverb-config
  }

  async function onTemperamentChange() {
    try {
      const t = temperaments[selectedTemperament];
      // name wordt per orgel opgeslagen zodat de keuze bij herladen hersteld kan worden.
      await invoke('set_temperament', { noteOffsets: t.cents, fineTune: fineTune, name: t.name });
    } catch (e) {
      console.error('Failed to set temperament:', e);
    }
  }

  // Laad per-orgel audio-instellingen (master volume + temperament-keuze + fijnstemming) uit
  // de backend (.jm-settings.json) en pas ze toe. Vervangt de oude globale/runtime-bron zodat
  // deze instellingen niet meer tussen orgels lekken en na herstart bewaard blijven.
  // Wordt 1x per echte orgelwissel aangeroepen (zie reactive op organInfo.id).
  async function loadAudioSettingsForOrgan() {
    try {
      const s = await invoke('get_organ_settings');
      // Master volume: opgeslagen waarde, anders de UI-default. Push altijd zodat audio +
      // opslag synchroon lopen met de slider (ook bij default bij een nieuw orgel).
      if (s && typeof s.master_volume_db === 'number') {
        volume = s.master_volume_db;
      }
      dispatch('volumeChange', volume);
      // Temperament: zoek de opgeslagen stemming terug in de lijst (op naam, anders op cents).
      if (s && s.temperament) {
        const t = s.temperament;
        fineTune = t.fine_tune_cents || 0;
        let idx = -1;
        if (t.name) idx = temperaments.findIndex(x => x.name === t.name);
        if (idx < 0 && Array.isArray(t.custom_cents)) {
          idx = temperaments.findIndex(x => Array.isArray(x.cents) && x.cents.length === 12
            && x.cents.every((c, i) => Math.abs(c - t.custom_cents[i]) < 0.01));
        }
        if (idx >= 0) {
          selectedTemperament = idx;
          await invoke('set_temperament', { noteOffsets: temperaments[idx].cents, fineTune, name: temperaments[idx].name });
        } else if (Array.isArray(t.custom_cents)) {
          customCents = [...t.custom_cents];
          await invoke('set_temperament', { noteOffsets: t.custom_cents, fineTune, name: t.name || 'Aangepast' });
        }
      } else {
        // Geen opgeslagen stemming → default (gelijkzwevend) toepassen i.p.v. die van het vorige orgel.
        selectedTemperament = 0;
        fineTune = 0;
        const t0 = temperaments[0];
        await invoke('set_temperament', { noteOffsets: t0.cents, fineTune: 0, name: t0.name });
      }

      // Parametric EQ: opgeslagen waarde of default (uit). Stored mid_q is gedeeld door 10.
      if (s && s.eq) {
        const e = s.eq;
        eqEnabled = !!e.enabled;
        eqLowFreq = e.low_freq; eqLowGain = e.low_gain;
        eqMidFreq = e.mid_freq; eqMidGain = e.mid_gain; eqMidQ = (e.mid_q ?? 1) * 10;
        eqHighFreq = e.high_freq; eqHighGain = e.high_gain;
      } else {
        eqEnabled = false;
        eqLowFreq = 200; eqLowGain = 0;
        eqMidFreq = 1000; eqMidGain = 0; eqMidQ = 10;
        eqHighFreq = 4000; eqHighGain = 0;
      }
      await updateEq();

      // Reverb: opgeslagen volledige configuratie of default. applyReverbToBackend() past
      // het toe met de afgestemde logica (geen galmstaart wissen tijdens poll).
      if (s && s.reverb) {
        const r = s.reverb;
        reverbType = r.reverb_type || 'convolution';
        reverbPreset = (r.algorithmic_preset ?? 1);
        reverbRt60 = r.rt60 ?? 2.0;
        reverbPreDelay = r.pre_delay_ms ?? 25;
        reverbDamping = r.damping ?? 50;
        reverbRoomSize = r.room_size ?? 80;
        reverb = r.mix ?? 30;
      } else {
        reverbType = 'convolution';
        reverbPreset = 1; reverbRt60 = 2.0; reverbPreDelay = 25;
        reverbDamping = 50; reverbRoomSize = 80; reverb = 30;
      }
      await applyReverbToBackend();

      // ===== Per-divisie DSP (pan, zwel-config, tremulant, wind-groep) per orgel uit backend =====
      // Pan: backend slaat -1..1 op; UI gebruikt -100..100. Push past de audio toe.
      divisionPans = {};
      for (const p of (s?.division_pans || [])) {
        divisionPans[p.division] = Math.round(p.pan * 100);
        invoke('set_division_pan', { division: p.division, pan: p.pan }).catch(() => {});
      }
      divisionPans = divisionPans;

      // Zwel-config (min-dB + filter): zelfde eenheden als de UI; push past toe.
      swellMinDb = {}; swellFilterCutoff = {};
      for (const sc of (s?.division_swell_configs || [])) {
        swellMinDb[sc.division] = sc.min_db;
        swellFilterCutoff[sc.division] = sc.filter_cutoff;
        invoke('set_swell_config', { division: sc.division, minDb: sc.min_db, filterCutoff: sc.filter_cutoff }).catch(() => {});
      }
      swellMinDb = swellMinDb; swellFilterCutoff = swellFilterCutoff;

      // Tremulant: alleen de UI-maps herstellen (beschikbaar + parameters). De audio-params
      // worden bij het live activeren toegepast. Daarna vullen de has_tremulant-defaults aan.
      tremLfoEnabled = {}; tremLfoRate = {}; tremLfoAmpDepth = {}; tremLfoPitchDepth = {};
      for (const t of (s?.division_tremulants || [])) {
        tremLfoEnabled[t.division] = t.enabled;
        tremLfoRate[t.division] = t.rate;
        tremLfoAmpDepth[t.division] = t.amp_depth;
        tremLfoPitchDepth[t.division] = t.pitch_depth;
      }
      // Bron-default: LFO-tremulant beschikbaar voor has_tremulant-divisies die nog niet bewaard zijn.
      for (const div of (organInfo?.divisions || [])) {
        if (div.has_tremulant && tremLfoEnabled[div.name] === undefined) tremLfoEnabled[div.name] = true;
      }
      tremLfoEnabled = tremLfoEnabled; tremLfoRate = tremLfoRate;
      tremLfoAmpDepth = tremLfoAmpDepth; tremLfoPitchDepth = tremLfoPitchDepth;
      persistTrem();

      // Wind-groep toewijzing (UI) uit backend + aantal groepen + per-groep config.
      try {
        const wg = await invoke('get_division_wind_groups');
        const n = await invoke('get_num_wind_groups');
        if (typeof n === 'number' && n >= 1 && n <= 8) numWindGroups = n;
        divisionWindGroups = {};
        if (Array.isArray(wg) && organInfo?.divisions) {
          organInfo.divisions.forEach((d, i) => { if (wg[i] !== undefined) divisionWindGroups[d.name] = wg[i]; });
        }
        divisionWindGroups = divisionWindGroups;
      } catch (e) {}
      windGroupConfig = {};
      for (const w of (s?.wind_group_configs || [])) {
        windGroupConfig[w.group] = {
          enabled: w.enabled, reservoir: w.reservoir_size, damping: w.damping, maxSag: w.max_sag,
        };
      }
      windGroupConfig = windGroupConfig;
      for (const gk of Object.keys(windGroupConfig)) pushWindGroupConfig(parseInt(gk, 10));

      // Output-kanalen + C/Cis: apply_saved past de backend-audio toe; lees daarna voor de UI.
      try { await invoke('apply_saved_output_channels'); } catch (e) {}
      try {
        const chans = await invoke('get_division_output_channels');
        const ccisArr = await invoke('get_division_ccis');
        const spread = await invoke('get_ccis_spread');
        divisionOutputChannels = {}; divisionCcis = {};
        if (organInfo?.divisions) {
          organInfo.divisions.forEach((d, i) => {
            if (Array.isArray(chans?.[i]) && chans[i].length) divisionOutputChannels[d.name] = chans[i];
            if (ccisArr?.[i] === true) divisionCcis[d.name] = true;
          });
        }
        divisionOutputChannels = divisionOutputChannels; divisionCcis = divisionCcis;
        if (Array.isArray(spread)) {
          ccisStrength = Math.round((spread[0] ?? 0.7) * 100);
          ccisFalloff = Math.round((spread[1] ?? 0.6) * 100);
          ccisSwap = !!spread[2];
        }
      } catch (e) {}
    } catch (e) {
      console.error('loadAudioSettingsForOrgan failed:', e);
    }
  }

  async function exportSettings() {
    try {
      const { save } = await import('@tauri-apps/plugin-dialog');
      const path = await save({
        filters: [{ name: 'JM-Orgue Instellingen', extensions: ['json'] }],
        defaultPath: 'jm-orgue-instellingen.json',
      });
      if (path) {
        await invoke('export_settings', { path });
      }
    } catch (e) {
      console.error('Export failed:', e);
    }
  }

  async function importSettings() {
    try {
      const { open } = await import('@tauri-apps/plugin-dialog');
      const path = await open({
        filters: [{ name: 'JM-Orgue Instellingen', extensions: ['json'] }],
        multiple: false,
      });
      if (path) {
        await invoke('import_settings', { path });
        // Refresh organ info to apply imported settings
        dispatch('refresh');
      }
    } catch (e) {
      console.error('Import failed:', e);
    }
  }

  async function loadReverbIR() {
    try {
      const selected = await open({
        title: tx('settings.reverb_ir_dialog_title'),
        filters: [{ name: 'WAV bestanden', extensions: ['wav'] }],
        multiple: false,
      });
      if (selected) {
        await invoke('load_impulse_response', { path: selected });
        reverbIrLoaded = true;
        reverbIrName = selected.split(/[/\\]/).pop();
      }
    } catch (e) {
      console.error('Failed to load IR:', e);
    }
  }

  onMount(() => {
    // Layout + knopgrootte worden per orgel geladen in loadUiPrefsForOrgan(); hier alleen
    // een neutrale begin-default vóórdat een orgel geladen is.
    window.addEventListener('click', handleGlobalClick);
    loadSwellBindings();
    swellPollInterval = setInterval(pollDivisionVolumes, 100);
    pollRecorder();
    recorderPoll = setInterval(pollRecorder, 500);
    crescLivePoll = setInterval(pollCrescLive, 100);
  });

  onDestroy(() => {
    window.removeEventListener('click', handleGlobalClick);
    if (swellPollInterval) clearInterval(swellPollInterval);
    if (recorderPoll) clearInterval(recorderPoll);
    if (midiRecPoll) clearInterval(midiRecPoll);
    if (crescLivePoll) clearInterval(crescLivePoll);
  });

  // Live crescendo-pedaalstand volgen: werkt de balk bij terwijl de gebruiker het
  // pedaal beweegt (of min/max instelt), zodat je direct ziet wat er gebeurt.
  let crescLivePoll = null;
  async function pollCrescLive() {
    try {
      const r = await invoke('get_crescendo_state'); // [enabled, stage, total]
      if (Array.isArray(r) && r.length === 3) {
        crescLiveEnabled = r[0];
        crescLiveStage = r[1];
        crescLiveTotal = r[2];
        // Houd de bewerk-balk in sync met de werkelijke (pedaal-gestuurde) stand.
        crescendoStage = r[1];
      }
    } catch (e) {}
  }
  let crescLiveEnabled = false;
  let crescLiveStage = 0;
  let crescLiveTotal = 0;

  function toggleMainLayout() {
    mainLayout = mainLayout === 'horizontal' ? 'vertical' : 'horizontal';
    localStorage.setItem(organUiKey('jm-orgue-main-layout'), mainLayout);
  }

  // Registerknop-grootte aanpassen (min-breedte in px); per orgel persistent.
  function adjustStopSize(delta) {
    stopSize = Math.max(70, Math.min(220, stopSize + delta));
    localStorage.setItem(organUiKey('jm-orgue-stop-size'), String(stopSize));
  }

  // Bewaar de set open extra-vensters in de sessie zodat ze bij opstart heropend kunnen worden.
  function saveOpenPanelsToSession() {
    try {
      const cur = JSON.parse(localStorage.getItem('jm-orgue-session') || '{}');
      cur.openPanels = openPanels.map(p => ({ count: 1 })); // alleen aantal — labels zijn tijdgebonden
      localStorage.setItem('jm-orgue-session', JSON.stringify(cur));
    } catch (e) {}
  }

  // Open extra register window. Geëxporteerd zodat App.svelte het bij sessie-herstel
  // kan aanroepen (om eerder open vensters automatisch te heropenen).
  export async function openPanel() { return openExtraWindow(); }

  async function openExtraWindow() {
    try {
      const { WebviewWindow } = await import('@tauri-apps/api/webviewWindow');
      // Find lowest available screen number
      const usedNumbers = openPanels.map(p => p.number);
      let screenNum = 2;
      while (usedNumbers.includes(screenNum)) screenNum++;
      const label = `panel-${Date.now()}`;
      const offset = openPanels.length;
      const webview = new WebviewWindow(label, {
        url: 'index.html#panel',
        title: `JM-Orgue - Scherm ${screenNum}`,
        width: 700,
        height: 500,
        decorations: true,
        resizable: true,
        center: false,
        x: 100 + (offset * 30),
        y: 100 + (offset * 30),
      });
      const panelEntry = { label, number: screenNum };
      openPanels = [...openPanels, panelEntry];
      saveOpenPanelsToSession();
      webview.once('tauri://destroyed', () => {
        openPanels = openPanels.filter(p => p.label !== label);
        saveOpenPanelsToSession();
      });
      webview.once('tauri://error', (e) => {
        console.error('Window creation error:', e);
        openPanels = openPanels.filter(p => p.label !== label);
        saveOpenPanelsToSession();
      });
    } catch (e) {
      console.error('Failed to open window:', e);
      alert(tx('register_panel.window_open_failed') + ': ' + e);
    }
  }

  async function openOrganFile() {
    try {
      const selected = await open({
        multiple: false,
        filters: [{
          name: 'Orgeldefinitie',
          extensions: ['organ']
        }]
      });
      if (selected) {
        dispatch('loadOrgan', selected);
      }
    } catch (e) {
      console.error('Failed to open file dialog:', e);
    }
  }

  async function scanSampleFolder() {
    try {
      const selected = await open({
        multiple: false,
        directory: true,
        title: tx('dialogs.sample_folder_title')
      });
      if (selected) {
        dispatch('scanFolder', selected);
      }
    } catch (e) {
      console.error('Failed to open folder dialog:', e);
    }
  }

  async function openExternalSampleset() {
    try {
      const selected = await open({
        multiple: false,
        directory: true,
        title: 'Externe sampleset — selecteer de hoofdmap (alleen onversleutelde .wav bestanden)'
      });
      if (selected) {
        dispatch('scanFolder', selected);
      }
    } catch (e) {
      console.error('Failed to open folder dialog:', e);
    }
  }

  async function exportOrganFile() {
    try {
      // First select source directory
      const sourceDir = await open({
        multiple: false,
        directory: true,
        title: 'Selecteer de sample map om te exporteren'
      });
      if (!sourceDir) return;

      // Then select output file
      const { save } = await import('@tauri-apps/plugin-dialog');
      const outputPath = await save({
        title: 'Sla .organ bestand op',
        defaultPath: 'orgel.organ',
        filters: [{ name: 'Orgeldefinitie', extensions: ['organ'] }]
      });
      if (!outputPath) return;

      await invoke('export_organ_file', { directory: sourceDir, outputPath });
      alert('Orgeldefinitie succesvol geëxporteerd!');
    } catch (e) {
      console.error('Export failed:', e);
      alert('Export mislukt: ' + e);
    }
  }


  // ==========================================
  // MIDI MAPPING functions
  // ==========================================

  // Reactieve lookup per divisie: de Kanaal/Transpose/bereik-controls moeten
  // bijwerken zodra mappings veranderen (na 'Leer' of handmatige keuze). Een kale
  // functie-aanroep in de template trackt `midiMappings` niet → verouderde weergave
  // (toont vaak "Alle" terwijl er wél een kanaal is) → speelt op alle klavieren.
  $: midiMapByDiv = Object.fromEntries((midiMappings || []).map(m => [m.division, m]));

  function getMidiChannel(divisionName) {
    const mapping = midiMappings.find(m => m.division === divisionName);
    return mapping?.channel ?? null;
  }

  function getTranspose(divisionName) {
    const mapping = midiMappings.find(m => m.division === divisionName);
    return mapping?.transpose ?? 0;
  }

  function getKeyboardRange(divisionName) {
    const mapping = midiMappings.find(m => m.division === divisionName);
    if (mapping && mapping.first_midi_note != null && mapping.last_midi_note != null) {
      return { first: mapping.first_midi_note, last: mapping.last_midi_note };
    }
    return null;
  }

  function midiToNoteName(midi) {
    const notes = ['C', 'C#', 'D', 'D#', 'E', 'F', 'F#', 'G', 'G#', 'A', 'A#', 'B'];
    const octave = Math.floor(midi / 12) - 1;
    const note = notes[midi % 12];
    return `${note}${octave}`;
  }

  function setMidiChannel(divisionName, channel) {
    const transpose = getTranspose(divisionName);
    dispatch('setMidiMapping', { division: divisionName, channel, transpose });
  }

  function setTranspose(divisionName, transpose) {
    const channel = getMidiChannel(divisionName);
    dispatch('setMidiMapping', { division: divisionName, channel, transpose });
  }

  async function learnChannel(divisionName) {
    if (!midiConnected) {
      alert('Verbind eerst een MIDI apparaat in de zijbalk!');
      return;
    }
    learningDivision = divisionName;
    learningStep = 1;
    setTimeout(() => {
      if (learningDivision === divisionName && learningStep === 1) {
        learningStep = 2;
      }
    }, 3000);
    dispatch('learnKeyboardRange', { division: divisionName, firstSampleNote: 36 });
  }

  export function onLearnComplete() {
    learningDivision = null;
    learningStep = 0;
  }

  // ==========================================
  // HELPERS
  // ==========================================

  // Library organs (loaded from backend)
  let libraryOrgans = [];
  let organImages = {}; // { [id]: base64_data_url }

  async function loadLibrary() {
    try {
      libraryOrgans = await invoke('get_organ_library');
      // Load images for each organ
      for (const organ of libraryOrgans) {
        if (!organImages[organ.id]) {
          invoke('get_organ_image', { sourcePath: organ.source_path }).then(img => {
            if (img) {
              organImages[organ.id] = img;
              organImages = organImages; // trigger reactivity
            }
          }).catch(() => {});
        }
      }
    } catch (e) {
      console.error('Failed to load organ library:', e);
    }
  }

  async function removeFromLibrary(e, id) {
    e.stopPropagation();
    try {
      await invoke('remove_from_library', { id });
      libraryOrgans = libraryOrgans.filter(o => o.id !== id);
      delete organImages[id];
      organImages = organImages;
    } catch (e) {
      console.error('Failed to remove from library:', e);
    }
  }

  function loadLibraryOrgan(organ) {
    if (organ.source_type === 'organ_file') {
      dispatch('loadOrgan', organ.source_path);
    } else {
      dispatch('scanFolder', organ.source_path);
    }
  }

  function getStopClass(stop) {
    const name = stop.name.toLowerCase();
    if (name.includes('trompet') || name.includes('hobo') || name.includes('bazuin') || name.includes('schalmei')) return 'reed';
    if (name.includes('fluit') || name.includes('gedekt') || name.includes('bourdon') || name.includes('gedackt') || name.includes('rörflöjt')) return 'flute';
    if (name.includes('viola') || name.includes('gamba') || name.includes('celeste') || name.includes('salicional') || name.includes('voix')) return 'string';
    if (name.includes('mixtuur') || name.includes('cymbel') || name.includes('sesquialter')) return 'mixture';
    if (name.includes('koppel') || name.includes('coupler')) return 'coupler';
    if (name.includes('tremulant')) return 'tremulant';
    if (name.includes('subbas')) return 'pedal';
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

  // Strip pitch from stop name to avoid double display
  function cleanStopName(stop) {
    let name = stop.name;
    if (stop.pitch_feet && stop.pitch_feet > 0) {
      const pitchStr = formatPitch(stop.pitch_feet);
      if (pitchStr) {
        // Remove trailing pitch like " 8'" or " 16'"
        const suffix1 = ' ' + pitchStr;
        const suffix2 = pitchStr;
        if (name.endsWith(suffix1)) {
          name = name.slice(0, -suffix1.length).trim();
        } else if (name.endsWith(suffix2)) {
          name = name.slice(0, -suffix2.length).trim();
        }
      }
    }
    if (stop.pitch) {
      const suffix1 = ' ' + stop.pitch;
      const suffix2 = stop.pitch;
      if (name.endsWith(suffix1)) {
        name = name.slice(0, -suffix1.length).trim();
      } else if (name.endsWith(suffix2)) {
        name = name.slice(0, -suffix2.length).trim();
      }
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

  const demoOrgan = {
    name: "Demo Orgel",
    builder: "Virtueel",
    location: "Uw Computer",
    divisions: [
      {
        name: "Hoofdwerk",
        display_name: "Hoofdwerk (I)",
        stops: [
          { id: "hw-prestant8", name: "Prestant", pitch_feet: 8, drawn: true },
          { id: "hw-roerfluit8", name: "Roerfluit", pitch_feet: 8, drawn: false },
          { id: "hw-octaaf4", name: "Octaaf", pitch_feet: 4, drawn: true },
          { id: "hw-mixtuur", name: "Mixtuur IV", pitch_feet: 2, drawn: false },
          { id: "hw-trompet8", name: "Trompet", pitch_feet: 8, drawn: false },
        ]
      },
      {
        name: "Positief",
        display_name: "Positief (II)",
        stops: [
          { id: "pos-holpijp8", name: "Holpijp", pitch_feet: 8, drawn: true },
          { id: "pos-fluit4", name: "Fluit", pitch_feet: 4, drawn: false },
          { id: "pos-nasard3", name: "Nasard", pitch_feet: 2.666, drawn: false },
          { id: "pos-kromhoorn8", name: "Kromhoorn", pitch_feet: 8, drawn: false },
        ]
      },
      {
        name: "Pedaal",
        display_name: "Pedaal",
        stops: [
          { id: "ped-subbas16", name: "Subbas", pitch_feet: 16, drawn: false },
          { id: "ped-octaaf8", name: "Octaaf", pitch_feet: 8, drawn: false },
          { id: "ped-bazuin16", name: "Bazuin", pitch_feet: 16, drawn: false },
        ]
      }
    ]
  };

  // Coupler MIDI learn context menu
  let couplerContextMenu = null;
  let couplerMidiBindings = {};

  // Stop MIDI learn context menu
  let stopContextMenu = null;
  let stopMidiBindings = {};

  // (Contextmenu openen loopt via use:midiLearn → show...At; één mechanisme
  //  voor rechtermuis én touch long-press.)
  function showCouplerContextMenuAt(actionCode) {
    couplerContextMenu = {
      x: Math.round(window.innerWidth / 2 - 80),
      y: Math.round(window.innerHeight / 2 - 40),
      action: actionCode,
    };
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

  // Load coupler MIDI binding counts
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
  function showStopContextMenuAt(actionCode) {
    stopContextMenu = {
      x: Math.round(window.innerWidth / 2 - 80),
      y: Math.round(window.innerHeight / 2 - 40),
      action: actionCode,
    };
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

  // Close context menus on click
  function handleGlobalClick() {
    closeCouplerContextMenu();
    closeStopContextMenu();
  }

  $: if (organInfo) { loadCouplerMidiCounts(); loadSwellBindings(); }

  // Stop order per division (drag & drop sorting)
  let stopOrder = {}; // { divisionName: [stopId1, stopId2, ...] }
  let dragDiv = null;
  let dragIdx = null;
  let dropIdx = null;

  function loadStopOrder() {
    if (!organInfo) return;
    try {
      const key = `jm-orgue-stop-order-${organInfo.id || 'default'}`;
      const saved = localStorage.getItem(key);
      if (saved) stopOrder = JSON.parse(saved);
      else stopOrder = {};
    } catch (e) { stopOrder = {}; }
  }

  function saveStopOrder() {
    if (!organInfo) return;
    const key = `jm-orgue-stop-order-${organInfo.id || 'default'}`;
    localStorage.setItem(key, JSON.stringify(stopOrder));
  }

  function sortStops(stops, divName) {
    const order = stopOrder[divName];
    if (!order || order.length === 0) return stops;
    const indexed = new Map(stops.map(s => [s.id, s]));
    const sorted = [];
    for (const id of order) {
      if (indexed.has(id)) { sorted.push(indexed.get(id)); indexed.delete(id); }
    }
    // Append any stops not in the saved order (new stops)
    for (const s of indexed.values()) sorted.push(s);
    return sorted;
  }

  function sortByPitch(divName, stops) {
    const pitchVal = (s) => {
      const m = s.pitch?.match(/(\d+)/);
      return m ? parseInt(m[1]) : 8;
    };
    stopOrder[divName] = [...stops].sort((a, b) => pitchVal(b) - pitchVal(a)).map(s => s.id);
    stopOrder = stopOrder;
    saveStopOrder();
  }

  function sortByFamily(divName, stops) {
    const familyOrder = (s) => {
      const n = s.name.toLowerCase();
      if (n.includes('prestant') || n.includes('principaal') || n.includes('octaaf') || n.includes('diapason')) return 0;
      if (n.includes('fluit') || n.includes('gedekt') || n.includes('bourdon') || n.includes('holpijp')) return 1;
      if (n.includes('viola') || n.includes('gamba') || n.includes('salicional') || n.includes('celeste')) return 2;
      if (n.includes('mixtuur') || n.includes('cymbel') || n.includes('sesquialter') || n.includes('cornet')) return 3;
      if (n.includes('trompet') || n.includes('hobo') || n.includes('bazuin') || n.includes('fagot') || n.includes('schalmei')) return 4;
      if (n.includes('tremulant') || n.includes('koppel')) return 5;
      return 2;
    };
    stopOrder[divName] = [...stops].sort((a, b) => familyOrder(a) - familyOrder(b)).map(s => s.id);
    stopOrder = stopOrder;
    saveStopOrder();
  }

  function resetOrder(divName) {
    delete stopOrder[divName];
    stopOrder = stopOrder;
    saveStopOrder();
  }

  function handleDragStart(e, divName, idx) {
    dragDiv = divName;
    dragIdx = idx;
    if (e.dataTransfer) {
      e.dataTransfer.effectAllowed = 'move';
      // Some webviews require data to be set for the drag to initiate.
      try { e.dataTransfer.setData('text/plain', String(idx)); } catch (_) {}
    }
  }
  function handleDragOver(e, idx) {
    e.preventDefault();
    if (e.dataTransfer) e.dataTransfer.dropEffect = 'move';
    dropIdx = idx;
  }
  function handleDragEnd() { dragDiv = null; dragIdx = null; dropIdx = null; }
  // Reliable reorder via up/down buttons (works on touch and mouse, unlike
  // native HTML5 drag-and-drop which doesn't fire on touch screens).
  function moveStop(divName, fromIdx, delta) {
    const organ = displayOrgan || organInfo || demoOrgan;
    const div = organ.divisions.find(d => d.name === divName);
    if (!div) return;
    const order = div.stops.map(s => s.id);
    const toIdx = fromIdx + delta;
    if (toIdx < 0 || toIdx >= order.length) return;
    const [item] = order.splice(fromIdx, 1);
    order.splice(toIdx, 0, item);
    stopOrder[divName] = order;
    stopOrder = stopOrder;
    saveStopOrder();
  }

  function handleDrop(divName, toIdx) {
    if (dragDiv !== divName || dragIdx === null || dragIdx === toIdx) return;
    // Base the reorder on the CURRENTLY DISPLAYED order so the drag/drop indices
    // always match what the user sees (and so the saved order is complete).
    const organ = displayOrgan || organInfo || demoOrgan;
    const div = organ.divisions.find(d => d.name === divName);
    if (!div) return;
    const currentOrder = div.stops.map(s => s.id);
    const item = currentOrder.splice(dragIdx, 1)[0];
    currentOrder.splice(toIdx, 0, item);
    stopOrder[divName] = currentOrder;
    stopOrder = stopOrder;
    saveStopOrder();
    dragDiv = null; dragIdx = null; dropIdx = null;
  }

  $: if (organInfo) loadStopOrder();

  // Apply stop order to displayOrgan.
  // `stopOrder` is passed as an argument so Svelte tracks it as a dependency
  // (sortStops reads it internally; Svelte only tracks directly-referenced vars).
  function buildDisplayOrgan(organ, _order) {
    const base = organ || demoOrgan;
    if (!base) return base;
    return {
      ...base,
      divisions: base.divisions.map(d => ({
        ...d,
        stops: sortStops(d.stops, d.name),
      })),
    };
  }
  $: displayOrgan = buildDisplayOrgan(organInfo, stopOrder);

  // Reset selectedDivisions whenever the organ (divisions) changes
  let prevDivisionNames = '';
  $: {
    const names = displayOrgan?.divisions?.map(d => d.name).join(',') || '';
    if (names && names !== prevDivisionNames) {
      prevDivisionNames = names;
      selectedDivisions = displayOrgan.divisions.map(d => d.name);
    }
  }

  // Bij een ECHTE orgelwissel (id verandert): reset de live tremulant-stand en laad de
  // crescendo-config van dít orgel. We keyen op organInfo.id (niet op divisie-namen),
  // want twee verschillende orgels kunnen identieke divisie-namen hebben — dan zou een
  // tremulant die op het vorige orgel aanstond ten onrechte "actief" blijven.
  let crescLoadedFor = '';
  $: if (organInfo && organInfo.id && organInfo.id !== crescLoadedFor) {
    crescLoadedFor = organInfo.id;
    tremActive = {};
    loadCrescendoForOrgan();
    loadAudioSettingsForOrgan();
    loadUiPrefsForOrgan();
  }

  // Load library when browser is shown
  $: if (showOrganBrowser) { loadLibrary(); }

  function toggleDivision(name) {
    if (selectedDivisions.includes(name)) {
      if (selectedDivisions.length > 1) {
        selectedDivisions = selectedDivisions.filter(d => d !== name);
      }
    } else {
      selectedDivisions = [...selectedDivisions, name];
    }
  }
</script>

<main class="console">
  {#if loading}
    <div class="loading-container fade-in">
      <div class="spinner"></div>
      <div class="loading-text">{loadingMessage}</div>
      <div class="loading-progress">
        <div class="loading-progress-bar" style="width: {loadingProgress}%"></div>
      </div>
    </div>

  {:else if showOrganBrowser}
    <div class="organ-browser slide-up">
      <div class="organ-browser-header">
        <h1 class="organ-browser-title">{$t('library.title')}</h1>
        <div class="organ-browser-actions">
          <button class="btn btn-secondary btn-sm" on:click={openOrganFile} title=".organ">
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"/>
              <line x1="12" y1="11" x2="12" y2="17"/>
              <line x1="9" y1="14" x2="15" y2="14"/>
            </svg>
            {$t('library.open_organ_file')}
          </button>
          <button class="btn btn-secondary btn-sm" on:click={scanSampleFolder} title={$t('library.scan_folder')}>
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <path d="M3 7v10a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2V9a2 2 0 0 0-2-2h-6l-2-2H5a2 2 0 0 0-2 2z"/>
              <circle cx="12" cy="13" r="3"/>
            </svg>
            {$t('library.scan_folder')}
          </button>
          <button class="btn btn-secondary btn-sm" on:click={openExternalSampleset} title={$t('library.external_set')}>
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <path d="M3 7v10a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2V9a2 2 0 0 0-2-2h-6l-2-2H5a2 2 0 0 0-2 2z"/>
              <path d="M12 11v6M9 14h6"/>
            </svg>
            {$t('library.external_set')}
          </button>
          <button class="btn btn-secondary btn-sm" on:click={exportOrganFile} title={$t('library.export')}>
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/>
              <polyline points="7 10 12 15 17 10"/>
              <line x1="12" y1="15" x2="12" y2="3"/>
            </svg>
            {$t('library.export')}
          </button>
          <button class="btn btn-secondary btn-sm" on:click={() => { showOrganBrowser = false; dispatch('setView', 'algemene-instellingen'); }} title={$t('library.app_settings_button')}>
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <circle cx="12" cy="12" r="3"/>
              <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1 0 2.83 2 2 0 0 1-2.83 0l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-2 2 2 2 0 0 1-2-2v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83 0 2 2 0 0 1 0-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1-2-2 2 2 0 0 1 2-2h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 0-2.83 2 2 0 0 1 2.83 0l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 2-2 2 2 0 0 1 2 2v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 0 2 2 0 0 1 0 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 2 2 2 2 0 0 1-2 2h-.09a1.65 1.65 0 0 0-1.51 1z"/>
            </svg>
            {$t('library.app_settings_button')}
          </button>
        </div>
      </div>

      <div class="organ-library-grid">
        {#each libraryOrgans as organ}
          <div
            class="library-card"
            on:click={() => loadLibraryOrgan(organ)}
            on:keypress={(e) => e.key === 'Enter' && loadLibraryOrgan(organ)}
            role="button"
            tabindex="0"
          >
            <button
              class="library-card-delete"
              on:click={(e) => removeFromLibrary(e, organ.id)}
              title="Verwijder uit bibliotheek"
            >&times;</button>
            <div class="library-card-image">
              {#if organImages[organ.id]}
                <img src={organImages[organ.id]} alt={organ.name} />
              {:else}
                <svg width="48" height="48" viewBox="0 0 36 36" fill="none">
                  <rect x="4" y="6" width="5" height="26" rx="1.5" fill="currentColor" opacity="0.3"/>
                  <rect x="11" y="10" width="5" height="22" rx="1.5" fill="currentColor" opacity="0.4"/>
                  <rect x="18" y="4" width="5" height="28" rx="1.5" fill="currentColor" opacity="0.5"/>
                  <rect x="25" y="8" width="5" height="24" rx="1.5" fill="currentColor" opacity="0.4"/>
                </svg>
              {/if}
            </div>
            <div class="library-card-info">
              <div class="library-card-name">{organ.name}</div>
              <div class="library-card-meta">{organ.stop_count} registers</div>
            </div>
          </div>
        {/each}

        {#if libraryOrgans.length === 0}
          <div class="library-empty">
            <svg width="48" height="48" viewBox="0 0 36 36" fill="none">
              <rect x="4" y="6" width="5" height="26" rx="1.5" fill="currentColor" opacity="0.15"/>
              <rect x="11" y="10" width="5" height="22" rx="1.5" fill="currentColor" opacity="0.2"/>
              <rect x="18" y="4" width="5" height="28" rx="1.5" fill="currentColor" opacity="0.25"/>
              <rect x="25" y="8" width="5" height="24" rx="1.5" fill="currentColor" opacity="0.2"/>
            </svg>
            <p>{$t('library.empty_title')}</p>
            <p>{$t('library.empty_hint')}</p>
          </div>
        {/if}
      </div>
    </div>

  {:else if displayOrgan}

    <!-- ========== ORGEL VIEW ========== -->
    {#if activeView === 'orgel'}
      <div class="orgel-view">
        <!-- Toolbar -->
        <div class="panel-toolbar">
          <div class="panel-config">
            {#each displayOrgan.divisions as div}
              <button
                class="panel-div-btn"
                class:selected={selectedDivisions.includes(div.name)}
                on:click={() => toggleDivision(div.name)}
              >{div.name}</button>
            {/each}
          </div>
          <div class="panel-toolbar-right">
            <div class="stop-size-control" title="Registerknop-grootte">
              <button class="btn btn-ghost btn-sm stop-size-btn" on:click={() => adjustStopSize(-15)} aria-label="Kleiner">−</button>
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="3" y="6" width="18" height="12" rx="2"/></svg>
              <button class="btn btn-ghost btn-sm stop-size-btn" on:click={() => adjustStopSize(15)} aria-label="Groter">+</button>
            </div>
            <button
              class="btn btn-ghost btn-sm panel-layout-btn"
              on:click={toggleMainLayout}
              title="{mainLayout === 'horizontal' ? 'Verticale rij' : 'Horizontale rij'}"
            >
              {#if mainLayout === 'horizontal'}
                <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                  <rect x="3" y="3" width="7" height="18" rx="1"/>
                  <rect x="14" y="3" width="7" height="18" rx="1"/>
                </svg>
                Verticaal
              {:else}
                <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                  <rect x="3" y="3" width="18" height="7" rx="1"/>
                  <rect x="3" y="14" width="18" height="7" rx="1"/>
                </svg>
                Horizontaal
              {/if}
            </button>
            <button class="btn btn-ghost btn-sm panel-add-btn" on:click={openExtraWindow}>
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <rect x="2" y="3" width="20" height="14" rx="2"/>
                <line x1="8" y1="21" x2="16" y2="21"/>
                <line x1="12" y1="17" x2="12" y2="21"/>
              </svg>
              Nieuw scherm
            </button>
            <button
              class="btn btn-ghost btn-sm record-btn"
              class:recording={recorderStatus.recording}
              on:click={toggleRecording}
              title={recorderStatus.recording
                ? `Audio-opname loopt — ${formatSeconds(recorderStatus.seconds)} · Klik om te stoppen`
                : 'MP3-opname (wat je hoort) starten in Documenten\\JM-Orgue-opnames'}
            >
              <span class="record-dot" class:on={recorderStatus.recording}></span>
              {#if recorderStatus.recording}
                {formatSeconds(recorderStatus.seconds)}
              {:else}
                MP3
              {/if}
            </button>
            <button
              class="btn btn-ghost btn-sm record-btn midi-record-btn"
              class:recording={midiRec.recording}
              on:click={toggleMidiRecording}
              title={midiRec.recording
                ? `MIDI-opname loopt — ${midiRec.event_count} events · ${formatSeconds(midiRec.seconds)} · Klik om te stoppen`
                : 'MIDI-opname (wat je speelt — alleen noten) starten; opslaan in Documenten\\JM-Orgue-opnames'}
            >
              <span class="record-dot" class:on={midiRec.recording}></span>
              {#if midiRec.recording}
                {midiRec.event_count} · {formatSeconds(midiRec.seconds)}
              {:else}
                MIDI
              {/if}
            </button>
            {#if lastMidiPath && !midiRec.recording}
              <button
                class="btn btn-ghost btn-sm"
                on:click={playLastMidi}
                title="Zojuist opgenomen MIDI in de speler laden en afspelen (trek zelf de registers)"
              >
                <svg width="13" height="13" viewBox="0 0 24 24" fill="currentColor">
                  <polygon points="6,4 20,12 6,20"/>
                </svg>
                Opname afspelen
              </button>
            {/if}
            <button
              class="btn btn-ghost btn-sm shutdown-btn"
              on:click={() => dispatch('shutdownRequest')}
              use:midiLearn={{ onTrigger: () => dispatch('shutdownLearn') }}
              title="Software + computer afsluiten · rechtermuis of 3s ingedrukt = MIDI inleren"
            >
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <path d="M18.36 6.64a9 9 0 1 1-12.73 0"/>
                <line x1="12" y1="2" x2="12" y2="12"/>
              </svg>
              Afsluiten
            </button>
          </div>
        </div>

        <!-- Single register view -->
        <div class="divisions-container" class:divisions-vertical={mainLayout === 'vertical'}>
          {#each selectedDivisions.map(name => displayOrgan.divisions.find(d => d.name === name)).filter(Boolean) as division}
            {@const tremDivIdx = displayOrgan.divisions.findIndex(d => d.name === division.name)}
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
              <div class="stops-grid" class:stops-vertical={mainLayout === 'vertical'} style="--stop-min-width: {stopSize}px">
                {#each division.stops as stop}
                  {@const name = cleanStopName(stop)}
                  <button
                    class="stop-knob {getStopClass(stop)}"
                    class:engaged={stop.drawn}
                    class:has-midi={stopMidiBindings[stop.midi_action_code] > 0}
                    on:click={() => dispatch('toggleStop', stop.id)}
                    use:midiLearn={{ onTrigger: () => showStopContextMenuAt(stop.midi_action_code) }}
                    aria-label="{stop.name} {stop.pitch || ''} — {stop.drawn ? 'ingeschakeld' : 'uitgeschakeld'}"
                    aria-pressed={stop.drawn}
                  >
                    <span class="stop-name" style="{stopNameStyle(name)}">{name}</span>
                    {#if stop.pitch || stop.pitch_feet}
                      <span class="stop-pitch">{stop.pitch || formatPitch(stop.pitch_feet)}</span>
                    {/if}
                  </button>
                {/each}
                {#if division.stops.some(s => s.has_tremulant) || tremLfoEnabled[division.name]}
                  <button
                    class="stop-knob tremulant"
                    class:engaged={tremActive[division.name] === true}
                    class:has-midi={tremDivIdx >= 0 && tremLfoMidiBindings[ACTION_TREM_LFO_BASE + tremDivIdx] > 0}
                    on:click={() => setTremActive(division, tremActive[division.name] !== true)}
                    use:midiLearn={{ onTrigger: () => { if (tremDivIdx >= 0) learnTremulantLfo(tremDivIdx); } }}
                    aria-label="Tremulant {division.name} — {tremActive[division.name] === true ? 'aan' : 'uit'}"
                    aria-pressed={tremActive[division.name] === true}
                  >
                    <span class="stop-name">Tremulant</span>
                  </button>
                {/if}
                <!-- Koppelknoppen (alleen zichtbare) -->
                {#if displayOrgan.couplers}
                  {#each displayOrgan.couplers.filter(c => c.display_in_division === division.name && isCouplerVisible(c.id)) as coupler}
                    <button
                      class="stop-knob coupler coupler-knob"
                      class:engaged={coupler.active}
                      class:has-midi={couplerMidiBindings[coupler.midi_action_code] > 0}
                      on:click={() => dispatch('toggleCoupler', coupler.id)}
                      use:midiLearn={{ onTrigger: () => showCouplerContextMenuAt(coupler.midi_action_code) }}
                      title={coupler.name.replace(/\n/g, ' ')}
                      aria-label="{coupler.name.replace(/\n/g, ' ')} — {coupler.active ? 'ingeschakeld' : 'uitgeschakeld'}"
                      aria-pressed={coupler.active}
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
        <SetzerBar
          organInfo={displayOrgan}
          on:externalAction={(e) => handleExternalAction(e.detail.actionCode)}
        />
      </div>

    <!-- ========== INSTELLINGEN VIEW ========== -->
    {:else if activeView === 'orgel-instellingen' && displayOrgan}
      <div class="instellingen-view">
        <div class="instellingen-columns">

          <!-- Kolom 1: Master, Reverb, EQ, Wind, Crescendo, Register Sort, Beheer -->
          <div class="instellingen-section">
            <!-- Master Volume, Reverb & Tuning -->
            <div class="settings-block">
              <h3 class="settings-block-title">{$t('settings.master')}</h3>
              <div class="slider-control">
                <div class="slider-header">
                  <span class="slider-label">Volume</span>
                  <span class="slider-value">{volume} dB</span>
                </div>
                <input
                  type="range"
                  min="-40"
                  max="6"
                  step="1"
                  bind:value={volume}
                  on:input={onVolumeChange}
                />
              </div>
              <div class="slider-control">
                <div class="slider-header">
                  <span class="slider-label">Stemming</span>
                </div>
                <select
                  class="temperament-select"
                  bind:value={selectedTemperament}
                  on:change={onTemperamentChange}
                >
                  {#each temperaments as t, i}
                    <option value={i}>{t.nameDutch || t.name}</option>
                  {/each}
                </select>
              </div>
              <div class="slider-control">
                <div class="slider-header">
                  <span class="slider-label">Fijnstemming</span>
                  <span class="slider-value">{fineTune > 0 ? '+' : ''}{fineTune} cent</span>
                </div>
                <input
                  type="range"
                  min="-50"
                  max="50"
                  step="1"
                  bind:value={fineTune}
                  on:input={onTemperamentChange}
                />
              </div>
              <!-- Custom temperament editor -->
              <button class="btn btn-secondary btn-sm" style="margin-top: 0.3rem;"
                on:click={() => { showCustomTemperament = !showCustomTemperament; if (showCustomTemperament) customCents = [...(temperaments[selectedTemperament]?.cents || [0,0,0,0,0,0,0,0,0,0,0,0])]; }}>
                {showCustomTemperament ? 'Sluiten' : 'Eigen stemming...'}
              </button>
              {#if showCustomTemperament}
                <div class="custom-temperament-grid">
                  {#each noteNames as noteName, i}
                    <div class="custom-temp-row">
                      <span class="custom-temp-note">{noteName}</span>
                      <input type="number" step="0.1" min="-50" max="50"
                        bind:value={customCents[i]}
                        on:input={applyCustomCents}
                        class="custom-temp-input"
                      />
                      <span class="custom-temp-unit">ct</span>
                    </div>
                  {/each}
                  <button class="btn btn-primary btn-sm" style="margin-top: 0.3rem; width: 100%;" on:click={saveCustomTemperament}>
                    Opslaan als preset
                  </button>
                </div>
              {/if}
            </div>

            <!-- Reverb -->
            <div class="settings-block">
              <h3 class="settings-block-title">{$t('settings.reverb')}</h3>
              <!-- Type selector -->
              <div class="reverb-type-selector">
                <button class="btn btn-sm" class:btn-active={reverbType === 'convolution'} on:click={() => switchReverbType('convolution')}>Convolutie (IR)</button>
                <button class="btn btn-sm" class:btn-active={reverbType === 'algorithmic'} on:click={() => switchReverbType('algorithmic')}>Algoritmisch</button>
              </div>

              <!-- Mix slider (shared) -->
              <div class="slider-control">
                <div class="slider-header">
                  <span class="slider-label">Mix</span>
                  <span class="slider-value">{reverb}%</span>
                </div>
                <input type="range" min="0" max="100" step="1"
                  bind:value={reverb}
                  on:input={() => { onReverbChange(); if (reverbType === 'algorithmic') updateAlgorithmicReverb(); }}
                />
              </div>

              {#if reverbType === 'convolution'}
                <div class="reverb-ir-control">
                  <button class="midi-learn-btn" on:click={loadReverbIR}>
                    {reverbIrLoaded ? 'Wijzig IR' : 'Laad Impuls Response'}
                  </button>
                  {#if reverbIrLoaded}
                    <span class="reverb-ir-name" title={reverbIrName}>{reverbIrName}</span>
                  {/if}
                </div>
              {:else}
                <!-- Preset selector -->
                <div class="slider-control" style="margin-bottom: 0.5rem;">
                  <div class="slider-header">
                    <span class="slider-label">Preset</span>
                  </div>
                  <select class="reverb-preset-select" bind:value={reverbPreset} on:change={() => applyReverbPreset(reverbPreset)}>
                    {#each reverbPresets as p, i}
                      <option value={i}>{p.name}</option>
                    {/each}
                  </select>
                </div>
                <!-- Sliders -->
                <div class="swell-config-sliders">
                  <div class="swell-config-row">
                    <span class="swell-config-label">RT60</span>
                    <input type="range" min="3" max="150" step="1"
                      value={reverbRt60 * 10}
                      on:input={(e) => { reverbRt60 = parseInt(e.target.value) / 10; updateAlgorithmicReverb(); }}
                    />
                    <span class="swell-config-value">{reverbRt60.toFixed(1)}s</span>
                  </div>
                  <div class="swell-config-row">
                    <span class="swell-config-label">Pre-delay</span>
                    <input type="range" min="0" max="150" step="5"
                      bind:value={reverbPreDelay}
                      on:input={() => updateAlgorithmicReverb()}
                    />
                    <span class="swell-config-value">{reverbPreDelay}ms</span>
                  </div>
                  <div class="swell-config-row">
                    <span class="swell-config-label">Demping</span>
                    <input type="range" min="0" max="95" step="5"
                      bind:value={reverbDamping}
                      on:input={() => updateAlgorithmicReverb()}
                    />
                    <span class="swell-config-value">{reverbDamping}%</span>
                  </div>
                  <div class="swell-config-row">
                    <span class="swell-config-label">Ruimte</span>
                    <input type="range" min="30" max="300" step="10"
                      bind:value={reverbRoomSize}
                      on:input={() => updateAlgorithmicReverb()}
                    />
                    <span class="swell-config-value">{reverbRoomSize}%</span>
                  </div>
                </div>
              {/if}
            </div>

            <!-- Parametric EQ -->
            <div class="settings-block">
              <h3 class="settings-block-title">{$t('settings.equalizer')}</h3>
              <div style="display:flex; align-items:center; gap:0.5rem;">
                <label class="swell-toggle">
                  <input type="checkbox" bind:checked={eqEnabled} on:change={updateEq} />
                  <span class="swell-toggle-label">Ingeschakeld</span>
                </label>
                <button
                  class="btn btn-ghost btn-sm"
                  class:learning={globalLearningAction === ACTION_EQ_ENABLE}
                  on:click={() => learnGlobalAction(ACTION_EQ_ENABLE)}
                  disabled={globalLearningAction !== null && globalLearningAction !== ACTION_EQ_ENABLE}
                  title="MIDI inleren voor EQ aan/uit ({globalMidiBindings[ACTION_EQ_ENABLE] || 0}/4)"
                  style="font-size:0.7rem; padding:0.2rem 0.5rem; border:1px solid {globalMidiBindings[ACTION_EQ_ENABLE] > 0 ? 'var(--midi-indicator)' : 'var(--accent-soft-2)'};"
                >
                  {#if globalLearningAction === ACTION_EQ_ENABLE}
                    <span class="learning-indicator"></span>Wacht...
                  {:else}
                    MIDI {globalMidiBindings[ACTION_EQ_ENABLE] || 0}/4
                  {/if}
                </button>
              </div>
              {#if eqEnabled}
                <div class="swell-config-sliders" style="margin-top: 0.4rem;">
                  <div class="swell-config-row">
                    <span class="swell-config-label">Laag {eqLowFreq}Hz</span>
                    <input type="range" min="-12" max="12" step="1" bind:value={eqLowGain} on:input={updateEq} />
                    <span class="swell-config-value">{eqLowGain > 0 ? '+' : ''}{eqLowGain}dB</span>
                  </div>
                  <div class="swell-config-row">
                    <span class="swell-config-label">Midden {eqMidFreq}Hz</span>
                    <input type="range" min="-12" max="12" step="1" bind:value={eqMidGain} on:input={updateEq} />
                    <span class="swell-config-value">{eqMidGain > 0 ? '+' : ''}{eqMidGain}dB</span>
                  </div>
                  <div class="swell-config-row">
                    <span class="swell-config-label">Hoog {eqHighFreq}Hz</span>
                    <input type="range" min="-12" max="12" step="1" bind:value={eqHighGain} on:input={updateEq} />
                    <span class="swell-config-value">{eqHighGain > 0 ? '+' : ''}{eqHighGain}dB</span>
                  </div>
                </div>
              {/if}
            </div>

            <!-- Wind-systeem (groepen) -->
            <div class="settings-block">
              <h3 class="settings-block-title">{$t('settings.wind_system')}</h3>
              <div class="swell-config-row" style="margin-bottom: 0.6rem;">
                <span class="swell-config-label" title="Aantal windvoorzieningen / windladen. Default = elke divisie eigen.">{$t('settings.wind_groups_count')}</span>
                <select
                  style="flex:1; font-size:0.85rem; padding:0.3rem 0.5rem; background:var(--bg-elevated); border:1px solid var(--accent-soft-2); border-radius:var(--radius-sm); color:var(--text);"
                  value={numWindGroups}
                  on:change={(e) => onNumWindGroupsChange(e.target.value)}
                >
                  {#each [1,2,3,4,5,6,7,8] as n}
                    <option value={n}>{n}</option>
                  {/each}
                </select>
              </div>
              <p style="margin:0 0 0.6rem; font-size:0.72rem; color:var(--text-muted); line-height:1.4;">
                {$t('settings.wind_groups_hint')}
              </p>

              {#each Array(Math.max(1, Math.min(numWindGroups, 8))) as _, gIdx}
                {@const divInGroup = (organInfo?.divisions || []).filter((d, i) => getWindGroup(d.name, i) === gIdx)}
                <div style="border-top:var(--border-subtle); padding:0.6rem 0;">
                  <div style="display:flex; justify-content:space-between; align-items:center; margin-bottom:0.35rem;">
                    <strong style="font-size:0.82rem;">{$t('settings.wind_group')} {gIdx + 1}</strong>
                    <span style="font-size:0.7rem; color:var(--text-muted);">
                      {divInGroup.length === 0 ? $t('settings.wind_no_divisions') : divInGroup.map(d => d.name).join(', ')}
                    </span>
                  </div>
                  <label class="swell-toggle">
                    <input
                      type="checkbox"
                      checked={windGroupConfig[gIdx]?.enabled === true}
                      on:change={() => updateWindGroup(gIdx, { enabled: !(windGroupConfig[gIdx]?.enabled === true) })}
                    />
                    <span class="swell-toggle-label">{$t('settings.wind_enabled')}</span>
                  </label>
                  {#if windGroupConfig[gIdx]?.enabled === true}
                    <div class="swell-config-sliders" style="margin-top:0.35rem;">
                      <div class="swell-config-row">
                        <span class="swell-config-label">{$t('settings.wind_reservoir')}</span>
                        <input type="range" min="10" max="200" step="5"
                          value={(windGroupConfig[gIdx]?.reservoir ?? 0.5) * 100}
                          on:input={(e) => updateWindGroup(gIdx, { reservoir: parseInt(e.target.value) / 100 })}
                        />
                        <span class="swell-config-value">{((windGroupConfig[gIdx]?.reservoir ?? 0.5) * 100).toFixed(0)}%</span>
                      </div>
                      <div class="swell-config-row">
                        <span class="swell-config-label">{$t('settings.wind_damping')}</span>
                        <input type="range" min="0" max="100" step="5"
                          value={(windGroupConfig[gIdx]?.damping ?? 0.5) * 100}
                          on:input={(e) => updateWindGroup(gIdx, { damping: parseInt(e.target.value) / 100 })}
                        />
                        <span class="swell-config-value">{((windGroupConfig[gIdx]?.damping ?? 0.5) * 100).toFixed(0)}%</span>
                      </div>
                      <div class="swell-config-row">
                        <span class="swell-config-label">{$t('settings.wind_max_loss')}</span>
                        <input type="range" min="1" max="30" step="1"
                          value={windGroupConfig[gIdx]?.maxSag ?? 10}
                          on:input={(e) => updateWindGroup(gIdx, { maxSag: parseInt(e.target.value) })}
                        />
                        <span class="swell-config-value">{windGroupConfig[gIdx]?.maxSag ?? 10}%</span>
                      </div>
                    </div>
                  {/if}
                </div>
              {/each}
            </div>

            <!-- Crescendo -->
            <div class="settings-block">
              <h3 class="settings-block-title">{$t('settings.crescendo')}</h3>
              <div style="display:flex; align-items:center; gap:0.5rem;">
                <label class="swell-toggle">
                  <input type="checkbox" bind:checked={crescendoEnabled}
                    on:change={() => saveCrescendo()}
                  />
                  <span class="swell-toggle-label">Ingeschakeld</span>
                </label>
                <button
                  class="btn btn-ghost btn-sm"
                  class:learning={globalLearningAction === ACTION_CRESCENDO_ENABLE}
                  on:click={() => learnGlobalAction(ACTION_CRESCENDO_ENABLE)}
                  disabled={globalLearningAction !== null && globalLearningAction !== ACTION_CRESCENDO_ENABLE}
                  title="MIDI inleren voor Crescendo aan/uit ({globalMidiBindings[ACTION_CRESCENDO_ENABLE] || 0}/4)"
                  style="font-size:0.7rem; padding:0.2rem 0.5rem; border:1px solid {globalMidiBindings[ACTION_CRESCENDO_ENABLE] > 0 ? 'var(--midi-indicator)' : 'var(--accent-soft-2)'};"
                >
                  {#if globalLearningAction === ACTION_CRESCENDO_ENABLE}
                    <span class="learning-indicator"></span>Wacht...
                  {:else}
                    MIDI {globalMidiBindings[ACTION_CRESCENDO_ENABLE] || 0}/4
                  {/if}
                </button>
              </div>
              {#if crescendoEnabled}
                <div style="margin-top: 0.5rem;">
                  <div class="swell-config-row" style="margin-bottom: 0.5rem;">
                    <span class="swell-config-label">Stappen</span>
                    <select style="flex:1; font-size: 0.8rem;" bind:value={crescendoNumStages} on:change={() => { crescendoStages = []; saveCrescendo(); }}>
                      {#each [8, 12, 15, 20, 24, 32] as n}
                        <option value={n}>{n}</option>
                      {/each}
                    </select>
                  </div>
                  <div style="display:flex; gap:0.4rem; margin-bottom: 0.5rem;">
                    <button class="btn btn-secondary btn-sm" style="flex:1;" on:click={autoFillCrescendo} title="Verdeel alle registers automatisch over de stappen (oplopend)">
                      Auto-fill
                    </button>
                    <button class="btn btn-ghost btn-sm" on:click={clearCrescendo} title="Alle stappen leegmaken">
                      Wissen
                    </button>
                  </div>
                  <!-- Stage indicator -->
                  <div class="crescendo-bar">
                    {#each Array(crescendoNumStages) as _, i}
                      <div
                        class="crescendo-step"
                        class:active={i < crescendoStage}
                        on:click={() => setCrescendoStage(i + 1)}
                        title="Stap {i + 1}: {crescendoStages[i] ? crescendoStages[i].length : 0} registers"
                      ></div>
                    {/each}
                  </div>
                  <div style="text-align: center; font-size: 0.7rem; color: var(--text-secondary); margin-top: 0.3rem;">
                    Stap {crescendoStage} / {crescendoNumStages}
                    {#if crescendoStages[crescendoStage - 1]}
                      — {crescendoStages[crescendoStage - 1].length} registers
                    {/if}
                  </div>

                  <!-- Crescendo-editor: matrix (rijen = registers per divisie, kolommen = stappen).
                       Klik een cel = aan/uit. Shift+klik = cumulatief vanaf die stap t/m het einde. -->
                  {#if displayOrgan}
                    <div class="cresc-editor-hint">
                      Klik een vakje = register aan/uit in die stap · <strong>Shift+klik</strong> = cumulatief vanaf die stap
                    </div>
                    <div class="cresc-table-wrap">
                      <table class="cresc-table">
                        <thead>
                          <tr>
                            <th class="cresc-corner">Register \ Stap</th>
                            {#each Array(crescendoNumStages) as _, s}
                              <th
                                class="cresc-step-h"
                                class:active={s + 1 === crescendoStage}
                                on:click={() => setCrescendoStage(s + 1)}
                                title="Stap {s + 1} voorbeluisteren"
                              >{s + 1}</th>
                            {/each}
                          </tr>
                        </thead>
                        <tbody>
                          {#each displayOrgan.divisions as div}
                            <tr class="cresc-div-row">
                              <td class="cresc-div-name" colspan={crescendoNumStages + 1}>{div.display_name || div.name}</td>
                            </tr>
                            {#each div.stops as stop}
                              <tr>
                                <td class="cresc-stop-name" title="{stop.name}{stop.pitch ? ' ' + stop.pitch : ''}">
                                  {stop.name}{stop.pitch ? ' ' + stop.pitch : ''}
                                </td>
                                {#each Array(crescendoNumStages) as _, s}
                                  <td class="cresc-cell" class:colactive={s + 1 === crescendoStage}>
                                    <button
                                      type="button"
                                      class="cresc-cell-btn"
                                      class:on={(crescendoStages[s] || []).includes(stop.id)}
                                      on:click={(e) => { if (e.shiftKey) fillStopFrom(s, stop.id); else toggleCrescCell(s, stop.id); }}
                                      title="Stap {s + 1} · {stop.name} — klik = aan/uit, Shift+klik = cumulatief"
                                      aria-label="Stap {s + 1} {stop.name}"
                                    >{(crescendoStages[s] || []).includes(stop.id) ? '●' : ''}</button>
                                  </td>
                                {/each}
                              </tr>
                            {/each}
                          {/each}
                        </tbody>
                      </table>
                    </div>
                  {/if}

                  <!-- MIDI Learn -->
                  <button class="btn btn-secondary midi-learn-btn" style="width: 100%; margin-top: 0.5rem;"
                    class:learning={crescendoLearning}
                    on:click={learnCrescendoPedal}
                  >
                    {#if crescendoLearning}
                      <span class="learning-indicator"></span> Wacht op pedaal...
                    {:else if crescendoBinding}
                      Pedaal: CC{crescendoBinding.cc}
                    {:else}
                      Leer pedaal
                    {/if}
                  </button>
                  {#if crescendoBinding}
                    <label class="swell-toggle" style="margin-top: 0.4rem; width: 100%;" title="Spiegelbeeld: keert de pedaalrichting om (handig als de pedaal de andere kant op werkt)">
                      <input
                        type="checkbox"
                        checked={crescendoBinding.invert === true}
                        on:change={(e) => toggleCrescendoInvert(e.target.checked)}
                      />
                      <span>Spiegelbeeld (pedaal omkeren)</span>
                    </label>
                    <div class="swell-config-row" style="margin-top:0.3rem;" title="Handmatig CC-bereik van het crescendo-pedaal (0–127). Min = af, Max = vol.">
                      <span class="swell-config-label">Pedaal-bereik</span>
                      <input type="number" min="0" max="127" class="pedal-range-input"
                        value={crescendoBinding.min}
                        on:change={(e) => setCrescendoRangeUI(parseInt(e.target.value), crescendoBinding.max)}
                        title="Min (af)"
                      />
                      <span style="color:var(--text-muted);">–</span>
                      <input type="number" min="0" max="127" class="pedal-range-input"
                        value={crescendoBinding.max}
                        on:change={(e) => setCrescendoRangeUI(crescendoBinding.min, parseInt(e.target.value))}
                        title="Max (vol)"
                      />
                    </div>
                    <!-- Live volgbalk: toont de actuele crescendo-trap terwijl je het pedaal beweegt. -->
                    <div class="swell-config-row" style="margin-top:0.3rem;" title="Live crescendo-stand (volgt het pedaal)">
                      <span class="swell-config-label">Live</span>
                      <div class="follow-bar">
                        <div class="follow-bar-fill" style="width: {crescLiveTotal > 0 ? Math.round((crescLiveStage / crescLiveTotal) * 100) : 0}%"></div>
                      </div>
                      <span class="swell-config-value">{crescLiveStage}/{crescLiveTotal}</span>
                    </div>
                  {/if}
                </div>
              {/if}
            </div>

            <!-- Register Sortering -->
            <div class="settings-block">
              <h3 class="settings-block-title">{$t('settings.register_sort')}</h3>
              {#if displayOrgan}
                {#each displayOrgan.divisions as division}
                  <div class="sort-division">
                    <div class="sort-division-header">
                      <span class="sort-division-name">{division.display_name || division.name}</span>
                      <div class="sort-presets">
                        <button class="btn btn-ghost btn-xs" on:click={() => sortByPitch(division.name, (organInfo || demoOrgan).divisions.find(d => d.name === division.name)?.stops || [])}>Voetmaat</button>
                        <button class="btn btn-ghost btn-xs" on:click={() => sortByFamily(division.name, (organInfo || demoOrgan).divisions.find(d => d.name === division.name)?.stops || [])}>Familie</button>
                        <button class="btn btn-ghost btn-xs" on:click={() => resetOrder(division.name)}>Reset</button>
                      </div>
                    </div>
                    <div class="sort-list">
                      {#each division.stops as stop, idx}
                        <div
                          class="sort-row"
                          class:drag-over={dragDiv === division.name && dropIdx === idx}
                          draggable="true"
                          on:dragstart={(e) => handleDragStart(e, division.name, idx)}
                          on:dragover={(e) => handleDragOver(e, idx)}
                          on:drop|preventDefault={() => handleDrop(division.name, idx)}
                          on:dragend={handleDragEnd}
                        >
                          <span class="sort-handle">&#x2630;</span>
                          <span class="sort-color" style="background: {stop.color || '#999'}"></span>
                          <span class="sort-name">{stop.name}</span>
                          <span class="sort-pitch">{stop.pitch || ''}</span>
                          <button class="btn btn-ghost btn-xs" title="Omhoog" disabled={idx === 0}
                            on:click|stopPropagation={() => moveStop(division.name, idx, -1)}>▲</button>
                          <button class="btn btn-ghost btn-xs" title="Omlaag" disabled={idx === division.stops.length - 1}
                            on:click|stopPropagation={() => moveStop(division.name, idx, 1)}>▼</button>
                        </div>
                      {/each}
                    </div>
                  </div>
                {/each}
              {/if}
            </div>

            <!-- Instellingen Export/Import -->
            <div class="settings-block">
              <h3 class="settings-block-title">{$t('settings.export_import')}</h3>
              <div style="display: flex; flex-direction: column; gap: 0.4rem;">
                <button class="btn btn-secondary btn-sm" on:click={exportSettings}>
                  Exporteer instellingen...
                </button>
                <button class="btn btn-secondary btn-sm" on:click={importSettings}>
                  Importeer instellingen...
                </button>
              </div>
              <p style="font-size: 0.7rem; color: var(--text-secondary); margin-top: 0.4rem;">
                Instellingen worden automatisch opgeslagen in de orgelmap en in de app.
              </p>
            </div>
          </div>

          <!-- Kolom 2: MIDI Mapping + Layout -->
          <div class="instellingen-section">
            <div class="midi-mapping-container">
              <div class="midi-mapping-header">
                <h3>{$t('settings.midi_mapping_title')}</h3>
                <p>Druk eerst de <strong>laagste toets</strong>, dan de <strong>hoogste toets</strong>.</p>
              </div>

              {#if audioChannelCount >= 2}
                <div class="ccis-global" title="Globale C/Cis-lade spreiding — werkt per klavier waar je hieronder de spreiding aanzet.">
                  <div style="display:flex; align-items:center; gap:0.5rem; margin-bottom:0.4rem;">
                    <strong style="color: var(--text);">C/Cis-lade spreiding (globaal)</strong>
                    <span style="font-size:0.72rem; color:var(--text-muted);">— C (en D, E, …) naar de ene kant, Cis (en Dis, F, …) naar de andere</span>
                  </div>
                  <div class="swell-config-sliders">
                    <div class="swell-config-row" title="Hoe sterk de twee laden van elkaar gescheiden klinken (0% = mono, 95% = bijna volledig L/R)">
                      <span class="swell-config-label">Sterkte</span>
                      <input type="range" min="0" max="100" step="5" bind:value={ccisStrength} on:change={applyCcisSpread} />
                      <span class="swell-config-value">{ccisStrength}%</span>
                    </div>
                    <div class="swell-config-row" title="Hoe snel de spreiding in elkaar overloopt naar boven toe (0% = bovenin nog even sterk, 100% = bovenin volledig mono — zoals een echt orgel)">
                      <span class="swell-config-label">Afval</span>
                      <input type="range" min="0" max="100" step="5" bind:value={ccisFalloff} on:change={applyCcisSpread} />
                      <span class="swell-config-value">{ccisFalloff}%</span>
                    </div>
                    <div class="swell-config-row" title="Wissel welke kant C en welke kant Cis is">
                      <span class="swell-config-label">Kanten</span>
                      <label class="swell-toggle" style="flex:1;">
                        <input type="checkbox" bind:checked={ccisSwap} on:change={applyCcisSpread} />
                        <span>omdraaien (Cis links, C rechts)</span>
                      </label>
                    </div>
                  </div>
                </div>
              {/if}

              {#each displayOrgan.divisions as division, divIdx}
                {@const m = midiMapByDiv[division.name]}
                <div class="midi-mapping-row">
                  <div class="midi-mapping-division">
                    <span class="division-label">{division.name}</span>
                    {#if m && m.first_midi_note != null && m.last_midi_note != null}
                      <span class="division-range">({midiToNoteName(m.first_midi_note)} - {midiToNoteName(m.last_midi_note)})</span>
                    {/if}
                  </div>

                  <div class="midi-mapping-controls">
                    <div class="midi-channel-select">
                      <label for="channel-{division.name}">Kanaal:</label>
                      <select
                        id="channel-{division.name}"
                        value={(m?.channel) ?? ''}
                        on:change={(e) => setMidiChannel(division.name, e.target.value === '' ? null : parseInt(e.target.value))}
                      >
                        <option value="">Alle</option>
                        {#each Array(16).fill().map((_, i) => i + 1) as ch}
                          <option value={ch}>{ch}</option>
                        {/each}
                      </select>
                    </div>

                    <button
                      class="btn btn-secondary midi-learn-btn"
                      class:learning={learningDivision === division.name}
                      on:click={() => learnChannel(division.name)}
                      disabled={learningDivision !== null && learningDivision !== division.name}
                    >
                      {#if learningDivision === division.name}
                        <span class="learning-indicator"></span>
                        {#if learningStep === 1}
                          Laagste toets...
                        {:else if learningStep === 2}
                          Hoogste toets...
                        {:else}
                          Leren...
                        {/if}
                      {:else}
                        Leer
                      {/if}
                    </button>

                    <div class="midi-transpose">
                      <label for="transpose-{division.name}">Transp:</label>
                      <input
                        id="transpose-{division.name}"
                        type="number"
                        min="-24"
                        max="24"
                        value={m?.transpose ?? 0}
                        on:change={(e) => setTranspose(division.name, parseInt(e.target.value) || 0)}
                      />
                    </div>

                    <label class="swell-toggle" title="Zwelkast aan/uit">
                      <input
                        type="checkbox"
                        checked={isSwellEnabled(division.name)}
                        on:change={() => toggleSwellEnabled(division.name)}
                      />
                      <span class="swell-toggle-label">Zwelkast</span>
                    </label>
                    {#if isSwellEnabled(division.name)}
                      <button
                        class="btn btn-secondary midi-learn-btn swell-learn-btn"
                        class:learning={learningSwellDivision === division.name}
                        on:click={() => learnSwellPedal(division.name)}
                        disabled={learningSwellDivision !== null && learningSwellDivision !== division.name}
                      >
                        {#if learningSwellDivision === division.name}
                          <span class="learning-indicator"></span>
                          Laagste stand...
                        {:else if swellBindings[division.name]}
                          CC{swellBindings[division.name].cc_num}
                        {:else}
                          Leer pedaal
                        {/if}
                      </button>
                      {#if swellBindings[division.name]}
                        <button
                          class="btn btn-ghost btn-sm swell-clear-btn"
                          on:click={() => clearSwellBinding(division.name)}
                          title="Wis zwelpedaal binding"
                        >&#x2715;</button>
                        <label class="swell-toggle" title="Spiegelbeeld: keert de pedaalrichting om (handig als de pedaal de andere kant op werkt)">
                          <input
                            type="checkbox"
                            checked={swellBindings[division.name].invert === true}
                            on:change={(e) => toggleSwellInvert(division.name, e.target.checked)}
                          />
                          <span>Spiegelbeeld</span>
                        </label>
                        <div class="swell-config-row" title="Handmatig CC-bereik van de zwelpedaal (0–127). Min = dicht, Max = open.">
                          <span class="swell-config-label">Pedaal-bereik</span>
                          <input type="number" min="0" max="127" class="pedal-range-input"
                            value={swellBindings[division.name].min_val}
                            on:change={(e) => setSwellRangeUI(division.name, parseInt(e.target.value), swellBindings[division.name].max_val)}
                            title="Min (dicht)"
                          />
                          <span style="color:var(--text-muted);">–</span>
                          <input type="number" min="0" max="127" class="pedal-range-input"
                            value={swellBindings[division.name].max_val}
                            on:change={(e) => setSwellRangeUI(division.name, swellBindings[division.name].min_val, parseInt(e.target.value))}
                            title="Max (open)"
                          />
                        </div>
                        <!-- Live volgbalk: toont de actuele zwelstand terwijl je het pedaal beweegt. -->
                        <div class="swell-config-row" title="Live zwelstand (volgt het pedaal)">
                          <span class="swell-config-label">Live</span>
                          <div class="follow-bar">
                            <div class="follow-bar-fill" style="width: {Math.round(getSwellLevel(division.name) * 100)}%"></div>
                          </div>
                          <span class="swell-config-value">{Math.round(getSwellLevel(division.name) * 100)}%</span>
                        </div>
                      {/if}
                      <div class="swell-config-sliders">
                        <div class="swell-config-row">
                          <span class="swell-config-label">Min. volume</span>
                          <input type="range" min="-30" max="-5" step="1"
                            value={getSwellMinDb(division.name)}
                            on:input={(e) => { swellMinDb[division.name] = parseInt(e.target.value); swellMinDb = swellMinDb; updateSwellConfig(division.name); }}
                          />
                          <span class="swell-config-value">{getSwellMinDb(division.name)} dB</span>
                        </div>
                        <div class="swell-config-row">
                          <span class="swell-config-label">Filter dicht</span>
                          <input type="range" min="200" max="5000" step="50"
                            value={getSwellFilterCutoff(division.name)}
                            on:input={(e) => { swellFilterCutoff[division.name] = parseInt(e.target.value); swellFilterCutoff = swellFilterCutoff; updateSwellConfig(division.name); }}
                          />
                          <span class="swell-config-value">{getSwellFilterCutoff(division.name)} Hz</span>
                        </div>
                      </div>
                    {/if}

                    <!-- Wind-groep keuze (per-groep config zit in aparte sectie boven) -->
                    <div style="display:flex; align-items:center; gap:0.6rem; margin-top:0.3rem; flex-wrap:wrap;">
                      <label style="display:flex; align-items:center; gap:0.35rem; font-size:0.78rem; color:var(--text-secondary);">
                        <span title="Wind-groep: divisies in dezelfde groep delen één wind-reservoir">Windvoorziening</span>
                        <select
                          value={getWindGroup(division.name, divIdx)}
                          on:change={(e) => onWindGroupChange(division.name, e.target.value)}
                          style="padding:0.25rem 0.5rem; background:var(--bg-elevated); border:1px solid var(--accent-soft-2); border-radius:var(--radius-sm); color:var(--text); font-size:0.78rem;"
                          title="Kies wind-groep. Hetzelfde nummer kiezen voor twee divisies = gedeeld reservoir (bv. pedaal apart, 2 manualen samen)."
                        >
                          {#each Array(Math.max(1, Math.min(numWindGroups, 8))) as _, gIdx}
                            <option value={gIdx}>Groep {gIdx + 1}</option>
                          {/each}
                        </select>
                      </label>
                    </div>

                    <!-- Stereo Pan -->
                    <div class="swell-config-sliders" style="margin-top: 0.3rem;">
                      <div class="swell-config-row">
                        <span class="swell-config-label">Pan</span>
                        <input type="range" min="-100" max="100" step="5"
                          value={getDivisionPan(division.name)}
                          on:input={(e) => onPanChange(division.name, parseInt(e.target.value))}
                        />
                        <span class="swell-config-value">{formatPan(getDivisionPan(division.name))}</span>
                      </div>
                      {#if audioChannelCount >= 2}
                        {@const divChannels = Array.isArray(divisionOutputChannels[division.name]) ? divisionOutputChannels[division.name] : []}
                        <div class="swell-config-row" title="Kies vrij welke fysieke output-kanalen deze divisie gebruikt (1 t/m alle). Opeenvolgende kanalen vormen een stereo paar.">
                          <span class="swell-config-label">Kanalen</span>
                          <div style="display:flex; flex-wrap:wrap; gap:0.25rem; flex:1;">
                            {#each Array(audioChannelCount) as _, ch}
                              {@const isOn = divChannels.includes(ch)}
                              <label class="ch-chip" class:on={isOn} title="{channelLabel(ch, audioChannelCount)}">
                                <input type="checkbox" checked={isOn} on:change={() => toggleDivisionChannel(division.name, ch)} />
                                <span>{ch + 1}</span>
                              </label>
                            {/each}
                          </div>
                          <span class="swell-config-value">{audioChannelCount}ch</span>
                        </div>
                        <div class="swell-config-row" title="C/Cis-lade spreiding: even tonen klinken naar de ene kant, oneven naar de andere — zoals een echt orgel met afzonderlijke C- en Cis-laden. Voor (laagste) tonen sterk gescheiden, naar boven steeds meer ineenlopend.">
                          <span class="swell-config-label">C/Cis</span>
                          <label class="swell-toggle" style="flex:1;">
                            <input type="checkbox" checked={getDivisionCcis(division.name)} on:change={() => toggleDivisionCcis(division.name)} />
                            <span>spreiding aan op dit klavier</span>
                          </label>
                        </div>
                      {/if}
                    </div>

                    <!-- Tremulant LFO + MIDI learn knopje -->
                    <div style="display:flex; align-items:center; gap:0.5rem; margin-top:0.3rem;">
                      <label class="swell-toggle" title="Tremulant beschikbaar maken op dit klavier — er verschijnt dan een Tremulant-register in de orgelweergave">
                        <input
                          type="checkbox"
                          checked={tremLfoEnabled[division.name] === true}
                          on:change={() => setTremEnabled(division, tremLfoEnabled[division.name] !== true)}
                        />
                        <span class="swell-toggle-label">Tremulant (LFO)</span>
                      </label>
                      <button
                        class="btn btn-ghost btn-sm"
                        class:learning={learningTremDivIdx === divIdx}
                        on:click={() => learnTremulantLfo(divIdx)}
                        disabled={learningTremDivIdx !== null && learningTremDivIdx !== divIdx}
                        title="MIDI inleren voor Tremulant LFO ({tremLfoMidiBindings[ACTION_TREM_LFO_BASE + divIdx] || 0}/4)"
                        style="font-size:0.7rem; padding:0.2rem 0.5rem; border:1px solid {tremLfoMidiBindings[ACTION_TREM_LFO_BASE + divIdx] > 0 ? 'var(--midi-indicator)' : 'var(--accent-soft-2)'};"
                      >
                        {#if learningTremDivIdx === divIdx}
                          <span class="learning-indicator"></span>Wacht...
                        {:else}
                          MIDI {tremLfoMidiBindings[ACTION_TREM_LFO_BASE + divIdx] || 0}/4
                        {/if}
                      </button>
                    </div>
                    {#if tremLfoEnabled[division.name]}
                      <div class="swell-config-sliders">
                        <div class="swell-config-row">
                          <span class="swell-config-label">Snelheid</span>
                          <input type="range" min="30" max="100" step="5"
                            value={(tremLfoRate[division.name] ?? 6.0) * 10}
                            on:input={(e) => { tremLfoRate[division.name] = parseInt(e.target.value) / 10; tremLfoRate = tremLfoRate; updateTremLfoParams(division.name); }}
                          />
                          <span class="swell-config-value">{(tremLfoRate[division.name] ?? 6.0).toFixed(1)} Hz</span>
                        </div>
                        <div class="swell-config-row">
                          <span class="swell-config-label">Volume</span>
                          <input type="range" min="1" max="25" step="1"
                            value={tremLfoAmpDepth[division.name] ?? 10}
                            on:input={(e) => { tremLfoAmpDepth[division.name] = parseInt(e.target.value); tremLfoAmpDepth = tremLfoAmpDepth; updateTremLfoParams(division.name); }}
                          />
                          <span class="swell-config-value">{tremLfoAmpDepth[division.name] ?? 10}%</span>
                        </div>
                        <div class="swell-config-row">
                          <span class="swell-config-label">Pitch</span>
                          <input type="range" min="5" max="40" step="1"
                            value={tremLfoPitchDepth[division.name] ?? 15}
                            on:input={(e) => { tremLfoPitchDepth[division.name] = parseInt(e.target.value); tremLfoPitchDepth = tremLfoPitchDepth; updateTremLfoParams(division.name); }}
                          />
                          <span class="swell-config-value">{tremLfoPitchDepth[division.name] ?? 15} ct</span>
                        </div>
                      </div>
                    {/if}
                  </div>
                </div>
              {/each}

              <!-- Koppels -->
              {#if displayOrgan.couplers && displayOrgan.couplers.length > 0}
                <div class="midi-mapping-header" style="margin-top: 1.5rem;">
                  <h3>Koppels</h3>
                </div>
                {#each displayOrgan.couplers as coupler}
                  <div class="midi-mapping-row coupler-mapping-row">
                    <div class="midi-mapping-division">
                      <label class="coupler-check">
                        <input
                          type="checkbox"
                          checked={isCouplerVisible(coupler.id)}
                          on:change={() => toggleCouplerVisibility(coupler.id)}
                        />
                        <span class="division-label">{coupler.name.replace(/\n/g, ' ')}</span>
                      </label>
                    </div>
                    <div class="midi-mapping-controls">
                      <button
                        class="btn btn-secondary midi-learn-btn swell-learn-btn"
                        on:click={() => learnCouplerMidi(coupler.midi_action_code)}
                      >
                        MIDI ({couplerMidiBindings[coupler.midi_action_code] || 0}/4)
                      </button>
                      {#if couplerMidiBindings[coupler.midi_action_code] > 0}
                        <button
                          class="btn btn-ghost btn-sm swell-clear-btn"
                          on:click={() => clearCouplerMidi(coupler.midi_action_code)}
                          title="Wis MIDI binding"
                        >&#x2715;</button>
                      {/if}
                    </div>
                  </div>
                {/each}
              {/if}
            </div>
          </div>

        </div>

        <!-- MIDI Speler -->
        <div style="margin-top: 1.5rem; max-width: 1200px;">
          <div class="settings-block" style="padding: 0;">
            <MidiPlayer />
          </div>
        </div>

        <!-- Pijp-voicing (volle breedte onderaan) -->
        <div style="margin-top: 1.5rem; max-width: 1200px;">
          <div class="settings-block" style="padding: 0;">
            <VoicingPanel organ={displayOrgan} on:persistSettings={() => dispatch('persistSettings')} />
          </div>
        </div>
      </div>

    {:else if activeView === 'algemene-instellingen'}
      <!-- ============ ALGEMENE INSTELLINGEN ============ -->
      <div class="instellingen-view">
        {#if !displayOrgan}
          <div style="margin-bottom: 1rem;">
            <button class="btn btn-ghost btn-sm" on:click={() => { showOrganBrowser = true; }}>
              {$t('nav.back_to_library')}
            </button>
          </div>
        {/if}
        <div class="instellingen-columns">
          <!-- Kolom 1: Audio + MIDI devices -->
          <div class="instellingen-section">
            <!-- Audio Uitvoer -->
            <div class="settings-block">
              <h3 class="settings-block-title">{$t('settings.audio_output')}</h3>

              <!-- Host / driver -->
              <div class="audio-select-row">
                <label class="audio-select-label" for="audio-host">{$t('settings.audio_host')}</label>
                <select
                  id="audio-host"
                  class="temperament-select"
                  on:change={(e) => dispatch('selectAudioHost', e.target.value)}
                >
                  {#each audioHosts as h}
                    <option value={h} selected={h === selectedAudioHost}>{prettyHost(h)}</option>
                  {/each}
                  {#if audioHosts.length === 0}
                    <option value="">—</option>
                  {/if}
                </select>
              </div>

              <!-- Output device -->
              <div class="device-select">
                {#each audioDevices as device}
                  <div
                    class="device-item"
                    class:selected={selectedAudioDevice === device.name}
                    on:click={() => dispatch('selectAudioDevice', device.name)}
                    on:keypress={(e) => e.key === 'Enter' && dispatch('selectAudioDevice', device.name)}
                    role="button"
                    tabindex="0"
                    title={device.name}
                  >
                    <div class="device-icon">
                      {#if deviceKind(device.name) === 'headphone'}
                        <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                          <path d="M3 14v-2a9 9 0 0 1 18 0v2"/>
                          <path d="M21 16a2 2 0 0 1-2 2h-1v-6h1a2 2 0 0 1 2 2z"/>
                          <path d="M3 16a2 2 0 0 0 2 2h1v-6H5a2 2 0 0 0-2 2z"/>
                        </svg>
                      {:else if deviceKind(device.name) === 'speaker'}
                        <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                          <rect x="6" y="2" width="12" height="20" rx="2"/>
                          <circle cx="12" cy="14" r="4"/>
                          <line x1="12" y1="6" x2="12.01" y2="6"/>
                        </svg>
                      {:else}
                        <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                          <polygon points="11 5 6 9 2 9 2 15 6 15 11 19 11 5"/>
                          <path d="M15.54 8.46a5 5 0 0 1 0 7.07"/>
                          <path d="M19.07 4.93a10 10 0 0 1 0 14.14"/>
                        </svg>
                      {/if}
                    </div>
                    <div class="device-info">
                      <div class="device-name">{deviceLabel(device.name)}</div>
                      <div class="device-status">
                        {#if deviceSub(device.name)}{deviceSub(device.name)}{/if}
                        {#if device.is_default}{deviceSub(device.name) ? ' · ' : ''}{$t('settings.default_device')}{/if}
                      </div>
                    </div>
                  </div>
                {/each}
                {#if audioDevices.length === 0}
                  <div class="device-item" style="opacity: 0.5; cursor: default;">
                    <div class="device-icon">
                      <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                        <line x1="1" y1="1" x2="23" y2="23"/>
                        <polygon points="11 5 6 9 2 9 2 15 6 15 11 19 11 5"/>
                      </svg>
                    </div>
                    <div class="device-info">
                      <div class="device-name">{$t('settings.no_audio_devices')}</div>
                    </div>
                  </div>
                {/if}
              </div>

              {#if isAsioHost}
                <!-- ASIO levert één apparaat; uitgang-keuze zit in het ASIO-paneel. -->
                <div class="audio-hint audio-hint-asio">
                  <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" style="flex-shrink:0;margin-top:1px;">
                    <circle cx="12" cy="12" r="10"/>
                    <line x1="12" y1="16" x2="12" y2="12"/>
                    <line x1="12" y1="8" x2="12.01" y2="8"/>
                  </svg>
                  <span>{$t('settings.audio_asio_hint')}</span>
                </div>
              {/if}

              <!-- Buffer size + apply -->
              <div class="audio-select-row" style="margin-top: 0.7rem;">
                <label class="audio-select-label" for="audio-buffer">{$t('settings.audio_buffer')}</label>
                <select
                  id="audio-buffer"
                  class="temperament-select"
                  on:change={(e) => dispatch('selectBuffer', e.target.value ? Number(e.target.value) : null)}
                >
                  <option value="" selected={!selectedBufferFrames}>{$t('settings.audio_buffer_default')}</option>
                  {#each [32, 48, 64, 96, 128, 256, 512, 1024] as b}
                    <option value={b} selected={b === selectedBufferFrames}>{b} ({(b / 48).toFixed(1)} ms @48k)</option>
                  {/each}
                </select>
              </div>
              <p style="margin: 0.3rem 0 0; font-size: 0.68rem; color: var(--text-muted); line-height: 1.35;">
                Laagste latency: kies <strong>ASIO</strong> als host en een kleine buffer (32–64). WASAPI (gedeeld) negeert kleine buffers en blijft op de driver-default.
              </p>
              <div style="margin-top: 0.7rem;">
                <button class="btn btn-primary btn-sm" on:click={() => dispatch('applyAudioOutput')}>
                  {$t('settings.audio_apply')}
                </button>
              </div>
              <p style="margin: 0.5rem 0 0; font-size: 0.7rem; color: var(--text-muted); line-height: 1.4;">
                {$t('settings.audio_latency_hint')}
              </p>

              <!-- Automatisch starten bij Windows -->
              <div style="margin-top: 0.85rem; padding-top: 0.75rem; border-top: var(--border-subtle);">
                <label class="swell-toggle" title="JM-Orgue wordt na aanmelden automatisch gestart. Handig bij een vaste console-PC.">
                  <input
                    type="checkbox"
                    checked={autostartEnabled}
                    on:change={(e) => dispatch('setAutostart', e.target.checked)}
                  />
                  <span>Automatisch starten bij Windows</span>
                </label>
                <label class="swell-toggle" style="margin-top:0.4rem;" title="Aan: bij het openen van een orgel komen de laatst getrokken registers terug. Uit: schone start (geen registers aan).">
                  <input
                    type="checkbox"
                    checked={restoreRegistration}
                    on:change={(e) => dispatch('setRestoreRegistration', e.target.checked)}
                  />
                  <span>Laatste registratie herstellen bij openen</span>
                </label>
              </div>

              <!-- Sample Rate info -->
              <div style="margin-top: 0.85rem; padding-top: 0.75rem; border-top: var(--border-subtle);">
                <div style="display: flex; justify-content: space-between; align-items: center; margin-bottom: 0.4rem;">
                  <span style="font-size: 0.8rem; color: var(--text-secondary); font-weight: 500;">{$t('settings.sample_rate')}</span>
                  <span style="font-family: 'JetBrains Mono', monospace; font-size: 0.85rem; color: {sampleRate >= 88200 ? 'var(--success)' : 'var(--text)'};">
                    {sampleRate > 0 ? `${sampleRate.toLocaleString()} Hz` : '—'}
                    {#if sampleRate >= 88200} (HD){/if}
                  </span>
                </div>
                <div style="display: flex; align-items: center; gap: 0.5rem; margin-bottom: 0.3rem;">
                  <button class="btn btn-ghost btn-sm" on:click={loadSupportedSampleRates}>
                    {$t('settings.supported_rates')}
                  </button>
                  {#if supportedSampleRates.length > 0}
                    <span style="font-size: 0.72rem; color: var(--text-muted);">
                      {supportedSampleRates.map(r => r.toLocaleString()).join(' • ')} Hz
                    </span>
                  {/if}
                </div>
                <p style="margin: 0; font-size: 0.7rem; color: var(--text-muted); line-height: 1.4;">
                  {$t('settings.sample_rate_hint')}
                </p>
              </div>
            </div>

            <!-- Sampleset: aanloop-stilte bijsnijden -->
            <div class="settings-block">
              <h3 class="settings-block-title">{$t('settings.sampleset_title')}</h3>
              {#if !samplesetDir()}
                <p style="font-size:0.75rem; color:var(--text-muted);">{$t('settings.sampleset_no_organ')}</p>
              {:else}
                <p style="font-size:0.72rem; color:var(--text-muted); line-height:1.4; margin:0 0 0.5rem;">{$t('settings.sampleset_desc')}</p>
                <div class="audio-select-row">
                  <label class="audio-select-label" for="ss-thr">{$t('settings.sampleset_threshold')}</label>
                  <input id="ss-thr" type="number" class="temperament-select" style="width:6rem;" bind:value={samplesetThresholdDb} min="-90" max="-20" step="1" />
                </div>
                <div class="audio-select-row" style="margin-top:0.5rem;">
                  <label class="audio-select-label" for="ss-pre">{$t('settings.sampleset_preroll')}</label>
                  <input id="ss-pre" type="number" class="temperament-select" style="width:6rem;" bind:value={samplesetPrerollMs} min="0" max="50" step="1" />
                </div>
                <div style="display:flex; gap:0.5rem; margin-top:0.7rem; flex-wrap:wrap;">
                  <button class="btn btn-secondary btn-sm" on:click={scanSampleset} disabled={samplesetBusy}>{$t('settings.sampleset_scan')}</button>
                  <button class="btn btn-primary btn-sm" on:click={trimSampleset} disabled={samplesetBusy || !samplesetScan || samplesetScan.files_with_silence === 0}>{$t('settings.sampleset_trim')}</button>
                </div>
                {#if samplesetBusy}
                  <p style="font-size:0.75rem; color:var(--text-secondary); margin-top:0.5rem;">{$t('settings.sampleset_busy')}</p>
                {/if}
                {#if samplesetScan}
                  <div style="margin-top:0.6rem; font-size:0.76rem;">
                    <div>{$t('settings.sampleset_scanned')}: {samplesetScan.files_scanned} · {$t('settings.sampleset_with_silence')}: <strong>{samplesetScan.files_with_silence}</strong> · {$t('settings.sampleset_total')}: {fmtMs(samplesetScan.total_leading_ms)}</div>
                    {#if samplesetScan.items.length > 0}
                      <ul style="margin:0.3rem 0 0; padding-left:1rem; max-height:140px; overflow:auto; color:var(--text-secondary);">
                        {#each samplesetScan.items.slice(0, 12) as it}
                          <li>{it.rel_path} — {fmtMs(it.leading_ms)}</li>
                        {/each}
                      </ul>
                      {#if samplesetScan.items.length > 12}
                        <div style="color:var(--text-muted);">… +{samplesetScan.items.length - 12}</div>
                      {/if}
                    {/if}
                    {#if samplesetScan.errors.length > 0}
                      <div style="color:#d47070; margin-top:0.3rem;">{samplesetScan.errors.length} ⚠</div>
                    {/if}
                  </div>
                {/if}
                {#if samplesetTrim}
                  <div style="margin-top:0.5rem; font-size:0.76rem; color:var(--success);">
                    {$t('settings.sampleset_trimmed')}: {samplesetTrim.files_trimmed} ({fmtMs(samplesetTrim.total_trimmed_ms)}) · {$t('settings.sampleset_backup')}: {samplesetTrim.backup_dir}
                  </div>
                {/if}
              {/if}
            </div>

            <!-- Sampleset: loops detecteren + inbakken -->
            <div class="settings-block">
              <h3 class="settings-block-title">Sampleset — loops</h3>
              {#if !samplesetDir()}
                <p style="font-size:0.75rem; color:var(--text-muted);">Laad eerst een orgel/sampleset.</p>
              {:else}
                <p style="font-size:0.72rem; color:var(--text-muted); line-height:1.4; margin:0 0 0.5rem;">
                  Detecteert per WAV een goede sustain-loop (grondtoon-periode, fase-uitgelijnd), bakt een crossfade in voor een naadloze overgang en schrijft de loop-punten in het bestand (smpl-chunk). Voor samplesets zonder loops, zodat noten blijven klinken. Originelen worden eerst geback-upt.
                </p>
                <label class="audio-select-row" style="cursor:pointer;">
                  <input type="checkbox" bind:checked={loopOnlyMissing} />
                  <span class="audio-select-label" style="margin-left:0.4rem;">Alleen bestanden zonder loop</span>
                </label>
                <div style="display:flex; gap:0.5rem; margin-top:0.7rem; flex-wrap:wrap;">
                  <button class="btn btn-secondary btn-sm" on:click={scanLoops} disabled={loopBusy}>Scannen</button>
                  <button class="btn btn-primary btn-sm" on:click={applyLoops} disabled={loopBusy || !loopScan || loopScan.files_loopable === 0}>Loops aanmaken</button>
                </div>
                {#if loopBusy}
                  <p style="font-size:0.75rem; color:var(--text-secondary); margin-top:0.5rem;">Bezig… (dit kan even duren bij veel bestanden)</p>
                {/if}
                {#if loopScan}
                  <div style="margin-top:0.6rem; font-size:0.76rem;">
                    <div>WAV's: {loopScan.files_scanned} · zonder loop: <strong>{loopScan.files_without_loop}</strong> · loopbaar: <strong>{loopScan.files_loopable}</strong></div>
                    {#if loopScan.errors.length > 0}
                      <div style="color:#d47070; margin-top:0.3rem;">{loopScan.errors.length} ⚠</div>
                    {/if}
                  </div>
                {/if}
                {#if loopApply}
                  <div style="margin-top:0.5rem; font-size:0.76rem; color:var(--success);">
                    Loops geschreven: {loopApply.files_looped} · overgeslagen: {loopApply.files_skipped} · backup: {loopApply.backup_dir}
                  </div>
                {/if}
              {/if}
            </div>

            <!-- MIDI Invoer -->
            <div class="settings-block">
              <div style="display: flex; justify-content: space-between; align-items: center; margin-bottom: 0.5rem;">
                <h3 class="settings-block-title" style="margin: 0;">{$t('settings.midi_input')}</h3>
                <button class="btn btn-ghost btn-sm" on:click={async () => { try { await invoke('refresh_midi_devices'); dispatch('refreshDevices'); } catch (e) {} }} title={$t('settings.midi_refresh')}>
                  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                    <path d="M23 4v6h-6M1 20v-6h6"/>
                    <path d="M3.51 9a9 9 0 0 1 14.85-3.36L23 10M1 14l4.64 4.36A9 9 0 0 0 20.49 15"/>
                  </svg>
                  {$t('settings.midi_refresh')}
                </button>
              </div>
              <div class="device-select">
                {#each midiDevices.filter(d => d.is_input) as device}
                  <div
                    class="device-item"
                    class:selected={selectedMidiDevice === device.name}
                    on:click={() => dispatch('connectMidi', device.name)}
                    on:keypress={(e) => e.key === 'Enter' && dispatch('connectMidi', device.name)}
                    role="button"
                    tabindex="0"
                  >
                    <div class="device-icon">
                      {#if device.is_bluetooth}
                        <svg width="18" height="18" viewBox="0 0 24 24" fill="none" style="stroke: var(--led-blue);" stroke-width="2">
                          <path d="M6.5 6.5l11 11L12 23V1l5.5 5.5-11 11"/>
                        </svg>
                      {:else}
                        <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                          <rect x="2" y="10" width="20" height="10" rx="2"/>
                          <circle cx="6" cy="15" r="1.5" fill="currentColor"/>
                          <circle cx="10" cy="15" r="1.5" fill="currentColor"/>
                          <circle cx="14" cy="15" r="1.5" fill="currentColor"/>
                          <circle cx="18" cy="15" r="1.5" fill="currentColor"/>
                          <path d="M6 10V6a2 2 0 0 1 2-2h8a2 2 0 0 1 2 2v4"/>
                        </svg>
                      {/if}
                    </div>
                    <div class="device-info">
                      <div class="device-name">
                        {device.name}
                        {#if device.is_bluetooth}
                          <span class="bt-badge">BT</span>
                        {/if}
                      </div>
                      <div class="device-status">
                        {#if selectedMidiDevice === device.name}
                          <span style="color: var(--success);">{$t('midi.connected')}</span>
                        {:else}
                          {$t('midi.connect_hint')}
                        {/if}
                      </div>
                    </div>
                  </div>
                {/each}
                {#if midiDevices.filter(d => d.is_input).length === 0}
                  <div class="device-item" style="opacity: 0.5; cursor: default;">
                    <div class="device-icon">
                      <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                        <line x1="1" y1="1" x2="23" y2="23"/>
                        <rect x="2" y="10" width="20" height="10" rx="2"/>
                      </svg>
                    </div>
                    <div class="device-info">
                      <div class="device-name">{$t('settings.midi_no_input_devices')}</div>
                      <div class="device-status">{$t('settings.midi_connect_keyboard')}</div>
                    </div>
                  </div>
                {/if}
              </div>
              <!-- Bluetooth LE MIDI -->
              <div class="ble-midi-section">
                <div style="display: flex; justify-content: space-between; align-items: center; margin-bottom: 0.4rem;">
                  <h4 style="margin: 0; font-size: 0.85rem; color: var(--led-blue);">
                    {$t('settings.midi_bluetooth_section')}
                    {#if bleMessageCount > 0}
                      <span class="ble-activity">● {bleMessageCount} {$t('settings.midi_messages')}</span>
                    {/if}
                  </h4>
                  <button class="btn btn-secondary btn-sm" on:click={scanBleMidi} disabled={bleScanning}>
                    {bleScanning ? $t('settings.midi_bluetooth_scanning') : $t('settings.midi_bluetooth_scan')}
                  </button>
                </div>
                {#if bleDevices.length > 0}
                  <div class="device-select">
                    {#each bleDevices as device}
                      <div class="device-item" class:selected={device.connected}
                        on:click={() => toggleBleDevice(device)}
                        on:keypress={(e) => e.key === 'Enter' && toggleBleDevice(device)}
                        role="button" tabindex="0">
                        <div class="device-icon">
                          <svg width="18" height="18" viewBox="0 0 24 24" fill="none" style="stroke: var(--led-blue);" stroke-width="2">
                            <path d="M6.5 6.5l11 11L12 23V1l5.5 5.5-11 11"/>
                          </svg>
                        </div>
                        <div class="device-info">
                          <div class="device-name">{device.name} <span class="bt-badge">BLE</span></div>
                          <div class="device-status">
                            {#if device.connected}
                              <span style="color: var(--success);">{$t('midi.connected')}</span>
                            {:else if device.rssi !== null}
                              {device.rssi} dBm — {$t('midi.connect_hint')}
                            {:else}
                              {$t('midi.connect_hint')}
                            {/if}
                          </div>
                        </div>
                      </div>
                    {/each}
                  </div>
                {:else}
                  <div class="bt-help" style="margin-top: 0.3rem; font-size: 0.72rem;">
                    <strong>{$t('settings.midi_bluetooth_help_strong')}</strong> {$t('settings.midi_bluetooth_help')}
                  </div>
                {/if}
              </div>
            </div>
          </div>

          <!-- Kolom 2: Sfeer & Layout -->
          <div class="instellingen-section">
            <LayoutSettings />
          </div>
        </div>
      </div>
    {/if}

  <!-- Coupler MIDI context menu -->
  {#if couplerContextMenu}
    <div class="coupler-context-menu" style="left: {couplerContextMenu.x}px; top: {couplerContextMenu.y}px;">
      <button on:click={() => learnCouplerMidi(couplerContextMenu.action)}>
        MIDI Inleren ({couplerMidiBindings[couplerContextMenu.action] || 0}/4)
      </button>
      {#if couplerMidiBindings[couplerContextMenu.action] > 0}
        <button on:click={() => clearCouplerMidi(couplerContextMenu.action)}>
          MIDI Wissen
        </button>
      {/if}
    </div>
  {/if}

  <!-- Stop MIDI context menu -->
  {#if stopContextMenu}
    <div class="coupler-context-menu" style="left: {stopContextMenu.x}px; top: {stopContextMenu.y}px;">
      <button on:click={() => learnStopMidi(stopContextMenu.action)}>
        MIDI Inleren ({stopMidiBindings[stopContextMenu.action] || 0}/4)
      </button>
      {#if stopMidiBindings[stopContextMenu.action] > 0}
        <button on:click={() => clearStopMidi(stopContextMenu.action)}>
          MIDI Wissen
        </button>
      {/if}
    </div>
  {/if}

  {:else}
    <div class="empty-state fade-in">
      <div class="empty-state-icon">
        <svg width="64" height="64" viewBox="0 0 36 36" fill="none">
          <rect x="4" y="6" width="5" height="26" rx="1.5" fill="currentColor" opacity="0.2"/>
          <rect x="11" y="10" width="5" height="22" rx="1.5" fill="currentColor" opacity="0.3"/>
          <rect x="18" y="4" width="5" height="28" rx="1.5" fill="currentColor" opacity="0.4"/>
          <rect x="25" y="8" width="5" height="24" rx="1.5" fill="currentColor" opacity="0.3"/>
        </svg>
      </div>
      <h2 class="empty-state-title">Geen orgel geladen</h2>
      <p class="empty-state-text">Selecteer een orgel uit de bibliotheek.</p>
      <button class="btn btn-primary" on:click={() => showOrganBrowser = true}>
        Open Orgel Bibliotheek
      </button>
    </div>
  {/if}
</main>
