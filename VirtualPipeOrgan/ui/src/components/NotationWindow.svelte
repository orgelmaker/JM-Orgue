<script>
  // Notatievenster: toont een MIDI-opname als notenschrift (bladmuziek).
  // De omzetting MIDI → MusicXML gebeurt in de Rust-backend
  // (convert_midi_to_musicxml); het renderen doet OpenSheetMusicDisplay
  // (BSD-3, gebundeld — geen externe software of internet nodig).
  // Raster/tempo/maatsoort/toonsoort/titel zijn achteraf bij te stellen;
  // elke wijziging converteert opnieuw. Afdrukken gaat via de webview-
  // printdialoog ("Microsoft Print to PDF" = ook PDF-opslag).
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { OpenSheetMusicDisplay } from 'opensheetmusicdisplay';

  // MIDI-bestand uit de URL-hash (#notation&file=<encodeURIComponent(pad)>).
  let filePath = (() => {
    const m = window.location.hash.match(/[&?]file=([^&]+)/);
    return m ? decodeURIComponent(m[1]) : null;
  })();

  let container;
  let osmd = null;
  let converting = false;
  let error = null;
  let xml = null;

  // Notatie-opties (zie backend NotationOptions).
  let bpm = 90;
  let beatsPerBar = 4;
  let quantize = 4; // rastereenheden per kwartnoot: 2=8e, 4=16e, 8=32e
  let keyFifths = 0;
  let title = '';

  // Vrij instelbare balk-indeling: per notenbalk vink je aan welke divisies
  // erop komen (meerdere per balk mag; zelfde divisie op meerdere balken mag).
  let divisions = [];      // divisienamen van het geladen orgel
  let staffConfig = [];    // [{ name, divisions: [naam,...], bass: bool }]

  function isPedalName(n) {
    const l = (n || '').toLowerCase();
    return l.includes('pedaal') || l.includes('pedal');
  }

  async function loadDivisions() {
    try {
      const info = await invoke('get_organ_info');
      divisions = (info?.divisions || []).map(d => d.name);
    } catch (e) {
      divisions = [];
    }
    // Standaard-indeling: één balk per divisie (zoals de automatische modus).
    if (divisions.length > 0 && staffConfig.length === 0) {
      staffConfig = divisions.map(name => ({
        name,
        divisions: [name],
        bass: isPedalName(name),
      }));
    }
  }

  function toggleStaffDivision(staffIdx, divName) {
    const st = staffConfig[staffIdx];
    if (st.divisions.includes(divName)) {
      st.divisions = st.divisions.filter(d => d !== divName);
    } else {
      st.divisions = [...st.divisions, divName];
    }
    staffConfig = staffConfig;
    convert();
  }

  function addStaff() {
    staffConfig = [...staffConfig, { name: `Balk ${staffConfig.length + 1}`, divisions: [], bass: false }];
    // Nog geen divisies aangevinkt → renderen verandert pas na aanvinken.
  }

  function removeStaff(staffIdx) {
    staffConfig = staffConfig.filter((_, i) => i !== staffIdx);
    convert();
  }

  function setStaffBass(staffIdx, v) {
    staffConfig[staffIdx].bass = v;
    staffConfig = staffConfig;
    convert();
  }

  function titleFromPath(p) {
    return p ? p.split(/[\\/]/).pop().replace(/\.(mid|midi)$/i, '') : 'Opname';
  }
  title = titleFromPath(filePath);

  // Toonsoorten als kwintenaantal (majeur / mineur-weergave).
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

  async function convert() {
    if (!filePath) { error = 'Geen MIDI-bestand opgegeven. Kies een bestand via "Openen…".'; return; }
    converting = true;
    error = null;
    try {
      // Balk-indeling meesturen zodra er divisies bekend zijn; balken zonder
      // aangevinkte divisies blijven leeg en worden niet gerenderd.
      const staves = (divisions.length > 0 && staffConfig.length > 0)
        ? staffConfig.map(s => ({ name: s.name || null, divisions: s.divisions, bass_clef: s.bass }))
        : null;
      xml = await invoke('convert_midi_to_musicxml', {
        path: filePath,
        options: {
          bpm: Number(bpm) || 90,
          beats_per_bar: Number(beatsPerBar) || 4,
          quantize: Number(quantize) || 4,
          key_fifths: Number(keyFifths) || 0,
          title,
          staves,
        },
      });
      if (!osmd) {
        osmd = new OpenSheetMusicDisplay(container, {
          autoResize: true,
          backend: 'svg',
          drawTitle: true,
        });
      }
      await osmd.load(xml);
      osmd.render();
    } catch (e) {
      error = String(e);
    } finally {
      converting = false;
    }
  }

  async function openOtherFile() {
    try {
      const { open } = await import('@tauri-apps/plugin-dialog');
      const sel = await open({
        multiple: false,
        filters: [{ name: 'MIDI-bestand', extensions: ['mid', 'midi'] }],
      });
      if (sel) {
        filePath = sel;
        title = titleFromPath(sel);
        await convert();
      }
    } catch (e) { error = String(e); }
  }

  async function saveMusicXml() {
    if (!xml) return;
    try {
      const { save } = await import('@tauri-apps/plugin-dialog');
      const suggested = filePath ? filePath.replace(/\.(mid|midi)$/i, '') + '.musicxml' : 'notatie.musicxml';
      const path = await save({
        defaultPath: suggested,
        filters: [{ name: 'MusicXML', extensions: ['musicxml', 'xml'] }],
      });
      if (path) {
        await invoke('save_musicxml', { path, xml });
        alert(`MusicXML opgeslagen:\n${path}`);
      }
    } catch (e) { alert(`Opslaan mislukt: ${e}`); }
  }

  function printScore() {
    // Webview-printdialoog: kies daar een printer of "Microsoft Print to PDF"
    // om een PDF-bestand te maken.
    window.print();
  }

  onMount(async () => {
    await loadDivisions();
    await convert();
  });
