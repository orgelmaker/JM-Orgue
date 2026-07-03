<script>
  import { invoke } from '@tauri-apps/api/core';
  import { onMount, createEventDispatcher } from 'svelte';
  import { t } from '../lib/i18n.js';

  export let organ = null;

  const dispatch = createEventDispatcher();

  const NOTE_NAMES = ['C', 'C#', 'D', 'D#', 'E', 'F', 'F#', 'G', 'G#', 'A', 'A#', 'B'];

  function midiToName(m) {
    const oct = Math.floor(m / 12) - 1;
    return `${NOTE_NAMES[m % 12]}${oct}`;
  }

  let selectedDivision = '';
  let selectedStop = null;       // StopDto-achtig object
  let selectedPipeMidi = null;   // gekozen MIDI noot binnen stop
  let voicings = {};             // key "stop_id-pipe_num" -> {volume_db, pitch_cents}
  let liveVolume = 0;
  let livePitch = 0;
  let saveTimer = null;

  $: divisions = organ?.divisions ?? [];
  $: stops = (organ?.divisions ?? [])
        .filter(d => !selectedDivision || d.name === selectedDivision)
        .flatMap(d => (d.stops || []).map(s => ({ ...s, _division: d.name })));

  $: if (selectedStop) {
    const key = `${selectedStop.internal_stop_id}-${selectedPipeMidi}`;
    const v = voicings[key];
    liveVolume = v ? v.volume_db : 0;
    livePitch = v ? v.pitch_cents : 0;
  }

  onMount(async () => {
    await loadVoicings();
  });

  // Herlaad de voicings zodra een (ander) orgel beschikbaar komt — anders blijft
  // de map leeg of hoort hij bij het vorige orgel.
  let voicingsLoadedForOrgan = null;
  $: if (organ && (organ.id || organ.name) !== voicingsLoadedForOrgan) {
    voicingsLoadedForOrgan = organ.id || organ.name;
    loadVoicings();
  }

  async function loadVoicings() {
    try {
      const list = await invoke('get_pipe_voicings');
      // De backend levert pipe_num als 0-based offset t.o.v. first_midi_note van
      // de stop (zo wordt het ook opgeslagen via set_pipe_voicing). De UI keyt op
      // absolute MIDI-noot — reken hier om, anders zijn opgeslagen voicings na
      // herladen onzichtbaar in dit paneel.
      const firstNoteByStop = {};
      for (const d of (organ?.divisions ?? [])) {
        for (const s of (d.stops ?? [])) {
          if (s.internal_stop_id != null && s.first_midi_note != null) {
            firstNoteByStop[s.internal_stop_id] = s.first_midi_note;
          }
        }
      }
      voicings = {};
      for (const [stop_id, pipe_num, volume_db, pitch_cents] of list) {
        const first = firstNoteByStop[stop_id];
        if (first == null) continue; // stop onbekend in dit orgel
        voicings[`${stop_id}-${first + pipe_num}`] = { volume_db, pitch_cents };
      }
      voicings = voicings;
    } catch (e) {
      console.warn('Kon voicings niet laden:', e);
    }
  }

  function selectStop(stop) {
    selectedStop = stop;
    if (stop && stop.first_midi_note != null) {
      selectedPipeMidi = stop.first_midi_note;
    }
  }

  function applyLive() {
    if (!selectedStop || selectedPipeMidi == null) return;
    const stop_id = selectedStop.internal_stop_id;
    const pipe_num = selectedPipeMidi - selectedStop.first_midi_note;

    const key = `${stop_id}-${selectedPipeMidi}`;
    if (liveVolume === 0 && livePitch === 0) {
      delete voicings[key];
    } else {
      voicings[key] = { volume_db: liveVolume, pitch_cents: livePitch };
    }
    voicings = voicings;

    invoke('set_pipe_voicing', {
      stopId: stop_id, pipeNum: pipe_num,
      volumeDb: liveVolume, pitchCents: livePitch,
    }).catch(e => console.warn('set_pipe_voicing failed:', e));

    queueSave();
  }

  function queueSave() {
    if (saveTimer) clearTimeout(saveTimer);
    saveTimer = setTimeout(() => {
      dispatch('persistSettings');
      saveTimer = null;
    }, 800);
  }

  async function resetCurrentPipe() {
    if (!selectedStop || selectedPipeMidi == null) return;
    const pipe_num = selectedPipeMidi - selectedStop.first_midi_note;
    liveVolume = 0;
    livePitch = 0;
    delete voicings[`${selectedStop.internal_stop_id}-${selectedPipeMidi}`];
    voicings = voicings;
    try {
      await invoke('reset_pipe_voicing', {
        stopId: selectedStop.internal_stop_id, pipeNum: pipe_num,
      });
    } catch (e) { console.warn(e); }
    queueSave();
  }

  async function resetCurrentStop() {
    if (!selectedStop) return;
    const stop_id = selectedStop.internal_stop_id;
    for (const k of Object.keys(voicings)) {
      if (k.startsWith(`${stop_id}-`)) delete voicings[k];
    }
    voicings = voicings;
    liveVolume = 0;
    livePitch = 0;
    try {
      await invoke('reset_stop_voicing', { stopId: stop_id });
    } catch (e) { console.warn(e); }
    queueSave();
  }

  function pipeHasVoicing(stop, midi) {
    return !!voicings[`${stop.internal_stop_id}-${midi}`];
  }

  function stopHasAnyVoicing(stop) {
    if (!stop) return false;
    return Object.keys(voicings).some(k => k.startsWith(`${stop.internal_stop_id}-`));
  }

  $: pipeRange = selectedStop
    ? Array.from(
        { length: (selectedStop.last_midi_note - selectedStop.first_midi_note + 1) },
        (_, i) => selectedStop.first_midi_note + i
      )
    : [];
