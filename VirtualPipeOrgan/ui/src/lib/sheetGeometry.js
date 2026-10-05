// Geometrie van het notenschrift (0.7.87): klikpositie → rastertijd binnen
// een maat. Zuivere functies, los van OSMD, zodat ze in Node te testen zijn.

function klem(v, lo, hi) { return Math.max(lo, Math.min(hi, v)); }

/**
 * x (ongezoomde px, zoals de VexFlow-stave) → rastereenheid binnen de maat.
 * `entries`: [{ x, t }] = de getekende inzetten van de maat (x in dezelfde
 * eenheden, t in rastereenheden); `x0`/`x1` = begin en einde van de
 * notenruimte van de stave; `measureLen` = maatlengte in rastereenheden.
 * Tussen twee ankerpunten (maatbegin, de inzetten, maateinde) wordt lineair
 * gerekend en op de dichtstbijzijnde rastereenheid afgerond; het resultaat
 * ligt altijd in [0, measureLen − 1].
 */
export function xToGridTime(entries, x0, x1, x, measureLen) {
  const len = Math.max(1, Math.round(Number(measureLen) || 1));
  const laatste = len - 1;
  if (!(x1 > x0)) return 0;
  const pts = (entries || [])
    .filter(e => e && Number.isFinite(e.x) && Number.isFinite(e.t) && e.x > x0 && e.x < x1)
    .map(e => ({ x: e.x, t: klem(e.t, 0, len) }))
    .sort((a, b) => a.x - b.x);
  const anker = [{ x: x0, t: 0 }, ...pts, { x: x1, t: len }];
  for (let i = 1; i < anker.length; i++) if (anker[i].t < anker[i - 1].t) anker[i].t = anker[i - 1].t;
  if (x <= anker[0].x) return 0;
  for (let i = 0; i + 1 < anker.length; i++) {
    const a = anker[i], b = anker[i + 1];
    if (x >= a.x && x <= b.x) {
      const f = b.x > a.x ? (x - a.x) / (b.x - a.x) : 0;
      return klem(Math.round(a.t + f * (b.t - a.t)), 0, laatste);
    }
  }
  return laatste;
}

/**
 * Rastereenheid binnen de maat → x (ongezoomde px), het omgekeerde van
 * xToGridTime met dezelfde ankerpunten: de invoercursor staat zo precies op
 * de inzet waar een klik op diezelfde plek zou landen.
 */
export function gridTimeToX(entries, x0, x1, t, measureLen) {
  const len = Math.max(1, Math.round(Number(measureLen) || 1));
  if (!(x1 > x0)) return x0;
  const pts = (entries || [])
    .filter(e => e && Number.isFinite(e.x) && Number.isFinite(e.t) && e.x > x0 && e.x < x1)
    .map(e => ({ x: e.x, t: klem(e.t, 0, len) }))
    .sort((a, b) => a.x - b.x);
  const anker = [{ x: x0, t: 0 }, ...pts, { x: x1, t: len }];
  for (let i = 1; i < anker.length; i++) if (anker[i].t < anker[i - 1].t) anker[i].t = anker[i - 1].t;
  const tt = klem(Number(t) || 0, 0, len);
  // Precies op een inzet: de laatst getekende inzet met die tijd.
  const exact = anker.filter(p => p.t === tt);
  if (exact.length) return exact[exact.length - 1].x;
  for (let i = 0; i + 1 < anker.length; i++) {
    const a = anker[i], b = anker[i + 1];
    if (tt > a.t && tt < b.t) return a.x + ((tt - a.t) / (b.t - a.t)) * (b.x - a.x);
  }
  return x1;
}

/**
 * Klikpositie in stapinvoer: op het raster van de gekozen nootwaarde (een
 * kwart klikt op de tel, een achtste op de halve tel) óf op een al getekende
 * inzet, wat het dichtstbij ligt. Zo landt een klik "ergens in een lege maat"
 * niet een zestiende naast de tel, en een klik op een rust precies op die rust.
 * `stap` = nootwaarde in rastereenheden; bij gelijke afstand wint de vroegste.
 */
export function snapGridTime(grid, entries, stap, measureLen) {
  const len = Math.max(1, Math.round(Number(measureLen) || 1));
  const g = klem(Math.round(Number(grid) || 0), 0, len - 1);
  const st = Math.max(1, Math.round(Number(stap) || 1));
  const kandidaten = new Set();
  for (let t = 0; t < len; t += st) kandidaten.add(t);
  for (const e of entries || []) if (e && Number.isFinite(e.t) && e.t >= 0 && e.t < len) kandidaten.add(Math.round(e.t));
  let best = g, afstand = Infinity;
  for (const t of [...kandidaten].sort((a, b) => a - b)) {
    const d = Math.abs(t - g);
    if (d < afstand) { afstand = d; best = t; }
  }
  return best;
}

/**
 * Breedte van één rastereenheid in px voor een maat: voor Shift+slepen in de
 * tijd (hoeveel cellen is een horizontale verplaatsing?).
 */
export function celBreedte(x0, x1, measureLen) {
  const len = Math.max(1, Math.round(Number(measureLen) || 1));
  return Math.max(1e-6, (x1 - x0) / len);
}
