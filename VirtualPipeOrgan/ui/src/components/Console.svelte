<script>
  import { createEventDispatcher, onMount, onDestroy } from 'svelte';
  import { open } from '@tauri-apps/plugin-dialog';
  import LayoutSettings from './LayoutSettings.svelte';
  import VoicingPanel from './VoicingPanel.svelte';
  import MidiPlayer from './MidiPlayer.svelte';
  import { t, tx } from '../lib/i18n.js';
  import SetzerBar from './SetzerBar.svelte';
  import { loadPanelState, savePanelState } from '../lib/panelState.js';
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
  // Secundaire modus: deze Console draait in een extra registerscherm
  // (PanelApp). Backend-pushes bij orgelwissel en MIDI-trigger-consumptie
  // blijven dan uit (het hoofdvenster doet die), en divisiekeuze/layout
  // worden per scherm bewaard (panelNumber) i.p.v. in de gedeelde keys.
  export let secondary = false;
  export let panelNumber = null;
  // Secundair + hoofdbalk verborgen: toon in de werkbalk een knop om de balk
  // (Header met tabs/profielwissel/start-stop) weer zichtbaar te maken.
  export let showPanelHeaderToggle = false;
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
  // Audio-uitvoerprofielen (speakers/hoofdtelefoon) — beheerd in App.svelte.
  export let audioProfiles = { speakers: null, headphones: null, active: null };
  // Telt op bij elke geslaagde audio-uitgang-herbouw: de per-orgel DSP moet dan
  // opnieuw op de verse audio-thread toegepast worden, ook al is het orgel-id
  // gelijk gebleven (auditbevinding 20).
  export let audioEpoch = 0;
  // Laatst geopende orgel automatisch laden bij het starten (App.svelte).
  export let autoLoadLastOrgan = true;

  const dispatch = createEventDispatcher();

  // --- Audio device UX helpers ---------------------------------------------
  // ASIO drivers (bv. ASIO4ALL) leveren maar één apparaat; de keuze van de
  // fysieke uitgang (koptelefoon/luidspreker) gebeurt in het ASIO-paneel, niet
  // hier. Dat detecteren we om een gerichte hint te tonen.
  $: isAsioHost = /asio/i.test(selectedAudioHost || '');

  // Actuele audio-status (host/buffer/samplerate/kanalen) voor de
  // latentie-indicatie in Algemene Instellingen.
  let audioStatus = null;
  async function refreshAudioStatus() {
    try { audioStatus = await invoke('get_status'); } catch (e) { /* polling */ }
  }
  $: if (activeView === 'algemene-instellingen') refreshAudioStatus();
  // Geschatte uitgangslatentie: ~2× de bufferduur (dubbele buffering in de
  // driver). 0 frames = driver-default (onbekend, WASAPI ≈ 10 ms per buffer).
  $: audioLatencyMs = (audioStatus && audioStatus.buffer_frames > 0 && audioStatus.sample_rate > 0)
    ? (2 * audioStatus.buffer_frames / audioStatus.sample_rate * 1000)
    : null;

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
  const ACTION_AUDIO_PROFILE = 43; // wissel speakers ↔ hoofdtelefoon
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
    } else if (actionCode === ACTION_AUDIO_PROFILE) {
      // Snelle wissel speakers ↔ hoofdtelefoon via MIDI (bv. een piston).
      dispatch('switchAudioProfile', null);
    } else if (actionCode === 42) {
      // Computer afsluiten — bubbel naar App.svelte zodat die bevestiging + opslag regelt
      dispatch('shutdownRequest');
    }
  }

  async function learnGlobalAction(actionCode) {
    globalLearningAction = actionCode;
    try {
      await invoke('learn_preset_binding', { presetNum: actionCode });
      await refreshMidiBindingsFull();
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
      await refreshMidiBindingsFull();
    } catch (e) {
      console.error('Tremulant MIDI learn failed:', e);
    } finally {
      learningTremDivIdx = null;
    }
  }

  // Tel bindings bij organ load — via de centrale structurele lijst, die
  // meteen ook de handmatige koppelingen-editor voedt.
  $: if (organInfo) {
    refreshMidiBindingsFull();
  }

  // Layout state for main view
  let mainLayout = 'horizontal';
  let stopSize = 100; // min-breedte registerknop in px (instelbaar)
  let knobShape = 'rect'; // 'rect' | 'round' — vorm van de registerknoppen (per orgel)

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

  // ---- Noteren: MIDI-opname als notenschrift in het notatievenster ----
  // Opent een eigen venster (#notation) dat de opname via de backend naar
  // MusicXML omzet en met OpenSheetMusicDisplay rendert (afdrukken/PDF/
  // MusicXML-opslag gebeurt dáár). Zonder recente opname: bestandskeuze.
  // Open het notatievenster in live-modus (verse Score) — geen bestand nodig.
  // Achter de rode opname-knop in het venster start capture_notation_event.
  async function openLiveNotation() {
    try {
      const { WebviewWindow } = await import('@tauri-apps/api/webviewWindow');
      const label = `notation-${Date.now()}`;
      const webview = new WebviewWindow(label, {
        url: `index.html#notation&live=1`,
        title: 'JM-Orgue - Live noteren',
        width: 1100, height: 800, center: true, resizable: true,
      });
      webview.once('tauri://error', (e) => {
        console.error('Live-notatievenster openen mislukt:', e);
        alert('Notatievenster openen mislukt.');
      });
    } catch (e) {
      alert(`Notatievenster openen mislukt: ${e}`);
    }
  }

  // (Oude file-modus openers verwijderd — één ingang naar notatie: het
  // live-notatievenster. Binnen het venster kies je via "Openen…" een
  // bestaand MIDI-bestand dat als import-take aan de score wordt toegevoegd.)
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
      // Secundair venster: alleen de UI-state laden — het hoofdvenster heeft de
      // config al naar de backend gepusht (dubbele push zou onnodig verkeer en
      // races geven).
      if (!secondary) await invoke('set_crescendo_config', { stages: crescendoStages, enabled: crescendoEnabled });
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
    try {
      const targetStops = await invoke('set_crescendo_stage', { stage });
      // De backend geeft null terug voor "niet toepassen" en een (mogelijk lege)
      // array voor "toepassen" — een lege stap moet registers ook kunnen
      // leegtrekken, dus alleen op array-zijn checken, niet op lengte.
      if (Array.isArray(targetStops)) {
        // Stappenteller alleen bijwerken als de stap echt is toegepast.
        crescendoStage = stage;
        // De backend zet hier alleen de trap-teller en geeft de doelregisters
        // terug; App.svelte past ze toe (on:crescendoChange → set_drawn_stops).
        dispatch('crescendoChange', targetStops);
      }
    } catch (e) {
      console.error('Failed to set crescendo stage:', e);
    }
  }

  async function learnCrescendoPedal() {
    await learnPedalFlow('crescendo');
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

  // EQ: vrije banden (GrandOrgue-stijl) — per band type, frequentie, gain,
  // bandbreedte (octaven) en doelkanaal (null = alle uitgangskanalen).
  let eqEnabled = false;
  const defaultEqBands = () => ([
    { enabled: true, band_type: 'lowshelf', freq: 200, gain_db: 0, bandwidth: 1.0, channel: null },
    { enabled: true, band_type: 'peak', freq: 1000, gain_db: 0, bandwidth: 2.0, channel: null },
    { enabled: true, band_type: 'highshelf', freq: 4000, gain_db: 0, bandwidth: 1.0, channel: null },
  ]);
  let eqBands = defaultEqBands();

  const eqBandTypes = [
    { value: 'peak', label: 'Piek' },
    { value: 'lowpass', label: 'Laagdoorlaat' },
    { value: 'highpass', label: 'Hoogdoorlaat' },
    { value: 'bandpass', label: 'Banddoorlaat' },
    { value: 'lowshelf', label: 'Lage shelf' },
    { value: 'highshelf', label: 'Hoge shelf' },
  ];

  // Log-schaal voor de frequentie-slider: 0..300 ↔ 20 Hz .. 20 kHz.
  const freqToSlider = (f) => Math.round(100 * Math.log10(Math.max(20, Math.min(20000, f)) / 20));
  const sliderToFreq = (v) => Math.round(20 * Math.pow(10, v / 100));

  // EQ wordt per orgel opgeslagen in de backend (.jm-settings.json) en bij load geladen
  // via loadAudioSettingsForOrgan() — niet meer in een globale localStorage-key.

  async function updateEq() {
    try {
      await invoke('set_eq_bands', {
        enabled: eqEnabled,
        bands: eqBands.map(b => ({
          enabled: !!b.enabled,
          band_type: b.band_type,
          freq: Number(b.freq) || 1000,
          gain_db: Number(b.gain_db) || 0,
          bandwidth: Number(b.bandwidth) || 1.0,
          channel: (b.channel === null || b.channel === undefined) ? null : Number(b.channel),
        })),
      });
    } catch (e) {
      console.error('Failed to set EQ:', e);
    }
  }

  function onEqBandChange() {
    eqBands = eqBands; // Svelte-reactiviteit na mutatie van een band-object
    updateEq();
  }

  function addEqBand() {
    eqBands = [...eqBands, { enabled: true, band_type: 'peak', freq: 1000, gain_db: 0, bandwidth: 2.0, channel: null }];
    updateEq();
  }

  function removeEqBand(idx) {
    eqBands = eqBands.filter((_, i) => i !== idx);
    updateEq();
  }

  let reverbIrLoaded = false;
  let reverbIrName = '';
  // Volledig pad van het geladen IR-bestand; wordt per orgel gepersisteerd en
  // bij orgel-load/audiowissel automatisch herladen (voorheen: irPath ging als
  // null de opslag in en was de eigen galm na elke herstart weg).
  let reverbIrPath = null;
  // Default 'algorithmic': convolutie zonder geladen IR-bestand gaf vroeger
  // helemaal geen galm ("galm werkt niet"). De engine valt inmiddels ook zelf
  // terug op de algoritmische galm zolang er geen IR geladen is.
  let reverbType = 'algorithmic'; // 'convolution' of 'algorithmic'
  // Preset-index 0..N-1 = vaste preset (zonder schuiven); -1 = "Eigen":
  // vrij instelbaar met schuiven, per orgel vastgelegd.
  let reverbPreset = 1; // 0=Kleine Kapel, 1=Dorpskerk, etc.; -1=Eigen
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
      // null in de opslag = "Eigen" (vrije schuifwaarden van dit orgel)
      preset: reverbPreset >= 0 ? reverbPreset : null,
      rt60: reverbRt60,
      preDelayMs: reverbPreDelay,
      damping: reverbDamping,
      roomSize: reverbRoomSize,
      mix: reverb,
      irPath: reverbIrPath,
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
    if (idx >= 0) {
      // Vaste preset: canonieke waarden toepassen; schuiven blijven verborgen.
      const p = reverbPresets[idx];
      reverbRt60 = p.rt60;
      reverbPreDelay = p.preDelay;
      reverbDamping = p.damping;
      reverbRoomSize = p.roomSize;
    }
    // Eigen (-1): huidige schuifwaarden zijn het vertrekpunt en worden per
    // orgel vastgelegd via persistReverbConfig (in updateAlgorithmicReverb).
    updateAlgorithmicReverb(idx >= 0);
  }

  // Stuur de (opgeslagen) reverb-instellingen naar de audio-engine. Nodig bij
  // opstarten en na elke organ-(her)load: een verse audio-thread start altijd op
  // standaard (convolutie zonder IR, mix 0) = geen galm. Zonder dit hoorde je
  // niets ondanks de UI-keuze "Algoritmisch + preset".
  async function applyReverbToBackend() {
    try {
      await invoke('set_reverb_type', { algorithmic: reverbType === 'algorithmic' });
      // FDN-parameters altijd meesturen: bij 'convolution' zonder geladen IR
      // valt de engine terug op de algoritmische galm, en die moet dan de
      // ingestelde ruimte/mix volgen in plaats van de fabrieksdefaults.
      await invoke('set_algorithmic_reverb', {
        preset: null,
        rt60: reverbRt60,
        preDelayMs: reverbPreDelay,
        damping: reverbDamping / 100,
        roomSize: reverbRoomSize / 100,
        mix: reverb / 100,
      });
      if (reverbType !== 'algorithmic') {
        // Convolutie: opgeslagen IR automatisch herladen (verse audio-thread
        // start altijd zonder IR) en de mix toepassen.
        if (reverbIrPath) {
          try {
            await invoke('load_impulse_response', { path: reverbIrPath });
            reverbIrLoaded = true;
            reverbIrName = reverbIrPath.split(/[/\\]/).pop();
          } catch (e) {
            console.error('Opgeslagen IR niet te herladen (bestand verplaatst?):', e);
          }
        }
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
      // Secundair venster pusht niet: het hoofdvenster heeft de galm al gezet;
      // een tweede set_algorithmic_reverb zou de galmstaart wissen.
      if (!secondary) applyReverbToBackend();
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
  let windAppliedFor = '';
  $: if (!secondary && organInfo?.id && `${organInfo.id}::${audioEpoch}` !== windAppliedFor) {
    // Alleen bij een echte orgel-load of audio-herbouw: dit vuurde voorheen bij
    // ELKE organInfo-wijziging (elke registerklik) en draaide een net gekozen
    // windgroep terug vóór de gebruiker kon opslaan (auditbevinding 19).
    windAppliedFor = `${organInfo.id}::${audioEpoch}`;
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
    return () => { if (bleCountInterval) clearInterval(bleCountInterval); };
  });

  // BLE-teller alleen pollen wanneer de Algemene Instellingen (met de
  // Bluetooth-sectie) zichtbaar zijn — elders is de waarde onzichtbaar.
  $: {
    if (activeView === 'algemene-instellingen' && !bleCountInterval) {
      bleCountInterval = setInterval(async () => {
        try {
          bleMessageCount = await invoke('get_ble_midi_count');
        } catch (e) {}
      }, 250);
    } else if (activeView !== 'algemene-instellingen' && bleCountInterval) {
      clearInterval(bleCountInterval);
      bleCountInterval = null;
    }
  }

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
  // (Auditbevinding 46: de reactive die hier has_tremulant-defaults zette is
  // verwijderd — hij draaide bij een orgelwissel met de stale map van het
  // vórige orgel en persisteerde die. Dezelfde defaulting gebeurt al in
  // loadAudioSettingsForOrgan, ná het resetten van de maps.)

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
    // Alleen het hoofdvenster schrijft de defaults terug (single writer).
    if (!secondary) localStorage.setItem(organUiKey('jm-orgue-swell-enabled'), JSON.stringify(swellEnabled));
    // Koppel-zichtbaarheid
    try { visibleCouplers = JSON.parse(readOrganUiPref('jm-orgue-visible-couplers') || '{}'); } catch (e) { visibleCouplers = {}; }
    for (const c of (organInfo.couplers || [])) {
      if (c.id && c.id.startsWith('real_coupler_') && visibleCouplers[c.id] === undefined) visibleCouplers[c.id] = true;
    }
    visibleCouplers = visibleCouplers;
    if (!secondary) localStorage.setItem(organUiKey('jm-orgue-visible-couplers'), JSON.stringify(visibleCouplers));
    // Layout (main-layout is exclusief van het hoofdvenster; secundaire
    // vensters laden hun layout per scherm uit de panel-state — zie de
    // divisie-reset-reactive) + knopgrootte (gedeeld, puur UI, per orgel)
    if (!secondary) {
      const ml = readOrganUiPref('jm-orgue-main-layout');
      if (ml === 'horizontal' || ml === 'vertical') mainLayout = ml;
    }
    const ss = parseInt(readOrganUiPref('jm-orgue-stop-size'), 10);
    if (ss >= 70 && ss <= 220) stopSize = ss;
    // Vorm van de registerknoppen (rechthoekig/rond)
    knobShape = readOrganUiPref('jm-orgue-knob-shape') === 'round' ? 'round' : 'rect';
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

  // Stapsgewijze trede-inleer (zwel + crescendo), zelfde patroon als het
  // klavier-inleren: laagste stand -> groen zodra herkend -> hoogste stand.
  let pedalLearnModal = null; // { kind:'zwel'|'crescendo', division, step:1|2|'klaar'|'fout', first, msg }

  async function learnPedalFlow(kind, divisionName = null) {
    if (!midiConnected) {
      alert('Verbind eerst een MIDI apparaat!');
      return;
    }
    pedalLearnModal = { kind, division: divisionName, step: 1, first: null, msg: '' };
    try {
      const first = await invoke('learn_pedal_position', { channel: null, ccNum: null });
      if (!pedalLearnModal) return; // geannuleerd
      if (!first) {
        pedalLearnModal = { ...pedalLearnModal, step: 'fout', msg: 'Geen pedaalbeweging ontvangen (time-out van 20 s). Beweeg de trede naar de laagste stand en houd hem daar stil.' };
        return;
      }
      pedalLearnModal = { ...pedalLearnModal, step: 2, first: { channel: first[0], cc: first[1], value: first[2] } };
      const second = await invoke('learn_pedal_position', { channel: first[0], ccNum: first[1] });
      if (!pedalLearnModal) return;
      if (!second) {
        pedalLearnModal = { ...pedalLearnModal, step: 'fout', msg: 'Geen hoogste stand ontvangen (time-out van 20 s).' };
        return;
      }
      if (kind === 'zwel') {
        const divisionIndex = Math.max(0, (organInfo?.divisions || []).findIndex(d => d.name === divisionName));
        await invoke('apply_learned_swell', {
          division: divisionName, divisionIndex,
          channel: first[0], ccNum: first[1],
          lowVal: pedalLearnModal.first.value, highVal: second[2],
        });
        await loadSwellBindings();
      } else {
        await invoke('apply_learned_crescendo', {
          channel: first[0], ccNum: first[1],
          lowVal: pedalLearnModal.first.value, highVal: second[2],
        });
        const inverted = pedalLearnModal.first.value > second[2];
        const lo = Math.min(pedalLearnModal.first.value, second[2]);
        const hi = Math.max(pedalLearnModal.first.value, second[2]);
        crescendoBinding = { channel: first[0], cc: first[1], min: lo, max: hi, invert: inverted };
      }
      pedalLearnModal = { ...pedalLearnModal, step: 'klaar', msg: `CC ${first[1]} · MIDI-kanaal ${first[0] + 1}` };
      dispatch('refreshMidiMappings');
      setTimeout(() => { if (pedalLearnModal && pedalLearnModal.step === 'klaar') pedalLearnModal = null; }, 2000);
    } catch (e) {
      if (pedalLearnModal) pedalLearnModal = { ...pedalLearnModal, step: 'fout', msg: String(e) };
    }
  }

  async function learnSwellPedal(divisionName) {
    learnPedalFlow('zwel', divisionName);
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

  // ===== Handmatig instellen van pedaal-koppelingen en toetsenbereik =====
  // Het handmatige alternatief voor 'Leer pedaal'/'Leer': kanaal, CC of
  // nootbereik direct intypen. Weergave-kanaal is 1-16; op de kabel 0-15.
  async function setSwellManualUI(divisionName, divIdx, channelDisplay, ccNum) {
    const ch = Math.max(1, Math.min(16, channelDisplay | 0)) - 1;
    const cc = Math.max(0, Math.min(127, ccNum | 0));
    try {
      await invoke('set_swell_binding_manual', { division: divisionName, divisionIndex: divIdx, channel: ch, ccNum: cc });
      await loadSwellBindings();
    } catch (e) {
      console.error('Handmatige zwelkast-koppeling mislukt:', e);
    }
  }

  async function setCrescendoManualUI(channelDisplay, ccNum) {
    const ch = Math.max(1, Math.min(16, channelDisplay | 0)) - 1;
    const cc = Math.max(0, Math.min(127, ccNum | 0));
    try {
      await invoke('set_crescendo_binding_manual', { channel: ch, ccNum: cc });
      const mn = crescendoBinding?.min ?? 0;
      const mx = crescendoBinding?.max ?? 127;
      const inv = crescendoBinding?.invert ?? false;
      crescendoBinding = { channel: ch, cc, min: mn, max: mx, invert: inv };
    } catch (e) {
      console.error('Handmatige crescendo-koppeling mislukt:', e);
    }
  }

  async function clearCrescendoBindingUI() {
    try {
      await invoke('clear_crescendo_binding');
      crescendoBinding = null;
    } catch (e) {
      console.error('Crescendo-koppeling wissen mislukt:', e);
    }
  }

  // Handmatig toetsenbereik (laagste/hoogste MIDI-noot) van een divisie.
  async function setKeyRangeUI(divisionName, first, last) {
    const clamp = (v) => (v == null || isNaN(v)) ? null : Math.max(0, Math.min(127, v | 0));
    try {
      await invoke('set_midi_mapping_range', {
        division: divisionName,
        firstMidiNote: clamp(first),
        lastMidiNote: clamp(last),
      });
      dispatch('refreshMidiMappings');
    } catch (e) {
      console.error('Toetsenbereik instellen mislukt:', e);
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
  // pushToBackend=false (secundaire vensters): alleen de UI-state uit de
  // backend-spiegel laden, GEEN set_*-commando's sturen — het hoofdvenster
  // heeft de instellingen al toegepast; een tweede push zou dubbel verkeer,
  // gewiste galmstaarten en teruggedraaide keuzes geven.
  async function loadAudioSettingsForOrgan(pushToBackend = true) {
    // Stale-guard: bij een snelle orgelwissel kan een OUDE run van deze
    // functie nog lopen; zonder guard pushte die instellingen van orgel A in
    // de mirrors/audio van orgel B (auditbevinding 47).
    const myOrgan = organInfo?.id;
    try {
      const s = await invoke('get_organ_settings');
      if (organInfo?.id !== myOrgan) return;
      // Master volume: opgeslagen waarde, anders de ECHTE default (-6 dB) — niet de
      // sliderwaarde van het vorige orgel, anders lekt diens volume dit orgel in
      // én wordt het bij de eerste autosave als instelling van dít orgel bewaard.
      volume = (s && typeof s.master_volume_db === 'number') ? s.master_volume_db : -6;
      if (pushToBackend) dispatch('volumeChange', volume);
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
          if (pushToBackend) await invoke('set_temperament', { noteOffsets: temperaments[idx].cents, fineTune, name: temperaments[idx].name });
        } else if (Array.isArray(t.custom_cents)) {
          customCents = [...t.custom_cents];
          if (pushToBackend) await invoke('set_temperament', { noteOffsets: t.custom_cents, fineTune, name: t.name || 'Aangepast' });
        }
      } else {
        // Geen opgeslagen stemming → default (gelijkzwevend) toepassen i.p.v. die van het vorige orgel.
        selectedTemperament = 0;
        fineTune = 0;
        const t0 = temperaments[0];
        if (pushToBackend) await invoke('set_temperament', { noteOffsets: t0.cents, fineTune: 0, name: t0.name });
      }

      // EQ: nieuw banden-formaat, of migratie vanuit het oude 3-band formaat.
      if (s && s.eq) {
        const e = s.eq;
        eqEnabled = !!e.enabled;
        if (Array.isArray(e.bands) && e.bands.length > 0) {
          eqBands = e.bands.map(b => ({
            enabled: !!b.enabled,
            band_type: b.band_type || 'peak',
            freq: b.freq ?? 1000,
            gain_db: b.gain_db ?? 0,
            bandwidth: b.bandwidth ?? 2.0,
            channel: (b.channel === null || b.channel === undefined) ? null : b.channel,
          }));
        } else {
          // Migratie: oud 3-band formaat → drie vrije banden (Q → octaven).
          const q = Math.max(0.05, e.mid_q ?? 1);
          const bw = (2 / Math.LN2) * Math.asinh(1 / (2 * q));
          eqBands = [
            { enabled: true, band_type: 'lowshelf', freq: e.low_freq ?? 200, gain_db: e.low_gain ?? 0, bandwidth: 1.0, channel: null },
            { enabled: true, band_type: 'peak', freq: e.mid_freq ?? 1000, gain_db: e.mid_gain ?? 0, bandwidth: bw, channel: null },
            { enabled: true, band_type: 'highshelf', freq: e.high_freq ?? 4000, gain_db: e.high_gain ?? 0, bandwidth: 1.0, channel: null },
          ];
        }
      } else {
        eqEnabled = false;
        eqBands = defaultEqBands();
      }
      if (pushToBackend) await updateEq();

      // Reverb: opgeslagen volledige configuratie of default. applyReverbToBackend() past
      // het toe met de afgestemde logica (geen galmstaart wissen tijdens poll).
      if (s && s.reverb) {
        const r = s.reverb;
        reverbType = r.reverb_type || 'algorithmic';
        reverbRt60 = r.rt60 ?? 2.0;
        reverbPreDelay = r.pre_delay_ms ?? 25;
        reverbDamping = r.damping ?? 50;
        reverbRoomSize = r.room_size ?? 80;
        reverb = r.mix ?? 30;
        // Preset: null = "Eigen". Migratie van oude opslag: stond er een vaste
        // preset mét bijgestelde schuiven (dat kon vroeger), dan wordt dat nu
        // "Eigen" zodat de klank van de gebruiker behouden blijft.
        const savedPreset = r.algorithmic_preset;
        if (savedPreset == null || savedPreset < 0 || savedPreset >= reverbPresets.length) {
          reverbPreset = -1;
        } else {
          const p = reverbPresets[savedPreset];
          const matches = Math.abs(reverbRt60 - p.rt60) < 0.05
            && Math.abs(reverbPreDelay - p.preDelay) < 3
            && Math.abs(reverbDamping - p.damping) < 3
            && Math.abs(reverbRoomSize - p.roomSize) < 3;
          reverbPreset = matches ? savedPreset : -1;
        }
        // Eigen IR-bestand: pad herstellen; applyReverbToBackend laadt hem.
        reverbIrPath = r.ir_path || null;
        reverbIrLoaded = !!reverbIrPath;
        reverbIrName = reverbIrPath ? reverbIrPath.split(/[/\\]/).pop() : '';
      } else {
        // Geen opgeslagen galm voor dit orgel. GO-/Hauptwerk-imports zijn NAT
        // opgenomen (de kerkakoestiek zit al in de samples) — daar bovenop
        // standaard 30% Dorpskerk zetten gaf een troebel dubbel-galm-geluid
        // ("wet-on-wet"). Externe orgelbestanden starten daarom droog; eigen
        // (droge) samplemappen houden de vertrouwde standaardgalm.
        const isImportedOrganFile = /\.(organ|organ_hauptwerk_xml)$/i.test(organInfo?.id || '');
        reverbType = 'algorithmic';
        reverbPreset = 1; reverbRt60 = 2.0; reverbPreDelay = 25;
        reverbDamping = 50; reverbRoomSize = 80;
        reverb = isImportedOrganFile ? 0 : 30;
        reverbIrPath = null; reverbIrLoaded = false; reverbIrName = '';
      }
      if (pushToBackend) await applyReverbToBackend();

      // ===== Per-divisie DSP (pan, zwel-config, tremulant, wind-groep) per orgel uit backend =====
      // Pan: backend slaat -1..1 op; UI gebruikt -100..100. Push past de audio toe.
      divisionPans = {};
      for (const p of (s?.division_pans || [])) {
        divisionPans[p.division] = Math.round(p.pan * 100);
        if (pushToBackend) invoke('set_division_pan', { division: p.division, pan: p.pan }).catch(() => {});
      }
      divisionPans = divisionPans;

      // Zwel-config (min-dB + filter): zelfde eenheden als de UI; push past toe.
      swellMinDb = {}; swellFilterCutoff = {};
      for (const sc of (s?.division_swell_configs || [])) {
        swellMinDb[sc.division] = sc.min_db;
        swellFilterCutoff[sc.division] = sc.filter_cutoff;
        if (pushToBackend) invoke('set_swell_config', { division: sc.division, minDb: sc.min_db, filterCutoff: sc.filter_cutoff }).catch(() => {});
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
      if (pushToBackend) persistTrem();

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
      if (pushToBackend) {
        for (const gk of Object.keys(windGroupConfig)) pushWindGroupConfig(parseInt(gk, 10));
      }

      // Output-kanalen + C/Cis: apply_saved past de backend-audio toe; lees daarna voor de UI.
      if (pushToBackend) { try { await invoke('apply_saved_output_channels'); } catch (e) {} }
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
        // Pad per orgel vastleggen zodat de eigen galm een herstart of
        // orgelwissel overleeft (applyReverbToBackend herlaadt hem dan).
        reverbIrPath = selected;
        persistReverbConfig();
      }
    } catch (e) {
      console.error('Failed to load IR:', e);
    }
  }

  // Gedeelde per-orgel-prefs (knopgrootte, knopvorm, zwel-/koppel-zichtbaarheid,
  // registervolgorde) van andere vensters volgen. Storage-events zijn tussen
  // WebView2-vensters onbetrouwbaar → 1s-poll met verander-guards zodat een
  // ongewijzigde waarde geen re-render triggert. Draait in ALLE vensters:
  // zo pikt ook het hoofdvenster wijzigingen uit een extra scherm op.
  let sharedPrefsInterval = null;
  function refreshSharedPrefs() {
    if (!organInfo) return;
    const ss = parseInt(readOrganUiPref('jm-orgue-stop-size'), 10);
    if (ss >= 70 && ss <= 220 && ss !== stopSize) stopSize = ss;
    const ks = readOrganUiPref('jm-orgue-knob-shape') === 'round' ? 'round' : 'rect';
    if (ks !== knobShape) knobShape = ks;
    try {
      const se = readOrganUiPref('jm-orgue-swell-enabled');
      if (se && se !== JSON.stringify(swellEnabled)) swellEnabled = JSON.parse(se);
    } catch (e) {}
    try {
      const vc = readOrganUiPref('jm-orgue-visible-couplers');
      if (vc && vc !== JSON.stringify(visibleCouplers)) visibleCouplers = JSON.parse(vc);
    } catch (e) {}
    try {
      const so = localStorage.getItem(`jm-orgue-stop-order-${organInfo.id || 'default'}`) || '{}';
      if (so !== JSON.stringify(stopOrder)) stopOrder = JSON.parse(so);
    } catch (e) {}
  }

  onMount(() => {
    // Layout + knopgrootte worden per orgel geladen in loadUiPrefsForOrgan(); hier alleen
    // een neutrale begin-default vóórdat een orgel geladen is.
    window.addEventListener('click', handleGlobalClick);
    loadSwellBindings();
    swellPollInterval = setInterval(pollDivisionVolumes, 100);
    pollRecorder();
    // Ook de MIDI-opname-status meepollen: een opname kan óók vanaf een extra
    // registerscherm gestart/gestopt worden en moet hier zichtbaar blijven.
    recorderPoll = setInterval(() => { pollRecorder(); pollMidiRec(); }, 500);
    sharedPrefsInterval = setInterval(refreshSharedPrefs, 1000);
  });

  onDestroy(() => {
    window.removeEventListener('click', handleGlobalClick);
    if (swellPollInterval) clearInterval(swellPollInterval);
    if (recorderPoll) clearInterval(recorderPoll);
    if (midiRecPoll) clearInterval(midiRecPoll);
    if (crescLivePoll) clearInterval(crescLivePoll);
    if (sharedPrefsInterval) clearInterval(sharedPrefsInterval);
  });

  // Live crescendo-pedaalstand volgen: werkt de balk bij terwijl de gebruiker het
  // pedaal beweegt (of min/max instelt), zodat je direct ziet wat er gebeurt.
  // Alleen pollen wanneer de bewerkbalk ook zichtbaar is (orgel-instellingen) —
  // scheelt 10 IPC-calls/s per venster in de andere weergaven.
  let crescLivePoll = null;
  $: {
    if (activeView === 'orgel-instellingen' && !crescLivePoll) {
      crescLivePoll = setInterval(pollCrescLive, 100);
    } else if (activeView !== 'orgel-instellingen' && crescLivePoll) {
      clearInterval(crescLivePoll);
      crescLivePoll = null;
    }
  }
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
    if (secondary) {
      // Per scherm onthouden — niet in de gedeelde main-layout-key, anders
      // vechten hoofdvenster en panelen om dezelfde waarde.
      savePanelState(organInfo?.id, panelNumber, { layout: mainLayout });
    } else {
      localStorage.setItem(organUiKey('jm-orgue-main-layout'), mainLayout);
    }
  }

  // Registerknop-grootte aanpassen (min-breedte in px); per orgel persistent.
  function adjustStopSize(delta) {
    stopSize = Math.max(70, Math.min(220, stopSize + delta));
    localStorage.setItem(organUiKey('jm-orgue-stop-size'), String(stopSize));
  }

  // Vorm van de registerknoppen (rechthoekig ↔ rond); per orgel persistent.
  // Extra schermen volgen via hun refreshSettings-poll (RegisterPanel).
  function toggleKnobShape() {
    knobShape = knobShape === 'round' ? 'rect' : 'round';
    localStorage.setItem(organUiKey('jm-orgue-knob-shape'), knobShape);
  }

  // ---- Extra register-vensters: per orgel onthouden en herstellen ----
  // Het hoofdvenster is de enige schrijver van de lijst "open schermen"
  // (jm-orgue-panels-open-<orgelhash>); elk scherm bewaart zelf zijn
  // divisiekeuze + positie/grootte onder jm-orgue-panel-state-<nr>-<orgelhash>
  // (zie RegisterPanel). Zo heeft elke sleutel één schrijver en zijn er geen races.
  function panelsOpenKey() { return organUiKey('jm-orgue-panels-open'); }
  function savePanelsOpenList() {
    try {
      localStorage.setItem(panelsOpenKey(), JSON.stringify(openPanels.map(p => p.number).sort((a, b) => a - b)));
    } catch (e) {}
  }

  // Vensters die de app zélf sluit (orgelwissel/afsluiten) mogen de bewaarde
  // lijst NIET wijzigen — anders "vergeet" de app welke schermen je open had.
  // Telt hoeveel destroyed-events programmatisch (te verwachten) zijn.
  let panelCloseGuard = 0;

  // Open extra register window. Geëxporteerd zodat App.svelte het kan aanroepen
  // ("Nieuw scherm" vanuit een extra venster komt als Tauri-event bij App binnen).
  export async function openPanel() { return openExtraWindow(); }

  // Alle extra vensters sluiten ZONDER de bewaarde per-orgel-lijst te wijzigen.
  // Aangeroepen (via App.closeExtraPanels) bij orgelwissel en bij afsluiten.
  export async function closeAllPanels() {
    if (secondary) return; // vensterbeheer is exclusief het hoofdvenster
    try {
      const { getAllWebviewWindows } = await import('@tauri-apps/api/webviewWindow');
      const wins = (await getAllWebviewWindows()).filter(w => w.label && w.label.startsWith('panel-'));
      panelCloseGuard += wins.length;
      // Direct leegmaken: restorePanels() kan dan meteen verse vensters openen
      // zonder dat de nummers van de (nog sluitende) oude vensters bezet lijken.
      openPanels = [];
      for (const w of wins) {
        try { await w.close(); } catch (e) { panelCloseGuard = Math.max(0, panelCloseGuard - 1); }
      }
    } catch (e) { /* niet in Tauri of geen extra vensters */ }
  }

  // Heropen de schermen die bij dít orgel open stonden (na het laden van het orgel).
  export async function restorePanels() {
    if (secondary) return; // vensterbeheer is exclusief het hoofdvenster
    if (!organInfo) return;
    let numbers = [];
    try { numbers = JSON.parse(localStorage.getItem(panelsOpenKey()) || '[]'); } catch (e) {}
    if (!Array.isArray(numbers)) numbers = [];
    // Migratie: oudere versies bewaarden alleen het AANTAL vensters in de sessie.
    if (numbers.length === 0) {
      try {
        const sess = JSON.parse(localStorage.getItem('jm-orgue-session') || '{}');
        if (Array.isArray(sess.openPanels) && sess.openPanels.length > 0) {
          numbers = sess.openPanels.map((_, i) => i + 2);
          delete sess.openPanels;
          localStorage.setItem('jm-orgue-session', JSON.stringify(sess));
        }
      } catch (e) {}
    }
    for (const n of numbers) {
      if (Number.isInteger(n) && n >= 2 && !openPanels.some(p => p.number === n)) {
        await openExtraWindow(n);
      }
    }
  }

  async function openExtraWindow(restoreNumber = null) {
    if (secondary) {
      // "Nieuw scherm" vanuit een extra venster: het hoofdvenster beheert de
      // vensterlijst (enige schrijver van jm-orgue-panels-open) — doorsturen.
      try {
        const { emit } = await import('@tauri-apps/api/event');
        await emit('jm-orgue:open-panel');
      } catch (e) { console.error('Nieuw scherm aanvragen mislukt:', e); }
      return;
    }
    try {
      const { WebviewWindow } = await import('@tauri-apps/api/webviewWindow');
      // Nummer: bij herstel het bewaarde nummer, anders het laagste vrije.
      // (restoreNumber kan ook een MouseEvent zijn als de knop direct doorgeeft.)
      let screenNum;
      if (Number.isInteger(restoreNumber) && restoreNumber >= 2) {
        screenNum = restoreNumber;
      } else {
        const usedNumbers = openPanels.map(p => p.number);
        screenNum = 2;
        while (usedNumbers.includes(screenNum)) screenNum++;
      }
      // Bewaarde positie/grootte van dít scherm bij dít orgel (door het scherm
      // zelf weggeschreven — zie PanelApp.saveGeometry).
      let st = null;
      try { st = JSON.parse(localStorage.getItem(organUiKey(`jm-orgue-panel-state-${screenNum}`)) || 'null'); } catch (e) {}
      // Schermnummer in het label: twee opens binnen dezelfde milliseconde
      // (sessie-herstel!) kregen anders hetzélfde label → venster 2 faalde
      // stil en werd door de error-handler ook nog uit de bewaarde lijst gewist.
      const label = `panel-${screenNum}-${Date.now()}`;
      const offset = openPanels.length;
      const webview = new WebviewWindow(label, {
        url: `index.html#panel&n=${screenNum}`,
        title: `JM-Orgue - Scherm ${screenNum}`,
        width: (st && st.width >= 200) ? Math.min(st.width, 4000) : 700,
        height: (st && st.height >= 150) ? Math.min(st.height, 4000) : 500,
        decorations: true,
        resizable: true,
        center: false,
        x: (st && typeof st.x === 'number') ? st.x : 100 + (offset * 30),
        y: (st && typeof st.y === 'number') ? st.y : 100 + (offset * 30),
      });
      const panelEntry = { label, number: screenNum };
      openPanels = [...openPanels, panelEntry];
      savePanelsOpenList();
      webview.once('tauri://destroyed', () => {
        openPanels = openPanels.filter(p => p.label !== label);
        if (panelCloseGuard > 0) panelCloseGuard--; // programmatisch gesloten → lijst bewaren
        else savePanelsOpenList();                  // gebruiker sloot het venster zelf
      });
      webview.once('tauri://error', (e) => {
        console.error('Window creation error:', e);
        openPanels = openPanels.filter(p => p.label !== label);
        savePanelsOpenList();
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
          name: 'Orgeldefinitie (GrandOrgue / Hauptwerk)',
          // Hauptwerk-sets gebruiken *.Organ_Hauptwerk_xml; de backend routeert
          // beide via do_load_organ (is_organ_file_id).
          extensions: ['organ', 'Organ_Hauptwerk_xml']
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

  // Stapsgewijs klavier-inleren met echte terugkoppeling (popup):
  // stap 1 laagste toets → groen zodra herkend → stap 2 hoogste toets → klaar.
  let kbLearnModal = null; // { division, step: 1|2|'klaar'|'fout', first, msg }
  const NOTE_NAMES = ['C', 'C#', 'D', 'D#', 'E', 'F', 'F#', 'G', 'G#', 'A', 'A#', 'B'];
  function noteName(n) { return NOTE_NAMES[n % 12] + (Math.floor(n / 12) - 1); }

  async function learnChannel(divisionName) {
    if (!midiConnected) {
      alert('Verbind eerst een MIDI apparaat!');
      return;
    }
    learningDivision = divisionName;
    learningStep = 1;
    kbLearnModal = { division: divisionName, step: 1, first: null, msg: '' };
    try {
      const first = await invoke('learn_keyboard_note');
      if (!kbLearnModal || kbLearnModal.division !== divisionName) return; // geannuleerd
      if (!first) {
        kbLearnModal = { ...kbLearnModal, step: 'fout', msg: 'Geen toets ontvangen (time-out van 20 s).' };
        return;
      }
      kbLearnModal = { ...kbLearnModal, step: 2, first: { channel: first[0], note: first[1] } };
      learningStep = 2;
      const second = await invoke('learn_keyboard_note');
      if (!kbLearnModal || kbLearnModal.division !== divisionName) return;
      if (!second) {
        kbLearnModal = { ...kbLearnModal, step: 'fout', msg: 'Geen tweede toets ontvangen (time-out van 20 s).' };
        return;
      }
      if (second[0] !== kbLearnModal.first.channel) {
        kbLearnModal = { ...kbLearnModal, step: 'fout',
          msg: `De tweede toets kwam op MIDI-kanaal ${second[0] + 1} i.p.v. ${kbLearnModal.first.channel + 1} — druk beide toetsen op hetzelfde klavier.` };
        return;
      }
      const res = await invoke('apply_learned_keyboard_range', {
        division: divisionName,
        channel: kbLearnModal.first.channel,
        low: kbLearnModal.first.note,
        high: second[1],
        firstSampleNote: 36,
      });
      kbLearnModal = { ...kbLearnModal, step: 'klaar',
        msg: `${noteName(res.first_note)} t/m ${noteName(res.last_note)} · MIDI-kanaal ${res.channel + 1}` };
      dispatch('refreshMidiMappings');
      setTimeout(() => { if (kbLearnModal && kbLearnModal.step === 'klaar') closeKbLearn(); }, 2000);
    } catch (e) {
      if (kbLearnModal) kbLearnModal = { ...kbLearnModal, step: 'fout', msg: String(e) };
    } finally {
      learningDivision = null;
      learningStep = 0;
    }
  }

  function closeKbLearn() {
    kbLearnModal = null;
    learningDivision = null;
    learningStep = 0;
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
        refreshMidiBindingsFull();
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
        refreshMidiBindingsFull();
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

  // ===== Centrale MIDI-koppelingen-editor (Orgel-instellingen) =====
  // Toont ALLE actie-koppelingen (setzer, functies, tremulanten, koppels,
  // registers) met hun structurele velden, en laat ze handmatig toevoegen,
  // aanpassen en per stuk verwijderen — naast het bestaande inleren.
  let midiBindingsFull = [];
  let midiBindingsError = '';
  let newBinding = { code: '', type: 'cc', channel: '', number: 0, value: 64 };

  async function refreshMidiBindingsFull() {
    try {
      midiBindingsFull = await invoke('get_preset_bindings_full');
      recountBindingCounters();
    } catch (e) { /* stil: geen bindings beschikbaar */ }
  }

  function applyBindingsFull(list) {
    midiBindingsFull = list;
    midiBindingsError = '';
    recountBindingCounters();
  }

  // Houd de losse tellers ('MIDI n/4' op knoppen) in sync met de centrale lijst.
  function recountBindingCounters() {
    const stops = {}, couplers = {}, trems = {}, globs = {};
    for (const b of midiBindingsFull) {
      if (b.preset_num >= 150) stops[b.preset_num] = (stops[b.preset_num] || 0) + 1;
      else if (b.preset_num >= 100) couplers[b.preset_num] = (couplers[b.preset_num] || 0) + 1;
      else if (b.preset_num >= ACTION_TREM_LFO_BASE && b.preset_num < ACTION_TREM_LFO_BASE + 16) trems[b.preset_num] = (trems[b.preset_num] || 0) + 1;
      else if (b.preset_num >= 40) globs[b.preset_num] = (globs[b.preset_num] || 0) + 1;
    }
    stopMidiBindings = stops;
    couplerMidiBindings = couplers;
    tremLfoMidiBindings = trems;
    globalMidiBindings = globs;
  }

  // Rijen met per-actie-index: nodig om precies één binding te kunnen
  // bewerken/verwijderen wanneer een actie er meerdere heeft (max 4).
  $: midiBindingRows = (() => {
    const seen = {};
    return [...midiBindingsFull]
      .sort((a, b) => a.preset_num - b.preset_num)
      .map(b => {
        const idx = (seen[b.preset_num] = (seen[b.preset_num] ?? -1) + 1);
        return { ...b, idx };
      });
  })();

  function midiActionLabel(code) {
    if (code >= 150) {
      for (const d of displayOrgan?.divisions || []) {
        const s = (d.stops || []).find(s => s.midi_action_code === code);
        if (s) return `Register: ${s.name}${s.pitch ? ' ' + s.pitch : ''} (${d.display_name || d.name})`;
      }
      return `Register (code ${code})`;
    }
    if (code >= 100) {
      const c = (displayOrgan?.couplers || []).find(c => c.midi_action_code === code);
      return c ? `Koppel: ${c.name.replace(/\n/g, ' ')}` : `Koppel (code ${code})`;
    }
    if (code >= ACTION_TREM_LFO_BASE && code < ACTION_TREM_LFO_BASE + 16) {
      const d = displayOrgan?.divisions?.[code - ACTION_TREM_LFO_BASE];
      return `Tremulant: ${d ? (d.display_name || d.name) : `divisie ${code - ACTION_TREM_LFO_BASE + 1}`}`;
    }
    const vast = {
      10: 'Setzer: SET', 11: 'Setzer: GC', 12: 'Setzer: vorige (−1)', 13: 'Setzer: volgende (+1)',
      14: 'Setzer: −10', 15: 'Setzer: +10',
      40: 'EQ aan/uit', 41: 'Crescendo aan/uit', 42: 'Computer afsluiten',
      43: 'Speakers ↔ hoofdtelefoon',
    };
    if (vast[code] != null) return vast[code];
    if (code <= 9) return `Setzer: knop ${code}`;
    if (code >= 16 && code <= 23) return `Setzer: geheugen M${code - 15}`;
    return `Actie ${code}`;
  }

  // Alle mogelijke doelen voor een nieuwe koppeling, gegroepeerd voor de select.
  $: bindingTargets = (() => {
    const functies = [];
    for (let i = 0; i <= 23; i++) functies.push({ code: i, label: midiActionLabel(i) });
    for (const c of [40, 41, 42, 43]) functies.push({ code: c, label: midiActionLabel(c) });
    const trems = (displayOrgan?.divisions || []).map((d, i) => ({
      code: ACTION_TREM_LFO_BASE + i,
      label: `${d.display_name || d.name}`,
    }));
    const koppels = (displayOrgan?.couplers || []).map(c => ({
      code: c.midi_action_code,
      label: c.name.replace(/\n/g, ' '),
    }));
    const registers = [];
    for (const d of displayOrgan?.divisions || []) {
      for (const s of d.stops || []) {
        if (s.midi_action_code) {
          registers.push({ code: s.midi_action_code, label: `${s.name}${s.pitch ? ' ' + s.pitch : ''} (${d.display_name || d.name})` });
        }
      }
    }
    return { functies, trems, koppels, registers };
  })();

  function bindingNumber(b) {
    return b.trigger_type === 'note' ? b.note : b.trigger_type === 'cc' ? b.controller : b.program;
  }

  function buildSavedBinding(presetNum, type, channelDisplay, number, value) {
    const ch = (channelDisplay === '' || channelDisplay == null) ? null : Math.max(1, Math.min(16, channelDisplay | 0)) - 1;
    const num = Math.max(0, Math.min(127, number | 0));
    const b = { preset_num: presetNum, trigger_type: type, note: null, channel: null, controller: null, value: null, program: null };
    if (type === 'note') b.note = num;
    else if (type === 'cc') { b.channel = ch; b.controller = num; b.value = Math.max(0, Math.min(127, value | 0)); }
    else { b.channel = ch; b.program = num; }
    return b;
  }

  async function updateBindingRow(row, patch = {}) {
    const type = patch.type ?? row.trigger_type;
    const chDisp = patch.channel !== undefined ? patch.channel : (row.channel != null ? row.channel + 1 : '');
    const num = patch.number !== undefined ? patch.number : (bindingNumber(row) ?? 0);
    const val = patch.value !== undefined ? patch.value : (row.value ?? 64);
    const binding = buildSavedBinding(row.preset_num, type, chDisp, num, val);
    try {
      applyBindingsFull(await invoke('update_preset_binding', { presetNum: row.preset_num, index: row.idx, binding }));
    } catch (e) {
      midiBindingsError = String(e);
      await refreshMidiBindingsFull();
    }
  }

  async function removeBindingRow(row) {
    try {
      applyBindingsFull(await invoke('remove_preset_binding', { presetNum: row.preset_num, index: row.idx }));
    } catch (e) {
      midiBindingsError = String(e);
      await refreshMidiBindingsFull();
    }
  }

  async function addNewBindingUI() {
    if (newBinding.code === '' || newBinding.code == null) {
      midiBindingsError = 'Kies eerst een doel voor de nieuwe koppeling.';
      return;
    }
    const binding = buildSavedBinding(parseInt(newBinding.code), newBinding.type, newBinding.channel, newBinding.number, newBinding.value);
    try {
      applyBindingsFull(await invoke('add_preset_binding_manual', { binding }));
    } catch (e) {
      midiBindingsError = String(e);
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
  // Pointer-based slepen van de sorteerlijst-rijen. Géén HTML5 draggable:
  // native drag-and-drop toont in WebView2 een verbodsteken en dropt nooit.
  let sortDrag = null; // { div, fromIdx, startX, startY, active, overIdx }

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

  function sortPointerDown(e, divName, idx) {
    // ▲▼-knoppen in de rij blijven gewoon klikbaar.
    if (e.target.closest('button')) return;
    // Muis: alleen linkermuisknop. Touch/pen: alleen via het ☰-handvat
    // (touch-action: none op het handvat voorkomt scroll-conflict).
    const onHandle = !!e.target.closest('.sort-handle');
    if (e.pointerType === 'mouse' ? e.button !== 0 : !onHandle) return;
    sortDrag = { div: divName, fromIdx: idx, startX: e.clientX, startY: e.clientY, active: false, overIdx: null };
    window.addEventListener('pointermove', sortPointerMove);
    window.addEventListener('pointerup', sortPointerUp);
    window.addEventListener('pointercancel', sortPointerUp);
  }

  function sortPointerMove(e) {
    if (!sortDrag) return;
    if (!sortDrag.active) {
      if (Math.hypot(e.clientX - sortDrag.startX, e.clientY - sortDrag.startY) < KNOB_DRAG_THRESHOLD_PX) return;
      sortDrag.active = true;
    }
    const el = document.elementFromPoint(e.clientX, e.clientY);
    const hit = el && el.closest ? el.closest('[data-sort-div]') : null;
    sortDrag.overIdx = (hit && hit.dataset.sortDiv === sortDrag.div)
      ? parseInt(hit.dataset.sortIdx, 10)
      : null;
    sortDrag = sortDrag; // reactieve update voor de visuele feedback
  }

  function sortPointerUp(e) {
    window.removeEventListener('pointermove', sortPointerMove);
    window.removeEventListener('pointerup', sortPointerUp);
    window.removeEventListener('pointercancel', sortPointerUp);
    const d = sortDrag;
    sortDrag = null;
    if (!d || !d.active || e.type === 'pointercancel') return;
    if (d.overIdx === null || Number.isNaN(d.overIdx) || d.overIdx === d.fromIdx) return;
    reorderStop(d.div, d.fromIdx, d.overIdx);
  }
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

  // Herordenen op basis van de GETOONDE volgorde (displayOrgan) zodat de indices
  // altijd kloppen met wat de gebruiker ziet (en de bewaarde volgorde compleet is).
  // Gedeeld door de sorteerlijst (Instellingen) én het pointer-slepen op het orgelscherm.
  function reorderStop(divName, fromIdx, toIdx) {
    const organ = displayOrgan || organInfo || demoOrgan;
    const div = organ.divisions.find(d => d.name === divName);
    if (!div) return;
    const order = div.stops.map(s => s.id);
    const [item] = order.splice(fromIdx, 1);
    order.splice(toIdx, 0, item);
    stopOrder[divName] = order;
    stopOrder = stopOrder;
    saveStopOrder();
  }

  // --- Registerknoppen verslepen op het orgelscherm zelf (alleen met de muis) ---
  // Pointer-events i.p.v. HTML5 draggable: native drag-and-drop is onbetrouwbaar
  // in WebView2 (daarom bestaan de ▲▼-knoppen hierboven). Een drempel van 8px
  // zorgt dat een gewone klik het register blijft togglen; rechtsklik en
  // touch-long-press blijven van use:midiLearn (touch wordt hier genegeerd).
  const KNOB_DRAG_THRESHOLD_PX = 8;
  let knobDrag = null;           // { div, fromIdx, startX, startY, active, overIdx }
  let knobDragJustEnded = false; // onderdrukt de click die direct op een sleep volgt

  function knobPointerDown(e, divName, idx) {
    if (e.pointerType !== 'mouse' || e.button !== 0) return; // alleen linkermuisknop
    knobDrag = { div: divName, fromIdx: idx, startX: e.clientX, startY: e.clientY, active: false, overIdx: null };
    // Pointer capture: ook buiten de knop (en het venster) blijven events binnenkomen.
    try { e.currentTarget.setPointerCapture(e.pointerId); } catch (_) {}
    window.addEventListener('pointermove', knobPointerMove);
    window.addEventListener('pointerup', knobPointerUp);
    window.addEventListener('pointercancel', knobPointerUp);
  }

  function knobPointerMove(e) {
    if (!knobDrag) return;
    if (!knobDrag.active) {
      // Pas een sleep na >8px beweging — daaronder blijft het een gewone klik.
      if (Math.hypot(e.clientX - knobDrag.startX, e.clientY - knobDrag.startY) < KNOB_DRAG_THRESHOLD_PX) return;
      knobDrag.active = true;
    }
    // Doelpositie via hit-test onder de cursor — werkt in horizontale én verticale layout.
    const el = document.elementFromPoint(e.clientX, e.clientY);
    const hit = el && el.closest ? el.closest('[data-knob-div]') : null;
    if (hit && hit.dataset.knobDiv === knobDrag.div) {
      knobDrag.overIdx = parseInt(hit.dataset.knobIdx, 10);
    } else {
      knobDrag.overIdx = null; // buiten de eigen divisie → geen geldig doel
    }
    knobDrag = knobDrag; // reactieve update voor de visuele feedback
  }

  function knobPointerUp(e) {
    window.removeEventListener('pointermove', knobPointerMove);
    window.removeEventListener('pointerup', knobPointerUp);
    window.removeEventListener('pointercancel', knobPointerUp);
    const d = knobDrag;
    knobDrag = null;
    if (!d || !d.active) return; // geen sleep geweest → het click-event togglet gewoon
    // De click die op deze pointerup volgt mag het register NIET togglen.
    knobDragJustEnded = true;
    setTimeout(() => { knobDragJustEnded = false; }, 0);
    if (e.type === 'pointercancel') return;
    if (d.overIdx === null || Number.isNaN(d.overIdx) || d.overIdx === d.fromIdx) return;
    reorderStop(d.div, d.fromIdx, d.overIdx);
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
      if (secondary && organInfo?.id) {
        // Divisiekeuze + layout van dít scherm bij dít orgel terugzetten
        // (per-scherm-state; zie lib/panelState.js).
        const st = loadPanelState(organInfo.id, panelNumber);
        if (st && Array.isArray(st.divisions) && st.divisions.length > 0) {
          const valid = st.divisions.filter(n => displayOrgan.divisions.some(d => d.name === n));
          if (valid.length > 0) selectedDivisions = valid;
        }
        if (st?.layout === 'horizontal' || st?.layout === 'vertical') {
          mainLayout = st.layout;
        } else {
          // Migratie: layout-key van het oude RegisterPanel (per orgel gedeeld).
          const legacy = readOrganUiPref('jm-orgue-panel-layout');
          if (legacy === 'horizontal' || legacy === 'vertical') mainLayout = legacy;
        }
      }
    }
  }

  // Bij een ECHTE orgelwissel (id verandert): reset de live tremulant-stand en laad de
  // crescendo-config van dít orgel. We keyen op organInfo.id (niet op divisie-namen),
  // want twee verschillende orgels kunnen identieke divisie-namen hebben — dan zou een
  // tremulant die op het vorige orgel aanstond ten onrechte "actief" blijven.
  let crescLoadedFor = '';
  $: if (organInfo && organInfo.id && `${organInfo.id}::${audioEpoch}` !== crescLoadedFor) {
    crescLoadedFor = `${organInfo.id}::${audioEpoch}`;
    tremActive = {};
    loadCrescendoForOrgan();
    // Secundaire vensters laden alleen de UI-state (geen backend-pushes).
    loadAudioSettingsForOrgan(!secondary);
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
    // Per-scherm onthouden (alleen extra vensters; het hoofdvenster toont
    // standaard alles en bewaart geen divisiekeuze).
    if (secondary) savePanelState(organInfo?.id, panelNumber, { divisions: selectedDivisions });
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
            <button
              class="btn btn-ghost btn-sm panel-layout-btn"
              on:click={toggleKnobShape}
              title="{knobShape === 'round' ? 'Rechthoekige registerknoppen' : 'Ronde registerknoppen (trekregisters)'}"
            >
              {#if knobShape === 'round'}
                <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                  <rect x="3" y="6" width="18" height="12" rx="2"/>
                </svg>
                Recht
              {:else}
                <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                  <circle cx="12" cy="12" r="9"/>
                </svg>
                Rond
              {/if}
            </button>
            {#if secondary && showPanelHeaderToggle}
              <!-- Hoofdbalk (tabs/profielwissel/start-stop) tonen op dit scherm -->
              <button
                class="btn btn-ghost btn-sm panel-layout-btn"
                on:click={() => dispatch('showHeaderRequest')}
                title="Hoofdbalk tonen (tabbladen, profielwissel, audio start/stop)"
              >
                <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                  <polyline points="6 9 12 15 18 9"/>
                </svg>
                Balk
              </button>
            {/if}
            <button class="btn btn-ghost btn-sm panel-add-btn" on:click={() => openExtraWindow()}>
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
            {#if !midiRec.recording}
              <button
                class="btn btn-ghost btn-sm"
                on:click={openLiveNotation}
                title="Notatievenster openen: live inspelen met noten die verschijnen terwijl je speelt (lagen, overdub, ritme-tolerantie, bewerken); binnen het venster kun je ook een bestaand MIDI-bestand openen"
              >
                <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                  <circle cx="7" cy="18" r="3"/>
                  <path d="M10 18V5l8-2v12"/>
                  <circle cx="15" cy="15" r="3"/>
                </svg>
                Noteren
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
        <!-- --stop-min-width op de container zodat ook .division (min-width in
             verticale modus) de ingestelde knopgrootte kan gebruiken -->
        <div class="divisions-container" class:divisions-vertical={mainLayout === 'vertical'} style="--stop-min-width: {stopSize}px">
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
              <div
                class="stops-grid"
                class:stops-vertical={mainLayout === 'vertical'}
                class:knobs-round={knobShape === 'round'}
                class:knob-dragging={!!(knobDrag && knobDrag.active && knobDrag.div === division.name)}
                style="--stop-min-width: {stopSize}px"
              >
                {#each division.stops as stop, stopIdx}
                  {@const name = cleanStopName(stop)}
                  <button
                    class="stop-knob {getStopClass(stop)}"
                    class:engaged={stop.drawn}
                    class:has-midi={stopMidiBindings[stop.midi_action_code] > 0}
                    class:dragging={!!(knobDrag && knobDrag.active && knobDrag.div === division.name && knobDrag.fromIdx === stopIdx)}
                    class:drag-target={!!(knobDrag && knobDrag.active && knobDrag.div === division.name && knobDrag.overIdx === stopIdx && knobDrag.fromIdx !== stopIdx)}
                    data-knob-div={division.name}
                    data-knob-idx={stopIdx}
                    on:pointerdown={(e) => knobPointerDown(e, division.name, stopIdx)}
                    on:click={() => { if (knobDragJustEnded) return; dispatch('toggleStop', stop.id); }}
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
        <!-- In secundaire vensters consumeert de SetzerBar GEEN MIDI-triggers:
             alleen het hoofdvenster leest het (consumerende) trigger-kanaal,
             anders worden presets/acties dubbel toegepast. -->
        <SetzerBar
          organInfo={displayOrgan}
          consumeMidiTriggers={!secondary}
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
                    <option value={-1}>Eigen (per orgel)</option>
                  </select>
                </div>
                {#if reverbPreset === -1}
                  <!-- Eigen preset: vrij instelbaar met schuiven; de waarden worden
                       per orgel vastgelegd. Vaste presets tonen geen schuiven. -->
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
                {:else}
                  <p style="margin: 0.2rem 0 0; font-size: 0.68rem; color: var(--text-muted); line-height: 1.35;">
                    Vaste ruimteklank. Kies "Eigen (per orgel)" om RT60, pre-delay,
                    demping en ruimte zelf in te stellen — die instelling wordt bij dit orgel bewaard.
                  </p>
                {/if}
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
                <div style="margin-top:0.5rem; display:flex; flex-direction:column; gap:0.5rem;">
                  {#each eqBands as band, bi (bi)}
                    <div style="border:1px solid var(--accent-soft-2); border-radius:var(--radius-sm); padding:0.4rem 0.5rem; opacity:{band.enabled ? 1 : 0.55};">
                      <div style="display:flex; align-items:center; gap:0.45rem; flex-wrap:wrap; margin-bottom:0.35rem;">
                        <label class="swell-toggle" style="margin:0;" title="Band aan/uit">
                          <input type="checkbox" bind:checked={band.enabled} on:change={onEqBandChange} />
                          <span class="swell-toggle-label">Band {bi + 1}</span>
                        </label>
                        <select
                          style="font-size:0.75rem; padding:0.15rem 0.3rem; background:var(--bg-elevated); border:1px solid var(--accent-soft-2); border-radius:var(--radius-sm); color:var(--text);"
                          bind:value={band.band_type} on:change={onEqBandChange} title="Filtertype"
                        >
                          {#each eqBandTypes as t}
                            <option value={t.value}>{t.label}</option>
                          {/each}
                        </select>
                        <select
                          style="font-size:0.75rem; padding:0.15rem 0.3rem; background:var(--bg-elevated); border:1px solid var(--accent-soft-2); border-radius:var(--radius-sm); color:var(--text);"
                          value={band.channel === null || band.channel === undefined ? 'all' : String(band.channel)}
                          on:change={(e) => { band.channel = e.target.value === 'all' ? null : parseInt(e.target.value, 10); onEqBandChange(); }}
                          title="Doelkanaal (Alle = hele mix)"
                        >
                          <option value="all">Alle kanalen</option>
                          {#each Array(Math.max(2, audioChannelCount)) as _, ch}
                            <option value={String(ch)}>Kanaal {ch + 1}</option>
                          {/each}
                        </select>
                        <button
                          class="btn btn-ghost btn-sm"
                          style="margin-left:auto; font-size:0.75rem; padding:0.1rem 0.45rem;"
                          on:click={() => removeEqBand(bi)}
                          title="Band verwijderen"
                        >✕</button>
                      </div>
                      <div class="swell-config-sliders">
                        <div class="swell-config-row">
                          <span class="swell-config-label">Frequentie</span>
                          <input type="range" min="0" max="300" step="1"
                            value={freqToSlider(band.freq)}
                            on:input={(e) => { band.freq = sliderToFreq(parseInt(e.target.value, 10)); onEqBandChange(); }}
                          />
                          <input type="number" min="20" max="20000" step="1"
                            style="width:4.6rem; font-size:0.75rem; padding:0.1rem 0.25rem; background:var(--bg-elevated); border:1px solid var(--accent-soft-2); border-radius:var(--radius-sm); color:var(--text);"
                            value={band.freq}
                            on:change={(e) => { band.freq = Math.max(20, Math.min(20000, parseFloat(e.target.value) || 1000)); onEqBandChange(); }}
                          />
                          <span class="swell-config-value" style="width:1.4rem;">Hz</span>
                        </div>
                        {#if band.band_type === 'peak' || band.band_type === 'lowshelf' || band.band_type === 'highshelf'}
                          <div class="swell-config-row">
                            <span class="swell-config-label">Gain</span>
                            <input type="range" min="-24" max="24" step="0.5" bind:value={band.gain_db} on:input={onEqBandChange} />
                            <span class="swell-config-value">{band.gain_db > 0 ? '+' : ''}{Number(band.gain_db).toFixed(1)} dB</span>
                          </div>
                        {/if}
                        {#if band.band_type !== 'lowshelf' && band.band_type !== 'highshelf'}
                          <div class="swell-config-row">
                            <span class="swell-config-label">Bandbreedte</span>
                            <input type="range" min="0.1" max="4" step="0.05" bind:value={band.bandwidth} on:input={onEqBandChange} />
                            <span class="swell-config-value">{Number(band.bandwidth).toFixed(2)} oct</span>
                          </div>
                        {/if}
                      </div>
                    </div>
                  {/each}
                  <div style="display:flex; gap:0.4rem;">
                    <button class="btn btn-secondary btn-sm" on:click={addEqBand}>+ Band toevoegen</button>
                    <span style="font-size:0.7rem; color:var(--text-muted); align-self:center;">
                      EQ werkt per uitgangskanaal, na de galm — kies "Alle kanalen" of een specifiek kanaal per band.
                    </span>
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
                    <select style="flex:1; font-size: 0.8rem;" bind:value={crescendoNumStages} on:change={() => { crescendoStages = ensureCrescStages(); saveCrescendo(); }}>
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
                          {#if (organInfo?.couplers || []).length}
                            <tr class="cresc-div-row">
                              <td class="cresc-div-name" colspan={crescendoNumStages + 1}>Koppels</td>
                            </tr>
                            {#each organInfo.couplers as koppel}
                              <tr>
                                <td class="cresc-stop-name" title="{koppel.name}">{koppel.name}</td>
                                {#each Array(crescendoNumStages) as _, s}
                                  <td class="cresc-cell" class:colactive={s + 1 === crescendoStage}>
                                    <button
                                      type="button"
                                      class="cresc-cell-btn"
                                      class:on={(crescendoStages[s] || []).includes(koppel.id)}
                                      on:click={(e) => { if (e.shiftKey) fillStopFrom(s, koppel.id); else toggleCrescCell(s, koppel.id); }}
                                      title="Stap {s + 1} · {koppel.name} — klik = aan/uit, Shift+klik = cumulatief"
                                      aria-label="Stap {s + 1} {koppel.name}"
                                    >{(crescendoStages[s] || []).includes(koppel.id) ? '●' : ''}</button>
                                  </td>
                                {/each}
                              </tr>
                            {/each}
                          {/if}
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
                  <!-- Handmatig: kanaal + CC direct invullen (alternatief voor inleren) -->
                  <div class="swell-config-row" style="margin-top:0.3rem;" title="Handmatig: MIDI-kanaal en CC-nummer van het crescendopedaal (alternatief voor inleren)">
                    <span class="swell-config-label">Handmatig</span>
                    <select
                      value={crescendoBinding ? crescendoBinding.channel + 1 : ''}
                      on:change={(e) => { if (e.target.value !== '') setCrescendoManualUI(parseInt(e.target.value), crescendoBinding?.cc ?? 7); }}
                      title="MIDI-kanaal (1-16)"
                    >
                      <option value="" disabled>kan.</option>
                      {#each Array(16).fill().map((_, i) => i + 1) as ch}
                        <option value={ch}>{ch}</option>
                      {/each}
                    </select>
                    <input type="number" min="0" max="127" class="pedal-range-input"
                      value={crescendoBinding?.cc ?? ''}
                      placeholder="CC"
                      on:change={(e) => setCrescendoManualUI((crescendoBinding?.channel ?? 0) + 1, parseInt(e.target.value) || 0)}
                      title="CC-nummer (0-127)"
                    />
                    {#if crescendoBinding}
                      <button class="btn btn-ghost btn-sm swell-clear-btn" on:click={clearCrescendoBindingUI} title="Crescendo-pedaalkoppeling wissen">&#x2715;</button>
                    {/if}
                  </div>
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
                          class:drag-over={!!(sortDrag && sortDrag.active && sortDrag.div === division.name && sortDrag.overIdx === idx && sortDrag.fromIdx !== idx)}
                          class:dragging={!!(sortDrag && sortDrag.active && sortDrag.div === division.name && sortDrag.fromIdx === idx)}
                          data-sort-div={division.name}
                          data-sort-idx={idx}
                          on:pointerdown={(e) => sortPointerDown(e, division.name, idx)}
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
                      <!-- Weergave is 1-16 (zoals op MIDI-apparatuur); intern/op de
                           kabel is het kanaal 0-15. De oude versie stuurde de
                           1-16-waarde ongecorrigeerd door, waardoor handmatig
                           "kanaal 1" op wire-kanaal 1 (= apparaat-kanaal 2)
                           filterde en het klavier zweeg; geleerde kanalen (0-15)
                           pasten bovendien bij geen enkele optie → lege dropdown. -->
                      <select
                        id="channel-{division.name}"
                        value={(m?.channel != null) ? m.channel + 1 : ''}
                        on:change={(e) => setMidiChannel(division.name, e.target.value === '' ? null : parseInt(e.target.value) - 1)}
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

                    <!-- Handmatig toetsenbereik (alternatief voor 'Leer'): laagste
                         en hoogste MIDI-noot van dit klavier, direct in te typen. -->
                    <div class="midi-transpose" title="Handmatig toetsenbereik: laagste en hoogste MIDI-noot (alternatief voor 'Leer'). Leeg = geen begrenzing.">
                      <label for="range-lo-{division.name}">Bereik:</label>
                      <input
                        id="range-lo-{division.name}"
                        type="number" min="0" max="127" placeholder="laag"
                        value={m?.first_midi_note ?? ''}
                        on:change={(e) => setKeyRangeUI(division.name, e.target.value === '' ? null : parseInt(e.target.value), m?.last_midi_note ?? null)}
                        title="Laagste MIDI-noot {m?.first_midi_note != null ? `(${midiToNoteName(m.first_midi_note)})` : ''}"
                      />
                      <input
                        type="number" min="0" max="127" placeholder="hoog"
                        value={m?.last_midi_note ?? ''}
                        on:change={(e) => setKeyRangeUI(division.name, m?.first_midi_note ?? null, e.target.value === '' ? null : parseInt(e.target.value))}
                        title="Hoogste MIDI-noot {m?.last_midi_note != null ? `(${midiToNoteName(m.last_midi_note)})` : ''}"
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
                      <!-- Handmatig: kanaal + CC direct invullen (alternatief voor inleren) -->
                      <div class="swell-config-row" title="Handmatig: MIDI-kanaal en CC-nummer van het zwelpedaal (alternatief voor inleren)">
                        <span class="swell-config-label">Handmatig</span>
                        <select
                          value={swellBindings[division.name] ? swellBindings[division.name].channel + 1 : ''}
                          on:change={(e) => { if (e.target.value !== '') setSwellManualUI(division.name, divIdx, parseInt(e.target.value), swellBindings[division.name]?.cc_num ?? 7); }}
                          title="MIDI-kanaal (1-16)"
                        >
                          <option value="" disabled>kan.</option>
                          {#each Array(16).fill().map((_, i) => i + 1) as ch}
                            <option value={ch}>{ch}</option>
                          {/each}
                        </select>
                        <input type="number" min="0" max="127" class="pedal-range-input"
                          value={swellBindings[division.name]?.cc_num ?? ''}
                          placeholder="CC"
                          on:change={(e) => setSwellManualUI(division.name, divIdx, (swellBindings[division.name]?.channel ?? 0) + 1, parseInt(e.target.value) || 0)}
                          title="CC-nummer (0-127)"
                        />
                      </div>
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

            <!-- ============ MIDI-koppelingen: overzicht + handmatig bewerken ============ -->
            <div class="settings-block">
              <h3 class="settings-block-title">MIDI-koppelingen (handmatig bewerken)</h3>
              <p style="margin: 0 0 0.5rem; font-size: 0.7rem; color: var(--text-muted); line-height: 1.4;">
                Alle MIDI-koppelingen van dit orgel in één lijst. Pas type, kanaal of nummer direct aan,
                verwijder een koppeling met ✕, of voeg er onderaan handmatig één toe — inleren blijft
                daarnaast gewoon werken. Kanaal 'Alle' = reageert op elk kanaal. Bij CC geldt:
                activeert zodra de waarde ≥ de drempel.
              </p>
              {#if midiBindingsError}
                <p style="color: #e07a6a; font-size: 0.72rem; margin: 0 0 0.4rem;">{midiBindingsError}</p>
              {/if}
              <div class="midi-bindings-wrap">
                <table class="midi-bindings-table">
                  <thead>
                    <tr>
                      <th>Doel</th>
                      <th>Type</th>
                      <th>Kanaal</th>
                      <th>Nummer</th>
                      <th>Drempel</th>
                      <th></th>
                    </tr>
                  </thead>
                  <tbody>
                    {#each midiBindingRows as row (`${row.preset_num}-${row.idx}`)}
                      <tr>
                        <td class="midi-bindings-doel" title={midiActionLabel(row.preset_num)}>
                          {midiActionLabel(row.preset_num)}{row.idx > 0 ? ` (${row.idx + 1})` : ''}
                        </td>
                        <td>
                          <select value={row.trigger_type} on:change={(e) => updateBindingRow(row, { type: e.target.value })}>
                            <option value="note">Noot</option>
                            <option value="cc">CC</option>
                            <option value="program">Program</option>
                          </select>
                        </td>
                        <td>
                          {#if row.trigger_type === 'note'}
                            <span style="color: var(--text-muted);">—</span>
                          {:else}
                            <select
                              value={row.channel != null ? row.channel + 1 : ''}
                              on:change={(e) => updateBindingRow(row, { channel: e.target.value === '' ? '' : parseInt(e.target.value) })}
                            >
                              <option value="">Alle</option>
                              {#each Array(16).fill().map((_, i) => i + 1) as ch}
                                <option value={ch}>{ch}</option>
                              {/each}
                            </select>
                          {/if}
                        </td>
                        <td style="white-space: nowrap;">
                          <input type="number" min="0" max="127" class="pedal-range-input"
                            value={bindingNumber(row) ?? 0}
                            on:change={(e) => updateBindingRow(row, { number: parseInt(e.target.value) || 0 })}
                            title={row.trigger_type === 'note' ? 'Nootnummer' : row.trigger_type === 'cc' ? 'CC-nummer' : 'Programmanummer'}
                          />
                          {#if row.trigger_type === 'note'}
                            <span style="font-size: 0.65rem; color: var(--text-muted);">{midiToNoteName(bindingNumber(row) ?? 0)}</span>
                          {/if}
                        </td>
                        <td>
                          {#if row.trigger_type === 'cc'}
                            <input type="number" min="0" max="127" class="pedal-range-input"
                              value={row.value ?? 64}
                              on:change={(e) => updateBindingRow(row, { value: parseInt(e.target.value) || 0 })}
                              title="Activeert zodra de CC-waarde ≥ deze drempel"
                            />
                          {:else}
                            <span style="color: var(--text-muted);">—</span>
                          {/if}
                        </td>
                        <td>
                          <button class="btn btn-ghost btn-sm swell-clear-btn" on:click={() => removeBindingRow(row)} title="Deze koppeling verwijderen">&#x2715;</button>
                        </td>
                      </tr>
                    {:else}
                      <tr>
                        <td colspan="6" style="color: var(--text-muted); padding: 0.6rem;">
                          Nog geen MIDI-koppelingen — leer ze in, of voeg er hieronder handmatig één toe.
                        </td>
                      </tr>
                    {/each}
                  </tbody>
                </table>
              </div>
              <!-- Nieuwe koppeling handmatig toevoegen -->
              <div style="display: flex; align-items: center; gap: 0.4rem; margin-top: 0.55rem; flex-wrap: wrap;">
                <span style="font-size: 0.72rem; color: var(--text-secondary);">Nieuw:</span>
                <select bind:value={newBinding.code} style="max-width: 15rem;">
                  <option value="">— kies doel —</option>
                  <optgroup label="Functies">
                    {#each bindingTargets.functies as t}<option value={t.code}>{t.label}</option>{/each}
                  </optgroup>
                  {#if bindingTargets.trems.length}
                    <optgroup label="Tremulanten">
                      {#each bindingTargets.trems as t}<option value={t.code}>{t.label}</option>{/each}
                    </optgroup>
                  {/if}
                  {#if bindingTargets.koppels.length}
                    <optgroup label="Koppels">
                      {#each bindingTargets.koppels as t}<option value={t.code}>{t.label}</option>{/each}
                    </optgroup>
                  {/if}
                  {#if bindingTargets.registers.length}
                    <optgroup label="Registers">
                      {#each bindingTargets.registers as t}<option value={t.code}>{t.label}</option>{/each}
                    </optgroup>
                  {/if}
                </select>
                <select bind:value={newBinding.type} title="Triggertype">
                  <option value="note">Noot</option>
                  <option value="cc">CC</option>
                  <option value="program">Program</option>
                </select>
                {#if newBinding.type !== 'note'}
                  <select bind:value={newBinding.channel} title="MIDI-kanaal">
                    <option value="">Alle kanalen</option>
                    {#each Array(16).fill().map((_, i) => i + 1) as ch}
                      <option value={ch}>Kanaal {ch}</option>
                    {/each}
                  </select>
                {/if}
                <input type="number" min="0" max="127" class="pedal-range-input" bind:value={newBinding.number}
                  title="Noot-/CC-/programmanummer (0-127)" />
                {#if newBinding.type === 'cc'}
                  <input type="number" min="0" max="127" class="pedal-range-input" bind:value={newBinding.value}
                    title="Drempel: activeert zodra de CC-waarde ≥ dit" />
                {/if}
                <button class="btn btn-secondary btn-sm" on:click={addNewBindingUI}>Toevoegen</button>
                <button class="btn btn-ghost btn-sm" on:click={refreshMidiBindingsFull} title="Lijst opnieuw laden (bv. na inleren via de knoppen)">↻</button>
              </div>
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
                <button class="btn btn-primary btn-sm" on:click={() => { dispatch('applyAudioOutput'); setTimeout(refreshAudioStatus, 1500); }}>
                  {$t('settings.audio_apply')}
                </button>
              </div>

              <!-- Actuele status + geschatte latentie -->
              {#if audioStatus && audioStatus.audio_running}
                <div style="margin-top: 0.6rem; padding: 0.45rem 0.6rem; background: var(--bg-elevated); border: 1px solid var(--accent-soft-2); border-radius: var(--radius-sm); font-size: 0.74rem; line-height: 1.5;">
                  <strong>Actueel:</strong>
                  {audioStatus.audio_host} · {audioStatus.audio_device || 'standaard'} ·
                  {audioStatus.sample_rate} Hz · {audioStatus.channels || 2} kanalen ·
                  buffer {audioStatus.buffer_frames > 0 ? `${audioStatus.buffer_frames} frames` : 'driver-default'}
                  {#if audioLatencyMs !== null}
                    · <strong>geschatte latentie ≈ {audioLatencyMs.toFixed(1)} ms</strong>
                  {:else}
                    · latentie: driver-default (WASAPI ≈ 20–30 ms; kies ASIO + kleine buffer voor minder)
                  {/if}
                </div>
              {/if}
              <p style="margin: 0.5rem 0 0; font-size: 0.7rem; color: var(--text-muted); line-height: 1.4;">
                {$t('settings.audio_latency_hint')}
              </p>

              <!-- Uitvoerprofielen: snelle wissel speakers ↔ hoofdtelefoon -->
              <div style="margin-top: 0.85rem; padding-top: 0.75rem; border-top: var(--border-subtle);">
                <h4 style="margin: 0 0 0.35rem; font-size: 0.85rem;">Uitvoerprofielen — snelle wissel</h4>
                <p style="margin: 0 0 0.5rem; font-size: 0.7rem; color: var(--text-muted); line-height: 1.4;">
                  Sla twee uitvoer-instellingen op (bijv. luidsprekers via ASIO en een hoofdtelefoon
                  via WASAPI — of dezelfde ASIO-interface met verschillende kanalen). De kanaalkeuze
                  per klavier (de "Kanalen"-vinkjes onder MIDI Kanaal per Klavier) wordt in het
                  profiel mee opgeslagen en wisselt mee. Daarna wissel je met één knop in de balk
                  bovenin — of via een ingeleerde MIDI-knop. Kies eerst host, apparaat, buffer en
                  kanalen, en sla dan op.
                </p>
                {#each [['speakers', 'Speakers'], ['headphones', 'Hoofdtelefoon']] as [kind, label]}
                  {@const prof = audioProfiles?.[kind]}
                  <div style="display:flex; align-items:center; gap:0.45rem; margin-bottom:0.4rem; flex-wrap:wrap;">
                    <strong style="font-size:0.78rem; width:7.5rem;">
                      {label}{audioProfiles?.active === kind ? ' ●' : ''}
                    </strong>
                    <span style="flex:1; font-size:0.72rem; color:var(--text-muted); min-width:10rem;">
                      {#if prof}
                        {prof.host || '?'} · {prof.device || 'standaard'}{prof.bufferFrames ? ` · ${prof.bufferFrames} frames` : ''}{prof.channels ? ' · incl. kanalen' : ''}
                      {:else}
                        — niet ingesteld —
                      {/if}
                    </span>
                    <button class="btn btn-secondary btn-sm" style="font-size:0.7rem; padding:0.15rem 0.5rem;"
                      on:click={() => dispatch('saveAudioProfile', kind)}
                      title="Huidige selectie (host/apparaat/buffer + kanaalkeuze per klavier) opslaan als {label}-profiel"
                    >Huidige selectie opslaan</button>
                    {#if prof}
                      <button class="btn btn-secondary btn-sm" style="font-size:0.7rem; padding:0.15rem 0.5rem;"
                        on:click={() => dispatch('switchAudioProfile', kind)}
                        title="Nu naar dit profiel wisselen (ook om een haperende uitgang opnieuw op te bouwen)"
                      >Activeer</button>
                      <button class="btn btn-ghost btn-sm" style="font-size:0.7rem; padding:0.15rem 0.4rem;"
                        on:click={() => dispatch('clearAudioProfile', kind)}
                        title="Profiel wissen"
                      >✕</button>
                    {/if}
                  </div>
                {/each}
                <div style="display:flex; align-items:center; gap:0.5rem;">
                  <span style="font-size:0.72rem; color:var(--text-muted);">MIDI-knop voor de wissel:</span>
                  <button
                    class="btn btn-ghost btn-sm"
                    class:learning={globalLearningAction === ACTION_AUDIO_PROFILE}
                    on:click={() => learnGlobalAction(ACTION_AUDIO_PROFILE)}
                    disabled={globalLearningAction !== null && globalLearningAction !== ACTION_AUDIO_PROFILE}
                    title="MIDI inleren voor speakers/hoofdtelefoon-wissel ({globalMidiBindings[ACTION_AUDIO_PROFILE] || 0}/4)"
                    style="font-size:0.7rem; padding:0.2rem 0.5rem; border:1px solid {globalMidiBindings[ACTION_AUDIO_PROFILE] > 0 ? 'var(--midi-indicator)' : 'var(--accent-soft-2)'};"
                  >
                    {#if globalLearningAction === ACTION_AUDIO_PROFILE}
                      <span class="learning-indicator"></span>Wacht...
                    {:else}
                      MIDI {globalMidiBindings[ACTION_AUDIO_PROFILE] || 0}/4
                    {/if}
                  </button>
                </div>
              </div>

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
                <label class="swell-toggle" style="margin-top:0.4rem;" title="Aan: bij het starten van de app wordt het laatst geopende orgel automatisch geladen. Uit: de app opent met de orgelkeuze.">
                  <input
                    type="checkbox"
                    checked={autoLoadLastOrgan}
                    on:change={(e) => dispatch('setAutoLoadLastOrgan', e.target.checked)}
                  />
                  <span>Laatst geopende orgel automatisch laden bij starten</span>
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

{#if pedalLearnModal}
  <div class="kb-learn-overlay">
    <div class="kb-learn-modal">
      <h3>{pedalLearnModal.kind === 'zwel' ? `Zweltrede inleren — ${pedalLearnModal.division}` : 'Generaal crescendo inleren'}</h3>
      {#if pedalLearnModal.step === 1}
        <div class="kb-learn-step kb-learn-wait">
          <span class="learning-indicator"></span>
          Zet de trede in de <strong>LAAGSTE</strong> stand (dicht) en houd hem even stil&hellip;
        </div>
      {:else if pedalLearnModal.step === 2}
        <div class="kb-learn-step kb-learn-ok">
          &#10004; Laagste stand herkend: CC {pedalLearnModal.first.cc}, kanaal {pedalLearnModal.first.channel + 1} (waarde {pedalLearnModal.first.value})
        </div>
        <div class="kb-learn-step kb-learn-wait">
          <span class="learning-indicator"></span>
          Zet de trede nu in de <strong>HOOGSTE</strong> stand (open) en houd hem even stil&hellip;
        </div>
      {:else if pedalLearnModal.step === 'klaar'}
        <div class="kb-learn-step kb-learn-ok">&#10004; Trede ingeleerd: {pedalLearnModal.msg}</div>
      {:else}
        <div class="kb-learn-step kb-learn-fout">&#10006; {pedalLearnModal.msg}</div>
      {/if}
      <div class="kb-learn-knoppen">
        {#if pedalLearnModal.step === 'fout'}
          <button class="btn btn-secondary btn-sm" on:click={() => { const k = pedalLearnModal.kind; const d = pedalLearnModal.division; pedalLearnModal = null; learnPedalFlow(k, d); }}>Opnieuw</button>
        {/if}
        <button class="btn btn-ghost btn-sm" on:click={() => pedalLearnModal = null}>
          {pedalLearnModal.step === 'klaar' ? 'Sluiten' : 'Annuleren'}
        </button>
      </div>
    </div>
  </div>
{/if}
{#if kbLearnModal}
  <div class="kb-learn-overlay">
    <div class="kb-learn-modal">
      <h3>Klavier inleren — {kbLearnModal.division}</h3>
      {#if kbLearnModal.step === 1}
        <div class="kb-learn-step kb-learn-wait">
          <span class="learning-indicator"></span>
          Druk nu de <strong>LAAGSTE</strong> toets van dit klavier in…
        </div>
      {:else if kbLearnModal.step === 2}
        <div class="kb-learn-step kb-learn-ok">
          ✔ Laagste toets herkend: <strong>{noteName(kbLearnModal.first.note)}</strong> (MIDI-kanaal {kbLearnModal.first.channel + 1})
        </div>
        <div class="kb-learn-step kb-learn-wait">
          <span class="learning-indicator"></span>
          Druk nu de <strong>HOOGSTE</strong> toets in…
        </div>
      {:else if kbLearnModal.step === 'klaar'}
        <div class="kb-learn-step kb-learn-ok">✔ Klavier ingeleerd: {kbLearnModal.msg}</div>
      {:else}
        <div class="kb-learn-step kb-learn-fout">✖ {kbLearnModal.msg}</div>
      {/if}
      <div class="kb-learn-knoppen">
        {#if kbLearnModal.step === 'fout'}
          <button class="btn btn-secondary btn-sm" on:click={() => { const d = kbLearnModal.division; closeKbLearn(); learnChannel(d); }}>Opnieuw</button>
        {/if}
        <button class="btn btn-ghost btn-sm" on:click={closeKbLearn}>
          {kbLearnModal.step === 'klaar' ? 'Sluiten' : 'Annuleren'}
        </button>
      </div>
    </div>
  </div>
{/if}
</main>
