<script>
  import { onMount } from 'svelte';
  import { open } from '@tauri-apps/plugin-dialog';
  import { convertFileSrc } from '@tauri-apps/api/core';
  import { t, locale, setLocale, AVAILABLE_LOCALES, LOCALE_LABELS } from '../lib/i18n.js';
  import { PRESETS, leesSfeer, pasSfeerToeAlsGewijzigd } from '../lib/sfeer.js';

  // ============================================================
  // SCHEMA — alle instelbare tokens met label/groep/type
  // ============================================================
  const TOKEN_GROUPS = [
    {
      titleKey: 'settings.appearance_color_groups.backgrounds',
      tokens: [
        { key: 'bg-dark', labelKey: 'colors.background' },
        { key: 'bg-panel', labelKey: 'colors.bg_panel' },
        { key: 'bg-elevated', labelKey: 'colors.bg_elevated' },
        { key: 'bg-darkest', labelKey: 'colors.bg_darkest' },
        { key: 'bg-control', labelKey: 'colors.bg_control' },
        { key: 'bg-hover', labelKey: 'colors.bg_hover' },
      ],
    },
    {
      titleKey: 'settings.appearance_color_groups.text',
      tokens: [
        { key: 'text', labelKey: 'colors.text' },
        { key: 'text-secondary', labelKey: 'colors.text_secondary' },
        { key: 'text-muted', labelKey: 'colors.text_muted' },
        { key: 'text-on-stop', labelKey: 'colors.text_on_stop' },
        { key: 'text-on-stop-secondary', labelKey: 'colors.text_on_stop_secondary' },
        { key: 'stop-active-text', labelKey: 'colors.stop_active_text' },
        { key: 'stop-active-text-secondary', labelKey: 'colors.stop_active_text_secondary' },
        { key: 'text-on-division', labelKey: 'colors.text_on_division' },
        { key: 'text-on-primary', labelKey: 'colors.text_on_primary' },
      ],
    },
    {
      titleKey: 'settings.appearance_color_groups.registers',
      tokens: [
        { key: 'stop-porcelain', labelKey: 'colors.stop_porcelain' },
        { key: 'stop-porcelain-border', labelKey: 'colors.stop_porcelain_border' },
        { key: 'stop-active', labelKey: 'colors.stop_active' },
        { key: 'stop-active-border', labelKey: 'colors.stop_active_border' },
      ],
    },
    {
      titleKey: 'settings.appearance_color_groups.accent',
      tokens: [
        { key: 'primary', labelKey: 'colors.primary' },
        { key: 'primary-hover', labelKey: 'colors.primary_hover' },
        { key: 'primary-dark', labelKey: 'colors.primary_dark' },
        { key: 'gold-border', labelKey: 'colors.gold_border' },
      ],
    },
    {
      titleKey: 'settings.appearance_color_groups.status',
      tokens: [
        { key: 'success', labelKey: 'colors.success' },
        { key: 'warning', labelKey: 'colors.warning' },
        { key: 'error', labelKey: 'colors.error' },
        { key: 'led-green', labelKey: 'colors.led_green' },
        { key: 'led-blue', labelKey: 'colors.led_blue' },
        { key: 'midi-indicator', labelKey: 'colors.midi_indicator' },
      ],
    },
    {
      titleKey: 'settings.appearance_color_groups.nameplate',
      tokens: [
        { key: 'nameplate-frame-mid', labelKey: 'colors.nameplate_frame_mid' },
        { key: 'nameplate-face-mid', labelKey: 'colors.nameplate_face_mid' },
        { key: 'nameplate-text', labelKey: 'colors.nameplate_text' },
      ],
    },
    {
      titleKey: 'settings.appearance_color_groups.misc',
      tokens: [
        { key: 'meter-bar-bg', labelKey: 'colors.meter_bar_bg' },
        { key: 'scrollbar-thumb', labelKey: 'colors.scrollbar_thumb' },
        { key: 'menu-bg', labelKey: 'colors.menu_bg' },
        { key: 'menu-text', labelKey: 'colors.menu_text' },
      ],
    },
  ];


  // ============================================================
  // FONT OPTIES
  // ============================================================
  const fontOptions = [
    { labelKey: 'fonts.georgia_classic', value: 'Georgia' },
    { labelKey: 'fonts.times_new_roman', value: 'Times New Roman' },
    { labelKey: 'fonts.garamond', value: 'Garamond' },
    { labelKey: 'fonts.palatino', value: 'Palatino Linotype' },
    { labelKey: 'fonts.book_antiqua', value: 'Book Antiqua' },
    { labelKey: 'fonts.segoe_ui_modern', value: 'Segoe UI' },
    { labelKey: 'fonts.inter_sans', value: 'Inter' },
    { labelKey: 'fonts.verdana', value: 'Verdana' },
  ];

  const fontSizeOptions = [
    { labelKey: 'fonts.size_small', value: '0.7rem' },
    { labelKey: 'fonts.size_normal', value: '0.8rem' },
    { labelKey: 'fonts.size_large', value: '0.9rem' },
    { labelKey: 'fonts.size_xlarge', value: '1.0rem' },
  ];

  const fontWeightOptions = [
    { labelKey: 'fonts.weight_light', value: '400' },
    { labelKey: 'fonts.weight_normal', value: '500' },
    { labelKey: 'fonts.weight_bold', value: '600' },
    { labelKey: 'fonts.weight_xbold', value: '700' },
  ];

  // ============================================================
  // STATE
  // ============================================================
  let colors = { ...PRESETS.basis.colors };
  let font = { ...PRESETS.basis.font };
  let textures = { background: null };
  let activePresetKey = 'basis';
  let userPresets = {};      // { naam: { colors, font, texture } }
  let newPresetName = '';

  onMount(() => {
    const s = leesSfeer();
    colors = s.colors;
    font = s.font;
    textures = s.textures;
    activePresetKey = s.activePresetKey;
    userPresets = s.userPresets;
    applyStyles();
  });

  // ============================================================
  // STYLE TOEPASSING — de echte toepassing zit sinds 0.7.68 in lib/sfeer.js,
  // zodat elk venster hem bij de start krijgt (zie App.svelte) en de
  // 1-seconde-poll in Console wijzigingen uit andere vensters volgt.
  // ============================================================
  function applyStyles() {
    pasSfeerToeAlsGewijzigd();
  }

  // ============================================================
  // PERSISTENTIE
  // ============================================================
  function saveSettings() {
    localStorage.setItem('jm-orgue-colors', JSON.stringify(colors));
    localStorage.setItem('jm-orgue-textures', JSON.stringify(textures));
    localStorage.setItem('jm-orgue-font', JSON.stringify(font));
    localStorage.setItem('jm-orgue-active-preset', activePresetKey);
    applyStyles();
  }

  function saveUserPresets() {
    localStorage.setItem('jm-orgue-user-presets', JSON.stringify(userPresets));
  }

  // ============================================================
  // PRESET ACTIES
  // ============================================================
  function applyPreset(key) {
    let preset;
    if (PRESETS[key]) {
      preset = PRESETS[key];
    } else if (userPresets[key]) {
      preset = userPresets[key];
    } else {
      return;
    }
    colors = { ...PRESETS.basis.colors, ...preset.colors };
    font = { ...PRESETS.basis.font, ...preset.font };
    textures = { background: preset.texture || null };
    activePresetKey = key;
    saveSettings();
  }

  function saveAsUserPreset() {
    const name = (newPresetName || '').trim();
    if (!name) return;
    const key = 'user_' + name.replace(/\s+/g, '_').toLowerCase();
    userPresets[key] = {
      name,
      colors: { ...colors },
      font: { ...font },
      texture: textures.background,
    };
    userPresets = userPresets;
    activePresetKey = key;
    newPresetName = '';
    saveUserPresets();
    saveSettings();
  }

  function deleteUserPreset(key) {
    delete userPresets[key];
    userPresets = userPresets;
    if (activePresetKey === key) activePresetKey = 'basis';
    saveUserPresets();
    saveSettings();
  }

  function exportCurrentAsJson() {
    const blob = new Blob([JSON.stringify({ colors, font, texture: textures.background }, null, 2)], { type: 'application/json' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `jm-orgue-thema-${activePresetKey}.json`;
    a.click();
    URL.revokeObjectURL(url);
  }

  // ============================================================
  // INPUT HANDLERS
  // ============================================================
  function handleColor(key, value) {
    colors[key] = value;
    activePresetKey = 'aangepast';
    saveSettings();
  }

  function handleFont(key, value) {
    font[key] = value;
    activePresetKey = 'aangepast';
    saveSettings();
  }

  async function selectTexture() {
    try {
      const selected = await open({
        multiple: false,
        filters: [{ name: 'Afbeeldingen', extensions: ['png', 'jpg', 'jpeg', 'bmp', 'gif', 'webp'] }],
      });
      if (selected) {
        textures.background = convertFileSrc(selected);
        activePresetKey = 'aangepast';
        saveSettings();
      }
    } catch (e) {
      console.error('Texture select failed:', e);
    }
  }

  function removeTexture() {
    textures.background = null;
    activePresetKey = 'aangepast';
    saveSettings();
  }
</script>

<div class="layout-settings">
  <div class="settings-header">
    <h3>{$t('settings.appearance')}</h3>
    <p>{$t('settings.appearance_subtitle')}</p>
  </div>

  <!-- ============================================================ -->
  <!-- TAAL / LANGUAGE                                               -->
  <!-- ============================================================ -->
  <div class="settings-section">
    <div class="section-title">{$t('settings.language')}</div>
    <div class="lang-grid">
      {#each AVAILABLE_LOCALES as code}
        <button
          type="button"
          class="lang-card"
          class:active={$locale === code}
          on:click={() => setLocale(code)}
          title={LOCALE_LABELS[code]}
        >
          <div class="lang-code">{code.toUpperCase()}</div>
          <div class="lang-name">{LOCALE_LABELS[code]}</div>
        </button>
      {/each}
    </div>
  </div>

  <!-- ============================================================ -->
  <!-- PRESETS                                                       -->
  <!-- ============================================================ -->
  <div class="settings-section">
    <div class="section-title">{$t('settings.theme_presets')}</div>

    <div class="preset-grid">
      {#each Object.entries(PRESETS) as [key, preset]}
        <button
          type="button"
          class="preset-card"
          class:active={activePresetKey === key}
          on:click={() => applyPreset(key)}
          title={$t('presets.' + key)}
        >
          <div class="preset-swatch">
            <div class="swatch-band" style="background: {preset.colors['bg-dark']}"></div>
            <div class="swatch-band" style="background: {preset.colors['gold-border']}"></div>
            <div class="swatch-band" style="background: {preset.colors['stop-active']}"></div>
            <div class="swatch-band" style="background: {preset.colors['bg-control']}"></div>
          </div>
          <div class="preset-name">{$t('presets.' + key)}</div>
        </button>
      {/each}
    </div>

    {#if Object.keys(userPresets).length > 0}
      <div class="user-presets-title">{$t('settings.theme_user_presets')}</div>
      <div class="preset-grid">
        {#each Object.entries(userPresets) as [key, preset]}
          <div class="preset-card-wrapper">
            <button
              type="button"
              class="preset-card"
              class:active={activePresetKey === key}
              on:click={() => applyPreset(key)}
            >
              <div class="preset-swatch">
                <div class="swatch-band" style="background: {preset.colors['bg-dark']}"></div>
                <div class="swatch-band" style="background: {preset.colors['gold-border']}"></div>
                <div class="swatch-band" style="background: {preset.colors['stop-active']}"></div>
                <div class="swatch-band" style="background: {preset.colors['bg-control']}"></div>
              </div>
              <div class="preset-name">{preset.name}</div>
            </button>
            <button
              type="button"
              class="preset-delete"
              on:click|stopPropagation={() => deleteUserPreset(key)}
              title={$t('actions.delete')}
            >×</button>
          </div>
        {/each}
      </div>
    {/if}

    <!-- Eigen sfeer opslaan -->
    <div class="save-preset-row">
      <input
        type="text"
        class="preset-name-input"
        placeholder={$t('settings.theme_preset_name')}
        bind:value={newPresetName}
        on:keydown={(e) => { if (e.key === 'Enter') saveAsUserPreset(); }}
      />
      <button class="btn btn-secondary btn-sm" on:click={saveAsUserPreset} disabled={!newPresetName.trim()}>
        {$t('settings.save_preset')}
      </button>
      <button class="btn btn-ghost btn-sm" on:click={exportCurrentAsJson} title={$t('settings.export_preset')}>
        {$t('settings.export_preset')}
      </button>
    </div>
  </div>

  <!-- ============================================================ -->
  <!-- LETTERTYPE                                                    -->
  <!-- ============================================================ -->
  <div class="settings-section">
    <div class="section-title">{$t('settings.appearance_font_section')}</div>

    <div class="font-row">
      <span class="font-label">{$t('settings.appearance_font_family')}</span>
      <select class="font-select" value={font.family} on:change={(e) => handleFont('family', e.target.value)}>
        {#each fontOptions as opt}
          <option value={opt.value}>{$t(opt.labelKey)}</option>
        {/each}
      </select>
    </div>

    <div class="font-row">
      <span class="font-label">{$t('settings.appearance_font_size')}</span>
      <select class="font-select" value={font.size} on:change={(e) => handleFont('size', e.target.value)}>
        {#each fontSizeOptions as opt}
          <option value={opt.value}>{$t(opt.labelKey)}</option>
        {/each}
      </select>
    </div>

    <div class="font-row">
      <span class="font-label">{$t('settings.appearance_font_weight')}</span>
      <select class="font-select" value={font.weight} on:change={(e) => handleFont('weight', e.target.value)}>
        {#each fontWeightOptions as opt}
          <option value={opt.value}>{$t(opt.labelKey)}</option>
        {/each}
      </select>
    </div>

    <div class="font-preview">
      <span style="font-family: '{font.family}', serif; font-size: {font.size}; font-weight: {font.weight};">
        {$t('fonts.preview_sample')}
      </span>
    </div>
  </div>

  <!-- ============================================================ -->
  <!-- ACHTERGROND TEXTUUR                                          -->
  <!-- ============================================================ -->
  <div class="settings-section">
    <div class="section-title">{$t('settings.appearance_texture_section')}</div>

    <div class="texture-row">
      <div class="texture-info">
        <span class="texture-label">{$t('settings.appearance_texture_label')}</span>
        <span class="texture-hint">{$t('settings.appearance_texture_hint')}</span>
      </div>
      <div class="texture-controls">
        {#if textures.background}
          <div class="texture-preview" style="background-image: url('{textures.background}')"></div>
          <button class="btn btn-ghost btn-icon" on:click={removeTexture} title={$t('actions.delete')}>×</button>
        {:else}
          <button class="btn btn-secondary btn-sm" on:click={selectTexture}>{$t('settings.appearance_texture_select')}</button>
        {/if}
      </div>
    </div>
  </div>

  <!-- ============================================================ -->
  <!-- KLEUREN — alle tokens per groep                              -->
  <!-- ============================================================ -->
  {#each TOKEN_GROUPS as group}
    <div class="settings-section">
      <div class="section-title">{$t(group.titleKey)}</div>
      {#each group.tokens as token}
        <div class="color-row">
          <div class="color-info">
            <span class="color-label">{$t(token.labelKey)}</span>
          </div>
          <div class="color-input-wrapper">
            <input
              type="color"
              value={colors[token.key] || '#000000'}
              on:input={(e) => handleColor(token.key, e.target.value)}
            />
            <span class="color-value">{colors[token.key] || ''}</span>
          </div>
        </div>
      {/each}
    </div>
  {/each}

  <!-- ============================================================ -->
  <!-- RESET                                                        -->
  <!-- ============================================================ -->
  <div class="settings-actions">
    <button class="btn btn-secondary" on:click={() => applyPreset('basis')}>
      {$t('settings.reset_to_base')}
    </button>
  </div>
</div>

<style>
  .layout-settings {
    padding: 1.5rem;
    max-width: 600px;
    overflow-y: auto;
    max-height: calc(100vh - 200px);
  }

  .settings-header {
    margin-bottom: 1.5rem;
  }

  .settings-header h3 {
    font-size: 1.1rem;
    font-weight: 600;
    color: var(--text);
    margin-bottom: 0.5rem;
  }

  .settings-header p {
    font-size: 0.85rem;
    color: var(--text-secondary);
  }

  .settings-section {
    margin-bottom: 2rem;
  }

  .section-title {
    font-size: 0.75rem;
    text-transform: uppercase;
    color: var(--text-muted);
    letter-spacing: 0.1em;
    font-weight: 600;
    margin-bottom: 0.75rem;
  }

  /* Taal */
  .lang-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(120px, 1fr));
    gap: 0.6rem;
  }
  .lang-card {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 0.25rem;
    padding: 0.75rem 0.5rem;
    background: var(--bg-panel);
    border: 2px solid var(--accent-soft);
    border-radius: var(--radius-md);
    cursor: pointer;
    transition: all var(--transition-fast);
    text-align: center;
  }
  .lang-card:hover {
    border-color: var(--gold-border);
  }
  .lang-card.active {
    border-color: var(--gold-border);
    background: var(--accent-soft-hover);
    box-shadow: 0 0 0 1px var(--gold-border);
  }
  .lang-code {
    font-family: 'Fraunces', 'Georgia', serif;
    font-size: 1.6rem;
    font-weight: 900;
    color: var(--primary);
    line-height: 1;
  }
  .lang-name {
    font-size: 0.78rem;
    color: var(--text);
  }

  /* Presets */
  .preset-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(140px, 1fr));
    gap: 0.6rem;
    margin-bottom: 0.75rem;
  }

  .preset-card-wrapper {
    position: relative;
  }

  .preset-card {
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
    padding: 0.6rem;
    background: var(--bg-panel);
    border: 2px solid var(--accent-soft);
    border-radius: var(--radius-md);
    cursor: pointer;
    transition: all var(--transition-fast);
    width: 100%;
    text-align: left;
  }

  .preset-card:hover {
    border-color: var(--gold-border);
  }

  .preset-card.active {
    border-color: var(--gold-border);
    background: var(--accent-soft-hover);
    box-shadow: 0 0 0 1px var(--gold-border);
  }

  .preset-swatch {
    display: flex;
    height: 24px;
    border-radius: var(--radius-sm);
    overflow: hidden;
    border: 1px solid var(--accent-soft);
  }

  .swatch-band {
    flex: 1;
    height: 100%;
  }

  .preset-name {
    font-size: 0.78rem;
    color: var(--text);
    line-height: 1.2;
  }

  .preset-delete {
    position: absolute;
    top: 4px;
    right: 4px;
    width: 20px;
    height: 20px;
    border-radius: 50%;
    border: none;
    background: var(--bg-overlay);
    color: var(--menu-text);
    font-size: 14px;
    line-height: 1;
    cursor: pointer;
    opacity: 0;
    transition: opacity var(--transition-fast);
  }

  .preset-card-wrapper:hover .preset-delete {
    opacity: 1;
  }

  .preset-delete:hover {
    background: var(--error);
    color: var(--text-on-primary);
  }

  .user-presets-title {
    font-size: 0.7rem;
    text-transform: uppercase;
    color: var(--text-muted);
    letter-spacing: 0.1em;
    font-weight: 600;
    margin: 1rem 0 0.5rem;
  }

  .save-preset-row {
    display: flex;
    gap: 0.5rem;
    align-items: center;
  }

  .preset-name-input {
    flex: 1;
    padding: 0.4rem 0.6rem;
    background: var(--bg-elevated);
    border: 1px solid var(--accent-soft-2);
    border-radius: var(--radius-sm);
    color: var(--text);
    font-size: 0.8rem;
  }

  .preset-name-input:focus {
    outline: none;
    border-color: var(--primary);
  }

  /* Font Settings */
  .font-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0.6rem 0.75rem;
    background: var(--bg-panel);
    border-radius: var(--radius-md);
    margin-bottom: 0.4rem;
    border: var(--border-subtle);
  }

  .font-label {
    font-size: 0.85rem;
    color: var(--text);
    font-weight: 500;
  }

  .font-select {
    padding: 0.35rem 0.6rem;
    background: var(--bg-elevated);
    border: 1px solid var(--accent-soft-2);
    border-radius: var(--radius-sm);
    color: var(--text);
    font-size: 0.8rem;
    cursor: pointer;
    min-width: 180px;
  }

  .font-select:focus {
    outline: none;
    border-color: var(--primary);
  }

  .font-preview {
    margin-top: 0.75rem;
    padding: 0.75rem;
    background: var(--bg-control);
    border-radius: 5px;
    text-align: center;
    position: relative;
  }

  .font-preview::before {
    content: '';
    position: absolute;
    left: 8px;
    right: 8px;
    top: 6px;
    bottom: 6px;
    background: var(--stop-porcelain);
    border-radius: 3px;
    border: 1px solid var(--stop-porcelain-border);
  }

  .font-preview span {
    position: relative;
    z-index: 1;
    color: var(--text-on-stop);
  }

  /* Texture Rows */
  .texture-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0.75rem;
    background: var(--bg-panel);
    border-radius: var(--radius-md);
    margin-bottom: 0.5rem;
    border: var(--border-subtle);
  }

  .texture-info {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
  }

  .texture-label {
    font-size: 0.9rem;
    font-weight: 500;
    color: var(--text);
  }

  .texture-hint {
    font-size: 0.75rem;
    color: var(--text-muted);
  }

  .texture-controls {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }

  .texture-preview {
    width: 48px;
    height: 48px;
    border-radius: var(--radius-sm);
    background-size: cover;
    background-position: center;
    border: 2px solid var(--accent-soft-2);
  }

  .btn-sm {
    padding: 0.4rem 0.75rem;
    font-size: 0.8rem;
  }

  /* Color Rows */
  .color-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0.55rem 0.75rem;
    background: var(--bg-panel);
    border-radius: var(--radius-md);
    margin-bottom: 0.35rem;
    border: var(--border-subtle);
  }

  .color-info {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
  }

  .color-label {
    font-size: 0.82rem;
    font-weight: 500;
    color: var(--text);
  }

  .color-input-wrapper {
    display: flex;
    align-items: center;
    gap: 0.6rem;
  }

  .color-input-wrapper input[type="color"] {
    width: 36px;
    height: 36px;
    padding: 0;
    border: 2px solid var(--accent-soft-2);
    border-radius: var(--radius-sm);
    cursor: pointer;
    background: transparent;
  }

  .color-input-wrapper input[type="color"]::-webkit-color-swatch-wrapper {
    padding: 2px;
  }

  .color-input-wrapper input[type="color"]::-webkit-color-swatch {
    border-radius: 4px;
    border: none;
  }

  .color-value {
    font-family: 'JetBrains Mono', monospace;
    font-size: 0.7rem;
    color: var(--text-muted);
    min-width: 65px;
  }

  /* Actions */
  .settings-actions {
    display: flex;
    justify-content: flex-start;
    padding-top: 1rem;
    border-top: var(--border-subtle);
  }
</style>
