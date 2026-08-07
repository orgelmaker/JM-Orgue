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

  function isPedalName(n) { const l = (n||'').toLowerCase(); return l.includes('pedaal')||l.includes('pedal'); }

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
      }
      error = null;
    } catch (e) {
      error = String(e);
    }
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

  // ---- Live-modus initieel + event-listeners ----
  let unlisteners = [];
  async function setupLive() {
    try {
      scoreId = await invoke('notation_new_score');
      score = await invoke('notation_get_score', { scoreId });
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

  async function toggleRecording() {
    if (!scoreId) return;
    try {
      if (recording) {
        await invoke('notation_stop_recording');
        recording = false;
        // De laatste openstaande notes hebben events geëmit; render zodat
        // ze zichtbaar worden.
        scheduleRender();
      } else {
        // Zorg dat er een armed laag + take is; anders eerste laag armen.
        if (!score) score = await invoke('notation_get_score', { scoreId });
        if (!score.armed_layer && score.layers.length > 0) {
          await invoke('notation_arm_layer', { scoreId, layerId: score.layers[0].id });
        }
        await invoke('notation_start_recording', { scoreId });
        recording = true;
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
    l.takes.filter(t => t.visible).flatMap(t => t.events.map(e => ({ ...e, _layer: l.name })))
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
    else if ((e.ctrlKey || e.metaKey) && !e.shiftKey && e.key.toLowerCase() === 'z') { doUndo(); e.preventDefault(); }
    else if ((e.ctrlKey || e.metaKey) && (e.shiftKey && e.key.toLowerCase() === 'z' || e.key.toLowerCase() === 'y')) { doRedo(); e.preventDefault(); }
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

  onMount(async () => {
    window.addEventListener('keydown', handleKey);
    if (isLive) {
      await setupLive();
    } else {
      await loadDivisions();
      await doRender();
    }
  });
  onDestroy(() => {
    window.removeEventListener('keydown', handleKey);
    if (renderDebounceTimer) clearTimeout(renderDebounceTimer);
    if (renderCeilingTimer) clearTimeout(renderCeilingTimer);
    for (const un of unlisteners) { try { un(); } catch (e) {} }
    // Live-opname veilig stoppen bij sluiten.
    if (recording && scoreId != null) invoke('notation_stop_recording').catch(() => {});
  });
</script>

<div class="notation-window">
  <div class="notation-toolbar">
    {#if isLive}
      <button
        class="btn record-toggle"
        class:recording
        on:click={toggleRecording}
        title={recording ? 'Opname stoppen' : 'Opname starten — begint bij de eerste noot'}
      >
        <span class="record-dot" class:on={recording}></span>
        {recording ? 'Stop' : 'Opname'}
      </button>
      <label>Tempo
        <input type="number" min="20" max="300" value={bpm} on:change={(e) => setBpmLive(e.currentTarget.value)} />
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
        </div>
      {/each}
      <button class="btn btn-ghost btn-sm layer-add" on:click={addLayer} title="Nieuwe notenbalk">+ Balk</button>
    </div>

    <!-- Selectie/cursor-indicator (visuele SVG-highlight komt in fase C+). -->
    <div class="notation-cursor">
      {#if flatEvents.length > 0 && cursorEvent}
        <span>Selectie: <b>{noteName(cursorEvent.midi)}</b> (noot {cursorIndex + 1} van {flatEvents.length}, balk "{cursorEvent._layer}")</span>
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

  <div class="notation-sheet" bind:this={container}></div>

  <div class="notation-hint">
    {#if isLive}
      Tip: druk op Opname en begin te spelen — de eerste noot is beat 1. Meerdere takes op dezelfde balk = overdub (vink ze in de LayerBar aan/uit). Pijltjes ←/→ selecteren noten; ↑/↓ transponeren, Delete verwijdert, Ctrl+Z ongedaan.
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
  .cursor-empty { color: #999; font-style: italic; }

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

  .notation-error { padding: 0.5rem 0.75rem; background: #7a2020; color: #fff; font-size: 0.85rem; }
  .notation-busy { padding: 0.35rem 0.75rem; background: #f4ecd4; color: #6b5b1e; font-size: 0.8rem; }
  .notation-sheet { flex: 1; overflow-y: auto; padding: 1rem 1.5rem; background: #fff; }
  .notation-hint { padding: 0.4rem 0.75rem; font-size: 0.75rem; color: #666; background: #f4f4f0; border-top: 1px solid #ddd; flex-shrink: 0; }

  @media print {
    .notation-toolbar, .notation-layers, .notation-cursor, .notation-staves, .notation-hint, .notation-busy, .notation-error {
      display: none !important;
    }
    .notation-window { height: auto; }
    .notation-sheet { overflow: visible; padding: 0; }
  }
</style>
