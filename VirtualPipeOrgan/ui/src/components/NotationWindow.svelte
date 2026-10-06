<script>
  // Notatievenster: bladmuziek uit een MIDI-opname (bestand) of live opnemen
  // met directe weergave. Twee modes via de URL-hash:
  //   #notation&file=<pad>   → file-modus: bestaand .mid-bestand converteren
  //   #notation&live=1       → live-modus: opnemen in een verse Score
  //
  // Live-modus flow: notation_new_score → toon LayerBar + toolbar. Rode
  // opname-knop start/stopt de armed take; capture_notation_event (backend)
  // schrijft events en emit `note-added` per afgeronde noot, waarna dit
  // venster de MusicXML regenereert (debounced) en OSMD re-rendert.
  //
  // De omzetting MIDI → MusicXML zit in de Rust-backend; renderen doet
  // OpenSheetMusicDisplay (BSD-3, gebundeld — geen externe software).
  import { onMount, onDestroy, tick } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { OpenSheetMusicDisplay } from 'opensheetmusicdisplay';
  import KlavarSheet from './KlavarSheet.svelte';
  import NewScoreWizard from './NewScoreWizard.svelte';
  import NotationHeader from './notation/NotationHeader.svelte';
  import NotationTabs from './notation/NotationTabs.svelte';
  import InputTab from './notation/InputTab.svelte';
  import TextsMarksTab from './notation/TextsMarksTab.svelte';
  import StavesVoicesTab from './notation/StavesVoicesTab.svelte';
  import PlaybackTab from './notation/PlaybackTab.svelte';
  import LayerBar from './notation/LayerBar.svelte';
  import StatusLine from './notation/StatusLine.svelte';
  import HelpDialog from './notation/HelpDialog.svelte';
  import ConfirmDialog from './notation/ConfirmDialog.svelte';
  import ContextMenu from './notation/ContextMenu.svelte';
  import { xToGridTime, gridTimeToX, celBreedte, snapGridTime } from '../lib/sheetGeometry.js';
  import { MAATSOORTEN, splitsMaatsoort, maatDuurUs, notemapSleutel, rasterPositie } from '../lib/notatieKeuzes.js';
  import { t, tx } from '../lib/i18n.js';
  import { pasSfeerToeAlsGewijzigd } from '../lib/sfeer.js';

  // URL-hash parsen: file-pad (file-modus) of live-vlag (live-modus).
  const isLive = /[&?]live=1/.test(window.location.hash);
  let filePath = (() => {
    const m = window.location.hash.match(/[&?]file=([^&]+)/);
    return m ? decodeURIComponent(m[1]) : null;
  })();

  // OSMD + weergave-state
  let container;
  let osmd = null;
  let converting = false;
  let error = null;
  let xml = null;
  // Rendertijd van OSMD (0.7.83): in de console, en in de statusregel met ?debug.
  let renderMs = 0, renderMaten = 0;
  const debugVlag = typeof location !== 'undefined' && /[?&]debug/.test(location.search);
  let lastGeneration = 0; // laatste gerenderde generation (dedup)

  // Weergave (0.7.71): notenschrift (OSMD) of klavar (eigen SVG, KlavarSheet).
  // Globaal onthouden; localStorage is per origin gedeeld, dus ook een nieuw
  // notatievenster ziet de keuze. Beide weergaven lezen dezelfde Score en
  // dezelfde kwantisatie (notation.rs::quantize_score).
  let viewMode = 'staff';
  try { if (localStorage.getItem('jm-orgue-notation-view') === 'klavar') viewMode = 'klavar'; } catch (e) {}
  let klavarModel = null;
  let lastKlavarGeneration = 0;
  // Vast bereik (0.7.73): 'auto' = rond de gespeelde noten (altijd c'–b'),
  // 'klavier' = het hele klavier van het geladen orgel (uit de registers),
  // zodat alle pagina's even breed zijn. Globaal onthouden, net als de weergave.
  let klavarBereik = 'auto';
  try { if (localStorage.getItem('jm-orgue-klavar-bereik') === 'klavier') klavarBereik = 'klavier'; } catch (e) {}
  let klavierBereik = null;   // { manual: [lo, hi], pedal: [lo, hi] } uit get_organ_info
  $: bereikVoorSheet = klavarBereik === 'klavier' ? (klavierBereik || { manual: [36, 96], pedal: [36, 67] }) : null;
  function setKlavarBereik(v) {
    klavarBereik = v === 'klavier' ? 'klavier' : 'auto';
    try { localStorage.setItem('jm-orgue-klavar-bereik', klavarBereik); } catch (e) {}
  }
  // Klavieromvang per balk uit de registers: de laagste eerste en hoogste
  // laatste toets van alle registers van de manualen c.q. het pedaal.
  function klavierBereikUit(info) {
    const divs = info?.divisions || [];
    const ped = divs.filter(d => isPedalName(d.name || '') || isPedalName(d.display_name || ''));
    const man = divs.filter(d => !ped.includes(d));
    // Per divisie: laagste eerste toets + de MODALE registerbreedte. De
    // laatste toets van een register is eerste + aantal pijpen − 1, en een
    // uitgebouwde unit-rang (73 pijpen op 61 toetsen, voor de octaafkoppels)
    // zou het klavier anders een octaaf oprekken.
    const bereik = (ds, lo0, hi0) => {
      let lo = 127, hi = 0;
      for (const d of ds) {
        const tel = new Map(); let dlo = 127;
        for (const s of d.stops || []) {
          const a = Number(s.first_midi_note), b = Number(s.last_midi_note);
          if (!(a >= 0 && b > a)) continue;
          dlo = Math.min(dlo, a);
          const w = b - a + 1; tel.set(w, (tel.get(w) || 0) + 1);
        }
        if (!tel.size) continue;
        const w = [...tel.entries()].sort((x, y) => y[1] - x[1] || y[0] - x[0])[0][0];
        lo = Math.min(lo, dlo); hi = Math.max(hi, dlo + w - 1);
      }
      return lo <= hi ? [Math.max(lo0, lo), Math.min(hi0, hi)] : null;
    };
    return { manual: bereik(man, 21, 108), pedal: bereik(ped, 24, 72) };
  }
  let osmdHost;           // inner div waar OSMD in tekent (blijft gemount, ook in klavar)
  function setViewMode(mode) {
    if (mode === viewMode) return;
    viewMode = mode;
    try { localStorage.setItem('jm-orgue-notation-view', mode); } catch (e) {}
    noteBoxes = [];       // geen oude kaders van de selectie-overlay
    lastGeneration = 0;
    lastKlavarGeneration = 0;
    doRender();
  }
  // ---- Meelopen tijdens het inspelen (0.7.72) ----
  // De backend zet de opnametijd op de eerste toets en meldt elke toets met
  // zijn tijdstip (note-open bij indrukken, note-added bij loslaten). Daaruit
  // loopt hier een klok mee: in klavar tekent die een nu-lijn en de nog
  // ingedrukte toetsen, en schuift het blad door zodra de lijn onderin komt.
  // In notenschrift springt het blad na elke render naar de laatst gespeelde
  // noot (OSMD kent geen tussenstand).
  let nowGrid = null;        // opnametijd in rastereenheden (breuk), null = geen klok
  let openNotes = [];        // [{ layerId, midi, channel, start }] ingedrukte toetsen
  let recOriginPerf = null;  // performance.now() van opnametijd 0 (in wandklok ÷ snelheid)
  let volgTimer = null;
  let volgBron = null;       // 'opname' | 'afspelen' (0.7.73: ook de afspeelpositie volgt)
  // Afspeelsnelheid van de MIDI-speler (schuif in het MIDI-paneel, blijft
  // staan bij het afspelen van de partituur): geschat uit twee polls, zodat de
  // nu-lijn tussen de polls niet op 1× loopt en elke poll verspringt.
  let volgSnelheid = 1;
  let volgVorige = null;     // { ms, perf } van de vorige poll
  let volgPauze = false;     // speler gepauzeerd: lijn blijft staan
  function rasterPerSeconde() {
    const bpm = Number(klavarModel?.bpm) || Number(score?.bpm) || 90;
    const q = Number(klavarModel?.q) || Number(score?.quantize) || 4;
    return (bpm / 60) * q;
  }
  function syncOpnameKlok(us) {
    if (!Number.isFinite(us)) return;
    recOriginPerf = performance.now() - (us / 1000) / volgSnelheid;
  }
  function zonderToets(lijst, p) {
    return lijst.filter(o => !(o.layerId === p.layer && o.midi === p.midi && o.channel === p.channel));
  }
  function onNoteOpen(p) {
    // Tijdens afspelen (opnameknop staat dan uit; dit is de achtervang) wint
    // de afspeelklok: anders trekken poll en toets de lijn heen en weer.
    const afspelen = volgBron === 'afspelen' && playingScore;
    if (!afspelen) syncOpnameKlok(p.at_us);
    openNotes = [...zonderToets(openNotes, p), { layerId: p.layer, midi: p.midi, channel: p.channel, start: Math.round((p.at_us / 1e6) * rasterPerSeconde()) }];
    if (!afspelen) startVolgen('opname');
  }
  function onNoteAdded(p) {
    if (!(volgBron === 'afspelen' && playingScore)) syncOpnameKlok(p.end_us);
    openNotes = zonderToets(openNotes, p);
  }
  function startVolgen(bron) {
    volgBron = bron;
    if (volgTimer) return;
    volgTimer = setInterval(volgTick, 50);
  }
  function stopVolgen() {
    if (volgTimer) { clearInterval(volgTimer); volgTimer = null; }
    volgBron = null;
    nowGrid = null;
    openNotes = [];
    recOriginPerf = null;
    volgSnelheid = 1;
    volgVorige = null;
    volgPauze = false;
  }
  async function volgTick() {
    const bronWeg = !volgBron || (volgBron === 'opname' && !recording) || (volgBron === 'afspelen' && !playingScore);
    if (bronWeg || recOriginPerf == null) { stopVolgen(); return; }
    if (viewMode !== 'klavar' || volgPauze) return;
    nowGrid = ((performance.now() - recOriginPerf) / 1000) * volgSnelheid * rasterPerSeconde();
    await tick();
    volgNuLijn();
  }
  // Houd de nu-lijn op zestig procent van het zichtbare blad: alleen omlaag,
  // behalve als de lijn boven het beeld staat (een tweede take begint weer in
  // maat 1 terwijl het blad nog onderaan staat).
  function volgNuLijn() {
    if (!container) return;
    const el = container.querySelector('.k-nu');
    if (!el) return;
    const r = el.getBoundingClientRect(), c = container.getBoundingClientRect();
    const y = r.top - c.top + container.scrollTop;
    const gewenst = y - 0.6 * container.clientHeight;
    if (gewenst > container.scrollTop || y < container.scrollTop) container.scrollTop = Math.max(0, gewenst);
  }
  // Na een render tijdens de opname: het blad naar de laatst gespeelde noot.
  function scrollNaarNieuwsteNoot() {
    if (!recording || !container) return;
    let nieuwste = null;
    for (const l of score?.layers ?? []) for (const t of l.takes) for (const e of t.events) if (nieuwste == null || e.id > nieuwste) nieuwste = e.id;
    if (nieuwste == null) return;
    let y, h;
    if (viewMode === 'klavar') {
      if (nowGrid != null) return;   // de nu-lijn bepaalt het scrollen
      const el = container.querySelector(`[data-id="${nieuwste}"]`);
      if (!el) return;
      const r = el.getBoundingClientRect(), c = container.getBoundingClientRect();
      y = r.top - c.top + container.scrollTop; h = r.height;
    } else {
      const box = noteBoxes.find(b => b.eventId === nieuwste);
      if (!box) return;
      y = box.y; h = box.h;
    }
    scrollNaarRect(y, h);
  }
  function scrollNaarRect(y, h) {
    const onder = y + h, zicht = container.clientHeight;
    if (onder > container.scrollTop + 0.85 * zicht || y < container.scrollTop) {
      container.scrollTop = Math.max(0, onder - 0.6 * zicht);
    }
  }
  // Afspelen in notenschrift (0.7.73): het blad volgt de noot die klinkt
  // (de laatste inzet op of vóór de afspeelpositie, via de OSMD-kaders).
  function scrollNaarNootBijTijd(us) {
    if (!container || !noteBoxes.length) return;
    let beste = null;
    for (const l of score?.layers ?? []) for (const t of l.takes) {
      if (!t.visible) continue;
      for (const e of t.events) if (e.start_us <= us && (beste == null || e.start_us > beste.start_us)) beste = e;
    }
    if (!beste) return;
    const box = noteBoxes.find(b => b.eventId === beste.id);
    if (box) scrollNaarRect(box.y, box.h);
  }

  // File-modus opties (blijven werken zoals in 0.7.0)
  let bpm = 90;
  let beatsPerBar = 4;
  let beatUnit = 4;      // noemer van de maatsoort (0.7.83)
  let quantize = 4;
  let keyFifths = 0;
  let minor = false;   // toongeslacht (0.7.72): ruit i.p.v. cirkel in klavar, <mode> in MusicXML
  let title = '';
  let tolerancePct = 80; // "los ↔ strak"

  // File-modus: vrij instelbare balk-indeling (divisie-vinkjes per balk)
  let divisions = [];
  let staffConfig = [];

  // Live-modus: score-state
  let scoreId = null;
  let score = null; // laatste Score-snapshot uit de backend
  let recording = false;
  let selectionIds = new Set(); // Set<eventId>

  // Klik-op-noot (0.7.4): na elke render correleren we elke gerenderde OSMD-noot
  // met een LayerEv.id, zodat een klik in de bladmuziek een noot kan selecteren
  // en de selectie als absolute-position overlay wordt gemarkeerd. De correlatie
  // is volledig frontend-side en backend-vrij: OSMD levert per noot een
  // frequentie (→ offset-vrije MIDI) en een absolute tijdstempel; de balk-index
  // wijst naar de n-de niet-lege laag (zoals build_musicxml de parts nummert);
  // matchen gebeurt op (laag, midi, tijd-in-interval) — dat vangt ook overgebonden
  // noten (ties) die als meerdere noteheads renderen. Coördinaten zijn relatief
  // aan de scroll-content van .notation-sheet.
  let noteBoxes = [];          // [{ eventId, midi, x, y, w, h, cx, cy }]
  let notemap = [];            // NoteRef[] van de laatste schermrender (0.7.86): exacte klik-correlatie
  let notatieQ = 4;            // effectief raster van die render (bij /8 minstens 2)
  // Muisinvoer (0.7.87)
  let gumActief = false;       // kleverige gum: klik op een kop verwijdert die noot
  let contextMenu = null;      // { x, y, items } van het rechtsklikmenu
  let chordTekst = '';         // statusregel na dubbelklik op een akkoord
  let geluidBijInvoer = true;  // geplaatste noot kort laten klinken
  try { if (localStorage.getItem('jm-orgue-notation-preview') === '0') geluidBijInvoer = false; } catch (e) {}
  function toggleGeluidBijInvoer() {
    geluidBijInvoer = !geluidBijInvoer;
    try { localStorage.setItem('jm-orgue-notation-preview', geluidBijInvoer ? '1' : '0'); } catch (e) {}
  }
  function toggleGum() { gumActief = !gumActief; if (gumActief) tekenSticky = null; }
  // Het menu sluit op mousedown; de click die erop volgt mag niets doen.
  function sluitContextMenu() {
    contextMenu = null;
    dragConsumedClick = true;
    setTimeout(() => { dragConsumedClick = false; }, 0);
  }

  // ---- Teksten (0.7.88): liedtekst typen en aanwijzingen ----
  let lyricMode = false;       // klik een noot → typen onder die noot
  let lyricStrofe = 1;
  let lyricEdit = null;        // { eventId, text, orig: { text, syllabic, extend } | null }
  let lyricVorigeKoppel = false; // de lettergreep ervóór eindigt met "-"
  let lyricInput = null;
  function toggleLyricMode() { lyricMode = !lyricMode; if (!lyricMode) sluitLyricEdit(false); if (lyricMode && stepMode) setStepMode(false); }
  function setStrofe(n) { lyricStrofe = Math.max(1, Math.min(3, Number(n) || 1)); if (lyricEdit) startLyricEdit(lyricEdit.eventId); }
  function lyricVan(ev) { return (ev?.lyrics || []).find(l => Number(l.number) === lyricStrofe) || null; }
  function huidigeLyric(eventId) { return lyricVan(flatEvents.find(x => x.id === eventId)); }
  // De drager van de liedtekst is de hoogste noot van het akkoord (zelfde
  // balk, stem en inzet), zoals buurNoot hem ook kiest.
  function akkoordDrager(eventId) {
    const ev = flatEvents.find(x => x.id === eventId);
    if (!ev) return eventId;
    const zelfde = flatEvents.filter(x => x._layerId === ev._layerId && (Number(x.voice) || 1) === (Number(ev.voice) || 1) && Math.abs(x.start_us - ev.start_us) < 2);
    return zelfde.sort((a, b) => b.midi - a.midi)[0]?.id ?? eventId;
  }
  async function lyricFocus() { await tick(); lyricInput?.focus(); lyricInput?.select(); }
  function openLyricOp(eventId) {
    const l = huidigeLyric(eventId);
    lyricEdit = { eventId, text: l ? l.text : '', orig: l ? { text: l.text, syllabic: l.syllabic || 'single', extend: !!l.extend } : null };
    const vl = lyricVan(buurNoot(eventId, -1));
    lyricVorigeKoppel = !!vl && (vl.syllabic === 'begin' || vl.syllabic === 'middle');
    selectionIds = new Set([eventId]);
    const idx = flatEvents.findIndex(ev => ev.id === eventId);
    if (idx >= 0) cursorIndex = idx;
  }
  function startLyricEdit(eventId) {
    openLyricOp(akkoordDrager(eventId));
    lyricFocus();
  }
  // Volgende/vorige noot van dezelfde balk en stem op een andere inzet.
  function buurNoot(eventId, richting) {
    const ev = flatEvents.find(x => x.id === eventId);
    if (!ev) return null;
    const zelfde = flatEvents.filter(x => x._layerId === ev._layerId && (Number(x.voice) || 1) === (Number(ev.voice) || 1));
    const kandidaten = richting > 0 ? zelfde.filter(x => x.start_us > ev.start_us + 1) : zelfde.filter(x => x.start_us < ev.start_us - 1);
    if (!kandidaten.length) return null;
    const doelStart = richting > 0 ? Math.min(...kandidaten.map(x => x.start_us)) : Math.max(...kandidaten.map(x => x.start_us));
    const opInzet = kandidaten.filter(x => Math.abs(x.start_us - doelStart) < 2);
    return opInzet.sort((a, b) => b.midi - a.midi)[0]; // de hoogste noot van het akkoord
  }
  // Slaat de lettergreep van de open editor op. `koppel` = er volgt een
  // koppelteken. Ongewijzigde tekst wordt niet opnieuw opgeslagen (anders
  // verliest "Hal-" zijn syllabic). Daarna de score vers ophalen: buurNoot en
  // huidigeLyric lezen flatEvents, en de gewone render komt pas na 300 ms.
  async function commitLyric(koppel) {
    if (!lyricEdit) return;
    const { eventId, text, orig } = lyricEdit;
    const leeg = !text.trim();
    if (!koppel && orig && text.trim() === orig.text) {
      lyricVorigeKoppel = orig.syllabic === 'begin' || orig.syllabic === 'middle';
      return;
    }
    let syllabic = 'single';
    if (koppel) syllabic = lyricVorigeKoppel ? 'middle' : 'begin';
    else syllabic = lyricVorigeKoppel ? 'end' : 'single';
    try {
      if (leeg) {
        // Lege lettergreep: melisma — de vorige noot loopt door.
        const vorige = buurNoot(eventId, -1);
        const vl = lyricVan(vorige);
        if (vorige && vl && !koppel && !vl.extend) await invoke('notation_set_lyric', { scoreId, eventId: vorige.id, number: lyricStrofe, text: vl.text, syllabic: vl.syllabic || 'single', extend: true });
        if (orig) await invoke('notation_set_lyric', { scoreId, eventId, number: lyricStrofe, text: '', syllabic: 'single', extend: false });
      } else {
        await invoke('notation_set_lyric', { scoreId, eventId, number: lyricStrofe, text: text.trim(), syllabic, extend: false });
      }
      score = await invoke('notation_get_score', { scoreId });
      await tick();
    } catch (e) { alert(String(e)); }
    lyricVorigeKoppel = koppel && !leeg;
  }
  async function lyricNaar(richting, koppel) {
    if (!lyricEdit) return;
    const mijn = lyricEdit;
    const huidig = lyricEdit.eventId;
    // Terug op een leeg veld = alleen navigeren (geen melisma op de noot ervoor).
    if (!(richting < 0 && !lyricEdit.text.trim())) await commitLyric(koppel);
    if (lyricEdit !== mijn) return; // ondertussen weggeklikt of een andere noot gekozen
    const buur = buurNoot(huidig, richting);
    if (buur) { openLyricOp(buur.id); await lyricFocus(); }
    else sluitLyricEdit(false);
  }
  async function sluitLyricEdit(bewaar) {
    if (!lyricEdit) return;
    const mijn = lyricEdit; // een ondertussen gestarte editor niet wegvagen
    if (bewaar) await commitLyric(false);
    if (lyricEdit !== mijn) return;
    lyricEdit = null;
    lyricVorigeKoppel = false;
  }
  function onLyricKey(e) {
    e.stopPropagation();
    if (e.key === ' ' || e.key === 'Enter') { e.preventDefault(); lyricNaar(+1, false); }
    else if (e.key === '-' && !e.ctrlKey) { e.preventDefault(); lyricNaar(+1, true); }
    else if (e.key === 'Backspace' && !lyricEdit.text) { e.preventDefault(); lyricNaar(-1, false); }
    else if (e.key === 'Escape') { e.preventDefault(); sluitLyricEdit(true); }
    else if (e.key === 'Tab') { e.preventDefault(); lyricNaar(e.shiftKey ? -1 : +1, false); }
  }
  // ---- Tekens (0.7.89): articulaties, spelling, bogen, maatstrepen ----
  let tekenSticky = null;          // teken op de muis: elke klik op een noot zet/haalt het
  let stepTekens = new Set();      // kleverige articulaties voor elke geplaatste noot
  const TEKEN_NAMEN = { staccato: 'Staccato', tenuto: 'Tenuto', accent: 'Accent', fermata: 'Fermate', breath: 'Ademteken', spelling: '♯↔♭' };
  // Geeft true als de backend een stap heeft vastgelegd (generatie gewijzigd).
  async function pasTekenToe(teken, ids) {
    if (!ids.length) return false;
    try {
      const voor = score?.generation ?? 0;
      const gen = teken === 'spelling'
        ? await invoke('notation_toggle_spelling', { scoreId, eventIds: ids })
        : await invoke('notation_toggle_articulation', { scoreId, eventIds: ids, articulation: teken });
      return Number(gen) !== Number(voor);
    } catch (e) { alert(String(e)); return false; }
  }
  // Knop of sneltoets: met een selectie meteen toepassen; in stapinvoer
  // kleverig voor elke volgende noot; anders kleverig op de muis (Esc stopt).
  function tekenKnop(teken) {
    if (selectionIds.size > 0) { pasTekenToe(teken, Array.from(selectionIds)); return; }
    if (stepMode) {
      if (teken === 'spelling') { if (stepLastIds.length) pasTekenToe(teken, stepLastIds); return; }
      const s = new Set(stepTekens);
      s.has(teken) ? s.delete(teken) : s.add(teken);
      stepTekens = s;
      return;
    }
    tekenSticky = tekenSticky === teken ? null : teken;
    if (tekenSticky) gumActief = false;
  }
  $: tekenTekst = tekenSticky ? tx('notation.sign_sticky').replace('{sign}', tx('notation.sign_' + tekenSticky)) : '';
  async function spanKnop(kind) {
    const ids = Array.from(selectionIds);
    if (ids.length < 2) { alert(tx('notation.span_needs_two')); return; }
    try { await invoke('notation_toggle_span', { scoreId, eventIds: ids, kind }); } catch (e) { alert(String(e)); }
  }
  async function spansWeg() {
    const ids = selectedIdsOrCursor();
    if (!ids.length) return;
    try { await invoke('notation_remove_spans', { scoreId, eventIds: ids }); } catch (e) { alert(String(e)); }
  }
  // Maat van de cursor: in stapinvoer de invoercursor, anders de cursornoot.
  function cursorMaat() {
    const maatUs = maatDuurUs(bpm, beatsPerBar, beatUnit);
    const us = stepMode ? stepPosUs : (cursorEvent ? cursorEvent.start_us : 0);
    return Math.max(0, Math.floor(us / maatUs + 1e-6));
  }
  async function maatteken(wat) {
    try { await invoke('notation_toggle_bar', { scoreId, measure: cursorMaat(), wat }); } catch (e) { alert(String(e)); }
  }
  // Aanwijzingen op de cursor (stapinvoer: de invoercursor; anders de cursornoot).
  function tekstDoel() {
    const layerId = (stepMode ? stepTargetLayerId() : (cursorEvent?._layerId ?? score?.armed_layer ?? score?.layers?.[0]?.id)) ?? null;
    const startUs = stepMode ? Math.round(stepPosUs) : (cursorEvent ? cursorEvent.start_us : 0);
    return { layerId, startUs };
  }
  async function addTextAtCursor(kind, text, placement) {
    const { layerId, startUs } = tekstDoel();
    if (layerId == null) return;
    try { await invoke('notation_add_text', { scoreId, layerId, startUs, kind, text, placement }); } catch (e) { alert(String(e)); }
  }
  async function editTextById(id, text) {
    try { await invoke('notation_edit_text', { scoreId, id, text }); } catch (e) { alert(String(e)); }
  }
  async function removeTextById(id) {
    try { await invoke('notation_remove_text', { scoreId, id }); } catch (e) { alert(String(e)); }
  }
  async function moveTextById(id, tellen) {
    const m = (score?.texts || []).find(x => x.id === id);
    if (!m) return;
    const telUs = (60e6 / (Number(bpm) || 90)) * (4 / (Number(beatUnit) || 4));
    const startUs = Math.max(0, Math.round(m.start_us + tellen * telUs));
    try { await invoke('notation_edit_text', { scoreId, id, startUs }); } catch (e) { alert(String(e)); }
  }
  async function vulRegistratie() {
    try {
      const lijst = await invoke('get_drawn_stop_names');
      if (!Array.isArray(lijst) || !lijst.length) return tx('notation.no_stops_drawn');
      return lijst.map(([div, stops]) => `${div}: ${stops.join(', ')}`).join(' · ');
    } catch (e) { return ''; }
  }
  function focusSelect(node) { node.focus(); node.select(); }
  function laagNaam(layerId) { return score?.layers?.find(l => l.id === layerId)?.name ?? ''; }
  function tijdTekst(startUs) {
    const maatUs = maatDuurUs(bpm, beatsPerBar, beatUnit);
    const m = Math.floor(startUs / maatUs + 1e-6);
    const telUs = maatUs / (Number(beatsPerBar) || 4);
    const b = Math.floor((startUs - m * maatUs) / telUs + 1e-6);
    return tx('notation.measure_beat').replace('{m}', m + 1).replace('{b}', b + 1);
  }
  async function setHeaderLive(titel, componist, ondertitel) {
    if (!isLive || scoreId == null) return;
    try {
      await invoke('notation_set_header', { scoreId, title: titel, composer: componist, subtitle: ondertitel });
      score = await invoke('notation_get_score', { scoreId });
      title = score.title || '';
    } catch (e) { alert(String(e)); }
  }
  let alleenActieveStem = false; // andere stemmen lichtgrijs (Shift+Alt+S)
  $: selectedBoxes = noteBoxes.filter(b => selectionIds.has(b.eventId));
  // Pedaalnoten hebben geen hand (0.7.73): L/R staan uit zolang alleen
  // pedaalnoten zijn gekozen; de backend negeert de hand daar toch.
  $: pedaalIds = new Set((klavarModel?.pedal?.notes ?? []).map(n => n.id).filter(id => id != null));
  $: selectieAlleenPedaal = selectionIds.size > 0 && [...selectionIds].every(id => pedaalIds.has(id));

  // Dezelfde regel als notation.rs::is_pedal_name (0.7.70): "ped" aan een
  // woordbegin, met diakrieten gestript ("Pédale", "Pedał", "PED").
  function isPedalName(n) {
    const plat = (n || '').toLowerCase().normalize('NFD').replace(/[\u0300-\u036f]/g, '').replace(/ł/g, 'l');
    return plat.split(/[^\p{L}\p{N}]+/u).some(w => w.startsWith('ped'));
  }

  // ---- Metronoom + count-in (0.7.5) ----
  // Aparte WebAudio-klik in het notatievenster: géén orgelpijp en volledig buiten
  // de sample-graph van de backend, zodat de tik nooit in een opname of galm
  // terechtkomt. Downbeat = hogere/hardere tik. Config (aan/uit + count-in) staat
  // in het Score-model en wordt via notation_set_metronome gepersisteerd.
  let metroOn = false;
  let countInBeats = 0;         // 0/1/2/4 tellen voortellen vóór opname
  let metroAudioCtx = null;
  let metroTimer = null;        // lookahead-scheduler
  let metroNextTime = 0;        // AudioContext-tijd van de volgende tik
  let metroBeat = 0;            // 0 = downbeat van de maat
  let countInRemaining = 0;     // tellen die nog voor-tikken (>0 = aftellen)
  let armedWaiting = false;     // true tijdens count-in: "wacht op tel 1"
  const METRO_LOOKAHEAD = 0.12; // s vooruit plannen

  function ensureMetroCtx() {
    if (!metroAudioCtx) {
      const AC = window.AudioContext || window.webkitAudioContext;
      metroAudioCtx = new AC();
    }
    if (metroAudioCtx.state === 'suspended') metroAudioCtx.resume();
    return metroAudioCtx;
  }
  function scheduleClick(time, downbeat) {
    const ctx = metroAudioCtx;
    const osc = ctx.createOscillator();
    const gain = ctx.createGain();
    osc.type = 'square';
    osc.frequency.value = downbeat ? 1600 : 1000;
    const peak = downbeat ? 0.5 : 0.28;
    gain.gain.setValueAtTime(0.0001, time);
    gain.gain.exponentialRampToValueAtTime(peak, time + 0.001);
    gain.gain.exponentialRampToValueAtTime(0.0001, time + 0.05);
    osc.connect(gain).connect(ctx.destination);
    osc.start(time);
    osc.stop(time + 0.06);
  }
  function metroSchedulerTick() {
    if (!metroAudioCtx) return;
    const ctx = metroAudioCtx;
    // Tik per tel van de maatsoort (0.7.83): bij /8 een achtste, bij /2 een halve.
    const beatDur = (60 / (Number(bpm) || 90)) * (4 / (Number(score?.beat_unit) || Number(beatUnit) || 4));
    const beatsPer = Number(score?.beats_per_bar) || Number(beatsPerBar) || 4;
    while (metroNextTime < ctx.currentTime + METRO_LOOKAHEAD) {
      const downbeat = (metroBeat % beatsPer) === 0;
      scheduleClick(metroNextTime, downbeat);
      metroNextTime += beatDur;
      metroBeat++;
      if (countInRemaining > 0) {
        countInRemaining--;
        if (countInRemaining === 0 && armedWaiting) {
          // Count-in klaar → nu pas echt opnemen. De backend nult op de eerste
          // gespeelde noot, dus dit hoeft niet exact op de tik te vallen.
          armedWaiting = false;
          startBackendRecording();
        }
      }
    }
  }
  function startMetronome(withCountIn) {
    ensureMetroCtx();
    metroBeat = 0;
    metroNextTime = metroAudioCtx.currentTime + 0.12;
    countInRemaining = withCountIn ? (Number(countInBeats) || 0) : 0;
    armedWaiting = countInRemaining > 0;
    if (metroTimer) clearInterval(metroTimer);
    metroTimer = setInterval(metroSchedulerTick, 25);
    metroSchedulerTick();
  }
  function stopMetronome() {
    if (metroTimer) { clearInterval(metroTimer); metroTimer = null; }
    countInRemaining = 0;
    armedWaiting = false;
  }
  // BPM-voorbeeld: laat één maat tikken zonder op te nemen, zodat de speler het
  // tempo hoort. Loopt niet door en raakt de opname niet.
  function previewTempo(bpmKeuze = null, tellenKeuze = null, noemerKeuze = null) {
    if (recording || armedWaiting) return;
    ensureMetroCtx();
    const ctx = metroAudioCtx;
    const noemer = Number(noemerKeuze) || Number(score?.beat_unit) || Number(beatUnit) || 4;
    const beatDur = (60 / (Number(bpmKeuze ?? bpm) || 90)) * (4 / noemer);
    const beatsPer = Number(tellenKeuze) || Number(score?.beats_per_bar) || Number(beatsPerBar) || 4;
    let t = ctx.currentTime + 0.08;
    for (let i = 0; i < beatsPer; i++) {
      scheduleClick(t, i === 0);
      t += beatDur;
    }
  }
  async function setMetronomeCfg(on, countIn) {
    metroOn = on;
    countInBeats = countIn;
    if (!isLive || scoreId == null) return;
    try { await invoke('notation_set_metronome', { scoreId, clickOn: on, countInBeats: countIn }); } catch (e) {}
  }
  function syncMetronomeFromScore() {
    if (score?.metronome) {
      metroOn = !!score.metronome.click_on;
      countInBeats = Number(score.metronome.count_in_beats) || 0;
    }
  }

  // Toonsoort-labels per taal (nl/de 'A / fis', en 'A / F#m', fr 'La / fa♯ m').
  const KEY_FIFTHS = [
    [-7, 'key_flat_7'], [-6, 'key_flat_6'], [-5, 'key_flat_5'], [-4, 'key_flat_4'],
    [-3, 'key_flat_3'], [-2, 'key_flat_2'], [-1, 'key_flat_1'], [0, 'key_c'],
    [1, 'key_sharp_1'], [2, 'key_sharp_2'], [3, 'key_sharp_3'], [4, 'key_sharp_4'],
    [5, 'key_sharp_5'], [6, 'key_sharp_6'], [7, 'key_sharp_7'],
  ];
  $: keyChoices = KEY_FIFTHS.map(([v, k]) => ({ v, label: $t('notation.' + k) }));
  $: gridChoices = [
    { v: 1, label: $t('notation.grid_quarter') }, { v: 2, label: $t('notation.grid_eighth') },
    { v: 4, label: $t('notation.grid_sixteenth') }, { v: 8, label: $t('notation.grid_32nd') },
  ];

  function titleFromPath(p) { return p ? p.split(/[\\/]/).pop().replace(/\.(mid|midi)$/i, '') : tx('notation.default_title'); }
  if (!isLive) title = titleFromPath(filePath);

  // ---- Rendering (debounced) ----
  // Live-events kunnen in bursts binnenkomen tijdens spelen; direct re-renderen
  // per note-added zou OSMD dwingen tientallen keren per seconde te herbouwen.
  // Debounce 300 ms met een harde ceiling van 700 ms zodat er altijd binnen
  // 0,7 s een re-render is; edits pakken dezelfde weg.
  let renderDebounceTimer = null;
  let renderCeilingTimer = null;
  function scheduleRender() {
    if (renderDebounceTimer) clearTimeout(renderDebounceTimer);
    renderDebounceTimer = setTimeout(doRender, 300);
    if (!renderCeilingTimer) renderCeilingTimer = setTimeout(() => { doRender(); }, 700);
  }
  async function doRender() {
    if (renderDebounceTimer) { clearTimeout(renderDebounceTimer); renderDebounceTimer = null; }
    if (renderCeilingTimer) { clearTimeout(renderCeilingTimer); renderCeilingTimer = null; }
    try {
      if (isLive) {
        // Alleen tijdens live-modus: score verversen + MusicXML ophalen.
        score = await invoke('notation_get_score', { scoreId });
        // Maatsoort in de werkbalk volgt de score (ook na undo/redo).
        beatsPerBar = score.beats_per_bar || 4; beatUnit = score.beat_unit || 4;
        scheduleAutosave();
        if (viewMode === 'klavar') {
          if (score.generation === lastKlavarGeneration && klavarModel) return;
          lastKlavarGeneration = score.generation;
          klavarModel = await invoke('notation_get_klavar_model', { scoreId });
          // MusicXML alleen voor Opslaan; een lege score geeft hier een fout
          // en dat is in klavar geen rode balk waard.
          try { xml = await invoke('notation_get_musicxml', { scoreId }); } catch (e) { xml = null; }
          error = null;
          await tick();
          scrollNaarNieuwsteNoot();
          return;
        }
        if (score.generation === lastGeneration && xml) return;
        lastGeneration = score.generation;
        // Schermvariant (0.7.86): stemkleuren + notemap; de export blijft kleurloos.
        const notatie = await invoke('notation_get_notation', { scoreId, dimOther: alleenActieveStem });
        xml = notatie.xml;
        notemap = Array.isArray(notatie.notemap) ? notatie.notemap : [];
        notatieQ = Number(notatie.q) || Number(score?.quantize) || 4;
      } else {
        // File-modus: bestand → MusicXML of klavar-model (zelfde balkindeling).
        const staves = (divisions.length > 0 && staffConfig.length > 0)
          ? staffConfig.map(s => {
              const [hand, split] = handVoorKeuze(s.hand, s.split);
              return { name: s.name || null, divisions: s.divisions, bass_clef: s.bass, klavar_hand: hand, klavar_split: split };
            })
          : null;
        const options = {
          bpm: Number(bpm) || 90, beats_per_bar: Number(beatsPerBar) || 4,
          quantize: Number(quantize) || 4, key_fifths: Number(keyFifths) || 0,
          title, staves, tolerance_pct: Number(tolerancePct), minor: !!minor,
        };
        if (viewMode === 'klavar') {
          klavarModel = await invoke('convert_midi_to_klavar_model', { path: filePath, options });
          try { xml = await invoke('convert_midi_to_musicxml', { path: filePath, options }); } catch (e) { xml = null; }
          error = null;
          return;
        }
        xml = await invoke('convert_midi_to_musicxml', { path: filePath, options });
      }
      if (!osmd) {
        osmd = new OpenSheetMusicDisplay(osmdHost, {
          autoResize: true, backend: 'svg', drawTitle: true,
          // Stemkleuren (0.7.86) komen als `notehead color` uit de MusicXML.
          coloringEnabled: true, colorStemsLikeNoteheads: true,
        });
        // Lege maten blijven losse maten (geen "7" als meermaatsrust): daar
        // moet je noten in kunnen plaatsen. De waardestrepen schrijft de
        // MusicXML zelf, per tel van de maatsoort (OSMD's autoBeam kent 6/8
        // niet).
        try { osmd.EngravingRules.RenderMultipleRestMeasures = false; } catch (e) {}
      }
      if (xml) {
        const t0 = performance.now();
        // Tweede argument = terugvaltitel; zonder titel géén "Untitled Score".
        await osmd.load(xml, '');
        osmd.render();
        renderMs = Math.round(performance.now() - t0);
        const parts = (xml.match(/<part id=/g) || []).length || 1;
        renderMaten = Math.round((xml.match(/<measure /g) || []).length / parts);
        console.debug(`[notatie] OSMD load+render ${renderMs} ms, ${renderMaten} maten, ${parts} balken`);
        if (isLive) { buildNoteBoxes(); scrollNaarNieuwsteNoot(); }
      }
      error = null;
    } catch (e) {
      error = String(e);
    }
  }

  // ---- Klik-op-noot: OSMD-noot ↔ LayerEv.id correlatie ----
  // OSMD's interne graphical-model is geen publieke API; alles staat in een
  // try/catch zodat een toekomstige OSMD-wijziging hooguit de overlay uitschakelt
  // (cursor-navigatie blijft dan werken) i.p.v. de render te breken.
  function midiFromPitch(pitch) {
    // Frequentie → MIDI is offset-vrij (geen aannames over OSMD's octaaf-interne
    // nummering). A4 = 440 Hz = MIDI 69.
    const f = pitch?.Frequency;
    if (!f || !(f > 0)) return null;
    return Math.round(69 + 12 * Math.log2(f / 440));
  }
  // Lagen in dezelfde volgorde als render_musicxml de parts nummert: sinds
  // 0.7.83 álle lagen (een lege laag is een part met hele-maatrusten), dus
  // OSMD-balkindex s hoort bij score.layers[s].
  function nonEmptyPartLayers() {
    return score?.layers ?? [];
  }
  function buildNoteBoxes() {
    noteBoxes = [];
    try {
      if (!osmd || !container) return;
      const gsheet = osmd.GraphicSheet || osmd.graphic;
      const measureList = gsheet?.MeasureList;
      if (!measureList) return;
      const partLayers = nonEmptyPartLayers();
      if (partLayers.length === 0) return;
      const bpm = Number(score?.bpm) || 90;
      const wholeNoteSec = 240 / bpm; // 1 hele noot = 4 kwartnoten
      const sheetRect = container.getBoundingClientRect();
      const sx = container.scrollLeft, sy = container.scrollTop;
      // Per laag: platte lijst events uit zichtbare takes (met tijden in sec).
      const eventsByLayer = partLayers.map(l => {
        const evs = [];
        for (const t of l.takes) {
          if (!t.visible) continue;
          for (const e of t.events) {
            evs.push({ id: e.id, midi: e.midi,
              startSec: e.start_us / 1e6, endSec: e.end_us / 1e6 });
          }
        }
        return evs;
      });
      const usedFirst = new Set(); // eventId → eerste notehead al gekoppeld (voor cx/cy-cursor)
      // Exacte koppeling (0.7.86): (part, maat, stem, positie, toon) → event-id.
      const index = new Map();
      for (const r of notemap) index.set(notemapSleutel(r.part, r.measure, r.voice, r.pos, r.midi), r.id);
      const q = notatieQ || Number(score?.quantize) || 4;
      for (let m = 0; m < measureList.length; m++) {
        const row = measureList[m];
        if (!row) continue;
        for (let s = 0; s < row.length; s++) {
          const gMeasure = row[s];
          if (!gMeasure || !gMeasure.staffEntries) continue;
          const layerEvents = eventsByLayer[s];
          if (!layerEvents) continue;
          for (const se of gMeasure.staffEntries) {
            let tsWhole = 0;
            try { const ts = se.getAbsoluteTimestamp();
              tsWhole = (ts?.RealValue != null) ? ts.RealValue
                : (ts ? ts.Numerator / ts.Denominator : 0); } catch (e) {}
            const tSec = tsWhole * wholeNoteSec;
            for (const gve of (se.graphicalVoiceEntries || [])) {
              for (const gnote of (gve.notes || [])) {
                const src = gnote.sourceNote;
                if (!src || (src.isRest && src.isRest())) continue;
                const midi = midiFromPitch(src.Pitch);
                if (midi == null) continue;
                let exactId = null;
                if (index.size) {
                  try {
                    const voice = Number(src.ParentVoiceEntry?.ParentVoice?.VoiceId) || 1;
                    const rt = se.relInMeasureTimestamp ?? se.sourceStaffEntry?.Timestamp;
                    const rv = rt?.RealValue != null ? rt.RealValue : (rt ? rt.Numerator / rt.Denominator : 0);
                    exactId = index.get(notemapSleutel(s, m, voice, rasterPositie(rv, q), midi)) ?? null;
                  } catch (e) { exactId = null; }
                }
                if (exactId != null) {
                  let g = null;
                  try { g = gnote.getSVGGElement && gnote.getSVGGElement(); } catch (e) {}
                  if (!g) continue;
                  const r = g.getBoundingClientRect();
                  if (!r || (r.width === 0 && r.height === 0)) continue;
                  const x = r.left - sheetRect.left + sx;
                  const y = r.top - sheetRect.top + sy;
                  noteBoxes.push({ eventId: exactId, midi, x, y, w: r.width, h: r.height, cx: x + r.width / 2, cy: y + r.height / 2, first: !usedFirst.has(exactId) });
                  usedFirst.add(exactId);
                  continue;
                }
                // Terugval (bestandsmodus, oudere render): zelfde midi in deze
                // laag, tijdstip binnen [start,end];
                // anders dichtstbijzijnde start binnen ~1 tel.
                let best = null, bestScore = Infinity;
                for (const ev of layerEvents) {
                  if (ev.midi !== midi) continue;
                  const contains = tSec >= ev.startSec - 1e-3 && tSec <= ev.endSec + 1e-3;
                  const dStart = Math.abs(ev.startSec - tSec);
                  const score2 = contains ? dStart : dStart + 1000; // interval-match wint
                  if (score2 < bestScore) { bestScore = score2; best = ev; }
                }
                const window = Math.max(0.35, 60 / bpm); // ~1 tel tolerantie
                if (!best || (bestScore >= 1000 && Math.abs(best.startSec - tSec) > window)) continue;
                let g = null;
                try { g = gnote.getSVGGElement && gnote.getSVGGElement(); } catch (e) {}
                if (!g) continue;
                const r = g.getBoundingClientRect();
                if (!r || (r.width === 0 && r.height === 0)) continue;
                const x = r.left - sheetRect.left + sx;
                const y = r.top - sheetRect.top + sy;
                noteBoxes.push({
                  eventId: best.id, midi, x, y, w: r.width, h: r.height,
                  cx: x + r.width / 2, cy: y + r.height / 2,
                  first: !usedFirst.has(best.id),
                });
                usedFirst.add(best.id);
              }
            }
          }
        }
      }
    } catch (e) {
      noteBoxes = []; // stille degradatie: cursor blijft werken
    }
    noteBoxes = noteBoxes; // reactiviteit
  }

  // Klik in de bladmuziek → dichtstbijzijnde gerenderde noot binnen drempel.
  // Shift+klik voegt toe aan de selectie; gewone klik vervangt. De cursor volgt
  // de klik zodat pijltjes vanaf daar verder navigeren.
  function handleSheetClick(e) {
    if (!isLive || viewMode === 'klavar') return;
    if (dragConsumedClick) return; // net een noot versleept, of het menu net gesloten — geen klik
    const rect = container.getBoundingClientRect();
    const px = e.clientX - rect.left + container.scrollLeft;
    const py = e.clientY - rect.top + container.scrollTop;
    const best = noteBoxes.length ? nootOnderPunt(px, py, 40) : null;
    // Gum (kleverig): klik op een kop verwijdert alleen die noot.
    if (gumActief) {
      // Eén noot per klik (niet nog eens bij een dubbelklik); de box meteen weg
      // zodat een trage render hem niet nog eens kan raken.
      if (best && e.detail <= 1) { noteBoxes = noteBoxes.filter(b => b.eventId !== best.eventId); deleteEvents([best.eventId]); }
      return;
    }
    // Stapinvoer: klik plaatst op de aangeklikte maat, tel en toonhoogte in
    // de actieve stem van die balk; Shift+klik voegt toe aan het akkoord op
    // die tel (cursor blijft staan), anders springt de cursor erachter.
    if (stepMode) {
      const hit = sheetPointToPlace(px, py);
      if (!hit) return;
      if (e.shiftKey) stepInsert([hit.midi], hit.layerId, { at: hit.atUs });
      else stepInsert([hit.midi], hit.layerId, { at: hit.atUs, advance: true });
      return;
    }
    if (!best) return;
    chordTekst = '';
    if (lyricMode) { startLyricEdit(best.eventId); return; }
    if (tekenSticky) { pasTekenToe(tekenSticky, [best.eventId]); return; }
    if (e.shiftKey || e.ctrlKey || e.metaKey) {
      const next = new Set(selectionIds);
      next.has(best.eventId) ? next.delete(best.eventId) : next.add(best.eventId);
      selectionIds = next;
    } else {
      selectionIds = new Set([best.eventId]);
    }
    // Cursor gelijktrekken met de aangeklikte noot.
    const idx = flatEvents.findIndex(ev => ev.id === best.eventId);
    if (idx >= 0) cursorIndex = idx;
  }
  // Dubbelklik = het hele akkoord (zelfde balk, maat, stem en inzet).
  function handleSheetDblClick(e) {
    if (!isLive || viewMode === 'klavar' || stepMode || gumActief) return;
    const rect = container.getBoundingClientRect();
    const px = e.clientX - rect.left + container.scrollLeft;
    const py = e.clientY - rect.top + container.scrollTop;
    const best = nootOnderPunt(px, py, 40);
    if (best) selecteerAkkoord(best.eventId);
  }
  // Rechtsklik op een noot: menu met de bewerkingen op de selectie.
  function handleSheetContext(e) {
    if (!isLive || viewMode === 'klavar') return;
    const rect = container.getBoundingClientRect();
    const px = e.clientX - rect.left + container.scrollLeft;
    const py = e.clientY - rect.top + container.scrollTop;
    const best = nootOnderPunt(px, py, 40);
    if (!best) { contextMenu = null; return; }
    if (!selectionIds.has(best.eventId)) {
      selectionIds = new Set([best.eventId]);
      const idx = flatEvents.findIndex(ev => ev.id === best.eventId);
      if (idx >= 0) cursorIndex = idx;
    }
    const nootId = best.eventId;
    contextMenu = { x: e.clientX, y: e.clientY, items: [
      { kop: true, label: tx('notation.ctx_edit') },
      { label: '÷2', on: () => changeDuration('halve') },
      { label: '×2', on: () => changeDuration('double') },
      { label: '• ' + tx('notation.dot'), on: () => changeDuration('dot') },
      { label: '−½', on: () => transposeSelection(-1) },
      { label: '+½', on: () => transposeSelection(+1) },
      { label: '−8va', on: () => transposeSelection(-12) },
      { label: '+8va', on: () => transposeSelection(+12) },
      { scheiding: true },
      { kop: true, label: tx('notation.voices') },
      ...[1, 2, 3, 4].map(v => ({ label: tx('notation.to_voice') + ' ' + v, on: () => setSelectionVoice(v) })),
      { label: tx('notation.split_chord'), on: splitSelectionChord },
      { scheiding: true },
      { kop: true, label: tx('notation.signs_section') },
      { label: '• ' + tx('notation.sign_staccato'), on: () => pasTekenToe('staccato', selectedIdsOrCursor()) },
      { label: '– ' + tx('notation.sign_tenuto'), on: () => pasTekenToe('tenuto', selectedIdsOrCursor()) },
      { label: '> ' + tx('notation.sign_accent'), on: () => pasTekenToe('accent', selectedIdsOrCursor()) },
      { label: '𝄐 ' + tx('notation.sign_fermata'), on: () => pasTekenToe('fermata', selectedIdsOrCursor()) },
      { label: '♯↔♭ ' + tx('notation.sign_spelling'), on: () => pasTekenToe('spelling', selectedIdsOrCursor()) },
      { label: tx('notation.slur'), on: () => spanKnop('slur'), disabled: selectionIds.size < 2 },
      { label: tx('notation.spans_remove'), on: spansWeg },
      { scheiding: true },
      { label: tx('notation.ctx_remove_from_chord'), on: () => deleteEvents([nootId]), disabled: akkoordIds(nootId).length < 2 },
      { label: tx('actions.delete'), on: deleteSelection },
    ] };
  }

  // ---- File-modus initieel: divisies + balk-indeling ----
  async function loadDivisions() {
    try {
      const info = await invoke('get_organ_info');
      divisions = (info?.divisions || []).map(d => d.name);
      klavierBereik = klavierBereikUit(info);
    } catch (e) { divisions = []; }
    if (divisions.length > 0 && staffConfig.length === 0) {
      staffConfig = divisions.map(name => ({ name, divisions: [name], bass: isPedalName(name), hand: 'auto', split: 60 }));
    }
  }
  function toggleStaffDivision(staffIdx, divName) {
    const st = staffConfig[staffIdx];
    st.divisions = st.divisions.includes(divName)
      ? st.divisions.filter(d => d !== divName)
      : [...st.divisions, divName];
    staffConfig = staffConfig;
    scheduleRender();
  }
  function addStaff() {
    staffConfig = [...staffConfig, { name: tx('notation.staff_n').replace('{n}', staffConfig.length + 1), divisions: [], bass: false, hand: 'auto', split: 60 }];
  }
  function removeStaff(staffIdx) { staffConfig = staffConfig.filter((_, i) => i !== staffIdx); scheduleRender(); }
  function setStaffBass(staffIdx, v) { staffConfig[staffIdx].bass = v; staffConfig = staffConfig; scheduleRender(); }

  // ---- Partituurbestand .jmscore (0.7.84): opslaan, openen, reservekopie ----
  let projectPath = null;       // pad van het geopende/opgeslagen stuk (null = nog nooit opgeslagen)
  let savedGeneration = 0;      // generation op het moment van opslaan/laden (-1 = herstelde reservekopie)
  let recentLijst = [];         // [{ path, title, date }] uit localStorage (max 8)
  let autosaveTimer = null;
  let laatsteAutosaveGen = 0;
  let heeftAutosave = false;    // dit venster schreef een reservekopie voor het huidige stuk
  let verwerpBevestigd = false; // "Niet opslaan" al gekozen voor dit stuk: niet nog eens vragen
  let sluitToegestaan = false;  // de sluitvraag is beantwoord: onCloseRequested laat het venster door
  let herstel = null;           // gevonden reservekopieën bij het openen [{ path, title, mtime_ms }]
  $: dirty = isLive && !!score && score.generation !== savedGeneration;
  // In functies NIET op `dirty` leunen: een $:-waarde loopt achter op een
  // zojuist toegekende `score` (Svelte-val) — hier rechtstreeks rekenen.
  function isDirty() { return isLive && !!score && score.generation !== savedGeneration; }
  $: zetVenstertitel(dirty, title);
  async function zetVenstertitel(vuil, titel) {
    if (!isLive) return;
    try {
      const { getCurrentWindow } = await import('@tauri-apps/api/window');
      await getCurrentWindow().setTitle(`${titel || tx('notation.default_title')}${vuil ? '*' : ''} — ${tx('notation.live_window_title')}`);
    } catch (e) {}
  }
  function uiPrefs() { return { view_mode: viewMode, zoom: osmdZoom, klavar_bereik: klavarBereik }; }
  function pasUiPrefsToe(ui) {
    if (!ui) return;
    if (ui.view_mode === 'staff' || ui.view_mode === 'klavar') setViewMode(ui.view_mode);
    if (ui.zoom > 0) setZoom(ui.zoom);
    if (ui.klavar_bereik === 'auto' || ui.klavar_bereik === 'klavier') setKlavarBereik(ui.klavar_bereik);
  }
  function leesRecent() {
    try { const l = JSON.parse(localStorage.getItem('jm-orgue-notation-recent') || '[]'); return Array.isArray(l) ? l.slice(0, 8) : []; } catch (e) { return []; }
  }
  function voegRecentToe(path, titel) {
    const l = leesRecent().filter(r => r && r.path !== path);
    l.unshift({ path, title: titel || '', date: new Date().toISOString() });
    try { localStorage.setItem('jm-orgue-notation-recent', JSON.stringify(l.slice(0, 8))); } catch (e) {}
    recentLijst = l.slice(0, 8);
  }
  async function laadRecent() {
    const l = leesRecent().filter(r => r && typeof r.path === 'string');
    try {
      const bestaat = await invoke('paths_exist', { paths: l.map(r => r.path) });
      recentLijst = l.filter((_, i) => bestaat[i]);
    } catch (e) { recentLijst = l; }
  }
  async function saveProject() {
    if (!isLive || scoreId == null) return false;
    if (!projectPath) return saveProjectAs();
    return schrijfProject(projectPath);
  }
  async function saveProjectAs() {
    if (!isLive || scoreId == null) return false;
    try {
      const { save } = await import('@tauri-apps/plugin-dialog');
      const suggested = (score?.title || tx('notation.default_title')) + '.jmscore';
      const path = await save({ defaultPath: projectPath || suggested, filters: [{ name: tx('notation.project_filter'), extensions: ['jmscore'] }] });
      if (!path) return false;
      return schrijfProject(path);
    } catch (e) { alert(tx('notation.save_failed').replace('{error}', e)); return false; }
  }
  async function schrijfProject(path) {
    try {
      // De backend geeft de generation van wat er op schijf staat terug: een
      // noot die tijdens het schrijven binnenkomt blijft zo "vuil".
      const gen = await invoke('notation_save_project', { scoreId, path, ui: uiPrefs() });
      projectPath = path;
      savedGeneration = Number(gen) || 0;
      verwerpBevestigd = false;
      voegRecentToe(path, score?.title);
      await verwijderAutosave();
      score = await invoke('notation_get_score', { scoreId });
      return true;
    } catch (e) { alert(tx('notation.save_failed').replace('{error}', e)); return false; }
  }
  function openProject(pad = null) {
    if (!isLive) return;
    const doorgaan = () => kiesEnLaadProject(pad);
    if (isDirty() && !verwerpBevestigd) vraagOpslaan(doorgaan); else doorgaan();
  }
  async function kiesEnLaadProject(pad) {
    try {
      let path = pad;
      if (!path) {
        const { open } = await import('@tauri-apps/plugin-dialog');
        path = await open({ multiple: false, filters: [{ name: tx('notation.project_filter'), extensions: ['jmscore'] }] });
        if (!path) return;
      }
      await laadProject(path, path);
    } catch (e) { alert(tx('notation.open_project_failed').replace('{error}', e)); }
  }
  // Laadt een .jmscore (of reservekopie) als het lopende stuk. `bewaarPad` =
  // pad voor Opslaan (null bij een reservekopie: die krijgt "Opslaan als").
  async function laadProject(path, bewaarPad) {
    annuleerAutosave();
    if (recording || armedWaiting) await toggleRecording();
    if (stepMode) await setStepMode(false);
    if (playingScore) await togglePlayScore();
    const oud = scoreId;
    const oudeKopie = heeftAutosave;
    const [nieuwId, ui] = await invoke('notation_load_project', { path });
    scoreId = nieuwId;
    heeftAutosave = false; verwerpBevestigd = false;
    if (oud != null) {
      if (oudeKopie) { try { await invoke('notation_delete_autosave', { scoreId: oud }); } catch (e) {} }
      try { await invoke('notation_close_score', { scoreId: oud }); } catch (e) {}
    }
    resetLokaleState();
    score = await invoke('notation_get_score', { scoreId });
    werkbalkUitScore();
    projectPath = bewaarPad;
    savedGeneration = bewaarPad ? score.generation : -1;
    laatsteAutosaveGen = 0;
    if (bewaarPad) voegRecentToe(bewaarPad, score.title);
    pasUiPrefsToe(ui);
    wizard = null;
    scheduleRender();
  }
  // Drie knoppen: Opslaan / Niet opslaan / Annuleren (eigen modaal).
  function vraagOpslaan(daarna) {
    bevestig = {
      tekst: tx('notation.unsaved_question').replace('{title}', score?.title || tx('notation.default_title')),
      opslaan: async () => { if (await saveProject()) daarna(); },
      ok: daarna, okTekst: tx('notation.dont_save'),
    };
  }
  // Reservekopie: 2 s na de laatste wijziging; weg bij opslaan en bij netjes
  // sluiten. Niet tijdens een opname (de MIDI-thread deelt de score-lock;
  // na Stop volgt de kopie vanzelf). De timer hoort bij één score-id.
  function scheduleAutosave() {
    if (!isDirty() || scoreId == null) return;
    if (recording || armedWaiting) return;
    if (score.generation === laatsteAutosaveGen) return;
    annuleerAutosave();
    const id = scoreId;
    autosaveTimer = setTimeout(async () => {
      autosaveTimer = null;
      if (id !== scoreId || !isDirty()) return;
      if (recording || armedWaiting) return;
      try {
        await invoke('notation_autosave', { scoreId: id, ui: uiPrefs() });
        if (id === scoreId) { laatsteAutosaveGen = score?.generation ?? 0; heeftAutosave = true; }
      } catch (e) {}
    }, 2000);
  }
  function annuleerAutosave() {
    if (autosaveTimer) { clearTimeout(autosaveTimer); autosaveTimer = null; }
  }
  // Alleen een kopie die dít venster schreef mag weg: score-id's beginnen
  // weliswaar boven oude kopieën, maar dubbel gestikt houdt beter.
  function verwijderAutosave() {
    annuleerAutosave();
    if (scoreId != null && heeftAutosave) {
      heeftAutosave = false;
      return invoke('notation_delete_autosave', { scoreId }).catch(() => {});
    }
    return Promise.resolve();
  }
  async function herstelAutosave() {
    const k = herstel && herstel[0];
    herstel = null;
    if (!k) return;
    try {
      await laadProject(k.path, null);
      // De oude kopie hoorde bij een vorig score-id; de nieuwe komt vanzelf.
      await invoke('notation_delete_autosave', { path: k.path });
    } catch (e) {
      // Onleesbaar: melden en opruimen, anders komt de vraag elke keer terug.
      alert(tx('notation.open_project_failed').replace('{error}', e));
      try { await invoke('notation_delete_autosave', { path: k.path }); } catch (e2) {}
      wizard = { mode: 'new', initial: wizardInitial('new') };
    }
  }
  async function verwijderHerstel() {
    const lijst = herstel || [];
    herstel = null;
    for (const k of lijst) { try { await invoke('notation_delete_autosave', { path: k.path }); } catch (e) {} }
    wizard = { mode: 'new', initial: wizardInitial('new') };
  }

  // ---- Wizard "Nieuw stuk" (0.7.83; balkindeling sinds 0.7.14) ----
  // mode 'new': bij het openen van het venster en via Nieuw (Ctrl+N) — het
  // oude stuk gaat dicht en er komt een vers stuk met kop, maatsoort,
  // toonsoort, tempo, raster, aantal maten en balken. mode 'edit' (knop
  // Stuk…): dezelfde velden op het lopende stuk; de balkindeling alleen
  // zolang er geen noten zijn (daarna via de LayerBar).
  let wizard = null;         // { mode: 'new'|'edit', initial } of null
  let divEditLayerId = null; // LayerBar: balk waarvan de divisie-chips openstaan
  let bevestig = null;       // eigen bevestigingsvraag { tekst, ok } (window.confirm werkt niet in Tauri)

  function wizardStaves() {
    const st = (score?.layers ?? []).map(l => ({
      name: l.name,
      bass: l.bass_clef === true || (l.bass_clef == null && isPedalName(l.name)),
      divisions: [...(l.divisions || [])],
      hand: layerHandKeuze(l),
      split: l.klavar_split ?? 60,
    }));
    return st.length ? st : [{ name: tx('notation.staff_n').replace('{n}', 1), bass: false, divisions: [], hand: 'auto', split: 60 }];
  }
  function wizardInitial(mode) {
    return {
      title: mode === 'new' ? '' : (score?.title ?? ''),
      composer: score?.composer ?? '', subtitle: score?.subtitle ?? '',
      beats: Number(score?.beats_per_bar) || 4, unit: Number(score?.beat_unit) || 4,
      keyFifths: Number(score?.key_fifths) || 0, minor: !!score?.minor,
      bpm: Number(score?.bpm) || 90, quantize: Number(score?.quantize) || 4,
      minMeasures: mode === 'new' ? 8 : (Number(score?.min_measures) || 0),
      staves: wizardStaves(),
    };
  }
  function openWizardFromScore() { wizard = { mode: 'edit', initial: wizardInitial('edit') }; }
  function openNewWizard() {
    if (!isLive || wizard || bevestig || herstel || hulpOpen) return;
    if (isDirty() && !verwerpBevestigd) {
      vraagOpslaan(() => { verwerpBevestigd = true; wizard = { mode: 'new', initial: wizardInitial('new') }; });
      return;
    }
    wizard = { mode: 'new', initial: wizardInitial('new') };
  }
  function laagSpecs(staves) {
    return staves.map(s => {
      const [hand, split] = handVoorKeuze(s.hand, s.split);
      return { name: s.name || tx('notation.staff'), bass_clef: !!s.bass, divisions: s.divisions, klavar_hand: hand, klavar_split: split };
    });
  }
  // Alle venster-eigen toestand terug naar nul na een nieuw stuk.
  function resetLokaleState() {
    selectionIds = new Set(); cursorIndex = 0; noteBoxes = []; xml = null; klavarModel = null;
    lastGeneration = 0; lastKlavarGeneration = 0; error = null;
    notationClipboard = { events: [] };
    stepPosUs = 0; stepHeld = new Set(); stepChord = new Set(); stepChordDivisie = new Map();
    stepLastIds = []; stepLastChordNotes = []; stepLastPos = null; stepLastEndUs = null; stepLastUndoCount = 0; stepLastGroepen = null;
    divEditLayerId = null; groeiLaatst = 0;
    stepAcc = null; stepTie = false; caret = null; notemap = [];
    gumActief = false; contextMenu = null; chordTekst = '';
    lyricMode = false; lyricEdit = null; lyricVorigeKoppel = false;
    tekenSticky = null; stepTekens = new Set();
  }
  function werkbalkUitScore() {
    groeiLaatst = 0;
    syncMetronomeFromScore();
    beatsPerBar = score.beats_per_bar || 4; beatUnit = score.beat_unit || 4;
    bpm = score.bpm || 90; keyFifths = score.key_fifths || 0; minor = !!score.minor; title = score.title || '';
    if (score.tolerance_pct != null) tolerancePct = score.tolerance_pct;
  }
  async function onWizardApply(e) {
    const { stuk, staves } = e.detail;
    if (!wizard) return;
    const mode = wizard.mode;
    try {
      if (mode === 'new') {
        annuleerAutosave();
        if (recording || armedWaiting) await toggleRecording();
        if (stepMode) await setStepMode(false);
        if (playingScore) await togglePlayScore();
        const oud = scoreId;
        const oudeKopie = heeftAutosave;
        const spec = {
          title: stuk.title, composer: stuk.composer, subtitle: stuk.subtitle,
          beats_per_bar: stuk.beats, beat_unit: stuk.unit, key_fifths: stuk.keyFifths, minor: !!stuk.minor,
          bpm: stuk.bpm, min_measures: stuk.minMeasures, quantize: stuk.quantize, layers: laagSpecs(staves),
        };
        scoreId = await invoke('notation_new_score', { spec });
        heeftAutosave = false; verwerpBevestigd = false;
        if (oud != null) {
          if (oudeKopie) { try { await invoke('notation_delete_autosave', { scoreId: oud }); } catch (e2) {} }
          try { await invoke('notation_close_score', { scoreId: oud }); } catch (e2) {}
        }
        resetLokaleState();
        score = await invoke('notation_get_score', { scoreId });
        projectPath = null;
        savedGeneration = score.generation;
        laatsteAutosaveGen = 0;
      } else {
        // Lopend stuk: kop, maatsoort, toonsoort, tempo, raster, maten; de
        // balken alleen zolang het stuk leeg is.
        const sc = score;
        if (stuk.title !== sc.title || stuk.composer !== (sc.composer ?? '') || stuk.subtitle !== (sc.subtitle ?? ''))
          await invoke('notation_set_header', { scoreId, title: stuk.title, composer: stuk.composer, subtitle: stuk.subtitle });
        if (stuk.beats !== sc.beats_per_bar || stuk.unit !== (sc.beat_unit ?? 4))
          await invoke('notation_set_meter', { scoreId, beatsPerBar: stuk.beats, beatUnit: stuk.unit });
        if (stuk.keyFifths !== sc.key_fifths) await invoke('notation_set_key', { scoreId, keyFifths: stuk.keyFifths });
        if (!!stuk.minor !== !!sc.minor) await invoke('notation_set_mode', { scoreId, minor: !!stuk.minor });
        if (stuk.bpm !== sc.bpm) await invoke('notation_set_bpm', { scoreId, bpm: stuk.bpm });
        if (stuk.quantize !== sc.quantize) await invoke('notation_set_quantize', { scoreId, quantize: stuk.quantize });
        if (stuk.minMeasures !== (sc.min_measures ?? 0)) await invoke('notation_set_min_measures', { scoreId, minMeasures: stuk.minMeasures });
        // Balken alleen herbouwen als de indeling echt veranderde: herbouwen
        // geeft nieuwe laag-ID's en zet de armed balk terug op de eerste.
        const nieuweIndeling = JSON.stringify(laagSpecs(staves));
        if (!scoreHasEvents && nieuweIndeling !== JSON.stringify(laagSpecs(wizardStaves())))
          await invoke('notation_configure_layers', { scoreId, layers: laagSpecs(staves) });
        score = await invoke('notation_get_score', { scoreId });
      }
      werkbalkUitScore();
      wizard = null;
      scheduleRender();
    } catch (e2) { alert(String(e2)); }
  }
  $: scoreHasEvents = (score?.layers ?? []).some(l => l.takes.some(t => t.events.length > 0));
  async function setLayerDivisions(layerId, divs) {
    try {
      await invoke('notation_set_layer_divisions', { scoreId, layerId, divisions: divs });
      score = await invoke('notation_get_score', { scoreId });
    } catch (e) { alert(String(e)); }
  }
  function toggleLayerDivision(layer, div) {
    const cur = layer.divisions || [];
    setLayerDivisions(layer.id, cur.includes(div) ? cur.filter(d => d !== div) : [...cur, div]);
  }

  // ---- Stapinvoer (0.7.15): noot-voor-noot op kiesbare waarde ----
  // MIDI: speel een toets/akkoord → geplaatst op de invoercursor met de gekozen
  // nootwaarde (gelijktijdig ingedrukte toetsen = akkoord; commit zodra alles
  // los is). Muis: klik op een notenbalk → toonhoogte uit de klik-y (diatonisch,
  // via de VexFlow-stave-lijnen), geplaatst op de invoercursor van die balk.
  let stepMode = false;
  let stepQuarters = 1;      // nootwaarde in kwartnoten (4=heel … 0.125=32e)
  let stepDotted = false;
  let stepPosUs = 0;         // invoercursor (µs)
  let stepHeld = new Set();  // nu ingedrukte toetsen
  let stepChord = new Set(); // verzameld akkoord van deze aanslag
  let stepChordDivisie = new Map(); // midi → divisienaam (0.7.82: routering per balk)
  $: stepDurUs = Math.round((60e6 / (Number(bpm) || 90)) * stepQuarters * (stepDotted ? 1.5 : 1));

  function stepTargetLayerId() {
    return score?.armed_layer ?? score?.layers?.[0]?.id ?? null;
  }
  function stepInitPos() {
    if (cursorEvent) { stepPosUs = cursorEvent.start_us; return; }
    const lid = stepTargetLayerId();
    const le = flatEvents.filter(e => e._layerId === lid);
    stepPosUs = le.length ? Math.max(...le.map(e => e.end_us)) : 0;
  }
  async function setStepMode(on) {
    stepMode = on;
    stepHeld = new Set(); stepChord = new Set();
    // Stapinvoer en liedtekst typen sluiten elkaar uit (reviewbevinding).
    if (on && lyricMode) { lyricMode = false; await sluitLyricEdit(true); }
    if (on) {
      if (recording || armedWaiting) await toggleRecording(); // opname stoppen
      stepInitPos();
      stepCaretMidi = caretStartToon();
    }
    try { await invoke('notation_set_step_input', { scoreId, enabled: on }); } catch (e) {}
  }
  // Laatst geplaatste invoer (voor R=herhaal, ↑/↓=kruis/mol, Shift+letter=
  // akkoord uitbreiden, Backspace=stap terug) — MuseScore-conventies.
  let stepLastIds = [];        // event-ids van de laatste plaatsing
  let stepLastChordNotes = []; // midi's van de laatste plaatsing
  let stepLastPos = null;      // starttijd van de laatste plaatsing
  let stepLastUndoCount = 0;   // backend-commando's van de laatste plaatsing (Backspace neemt ze alle terug)
  let stepLastEndUs = null;    // einde van de laatste plaatsing: overbinden en Alt+cijfer werken alleen aansluitend
  let stepLastGroepen = null;  // [[laagId, noten]] van de laatste meerlaagse aanslag (R herhaalt per balk)
  let stepLastMidi = 60;       // referentie voor letter-invoer (dichtstbijzijnde octaaf)

  async function stepInsert(notes, layerId = null, opts = {}) {
    const lid = layerId ?? stepTargetLayerId();
    if (lid == null || !notes.length) return;
    const at = opts.at != null ? opts.at : stepPosUs;
    const dur = stepDurUs;
    // opts.advance (klavar, 0.7.71): plaatsing op een aangeklikte tijd die
    // de invoercursor meeneemt — in tegenstelling tot opts.at zonder advance,
    // dat het zojuist geplaatste akkoord uitbreidt.
    // Uitbreiden van het laatste akkoord alleen op dezelfde inzet (Shift+letter,
    // tweede balk); een Shift+klik elders is een verse plaatsing die de cursor
    // laat staan (reviewbevinding 0.7.87).
    const zelfdeInzet = opts.at != null && stepLastPos != null && Math.abs(opts.at - stepLastPos) < 1;
    const vers = opts.at == null || opts.advance || !zelfdeInzet;
    const schuif = opts.at == null || opts.advance;
    // Cursor DIRECT opschuiven (vóór de await): bij snel typen lazen
    // opeenvolgende inserts anders dezelfde positie en stapelden de noten
    // als een cluster op één tel (gezien bij de eerste visuele test).
    if (schuif) stepPosUs = at + dur;
    try {
      const res = await invoke('notation_insert_notes', {
        scoreId, layerId: lid, notes,
        startUs: Math.round(at), durUs: dur,
      });
      const ids = Array.isArray(res) ? (res[1] || []) : [];
      // Kleverige articulaties (0.7.89) op elke geplaatste noot; elke toggle
      // is een eigen undo-stap, dus Backspace moet ze meetellen.
      let tekenStappen = 0;
      if (ids.length && stepTekens.size) {
        for (const t of stepTekens) { if (await pasTekenToe(t, ids)) tekenStappen += 1; }
      }
      if (!opts.stil) for (const m of notes) klinkKort(m, lid);
      if (!vers) {
        // Toevoeging aan het zojuist geplaatste akkoord (Shift+letter, tweede balk).
        stepLastIds = [...stepLastIds, ...ids];
        stepLastChordNotes = [...stepLastChordNotes, ...notes];
        stepLastUndoCount += 1 + tekenStappen;
      } else {
        stepLastIds = ids;
        stepLastChordNotes = [...notes];
        stepLastPos = at;
        stepLastEndUs = at + dur;
        stepLastUndoCount = 1 + tekenStappen;
        if (!opts.behoudGroepen) stepLastGroepen = null;
      }
      stepLastMidi = notes[notes.length - 1];
      // De caret blijft een stamtoon (het kleverige voorteken komt er bij het
      // plaatsen pas bij); letters en Enter zetten hieronder de echte stam.
      stepCaretMidi = diatonicToMidi(nearestDiatonicIndex(stepLastMidi));
      groeiMatenNaarCursor();
    } catch (e) {
      if (schuif) stepPosUs = at; // mislukt → cursor terug
      alert(String(e));
    }
  }

  // ---- Invoercursor en kleverige keuzes (0.7.85) ----
  // De caret staat op stepPosUs in de armed balk; ↑/↓ kiest de toon
  // (diatonisch, Shift = octaaf) en Enter plaatst met de paletwaarde. Het
  // voorteken (♯ ♭ ♮) en de overbinding blijven aan tot ze worden uitgezet
  // (Finale: gereedschap kiezen, dan plaatsen).
  let stepCaretMidi = 60;
  let stepAcc = null;          // 1 | -1 | 'nat' | null
  let stepTie = false;
  let caret = null;            // { x, y, h } in content-coördinaten van het blad
  let actieveTab = 'input';
  let hulpOpen = false;
  function caretStartToon() {
    const laag = score?.layers?.find(l => l.id === stepTargetLayerId());
    const bas = !!laag && (laag.bass_clef === true || (laag.bass_clef == null && isPedalName(laag.name)));
    return bas ? 48 : 60;
  }
  function caretStap(d) {
    stepCaretMidi = Math.max(0, Math.min(127, diatonicToMidi(nearestDiatonicIndex(stepCaretMidi) + d)));
  }
  function caretOctaaf(d) { stepCaretMidi = Math.max(0, Math.min(127, stepCaretMidi + 12 * d)); }
  function pasVoortekenToe(midi) {
    if (stepAcc === 1) return Math.min(127, midi + 1);
    if (stepAcc === -1) return Math.max(0, midi - 1);
    return midi; // ♮ en geen keuze: de stamtoon zelf
  }
  async function stepPlaatsCaret() {
    if (!stepMode) return;
    const stam = stepCaretMidi;
    await stepPlaatsMetTie([pasVoortekenToe(stam)]);
    stepCaretMidi = stam;
  }
  // Overbinding (kleverig): dezelfde toon als de laatste plaatsing verlengt
  // die noot in plaats van een nieuwe te zetten.
  async function stepPlaatsMetTie(notes) {
    // Alleen aansluitend: na een rust of een verplaatste cursor komt er een
    // nieuwe noot, geen verlenging over de rust heen.
    const aansluitend = stepLastEndUs != null && Math.abs(stepPosUs - stepLastEndUs) < 1;
    const zelfde = stepTie && aansluitend && stepLastIds.length && stepLastPos != null
      && notes.length === stepLastChordNotes.length && notes.every(n => stepLastChordNotes.includes(n));
    if (zelfde) {
      const nieuwEind = Math.round(stepPosUs + stepDurUs);
      try {
        await invoke('notation_set_durations', { scoreId, ends: stepLastIds.map(id => [id, nieuwEind]) });
        stepPosUs = nieuwEind;
        stepLastEndUs = nieuwEind;
        stepLastUndoCount += 1;
        groeiMatenNaarCursor();
      } catch (e) { alert(String(e)); }
      return;
    }
    await stepInsert(notes);
  }
  // Alt+cijfer: de duur van de laatst geplaatste noot (Finale); de cursor volgt.
  async function stepZetLaatsteDuur(q) {
    if (!stepLastIds.length || stepLastPos == null) return;
    const dur = Math.round((60e6 / (Number(bpm) || 90)) * q * (stepDotted ? 1.5 : 1));
    const nieuwEind = Math.round(stepLastPos + dur);
    stepQuarters = q;
    // Zelfde duur: de backend zet dan geen undo-stap, dus niet meetellen.
    if (stepLastEndUs != null && Math.abs(nieuwEind - stepLastEndUs) < 1) { stepPosUs = nieuwEind; return; }
    try {
      await invoke('notation_set_durations', { scoreId, ends: stepLastIds.map(id => [id, nieuwEind]) });
      stepLastUndoCount += 1;
      stepLastEndUs = nieuwEind;
      stepPosUs = nieuwEind;
      groeiMatenNaarCursor();
    } catch (e) { alert(String(e)); }
  }
  // Gum: de selectie weg; in stapinvoer zonder selectie de laatste plaatsing.
  function stepGum() {
    if (selectionIds.size || (!stepMode && cursorEvent)) deleteSelection();
    else if (stepMode) stepUndoLast();
  }
  function caretOmschrijving(pos, midi, acc) {
    const maatUs = maatDuurUs(bpm, beatsPerBar, beatUnit);
    const m = Math.floor(pos / maatUs + 1e-6);
    const telUs = maatUs / (Number(beatsPerBar) || 4);
    const tel = Math.floor((pos - m * maatUs) / telUs + 1e-6);
    const laag = score?.layers?.find(l => l.id === stepTargetLayerId());
    const toon = noteName(pasVoortekenToe(midi));
    void acc;
    return tx('notation.measure_beat').replace('{m}', m + 1).replace('{b}', tel + 1) + ' · ' + toon + (laag ? ' · ' + laag.name : '') + ' · Enter';
  }
  $: caretTekst = stepMode && score ? caretOmschrijving(stepPosUs, stepCaretMidi, stepAcc) : '';
  // Caretpositie uit de VexFlow-stave van de maat waarin de cursor staat:
  // x lineair tussen het begin en het einde van de notenruimte.
  function herberekenCaret() {
    caret = null;
    if (!isLive || !stepMode || viewMode !== 'staff' || !osmd || !container) return;
    try {
      const gsheet = osmd.GraphicSheet || osmd.graphic;
      const measureList = gsheet?.MeasureList;
      const svg = osmdHost?.querySelector('svg');
      if (!measureList || !measureList.length || !svg) return;
      const lid = stepTargetLayerId();
      const sIdx = (score?.layers ?? []).findIndex(l => l.id === lid);
      if (sIdx < 0) return;
      const maatUs = maatDuurUs(bpm, beatsPerBar, beatUnit);
      let m = Math.floor(stepPosUs / maatUs + 1e-6);
      let frac = (stepPosUs - m * maatUs) / maatUs;
      if (m >= measureList.length) { m = measureList.length - 1; frac = 1; }
      const gm = measureList[m]?.[sIdx];
      const stave = gm && gm.getVFStave && gm.getVFStave();
      if (!stave || !stave.getYForLine) return;
      const x0 = stave.getNoteStartX ? stave.getNoteStartX() : (stave.getX ? stave.getX() : stave.x);
      const x1 = stave.getNoteEndX ? stave.getNoteEndX() : ((stave.getX ? stave.getX() : stave.x) + (stave.getWidth ? stave.getWidth() : stave.width));
      const yTop = stave.getYForLine(0), yBot = stave.getYForLine(4);
      const svgRect = svg.getBoundingClientRect();
      const sheetRect = container.getBoundingClientRect();
      const offX = svgRect.left - sheetRect.left + container.scrollLeft;
      const offY = svgRect.top - sheetRect.top + container.scrollTop;
      // VexFlow-coördinaten zijn ongezoomd (OSMD schaalt via de viewBox):
      // schermpixels = coördinaat × zoom. De x volgt dezelfde inzet-ankers
      // als een klik (sheetGeometry), zodat caret en klik samenvallen.
      const z = osmdZoom || 1;
      const q = notatieQ || Number(score?.quantize) || 4;
      const maatLen = Math.max(1, Math.round((Number(beatsPerBar) || 4) * q * 4 / (Number(beatUnit) || 4)));
      const xs = gridTimeToX(inzettenVanMaat(gm, q), x0, x1, Math.max(0, Math.min(1, frac)) * maatLen, maatLen);
      caret = { x: offX + xs * z, y: offY + yTop * z - 10, h: (yBot - yTop) * z + 20 };
    } catch (e) { caret = null; }
  }
  $: if (isLive) { void [stepMode, stepPosUs, viewMode, score, osmdZoom, noteBoxes]; herberekenCaret(); }
  // Maten groeien mee met de invoercursor (0.7.83): staat hij voorbij de
  // laatste maat, dan vraagt het venster er één bij (geen undo-stap).
  let groeiBezig = false, groeiNogEens = false, groeiLaatst = 0;
  async function groeiMatenNaarCursor() {
    if (!isLive || scoreId == null || !score) return;
    // Een verzoek dat binnenkomt terwijl er een loopt wordt niet weggegooid
    // maar na afloop opnieuw bekeken (toetsherhaling op de rust-toets).
    if (groeiBezig) { groeiNogEens = true; return; }
    groeiBezig = true;
    try {
      do {
        groeiNogEens = false;
        const maatUs = maatDuurUs(bpm, beatsPerBar, beatUnit);
        const inhoud = flatEvents.length ? Math.ceil(Math.max(...flatEvents.map(e => e.end_us)) / maatUs) : 0;
        const huidig = Math.max(Number(score?.min_measures) || 0, inhoud, groeiLaatst, 1);
        const nodig = Math.floor(stepPosUs / maatUs + 1e-6) + 1;
        if (nodig > huidig) {
          groeiLaatst = nodig;
          await invoke('notation_set_min_measures', { scoreId, minMeasures: nodig });
        }
      } while (groeiNogEens);
    } catch (e) {}
    finally { groeiBezig = false; }
  }

  // ---- Klavar (0.7.71): events uit KlavarSheet, handen per balk en per noot ----
  function onKlavarSelect(e) {
    const { id, shift } = e.detail;
    if (gumActief) { deleteEvents([id]); return; }
    if (tekenSticky) { pasTekenToe(tekenSticky, [id]); return; } // kleverig teken ook in klavar (0.7.89)
    if (shift) {
      const next = new Set(selectionIds);
      next.has(id) ? next.delete(id) : next.add(id);
      selectionIds = next;
    } else {
      selectionIds = new Set([id]);
    }
    const idx = flatEvents.findIndex(ev => ev.id === id);
    if (idx >= 0) cursorIndex = idx;
  }
  async function onKlavarDrag(e) {
    const { id, semitones } = e.detail;
    selectionIds = new Set([id]);
    try { await invoke('notation_transpose', { scoreId, eventIds: [id], semitones }); } catch (e2) {}
  }
  // Klik op een lege plek in stapinvoer: de klik-y is de tijd, de x de toets;
  // op de pedaalbalk gaat de noot naar de pedaallaag, anders naar de armed laag.
  function onKlavarStepClick(e) {
    const { midi, staff, gridTime } = e.detail;
    const q = Number(score?.quantize) || 4;
    const gridUs = (60e6 / (Number(bpm) || 90)) / q;
    const at = Math.round(gridTime * gridUs);
    let layerId = null;
    const legend = klavarModel?.legend || [];
    if (staff === 'pedal') {
      const l = legend.find(x => x.hand === 'pedal' && x.layer_id != null);
      layerId = l ? l.layer_id : null;
    } else {
      // Op de manuaalbalk nooit in de pedaallaag plaatsen, ook als die armed is.
      const armed = legend.find(x => x.layer_id === stepTargetLayerId());
      if (armed && armed.hand === 'pedal') {
        const l = legend.find(x => x.hand !== 'pedal' && x.layer_id != null);
        layerId = l ? l.layer_id : null;
      }
    }
    // Shift+klik: noot toevoegen aan het akkoord op die tijd (cursor blijft).
    if (e.detail.shift) stepInsert([midi], layerId, { at });
    else stepInsert([midi], layerId, { at, advance: true });
  }
  // Hand per noot (Alt+←/→ en de knoppen ← L / R →); alleen links of rechts.
  async function setHandsSelection(hand) {
    const ids = selectedIdsOrCursor();
    if (!ids.length) return;
    try { await invoke('notation_set_hands', { scoreId, eventIds: ids, hand }); } catch (e) { alert(String(e)); }
  }
  // Keuze per balk: auto / right / left / pedal / rl (R+L met splitspunt).
  // Codering naar de backend: R+L = hand right + split_midi (0.7.70).
  const SPLIT_OPTIES = [[48, 'c'], [53, 'f'], [60, "c'"], [65, "f'"], [72, "c''"]];
  function handVoorKeuze(keuze, split) {
    if (keuze === 'rl') return ['right', split ?? 60];
    if (keuze === 'right' || keuze === 'left' || keuze === 'pedal') return [keuze, null];
    return [null, null];
  }
  function layerHandKeuze(layer) {
    if (layer.klavar_split != null) return 'rl';
    return layer.klavar_hand || 'auto';
  }
  // Wat de standaardregel voor deze balk heeft gekozen (uit de legenda van het
  // model); `model` als argument zodat de markup meeverandert met het model.
  function autoHandLabel(layer, model, _vertaal) {
    const l = (model?.legend || []).find(x => x.layer_id === layer.id);
    if (!l) return '';
    if (l.hand === 'pedal') return tx('notation.klavar_pedal_label');
    if (l.split_midi != null) return 'R+L';
    return l.hand === 'left' ? 'L' : 'R';
  }
  async function setLayerHand(layer, keuze, split = null) {
    const [hand, splitMidi] = handVoorKeuze(keuze, split ?? layer.klavar_split ?? 60);
    try {
      await invoke('notation_set_layer_hand', { scoreId, layerId: layer.id, hand, splitMidi });
      score = await invoke('notation_get_score', { scoreId });
    } catch (e) { alert(String(e)); }
  }
  // Akkoord afsluiten: per doelbalk invoegen (0.7.82). Een noot waarvan de
  // divisie door een laag wordt gerouteerd gaat naar díe laag, de rest naar
  // de armed laag; alle groepen op dezelfde tel, de cursor schuift één keer.
  function stepCommitChord() {
    const notes = Array.from(stepChord);
    const divisies = stepChordDivisie;
    stepChord = new Set();
    stepChordDivisie = new Map();
    if (!notes.length) return;
    const standaard = stepTargetLayerId();
    const groepen = new Map();
    for (const m of notes) {
      const d = divisies.get(m);
      const laag = d ? (score?.layers || []).find((l) => (l.divisions || []).includes(d)) : null;
      const lid = laag ? laag.id : standaard;
      if (!groepen.has(lid)) groepen.set(lid, []);
      groepen.get(lid).push(m);
    }
    stepPlaatsGroepen([...groepen]);
  }
  // Groepen per balk sequentieel plaatsen: de eerste schuift de cursor, de
  // volgende komen op dezelfde tel; Backspace telt alle commando's mee en R
  // herhaalt dezelfde indeling (reviewbevindingen 0.7.82).
  async function stepPlaatsGroepen(lijst) {
    const at = stepPosUs;
    let eerste = true;
    for (const [lid, groep] of lijst) {
      if (eerste) await stepInsert(groep, lid, { behoudGroepen: true, stil: true });
      else await stepInsert(groep, lid, { at, stil: true });
      eerste = false;
    }
    stepLastGroepen = lijst.length > 1 ? lijst.map(([lid, g]) => [lid, [...g]]) : null;
  }
  function stepRepeatLast() {
    if (stepLastGroepen) stepPlaatsGroepen(stepLastGroepen);
    else if (stepLastChordNotes.length) stepInsert([...stepLastChordNotes]);
  }
  function stepRest() { if (stepMode) { stepPosUs += stepDurUs; groeiMatenNaarCursor(); } }
  // Letter-invoer (A–G): dichtstbijzijnde toonhoogte bij de vorige noot.
  const LETTER_PC = { c: 0, d: 2, e: 4, f: 5, g: 7, a: 9, b: 11 };
  function nearestMidiForLetter(letter) {
    const pc = LETTER_PC[letter];
    let best = null, bestD = Infinity;
    for (let oct = 0; oct <= 10; oct++) {
      const m = 12 * oct + pc;
      if (m > 127) break;
      const d = Math.abs(m - stepLastMidi);
      if (d < bestD) { bestD = d; best = m; }
    }
    return best;
  }
  // Kruis/mol (±1) of octaaf (±12) op de laatste plaatsing.
  async function stepAlterLast(semi) {
    if (!stepLastIds.length) return;
    try {
      await invoke('notation_transpose', { scoreId, eventIds: stepLastIds, semitones: semi });
      stepLastUndoCount += 1; // ook deze stap neemt Backspace terug
      stepLastChordNotes = stepLastChordNotes.map(n => Math.max(0, Math.min(127, n + semi)));
      if (stepLastGroepen) stepLastGroepen = stepLastGroepen.map(([lid, g]) => [lid, g.map(n => Math.max(0, Math.min(127, n + semi)))]);
      stepLastMidi = Math.max(0, Math.min(127, stepLastMidi + semi));
    } catch (e) {}
  }
  // Backspace: laatste plaatsing terugnemen (undo) en de cursor terugzetten.
  async function stepUndoLast() {
    if (stepLastPos == null) return;
    const n = Math.max(1, stepLastUndoCount);
    for (let i = 0; i < n; i++) await doUndo();
    stepPosUs = stepLastPos;
    stepLastIds = []; stepLastChordNotes = []; stepLastPos = null; stepLastEndUs = null; stepLastUndoCount = 0; stepLastGroepen = null;
  }
  function onStepNoteOn(midi, divisie = null) {
    stepHeld.add(midi); stepChord.add(midi);
    if (divisie) stepChordDivisie.set(midi, divisie);
  }
  function onStepNoteOff(midi) {
    stepHeld.delete(midi);
    if (stepHeld.size === 0) stepCommitChord();
  }

  // Muisklik → toonhoogte: vind de maat (VexFlow-stave) onder de klik en reken
  // de balklijn om naar een diatonische toon (stamtoon; alteratie daarna via
  // de transponeer-knoppen/↑↓). Geeft {midi, layerId} of null.
  const DEG_TO_SEMI = [0, 2, 4, 5, 7, 9, 11]; // C D E F G A B
  function diatonicToMidi(di) {
    return 12 * Math.floor(di / 7) + DEG_TO_SEMI[((di % 7) + 7) % 7];
  }
  // Klikpunt → balk, toonhoogte én rastertijd (0.7.87). VexFlow-coördinaten
  // zijn ongezoomd; het klikpunt wordt eerst door de zoom gedeeld. De tel
  // komt uit de getekende inzetten van de maat (lineair ertussen, zie
  // sheetGeometry.xToGridTime). Geeft { midi, layerId, atUs, measure, grid }
  // of null.
  function sheetPointToPlace(px, py) {
    try {
      const gsheet = osmd?.GraphicSheet || osmd?.graphic;
      const measureList = gsheet?.MeasureList;
      const svg = osmdHost?.querySelector('svg');
      if (!measureList || !svg) return null;
      const partLayers = nonEmptyPartLayers();
      const z = osmdZoom || 1;
      const svgRect = svg.getBoundingClientRect();
      const sheetRect = container.getBoundingClientRect();
      const sx = (px - (svgRect.left - sheetRect.left + container.scrollLeft)) / z;
      const sy = (py - (svgRect.top - sheetRect.top + container.scrollTop)) / z;
      const q = notatieQ || Number(score?.quantize) || 4;
      const maatLen = Math.max(1, Math.round((Number(beatsPerBar) || 4) * q * 4 / (Number(beatUnit) || 4)));
      const gridUs = (60e6 / (Number(bpm) || 90)) / q;
      for (let m = 0; m < measureList.length; m++) {
        const row = measureList[m];
        if (!row) continue;
        for (let s = 0; s < row.length; s++) {
          const gm = row[s];
          const stave = gm && gm.getVFStave && gm.getVFStave();
          if (!stave || !stave.getYForLine) continue;
          const x0 = stave.getX ? stave.getX() : stave.x;
          const wd = stave.getWidth ? stave.getWidth() : stave.width;
          const yTop = stave.getYForLine(0), yBot = stave.getYForLine(4);
          const margin = 30; // ook net boven/onder de balk (hulplijn-gebied)
          if (sx >= x0 && sx <= x0 + wd && sy >= yTop - margin && sy <= yBot + margin) {
            const spacing = (yBot - yTop) / 4;
            const stepsFromTop = Math.round(((sy - yTop) / spacing) * 2);
            const layer = partLayers[s];
            if (!layer) return null;
            const bass = layer.bass_clef === true || (layer.bass_clef == null && isPedalName(layer.name));
            const refDi = bass ? 33 : 45; // bovenste lijn: A3 (bas) / F5 (viool)
            const midi = Math.max(0, Math.min(127, diatonicToMidi(refDi - stepsFromTop)));
            // Tel: inzetten van deze maat (OSMD-eenheden × 10 = ongezoomde px).
            const nx0 = stave.getNoteStartX ? stave.getNoteStartX() : x0;
            const nx1 = stave.getNoteEndX ? stave.getNoteEndX() : x0 + wd;
            const entries = inzettenVanMaat(gm, q);
            let grid = xToGridTime(entries, nx0, nx1, sx, maatLen);
            // Stapinvoer: op het raster van de gekozen nootwaarde of op een
            // bestaande inzet, zodat een kwart "ergens op tel 3" ook op tel 3 komt.
            if (stepMode) grid = snapGridTime(grid, entries, Math.round(stepQuarters * q), maatLen);
            const atUs = Math.round((m * maatLen + grid) * gridUs);
            return { midi, layerId: layer.id, atUs, measure: m, grid, celPx: celBreedte(nx0, nx1, maatLen) * z };
          }
        }
      }
    } catch (e) {}
    return null;
  }
  function sheetPointToPitch(px, py) {
    const p = sheetPointToPlace(px, py);
    return p ? { midi: p.midi, layerId: p.layerId } : null;
  }
  // Getekende inzetten van een maat (OSMD-eenheden × 10 = ongezoomde px).
  // Rusten doen niet mee: een hele-maatrust staat in het midden van de maat
  // op tijd 0 en zou het hele linkerdeel op tel 1 leggen (reviewbevinding).
  function inzettenVanMaat(gm, q) {
    const entries = [];
    for (const se of (gm?.staffEntries || [])) {
      try {
        const gves = se.graphicalVoiceEntries || [];
        const alleenRust = gves.length === 0 || gves.every(g => (g.notes || []).every(n => !n.sourceNote || (n.sourceNote.isRest && n.sourceNote.isRest())));
        if (alleenRust) continue;
        const ax = se.PositionAndShape?.AbsolutePosition?.x;
        const rt = se.relInMeasureTimestamp ?? se.sourceStaffEntry?.Timestamp;
        const rv = rt?.RealValue != null ? rt.RealValue : (rt ? rt.Numerator / rt.Denominator : null);
        if (ax != null && rv != null) entries.push({ x: ax * 10, t: rasterPositie(rv, q) });
      } catch (e) {}
    }
    return entries;
  }
  // Dichtstbijzijnde getekende noot bij een punt (content-coördinaten).
  function nootOnderPunt(px, py, drempel) {
    let best = null, bestDist = Infinity;
    for (const b of noteBoxes) {
      const inside = px >= b.x && px <= b.x + b.w && py >= b.y && py <= b.y + b.h;
      const d = inside ? 0 : Math.hypot(px - b.cx, py - b.cy);
      if (d < bestDist) { bestDist = d; best = b; }
    }
    return best && bestDist <= drempel ? best : null;
  }
  async function deleteEvents(ids) {
    if (!ids.length) return;
    try { await invoke('notation_delete_events', { scoreId, eventIds: ids }); selectionIds = new Set(); } catch (e) { alert(String(e)); }
  }
  // Alle noten van hetzelfde akkoord (balk, maat, stem, positie) uit de notemap.
  function akkoordIds(eventId) {
    const ref = notemap.find(r => r.id === eventId);
    if (!ref) return [eventId];
    const ids = new Set(notemap.filter(r => r.part === ref.part && r.measure === ref.measure && r.voice === ref.voice && r.pos === ref.pos && r.id != null).map(r => r.id));
    ids.add(eventId);
    return [...ids];
  }
  function selecteerAkkoord(eventId) {
    const ids = akkoordIds(eventId);
    selectionIds = new Set(ids);
    const evs = flatEvents.filter(ev => ids.includes(ev.id)).sort((a, b) => a.midi - b.midi);
    chordTekst = tx('notation.chord_status').replace('{notes}', evs.map(ev => noteName(ev.midi)).join(' ')).replace('{n}', String(evs.length));
    const idx = flatEvents.findIndex(ev => ev.id === eventId);
    if (idx >= 0) cursorIndex = idx;
  }
  // Geluid bij invoer (0.7.87): de geplaatste noot 150 ms laten klinken op de
  // getrokken registers van de divisie van de balk — niet via de MIDI-lus,
  // dus geen opname of stapinvoer-echo.
  function klinkKort(midi, layerId) {
    if (!geluidBijInvoer || recording || armedWaiting) return;
    const laag = score?.layers?.find(l => l.id === layerId);
    const onlyDivision = laag && laag.divisions && laag.divisions.length ? laag.divisions[0] : null;
    invoke('play_note_all_stops', { note: midi, velocity: 0.8, onlyDivision }).catch(() => {});
    setTimeout(() => { invoke('stop_note_all_stops', { note: midi, onlyDivision }).catch(() => {}); }, 150);
  }

  // ---- Live-modus initieel + event-listeners ----
  let unlisteners = [];
  async function setupLive() {
    try {
      scoreId = await invoke('notation_new_score');
      score = await invoke('notation_get_score', { scoreId });
      syncMetronomeFromScore();
      beatsPerBar = score.beats_per_bar || 4;
      beatUnit = score.beat_unit || 4;
      minor = !!score.minor;
      title = score.title || '';
      await loadDivisions();      // divisienamen voor wizard + LayerBar-chips
      savedGeneration = score.generation; // een vers stuk is schoon
      await laadRecent();
      // Een niet-opgeslagen stuk van een vorige keer? Eerst herstel aanbieden.
      let kopieen = [];
      try { kopieen = await invoke('notation_list_autosaves'); } catch (e) {}
      if (Array.isArray(kopieen) && kopieen.length) herstel = kopieen;
      else wizard = { mode: 'new', initial: wizardInitial('new') }; // nieuw stuk kiezen vóór het inspelen
      // Sluiten met onopgeslagen wijzigingen: eerst vragen (eigen modaal).
      try {
        const { getCurrentWindow } = await import('@tauri-apps/api/window');
        const venster = getCurrentWindow();
        unlisteners.push(await venster.onCloseRequested(async (ev) => {
          if (sluitToegestaan || !isDirty()) { await verwijderAutosave(); return; }
          ev.preventDefault();
          vraagOpslaan(async () => {
            sluitToegestaan = true;
            await verwijderAutosave();
            try { await venster.close(); } catch (e2) {}
          });
        }));
      } catch (e) {}
      lastGeneration = 0;
      // Event-listeners voor live updates + edit-emits.
      const { listen } = await import('@tauri-apps/api/event');
      unlisteners.push(await listen('jm-orgue:notation:note-added', (e) => {
        if (e?.payload?.score !== scoreId) return;
        if (recording) onNoteAdded(e.payload);
        scheduleRender();
      }));
      unlisteners.push(await listen('jm-orgue:notation:note-open', (e) => {
        if (e?.payload?.score !== scoreId || !recording) return;
        onNoteOpen(e.payload);
      }));
      unlisteners.push(await listen('jm-orgue:notation:score-changed', (e) => {
        if (e?.payload?.score !== scoreId) return;
        scheduleRender();
      }));
      // Stapinvoer: gespeelde toetsen op de invoercursor plaatsen.
      unlisteners.push(await listen('jm-orgue:notation:step-note-on', (e) => {
        if (e?.payload?.score !== scoreId || !stepMode) return;
        onStepNoteOn(e.payload.midi, e.payload.division || null);
      }));
      unlisteners.push(await listen('jm-orgue:notation:step-note-off', (e) => {
        if (e?.payload?.score !== scoreId || !stepMode) return;
        onStepNoteOff(e.payload.midi);
      }));
      // Eerste render (lege partituur → foutmelding "geen noten" is normaal).
      await doRender();
    } catch (e) {
      error = String(e);
    }
  }

  async function startBackendRecording() {
    try {
      await invoke('notation_start_recording', { scoreId });
      recording = true;
    } catch (e) { alert(tx('notation.record_error').replace('{error}', e)); }
  }
  async function toggleRecording() {
    if (!scoreId) return;
    try {
      if (recording || armedWaiting) {
        // Stoppen — breekt ook een lopende count-in af.
        stopMetronome();
        if (recording) await invoke('notation_stop_recording');
        recording = false;
        armedWaiting = false;
        stopVolgen();
        // De laatste openstaande notes hebben events geëmit; render zodat
        // ze zichtbaar worden.
        scheduleRender();
      } else {
        // Stapinvoer uit vóór een live-opname (opname gaat vóór).
        if (stepMode) await setStepMode(false);
        // Zorg dat er een armed laag + take is; anders eerste laag armen.
        if (!score) score = await invoke('notation_get_score', { scoreId });
        if (!score.armed_layer && score.layers.length > 0) {
          await invoke('notation_arm_layer', { scoreId, layerId: score.layers[0].id });
        }
        const withCountIn = metroOn && (Number(countInBeats) || 0) > 0;
        if (metroOn) {
          startMetronome(withCountIn);
          // Zonder count-in meteen opnemen; mét count-in start de scheduler de
          // opname zodra het aftellen klaar is.
          if (!withCountIn) await startBackendRecording();
        } else {
          await startBackendRecording();
        }
      }
    } catch (e) { alert(tx('notation.record_error').replace('{error}', e)); }
  }

  // ---- LayerBar-acties ----
  async function armLayer(layerId) {
    try {
      await invoke('notation_arm_layer', { scoreId, layerId });
      score = await invoke('notation_get_score', { scoreId });
    } catch (e) { error = String(e); }
  }
  async function addLayer() {
    try {
      // Geen window.prompt (werkt niet in Tauri): "Balk n", daarna hernoemen
      // met een dubbelklik op de naam (0.7.83).
      const name = tx('notation.staff_n').replace('{n}', (score?.layers?.length ?? 0) + 1);
      await invoke('notation_add_layer', { scoreId, name, bassClef: null });
      score = await invoke('notation_get_score', { scoreId });
      scheduleRender();
    } catch (e) { error = String(e); }
  }
  // Balk hernoemen (dubbelklik in de lagenbalk), verplaatsen (▲▼) en
  // verwijderen (✕) — 0.7.83; de invoer zelf zit sinds 0.7.85 in LayerBar.
  async function renameLayer(layerId, naam) {
    const n = (naam || '').trim();
    const laag = score?.layers?.find(l => l.id === layerId);
    if (!n || !laag || n === laag.name) return;
    try {
      await invoke('notation_rename_layer', { scoreId, layerId, name: n });
      score = await invoke('notation_get_score', { scoreId });
    } catch (e) { alert(String(e)); }
  }
  async function moveLayer(layerId, delta) {
    try {
      await invoke('notation_move_layer', { scoreId, layerId, delta });
      score = await invoke('notation_get_score', { scoreId });
    } catch (e) { alert(String(e)); }
  }
  async function removeLayerNow(layerId) {
    try {
      // Open noten van een opname hangen aan (laag, take): opname eerst stoppen.
      if (recording || armedWaiting) await toggleRecording();
      await invoke('notation_remove_layer', { scoreId, layerId });
      score = await invoke('notation_get_score', { scoreId });
    } catch (e) { alert(String(e)); }
  }
  function removeLayer(layer) {
    const heeftNoten = layer.takes.some(t => t.events.length > 0);
    if (!heeftNoten) { removeLayerNow(layer.id); return; }
    bevestig = { tekst: tx('notation.remove_staff_confirm').replace('{name}', layer.name), ok: () => removeLayerNow(layer.id) };
  }
  async function addTake(layerId) {
    try {
      await invoke('notation_add_take', { scoreId, layerId });
      score = await invoke('notation_get_score', { scoreId });
      scheduleRender();
    } catch (e) { error = String(e); }
  }
  async function toggleTakeVisible(layerId, takeId, visible) {
    try {
      await invoke('notation_set_take_visible', { scoreId, layerId, takeId, visible });
      score = await invoke('notation_get_score', { scoreId });
      scheduleRender();
    } catch (e) { error = String(e); }
  }

  // "Openen…" in live-modus: kies een MIDI-bestand en importeer het als
  // extra take(s) in de huidige score — per laag een nieuwe take
  // "Import (<bestandsnaam>)". Zo blijft bestaand werk staan en zet de
  // import zich er náást (in plaats van "een ander scherm openen").
  async function openMidiFile() {
    if (!isLive || scoreId == null) return;
    try {
      const { open } = await import('@tauri-apps/plugin-dialog');
      const sel = await open({
        multiple: false,
        filters: [{ name: tx('notation.midi_file_filter'), extensions: ['mid', 'midi'] }],
      });
      if (!sel) return;
      await invoke('notation_import_midi', { scoreId, path: sel, intoTake: null });
      // score-changed-event zorgt voor render; score-snapshot verfrissen voor LayerBar.
      score = await invoke('notation_get_score', { scoreId });
    } catch (e) {
      alert(tx('notation.midi_import_failed').replace('{error}', e));
    }
  }

  // ---- Selectie + bewerkacties (cursor-based) ----
  // Alle noot-events uit alle zichtbare takes op tijd-volgorde. Cursor-index
  // volgt deze lijst; visuele highlight is voorlopig een tekstuele indicator
  // in de toolbar (klik-op-noot in de SVG is v2 werk).
  $: flatEvents = (score?.layers ?? []).flatMap(l =>
    l.takes.filter(t => t.visible).flatMap(t => t.events.map(e => ({ ...e, _layer: l.name, _layerId: l.id, _takeId: t.id })))
  ).sort((a, b) => a.start_us - b.start_us);
  let cursorIndex = 0;
  $: if (cursorIndex >= flatEvents.length) cursorIndex = Math.max(0, flatEvents.length - 1);
  $: cursorEvent = flatEvents[cursorIndex] || null;

  function noteName(midi) {
    const names = ['C','C#','D','D#','E','F','F#','G','G#','A','A#','B'];
    return names[midi % 12] + Math.floor(midi / 12 - 1);
  }

  function moveCursor(delta) {
    if (flatEvents.length === 0) return;
    cursorIndex = Math.max(0, Math.min(flatEvents.length - 1, cursorIndex + delta));
    selectionIds = new Set([flatEvents[cursorIndex].id]);
  }
  async function deleteSelection() {
    if (selectionIds.size === 0 && cursorEvent) selectionIds = new Set([cursorEvent.id]);
    if (selectionIds.size === 0) return;
    try {
      await invoke('notation_delete_events', { scoreId, eventIds: Array.from(selectionIds) });
      selectionIds = new Set();
      // Score wordt via score-changed-event opgehaald → cursor volgt automatisch.
    } catch (e) { alert(String(e)); }
  }
  async function transposeSelection(semitones) {
    if (selectionIds.size === 0 && cursorEvent) selectionIds = new Set([cursorEvent.id]);
    if (selectionIds.size === 0) return;
    try {
      await invoke('notation_transpose', { scoreId, eventIds: Array.from(selectionIds), semitones });
    } catch (e) { alert(String(e)); }
  }
  async function setToleranceLive(pct) {
    if (!isLive || scoreId == null) return;
    tolerancePct = pct;
    try { await invoke('notation_set_tolerance', { scoreId, tolerancePct: pct }); } catch (e) {}
  }
  async function setBpmLive(v) {
    if (!isLive || scoreId == null) return;
    bpm = Number(v) || 90;
    try { await invoke('notation_set_bpm', { scoreId, bpm }); } catch (e) {}
  }
  async function setKeyLive(v) {
    if (!isLive || scoreId == null) return;
    keyFifths = Number(v) || 0;
    try { await invoke('notation_set_key', { scoreId, keyFifths }); } catch (e) {}
  }
  async function setModeLive(isMinor) {
    if (!isLive || scoreId == null) return;
    minor = !!isMinor;
    try { await invoke('notation_set_mode', { scoreId, minor }); } catch (e) {}
  }
  async function doUndo() { try { await invoke('notation_undo', { scoreId }); } catch (e) { alert(String(e)); } }
  async function doRedo() { try { await invoke('notation_redo', { scoreId }); } catch (e) { alert(String(e)); } }

  // ---- Corrigeren: verschuiven, slepen, zoom, afspelen (0.7.27) ----
  // Verschuif de selectie één rastereenheid in de tijd (Shift+←/→).
  async function shiftSelection(gridSteps) {
    const ids = selectedIdsOrCursor();
    if (!ids.length) return;
    const gridUs = (60e6 / (Number(bpm) || 90)) / (Number(score?.quantize) || 4);
    try {
      await invoke('notation_shift_events', { scoreId, eventIds: ids, deltaUs: Math.round(gridSteps * gridUs) });
    } catch (e) {}
  }

  // Verticaal slepen op een noot = toonhoogte wijzigen (diatonisch, zoals
  // MuseScore; halve balklijn ≈ 5 px bij zoom 1). Horizontaal verschuiven gaat
  // via Shift+←/→ (raster-exact — slepen in de tijd is optisch onnauwkeurig).
  let noteDrag = null; // { id, startY, origMidi, moved }
  let dragConsumedClick = false;
  function nearestDiatonicIndex(midi) {
    const oct = Math.floor(midi / 12), pc = midi % 12;
    const DEG = [0, 2, 4, 5, 7, 9, 11];
    let deg = 0;
    for (let i = 0; i < 7; i++) if (DEG[i] <= pc) deg = i;
    return oct * 7 + deg;
  }
  function handleSheetMouseDown(e) {
    if (!isLive || viewMode === 'klavar' || stepMode || gumActief || dragConsumedClick || noteBoxes.length === 0) return;
    const rect = container.getBoundingClientRect();
    const px = e.clientX - rect.left + container.scrollLeft;
    const py = e.clientY - rect.top + container.scrollTop;
    let best = null, bestDist = Infinity;
    for (const b of noteBoxes) {
      const inside = px >= b.x && px <= b.x + b.w && py >= b.y && py <= b.y + b.h;
      const d = inside ? 0 : Math.hypot(px - b.cx, py - b.cy);
      if (d < bestDist) { bestDist = d; best = b; }
    }
    if (!best || bestDist > 12) return; // alleen op/vlakbij een noot beginnen
    if (e.button !== 0) return;
    // Celbreedte van de maat van deze noot (voor Shift+slepen in de tijd).
    let celPx = null;
    try {
      const ref = notemap.find(r => r.id === best.eventId);
      const gm = ref ? (osmd?.GraphicSheet || osmd?.graphic)?.MeasureList?.[ref.measure]?.[ref.part] : null;
      const stave = gm && gm.getVFStave && gm.getVFStave();
      if (stave) {
        const q = notatieQ || Number(score?.quantize) || 4;
        const maatLen = Math.max(1, Math.round((Number(beatsPerBar) || 4) * q * 4 / (Number(beatUnit) || 4)));
        const nx0 = stave.getNoteStartX ? stave.getNoteStartX() : stave.getX();
        const nx1 = stave.getNoteEndX ? stave.getNoteEndX() : stave.getX() + stave.getWidth();
        celPx = celBreedte(nx0, nx1, maatLen) * (osmdZoom || 1);
      }
    } catch (e2) { celPx = null; }
    noteDrag = { id: best.eventId, startX: e.clientX, startY: e.clientY, origMidi: best.midi, moved: false, celPx };
    window.addEventListener('mousemove', handleNoteDragMove);
    window.addEventListener('mouseup', handleNoteDragUp);
  }
  function handleNoteDragMove(e) {
    if (noteDrag && (Math.abs(e.clientY - noteDrag.startY) > 5 || Math.abs(e.clientX - noteDrag.startX) > 5)) noteDrag.moved = true;
  }
  async function handleNoteDragUp(e) {
    window.removeEventListener('mousemove', handleNoteDragMove);
    window.removeEventListener('mouseup', handleNoteDragUp);
    const d = noteDrag; noteDrag = null;
    if (!d || !d.moved) return;
    // De click ná de mouseup mag geen selectie-wijziging meer doen.
    dragConsumedClick = true;
    setTimeout(() => { dragConsumedClick = false; }, 0);
    const dx = e.clientX - d.startX, dy = e.clientY - d.startY;
    const ids = selectionIds.has(d.id) && selectionIds.size > 1 ? Array.from(selectionIds) : [d.id];
    // Shift + overwegend horizontaal: verschuiven in de tijd, per rastercel (0.7.87).
    if (e.shiftKey && Math.abs(dx) >= Math.abs(dy) && d.celPx) {
      const cellen = Math.round(dx / d.celPx);
      if (!cellen) return;
      const q = notatieQ || Number(score?.quantize) || 4;
      const gridUs = (60e6 / (Number(bpm) || 90)) / q;
      selectionIds = new Set(ids);
      try { await invoke('notation_shift_events', { scoreId, eventIds: ids, deltaUs: Math.round(cellen * gridUs) }); } catch (e2) {}
      return;
    }
    const steps = -Math.round(dy / (5 * (osmdZoom || 1)));
    if (!steps) return;
    // Alt + verticaal: chromatisch (halve tonen); anders diatonisch.
    const to = e.altKey
      ? Math.max(0, Math.min(127, d.origMidi + steps))
      : Math.max(0, Math.min(127, diatonicToMidi(nearestDiatonicIndex(d.origMidi) + steps)));
    const delta = Math.max(-24, Math.min(24, to - d.origMidi));
    if (!delta) return;
    selectionIds = new Set(ids);
    try { await invoke('notation_transpose', { scoreId, eventIds: ids, semitones: delta }); } catch (e2) {}
  }

  // Zoom (OSMD.Zoom): knoppen −/+ in de toolbar; noot-boxen daarna hermeten.
  let osmdZoom = 1;
  function setZoom(z) {
    osmdZoom = Math.max(0.5, Math.min(2, Math.round(z * 10) / 10));
    if (osmd) {
      try {
        // Ook in klavar bijhouden, anders loopt OSMD achter bij terugschakelen
        // (en rekent het verticaal slepen met de verkeerde schaal).
        osmd.Zoom = osmdZoom;
        if (viewMode !== 'klavar') { osmd.render(); if (isLive) buildNoteBoxes(); }
      } catch (e) {}
    }
  }

  // Afspelen: partituur → tijdelijk .mid → bestaande MIDI-speler (speelt door
  // het orgel met de huidige registratie). Nogmaals klikken stopt.
  let playingScore = false;
  let playPollTimer = null;
  let afspelenStarten = false;   // herintrede-guard: ▶ tweemaal vóór playingScore true
  async function togglePlayScore() {
    if (playingScore) {
      try { await invoke('midi_stop_playback'); } catch (e) {}
      playingScore = false;
      if (playPollTimer) { clearInterval(playPollTimer); playPollTimer = null; }
      stopVolgen();
      return;
    }
    if (afspelenStarten) return;
    afspelenStarten = true;
    try {
      const { tempDir, join } = await import('@tauri-apps/api/path');
      const path = await join(await tempDir(), `jm-orgue-notatie-${scoreId}.mid`);
      await invoke('notation_export_midi', { scoreId, path });
      await invoke('midi_play_file', { path });
      playingScore = true;
      // Meelopen (0.7.73): de MIDI-export begint op tijd 0 van de score, dus
      // de spelerpositie is de scoretijd. De klok wordt elke poll bijgesteld;
      // in klavar loopt de nu-lijn, in notenschrift volgt het blad de noot.
      syncOpnameKlok(0);
      startVolgen('afspelen');
      const gestart = performance.now();
      // Einde detecteren zodat de knop terugspringt naar ▶ (de speler meldt
      // vlak na de start soms nog "stopped": de eerste 600 ms negeren). De
      // timer wist zichzelf via zijn eigen id, nooit via de gedeelde variabele.
      if (playPollTimer) clearInterval(playPollTimer);
      const timer = setInterval(async () => {
        try {
          const s = await invoke('midi_player_status');
          if (s?.state === 'paused') { volgPauze = true; return; }
          volgPauze = false;
          if (!s || s.state !== 'playing') {
            if (performance.now() - gestart < 600) return;
            playingScore = false;
            clearInterval(timer);
            if (playPollTimer === timer) playPollTimer = null;
            stopVolgen();
            return;
          }
          // Snelheid schatten uit het verschil tussen twee polls (0,25×–4×).
          const nu = performance.now();
          if (volgVorige && nu - volgVorige.perf > 100) {
            const f = (s.position_ms - volgVorige.ms) / (nu - volgVorige.perf);
            if (f > 0) volgSnelheid = Math.min(4, Math.max(0.25, f));
          }
          volgVorige = { ms: s.position_ms, perf: nu };
          syncOpnameKlok(s.position_ms * 1000);
          if (viewMode !== 'klavar') scrollNaarNootBijTijd(s.position_ms * 1000);
        } catch (e) {}
      }, 250);
      playPollTimer = timer;
    } catch (e) { alert(tx('notation.play_failed').replace('{error}', e)); }
    finally { afspelenStarten = false; }
  }

  // ---- Maatsoort + titel + MIDI-export (0.7.16) ----
  $: maatsoortKeuzes = MAATSOORTEN.includes(beatsPerBar + '/' + beatUnit) ? MAATSOORTEN : [...MAATSOORTEN, beatsPerBar + '/' + beatUnit];
  async function setQuantizeLive(q) {
    if (!isLive || scoreId == null) return;
    try { await invoke('notation_set_quantize', { scoreId, quantize: Number(q) || 4 }); } catch (e) {}
  }
  async function setMeterLive(v) {
    if (!isLive || scoreId == null) return;
    const [b, u] = splitsMaatsoort(v);
    beatsPerBar = b; beatUnit = u;
    try { await invoke('notation_set_meter', { scoreId, beatsPerBar: b, beatUnit: u }); } catch (e) {}
  }
  async function setTitleLive(v) {
    if (!isLive || scoreId == null) return;
    title = v;
    try { await invoke('notation_set_title', { scoreId, title: v }); } catch (e) {}
  }
  async function saveMidiAs() {
    if (!isLive || scoreId == null) return;
    try {
      const { save } = await import('@tauri-apps/plugin-dialog');
      const suggested = (score?.title || tx('notation.default_title')) + '.mid';
      const path = await save({ defaultPath: suggested, filters: [{ name: tx('notation.midi_file_filter'), extensions: ['mid'] }] });
      if (path) { await invoke('notation_export_midi', { scoreId, path }); alert(tx('notation.midi_saved').replace('{path}', path)); }
    } catch (e) { alert(tx('notation.midi_save_failed').replace('{error}', e)); }
  }

  // ---- Duur wijzigen + kopiëren/plakken (0.7.6) ----
  // In-app klembord (verdwijnt bij sluiten van het venster). Elke entry bewaart
  // de noot relatief aan de vroegste geselecteerde inzet, zodat plakken de groep
  // als geheel cursor-relatief neerzet.
  let notationClipboard = { events: [] };
  $: clipboardCount = notationClipboard.events.length;

  function selectedIdsOrCursor() {
    if (selectionIds.size) return Array.from(selectionIds);
    return cursorEvent ? [cursorEvent.id] : [];
  }
  // Heuristiek: is deze duur (µs) een gepunteerde waarde? → dan haalt de
  // punt-toggle de punt eraf (×2/3), anders zet hij er een op (×1.5).
  function isDottedDur(durUs) {
    const q = 60e6 / (Number(bpm) || 90); // µs per kwartnoot
    const base = (durUs / q) / 1.5;       // ontpunte lengte in kwartnoten
    return [0.125, 0.25, 0.5, 1, 2, 4, 8].some(p => Math.abs(base - p) < 0.06 * p + 0.02);
  }
  async function changeDuration(kind) {
    const ids = selectedIdsOrCursor();
    if (!ids.length) return;
    const byId = new Map(flatEvents.map(e => [e.id, e]));
    const ends = [];
    for (const id of ids) {
      const ev = byId.get(id);
      if (!ev) continue;
      const dur = ev.end_us - ev.start_us;
      let f = kind === 'halve' ? 0.5 : kind === 'double' ? 2 : (isDottedDur(dur) ? (2 / 3) : 1.5);
      const newEnd = Math.round(ev.start_us + Math.max(1000, dur * f));
      ends.push([id, newEnd]);
    }
    if (!ends.length) return;
    try { await invoke('notation_set_durations', { scoreId, ends }); } catch (e) { alert(String(e)); }
  }
  function copySelection() {
    const ids = selectedIdsOrCursor();
    const evs = flatEvents.filter(e => ids.includes(e.id));
    if (!evs.length) return;
    const minStart = Math.min(...evs.map(e => e.start_us));
    notationClipboard = { events: evs.map(e => ({
      midi: e.midi, channel: e.channel ?? 0, hand: e.hand ?? null, voice: Number(e.voice) || 0,
      rel_start: e.start_us - minStart, dur: Math.max(1000, e.end_us - e.start_us),
      lyrics: Array.isArray(e.lyrics) ? e.lyrics.map(l => ({ ...l })) : [],
      articulations: Array.isArray(e.articulations) ? [...e.articulations] : [], spelling: e.spelling ? { ...e.spelling } : null,
    })) };
  }
  async function pasteClipboard() {
    if (!isLive || scoreId == null || notationClipboard.events.length === 0) return;
    // Doellaag: laag van de cursor-noot, anders de armed laag, anders de eerste.
    const targetLayerId = cursorEvent?._layerId ?? score?.armed_layer ?? score?.layers?.[0]?.id;
    if (targetLayerId == null) return;
    // Basistijd: cursorpositie; zonder cursor het eind van de doel-take.
    let baseUs;
    if (cursorEvent) {
      baseUs = cursorEvent.start_us;
    } else {
      const le = flatEvents.filter(e => e._layerId === targetLayerId);
      baseUs = le.length ? Math.max(...le.map(e => e.end_us)) : 0;
    }
    // Snap naar de rasterlijn van de doelbalk.
    const gridUs = (60e6 / (Number(bpm) || 90)) / (Number(score?.quantize) || 4);
    const snapped = Math.round(baseUs / gridUs) * gridUs;
    const notes = notationClipboard.events.map(e => ({
      midi: e.midi, channel: e.channel ?? 0, hand: e.hand ?? null, voice: Number(e.voice) || 0,
      start_us: Math.round(snapped + e.rel_start),
      end_us: Math.round(snapped + e.rel_start + e.dur),
      lyrics: e.lyrics ?? [],
      articulations: e.articulations ?? [], spelling: e.spelling ?? null,
    }));
    try { await invoke('notation_paste', { scoreId, layerId: targetLayerId, notes }); } catch (e) { alert(String(e)); }
  }

  function handleKey(e) {
    if (e.target && (e.target.tagName === 'INPUT' || e.target.tagName === 'TEXTAREA' || e.target.tagName === 'SELECT')) return;
    // Modaal open: alleen Escape (sluiten), verder geen sneltoetsen op de partituur.
    if (wizard || bevestig || herstel || hulpOpen || contextMenu) {
      if (e.key === 'Escape' && !herstel) { wizard = null; bevestig = null; hulpOpen = false; contextMenu = null; e.preventDefault(); }
      return;
    }
    // Nieuw stuk (0.7.83), opslaan/openen (0.7.84), in beide standen.
    if ((e.ctrlKey || e.metaKey) && !e.shiftKey && !e.altKey && e.key.toLowerCase() === 'n') { openNewWizard(); e.preventDefault(); return; }
    if ((e.ctrlKey || e.metaKey) && !e.altKey && e.key.toLowerCase() === 's') { if (e.shiftKey) saveProjectAs(); else saveProject(); e.preventDefault(); return; }
    if ((e.ctrlKey || e.metaKey) && !e.shiftKey && !e.altKey && e.key.toLowerCase() === 'o') { openProject(); e.preventDefault(); return; }
    // Ctrl+1..4: tabbladen (0.7.85) — op e.code, onafhankelijk van de toetsenbordindeling.
    if (isLive && (e.ctrlKey || e.metaKey) && !e.altKey && /^Digit[1-4]$/.test(e.code)) {
      actieveTab = ['input', 'texts', 'staves', 'playback'][Number(e.code.slice(5)) - 1];
      e.preventDefault(); return;
    }
    // K wisselt de weergave (0.7.73), in beide standen.
    if ((e.key === 'k' || e.key === 'K') && !e.ctrlKey && !e.altKey && !e.metaKey) {
      setViewMode(viewMode === 'klavar' ? 'staff' : 'klavar');
      e.preventDefault();
      return;
    }
    if (!isLive) return;
    // Stemmen (0.7.86): Shift+Alt+1..4 = actieve stem van de armed balk,
    // Shift+Alt+↑/↓ = selectie naar vorige/volgende stem (V = alleen actieve stem).
    if (e.shiftKey && e.altKey && !e.ctrlKey && !e.metaKey) {
      if (/^Digit[1-4]$/.test(e.code)) { const lid = stepTargetLayerId(); if (lid != null) setActiveVoice(lid, Number(e.code.slice(5))); e.preventDefault(); return; }
      if (e.key === 'ArrowUp' || e.key === 'ArrowDown') {
        setSelectionVoice(Math.max(1, Math.min(4, stemVanSelectie() + (e.key === 'ArrowUp' ? -1 : 1))));
        e.preventDefault(); return;
      }
    }
    if (e.key === 'Escape' && tekenSticky) { tekenSticky = null; e.preventDefault(); return; }
    if (e.key === 'Escape' && gumActief) { gumActief = false; e.preventDefault(); return; }
    // V = alleen de actieve stem in kleur (Shift+Alt+S botst met de Windows-indelingswissel).
    if ((e.key === 'v' || e.key === 'V') && !e.ctrlKey && !e.altKey && !e.metaKey && !e.shiftKey) { toggleAlleenActieveStem(); e.preventDefault(); return; }
    // ---- Stapinvoer-sneltoetsen (MuseScore-conventies) ----
    if (stepMode) {
      const k = e.key.toLowerCase();
      const DUR_CODES = { Digit2: 0.125, Digit3: 0.25, Digit4: 0.5, Digit5: 1, Digit6: 2, Digit7: 4,
        Numpad2: 0.125, Numpad3: 0.25, Numpad4: 0.5, Numpad5: 1, Numpad6: 2, Numpad7: 4 };
      if (e.key === 'Escape') { setStepMode(false); e.preventDefault(); return; }
      // Letters A–G: noot plaatsen (dichtstbijzijnde octaaf, met het kleverige
      // voorteken); Shift+letter = toevoegen aan het zojuist geplaatste akkoord.
      if (!e.ctrlKey && !e.metaKey && !e.altKey && LETTER_PC[k] !== undefined) {
        const m0 = nearestMidiForLetter(k);
        if (m0 != null) {
          const m = pasVoortekenToe(m0);
          if (e.shiftKey && stepLastPos != null) stepInsert([m], null, { at: stepLastPos }).then(() => { stepCaretMidi = m0; });
          else stepPlaatsMetTie([m]).then(() => { stepCaretMidi = m0; });
        }
        e.preventDefault(); return;
      }
      // Enter plaatst op de toonhoogte van de invoercursor (0.7.85).
      if (e.key === 'Enter' && !e.ctrlKey && !e.metaKey && !e.altKey) { stepPlaatsCaret(); e.preventDefault(); return; }
      if (k === 'r' && !e.ctrlKey && !e.metaKey && !e.altKey && stepLastChordNotes.length) { stepRepeatLast(); e.preventDefault(); return; }
      // Numpad alleen als de toets echt een cijfer gaf (NumLock aan), anders is het een pijl/Insert.
      const isCijfer = /^[0-9]$/.test(e.key);
      if (e.code === 'Digit0' || (e.code === 'Numpad0' && isCijfer)) { stepRest(); e.preventDefault(); return; }
      // Cijfer: paletwaarde; Alt+cijfer: duur van de laatst geplaatste noot (Finale).
      if (DUR_CODES[e.code] !== undefined && (e.code.startsWith('Digit') || isCijfer) && !e.ctrlKey && !e.metaKey) {
        if (e.altKey) stepZetLaatsteDuur(DUR_CODES[e.code]); else stepQuarters = DUR_CODES[e.code];
        e.preventDefault(); return;
      }
      if (e.key === '.') { stepDotted = !stepDotted; e.preventDefault(); return; }
      // ↑/↓: invoercursor een toon (Shift: octaaf); Alt+↑/↓: laatste noot kruis/mol.
      if (e.key === 'ArrowUp' || e.key === 'ArrowDown') {
        const d = e.key === 'ArrowUp' ? 1 : -1;
        if (e.altKey) stepAlterLast(e.ctrlKey ? 12 * d : d);
        else if (e.shiftKey) caretOctaaf(d);
        else caretStap(d);
        e.preventDefault(); return;
      }
      if (e.key === 'Backspace') { stepUndoLast(); e.preventDefault(); return; }
      // overige toetsen (Ctrl+Z e.d.) vallen door naar de gewone afhandeling
    }
    // N = stapinvoer aan/uit (MuseScore).
    if (e.key.toLowerCase() === 'n' && !e.ctrlKey && !e.metaKey && !e.shiftKey) { setStepMode(!stepMode); e.preventDefault(); return; }
    // Tekens (0.7.89, Finale-metatools, alleen buiten stapinvoer — daar zijn
    // A–G toonnamen): S staccato, E tenuto, A accent, F fermate, 9 = ♯↔♭,
    // L legatoboog, + / − haarspeld, 8 = 8va (Shift+8 = 8vb).
    if (!e.ctrlKey && !e.metaKey && !e.altKey) {
      const kl = e.key.toLowerCase();
      const TEKEN_TOETS = { s: 'staccato', e: 'tenuto', a: 'accent', f: 'fermata' };
      if (!e.shiftKey && TEKEN_TOETS[kl]) { tekenKnop(TEKEN_TOETS[kl]); e.preventDefault(); return; }
      if (e.code === 'Digit9' && !e.shiftKey) { tekenKnop('spelling'); e.preventDefault(); return; }
      // Bogen alleen buiten stapinvoer (daar is 8 een duur en geeft een lege
      // selectie anders een melding); de fysieke 8 wint van '-'/'_' (AZERTY).
      if (!stepMode && kl === 'l' && !e.shiftKey) { spanKnop('slur'); e.preventDefault(); return; }
      if (!stepMode && e.code === 'Digit8') { spanKnop(e.shiftKey ? 'octavedown' : 'octaveup'); e.preventDefault(); return; }
      if (!stepMode && (e.key === '+' || e.key === '=')) { spanKnop('crescendo'); e.preventDefault(); return; }
      if (!stepMode && (e.key === '-' || e.key === '_')) { spanKnop('diminuendo'); e.preventDefault(); return; }
    }
    // Alt+←/→ = geselecteerde noten naar de linker-/rechterhand (klavar, 0.7.71).
    if (e.altKey && e.key === 'ArrowLeft') { setHandsSelection('left'); e.preventDefault(); return; }
    if (e.altKey && e.key === 'ArrowRight') { setHandsSelection('right'); e.preventDefault(); return; }
    // Shift+←/→ = selectie één rastereenheid verschuiven in de tijd (0.7.27).
    if (e.key === 'ArrowRight' && e.shiftKey) { shiftSelection(+1); e.preventDefault(); return; }
    if (e.key === 'ArrowLeft' && e.shiftKey) { shiftSelection(-1); e.preventDefault(); return; }
    if (e.key === 'ArrowRight') { moveCursor(+1); e.preventDefault(); }
    else if (e.key === 'ArrowLeft') { moveCursor(-1); e.preventDefault(); }
    else if (e.key === 'Delete' || e.key === 'Backspace') { deleteSelection(); e.preventDefault(); }
    else if (e.key === 'ArrowUp' && e.shiftKey) { transposeSelection(12); e.preventDefault(); }
    else if (e.key === 'ArrowDown' && e.shiftKey) { transposeSelection(-12); e.preventDefault(); }
    else if (e.key === 'ArrowUp') { transposeSelection(1); e.preventDefault(); }
    else if (e.key === 'ArrowDown') { transposeSelection(-1); e.preventDefault(); }
    else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'c') { copySelection(); e.preventDefault(); }
    else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'v') { pasteClipboard(); e.preventDefault(); }
    else if ((e.ctrlKey || e.metaKey) && !e.shiftKey && e.key.toLowerCase() === 'z') { doUndo(); e.preventDefault(); }
    else if ((e.ctrlKey || e.metaKey) && (e.shiftKey && e.key.toLowerCase() === 'z' || e.key.toLowerCase() === 'y')) { doRedo(); e.preventDefault(); }
    else if (e.key === '[') { changeDuration('halve'); e.preventDefault(); }
    else if (e.key === ']') { changeDuration('double'); e.preventDefault(); }
    else if (e.key === '.') { changeDuration('dot'); e.preventDefault(); }
  }

  // ---- File-modus bestandsacties ----
  async function openOtherFile() {
    try {
      const { open } = await import('@tauri-apps/plugin-dialog');
      const sel = await open({ multiple: false, filters: [{ name: tx('notation.midi_file_filter'), extensions: ['mid', 'midi'] }] });
      if (sel) { filePath = sel; title = titleFromPath(sel); scheduleRender(); }
    } catch (e) { error = String(e); }
  }
  async function saveMusicXml() {
    if (!xml) return;
    try {
      // Live: de kleurloze exportvariant, niet de schermrender (0.7.86).
      let exportXml = xml;
      if (isLive && scoreId != null) { try { exportXml = await invoke('notation_get_musicxml', { scoreId }); } catch (e) {} }
      const { save } = await import('@tauri-apps/plugin-dialog');
      const suggested = (filePath ? filePath.replace(/\.(mid|midi)$/i, '') : (score?.title || tx('notation.default_musicxml_name'))) + '.musicxml';
      const path = await save({ defaultPath: suggested, filters: [{ name: 'MusicXML', extensions: ['musicxml', 'xml'] }] });
      if (path) { await invoke('save_musicxml', { path, xml: exportXml }); alert(tx('notation.musicxml_saved').replace('{path}', path)); }
    } catch (e) { alert(tx('notation.save_failed').replace('{error}', e)); }
  }
  function printScore() { window.print(); }

  // Opslaan als SVG (0.7.72, klavar): de getekende systemen uit de DOM, onder
  // elkaar in één SVG in mm, met de kop (titel, tempo, legenda) en de
  // tekenstijlen ingebed, zodat het bestand op zichzelf staat.
  const KLAVAR_SVG_STIJL = `
    .k-lijn{stroke:#000} .k-tel{stroke:#000} .k-maat{stroke:#000}
    .k-tekst{fill:#333;font-family:Georgia,'Times New Roman',serif}
    .k-stok{stroke:#000} .k-kop{fill:#fff;stroke:#000} .k-kop.zwart{fill:#000}
    .k-stop{fill:none;stroke:#000;stroke-linejoin:miter} .k-stip{fill:#000}
    .k-balk{fill:none;stroke:#000;stroke-linejoin:round}
    .k-label{fill:#333;font-family:Georgia,'Times New Roman',serif;font-style:italic}
    .k-toonsoort{fill:none;stroke:#000}`;
  function klavarAlsSvg() {
    const systemen = Array.from(container?.querySelectorAll('svg.klavar-systeem') || []);
    if (!systemen.length || !klavarModel) return null;
    const esc = (s) => String(s ?? '').replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
    const kop = 16;
    // Maten in mm met twee decimalen (geen 110.92000000000002 in het bestand).
    const mm = (v) => String(Math.round(v * 100) / 100);
    let breedte = 0, hoogte = kop;
    const delen = [];
    for (const svg of systemen) {
      const [, , w, h] = (svg.getAttribute('viewBox') || '0 0 100 100').split(/\s+/).map(v => Math.round(Number(v) * 100) / 100);
      breedte = Math.max(breedte, w);
      // Zonder de opname-overlay (nu-lijn, aangehouden toetsen); de
      // Svelte-scope-klassen weglaten, de k-*-klassen blijven.
      const kloon = svg.cloneNode(true);
      kloon.querySelectorAll('.k-nu, .k-open').forEach(el => el.remove());
      const inhoud = kloon.innerHTML.replace(/\s+class="([^"]*)"/g, (m, c) => ' class="' + c.split(/\s+/).filter(k => !k.startsWith('svelte-')).join(' ') + '"');
      delen.push(`<svg x="0" y="${mm(hoogte)}" width="${mm(w)}" height="${mm(h)}" viewBox="0 0 ${mm(w)} ${mm(h)}">${inhoud}</svg>`);
      hoogte += h;
    }
    const legenda = (klavarModel.legend || []).map(l => `${l.name} (${l.hand === 'pedal' ? tx('notation.klavar_pedal_label') : l.split_midi != null ? 'R+L' : l.hand === 'left' ? 'L' : 'R'})`).join(' · ');
    const tekst = `<text x="2" y="6" font-size="4.2" font-family="Georgia, serif" font-weight="bold">${esc(klavarModel.title)}</text>` +
      `<text x="2" y="11" font-size="2.6" font-family="Georgia, serif">${esc(tx('notation.klavar_tempo').replace('{bpm}', Math.round(klavarModel.bpm)))}   ${esc(tx('notation.klavar_legend'))}: ${esc(legenda)}</text>`;
    return `<?xml version="1.0" encoding="UTF-8"?>\n<svg xmlns="http://www.w3.org/2000/svg" width="${mm(breedte)}mm" height="${mm(hoogte)}mm" viewBox="0 0 ${mm(breedte)} ${mm(hoogte)}">\n<style>${KLAVAR_SVG_STIJL}</style>\n${tekst}\n${delen.join('\n')}\n</svg>\n`;
  }
  async function saveKlavarSvg() {
    const svg = klavarAlsSvg();
    if (!svg) return;
    try {
      const { save } = await import('@tauri-apps/plugin-dialog');
      const suggested = (filePath ? filePath.replace(/\.(mid|midi)$/i, '') : (score?.title || tx('notation.default_title'))) + '-klavar.svg';
      const path = await save({ defaultPath: suggested, filters: [{ name: 'SVG', extensions: ['svg'] }] });
      if (path) { await invoke('save_text_file', { path, text: svg }); alert(tx('notation.svg_saved').replace('{path}', path)); }
    } catch (e) { alert(tx('notation.svg_save_failed').replace('{error}', e)); }
  }

  // OSMD's autoResize re-rendert bij vensterwijziging; de noot-boxen (in pixels)
  // moeten dan opnieuw worden opgemeten. Debounce zodat een sleep-resize niet
  // tientallen keren herberekent.
  let resizeTimer = null;
  function handleResize() {
    if (!isLive || viewMode === 'klavar') return;
    if (resizeTimer) clearTimeout(resizeTimer);
    resizeTimer = setTimeout(() => { buildNoteBoxes(); }, 200);
  }

  // Sfeer uit het hoofdvenster volgen (dit venster heeft geen Console-poll).
  let sfeerTimer = null;
  onMount(async () => {
    window.addEventListener('keydown', handleKey);
    window.addEventListener('resize', handleResize);
    sfeerTimer = setInterval(pasSfeerToeAlsGewijzigd, 1000);
    if (isLive) {
      await setupLive();
    } else {
      await loadDivisions();
      await doRender();
    }
  });
  onDestroy(() => {
    window.removeEventListener('keydown', handleKey);
    window.removeEventListener('resize', handleResize);
    if (renderDebounceTimer) clearTimeout(renderDebounceTimer);
    if (renderCeilingTimer) clearTimeout(renderCeilingTimer);
    if (resizeTimer) clearTimeout(resizeTimer);
    if (playPollTimer) clearInterval(playPollTimer);
    if (sfeerTimer) clearInterval(sfeerTimer);
    if (playingScore) invoke('midi_stop_playback').catch(() => {});
    window.removeEventListener('mousemove', handleNoteDragMove);
    window.removeEventListener('mouseup', handleNoteDragUp);
    stopMetronome();
    if (metroAudioCtx) { try { metroAudioCtx.close(); } catch (e) {} metroAudioCtx = null; }
    for (const un of unlisteners) { try { un(); } catch (e) {} }
    stopVolgen();
    // Live-opname + stapinvoer veilig stoppen bij sluiten.
    if (recording && scoreId != null) invoke('notation_stop_recording').catch(() => {});
    if (stepMode && scoreId != null) invoke('notation_set_step_input', { scoreId, enabled: false }).catch(() => {});
  });

  // ---- Stemmen per balk (0.7.86) ----
  async function setActiveVoice(layerId, v) {
    try {
      await invoke('notation_set_active_voice', { scoreId, layerId, voice: v });
      score = await invoke('notation_get_score', { scoreId });
      if (alleenActieveStem) { lastGeneration = 0; scheduleRender(); }
    } catch (e) { alert(String(e)); }
  }
  function toggleAlleenActieveStem() {
    alleenActieveStem = !alleenActieveStem;
    lastGeneration = 0;
    scheduleRender();
  }
  async function setSelectionVoice(v) {
    const ids = selectedIdsOrCursor();
    if (!ids.length) return;
    try { await invoke('notation_set_voice', { scoreId, eventIds: ids, voice: v }); } catch (e) { alert(String(e)); }
  }
  async function splitSelectionChord() {
    const ids = selectedIdsOrCursor();
    if (!ids.length) return;
    try { await invoke('notation_split_chord_to_voices', { scoreId, eventIds: ids }); } catch (e) { alert(String(e)); }
  }
  // Stem van de selectie zelf (de cursor hoeft er niet in te liggen); bij
  // gemengde stemmen de laagste.
  function stemVanSelectie() {
    const ids = selectedIdsOrCursor();
    const stemmen = flatEvents.filter(ev => ids.includes(ev.id)).map(ev => Number(ev.voice) || 1);
    if (stemmen.length) return Math.min(...stemmen);
    return cursorEvent ? (Number(cursorEvent.voice) || 1) : 1;
  }

  // ---- Acties voor de onderdelen (0.7.85): de logica blijft hier ----
  const kopActies = {
    nieuw: openNewWizard, openProject: () => openProject(), openRecent: (p) => openProject(p),
    save: () => saveProject(), saveAs: () => saveProjectAs(), importMidi: openMidiFile,
    exportMusicXml: saveMusicXml, exportMidi: saveMidiAs, exportSvg: saveKlavarSvg, print: printScore,
    toggleRecording, togglePlay: togglePlayScore, setBpm: setBpmLive, undo: doUndo, redo: doRedo,
    setViewMode, zoom: (d) => setZoom(osmdZoom + d), help: () => { hulpOpen = true; }, setTitle: setTitleLive,
  };
  const invoerActies = {
    setStepMode, setQuarters: (q) => { stepQuarters = q; }, toggleDot: () => { stepDotted = !stepDotted; },
    rest: stepRest, repeat: () => { if (stepLastChordNotes.length) stepRepeatLast(); },
    setAcc: (a) => { stepAcc = a; }, toggleTie: () => { stepTie = !stepTie; }, gum: toggleGum, alterLast: stepAlterLast,
    togglePreview: toggleGeluidBijInvoer,
    moveCursor, deleteSelection, transpose: transposeSelection, setHands: setHandsSelection,
    changeDuration, copy: copySelection, paste: pasteClipboard,
  };
  const balkenActies = {
    setMeter: setMeterLive, setKey: setKeyLive, setMode: setModeLive, setQuantize: setQuantizeLive,
    setTolerance: setToleranceLive, addLayer, openWizard: openWizardFromScore, toggleLayerDivision, setLayerHand,
    toVoice: setSelectionVoice, splitChord: splitSelectionChord, toggleOnlyActive: toggleAlleenActieveStem,
  };
  const afspeelActies = { setMetronome: setMetronomeCfg, previewTempo: () => previewTempo(), setKlavarBereik };
  const tekstActies = {
    setHeader: setHeaderLive, toggleLyricMode, setStrofe, addText: addTextAtCursor, editText: editTextById,
    removeText: removeTextById, moveText: moveTextById, vulRegistratie,
    teken: tekenKnop, span: spanKnop, spansWeg, maatteken,
  };
  const laagActies = { arm: armLayer, rename: renameLayer, move: moveLayer, remove: removeLayer, addTake, toggleTakeVisible, addLayer, setActiveVoice };
  function bevestigKnoppen(b) {
    const lijst = [{ label: tx('actions.cancel'), stijl: 'secondary', on: () => { bevestig = null; } }];
    if (b.opslaan) {
      lijst.push({ label: b.okTekst, stijl: 'ghost', on: () => { const ok = b.ok; bevestig = null; ok(); } });
      lijst.push({ label: tx('actions.save'), stijl: 'primary', on: () => { const f = b.opslaan; bevestig = null; f(); } });
    } else {
      lijst.push({ label: b.okTekst || tx('notation.confirm_continue'), stijl: 'primary', on: () => { const ok = b.ok; bevestig = null; ok(); } });
    }
    return lijst;
  }
  function herstelTekst(h) {
    return tx('notation.autosave_found').replace('{title}', h[0].title || tx('notation.default_title')).replace('{date}', new Date(h[0].mtime_ms).toLocaleString());
  }
