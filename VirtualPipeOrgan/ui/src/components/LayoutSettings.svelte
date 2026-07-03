<script>
  import { onMount } from 'svelte';
  import { open } from '@tauri-apps/plugin-dialog';
  import { convertFileSrc } from '@tauri-apps/api/core';
  import { t, locale, setLocale, AVAILABLE_LOCALES, LOCALE_LABELS } from '../lib/i18n.js';

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
  // PRESETS — meegeleverde sferen
  // ============================================================
  const PRESETS = {
    basis: {
      name: 'Basis',
      colors: {
        'primary': '#b8960b',
        'primary-hover': '#d4af37',
        'primary-dark': '#8b7209',
        'bg-darkest': '#e8e0d0',
        'bg-dark': '#f0ebe0',
        'bg-panel': '#f5f0e6',
        'bg-elevated': '#faf6ee',
        'bg-control': '#1a1a1a',
        'bg-hover': '#2a2a2a',
        'text': '#2c1810',
        'text-secondary': '#5c4030',
        'text-muted': '#8a7060',
        'text-on-stop': '#1a1200',
        'text-on-stop-secondary': '#5a4a30',
        'stop-active-text': '#1a1200',
        'stop-active-text-secondary': '#4a3800',
        'text-on-division': '#2c1810',
        'text-on-primary': '#ffffff',
        'stop-porcelain': '#f5f0e0',
        'stop-porcelain-border': '#c8b898',
        'stop-active': '#e8c820',
        'stop-active-border': '#c8a810',
        'gold-border': '#d4af37',
        'success': '#4caf50',
        'warning': '#ff9800',
        'error': '#c44030',
        'led-green': '#4ade80',
        'led-blue': '#4a90d9',
        'midi-indicator': '#6495ed',
        'meter-bar-bg': '#d8d0c0',
        'scrollbar-thumb': '#c8b898',
        'menu-bg': '#2a2a2a',
        'menu-text': '#cccccc',
        'nameplate-frame-mid': '#1a1510',
        'nameplate-face-mid': '#f0ebe0',
        'nameplate-text': '#1a1008',
      },
      font: { family: 'Georgia', size: '0.8rem', weight: '500' },
      texture: null,
    },
    klassiek_hout: {
      name: 'Klassiek hout (Eiken)',
      colors: {
        'primary': '#a8732b',
        'primary-hover': '#c89548',
        'primary-dark': '#80531a',
        'bg-darkest': '#3a2a1a',
        'bg-dark': '#4a3826',
        'bg-panel': '#5a4530',
        'bg-elevated': '#6a553e',
        'bg-control': '#2a1f15',
        'bg-hover': '#3a2d20',
        'text': '#f0e0c8',
        'text-secondary': '#d8c0a0',
        'text-muted': '#a89878',
        'text-on-stop': '#2a1f10',
        'text-on-stop-secondary': '#5a4a30',
        'stop-active-text': '#2a1f10',
        'stop-active-text-secondary': '#5a4020',
        'text-on-division': '#2c1810',
        'text-on-primary': '#ffffff',
        'stop-porcelain': '#ede2c8',
        'stop-porcelain-border': '#a89070',
        'stop-active': '#e8a830',
        'stop-active-border': '#b88820',
        'gold-border': '#c89548',
        'success': '#7bc878',
        'warning': '#e8a040',
        'error': '#c84040',
        'led-green': '#88e898',
        'led-blue': '#7ab0d8',
        'midi-indicator': '#88a8d8',
        'meter-bar-bg': '#8a7050',
        'scrollbar-thumb': '#8a6a48',
        'menu-bg': '#2a1f15',
        'menu-text': '#e0d0b0',
        'nameplate-frame-mid': '#1a0f08',
        'nameplate-face-mid': '#d8c098',
        'nameplate-text': '#2a1808',
      },
      font: { family: 'Garamond', size: '0.8rem', weight: '500' },
      texture: null,
    },
    modern: {
      name: 'Modern minimalistisch',
      colors: {
        'primary': '#3b82f6',
        'primary-hover': '#60a5fa',
        'primary-dark': '#2563eb',
        'bg-darkest': '#e2e8f0',
        'bg-dark': '#f1f5f9',
        'bg-panel': '#f8fafc',
        'bg-elevated': '#ffffff',
        'bg-control': '#1e293b',
        'bg-hover': '#334155',
        'text': '#0f172a',
        'text-secondary': '#475569',
        'text-muted': '#94a3b8',
        'text-on-stop': '#0f172a',
        'text-on-stop-secondary': '#475569',
        'stop-active-text': '#ffffff',
        'stop-active-text-secondary': '#dbeafe',
        'text-on-division': '#ffffff',
        'text-on-primary': '#ffffff',
        'stop-porcelain': '#f8fafc',
        'stop-porcelain-border': '#cbd5e1',
        'stop-active': '#3b82f6',
        'stop-active-border': '#1d4ed8',
        'gold-border': '#3b82f6',
        'success': '#10b981',
        'warning': '#f59e0b',
        'error': '#ef4444',
        'led-green': '#10b981',
        'led-blue': '#3b82f6',
        'midi-indicator': '#8b5cf6',
        'meter-bar-bg': '#cbd5e1',
        'scrollbar-thumb': '#94a3b8',
        'menu-bg': '#1e293b',
        'menu-text': '#e2e8f0',
        'nameplate-frame-mid': '#0f172a',
        'nameplate-face-mid': '#f8fafc',
        'nameplate-text': '#0f172a',
      },
      font: { family: 'Inter', size: '0.8rem', weight: '500' },
      texture: null,
    },
    nacht: {
      name: 'Nacht (Donker)',
      colors: {
        'primary': '#a08828',
        'primary-hover': '#c8a838',
        'primary-dark': '#806818',
        'bg-darkest': '#0a0a0d',
        'bg-dark': '#14141a',
        'bg-panel': '#1c1c24',
        'bg-elevated': '#26262e',
        'bg-control': '#08080a',
        'bg-hover': '#2e2e36',
        'text': '#e8e4dc',
        'text-secondary': '#b8b0a0',
        'text-muted': '#807868',
        'text-on-stop': '#1a1200',
        'text-on-stop-secondary': '#5a4030',
        'stop-active-text': '#1a1200',
        'stop-active-text-secondary': '#4a3000',
        'text-on-division': '#1a1008',
        'text-on-primary': '#ffffff',
        'stop-porcelain': '#e8dcc0',
        'stop-porcelain-border': '#a89878',
        'stop-active': '#c8a810',
        'stop-active-border': '#a08808',
        'gold-border': '#a08828',
        'success': '#66bb6a',
        'warning': '#ffa726',
        'error': '#ef5350',
        'led-green': '#66e088',
        'led-blue': '#5da0e8',
        'midi-indicator': '#7a98ea',
        'meter-bar-bg': '#3a3a44',
        'scrollbar-thumb': '#48485a',
        'menu-bg': '#1c1c24',
        'menu-text': '#e0e0e0',
        'nameplate-frame-mid': '#000000',
        'nameplate-face-mid': '#d0c8b8',
        'nameplate-text': '#0a0500',
      },
      font: { family: 'Georgia', size: '0.8rem', weight: '500' },
      texture: null,
    },
    avond: {
      name: 'Avond (Warm donker)',
      colors: {
        'primary': '#d4a050',
        'primary-hover': '#e8b870',
        'primary-dark': '#a87830',
        'bg-darkest': '#1a1410',
        'bg-dark': '#241c16',
        'bg-panel': '#2e261e',
        'bg-elevated': '#3a3026',
        'bg-control': '#100a06',
        'bg-hover': '#3e3428',
        'text': '#f0e0c8',
        'text-secondary': '#d0b898',
        'text-muted': '#988068',
        'text-on-stop': '#2a1808',
        'text-on-stop-secondary': '#604830',
        'stop-active-text': '#2a1808',
        'stop-active-text-secondary': '#5a3818',
        'text-on-division': '#2a1808',
        'text-on-primary': '#1a0a00',
        'stop-porcelain': '#f0d8a8',
        'stop-porcelain-border': '#a88858',
        'stop-active': '#e8a838',
        'stop-active-border': '#a87820',
        'gold-border': '#d4a050',
        'success': '#80c060',
        'warning': '#e89030',
        'error': '#d05848',
        'led-green': '#90d878',
        'led-blue': '#80b8e0',
        'midi-indicator': '#a098e8',
        'meter-bar-bg': '#5a4830',
        'scrollbar-thumb': '#705a40',
        'menu-bg': '#2e261e',
        'menu-text': '#e8d8b8',
        'nameplate-frame-mid': '#0a0500',
        'nameplate-face-mid': '#e0c898',
        'nameplate-text': '#1a0a00',
      },
      font: { family: 'Palatino Linotype', size: '0.8rem', weight: '500' },
      texture: null,
    },
  };

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
    try {
      const savedColors = localStorage.getItem('jm-orgue-colors');
      if (savedColors) colors = { ...PRESETS.basis.colors, ...JSON.parse(savedColors) };
    } catch (e) {}
    try {
      const savedFont = localStorage.getItem('jm-orgue-font');
      if (savedFont) font = { ...PRESETS.basis.font, ...JSON.parse(savedFont) };
    } catch (e) {}
    try {
      const savedTextures = localStorage.getItem('jm-orgue-textures');
      if (savedTextures) textures = { background: null, ...JSON.parse(savedTextures) };
    } catch (e) {}
    try {
      const savedActive = localStorage.getItem('jm-orgue-active-preset');
      if (savedActive) activePresetKey = savedActive;
    } catch (e) {}
    try {
      const savedUser = localStorage.getItem('jm-orgue-user-presets');
      if (savedUser) userPresets = JSON.parse(savedUser);
    } catch (e) {}

    applyStyles();
  });

  // ============================================================
  // STYLE TOEPASSING
  // ============================================================
  function applyStyles() {
    const root = document.documentElement;

    // Achtergrond textuur
    if (textures.background) {
      document.body.style.backgroundImage = `url("${textures.background}")`;
      document.body.style.backgroundSize = 'cover';
      document.body.style.backgroundPosition = 'center';
      document.body.style.backgroundAttachment = 'fixed';
    } else {
      document.body.style.backgroundImage = 'none';
      document.body.style.backgroundColor = colors['bg-dark'] || '#f0ebe0';
    }

    // Alle kleur tokens — direct 1-op-1
    for (const [key, value] of Object.entries(colors)) {
      if (value) root.style.setProperty(`--${key}`, value);
    }

    // Afgeleide tokens (alpha varianten van accent)
    if (colors['gold-border']) {
      root.style.setProperty('--gold-shadow', hexToRgba(colors['gold-border'], 0.3));
      root.style.setProperty('--accent-soft', hexToRgba(colors['gold-border'], 0.15));
      root.style.setProperty('--accent-soft-2', hexToRgba(colors['gold-border'], 0.20));
      root.style.setProperty('--accent-soft-hover', hexToRgba(colors['gold-border'], 0.08));
    }
    if (colors['primary']) {
      root.style.setProperty('--gold-gradient',
        `linear-gradient(135deg, ${colors['gold-border'] || colors['primary']} 0%, ${adjustLightness(colors['primary'], 18)} 30%, ${colors['gold-border'] || colors['primary']} 50%, ${adjustLightness(colors['primary'], -10)} 100%)`
      );
    }
    if (colors['led-green']) {
      root.style.setProperty('--led-green-glow', hexToRgba(colors['led-green'], 0.4));
    }

    // Naamplaatje afgeleide kleuren (light/dark variants van mid)
    if (colors['nameplate-frame-mid']) {
      root.style.setProperty('--nameplate-frame-start', adjustLightness(colors['nameplate-frame-mid'], 8));
      root.style.setProperty('--nameplate-frame-end', adjustLightness(colors['nameplate-frame-mid'], 14));
    }
    if (colors['nameplate-face-mid']) {
      root.style.setProperty('--nameplate-face-start', adjustLightness(colors['nameplate-face-mid'], 5));
      root.style.setProperty('--nameplate-face-end', adjustLightness(colors['nameplate-face-mid'], -4));
      root.style.setProperty('--nameplate-face-border', adjustLightness(colors['nameplate-face-mid'], -15));
    }

    // Menu hover varianten
    if (colors['menu-bg']) {
      root.style.setProperty('--menu-border', adjustLightness(colors['menu-bg'], 18));
      root.style.setProperty('--menu-hover-bg', adjustLightness(colors['menu-bg'], 12));
    }
    if (colors['menu-text']) {
      root.style.setProperty('--menu-hover-text', adjustLightness(colors['menu-text'], 15));
    }

    // Scrollbar hover = primary
    if (colors['primary']) {
      root.style.setProperty('--scrollbar-thumb-hover', colors['primary']);
    }

    // Font tokens
    root.style.setProperty('--stop-font-family', `'${font.family}', serif`);
    root.style.setProperty('--stop-font-size', font.size);
    root.style.setProperty('--stop-font-weight', font.weight);
  }

  // ============================================================
  // KLEUR HELPERS
  // ============================================================
  function hexToRgba(hex, alpha) {
    if (!hex || !hex.startsWith('#')) return `rgba(0,0,0,${alpha})`;
    const h = hex.length === 4
      ? '#' + hex[1] + hex[1] + hex[2] + hex[2] + hex[3] + hex[3]
      : hex;
    const r = parseInt(h.slice(1, 3), 16);
    const g = parseInt(h.slice(3, 5), 16);
    const b = parseInt(h.slice(5, 7), 16);
    return `rgba(${r}, ${g}, ${b}, ${alpha})`;
  }

  function adjustLightness(hex, percent) {
    if (!hex || !hex.startsWith('#')) return hex;
    const h = hex.length === 4
      ? '#' + hex[1] + hex[1] + hex[2] + hex[2] + hex[3] + hex[3]
      : hex;
    let r = parseInt(h.slice(1, 3), 16);
    let g = parseInt(h.slice(3, 5), 16);
    let b = parseInt(h.slice(5, 7), 16);
    const delta = percent * 2.55;
    r = Math.min(255, Math.max(0, r + delta));
    g = Math.min(255, Math.max(0, g + delta));
    b = Math.min(255, Math.max(0, b + delta));
    return '#' + [r, g, b].map(x => Math.round(x).toString(16).padStart(2, '0')).join('');
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
        Prestant 8' &bull; Holpijp 8' &bull; Tremulant
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
