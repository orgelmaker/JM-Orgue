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
