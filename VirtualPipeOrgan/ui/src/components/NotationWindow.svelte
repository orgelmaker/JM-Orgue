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
  import { onMount, onDestroy } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { OpenSheetMusicDisplay } from 'opensheetmusicdisplay';

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
  let lastGeneration = 0; // laatste gerenderde generation (dedup)

  // File-modus opties (blijven werken zoals in 0.7.0)
  let bpm = 90;
  let beatsPerBar = 4;
  let quantize = 4;
  let keyFifths = 0;
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
  $: selectedBoxes = noteBoxes.filter(b => selectionIds.has(b.eventId));

  function isPedalName(n) { const l = (n||'').toLowerCase(); return l.includes('pedaal')||l.includes('pedal'); }

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
    const beatDur = 60 / (Number(bpm) || 90);
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
  function previewTempo() {
    if (recording || armedWaiting) return;
    ensureMetroCtx();
    const ctx = metroAudioCtx;
    const beatDur = 60 / (Number(bpm) || 90);
    const beatsPer = Number(score?.beats_per_bar) || Number(beatsPerBar) || 4;
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

  const keyChoices = [
    { v: -7, label: 'Ces / as' }, { v: -6, label: 'Ges / es' }, { v: -5, label: 'Des / bes' },
    { v: -4, label: 'As / f' }, { v: -3, label: 'Es / c' }, { v: -2, label: 'Bes / g' },
    { v: -1, label: 'F / d' }, { v: 0, label: 'C / a' }, { v: 1, label: 'G / e' },
    { v: 2, label: 'D / b' }, { v: 3, label: 'A / fis' }, { v: 4, label: 'E / cis' },
    { v: 5, label: 'B / gis' }, { v: 6, label: 'Fis / dis' }, { v: 7, label: 'Cis / ais' },
  ];
  const gridChoices = [
    { v: 1, label: 'Kwartnoten' }, { v: 2, label: 'Achtsten' },
    { v: 4, label: 'Zestienden' }, { v: 8, label: '32e noten' },
  ];

  function titleFromPath(p) { return p ? p.split(/[\\/]/).pop().replace(/\.(mid|midi)$/i, '') : 'Opname'; }
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
        if (score.generation === lastGeneration && xml) return;
        lastGeneration = score.generation;
        xml = await invoke('notation_get_musicxml', { scoreId });
      } else {
        // File-modus: bestand → MusicXML (bestaande weg).
        const staves = (divisions.length > 0 && staffConfig.length > 0)
          ? staffConfig.map(s => ({ name: s.name || null, divisions: s.divisions, bass_clef: s.bass }))
          : null;
        xml = await invoke('convert_midi_to_musicxml', {
          path: filePath,
          options: {
            bpm: Number(bpm) || 90, beats_per_bar: Number(beatsPerBar) || 4,
            quantize: Number(quantize) || 4, key_fifths: Number(keyFifths) || 0,
            title, staves, tolerance_pct: Number(tolerancePct),
          },
        });
      }
      if (!osmd) {
        osmd = new OpenSheetMusicDisplay(container, {
          autoResize: true, backend: 'svg', drawTitle: true,
        });
      }
      if (xml) {
        await osmd.load(xml);
        osmd.render();
        if (isLive) buildNoteBoxes();
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
  // Lagen in dezelfde volgorde als build_musicxml de parts nummert: elke laag
  // met minstens één noot in een zichtbare take, op laag-volgorde. OSMD-balkindex
  // s hoort bij partLayers[s].
  function nonEmptyPartLayers() {
    return (score?.layers ?? []).filter(l =>
      l.takes.some(t => t.visible && t.events.length > 0));
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
                // Match: zelfde midi in deze laag, tijdstip binnen [start,end];
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
    if (!isLive || noteBoxes.length === 0) return;
    const rect = container.getBoundingClientRect();
    const px = e.clientX - rect.left + container.scrollLeft;
    const py = e.clientY - rect.top + container.scrollTop;
    let best = null, bestDist = Infinity;
    for (const b of noteBoxes) {
      // Binnen de bounding box telt als 0; anders afstand tot het centrum.
      const inside = px >= b.x && px <= b.x + b.w && py >= b.y && py <= b.y + b.h;
      const d = inside ? 0 : Math.hypot(px - b.cx, py - b.cy);
      if (d < bestDist) { bestDist = d; best = b; }
    }
    const threshold = 40; // px — ruim genoeg om net-naast te klikken
    if (!best || bestDist > threshold) return;
    if (e.shiftKey) {
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

  // ---- File-modus initieel: divisies + balk-indeling ----
  async function loadDivisions() {
    try {
      const info = await invoke('get_organ_info');
      divisions = (info?.divisions || []).map(d => d.name);
    } catch (e) { divisions = []; }
    if (divisions.length > 0 && staffConfig.length === 0) {
      staffConfig = divisions.map(name => ({ name, divisions: [name], bass: isPedalName(name) }));
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
    staffConfig = [...staffConfig, { name: `Balk ${staffConfig.length + 1}`, divisions: [], bass: false }];
  }
  function removeStaff(staffIdx) { staffConfig = staffConfig.filter((_, i) => i !== staffIdx); scheduleRender(); }
  function setStaffBass(staffIdx, v) { staffConfig[staffIdx].bass = v; staffConfig = staffConfig; scheduleRender(); }

  // ---- Wizard: balkindeling + divisie-routering (0.7.14) ----
  // Bij het openen van het live-venster: kies aantal balken, naam, sleutel en
  // welke divisies tijdens het inspelen naar welke balk routeren. Zolang de
  // partituur leeg is kan de indeling volledig vervangen worden; daarna zijn
  // alleen de divisie-chips per balk nog aanpasbaar (LayerBar).
  let wizard = null; // { staves: [{name, bass, divisions: []}] } of null
  let divEditLayerId = null; // LayerBar: balk waarvan de divisie-chips openstaan

  function openWizardFromScore() {
    wizard = {
      staves: (score?.layers ?? []).map(l => ({
        name: l.name,
        bass: l.bass_clef === true || (l.bass_clef == null && isPedalName(l.name)),
        divisions: [...(l.divisions || [])],
      })),
    };
    if (wizard.staves.length === 0) wizard.staves = [{ name: 'Balk 1', bass: false, divisions: [] }];
  }
  function wizardAddStaff() {
    wizard.staves = [...wizard.staves, { name: `Balk ${wizard.staves.length + 1}`, bass: false, divisions: [] }];
  }
  function wizardRemoveStaff(i) {
    wizard.staves = wizard.staves.filter((_, idx) => idx !== i);
  }
  function wizardToggleDivision(i, div) {
    const st = wizard.staves[i];
    st.divisions = st.divisions.includes(div) ? st.divisions.filter(d => d !== div) : [...st.divisions, div];
    wizard.staves = wizard.staves;
  }
  async function wizardApply() {
    if (!wizard || wizard.staves.length === 0) return;
    try {
      await invoke('notation_configure_layers', {
        scoreId,
        layers: wizard.staves.map(s => ({ name: s.name || 'Balk', bass_clef: !!s.bass, divisions: s.divisions })),
      });
      score = await invoke('notation_get_score', { scoreId });
      wizard = null;
      scheduleRender();
    } catch (e) { alert(String(e)); }
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

  // ---- Live-modus initieel + event-listeners ----
  let unlisteners = [];
  async function setupLive() {
    try {
      scoreId = await invoke('notation_new_score');
      score = await invoke('notation_get_score', { scoreId });
      syncMetronomeFromScore();
      await loadDivisions();      // divisienamen voor wizard + LayerBar-chips
      openWizardFromScore();      // balkindeling kiezen vóór het inspelen
      lastGeneration = 0;
      // Event-listeners voor live updates + edit-emits.
      const { listen } = await import('@tauri-apps/api/event');
      unlisteners.push(await listen('jm-orgue:notation:note-added', (e) => {
        if (e?.payload?.score !== scoreId) return;
        scheduleRender();
      }));
      unlisteners.push(await listen('jm-orgue:notation:score-changed', (e) => {
        if (e?.payload?.score !== scoreId) return;
        scheduleRender();
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
    } catch (e) { alert(`Opname-fout: ${e}`); }
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
        // De laatste openstaande notes hebben events geëmit; render zodat
        // ze zichtbaar worden.
        scheduleRender();
      } else {
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
    } catch (e) { alert(`Opname-fout: ${e}`); }
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
      const name = window.prompt('Naam van de nieuwe notenbalk?', `Balk ${(score?.layers?.length ?? 0) + 1}`);
      if (!name) return;
      await invoke('notation_add_layer', { scoreId, name, bassClef: null });
      score = await invoke('notation_get_score', { scoreId });
      scheduleRender();
    } catch (e) { error = String(e); }
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
        filters: [{ name: 'MIDI-bestand', extensions: ['mid', 'midi'] }],
      });
      if (!sel) return;
      await invoke('notation_import_midi', { scoreId, path: sel, intoTake: null });
      // score-changed-event zorgt voor render; score-snapshot verfrissen voor LayerBar.
      score = await invoke('notation_get_score', { scoreId });
    } catch (e) {
      alert(`MIDI importeren mislukt: ${e}`);
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
  async function doUndo() { try { await invoke('notation_undo', { scoreId }); } catch (e) { alert(String(e)); } }
  async function doRedo() { try { await invoke('notation_redo', { scoreId }); } catch (e) { alert(String(e)); } }

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
      midi: e.midi, channel: e.channel ?? 0,
      rel_start: e.start_us - minStart, dur: Math.max(1000, e.end_us - e.start_us),
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
      midi: e.midi, channel: e.channel ?? 0,
      start_us: Math.round(snapped + e.rel_start),
      end_us: Math.round(snapped + e.rel_start + e.dur),
    }));
    try { await invoke('notation_paste', { scoreId, layerId: targetLayerId, notes }); } catch (e) { alert(String(e)); }
  }

  function handleKey(e) {
    if (e.target && (e.target.tagName === 'INPUT' || e.target.tagName === 'TEXTAREA')) return;
    if (!isLive) return;
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
      const sel = await open({ multiple: false, filters: [{ name: 'MIDI-bestand', extensions: ['mid', 'midi'] }] });
      if (sel) { filePath = sel; title = titleFromPath(sel); scheduleRender(); }
    } catch (e) { error = String(e); }
  }
  async function saveMusicXml() {
    if (!xml) return;
    try {
      const { save } = await import('@tauri-apps/plugin-dialog');
      const suggested = (filePath ? filePath.replace(/\.(mid|midi)$/i, '') : (score?.title || 'notatie')) + '.musicxml';
      const path = await save({ defaultPath: suggested, filters: [{ name: 'MusicXML', extensions: ['musicxml', 'xml'] }] });
      if (path) { await invoke('save_musicxml', { path, xml }); alert(`MusicXML opgeslagen:\n${path}`); }
    } catch (e) { alert(`Opslaan mislukt: ${e}`); }
  }
  function printScore() { window.print(); }

  // OSMD's autoResize re-rendert bij vensterwijziging; de noot-boxen (in pixels)
  // moeten dan opnieuw worden opgemeten. Debounce zodat een sleep-resize niet
  // tientallen keren herberekent.
  let resizeTimer = null;
  function handleResize() {
    if (!isLive) return;
    if (resizeTimer) clearTimeout(resizeTimer);
    resizeTimer = setTimeout(() => { buildNoteBoxes(); }, 200);
  }

  onMount(async () => {
    window.addEventListener('keydown', handleKey);
    window.addEventListener('resize', handleResize);
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
    stopMetronome();
    if (metroAudioCtx) { try { metroAudioCtx.close(); } catch (e) {} metroAudioCtx = null; }
    for (const un of unlisteners) { try { un(); } catch (e) {} }
    // Live-opname veilig stoppen bij sluiten.
    if (recording && scoreId != null) invoke('notation_stop_recording').catch(() => {});
  });
