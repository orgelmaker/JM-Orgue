// A console design is organ data. This module never writes browser storage or
// changes sounding controls; only the parent commits a draft to the backend.
export const CONSOLE_LAYOUT_VERSION = 1;
export const CONSOLE_MAX_PAGES = 64;
// De laatste vier zijn effen orgelkleuren (geschilderde kasten): wijnrood, roomwit, groen en grijsblauw.
export const CONSOLE_THEMES = ['walnut', 'ivory', 'slate', 'burgundy', 'cream', 'green', 'blue'];
export const CONSOLE_APPEARANCES = ['rocker', 'rocker-ivory', 'rocker-wood', 'drawknob', 'drawknob-ivory', 'piston', 'tablet', 'flat',
  'drawknob-ebony', 'drawknob-rosewood', 'oval-porcelain', 'stopkey', 'lit', 'square-wood'];
export const CONSOLE_TEXTURES = ['grain', 'oak', 'burl', 'ebony', 'linen', 'brushed', 'plain', 'custom'];
export const CONSOLE_TEXTURE_MAX_BYTES = 2 * 1024 * 1024;
const MAX_TEXTURE_BASE64 = Math.ceil(CONSOLE_TEXTURE_MAX_BYTES / 3) * 4;
const TEXTURE_BASE64_ALPHABET = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/';
let lastTextureDataUrl = null, lastTextureError = null;

// Strings are immutable. Reusing them keeps a custom image out of the many
// temporary JSON encode/decode buffers otherwise created by Undo snapshots.
// Objects (including the image metadata) and arrays remain independent copies.
export const cloneConsoleLayout = value => {
  if (value == null) return null;
  const clone = node => Array.isArray(node) ? node.map(clone) : node && typeof node === 'object'
    ? Object.fromEntries(Object.entries(node).map(([key, child]) => [key, clone(child)])) : node;
  return clone(value);
};

const consoleText = (s, min, max) => typeof s === 'string' && (min === 0 || s.trim().length > 0) &&
  [...s].length <= max && !/[\u0000-\u0009\u000b-\u001f\u007f-\u009f]/.test(s);

export function consoleCustomTextureError(image) {
  if (!image || typeof image !== 'object' || Array.isArray(image) || !consoleText(image.name, 0, 120) ||
    typeof image.data_url !== 'string') return 'texture-format';
  // A draft move must not decode the same 2 MiB image on every pointer event.
  // Cache only one immutable URL; metadata is validated anew on each call.
  if (image.data_url === lastTextureDataUrl) return lastTextureError;
  lastTextureDataUrl = image.data_url;
  lastTextureError = textureDataError(image.data_url);
  return lastTextureError;
}

function textureDataError(dataUrl) {
  const comma = dataUrl.indexOf(',');
  const prefix = dataUrl.slice(0, comma);
  if (!['data:image/png;base64', 'data:image/jpeg;base64', 'data:image/webp;base64'].includes(prefix)) return 'texture-format';
  const encoded = dataUrl.slice(comma + 1);
  if (encoded.length > MAX_TEXTURE_BASE64) return 'texture-size';
  // atob alone accepts noncanonical padding and whitespace. Match the backend's
  // strict standard base64 alphabet, complete groups and zero trailing bits.
  if (!encoded.length || encoded.length % 4 !== 0 || !/^[A-Za-z0-9+/]*={0,2}$/.test(encoded)) return 'texture-data';
  const padding = encoded.endsWith('==') ? 2 : encoded.endsWith('=') ? 1 : 0;
  const last = TEXTURE_BASE64_ALPHABET.indexOf(encoded[encoded.length - padding - 1]);
  if ((padding === 2 && (last & 15) !== 0) || (padding === 1 && (last & 3) !== 0)) return 'texture-data';
  if (encoded.length / 4 * 3 - padding > CONSOLE_TEXTURE_MAX_BYTES) return 'texture-size';
  let bytes;
  try { bytes = atob(encoded); } catch { return 'texture-data'; }
  const starts = values => values.every((value, index) => bytes.charCodeAt(index) === value);
  const valid = prefix === 'data:image/png;base64' ? starts([137, 80, 78, 71, 13, 10, 26, 10])
    : prefix === 'data:image/jpeg;base64' ? starts([255, 216, 255])
      : starts([82, 73, 70, 70]) && bytes.slice(8, 12) === 'WEBP';
  return valid ? null : 'texture-data';
}