</script>

<div class="notation-window">
  {#if isLive && wizard}
    <!-- {#key}: elke opening een verse instantie, ook als `wizard` wordt vervangen. -->
    {#key wizard}
      <NewScoreWizard mode={wizard.mode} initial={wizard.initial} {divisions} hasNotes={scoreHasEvents}
        showHand={viewMode === 'klavar'} splitOpties={SPLIT_OPTIES} {keyChoices} {gridChoices}
        on:apply={onWizardApply} on:close={() => wizard = null}
        on:previewTempo={(e) => previewTempo(e.detail.bpm, e.detail.beats, e.detail.unit)}
        recent={recentLijst} on:openProject={() => openProject()} on:openRecent={(e) => openProject(e.detail.path)} />
    {/key}
  {/if}
  {#if bevestig}
    <ConfirmDialog tekst={bevestig.tekst} knoppen={bevestigKnoppen(bevestig)} on:cancel={() => bevestig = null} />
  {/if}
  {#if herstel}
    <!-- Reservekopie van een vorige keer (0.7.84); Escape sluit niet: eerst kiezen. -->
    <ConfirmDialog tekst={herstelTekst(herstel)}
      knoppen={[{ label: $t('notation.discard'), stijl: 'secondary', on: verwijderHerstel }, { label: $t('notation.restore'), stijl: 'primary', on: herstelAutosave }]} />
  {/if}
  {#if hulpOpen}
    <HelpDialog on:close={() => hulpOpen = false} />
  {/if}
  {#if contextMenu}
    <ContextMenu x={contextMenu.x} y={contextMenu.y} items={contextMenu.items} on:close={sluitContextMenu} />
  {/if}
  <!-- Het notatievenster is nog niet nagelopen; dat hoort de gebruiker te
       weten vóór hij op het resultaat vertrouwt. -->
  <div class="notation-alpha" role="note">
    <span class="alpha-tag">{$t('notation.alpha_badge')}</span>
    <span>{$t('notation.alpha_notice')}</span>
  </div>
  {#if isLive}
    <NotationHeader {title} {dirty} {recording} {armedWaiting} {countInRemaining} {playingScore} {bpm} {viewMode}
      zoom={osmdZoom} recent={recentLijst} kanExportXml={!!xml} kanExportSvg={!!(klavarModel && scoreHasEvents)}
      kanPrint={viewMode === 'klavar' ? !!(klavarModel && scoreHasEvents) : !!xml} acties={kopActies} />
    <NotationTabs tab={actieveTab} on:change={(e) => actieveTab = e.detail} />
    <div class="tab-inhoud">
      {#if actieveTab === 'input'}
        <InputTab {stepMode} {stepQuarters} {stepDotted} acc={stepAcc} tie={stepTie} kanHerhaal={stepLastChordNotes.length > 0} kanAlterLast={stepLastIds.length > 0}
          gum={gumActief} previewOn={geluidBijInvoer}
          {viewMode} {selectieAlleenPedaal} {clipboardCount} acties={invoerActies} />
      {:else if actieveTab === 'texts'}
        <TextsMarksTab {title} composer={score?.composer ?? ''} subtitle={score?.subtitle ?? ''} {lyricMode} strofe={lyricStrofe}
          texts={score?.texts ?? []} {laagNaam} {tijdTekst} acties={tekstActies}
          {tekenSticky} {stepTekens} {stepMode} selectieAantal={selectionIds.size} />
      {:else if actieveTab === 'staves'}
        <StavesVoicesTab layers={score?.layers ?? []} {divisions} {viewMode} {klavarModel} splitOpties={SPLIT_OPTIES}
          {layerHandKeuze} {autoHandLabel} maatsoort={beatsPerBar + '/' + beatUnit} {maatsoortKeuzes} {keyFifths} {minor}
          quantize={Number(score?.quantize) || 4} {tolerancePct} {keyChoices} {gridChoices} onlyActive={alleenActieveStem} acties={balkenActies} />
      {:else}
        <PlaybackTab {metroOn} {countInBeats} {viewMode} {klavarBereik} acties={afspeelActies} />
      {/if}
    </div>
  {:else}
  <div class="notation-toolbar">
      <label>{$t('notation.tempo')}
        <input type="number" min="20" max="300" bind:value={bpm} on:change={scheduleRender} />
      </label>
      <label>{$t('notation.meter')}
        <select bind:value={beatsPerBar} on:change={scheduleRender}>
          <option value={2}>2/4</option><option value={3}>3/4</option>
          <option value={4}>4/4</option><option value={6}>6/4</option>
        </select>
      </label>
      <label>{$t('notation.grid')}
        <select bind:value={quantize} on:change={scheduleRender}>
          {#each gridChoices as g}<option value={g.v}>{g.label}</option>{/each}
        </select>
      </label>
      <label>{$t('notation.key_signature')}
        <select bind:value={keyFifths} on:change={scheduleRender}>
          {#each keyChoices as k}<option value={k.v}>{k.label}</option>{/each}
        </select>
        <select bind:value={minor} on:change={scheduleRender} title={$t('notation.key_mode')}>
          <option value={false}>{$t('notation.key_major')}</option>
          <option value={true}>{$t('notation.key_minor')}</option>
        </select>
      </label>
      <label class="tolerance-slider" title={$t('notation.tolerance_title')}>
        {$t('notation.rhythm')}
        <input type="range" min="0" max="100" step="5" bind:value={tolerancePct} on:change={scheduleRender} />
        <span class="tolerance-value">{tolerancePct < 33 ? $t('notation.tol_loose') : tolerancePct > 66 ? $t('notation.tol_tight') : $t('notation.tol_medium')}</span>
      </label>
      <label class="notation-title-field">{$t('notation.title')}
        <input type="text" bind:value={title} on:change={scheduleRender} />
      </label>
      <span class="notation-spacer"></span>
      <span class="view-switch" role="group" aria-label={$t('notation.view')}>
        <button class="btn btn-ghost btn-sm" class:active={viewMode === 'staff'} aria-pressed={viewMode === 'staff'} on:click={() => setViewMode('staff')} title={$t('notation.view_staff_title') + ' (K)'}>{$t('notation.view_staff')}</button>
        <button class="btn btn-ghost btn-sm" class:active={viewMode === 'klavar'} aria-pressed={viewMode === 'klavar'} on:click={() => setViewMode('klavar')} title={$t('notation.view_klavar_title') + ' (K)'}>{$t('notation.view_klavar')}</button>
      </span>
      {#if viewMode === 'klavar'}
        <label class="klavar-bereik" title={$t('notation.klavar_range_title')}>{$t('notation.klavar_range')}
          <select value={klavarBereik} on:change={(e) => setKlavarBereik(e.currentTarget.value)}>
            <option value="auto">{$t('notation.klavar_range_auto')}</option>
            <option value="klavier">{$t('notation.klavar_range_keyboard')}</option>
          </select>
        </label>
      {/if}
      <button class="btn btn-ghost btn-sm" on:click={openOtherFile}>{$t('notation.open')}</button>
      <button class="btn btn-ghost btn-sm" on:click={saveMusicXml} disabled={!xml}>{$t('notation.save_as_musicxml')}</button>
      {#if viewMode === 'klavar'}
        <button class="btn btn-ghost btn-sm" on:click={saveKlavarSvg} disabled={!klavarModel} title={$t('notation.save_as_svg_title')}>{$t('notation.save_as_svg')}</button>
      {/if}
      <button class="btn btn-primary btn-sm" on:click={printScore} disabled={viewMode === 'klavar' ? !klavarModel : !xml}>{$t('notation.print_pdf')}</button>
  </div>
  {/if}

  {#if isLive && score}
    <LayerBar layers={score.layers} armedLayer={score.armed_layer} {recording} acties={laagActies} />
    <StatusLine {armedWaiting} {countInRemaining} aantalNoten={flatEvents.length} selectieAantal={selectionIds.size}
      cursorNaam={cursorEvent ? noteName(cursorEvent.midi) : ''} {cursorIndex} cursorLaag={cursorEvent?._layer ?? ''}
      {stepMode} {caretTekst} extra={tekenTekst || chordTekst} debugTekst={debugVlag ? `OSMD ${renderMs} ms · ${renderMaten} m.` : ''} />
  {/if}

  {#if !isLive && divisions.length > 0}
    <div class="notation-staves">
      <span class="notation-staves-label">{$t('notation.staves_label_full')}</span>
      {#each staffConfig as st, i}
        <div class="notation-staff-card">
          <input class="notation-staff-name" type="text" bind:value={st.name} on:change={scheduleRender} />
          {#each divisions as div}
            <label class="notation-staff-div">
              <input type="checkbox" checked={st.divisions.includes(div)} on:change={() => toggleStaffDivision(i, div)} />
              {div}
            </label>
          {/each}
          <label class="notation-staff-div notation-staff-bass" title={$t('notation.bass_clef')}>
            <input type="checkbox" checked={st.bass} on:change={(e) => setStaffBass(i, e.currentTarget.checked)} />𝄢
          </label>
          {#if viewMode === 'klavar'}
            <select class="layer-hand" bind:value={st.hand} on:change={scheduleRender} title={$t('notation.hand_title')}>
              <option value="auto">{$t('notation.hand')}: {$t('notation.hand_auto')}</option>
              <option value="right">{$t('notation.hand_right')}</option>
              <option value="left">{$t('notation.hand_left')}</option>
              <option value="pedal">{$t('notation.hand_pedal')}</option>
              <option value="rl">{$t('notation.hand_split')}</option>
            </select>
            {#if st.hand === 'rl'}
              <select class="layer-split" bind:value={st.split} on:change={scheduleRender} title={$t('notation.hand_split_title')}>
                {#each SPLIT_OPTIES as [m, naam]}<option value={m}>{naam}</option>{/each}
              </select>
            {/if}
          {/if}
          {#if staffConfig.length > 1}
            <button class="notation-staff-remove" on:click={() => removeStaff(i)} aria-label={$t('notation.remove_staff')}>×</button>
          {/if}
        </div>
      {/each}
      <button class="btn btn-ghost btn-sm" on:click={addStaff}>{$t('notation.add_staff')}</button>
    </div>
  {/if}

  {#if error}<div class="notation-error">{error}</div>{/if}
  {#if converting}<div class="notation-busy">{$t('notation.busy')}</div>{/if}

  <div class="notation-sheet" class:clickable={isLive && viewMode === 'staff'} class:gum={gumActief} bind:this={container}
    on:click={handleSheetClick} on:mousedown={handleSheetMouseDown} on:dblclick={handleSheetDblClick} on:contextmenu|preventDefault={handleSheetContext}>
    <!-- OSMD tekent in zijn eigen host; die blijft gemount (OSMD houdt de
         containerreferentie) en wordt in klavar met visibility verborgen,
         niet met display:none — OSMD meet de breedte bij render/autoResize. -->
    <div class="osmd-host" class:verborgen={viewMode === 'klavar'} bind:this={osmdHost}></div>
    {#if viewMode === 'klavar'}
      <KlavarSheet
        model={klavarModel}
        {selectionIds}
        cursorId={cursorEvent?.id ?? null}
        zoom={osmdZoom}
        {stepMode}
        gum={gumActief}
        {recording}
        {nowGrid}
        {openNotes}
        bereik={bereikVoorSheet}
        live={isLive}
        on:select={onKlavarSelect}
        on:drag={onKlavarDrag}
        on:stepclick={onKlavarStepClick}
      />
    {/if}
    {#if isLive && viewMode === 'staff'}
      <!-- Selectie-overlay: absolute-position markering per geselecteerde noot
           (OSMD zelf blijft ongemuteerd; selectie wijzigen vergt geen re-render). -->
      <div class="notation-selection-overlay">
        {#each selectedBoxes as b (b.eventId + '@' + b.x + ',' + b.y)}
          <div class="note-highlight"
            style="left:{b.x - 3}px; top:{b.y - 3}px; width:{b.w + 6}px; height:{b.h + 6}px;"></div>
        {/each}
        {#if stepMode && caret}
          <!-- Invoercursor (0.7.85): waar de volgende noot komt. -->
          <div class="step-caret" style="left:{caret.x}px; top:{caret.y}px; height:{caret.h}px;"></div>
        {/if}
      </div>
      {#if lyricEdit}
        <!-- Liedtekst typen (0.7.88): invoerveld onder de noot; spatie/- naar de volgende. -->
        {@const lb = noteBoxes.find(b => b.eventId === lyricEdit.eventId && b.first) || noteBoxes.find(b => b.eventId === lyricEdit.eventId)}
        {#if lb}
          <input class="lyric-editor" type="text" bind:value={lyricEdit.text} bind:this={lyricInput}
            style="left:{lb.x - 10}px; top:{lb.y + lb.h + 6}px;" use:focusSelect
            on:keydown={onLyricKey} on:blur={() => sluitLyricEdit(true)} on:click|stopPropagation on:mousedown|stopPropagation />
        {/if}
      {/if}
    {/if}
  </div>

  {#if !isLive}<div class="notation-hint">{$t('notation.hint_file')}</div>{/if}
</div>

<style>
  .notation-window { display: flex; flex-direction: column; height: 100vh; background: #fff; color: #222; }
  .notation-alpha {
    display: flex; align-items: center; gap: 0.5rem;
    padding: 0.35rem 0.75rem;
    background: color-mix(in srgb, var(--warning, #b8860b) 22%, var(--bg-panel, #2a2a2a));
    color: var(--text, #eee);
    border-bottom: 1px solid var(--warning, #b8860b);
    font-size: 0.78rem;
    flex-shrink: 0;
  }
  .notation-alpha .alpha-tag {
    padding: 0 0.3rem; border-radius: 3px;
    background: var(--warning, #b8860b); color: #fff;
    font-size: 0.62rem; font-weight: 700; letter-spacing: 0.04em;
    text-transform: uppercase; white-space: nowrap;
  }
  .notation-toolbar {
    display: flex; align-items: center; flex-wrap: wrap; gap: 0.6rem;
    padding: 0.5rem 0.75rem;
    background: var(--bg-panel, #2a2a2a); color: var(--text, #eee);
    border-bottom: 1px solid var(--text-muted, #555);
    flex-shrink: 0;
  }
  .notation-toolbar label { display: flex; align-items: center; gap: 0.3rem; font-size: 0.8rem; white-space: nowrap; }
  .notation-toolbar input[type="number"] { width: 4.5rem; }
  .notation-title-field input { width: 12rem; }
  .notation-spacer { flex: 1; }
  .tolerance-slider input[type="range"] { width: 8rem; }
  .tolerance-value { font-size: 0.72rem; color: var(--text-muted, #aaa); min-width: 3rem; text-align: center; }
  .notation-staves-label { font-weight: 600; color: var(--text-muted, #aaa); }

  .notation-staves {
    display: flex; align-items: center; flex-wrap: wrap; gap: 0.5rem;
    padding: 0.4rem 0.75rem;
    background: var(--bg-elevated, #333); color: var(--text, #eee);
    border-bottom: 1px solid var(--text-muted, #555);
    font-size: 0.78rem; flex-shrink: 0;
  }
  .notation-staff-card {
    display: flex; align-items: center; gap: 0.45rem;
    padding: 0.2rem 0.45rem;
    border: 1px solid var(--text-muted, #555); border-radius: 6px;
    background: var(--bg-panel, #2a2a2a);
  }
  .notation-staff-name { width: 7.5rem; font-size: 0.78rem; }
  .notation-staff-div { display: flex; align-items: center; gap: 0.2rem; white-space: nowrap; cursor: pointer; }
  .notation-staff-bass { font-size: 1.1rem; line-height: 1; }
  .notation-staff-remove { border: none; background: none; color: #cc6666; font-size: 1rem; cursor: pointer; padding: 0 0.2rem; }

  .tab-inhoud {
    padding: 0.4rem 0.75rem; min-width: 0;
    background: var(--bg-elevated, #333); color: var(--text, #eee);
    border-bottom: 1px solid var(--text-muted, #555); flex-shrink: 0;
  }
  .step-caret {
    position: absolute; width: 2px; background: #2a6fdb;
    box-shadow: 0 0 0 1px rgba(42, 111, 219, 0.35);
    animation: caret-knipper 1s steps(2) infinite;
  }
  @keyframes caret-knipper { 50% { opacity: 0.25; } }
  .notation-error { padding: 0.5rem 0.75rem; background: #7a2020; color: #fff; font-size: 0.85rem; }
  .notation-busy { padding: 0.35rem 0.75rem; background: #f4ecd4; color: #6b5b1e; font-size: 0.8rem; }
  .notation-sheet { flex: 1; overflow-y: auto; padding: 1rem 1.5rem; background: #fff; position: relative; }
  .notation-sheet.clickable { cursor: pointer; }
  .notation-sheet.gum { cursor: not-allowed; }
  .lyric-editor {
    position: absolute; z-index: 6; width: 6rem; font-size: 0.85rem; padding: 0.1rem 0.25rem;
    border: 1px solid #2a6fdb; border-radius: 3px; background: #fff; color: #222;
  }
  .osmd-host { display: block; }
  .osmd-host.verborgen { visibility: hidden; position: absolute; inset: 0; overflow: hidden; }
  .view-switch { display: inline-flex; gap: 0.15rem; }
  .view-switch .btn.active {
    border-color: var(--gold-border, #d4af37);
    background: var(--bg-darkest, #e8e0d0);
    color: var(--primary, #b8960b);
    box-shadow: 0 0 0 1px var(--gold-border, #d4af37);
  }
  .layer-hand, .layer-split { font-size: 0.75rem; margin-left: 0.25rem; }
  /* Overlay ligt over de SVG maar vangt zelf geen klikken (die gaan naar de
     sheet-handler). Verankerd op de content-oorsprong (top/left:0, 0×0 met
     zichtbare overflow) zodat de markeringen — die in content-coördinaten staan —
     met de bladmuziek meescrollen i.p.v. aan de zichtbare rand te blijven kleven. */
  .notation-selection-overlay { position: absolute; top: 0; left: 0; width: 0; height: 0; overflow: visible; pointer-events: none; z-index: 5; }
  .note-highlight {
    position: absolute;
    background: rgba(80, 140, 255, 0.22);
    border: 1.5px solid rgba(40, 110, 240, 0.85);
    border-radius: 4px;
    box-sizing: border-box;
  }
  .notation-hint { padding: 0.4rem 0.75rem; font-size: 0.75rem; color: #666; background: #f4f4f0; border-top: 1px solid #ddd; flex-shrink: 0; }

  @media print {
    @page { size: A4 portrait; margin: 12mm; }
    /* Stemkleuren (0.7.86) alleen op het scherm: afdrukken in zwart. */
    .osmd-host svg [fill]:not([fill="none"]) { fill: #000 !important; }
    .osmd-host svg [stroke]:not([stroke="none"]) { stroke: #000 !important; }
    .notation-toolbar, .notation-staves, .notation-hint, .notation-busy, .notation-error, .notation-alpha,
    .tab-inhoud, .notation-selection-overlay, .osmd-host.verborgen {
      display: none !important;
    }
    .notation-window { height: auto; }
    .notation-sheet { overflow: visible; padding: 0; }
  }
</style>
