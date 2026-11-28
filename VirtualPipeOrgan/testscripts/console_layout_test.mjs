import test from 'node:test';
import assert from 'node:assert/strict';
import { boundedConsoleElement, cloneConsoleLayout, consoleLayoutError, consoleTargetMap,
  defaultConsoleLayout, missingConsoleTargets, nextConsoleId, normalizeConsoleLayout,
  CONSOLE_APPEARANCES, CONSOLE_TEXTURES, CONSOLE_TEXTURE_MAX_BYTES, consoleCustomTextureError,
  roundConsoleAppearance } from '../ui/src/lib/consoleLayout.js';

const organ = { id: 'same-folder/a.organ', name: 'Organ A', divisions: [
  { name: 'great', display_name: 'Great', stops: [{ id: 's1', name: 'Principal', pitch: "8′", drawn: false }, { id: 's2', name: 'Flute', pitch: "4′", drawn: true }] },
  { name: 'swell', display_name: 'Great', stops: [{ id: 's3', name: 'Trumpet', pitch: "8′", groep: 'tong', drawn: false }] },
], couplers: [{ id: 'c1', name: 'II / I', active: true }] };

test('automatic design includes each stop and coupler once despite duplicate display names', () => {
  const design = defaultConsoleLayout(organ);
  assert.equal(consoleLayoutError(design), null);
  const bindings = design.pages.flatMap(p => p.elements).filter(e => e.kind !== 'label').map(e => e.target_id);
  assert.deepEqual(bindings, ['s1', 's2', 's3', 'c1']);
  assert.equal(new Set(design.pages.flatMap(p => p.elements.map(e => e.id))).size, design.pages.flatMap(p => p.elements).length);
});

test('drafts and two organ definitions are independent copies', () => {
  const first = defaultConsoleLayout(organ), second = defaultConsoleLayout({ ...organ, id: 'same-folder/b.organ' });
  const draft = cloneConsoleLayout(first);
  draft.pages[0].elements[1].label = 'Custom'; draft.theme = 'slate';
  assert.notDeepEqual(draft, first);
  assert.equal(first.theme, 'walnut'); assert.equal(second.theme, 'walnut');
  assert.equal(first.pages[0].elements[1].label, '');
  const normalized = normalizeConsoleLayout(first); normalized.enabled = false;
  assert.equal(first.enabled, true);
});

test('unsupported, duplicate and out of bounds designs are not silently normalized', () => {
  const future = defaultConsoleLayout(organ); future.version = 2;
  assert.equal(normalizeConsoleLayout(future), null); assert.equal(future.version, 2);
  const duplicate = defaultConsoleLayout(organ); duplicate.pages[0].elements[1].id = duplicate.pages[0].elements[0].id;
  assert.equal(normalizeConsoleLayout(duplicate), null);
  const invalid = defaultConsoleLayout(organ); invalid.pages[0].elements[1].x = Infinity;
  assert.equal(normalizeConsoleLayout(invalid), null);
  invalid.pages[0].elements[1].x = 1199; assert.equal(normalizeConsoleLayout(invalid), null);
  assert.equal(normalizeConsoleLayout(null), null);
  const control = defaultConsoleLayout(organ); control.name = 'Bad\u0000name';
  assert.equal(normalizeConsoleLayout(control), null);
  const unicode = defaultConsoleLayout(organ); unicode.name = '🎹'.repeat(120);
  assert.notEqual(normalizeConsoleLayout(unicode), null);
  unicode.name += '🎹'; assert.equal(normalizeConsoleLayout(unicode), null);
});

test('missing references survive normalization and remain detectable after an organ update', () => {
  const design = defaultConsoleLayout(organ);
  const updated = { ...organ, divisions: [] };
  assert.equal(missingConsoleTargets(design, updated).length, 3);
  assert.notEqual(normalizeConsoleLayout(design), null);
  assert.equal(missingConsoleTargets(design, organ).length, 0);
  assert.equal(consoleTargetMap(organ).get('stop:s2').active, true);
  assert.equal(consoleTargetMap(organ).get('coupler:c1').active, true);
});