export const roundConsoleAppearance = appearance => ['drawknob', 'drawknob-ivory', 'piston', 'drawknob-ebony', 'drawknob-rosewood'].includes(appearance);

export function consoleTargets(organInfo) {
  const result = [];
  for (const division of organInfo?.divisions || []) {
    for (const stop of division.stops || []) result.push({
      kind: 'stop', id: stop.id, name: stop.name, pitch: stop.pitch || '',
      division: division.display_name || division.name, divisionName: division.name, group: stop.groep || '',
      active: !!stop.drawn, source: stop,
    });
  }
  for (const coupler of organInfo?.couplers || []) result.push({
    kind: 'coupler', id: coupler.id, name: (coupler.name || '').replace(/\n/g, ' '),
    pitch: '', division: coupler.display_in_division || coupler.destination_division || '',
    group: 'coupler', active: !!coupler.active, source: coupler,
  });
  return result;
}

export function consoleTargetMap(organInfo) {
  return new Map(consoleTargets(organInfo).map(t => [`${t.kind}:${t.id}`, t]));
}

export function missingConsoleTargets(layout, organInfo) {
  const targets = consoleTargetMap(organInfo);
  return (layout?.pages || []).flatMap(page => (page.elements || []).filter(e =>
    e.kind !== 'label' && !targets.has(`${e.kind}:${e.target_id}`),
  ).map(element => ({ pageId: page.id, element })));
}

export function consoleLayoutError(layout) {
  if (!layout || layout.version !== CONSOLE_LAYOUT_VERSION) return 'version';
  if (typeof layout.enabled !== 'boolean' || !CONSOLE_THEMES.includes(layout.theme)) return 'format';
  const text = consoleText;
  const num = (n, min, max) => Number.isFinite(n) && n >= min && n <= max;
  if (!text(layout.name, 0, 120) || !Array.isArray(layout.pages) || !num(layout.pages.length, 1, CONSOLE_MAX_PAGES)) return 'format';
  const texture = layout.texture === undefined ? 'grain' : layout.texture;
  if (!CONSOLE_TEXTURES.includes(texture) ||
    (layout.texture_opacity !== undefined && !num(layout.texture_opacity, 0, 1)) ||
    (layout.texture_scale !== undefined && !num(layout.texture_scale, 25, 400))) return 'texture-format';
  if (layout.custom_texture != null) {
    const error = consoleCustomTextureError(layout.custom_texture);
    if (error) return error;
  } else if (texture === 'custom') return 'texture-format';
  const pageIds = new Set(), elementIds = new Set();
  let total = 0;
  for (const page of layout.pages) {
    if (!text(page.id, 1, 128) || pageIds.has(page.id) || !text(page.name, 0, 120) ||
      !num(page.width, 640, 2400) || !num(page.height, 400, 1600) || !Array.isArray(page.elements) || page.elements.length > 512) return 'page';
    pageIds.add(page.id);
    for (const e of page.elements) {
      if (!text(e.id, 1, 128) || elementIds.has(e.id) || !['stop', 'coupler', 'label'].includes(e.kind) ||
        !CONSOLE_APPEARANCES.includes(e.appearance) || !text(e.label, 0, 160) ||
        !num(e.x, 0, page.width) || !num(e.y, 0, page.height) ||
        !num(e.width, e.kind === 'label' ? 16 : 48, page.width) ||
        !num(e.height, e.kind === 'label' ? 16 : 40, page.height) ||
        e.x + e.width > page.width || e.y + e.height > page.height ||
        (e.kind === 'label' ? e.target_id !== null : !text(e.target_id, 1, 256))) return 'element';
      elementIds.add(e.id);
      total++;
    }
  }
  return total > 4096 ? 'format' : null;
}

