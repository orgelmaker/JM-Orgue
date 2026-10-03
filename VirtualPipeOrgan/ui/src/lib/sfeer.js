// Sfeer: kleuren, lettertype en achtergrondtextuur uit Sfeer & Layout.
//
// Tot 0.7.67 werden de kleurtokens uit localStorage alleen toegepast in
// LayoutSettings.onMount, dus pas nadat Algemene instellingen één keer open
// was geweest. Tot die tijd stond elk venster op de :root-standaarden uit
// styles.css; extra registerschermen en het notatievenster kregen de eigen
// sfeer nooit. Nu past App.svelte hem in elk venster vóór de eerste render
// toe, en de 1-seconde-poll in Console (refreshSharedPrefs) volgt
// wijzigingen uit een ander venster. Storage-events zijn tussen
// WebView2-vensters onbetrouwbaar, vandaar een poll met een
// verander-guard: een ongewijzigde sfeer doet niets.

export const PRESETS = {
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
      // Registertinten (0.7.69): letters per groep, alleen met de schakelaar aan.
      'stop-tint-grond': '#1a1200',
      'stop-tint-vul': '#1f4e8c',
      'stop-tint-tong': '#a61c1c',
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
      // Registertinten (0.7.69): letters per groep, alleen met de schakelaar aan.
      'stop-tint-grond': '#2a1f10',
      'stop-tint-vul': '#1f4e8c',
      'stop-tint-tong': '#a61c1c',
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
      // Registertinten (0.7.69): letters per groep, alleen met de schakelaar aan.
      'stop-tint-grond': '#0f172a',
      'stop-tint-vul': '#1f4e8c',
      'stop-tint-tong': '#a61c1c',
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
      // Registertinten (0.7.69): letters per groep, alleen met de schakelaar aan.
      'stop-tint-grond': '#1a1200',
      'stop-tint-vul': '#1f4e8c',
      'stop-tint-tong': '#a61c1c',
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
      // Registertinten (0.7.69): letters per groep, alleen met de schakelaar aan.
      'stop-tint-grond': '#2a1808',
      'stop-tint-vul': '#1f4e8c',
      'stop-tint-tong': '#a61c1c',
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

export const SFEER_SLEUTELS = {
  colors: 'jm-orgue-colors',
  font: 'jm-orgue-font',
  textures: 'jm-orgue-textures',
  activePreset: 'jm-orgue-active-preset',
  userPresets: 'jm-orgue-user-presets',
  // Schakelaar voor de registertinten (0.7.69); apart van colors, anders zou
  // elk preset hem resetten.
  stopTinten: 'jm-orgue-stop-tinten',
};

function leesJson(sleutel) {
  try {
    const v = localStorage.getItem(sleutel);
    return v ? JSON.parse(v) : null;
  } catch (e) {
    return null;
  }
}

// Alleen een echt object telt; een string, getal of array uit een handmatig
// bewerkte opslag valt terug op leeg (anders struikelt de markup van
// LayoutSettings over Object.entries).
function obj(v) {
  return v && typeof v === 'object' && !Array.isArray(v) ? v : {};
}

/** Bewaarde sfeer, met de basis-preset als terugval voor ontbrekende tokens. */
export function leesSfeer() {
  const colors = { ...PRESETS.basis.colors, ...obj(leesJson(SFEER_SLEUTELS.colors)) };
  const font = { ...PRESETS.basis.font, ...obj(leesJson(SFEER_SLEUTELS.font)) };
  const textures = { background: null, ...obj(leesJson(SFEER_SLEUTELS.textures)) };
  let activePresetKey = 'basis';
  try { activePresetKey = localStorage.getItem(SFEER_SLEUTELS.activePreset) || 'basis'; } catch (e) {}
  const userPresets = obj(leesJson(SFEER_SLEUTELS.userPresets));
  let stopTinten = false;
  try { stopTinten = localStorage.getItem(SFEER_SLEUTELS.stopTinten) === '1'; } catch (e) {}
  return { colors, font, textures, activePresetKey, userPresets, stopTinten };
}

/** Zet de tokens op :root en de achtergrond op body. */
export function pasSfeerToe({ colors, font, textures, stopTinten }) {
  const root = document.documentElement;

  // Registertinten aan/uit (styles.css kijkt naar :root[data-stop-tinten]).
  root.dataset.stopTinten = stopTinten ? '1' : '0';

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
    if (value && typeof value === 'string') root.style.setProperty(`--${key}`, value);
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

  // Registertinten: op het porseleinen plaatje en op het (gele) plaatje van
  // een getrokken register wordt de tint zo nodig donkerder of lichter
  // gemaakt tot hij leesbaar is (4,5:1); lukt dat niet (Modern: donkerblauw
  // vlak), dan de gewone tekstkleur. Berekend, dus ook juist bij eigen
  // kleuren.
  for (const g of ['grond', 'vul', 'tong']) {
    const tint = colors['stop-tint-' + g];
    if (typeof tint !== 'string') continue;
    root.style.setProperty(`--stop-tint-${g}`, leesbaar(tint, colors['stop-porcelain'], colors['text-on-stop']));
    root.style.setProperty(`--stop-tint-${g}-actief`, leesbaar(tint, colors['stop-active'], colors['stop-active-text']));
  }

  // Font tokens
  root.style.setProperty('--stop-font-family', `'${font.family}', serif`);
  root.style.setProperty('--stop-font-size', font.size);
  root.style.setProperty('--stop-font-weight', font.weight);
}

// Vingerafdruk van de drie sleutels die het beeld bepalen; de poll past de
// sfeer alleen opnieuw toe als die verandert.
let laatsteVingerafdruk = null;

function vingerafdruk() {
  try {
    return [SFEER_SLEUTELS.colors, SFEER_SLEUTELS.font, SFEER_SLEUTELS.textures, SFEER_SLEUTELS.stopTinten]
      .map(k => localStorage.getItem(k) || '')
      .join('\u0000');
  } catch (e) {
    return '';
  }
}

/** Past de bewaarde sfeer toe als die sinds de vorige keer is veranderd. */
export function pasSfeerToeAlsGewijzigd() {
  const v = vingerafdruk();
  if (v === laatsteVingerafdruk) return false;
  laatsteVingerafdruk = v;
  try {
    pasSfeerToe(leesSfeer());
  } catch (e) {
    console.error('Sfeer toepassen mislukt:', e);
  }
  return true;
}

function hexToRgba(hex, alpha) {
  if (typeof hex !== 'string' || !hex.startsWith('#')) return `rgba(0,0,0,${alpha})`;
  const h = hex.length === 4
    ? '#' + hex[1] + hex[1] + hex[2] + hex[2] + hex[3] + hex[3]
    : hex;
  const r = parseInt(h.slice(1, 3), 16);
  const g = parseInt(h.slice(3, 5), 16);
  const b = parseInt(h.slice(5, 7), 16);
  return `rgba(${r}, ${g}, ${b}, ${alpha})`;
}

/** Relatieve luminantie (WCAG) van een #rrggbb-kleur; 0 bij ongeldige invoer. */
function luminantie(hex) {
  if (typeof hex !== 'string' || !hex.startsWith('#')) return 0;
  const h = hex.length === 4
    ? '#' + hex[1] + hex[1] + hex[2] + hex[2] + hex[3] + hex[3]
    : hex;
  const kanaal = (i) => {
    const c = parseInt(h.slice(i, i + 2), 16) / 255;
    return c <= 0.03928 ? c / 12.92 : Math.pow((c + 0.055) / 1.055, 2.4);
  };
  return 0.2126 * kanaal(1) + 0.7152 * kanaal(3) + 0.0722 * kanaal(5);
}

/** Contrastverhouding (WCAG) tussen twee kleuren, 1..21. */
function contrast(a, b) {
  const la = luminantie(a), lb = luminantie(b);
  const [hoog, laag] = la >= lb ? [la, lb] : [lb, la];
  return (hoog + 0.05) / (laag + 0.05);
}

/**
 * Maak een tint leesbaar op een achtergrond: zolang het contrast onder 4,5
 * blijft, stapsgewijs donkerder (lichte achtergrond) of lichter (donkere
 * achtergrond). Lukt dat niet binnen twaalf stappen, dan de terugvalkleur.
 */
function leesbaar(tint, achtergrond, terugval) {
  if (typeof tint !== 'string' || typeof achtergrond !== 'string') return tint;
  const stap = luminantie(achtergrond) > 0.3 ? -6 : 6;
  let k = tint;
  for (let i = 0; i < 12; i++) {
    if (contrast(k, achtergrond) >= 4.5) return k;
    k = adjustLightness(k, stap);
  }
  return contrast(k, achtergrond) >= 4.5 ? k : (typeof terugval === 'string' ? terugval : tint);
}

function adjustLightness(hex, percent) {
  if (typeof hex !== 'string' || !hex.startsWith('#')) return hex;
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