</script>

<div class="voicing-panel">
  <div class="voicing-header">
    <h3>{$t('voicing.title')}</h3>
    <p>{$t('voicing.subtitle')}</p>
  </div>

  {#if !organ}
    <div class="empty-msg">{$t('voicing.no_organ')}</div>
  {:else}
    <div class="voicing-grid">
      <!-- Divisie filter -->
      <div class="filter-row">
        <label>{$t('voicing.division')}</label>
        <select bind:value={selectedDivision}>
          <option value="">{$t('voicing.all')}</option>
          {#each divisions as div}
            <option value={div.name}>{div.name}</option>
          {/each}
        </select>
      </div>

      <!-- Stop kiezer -->
      <div class="stop-list">
        <div class="list-title">{$t('voicing.stops_count').replace('{count}', stops.length)}</div>
        <div class="stop-list-scroll">
          {#each stops as stop}
            <button
              type="button"
              class="stop-item"
              class:selected={selectedStop && selectedStop.id === stop.id}
              class:has-edits={stopHasAnyVoicing(stop)}
              on:click={() => selectStop(stop)}
            >
              <span class="stop-item-div">{stop._division}</span>
              <span class="stop-item-name">{stop.name}</span>
              <span class="stop-item-pitch">{stop.pitch}</span>
              {#if stopHasAnyVoicing(stop)}
                <span class="edit-dot" title="Heeft voicing-aanpassingen">●</span>
              {/if}
            </button>
          {/each}
        </div>
      </div>

      <!-- Voicing editor -->
      <div class="editor">
        {#if selectedStop}
          <div class="editor-title">
            {selectedStop._division} · {selectedStop.name} {selectedStop.pitch}
          </div>

          <!-- Pijp keuze -->
          <div class="pipe-strip">
            {#each pipeRange as midi}
              <button
                type="button"
                class="pipe-btn"
                class:selected={midi === selectedPipeMidi}
                class:has-voicing={pipeHasVoicing(selectedStop, midi)}
                on:click={() => selectedPipeMidi = midi}
                title={midiToName(midi)}
              >
                {midiToName(midi)}
              </button>
            {/each}
          </div>

          <!-- Volume slider -->
          <div class="slider-row">
            <span class="slider-label">{$t('voicing.volume')}</span>
            <input
              type="range"
              min="-12" max="12" step="0.1"
              bind:value={liveVolume}
              on:input={applyLive}
            />
            <span class="slider-value">{liveVolume.toFixed(1)} dB</span>
          </div>

          <!-- Pitch slider -->
          <div class="slider-row">
            <span class="slider-label">{$t('voicing.pitch')}</span>
            <input
              type="range"
              min="-50" max="50" step="0.5"
              bind:value={livePitch}
              on:input={applyLive}
            />
            <span class="slider-value">{livePitch >= 0 ? '+' : ''}{livePitch.toFixed(1)} ¢</span>
          </div>

          <div class="actions">
            <button class="btn btn-ghost btn-sm" on:click={resetCurrentPipe}>
              {$t('voicing.reset_pipe')}
            </button>
            <button class="btn btn-secondary btn-sm" on:click={resetCurrentStop}>
              {$t('voicing.reset_stop')}
            </button>
          </div>

          <p class="hint">{$t('voicing.hint_save').replace('{file}', '.jm-settings.json')}</p>

        {:else}
          <div class="empty-msg">{$t('voicing.choose_register')}</div>
        {/if}
      </div>
    </div>
  {/if}
</div>

<style>
  .voicing-panel {
    padding: 1.5rem;
    max-width: 1100px;
    margin: 0 auto;
  }

  .voicing-header {
    margin-bottom: 1rem;
  }

  .voicing-header h3 {
    font-size: 1.1rem;
    font-weight: 600;
    color: var(--text);
    margin: 0 0 0.25rem;
  }

  .voicing-header p {
    font-size: 0.85rem;
    color: var(--text-secondary);
    margin: 0;
  }

  .empty-msg {
    padding: 2rem;
    text-align: center;
    color: var(--text-muted);
    font-size: 0.9rem;
  }

  .voicing-grid {
    display: grid;
    grid-template-columns: 280px 1fr;
    grid-template-rows: auto 1fr;
    gap: 1rem;
  }

  .filter-row {
    grid-column: 1 / 2;
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.5rem 0.75rem;
    background: var(--bg-panel);
    border-radius: var(--radius-md);
    border: var(--border-subtle);
  }

  .filter-row label {
    font-size: 0.8rem;
    color: var(--text-secondary);
    font-weight: 500;
  }

  .filter-row select {
    flex: 1;
    padding: 0.3rem 0.5rem;
    background: var(--bg-elevated);
    border: 1px solid var(--accent-soft-2);
    border-radius: var(--radius-sm);
    color: var(--text);
    font-size: 0.8rem;
    cursor: pointer;
  }

  .stop-list {
    grid-row: 2 / 3;
    display: flex;
    flex-direction: column;
    background: var(--bg-panel);
    border-radius: var(--radius-md);
    border: var(--border-subtle);
    overflow: hidden;
    min-height: 0;
  }

  .list-title {
    padding: 0.5rem 0.75rem;
    font-size: 0.7rem;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--text-muted);
    font-weight: 600;
    border-bottom: var(--border-subtle);
    background: var(--bg-elevated);
  }

  .stop-list-scroll {
    flex: 1;
    overflow-y: auto;
    max-height: 60vh;
  }

  .stop-item {
    display: grid;
    grid-template-columns: 84px minmax(0, 1fr) auto auto;
    gap: 0.5rem;
    align-items: center;
    width: 100%;
    padding: 0.5rem 0.75rem;
    border: none;
    background: transparent;
    color: var(--text);
    text-align: left;
    cursor: pointer;
    font-size: 0.78rem;
    border-bottom: 1px solid var(--accent-soft);
    transition: background var(--transition-fast);
  }

  .stop-item:hover {
    background: var(--accent-soft-hover);
  }

  .stop-item.selected {
    background: var(--accent-soft-2);
    font-weight: 600;
  }

  .stop-item-div {
    font-size: 0.65rem;
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.05em;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .stop-item-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .stop-item-pitch {
    font-family: 'JetBrains Mono', monospace;
    font-size: 0.7rem;
    color: var(--text-muted);
  }

  .edit-dot {
    color: var(--primary);
    font-size: 0.6rem;
  }

  .editor {
    grid-column: 2 / 3;
    grid-row: 1 / 3;
    display: flex;
    flex-direction: column;
    gap: 1rem;
    padding: 1rem 1.25rem;
    background: var(--bg-panel);
    border-radius: var(--radius-md);
    border: var(--border-subtle);
  }

  .editor-title {
    font-size: 0.95rem;
    font-weight: 600;
    color: var(--text);
    padding-bottom: 0.5rem;
    border-bottom: var(--border-subtle);
    font-family: var(--stop-font-family);
  }

  .pipe-strip {
    display: flex;
    flex-wrap: wrap;
    gap: 3px;
    max-height: 180px;
    overflow-y: auto;
    padding: 0.4rem;
    background: var(--bg-elevated);
    border-radius: var(--radius-sm);
  }

  .pipe-btn {
    min-width: 48px;
    padding: 0.3rem 0.4rem;
    background: var(--bg-darkest);
    border: 1px solid var(--accent-soft-2);
    border-radius: var(--radius-sm);
    color: var(--text);
    font-family: 'JetBrains Mono', monospace;
    font-size: 0.7rem;
    cursor: pointer;
    transition: all var(--transition-fast);
    position: relative;
  }

  .pipe-btn:hover {
    border-color: var(--gold-border);
  }

  .pipe-btn.selected {
    background: var(--primary);
    color: var(--text-on-primary);
    border-color: var(--primary-hover);
    font-weight: 700;
  }

  .pipe-btn.has-voicing::after {
    content: '';
    position: absolute;
    top: 3px;
    right: 3px;
    width: 5px;
    height: 5px;
    background: var(--stop-active);
    border-radius: 50%;
  }

  .slider-row {
    display: grid;
    grid-template-columns: 90px 1fr 80px;
    align-items: center;
    gap: 0.75rem;
  }

  .slider-label {
    font-size: 0.85rem;
    color: var(--text);
    font-weight: 500;
  }

  .slider-row input[type="range"] {
    width: 100%;
  }

  .slider-value {
    font-family: 'JetBrains Mono', monospace;
    font-size: 0.85rem;
    color: var(--text);
    text-align: right;
  }

  .actions {
    display: flex;
    gap: 0.5rem;
    margin-top: 0.5rem;
  }

  .hint {
    margin: 0.5rem 0 0;
    font-size: 0.72rem;
    color: var(--text-muted);
    line-height: 1.4;
  }

  .hint code {
    background: var(--bg-elevated);
    padding: 1px 4px;
    border-radius: 3px;
    font-family: 'JetBrains Mono', monospace;
    font-size: 0.7rem;
  }

  /* Touchscreen */
  @media (pointer: coarse) {
    .pipe-btn {
      min-width: 56px;
      min-height: 40px;
      font-size: 0.78rem;
    }
    .stop-item {
      padding: 0.7rem 0.75rem;
      font-size: 0.85rem;
    }
    .slider-row input[type="range"] {
      height: 26px;
    }
  }
</style>