</script>

<div class="notation-window">
  <div class="notation-toolbar">
    <label>Tempo
      <input type="number" min="20" max="300" bind:value={bpm} on:change={convert} />
    </label>
    <label>Maat
      <select bind:value={beatsPerBar} on:change={convert}>
        <option value={2}>2/4</option>
        <option value={3}>3/4</option>
        <option value={4}>4/4</option>
        <option value={6}>6/4</option>
      </select>
    </label>
    <label>Raster
      <select bind:value={quantize} on:change={convert}>
        {#each gridChoices as g}<option value={g.v}>{g.label}</option>{/each}
      </select>
    </label>
    <label>Toonsoort
      <select bind:value={keyFifths} on:change={convert}>
        {#each keyChoices as k}<option value={k.v}>{k.label}</option>{/each}
      </select>
    </label>
    <label class="notation-title-field">Titel
      <input type="text" bind:value={title} on:change={convert} />
    </label>
    <span class="notation-spacer"></span>
    <button class="btn btn-ghost btn-sm" on:click={openOtherFile} title="Een ander MIDI-bestand openen">Openen…</button>
    <button class="btn btn-ghost btn-sm" on:click={saveMusicXml} disabled={!xml} title="Opslaan als MusicXML (te openen in andere notatieprogramma's)">Opslaan als MusicXML</button>
    <button class="btn btn-primary btn-sm" on:click={printScore} disabled={!xml} title="Afdrukken — kies 'Microsoft Print to PDF' voor een PDF-bestand">Afdrukken / PDF</button>
  </div>

  {#if divisions.length > 0}
    <!-- Vrij instelbare balk-indeling: vink per notenbalk de divisies aan. -->
    <div class="notation-staves">
      <span class="notation-staves-label">Notenbalken:</span>
      {#each staffConfig as st, i}
        <div class="notation-staff-card">
          <input
            class="notation-staff-name"
            type="text"
            bind:value={st.name}
            on:change={convert}
            title="Naam van deze notenbalk"
          />
          {#each divisions as div}
            <label class="notation-staff-div" title="Noten van {div} op deze balk">
              <input
                type="checkbox"
                checked={st.divisions.includes(div)}
                on:change={() => toggleStaffDivision(i, div)}
              />
              {div}
            </label>
          {/each}
          <label class="notation-staff-div notation-staff-bass" title="Bassleutel (F-sleutel) voor deze balk">
            <input
              type="checkbox"
              checked={st.bass}
              on:change={(e) => setStaffBass(i, e.currentTarget.checked)}
            />
            𝄢
          </label>
          {#if staffConfig.length > 1}
            <button
              class="notation-staff-remove"
              on:click={() => removeStaff(i)}
              title="Deze notenbalk verwijderen"
              aria-label="Balk verwijderen"
            >×</button>
          {/if}
        </div>
      {/each}
      <button class="btn btn-ghost btn-sm" on:click={addStaff} title="Extra notenbalk toevoegen">+ Balk</button>
    </div>
  {/if}

  {#if error}
    <div class="notation-error">{error}</div>
  {/if}
  {#if converting}
    <div class="notation-busy">Bezig met noteren…</div>
  {/if}

  <div class="notation-sheet" bind:this={container}></div>

  <div class="notation-hint">
    Tip: vrij ingespeeld? Stel het tempo in waarop je speelde en kies een grover
    raster (achtsten) voor een rustiger notenbeeld. Wijzigingen converteren direct opnieuw.
  </div>
</div>

<style>
  .notation-window {
    display: flex;
    flex-direction: column;
    height: 100vh;
    background: #fff; /* bladmuziek: altijd licht, ook in de donkere app */
    color: #222;
  }
  .notation-toolbar {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 0.6rem;
    padding: 0.5rem 0.75rem;
    background: var(--bg-panel, #2a2a2a);
    color: var(--text, #eee);
    border-bottom: 1px solid var(--accent-soft, #555);
    flex-shrink: 0;
  }
  .notation-toolbar label {
    display: flex;
    align-items: center;
    gap: 0.3rem;
    font-size: 0.8rem;
    white-space: nowrap;
  }
  .notation-toolbar input[type="number"] { width: 4.5rem; }
  .notation-title-field input { width: 12rem; }
  .notation-spacer { flex: 1; }
  .notation-staves {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 0.5rem;
    padding: 0.4rem 0.75rem;
    background: var(--bg-elevated, #333);
    color: var(--text, #eee);
    border-bottom: 1px solid var(--accent-soft, #555);
    font-size: 0.78rem;
    flex-shrink: 0;
  }
  .notation-staves-label {
    font-weight: 600;
    color: var(--text-muted, #aaa);
  }
  .notation-staff-card {
    display: flex;
    align-items: center;
    gap: 0.45rem;
    padding: 0.2rem 0.45rem;
    border: 1px solid var(--accent-soft, #555);
    border-radius: 6px;
    background: var(--bg-panel, #2a2a2a);
  }
  .notation-staff-name {
    width: 7.5rem;
    font-size: 0.78rem;
  }
  .notation-staff-div {
    display: flex;
    align-items: center;
    gap: 0.2rem;
    white-space: nowrap;
    cursor: pointer;
  }
  .notation-staff-bass {
    font-size: 1.1rem;
    line-height: 1;
  }
  .notation-staff-remove {
    border: none;
    background: none;
    color: #cc6666;
    font-size: 1rem;
    cursor: pointer;
    padding: 0 0.2rem;
  }

  .notation-error {
    padding: 0.5rem 0.75rem;
    background: #7a2020;
    color: #fff;
    font-size: 0.85rem;
  }
  .notation-busy {
    padding: 0.35rem 0.75rem;
    background: #f4ecd4;
    color: #6b5b1e;
    font-size: 0.8rem;
  }
  .notation-sheet {
    flex: 1;
    overflow-y: auto;
    padding: 1rem 1.5rem;
    background: #fff;
  }
  .notation-hint {
    padding: 0.4rem 0.75rem;
    font-size: 0.75rem;
    color: #666;
    background: #f4f4f0;
    border-top: 1px solid #ddd;
    flex-shrink: 0;
  }

  /* Afdrukken: alleen de bladmuziek, zonder werkbalk/hint. */
  @media print {
    .notation-toolbar, .notation-staves, .notation-hint, .notation-busy, .notation-error {
      display: none !important;
    }
    .notation-window { height: auto; }
    .notation-sheet { overflow: visible; padding: 0; }
  }
</style>