test('drag and size operations stay inside the page, including small labels', () => {
  const page = { width: 640, height: 400 };
  const e = { id: 'a', kind: 'stop', x: 40, y: 40, width: 120, height: 92 };
  const moved = boundedConsoleElement(e, page, { x: 5000, y: -500 }, 8);
  assert.equal(moved.x, 520); assert.equal(moved.y, 0);
  assert.equal(e.x, 40);
  const resized = boundedConsoleElement(e, page, { width: 10000, height: -1 });
  assert.equal(resized.width, 640); assert.equal(resized.x, 0); assert.equal(resized.height, 40);
  const label = boundedConsoleElement({ ...e, kind: 'label' }, page, { width: 1, height: 1 });
  assert.equal(label.width, 16); assert.equal(label.height, 16);
  assert.equal(boundedConsoleElement(e, page, { x: NaN }).x, 40);
});

test('large automatic designs paginate within all limits and retain stable IDs', () => {
  const huge = { id: 'large', name: 'Large', divisions: [{ name: 'great', stops: Array.from({ length: 250 }, (_, i) => ({ id: `s${i}`, name: `Stop ${i}` })) }] };
  const design = defaultConsoleLayout(huge);
  assert(design.pages.length > 1); assert.equal(consoleLayoutError(design), null);
  assert.equal(design.pages.flatMap(p => p.elements).filter(e => e.kind === 'stop').length, 250);
  assert(!design.pages.some(p => p.elements.some(e => e.x + e.width > p.width || e.y + e.height > p.height)));
  const id = nextConsoleId(design, 'control');
  assert(!design.pages.flatMap(p => p.elements).some(e => e.id === id));
});

test('2048 registers and 512 couplers all remain reachable in a paginated console', () => {
  const large = { id: 'largest-disposition.jmorgan', name: 'Large project', divisions: Array.from({ length: 8 }, (_, division) => ({
    name: `manual-${division}`, stops: Array.from({ length: 256 }, (_, stop) => ({
      id: `project_${division * 256 + stop + 1}`, name: `Register ${division * 256 + stop + 1}`, midi_action_code: 65537 + division * 256 + stop,
    })),
  })), couplers: Array.from({ length: 512 }, (_, index) => ({ id: `coupler-${index}`, name: `Coupler ${index}` })) };
  const design = defaultConsoleLayout(large);
  const controls = design.pages.flatMap(page => page.elements).filter(element => element.kind !== 'label');
  assert(design.pages.length > 8 && design.pages.length <= 64);
  assert.equal(consoleLayoutError(design), null);
  assert.equal(controls.length, 2560);
  assert.equal(new Set(controls.map(control => control.target_id)).size, 2560);
  assert(controls.some(control => control.target_id === 'project_2048'));
  assert(controls.some(control => control.target_id === 'coupler-511'));
  assert.deepEqual(missingConsoleTargets(design, large), []);
});

const texture = (mime, bytes) => ({ name: 'Eigen textuur', data_url: `data:${mime};base64,${Buffer.from(bytes).toString('base64')}` });
const pngHeader = Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]);

test('additive texture options do not rewrite existing version-one designs', () => {
  const old = defaultConsoleLayout(organ);
  const normalized = normalizeConsoleLayout(old);
  assert.deepEqual(normalized, old);
  for (const key of ['texture', 'texture_opacity', 'texture_scale', 'custom_texture']) assert(!Object.hasOwn(normalized, key));
  for (const choice of CONSOLE_TEXTURES.filter(t => t !== 'custom')) {
    assert.equal(consoleLayoutError({ ...old, texture: choice, texture_opacity: 0.65, texture_scale: 100 }), null);
  }
  assert.equal(consoleLayoutError({ ...old, texture: 'custom' }), 'texture-format');
  for (const patch of [{ texture: 'svg' }, { texture_opacity: -0.1 }, { texture_opacity: NaN }, { texture_scale: 401 }, { texture_scale: null }]) {
    assert.equal(normalizeConsoleLayout({ ...old, ...patch }), null);
  }
});

