<script>
  // Kopbalk van het notatievenster (0.7.85): Bestand-menu, titel, opnemen,
  // afspelen, tempo, ongedaan maken, weergave, zoom en hulp. Alle logica
  // blijft in NotationWindow; dit onderdeel roept alleen `acties` aan.
  import { t } from '../../lib/i18n.js';

  export let title = '';
  export let dirty = false;
  export let recording = false;
  export let armedWaiting = false;
  export let countInRemaining = 0;
  export let playingScore = false;
  export let bpm = 90;
  export let viewMode = 'staff';
  export let zoom = 1;
  export let recent = [];
  export let kanExportXml = false;
  export let kanExportSvg = false;
  export let kanPrint = false;
  export let acties = {};

  let menuOpen = false;
  function doe(naam, ...args) {
    menuOpen = false;
    const f = acties[naam];
    if (typeof f === 'function') f(...args);
  }
  function buitenKlik(e) {
    if (menuOpen && !(e.target && e.target.closest && e.target.closest('.bestand-menu'))) menuOpen = false;
  }
  function bestandsnaam(p) { return String(p || '').split(/[\\/]/).pop(); }
</script>

<svelte:window on:mousedown={buitenKlik} />

<div class="kop">
  <div class="bestand-menu">
    <button class="btn btn-ghost btn-sm" on:click={() => menuOpen = !menuOpen} aria-haspopup="menu" aria-expanded={menuOpen}>{$t('notation.file_menu')} ▾</button>
    {#if menuOpen}
      <div class="menu" role="menu">
        <button role="menuitem" on:click={() => doe('nieuw')}>{$t('notation.new')}<kbd>Ctrl+N</kbd></button>
        <button role="menuitem" on:click={() => doe('openProject')}>{$t('notation.open_project')}<kbd>Ctrl+O</kbd></button>
        {#if recent.length}
          <div class="menu-kop">{$t('notation.recent_menu')}</div>
          {#each recent.slice(0, 6) as r (r.path)}
            <button role="menuitem" class="recent" title={r.path} on:click={() => doe('openRecent', r.path)}>{r.title || bestandsnaam(r.path)}</button>
          {/each}
        {/if}
        <div class="menu-scheiding"></div>
        <button role="menuitem" on:click={() => doe('save')}>{$t('notation.save')}<kbd>Ctrl+S</kbd></button>
        <button role="menuitem" on:click={() => doe('saveAs')}>{$t('notation.save_as')}<kbd>Ctrl+Shift+S</kbd></button>
        <div class="menu-scheiding"></div>
        <button role="menuitem" on:click={() => doe('importMidi')}>{$t('notation.import_midi')}</button>
        <div class="menu-kop">{$t('notation.export_menu')}</div>
        <button role="menuitem" disabled={!kanExportXml} on:click={() => doe('exportMusicXml')}>MusicXML…</button>
        <button role="menuitem" on:click={() => doe('exportMidi')}>MIDI…</button>
        <button role="menuitem" disabled={!kanExportSvg} on:click={() => doe('exportSvg')}>SVG ({$t('notation.view_klavar')})…</button>
        <div class="menu-scheiding"></div>
        <button role="menuitem" disabled={!kanPrint} on:click={() => doe('print')}>{$t('notation.print_pdf')}</button>
      </div>
    {/if}
  </div>

  <input class="titel" type="text" value={title} placeholder={$t('notation.title')} title={$t('notation.title')}
    on:change={(e) => doe('setTitle', e.currentTarget.value)} />
  {#if dirty}<span class="vuil" title={$t('notation.unsaved_marker')}>*</span>{/if}

  <button class="btn record-toggle" class:recording={recording || armedWaiting} disabled={playingScore}
    on:click={() => doe('toggleRecording')}
    title={recording || armedWaiting ? $t('notation.record_stop_title') : $t('notation.record_start_title')}>
    <span class="record-dot" class:on={recording || armedWaiting}></span>
    {#if armedWaiting}{$t('notation.counting_in').replace('{n}', countInRemaining)}{:else if recording}{$t('midi_player.stop')}{:else}{$t('notation.record')}{/if}
  </button>
  <button class="btn btn-secondary btn-sm" on:click={() => doe('togglePlay')} disabled={recording || armedWaiting}
    title={playingScore ? $t('notation.play_stop_title') : $t('notation.play_title')}>
    {playingScore ? '◼ ' + $t('midi_player.stop') : '▶ ' + $t('notation.play')}
  </button>
  <label class="tempo">{$t('notation.tempo')}
    <input type="number" min="20" max="300" value={bpm} on:change={(e) => doe('setBpm', e.currentTarget.value)} />
  </label>
  <button class="btn btn-ghost btn-sm" on:click={() => doe('undo')} title={$t('notation.undo_title')}>↶</button>
  <button class="btn btn-ghost btn-sm" on:click={() => doe('redo')} title={$t('notation.redo_title')}>↷</button>

  <span class="notation-spacer"></span>

  <span class="view-switch" role="group" aria-label={$t('notation.view')}>
    <button class="btn btn-ghost btn-sm" class:active={viewMode === 'staff'} aria-pressed={viewMode === 'staff'}
      on:click={() => doe('setViewMode', 'staff')} title={$t('notation.view_staff_title') + ' (K)'}>{$t('notation.view_staff')}</button>
    <button class="btn btn-ghost btn-sm" class:active={viewMode === 'klavar'} aria-pressed={viewMode === 'klavar'}
      on:click={() => doe('setViewMode', 'klavar')} title={$t('notation.view_klavar_title') + ' (K)'}>{$t('notation.view_klavar')}</button>
  </span>
  <button class="btn btn-ghost btn-sm" on:click={() => doe('zoom', -0.1)} title={$t('notation.zoom_out')}>−</button>
  <span class="zoom-pct">{Math.round(zoom * 100)}%</span>
  <button class="btn btn-ghost btn-sm" on:click={() => doe('zoom', +0.1)} title={$t('notation.zoom_in')}>+</button>
  <button class="btn btn-ghost btn-sm hulp" on:click={() => doe('help')} title={$t('notation.help_title')}>?</button>
</div>

<style>
  .kop {
    display: flex; align-items: center; flex-wrap: wrap; gap: 0.5rem;
    padding: 0.4rem 0.75rem; min-width: 0;
    background: var(--bg-panel, #2a2a2a); color: var(--text, #eee);
    border-bottom: 1px solid var(--text-muted, #555);
    flex-shrink: 0;
  }
  .kop label { display: flex; align-items: center; gap: 0.3rem; font-size: 0.8rem; white-space: nowrap; }
  .kop input[type="number"] { width: 4.5rem; }
  .titel { width: 14rem; min-width: 6rem; font-size: 0.9rem; }
  .vuil { font-weight: 700; color: var(--gold-border, #d4af37); margin-left: -0.3rem; }
  .notation-spacer { flex: 1; }
  .bestand-menu { position: relative; }
  .menu {
    position: absolute; top: 100%; left: 0; z-index: 60; min-width: 15rem;
    display: flex; flex-direction: column; padding: 0.3rem 0;
    background: var(--bg-elevated, #333); color: var(--text, #eee);
    border: 1px solid var(--text-muted, #555); border-radius: 6px;
    box-shadow: 0 6px 18px rgba(0, 0, 0, 0.4);
  }
  .menu button {
    display: flex; justify-content: space-between; align-items: center; gap: 1rem;
    border: none; background: transparent; color: inherit; text-align: left;
    padding: 0.35rem 0.9rem; font-size: 0.85rem; cursor: pointer; min-height: 2rem;
  }
  .menu button:hover:not(:disabled) { background: var(--bg-panel, #2a2a2a); }
  .menu button:disabled { opacity: 0.4; cursor: default; }
  .menu button.recent { padding-left: 1.6rem; font-size: 0.8rem; }
  .menu kbd { font-size: 0.7rem; color: var(--text-muted, #aaa); font-family: inherit; }
  .menu-kop { padding: 0.25rem 0.9rem 0.1rem; font-size: 0.72rem; color: var(--text-muted, #aaa); text-transform: uppercase; letter-spacing: 0.04em; }
  .menu-scheiding { height: 1px; margin: 0.25rem 0; background: var(--text-muted, #555); }
  .record-toggle {
    display: inline-flex; align-items: center; gap: 0.35rem;
    padding: 0.3rem 0.8rem; border: 1px solid #cc4444; border-radius: 6px;
    background: transparent; color: #cc6666; font-weight: 600; cursor: pointer; font-size: 0.9rem;
  }
  .record-toggle.recording { background: #7a2020; color: #fff; }
  .record-dot { width: 0.7rem; height: 0.7rem; border-radius: 50%; background: #cc4444; opacity: 0.5; }
  .record-dot.on { background: #ff4040; opacity: 1; animation: rec-pulse 1s infinite; }
  @keyframes rec-pulse { 50% { opacity: 0.4; } }
  .view-switch { display: inline-flex; gap: 0.15rem; }
  .view-switch .btn.active {
    border-color: var(--gold-border, #d4af37);
    background: var(--bg-darkest, #e8e0d0);
    color: var(--primary, #b8960b);
    box-shadow: 0 0 0 1px var(--gold-border, #d4af37);
  }
  .zoom-pct { font-size: 0.72rem; color: var(--text-muted, #aaa); min-width: 2.6rem; text-align: center; }
  .hulp { font-weight: 700; min-width: 2rem; }
  @media print { .kop { display: none !important; } }
</style>
