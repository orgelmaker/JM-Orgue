<script>
  import { createEventDispatcher } from 'svelte';
  import { t } from '../lib/i18n.js';

  export let organName = null;
  export let organLoaded = false;
  export let status = {};
  export let activeView = 'orgel';

  const dispatch = createEventDispatcher();
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
  </div>
</header>
