<script>
  // Secundaire App-shell voor extra registerschermen (vensters "panel-*").
  // Rendert dezelfde Header + Console + StatusBar als het hoofdvenster, maar
  // in secondary-modus: backend-pushes bij orgelwissel en MIDI-trigger-
  // consumptie blijven uit (het hoofdvenster doet die), vensterbeheer-acties
  // (orgel laden/sluiten, profielwissel, nieuw scherm, afsluiten) worden als
  // Tauri-event naar het hoofdvenster doorgestuurd, en divisiekeuze/layout/
  // hoofdbalk-zichtbaarheid worden per scherm × orgel bewaard (panel-state).
  import { onMount, onDestroy } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';

  import Header from './Header.svelte';
  import Console from './Console.svelte';
  import StatusBar from './StatusBar.svelte';
  import { loadPanelState, savePanelState } from '../lib/panelState.js';
  import { loadAudioProfiles, deriveProfileFromOutput } from '../lib/audioProfiles.js';
  import { pickDevice } from '../lib/audioDevices.js';

  // Schermnummer uit de URL-hash (#panel&n=2): bepaalt de panel-state-sleutel.
  const panelNumber = (() => {
    const m = window.location.hash.match(/[&?]n=(\d+)/);
    return m ? parseInt(m[1], 10) : null;
  })();

  let consoleComponent;
  let organInfo = null;
  let status = {
    audioRunning: false,
    midiConnected: false,
    organLoaded: false,
    voiceCount: 0,
    peakLeft: 0,
    peakRight: 0,
  };
  let error = null;
  let activeView = 'orgel';
  let showOrganBrowser = true;
  let midiMappings = [];

  // Hoofdbalk aan/uit — per scherm × orgel bewaard; default UIT zodat
  // bestaande gebruikers het vertrouwde compacte scherm houden.
  let showHeader = false;

  // Audio-instellingen-spiegels voor de Algemene Instellingen-weergave.
  let audioHosts = [];
  let audioDevices = [];
  let midiDevices = [];
  let selectedAudioHost = null;
  let selectedAudioDevice = null;
  let selectedBufferFrames = null;
  let selectedMidiDevice = null;

  // Profielen: inhoud uit localStorage (1s-poll), wissel-status via het
  // profile-switch-state-event van het hoofdvenster, en het actieve profiel
  // als vangnet afgeleid uit de eigen status-poll (host/device).
  let audioProfiles = loadAudioProfiles();
  let audioProfileSwitching = false;

  async function emitToMain(event, payload = undefined) {
    try {
      const { emit } = await import('@tauri-apps/api/event');
      await emit(event, payload);
    } catch (e) {
      console.error(`Event ${event} versturen mislukt:`, e);
    }
  }

  // ---- Polls ----
  let lastOrganJson = null;
  let prevOrganId = '';
  async function pollOrganInfo(force = false) {
    try {
      const info = await invoke('get_organ_info');
      const json = JSON.stringify(info);
      if (!force && json === lastOrganJson) return;
      lastOrganJson = json;
      organInfo = info;
      if (organInfo && organInfo.id !== prevOrganId) {
        prevOrganId = organInfo.id;
        // Per-scherm-state van dít orgel: hoofdbalk-zichtbaarheid.
        const st = loadPanelState(organInfo.id, panelNumber);
        showHeader = st?.showHeader === true;
        refreshMidiMappings();
      }
      if (organInfo) showOrganBrowser = false;
    } catch (e) {}
  }

  async function pollStatus() {
    try {
      const s = await invoke('get_status');
      status = {
        audioRunning: s.audio_running,
        midiConnected: s.midi_connected,
        organLoaded: s.organ_loaded,
        voiceCount: s.voice_count,
        peakLeft: s.peak_left,
        peakRight: s.peak_right,
        sampleRate: s.sample_rate,
        audioHost: s.audio_host,
        audioDevice: s.audio_device,
      };
      // Vangnet voor gemiste events: actief profiel afleiden uit wat er echt
      // speelt (zelfde logica als het hoofdvenster; niet tijdens een wissel).
      if (!audioProfileSwitching && s.audio_host) {
        const derived = deriveProfileFromOutput(audioProfiles, s.audio_host, s.audio_device);
        if (derived !== audioProfiles.active) {
          audioProfiles.active = derived;
          audioProfiles = audioProfiles;
        }
      }
    } catch (e) {}
  }

  function refreshProfilesFromStorage() {
    const fresh = loadAudioProfiles();
    // Inhoud volgen, maar de actieve stand laten bepalen door status/event.
    fresh.active = audioProfiles.active;
    if (JSON.stringify(fresh) !== JSON.stringify(audioProfiles)) audioProfiles = fresh;
  }

  async function refreshMidiMappings() {
    try { midiMappings = await invoke('get_midi_mappings'); } catch (e) {}
  }

  async function refreshDevices() {
    try {
      audioHosts = await invoke('list_audio_hosts');
      selectedAudioHost = selectedAudioHost
        || localStorage.getItem('jm-orgue-audio-host')
        || audioHosts.find(h => /wasapi/i.test(h)) || audioHosts[0] || null;
      await refreshDevicesForHost();
      const savedBuffer = Number(localStorage.getItem('jm-orgue-audio-buffer'));
      if (selectedBufferFrames == null) selectedBufferFrames = savedBuffer > 0 ? savedBuffer : null;
      midiDevices = await invoke('get_midi_devices');
    } catch (e) { /* instellingen-lijsten zijn niet kritiek voor het paneel */ }
  }

  async function refreshDevicesForHost() {
    try {
      audioDevices = await invoke('list_output_devices', { host: selectedAudioHost });
    } catch (e) {
      audioDevices = [];
    }
    selectedAudioDevice = pickDevice(audioDevices, localStorage.getItem('jm-orgue-audio-device'));
  }

  // ---- Hoofdbalk-toggle (per scherm × orgel onthouden) ----
  function setShowHeader(v) {
    showHeader = v;
    savePanelState(organInfo?.id, panelNumber, { showHeader: v });
  }

  // ---- Venstergeometrie bewaren (zoals het oude RegisterPanel) ----
  let geomTimer = null;
  let unlistenMoved = null;
  let unlistenResized = null;
  let unlistenCloseReq = null;
  async function initGeometryTracking() {
    if (panelNumber == null) return;
    try {
      const { getCurrentWindow } = await import('@tauri-apps/api/window');
      const w = getCurrentWindow();
      const schedule = () => {
        if (geomTimer) clearTimeout(geomTimer);
        geomTimer = setTimeout(saveGeometry, 400);
      };
      unlistenMoved = await w.onMoved(schedule);
      unlistenResized = await w.onResized(schedule);
      // Bij sluiten (handmatig óf programmatisch) de laatste positie vastleggen.
      unlistenCloseReq = await w.onCloseRequested(async () => {
        if (geomTimer) { clearTimeout(geomTimer); geomTimer = null; }
        await saveGeometry();
      });
    } catch (e) { /* niet in Tauri */ }
  }
  async function saveGeometry() {
    try {
      const { getCurrentWindow } = await import('@tauri-apps/api/window');
      const w = getCurrentWindow();
      // Geminimaliseerd venster meldt op Windows -32000,-32000 — niet bewaren.
      if (await w.isMinimized()) return;
      const factor = await w.scaleFactor();
      const pos = (await w.outerPosition()).toLogical(factor);
      const size = (await w.innerSize()).toLogical(factor);
      if (pos.x < -30000 || pos.y < -30000) return;
      savePanelState(organInfo?.id, panelNumber, {
        x: Math.round(pos.x), y: Math.round(pos.y),
        width: Math.round(size.width), height: Math.round(size.height),
      });
    } catch (e) {}
  }

  // ---- Console-events: globale acties direct, vensterbeheer via events ----
  async function toggleStop(stopId) {
    try {
      await invoke('toggle_stop', { stopId });
      await pollOrganInfo(true);
    } catch (e) { error = e.toString(); }
  }

  async function toggleCoupler(couplerId) {
    try {
      await invoke('toggle_coupler', { couplerId });
      await pollOrganInfo(true);
    } catch (e) { error = e.toString(); }
  }

  // Zelfde patroon als App.applyCrescendoStage (globale backend-commando's).
  async function applyCrescendoStage(stopIds) {
    try {
      const stops = stopIds.filter(id => !id.startsWith('coupler_'));
      const couplers = stopIds.filter(id => id.startsWith('coupler_'));
      await invoke('set_drawn_stops', { stopIds: stops });
      await invoke('set_active_couplers', { couplerIds: couplers });
      await pollOrganInfo(true);
    } catch (e) { error = e.toString(); }
  }

  async function setTremulant(divisionName, active) {
    try { await invoke('set_tremulant', { divisionName, active }); }
    catch (e) { console.error('Failed to set tremulant:', e); }
  }

  async function setMidiMapping(division, channel, transpose) {
    try {
      await invoke('set_midi_mapping', { division, channel, transpose });
      await refreshMidiMappings();
      emitToMain('jm-orgue:settings-changed', { scope: 'midi' });
    } catch (e) { error = e.toString(); }
  }

  async function learnMidiChannel(division) {
    try {
      const learned = await invoke('learn_midi_channel', { division });
      if (learned !== null) {
        await refreshMidiMappings();
        emitToMain('jm-orgue:settings-changed', { scope: 'midi' });
      }
    } catch (e) { error = e.toString(); }
  }

  async function learnKeyboardRange(division, firstSampleNote) {
    try {
      const result = await invoke('learn_keyboard_range', { division, firstSampleNote });
      if (result !== null) {
        await refreshMidiMappings();
        emitToMain('jm-orgue:settings-changed', { scope: 'midi' });
      }
      consoleComponent?.onLearnComplete?.();
    } catch (e) {
      error = e.toString();
      consoleComponent?.onLearnComplete?.();
    }
  }

  async function connectMidi(deviceName) {
    try {
      await invoke('connect_midi', { deviceName });
      selectedMidiDevice = deviceName;
      emitToMain('jm-orgue:settings-changed', { scope: 'midi' });
    } catch (e) { error = e.toString(); }
  }

  async function startAudio() {
    try { await invoke('start_audio', { deviceName: selectedAudioDevice }); }
    catch (e) { error = e.toString(); }
  }
  async function stopAudio() {
    try { await invoke('stop_audio'); }
    catch (e) { error = e.toString(); }
  }

  function requestShutdown() {
    if (!window.confirm('Software én computer afsluiten?\n\nDe laatste stand wordt opgeslagen.')) return;
    emitToMain('jm-orgue:shutdown');
  }

  async function learnShutdownAction() {
    try { await invoke('learn_preset_binding', { presetNum: 42 }); }
    catch (e) { console.error('Shutdown MIDI-leren mislukt:', e); }
  }

  function setAutostart(enabled) {
    invoke('set_autostart_enabled', { enabled }).catch(e => { error = `Autostart wijzigen mislukt: ${e}`; });
  }
  function setRestoreRegistration(enabled) {
    localStorage.setItem('jm-orgue-restore-registration', String(enabled));
    invoke('set_restore_registration', { enabled }).catch(() => {});
  }
  function setAutoLoadLastOrgan(enabled) {
    localStorage.setItem('jm-orgue-autoload-organ', String(enabled));
  }

  // F1/F2/F3-tabwissel, zoals in het hoofdvenster (alleen zinvol met balk aan).
  function handleKeyDown(event) {
    if (event.target.tagName === 'INPUT' || event.target.tagName === 'TEXTAREA') return;
    if (event.key === 'F1') { event.preventDefault(); activeView = 'orgel'; }
    else if (event.key === 'F2') { event.preventDefault(); activeView = 'orgel-instellingen'; }
    else if (event.key === 'F3') { event.preventDefault(); activeView = 'algemene-instellingen'; }
  }

  // Apparaatlijsten pas (ver)laden wanneer de instellingen zichtbaar worden.
  $: if (activeView === 'algemene-instellingen') refreshDevices();

  let organPoll = null;
  let statusPoll = null;
  let profilesPoll = null;
  let panelUnlisteners = [];

  onMount(async () => {
    await pollOrganInfo();
    organPoll = setInterval(pollOrganInfo, 300);
    // Bewust 250ms i.p.v. de 100ms van het hoofdvenster: ruim vlot genoeg voor
    // Header/StatusBar, en scheelt IPC-verkeer per extra scherm.
    pollStatus();
    statusPoll = setInterval(pollStatus, 250);
    profilesPoll = setInterval(refreshProfilesFromStorage, 1000);
    initGeometryTracking();
    refreshMidiMappings();
    window.addEventListener('keydown', handleKeyDown);

    // Wissel-status + actieve profiel van het hoofdvenster volgen.
    try {
      const { listen } = await import('@tauri-apps/api/event');
      panelUnlisteners.push(await listen('jm-orgue:profile-switch-state', (e) => {
        const p = e?.payload || {};
        audioProfileSwitching = p.switching === true;
        if (!p.switching && (p.active === 'speakers' || p.active === 'headphones' || p.active === null)) {
          audioProfiles.active = p.active;
          audioProfiles = audioProfiles;
        }
      }));
    } catch (e) {}
  });

  onDestroy(() => {
    if (organPoll) clearInterval(organPoll);
    if (statusPoll) clearInterval(statusPoll);
    if (profilesPoll) clearInterval(profilesPoll);
    if (geomTimer) clearTimeout(geomTimer);
    if (unlistenMoved) try { unlistenMoved(); } catch (e) {}
    if (unlistenResized) try { unlistenResized(); } catch (e) {}
    if (unlistenCloseReq) try { unlistenCloseReq(); } catch (e) {}
    for (const un of panelUnlisteners) { try { un(); } catch (e) {} }
    window.removeEventListener('keydown', handleKeyDown);
  });