test('custom textures accept only bounded canonical raster data URLs with matching magic', () => {
  const images = [
    { name: 'Pixel.png', data_url: 'data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAusB9Y9Zl1EAAAAASUVORK5CYII=' },
    texture('image/jpeg', [255, 216, 255, 217]),
    texture('image/webp', Buffer.from('RIFF\x04\0\0\0WEBP')),
  ];
  for (const image of images) {
    assert.equal(consoleCustomTextureError(image), null);
    assert.equal(consoleLayoutError({ ...defaultConsoleLayout(organ), texture: 'custom', custom_texture: image }), null);
    assert.equal(consoleLayoutError({ ...defaultConsoleLayout(organ), texture: 'oak', custom_texture: image }), null);
  }
  const invalid = [texture('image/svg+xml', '<svg/>'), texture('image/jpeg', pngHeader),
    { name: 'Bad', data_url: 'data:image/png;base64,iVBORw0KGgo=!' },
    { name: 'Bad', data_url: 'data:image/png;base64,iVBORw0KGgp=' },
    { name: 'Bad', data_url: 'data:image/png;base64,iVBORw0KGgo' },
    { name: 'Bad', data_url: 'data:image/png;base64,iVBORw0KGgo=\n' },
    { name: 'Bad', data_url: 'https://example.org/image.png' },
    texture('image/png', Buffer.alloc(CONSOLE_TEXTURE_MAX_BYTES + 1)),
  ];
  for (const image of invalid) assert.notEqual(consoleCustomTextureError(image), null);
  const boundary = Buffer.alloc(CONSOLE_TEXTURE_MAX_BYTES); pngHeader.copy(boundary);
  assert.equal(consoleCustomTextureError(texture('image/png', boundary)), null);
  const unicode = texture('image/png', pngHeader); unicode.name = '🎹'.repeat(120);
  assert.equal(consoleCustomTextureError(unicode), null); unicode.name += '🎹';
  assert.equal(consoleCustomTextureError(unicode), 'texture-format');
});

test('texture snapshots share immutable image strings and clone all mutable metadata', () => {
  const original = { ...defaultConsoleLayout(organ), texture: 'custom', custom_texture: {
    ...texture('image/png', pngHeader), future: { keep: ['value'] },
  } };
  const draft = cloneConsoleLayout(original);
  assert.deepEqual(draft, original); assert.notEqual(draft, original);
  assert.notEqual(draft.custom_texture, original.custom_texture);
  assert.equal(draft.custom_texture.data_url, original.custom_texture.data_url);
  draft.custom_texture.name = 'Changed'; draft.custom_texture.future.keep[0] = 'changed'; draft.pages[0].name = 'Changed';
  assert.equal(original.custom_texture.name, 'Eigen textuur');
  assert.equal(original.custom_texture.future.keep[0], 'value'); assert.notEqual(draft.pages[0].name, original.pages[0].name);
});

test('all appearances validate and round control changes become square within bounds', () => {
  for (const appearance of CONSOLE_APPEARANCES) {
    const design = defaultConsoleLayout(organ, { appearance });
    assert.equal(consoleLayoutError(design), null, appearance);
    const control = design.pages[0].elements.find(e => e.kind === 'stop');
    if (roundConsoleAppearance(appearance)) assert.equal(control.width, control.height);
  }
  const page = { width: 640, height: 400 }, original = { kind: 'stop', x: 510, y: 308, width: 120, height: 92, appearance: 'rocker' };
  for (const appearance of ['drawknob', 'drawknob-ivory', 'piston']) {
    const changed = boundedConsoleElement(original, page, { appearance });
    assert.equal(changed.width, 92); assert.equal(changed.height, 92);
    assert(changed.x + changed.width <= page.width && changed.y + changed.height <= page.height);
  }
  assert.equal(original.width, 120);
});

test('painted organ colours and the extra control styles are valid, round knobs stay round', async () => {
  const { CONSOLE_THEMES } = await import('../ui/src/lib/consoleLayout.js');
  for (const theme of ['burgundy', 'cream', 'green', 'blue']) assert(CONSOLE_THEMES.includes(theme));
  for (const appearance of ['drawknob-ebony', 'drawknob-rosewood', 'oval-porcelain', 'stopkey', 'lit', 'square-wood'])
    assert(CONSOLE_APPEARANCES.includes(appearance));
  assert(roundConsoleAppearance('drawknob-ebony')); assert(roundConsoleAppearance('drawknob-rosewood'));
  assert(!roundConsoleAppearance('oval-porcelain')); assert(!roundConsoleAppearance('stopkey'));
  const layout = defaultConsoleLayout(organ);
  for (const theme of CONSOLE_THEMES) assert.equal(consoleLayoutError({ ...layout, theme }), null);
  assert.equal(consoleLayoutError({ ...layout, theme: 'purple' }), 'format');
});

test('the screen switcher is optional and off by default', () => {
  const layout = defaultConsoleLayout(organ);
  assert.equal(layout.screen_switcher, undefined);
  assert.equal(consoleLayoutError({ ...layout, screen_switcher: true }), null);
  assert.equal(consoleLayoutError({ ...layout, screen_switcher: 'yes' }), 'format');
});