// Unknown future versions and malformed designs are never silently rewritten.
export function normalizeConsoleLayout(layout) {
  return consoleLayoutError(layout) ? null : cloneConsoleLayout(layout);
}

export function nextConsoleId(layout, prefix = 'element') {
  const used = new Set((layout?.pages || []).flatMap(p => [p.id, ...p.elements.map(e => e.id)]));
  let n = 1;
  while (used.has(`${prefix}-${n}`)) n++;
  return `${prefix}-${n}`;
}

export function boundedConsoleElement(element, page, patch = {}, snap = 1) {
  const e = { ...element, ...patch };
  const numeric = (n, fallback) => Number.isFinite(Number(n)) ? Number(n) : fallback;
  const minW = e.kind === 'label' ? 16 : 48, minH = e.kind === 'label' ? 16 : 40;
  e.width = Math.max(minW, Math.min(page.width, numeric(e.width, element.width)));
  e.height = Math.max(minH, Math.min(page.height, numeric(e.height, element.height)));
  if (patch.appearance !== undefined && patch.appearance !== element.appearance && roundConsoleAppearance(e.appearance)) {
    e.width = e.height = Math.max(minW, minH, Math.min(e.width, e.height, page.width, page.height));
  }
  const grid = Number.isFinite(snap) && snap > 0 ? snap : 1;
  e.x = Math.max(0, Math.min(page.width - e.width, Math.round(numeric(e.x, element.x) / grid) * grid));
  e.y = Math.max(0, Math.min(page.height - e.height, Math.round(numeric(e.y, element.y) / grid) * grid));
  return e;
}

export function defaultConsoleLayout(organInfo, { name = organInfo?.name || 'Console', pageName = 'Page', couplersName = 'Couplers', appearance = 'rocker' } = {}) {
  const layout = { version: 1, enabled: true, name: name.slice(0, 120), theme: 'walnut', pages: [] };
  const targets = consoleTargets(organInfo);
  const groups = [];
  for (const division of organInfo?.divisions || []) {
    const stops = targets.filter(t => t.kind === 'stop' && t.divisionName === division.name);
    if (stops.length) groups.push({ name: division.display_name || division.name, targets: stops });
  }
  const couplers = targets.filter(t => t.kind === 'coupler');
  if (couplers.length) groups.push({ name: couplersName, targets: couplers });
  let page, y;
  const newPage = () => {
    if (layout.pages.length >= CONSOLE_MAX_PAGES) return false;
    page = { id: `page-${layout.pages.length + 1}`, name: `${pageName} ${layout.pages.length + 1}`, width: 1200, height: 760, elements: [] };
    layout.pages.push(page); y = 34; return true;
  };
  newPage();
  for (const group of groups) {
    for (let index = 0; index < group.targets.length;) {
      if (y + 132 > page.height - 25 && !newPage()) return layout;
      page.elements.push({ id: nextConsoleId(layout, 'title'), kind: 'label', target_id: null,
        label: group.name.slice(0, 160), x: 42, y, width: 1100, height: 30, appearance: 'flat' });
      y += 44;
      while (index < group.targets.length && y + 92 <= page.height - 25) {
        const row = group.targets.slice(index, index + 8);
        row.forEach((target, col) => page.elements.push({
          id: nextConsoleId(layout, 'control'), kind: target.kind, target_id: target.id,
          label: '', x: 42 + col * 140, y, width: roundConsoleAppearance(appearance) ? 92 : 120, height: 92, appearance,
        }));
        index += row.length; y += 112;
      }
      y += 14;
    }
  }
  return layout;
}
