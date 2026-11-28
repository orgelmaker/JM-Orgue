import test from 'node:test';
import assert from 'node:assert/strict';
import { snapConsoleElement } from '../ui/src/lib/consoleSnap.js';

const page = { width: 1200, height: 760 };
const box = (id, x, y, width = 120, height = 92) => ({ id, kind: 'stop', x, y, width, height });

test('snaps like edges and centers and returns bounded guide extents', () => {
  const first = box('a', 204, 304), peer = box('b', 200, 80);
  const result = snapConsoleElement(first, page, [peer]);
  assert.equal(result.element.x, 200);
  assert(result.guides.some(g => g.axis === 'x' && g.position === 200));
  const center = snapConsoleElement(box('a', 207, 200, 100), page, [box('b', 200, 80, 120)], { grid: 1 });
  assert.equal(center.element.x, 210);
  assert(center.guides.some(g => g.axis === 'x' && g.position === 260));
  assert(result.guides.every(g => g.from >= 0 && g.to <= (g.axis === 'x' ? page.height : page.width)));
});

test('page edges and page centers attract independently on each axis', () => {
  const center = snapConsoleElement(box('a', 544, 338), page);
  assert.equal(center.element.x, 540); assert.equal(center.element.y, 334);
  assert.deepEqual(center.guides.map(g => g.position), [600, 380]);
  const corner = snapConsoleElement(box('a', 1076, 665), page);
  assert.equal(corner.element.x, 1080); assert.equal(corner.element.y, 668);
  assert(corner.guides.some(g => g.axis === 'x' && g.position === 1200));
});

test('tolerance stays six physical pixels across zoom levels', () => {
  const input = box('a', 210, 300), peer = box('b', 200, 80);
  assert.equal(snapConsoleElement(input, page, [peer], { scale: 0.5, grid: 1 }).element.x, 200);
  assert.equal(snapConsoleElement(input, page, [peer], { scale: 1, grid: 1 }).element.x, 210);
});

test('Alt is temporary free movement; disabling magnets keeps grid fallback', () => {
  const input = box('a', 203, 307), peer = box('b', 200, 80);
  const alt = snapConsoleElement(input, page, [peer], { altKey: true });
  assert.equal(alt.element.x, 203); assert.equal(alt.element.y, 307); assert.deepEqual(alt.guides, []);
  const disabled = snapConsoleElement(input, page, [peer], { enabled: false });
  assert.equal(disabled.element.x, 200); assert.equal(disabled.element.y, 304); assert.deepEqual(disabled.guides, []);
});

test('resize snaps right and bottom edges without shifting the fixed origin', () => {
  const input = box('a', 40, 50, 157, 147), peer = box('b', 100, 100, 100, 100);
  const result = snapConsoleElement(input, page, [peer], { mode: 'resize', grid: 1 });
  assert.equal(result.element.x, 40); assert.equal(result.element.y, 50);
  assert.equal(result.element.width, 160); assert.equal(result.element.height, 150);
  assert(result.guides.some(g => g.position === 200));
});

test('clamping, no peers and no matching magnets never mutate inputs', () => {
  const input = box('a', 1190, -5, 120, 92), peer = box('b', 45, 90);
  const original = structuredClone(input), other = structuredClone(peer);
  const result = snapConsoleElement(input, page, [input, peer], { altKey: true });
  assert.equal(result.element.x, 1080); assert.equal(result.element.y, 0);
  assert.notEqual(result.element, input); assert.deepEqual(input, original); assert.deepEqual(peer, other);
  const resize = snapConsoleElement(box('a', 1100, 700, 1000, 1000), page, [], { mode: 'resize' });
  assert.equal(resize.element.x, 1100); assert.equal(resize.element.y, 700);
  assert(resize.element.x + resize.element.width <= page.width);
  assert(resize.element.y + resize.element.height <= page.height);
  const tiny = snapConsoleElement({ ...box('a', 0, 0, 1, 1), kind: 'label' }, page, [], { mode: 'resize', altKey: true });
  assert.equal(tiny.element.width, 16); assert.equal(tiny.element.height, 16);
});

test('a row is recognised as an array and a dragged control snaps to its spacing', async () => {
  const { consoleArray, alignConsoleArray } = await import('../ui/src/lib/consoleSnap.js');
  const row = [box('a', 40, 100), box('b', 180, 100), box('c', 320, 100)];
  const other = box('d', 40, 300), title = { ...box('t', 40, 96, 400, 30), kind: 'label' };
  const dragged = box('n', 463, 103);
  assert.deepEqual(consoleArray(dragged, [...row, other, title], 'x').map(e => e.id), ['a', 'b', 'c', 'n']);
  assert.deepEqual(consoleArray(row[0], [...row, other], 'y').map(e => e.id), ['a', 'd']);
  // 20 px between the neighbours, so the fourth control lands at 320 + 120 + 20.
  const snapped = snapConsoleElement(dragged, page, [...row, other, title], { grid: 1 });
  assert.equal(snapped.element.x, 460); assert.equal(snapped.element.y, 100);
  const left = snapConsoleElement(box('n', 57, 100, 100), page, [box('b', 180, 100), box('c', 320, 100)], { grid: 1 });
  assert.equal(left.element.x, 60);
  // Alt keeps free movement; too far away does not snap to the spacing.
  assert.equal(snapConsoleElement(dragged, page, row, { grid: 1, altKey: true }).element.x, 463);
  assert.equal(snapConsoleElement(box('n', 480, 100), page, row, { grid: 1 }).element.x, 480);
  // Aligning puts the row on the selected control's line with equal gaps and keeps the outer two.
  const messy = [box('a', 40, 100), box('b', 185, 108), box('c', 290, 95), box('e', 480, 102)];
  const patches = alignConsoleArray(messy[1], messy, page, 'x');
  assert.deepEqual(Object.keys(patches).sort(), ['a', 'b', 'c', 'e']);
  assert.equal(patches.a.x, 40); assert(Math.abs(patches.e.x - 480) <= 2);
  assert.equal(patches.b.x - (patches.a.x + 120), patches.c.x - (patches.b.x + 120));
  assert.equal(patches.c.x - (patches.b.x + 120), patches.e.x - (patches.c.x + 120));
  for (const id of ['a', 'b', 'c', 'e']) assert.equal(patches[id].y, 108);
  assert.equal(alignConsoleArray(other, [other, title], page, 'x'), null);
  // Overlapping controls are put side by side and never leave the page.
  const stacked = [box('a', 1000, 100), box('b', 1010, 100), box('c', 1020, 100)];
  const fixed = alignConsoleArray(stacked[0], stacked, page, 'x');
  assert(fixed.c.x + 120 <= page.width); assert(fixed.b.x >= fixed.a.x + 120);
});
