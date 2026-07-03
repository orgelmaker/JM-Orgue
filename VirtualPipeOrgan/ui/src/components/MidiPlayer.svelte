<script>
  import { invoke } from '@tauri-apps/api/core';
  import { open } from '@tauri-apps/plugin-dialog';
  import { onMount, onDestroy } from 'svelte';
  import { t, tx } from '../lib/i18n.js';

  let status = { state: 'stopped', position_ms: 0, duration_ms: 0, loaded: false };
  let speed = 1.0;
  let pollTimer = null;
  let loadedFileName = '';

  onMount(() => {
    pollTimer = setInterval(refresh, 200);
  });

  onDestroy(() => {
    if (pollTimer) clearInterval(pollTimer);
  });

  async function refresh() {
    try {
      status = await invoke('midi_player_status');
    } catch (e) {
      // ignore
    }
  }

  async function pickFile() {
    try {
      const selected = await open({
        multiple: false,
        filters: [{ name: tx('midi_player.file_dialog_title'), extensions: ['mid', 'midi', 'smf'] }],
      });
      if (selected) {
        loadedFileName = selected.split(/[\\/]/).pop();
        await invoke('midi_play_file', { path: selected });
        await refresh();
      }
    } catch (e) {
      console.error('MIDI play failed:', e);
    }
  }

  async function pause() {
    try { await invoke('midi_pause_playback'); await refresh(); } catch (e) {}
  }
  async function resume() {
    try { await invoke('midi_resume_playback'); await refresh(); } catch (e) {}
  }
  async function stop() {
    try { await invoke('midi_stop_playback'); await refresh(); } catch (e) {}
  }
  async function seek(ms) {
    try { await invoke('midi_seek', { positionMs: ms }); } catch (e) {}
  }
  async function changeSpeed(v) {
    speed = v;
    try { await invoke('midi_set_speed', { speed: v }); } catch (e) {}
  }

  function fmtTime(ms) {
    if (!ms || ms < 0) return '0:00';
    const sec = Math.floor(ms / 1000);
    const m = Math.floor(sec / 60);
    const s = sec % 60;
    return `${m}:${s.toString().padStart(2, '0')}`;
  }

  $: progress = status.duration_ms > 0 ? (status.position_ms / status.duration_ms) * 100 : 0;
</script>

<div class="midi-player">
  <div class="player-header">
    <h3>{$t('midi_player.title')}</h3>
    <p>{$t('midi_player.subtitle')}</p>
  </div>

  <div class="player-row">
    <button class="btn btn-secondary btn-sm" on:click={pickFile}>
      <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
        <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"/>
      </svg>
      {$t('midi_player.open_file')}
    </button>
    {#if loadedFileName}
      <span class="file-label">{loadedFileName}</span>
    {/if}
  </div>

  {#if status.loaded || status.state !== 'stopped'}
    <div class="transport">
      {#if status.state === 'playing'}
        <button class="btn btn-secondary transport-btn" on:click={pause} title={$t('midi_player.pause')}>
          <svg width="18" height="18" viewBox="0 0 24 24" fill="currentColor">
            <rect x="6" y="5" width="4" height="14"/>
            <rect x="14" y="5" width="4" height="14"/>
          </svg>
        </button>
      {:else if status.state === 'paused'}
        <button class="btn btn-primary transport-btn" on:click={resume} title={$t('midi_player.resume')}>
          <svg width="18" height="18" viewBox="0 0 24 24" fill="currentColor">
            <polygon points="6,4 20,12 6,20"/>
          </svg>
        </button>
      {:else}
        <button class="btn btn-primary transport-btn" on:click={pickFile} title="Start afspelen">
          <svg width="18" height="18" viewBox="0 0 24 24" fill="currentColor">
            <polygon points="6,4 20,12 6,20"/>
          </svg>
        </button>
      {/if}

      <button class="btn btn-secondary transport-btn" on:click={stop} title={$t('midi_player.stop')}>
        <svg width="18" height="18" viewBox="0 0 24 24" fill="currentColor">
          <rect x="6" y="6" width="12" height="12"/>
        </svg>
      </button>

      <span class="time">{fmtTime(status.position_ms)} / {fmtTime(status.duration_ms)}</span>
    </div>

    <input
      class="seek"
      type="range"
      min="0" max={status.duration_ms || 1} step="100"
      value={status.position_ms}
      on:change={(e) => seek(parseInt(e.target.value, 10))}
    />

    <div class="speed-row">
      <span>{$t('midi_player.speed')}</span>
      <input
        type="range"
        min="0.5" max="2" step="0.05"
        bind:value={speed}
        on:input={() => changeSpeed(speed)}
      />
      <span class="speed-val">{speed.toFixed(2)}×</span>
    </div>
  {/if}
</div>

<style>
  .midi-player {
    padding: 1rem 1.25rem;
  }

  .player-header h3 {
    margin: 0 0 0.25rem;
    font-size: 1rem;
    font-weight: 600;
    color: var(--text);
  }

  .player-header p {
    margin: 0 0 1rem;
    font-size: 0.8rem;
    color: var(--text-secondary);
  }

  .player-row {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    margin-bottom: 0.75rem;
  }

  .file-label {
    font-size: 0.78rem;
    color: var(--text-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .transport {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    margin: 0.5rem 0;
  }

  .transport-btn {
    width: 40px;
    height: 40px;
    padding: 0;
    display: inline-flex;
    align-items: center;
    justify-content: center;
  }

  .time {
    font-family: 'JetBrains Mono', monospace;
    font-size: 0.85rem;
    color: var(--text);
    margin-left: auto;
  }

  .seek {
    width: 100%;
  }

  .speed-row {
    display: grid;
    grid-template-columns: 90px 1fr 60px;
    gap: 0.6rem;
    align-items: center;
    margin-top: 0.6rem;
    font-size: 0.8rem;
    color: var(--text-secondary);
  }

  .speed-val {
    font-family: 'JetBrains Mono', monospace;
    color: var(--text);
    text-align: right;
  }
</style>
