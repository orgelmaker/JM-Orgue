<script>
  import { onMount, tick } from 'svelte';
  import { fade } from 'svelte/transition';
  import { invoke } from '@tauri-apps/api/core';

  import Header from './components/Header.svelte';
  import Console from './components/Console.svelte';
  import StatusBar from './components/StatusBar.svelte';
  import PanelApp from './components/PanelApp.svelte';
  import NotationWindow from './components/NotationWindow.svelte';
  import { t, tx } from './lib/i18n.js';
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
  // Gevraagde samplerate (0.7.48). null = wat het apparaat zelf als standaard
  // opgeeft; een waarde die het apparaat niet kan, laat de backend staan en
  // schrijft een waarschuwing in het log.
  let selectedSampleRate = null;
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

  // Opstart-splash: dekt de eerste seconden af zodat het startscherm in één
  // keer compleet in beeld komt — inclusief de update-balk, die anders ná de
  // bibliotheek inplofte en alles naar beneden duwde. Alleen het hoofdvenster.
  let booting = true;
  // Splash-tekst "Audio-uitgang (ASIO) wordt gestart…" zodra de start op de
  // uitgestelde ASIO-wissel wacht (0.7.43, zie audioReadyWait in onMount).
  let audioStarting = false;
  let appVersion = '';

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

  // ---- Update-check + automatische update (0.7.40) ----
  // Stil bij elke start; toont een wegklikbare balk wanneer er een nieuwere
  // release staat. Wegklikken onthoudt die ene versie (volgende versie meldt
  // zich gewoon weer). Faalt geluidloos zonder internet.
  // Is de update automatisch te installeren (Windows, latest.json gevonden),
  // dan staat er een knop "Nu bijwerken" naast de downloadpagina: downloaden
  // met voortgang, opslaan, installeren en opnieuw starten.
  let updateInfo = null; // { version, url, notes, update, auto } of null
  let updateBezig = false;      // download/installatie loopt
  let updateVoortgang = null;   // { fase: 'download'|'install', pct, gedaan, totaal }
  let updateFout = null;        // nette melding; de app blijft gewoon draaien
  let updateOpMac = false;      // macOS: alleen de downloadpagina (zie lib/updater.js)
  const updateMb = (n) => ((n || 0) / (1024 * 1024)).toFixed(1);

  // De splash wacht ook op de eerste ophaalronde van de online samplesets
  // (Console.refreshOnlineSets). Zonder dat verscheen het blok "Online
  // beschikbaar" op een trage verbinding een tot twee seconden ná het
  // startscherm en sprong de bibliotheek alsnog na. Console meldt zich via
  // on:onlineSetsSettled (ook bij een mislukte fetch); de race met dezelfde
  // 4 s als de update-check zorgt dat een dode verbinding de opstart nooit
  // ophoudt.
  let meldOnlineSetsKlaar = () => {};
  const onlineSetsSettled = new Promise((r) => { meldOnlineSetsKlaar = r; });

  async function runUpdateCheck() {
    try {
      const { zoekUpdate, autoUpdateOndersteund } = await import('./lib/updater.js');
      const { getVersion } = await import('@tauri-apps/api/app');
      const current = await getVersion();
      appVersion = current;
      updateOpMac = !autoUpdateOndersteund();
      const upd = await zoekUpdate(current);
      if (upd && localStorage.getItem('jm-orgue-update-dismissed') !== upd.version) {
        updateInfo = upd;
      }
    } catch (e) { /* stil — update-check mag de opstart nooit storen */ }
  }
  async function openUpdatePage() {
    try { await invoke('open_external_url', { url: updateInfo.url }); } catch (e) {}
  }
  function dismissUpdate() {
    try { localStorage.setItem('jm-orgue-update-dismissed', updateInfo.version); } catch (e) {}
    updateInfo = null;
    updateFout = null;
  }

  // Moet er eerst bevestigd worden? De app sluit af en start opnieuw op, dus
  // alles wat "loopt" is een reden om het even te vragen: een geladen orgel
  // (iemand zit te spelen), een lopende opname, of een actieve
  // afstandsbediening (iemand anders bedient het orgel op een tablet).
  async function updateBevestigingNodig() {
    if (organInfo) return true;
    try { const rec = await invoke('get_recording_status'); if (rec && rec.recording) return true; } catch (e) {}
    try { const r = await invoke('get_remote_status'); if (r && r.running) return true; } catch (e) {}
    return false;
  }

  // Eén knop: downloaden (met voortgang), opslaan, installeren, herstarten.
  // `info` maakt het aanroepbaar vanuit Algemene Instellingen (Console) met
  // het resultaat van de handmatige controle.
  async function startUpdate(info = null) {
    const doel = info || updateInfo;
    if (!doel || !doel.update || updateBezig) return;
    // Vlag VÓÓR de eerste await: updateBevestigingNodig() wacht op twee
    // invokes, en twee snelle klikken (balk + knop in Algemene Instellingen)
    // zonder geladen orgel kwamen anders allebei voorbij de guard hierboven en
    // startten de download dubbel. De balk toont pas voortgang zodra
    // updateVoortgang gezet is, dus tijdens de bevestigingsvraag staat er nog
    // gewoon de melding met de knoppen.
    updateBezig = true;
    updateInfo = doel; // balk toont vanaf nu deze update (ook bij een handmatige controle)
    let bevestigd = true;
    try {
      // LET OP: de dialoog-plugin van Tauri vervangt window.confirm door een
      // asynchrone variant (init-iife.js) die een Promise teruggeeft. Zonder
      // await gold elke vraag als "ja" zonder dat er iets verscheen (0.7.43).
      if (await updateBevestigingNodig()) bevestigd = await window.confirm(tx('update.confirm_install'));
    } catch (e) { bevestigd = false; }
    if (!bevestigd) {
      updateBezig = false; // geannuleerd: guard weer vrijgeven
      return;
    }
    updateFout = null;
    updateVoortgang = { fase: 'download', pct: null, gedaan: 0, totaal: 0 };
    try {
      const { installeerUpdate } = await import('./lib/updater.js');
      await installeerUpdate(doel.update, {
        onVoortgang: (p) => { updateVoortgang = p; },
        voorInstalleren: bewaarVoorAfsluiten,
      });
      // Windows: onbereikbaar (het proces is al beëindigd door de installer).
    } catch (e) {
      // Mislukte download, handtekening of installatie: gewoon doorspelen, met
      // een nette melding en de downloadpagina als terugval.
      updateBezig = false;
      updateVoortgang = null;
      updateFout = String(e?.message || e);
      console.error('Bijwerken mislukt:', e);
    }
  }

  // ---- Hoofdvenster-geometrie (globaal — het hoofdvenster is er maar één,
  // dus bewust níet per orgel zoals de panel-state). Zelfde aanpak als
  // PanelApp.saveGeometry: logische coördinaten, minimized negeren, en bij
  // gemaximaliseerd alleen de vlag bijwerken zodat "herstellen" na een
  // herstart op de laatst bekende vensterpositie terugvalt.
  // FYSIEKE pixels (0.7.21): logische coördinaten zijn dubbelzinnig bij meerdere
  // beeldschermen met verschillende schaal — precies de opstelling van een
  // orgelconsole. Fysieke coördinaten zijn absoluut over alle monitoren.
  const MAIN_GEOM_KEY = 'jm-orgue-main-window';
  let mainGeomTimer = null;
  // Wijzigingen in de monitoropstelling (scherm valt uit, of is bij het booten
  // van het testorgel nog niet actief): Windows verplaatst vensters dan zélf,
  // en die noodpositie mag de bewaarde stand niet overschrijven. We pollen het
  // aantal monitoren; kort na een wijziging worden geometrie-saves overgeslagen.
  let monCount = null;
  let monChangedAt = 0;
  async function pollMonitors() {
    try {
      const { availableMonitors } = await import('@tauri-apps/api/window');
      const n = (await availableMonitors()).length;
      if (monCount !== null && n !== monCount) monChangedAt = Date.now();
      monCount = n;
    } catch (e) {}
  }
  // Diagnose: laatste restore-uitkomst (stap of fout) — uitleesbaar via
  // localStorage bij meerschermen-problemen op afstand (testorgel).
  const geomDbg = (s) => { try { localStorage.setItem('jm-orgue-geom-last-restore', `${s} @${new Date().toISOString()}`); } catch (e) {} };
  async function restoreMainGeometry() {
    try {
      geomDbg('start');
      const st = JSON.parse(localStorage.getItem(MAIN_GEOM_KEY) || 'null');
      if (!st) { geomDbg('geen-opslag'); return; }
      const { getCurrentWindow, PhysicalPosition, PhysicalSize } = await import('@tauri-apps/api/window');
      const w = getCurrentWindow();
      if (st.maximized) {
        // Eerst het venster op het juiste SCHERM zetten (anker uit de
        // maximized-save, of anders de laatste windowed-positie), dán pas
        // maximaliseren: maximize() pakt het scherm waar het venster op dat
        // moment staat. +64 zodat het punt ruim binnen de monitor valt (de
        // outerPosition van een gemaximaliseerd venster ligt door de
        // vensterrand iets búiten de werkruimte).
        const ax = (typeof st.mpx === 'number') ? st.mpx : (typeof st.px === 'number' ? st.px : null);
        const ay = (typeof st.mpy === 'number') ? st.mpy : (typeof st.py === 'number' ? st.py : null);
        if (ax !== null && ay !== null && ax > -30000 && ay > -30000) {
          await w.setPosition(new PhysicalPosition(ax + 64, ay + 64));
        }
        await w.maximize();
        geomDbg('gemaximaliseerd-hersteld');
        return;
      }
      // Alleen het fysieke formaat (0.7.21) herstellen; oude logische opslag
      // (x/y/width/height) bewust negeren — verkeerd terugrekenen is erger
      // dan één keer opnieuw neerzetten.
      if (typeof st.px !== 'number') return;
      if (st.px > -30000 && st.py > -30000) {
        await w.setPosition(new PhysicalPosition(st.px, st.py));
      }
      if (st.pw >= 400 && st.ph >= 300) {
        const apply = () => w.setSize(new PhysicalSize(Math.min(st.pw, 16000), Math.min(st.ph, 16000)));
        await apply();
        // Cross-DPI-controle: verhuist het venster hierboven naar een scherm
        // met een andere schaal, dan kan de asynchrone DPI-herschaling de
        // gezette maat overschrijven. Eén keer verifiëren en zonodig opnieuw.
        setTimeout(async () => {
          try {
            const cur = await w.innerSize();
            if (Math.abs(cur.width - st.pw) > 4 || Math.abs(cur.height - st.ph) > 4) await apply();
          } catch (e) {}
        }, 250);
      }
      geomDbg('windowed-hersteld');
    } catch (e) { geomDbg(`FOUT: ${e?.message || e}`); /* standaardpositie is prima */ }
  }
  async function saveMainGeometry() {
    try {
      // Vlak na een monitor-wijziging niet saven: de positie is dan vaak een
      // door Windows gekozen noodpositie, niet de wens van de gebruiker.
      if (Date.now() - monChangedAt < 8000) return;
      const { getCurrentWindow } = await import('@tauri-apps/api/window');
      const w = getCurrentWindow();
      if (await w.isMinimized()) return; // Windows meldt dan -32000,-32000
      const prev = (() => { try { return JSON.parse(localStorage.getItem(MAIN_GEOM_KEY) || '{}') || {}; } catch (e) { return {}; } })();
      if (await w.isMaximized()) {
        // Ook het monitor-anker vastleggen: de outerPosition van het
        // gemaximaliseerde venster identificeert het scherm, zodat het herstel
        // op het júiste scherm maximaliseert (3-schermen-testorgel).
        const pos = await w.outerPosition();
        const patch = { ...prev, maximized: true };
        if (pos.x > -30000 && pos.y > -30000) { patch.mpx = pos.x; patch.mpy = pos.y; }
        localStorage.setItem(MAIN_GEOM_KEY, JSON.stringify(patch));
        return;
      }
      const pos = await w.outerPosition();   // fysieke pixels
      const size = await w.innerSize();      // fysieke pixels
      if (pos.x < -30000 || pos.y < -30000) return;
      localStorage.setItem(MAIN_GEOM_KEY, JSON.stringify({
        px: pos.x, py: pos.y, pw: size.width, ph: size.height,
        maximized: false,
      }));
    } catch (e) {}
  }
  async function initMainGeometryTracking() {
    try {
      const { getCurrentWindow } = await import('@tauri-apps/api/window');
      const w = getCurrentWindow();
      const schedule = () => {
        if (mainGeomTimer) clearTimeout(mainGeomTimer);
        mainGeomTimer = setTimeout(saveMainGeometry, 300);
      };
      await w.onMoved(schedule);
      await w.onResized(schedule);
      pollMonitors();
      setInterval(pollMonitors, 3000);
      // NB: bewust géén eigen onCloseRequested meer — de centrale close-handler
      // (verderop in onMount) bewaart de geometrie expliciet vóór destroy();
      // twee close-handlers raceten met elkaar en de write verloor het weleens.
    } catch (e) { /* niet in Tauri */ }
  }

  onMount(async () => {
    if (isPanel || isNotation) return; // panel-/notatievensters regelen hun eigen state

    // Tijdlijn van de opstart-splash: hij verdwijnt pas als de init klaar is
    // én de update-check is afgerond (of 4 s om zijn), met een ondergrens van
    // 1,5 s zodat hij niet even opflitst. Zo staat de update-balk er al vóór
    // het wegfaden en springt er achteraf niets meer na.
    const bootStart = performance.now();
    // Noodrem: wat er in de init ook blijft hangen, na 8 s is de splash weg —
    // een onklikbaar startscherm is erger dan een balk die alsnog inploft.
    const bootFailsafe = setTimeout(() => { booting = false; audioStarting = false; }, 18000);

    // Hoofdvenster-geometrie herstellen + vastleggen (extra schermen doen dit
    // al zelf in PanelApp; het hoofdvenster had tot 0.7.11 géén persistentie
    // en opende dus altijd op de standaardpositie).
    await restoreMainGeometry();
    initMainGeometryTracking();

    // Update-check meteen bij de start (was: 3 s uitgesteld "ná de drukke
    // opstart"). Die vertraging kan vervallen omdat dit één fetch is op de
    // netwerkthread van de webview (lib/github.js) die de audio-opstart in Rust
    // niet raakt — en de splash wacht er nu op, zodat de balk er staat vóórdat
    // het startscherm verschijnt in plaats van er later overheen te ploffen.
    // BEWUST alleen hier en via de knop "Controleer op updates" in Algemene
    // Instellingen: een melding die tijdens het spelen in beeld ploft stoort een
    // dienst of opname. Nieuwe versies komen dus bij de eerstvolgende start
    // binnen (0.7.34 probeerde een uurlijkse check — teruggedraaid op verzoek).
    // De fetch heeft geen eigen time-out; vandaar de race met 4 s, anders hangt
    // de splash zonder internet op de socket-time-out van het besturingssysteem.
    const updateSettled = Promise.race([
      runUpdateCheck(),
      new Promise((r) => setTimeout(r, 4000)),
    ]);
    // Zelfde grens voor de online-sampleset-lijst: klaar is klaar, maar nooit
    // langer dan 4 s wachten (en zonder netwerk hangt de opstart dus niet).
    const onlineSetsWait = Promise.race([
      onlineSetsSettled,
      new Promise((r) => setTimeout(r, 4000)),
    ]);

    await refreshDevices();
    await refreshStatus();

    // Frontend draait: startsein voor de uitgestelde ASIO-wissel in de backend
    // (0.7.43; die wachtte tot nu toe een vaste 10 s na de processtart). Hier,
    // direct na de apparaat-enumeratie: de rest van de init (profielen, MIDI)
    // raakt de audio-host niet en mag met de wissel overlappen.
    try { await invoke('frontend_ready'); } catch (e) { /* niet in Tauri context */ }

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
            const hadOrgan = !!organInfo;
            organInfo = fresh;
            // Orgel buiten deze UI om geladen (test-API, of de webview is
            // herladen terwijl de backend zijn orgel hield): niet in de
            // bibliotheek blijven staan met een spelend orgel erachter.
            if (!hadOrgan && fresh && showOrganBrowser && !loading) setView('orgel');
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
        // Zelfde opslagronde als vóór een update-installatie (bewaarVoorAfsluiten).
        await bewaarVoorAfsluiten();
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
      // Kruisje op een extra scherm = de HELE software afsluiten (0.7.21).
      // close() triggert de centrale close-handler hierboven (opslaan +
      // resterende panelen sluiten + destroy).
      panelUnlisteners.push(await listen('jm-orgue:app-quit', async () => {
        try {
          const { getCurrentWindow } = await import('@tauri-apps/api/window');
          await getCurrentWindow().close();
        } catch (e) {}
      }));
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
      // Kanaalkeuze gewijzigd op een extra scherm: profielkopie bijwerken en de
      // spiegel in het hoofdvenster verversen (divisionChannelsVersion).
      panelUnlisteners.push(await listen('jm-orgue:division-channels-changed', () => {
        syncActiveProfileChannels();
      }));
      // Instellingen gewijzigd in een paneel: lijsten verversen + autosaven
      // (zelfde route als Console's eigen refreshMidiMappings-event).
      panelUnlisteners.push(await listen('jm-orgue:settings-changed', (e) => {
        try { refreshMidiMappings(); } catch (err) {}
        if (e?.payload?.scope === 'persist') persistOrganSettings();
        else scheduleAutoSave();
      }));
    } catch (e) { /* niet in Tauri context */ }

    // Init is klaar: de splash mag weg zodra ook de update-check is afgerond
    // (of de 4 s om zijn) én hij minstens ~1,5 s te zien was. De autoload
    // hieronder valt er bewust buiten: die toont zijn eigen voortgangsbalk, en
    // een grote sampleset achter een splash zonder voortgang oogt als een
    // vastloper (les uit 0.6.4).
    // Wachten tot de audio-uitgang definitief is (status.audioReady — meteen
    // waar zonder ASIO-voorkeur), zodat het laatste orgel één keer op de
    // definitieve audio-thread geladen wordt in plaats van twee keer, de
    // tweede keer midden in het spel. Bovengrens 15 s: een hangende wissel
    // houdt de start nooit tegen.
    const audioReadyWait = new Promise((r) => {
      const t0 = Date.now();
      const tick = () => {
        if (status.audioReady || Date.now() - t0 > 15000) { audioStarting = false; r(); return; }
        if (Date.now() - t0 > 1200) audioStarting = true;
        setTimeout(tick, 100);
      };
      tick();
    });

    Promise.all([
      updateSettled,
      onlineSetsWait,
      audioReadyWait,
      new Promise((r) => setTimeout(r, Math.max(0, 1500 - (performance.now() - bootStart)))),
    ]).then(() => { clearTimeout(bootFailsafe); booting = false; });

    // Herstel laatst geopend orgel als er een sessie is — instelbaar via
    // "Laatst geopende orgel automatisch laden" (Algemene Instellingen).
    // De extra registerschermen van dat orgel heropent loadOrgan/loadFromFolder
    // zelf (per orgel onthouden — zie Console.restorePanels).
    try {
      await audioReadyWait;
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

  // Eén doorgang voor élke weergavewissel (tabs in de balk, F1/F2/F3 en de
  // knoppen in de Console). Zonder geladen orgel bestaat er geen zinvolle
  // Orgel- of Orgel-Instellingen-weergave — die toonden tot nu toe het
  // ingebouwde "Demo Orgel" — dus die routes leiden terug naar de bibliotheek.
  // Algemene Instellingen mag wél zonder orgel, maar moet de bibliotheek dan
  // expliciet verlaten: in de Console wint de bibliotheek-tak van de
  // instellingen-tak. Zo kun je nooit meer stranden in de instellingen.
  function setView(view) {
    if (!organInfo && !loading && view !== 'algemene-instellingen') {
      activeView = 'orgel';
      showOrganBrowser = true;
      return;
    }
    activeView = view;
    showOrganBrowser = false;
  }

  function handleKeyDown(event) {
    // Ignore if typing in input
    if (event.target.tagName === 'INPUT' || event.target.tagName === 'TEXTAREA') return;

    // Global function keys (always handled)
    if (event.key === 'F1') { event.preventDefault(); setView('orgel'); return; }
    if (event.key === 'F2') { event.preventDefault(); setView('orgel-instellingen'); return; }
    if (event.key === 'F3') { event.preventDefault(); setView('algemene-instellingen'); return; }
    // Escape gaat altijd naar de bibliotheek, óók zonder geladen orgel: dat is
    // de vluchtroute als je in de instellingen bent beland. Zonder orgel ook de
    // weergave terugzetten, anders opent het orgel dat je zo kiest meteen in de
    // instellingen in plaats van aan de klavieren.
    if (event.key === 'Escape') {
      event.preventDefault();
      if (!organInfo) activeView = 'orgel';
      showOrganBrowser = true;
      return;
    }

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
    const savedRate = parseInt(localStorage.getItem('jm-orgue-audio-rate') || '0', 10);
    selectedSampleRate = savedRate > 0 ? savedRate : null;

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

  // Handmatige kanaalwijziging (chips per klavier of uitgangspaar) terwijl het
  // actieve profiel een kanaallijst heeft: die lijst mee-updaten, anders zet de
  // volgende profielwissel de oude profielkanalen stilletjes terug. De backend-
  // override is door set_division_output_channels al bijgewerkt; hier alleen de
  // opgeslagen profielkopie en de dedupe-sleutel gelijktrekken.
  let divisionChannelsVersion = 0;
  async function syncActiveProfileChannels() {
    divisionChannelsVersion += 1;
    const kind = audioProfiles.active;
    const p = kind && audioProfiles[kind];
    if (!p || !p.channels || !organInfo?.divisions?.length) return;
    try {
      const chans = await invoke('get_division_output_channels');
      const map = new Map(p.channels);
      organInfo.divisions.forEach((d, i) => map.set(d.name, Array.isArray(chans?.[i]) ? chans[i] : []));
      p.channels = [...map];
      audioProfiles = audioProfiles;
      persistAudioProfiles();
      lastPushedChannelOverride = JSON.stringify(p.channels);
    } catch (e) {}
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
      error = tx('audio.profile_device_unavailable').replace('{label}', label).replace('{device}', p.device);
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
    // Zelfde host + apparaat + buffer als wat nu speelt (bv. hoofdtelefoon =
    // ander uitgangspaar op dezelfde interface): de stream NIET herbouwen en
    // het orgel niet herladen — de kanaal-override is al gepusht en door de
    // engine direct toegepast (set_profile_channel_override). Dat maakt de
    // wissel klikvrij en onmiddellijk, zoals een monitor-set-wissel in een DAW.
    // Alleen de spiegel van de kanaalkeuze in de vensters verversen.
    const sameHost = (p.host || '').toLowerCase() === (status.audioHost || '').toLowerCase();
    const sameDevice = !!p.device && p.device === status.audioDevice;
    const sameBuffer = (p.bufferFrames || 0) === (status.bufferFrames || 0);
    if (status.audioRunning && sameHost && sameDevice && sameBuffer) {
      divisionChannelsVersion += 1;
      return 'ok';
    }
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
    // hoofdtelefoon; zonder actieve stand (zeldzaam sinds de 0.7.3-migratie,
    // bv. net een profiel gewist): naar het profiel dat wél bestaat, en bestaat
    // er geen enkel profiel dan Speakers — de normale stand; de huidige uitgang
    // wordt dan hieronder als Speakers vastgelegd, niet als Hoofdtelefoon.
    const target = kind
      || (audioProfiles.active === 'headphones' ? 'speakers'
        : audioProfiles.active === 'speakers' ? 'headphones'
        : (!audioProfiles.speakers && audioProfiles.headphones ? 'headphones' : 'speakers'));
    const p = audioProfiles[target];
    if (!p) {
      if (target === 'headphones') {
        // Een niet-ingesteld hóófdtelefoonprofiel stilzwijgend vullen met de
        // huidige (speaker-)uitgang gaf "er gebeurt niets" — de knop leek stuk
        // ("schakeling hoofdtelefoon lukt niet") en beide profielen werden
        // identiek. Eerlijk zeggen wat er moet gebeuren in plaats van raden.
        error = tx('audio.headphones_profile_not_set');
        return;
      }
      // Speakers zonder profiel: de huidige (echt spelende) uitgang vastleggen
      // is dáár wel de juiste aanname — dat is de normale stand (0.7.3-migratie).
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
      const label = target === 'headphones' ? tx('settings.audio_profile_headphones') : tx('settings.audio_profile_speakers');
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
        sampleRate: selectedSampleRate || null,
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
        error = res.message || tx('audio.switch_taking_long');
        return 'busy';
      }
      if (res.switched) {
        if (selectedAudioHost) localStorage.setItem('jm-orgue-audio-host', selectedAudioHost);
        if (selectedAudioDevice) localStorage.setItem('jm-orgue-audio-device', selectedAudioDevice);
        if (selectedSampleRate) localStorage.setItem('jm-orgue-audio-rate', String(selectedSampleRate));
        else localStorage.removeItem('jm-orgue-audio-rate');
        if (selectedBufferFrames) localStorage.setItem('jm-orgue-audio-buffer', String(selectedBufferFrames));
        else localStorage.removeItem('jm-orgue-audio-buffer');
        return 'ok';
      }
      error = res.message || tx('audio.switch_failed');
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
        polyphony: s.polyphony,
        renderLoad: s.render_load,
        renderPeak: s.render_peak,
        renderOverloads: s.render_overloads,
        rtDrops: s.rt_drops,
        peakLeft: s.peak_left,
        peakRight: s.peak_right,
        sampleRate: s.sample_rate,
        audioHost: s.audio_host,
        audioDevice: s.audio_device,
        channels: s.channels,
        bufferFrames: s.buffer_frames,
        midiArchiving: s.midi_archiving,
        audioReady: s.audio_ready,
        renderFrames: s.render_frames,
        backendReloads: s.backend_reloads
      };
      volgAudioOverbelasting(s);
      volgAsioHerstartAdvies(s);
    } catch (e) {
      // Ignore polling errors
    }
  }

  // ===== Advies bij herhaalde audio-overbelasting =====
  // Een te kleine buffer (32/48 frames) is de vaakst voorkomende oorzaak van
  // "het hapert": bij een crescendo-trapwissel of registerwissel moet de
  // callback in 0,67 ms tientallen stemmen starten en stoppen. Eén piek is
  // normaal (orgel laden, tutti); pas bij HERHALING binnen ~10 s is het advies
  // terecht. We tellen daarvoor per 100 ms-poll de metingen met piek > 100 % van
  // de buffertijd, plus hoeveel zware callbacks de backend er sindsdien bij
  // kreeg (render_overloads) — dat laatste onderscheidt één trage callback met
  // een langzaam dalende piek van echte, doorlopende overbelasting.
  let overbelastingMetingen = [];   // [{ t, nieuw }]
  let vorigeOverloadTeller = null;
  let audioOverbelastAdvies = false;
  // Wegklikken geldt tien minuten, niet de hele sessie: blijft de buffer te
  // klein, dan mag het advies bij nieuwe overbelasting terugkomen.
  let audioOverbelastGenegeerdTot = 0;
  const OVERBELAST_VENSTER_MS = 10000;
  const OVERBELAST_NEGEER_MS = 10 * 60 * 1000;
  function volgAudioOverbelasting(s) {
    const nu = Date.now();
    const teller = typeof s.render_overloads === 'number' ? s.render_overloads : 0;
    const nieuw = vorigeOverloadTeller == null ? 0 : Math.max(0, teller - vorigeOverloadTeller);
    vorigeOverloadTeller = teller;
    if ((s.render_peak || 0) > 1.0) overbelastingMetingen.push({ t: nu, nieuw });
    while (overbelastingMetingen.length && nu - overbelastingMetingen[0].t > OVERBELAST_VENSTER_MS) overbelastingMetingen.shift();
    const zwareCallbacks = overbelastingMetingen.reduce((a, m) => a + m.nieuw, 0);
    // render_frames is de werkelijke framegrootte van de callback; buffer_frames
    // is 0 wanneer de driver zijn eigen paneelinstelling aanhoudt (ESI/RME), en
    // dan verscheen het advies tot 0.7.42 nooit.
    const buf = s.render_frames || s.buffer_frames || 0;
    audioOverbelastAdvies = Date.now() > audioOverbelastGenegeerdTot && !!s.audio_running
      && overbelastingMetingen.length >= 3 && zwareCallbacks >= 3
      && buf > 0 && buf < 128;
  }

  // ===== ASIO-herstart-advies (0.7.43) =====
  // De backend herstart de app niet meer zelf wanneer de ASIO-driver in dit
  // proces niet opnieuw kan starten (vrijgegeven voor een WASAPI-wissel, of dood
  // na een stilstand). Hij meldt het in de status; wij tonen een balk met een
  // herstart-knop. Wegklikken geldt tot het advies vervalt (ASIO werkt weer).
  let asioHerstartAdvies = null;
  let asioHerstartGenegeerd = false;
  let asioHerstartBezig = false;
  function volgAsioHerstartAdvies(s) {
    const advies = s.asio_restart_advice || null;
    if (!advies) { asioHerstartAdvies = null; asioHerstartGenegeerd = false; return; }
    if (!asioHerstartGenegeerd) asioHerstartAdvies = advies;
  }
  async function herstartVoorAsio() {
    if (asioHerstartBezig) return;
    if (organInfo && !(await window.confirm(tx('audio.asio_restart_confirm')))) return;
    asioHerstartBezig = true;
    try {
      await bewaarVoorAfsluiten();
      await invoke('restart_for_asio');
    } catch (e) {
      error = String(e?.message || e);
      asioHerstartBezig = false;
    }
  }

  // Knop in de melding: buffer op 128 frames zetten via het gewone
  // applyAudioOutput-pad (incl. herladen van het orgel) en het ACTIEVE profiel
  // meteen bijwerken — anders zet de eerstvolgende profielwissel de oude,
  // te kleine buffer stilletjes terug.
  async function zetBufferOp128() {
    // Bewust GEEN 'genegeerd'-vlag: mislukt de wissel (apparaat weg), dan blijft
    // de buffer klein en mag het advies na ~10 s nieuwe overbelasting terugkomen.
    audioOverbelastAdvies = false;
    overbelastingMetingen = [];
    vorigeOverloadTeller = null;
    selectedBufferFrames = 128;
    const kind = audioProfiles.active;
    if (kind && audioProfiles[kind]) {
      audioProfiles[kind].bufferFrames = 128;
      audioProfiles = audioProfiles;
      persistAudioProfiles();
    }
    await applyAudioOutput();
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

  // Registratie direct verversen na een backend-mutatie die de 300 ms-poll
  // niet mag afwachten (crescendo-trap/aan-uit vanuit de Console). De backend
  // past de trede zelf additief toe (apply_crescendo_stage); hier alleen lezen.
  async function refreshOrganInfoNow() {
    try {
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
    // Ook de weergave terugzetten via dezelfde helper als de rest (setView):
    // sloot je het orgel vanuit een instellingen-tab, dan bleef activeView op
    // die tab staan en opende het orgel dat je daarna koos meteen in de
    // instellingen in plaats van aan de klavieren. showOrganBrowser daarna nog
    // eens hard op true, want tijdens een laadactie neemt setView de andere tak.
    setView('orgel');
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
  // Kruisje op een extra scherm: alleen dat scherm sluiten (standaard, de
  // gangbare vensterconventie) of de hele software (orgelconsole-gebruik,
  // 0.7.21). PanelApp leest de vlag zelf uit localStorage bij het sluiten.
  let panelCloseQuits = localStorage.getItem('jm-orgue-panel-close-quits') === 'true';
  function setPanelCloseQuits(enabled) {
    panelCloseQuits = enabled;
    localStorage.setItem('jm-orgue-panel-close-quits', String(enabled));
  }

  async function setAutostart(enabled) {
    try {
      await invoke('set_autostart_enabled', { enabled });
      autostartEnabled = enabled;
    } catch (e) {
      error = tx('errors.autostart_failed').replace('{error}', String(e));
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
    // Automatisch MIDI-archief: lopende take nog wegschrijven vóór de computer uitgaat.
    try { await invoke('midi_archive_flush'); } catch (e) {}
    try { await invoke('shutdown_computer'); }
    catch (e) { error = tx('errors.shutdown_failed').replace('{error}', String(e)); }
  }

  // Laatste stand veilig wegschrijven vlak vóór het afsluiten: bij het kruisje
  // (onCloseRequested) én vóór het installeren van een update. Harde grens van
  // 2,5 s zodat een hangende write het afsluiten nooit blokkeert.
  async function bewaarVoorAfsluiten() {
    try {
      await Promise.race([
        (async () => {
          // Geometrie éérst (goedkoop en synchroon af te ronden) zodat de
          // vensterpositie ook bij een snelle close zeker bewaard is.
          if (mainGeomTimer) { clearTimeout(mainGeomTimer); mainGeomTimer = null; }
          await saveMainGeometry();
          const rec = await invoke('get_recording_status').catch(() => null);
          if (rec && rec.recording) { await invoke('stop_recording').catch(() => {}); }
          // Automatisch MIDI-archief: lopende take nu wegschrijven (synchroon, enkele ms).
          await invoke('midi_archive_flush').catch(() => {});
          await flushAutoSave();
          await persistOrganSettings();
          await closeExtraPanels();
        })(),
        new Promise((r) => setTimeout(r, 2500)),
      ]);
    } catch (e) { /* opslaan mag het afsluiten nooit tegenhouden */ }
  }

  // Bevestiging vragen, opslaan, computer netjes afsluiten.
  async function requestShutdown() {
    // await: window.confirm is in Tauri asynchroon (zie startUpdate) — zonder
    // await zette één klik op Afsluiten de computer uit zonder vraag.
    const ok = await window.confirm(tx('dialogs.shutdown_confirm'));
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
{#if booting}
  <!-- Opstart-splash: vult de eerste seconden zodat het startscherm compleet
       verschijnt (inclusief een eventuele update-balk) in plaats van stukje bij
       beetje. Staat bewust alléén hier — extra registerschermen (PanelApp) en
       het notatievenster krijgen geen splash. -->
  <div class="boot-splash" out:fade={{ duration: 400 }}>
    <div class="nameplate nameplate-lg">
      <div class="nameplate-inner">
        <span class="nameplate-title">{$t('app.title')}</span>
      </div>
    </div>
    <svg class="boot-splash-mark" width="72" height="72" viewBox="0 0 36 36" fill="none" aria-hidden="true">
      <rect x="4" y="6" width="5" height="26" rx="1.5" fill="currentColor" opacity="0.3"/>
      <rect x="11" y="10" width="5" height="22" rx="1.5" fill="currentColor" opacity="0.4"/>
      <rect x="18" y="4" width="5" height="28" rx="1.5" fill="currentColor" opacity="0.5"/>
      <rect x="25" y="8" width="5" height="24" rx="1.5" fill="currentColor" opacity="0.4"/>
    </svg>
    <div class="boot-splash-sub">{$t('app.subtitle')}</div>
    {#if appVersion}
      <div class="boot-splash-version">{$t('about.version').replace('{version}', appVersion)}</div>
    {/if}
    <div class="loading-progress boot-splash-bar">
      <div class="loading-progress-bar boot-splash-bar-run"></div>
    </div>
    <div class="loading-text">{audioStarting ? $t('splash.audio_starting') : $t('splash.starting')}</div>
  </div>
{/if}
<div id="app">
  <Header
    organName={organInfo?.name}
    organLoaded={!showOrganBrowser && organInfo}
    showTabs={!showOrganBrowser}
    {status}
    {activeView}
    {audioProfiles}
    {audioProfileSwitching}
    on:startAudio={startAudio}
    on:stopAudio={stopAudio}
    on:closeOrgan={closeOrgan}
    on:setView={(e) => setView(e.detail)}
    on:toggleAudioProfile={() => switchAudioProfile()}
  />

  {#if updateInfo}
    <!-- Update-melding: wegklikken onthoudt déze versie; een volgende release
         meldt zich gewoon weer (zie runUpdateCheck). Is de update automatisch
         te installeren, dan staat "Nu bijwerken" ernaast; tijdens het
         downloaden vervangt de voortgang de knoppen. -->
    <div class="update-banner">
      {#if updateBezig && updateVoortgang}
        <span>
          {updateVoortgang?.fase === 'install' ? $t('update.installing') : $t('update.downloading')}
        </span>
        <div class="update-progress" role="progressbar" aria-valuemin="0" aria-valuemax="100" aria-valuenow={updateVoortgang?.pct ?? 0}>
          <div class="update-progress-fill" class:onbekend={updateVoortgang?.pct == null} style="width: {updateVoortgang?.pct ?? 100}%"></div>
        </div>
        <span class="update-progress-tekst">
          {#if updateVoortgang?.pct != null}{updateVoortgang.pct}%{/if}
          {#if updateVoortgang?.totaal}
            ({updateMb(updateVoortgang.gedaan)} / {updateMb(updateVoortgang.totaal)} MB)
          {/if}
        </span>
      {:else}
        <span>{$t('update.new_version_prefix')} <b>{updateInfo.version}</b> {$t('update.new_version_suffix')}</span>
        {#if updateFout}
          <span class="update-fout">{$t('update.failed').replace('{error}', updateFout)}</span>
        {:else if !updateInfo.auto && updateOpMac}
          <span class="update-uitleg">{$t('update.macos_manual')}</span>
        {/if}
        {#if updateInfo.auto}
          <button class="btn btn-primary btn-sm" on:click={() => startUpdate()} title={$t('update.install_now_title')}>{$t('update.install_now')}</button>
          <button class="btn btn-ghost btn-sm" on:click={openUpdatePage}>{$t('update.download')}</button>
        {:else}
          <button class="btn btn-primary btn-sm" on:click={openUpdatePage}>{$t('update.download')}</button>
        {/if}
        <button class="btn btn-ghost btn-sm" on:click={dismissUpdate} title={$t('update.dismiss_title')} aria-label={$t('actions.close')}>✕</button>
      {/if}
    </div>
  {/if}

  {#if audioOverbelastAdvies}
    <!-- Niet-modaal: de organist kan gewoon doorspelen; één klik zet de buffer
         op 128 frames (4x meer tijd per callback dan 32). -->
    <div class="update-banner">
      <span>{$t('audio.overload_warn')
        .replace('{peak}', Math.round((status.renderPeak || 0) * 100))
        .replace('{frames}', status.renderFrames || status.bufferFrames || 0)}</span>
      <button class="btn btn-primary btn-sm" on:click={zetBufferOp128}>{$t('audio.overload_fix')}</button>
      <button class="btn btn-ghost btn-sm" on:click={() => { audioOverbelastAdvies = false; audioOverbelastGenegeerdTot = Date.now() + OVERBELAST_NEGEER_MS; }} title={$t('actions.close')} aria-label={$t('actions.close')}>✕</button>
    </div>
  {/if}

  {#if asioHerstartAdvies}
    <!-- ASIO-driver kan in deze sessie niet opnieuw starten (0.7.43): de app
         herstart niet meer uit zichzelf — dat oogde als "valt steeds terug naar
         de bibliotheek" — maar laat de organist kiezen. Het orgel speelt
         intussen door op de teruggevallen uitgang. -->
    <div class="update-banner">
      <span>{$t('audio.asio_restart_advice')
        .replace('{device}', asioHerstartAdvies)
        .replace('{host}', status.audioHost || 'WASAPI')}</span>
      <button class="btn btn-primary btn-sm" on:click={herstartVoorAsio} disabled={asioHerstartBezig}>{$t('audio.asio_restart_now')}</button>
      <button class="btn btn-ghost btn-sm" on:click={() => { asioHerstartAdvies = null; asioHerstartGenegeerd = true; }} title={$t('actions.close')} aria-label={$t('actions.close')}>✕</button>
    </div>
  {/if}

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
      audioChannels={status.channels || 0}
      audioHostActual={status.audioHost || ''}
      audioDeviceActual={status.audioDevice || ''}
      audioBufferFrames={status.bufferFrames || 0}
      {divisionChannelsVersion}
      {autostartEnabled}
      {restoreRegistration}
      {autoLoadLastOrgan}
      {panelCloseQuits}
      on:toggleStop={(e) => toggleStop(e.detail)}
      on:toggleCoupler={(e) => toggleCoupler(e.detail)}
      on:divisionChannelsChanged={syncActiveProfileChannels}
      on:onlineSetsSettled={() => meldOnlineSetsKlaar()}
      on:startUpdate={(e) => startUpdate(e.detail)}
      on:refreshOrgan={refreshOrganInfoNow}
      on:refreshDevices={refreshDevices}
      on:refresh={refreshDevices}
      on:setView={(e) => setView(e.detail)}
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
      on:selectSampleRate={(e) => (selectedSampleRate = e.detail)}
      {selectedSampleRate}
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
      on:setPanelCloseQuits={(e) => setPanelCloseQuits(e.detail)}
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