</script>

<div id="app">
  {#if showHeader}
    <Header
      organName={organInfo?.name}
      organLoaded={!showOrganBrowser && organInfo}
      {status}
      {activeView}
      {audioProfiles}
      {audioProfileSwitching}
      secondary
      on:startAudio={startAudio}
      on:stopAudio={stopAudio}
      on:closeOrgan={() => emitToMain('jm-orgue:close-organ', {})}
      on:setView={(e) => activeView = e.detail}
      on:toggleAudioProfile={() => emitToMain('jm-orgue:switch-profile', { kind: null })}
      on:toggleHeader={() => setShowHeader(false)}
    />
  {/if}

  <div class="main-content">
    <Console
      bind:this={consoleComponent}
      secondary
      {panelNumber}
      showPanelHeaderToggle={!showHeader}
      {organInfo}
      loading={false}
      loadingProgress={0}
      loadingMessage={''}
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
      autostartEnabled={false}
      restoreRegistration={localStorage.getItem('jm-orgue-restore-registration') === 'true'}
      autoLoadLastOrgan={localStorage.getItem('jm-orgue-autoload-organ') !== 'false'}
      {audioProfiles}
      audioEpoch={0}
      on:toggleStop={(e) => toggleStop(e.detail)}
      on:toggleCoupler={(e) => toggleCoupler(e.detail)}
      on:crescendoChange={(e) => applyCrescendoStage(e.detail)}
      on:refreshDevices={refreshDevices}
      on:refresh={refreshDevices}
      on:setView={(e) => activeView = e.detail}
      on:loadOrgan={(e) => emitToMain('jm-orgue:load-organ', { kind: 'organ', path: e.detail })}
      on:scanFolder={(e) => emitToMain('jm-orgue:load-organ', { kind: 'folder', path: e.detail })}
      on:setTremulant={(e) => setTremulant(e.detail.division, e.detail.active)}
      on:setMidiMapping={(e) => setMidiMapping(e.detail.division, e.detail.channel, e.detail.transpose)}
      on:refreshMidiMappings={() => { refreshMidiMappings(); emitToMain('jm-orgue:settings-changed', { scope: 'midi' }); }}
      on:learnMidiChannel={(e) => learnMidiChannel(e.detail)}
      on:learnKeyboardRange={(e) => learnKeyboardRange(e.detail.division, e.detail.firstSampleNote)}
      on:selectAudioDevice={(e) => selectedAudioDevice = e.detail}
      on:selectAudioHost={async (e) => { selectedAudioHost = e.detail; await refreshDevicesForHost(); }}
      on:selectBuffer={(e) => selectedBufferFrames = e.detail}
      on:applyAudioOutput={() => emitToMain('jm-orgue:apply-audio-output', { host: selectedAudioHost, device: selectedAudioDevice, bufferFrames: selectedBufferFrames || null })}
      on:saveAudioProfile={(e) => emitToMain('jm-orgue:save-audio-profile', { kind: e.detail, host: selectedAudioHost, device: selectedAudioDevice, bufferFrames: selectedBufferFrames || null })}
      on:clearAudioProfile={(e) => emitToMain('jm-orgue:clear-audio-profile', { kind: e.detail })}
      on:switchAudioProfile={(e) => emitToMain('jm-orgue:switch-profile', { kind: e.detail ?? null })}
      on:connectMidi={(e) => connectMidi(e.detail)}
      on:volumeChange={(e) => invoke('set_master_volume', { db: e.detail }).catch(() => {})}
      on:reverbChange={(e) => invoke('set_reverb_mix', { mix: e.detail }).catch(() => {})}
      on:shutdownRequest={requestShutdown}
      on:shutdownLearn={learnShutdownAction}
      on:setAutostart={(e) => setAutostart(e.detail)}
      on:setRestoreRegistration={(e) => setRestoreRegistration(e.detail)}
      on:setAutoLoadLastOrgan={(e) => setAutoLoadLastOrgan(e.detail)}
      on:persistSettings={() => emitToMain('jm-orgue:settings-changed', { scope: 'persist' })}
      on:showHeaderRequest={() => setShowHeader(true)}
    />
  </div>

  {#if showHeader}
    <StatusBar {status} {error} />
  {/if}
</div>

<style>
  .main-content {
    position: relative;
  }
</style>
