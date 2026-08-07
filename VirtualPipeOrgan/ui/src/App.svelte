<script>
  import { onMount, tick } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';

  import Header from './components/Header.svelte';
  import Console from './components/Console.svelte';
  import StatusBar from './components/StatusBar.svelte';
  import PanelApp from './components/PanelApp.svelte';
  import NotationWindow from './components/NotationWindow.svelte';
  import { tx } from './lib/i18n.js';
  import { loadAudioProfiles, profileMatchesOutput, deriveProfileFromOutput as deriveProfile, AUDIO_PROFILES_KEY } from './lib/audioProfiles.js';
  import { pickDevice } from './lib/audioDevices.js';

  // Venstertype uit de URL-hash: extra registerscherm (#panel&n=N) of
  // notatievenster (#notation&file=...); anders het hoofdvenster.
  const isPanel = window.location.hash.includes('panel');
  const isNotation = window.location.hash.includes('notation');

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
    if (isPanel || isNotation) return; // panel-/notatievensters regelen hun eigen state

    await refreshDevices();
    await refreshStatus();

    // Migratie (0.7.3): er mag nooit een "profielloze" derde toestand bestaan.
    // Draait er wél een uitgang maar is er geen profiel actief, leg die uitgang
    // dan eenmalig vast als profiel — Hoofdtelefoon als alleen dát bestaat,
    // anders Speakers — zodat de wisselknop altijd twee heldere standen heeft.
    if (!audioProfiles.active && status.audioHost) {
      const kind = (audioProfiles.headphones && !audioProfiles.speakers) ? 'headphones' : 'speakers';
      if (!audioProfiles[kind]) {
        audioProfiles[kind] = {
          host: status.audioHost,
          device: status.audioDevice || null,
          bufferFrames: selectedBufferFrames || null,
          channels: null,
        };
      }
      audioProfiles.active = kind;
      audioProfiles = audioProfiles;
      persistAudioProfiles();
    }

    // Kanaal-override van het actieve uitvoerprofiel direct naar de backend —
    // vóór de autoload van het laatste orgel, zodat diens
    // apply_saved_output_channels de profielkanalen meteen meeneemt.
    if (audioProfiles.active && audioProfiles[audioProfiles.active]) {
      await pushProfileChannelOverride(audioProfiles[audioProfiles.active]);
    }
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
        const countChanged = newDevices.length !== midiDevices.length;
        if (countChanged) {
          midiDevices = newDevices;
        }
        // Ook zonder apparaat-wijziging opnieuw verbinden zolang er inputs
        // zijn maar geen actieve verbinding: een console die nét ingeschakeld
        // is kan de eerste connect-poging laten mislukken (USB-handshake), en
        // die werd voorheen nooit herhaald — de gebruiker moest dan handmatig
        // MIDI "aan/uit zetten" om de klavieren te activeren.
        if ((countChanged || !status.midiConnected) && newDevices.some(d => d.is_input)) {
          const count = await invoke('connect_all_midi');
          if (count > 0) {
            selectedMidiDevice = 'all';
          }
        }
      } catch(e) {}
    }, 5000);

    // Add keyboard listeners
    window.addEventListener('keydown', handleKeyDown);
    window.addEventListener('keyup', handleKeyUp);

    // Laad-voortgang van de backend (organ-load): vult de voortgangsbalk in de
    // LoadingOverlay. Zonder deze events bleef de balk op 0% staan en oogde een
    // grote (koude) GrandOrgue-load als een vastgelopen app.
    let unlistenProgress = null;
    try {
      const { listen } = await import('@tauri-apps/api/event');
      unlistenProgress = await listen('load-progress', (event) => {
        const p = event.payload || {};
        if (loading && p.total > 0) {
          loadingProgress = Math.min(100, Math.round((p.loaded / p.total) * 100));
          loadingMessage = `${p.message || tx('status.loading_samples')} (${p.loaded}/${p.total})`;
        }
      });
    } catch (e) { /* niet in Tauri context */ }

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

    // Acties vanuit extra registerschermen (die missen de App-context) komen
    // als Tauri-event bij dit hoofdvenster binnen: vensterbeheer, afsluiten,
    // orgel laden/sluiten, audio-uitvoer/profielen en settings-wijzigingen.
    let panelUnlisteners = [];
    try {
      const { listen } = await import('@tauri-apps/api/event');
      panelUnlisteners.push(await listen('jm-orgue:open-panel', () => {
        try { consoleComponent?.openPanel?.(); } catch (e) {}
      }));
      // De bevestigingsvraag is al in het extra scherm gesteld.
      panelUnlisteners.push(await listen('jm-orgue:shutdown', () => { doShutdown(); }));
      // Orgel laden/sluiten vanuit een paneel: de bestaande flows hier regelen
      // het sluiten + herstellen van de panelen al correct.
      panelUnlisteners.push(await listen('jm-orgue:load-organ', (e) => {
        const p = e?.payload || {};
        if (!p.path) return;
        if (p.kind === 'folder') loadFromFolder(p.path);
        else loadOrgan(p.path);
      }));
      panelUnlisteners.push(await listen('jm-orgue:close-organ', () => { closeOrgan(); }));
      // Profielwissel/audio-uitvoer vanuit een paneel: alleen het hoofdvenster
      // draait de volledige wissel-flow (herlaad, vlaggen, panelbeheer).
      panelUnlisteners.push(await listen('jm-orgue:switch-profile', (e) => {
        switchAudioProfile(e?.payload?.kind ?? null);
      }));
      panelUnlisteners.push(await listen('jm-orgue:apply-audio-output', async (e) => {
        const p = e?.payload || {};
        try {
          if (p.host) { selectedAudioHost = p.host; await refreshDevicesForHost(); }
          if (p.device) selectedAudioDevice = p.device;
          selectedBufferFrames = p.bufferFrames || null;
          await applyAudioOutput();
        } catch (err) { /* fout staat al in `error` */ }
      }));
      panelUnlisteners.push(await listen('jm-orgue:save-audio-profile', (e) => {
        const p = e?.payload || {};
        if (p.kind) saveAudioProfile(p.kind, { host: p.host, device: p.device, bufferFrames: p.bufferFrames });
      }));
      panelUnlisteners.push(await listen('jm-orgue:clear-audio-profile', (e) => {
        const p = e?.payload || {};
        if (p.kind) clearAudioProfile(p.kind);
      }));
      // Instellingen gewijzigd in een paneel: lijsten verversen + autosaven
      // (zelfde route als Console's eigen refreshMidiMappings-event).
      panelUnlisteners.push(await listen('jm-orgue:settings-changed', (e) => {
        try { refreshMidiMappings(); } catch (err) {}
        if (e?.payload?.scope === 'persist') persistOrganSettings();
        else scheduleAutoSave();
      }));
    } catch (e) { /* niet in Tauri context */ }

    // Herstel laatst geopend orgel als er een sessie is — instelbaar via
    // "Laatst geopende orgel automatisch laden" (Algemene Instellingen).
    // De extra registerschermen van dat orgel heropent loadOrgan/loadFromFolder
    // zelf (per orgel onthouden — zie Console.restorePanels).
    try {
      const sess = loadSession();
      if (autoLoadLastOrgan && sess?.lastOrgan?.path && !organInfo) {
        if (sess.lastOrgan.kind === 'folder') await loadFromFolder(sess.lastOrgan.path);
        else await loadOrgan(sess.lastOrgan.path);
      }
    } catch (e) { /* sessie-herstel mag niet de app slopen */ }

    return () => {
      clearInterval(statusInterval);
      clearInterval(organInterval);
      clearInterval(midiRescanInterval);
      window.removeEventListener('keydown', handleKeyDown);
      window.removeEventListener('keyup', handleKeyUp);
      if (unlistenClose) try { unlistenClose(); } catch(e) {}
      if (unlistenProgress) try { unlistenProgress(); } catch(e) {}
      for (const un of panelUnlisteners) { try { un(); } catch(e) {} }
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
    selectedAudioDevice = pickDevice(audioDevices, localStorage.getItem('jm-orgue-audio-device'));
  }

  async function handleSelectAudioHost(host) {
    selectedAudioHost = host;
    await refreshDevicesForHost();
  }

  function handleSelectBuffer(frames) {
    selectedBufferFrames = frames;
  }

  // ===== Audio-uitvoerprofielen: snel wisselen speakers ↔ hoofdtelefoon =====
  // Twee opgeslagen host/apparaat/buffer-combinaties; één knop (of MIDI-actie)
  // wisselt ertussen via de bestaande applyAudioOutput-route (incl. herladen
  // van het orgel op de nieuwe audio-thread).
  // Sleutel + laden via lib/audioProfiles.js (gedeeld met de extra schermen).
  let audioProfiles = loadAudioProfiles();
  let audioProfileSwitching = false;
  // Dedup-cache van de organ-poll (component-scope: directe organInfo-updates
  // zetten hem op null zodat de eerstvolgende poll gegarandeerd verst; in de
  // onMount-closure kon een directe update permanent gemaskeerd blijven —
  // auditbevinding 40).
  let lastOrganInfoJson = null;
  let lastDrawnSignature = '';
  // Telt op bij elke geslaagde audio-herbouw; Console herlaadt dan de per-orgel
  // DSP op de verse audio-thread (auditbevinding 20).
  let audioEpoch = 0;

  function persistAudioProfiles() {
    localStorage.setItem(AUDIO_PROFILES_KEY, JSON.stringify(audioProfiles));
  }

  // Bewaar de HUIDIGE selectie (host/apparaat/buffer + kanaalroutering) als
  // profiel. De kanalen (per divisie, op naam) horen bij het profiel: zo kan
  // dezelfde ASIO-interface met speakers op eigen kanalen én de hoofdtelefoon
  // op een ander kanalenpaar gebruikt worden, en wisselt de kanaalkeuze mee.
  // `config` (host/device/bufferFrames) is optioneel: een extra scherm stuurt
  // zijn eigen selectie mee via het save-audio-profile-event — die mag de
  // selectie van dit hoofdvenster niet hoeven te delen.
  async function saveAudioProfile(kind, config = null) {
    let channels = null;
    try {
      if (organInfo?.divisions?.length) {
        const chans = await invoke('get_division_output_channels');
        channels = organInfo.divisions.map((d, i) => [d.name, Array.isArray(chans?.[i]) ? chans[i] : []]);
      }
    } catch (e) { /* geen orgel of backend-fout → profiel zonder kanalen */ }
    audioProfiles[kind] = {
      host: config ? (config.host ?? null) : selectedAudioHost,
      device: config ? (config.device ?? null) : selectedAudioDevice,
      bufferFrames: (config ? config.bufferFrames : selectedBufferFrames) || null,
      channels,
    };
    if (!audioProfiles.active) audioProfiles.active = kind;
    audioProfiles = audioProfiles;
    persistAudioProfiles();
    // Is dit profiel nu actief, dan is de zojuist vastgelegde routing meteen de
    // geldende override (zodat een orgel-load hem niet terugdraait).
    if (audioProfiles.active === kind) pushProfileChannelOverride(audioProfiles[kind]);
  }

  function clearAudioProfile(kind) {
    audioProfiles[kind] = null;
    if (audioProfiles.active === kind) {
      audioProfiles.active = null;
      pushProfileChannelOverride(null);
    }
    audioProfiles = audioProfiles;
    persistAudioProfiles();
  }

  // Kanaal-override van het actieve profiel naar de backend pushen (dedupe op
  // inhoud). null = geen override → per-orgel opgeslagen routing geldt weer.
  let lastPushedChannelOverride;
  async function pushProfileChannelOverride(profile) {
    const payload = profile?.channels ?? null;
    const key = JSON.stringify(payload);
    if (key === lastPushedChannelOverride) return;
    lastPushedChannelOverride = key;
    try { await invoke('set_profile_channel_override', { channels: payload }); } catch (e) {}
  }

  // Pas één profiel toe. Uitkomsten:
  //   'ok'             — wissel geslaagd;
  //   'unavailable'    — profiel-apparaat is er niet (bv. hoofdtelefoon uit);
  //                      de huidige uitgang is NIET aangeraakt, selectie terug;
  //   'failed-intact'  — wissel mislukt, de oude uitgang speelt nog gewoon;
  //   'failed-rebuilt' — wissel mislukt, maar de backend heeft zelf de vorige
  //                      (of de standaard-)uitgang herbouwd en het orgel is al
  //                      herladen — géén eigen revert meer doen.
  async function applyProfileConfig(p, label) {
    const snapshot = {
      host: selectedAudioHost,
      device: selectedAudioDevice,
      buffer: selectedBufferFrames,
    };
    selectedAudioHost = p.host || selectedAudioHost;
    await refreshDevicesForHost();
    if (p.device && !audioDevices.some(d => d.name === p.device)) {
      error = `${label}: apparaat "${p.device}" is nu niet beschikbaar (staat het aan / is het aangesloten?). De huidige uitgang blijft actief.`;
      // Huidige uitgang nooit aangeraakt — UI-selectie netjes terugzetten.
      selectedAudioHost = snapshot.host;
      await refreshDevicesForHost();
      selectedAudioDevice = snapshot.device;
      selectedBufferFrames = snapshot.buffer;
      return 'unavailable';
    }
    // p.device null = de standaard van die host (bewust géén oud apparaat erven).
    selectedAudioDevice = p.device || (audioDevices.find(d => d.is_default)?.name ?? selectedAudioDevice);
    selectedBufferFrames = p.bufferFrames || null;
    return await applyAudioOutput();
  }

  // Bepaal welk profiel bij de (echt spelende) uitgang hoort; null = geen.
  // Pure logica in lib/audioProfiles.js (gedeeld met de extra schermen).
  function deriveProfileFromOutput(host, device) {
    return deriveProfile(audioProfiles, host, device);
  }

  // Wissel naar `kind`, of zonder argument: naar het ándere profiel.
  // De backend is nu zelf faal-veilig: bij een mislukte wissel herstelt hij de
  // vorige uitgang (of desnoods de standaard) en zegt hij eerlijk wat er
  // draait. De frontend hoeft dus nooit meer zelf te reverten — alleen de
  // status gelijktrekken met wat er werkelijk speelt.
  async function switchAudioProfile(kind = null) {
    if (audioProfileSwitching) return;
    // Zonder expliciet doel: vanaf hoofdtelefoon → speakers; vanaf speakers →
    // hoofdtelefoon; vanaf een onbekende/handmatige uitgang (active=null) →
    // eerst terug naar speakers (de normale stand), anders hoofdtelefoon.
    const target = kind
      || (audioProfiles.active === 'headphones' ? 'speakers'
        : audioProfiles.active === 'speakers' ? 'headphones'
        : (audioProfiles.speakers ? 'speakers' : 'headphones'));
    const p = audioProfiles[target];
    if (!p) {
      // Geen doelprofiel: geen foutmelding en geen derde toestand meer (0.7.3).
      // Leg de HUIDIGE (echt spelende) uitgang vast als dit profiel en maak het
      // actief. De gebruiker verfijnt het daarna in Instellingen → Audio-uitvoer
      // naar een ander apparaat als de fysieke uitgang echt verschilt.
      await saveAudioProfile(target, {
        host: status.audioHost || selectedAudioHost || null,
        device: status.audioDevice || selectedAudioDevice || null,
        bufferFrames: selectedBufferFrames || null,
      });
      audioProfiles.active = target;
      audioProfiles = audioProfiles;
      persistAudioProfiles();
      await pushProfileChannelOverride(audioProfiles[target]);
      return;
    }
    audioProfileSwitching = true;
    // Extra schermen tonen de wissel-spinner via dit broadcast-event.
    emitProfileSwitchState(true);
    try {
      const label = target === 'headphones' ? 'Hoofdtelefoon' : 'Speakers';
      // Kanaal-override VÓÓR de wissel zetten: de wissel herlaadt het orgel en
      // elke apply_saved_output_channels tijdens die load moet de kanalen van
      // het DOELprofiel al meenemen (anders wint de per-orgel routing even).
      await pushProfileChannelOverride(p);
      const outcome = await applyProfileConfig(p, label);
      if (outcome === 'ok') {
        audioProfiles.active = target;
        audioProfiles = audioProfiles;
        persistAudioProfiles();
        // Gratieperiode voor de status-afleiding: get_status kan nog even de
        // oude uitgang tonen; niet direct terug-deriven.
        profileDeriveGraceUntil = Date.now() + 5000;
        return;
      }
      if (outcome === 'unavailable') {
        // Uitgang onaangeraakt; alleen de melding tonen — geen revert nodig,
        // wel de kanaal-override terug naar het profiel dat echt actief is.
        await pushProfileChannelOverride(audioProfiles.active ? audioProfiles[audioProfiles.active] : null);
        return;
      }
      if (outcome === 'busy') {
        // Al een wissel bezig of deadline bereikt: huidige uitgang blijft. Geen
        // revert, geen herlaad; de finally reset de bezig-vlag zodat de knop
        // vrijkomt (voorheen bleef die hangen bij een traag/hangend scenario).
        await pushProfileChannelOverride(audioProfiles.active ? audioProfiles[audioProfiles.active] : null);
        return;
      }
      // Mislukt ('failed-intact' of 'failed-rebuilt'): de backend heeft de
      // uitgang intact gelaten of zelf hersteld. Alleen de UI gelijktrekken
      // met wat er werkelijk speelt; de foutmelding staat al in `error`.
      await refreshStatus();
      const actual = deriveProfileFromOutput(status.audioHost, status.audioDevice);
      if (actual !== audioProfiles.active) {
        audioProfiles.active = actual;
        audioProfiles = audioProfiles;
        persistAudioProfiles();
      }
      await pushProfileChannelOverride(audioProfiles.active ? audioProfiles[audioProfiles.active] : null);
      // Instellingen-selectie terug naar de echte uitgang.
      if (status.audioHost) {
        selectedAudioHost = status.audioHost;
        await refreshDevicesForHost();
        if (status.audioDevice) selectedAudioDevice = status.audioDevice;
      }
    } finally {
      audioProfileSwitching = false;
      emitProfileSwitchState(false);
    }
  }

  // Broadcast de wissel-status naar alle vensters (extra schermen tonen de
  // spinner + het actieve profiel in hun eigen hoofdbalk).
  async function emitProfileSwitchState(switching) {
    try {
      const { emit } = await import('@tauri-apps/api/event');
      await emit('jm-orgue:profile-switch-state', { switching, active: audioProfiles.active });
    } catch (e) { /* niet in Tauri context */ }
  }

  // Apply the chosen host/device/buffer and reload the current organ so its
  // samples re-register on the new audio thread and DSP settings re-apply.
  // De backend geeft een eerlijke uitkomst terug: `switched` (gevraagde config
  // draait echt), `player_rebuilt` (nieuwe audio-thread → orgel herladen, ook
  // wanneer de wissel zelf mislukte maar de vorige uitgang is herbouwd) en
  // `message` (Nederlandse uitleg bij falen).
  async function applyAudioOutput() {
    try {
      const res = await invoke('set_audio_output', {
        host: selectedAudioHost,
        device: selectedAudioDevice,
        bufferFrames: selectedBufferFrames || null,
      });
      if (res.player_rebuilt) audioEpoch += 1;
      if (res.player_rebuilt && res.organ_id) {
        // Orgel-definitiebestanden (GrandOrgue .organ én Hauptwerk
        // .Organ_Hauptwerk_xml) via loadOrgan; anders is het een sample-map.
        const idLower = res.organ_id.toLowerCase();
        if (idLower.endsWith('.organ') || idLower.endsWith('.organ_hauptwerk_xml')) {
          await loadOrgan(res.organ_id);
        } else {
          await loadFromFolder(res.organ_id);
        }
      }
      if (res.busy) {
        // Er liep al een wissel of de harde deadline werd bereikt: huidige
        // uitgang blijft intact, niets herladen. De aanroeper reset zijn
        // bezig-vlag zodat de knop niet eeuwig blijft hangen.
        error = res.message || 'Audio-wissel loopt langer dan verwacht — nog even geduld.';
        return 'busy';
      }
      if (res.switched) {
        if (selectedAudioHost) localStorage.setItem('jm-orgue-audio-host', selectedAudioHost);
        if (selectedAudioDevice) localStorage.setItem('jm-orgue-audio-device', selectedAudioDevice);
        if (selectedBufferFrames) localStorage.setItem('jm-orgue-audio-buffer', String(selectedBufferFrames));
        else localStorage.removeItem('jm-orgue-audio-buffer');
        return 'ok';
      }
      error = res.message || 'Audio-wissel mislukt.';
      return res.player_rebuilt ? 'failed-rebuilt' : 'failed-intact';
    } catch (e) {
      // Catastrofaal (geen fallback meer beschikbaar): backend-melding tonen.
      error = e.toString();
      return 'failed-intact';
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
        sampleRate: s.sample_rate,
        audioHost: s.audio_host,
        audioDevice: s.audio_device
      };
    } catch (e) {
      // Ignore polling errors
    }
  }

  // Houd het getoonde actieve profiel in lijn met de ÉCHTE actuele uitgang.
  // Na een app-herstart (de backend start op de opgeslagen voorkeur) of een
  // handmatige wijziging in Instellingen zou de wisselknop anders een oude
  // stand tonen — en dan lijkt "geen geluid op de speakers" een kapotte
  // schakeling terwijl gewoon het hoofdtelefoon-profiel actief is.
  // (profileMatchesOutput komt uit lib/audioProfiles.js.)
  // Direct na een geslaagde wissel kan get_status kortstondig nog de oude
  // uitgang rapporteren; binnen deze gratieperiode niet terug-deriven.
  let profileDeriveGraceUntil = 0;
  $: {
    const host = status.audioHost;
    const device = status.audioDevice;
    if (host && !audioProfileSwitching && Date.now() >= profileDeriveGraceUntil) {
      const derived = deriveProfileFromOutput(host, device);
      if (derived !== audioProfiles.active) {
        // Alleen weergave-sync — bewust NIET persist'en: een tijdelijke
        // watchdog-herbouw of noodherstel mag de opgeslagen profielkeuze
        // van de gebruiker niet overschrijven.
        audioProfiles.active = derived;
        audioProfiles = audioProfiles;
        // De kanaal-override hoort bij het profiel dat ECHT speelt (ook na
        // een handmatige wissel in Instellingen: derived=null → override weg).
        pushProfileChannelOverride(derived ? audioProfiles[derived] : null);
      }
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
  // met de registers van het vorige orgel. Loopt via Console.closeAllPanels,
  // die de per-orgel-lijst "open schermen" intact laat (guard) zodat de vensters
  // bij het volgende laden van dit orgel weer terugkomen.
  async function closeExtraPanels() {
    if (consoleComponent && consoleComponent.closeAllPanels) {
      try { await consoleComponent.closeAllPanels(); return; } catch (e) {}
    }
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
      lastOrganInfoJson = null;
      showOrganBrowser = false;
      await restorePresetsToLocalStorage();
      try { await invoke('apply_saved_voicings'); } catch (e) {}
      try { await invoke('apply_saved_output_channels'); } catch (e) {}
      // De backend herstelt de per-orgel MIDI-koppelingen (klavier-inleren)
      // tijdens de load; zonder deze refresh bleef de UI de lijst van vóór de
      // load tonen ("leeg") en leerde de gebruiker onnodig elke keer opnieuw in.
      try { await refreshMidiMappings(); } catch (e) {}
      saveSession({ lastOrgan: { kind: 'organ', path } });
      // Extra registerschermen die bij dít orgel open stonden heropenen
      // (met hun eigen divisiekeuze en vensterpositie).
      try { await tick(); await consoleComponent?.restorePanels?.(); } catch (e) {}
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
      lastOrganInfoJson = null;
      showOrganBrowser = false;
      await restorePresetsToLocalStorage();
      try { await invoke('apply_saved_voicings'); } catch (e) {}
      try { await invoke('apply_saved_output_channels'); } catch (e) {}
      // Zie loadOrgan: herstelde MIDI-koppelingen ook in de UI tonen.
      try { await refreshMidiMappings(); } catch (e) {}
      saveSession({ lastOrgan: { kind: 'folder', path: directory } });
      // Extra registerschermen die bij dít orgel open stonden heropenen.
      try { await tick(); await consoleComponent?.restorePanels?.(); } catch (e) {}
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
        const storageKey = `jm-orgue-presets-${hash}-m1`;
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
      // Crescendostappen kunnen ook koppels bevatten ("coupler_"-prefix).
      const stops = stopIds.filter(id => !id.startsWith('coupler_'));
      const couplers = stopIds.filter(id => id.startsWith('coupler_'));
      await invoke('set_drawn_stops', { stopIds: stops });
      await invoke('set_active_couplers', { couplerIds: couplers });
      organInfo = await invoke('get_organ_info');
      lastOrganInfoJson = null;
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
    // Extra schermen mee sluiten — de guard houdt de bewaarde per-orgel-lijst
    // intact, dus bij de volgende load van dit orgel komen ze gewoon terug.
    // Zonder dit bleven panelen het gesloten orgel tonen (ook wanneer "orgel
    // sluiten" vanuit een paneel-hoofdbalk wordt aangevraagd).
    await closeExtraPanels();
    organInfo = null;
    showOrganBrowser = true;
  }

  // Autostart bij Windows + computer afsluiten (#13)
  let autostartEnabled = false;
  if (!isPanel && !isNotation) invoke('get_autostart_enabled').then(v => { autostartEnabled = !!v; }).catch(() => {});

  // Laatste registratie herstellen bij openen (default uit = schone start).
  let restoreRegistration = localStorage.getItem('jm-orgue-restore-registration') === 'true';
  // Push de opgeslagen voorkeur direct naar de backend zodat het herstel klopt.
  // Alleen vanuit het hoofdvenster (paneel-vensters zouden dubbel pushen).
  if (!isPanel && !isNotation) invoke('set_restore_registration', { enabled: restoreRegistration }).catch(() => {});
  async function setRestoreRegistration(enabled) {
    restoreRegistration = enabled;
    localStorage.setItem('jm-orgue-restore-registration', String(enabled));
    try { await invoke('set_restore_registration', { enabled }); } catch (e) {}
  }

  // Laatst geopende orgel automatisch laden bij het starten van de app
  // (default aan — het gedrag van vorige versies). Staat LOS van
  // restoreRegistration: dit gaat over het ORGEL, dat over de registers.
  let autoLoadLastOrgan = localStorage.getItem('jm-orgue-autoload-organ') !== 'false';
  function setAutoLoadLastOrgan(enabled) {
    autoLoadLastOrgan = enabled;
    localStorage.setItem('jm-orgue-autoload-organ', String(enabled));
  }

  async function setAutostart(enabled) {
    try {
      await invoke('set_autostart_enabled', { enabled });
      autostartEnabled = enabled;
    } catch (e) {
      error = `Autostart wijzigen mislukt: ${e}`;
    }
  }

  // Opslaan en computer netjes afsluiten. De bevestigingsvraag zit bij de
  // aanroeper: requestShutdown (hoofdvenster) of het extra registerscherm
  // (dat stuurt na bevestiging het 'jm-orgue:shutdown'-event).
  async function doShutdown() {
    try {
      await flushAutoSave();
      await persistOrganSettings();
      await closeExtraPanels();
    } catch (e) {}
    try { await invoke('shutdown_computer'); }
    catch (e) { error = `Afsluiten mislukt: ${e}`; }
  }

  // Bevestiging vragen, opslaan, computer netjes afsluiten.
  async function requestShutdown() {
    const ok = window.confirm('Software én computer afsluiten?\n\nDe laatste stand wordt opgeslagen.');
    if (!ok) return;
    await doShutdown();
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
      // Zelfde sleutel als SetzerBar (geheugenniveau 1 = standaard): de oude
      // suffix-loze sleutel bestond niet meer, waardoor setzer-presets nooit in
      // de bibliotheek belandden (auditbevinding 26).
      const storageKey = `jm-orgue-presets-${hash}-m1`;
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
      // Direct persistent maken: geleerde/gezette MIDI-koppelingen werden
      // voorheen alleen bij een nette afsluiting of orgelwissel opgeslagen —
      // een geforceerde afsluiting (of de 2,5s-close-timeout) gooide ze weg
      // en de klavieren moesten elke sessie opnieuw ingeleerd worden.
      scheduleAutoSave();
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
        // Direct persistent maken (zie setMidiMapping).
        scheduleAutoSave();
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
        // Direct persistent maken (zie setMidiMapping).
        scheduleAutoSave();
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

{#if isNotation}
  <!-- Notatievenster: MIDI-opname als notenschrift (bladmuziek). -->
  <NotationWindow />
{:else if isPanel}
  <!-- Extra registerscherm: secundaire App-shell (Header optioneel + volledige
       Console in secondary-modus + StatusBar) — zie PanelApp.svelte. -->
  <PanelApp />
{:else}
<div id="app">
  <Header
    organName={organInfo?.name}
    organLoaded={!showOrganBrowser && organInfo}
    {status}
    {activeView}
    {audioProfiles}
    {audioProfileSwitching}
    on:startAudio={startAudio}
    on:stopAudio={stopAudio}
    on:closeOrgan={closeOrgan}
    on:setView={(e) => activeView = e.detail}
    on:toggleAudioProfile={() => switchAudioProfile()}
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
      {autoLoadLastOrgan}
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
      on:refreshMidiMappings={() => { refreshMidiMappings(); scheduleAutoSave(); }}
      on:learnMidiChannel={(e) => learnMidiChannel(e.detail)}
      on:learnKeyboardRange={(e) => learnKeyboardRange(e.detail.division, e.detail.firstSampleNote)}
      on:selectAudioDevice={(e) => handleSelectAudioDevice(e.detail)}
      on:selectAudioHost={(e) => handleSelectAudioHost(e.detail)}
      on:selectBuffer={(e) => handleSelectBuffer(e.detail)}
      on:applyAudioOutput={applyAudioOutput}
      {audioProfiles}
      {audioEpoch}
      on:saveAudioProfile={(e) => saveAudioProfile(e.detail)}
      on:clearAudioProfile={(e) => clearAudioProfile(e.detail)}
      on:switchAudioProfile={(e) => switchAudioProfile(e.detail)}
      on:connectMidi={(e) => connectMidi(e.detail)}
      on:volumeChange={(e) => handleVolumeChange(e.detail)}
      on:reverbChange={(e) => handleReverbChange(e.detail)}
      on:shutdownRequest={requestShutdown}
      on:shutdownLearn={learnShutdownAction}
      on:setAutostart={(e) => setAutostart(e.detail)}
      on:setRestoreRegistration={(e) => setRestoreRegistration(e.detail)}
      on:setAutoLoadLastOrgan={(e) => setAutoLoadLastOrgan(e.detail)}
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
