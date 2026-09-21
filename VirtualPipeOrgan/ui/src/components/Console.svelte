<script>
  import { createEventDispatcher, onMount, onDestroy, tick } from 'svelte';
  import { open } from '@tauri-apps/plugin-dialog';
  import LayoutSettings from './LayoutSettings.svelte';
  import VoicingPanel from './VoicingPanel.svelte';
  import MidiPlayer from './MidiPlayer.svelte';
  import { t, tx, locale, setLocale, AVAILABLE_LOCALES, LOCALE_LABELS } from '../lib/i18n.js';
  import SetzerBar from './SetzerBar.svelte';
  import { loadPanelState, savePanelState } from '../lib/panelState.js';
  import { midiLearn } from '../lib/midiLearn.js';
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';

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
    if (!(await confirm(tx('settings.sampleset_confirm')))) return;
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
    if (!(await confirm(tx('sampleset.loops_confirm')))) return;
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
  // Actuele stream (uit de 100-ms status-poll in App/PanelApp): kanaalaantal,
  // host, apparaat, buffer. Eén bron voor chips, EQ-kanaalkeuze en uitgangspaar
  // — vóór 0.7.37 werd het kanaalaantal één keer bij het aanmaken opgevraagd en
  // nooit ververst, zodat na de (uitgestelde) ASIO-wissel 2 chips bleven staan.
  export let audioChannels = 0;
  export let audioHostActual = '';
  export let audioDeviceActual = '';
  export let audioBufferFrames = 0;
  /// Gevraagde samplerate; null = de standaard van het apparaat.
  export let selectedSampleRate = null;
  // Telt op wanneer een ander venster de kanaalkeuze per klavier wijzigde.
  export let divisionChannelsVersion = 0;
  // Laatst geopende orgel automatisch laden bij het starten (App.svelte).
  export let autoLoadLastOrgan = true;
  // Kruisje op een extra scherm sluit de hele software (App.svelte; standaard
  // uit — een tester verwachtte dat alleen dát venster dichtgaat).
  export let panelCloseQuits = false;

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
  $: audioStatusKey = `${audioHostActual}|${audioDeviceActual}|${audioChannels}|${audioBufferFrames}|${audioEpoch}`;
  $: if (activeView === 'algemene-instellingen' && audioStatusKey) refreshAudioStatus();

  // ===== MIDI-uit terugkoppeling (0.7.9): registerlampen/display op de console =====
  // Protocol + poort + kanaal komen uit localStorage; bij een registratiewijziging
  // stuurt fbSendActive() de actieve id's naar de backend (die diff't en batcht).
  let fbProtocol = localStorage.getItem('jm-orgue-fb-protocol') || 'off';
  let fbPort = localStorage.getItem('jm-orgue-fb-port') || '';
  let fbChannel = Number(localStorage.getItem('jm-orgue-fb-channel')) || 1;      // 1..16
  let fbBaseNote = Number(localStorage.getItem('jm-orgue-fb-basenote')) || 36;   // NoteOnOff auto-toewijzing
  let fbOutputs = [];
  let fbConfigured = false;
  let fbLastSent = null;   // signature van laatst-verstuurde actieve set (dedup)
  let fbBusy = false;

  async function fbRefreshOutputs() {
    try { fbOutputs = await invoke('feedback_list_outputs'); } catch (e) { fbOutputs = []; }
  }
  // Per-register ingeleerde nootnummers (console-tabs volgen zelden een nette
  // reeks). Per orgel bewaard; een ingeleerde noot wint van de auto-toewijzing.
  let fbLearnedNotes = {};   // registerId -> nootnummer
  let fbLearningId = null;   // register dat nú inleert (wacht op consoleknop)
  let fbLearnAll = false;    // sequentiële "alles inleren"-loop actief
  let fbShowLearnList = false;
  function fbLoadLearned() {
    try { fbLearnedNotes = JSON.parse(localStorage.getItem(organUiKey('jm-orgue-fb-notes')) || '{}') || {}; }
    catch (e) { fbLearnedNotes = {}; }
  }
  function fbSaveLearned() {
    try { localStorage.setItem(organUiKey('jm-orgue-fb-notes'), JSON.stringify(fbLearnedNotes)); } catch (e) {}
  }
  // Alle registers + koppels in stabiele volgorde; ingeleerde noot wint van
  // basisnoot + index (zelfde volgorde als fbRegList hieronder).
  function fbBuildSlots() {
    const slots = [];
    let i = 0;
    for (const d of (organInfo?.divisions || [])) {
      for (const s of (d.stops || [])) { slots.push({ id: s.id, note: fbLearnedNotes[s.id] ?? ((fbBaseNote + i) & 0x7F) }); i++; }
    }
    for (const c of (organInfo?.couplers || [])) { slots.push({ id: c.id, note: fbLearnedNotes[c.id] ?? ((fbBaseNote + i) & 0x7F) }); i++; }
    return slots;
  }
  // Lijst voor de inleer-UI. Als $:-statement met expliciete afhankelijkheden —
  // een kale helper-aanroep in de markup zou updates missen (zelfde Svelte-
  // valkuil als de zwel-lampjes, zie 0.7.12).
  $: fbRegList = (() => {
    const list = [];
    let i = 0;
    for (const d of (organInfo?.divisions || [])) {
      for (const s of (d.stops || [])) {
        list.push({ id: s.id, name: `${d.name} — ${s.name} ${s.pitch || ''}`.trim(), auto: (fbBaseNote + i) & 0x7F, learned: fbLearnedNotes[s.id] });
        i++;
      }
    }
    for (const c of (organInfo?.couplers || [])) {
      list.push({ id: c.id, name: $t('feedback.coupler_prefix').replace('{name}', (c.name || '').replace(/\n/g, ' ')), auto: (fbBaseNote + i) & 0x7F, learned: fbLearnedNotes[c.id] });
      i++;
    }
    return list;
  })();
  // Eén register inleren: wacht op de eerstvolgende NoteOn van de console
  // (hergebruikt learn_keyboard_note, 20s timeout). De eerste inleer neemt ook
  // het kanaal van de console over.
  async function fbLearnOne(id) {
    if (fbLearningId) return false;
    if (!midiConnected) { alert(tx('midi.connect_first')); return false; }
    fbLearningId = id;
    try {
      const res = await invoke('learn_keyboard_note');
      if (res) {
        const [ch, note] = res;
        fbLearnedNotes[id] = note;
        fbLearnedNotes = fbLearnedNotes;
        fbChannel = ch + 1; // kanaal van de console overnemen
        fbSaveLearned();
        await fbApplyConfig(true);
        return true;
      }
      return false; // time-out: niets ontvangen
    } catch (e) { return false; }
    finally { fbLearningId = null; }
  }
  // Alles inleren, op volgorde van de lijst; nogmaals klikken stopt. Stopt ook
  // vanzelf bij een time-out (geen knop ingedrukt binnen 20s).
  async function fbLearnAllSeq() {
    if (fbLearnAll) { fbLearnAll = false; return; }
    fbLearnAll = true;
    for (const r of fbRegList) {
      if (!fbLearnAll) break;
      const ok = await fbLearnOne(r.id);
      if (!ok) break;
    }
    fbLearnAll = false;
  }
  function fbClearLearned() {
    fbLearnedNotes = {};
    fbSaveLearned();
    fbApplyConfig(true);
  }
  function fbActiveIds() {
    const ids = [];
    for (const d of (organInfo?.divisions || [])) for (const s of (d.stops || [])) if (s.drawn) ids.push(s.id);
    for (const c of (organInfo?.couplers || [])) if (c.active) ids.push(c.id);
    return ids;
  }
  // `silent`: de automatische paden (opstart/orgelwissel) mogen geen alert-popup
  // opwerpen wanneer de opgeslagen poort even niet bestaat (console niet
  // aangesloten) — stil degraderen; alleen handmatig "Toepassen" meldt hardop.
  async function fbApplyConfig(silent = false) {
    fbBusy = true;
    localStorage.setItem('jm-orgue-fb-protocol', fbProtocol);
    localStorage.setItem('jm-orgue-fb-port', fbPort);
    localStorage.setItem('jm-orgue-fb-channel', String(fbChannel));
    localStorage.setItem('jm-orgue-fb-basenote', String(fbBaseNote));
    try {
      await invoke('feedback_configure', {
        protocol: fbProtocol,
        port: fbProtocol === 'off' ? null : (fbPort || null),
        channel: Math.max(0, Math.min(15, (Number(fbChannel) || 1) - 1)),
        slots: fbBuildSlots(),
        organName: organInfo?.name || '',
      });
      fbConfigured = fbProtocol !== 'off' && !!fbPort;
      fbLastSent = null;          // forceer een volledige (re)sync
      await fbSendActive();
    } catch (e) {
      fbConfigured = false;
      if (silent === true) console.warn('Terugkoppeling auto-verbinden mislukt:', e);
      else alert(tx('feedback.configure_failed').replace('{error}', String(e)));
    }
    finally { fbBusy = false; }
  }
  async function fbAllOff() {
    try { await invoke('feedback_all_off'); fbLastSent = null; } catch (e) {}
  }
  async function fbSendActive() {
    if (!fbConfigured) return;
    const ids = fbActiveIds();
    const sig = ids.join(',');
    if (sig === fbLastSent) return;
    fbLastSent = sig;
    try { await invoke('feedback_apply', { activeIds: ids }); } catch (e) {}
  }
  // Registratie gewijzigd (toggle/preset/tutti/crescendo/orgel-load) → console
  // meelichten. organInfo wordt door App bij elke wijziging vervangen; de
  // signature-dedup in fbSendActive voorkomt onnodige sends bij ongewijzigde poll.
  // Alleen het HOOFDVENSTER stuurt (niet elk extra scherm — de backend is één
  // gedeelde FeedbackManager; panelen zouden alles dubbel sturen).
  $: if (!secondary && organInfo && fbConfigured) { fbSendActive(); }
  // Auto-verbinden bij opstart én HER-configureren bij orgelwissel: de slot-lijst
  // (register-id's → nootnummers) en de orgelnaam (LCD) horen bij het geladen
  // orgel; zonder resync zou een nieuw orgel geen lampen krijgen totdat de
  // gebruiker handmatig "Toepassen" klikt.
  let fbConfiguredOrganId = null;
  $: if (!secondary && fbProtocol !== 'off' && fbPort && organInfo
        && organInfo.id !== fbConfiguredOrganId) {
    fbConfiguredOrganId = organInfo.id;
    fbLoadLearned();
    fbRefreshOutputs();
    fbApplyConfig(true);
  }
  // Ingeleerde register-noten horen bij het orgel — ook laden wanneer de
  // terugkoppeling (nog) niet actief is, zodat de inleer-lijst klopt.
  let fbNotesOrganId = null;
  $: if (organInfo && organInfo.id !== fbNotesOrganId) {
    fbNotesOrganId = organInfo.id;
    fbLoadLearned();
  }
  // Poortlijst verversen wanneer de Algemene Instellingen open gaan.
  $: if (activeView === 'algemene-instellingen') fbRefreshOutputs();

  // ===== Over & feedback (GitHub) =====
  let appVersion = '';
  let manualUpdateResult = null; // null | 'checking' | 'uptodate' | 'failed' | { version, url }
  async function loadAppVersion() {
    try {
      const { getVersion } = await import('@tauri-apps/api/app');
      appVersion = await getVersion();
    } catch (e) {}
  }
  async function openFeedbackPage() {
    try {
      const { githubIssuesUrl } = await import('../lib/github.js');
      await invoke('open_external_url', { url: githubIssuesUrl() });
    } catch (e) { alert(tx('about.feedback_page_failed').replace('{error}', String(e))); }
  }

  // Feedback-popup (0.7.23): bericht gaat via een mail-relay (FEEDBACK_ENDPOINT
  // in lib/github.js) rechtstreeks als e-mail naar de maker — zonder dat diens
  // adres in de app staat. Zonder endpoint valt de popup terug op GitHub-issues.
  let fbkOpen = false;
  let fbkMessage = '';
  let fbkEmail = '';
  let fbkIncludeLog = true; // logstaart (huidige + vorige sessie) meesturen
  let fbkStatus = null; // null | 'sending' | 'ok' | 'error' | 'noendpoint'
  async function fbkSubmit() {
    if (!fbkMessage.trim() || fbkStatus === 'sending') return;
    fbkStatus = 'sending';
    try {
      const { FEEDBACK_ENDPOINT } = await import('../lib/github.js');
      if (!FEEDBACK_ENDPOINT) { fbkStatus = 'noendpoint'; return; }
      if (!appVersion) await loadAppVersion();
      // Logstaart alleen ophalen als de gebruiker het vinkje aan laat; fouten
      // (bv. geen logbestand) mogen het versturen niet blokkeren.
      let logTail = null;
      if (fbkIncludeLog) {
        try { logTail = await invoke('get_log_tail', { maxKb: 24 }); } catch (e) { logTail = `(log ophalen mislukt: ${e})`; }
      }
      const res = await fetch(FEEDBACK_ENDPOINT, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', Accept: 'application/json' },
        body: JSON.stringify({
          bericht: fbkMessage,
          afzender: fbkEmail || '(geen afzender opgegeven)',
          versie: appVersion || '?',
          orgel: organInfo?.name || '-',
          ...(logTail != null ? { log: logTail } : {}),
        }),
      });
      fbkStatus = res.ok ? 'ok' : 'error';
      if (res.ok) fbkMessage = '';
    } catch (e) { fbkStatus = 'error'; }
  }
  // Handmatige update-controle. Samen met de controle bij het starten van de
  // app zijn dit de ENIGE twee momenten waarop er gekeken wordt — bewust geen
  // periodieke controle (een melding die tijdens een dienst in beeld ploft).
  // Is de update automatisch te installeren, dan handelt App.svelte dat af
  // (balk met voortgang); hier staat alleen de knop.
  let manualUpdateMac = false;
  async function manualUpdateCheck() {
    manualUpdateResult = 'checking';
    try {
      const { zoekUpdate, autoUpdateOndersteund } = await import('../lib/updater.js');
      if (!appVersion) await loadAppVersion();
      manualUpdateMac = !autoUpdateOndersteund();
      const upd = await zoekUpdate(appVersion);
      manualUpdateResult = upd || 'uptodate';
    } catch (e) {
      // Geen internet, firewall of GitHub-rate-limit: dat is géén "je bent
      // bij". Eigen melding, en de knop blijft actief voor een nieuwe poging.
      console.warn('Handmatige update-controle mislukt:', e);
      manualUpdateResult = 'failed';
    }
  }
  async function openManualUpdate() {
    if (manualUpdateResult && manualUpdateResult.url) {
      try { await invoke('open_external_url', { url: manualUpdateResult.url }); } catch (e) {}
    }
  }
  function startManualUpdate() {
    if (manualUpdateResult && manualUpdateResult.update) dispatch('startUpdate', manualUpdateResult);
  }
  $: if (activeView === 'algemene-instellingen' && !appVersion) loadAppVersion();

  // Geschatte uitgangslatentie: ~2× de bufferduur (dubbele buffering in de
  // driver). 0 frames = driver-default (onbekend, WASAPI ≈ 10 ms per buffer).
  $: audioLatencyMs = (audioStatus && audioStatus.buffer_frames > 0 && audioStatus.sample_rate > 0)
    ? (2 * audioStatus.buffer_frames / audioStatus.sample_rate * 1000)
    : null;

  // Begrijpelijker label voor de host-keuze (waarde blijft de echte cpal-naam).
  // Tweede parameter `_t` wordt in het template als $t meegegeven zodat de
  // keuzelijst bij een taalwissel opnieuw rendert (anders blijft het oude label).
  function prettyHost(name, _t = $t) {
    const n = (name || '').toLowerCase();
    if (/asio/.test(n)) return _t('audio.host_low_latency').replace('{name}', name);
    if (/wasapi/.test(n)) return _t('audio.host_wasapi').replace('{name}', name);
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
      setCrescendoEnabledUI(!crescendoEnabled);
    } else if (actionCode === ACTION_AUDIO_PROFILE) {
      // Snelle wissel speakers ↔ hoofdtelefoon via MIDI (bv. een piston).
      dispatch('switchAudioProfile', null);
    } else if (actionCode === 42) {
      // Computer afsluiten — bubbel naar App.svelte zodat die bevestiging + opslag regelt
      dispatch('shutdownRequest');
    }
  }

  // ===== Consoleknoppen (pistons): zichtbare inleer-lijst (0.7.19) =====
  // Deze acties waren alleen inleerbaar via rechtermuis/3s op de setzerbalk
  // (en "Computer afsluiten" helemaal nergens). Eén vindbare lijst in de
  // Algemene Instellingen; hergebruikt learn_preset_binding/clear_preset_binding.
  // Reactief ($t) zodat de labels de taalkeuze volgen; de markup leest `.name`.
  $: GLOBAL_ACTIONS = [
    { code: 10, name: $t('pistons.set') },
    { code: 11, name: $t('pistons.general_cancel') },
    { code: 12, name: $t('pistons.preset_prev') },
    { code: 13, name: $t('pistons.preset_next') },
    { code: 14, name: $t('pistons.preset_minus_10') },
    { code: 15, name: $t('pistons.preset_plus_10') },
    { code: 16, name: $t('pistons.memory_level').replace('{n}', '1') },
    { code: 17, name: $t('pistons.memory_level').replace('{n}', '2') },
    { code: 18, name: $t('pistons.memory_level').replace('{n}', '3') },
    { code: 19, name: $t('pistons.memory_level').replace('{n}', '4') },
    { code: 20, name: $t('pistons.memory_level').replace('{n}', '5') },
    { code: 21, name: $t('pistons.memory_level').replace('{n}', '6') },
    { code: 22, name: $t('pistons.memory_level').replace('{n}', '7') },
    { code: 23, name: $t('pistons.memory_level').replace('{n}', '8') },
    { code: 40, name: $t('pistons.eq_toggle') },
    { code: 41, name: $t('pistons.crescendo_toggle') },
    { code: 42, name: $t('pistons.shutdown') },
    { code: 43, name: $t('pistons.audio_profile') },
  ];
  let showActionLearnList = false;
  $: actionBound = (() => {
    const m = {};
    for (const b of midiBindingsFull) m[b.preset_num] = (m[b.preset_num] || 0) + 1;
    return m;
  })();
  async function clearGlobalAction(actionCode) {
    try {
      await invoke('clear_preset_binding', { presetNum: actionCode });
      await refreshMidiBindingsFull();
    } catch (e) {}
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
          alert(tx('recording.audio_saved').replace('{path}', final.path).replace('{time}', formatSeconds(final.seconds)));
        }
      } else {
        const path = await invoke('start_recording', { path: null });
        recorderStatus = { recording: true, seconds: 0, path, frames_dropped: 0, error: null };
      }
    } catch (e) {
      alert(tx('recording.audio_error').replace('{error}', String(e)));
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
          alert(tx('recording.midi_no_notes'));
          return;
        }
        // Opslagdialoog, standaard in Documenten\JM-Orgue-opnames met tijdgestempelde naam.
        const suggested = await invoke('suggest_midi_recording_path');
        const { save } = await import('@tauri-apps/plugin-dialog');
        const path = await save({
          defaultPath: suggested,
          filters: [{ name: tx('recording.midi_file_filter'), extensions: ['mid'] }],
        });
        if (path) {
          await invoke('save_midi_recording', { path });
          lastMidiPath = path;
          alert(tx('recording.midi_saved').replace('{path}', path).replace('{n}', String(count)));
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
      alert(tx('recording.midi_error').replace('{error}', String(e)));
    }
  }

  // Zojuist opgenomen take direct in de MIDI-speler laden en afspelen.
  // Let op: trek zelf de registers, anders blijft het orgel stil (opname = alleen noten).
  async function playLastMidi() {
    if (!lastMidiPath) return;
    try {
      await invoke('midi_play_file', { path: lastMidiPath });
    } catch (e) {
      alert(tx('recording.play_failed').replace('{error}', String(e)));
    }
  }

  // ---- Automatisch MIDI-archief (0.7.38) ----
  // Achtergrond-recorder in de backend (midi_archive.rs): start bij de eerste
  // noot, stopt na stilte, schrijft .mid in de archiefmap. Hier alleen de
  // instellingen, de live status en de lijst met de nieuwste bestanden.
  let archiveCfg = { enabled: false, dir: '', dir_is_default: true, silence_secs: 20, min_notes: 4, min_secs: 5 };
  let archiveStatus = { enabled: false, archiving: false, event_count: 0, seconds: 0, files_written: 0, last_file: null, last_error: null };
  let archiveFiles = [];
  let archiveFilesStamp = -1;
  let archivePoll = null;

  async function loadArchiveConfig() {
    try { archiveCfg = await invoke('get_midi_archive_config'); } catch (e) {}
  }
  async function saveArchiveConfig() {
    try {
      archiveCfg = await invoke('set_midi_archive_config', { config: {
        enabled: !!archiveCfg.enabled,
        // Standaardmap niet als eigen keuze vastleggen: leeg = standaard.
        dir: archiveCfg.dir_is_default ? '' : (archiveCfg.dir || ''),
        dir_is_default: !!archiveCfg.dir_is_default,
        silence_secs: Number(archiveCfg.silence_secs) || 0,
        min_notes: Number(archiveCfg.min_notes) || 0,
        min_secs: Number(archiveCfg.min_secs) || 0,
      } });
      await refreshArchiveList();
    } catch (e) {
      alert(tx('midi_archive.error').replace('{error}', String(e)));
      await loadArchiveConfig();
    }
  }
  async function refreshArchiveList() {
    try { archiveFiles = await invoke('midi_archive_list'); } catch (e) { archiveFiles = []; }
  }
  async function pollArchive() {
    try {
      const s = await invoke('midi_archive_status');
      archiveStatus = s;
      if (s.files_written !== archiveFilesStamp) {
        archiveFilesStamp = s.files_written;
        await refreshArchiveList();
      }
    } catch (e) {}
  }
  async function chooseArchiveDir() {
    try {
      const d = await open({ directory: true, multiple: false, defaultPath: archiveCfg.dir || undefined });
      if (d) {
        archiveCfg.dir = d;
        archiveCfg.dir_is_default = false;
        await saveArchiveConfig();
      }
    } catch (e) {
      alert(tx('midi_archive.error').replace('{error}', String(e)));
    }
  }
  async function resetArchiveDir() {
    archiveCfg.dir = '';
    archiveCfg.dir_is_default = true;
    await saveArchiveConfig();
  }
  async function openArchiveDir() {
    try { await invoke('midi_archive_open_dir'); }
    catch (e) { alert(tx('midi_archive.error').replace('{error}', String(e))); }
  }
  // Bestaande speler; MidiPlayer.svelte toont de voortgang. Net als bij
  // playLastMidi: registers zelf trekken (archief = alleen noten).
  async function playArchiveFile(f) {
    try { await invoke('midi_play_file', { path: f.path }); }
    catch (e) { alert(tx('recording.play_failed').replace('{error}', String(e))); }
  }
  async function deleteArchiveFile(f) {
    if (!(await window.confirm(tx('midi_archive.delete_confirm').replace('{name}', f.name)))) return;
    try {
      await invoke('midi_archive_delete', { path: f.path });
      await refreshArchiveList();
    } catch (e) {
      alert(tx('midi_archive.error').replace('{error}', String(e)));
    }
  }
  function fmtEpoch(e) {
    try { return new Date(e * 1000).toLocaleString(); } catch (err) { return ''; }
  }
  // Status/lijst alleen pollen zolang de Algemene Instellingen zichtbaar zijn
  // (zelfde patroon als de BLE-teller); elders is het blok onzichtbaar.
  $: {
    if (activeView === 'algemene-instellingen' && !archivePoll) {
      loadArchiveConfig();
      refreshArchiveList();
      pollArchive();
      archivePoll = setInterval(pollArchive, 500);
    } else if (activeView !== 'algemene-instellingen' && archivePoll) {
      clearInterval(archivePoll);
      archivePoll = null;
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
        title: tx('notation.live_window_title'),
        width: 1100, height: 800, center: true, resizable: true,
      });
      webview.once('tauri://error', (e) => {
        console.error('Live-notatievenster openen mislukt:', e);
        alert(tx('notation.open_failed'));
      });
    } catch (e) {
      alert(tx('notation.open_failed_error').replace('{error}', String(e)));
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
  let selectedTemperament = 0; // index into temperaments array (0 = Origineel (zoals opgenomen), 1 = gelijkzwevend)
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
    const name = prompt(tx('temperaments.name_prompt'));
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
    // Eigen cents = een echte stemming → hertemperen (zoals GrandOrgue).
    invoke('set_temperament', { noteOffsets: customCents, fineTune, name, retune: true }).catch(console.error);
  }
  let fineTune = 0; // cents offset from A440
  // Crescendo state
  let crescendoEnabled = false;
  let crescendoStages = [];
  let crescendoStage = 0;
  let crescendoNumStages = 15;
  let crescendoLearning = false;
  let crescendoBinding = null;
  // Na een lokale matrix-bewerking de backend-poll even niet laten terugschrijven
  // (de push is nog onderweg; een oud poll-antwoord zou de bewerking wissen).
  let crescDirtyUntil = 0;

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

  // Crescendo wordt PER ORGEL bewaard — sinds 0.7.38 in de backend (OrganSettings,
  // .jm-settings.json), niet meer in localStorage. De oude per-orgel-sleutel
  // (hash van het orgel-id) wordt eenmalig gemigreerd en daarna verwijderd.
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

  // Matrix/aantal stappen naar de backend (bron van waarheid; past de huidige
  // trap direct opnieuw toe) en de per-orgel opslag laten bijwerken.
  async function saveCrescendo() {
    crescDirtyUntil = Date.now() + 1500;
    try {
      await invoke('set_crescendo_config', { stages: crescendoStages, enabled: crescendoEnabled, numStages: crescendoNumStages });
      dispatch('refreshOrgan');
      dispatch('refreshMidiMappings');
    } catch (e) {
      console.error('Failed to save crescendo:', e);
    }
  }

  // Alleen aan/uit (checkbox, piston 41): stuurt de (mogelijk verouderde) matrix
  // van dit venster NIET mee. Uit → backend trekt de trede-registers weg.
  async function setCrescendoEnabledUI(enabled) {
    crescendoEnabled = enabled;
    crescDirtyUntil = Date.now() + 1500;
    try {
      await invoke('set_crescendo_enabled', { enabled });
      dispatch('refreshOrgan');
      dispatch('refreshMidiMappings');
    } catch (e) {
      console.error('Failed to toggle crescendo:', e);
    }
  }

  // Backend-config → UI-state, met JSON-guard (geen re-render bij gelijke inhoud).
  function applyCrescendoConfig(cfg) {
    if (!cfg || typeof cfg !== 'object') return;
    const stages = Array.isArray(cfg.stages) ? cfg.stages : [];
    if (JSON.stringify(stages) !== JSON.stringify(crescendoStages)) crescendoStages = stages;
    if (!!cfg.enabled !== crescendoEnabled) crescendoEnabled = !!cfg.enabled;
    const n = cfg.num_stages > 0 ? cfg.num_stages : 15;
    if (n !== crescendoNumStages) crescendoNumStages = n;
    if (typeof cfg.stage === 'number' && cfg.stage !== crescendoStage) crescendoStage = cfg.stage;
    // Pedaalbinding (elders ingeleerd/gewijzigd) meenemen, met JSON-guard.
    const b = Array.isArray(cfg.binding) && cfg.binding.length === 5
      ? { channel: cfg.binding[0], cc: cfg.binding[1], min: cfg.binding[2], max: cfg.binding[3], invert: !!cfg.binding[4] }
      : null;
    if (JSON.stringify(b) !== JSON.stringify(crescendoBinding)) crescendoBinding = b;
  }

  // Laad de crescendo-config voor het huidige orgel uit de backend. Staat daar
  // nog niets (orgel van vóór 0.7.38), dan eenmalig de oude localStorage-config
  // migreren (alleen het hoofdvenster pusht) en de oude sleutels verwijderen.
  async function loadCrescendoForOrgan() {
    let cfg = null;
    try { cfg = await invoke('get_crescendo_config'); } catch (e) { cfg = null; }
    const backendLeeg = !cfg || (!Array.isArray(cfg.stages) || cfg.stages.length === 0) && !cfg.enabled;
    if (backendLeeg && !secondary) {
      let legacy = null;
      try {
        const saved = localStorage.getItem(crescKey()) || localStorage.getItem('jm-orgue-crescendo');
        if (saved) legacy = JSON.parse(saved);
      } catch (e) { legacy = null; }
      let gepusht = false;
      if (legacy && Array.isArray(legacy.stages) && legacy.stages.length) {
        // Alleen IDs die in dít orgel bestaan (de globale sleutel kon van een ander orgel zijn).
        const known = new Set([...getAllStopIds(), ...((organInfo?.couplers || []).map(c => c.id))]);
        const stages = legacy.stages.map(s => (Array.isArray(s) ? s.filter(id => known.has(id)) : []));
        if (stages.some(s => s.length)) {
          crescendoStages = stages;
          crescendoEnabled = legacy.enabled || false;
          crescendoNumStages = legacy.numStages || 15;
          try {
            await invoke('set_crescendo_config', { stages: crescendoStages, enabled: crescendoEnabled, numStages: crescendoNumStages });
            dispatch('refreshMidiMappings');
            gepusht = true;
          } catch (e) { console.error('Crescendo-migratie mislukt:', e); }
        }
      }
      try { localStorage.removeItem(crescKey()); localStorage.removeItem('jm-orgue-crescendo'); } catch (e) {}
      if (!gepusht) { crescendoStages = []; crescendoEnabled = false; crescendoNumStages = 15; }
    } else if (cfg) {
      applyCrescendoConfig(cfg);
    } else {
      crescendoStages = [];
    }
    crescendoStage = 0;
    loadCrescendoBinding();
  }

  // Backend-config volgen (andere vensters/piston/pedaal): elke seconde vanuit
  // refreshSharedPrefs, met guard tegen terugschrijven van een lopende bewerking.
  async function pollCrescendoConfig() {
    if (Date.now() < crescDirtyUntil) return;
    try {
      const cfg = await invoke('get_crescendo_config');
      if (Date.now() < crescDirtyUntil) return;
      applyCrescendoConfig(cfg);
    } catch (e) {}
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
  // Aantal stappen wijzigen: bij krimp met gevulde hogere stappen eerst bevestigen
  // (die stappen gaan verloren); de backend clampt de huidige trap.
  async function setCrescendoNumStagesUI(n) {
    const oud = crescendoNumStages;
    const verlies = n < oud && (crescendoStages || []).slice(n).some(s => Array.isArray(s) && s.length);
    // LET OP: {new} staat twee keer in de tekst (in alle talen), dus met /g
    // vervangen — een gewone .replace() pakt alleen het eerste voorkomen en
    // laat letterlijk "{new}" in de bevestigingsvraag staan.
    if (verlies && !(await confirm(tx('crescendo.shrink_confirm').replace(/\{old\}/g, String(oud)).replace(/\{new\}/g, String(n))))) {
      crescendoNumStages = oud; // select terugzetten
      return;
    }
    crescendoNumStages = n;
    crescendoStages = ensureCrescStages();
    if (crescendoStage > n) crescendoStage = n;
    saveCrescendo();
  }

  // Trap zetten (klik op de balk/kolomkop): de backend past hem additief toe
  // (zelfde kern als het pedaal); daarna alleen get_organ_info verversen.
  async function setCrescendoStage(stage) {
    try {
      await invoke('set_crescendo_stage', { stage });
      crescendoStage = stage;
      dispatch('refreshOrgan');
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
      await loadCrescendoBinding();
      dispatch('refreshMidiMappings');
    } catch (e) {
      console.error('Failed to toggle crescendo invert:', e);
    }
  }

  async function setCrescendoRangeUI(minVal, maxVal) {
    const mn = Math.max(0, Math.min(127, minVal | 0));
    const mx = Math.max(0, Math.min(127, maxVal | 0));
    if (Math.abs(mx - mn) < 8) {
      alert(tx('learn.pedal_range_too_small'));
      await loadCrescendoBinding();
      return;
    }
    try {
      await invoke('set_crescendo_range', { minVal: mn, maxVal: mx });
      await loadCrescendoBinding();
      dispatch('refreshMidiMappings');
    } catch (e) {
      console.error('Failed to set crescendo range:', e);
    }
  }

  // Crescendo-binding (incl. min/max/invert) uit de backend — bij orgel-load en
  // na elke binding-mutatie expliciet, niet via een reactive op organInfo.
  async function loadCrescendoBinding() {
    try {
      const r = await invoke('get_crescendo_binding');
      if (Array.isArray(r) && r.length === 5) {
        crescendoBinding = { channel: r[0], cc: r[1], min: r[2], max: r[3], invert: !!r[4] };
      } else {
        crescendoBinding = null;
      }
    } catch (e) {}
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

  // Reactief ($t) zodat de labels de taalkeuze volgen; de markup leest `.label`.
  $: eqBandTypes = [
    { value: 'peak', label: $t('eq.band_peak') },
    { value: 'lowpass', label: $t('eq.band_lowpass') },
    { value: 'highpass', label: $t('eq.band_highpass') },
    { value: 'bandpass', label: $t('eq.band_bandpass') },
    { value: 'lowshelf', label: $t('eq.band_lowshelf') },
    { value: 'highshelf', label: $t('eq.band_highshelf') },
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

  // Luisterprofielen (0.7.47). Bewust GEEN correcties per koptelefoonmodel:
  // die zouden gemeten moeten zijn, en verzinnen we niet. Dit zijn eerlijke,
  // algemeen omschreven correcties voor de drie situaties waarin een huisorgel
  // meestal klinkt. Ze vullen de banden; daarna is alles nog met de hand bij
  // te stellen.
  $: eqProfielen = [
    { id: 'flat', label: $t('eq.profile_flat'), bands: [
      { enabled: true, band_type: 'lowshelf',  freq: 200,  gain_db: 0, bandwidth: 1.0, channel: null },
      { enabled: true, band_type: 'peak',      freq: 1000, gain_db: 0, bandwidth: 2.0, channel: null },
      { enabled: true, band_type: 'highshelf', freq: 4000, gain_db: 0, bandwidth: 1.0, channel: null },
    ] },
    // Koptelefoon: de 16' dreunt op een koptelefoon veel eerder dan in een kerk,
    // en het bovenwerk wordt scherp omdat er geen ruimte tussen zit.
    { id: 'headphones', label: $t('eq.profile_headphones'), bands: [
      { enabled: true, band_type: 'lowshelf',  freq: 120,  gain_db: -3.0, bandwidth: 1.0, channel: null },
      { enabled: true, band_type: 'peak',      freq: 3000, gain_db: -2.0, bandwidth: 1.5, channel: null },
      { enabled: true, band_type: 'highshelf', freq: 8000, gain_db:  1.5, bandwidth: 1.0, channel: null },
    ] },
    // Kleine luidsprekers kunnen onder ~60 Hz toch niets; die energie kost
    // alleen membraanslag en vervorming.
    { id: 'small_speakers', label: $t('eq.profile_small_speakers'), bands: [
      { enabled: true, band_type: 'highpass',  freq: 60,   gain_db:  0,   bandwidth: 1.0, channel: null },
      { enabled: true, band_type: 'lowshelf',  freq: 200,  gain_db:  2.5, bandwidth: 1.0, channel: null },
      { enabled: true, band_type: 'highshelf', freq: 6000, gain_db: -1.5, bandwidth: 1.0, channel: null },
    ] },
  ];
  function pasEqProfielToe(id) {
    const p = eqProfielen.find(x => x.id === id);
    if (!p) return;
    eqBands = p.bands.map(b => ({ ...b }));
    eqEnabled = true;
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

  // Reactief ($t) zodat de namen de taalkeuze volgen; de markup leest `.name`,
  // de functies hieronder lezen alleen de numerieke velden (index blijft gelijk).
  $: reverbPresets = [
    { name: $t('reverb.preset_small_chapel'), rt60: 1.2, preDelay: 10, damping: 60, roomSize: 50 },
    { name: $t('reverb.preset_village_church'), rt60: 2.0, preDelay: 25, damping: 50, roomSize: 80 },
    { name: $t('reverb.preset_large_church'), rt60: 3.5, preDelay: 40, damping: 40, roomSize: 120 },
    { name: $t('reverb.preset_cathedral'), rt60: 6.0, preDelay: 60, damping: 30, roomSize: 180 },
    { name: $t('reverb.preset_gothic_cathedral'), rt60: 10.0, preDelay: 80, damping: 20, roomSize: 250 },
    { name: $t('reverb.preset_concert_hall'), rt60: 2.2, preDelay: 30, damping: 50, roomSize: 100 },
  ];

  // Heeft deze sampleset de kerkakoestiek zelf al in het geluid? Dat is zo
  // zodra er release-opnamen zijn: de uitklank van de pijp in de ruimte.
  $: natteSampleset = (organInfo?.release_pipes || 0) > 0;

  /// Terug naar wat de sampleset zelf opgenomen heeft: alle kunstmatige galm
  /// eraf. Eén klik, zodat een misgelopen preset geen zoekplaatje wordt.
  function reverbTerugNaarOpname() {
    reverb = 0;
    onReverbChange();
    if (reverbType === 'algorithmic') updateAlgorithmicReverb();
    else persistReverbConfig();
  }

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
      dispatch('refreshMidiMappings'); // autosave
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
  function formatPan(v, _t = $t) {
    if (v === 0) return _t('pan.center');
    if (v < 0) return _t('pan.left').replace('{n}', String(Math.abs(v)));
    return _t('pan.right').replace('{n}', String(v));
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
  // Kanaalaantal van de lopende stream: gelatcht uit de status-poll (blijft
  // stabiel tijdens het korte 'geen player'-venster van een ASIO-wissel of
  // noodherstel), met de eenmalige query als bootstrap vóór de eerste poll.
  let lastKnownChannels = 0;
  let queriedChannelCount = 0;
  $: if (audioChannels > 0) lastKnownChannels = audioChannels;
  $: audioChannelCount = lastKnownChannels || queriedChannelCount || 2;

  // Output-kanalen, C/Cis-aan/uit per divisie en C/Cis-spreiding worden per orgel in de backend
  // bewaard (apply_saved_output_channels) en in loadAudioSettingsForOrgan() voor de UI geladen —
  // niet meer in globale localStorage-keys.

  invoke('query_audio_channel_count').then(c => { queriedChannelCount = c; }).catch(() => {});

  // Neutrale uitgangsnamen: bij stereo Links/Rechts, anders 'Uitgang N' — op een
  // 8-kanaals interface zijn het uitgangen 3…8, geen 7.1-luidsprekers.
  function channelLabel(ch, total) {
    if (total <= 2) return ch === 0 ? $t('settings.channel_left') : ch === 1 ? $t('settings.channel_right') : $t('settings.channel_n').replace('{n}', ch + 1);
    return $t('settings.channel_n').replace('{n}', ch + 1);
  }

  // Spiegel van de per-klavier-kanalen verversen wanneer een ander venster ze
  // wijzigde (alleen lezen; apply_saved_output_channels niet opnieuw draaien).
  $: if (divisionChannelsVersion > 0 && organInfo?.divisions?.length) refreshDivisionChannelMirror();
  async function refreshDivisionChannelMirror() {
    try {
      const chans = await invoke('get_division_output_channels');
      const m = {};
      organInfo.divisions.forEach((d, i) => { if (Array.isArray(chans?.[i]) && chans[i].length) m[d.name] = chans[i]; });
      divisionOutputChannels = m;
    } catch (e) {}
  }

  function getDivisionChannels(div) {
    const c = divisionOutputChannels[div];
    return Array.isArray(c) ? c : [];
  }

  // Uitgangspaar voor het hele orgel (Algemene instellingen → Audio-uitvoer):
  // één keuze "kanalen 1–2 / 3–4 / …" die alle klavieren tegelijk zet. Wie
  // klavieren over verschillende uitgangen verdeelt (per-klavier-instelling
  // onder MIDI-kanaal per klavier) ziet hier "aangepast".
  $: outputChannelTotal = Math.max(2, audioChannelCount);

  // Luidspreker-testsignaal (0.7.47): stuurt geluid naar precies één uitgang,
  // zodat je kunt horen welke stekker in welke kast zit. null = uit.
  let testSignaalKanaal = null;
  let testSignaalSoort = 0;
  async function zetTestsignaal(kanaal) {
    // Nogmaals op hetzelfde kanaal = uit (de knop is een schakelaar).
    const doel = (testSignaalKanaal === kanaal) ? null : kanaal;
    try {
      await invoke('set_output_test_signal', { channel: doel, kind: testSignaalSoort, levelDb: -20 });
      testSignaalKanaal = doel;
    } catch (e) { console.error('Testsignaal mislukt:', e); }
  }
  // Nooit met een lopend testsignaal blijven zitten: bij het verlaten van het
  // instellingenscherm of het sluiten van het venster gaat hij uit.
  $: if (activeView !== 'algemene-instellingen' && testSignaalKanaal !== null) zetTestsignaal(testSignaalKanaal);
  $: outputPairSel = computeOutputPair(divisionOutputChannels, organInfo);
  function computeOutputPair(chmap, oi) {
    const divs = oi?.divisions || [];
    if (!divs.length) return '0';
    let first = null;
    for (const d of divs) {
      const c = Array.isArray(chmap[d.name]) && chmap[d.name].length ? chmap[d.name] : [0, 1];
      const key = c.join(',');
      if (first === null) first = key;
      else if (first !== key) return 'custom';
    }
    const arr = first.split(',').map(Number);
    if (arr.length === 2 && arr[1] === arr[0] + 1 && arr[0] % 2 === 0) return String(arr[0] / 2);
    return 'custom';
  }
  function setOutputPair(v) {
    if (v === 'custom') return;
    const pair = Number(v);
    const list = [pair * 2, pair * 2 + 1];
    const divs = organInfo?.divisions || [];
    for (const d of divs) divisionOutputChannels[d.name] = list.slice();
    divisionOutputChannels = divisionOutputChannels;
    Promise.all(divs.map(d => invoke('set_division_output_channels', { division: d.name, channels: list })))
      .then(() => dispatch('divisionChannelsChanged'))
      .catch(() => {});
  }

  // Polyfonie-kap en stereo-samples (globale audio-voorkeuren; waarde via get_status).
  // Tot 32.768, gelijk aan het hoogste dat andere software noemt. Wat een pc
  // écht haalt bepaalt de rekentijd, niet dit getal: gemeten kost elke stem
  // ongeveer 0,17 % van de buffertijd, dus loopt een gewone pc rond de 500
  // stemmen tegen zijn deadline. De belastingsmeter eronder toont dat.
  const POLYPHONY_CHOICES = [512, 1024, 1536, 2048, 3072, 4096, 8192, 16384, 32768];
  async function setPolyphony(v) {
    try { await invoke('set_polyphony', { voices: Number(v) }); } catch (e) { console.error(e); }
    refreshAudioStatus();
  }
  let stereoReloadHint = false;
  async function setStereoSamples(on) {
    try { await invoke('set_stereo_samples', { on }); stereoReloadHint = true; } catch (e) { console.error(e); }
    refreshAudioStatus();
  }

  // Perspectieven en gestapelde ranks (0.7.38): per orgel welke microfoon-
  // posities geladen worden (vraagt herladen) en hun live volume. De backend
  // spiegelt de runtime-staat in organInfo.perspectives, dus de 300 ms-poll
  // zet geen verouderde waarden terug.
  let perspectives = [];
  $: perspectives = organInfo?.perspectives || [];
  let perspReloadHint = false;
  async function setPerspectiveEnabled(name, on) {
    try {
      perspReloadHint = await invoke('set_perspective_enabled', { name, enabled: on });
      perspectives = perspectives.map(p => p.name === name ? { ...p, enabled: on } : p);
      // Autosave via App (refreshMidiMappings → scheduleAutoSave).
      dispatch('refreshMidiMappings');
    } catch (e) { console.error(e); }
  }
  // Alle opnameposities in het geheugen houden: dan is wisselen ogenblikkelijk
  // in plaats van een herlaad. Kost geheugen, dus expliciet aan te zetten.
  let laadAllePosities = false;
  async function haalLaadAllePosities() {
    try { laadAllePosities = await invoke('get_load_all_perspectives'); } catch (e) {}
  }
  $: if (organInfo?.id) haalLaadAllePosities();
  async function zetLaadAllePosities(on) {
    try {
      perspReloadHint = await invoke('set_load_all_perspectives', { enabled: on });
      laadAllePosities = on;
      dispatch('refreshMidiMappings');
    } catch (e) { console.error(e); }
  }

  // Extra koppels (0.7.47): koppels die de app uit de divisie-indeling kan
  // afleiden maar die de sampleset zelf niet levert. Bijschakelen werkt
  // ogenblikkelijk, zonder het orgel opnieuw te laden.
  let extraKoppelOpties = [];
  async function haalExtraKoppels() {
    try { extraKoppelOpties = await invoke('get_extra_coupler_options'); } catch (e) { extraKoppelOpties = []; }
  }
  $: if (organInfo?.id) haalExtraKoppels();
  async function zetExtraKoppel(id, aan) {
    const ids = extraKoppelOpties.filter(k => (k.id === id ? aan : k.active)).map(k => k.id);
    try {
      await invoke('set_extra_couplers', { ids });
      await haalExtraKoppels();
      dispatch('refreshMidiMappings');
    } catch (e) { console.error('Extra koppel zetten mislukt:', e); }
  }

  // Doorlopend klavier: toetsen buiten het opgenomen bereik spelen de pijp een
  // octaaf hoger of lager in plaats van te zwijgen.
  let doorlopendKlavier = false;
  async function haalDoorlopendKlavier() {
    try { doorlopendKlavier = await invoke('get_continuous_keyboard'); } catch (e) {}
  }
  $: if (organInfo?.id) haalDoorlopendKlavier();
  async function zetDoorlopendKlavier(aan) {
    try {
      await invoke('set_continuous_keyboard', { enabled: aan });
      doorlopendKlavier = aan;
      dispatch('refreshMidiMappings');
    } catch (e) { console.error('Doorlopend klavier zetten mislukt:', e); }
  }

  let perspGainTimer = null;
  function setPerspectiveGain(name, v) {
    const gainDb = Number(v);
    perspectives = perspectives.map(p => p.name === name ? { ...p, gain_db: gainDb } : p);
    invoke('set_perspective_gain', { name, gainDb }).catch(console.error);
    clearTimeout(perspGainTimer);
    perspGainTimer = setTimeout(() => dispatch('refreshMidiMappings'), 400);
  }
  function reloadCurrentOrgan() {
    const id = organInfo?.id;
    if (!id) return;
    perspReloadHint = false;
    const l = id.toLowerCase();
    if (l.endsWith('.organ') || l.endsWith('.organ_hauptwerk_xml')) dispatch('loadOrgan', id);
    else dispatch('scanFolder', id);
  }
  function isChannelSelected(div, ch) { return getDivisionChannels(div).includes(ch); }
  function toggleDivisionChannel(div, ch) {
    let list = getDivisionChannels(div).slice();
    if (list.includes(ch)) list = list.filter(x => x !== ch);
    else { list.push(ch); list.sort((a, b) => a - b); }
    divisionOutputChannels[div] = list;
    divisionOutputChannels = divisionOutputChannels;
    // set_division_output_channels schrijft de per-orgel state in de backend (geen localStorage meer);
    // daarna het actieve profiel laten bijwerken (App.svelte) — pas na bevestiging.
    invoke('set_division_output_channels', { division: div, channels: list })
      .then(() => dispatch('divisionChannelsChanged'))
      .catch(() => {});
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
      alert(tx('midi.bluetooth_scan_failed').replace('{error}', String(e)));
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
      alert(tx('midi.bluetooth_connect_failed').replace('{error}', String(e)));
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

  // Koppels op het hoofdscherm: in een eigen balk ('bar', standaard — zoals
  // een echte speeltafel en zoals GrandOrgue/Hauptwerk ze tonen) of als
  // registerknop binnen de divisie ('division', het oude gedrag). Per orgel.
  let couplerPlacement = 'bar';
  function toggleCouplerPlacement() {
    couplerPlacement = couplerPlacement === 'bar' ? 'division' : 'bar';
    localStorage.setItem(organUiKey('jm-orgue-coupler-placement'), couplerPlacement);
    publishRemoteLayout();
  }
  // Balkgroepen: per klavier (in divisievolgorde) de zichtbare koppels,
  // unison eerst, dan super/sub, dan de rest.
  $: couplerBarGroups = buildCouplerGroups(displayOrgan, visibleCouplers, couplerPlacement);
  function buildCouplerGroups(org, vis, placement) {
    if (placement !== 'bar' || !org?.couplers?.length) return [];
    const order = (org.divisions || []).map(d => d.name);
    const groups = new Map();
    for (const c of org.couplers) {
      if (vis[c.id] !== true) continue;
      const key = c.display_in_division || c.source_division || '';
      if (!groups.has(key)) groups.set(key, []);
      groups.get(key).push(c);
    }
    const rank = t => (t === 'unison' ? 0 : t === 'super' ? 1 : t === 'sub' ? 2 : 3);
    return [...groups.entries()]
      .sort((a, b) => order.indexOf(a[0]) - order.indexOf(b[0]))
      .map(([division, couplers]) => ({
        division,
        couplers: couplers.slice().sort((a, b) => rank(a.coupler_type) - rank(b.coupler_type) || a.name.localeCompare(b.name)),
      }));
  }
  function couplerLabel(c) { return (c.name || '').replace(/\n/g, ' '); }

  function toggleCouplerVisibility(couplerId) {
    visibleCouplers[couplerId] = !isCouplerVisible(couplerId);
    visibleCouplers = visibleCouplers;
    localStorage.setItem(organUiKey('jm-orgue-visible-couplers'), JSON.stringify(visibleCouplers));
    publishRemoteLayout();
  }

  function isSwellEnabled(divisionName) {
    return swellEnabled[divisionName] === true;
  }

  function toggleSwellEnabled(divisionName) {
    swellEnabled[divisionName] = !swellEnabled[divisionName];
    swellEnabled = swellEnabled;
    localStorage.setItem(organUiKey('jm-orgue-swell-enabled'), JSON.stringify(swellEnabled));
    const stashKey = organUiKey('jm-orgue-swell-binding-stash');
    let stash = {};
    try { stash = JSON.parse(localStorage.getItem(stashKey) || '{}'); } catch (e) { stash = {}; }
    if (!swellEnabled[divisionName]) {
      // Vinkje uit: kast open (100 %) en de pedaalkoppeling PARKEREN — niet
      // wissen (uit/aan vergde vroeger opnieuw inleren), maar ook niet actief
      // laten (het pedaal zou een verborgen kast blijven sluiten).
      const b = swellBindings[divisionName];
      if (b) {
        stash[divisionName] = { channel: b.channel, cc_num: b.cc_num, min_val: b.min_val, max_val: b.max_val, invert: !!b.invert };
        localStorage.setItem(stashKey, JSON.stringify(stash));
        invoke('clear_swell_binding', { division: divisionName }).catch(() => {});
        delete swellBindings[divisionName];
        swellBindings = swellBindings;
        // Ook opslaan: zonder autosave kwam de gewiste koppeling bij de
        // volgende load uit .jm-settings.json terug (0.7.44).
        dispatch('refreshMidiMappings');
      }
      setSwellLevel(divisionName, 1.0);
    } else if (stash[divisionName]) {
      // Vinkje aan: geparkeerde koppeling terugzetten (incl. bereik/inversie).
      const b = stash[divisionName];
      const divIdx = Math.max(0, (organInfo?.divisions || []).findIndex(d => d.name === divisionName));
      (async () => {
        try {
          const verdrongen = await invoke('set_swell_binding_manual', { division: divisionName, divisionIndex: divIdx, channel: b.channel, ccNum: b.cc_num });
          if (b.min_val != null && b.max_val != null) await invoke('set_swell_range', { division: divisionName, minVal: b.min_val, maxVal: b.max_val });
          if (b.invert) await invoke('set_swell_invert', { division: divisionName, invert: true });
          showPedalNotice(divisionName, displacedText(verdrongen) && tx('pedal.taken_over').replace('{what}', displacedText(verdrongen)));
        } catch (e) { console.error('Zwelkoppeling terugzetten mislukt:', e); }
        delete stash[divisionName];
        localStorage.setItem(stashKey, JSON.stringify(stash));
        await reloadPedalBindings();
        dispatch('refreshMidiMappings');
      })();
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
      // Echte ODF/JM-Rec-koppels én de gegenereerde unison-koppels standaard
      // zichtbaar; super/sub e.d. blijven opt-in (instellingen → koppels).
      if (c.id && (c.id.startsWith('real_coupler_') || c.coupler_type === 'unison') && visibleCouplers[c.id] === undefined) visibleCouplers[c.id] = true;
    }
    visibleCouplers = visibleCouplers;
    if (!secondary) localStorage.setItem(organUiKey('jm-orgue-visible-couplers'), JSON.stringify(visibleCouplers));
    couplerPlacement = readOrganUiPref('jm-orgue-coupler-placement') === 'division' ? 'division' : 'bar';
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

  // ===== Wederzijdse uitsluiting zwelkast ↔ generaal crescendo =====
  // Eén (kanaal, CC) kan maar één functie hebben; de backend dwingt dat af
  // (claim_pedal_cc) en geeft terug wát er is verdrongen. De UI moet daarom na
  // ELKE bindingmutatie BEIDE vakken verversen — anders bleef het andere vak de
  // gewiste koppeling tonen ("ingesteld maar werkt niet").
  async function reloadPedalBindings() {
    await Promise.all([loadSwellBindings(), loadCrescendoBinding()]);
  }
  // Korte, niet-blokkerende melding bij het vak waar de actie plaatsvond.
  // scope = 'crescendo' of de divisienaam.
  let pedalNotice = null;   // { scope, text }
  let pedalNoticeTimer = null;
  function showPedalNotice(scope, text) {
    if (!text) return;
    clearTimeout(pedalNoticeTimer);
    pedalNotice = { scope, text };
    pedalNoticeTimer = setTimeout(() => { pedalNotice = null; }, 8000);
  }
  // Backend-lijst met verdrongen koppelingen → leesbare tekst.
  function displacedText(list) {
    if (!Array.isArray(list) || !list.length) return '';
    return list.map(d => d && d.kind === 'crescendo'
      ? tx('pedal.replaces_crescendo')
      : tx('pedal.replaces_swell').replace('{division}', (d && d.division) || '')).join(', ');
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

  // LET OP: in de MARKUP niet via deze helper lezen maar rechtstreeks
  // `divisionVolumes[naam]` — Svelte 4 trackt template-afhankelijkheden op de
  // variabelen die in de expressie zelf staan; via de helper zag het fragment
  // de 100ms-poll-updates niet en stonden de zwel-lampjes/percentage stil
  // terwijl de audio wél reageerde (bug gefixt in 0.7.12).
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
  // Flow-token: Annuleren/Opnieuw laat de oude (nog blokkerende) backend-lus
  // niet meer in een nieuwe flow landen — antwoorden van een oude generatie
  // worden genegeerd en de backend-lus wordt expliciet afgebroken.
  let learnGen = 0;
  const MIN_PEDAL_RANGE = 8; // CC-eenheden tussen laagste en hoogste stand

  async function cancelPedalLearn() {
    learnGen++;
    // De finally van de afgebroken flow ziet een andere generatie en laat de
    // learning-vlaggen dus staan — de leerknoppen bleven daardoor disabled en
    // de crescendoknop bleef "inleren…" tonen. Hier expliciet wissen; een
    // direct volgende flow (Opnieuw) zet ze zelf weer.
    learningSwellDivision = null;
    crescendoLearning = false;
    try { await invoke('cancel_pedal_learn'); } catch (e) {}
  }

  async function learnPedalFlow(kind, divisionName = null) {
    if (!midiConnected) {
      alert(tx('midi.connect_first'));
      return;
    }
    const gen = ++learnGen;
    pedalLearnModal = { kind, division: divisionName, step: 1, first: null, msg: '' };
    if (kind === 'zwel') learningSwellDivision = divisionName; else crescendoLearning = true;
    try {
      const first = await invoke('learn_pedal_position', { channel: null, ccNum: null });
      if (gen !== learnGen || !pedalLearnModal) return; // geannuleerd / nieuwe flow
      if (!first) {
        pedalLearnModal = { ...pedalLearnModal, step: 'fout', msg: tx('learn.pedal_timeout_low') };
        return;
      }
      pedalLearnModal = { ...pedalLearnModal, step: 2, first: { channel: first[0], cc: first[1], value: first[2] } };
      const second = await invoke('learn_pedal_position', { channel: first[0], ccNum: first[1] });
      if (gen !== learnGen || !pedalLearnModal) return;
      if (!second) {
        pedalLearnModal = { ...pedalLearnModal, step: 'fout', msg: tx('learn.pedal_timeout_high') };
        return;
      }
      // Bereikcontrole: (vrijwel) gelijke laag/hoog-waarde zou de zwel dicht
      // laten (of de crescendo op 0) — weigeren met 'Opnieuw'.
      if (Math.abs(second[2] - pedalLearnModal.first.value) < MIN_PEDAL_RANGE) {
        pedalLearnModal = { ...pedalLearnModal, step: 'fout', msg: tx('learn.pedal_range_too_small') };
        return;
      }
      let verdrongen = [];
      if (kind === 'zwel') {
        const divisionIndex = Math.max(0, (organInfo?.divisions || []).findIndex(d => d.name === divisionName));
        verdrongen = await invoke('apply_learned_swell', {
          division: divisionName, divisionIndex,
          channel: first[0], ccNum: first[1],
          lowVal: pedalLearnModal.first.value, highVal: second[2],
        });
      } else {
        verdrongen = await invoke('apply_learned_crescendo', {
          channel: first[0], ccNum: first[1],
          lowVal: pedalLearnModal.first.value, highVal: second[2],
        });
      }
      // Altijd BEIDE vakken: een crescendo-inleer wist de zwelkoppeling op
      // dezelfde CC (en omgekeerd) — zonder deze verversing bleef het andere
      // vak "CC7" tonen terwijl de koppeling backend-zijdig al weg was.
      await reloadPedalBindings();
      if (gen !== learnGen || !pedalLearnModal) return;
      const vervangt = displacedText(verdrongen);
      pedalLearnModal = { ...pedalLearnModal, step: 'klaar',
        msg: tx('learn.pedal_result').replace('{cc}', String(first[1])).replace('{channel}', String(first[0] + 1))
             + (vervangt ? ` — ${tx('pedal.replaces_prefix')} ${vervangt}` : '') };
      dispatch('refreshMidiMappings');
      setTimeout(() => { if (pedalLearnModal && pedalLearnModal.step === 'klaar') pedalLearnModal = null; }, vervangt ? 5000 : 2000);
    } catch (e) {
      if (gen === learnGen && pedalLearnModal) pedalLearnModal = { ...pedalLearnModal, step: 'fout', msg: String(e) };
    } finally {
      if (gen === learnGen) {
        learningSwellDivision = null;
        crescendoLearning = false;
      }
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
      await reloadPedalBindings();
      dispatch('refreshMidiMappings');
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
      dispatch('refreshMidiMappings');
    } catch (e) {
      console.error('Failed to toggle swell invert:', e);
    }
  }

  async function setSwellRangeUI(divisionName, minVal, maxVal) {
    const mn = Math.max(0, Math.min(127, minVal | 0));
    const mx = Math.max(0, Math.min(127, maxVal | 0));
    if (Math.abs(mx - mn) < MIN_PEDAL_RANGE) {
      alert(tx('learn.pedal_range_too_small'));
      await loadSwellBindings(); // invoervelden terug op de geldende waarden
      return;
    }
    try {
      await invoke('set_swell_range', { division: divisionName, minVal: mn, maxVal: mx });
      await loadSwellBindings(); // backend ordent min/max
      dispatch('refreshMidiMappings');
    } catch (e) {
      console.error('Failed to set swell range:', e);
    }
  }

  // ===== Handmatig instellen van pedaal-koppelingen en toetsenbereik =====
  // Het handmatige alternatief voor 'Leer pedaal'/'Leer': kanaal, CC of
  // nootbereik direct intypen. Weergave-kanaal is 1-16; op de kabel 0-15.
  // ccNum mag hier NIET meer stilzwijgend 7 worden: alleen een kanaal kiezen
  // (zonder CC) maakte vroeger een koppeling op CC7 aan — precies het conflict
  // waarin zwel én crescendo op (kanaal, 7) terechtkwamen. Zonder geldige CC
  // gebeurt er nu niets.
  async function setSwellManualUI(divisionName, divIdx, channelDisplay, ccNum) {
    const ch = Math.max(1, Math.min(16, channelDisplay | 0)) - 1;
    if (ccNum == null || isNaN(ccNum)) { showPedalNotice(divisionName, tx('pedal.cc_required')); return; }
    const cc = Math.max(0, Math.min(127, ccNum | 0));
    try {
      const verdrongen = await invoke('set_swell_binding_manual', { division: divisionName, divisionIndex: divIdx, channel: ch, ccNum: cc });
      await reloadPedalBindings();
      const vervangt = displacedText(verdrongen);
      if (vervangt) showPedalNotice(divisionName, tx('pedal.taken_over').replace('{what}', vervangt));
      dispatch('refreshMidiMappings');
    } catch (e) {
      console.error('Handmatige zwelkast-koppeling mislukt:', e);
    }
  }

  async function setCrescendoManualUI(channelDisplay, ccNum) {
    const ch = Math.max(1, Math.min(16, channelDisplay | 0)) - 1;
    if (ccNum == null || isNaN(ccNum)) { showPedalNotice('crescendo', tx('pedal.cc_required')); return; }
    const cc = Math.max(0, Math.min(127, ccNum | 0));
    try {
      const verdrongen = await invoke('set_crescendo_binding_manual', { channel: ch, ccNum: cc });
      await reloadPedalBindings();
      const vervangt = displacedText(verdrongen);
      if (vervangt) showPedalNotice('crescendo', tx('pedal.taken_over').replace('{what}', vervangt));
      dispatch('refreshMidiMappings');
    } catch (e) {
      console.error('Handmatige crescendo-koppeling mislukt:', e);
    }
  }

  async function clearCrescendoBindingUI() {
    try {
      await invoke('clear_crescendo_binding');
      crescendoBinding = null;
      await reloadPedalBindings();
      dispatch('refreshMidiMappings');
    } catch (e) {
      console.error('Crescendo-koppeling wissen mislukt:', e);
    }
  }

  // Kort octaaf (C/E) per klavier. Daarna de koppelingen opnieuw ophalen zodat
  // het vinkje de backend volgt.
  async function setShortOctave(division, enabled) {
    try {
      await invoke('set_short_octave', { division, enabled });
      midiMappings = await invoke('get_midi_mappings');
    } catch (e) { console.error('Kort octaaf zetten mislukt:', e); }
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
      // retune: alles behalve "Origineel (zoals opgenomen)" hertempert per pijp
      // vanaf de gemeten toonhoogte (GrandOrgue-semantiek).
      await invoke('set_temperament', { noteOffsets: t.cents, fineTune: fineTune, name: t.name, retune: !t.original });
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
      // Indeling van de afstandsbediening (0.7.39): welke onderdelen en welke
      // divisies het externe scherm toont. De rest van de indeling (zichtbare
      // koppels, koppelplaatsing, registervolgorde, knopvorm) komt uit de
      // UI-voorkeuren van dit orgel en wordt hieronder meegepubliceerd.
      const rl = (s && s.remote_layout) ? s.remote_layout : null;
      remoteParts = {
        couplers: rl ? rl.show_couplers !== false : true,
        tremulant: rl ? rl.show_tremulant !== false : true,
        setzer: rl ? rl.show_setzer !== false : true,
        volume: rl ? rl.show_volume !== false : true,
        panic: rl ? rl.show_panic !== false : true,
      };
      remoteDivisions = {};
      if (rl && Array.isArray(rl.divisions) && rl.divisions.length > 0) {
        for (const d of (organInfo?.divisions || [])) remoteDivisions[d.name] = rl.divisions.includes(d.name);
        // Bewaarde lijst die geen enkele huidige divisie noemt (orgel gewijzigd):
        // alles tonen in plaats van een leeg extern scherm.
        if (!Object.values(remoteDivisions).some(Boolean)) remoteDivisions = {};
      }
      lastRemoteLayoutJson = '';
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
        // Migratie (0.7.38): een bestand van vóór het retune-veld met de oude
        // default "Equal Temperament" (of zonder naam) wordt "Origineel (zoals
        // opgenomen)" — klank ongewijzigd; de naam wijzigt bij de eerstvolgende
        // autosave. Alle andere opgeslagen stemmingen worden op naam gevonden
        // en hertemperen voortaan per pijp (retune = !original).
        const legacy = (t.retune === null || t.retune === undefined);
        if (legacy && (!t.name || t.name === 'Equal Temperament')) {
          idx = 0;
        } else {
          if (t.name) idx = temperaments.findIndex(x => x.name === t.name);
          // Cents-fallback: Origineel (allemaal nullen) uitsluiten, anders
          // matcht elke nul-cents-stemming met onbekende naam index 0.
          if (idx < 0 && Array.isArray(t.custom_cents)) {
            idx = temperaments.findIndex(x => !x.original && Array.isArray(x.cents) && x.cents.length === 12
              && x.cents.every((c, i) => Math.abs(c - t.custom_cents[i]) < 0.01));
          }
        }
        if (idx >= 0) {
          selectedTemperament = idx;
          if (pushToBackend) await invoke('set_temperament', { noteOffsets: temperaments[idx].cents, fineTune, name: temperaments[idx].name, retune: !temperaments[idx].original });
        } else if (Array.isArray(t.custom_cents)) {
          customCents = [...t.custom_cents];
          if (pushToBackend) await invoke('set_temperament', { noteOffsets: t.custom_cents, fineTune, name: t.name || 'Aangepast', retune: true });
        }
      } else {
        // Geen opgeslagen stemming → default (Origineel, zoals opgenomen) toepassen i.p.v. die van het vorige orgel.
        selectedTemperament = 0;
        fineTune = 0;
        const t0 = temperaments[0];
        if (pushToBackend) await invoke('set_temperament', { noteOffsets: t0.cents, fineTune: 0, name: t0.name, retune: false });
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
        // Geen opgeslagen galm voor dit orgel. Een sampleset met RELEASE-opnamen
        // heeft de uitklank van de pijp ín de kerk meegenomen: de akoestiek zit
        // dan al in het geluid. Daar bovenop standaard 30% Dorpskerk zetten gaf
        // een troebel dubbel-galm-geluid ("wet-on-wet"). Zo'n set start dus
        // droog; een set zonder release-opnamen houdt de vertrouwde
        // standaardgalm.
        //
        // Tot 0.7.45 keek deze regel naar de BESTANDSNAAM (.organ /
        // .organ_hauptwerk_xml). Dat ging twee kanten op mis: dezelfde natte
        // GrandOrgue-set als sample-MAP geopend kreeg alsnog 30% galm erover,
        // en een droge ODF-set kreeg er juist geen.
        reverbType = 'algorithmic';
        reverbPreset = 1; reverbRt60 = 2.0; reverbPreDelay = 25;
        reverbDamping = 50; reverbRoomSize = 80;
        // Bewust NIET de reactieve `natteSampleset`: die volgt organInfo en kan
        // op dit moment nog de vorige waarde hebben. Hier direct uit de DTO.
        reverb = (organInfo?.release_pipes || 0) > 0 ? 0 : 30;
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
      // Uitzondering: een golfvormtremulant (GrandOrgue TremulantType=Wave) heeft
      // echte tremulant-OPNAMEN; daar hoort geen synthetische LFO overheen — ook
      // GrandOrgue moduleert dan niets. Een bewaarde keuze wint altijd.
      for (const div of (organInfo?.divisions || [])) {
        // Nagebootste tremulant (LFO) standaard AAN, maar niet als de set echte
        // tremulant-opnamen heeft: 'wave' (GrandOrgue-golfvormtremulant) én
        // 'samples' (Hauptwerk-"tremmed"-laag, eigen sets met _trem-mappen)
        // klinken dan dubbel — opname én nabootsing over elkaar heen.
        const echteTremOpnamen = div.tremulant_kind === 'wave' || div.tremulant_kind === 'samples';
        if (div.has_tremulant && !echteTremOpnamen && tremLfoEnabled[div.name] === undefined) tremLfoEnabled[div.name] = true;
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
    // Pas NU (na get_organ_settings) de indeling van de afstandsbediening
    // publiceren: eerder zou de opgeslagen indeling van dit orgel met de
    // defaults overschreven worden. Alleen het hoofdvenster (pushToBackend)
    // en alleen als dit nog de actuele orgel-load is (stale-guard).
    if (pushToBackend && organInfo?.id === myOrgan) {
      remoteLayoutReady = true;
      publishRemoteLayout();
    }
  }

  async function exportSettings() {
    try {
      const { save } = await import('@tauri-apps/plugin-dialog');
      const path = await save({
        filters: [{ name: tx('dialogs.settings_file_filter'), extensions: ['json'] }],
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
        filters: [{ name: tx('dialogs.settings_file_filter'), extensions: ['json'] }],
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
        filters: [{ name: tx('dialogs.wav_files'), extensions: ['wav'] }],
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
    const cp = readOrganUiPref('jm-orgue-coupler-placement') === 'division' ? 'division' : 'bar';
    if (cp !== couplerPlacement) couplerPlacement = cp;
    try {
      const so = localStorage.getItem(`jm-orgue-stop-order-${organInfo.id || 'default'}`) || '{}';
      if (so !== JSON.stringify(stopOrder)) stopOrder = JSON.parse(so);
    } catch (e) {}
    // Crescendo-matrix/aan-uit uit de backend (bewerkt in een ander venster,
    // piston 41, of de pedaal).
    pollCrescendoConfig();
    // Indeling van de afstandsbediening: wijzigingen uit een EXTRA scherm
    // (koppel-vinkje, knopvorm, registervolgorde) lopen niet door de
    // toggle-functies hierboven; het hoofdvenster publiceert ze alsnog.
    // De verander-guard in publishRemoteLayout voorkomt IPC elke seconde.
    publishRemoteLayout();
  }

  // ===== Afstandsbediening in het netwerk (0.7.38) =====
  // Instellingenblok (Algemene Instellingen): schakelaar, poort, URL('s), QR,
  // nieuw token. Alleen het hoofdvenster laadt/pollt dit (niet in PanelApp).
  let remote = { enabled: false, running: false, port: 8766, token: '', urls: [], qr_svg: null, error: null };
  let remotePortInput = 8766;
  let remoteBusy = false;
  let unlistenRemoteVol = null;
  // De backend frist de kaartnamen één keer per proces op de achtergrond op en
  // meldt dat met 'jm-orgue:library-updated' (get_organ_library wacht daar niet
  // meer op). Promise<UnlistenFn|null>, één keer geregistreerd.
  let libraryUpdatedListener = null;
  async function refreshRemote() {
    try {
      remote = await invoke('get_remote_status');
      remotePortInput = remote.port;
    } catch (e) { console.warn('get_remote_status:', e); }
  }
  async function setRemoteEnabled(on) {
    remoteBusy = true;
    try {
      remote = await invoke('set_remote_enabled', { enabled: on, port: Number(remotePortInput) || 8766 });
    } catch (e) {
      remote = { ...remote, enabled: false, running: false, error: String(e) };
    } finally { remoteBusy = false; }
  }
  async function newRemoteToken() {
    if (!(await confirm(tx('remote.new_token_confirm')))) return;
    try { remote = await invoke('new_remote_token'); }
    catch (e) { remote = { ...remote, error: String(e) }; }
  }

  // ---- Indeling van het externe scherm (0.7.39) ----
  // De afstandsbediening krijgt dezelfde indeling als het registreerscherm:
  // divisievolgorde/-selectie, registervolgorde, alléén de koppels die hier
  // zichtbaar zijn, koppelplaatsing en knopvorm. Plus een keuze welke
  // onderdelen op het externe scherm staan. Alleen het HOOFDVENSTER publiceert
  // (single writer); wijzigingen uit een extra scherm komen via
  // refreshSharedPrefs (localStorage-poll) alsnog hier langs.
  let remoteParts = { couplers: true, tremulant: true, setzer: true, volume: true, panic: true };
  let remoteDivisions = {};        // divisienaam → tonen (afwezig/true = tonen)
  let remoteLayoutReady = false;   // pas publiceren nadat de opgeslagen indeling geladen is
  let lastRemoteLayoutJson = '';   // verander-guard (refreshSharedPrefs draait 1×/s)

  function isRemoteDivisionOn(name) { return remoteDivisions[name] !== false; }

  function toggleRemoteDivision(name, ev) {
    const namen = (displayOrgan?.divisions || []).map(d => d.name);
    // Minstens één divisie moet zichtbaar blijven, anders is het externe
    // scherm leeg (de backend zou een lege lijst als "alles" lezen).
    if (isRemoteDivisionOn(name) && namen.filter(n => isRemoteDivisionOn(n)).length <= 1) {
      if (ev && ev.target) ev.target.checked = true; // vinkje terugzetten
      return;
    }
    remoteDivisions[name] = !isRemoteDivisionOn(name);
    remoteDivisions = remoteDivisions;
    publishRemoteLayout();
  }

  function setRemotePart(key, on) {
    remoteParts = { ...remoteParts, [key]: !!on };
    publishRemoteLayout();
  }

  async function publishRemoteLayout() {
    if (secondary || !remoteLayoutReady || !organInfo) return;
    const divs = displayOrgan?.divisions || [];
    const stop_order = {};
    for (const d of divs) stop_order[d.name] = (d.stops || []).map(s => s.id);
    const layout = {
      divisions: divs.map(d => d.name).filter(n => isRemoteDivisionOn(n)),
      visible_couplers: (displayOrgan?.couplers || []).filter(c => isCouplerVisible(c.id)).map(c => c.id),
      coupler_placement: couplerPlacement,
      stop_order,
      knob_shape: knobShape,
      show_couplers: remoteParts.couplers !== false,
      show_tremulant: remoteParts.tremulant !== false,
      show_setzer: remoteParts.setzer !== false,
      show_volume: remoteParts.volume !== false,
      show_panic: remoteParts.panic !== false,
    };
    const json = JSON.stringify(layout);
    if (json === lastRemoteLayoutJson) return;
    lastRemoteLayoutJson = json;
    try {
      await invoke('set_remote_layout', { layout });
      // Bewaren loopt via het bestaande (gedebouncede) autosave-pad in App.svelte.
      dispatch('refreshMidiMappings');
    } catch (e) { console.warn('set_remote_layout:', e); }
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
    if (!secondary) {
      // Volume-event van de afstandsbediening: slider volgt zonder echo naar de
      // backend (die heeft master_volume_db al; dispatch('volumeChange') niet nodig).
      (async () => {
        try {
          unlistenRemoteVol = await listen('jm-orgue:remote-master-volume', (e) => {
            const db = e.payload && e.payload.db;
            if (typeof db === 'number') volume = db;
          });
        } catch (e) {}
      })();
      refreshRemote();
    }
  });

  onDestroy(() => {
    window.removeEventListener('click', handleGlobalClick);
    if (swellPollInterval) clearInterval(swellPollInterval);
    if (recorderPoll) clearInterval(recorderPoll);
    if (midiRecPoll) clearInterval(midiRecPoll);
    if (archivePoll) clearInterval(archivePoll);
    if (crescLivePoll) clearInterval(crescLivePoll);
    if (sharedPrefsInterval) clearInterval(sharedPrefsInterval);
    if (unlistenRemoteVol) unlistenRemoteVol();
    if (libraryUpdatedListener) libraryUpdatedListener.then(un => { if (un) un(); });
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
        // Houd de bewerk-balk in sync met de werkelijke (pedaal-gestuurde) stand
        // én de aan/uit-stand (piston/paneel) — zonder terug te schrijven.
        crescendoStage = r[1];
        if (r[0] !== crescendoEnabled && Date.now() >= crescDirtyUntil) crescendoEnabled = r[0];
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
    publishRemoteLayout();
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
      // Panelen laten weten dat dít programmatisch sluiten is (orgelwissel/
      // afsluiten) — anders zou hun kruisje-handler de hele app afsluiten (0.7.21).
      try {
        const { emit } = await import('@tauri-apps/api/event');
        await emit('jm-orgue:panels-closing');
      } catch (e) {}
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
      // Fysieke geometrie (px/py/pw/ph, 0.7.21) wordt NA aanmaak toegepast via
      // setPosition/setSize — de config-x/y zijn logisch en misplaatsten
      // vensters op een tweede beeldscherm met andere schaal. Oude logische
      // opslag (x/y/width/height) dient alleen nog als beginschatting.
      const hasPhysical = st && typeof st.px === 'number';
      const webview = new WebviewWindow(label, {
        url: `index.html#panel&n=${screenNum}`,
        title: tx('dialogs.panel_window_title').replace('{n}', String(screenNum)),
        width: (!hasPhysical && st && st.width >= 200) ? Math.min(st.width, 4000) : 700,
        height: (!hasPhysical && st && st.height >= 150) ? Math.min(st.height, 4000) : 500,
        decorations: true,
        resizable: true,
        center: false,
        x: (!hasPhysical && st && typeof st.x === 'number') ? st.x : 100 + (offset * 30),
        y: (!hasPhysical && st && typeof st.y === 'number') ? st.y : 100 + (offset * 30),
      });
      if (hasPhysical || (st && st.maximized)) {
        webview.once('tauri://created', async () => {
          try {
            const { PhysicalPosition, PhysicalSize } = await import('@tauri-apps/api/dpi');
            if (st.maximized) {
              // Eerst op het scherm van de maximalisatie zetten (anker uit de
              // maximized-save of de windowed-terugvalpositie), dán maximaliseren
              // — maximize() pakt het scherm waar het venster op dat moment staat.
              const ax = (typeof st.mpx === 'number') ? st.mpx : (typeof st.px === 'number' ? st.px : null);
              const ay = (typeof st.mpy === 'number') ? st.mpy : (typeof st.py === 'number' ? st.py : null);
              if (ax !== null && ay !== null && ax > -30000 && ay > -30000) {
                await webview.setPosition(new PhysicalPosition(ax + 64, ay + 64));
              }
              await webview.maximize();
              return;
            }
            if (st.px > -30000 && st.py > -30000) {
              await webview.setPosition(new PhysicalPosition(st.px, st.py));
            }
            if (st.pw >= 200 && st.ph >= 150) {
              const apply = () => webview.setSize(new PhysicalSize(Math.min(st.pw, 16000), Math.min(st.ph, 16000)));
              await apply();
              // Cross-DPI-controle (zie App.svelte): asynchrone herschaling bij
              // verhuizing naar een anders-geschaald scherm kan de maat
              // overschrijven — één keer verifiëren en zonodig opnieuw zetten.
              setTimeout(async () => {
                try {
                  const cur = await webview.innerSize();
                  if (Math.abs(cur.width - st.pw) > 4 || Math.abs(cur.height - st.ph) > 4) await apply();
                } catch (e) {}
              }, 250);
            }
          } catch (e) { /* venster blijft dan op de beginschatting staan */ }
        });
      }
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
          name: tx('dialogs.organ_definition_filter'),
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

  // ---- Online samplesets (bibliotheek-downloads) ----
  // Manifest in onze eigen repo; raw.githubusercontent.com staat CORS toe
  // (zelfde patroon als de update-check). Faalt stil: geen internet = geen
  // downloadkaarten, de bibliotheek werkt gewoon door.
  const SAMPLESETS_MANIFEST = 'https://raw.githubusercontent.com/orgelmaker/JM-Orgue/master/samplesets.json';
  let onlineSets = [];
  let onlineBusy = null;   // id van de lopende download
  let onlinePct = 0;
  let onlineFase = 'download';
  // Mapnaam van een bibliotheek-entry: bij een .organ de map waarin het
  // bestand staat, bij een sample-map de map zelf. De naam van een set kan
  // veranderen (JM-Rec-sets heten nu "Kerk - Bouwer - Plaats"), de mapnaam
  // niet — daarom bepaalt die of een online set al geïnstalleerd is.
  function setFolderKey(o) {
    const delen = String(o?.source_path || '').replace(/\\/g, '/').split('/').filter(Boolean);
    if (o?.source_type === 'organ_file') delen.pop();
    return (delen.pop() || '').toLowerCase();
  }

  // De opstart-splash in App.svelte wacht op deze eerste ronde (zie
  // onlineSetsSettled daar), zodat het blok "Online beschikbaar" er meteen
  // staat in plaats van ná het startscherm in te ploffen. Eén melding per
  // venster: latere verversingen (na een download of verwijderen) tellen niet.
  let onlineSetsGemeld = false;
  function meldOnlineSetsKlaar() {
    if (onlineSetsGemeld || secondary) return;
    onlineSetsGemeld = true;
    dispatch('onlineSetsSettled');
  }

  async function refreshOnlineSets() {
    try {
      const res = await fetch(SAMPLESETS_MANIFEST, { cache: 'no-cache' });
      if (!res.ok) { onlineSets = []; return; }
      const mf = await res.json();
      const geinstalleerd = new Set();
      for (const o of (libraryOrgans || [])) {
        if (o?.name) geinstalleerd.add(String(o.name).toLowerCase());
        const map = setFolderKey(o);
        if (map) geinstalleerd.add(map);
      }
      onlineSets = (mf.samplesets || []).filter(s =>
        s.naam && s.zip_url
        && !geinstalleerd.has(String(s.naam).toLowerCase())
        && !geinstalleerd.has(String(s.map_naam || s.naam).toLowerCase())
      );
    } catch (e) { onlineSets = []; }
    finally { meldOnlineSetsKlaar(); }
  }
  async function downloadOnlineSet(s) {
    if (onlineBusy) return;
    let un = null;
    try {
      const basis = await open({ multiple: false, directory: true, title: tx('library.download_where') });
      if (!basis) return;
      onlineBusy = s.id; onlinePct = 0; onlineFase = 'download';
      const { listen } = await import('@tauri-apps/api/event');
      un = await listen('jm-orgue:sampleset-download', (e) => {
        if (e.payload?.id === s.id) { onlinePct = e.payload.pct || 0; onlineFase = e.payload.fase || 'download'; }
      });
      const pad = await invoke('download_sampleset', {
        id: s.id, url: s.zip_url, doelMap: `${basis}/${s.map_naam || s.naam}`,
      });
      // Zelfde route als "Scan map": laadt het orgel en zet het in de bibliotheek.
      dispatch('scanFolder', pad);
    } catch (e) {
      alert(`${tx('library.download_failed')}: ${e}`);
    } finally {
      if (un) un();
      onlineBusy = null;
      refreshOnlineSets();
    }
  }

  async function openExternalSampleset() {
    try {
      const selected = await open({
        multiple: false,
        directory: true,
        title: tx('dialogs.external_sampleset_title')
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
        title: tx('dialogs.export_source_folder_title')
      });
      if (!sourceDir) return;

      // Then select output file
      const { save } = await import('@tauri-apps/plugin-dialog');
      const outputPath = await save({
        title: tx('dialogs.export_organ_save_title'),
        defaultPath: 'orgel.organ',
        filters: [{ name: tx('dialogs.organ_definition'), extensions: ['organ'] }]
      });
      if (!outputPath) return;

      await invoke('export_organ_file', { directory: sourceDir, outputPath });
      alert(tx('dialogs.export_organ_success'));
    } catch (e) {
      console.error('Export failed:', e);
      alert(tx('dialogs.export_failed').replace('{error}', String(e)));
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
      alert(tx('midi.connect_first'));
      return;
    }
    learningDivision = divisionName;
    learningStep = 1;
    kbLearnModal = { division: divisionName, step: 1, first: null, msg: '' };
    try {
      const first = await invoke('learn_keyboard_note');
      if (!kbLearnModal || kbLearnModal.division !== divisionName) return; // geannuleerd
      if (!first) {
        kbLearnModal = { ...kbLearnModal, step: 'fout', msg: tx('learn.key_timeout_first') };
        return;
      }
      kbLearnModal = { ...kbLearnModal, step: 2, first: { channel: first[0], note: first[1] } };
      learningStep = 2;
      const second = await invoke('learn_keyboard_note');
      if (!kbLearnModal || kbLearnModal.division !== divisionName) return;
      if (!second) {
        kbLearnModal = { ...kbLearnModal, step: 'fout', msg: tx('learn.key_timeout_second') };
        return;
      }
      if (second[0] !== kbLearnModal.first.channel) {
        kbLearnModal = { ...kbLearnModal, step: 'fout',
          msg: tx('learn.key_channel_mismatch').replace('{second}', String(second[0] + 1)).replace('{first}', String(kbLearnModal.first.channel + 1)) };
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
        msg: tx('learn.key_result').replace('{low}', noteName(res.first_note)).replace('{high}', noteName(res.last_note)).replace('{channel}', String(res.channel + 1)) };
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

  // "bouwer · plaats" voor onder de naam op de kaart. Lege regel als er
  // niets zinnigs staat: een gescande sample-map heeft bouwer "Custom Samples"
  // en als plaats het mappad zelf.
  function cardSub(o) {
    const bouwer = (o?.builder && o.builder !== 'Custom Samples') ? o.builder : '';
    const plaats = (o?.location && o.location !== o.source_path) ? o.location : '';
    return [bouwer, plaats].filter(Boolean).join(' · ');
  }

  async function loadLibrary() {
    try {
      // Luisteraar vóór de eerste invoke, zodat de melding van de
      // achtergrondopfris nooit gemist wordt.
      if (!libraryUpdatedListener) {
        libraryUpdatedListener = listen('jm-orgue:library-updated', () => { if (showOrganBrowser) loadLibrary(); })
          .catch(() => null);
      }
      await libraryUpdatedListener;
      libraryOrgans = await invoke('get_organ_library');
      // Afbeelding per orgel; de backend gebruikt de opgeslagen afbeelding en
      // zoekt alleen als er nog nooit (met deze zoekversie) gezocht is.
      for (const organ of libraryOrgans) {
        if (!organImages[organ.id]) {
          invoke('get_organ_image', { id: organ.id }).then(img => {
            if (img) {
              organImages[organ.id] = img;
              organImages = organImages; // trigger reactivity
            }
          }).catch(() => {});
        }
      }
      // Online-lijst pas ná de bibliotheek: het "al geïnstalleerd"-filter leest
      // libraryOrgans, en als losse reactive statement was die volgorde toeval.
      if (!secondary) refreshOnlineSets();
    } catch (e) {
      console.error('Failed to load organ library:', e);
      // Ook dán de splash vrijgeven: zonder bibliotheek komt refreshOnlineSets
      // niet aan bod en zou de opstart op de time-out moeten wachten.
      meldOnlineSetsKlaar();
    }
  }

  // Handmatig een foto van het orgel kiezen (bibliotheekkaart). De backend
  // kopieert het bestand naar de app-datamap en zet image_manual, zodat
  // automatisch zoeken de keuze niet overschrijft.
  async function chooseOrganImage(e, organ) {
    e.stopPropagation();
    try {
      const gekozen = await open({
        multiple: false,
        title: tx('library.choose_image_title'),
        filters: [{ name: tx('dialogs.image_filter'), extensions: ['jpg', 'jpeg', 'png', 'bmp', 'webp'] }],
      });
      if (!gekozen) return;
      await invoke('set_organ_image', { id: organ.id, path: gekozen });
      delete organImages[organ.id];
      organImages = organImages;
      await loadLibrary();
    } catch (err) {
      console.error('Afbeelding instellen mislukt:', err);
      alert(`${tx('library.image_failed')}: ${err}`);
    }
  }

  // Terug naar automatisch zoeken (wist de handmatige keuze).
  async function autoOrganImage(e, organ) {
    e.stopPropagation();
    try {
      await invoke('set_organ_image', { id: organ.id, path: null });
      delete organImages[organ.id];
      organImages = organImages;
      await loadLibrary();
    } catch (err) {
      console.error('Automatisch zoeken mislukt:', err);
      alert(`${tx('library.image_failed')}: ${err}`);
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

  function midiActionLabel(code, _t = $t) {
    if (code >= 150) {
      for (const d of displayOrgan?.divisions || []) {
        const s = (d.stops || []).find(s => s.midi_action_code === code);
        if (s) return _t('bindings.stop_label').replace('{name}', `${s.name}${s.pitch ? ' ' + s.pitch : ''}`).replace('{division}', d.display_name || d.name);
      }
      return _t('bindings.stop_code').replace('{code}', String(code));
    }
    if (code >= 100) {
      const c = (displayOrgan?.couplers || []).find(c => c.midi_action_code === code);
      return c ? _t('bindings.coupler_label').replace('{name}', c.name.replace(/\n/g, ' ')) : _t('bindings.coupler_code').replace('{code}', String(code));
    }
    if (code >= ACTION_TREM_LFO_BASE && code < ACTION_TREM_LFO_BASE + 16) {
      const d = displayOrgan?.divisions?.[code - ACTION_TREM_LFO_BASE];
      const divName = d ? (d.display_name || d.name) : _t('bindings.division_n').replace('{n}', String(code - ACTION_TREM_LFO_BASE + 1));
      return _t('bindings.tremulant_label').replace('{name}', divName);
    }
    const vast = {
      10: _t('bindings.setzer_set'), 11: _t('bindings.setzer_gc'), 12: _t('bindings.setzer_prev'), 13: _t('bindings.setzer_next'),
      14: _t('bindings.setzer_minus_10'), 15: _t('bindings.setzer_plus_10'),
      40: _t('pistons.eq_toggle'), 41: _t('pistons.crescendo_toggle'), 42: _t('pistons.shutdown'),
      43: _t('pistons.audio_profile'),
    };
    if (vast[code] != null) return vast[code];
    if (code <= 9) return _t('bindings.setzer_button').replace('{n}', String(code));
    if (code >= 16 && code <= 23) return _t('bindings.setzer_memory').replace('{n}', String(code - 15));
    return _t('bindings.action_n').replace('{n}', String(code));
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
    if (b.trigger_type === 'note') return b.note;
    if (b.trigger_type === 'cc' || b.trigger_type === 'ccbit') return b.controller;
    if (b.trigger_type === 'sysex') return 0;
    return b.program;
  }

  function buildSavedBinding(presetNum, type, channelDisplay, number, value, sysexHex) {
    const ch = (channelDisplay === '' || channelDisplay == null) ? null : Math.max(1, Math.min(16, channelDisplay | 0)) - 1;
    const num = Math.max(0, Math.min(127, number | 0));
    const b = { preset_num: presetNum, trigger_type: type, note: null, channel: null, controller: null, value: null, program: null, sysex_hex: null, bit: null };
    if (type === 'note') b.note = num;
    else if (type === 'cc') { b.channel = ch; b.controller = num; b.value = Math.max(0, Math.min(127, value | 0)); }
    else if (type === 'ccbit') { b.channel = ch; b.controller = num; b.bit = Math.max(0, Math.min(6, value | 0)); }
    else if (type === 'sysex') { b.sysex_hex = sysexHex ?? ''; }
    else { b.channel = ch; b.program = num; }
    return b;
  }

  async function updateBindingRow(row, patch = {}) {
    const type = patch.type ?? row.trigger_type;
    const chDisp = patch.channel !== undefined ? patch.channel : (row.channel != null ? row.channel + 1 : '');
    const num = patch.number !== undefined ? patch.number : (bindingNumber(row) ?? 0);
    // Bij een bitveld staat het bitnummer in dezelfde kolom als de CC-drempel.
    const val = patch.value !== undefined ? patch.value
      : (type === 'ccbit' ? (row.bit ?? 0) : (row.value ?? 64));
    const hex = patch.sysexHex !== undefined ? patch.sysexHex : (row.sysex_hex ?? '');
    const binding = buildSavedBinding(row.preset_num, type, chDisp, num, val, hex);
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
      midiBindingsError = tx('bindings.choose_target_first');
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
    // Pas ná de Svelte-update publiceren: publishRemoteLayout leest de volgorde
    // uit displayOrgan, en dat is vlak na een mutatie van stopOrder nog de
    // vorige waarde — de afstandsbediening liep daardoor precies één
    // verplaatsing achter.
    tick().then(publishRemoteLayout);
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

  // Twee sleepmodi (feedback professionele tester, GrandOrgue/Hauptwerk-
  // conventie): gewoon slepen "schildert" registers aan of uit — de stand van
  // de eerste knop bepaalt of de knoppen eronder AAN of UIT gaan. Alleen
  // Shift+slepen hersorteert nog (de sorteerlijst in Instellingen blijft).
  function knobPointerDown(e, divName, idx) {
    if (e.pointerType !== 'mouse' || e.button !== 0) return; // alleen linkermuisknop
    const mode = e.shiftKey ? 'reorder' : 'paint';
    const organ = displayOrgan || organInfo || demoOrgan;
    const first = organ?.divisions?.find(d => d.name === divName)?.stops?.[idx];
    knobDrag = {
      div: divName, fromIdx: idx, startX: e.clientX, startY: e.clientY, active: false, overIdx: null,
      mode, target: first ? !first.drawn : true, firstId: first?.id ?? null, painted: new Set(),
    };
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
      if (knobDrag.mode === 'paint' && knobDrag.firstId) {
        // De click op de eerste knop wordt straks onderdrukt (knobDragJustEnded);
        // die eerste knop hier zelf in de doelstand zetten.
        paintStop(knobDrag.firstId, knobDrag.target);
      }
    }
    // Doelpositie via hit-test onder de cursor — werkt in horizontale én verticale layout.
    const el = document.elementFromPoint(e.clientX, e.clientY);
    const hit = el && el.closest ? el.closest('[data-knob-div]') : null;
    if (knobDrag.mode === 'paint') {
      // Schilderen mag over divisiegrenzen heen: elke knop onder de cursor
      // krijgt de doelstand, één keer per sleep.
      if (hit) {
        const organ = displayOrgan || organInfo || demoOrgan;
        const st = organ?.divisions?.find(d => d.name === hit.dataset.knobDiv)?.stops?.[parseInt(hit.dataset.knobIdx, 10)];
        if (st) paintStop(st.id, knobDrag.target);
      }
      return;
    }
    if (hit && hit.dataset.knobDiv === knobDrag.div) {
      knobDrag.overIdx = parseInt(hit.dataset.knobIdx, 10);
    } else {
      knobDrag.overIdx = null; // buiten de eigen divisie → geen geldig doel
    }
    knobDrag = knobDrag; // reactieve update voor de visuele feedback
  }

  // Zet een register in de gevraagde stand (één keer per sleep). De stand
  // wordt uit de huidige orgeldata gelezen; toggleStop draait hem om.
  function paintStop(stopId, target) {
    if (!knobDrag || knobDrag.painted.has(stopId)) return;
    knobDrag.painted.add(stopId);
    const organ = displayOrgan || organInfo || demoOrgan;
    let cur = null;
    for (const d of (organ?.divisions || [])) {
      const f = (d.stops || []).find(x => x.id === stopId);
      if (f) { cur = f; break; }
    }
    if (cur && !!cur.drawn !== !!target) dispatch('toggleStop', stopId);
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
    if (d.mode !== 'reorder') return; // schilderen is al tijdens het slepen gebeurd
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
    // Niets publiceren naar de afstandsbediening tot de opgeslagen indeling van
    // DIT orgel geladen is (loadAudioSettingsForOrgan zet de vlag weer aan).
    remoteLayoutReady = false;
    // Secundaire vensters laden alleen de UI-state (geen backend-pushes).
    loadAudioSettingsForOrgan(!secondary);
    loadUiPrefsForOrgan();
  }

  // Bibliotheek laden zodra het startscherm getoond wordt. Alleen in het
  // hoofdvenster: een extra registerscherm toont de bibliotheek hooguit een
  // oogwenk (tot organInfo binnen is) en zou anders per venster alle
  // kaartafbeeldingen opnieuw ophalen.
  $: if (showOrganBrowser && !secondary) { loadLibrary(); }

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
        <div class="organ-browser-titlegroup">
          <h1 class="organ-browser-title">{$t('library.title')}</h1>
          <!-- Taal meteen op het startscherm: dezelfde store en dezelfde
               setLocale als de taalkaarten in Algemene Instellingen, dus beide
               plekken lopen vanzelf in de pas (bewaard in localStorage). -->
          <div class="lang-switch" role="group" aria-label={$t('settings.language')}>
            {#each AVAILABLE_LOCALES as code}
              <button
                type="button"
                class="btn btn-secondary btn-sm"
                class:active={$locale === code}
                aria-pressed={$locale === code}
                title={LOCALE_LABELS[code]}
                on:click={() => setLocale(code)}
              >{code.toUpperCase()}</button>
            {/each}
          </div>
        </div>
        <div class="organ-browser-actions">
          <button class="btn btn-secondary btn-sm" on:click={openOrganFile} title={$t('library.open_organ_file')}>
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
            <div class="library-card-actions">
              {#if organ.image_manual}
                <button
                  class="library-card-btn"
                  on:click={(e) => autoOrganImage(e, organ)}
                  title={$t('library.image_auto')}
                  aria-label={$t('library.image_auto')}
                >
                  <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2">
                    <polyline points="1 4 1 10 7 10"/>
                    <path d="M3.51 15a9 9 0 1 0 2.13-9.36L1 10"/>
                  </svg>
                </button>
              {/if}
              <button
                class="library-card-btn"
                on:click={(e) => chooseOrganImage(e, organ)}
                title={$t('library.choose_image')}
                aria-label={$t('library.choose_image')}
              >
                <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                  <path d="M23 19a2 2 0 0 1-2 2H3a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h4l2-3h6l2 3h4a2 2 0 0 1 2 2z"/>
                  <circle cx="12" cy="13" r="4"/>
                </svg>
              </button>
              <button
                class="library-card-delete library-card-btn"
                on:click={(e) => removeFromLibrary(e, organ.id)}
                title={$t('library.remove_from_library')}
              >&times;</button>
            </div>
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
                <!-- Geen foto gevonden: hier zelf een foto kiezen. -->
                <button class="library-card-pick" on:click={(e) => chooseOrganImage(e, organ)}>
                  {$t('library.choose_image')}
                </button>
              {/if}
            </div>
            <div class="library-card-info">
              <div class="library-card-name">{organ.name}</div>
              <!-- "bouwer · plaats" onder de naam. Een gescande sample-map zonder
                   JM-Rec-manifest heeft geen echte bouwer/plaats (bouwer =
                   "Custom Samples", plaats = het mappad) — dan niets tonen. -->
              {#if cardSub(organ)}
                <div class="library-card-sub">{cardSub(organ)}</div>
              {/if}
              <div class="library-card-meta">{$t('library.stops_count').replace('{count}', organ.stop_count)}</div>
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

      <!-- Online beschikbare samplesets (manifest in de repo); verdwijnen uit
           deze lijst zodra ze geïnstalleerd zijn. -->
      {#if onlineSets.length > 0}
        <h2 class="organ-browser-subtitle">{$t('library.online_title')}</h2>
        <div class="organ-library-grid">
          {#each onlineSets as s (s.id)}
            <div
              class="library-card"
              class:library-card-busy={onlineBusy === s.id}
              on:click={() => downloadOnlineSet(s)}
              on:keypress={(e) => e.key === 'Enter' && downloadOnlineSet(s)}
              role="button"
              tabindex="0"
            >
              <div class="library-card-image">
                <svg width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" opacity="0.55">
                  <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/>
                  <polyline points="7 10 12 15 17 10"/>
                  <line x1="12" y1="15" x2="12" y2="3"/>
                </svg>
              </div>
              <div class="library-card-info">
                <div class="library-card-name">{s.naam}</div>
                <div class="library-card-meta">
                  {#if onlineBusy === s.id}
                    {onlineFase === 'uitpakken' ? $t('library.unpacking') : $t('library.downloading')} {onlinePct}%
                  {:else}
                    {$t('library.download_size').replace('{mb}', s.zip_mb || '?')}
                  {/if}
                </div>
                {#if s.beschrijving}
                  <div class="library-card-meta">{s.beschrijving}</div>
                {/if}
              </div>
            </div>
          {/each}
        </div>
      {/if}
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
            <div class="stop-size-control" title={$t('toolbar.size_title')}>
              <button class="btn btn-ghost btn-sm stop-size-btn" on:click={() => adjustStopSize(-15)} aria-label={$t('toolbar.size_smaller')}>−</button>
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="3" y="6" width="18" height="12" rx="2"/></svg>
              <button class="btn btn-ghost btn-sm stop-size-btn" on:click={() => adjustStopSize(15)} aria-label={$t('toolbar.size_larger')}>+</button>
            </div>
            <button
              class="btn btn-ghost btn-sm panel-layout-btn"
              on:click={toggleMainLayout}
              title={mainLayout === 'horizontal' ? $t('toolbar.layout_title_vertical') : $t('toolbar.layout_title_horizontal')}
            >
              {#if mainLayout === 'horizontal'}
                <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                  <rect x="3" y="3" width="7" height="18" rx="1"/>
                  <rect x="14" y="3" width="7" height="18" rx="1"/>
                </svg>
                {$t('toolbar.vertical')}
              {:else}
                <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                  <rect x="3" y="3" width="18" height="7" rx="1"/>
                  <rect x="3" y="14" width="18" height="7" rx="1"/>
                </svg>
                {$t('toolbar.horizontal')}
              {/if}
            </button>
            <button
              class="btn btn-ghost btn-sm panel-layout-btn"
              on:click={toggleKnobShape}
              title={knobShape === 'round' ? $t('toolbar.shape_title_rect') : $t('toolbar.shape_title_round')}
            >
              {#if knobShape === 'round'}
                <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                  <rect x="3" y="6" width="18" height="12" rx="2"/>
                </svg>
                {$t('toolbar.rect')}
              {:else}
                <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                  <circle cx="12" cy="12" r="9"/>
                </svg>
                {$t('toolbar.round')}
              {/if}
            </button>
            {#if displayOrgan.couplers && displayOrgan.couplers.length > 0}
              <button
                class="btn btn-ghost btn-sm panel-layout-btn"
                on:click={toggleCouplerPlacement}
                title={couplerPlacement === 'bar' ? $t('toolbar.couplers_to_division') : $t('toolbar.couplers_to_bar')}
              >
                <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                  <path d="M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71"/>
                  <path d="M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71"/>
                </svg>
                {$t('toolbar.couplers')}
              </button>
            {/if}
            {#if secondary && showPanelHeaderToggle}
              <!-- Hoofdbalk (tabs/profielwissel/start-stop) tonen op dit scherm -->
              <button
                class="btn btn-ghost btn-sm panel-layout-btn"
                on:click={() => dispatch('showHeaderRequest')}
                title={$t('toolbar.bar_title')}
              >
                <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                  <polyline points="6 9 12 15 18 9"/>
                </svg>
                {$t('toolbar.bar')}
              </button>
            {/if}
            <button class="btn btn-ghost btn-sm panel-add-btn" on:click={() => openExtraWindow()}>
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <rect x="2" y="3" width="20" height="14" rx="2"/>
                <line x1="8" y1="21" x2="16" y2="21"/>
                <line x1="12" y1="17" x2="12" y2="21"/>
              </svg>
              {$t('toolbar.new_screen')}
            </button>
            <button
              class="btn btn-ghost btn-sm record-btn"
              class:recording={recorderStatus.recording}
              on:click={toggleRecording}
              title={recorderStatus.recording
                ? $t('toolbar.rec_audio_running').replace('{t}', formatSeconds(recorderStatus.seconds))
                : $t('toolbar.rec_audio_start')}
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
                ? $t('toolbar.rec_midi_running').replace('{n}', midiRec.event_count).replace('{t}', formatSeconds(midiRec.seconds))
                : $t('toolbar.rec_midi_start')}
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
                title={$t('toolbar.play_recording_title')}
              >
                <svg width="13" height="13" viewBox="0 0 24 24" fill="currentColor">
                  <polygon points="6,4 20,12 6,20"/>
                </svg>
                {$t('toolbar.play_recording')}
              </button>
            {/if}
            {#if !midiRec.recording}
              <button
                class="btn btn-ghost btn-sm"
                on:click={openLiveNotation}
                title={$t('toolbar.notate_title')}
              >
                <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                  <circle cx="7" cy="18" r="3"/>
                  <path d="M10 18V5l8-2v12"/>
                  <circle cx="15" cy="15" r="3"/>
                </svg>
                {$t('toolbar.notate')}
              </button>
            {/if}
            <button
              class="btn btn-ghost btn-sm shutdown-btn"
              on:click={() => dispatch('shutdownRequest')}
              use:midiLearn={{ onTrigger: () => dispatch('shutdownLearn') }}
              title={$t('toolbar.shutdown_title')}
            >
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <path d="M18.36 6.64a9 9 0 1 1-12.73 0"/>
                <line x1="12" y1="2" x2="12" y2="12"/>
              </svg>
              {$t('toolbar.shutdown')}
            </button>
          </div>
        </div>

        <!-- Single register view -->
        <!-- --stop-min-width op de container zodat ook .division (min-width in
             verticale modus) de ingestelde knopgrootte kan gebruiken;
             --stop-size-n is hetzelfde getal zonder eenheid (CSS-calc kan niet
             door een lengte delen) voor de vensterbreedte-schaling in styles.css -->
        <div class="divisions-container" class:divisions-vertical={mainLayout === 'vertical'} style="--stop-min-width: {stopSize}px; --stop-size-n: {stopSize}">
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
                        class:active={i < Math.round((divisionVolumes[division.name] ?? 1.0) * 10)}
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
                class:knob-dragging={!!(knobDrag && knobDrag.active && knobDrag.mode === 'reorder' && knobDrag.div === division.name)}
                style="--stop-min-width: {stopSize}px; --stop-size-n: {stopSize}"
              >
                {#each division.stops as stop, stopIdx}
                  {@const name = cleanStopName(stop)}
                  <button
                    class="stop-knob {getStopClass(stop)}"
                    class:engaged={stop.drawn}
                    class:has-midi={stopMidiBindings[stop.midi_action_code] > 0}
                    class:dragging={!!(knobDrag && knobDrag.active && knobDrag.mode === 'reorder' && knobDrag.div === division.name && knobDrag.fromIdx === stopIdx)}
                    class:drag-target={!!(knobDrag && knobDrag.active && knobDrag.mode === 'reorder' && knobDrag.div === division.name && knobDrag.overIdx === stopIdx && knobDrag.fromIdx !== stopIdx)}
                    data-knob-div={division.name}
                    data-knob-idx={stopIdx}
                    on:pointerdown={(e) => knobPointerDown(e, division.name, stopIdx)}
                    on:click={() => { if (knobDragJustEnded) return; dispatch('toggleStop', stop.id); }}
                    use:midiLearn={{ onTrigger: () => showStopContextMenuAt(stop.midi_action_code) }}
                    aria-label="{stop.name} {stop.pitch || ''} — {stop.drawn ? $t('stops.drawn') : $t('stops.not_drawn')}"
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
                    aria-label="{$t('stops.tremulant')} {division.name} — {tremActive[division.name] === true ? $t('stops.drawn') : $t('stops.not_drawn')}"
                    aria-pressed={tremActive[division.name] === true}
                  >
                    <span class="stop-name">{$t('stops.tremulant')}</span>
                  </button>
                {/if}
                <!-- Koppelknoppen (alleen zichtbare) -->
                {#if displayOrgan.couplers}
                  {#each displayOrgan.couplers.filter(c => couplerPlacement === 'division' && c.display_in_division === division.name && isCouplerVisible(c.id)) as coupler}
                    <button
                      class="stop-knob coupler coupler-knob"
                      class:engaged={coupler.active}
                      class:has-midi={couplerMidiBindings[coupler.midi_action_code] > 0}
                      on:click={() => dispatch('toggleCoupler', coupler.id)}
                      use:midiLearn={{ onTrigger: () => showCouplerContextMenuAt(coupler.midi_action_code) }}
                      title={coupler.name.replace(/\n/g, ' ')}
                      aria-label="{coupler.name.replace(/\n/g, ' ')} — {coupler.active ? $t('stops.drawn') : $t('stops.not_drawn')}"
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
        <!-- Koppelbalk: alle zichtbare koppels gegroepeerd per klavier -->
        {#if couplerBarGroups.length > 0}
          <div class="coupler-bar" role="group" aria-label={$t('couplers.title')}>
            {#each couplerBarGroups as grp (grp.division)}
              <div class="coupler-bar-group">
                <span class="coupler-bar-title">{grp.division}</span>
                {#each grp.couplers as coupler (coupler.id)}
                  <button
                    class="coupler-bar-btn"
                    class:engaged={coupler.active}
                    class:has-midi={couplerMidiBindings[coupler.midi_action_code] > 0}
                    on:click={() => dispatch('toggleCoupler', coupler.id)}
                    use:midiLearn={{ onTrigger: () => showCouplerContextMenuAt(coupler.midi_action_code) }}
                    title={couplerLabel(coupler)}
                    aria-pressed={coupler.active}
                  >{couplerLabel(coupler)}</button>
                {/each}
              </div>
            {/each}
          </div>
        {/if}
        <!-- In secundaire vensters consumeert de SetzerBar GEEN MIDI-triggers:
             alleen het hoofdvenster leest het (consumerende) trigger-kanaal,
             anders worden presets/acties dubbel toegepast. -->
        <SetzerBar
          organInfo={displayOrgan}
          consumeMidiTriggers={!secondary}
          on:externalAction={(e) => handleExternalAction(e.detail.actionCode)}
          on:learnCrescendo={() => learnPedalFlow('crescendo')}
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
                  <span class="slider-label">{$t('settings.master_volume')}</span>
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
                  <span class="slider-label">{$t('settings.temperament')}</span>
                </div>
                <select
                  class="temperament-select"
                  bind:value={selectedTemperament}
                  on:change={onTemperamentChange}
                >
                  {#each temperaments as tp, i}
                    <option value={i}>{tp.names?.[$locale] ?? tp.nameDutch ?? tp.name}</option>
                  {/each}
                </select>
                <!-- Hertemperen op gemeten pijptoonhoogte: indicator + uitleg per modus -->
                <div class="temperament-hint">
                  {#if organInfo?.retune_pipes > 0}
                    {$t('settings.temperament_measured')
                      .replace('{n}', Number(organInfo.retune_pipes).toLocaleString($locale))
                      .replace('{total}', Number(organInfo.retune_total).toLocaleString($locale))}
                  {:else}
                    {$t('settings.temperament_no_measured')}
                  {/if}
                  <br/>
                  {temperaments[selectedTemperament]?.original ? $t('settings.temperament_original_hint') : $t('settings.temperament_retune_hint')}
                </div>
              </div>
              <div class="slider-control">
                <div class="slider-header">
                  <span class="slider-label">{$t('settings.temperament_fine_tune')}</span>
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
                {showCustomTemperament ? $t('settings.temperament_custom_close') : $t('tuning.custom_open')}
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
                    {$t('tuning.save_as_preset')}
                  </button>
                </div>
              {/if}
            </div>

            <!-- Reverb -->
            <div class="settings-block">
              <h3 class="settings-block-title">{$t('settings.reverb')}</h3>
              {#if natteSampleset}
                <!-- De set heeft zijn eigen akoestiek; kunstmatige galm is hier
                     smaak, geen noodzaak. Eén knop om alles weer af te zetten. -->
                <p class="settings-hint" style="margin: 0 0 0.4rem;">
                  {$t('reverb.recorded_room_hint').replace('{n}', organInfo.release_pipes)}
                </p>
                {#if reverb > 0}
                  <button class="btn btn-sm" style="margin-bottom: 0.5rem;" on:click={reverbTerugNaarOpname}>
                    {$t('reverb.back_to_recording')}
                  </button>
                {/if}
              {/if}
              <!-- Type selector -->
              <div class="reverb-type-selector">
                <button class="btn btn-sm" class:btn-active={reverbType === 'convolution'} on:click={() => switchReverbType('convolution')}>{$t('settings.reverb_type_convolution')}</button>
                <button class="btn btn-sm" class:btn-active={reverbType === 'algorithmic'} on:click={() => switchReverbType('algorithmic')}>{$t('settings.reverb_type_algorithmic')}</button>
              </div>

              <!-- Mix slider (shared) -->
              <div class="slider-control">
                <div class="slider-header">
                  <span class="slider-label">{$t('settings.reverb_mix')}</span>
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
                    {reverbIrLoaded ? $t('settings.reverb_ir_change') : $t('settings.reverb_ir_load')}
                  </button>
                  {#if reverbIrLoaded}
                    <span class="reverb-ir-name" title={reverbIrName}>{reverbIrName}</span>
                  {/if}
                </div>
              {:else}
                <!-- Preset selector -->
                <div class="slider-control" style="margin-bottom: 0.5rem;">
                  <div class="slider-header">
                    <span class="slider-label">{$t('reverb.preset')}</span>
                  </div>
                  <select class="reverb-preset-select" bind:value={reverbPreset} on:change={() => applyReverbPreset(reverbPreset)}>
                    {#each reverbPresets as p, i}
                      <option value={i}>{p.name}</option>
                    {/each}
                    <option value={-1}>{$t('reverb.custom_per_organ')}</option>
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
                      <span class="swell-config-label">{$t('settings.reverb_pre_delay')}</span>
                      <input type="range" min="0" max="150" step="5"
                        bind:value={reverbPreDelay}
                        on:input={() => updateAlgorithmicReverb()}
                      />
                      <span class="swell-config-value">{reverbPreDelay}ms</span>
                    </div>
                    <div class="swell-config-row">
                      <span class="swell-config-label">{$t('settings.reverb_damping')}</span>
                      <input type="range" min="0" max="95" step="5"
                        bind:value={reverbDamping}
                        on:input={() => updateAlgorithmicReverb()}
                      />
                      <span class="swell-config-value">{reverbDamping}%</span>
                    </div>
                    <div class="swell-config-row">
                      <span class="swell-config-label">{$t('settings.reverb_room_size')}</span>
                      <input type="range" min="30" max="300" step="10"
                        bind:value={reverbRoomSize}
                        on:input={() => updateAlgorithmicReverb()}
                      />
                      <span class="swell-config-value">{reverbRoomSize}%</span>
                    </div>
                  </div>
                {:else}
                  <p style="margin: 0.2rem 0 0; font-size: 0.68rem; color: var(--text-muted); line-height: 1.35;">
                    {$t('reverb.fixed_room_hint')}
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
                  <span class="swell-toggle-label">{$t('settings.eq_enabled')}</span>
                </label>
                <button
                  class="btn btn-ghost btn-sm"
                  class:learning={globalLearningAction === ACTION_EQ_ENABLE}
                  on:click={() => learnGlobalAction(ACTION_EQ_ENABLE)}
                  disabled={globalLearningAction !== null && globalLearningAction !== ACTION_EQ_ENABLE}
                  title={$t('eq.learn_enable_title').replace('{n}', globalMidiBindings[ACTION_EQ_ENABLE] || 0)}
                  style="font-size:0.7rem; padding:0.2rem 0.5rem; border:1px solid {globalMidiBindings[ACTION_EQ_ENABLE] > 0 ? 'var(--midi-indicator)' : 'var(--accent-soft-2)'};"
                >
                  {#if globalLearningAction === ACTION_EQ_ENABLE}
                    <span class="learning-indicator"></span>{$t('midi.learning_wait')}
                  {:else}
                    {$t('midi.learn_short').replace('{count}', globalMidiBindings[ACTION_EQ_ENABLE] || 0).replace('{max}', 4)}
                  {/if}
                </button>
              </div>
              {#if eqEnabled}
                <div style="margin-top:0.5rem; display:flex; flex-direction:column; gap:0.5rem;">
                  <div style="display:flex; flex-wrap:wrap; gap:0.3rem; margin-bottom:0.5rem; align-items:center;">
                    <span class="audio-select-label">{$t('eq.profile')}</span>
                    {#each eqProfielen as prof (prof.id)}
                      <button class="btn btn-sm" on:click={() => pasEqProfielToe(prof.id)}>{prof.label}</button>
                    {/each}
                  </div>
                  {#each eqBands as band, bi (bi)}
                    <div style="border:1px solid var(--accent-soft-2); border-radius:var(--radius-sm); padding:0.4rem 0.5rem; opacity:{band.enabled ? 1 : 0.55};">
                      <div style="display:flex; align-items:center; gap:0.45rem; flex-wrap:wrap; margin-bottom:0.35rem;">
                        <label class="swell-toggle" style="margin:0;" title={$t('eq.band_toggle_title')}>
                          <input type="checkbox" bind:checked={band.enabled} on:change={onEqBandChange} />
                          <span class="swell-toggle-label">{$t('eq.band_n').replace('{n}', bi + 1)}</span>
                        </label>
                        <select
                          style="font-size:0.75rem; padding:0.15rem 0.3rem; background:var(--bg-elevated); border:1px solid var(--accent-soft-2); border-radius:var(--radius-sm); color:var(--text);"
                          bind:value={band.band_type} on:change={onEqBandChange} title={$t('eq.filter_type')}
                        >
                          {#each eqBandTypes as t}
                            <option value={t.value}>{t.label}</option>
                          {/each}
                        </select>
                        <select
                          style="font-size:0.75rem; padding:0.15rem 0.3rem; background:var(--bg-elevated); border:1px solid var(--accent-soft-2); border-radius:var(--radius-sm); color:var(--text);"
                          value={band.channel === null || band.channel === undefined ? 'all' : String(band.channel)}
                          on:change={(e) => { band.channel = e.target.value === 'all' ? null : parseInt(e.target.value, 10); onEqBandChange(); }}
                          title={$t('settings.eq_channel_hint')}
                        >
                          <option value="all">{$t('settings.eq_all_channels')}</option>
                          {#each Array(Math.max(2, audioChannelCount)) as _, ch}
                            <option value={String(ch)}>{$t('settings.channel_n').replace('{n}', ch + 1)}</option>
                          {/each}
                        </select>
                        <button
                          class="btn btn-ghost btn-sm"
                          style="margin-left:auto; font-size:0.75rem; padding:0.1rem 0.45rem;"
                          on:click={() => removeEqBand(bi)}
                          title={$t('eq.band_remove')}
                        >✕</button>
                      </div>
                      <div class="swell-config-sliders">
                        <div class="swell-config-row">
                          <span class="swell-config-label">{$t('eq.frequency')}</span>
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
                            <span class="swell-config-label">{$t('eq.gain')}</span>
                            <input type="range" min="-24" max="24" step="0.5" bind:value={band.gain_db} on:input={onEqBandChange} />
                            <span class="swell-config-value">{band.gain_db > 0 ? '+' : ''}{Number(band.gain_db).toFixed(1)} dB</span>
                          </div>
                        {/if}
                        {#if band.band_type !== 'lowshelf' && band.band_type !== 'highshelf'}
                          <div class="swell-config-row">
                            <span class="swell-config-label">{$t('eq.bandwidth')}</span>
                            <input type="range" min="0.1" max="4" step="0.05" bind:value={band.bandwidth} on:input={onEqBandChange} />
                            <span class="swell-config-value">{Number(band.bandwidth).toFixed(2)} oct</span>
                          </div>
                        {/if}
                      </div>
                    </div>
                  {/each}
                  <div style="display:flex; gap:0.4rem;">
                    <button class="btn btn-secondary btn-sm" on:click={addEqBand}>{$t('eq.band_add')}</button>
                    <span style="font-size:0.7rem; color:var(--text-muted); align-self:center;">
                      {$t('eq.channel_note')}
                    </span>
                  </div>
                </div>
              {/if}
            </div>

            <!-- Wind-systeem (groepen) -->
            <div class="settings-block">
              <h3 class="settings-block-title">{$t('settings.wind_system')}</h3>
              <div class="swell-config-row" style="margin-bottom: 0.6rem;">
                <span class="swell-config-label" title={$t('wind.groups_count_title')}>{$t('settings.wind_groups_count')}</span>
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

            <!-- Perspectieven en gestapelde ranks (0.7.38) -->
            {#if !secondary && ((organInfo?.perspectives?.length || 0) > 0 || (organInfo?.layered_stops || 0) > 0)}
            <div class="settings-block">
              <h3 class="settings-block-title">{$t('perspectives.title')}</h3>
              {#if (organInfo?.layered_stops || 0) > 0}
                <p class="settings-hint" style="margin: 0 0 0.4rem;">{$t('perspectives.stacked_note').replace('{n}', organInfo.layered_stops)}</p>
              {/if}
              {#if perspectives.length > 0}
                <p class="settings-hint" style="margin: 0 0 0.4rem;">{$t('perspectives.desc')}</p>
                <label class="swell-toggle" style="margin-bottom:0.4rem;" title={$t('perspectives.keep_loaded_title')}>
                  <input type="checkbox" checked={laadAllePosities}
                    on:change={(e) => zetLaadAllePosities(e.target.checked)} />
                  <span class="swell-toggle-label">{$t('perspectives.keep_loaded')}</span>
                </label>
                {#each perspectives as p (p.name)}
                  <div class="slider-control">
                    <div class="slider-header">
                      <label class="swell-toggle" style="display:flex; align-items:center; gap:0.4rem;" title={$t('perspectives.enabled')}>
                        <input type="checkbox" checked={p.enabled} on:change={(e) => setPerspectiveEnabled(p.name, e.target.checked)} />
                        <span class="slider-label">{p.name}</span>
                      </label>
                      <span class="slider-value">
                        {$t('perspectives.pipes').replace('{n}', p.pipe_count)}
                        {#if p.loaded}
                          · {p.gain_db} dB
                        {:else}
                          · {$t('perspectives.not_loaded')} · {$t('perspectives.ram_hint').replace('{mb}', Math.round(p.pipe_count * 0.75))}
                        {/if}
                      </span>
                    </div>
                    <input
                      type="range"
                      min="-24"
                      max="6"
                      step="0.5"
                      value={p.gain_db}
                      disabled={!p.loaded}
                      title={$t('perspectives.gain')}
                      on:input={(e) => setPerspectiveGain(p.name, e.target.value)}
                    />
                  </div>
                {/each}
              {:else}
                <p class="settings-hint" style="margin: 0;">{$t('perspectives.none')}</p>
              {/if}
              {#if perspReloadHint}
                <p style="margin: 0.2rem 0 0; font-size: 0.68rem; color: var(--warning, #d9a441); line-height: 1.35;">{$t('perspectives.reload_hint')}</p>
                <button class="btn btn-sm" style="margin-top: 0.3rem;" on:click={reloadCurrentOrgan}>{$t('perspectives.reload_now')}</button>
              {/if}
            </div>
            {/if}

            <!-- Crescendo -->
            <div class="settings-block">
              <h3 class="settings-block-title">{$t('settings.crescendo')}</h3>
              <div style="display:flex; align-items:center; gap:0.5rem;">
                <label class="swell-toggle">
                  <input type="checkbox" checked={crescendoEnabled}
                    on:change={(e) => setCrescendoEnabledUI(e.target.checked)}
                  />
                  <span class="swell-toggle-label">{$t('settings.crescendo_enabled')}</span>
                </label>
                <button
                  class="btn btn-ghost btn-sm"
                  class:learning={globalLearningAction === ACTION_CRESCENDO_ENABLE}
                  on:click={() => learnGlobalAction(ACTION_CRESCENDO_ENABLE)}
                  disabled={globalLearningAction !== null && globalLearningAction !== ACTION_CRESCENDO_ENABLE}
                  title={$t('crescendo.learn_enable_title').replace('{n}', globalMidiBindings[ACTION_CRESCENDO_ENABLE] || 0)}
                  style="font-size:0.7rem; padding:0.2rem 0.5rem; border:1px solid {globalMidiBindings[ACTION_CRESCENDO_ENABLE] > 0 ? 'var(--midi-indicator)' : 'var(--accent-soft-2)'};"
                >
                  {#if globalLearningAction === ACTION_CRESCENDO_ENABLE}
                    <span class="learning-indicator"></span>{$t('midi.learning_wait')}
                  {:else}
                    {$t('midi.learn_short').replace('{count}', globalMidiBindings[ACTION_CRESCENDO_ENABLE] || 0).replace('{max}', 4)}
                  {/if}
                </button>
              </div>
              <!-- Dode-trede-waarschuwing: BUITEN het {#if crescendoEnabled}-blok,
                   want juist met de crescendo uit (of zonder gevulde trappen) is de
                   ingeleerde CC voor álles dood — ook als zwelkast — en verdwijnt
                   het hele crescendo-blok uit beeld. -->
              {#if crescendoBinding && !crescendoEnabled}
                <p class="pedal-warn">{$t('crescendo.binding_disabled_warn').replace('{channel}', crescendoBinding.channel + 1).replace('{cc}', crescendoBinding.cc)}</p>
              {:else if crescendoBinding && !(crescendoStages || []).some(st => Array.isArray(st) && st.length)}
                <p class="pedal-warn">{$t('crescendo.binding_empty_warn').replace('{channel}', crescendoBinding.channel + 1).replace('{cc}', crescendoBinding.cc)}</p>
              {/if}
              {#if pedalNotice && pedalNotice.scope === 'crescendo'}
                <p class="pedal-notice">{pedalNotice.text}</p>
              {/if}
              {#if crescendoEnabled}
                <div style="margin-top: 0.5rem;">
                  <div class="swell-config-row" style="margin-bottom: 0.5rem;">
                    <span class="swell-config-label">{$t('settings.crescendo_steps')}</span>
                    <select style="flex:1; font-size: 0.8rem;" value={crescendoNumStages} on:change={(e) => setCrescendoNumStagesUI(parseInt(e.target.value, 10) || 15)}>
                      {#each [8, 12, 15, 20, 24, 32] as n}
                        <option value={n}>{n}</option>
                      {/each}
                    </select>
                  </div>
                  <div style="display:flex; gap:0.4rem; margin-bottom: 0.5rem;">
                    <button class="btn btn-secondary btn-sm" style="flex:1;" on:click={autoFillCrescendo} title={$t('crescendo.autofill_title')}>
                      {$t('crescendo.autofill')}
                    </button>
                    <button class="btn btn-ghost btn-sm" on:click={clearCrescendo} title={$t('crescendo.clear_title')}>
                      {$t('settings.crescendo_clear')}
                    </button>
                  </div>
                  <!-- Stage indicator -->
                  <div class="crescendo-bar">
                    {#each Array(crescendoNumStages) as _, i}
                      <div
                        class="crescendo-step"
                        class:active={i < crescendoStage}
                        on:click={() => setCrescendoStage(i + 1 === crescendoStage ? 0 : i + 1)}
                        title={$t('crescendo.step_title').replace('{n}', i + 1).replace('{count}', crescendoStages[i] ? crescendoStages[i].length : 0)}
                      ></div>
                    {/each}
                  </div>
                  <div style="text-align: center; font-size: 0.7rem; color: var(--text-secondary); margin-top: 0.3rem;">
                    {$t('settings.crescendo_stage')} {crescendoStage} / {crescendoNumStages}
                    {#if crescendoStages[crescendoStage - 1]}
                      — {$t('library.stops_count').replace('{count}', crescendoStages[crescendoStage - 1].length)}
                    {/if}
                  </div>

                  <!-- Crescendo-editor: matrix (rijen = registers per divisie, kolommen = stappen).
                       Klik een cel = aan/uit. Shift+klik = cumulatief vanaf die stap t/m het einde. -->
                  {#if displayOrgan}
                    <div class="cresc-editor-hint">
                      {$t('crescendo.editor_hint_click')} · <strong>{$t('crescendo.shift_click')}</strong> {$t('crescendo.editor_hint_shift')}
                    </div>
                    <div class="cresc-table-wrap">
                      <table class="cresc-table">
                        <thead>
                          <tr>
                            <th class="cresc-corner">{$t('crescendo.corner_header')}</th>
                            {#each Array(crescendoNumStages) as _, s}
                              <th
                                class="cresc-step-h"
                                class:active={s + 1 === crescendoStage}
                                on:click={() => setCrescendoStage(s + 1 === crescendoStage ? 0 : s + 1)}
                                title={$t('crescendo.step_preview_title').replace('{n}', s + 1)}
                              >{s + 1}</th>
                            {/each}
                          </tr>
                        </thead>
                        <tbody>
                          {#if (organInfo?.couplers || []).length}
                            <tr class="cresc-div-row">
                              <td class="cresc-div-name" colspan={crescendoNumStages + 1}>{$t('couplers.title')}</td>
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
                                      title={$t('crescendo.cell_title').replace('{n}', s + 1).replace('{name}', koppel.name)}
                                      aria-label={$t('crescendo.cell_label').replace('{n}', s + 1).replace('{name}', koppel.name)}
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
                                      title={$t('crescendo.cell_title').replace('{n}', s + 1).replace('{name}', stop.name)}
                                      aria-label={$t('crescendo.cell_label').replace('{n}', s + 1).replace('{name}', stop.name)}
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
                    disabled={crescendoLearning || learningSwellDivision !== null}
                  >
                    {#if crescendoLearning}
                      <span class="learning-indicator"></span> {$t('settings.crescendo_learning')}
                    {:else if crescendoBinding}
                      {$t('crescendo.pedal_cc').replace('{cc}', crescendoBinding.cc)}
                    {:else}
                      {$t('settings.crescendo_learn')}
                    {/if}
                  </button>
                  <!-- Handmatig: kanaal + CC direct invullen (alternatief voor inleren) -->
                  <div class="swell-config-row" style="margin-top:0.3rem;" title={$t('crescendo.manual_title')}>
                    <span class="swell-config-label">{$t('crescendo.manual')}</span>
                    <select
                      value={crescendoBinding ? crescendoBinding.channel + 1 : ''}
                      on:change={(e) => { if (e.target.value !== '') setCrescendoManualUI(parseInt(e.target.value), crescendoBinding ? crescendoBinding.cc : null); }}
                      title={$t('crescendo.midi_channel_title')}
                    >
                      <option value="" disabled>{$t('crescendo.channel_abbr')}</option>
                      {#each Array(16).fill().map((_, i) => i + 1) as ch}
                        <option value={ch}>{ch}</option>
                      {/each}
                    </select>
                    <input type="number" min="0" max="127" class="pedal-range-input"
                      value={crescendoBinding?.cc ?? ''}
                      placeholder="CC"
                      on:change={(e) => setCrescendoManualUI((crescendoBinding?.channel ?? 0) + 1, e.target.value === '' ? null : parseInt(e.target.value))}
                      title={$t('crescendo.cc_number_title')}
                    />
                    {#if crescendoBinding}
                      <button class="btn btn-ghost btn-sm swell-clear-btn" on:click={clearCrescendoBindingUI} title={$t('crescendo.clear_binding_title')}>&#x2715;</button>
                    {/if}
                  </div>
                  {#if crescendoBinding}
                    <label class="swell-toggle" style="margin-top: 0.4rem; width: 100%;" title={$t('crescendo.invert_title')}>
                      <input
                        type="checkbox"
                        checked={crescendoBinding.invert === true}
                        on:change={(e) => toggleCrescendoInvert(e.target.checked)}
                      />
                      <span>{$t('crescendo.invert')}</span>
                    </label>
                    <div class="swell-config-row" style="margin-top:0.3rem;" title={$t('crescendo.range_title')}>
                      <span class="swell-config-label">{$t('crescendo.range')}</span>
                      <input type="number" min="0" max="127" class="pedal-range-input"
                        value={crescendoBinding.min}
                        on:change={(e) => setCrescendoRangeUI(parseInt(e.target.value), crescendoBinding.max)}
                        title={$t('crescendo.range_min')}
                      />
                      <span style="color:var(--text-muted);">–</span>
                      <input type="number" min="0" max="127" class="pedal-range-input"
                        value={crescendoBinding.max}
                        on:change={(e) => setCrescendoRangeUI(crescendoBinding.min, parseInt(e.target.value))}
                        title={$t('crescendo.range_max')}
                      />
                    </div>
                    <!-- Live volgbalk: toont de actuele crescendo-trap terwijl je het pedaal beweegt. -->
                    <div class="swell-config-row" style="margin-top:0.3rem;" title={$t('crescendo.live_title')}>
                      <span class="swell-config-label">{$t('crescendo.live')}</span>
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
                        <button class="btn btn-ghost btn-xs" on:click={() => sortByPitch(division.name, (organInfo || demoOrgan).divisions.find(d => d.name === division.name)?.stops || [])}>{$t('sorting.by_pitch')}</button>
                        <button class="btn btn-ghost btn-xs" on:click={() => sortByFamily(division.name, (organInfo || demoOrgan).divisions.find(d => d.name === division.name)?.stops || [])}>{$t('sorting.by_family')}</button>
                        <button class="btn btn-ghost btn-xs" on:click={() => resetOrder(division.name)}>{$t('actions.reset')}</button>
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
                          <button class="btn btn-ghost btn-xs" title={$t('sorting.move_up')} disabled={idx === 0}
                            on:click|stopPropagation={() => moveStop(division.name, idx, -1)}>▲</button>
                          <button class="btn btn-ghost btn-xs" title={$t('sorting.move_down')} disabled={idx === division.stops.length - 1}
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
                  {$t('settings.export_settings')}
                </button>
                <button class="btn btn-secondary btn-sm" on:click={importSettings}>
                  {$t('settings.import_settings')}
                </button>
              </div>
              <p style="font-size: 0.7rem; color: var(--text-secondary); margin-top: 0.4rem;">
                {$t('settings.autosave_hint')}
              </p>
            </div>
          </div>

          <!-- Kolom 2: MIDI Mapping + Layout -->
          <div class="instellingen-section">
            <div class="midi-mapping-container">
              <div class="midi-mapping-header">
                <h3>{$t('settings.midi_mapping_title')}</h3>
                <p>{$t('learn.hint_press_first')} <strong>{$t('learn.hint_lowest_key')}</strong>{$t('learn.hint_then')} <strong>{$t('learn.hint_highest_key')}</strong>.</p>
              </div>

              {#if audioChannelCount >= 2}
                <div class="ccis-global" title={$t('ccis.global_title')}>
                  <div style="display:flex; align-items:center; gap:0.5rem; margin-bottom:0.4rem;">
                    <strong style="color: var(--text);">{$t('ccis.global_label')}</strong>
                    <span style="font-size:0.72rem; color:var(--text-muted);">{$t('ccis.global_desc')}</span>
                  </div>
                  <div class="swell-config-sliders">
                    <div class="swell-config-row" title={$t('ccis.strength_title')}>
                      <span class="swell-config-label">{$t('ccis.strength')}</span>
                      <input type="range" min="0" max="100" step="5" bind:value={ccisStrength} on:change={applyCcisSpread} />
                      <span class="swell-config-value">{ccisStrength}%</span>
                    </div>
                    <div class="swell-config-row" title={$t('ccis.falloff_title')}>
                      <span class="swell-config-label">{$t('ccis.falloff')}</span>
                      <input type="range" min="0" max="100" step="5" bind:value={ccisFalloff} on:change={applyCcisSpread} />
                      <span class="swell-config-value">{ccisFalloff}%</span>
                    </div>
                    <div class="swell-config-row" title={$t('ccis.swap_title')}>
                      <span class="swell-config-label">{$t('ccis.sides')}</span>
                      <label class="swell-toggle" style="flex:1;">
                        <input type="checkbox" bind:checked={ccisSwap} on:change={applyCcisSpread} />
                        <span>{$t('ccis.swap_label')}</span>
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
                      <label for="channel-{division.name}">{$t('settings.midi_channel')}:</label>
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
                        <option value="">{$t('settings.midi_all_channels')}</option>
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
                          {$t('learn.lowest_key_wait')}
                        {:else if learningStep === 2}
                          {$t('learn.highest_key_wait')}
                        {:else}
                          {$t('learn.learning')}
                        {/if}
                      {:else}
                        {$t('learn.learn')}
                      {/if}
                    </button>

                    <div class="midi-transpose">
                      <label for="transpose-{division.name}">{$t('settings.midi_transpose_abbr')}:</label>
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
                    <div class="midi-transpose" title={$t('settings.midi_key_range_title')}>
                      <label for="range-lo-{division.name}">{$t('settings.midi_key_range')}:</label>
                      <input
                        id="range-lo-{division.name}"
                        type="number" min="0" max="127" placeholder={$t('settings.midi_key_range_low')}
                        value={m?.first_midi_note ?? ''}
                        on:change={(e) => setKeyRangeUI(division.name, e.target.value === '' ? null : parseInt(e.target.value), m?.last_midi_note ?? null)}
                        title="{$t('settings.midi_lowest_note_title')} {m?.first_midi_note != null ? `(${midiToNoteName(m.first_midi_note)})` : ''}"
                      />
                      <input
                        type="number" min="0" max="127" placeholder={$t('settings.midi_key_range_high')}
                        value={m?.last_midi_note ?? ''}
                        on:change={(e) => setKeyRangeUI(division.name, m?.first_midi_note ?? null, e.target.value === '' ? null : parseInt(e.target.value))}
                        title="{$t('settings.midi_highest_note_title')} {m?.last_midi_note != null ? `(${midiToNoteName(m.last_midi_note)})` : ''}"
                      />
                    </div>

                    <!-- Kort octaaf (C/E): historische klaviatuur waarvan de
                         onderste octaaf niet volledig is. Werkt alleen met een
                         ingeleerde laagste toets, dus dat staat erbij. -->
                    <label class="swell-toggle" title={$t('settings.short_octave_title')}>
                      <input
                        type="checkbox"
                        checked={m?.short_octave === true}
                        disabled={m?.first_midi_note == null}
                        on:change={(e) => setShortOctave(division.name, e.target.checked)}
                      />
                      <span class="swell-toggle-label">{$t('settings.short_octave')}</span>
                    </label>
                    {#if m?.short_octave && m?.first_midi_note == null}
                      <p class="settings-hint" style="margin:0;">{$t('settings.short_octave_needs_range')}</p>
                    {/if}

                    <label class="swell-toggle" title={$t('swell.toggle_title')}>
                      <input
                        type="checkbox"
                        checked={isSwellEnabled(division.name)}
                        on:change={() => toggleSwellEnabled(division.name)}
                      />
                      <span class="swell-toggle-label">{$t('settings.swell_box')}</span>
                    </label>
                    {#if isSwellEnabled(division.name)}
                      <button
                        class="btn btn-secondary midi-learn-btn swell-learn-btn"
                        class:learning={learningSwellDivision === division.name}
                        on:click={() => learnSwellPedal(division.name)}
                        disabled={learningSwellDivision !== null || crescendoLearning}
                      >
                        {#if learningSwellDivision === division.name}
                          <span class="learning-indicator"></span>
                          {$t('settings.midi_swell_learning')}
                        {:else if swellBindings[division.name]}
                          CC{swellBindings[division.name].cc_num}
                        {:else}
                          {$t('settings.midi_swell_learn')}
                        {/if}
                      </button>
                      <!-- Handmatig: kanaal + CC direct invullen (alternatief voor inleren) -->
                      <div class="swell-config-row" title={$t('swell.manual_title')}>
                        <span class="swell-config-label">{$t('swell.manual')}</span>
                        <select
                          value={swellBindings[division.name] ? swellBindings[division.name].channel + 1 : ''}
                          on:change={(e) => { if (e.target.value !== '') setSwellManualUI(division.name, divIdx, parseInt(e.target.value), swellBindings[division.name] ? swellBindings[division.name].cc_num : null); }}
                          title={$t('swell.midi_channel_title')}
                        >
                          <option value="" disabled>{$t('swell.channel_abbr')}</option>
                          {#each Array(16).fill().map((_, i) => i + 1) as ch}
                            <option value={ch}>{ch}</option>
                          {/each}
                        </select>
                        <input type="number" min="0" max="127" class="pedal-range-input"
                          value={swellBindings[division.name]?.cc_num ?? ''}
                          placeholder="CC"
                          on:change={(e) => setSwellManualUI(division.name, divIdx, (swellBindings[division.name]?.channel ?? 0) + 1, e.target.value === '' ? null : parseInt(e.target.value))}
                          title={$t('swell.cc_number_title')}
                        />
                      </div>
                      {#if pedalNotice && pedalNotice.scope === division.name}
                        <p class="pedal-notice">{pedalNotice.text}</p>
                      {/if}
                      {#if swellBindings[division.name]}
                        <button
                          class="btn btn-ghost btn-sm swell-clear-btn"
                          on:click={() => clearSwellBinding(division.name)}
                          title={$t('swell.clear_binding_title')}
                        >&#x2715;</button>
                        <label class="swell-toggle" title={$t('swell.invert_title')}>
                          <input
                            type="checkbox"
                            checked={swellBindings[division.name].invert === true}
                            on:change={(e) => toggleSwellInvert(division.name, e.target.checked)}
                          />
                          <span>{$t('swell.invert')}</span>
                        </label>
                        <div class="swell-config-row" title={$t('swell.range_title')}>
                          <span class="swell-config-label">{$t('swell.range')}</span>
                          <input type="number" min="0" max="127" class="pedal-range-input"
                            value={swellBindings[division.name].min_val}
                            on:change={(e) => setSwellRangeUI(division.name, parseInt(e.target.value), swellBindings[division.name].max_val)}
                            title={$t('swell.range_min')}
                          />
                          <span style="color:var(--text-muted);">–</span>
                          <input type="number" min="0" max="127" class="pedal-range-input"
                            value={swellBindings[division.name].max_val}
                            on:change={(e) => setSwellRangeUI(division.name, swellBindings[division.name].min_val, parseInt(e.target.value))}
                            title={$t('swell.range_max')}
                          />
                        </div>
                        <!-- Live volgbalk: toont de actuele zwelstand terwijl je het pedaal beweegt. -->
                        <div class="swell-config-row" title={$t('swell.live_title')}>
                          <span class="swell-config-label">{$t('swell.live')}</span>
                          <div class="follow-bar">
                            <div class="follow-bar-fill" style="width: {Math.round((divisionVolumes[division.name] ?? 1.0) * 100)}%"></div>
                          </div>
                          <span class="swell-config-value">{Math.round((divisionVolumes[division.name] ?? 1.0) * 100)}%</span>
                        </div>
                      {/if}
                      <div class="swell-config-sliders">
                        <div class="swell-config-row">
                          <span class="swell-config-label">{$t('swell.min_volume')}</span>
                          <input type="range" min="-30" max="-5" step="1"
                            value={getSwellMinDb(division.name)}
                            on:input={(e) => { swellMinDb[division.name] = parseInt(e.target.value); swellMinDb = swellMinDb; updateSwellConfig(division.name); }}
                          />
                          <span class="swell-config-value">{getSwellMinDb(division.name)} dB</span>
                        </div>
                        <div class="swell-config-row">
                          <span class="swell-config-label">{$t('swell.filter_closed')}</span>
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
                        <span title={$t('wind.group_title')}>{$t('settings.windvoorziening')}</span>
                        <select
                          value={getWindGroup(division.name, divIdx)}
                          on:change={(e) => onWindGroupChange(division.name, e.target.value)}
                          style="padding:0.25rem 0.5rem; background:var(--bg-elevated); border:1px solid var(--accent-soft-2); border-radius:var(--radius-sm); color:var(--text); font-size:0.78rem;"
                          title={$t('wind.group_select_title')}
                        >
                          {#each Array(Math.max(1, Math.min(numWindGroups, 8))) as _, gIdx}
                            <option value={gIdx}>{$t('settings.wind_group')} {gIdx + 1}</option>
                          {/each}
                        </select>
                      </label>
                    </div>

                    <!-- Stereo Pan -->
                    <div class="swell-config-sliders" style="margin-top: 0.3rem;">
                      <div class="swell-config-row">
                        <span class="swell-config-label">{$t('settings.pan')}</span>
                        <input type="range" min="-100" max="100" step="5"
                          value={getDivisionPan(division.name)}
                          on:input={(e) => onPanChange(division.name, parseInt(e.target.value))}
                        />
                        <span class="swell-config-value">{formatPan(getDivisionPan(division.name), $t)}</span>
                      </div>
                      {#if audioChannelCount >= 2}
                        {@const divChannels = Array.isArray(divisionOutputChannels[division.name]) ? divisionOutputChannels[division.name] : []}
                        {@const allOut = divChannels.length > 0 && divChannels.every(c => c >= audioChannelCount)}
                        {@const partOut = !allOut && divChannels.some(c => c >= audioChannelCount)}
                        <div class="swell-config-row" title={$t('settings.channels_per_division_hint').replace('{host}', audioHostActual || '?')}>
                          <span class="swell-config-label">{$t('settings.channels_per_division')}</span>
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
                        {#if allOut || partOut}
                          <div class="swell-config-row">
                            <span class="swell-config-label"></span>
                            <span class="swell-config-value" style="color: var(--warning, #d9a441); flex:1; text-align:left;" title={$t('settings.channel_out_of_range_hint')}>
                              {allOut
                                ? $t('settings.channel_out_of_range')
                                : $t('settings.channel_partly_out_of_range').replace('{list}', divChannels.filter(c => c >= audioChannelCount).map(c => c + 1).join(', '))}
                            </span>
                          </div>
                        {/if}
                        <div class="swell-config-row" title={$t('ccis.division_title')}>
                          <span class="swell-config-label">{$t('ccis.short_label')}</span>
                          <label class="swell-toggle" style="flex:1;">
                            <input type="checkbox" checked={getDivisionCcis(division.name)} on:change={() => toggleDivisionCcis(division.name)} />
                            <span>{$t('ccis.enable_on_division')}</span>
                          </label>
                        </div>
                      {/if}
                    </div>

                    <!-- Waar komt de tremulant van deze divisie vandaan? Zonder
                         dit regeltje bleef "waarom hoor ik de nagebootste?" een
                         raadsel: bij de meeste GrandOrgue-sets staat er domweg
                         geen tremulant-opname in de set. -->
                    {#if division.has_tremulant}
                      <p class="settings-hint" style="margin:0.35rem 0 0;">
                        {#if division.tremulant_kind === 'wave' || division.tremulant_kind === 'samples'}
                          {$t('settings.tremulant_from_samples')}
                        {:else}
                          {$t('settings.tremulant_simulated')}
                        {/if}
                      </p>
                    {/if}

                    <!-- Tremulant LFO + MIDI learn knopje -->
                    <div style="display:flex; align-items:center; gap:0.5rem; margin-top:0.3rem;">
                      <label class="swell-toggle" title={$t('settings.tremulant_enable_title')}>
                        <input
                          type="checkbox"
                          checked={tremLfoEnabled[division.name] === true}
                          on:change={() => setTremEnabled(division, tremLfoEnabled[division.name] !== true)}
                        />
                        <span class="swell-toggle-label">{$t('settings.tremulant_lfo')}</span>
                      </label>
                      <button
                        class="btn btn-ghost btn-sm"
                        class:learning={learningTremDivIdx === divIdx}
                        on:click={() => learnTremulantLfo(divIdx)}
                        disabled={learningTremDivIdx !== null && learningTremDivIdx !== divIdx}
                        title={$t('settings.tremulant_learn_title').replace('{n}', tremLfoMidiBindings[ACTION_TREM_LFO_BASE + divIdx] || 0)}
                        style="font-size:0.7rem; padding:0.2rem 0.5rem; border:1px solid {tremLfoMidiBindings[ACTION_TREM_LFO_BASE + divIdx] > 0 ? 'var(--midi-indicator)' : 'var(--accent-soft-2)'};"
                      >
                        {#if learningTremDivIdx === divIdx}
                          <span class="learning-indicator"></span>{$t('midi.learning_wait')}
                        {:else}
                          {$t('midi.learn_short').replace('{count}', tremLfoMidiBindings[ACTION_TREM_LFO_BASE + divIdx] || 0).replace('{max}', 4)}
                        {/if}
                      </button>
                    </div>
                    {#if tremLfoEnabled[division.name]}
                      <div class="swell-config-sliders">
                        <div class="swell-config-row">
                          <span class="swell-config-label">{$t('settings.tremulant_rate')}</span>
                          <input type="range" min="30" max="100" step="5"
                            value={(tremLfoRate[division.name] ?? 6.0) * 10}
                            on:input={(e) => { tremLfoRate[division.name] = parseInt(e.target.value) / 10; tremLfoRate = tremLfoRate; updateTremLfoParams(division.name); }}
                          />
                          <span class="swell-config-value">{(tremLfoRate[division.name] ?? 6.0).toFixed(1)} Hz</span>
                        </div>
                        <div class="swell-config-row">
                          <span class="swell-config-label">{$t('settings.tremulant_amp_depth')}</span>
                          <input type="range" min="1" max="25" step="1"
                            value={tremLfoAmpDepth[division.name] ?? 10}
                            on:input={(e) => { tremLfoAmpDepth[division.name] = parseInt(e.target.value); tremLfoAmpDepth = tremLfoAmpDepth; updateTremLfoParams(division.name); }}
                          />
                          <span class="swell-config-value">{tremLfoAmpDepth[division.name] ?? 10}%</span>
                        </div>
                        <div class="swell-config-row">
                          <span class="swell-config-label">{$t('settings.tremulant_pitch_depth')}</span>
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

              <!-- Doorlopend klavier -->
              <label class="swell-toggle" style="margin-top:1rem;" title={$t('settings.continuous_keyboard_title')}>
                <input type="checkbox" checked={doorlopendKlavier}
                  on:change={(e) => zetDoorlopendKlavier(e.target.checked)} />
                <span class="swell-toggle-label">{$t('settings.continuous_keyboard')}</span>
              </label>

              <!-- Koppels -->
              {#if displayOrgan.couplers && displayOrgan.couplers.length > 0}
                <div class="midi-mapping-header" style="margin-top: 1.5rem;">
                  <h3>{$t('couplers.title')}</h3>
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
                        {$t('midi.learn_short').replace('{count}', couplerMidiBindings[coupler.midi_action_code] || 0).replace('{max}', 4)}
                      </button>
                      {#if couplerMidiBindings[coupler.midi_action_code] > 0}
                        <button
                          class="btn btn-ghost btn-sm swell-clear-btn"
                          on:click={() => clearCouplerMidi(coupler.midi_action_code)}
                          title={$t('midi.learn_clear')}
                        >&#x2715;</button>
                      {/if}
                    </div>
                  </div>
                {/each}
              {/if}

              <!-- Koppels die deze sampleset niet meebrengt maar die wel uit de
                   klavierindeling volgen: sub, super, melodie, bas, tongwerken af. -->
              {#if extraKoppelOpties.length > 0}
                <div class="midi-mapping-header" style="margin-top: 1.2rem;">
                  <h3>{$t('couplers.extra_title')}</h3>
                </div>
                <p class="settings-hint" style="margin: 0 0 0.4rem;">{$t('couplers.extra_hint')}</p>
                <div style="display:flex; flex-wrap:wrap; gap:0.5rem 1rem;">
                  {#each extraKoppelOpties as k (k.id)}
                    <label class="coupler-check" style="min-width: 10rem;">
                      <input type="checkbox" checked={k.active}
                        on:change={(e) => zetExtraKoppel(k.id, e.target.checked)} />
                      <span class="division-label">{k.name.replace(/\n/g, ' ')}</span>
                    </label>
                  {/each}
                </div>
              {/if}
            </div>

            <!-- ============ MIDI-koppelingen: overzicht + handmatig bewerken ============ -->
            <div class="settings-block">
              <h3 class="settings-block-title">{$t('bindings.editor_title')}</h3>
              <p style="margin: 0 0 0.5rem; font-size: 0.7rem; color: var(--text-muted); line-height: 1.4;">
                {$t('bindings.editor_intro')}
              </p>
              {#if midiBindingsError}
                <p style="color: #e07a6a; font-size: 0.72rem; margin: 0 0 0.4rem;">{midiBindingsError}</p>
              {/if}
              <div class="midi-bindings-wrap">
                <table class="midi-bindings-table">
                  <thead>
                    <tr>
                      <th>{$t('bindings.col_target')}</th>
                      <th>{$t('bindings.col_type')}</th>
                      <th>{$t('settings.midi_channel')}</th>
                      <th>{$t('bindings.col_number')}</th>
                      <th>{$t('bindings.col_threshold')}</th>
                      <th></th>
                    </tr>
                  </thead>
                  <tbody>
                    {#each midiBindingRows as row (`${row.preset_num}-${row.idx}`)}
                      <tr>
                        <td class="midi-bindings-doel" title={midiActionLabel(row.preset_num, $t)}>
                          {midiActionLabel(row.preset_num, $t)}{row.idx > 0 ? ` (${row.idx + 1})` : ''}
                        </td>
                        <td>
                          <select value={row.trigger_type} on:change={(e) => updateBindingRow(row, { type: e.target.value })}>
                            <option value="note">{$t('bindings.type_note')}</option>
                            <option value="cc">CC</option>
                            <option value="program">{$t('bindings.type_program')}</option>
                            <option value="ccbit">{$t('bindings.type_ccbit')}</option>
                            <option value="sysex">{$t('bindings.type_sysex')}</option>
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
                              <option value="">{$t('settings.midi_all_channels')}</option>
                              {#each Array(16).fill().map((_, i) => i + 1) as ch}
                                <option value={ch}>{ch}</option>
                              {/each}
                            </select>
                          {/if}
                        </td>
                        <td style="white-space: nowrap;">
                          {#if row.trigger_type === 'sysex'}
                            <input type="text" class="pedal-range-input" style="width: 9rem;"
                              value={row.sysex_hex ?? ''}
                              placeholder="7D 01 04"
                              on:change={(e) => updateBindingRow(row, { sysexHex: e.target.value })}
                              title={$t('bindings.sysex_title')}
                            />
                          {:else}
                            <input type="number" min="0" max="127" class="pedal-range-input"
                              value={bindingNumber(row) ?? 0}
                              on:change={(e) => updateBindingRow(row, { number: parseInt(e.target.value) || 0 })}
                              title={row.trigger_type === 'note' ? $t('bindings.note_number') : (row.trigger_type === 'cc' || row.trigger_type === 'ccbit') ? $t('bindings.cc_number') : $t('bindings.program_number')}
                            />
                            {#if row.trigger_type === 'note'}
                              <span style="font-size: 0.65rem; color: var(--text-muted);">{midiToNoteName(bindingNumber(row) ?? 0)}</span>
                            {/if}
                          {/if}
                        </td>
                        <td>
                          {#if row.trigger_type === 'cc'}
                            <input type="number" min="0" max="127" class="pedal-range-input"
                              value={row.value ?? 64}
                              on:change={(e) => updateBindingRow(row, { value: parseInt(e.target.value) || 0 })}
                              title={$t('bindings.threshold_title')}
                            />
                          {:else if row.trigger_type === 'ccbit'}
                            <input type="number" min="0" max="6" class="pedal-range-input"
                              value={row.bit ?? 0}
                              on:change={(e) => updateBindingRow(row, { value: parseInt(e.target.value) || 0 })}
                              title={$t('bindings.bit_title')}
                            />
                          {:else}
                            <span style="color: var(--text-muted);">—</span>
                          {/if}
                        </td>
                        <td>
                          <button class="btn btn-ghost btn-sm swell-clear-btn" on:click={() => removeBindingRow(row)} title={$t('bindings.remove_title')}>&#x2715;</button>
                        </td>
                      </tr>
                    {:else}
                      <tr>
                        <td colspan="6" style="color: var(--text-muted); padding: 0.6rem;">
                          {$t('bindings.empty')}
                        </td>
                      </tr>
                    {/each}
                  </tbody>
                </table>
              </div>
              <!-- Nieuwe koppeling handmatig toevoegen -->
              <div style="display: flex; align-items: center; gap: 0.4rem; margin-top: 0.55rem; flex-wrap: wrap;">
                <span style="font-size: 0.72rem; color: var(--text-secondary);">{$t('bindings.new')}</span>
                <select bind:value={newBinding.code} style="max-width: 15rem;">
                  <option value="">{$t('bindings.choose_target')}</option>
                  <optgroup label={$t('bindings.group_functions')}>
                    {#each bindingTargets.functies as tgt}<option value={tgt.code}>{tgt.label}</option>{/each}
                  </optgroup>
                  {#if bindingTargets.trems.length}
                    <optgroup label={$t('bindings.group_tremulants')}>
                      {#each bindingTargets.trems as tgt}<option value={tgt.code}>{tgt.label}</option>{/each}
                    </optgroup>
                  {/if}
                  {#if bindingTargets.koppels.length}
                    <optgroup label={$t('couplers.title')}>
                      {#each bindingTargets.koppels as tgt}<option value={tgt.code}>{tgt.label}</option>{/each}
                    </optgroup>
                  {/if}
                  {#if bindingTargets.registers.length}
                    <optgroup label={$t('bindings.group_stops')}>
                      {#each bindingTargets.registers as tgt}<option value={tgt.code}>{tgt.label}</option>{/each}
                    </optgroup>
                  {/if}
                </select>
                <select bind:value={newBinding.type} title={$t('bindings.trigger_type_title')}>
                  <option value="note">{$t('bindings.type_note')}</option>
                  <option value="cc">CC</option>
                  <option value="program">{$t('bindings.type_program')}</option>
                </select>
                {#if newBinding.type !== 'note'}
                  <select bind:value={newBinding.channel} title={$t('bindings.channel_title')}>
                    <option value="">{$t('settings.eq_all_channels')}</option>
                    {#each Array(16).fill().map((_, i) => i + 1) as ch}
                      <option value={ch}>{$t('settings.midi_channel')} {ch}</option>
                    {/each}
                  </select>
                {/if}
                <input type="number" min="0" max="127" class="pedal-range-input" bind:value={newBinding.number}
                  title={$t('bindings.number_title')} />
                {#if newBinding.type === 'cc'}
                  <input type="number" min="0" max="127" class="pedal-range-input" bind:value={newBinding.value}
                    title={$t('bindings.threshold_new_title')} />
                {/if}
                <button class="btn btn-secondary btn-sm" on:click={addNewBindingUI}>{$t('bindings.add')}</button>
                <button class="btn btn-ghost btn-sm" on:click={refreshMidiBindingsFull} title={$t('bindings.reload_title')}>↻</button>
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
        {#if !organInfo}
          <!-- Zonder geladen orgel is dit de weg terug naar het startscherm.
               Stond er al sinds 0.4.0, maar hing achter "!displayOrgan" — en dat
               is nooit waar omdat buildDisplayOrgan terugvalt op het ingebouwde
               "Demo Orgel". Nu gekoppeld aan het échte orgel (organInfo). -->
          <div style="margin-bottom: 1rem;">
            <button class="btn btn-secondary btn-sm" on:click={() => { showOrganBrowser = true; dispatch('setView', 'orgel'); }}>
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
                    <option value={h} selected={h === selectedAudioHost}>{prettyHost(h, $t)}</option>
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

              <!-- Beschikbare uitgangen van de nu geopende audio-uitgang -->
              <p style="margin: 0.5rem 0 0; font-size: 0.72rem; color: var(--text);">
                {$t('settings.audio_channels_available').replace('{n}', audioChannelCount).replace('{host}', audioHostActual || (audioStatus?.audio_host ?? '?'))}
              </p>
              <p style="margin: 0.15rem 0 0; font-size: 0.68rem; color: var(--text-muted); line-height: 1.35;">
                {$t('settings.audio_channels_hint')}{#if !isAsioHost} {$t('settings.audio_channels_hint_wasapi')}{/if}
              </p>

              <!-- Uitgangspaar (welke kanalen van het apparaat) -->
              {#if organInfo && outputChannelTotal > 2}
                <div class="audio-select-row" style="margin-top: 0.7rem;" title={$t('settings.output_pair_hint')}>
                  <label class="audio-select-label" for="audio-output-pair">{$t('settings.output_pair')}</label>
                  <select id="audio-output-pair" class="temperament-select" on:change={(e) => setOutputPair(e.target.value)}>
                    {#each Array(Math.floor(outputChannelTotal / 2)) as _, i}
                      <option value={String(i)} selected={outputPairSel === String(i)}>{i * 2 + 1}–{i * 2 + 2}</option>
                    {/each}
                    <option value="custom" selected={outputPairSel === 'custom'} disabled={outputPairSel !== 'custom'}>{$t('settings.output_pair_custom')}</option>
                  </select>
                </div>
              {/if}

              <!-- Testsignaal per uitgang: welke stekker zit in welke kast? -->
              {#if audioStatus && audioStatus.audio_running}
                <div style="margin-top: 0.7rem;">
                  <div class="audio-select-row">
                    <span class="audio-select-label">{$t('settings.test_signal')}</span>
                    <select class="temperament-select" bind:value={testSignaalSoort}
                      on:change={() => { if (testSignaalKanaal !== null) { const k = testSignaalKanaal; testSignaalKanaal = null; zetTestsignaal(k); } }}>
                      <option value={0}>{$t('settings.test_signal_pink')}</option>
                      <option value={1}>{$t('settings.test_signal_sine')}</option>
                    </select>
                  </div>
                  <div style="display:flex; flex-wrap:wrap; gap:0.3rem; margin-top:0.4rem;">
                    {#each Array(outputChannelTotal) as _, ch}
                      <button class="btn btn-sm" class:btn-active={testSignaalKanaal === ch}
                        on:click={() => zetTestsignaal(ch)}>
                        {ch + 1}{#if ch === 0} {$t('settings.test_signal_left')}{:else if ch === 1} {$t('settings.test_signal_right')}{/if}
                      </button>
                    {/each}
                  </div>
                  <p class="settings-hint" style="margin: 0.3rem 0 0;">{$t('settings.test_signal_hint')}</p>
                </div>
              {/if}

              <!-- Polyfonie -->
              <div class="audio-select-row" style="margin-top: 0.7rem;" title={$t('settings.polyphony_hint')}>
                <label class="audio-select-label" for="audio-polyphony">{$t('settings.polyphony')}</label>
                <select id="audio-polyphony" class="temperament-select" on:change={(e) => setPolyphony(e.target.value)}>
                  {#each POLYPHONY_CHOICES as n}
                    <option value={n} selected={(audioStatus?.polyphony || 1024) === n}>{n}{n === 1024 ? ` (${$t('settings.polyphony_default')})` : ''}</option>
                  {/each}
                </select>
              </div>
              {#if audioStatus && audioStatus.audio_running}
                <p style="margin: 0.2rem 0 0; font-size: 0.68rem; color: var(--text-muted); line-height: 1.35;">
                  {$t('settings.polyphony_load').replace('{load}', Math.round((audioStatus.render_load || 0) * 100)).replace('{peak}', Math.round((audioStatus.render_peak || 0) * 100))}
                </p>
              {/if}

              <!-- Stereo-samples -->
              <label class="swell-toggle" style="margin-top: 0.7rem; display: flex; align-items: center; gap: 0.4rem;" title={$t('settings.stereo_samples_hint')}>
                <input type="checkbox" checked={audioStatus ? audioStatus.stereo_samples !== false : true} on:change={(e) => setStereoSamples(e.target.checked)} />
                <span>{$t('settings.stereo_samples')}</span>
              </label>
              {#if stereoReloadHint}
                <p style="margin: 0.2rem 0 0; font-size: 0.68rem; color: var(--warning, #d9a441); line-height: 1.35;">{$t('settings.reload_hint')}</p>
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
                    <option value={b} selected={b === selectedBufferFrames}>{b} ({(b / 48).toFixed(1)} ms @48k){b < 64 ? ` — ${$t('settings.audio_buffer_fast_pc')}` : (b === 128 ? ` — ${$t('settings.audio_buffer_recommended')}` : '')}</option>
                  {/each}
                </select>
              </div>
              <p style="margin: 0.25rem 0 0; font-size: 0.68rem; color: var(--text-muted); line-height: 1.35;">{$t('settings.audio_buffer_hint')}</p>
              <div style="margin-top: 0.7rem;">
                <button class="btn btn-primary btn-sm" on:click={() => { dispatch('applyAudioOutput'); setTimeout(refreshAudioStatus, 1500); }}>
                  {$t('settings.audio_apply')}
                </button>
              </div>

              <!-- Actuele status + geschatte latentie -->
              {#if audioStatus && audioStatus.audio_running}
                <div style="margin-top: 0.6rem; padding: 0.45rem 0.6rem; background: var(--bg-elevated); border: 1px solid var(--accent-soft-2); border-radius: var(--radius-sm); font-size: 0.74rem; line-height: 1.5;">
                  <strong>{$t('settings.audio_current')}</strong>
                  {audioStatus.audio_host} · {audioStatus.audio_device || $t('settings.default_device')} ·
                  {audioStatus.sample_rate} Hz · {$t('settings.audio_current_channels').replace('{n}', audioStatus.channels || audioChannelCount)} ·
                  {audioStatus.buffer_frames > 0 ? $t('settings.audio_buffer_frames').replace('{n}', audioStatus.buffer_frames) : $t('settings.audio_buffer_driver_default')}
                  {#if audioLatencyMs !== null}
                    · <strong>{$t('settings.audio_latency_est').replace('{ms}', audioLatencyMs.toFixed(1))}</strong>
                  {:else}
                    · {$t('settings.audio_latency_driver_default')}
                  {/if}
                </div>
              {/if}
              <p style="margin: 0.5rem 0 0; font-size: 0.7rem; color: var(--text-muted); line-height: 1.4;">
                {$t('settings.audio_latency_hint')}
              </p>

              <!-- Uitvoerprofielen: snelle wissel speakers ↔ hoofdtelefoon -->
              <div style="margin-top: 0.85rem; padding-top: 0.75rem; border-top: var(--border-subtle);">
                <h4 style="margin: 0 0 0.35rem; font-size: 0.85rem;">{$t('settings.audio_profiles_title')}</h4>
                <p style="margin: 0 0 0.5rem; font-size: 0.7rem; color: var(--text-muted); line-height: 1.4;">
                  {$t('settings.audio_profiles_desc')}
                </p>
                {#each [['speakers', $t('settings.audio_profile_speakers')], ['headphones', $t('settings.audio_profile_headphones')]] as [kind, label]}
                  {@const prof = audioProfiles?.[kind]}
                  <div style="display:flex; align-items:center; gap:0.45rem; margin-bottom:0.4rem; flex-wrap:wrap;">
                    <strong style="font-size:0.78rem; width:7.5rem;">
                      {label}{audioProfiles?.active === kind ? ' ●' : ''}
                    </strong>
                    <span style="flex:1; font-size:0.72rem; color:var(--text-muted); min-width:10rem;">
                      {#if prof}
                        {prof.host || '?'} · {prof.device || $t('settings.default_device')}{prof.bufferFrames ? ` · ${$t('settings.audio_buffer_frames').replace('{n}', prof.bufferFrames)}` : ''}{prof.channels ? ` · ${$t('settings.audio_profile_incl_channels')}` : ''}
                      {:else}
                        {$t('settings.audio_profile_not_set')}
                      {/if}
                    </span>
                    <button class="btn btn-secondary btn-sm" style="font-size:0.7rem; padding:0.15rem 0.5rem;"
                      on:click={() => dispatch('saveAudioProfile', kind)}
                      title={$t('settings.audio_profile_save_title').replace('{name}', label)}
                    >{$t('settings.audio_profile_save')}</button>
                    {#if prof}
                      <button class="btn btn-secondary btn-sm" style="font-size:0.7rem; padding:0.15rem 0.5rem;"
                        on:click={() => dispatch('switchAudioProfile', kind)}
                        title={$t('settings.audio_profile_activate_title')}
                      >{$t('settings.audio_profile_activate')}</button>
                      <button class="btn btn-ghost btn-sm" style="font-size:0.7rem; padding:0.15rem 0.4rem;"
                        on:click={() => dispatch('clearAudioProfile', kind)}
                        title={$t('settings.audio_profile_clear')}
                      >✕</button>
                    {/if}
                  </div>
                {/each}
                <div style="display:flex; align-items:center; gap:0.5rem;">
                  <span style="font-size:0.72rem; color:var(--text-muted);">{$t('settings.audio_profile_midi_button')}</span>
                  <button
                    class="btn btn-ghost btn-sm"
                    class:learning={globalLearningAction === ACTION_AUDIO_PROFILE}
                    on:click={() => learnGlobalAction(ACTION_AUDIO_PROFILE)}
                    disabled={globalLearningAction !== null && globalLearningAction !== ACTION_AUDIO_PROFILE}
                    title={$t('settings.audio_profile_learn_title').replace('{n}', globalMidiBindings[ACTION_AUDIO_PROFILE] || 0)}
                    style="font-size:0.7rem; padding:0.2rem 0.5rem; border:1px solid {globalMidiBindings[ACTION_AUDIO_PROFILE] > 0 ? 'var(--midi-indicator)' : 'var(--accent-soft-2)'};"
                  >
                    {#if globalLearningAction === ACTION_AUDIO_PROFILE}
                      <span class="learning-indicator"></span>{$t('midi.learning_wait')}
                    {:else}
                      {$t('midi.learn_short').replace('{count}', globalMidiBindings[ACTION_AUDIO_PROFILE] || 0).replace('{max}', 4)}
                    {/if}
                  </button>
                </div>
              </div>

              <!-- Automatisch starten bij Windows -->
              <div style="margin-top: 0.85rem; padding-top: 0.75rem; border-top: var(--border-subtle);">
                <label class="swell-toggle" title={$t('general.autostart_title')}>
                  <input
                    type="checkbox"
                    checked={autostartEnabled}
                    on:change={(e) => dispatch('setAutostart', e.target.checked)}
                  />
                  <span>{$t('general.autostart')}</span>
                </label>
                <label class="swell-toggle" style="margin-top:0.4rem;" title={$t('general.auto_load_last_organ_title')}>
                  <input
                    type="checkbox"
                    checked={autoLoadLastOrgan}
                    on:change={(e) => dispatch('setAutoLoadLastOrgan', e.target.checked)}
                  />
                  <span>{$t('general.auto_load_last_organ')}</span>
                </label>
                <label class="swell-toggle" style="margin-top:0.4rem;" title={$t('settings.panel_close_quits_title')}>
                  <input
                    type="checkbox"
                    checked={panelCloseQuits}
                    on:change={(e) => dispatch('setPanelCloseQuits', e.target.checked)}
                  />
                  <span>{$t('settings.panel_close_quits')}</span>
                </label>
                <label class="swell-toggle" style="margin-top:0.4rem;" title={$t('general.restore_registration_title')}>
                  <input
                    type="checkbox"
                    checked={restoreRegistration}
                    on:change={(e) => dispatch('setRestoreRegistration', e.target.checked)}
                  />
                  <span>{$t('general.restore_registration')}</span>
                </label>
              </div>

              <!-- Automatisch MIDI-archief (0.7.38) -->
              <div style="margin-top: 0.85rem; padding-top: 0.75rem; border-top: var(--border-subtle);">
                <div style="display:flex; justify-content:space-between; align-items:center; margin-bottom:0.4rem;">
                  <span style="font-size:0.8rem; color:var(--text-secondary); font-weight:500;">{$t('midi_archive.title')}</span>
                  <span style="display:flex; align-items:center; gap:0.4rem; font-size:0.72rem; color:var(--text-muted);">
                    <span class="record-dot" class:on={archiveStatus.archiving}></span>
                    {#if !archiveCfg.enabled}
                      {$t('midi_archive.status_off')}
                    {:else if archiveStatus.archiving}
                      {$t('midi_archive.status_archiving').replace('{n}', String(archiveStatus.event_count)).replace('{t}', formatSeconds(archiveStatus.seconds))}
                    {:else}
                      {$t('midi_archive.status_idle')}
                    {/if}
                  </span>
                </div>
                <div style="font-size:0.72rem; color:var(--text-muted); margin-bottom:0.4rem;">{$t('midi_archive.subtitle')}</div>
                <label class="swell-toggle" title={$t('midi_archive.enabled_title')}>
                  <input
                    type="checkbox"
                    checked={archiveCfg.enabled}
                    on:change={(e) => { archiveCfg.enabled = e.target.checked; saveArchiveConfig(); }}
                  />
                  <span>{$t('midi_archive.enabled')}</span>
                </label>
                <div class="audio-select-row" style="margin-top:0.5rem;">
                  <label class="audio-select-label">{$t('midi_archive.folder')}</label>
                  <span style="flex:1; min-width:0; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; font-size:0.75rem;" title={archiveCfg.dir}>{archiveCfg.dir}</span>
                  <button class="btn btn-ghost btn-sm" on:click={chooseArchiveDir}>{$t('midi_archive.folder_choose')}</button>
                  <button class="btn btn-ghost btn-sm" on:click={openArchiveDir}>{$t('midi_archive.folder_open')}</button>
                  {#if !archiveCfg.dir_is_default}
                    <button class="btn btn-ghost btn-sm" on:click={resetArchiveDir}>{$t('midi_archive.folder_default')}</button>
                  {/if}
                </div>
                <!-- Eigen flex-rij (niet .audio-select-row: die zet flex:1 op inputs) -->
                <div style="display:flex; align-items:center; gap:0.6rem; margin-top:0.4rem; flex-wrap:wrap;" title={$t('midi_archive.limits_title')}>
                  <label class="audio-select-label" for="archive-silence">{$t('midi_archive.silence')}</label>
                  <input id="archive-silence" type="number" min="3" max="600" step="1" style="width:4.5rem;" bind:value={archiveCfg.silence_secs} on:change={saveArchiveConfig} />
                  <label class="audio-select-label" for="archive-min-notes">{$t('midi_archive.min_notes')}</label>
                  <input id="archive-min-notes" type="number" min="0" max="100" step="1" style="width:4rem;" bind:value={archiveCfg.min_notes} on:change={saveArchiveConfig} />
                  <label class="audio-select-label" for="archive-min-secs">{$t('midi_archive.min_secs')}</label>
                  <input id="archive-min-secs" type="number" min="0" max="600" step="1" style="width:4.5rem;" bind:value={archiveCfg.min_secs} on:change={saveArchiveConfig} />
                </div>
                {#if archiveStatus.last_error}
                  <div style="font-size:0.72rem; color:var(--error); margin-top:0.3rem;">{archiveStatus.last_error}</div>
                {/if}
                <div style="margin-top:0.6rem; font-size:0.75rem; color:var(--text-secondary);">{$t('midi_archive.recent')}</div>
                {#if archiveFiles.length === 0}
                  <div style="font-size:0.72rem; color:var(--text-muted);">{$t('midi_archive.empty')}</div>
                {:else}
                  <div style="display:flex; flex-direction:column; gap:0.2rem; margin-top:0.3rem;">
                    {#each archiveFiles as f (f.path)}
                      <div style="display:flex; align-items:center; gap:0.5rem; font-size:0.75rem;">
                        <span style="flex:1; min-width:0; overflow:hidden; text-overflow:ellipsis; white-space:nowrap;" title={f.path}>{f.name}</span>
                        <span style="color:var(--text-muted); white-space:nowrap;">{fmtEpoch(f.modified_epoch)} · {Math.max(1, Math.round(f.size_bytes / 1024))} kB</span>
                        <button class="btn btn-ghost btn-sm" on:click={() => playArchiveFile(f)}>{$t('midi_archive.play')}</button>
                        <button class="btn btn-ghost btn-sm" on:click={() => deleteArchiveFile(f)}>{$t('midi_archive.delete')}</button>
                      </div>
                    {/each}
                  </div>
                {/if}
              </div>

              <!-- Afstandsbediening in het netwerk (0.7.38) -->
              <div style="margin-top: 0.85rem; padding-top: 0.75rem; border-top: var(--border-subtle);">
                <label class="swell-toggle" title={$t('remote.enable_title')}>
                  <input
                    type="checkbox"
                    checked={remote.enabled}
                    disabled={remoteBusy}
                    on:change={(e) => setRemoteEnabled(e.target.checked)}
                  />
                  <span>{$t('remote.enable')}</span>
                </label>
                <div style="display:flex; align-items:center; gap:0.5rem; margin-top:0.4rem; flex-wrap:wrap;">
                  <span style="font-size:0.8rem; color:var(--text-secondary);">{$t('remote.port')}</span>
                  <input type="number" min="1024" max="65535" bind:value={remotePortInput} disabled={remote.enabled} style="width:6rem;" />
                  <button class="btn btn-ghost btn-sm" on:click={newRemoteToken}>{$t('remote.new_token')}</button>
                  <span style="font-size:0.75rem; color:{remote.running ? 'var(--success)' : 'var(--text-muted)'};">{remote.running ? $t('remote.running') : $t('remote.stopped')}</span>
                </div>
                {#if remote.enabled && remote.running}
                  <div style="margin-top:0.4rem; font-size:0.8rem; color:var(--text-secondary);">{$t('remote.url_label')}</div>
                  {#each remote.urls as u}
                    <div style="font-family:'JetBrains Mono',monospace; font-size:0.85rem; user-select:text;">{u}</div>
                  {/each}
                  {#if remote.urls.length === 0}
                    <p style="margin:0.2rem 0 0; font-size:0.75rem; color:var(--text-muted);">{$t('remote.no_lan_ip')}</p>
                  {/if}
                  {#if remote.qr_svg}
                    <!-- {@html} is veilig: de SVG komt uit de eigen qrcode-crate, niet uit gebruikersinvoer. -->
                    <div style="background:#fff; padding:8px; width:196px; border-radius:6px; margin-top:0.4rem;">{@html remote.qr_svg}</div>
                  {/if}
                {/if}
                {#if remote.error}
                  <p style="margin:0.3rem 0 0; font-size:0.75rem; color:var(--error);">{remote.error}</p>
                {/if}
                <!-- Welke onderdelen staan op het externe scherm (0.7.39)?
                     De rest van de indeling (volgorde van divisies/registers,
                     zichtbare koppels, koppelplaatsing, knopvorm) volgt
                     automatisch het orgelscherm. -->
                {#if !secondary}
                  <div style="margin-top:0.7rem; padding-top:0.6rem; border-top: var(--border-subtle);">
                    <span style="font-size:0.8rem; color:var(--text-secondary); font-weight:500;">{$t('remote.parts_title')}</span>
                    <div style="display:flex; flex-wrap:wrap; gap:0.2rem 1rem; margin-top:0.3rem;">
                      <label class="swell-toggle" style="margin:0;">
                        <input type="checkbox" checked={remoteParts.couplers !== false} on:change={(e) => setRemotePart('couplers', e.target.checked)} />
                        <span>{$t('remote.part_couplers')}</span>
                      </label>
                      <label class="swell-toggle" style="margin:0;">
                        <input type="checkbox" checked={remoteParts.tremulant !== false} on:change={(e) => setRemotePart('tremulant', e.target.checked)} />
                        <span>{$t('remote.part_tremulant')}</span>
                      </label>
                      <label class="swell-toggle" style="margin:0;">
                        <input type="checkbox" checked={remoteParts.setzer !== false} on:change={(e) => setRemotePart('setzer', e.target.checked)} />
                        <span>{$t('remote.part_setzer')}</span>
                      </label>
                      <label class="swell-toggle" style="margin:0;">
                        <input type="checkbox" checked={remoteParts.volume !== false} on:change={(e) => setRemotePart('volume', e.target.checked)} />
                        <span>{$t('remote.part_volume')}</span>
                      </label>
                      <label class="swell-toggle" style="margin:0;">
                        <input type="checkbox" checked={remoteParts.panic !== false} on:change={(e) => setRemotePart('panic', e.target.checked)} />
                        <span>{$t('remote.part_panic')}</span>
                      </label>
                    </div>
                    <!-- Op organInfo, niet op displayOrgan: dat laatste valt terug
                         op het ingebouwde "Demo Orgel" en is dus nooit leeg —
                         zonder geladen orgel stonden hier vinkjes voor divisies
                         die niet bestaan. Zelfde criterium als de terug-knop. -->
                    {#if organInfo && organInfo.divisions && organInfo.divisions.length > 0}
                      <div style="margin-top:0.45rem; font-size:0.78rem; color:var(--text-secondary);">{$t('remote.parts_divisions')}</div>
                      <div style="display:flex; flex-wrap:wrap; gap:0.2rem 1rem; margin-top:0.2rem;">
                        {#each organInfo.divisions as d (d.name)}
                          <label class="swell-toggle" style="margin:0;">
                            <input type="checkbox" checked={remoteDivisions[d.name] !== false} on:change={(e) => toggleRemoteDivision(d.name, e)} />
                            <span>{d.name}</span>
                          </label>
                        {/each}
                      </div>
                    {/if}
                    <p style="margin:0.35rem 0 0; font-size:0.7rem; color:var(--text-muted); line-height:1.4;">{$t('remote.parts_hint')}</p>
                  </div>
                {/if}
                <p style="margin:0.4rem 0 0; font-size:0.7rem; color:var(--text-muted); line-height:1.4;">{$t('remote.hint')}</p>
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
                <div class="audio-select-row" style="margin-bottom: 0.4rem;">
                  <label class="audio-select-label" for="audio-rate">{$t('settings.sample_rate_choose')}</label>
                  <select id="audio-rate" class="temperament-select"
                    on:change={(e) => dispatch('selectSampleRate', e.target.value ? Number(e.target.value) : null)}>
                    <option value="" selected={!selectedSampleRate}>{$t('settings.sample_rate_device_default')}</option>
                    {#each (supportedSampleRates.length > 0 ? supportedSampleRates : [44100, 48000, 88200, 96000]) as r}
                      <option value={r} selected={r === selectedSampleRate}>{r.toLocaleString()} Hz{r >= 88200 ? ' (HD)' : ''}</option>
                    {/each}
                  </select>
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
                <p class="settings-hint" style="margin: 0 0 0.3rem;">{$t('settings.sample_rate_apply_hint')}</p>
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
              <h3 class="settings-block-title">{$t('settings.loops_title')}</h3>
              {#if !samplesetDir()}
                <p style="font-size:0.75rem; color:var(--text-muted);">{$t('settings.sampleset_no_organ')}</p>
              {:else}
                <p style="font-size:0.72rem; color:var(--text-muted); line-height:1.4; margin:0 0 0.5rem;">
                  {$t('settings.loops_desc')}
                </p>
                <label class="audio-select-row" style="cursor:pointer;">
                  <input type="checkbox" bind:checked={loopOnlyMissing} />
                  <span class="audio-select-label" style="margin-left:0.4rem;">{$t('settings.loops_only_missing')}</span>
                </label>
                <div style="display:flex; gap:0.5rem; margin-top:0.7rem; flex-wrap:wrap;">
                  <button class="btn btn-secondary btn-sm" on:click={scanLoops} disabled={loopBusy}>{$t('settings.sampleset_scan')}</button>
                  <button class="btn btn-primary btn-sm" on:click={applyLoops} disabled={loopBusy || !loopScan || loopScan.files_loopable === 0}>{$t('settings.loops_apply')}</button>
                </div>
                {#if loopBusy}
                  <p style="font-size:0.75rem; color:var(--text-secondary); margin-top:0.5rem;">{$t('settings.sampleset_busy')}</p>
                {/if}
                {#if loopScan}
                  <div style="margin-top:0.6rem; font-size:0.76rem;">
                    <div>{$t('settings.loops_wavs')}: {loopScan.files_scanned} · {$t('settings.loops_without_loop')}: <strong>{loopScan.files_without_loop}</strong> · {$t('settings.loops_loopable')}: <strong>{loopScan.files_loopable}</strong></div>
                    {#if loopScan.errors.length > 0}
                      <div style="color:#d47070; margin-top:0.3rem;">{loopScan.errors.length} ⚠</div>
                    {/if}
                  </div>
                {/if}
                {#if loopApply}
                  <div style="margin-top:0.5rem; font-size:0.76rem; color:var(--success);">
                    {$t('settings.loops_written')}: {loopApply.files_looped} · {$t('settings.loops_skipped')}: {loopApply.files_skipped} · {$t('settings.sampleset_backup')}: {loopApply.backup_dir}
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

            <!-- Terugkoppeling: MIDI-uit naar de fysieke console (registerlampen/display) -->
            <div class="settings-block">
              <h3 class="settings-block-title">{$t('feedback.title')}</h3>
              <p class="settings-hint" style="margin: 0 0 0.5rem;">
                {$t('feedback.desc')}
              </p>
              <div class="audio-select-row">
                <label class="audio-select-label" for="fb-protocol">{$t('feedback.protocol')}</label>
                <select id="fb-protocol" class="temperament-select" bind:value={fbProtocol} on:change={fbApplyConfig}>
                  <option value="off">{$t('feedback.protocol_off')}</option>
                  <option value="note">{$t('feedback.protocol_note')}</option>
                  <option value="lcd">{$t('feedback.protocol_lcd')}</option>
                  <option value="johannus">{$t('feedback.protocol_johannus')}</option>
                </select>
              </div>
              {#if fbProtocol !== 'off'}
                <div class="audio-select-row">
                  <label class="audio-select-label" for="fb-port">{$t('feedback.midi_output')}</label>
                  <select id="fb-port" class="temperament-select" bind:value={fbPort} on:change={fbApplyConfig}>
                    <option value="">{$t('feedback.choose_port')}</option>
                    {#each fbOutputs as p}<option value={p}>{p}</option>{/each}
                  </select>
                </div>
                <div class="audio-select-row">
                  <label class="audio-select-label" for="fb-channel">{$t('settings.midi_channel')}</label>
                  <input id="fb-channel" class="temperament-select" type="number" min="1" max="16"
                    bind:value={fbChannel} on:change={fbApplyConfig} style="width: 5rem;" />
                </div>
                {#if fbProtocol === 'note'}
                  <div class="audio-select-row">
                    <label class="audio-select-label" for="fb-basenote">{$t('feedback.base_note')}</label>
                    <input id="fb-basenote" class="temperament-select" type="number" min="0" max="127"
                      bind:value={fbBaseNote} on:change={fbApplyConfig} style="width: 5rem;" />
                  </div>
                  <p class="settings-hint" style="margin: 0.2rem 0 0;">
                    {$t('feedback.base_note_hint')}
                  </p>
                  <button class="btn btn-ghost btn-sm" style="margin-top: 0.3rem;" on:click={() => fbShowLearnList = !fbShowLearnList}>
                    {fbShowLearnList ? $t('feedback.hide_learn_list') : $t('feedback.learn_per_stop')}
                    {Object.keys(fbLearnedNotes).length ? ` ${$t('feedback.learned_count').replace('{n}', Object.keys(fbLearnedNotes).length)}` : ''}
                  </button>
                  {#if fbShowLearnList}
                    <div style="display: flex; gap: 0.5rem; margin: 0.4rem 0;">
                      <button class="btn btn-secondary btn-sm" on:click={fbLearnAllSeq} disabled={!!fbLearningId && !fbLearnAll}>
                        {fbLearnAll ? $t('feedback.learn_stop') : $t('feedback.learn_all')}
                      </button>
                      <button class="btn btn-ghost btn-sm" on:click={fbClearLearned} disabled={!Object.keys(fbLearnedNotes).length}>{$t('feedback.learn_clear')}</button>
                    </div>
                    {#if fbLearningId}
                      <p class="settings-hint" style="color: var(--warning, #db5);">
                        {$t('feedback.learn_prompt')}
                      </p>
                    {/if}
                    <div class="fb-learn-list">
                      {#each fbRegList as r (r.id)}
                        <div class="fb-learn-row" class:learning={fbLearningId === r.id}>
                          <span class="fb-learn-name" title={r.name}>{r.name}</span>
                          <span class="fb-learn-note">{r.learned != null ? $t('feedback.note_n').replace('{n}', r.learned) : $t('feedback.auto_n').replace('{n}', r.auto)}</span>
                          <button class="btn btn-ghost btn-sm" on:click={() => fbLearnOne(r.id)} disabled={!!fbLearningId}>
                            {fbLearningId === r.id ? '…' : $t('learn.learn')}
                          </button>
                        </div>
                      {/each}
                    </div>
                  {/if}
                {/if}
                <div style="display: flex; gap: 0.5rem; margin-top: 0.5rem;">
                  <button class="btn btn-secondary btn-sm" on:click={fbApplyConfig} disabled={fbBusy}>
                    {fbBusy ? $t('feedback.busy') : $t('feedback.apply_connect')}
                  </button>
                  <button class="btn btn-ghost btn-sm" on:click={fbAllOff}>{$t('feedback.all_off')}</button>
                  <button class="btn btn-ghost btn-sm" on:click={fbRefreshOutputs} title={$t('feedback.refresh_ports')}>↻</button>
                </div>
                <p class="settings-hint" style="margin: 0.4rem 0 0;">
                  {#if fbConfigured}
                    <span style="color: var(--success);">{$t('feedback.active')}</span> {$t('feedback.active_desc')}
                  {:else}
                    {$t('feedback.inactive_hint')}
                  {/if}
                </p>
              {/if}
            </div>

            <!-- Consoleknoppen (pistons): vaste functies inleerbaar op één plek -->
            <div class="settings-block">
              <h3 class="settings-block-title">{$t('pistons.learn_title')}</h3>
              <p class="settings-hint" style="margin: 0 0 0.5rem;">
                {$t('pistons.learn_desc')}
              </p>
              <button class="btn btn-ghost btn-sm" on:click={() => { showActionLearnList = !showActionLearnList; if (showActionLearnList) refreshMidiBindingsFull(); }}>
                {showActionLearnList ? $t('pistons.hide_list') : $t('pistons.learn_buttons')}
              </button>
              {#if showActionLearnList}
                {#if globalLearningAction != null}
                  <p class="settings-hint" style="color: var(--warning, #db5); margin: 0.4rem 0 0;">
                    {$t('pistons.learn_prompt')}
                  </p>
                {/if}
                <div class="fb-learn-list" style="margin-top: 0.4rem;">
                  {#each GLOBAL_ACTIONS as a (a.code)}
                    <div class="fb-learn-row" class:learning={globalLearningAction === a.code}>
                      <span class="fb-learn-name">{a.name}</span>
                      <span class="fb-learn-note">{actionBound[a.code] ? $t('pistons.learned') : '—'}</span>
                      <button class="btn btn-ghost btn-sm" on:click={() => learnGlobalAction(a.code)} disabled={globalLearningAction != null}>
                        {globalLearningAction === a.code ? '…' : $t('learn.learn')}
                      </button>
                      <button class="btn btn-ghost btn-sm" on:click={() => clearGlobalAction(a.code)} disabled={!actionBound[a.code] || globalLearningAction != null} title={$t('pistons.clear_binding')}>×</button>
                    </div>
                  {/each}
                </div>
              {/if}
            </div>

            <!-- Over & feedback: versie, GitHub-feedback en handmatige update-check -->
            <div class="settings-block">
              <h3 class="settings-block-title">{$t('about.title')}</h3>
              <p class="settings-hint" style="margin: 0 0 0.5rem;">
                JM-Orgue {appVersion ? $t('about.version').replace('{version}', appVersion) : ''} — {$t('about.copyright')}
              </p>
              <div style="display: flex; gap: 0.5rem; flex-wrap: wrap;">
                <button class="btn btn-secondary btn-sm" on:click={() => { fbkOpen = true; fbkStatus = null; }} title={$t('about.send_feedback_title')}>
                  {$t('about.send_feedback')}
                </button>
                <button class="btn btn-ghost btn-sm" on:click={openFeedbackPage} title={$t('about.github_title')}>
                  {$t('about.github')}
                </button>
                <button class="btn btn-ghost btn-sm" on:click={manualUpdateCheck} disabled={manualUpdateResult === 'checking'}>
                  {manualUpdateResult === 'checking' ? $t('update.checking') : $t('update.check')}
                </button>
              </div>
              {#if fbkOpen}
                <div class="fbk-overlay">
                  <div class="fbk-modal">
                    <h3>{$t('about.feedback_modal_title')}</h3>
                    <p class="settings-hint" style="margin: 0 0 0.35rem;">
                      {$t('about.feedback_privacy')}
                    </p>
                    <p class="settings-hint" style="margin: 0 0 0.5rem; font-style: italic;">
                      {$t('about.feedback_hobby')}
                    </p>
                    <textarea rows="6" bind:value={fbkMessage} placeholder={$t('about.feedback_placeholder')} style="width: 100%; resize: vertical;"></textarea>
                    <input type="text" bind:value={fbkEmail} placeholder={$t('about.feedback_email_placeholder')} style="width: 100%; margin-top: 0.4rem;" />
                    <label style="display: flex; align-items: center; gap: 0.4rem; margin-top: 0.45rem; cursor: pointer;">
                      <input type="checkbox" bind:checked={fbkIncludeLog} />
                      <span class="settings-hint" style="margin: 0;">
                        {$t('about.feedback_include_log')}
                      </span>
                    </label>
                    {#if fbkStatus === 'ok'}
                      <p class="settings-hint" style="color: var(--success); margin: 0.4rem 0 0;">{$t('about.feedback_sent')}</p>
                    {:else if fbkStatus === 'error'}
                      <p class="settings-hint" style="color: var(--error, #e66); margin: 0.4rem 0 0;">{$t('about.feedback_failed')}</p>
                    {:else if fbkStatus === 'noendpoint'}
                      <p class="settings-hint" style="color: var(--warning, #db5); margin: 0.4rem 0 0;">{$t('about.feedback_no_endpoint')}</p>
                    {/if}
                    <div style="display: flex; gap: 0.5rem; justify-content: flex-end; margin-top: 0.6rem;">
                      <button class="btn btn-ghost btn-sm" on:click={() => { fbkOpen = false; }}>{$t('actions.close')}</button>
                      <button class="btn btn-primary btn-sm" on:click={fbkSubmit} disabled={!fbkMessage.trim() || fbkStatus === 'sending'}>
                        {fbkStatus === 'sending' ? $t('about.feedback_sending') : $t('about.feedback_send')}
                      </button>
                    </div>
                  </div>
                </div>
              {/if}
              {#if manualUpdateResult === 'uptodate'}
                <p class="settings-hint" style="margin: 0.4rem 0 0;">{$t('update.up_to_date')}</p>
              {:else if manualUpdateResult === 'failed'}
                <p class="settings-hint" style="color: var(--warning, #db5); margin: 0.4rem 0 0;">{$t('update.check_failed')}</p>
              {:else if manualUpdateResult && manualUpdateResult.version}
                <p class="settings-hint" style="margin: 0.4rem 0 0;">
                  {$t('update.new_version_prefix')} <b>{manualUpdateResult.version}</b> {$t('update.new_version_suffix')}
                  {#if manualUpdateResult.update}
                    <button class="btn btn-primary btn-sm" on:click={startManualUpdate} title={$t('update.install_now_title')}>{$t('update.install_now')}</button>
                    <button class="btn btn-ghost btn-sm" on:click={openManualUpdate}>{$t('update.download')}</button>
                  {:else}
                    <button class="btn btn-primary btn-sm" on:click={openManualUpdate}>{$t('update.download')}</button>
                  {/if}
                </p>
                {#if !manualUpdateResult.update && manualUpdateMac}
                  <p class="settings-hint" style="margin: 0.2rem 0 0;">{$t('update.macos_manual')}</p>
                {/if}
              {/if}
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
      <h2 class="empty-state-title">{$t('status.no_organ_loaded')}</h2>
      <p class="empty-state-text">{$t('register_panel.select_from_library')}</p>
      <button class="btn btn-primary" on:click={() => showOrganBrowser = true}>
        {$t('library.open_button')}
      </button>
    </div>
  {/if}

{#if pedalLearnModal}
  <div class="kb-learn-overlay">
    <div class="kb-learn-modal">
      <h3>{pedalLearnModal.kind === 'zwel' ? $t('learn.swell_pedal_title').replace('{name}', pedalLearnModal.division) : $t('learn.crescendo_title')}</h3>
      {#if pedalLearnModal.step === 1}
        <div class="kb-learn-step kb-learn-wait">
          <span class="learning-indicator"></span>
          {$t('learn.pedal_low_prompt')}
        </div>
      {:else if pedalLearnModal.step === 2}
        <div class="kb-learn-step kb-learn-ok">
          &#10004; {$t('learn.pedal_low_detected').replace('{cc}', pedalLearnModal.first.cc).replace('{channel}', pedalLearnModal.first.channel + 1).replace('{value}', pedalLearnModal.first.value)}
        </div>
        <div class="kb-learn-step kb-learn-wait">
          <span class="learning-indicator"></span>
          {$t('learn.pedal_high_prompt')}
        </div>
      {:else if pedalLearnModal.step === 'klaar'}
        <div class="kb-learn-step kb-learn-ok">&#10004; {$t('learn.pedal_learned')} {pedalLearnModal.msg}</div>
      {:else}
        <div class="kb-learn-step kb-learn-fout">&#10006; {pedalLearnModal.msg}</div>
      {/if}
      <div class="kb-learn-knoppen">
        {#if pedalLearnModal.step === 'fout'}
          <button class="btn btn-secondary btn-sm" on:click={async () => { const k = pedalLearnModal.kind; const d = pedalLearnModal.division; pedalLearnModal = null; await cancelPedalLearn(); learnPedalFlow(k, d); }}>{$t('learn.retry')}</button>
        {/if}
        <button class="btn btn-ghost btn-sm" on:click={() => { const klaar = pedalLearnModal.step === 'klaar'; pedalLearnModal = null; if (!klaar) cancelPedalLearn(); }}>
          {pedalLearnModal.step === 'klaar' ? $t('actions.close') : $t('actions.cancel')}
        </button>
      </div>
    </div>
  </div>
{/if}
{#if kbLearnModal}
  <div class="kb-learn-overlay">
    <div class="kb-learn-modal">
      <h3>{$t('learn.keyboard_title').replace('{name}', kbLearnModal.division)}</h3>
      {#if kbLearnModal.step === 1}
        <div class="kb-learn-step kb-learn-wait">
          <span class="learning-indicator"></span>
          {$t('learn.key_low_prompt')}
        </div>
      {:else if kbLearnModal.step === 2}
        <div class="kb-learn-step kb-learn-ok">
          ✔ {$t('learn.key_low_detected')} <strong>{noteName(kbLearnModal.first.note)}</strong> {$t('learn.key_midi_channel').replace('{channel}', kbLearnModal.first.channel + 1)}
        </div>
        <div class="kb-learn-step kb-learn-wait">
          <span class="learning-indicator"></span>
          {$t('learn.key_high_prompt')}
        </div>
      {:else if kbLearnModal.step === 'klaar'}
        <div class="kb-learn-step kb-learn-ok">✔ {$t('learn.keyboard_learned')} {kbLearnModal.msg}</div>
      {:else}
        <div class="kb-learn-step kb-learn-fout">✖ {kbLearnModal.msg}</div>
      {/if}
      <div class="kb-learn-knoppen">
        {#if kbLearnModal.step === 'fout'}
          <button class="btn btn-secondary btn-sm" on:click={() => { const d = kbLearnModal.division; closeKbLearn(); learnChannel(d); }}>{$t('learn.retry')}</button>
        {/if}
        <button class="btn btn-ghost btn-sm" on:click={closeKbLearn}>
          {kbLearnModal.step === 'klaar' ? $t('actions.close') : $t('actions.cancel')}
        </button>
      </div>
    </div>
  </div>
{/if}
</main>
