import { boundedConsoleElement } from './consoleLayout.js';

// Work in design coordinates, but keep the magnetic tolerance constant on the
// user's screen. Alt bypasses both magnets and the grid for one-pixel work.
export function snapConsoleElement(input, page, others = [], {
  scale = 1, grid = 8, enabled = true, altKey = false, mode = 'move',
} = {}) {
  const unit = Number.isFinite(scale) && scale > 0 ? scale : 1;
  const step = !altKey && Number.isFinite(grid) && grid > 0 ? grid : 1;
  const tolerance = 6 / unit;
  const minWidth = input.kind === 'label' ? 16 : 48, minHeight = input.kind === 'label' ? 16 : 40;
  const originX = Number.isFinite(input.x) ? Math.max(0, Math.min(page.width - minWidth, input.x)) : 0;
  const originY = Number.isFinite(input.y) ? Math.max(0, Math.min(page.height - minHeight, input.y)) : 0;
  const candidate = mode === 'resize' ? boundedConsoleElement({ ...input,
    x: originX, y: originY,
    width: Math.max(minWidth, Math.min(page.width - originX, Number.isFinite(input.width) ? input.width : minWidth)),
    height: Math.max(minHeight, Math.min(page.height - originY, Number.isFinite(input.height) ? input.height : minHeight)),
  }, page) : boundedConsoleElement(input, page);
  const peers = others.filter(e => e.id !== input.id &&
    [e.x, e.y, e.width, e.height].every(Number.isFinite));
  const references = [
    { x: 0, y: 0, width: page.width, height: page.height, page: true },
    ...peers,
  ];
  const result = { ...candidate };
  const winners = [];
  for (const axis of ['x', 'y']) {
    const dimension = axis === 'x' ? 'width' : 'height';
    const extent = axis === 'x' ? page.width : page.height;
    const minimum = input.kind === 'label' ? 16 : axis === 'x' ? 48 : 40;
    let winner = null;
    if (enabled && !altKey) {
      const anchors = mode === 'resize' ? [0.5, 1] : [0, 0.5, 1];
      for (const reference of references) for (const anchor of anchors) {
        const position = reference[axis] + reference[dimension] * anchor;
        const current = candidate[axis] + candidate[dimension] * anchor;
        const delta = position - current;
        const value = mode === 'resize' ? (position - candidate[axis]) / anchor : candidate[axis] + delta;
        if (Math.abs(delta) > tolerance ||
          (mode === 'resize' ? value < minimum || candidate[axis] + value > extent : value < 0 || value + candidate[dimension] > extent)) continue;
        if (!winner || Math.abs(delta) < winner.distance) winner = { axis, position, reference, value, distance: Math.abs(delta) };
      }
    }
    // Gelijke tussenruimte: een bediening die je naast een rij (of onder een kolom)
    // sleept, klikt vast op dezelfde afstand als de buren onderling al hebben.
    if (enabled && !altKey && mode === 'move') {
      const row = consoleArray(candidate, peers, axis).filter(e => e.id !== candidate.id);
      const gaps = new Set();
      for (let i = 0; i + 1 < row.length; i++) {
        const gap = row[i + 1][axis] - (row[i][axis] + row[i][dimension]);
        if (gap >= 0) gaps.add(Math.round(gap));
      }
      for (const gap of gaps) for (const peer of row) for (const value of [peer[axis] + peer[dimension] + gap, peer[axis] - gap - candidate[dimension]]) {
        const distance = Math.abs(value - candidate[axis]);
        if (distance > tolerance || value < 0 || value + candidate[dimension] > extent) continue;
        if (row.some(e => value < e[axis] + e[dimension] && value + candidate[dimension] > e[axis])) continue;
        if (!winner || distance <= winner.distance) winner = { axis, position: value, reference: peer, value, distance, spacing: true };
      }
    }
    if (winner) {
      result[mode === 'resize' ? dimension : axis] = winner.value;
      winners.push(winner);
    } else if (mode === 'resize') {
      result[dimension] = Math.max(minimum, Math.min(extent - candidate[axis], Math.round(candidate[dimension] / step) * step));
    } else {
      result[axis] = Math.max(0, Math.min(extent - candidate[dimension], Math.round(candidate[axis] / step) * step));
    }
  }
  const guides = winners.map(winner => {
    const cross = winner.axis === 'x' ? 'y' : 'x';
    const crossDimension = winner.axis === 'x' ? 'height' : 'width';
    const crossExtent = winner.axis === 'x' ? page.height : page.width;
    return {
      axis: winner.axis, position: winner.position,
      from: winner.reference.page ? 0 : Math.max(0, Math.min(result[cross], winner.reference[cross]) - 12 / unit),
      to: winner.reference.page ? crossExtent : Math.min(crossExtent,
        Math.max(result[cross] + result[crossDimension], winner.reference[cross] + winner.reference[crossDimension]) + 12 / unit),
    };
  });
  return { element: result, guides };
}

// Een "array" is een rij (axis 'x') of kolom (axis 'y') gelijksoortige bedieningen
// waarvan de middens op de dwarsas dicht bij elkaar liggen. Gesorteerd langs de as.
export function consoleArray(element, elements, axis = 'x') {
  const cross = axis === 'x' ? 'y' : 'x', crossDimension = axis === 'x' ? 'height' : 'width';
  const label = element.kind === 'label';
  const centre = element[cross] + element[crossDimension] / 2;
  const members = elements.filter(e => e.id !== element.id && (e.kind === 'label') === label &&
    [e.x, e.y, e.width, e.height].every(Number.isFinite) &&
    Math.abs(e[cross] + e[crossDimension] / 2 - centre) <= Math.min(e[crossDimension], element[crossDimension]) / 2);
  return [...members, element].sort((a, b) => a[axis] - b[axis] || String(a.id).localeCompare(String(b.id)));
}

// Lijnt de herkende rij of kolom uit op het geselecteerde element en verdeelt de
// tussenruimte gelijk tussen de buitenste twee. Geeft per id de nieuwe x en y,
// of null wanneer er geen rij/kolom van minstens twee bedieningen is.
export function alignConsoleArray(element, elements, page, axis = 'x') {
  const members = consoleArray(element, elements, axis);
  if (members.length < 2) return null;
  const cross = axis === 'x' ? 'y' : 'x';
  const dimension = axis === 'x' ? 'width' : 'height', crossDimension = axis === 'x' ? 'height' : 'width';
  const extent = axis === 'x' ? page.width : page.height, crossExtent = axis === 'x' ? page.height : page.width;
  const centre = element[cross] + element[crossDimension] / 2;
  const total = members.reduce((sum, e) => sum + e[dimension], 0);
  const first = members[0], last = members[members.length - 1];
  let start = first[axis];
  let gap = (last[axis] + last[dimension] - start - total) / (members.length - 1);
  if (gap < 0) gap = 8;                                    // overlappend: naast elkaar zetten
  // Hele pixels, zodat alle tussenruimtes precies gelijk zijn (de laatste schuift hooguit een paar pixels).
  gap = Math.round(gap);
  if (total + gap * (members.length - 1) > extent) gap = Math.max(0, Math.floor((extent - total) / (members.length - 1)));
  start = Math.max(0, Math.min(extent - (total + gap * (members.length - 1)), start));
  const result = {};
  let position = start;
  for (const e of members) {
    result[e.id] = {
      [axis]: Math.round(position),
      [cross]: Math.max(0, Math.min(crossExtent - e[crossDimension], Math.round(centre - e[crossDimension] / 2))),
    };
    position += e[dimension] + gap;
  }
  return result;
}
