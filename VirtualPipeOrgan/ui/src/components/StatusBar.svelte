<script>
  import { onMount } from 'svelte';
  import { getVersion } from '@tauri-apps/api/app';
  import { t } from '../lib/i18n.js';

  export let status = {};
  export let error = null;

  // App-versie dynamisch uit tauri.conf.json (voorheen hardcoded en verouderd).
  let appVersion = '';
  onMount(async () => {
    try {
      appVersion = await getVersion();
    } catch (e) { /* buiten Tauri-context (browser/dev) */ }
  });
</script>

<footer class="status-bar">
  <div class="status-group">
    <div class="status-item">
      <div class="status-dot" class:active={status.audioRunning}></div>
      <span>{status.audioRunning ? $t('status_bar.audio_running') : $t('status_bar.audio_stopped')}</span>
    </div>

    <div class="status-item">
      <div class="status-dot" class:active={status.midiConnected}></div>
      <span>{status.midiConnected ? $t('status_bar.midi_connected') : $t('status_bar.midi_disconnected')}</span>
    </div>

    <div class="status-item">
      <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
        <path d="M12 20v-6M6 20V10M18 20V4"/>
      </svg>
      <span>{status.voiceCount} {$t('status_bar.voices')}</span>
    </div>
  </div>

  <div class="status-group">
    {#if error}
      <div class="status-item">
        <div class="status-dot error"></div>
        <span style="color: var(--error);">{error}</span>
      </div>
    {:else}
      <span style="color: var(--text-muted);">JM-Orgue{appVersion ? ` v${appVersion}` : ''}</span>
    {/if}
  </div>
</footer>