</script>

<div class="notation-window">
  {#if isLive && wizard}
    <!-- Wizard: balkindeling + divisie-routering, vóór het inspelen. -->
    <div class="wizard-overlay">
      <div class="wizard-modal">
        <h3>Balkindeling</h3>
        <p class="wizard-hint">
          Kies hoeveel notenbalken je wilt, met welke sleutel, en welke divisies
          tijdens het inspelen naar welke balk gaan. Meerdere opnamen (takes) over
          elkaar per balk kan daarna via de LayerBar.
        </p>
        {#each wizard.staves as st, i}
          <div class="wizard-staff">
            <input class="wizard-staff-name" type="text" bind:value={st.name} title="Naam van de balk" />
            <select bind:value={st.bass} title="Sleutel">
              <option value={false}>𝄞 viool</option>
              <option value={true}>𝄢 bas</option>
            </select>
            <span class="wizard-divs">
              {#each divisions as div}
                <label class="wizard-div">
                  <input type="checkbox" checked={st.divisions.includes(div)} on:change={() => wizardToggleDivision(i, div)} />
                  {div}
                </label>
              {/each}
              {#if divisions.length === 0}<span class="wizard-hint">geen orgel geladen — routering kan later</span>{/if}
            </span>
            {#if wizard.staves.length > 1}
              <button class="wizard-remove" on:click={() => wizardRemoveStaff(i)} title="Balk verwijderen" aria-label="Balk verwijderen">×</button>
            {/if}
          </div>
        {/each}
        <div class="wizard-actions">
          <button class="btn btn-ghost btn-sm" on:click={wizardAddStaff}>+ Balk</button>
          <span class="notation-spacer"></span>
          {#if scoreHasEvents}
            <span class="wizard-hint">Er staan al noten — indeling vervangen kan alleen bij een lege partituur.</span>
            <button class="btn btn-secondary btn-sm" on:click={() => wizard = null}>Sluiten</button>
          {:else}
            <button class="btn btn-primary btn-sm" on:click={wizardApply}>Start met deze indeling</button>
          {/if}
        </div>
      </div>
    </div>
  {/if}
  <div class="notation-toolbar">
    {#if isLive}
      <button
        class="btn record-toggle"
        class:recording={recording || armedWaiting}
        on:click={toggleRecording}
        title={recording || armedWaiting ? 'Opname stoppen' : 'Opname starten — begint bij de eerste noot'}
      >
        <span class="record-dot" class:on={recording || armedWaiting}></span>
        {#if armedWaiting}Aftellen… {countInRemaining}{:else if recording}Stop{:else}Opname{/if}
      </button>
      <label>Tempo
        <input type="number" min="20" max="300" value={bpm} on:change={(e) => setBpmLive(e.currentTarget.value)} />
      </label>
      <button class="btn btn-ghost btn-sm" on:click={previewTempo} title="Eén maat voortikken om het tempo te horen (neemt niets op)">♪ Test</button>
      <label class="metro-toggle" title="Metronoom-tik in het notatievenster (aparte klik, geen orgelpijp)">
        <input type="checkbox" checked={metroOn} on:change={(e) => setMetronomeCfg(e.currentTarget.checked, countInBeats)} />
        Metronoom
      </label>
      <label title="Aantal tellen voortellen vóór de opname begint">Count-in
        <select value={countInBeats} on:change={(e) => setMetronomeCfg(metroOn, Number(e.currentTarget.value))}>
          <option value={0}>uit</option>
          <option value={1}>1</option>
          <option value={2}>2</option>
          <option value={4}>4</option>
        </select>
      </label>
      <label>Toonsoort
        <select value={keyFifths} on:change={(e) => setKeyLive(e.currentTarget.value)}>
          {#each keyChoices as k}<option value={k.v}>{k.label}</option>{/each}
        </select>
      </label>
      <label class="tolerance-slider" title="Sleep naar links voor letterlijk (los), naar rechts voor strak (hard kwantiseren)">
        Ritme
        <input type="range" min="0" max="100" step="5"
          bind:value={tolerancePct}
          on:change={(e) => setToleranceLive(Number(e.currentTarget.value))} />
        <span class="tolerance-value">{tolerancePct < 33 ? 'los' : tolerancePct > 66 ? 'strak' : 'midden'}</span>
      </label>
      <span class="notation-spacer"></span>
      <button class="btn btn-ghost btn-sm" on:click={() => moveCursor(-1)} title="Vorige noot (←)">◄</button>
      <button class="btn btn-ghost btn-sm" on:click={() => moveCursor(+1)} title="Volgende noot (→)">►</button>
      <button class="btn btn-ghost btn-sm" on:click={deleteSelection} title="Selectie verwijderen (Delete)">Delete</button>
      <button class="btn btn-ghost btn-sm" on:click={() => transposeSelection(-1)} title="Halve toon omlaag (↓)">−½</button>
      <button class="btn btn-ghost btn-sm" on:click={() => transposeSelection(+1)} title="Halve toon omhoog (↑)">+½</button>
      <button class="btn btn-ghost btn-sm" on:click={() => transposeSelection(-12)} title="Octaaf omlaag (Shift+↓)">−8va</button>
      <button class="btn btn-ghost btn-sm" on:click={() => transposeSelection(+12)} title="Octaaf omhoog (Shift+↑)">+8va</button>
      <button class="btn btn-ghost btn-sm" on:click={() => changeDuration('halve')} title="Duur halveren ([)">÷2</button>
      <button class="btn btn-ghost btn-sm" on:click={() => changeDuration('double')} title="Duur verdubbelen (])">×2</button>
      <button class="btn btn-ghost btn-sm" on:click={() => changeDuration('dot')} title="Punt aan/uit (.)">• punt</button>
      <button class="btn btn-ghost btn-sm" on:click={copySelection} title="Kopiëren (Ctrl+C)">Kopiëren</button>
      <button class="btn btn-ghost btn-sm" on:click={pasteClipboard} disabled={clipboardCount === 0} title="Plakken op de cursor (Ctrl+V)">Plakken{clipboardCount ? ` (${clipboardCount})` : ''}</button>
      <button class="btn btn-ghost btn-sm" on:click={doUndo} title="Ongedaan (Ctrl+Z)">↶</button>
      <button class="btn btn-ghost btn-sm" on:click={doRedo} title="Opnieuw (Ctrl+Y)">↷</button>
      <button class="btn btn-ghost btn-sm" on:click={openMidiFile} title="Bestaand MIDI-bestand als extra take(s) in deze partituur importeren">Openen…</button>
      <button class="btn btn-primary btn-sm" on:click={printScore} disabled={!xml}>Afdrukken / PDF</button>
      <button class="btn btn-ghost btn-sm" on:click={saveMusicXml} disabled={!xml}>Opslaan</button>
    {:else}
      <label>Tempo
        <input type="number" min="20" max="300" bind:value={bpm} on:change={scheduleRender} />
      </label>
      <label>Maat
        <select bind:value={beatsPerBar} on:change={scheduleRender}>
          <option value={2}>2/4</option><option value={3}>3/4</option>
          <option value={4}>4/4</option><option value={6}>6/4</option>
        </select>
      </label>
      <label>Raster
        <select bind:value={quantize} on:change={scheduleRender}>
          {#each gridChoices as g}<option value={g.v}>{g.label}</option>{/each}
        </select>
      </label>
      <label>Toonsoort
        <select bind:value={keyFifths} on:change={scheduleRender}>
          {#each keyChoices as k}<option value={k.v}>{k.label}</option>{/each}
        </select>
      </label>
      <label class="tolerance-slider" title="Sleep naar links voor letterlijk (los), naar rechts voor strak (hard kwantiseren)">
        Ritme
        <input type="range" min="0" max="100" step="5" bind:value={tolerancePct} on:change={scheduleRender} />
        <span class="tolerance-value">{tolerancePct < 33 ? 'los' : tolerancePct > 66 ? 'strak' : 'midden'}</span>
      </label>
      <label class="notation-title-field">Titel
        <input type="text" bind:value={title} on:change={scheduleRender} />
      </label>
      <span class="notation-spacer"></span>
      <button class="btn btn-ghost btn-sm" on:click={openOtherFile}>Openen…</button>
      <button class="btn btn-ghost btn-sm" on:click={saveMusicXml} disabled={!xml}>Opslaan als MusicXML</button>
      <button class="btn btn-primary btn-sm" on:click={printScore} disabled={!xml}>Afdrukken / PDF</button>
    {/if}
  </div>

  {#if isLive && score}
    <!-- LayerBar: per laag armed-bol + naam + Take-vinkjes + "+ Take". -->
    <div class="notation-layers">
      <span class="notation-staves-label">Balken:</span>
      {#each score.layers as layer}
        <div class="notation-layer">
          <button
            class="layer-arm"
            class:armed={score.armed_layer === layer.id}
            on:click={() => armLayer(layer.id)}
            title={score.armed_layer === layer.id ? 'Deze balk staat armed — opname schrijft hierheen' : 'Klik om deze balk armed te zetten'}
          >●</button>
          <span class="layer-name">{layer.name}</span>
          <span class="layer-takes">
            {#each layer.takes as take}
              <label class="take-chip" title="Take zichtbaar in notatie? Meerdere zichtbare takes samen = overdub op deze balk">
                <input
                  type="checkbox"
                  checked={take.visible}
                  on:change={(e) => toggleTakeVisible(layer.id, take.id, e.currentTarget.checked)}
                />
                {take.name}
                {#if layer.armed_take === take.id && score.armed_layer === layer.id && recording}
                  <span class="take-live">•REC</span>
                {/if}
              </label>
            {/each}
            <button class="btn btn-ghost btn-sm take-add" on:click={() => addTake(layer.id)} title="Nieuwe take (overdub)">+</button>
          </span>
          <!-- Divisie-routering van deze balk (klik om te wijzigen) -->
          <button
            class="layer-divs"
            on:click={() => divEditLayerId = divEditLayerId === layer.id ? null : layer.id}
            title="Welke divisies routeren tijdens het inspelen naar deze balk"
          >
            {(layer.divisions && layer.divisions.length) ? layer.divisions.join(' + ') : 'geen divisie'} ▾
          </button>
          {#if divEditLayerId === layer.id}
            <span class="layer-divs-edit">
              {#each divisions as div}
                <label class="wizard-div">
                  <input type="checkbox" checked={(layer.divisions || []).includes(div)} on:change={() => toggleLayerDivision(layer, div)} />
                  {div}
                </label>
              {/each}
            </span>
          {/if}
        </div>
      {/each}
      <button class="btn btn-ghost btn-sm layer-add" on:click={addLayer} title="Nieuwe notenbalk">+ Balk</button>
      <button class="btn btn-ghost btn-sm" on:click={openWizardFromScore} title="Balkindeling (wizard) openen">Indeling…</button>
    </div>

    <!-- Selectie/cursor-indicator; klik in de bladmuziek selecteert een noot. -->
    <div class="notation-cursor" class:counting={armedWaiting}>
      {#if armedWaiting}
        <span>Aftellen vóór opname… nog <b>{countInRemaining}</b> tel{countInRemaining === 1 ? '' : 'len'} — speel op tel 1.</span>
      {:else if flatEvents.length > 0}
        {#if selectionIds.size > 1}
          <span><b>{selectionIds.size}</b> noten geselecteerd — klik een noot (Shift+klik = meerdere), of pijltjes ←/→.</span>
        {:else if cursorEvent}
          <span>Selectie: <b>{noteName(cursorEvent.midi)}</b> (noot {cursorIndex + 1} van {flatEvents.length}, balk "{cursorEvent._layer}") — klik een noot om te kiezen.</span>
        {:else}
          <span>Klik een noot in de bladmuziek om te selecteren, of gebruik ←/→.</span>
        {/if}
      {:else}
        <span class="cursor-empty">Nog geen noten in de partituur — druk op Opname en speel.</span>
      {/if}
    </div>
  {/if}

  {#if !isLive && divisions.length > 0}
    <div class="notation-staves">
      <span class="notation-staves-label">Notenbalken:</span>
      {#each staffConfig as st, i}
        <div class="notation-staff-card">
          <input class="notation-staff-name" type="text" bind:value={st.name} on:change={scheduleRender} />
          {#each divisions as div}
            <label class="notation-staff-div">
              <input type="checkbox" checked={st.divisions.includes(div)} on:change={() => toggleStaffDivision(i, div)} />
              {div}
            </label>
          {/each}
          <label class="notation-staff-div notation-staff-bass" title="Bassleutel">
            <input type="checkbox" checked={st.bass} on:change={(e) => setStaffBass(i, e.currentTarget.checked)} />𝄢
          </label>
          {#if staffConfig.length > 1}
            <button class="notation-staff-remove" on:click={() => removeStaff(i)} aria-label="Balk verwijderen">×</button>
          {/if}
        </div>
      {/each}
      <button class="btn btn-ghost btn-sm" on:click={addStaff}>+ Balk</button>
    </div>
  {/if}

  {#if error}<div class="notation-error">{error}</div>{/if}
  {#if converting}<div class="notation-busy">Bezig met noteren…</div>{/if}

  <div class="notation-sheet" class:clickable={isLive} bind:this={container} on:click={handleSheetClick}>
    {#if isLive}
      <!-- Selectie-overlay: absolute-position markering per geselecteerde noot
           (OSMD zelf blijft ongemuteerd; selectie wijzigen vergt geen re-render). -->
      <div class="notation-selection-overlay">
        {#each selectedBoxes as b (b.eventId + '@' + b.x + ',' + b.y)}
          <div class="note-highlight"
            style="left:{b.x - 3}px; top:{b.y - 3}px; width:{b.w + 6}px; height:{b.h + 6}px;"></div>
        {/each}
      </div>
    {/if}
  </div>

  <div class="notation-hint">
    {#if isLive}
      Tip: druk op Opname en begin te spelen — de eerste noot is beat 1. Meerdere takes op dezelfde balk = overdub (vink ze in de LayerBar aan/uit). Klik een noot om te selecteren (Shift+klik = meerdere); ←/→ selecteren, ↑/↓ transponeren, Delete verwijdert. Duur: [ halveren, ] verdubbelen, . punt. Ctrl+C/Ctrl+V kopiëren/plakken (op de cursor), Ctrl+Z ongedaan.
    {:else}
      Tip: vrij ingespeeld? Stel het tempo in waarop je speelde en schuif "Ritme" naar los als de kwantisatie te strak aanvoelt.
    {/if}
  </div>
</div>

<style>
  .notation-window { display: flex; flex-direction: column; height: 100vh; background: #fff; color: #222; }
  .notation-toolbar {
    display: flex; align-items: center; flex-wrap: wrap; gap: 0.6rem;
    padding: 0.5rem 0.75rem;
    background: var(--bg-panel, #2a2a2a); color: var(--text, #eee);
    border-bottom: 1px solid var(--accent-soft, #555);
    flex-shrink: 0;
  }
  .notation-toolbar label { display: flex; align-items: center; gap: 0.3rem; font-size: 0.8rem; white-space: nowrap; }
  .notation-toolbar input[type="number"] { width: 4.5rem; }
  .notation-title-field input { width: 12rem; }
  .notation-spacer { flex: 1; }
  .tolerance-slider input[type="range"] { width: 8rem; }
  .tolerance-value { font-size: 0.72rem; color: var(--text-muted, #aaa); min-width: 3rem; text-align: center; }

  .record-toggle {
    display: inline-flex; align-items: center; gap: 0.35rem;
    padding: 0.25rem 0.7rem; border: 1px solid #cc4444; border-radius: 6px;
    background: transparent; color: #cc6666; font-weight: 600; cursor: pointer;
  }
  .record-toggle.recording { background: #7a2020; color: #fff; }
  .record-dot {
    width: 0.7rem; height: 0.7rem; border-radius: 50%; background: #cc4444; opacity: 0.5;
  }
  .record-dot.on { background: #ff4040; opacity: 1; animation: rec-pulse 1s infinite; }
  @keyframes rec-pulse { 50% { opacity: 0.4; } }

  .notation-layers {
    display: flex; align-items: center; flex-wrap: wrap; gap: 0.5rem;
    padding: 0.4rem 0.75rem;
    background: var(--bg-elevated, #333); color: var(--text, #eee);
    border-bottom: 1px solid var(--accent-soft, #555);
    font-size: 0.78rem; flex-shrink: 0;
  }
  .notation-staves-label { font-weight: 600; color: var(--text-muted, #aaa); }
  .notation-layer {
    display: flex; align-items: center; gap: 0.45rem;
    padding: 0.2rem 0.5rem;
    border: 1px solid var(--accent-soft, #555); border-radius: 6px;
    background: var(--bg-panel, #2a2a2a);
  }
  .layer-arm {
    width: 1.1rem; height: 1.1rem; border-radius: 50%; border: 1px solid #666;
    background: transparent; color: #666; cursor: pointer; font-size: 0.9rem;
    display: inline-flex; align-items: center; justify-content: center;
    padding: 0;
  }
  .layer-arm.armed { background: #cc3030; color: #fff; border-color: #ff5050; }
  .layer-name { font-weight: 600; }
  .layer-takes { display: flex; align-items: center; gap: 0.35rem; }
  .take-chip { display: flex; align-items: center; gap: 0.2rem; }
  .take-live { color: #ff5050; font-weight: 700; font-size: 0.7rem; margin-left: 0.2rem; }
  .take-add { padding: 0 0.35rem; }
  .layer-add { margin-left: auto; }

  .notation-cursor {
    padding: 0.3rem 0.75rem;
    background: #f4ecd4; color: #6b5b1e;
    font-size: 0.78rem; border-bottom: 1px solid #ddd; flex-shrink: 0;
  }
  .notation-cursor.counting { background: #7a2020; color: #fff; font-weight: 600; }
  .cursor-empty { color: #999; font-style: italic; }
  .metro-toggle { cursor: pointer; }

  .notation-staves {
    display: flex; align-items: center; flex-wrap: wrap; gap: 0.5rem;
    padding: 0.4rem 0.75rem;
    background: var(--bg-elevated, #333); color: var(--text, #eee);
    border-bottom: 1px solid var(--accent-soft, #555);
    font-size: 0.78rem; flex-shrink: 0;
  }
  .notation-staff-card {
    display: flex; align-items: center; gap: 0.45rem;
    padding: 0.2rem 0.45rem;
    border: 1px solid var(--accent-soft, #555); border-radius: 6px;
    background: var(--bg-panel, #2a2a2a);
  }
  .notation-staff-name { width: 7.5rem; font-size: 0.78rem; }
  .notation-staff-div { display: flex; align-items: center; gap: 0.2rem; white-space: nowrap; cursor: pointer; }
  .notation-staff-bass { font-size: 1.1rem; line-height: 1; }
  .notation-staff-remove { border: none; background: none; color: #cc6666; font-size: 1rem; cursor: pointer; padding: 0 0.2rem; }

  /* Wizard: balkindeling + divisie-routering */
  .wizard-overlay {
    position: fixed; inset: 0; z-index: 50;
    background: rgba(0, 0, 0, 0.55);
    display: flex; align-items: center; justify-content: center;
  }
  .wizard-modal {
    background: var(--bg-panel, #2a2a2a); color: var(--text, #eee);
    border: 1px solid var(--accent-soft, #555); border-radius: 8px;
    padding: 1rem 1.2rem; max-width: 46rem; width: calc(100% - 3rem);
    max-height: 80vh; overflow-y: auto;
  }
  .wizard-modal h3 { margin: 0 0 0.4rem; }
  .wizard-hint { font-size: 0.78rem; color: var(--text-muted, #aaa); margin: 0 0 0.6rem; }
  .wizard-staff {
    display: flex; align-items: center; flex-wrap: wrap; gap: 0.5rem;
    padding: 0.35rem 0.5rem; margin-bottom: 0.4rem;
    border: 1px solid var(--accent-soft, #555); border-radius: 6px;
  }
  .wizard-staff-name { width: 9rem; }
  .wizard-divs { display: flex; align-items: center; flex-wrap: wrap; gap: 0.5rem; }
  .wizard-div { display: flex; align-items: center; gap: 0.25rem; font-size: 0.8rem; white-space: nowrap; cursor: pointer; }
  .wizard-remove { margin-left: auto; border: none; background: none; color: #cc6666; font-size: 1.1rem; cursor: pointer; }
  .wizard-actions { display: flex; align-items: center; gap: 0.5rem; margin-top: 0.6rem; }
  .layer-divs {
    border: 1px dashed var(--accent-soft, #666); border-radius: 5px;
    background: transparent; color: var(--text-muted, #bbb);
    font-size: 0.72rem; padding: 0.1rem 0.4rem; cursor: pointer;
  }
  .layer-divs-edit {
    display: flex; align-items: center; flex-wrap: wrap; gap: 0.45rem;
    padding: 0.15rem 0.4rem;
    border: 1px solid var(--accent-soft, #555); border-radius: 5px;
    background: var(--bg-elevated, #333);
  }

  .notation-error { padding: 0.5rem 0.75rem; background: #7a2020; color: #fff; font-size: 0.85rem; }
  .notation-busy { padding: 0.35rem 0.75rem; background: #f4ecd4; color: #6b5b1e; font-size: 0.8rem; }
  .notation-sheet { flex: 1; overflow-y: auto; padding: 1rem 1.5rem; background: #fff; position: relative; }
  .notation-sheet.clickable { cursor: pointer; }
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
    .notation-toolbar, .notation-layers, .notation-cursor, .notation-staves, .notation-hint, .notation-busy, .notation-error {
      display: none !important;
    }
    .notation-window { height: auto; }
    .notation-sheet { overflow: visible; padding: 0; }
  }
</style>
