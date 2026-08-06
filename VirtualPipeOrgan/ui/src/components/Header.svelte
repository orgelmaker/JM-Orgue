<script>
  import { createEventDispatcher } from 'svelte';
  import { t } from '../lib/i18n.js';

  export let organName = null;
  export let organLoaded = false;
  export let status = {};
  export let activeView = 'orgel';
  // Audio-uitvoerprofielen (speakers/hoofdtelefoon) voor de snelle wissel.
  export let audioProfiles = { speakers: null, headphones: null, active: null };
  export let audioProfileSwitching = false;
  // Secundaire modus (extra registerscherm): toont een "balk verbergen"-knop;
  // de betekenis van de overige knoppen wordt door de shell (PanelApp) bepaald.
  export let secondary = false;

  const dispatch = createEventDispatcher();

  $: onHeadphones = audioProfiles?.active === 'headphones';
  $: hasAnyProfile = !!(audioProfiles?.speakers || audioProfiles?.headphones);
  // active === null: de huidige uitgang hoort bij geen van beide profielen
  // (bv. handmatig gekozen in Instellingen) — toon dan een neutraal label.
  $: profileLabel = audioProfiles?.active === 'headphones' ? 'Hoofdtelefoon'
    : audioProfiles?.active === 'speakers' ? 'Speakers'
    : 'Uitgang';
</script>

<header class="header">
  <div class="header-brand">
    <div class="nameplate">
      <div class="nameplate-inner">
        <span class="nameplate-title">JM-Orgue</span>
      </div>
    </div>
  </div>

  {#if organLoaded && organName}
    <div class="header-center">
      <div class="header-organ-name">
        <span>{organName}</span>
        <button class="btn btn-ghost btn-icon btn-icon-sm" on:click={() => dispatch('closeOrgan')} title={$t('header.close_organ')}>
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <path d="M18 6L6 18M6 6l12 12"/>
          </svg>
        </button>
      </div>
      <div class="header-tabs" role="tablist" aria-label={$t('nav.orgel')}>
        <button
          class="header-tab"
          class:active={activeView === 'orgel'}
          on:click={() => dispatch('setView', 'orgel')}
          on:keydown={(e) => { if (e.key === 'ArrowRight') dispatch('setView', 'orgel-instellingen'); }}
          role="tab"
          aria-selected={activeView === 'orgel'}
          tabindex={activeView === 'orgel' ? 0 : -1}
          title="F1"
        >{$t('nav.orgel')}</button>
        <button
          class="header-tab"
          class:active={activeView === 'orgel-instellingen'}
          on:click={() => dispatch('setView', 'orgel-instellingen')}
          on:keydown={(e) => {
            if (e.key === 'ArrowLeft') dispatch('setView', 'orgel');
            if (e.key === 'ArrowRight') dispatch('setView', 'algemene-instellingen');
          }}
          role="tab"
          aria-selected={activeView === 'orgel-instellingen'}
          tabindex={activeView === 'orgel-instellingen' ? 0 : -1}
          title="F2"
        >{$t('nav.orgel_instellingen')}</button>
        <button
          class="header-tab"
          class:active={activeView === 'algemene-instellingen'}
          on:click={() => dispatch('setView', 'algemene-instellingen')}
          on:keydown={(e) => { if (e.key === 'ArrowLeft') dispatch('setView', 'orgel-instellingen'); }}
          role="tab"
          aria-selected={activeView === 'algemene-instellingen'}
          tabindex={activeView === 'algemene-instellingen' ? 0 : -1}
          title="F3"
        >{$t('nav.algemene_instellingen')}</button>
      </div>
    </div>
  {/if}

  <div class="header-controls">
    {#if hasAnyProfile}
      <!-- Snelle wissel speakers ↔ hoofdtelefoon (audio-uitvoerprofielen) -->
      <button
        class="btn btn-secondary"
        disabled={audioProfileSwitching}
        on:click={() => dispatch('toggleAudioProfile')}
        title={onHeadphones ? 'Wissel naar speakers' : 'Wissel naar hoofdtelefoon'}
      >
        {#if audioProfileSwitching}
          <span class="learning-indicator"></span>
        {:else if onHeadphones}
          <!-- hoofdtelefoon-icoon: actief profiel -->
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <path d="M3 18v-6a9 9 0 0 1 18 0v6"/>
            <path d="M21 19a2 2 0 0 1-2 2h-1a2 2 0 0 1-2-2v-3a2 2 0 0 1 2-2h3zM3 19a2 2 0 0 0 2 2h1a2 2 0 0 0 2-2v-3a2 2 0 0 0-2-2H3z"/>
          </svg>
        {:else}
          <!-- speaker-icoon: actief profiel -->
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <polygon points="11 5 6 9 2 9 2 15 6 15 11 19 11 5"/>
            <path d="M15.54 8.46a5 5 0 0 1 0 7.07M19.07 4.93a10 10 0 0 1 0 14.14"/>
          </svg>
        {/if}
        {profileLabel}
      </button>
    {/if}
    {#if status.audioRunning}
      <button class="btn btn-secondary" on:click={() => dispatch('stopAudio')}>
        <svg width="16" height="16" viewBox="0 0 24 24" fill="currentColor">
          <rect x="6" y="6" width="12" height="12" rx="1"/>
        </svg>
        {$t('header.stop')}
      </button>
    {:else}
      <button class="btn btn-primary" on:click={() => dispatch('startAudio')}>
        <svg width="16" height="16" viewBox="0 0 24 24" fill="currentColor">
          <polygon points="6,4 20,12 6,20"/>
        </svg>
        {$t('header.start')}
      </button>
    {/if}
    {#if secondary}
      <!-- Hoofdbalk verbergen op dit extra scherm (keuze wordt per scherm onthouden) -->
      <button
        class="btn btn-ghost btn-icon btn-icon-sm"
        on:click={() => dispatch('toggleHeader')}
        title="Hoofdbalk verbergen (via de knop in de werkbalk weer te tonen)"
        aria-label="Hoofdbalk verbergen"
      >
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <polyline points="18 15 12 9 6 15"/>
        </svg>
      </button>
    {/if}
  </div>
</header>
