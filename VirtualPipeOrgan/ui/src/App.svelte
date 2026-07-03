<script>
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';

  import Header from './components/Header.svelte';
  import Console from './components/Console.svelte';
  import StatusBar from './components/StatusBar.svelte';
  import RegisterPanel from './components/RegisterPanel.svelte';
  import { tx } from './lib/i18n.js';

  // Check if this window is a panel (extra register window)
  const isPanel = window.location.hash.includes('panel');

  // Sessie-bestand: laatst geopend orgel + open extra-vensters (per gebruiker).
  // Bewaard in localStorage zodat de app opent waar je gebleven was.
  const SESSION_KEY = 'jm-orgue-session';
  function loadSession() {
    try { return JSON.parse(localStorage.getItem(SESSION_KEY) || '{}'); }
    catch (e) { return {}; }
  }
  function saveSession(patch) {
    const cur = loadSession();
    const next = { ...cur, ...patch };
    localStorage.setItem(SESSION_KEY, JSON.stringify(next));
  }

  let audioDevices = [];
  let audioHosts = [];
  let selectedAudioHost = null;
  let selectedBufferFrames = null;
  let midiDevices = [];
  let selectedAudioDevice = null;
  let selectedMidiDevice = null;
  let organInfo = null;
  let status = {
    audioRunning: false,
    midiConnected: false,
    organLoaded: false,
    voiceCount: 0,
    peakLeft: 0,
    peakRight: 0
  };

  let loading = false;
  let loadingProgress = 0;
  let loadingMessage = '';
  let error = null;

  // UI state
  let showOrganBrowser = true;
  let activeView = 'orgel';

  // MIDI mapping state
  let midiMappings = [];

  // Console component reference
  let consoleComponent;

  // Keyboard state for playing notes
  let pressedKeys = new Set();

  // Toetsenbord → MIDI-noten (vanaf C3 = 48)
  // Onderste octaaf wit:   z=C3, x=D3, c=E3, v=F3, b=G3, n=A3, m=B3
  // Onderste octaaf zwart: s=C#3, d=D#3, g=F#3, h=G#3, j=A#3
  // Bovenste octaaf wit:   q=C4, w=D4, e=E4, r=F4, t=G4, y=A4, u=B4, i=C5
  // Bovenste octaaf zwart: 2=C#4, 3=D#4, 5=F#4, 6=G#4, 7=A#4
  const keyToNote = {
    // Lower octave white keys (C3-B3)
    'z': 48, 'x': 50, 'c': 52, 'v': 53, 'b': 55, 'n': 57, 'm': 59,
    // Lower octave black keys
    's': 49, 'd': 51, 'g': 54, 'h': 56, 'j': 58,
    // Upper octave white keys (C4-C5)
    'q': 60, 'w': 62, 'e': 64, 'r': 65, 't': 67, 'y': 69, 'u': 71, 'i': 72,
    // Upper octave black keys
    '2': 61, '3': 63, '5': 66, '6': 68, '7': 70,
  };

  onMount(async () => {
    if (isPanel) return; // Panel windows handle their own state

    await refreshDevices();
    await refreshStatus();
    await refreshMidiMappings();

    // Auto-connect ALL available MIDI devices at startup
    if (midiDevices.some(d => d.is_input)) {
      try {
        const count = await invoke('connect_all_midi');
        if (count > 0) {
          selectedMidiDevice = 'all';
          status.midiConnected = true;
        }
      } catch (e) {
        console.warn('Auto-connect MIDI failed:', e);
      }
    }

    // Poll status every 100ms
    const statusInterval = setInterval(refreshStatus, 100);

    // Poll organ info every 300ms to sync with extra windows. Ook syncen wanneer
    // een orgel buiten de UI om is geladen (bv. via de test-API): status.organLoaded
    // komt uit de 100ms-statuspoll. Zo weet de UI altijd van het geladen orgel,
    // waardoor o.a. de reverb-instellingen correct (her)toegepast worden.
    //
    // BELANGRIJK: alleen hertoewijzen als de inhoud écht veranderde. Anders triggert
    // elke poll (300ms) ALLE `$: if (organInfo)`-reactives in Console, die backend-
    // commando's sturen (wind, output-pairs, reverb). Die continue commando-stroom
    // belastte de audio-thread en kon — met actieve reverb — de audio-callback laten
    // vastlopen bij een note-on. Door de no-op-poll te negeren blijft de UI gesynct
    // zónder onnodig commando-verkeer.
    let lastOrganInfoJson = null;
    let lastDrawnSignature = '';
    const organInterval = setInterval(async () => {
      if (organInfo || status.organLoaded) {
        try {
          const fresh = await invoke('get_organ_info');
          const freshJson = JSON.stringify(fresh);
          if (freshJson !== lastOrganInfoJson) {
            lastOrganInfoJson = freshJson;
            organInfo = fresh;
            // Auto-save de laatste stand (registratie + koppels) bij elke wijziging,
            // gedebounced. Schrijft via save_current_organ_settings — die leest
            // drawn_stops + actieve koppels uit de backend en bewaart .jm-settings.json.
            const sig = (fresh?.divisions || [])
              .flatMap(d => d.stops.filter(s => s.drawn).map(s => s.id))
              .sort().join('|')
              + '||' + (fresh?.couplers || []).filter(c => c.active).map(c => c.id).sort().join('|');
            if (sig !== lastDrawnSignature) {
              lastDrawnSignature = sig;
              scheduleAutoSave();
            }
          }
        } catch(e) {}
      }
    }, 300);

    // Auto-rescan MIDI devices every 5s to detect newly paired Bluetooth devices
    const midiRescanInterval = setInterval(async () => {
      try {
        const newDevices = await invoke('get_midi_devices');
        if (newDevices.length !== midiDevices.length) {
          midiDevices = newDevices;
          // Auto-connect newly available devices
          await invoke('connect_all_midi');
        }
      } catch(e) {}
    }, 5000);

    // Add keyboard listeners
    window.addEventListener('keydown', handleKeyDown);
    window.addEventListener('keyup', handleKeyUp);

    // Bij sluiten van het hoofdvenster: laatste stand veilig wegschrijven (sync zo veel
    // mogelijk via flushAutoSave). De Tauri close-event laat ons de close uitstellen
    // tot persistOrganSettings klaar is.
    let unlistenClose = null;
    try {
      const { getCurrentWindow } = await import('@tauri-apps/api/window');
      const w = getCurrentWindow();
      unlistenClose = await w.onCloseRequested(async (event) => {
        // Tauri v2: voorkom de automatische close, doe (begrensde) opslag, sluit dan
        // expliciet met destroy(). Een harde timeout van 2,5s zorgt dat het kruisje
        // ALTIJD sluit, ook als opslaan ergens blijft hangen.
        event.preventDefault();
        try {
          await Promise.race([
            (async () => {
              const rec = await invoke('get_recording_status').catch(() => null);
              if (rec && rec.recording) { await invoke('stop_recording').catch(() => {}); }
              await flushAutoSave();
              await persistOrganSettings();
              await closeExtraPanels();
            })(),
            new Promise((r) => setTimeout(r, 2500)),
          ]);
        } catch (e) { /* sluiten mag niet falen */ }
        // destroy() sluit zonder opnieuw close-requested te triggeren (geen lus).
        try { await w.destroy(); } catch (e) {}
      });
    } catch (e) { /* niet in Tauri context (browser/dev) */ }

    // Herstel laatst geopend orgel als er een sessie is.
    try {
      const sess = loadSession();
      if (sess?.lastOrgan?.path && !organInfo) {
        if (sess.lastOrgan.kind === 'folder') await loadFromFolder(sess.lastOrgan.path);
        else await loadOrgan(sess.lastOrgan.path);
        // Heropen extra register-vensters die open waren (aantal bewaard in sessie).
        if (Array.isArray(sess.openPanels) && sess.openPanels.length > 0) {
          for (let i = 0; i < sess.openPanels.length; i++) {
            try { await consoleComponent?.openPanel?.(); } catch (e) {}
          }
        }
      }
    } catch (e) { /* sessie-herstel mag niet de app slopen */ }

    return () => {
      clearInterval(statusInterval);
      clearInterval(organInterval);
      clearInterval(midiRescanInterval);
      window.removeEventListener('keydown', handleKeyDown);
      window.removeEventListener('keyup', handleKeyUp);
      if (unlistenClose) try { unlistenClose(); } catch(e) {}
    };
  });

  // Debounced auto-save: bij wijziging na 800ms wegschrijven. flushAutoSave() wacht
  // op een eventuele pending save (gebruikt door de close-handler).
  let autoSaveTimer = null;
  let autoSavePromise = null;
  function scheduleAutoSave() {
    if (autoSaveTimer) clearTimeout(autoSaveTimer);
    autoSaveTimer = setTimeout(() => {
      autoSaveTimer = null;
      autoSavePromise = persistOrganSettings().catch(() => {}).finally(() => { autoSavePromise = null; });
    }, 800);
  }
  async function flushAutoSave() {
    if (autoSaveTimer) {
      clearTimeout(autoSaveTimer);
      autoSaveTimer = null;
      autoSavePromise = persistOrganSettings().catch(() => {});
    }
    if (autoSavePromise) await autoSavePromise;
  }

  function handleKeyDown(event) {
    // Ignore if typing in input
    if (event.target.tagName === 'INPUT' || event.target.tagName === 'TEXTAREA') return;

    // Global function keys (always handled)
    if (event.key === 'F1') { event.preventDefault(); activeView = 'orgel'; return; }
    if (event.key === 'F2') { event.preventDefault(); activeView = 'orgel-instellingen'; return; }
    if (event.key === 'F3') { event.preventDefault(); activeView = 'algemene-instellingen'; return; }
    if (event.key === 'Escape' && organInfo) { event.preventDefault(); showOrganBrowser = true; return; }

    // Ignore repeated keydown events
    if (event.repeat) return;

    const key = event.key.toLowerCase();
    const note = keyToNote[key];

    if (note !== undefined && !pressedKeys.has(key)) {
      pressedKeys.add(key);
      playNote(note, 0.8);
    }
  }

  function handleKeyUp(event) {
    const key = event.key.toLowerCase();
    const note = keyToNote[key];

    if (note !== undefined && pressedKeys.has(key)) {
      pressedKeys.delete(key);
      stopNote(note);
    }
  }

  async function playNote(note, velocity) {
    try {
      await invoke('play_note_all_stops', { note, velocity });
    } catch (e) {
      console.error('Failed to play note:', e);
    }
  }

  async function stopNote(note) {
    try {
      await invoke('stop_note_all_stops', { note });
    } catch (e) {
      console.error('Failed to stop note:', e);
    }
  }

  async function refreshDevices() {
    try {
      audioHosts = await invoke('list_audio_hosts');
      selectedAudioHost = localStorage.getItem('jm-orgue-audio-host')
        || audioHosts.find(h => /wasapi/i.test(h)) || audioHosts[0] || null;
      await refreshDevicesForHost();

      const savedBuffer = Number(localStorage.getItem('jm-orgue-audio-buffer'));
      selectedBufferFrames = savedBuffer > 0 ? savedBuffer : null;

      midiDevices = await invoke('get_midi_devices');
    } catch (e) {
      error = e.toString();
    }
  }

  // Load the output device list for the currently selected host and pick a device.
  async function refreshDevicesForHost() {
    try {
      audioDevices = await invoke('list_output_devices', { host: selectedAudioHost });
    } catch (e) {
      audioDevices = [];
    }
    const saved = localStorage.getItem('jm-orgue-audio-device');
    if (saved && audioDevices.some(d => d.name === saved)) {
      selectedAudioDevice = saved;
    } else {
      const def = audioDevices.find(d => d.is_default);
      selectedAudioDevice = def ? def.name : (audioDevices[0] ? audioDevices[0].name : null);
    }
  }

  async function handleSelectAudioHost(host) {
    selectedAudioHost = host;
    await refreshDevicesForHost();
  }

  function handleSelectBuffer(frames) {
    selectedBufferFrames = frames;
  }

  // Apply the chosen host/device/buffer, persist it, and reload the current organ
  // so its samples re-register on the new audio thread and DSP settings re-apply.
  async function applyAudioOutput() {
    try {
      const organId = await invoke('set_audio_output', {
        host: selectedAudioHost,
        device: selectedAudioDevice,
        bufferFrames: selectedBufferFrames || null,
      });
      if (selectedAudioHost) localStorage.setItem('jm-orgue-audio-host', selectedAudioHost);
      if (selectedAudioDevice) localStorage.setItem('jm-orgue-audio-device', selectedAudioDevice);
      if (selectedBufferFrames) localStorage.setItem('jm-orgue-audio-buffer', String(selectedBufferFrames));
      else localStorage.removeItem('jm-orgue-audio-buffer');

      if (organId) {
        // Orgel-definitiebestanden (GrandOrgue .organ én Hauptwerk
        // .Organ_Hauptwerk_xml) via loadOrgan; anders is het een sample-map.
        const idLower = organId.toLowerCase();
        if (idLower.endsWith('.organ') || idLower.endsWith('.organ_hauptwerk_xml')) {
          await loadOrgan(organId);
        } else {
          await loadFromFolder(organId);
        }
      }
    } catch (e) {
      error = e.toString();
    }
  }

  async function refreshStatus() {
    try {
      const s = await invoke('get_status');
      status = {
        audioRunning: s.audio_running,
        midiConnected: s.midi_connected,
        organLoaded: s.organ_loaded,
        voiceCount: s.voice_count,
        peakLeft: s.peak_left,
        peakRight: s.peak_right,
        sampleRate: s.sample_rate
      };
    } catch (e) {
      // Ignore polling errors
    }
  }

  async function startAudio() {
    try {
      await invoke('start_audio', { deviceName: selectedAudioDevice });
    } catch (e) {
      error = e.toString();
    }
  }

  async function stopAudio() {
    try {
      await invoke('stop_audio');
    } catch (e) {
      error = e.toString();
    }
  }

  async function connectMidi(deviceName) {
    try {
      await invoke('connect_midi', { deviceName });
      selectedMidiDevice = deviceName;
    } catch (e) {
      error = e.toString();
    }
  }

  // Sluit alle extra register-paneel-vensters (labels "panel-..."). Aangeroepen
  // bij het laden van een (ander) orgel — anders blijven oude panels open staan
  // met de registers van het vorige orgel.
  async function closeExtraPanels() {
    try {
      const { getAllWebviewWindows } = await import('@tauri-apps/api/webviewWindow');
      const wins = await getAllWebviewWindows();
      for (const w of wins) {
        if (w.label && w.label.startsWith('panel-')) {
          try { await w.close(); } catch (e) {}
        }
      }
    } catch (e) { /* niet in Tauri of geen extra vensters */ }
  }

  async function loadOrgan(path) {
    loading = true;
    loadingProgress = 0;
    loadingMessage = tx('status.loading');
    error = null;

    try {
      // Bewaar laatste stand van het huidige orgel voordat we wisselen.
      if (organInfo) { try { await flushAutoSave(); await persistOrganSettings(); } catch(e) {} }
      await closeExtraPanels();
      loadingMessage = tx('status.loading_samples');
      organInfo = await invoke('load_organ', { path });
      showOrganBrowser = false;
      await restorePresetsToLocalStorage();
      try { await invoke('apply_saved_voicings'); } catch (e) {}
      try { await invoke('apply_saved_output_channels'); } catch (e) {}
      saveSession({ lastOrgan: { kind: 'organ', path } });
    } catch (e) {
      error = e.toString();
    } finally {
      loading = false;
    }
  }

  async function loadFromFolder(directory) {
    loading = true;
    loadingProgress = 0;
    loadingMessage = tx('status.scanning_folder');
    error = null;

    try {
      // Bewaar laatste stand van het huidige orgel voordat we wisselen.
      if (organInfo) { try { await flushAutoSave(); await persistOrganSettings(); } catch(e) {} }
      await closeExtraPanels();
      loadingMessage = tx('status.loading_samples');
      organInfo = await invoke('load_samples_from_directory', { directory });
      showOrganBrowser = false;
      await restorePresetsToLocalStorage();
      try { await invoke('apply_saved_voicings'); } catch (e) {}
      try { await invoke('apply_saved_output_channels'); } catch (e) {}
      saveSession({ lastOrgan: { kind: 'folder', path: directory } });
    } catch (e) {
      error = e.toString();
    } finally {
      loading = false;
    }
  }

  // Restore saved presets from library to localStorage (so SetzerBar picks them up)
  async function restorePresetsToLocalStorage() {
    if (!organInfo) return;
    try {
      const presets = await invoke('get_saved_presets');
      if (presets && Object.keys(presets).length > 0) {
        const hash = hashCode(organInfo.id);
        const storageKey = `jm-orgue-presets-${hash}`;
        localStorage.setItem(storageKey, JSON.stringify(presets));
      }
    } catch (e) {
      // Ignore - presets from localStorage will be used as fallback
    }
  }

  async function toggleStop(stopId) {
    try {
      await invoke('toggle_stop', { stopId });
      // Refresh organ info
      organInfo = await invoke('get_organ_info');
    } catch (e) {
      error = e.toString();
    }
  }

  async function toggleCoupler(couplerId) {
    try {
      await invoke('toggle_coupler', { couplerId });
      organInfo = await invoke('get_organ_info');
    } catch (e) {
      error = e.toString();
    }
  }

  // Crescendo-stap uit de UI toepassen: de backend zet bij een klik alleen de
  // trap-teller en geeft de doelregisters terug (Console dispatcht die hierheen).
  // Toepassen gaat via hetzelfde patroon als preset-herstel in SetzerBar:
  // set_drawn_stops + organ-info verversen.
  async function applyCrescendoStage(stopIds) {
    try {
      await invoke('set_drawn_stops', { stopIds });
      organInfo = await invoke('get_organ_info');
    } catch (e) {
      error = e.toString();
    }
  }

  function handleVolumeChange(db) {
    invoke('set_master_volume', { db });
  }

  function handleReverbChange(mix) {
    invoke('set_reverb_mix', { mix });
  }

  function handleSelectAudioDevice(deviceName) {
    selectedAudioDevice = deviceName;
  }

  async function closeOrgan() {
    // Save current organ settings before closing
    await flushAutoSave();
    await persistOrganSettings();
    organInfo = null;
    showOrganBrowser = true;
  }

  // Autostart bij Windows + computer afsluiten (#13)
  let autostartEnabled = false;
  invoke('get_autostart_enabled').then(v => { autostartEnabled = !!v; }).catch(() => {});

  // Laatste registratie herstellen bij openen (default uit = schone start).
  let restoreRegistration = localStorage.getItem('jm-orgue-restore-registration') === 'true';
  // Push de opgeslagen voorkeur direct naar de backend zodat het herstel klopt.
  invoke('set_restore_registration', { enabled: restoreRegistration }).catch(() => {});
  async function setRestoreRegistration(enabled) {
    restoreRegistration = enabled;
    localStorage.setItem('jm-orgue-restore-registration', String(enabled));
    try { await invoke('set_restore_registration', { enabled }); } catch (e) {}
  }

  async function setAutostart(enabled) {
    try {
      await invoke('set_autostart_enabled', { enabled });
      autostartEnabled = enabled;
    } catch (e) {
      error = `Autostart wijzigen mislukt: ${e}`;
    }
  }

  // Bevestiging vragen, opslaan, computer netjes afsluiten.
  async function requestShutdown() {
    const ok = window.confirm('Software én computer afsluiten?\n\nDe laatste stand wordt opgeslagen.');
    if (!ok) return;
    try {
      await flushAutoSave();
      await persistOrganSettings();
      await closeExtraPanels();
    } catch (e) {}
    try { await invoke('shutdown_computer'); }
    catch (e) { error = `Afsluiten mislukt: ${e}`; }
  }

  // MIDI-leren voor de afsluit-actie (preset_num 42).
  async function learnShutdownAction() {
    try { await invoke('learn_preset_binding', { presetNum: 42 }); }
    catch (e) { console.error('Shutdown MIDI-leren mislukt:', e); }
  }

  async function persistOrganSettings() {
    if (!organInfo) return;
    try {
      const hash = hashCode(organInfo.id);
      const storageKey = `jm-orgue-presets-${hash}`;
      let presets = {};
      try {
        const saved = localStorage.getItem(storageKey);
        if (saved) presets = JSON.parse(saved);
      } catch (e) {}
      await invoke('save_current_organ_settings', { presets });
    } catch (e) {
      console.error('Failed to save organ settings:', e);
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

  // MIDI mapping functions
  async function refreshMidiMappings() {
    try {
      midiMappings = await invoke('get_midi_mappings');
    } catch (e) {
      console.error('Failed to get MIDI mappings:', e);
    }
  }

  async function setTremulant(divisionName, active) {
    try {
      await invoke('set_tremulant', { divisionName, active });
    } catch (e) {
      console.error('Failed to set tremulant:', e);
    }
  }

  async function setMidiMapping(division, channel, transpose) {
    try {
      await invoke('set_midi_mapping', { division, channel, transpose });
      await refreshMidiMappings();
    } catch (e) {
      error = e.toString();
    }
  }

  async function learnMidiChannel(division) {
    try {
      // Start learning - this will wait for MIDI input
      const learnedChannel = await invoke('learn_midi_channel', { division });
      if (learnedChannel !== null) {
        // A channel was learned, refresh mappings
        await refreshMidiMappings();
      }
    } catch (e) {
      error = e.toString();
    }
  }

  async function learnKeyboardRange(division, firstSampleNote) {
    try {
      // Start learning keyboard range like the reference software:
      // Press lowest key, then highest key
      const result = await invoke('learn_keyboard_range', { division, firstSampleNote });
      if (result !== null) {
        // Range was learned, refresh mappings
        await refreshMidiMappings();
      }
      // Notify console that learning is complete
      if (consoleComponent) {
        consoleComponent.onLearnComplete();
      }
    } catch (e) {
      error = e.toString();
      // Also notify on error
      if (consoleComponent) {
        consoleComponent.onLearnComplete();
      }
    }
  }

</script>

{#if isPanel}
  <!-- Extra register window - standalone panel -->
  <RegisterPanel />
{:else}
<div id="app">
  <Header
    organName={organInfo?.name}
    organLoaded={!showOrganBrowser && organInfo}
    {status}
    {activeView}
    on:startAudio={startAudio}
    on:stopAudio={stopAudio}
    on:closeOrgan={closeOrgan}
    on:setView={(e) => activeView = e.detail}
  />

  <div class="main-content">
    <Console
      bind:this={consoleComponent}
      {organInfo}
      {loading}
      {loadingProgress}
      {loadingMessage}
      bind:showOrganBrowser
      {midiMappings}
      {activeView}
      {audioDevices}
      {audioHosts}
      {selectedAudioHost}
      {selectedBufferFrames}
      {midiDevices}
      {selectedAudioDevice}
      {selectedMidiDevice}
      midiConnected={status.midiConnected}
      sampleRate={status.sampleRate || 0}
      {autostartEnabled}
      {restoreRegistration}
      on:toggleStop={(e) => toggleStop(e.detail)}
      on:toggleCoupler={(e) => toggleCoupler(e.detail)}
      on:crescendoChange={(e) => applyCrescendoStage(e.detail)}
      on:refreshDevices={refreshDevices}
      on:refresh={refreshDevices}
      on:setView={(e) => activeView = e.detail}
      on:loadOrgan={(e) => loadOrgan(e.detail)}
      on:scanFolder={(e) => loadFromFolder(e.detail)}
      on:setTremulant={(e) => setTremulant(e.detail.division, e.detail.active)}
      on:setMidiMapping={(e) => setMidiMapping(e.detail.division, e.detail.channel, e.detail.transpose)}
      on:learnMidiChannel={(e) => learnMidiChannel(e.detail)}
      on:learnKeyboardRange={(e) => learnKeyboardRange(e.detail.division, e.detail.firstSampleNote)}
      on:selectAudioDevice={(e) => handleSelectAudioDevice(e.detail)}
      on:selectAudioHost={(e) => handleSelectAudioHost(e.detail)}
      on:selectBuffer={(e) => handleSelectBuffer(e.detail)}
      on:applyAudioOutput={applyAudioOutput}
      on:connectMidi={(e) => connectMidi(e.detail)}
      on:volumeChange={(e) => handleVolumeChange(e.detail)}
      on:reverbChange={(e) => handleReverbChange(e.detail)}
      on:shutdownRequest={requestShutdown}
      on:shutdownLearn={learnShutdownAction}
      on:setAutostart={(e) => setAutostart(e.detail)}
      on:setRestoreRegistration={(e) => setRestoreRegistration(e.detail)}
      on:persistSettings={persistOrganSettings}
    />
  </div>

  <StatusBar
    {status}
    {error}
  />
</div>
{/if}

<style>
  .main-content {
    position: relative;
  }
</style>
