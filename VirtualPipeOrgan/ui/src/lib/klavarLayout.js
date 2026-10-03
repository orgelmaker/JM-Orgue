// Geometrie van de klavar-weergave (0.7.71), in millimeters op papier; het
// scherm schaalt met één factor (PX_PER_MM × zoom). Pure functies zonder DOM,
// zodat ze in Node te testen zijn (testscripts/klavar_layout_test.mjs) en
// later ook een SVG-export kunnen bedienen.
//
// Tekenregels: PLAN_klavar_notatie.md, paragraaf 2. Kort:
// - verticale balk, tijd van boven naar beneden, laag links, hoog rechts;
// - een lijn per zwarte toets: de groep van twee (cis, dis) dun, de groep van
//   drie (fis, gis, ais) dik; cis' en dis' gestreept (de enige "sleutel");
// - witte toets k (c = 0 … b = 6, per octaaf +7) op (k + ½)·w, zwarte toets
//   op (k + 1)·w met k de witte toets eronder; tussen e en f en tussen b en c
//   staat dus geen lijn;
// - open kop = witte toets (hangt onder de stok), dichte kop = zwarte toets
//   (staat boven de stok); stok naar rechts = rechterhand, naar links =
//   linkerhand en pedaal;
// - duur is afstand; stopteken "v" op het einde, doorklinkstip op een latere
//   inzet; maatstrepen doorgetrokken, telstrepen in de eerste maat over de
//   volle breedte met telnummers, daarna korte streepjes aan de randen en bij
//   cis'/dis'.

export const MAAT = {
  w: 1.6,              // witte-toetsafstand op het manuaal (mm)
  pedaalSchaal: 0.8,   // de pedaalbalk is kleiner
  kwart: 7.5,          // mm per kwartnoot
  tussen: 8,           // ruimte tussen pedaalbalk en manuaalbalk
  randLinks: 11,       // telnummers, buiten de reikwijdte van een linkerstok (4 w)
  randRechts: 10,      // maatnummers, buiten de reikwijdte van een rechterstok
  randBoven: 2,
  randOnder: 2,
  lijnDun: 0.15,
  lijnDik: 0.30,
  stok: 0.3,           // dikte
  stokLengte: 4,       // × w
  stokKort: 2,         // × w, bij verstrengelde handen
  kop: 0.9,            // doorsnede × w
  kopLijn: 0.2,
  maatstreep: 0.35,
  slotstreep: 1.0,
  telstreep: 0.2,
  telstreepje: 3,      // × w
  stopB: 0.8,          // × w
  stopH: 0.5,          // × w
  stip: 0.4,           // × w (bij 50 % zoom nog zichtbaar)
  akkoordUitsteek: 2,  // × w
  tekst: 2.4,          // lettergrootte (mm)
  // Papier (A4 staand, 12 mm marge) voor de paginering.
  paginaBreedte: 186,
  paginaHoogte: 273,   // 297 − 2 × 12
  kopHoogte: 16,       // titel + tempo + legenda op elke pagina
  kolomTussen: 10,
};

/** Schermpixels per mm bij 100 % (2 × CSS-mm). */
export const PX_PER_MM = 7.56;

const WIT_INDEX = [0, 0, 1, 1, 2, 3, 3, 4, 4, 5, 5, 6]; // pitch-class → witte toets eronder
const ZWART = [false, true, false, true, false, false, true, false, true, false, true, false];

export function isZwart(midi) { return ZWART[((midi % 12) + 12) % 12]; }
function witIndex(midi) { return 7 * Math.floor(midi / 12) + WIT_INDEX[((midi % 12) + 12) % 12]; }

/** Balkgeometrie voor een bereik [min..max] (min een C, max een B) op schaal w. */
export function balkGeometrie(min, max, w, x0) {
  const aantal = witIndex(max) - witIndex(min) + 1;
  return { min, max, w, x0, breedte: aantal * w };
}

/** Middelpunt (x in mm) van een toets op een balk. */
export function xOfMidi(midi, balk) {
  const k = witIndex(midi) - witIndex(balk.min);
  return balk.x0 + (k + 0.5 + (isZwart(midi) ? 0.5 : 0)) * balk.w;
}

/** Dichtstbijzijnde toets bij een x (ook zwarte toetsen), binnen het bereik. */
export function nearestKey(x, balk) {
  let best = balk.min, bestD = Infinity;
  for (let m = balk.min; m <= balk.max; m++) {
    const d = Math.abs(xOfMidi(m, balk) - x);
    if (d < bestD) { bestD = d; best = m; }
  }
  return best;
}

/** y (mm) van een rastermoment t (relatief aan de systeemstart). */
export function yOfGrid(t, q) { return MAAT.randBoven + (t / q) * MAAT.kwart; }

/** Rastermoment bij een y, gesnapt op het raster en geklemd op het systeem. */
export function gridTimeAtY(layout, y) {
  const t = Math.round(((y - MAAT.randBoven) / MAAT.kwart) * layout.q);
  const max = layout.sys.bars * layout.measureLen;
  return layout.sys.t0 + Math.max(0, Math.min(max, t));
}

/** Aantal maten per systeem dat op een pagina past (minimaal 1). */
export function barsPerSystem(model) {
  const maatMm = (model.measure_len / model.q) * MAAT.kwart;
  const beschikbaar = MAAT.paginaHoogte - MAAT.kopHoogte - MAAT.randBoven - MAAT.randOnder;
  return Math.max(1, Math.floor(beschikbaar / maatMm));
}

/** Verdeel de maten in systemen van hele maten; het laatste is korter. */
export function splitSystems(model, maxBars = barsPerSystem(model)) {
  const out = [];
  const totaal = Math.max(1, model.num_measures);
  for (let bar0 = 0, i = 0; bar0 < totaal; bar0 += maxBars, i++) {
    const bars = Math.min(maxBars, totaal - bar0);
    out.push({ index: i, bar0, bars, t0: bar0 * model.measure_len, t1: (bar0 + bars) * model.measure_len });
  }
  return out;
}

function lijnen(balk, manuaal) {
  const out = [];
  for (let m = balk.min; m <= balk.max; m++) {
    if (!isZwart(m)) continue;
    const pc = m % 12;
    const dik = pc === 6 || pc === 8 || pc === 10;
    out.push({ x: xOfMidi(m, balk), dik, gestreept: manuaal && (m === 61 || m === 63), midi: m });
  }
  return out;
}

/**
 * Lay-out van één systeem: alles wat de renderer tekent, in mm.
 * opts.verbergLaatsteStop: tijdens het opnemen geen "v" onder de laatste
 * noot per (balk, hand), anders flikkert er een stopteken onder alles wat
 * je speelt.
 */
export function layoutSystem(model, sys, opts = {}) {
  const q = model.q;
  const measureLen = model.measure_len;
  const beats = model.beats_per_bar;
  const w = MAAT.w;
  const heeftPedaal = !!model.pedal;
  const wp = w * MAAT.pedaalSchaal;
  let x = MAAT.randLinks;
  const pedaal = heeftPedaal ? balkGeometrie(model.pedal.midi_min, model.pedal.midi_max, wp, x) : null;
  if (pedaal) x += pedaal.breedte + MAAT.tussen;
  const manuaal = balkGeometrie(model.manual.midi_min, model.manual.midi_max, w, x);
  const breedte = manuaal.x0 + manuaal.breedte + MAAT.randRechts;
  const hoogte = MAAT.randBoven + (sys.bars * measureLen / q) * MAAT.kwart + MAAT.randOnder;
  const xLinks = pedaal ? pedaal.x0 : manuaal.x0;
  const xRechts = manuaal.x0 + manuaal.breedte;

  const lay = {
    sys, q, measureLen, beats, w, breedte, hoogte, pedaal, manuaal, xLinks, xRechts,
    lijnen: [], maatstrepen: [], telstrepen: [], telnummers: [], maatnummers: [],
    noten: [], akkoordlijnen: [], stops: [], stippen: [],
  };

  // Lijnen van de balken (over de hele systeemhoogte).
  for (const l of lijnen(manuaal, true)) lay.lijnen.push({ ...l, y0: 0, y1: hoogte });
  if (pedaal) for (const l of lijnen(pedaal, false)) lay.lijnen.push({ ...l, y0: 0, y1: hoogte });

  // Maatstrepen (ook aan het eind van het systeem) en maatnummers.
  for (let i = 0; i <= sys.bars; i++) {
    const t = i * measureLen;
    const y = yOfGrid(t, q);
    const globaal = sys.bar0 + i;
    const slot = globaal === model.num_measures;
    lay.maatstrepen.push({ y, dikte: slot ? MAAT.slotstreep : MAAT.maatstreep, x0: xLinks, x1: xRechts });
    if (i < sys.bars) lay.maatnummers.push({ x: xRechts + MAAT.stokLengte * w + 1, y: y + MAAT.tekst, tekst: String(globaal + 1) });
  }

  // Telstrepen: eerste maat van het stuk over de volle breedte met telnummers,
  // daarna korte streepjes aan de randen en over cis'/dis'.
  const cis = manuaal.min <= 61 && manuaal.max >= 63 ? [xOfMidi(61, manuaal) - w, xOfMidi(63, manuaal) + w] : null;
  for (let i = 0; i < sys.bars; i++) {
    const eerste = sys.bar0 + i === 0;
    for (let b = 0; b < beats; b++) {
      const t = i * measureLen + b * q;
      const y = yOfGrid(t, q);
      if (eerste) {
        lay.telnummers.push({ x: MAAT.randLinks - MAAT.stokLengte * w - 1.2, y: y + MAAT.tekst * 0.4, tekst: String(b + 1) });
        if (b > 0) lay.telstrepen.push({ y, x0: xLinks, x1: xRechts, gestreept: true });
      } else if (b > 0) {
        for (const balk of [pedaal, manuaal]) {
          if (!balk) continue;
          const k = MAAT.telstreepje * balk.w;
          lay.telstrepen.push({ y, x0: balk.x0, x1: balk.x0 + k, gestreept: false });
          lay.telstrepen.push({ y, x0: balk.x0 + balk.breedte - k, x1: balk.x0 + balk.breedte, gestreept: false });
        }
        if (cis) lay.telstrepen.push({ y, x0: cis[0], x1: cis[1], gestreept: false });
      }
    }
  }

  // Noten.
  const verbergLaatste = !!opts.verbergLaatsteStop;
  const laatstePer = new Map(); // "balk/hand" → laatste start (over het hele model)
  const balken = [['manual', model.manual, manuaal], ['pedal', model.pedal, pedaal]];
  if (verbergLaatste) {
    for (const [naam, st] of balken) {
      if (!st) continue;
      for (const n of st.notes) {
        const sleutel = naam + '/' + n.hand;
        const cur = laatstePer.get(sleutel);
        if (cur == null || n.start > cur) laatstePer.set(sleutel, n.start);
      }
    }
  }
  const groepen = new Map(); // "balk/start/hand" → noten (voor akkoordlijnen)
  for (const [naam, st, balk] of balken) {
    if (!st || !balk) continue;
    const r = (MAAT.kop * balk.w) / 2;
    for (let i = 0; i < st.notes.length; i++) {
      const n = st.notes[i];
      const inSysteem = n.start >= sys.t0 && n.start < sys.t1;
      const dir = n.hand === 'right' ? 1 : -1;
      const cx = xOfMidi(n.midi, balk);
      if (inSysteem) {
        const yStok = yOfGrid(n.start - sys.t0, q);
        const zwart = isZwart(n.midi);
        const noot = {
          key: n.id != null ? String(n.id) : naam + '-' + i,
          id: n.id, layerId: n.layer_id, midi: n.midi, hand: n.hand, balk: naam,
          start: n.start, end: n.end,
          cx, cy: zwart ? yStok - r : yStok + r, r, yStok, zwart, dir,
          stokX0: cx, stokX1: cx + dir * MAAT.stokLengte * balk.w,
          inAkkoord: false,
        };
        lay.noten.push(noot);
        const g = naam + '/' + n.start + '/' + n.hand;
        if (!groepen.has(g)) groepen.set(g, []);
        groepen.get(g).push(noot);
        // Doorklinkstippen in dit systeem.
        for (const t of n.dots || []) {
          if (t >= sys.t0 && t < sys.t1) {
            // Op een maatstreep zou de stip óp de streep liggen: er net onder.
            const y = yOfGrid(t - sys.t0, q) + (t % measureLen === 0 ? MAAT.stip * balk.w * 1.5 : 0);
            lay.stippen.push({ x: cx, y, r: (MAAT.stip * balk.w) / 2, id: n.id });
          }
        }
      } else {
        // Een noot uit een eerder systeem die hier nog klinkt: stippen op
        // latere inzetten in dit systeem.
        for (const t of n.dots || []) {
          if (t >= sys.t0 && t < sys.t1) {
            // Op een maatstreep zou de stip óp de streep liggen: er net onder.
            const y = yOfGrid(t - sys.t0, q) + (t % measureLen === 0 ? MAAT.stip * balk.w * 1.5 : 0);
            lay.stippen.push({ x: cx, y, r: (MAAT.stip * balk.w) / 2, id: n.id });
          }
        }
      }
      // Stip onder de bovenste maatstreep als de noot de systeemgrens kruist
      // (tenzij daar al een stip van een inzet staat).
      if ((n.bar_crossings || []).includes(sys.t0) && n.start < sys.t0 && !(n.dots || []).includes(sys.t0)) {
        lay.stippen.push({ x: cx, y: MAAT.randBoven + MAAT.stip * balk.w * 1.5, r: (MAAT.stip * balk.w) / 2, id: n.id });
      }
      // Stopteken op het (legato-afgekapte) einde, in het systeem waar dat
      // valt; bij een witte kop nooit in de kop zelf (een zestiende is korter
      // dan kop + chevron).
      const eind = n.end_cut != null ? n.end_cut : n.end;
      if (n.stop && eind > sys.t0 && eind <= sys.t1) {
        const onderdruk = verbergLaatste && laatstePer.get(naam + '/' + n.hand) === n.start;
        if (!onderdruk) {
          const h = MAAT.stopH * balk.w;
          let y = yOfGrid(eind - sys.t0, q);
          if (inSysteem && !isZwart(n.midi)) y = Math.max(y, yOfGrid(n.start - sys.t0, q) + MAAT.kop * balk.w + h);
          lay.stops.push({ x: cx, y, b: MAAT.stopB * balk.w, h, id: n.id });
        }
      }
    }
  }

  // Akkoordlijnen: noten van één hand op dezelfde inzet delen één stok. Bij
  // verstrengeling (een kop van de andere hand tussen de buitenste koppen)
  // korte stokken per kop.
  for (const [g, noten] of groepen) {
    if (noten.length < 2) continue;
    const [naam, start, hand] = g.split('/');
    const xs = noten.map(n => n.cx);
    const xMin = Math.min(...xs), xMax = Math.max(...xs);
    const andere = lay.noten.filter(n => n.balk === naam && String(n.start) === start && n.hand !== hand
      && n.cx > xMin && n.cx < xMax);
    const balkW = naam === 'pedal' ? pedaal.w : manuaal.w;
    if (andere.length) {
      for (const n of noten) { n.stokX1 = n.cx + n.dir * MAAT.stokKort * balkW; }
      continue;
    }
    const dir = noten[0].dir;
    for (const n of noten) n.inAkkoord = true;
    lay.akkoordlijnen.push({
      y: noten[0].yStok,
      x0: dir > 0 ? xMin : xMin - MAAT.akkoordUitsteek * balkW,
      x1: dir > 0 ? xMax + MAAT.akkoordUitsteek * balkW : xMax,
      balk: naam, start: Number(start), hand, dir,
    });
  }

  // Gekruiste handen op dezelfde inzet: een stok of akkoordlijn eindigt een
  // halve w vóór een kop van de andere hand, zodat twee stokken op dezelfde
  // y nooit aansluiten en nooit door een vreemde kop lopen.
  const perInzet = new Map();
  for (const n of lay.noten) {
    const k = n.balk + '/' + n.start;
    if (!perInzet.has(k)) perInzet.set(k, []);
    perInzet.get(k).push(n);
  }
  const beperk = (balkNaam, start, hand, dir, x0, x1) => {
    const balkW = balkNaam === 'pedal' ? pedaal.w : manuaal.w;
    let grens = dir > 0 ? x1 : x0;
    for (const m of perInzet.get(balkNaam + '/' + start) || []) {
      if (m.hand === hand) continue;
      if (dir > 0 && m.cx > x0 && m.cx - 0.5 * balkW < grens) grens = Math.max(x0, m.cx - 0.5 * balkW);
      if (dir < 0 && m.cx < x1 && m.cx + 0.5 * balkW > grens) grens = Math.min(x1, m.cx + 0.5 * balkW);
    }
    return grens;
  };
  for (const n of lay.noten) {
    if (n.inAkkoord) continue;
    n.stokX1 = beperk(n.balk, n.start, n.hand, n.dir, Math.min(n.stokX0, n.stokX1), Math.max(n.stokX0, n.stokX1));
  }
  for (const a of lay.akkoordlijnen) {
    if (a.dir > 0) a.x1 = beperk(a.balk, a.start, a.hand, 1, a.x0, a.x1);
    else a.x0 = beperk(a.balk, a.start, a.hand, -1, a.x0, a.x1);
  }
  return lay;
}

/** Welke balk ligt onder een x: 'pedal' of 'manual'. */
export function staffAtX(layout, x) {
  if (!layout.pedaal) return 'manual';
  const grens = layout.pedaal.x0 + layout.pedaal.breedte + MAAT.tussen / 2;
  return x < grens ? 'pedal' : 'manual';
}

/** Dichtstbijzijnde noot bij (x, y) binnen `radius` mm, of null. */
export function hitTest(layout, x, y, radius) {
  let best = null, bestD = Infinity;
  for (const n of layout.noten) {
    const d = Math.hypot(n.cx - x, n.cy - y);
    if (d < bestD) { bestD = d; best = n; }
  }
  return best && bestD <= radius ? best : null;
}

/**
 * Pagina's en kolommen: twee kolommen als twee systemen naast elkaar op A4
 * passen, anders één; kolommen worden van boven naar beneden gevuld, dus de
 * DOM-volgorde is de leesvolgorde.
 */
export function paginate(layouts) {
  if (!layouts.length) return [];
  const breedte = Math.max(...layouts.map(l => l.breedte));
  const kolommen = 2 * breedte + MAAT.kolomTussen <= MAAT.paginaBreedte ? 2 : 1;
  const hoogte = MAAT.paginaHoogte - MAAT.kopHoogte;
  const paginas = [];
  let pagina = null, kolom = null, hoogteKolom = 0;
  for (const lay of layouts) {
    const nieuweKolom = !kolom || (kolom.length > 0 && hoogteKolom + lay.hoogte > hoogte);
    if (nieuweKolom) {
      if (!pagina || pagina.kolommen.length >= kolommen) {
        pagina = { kolommen: [] };
        paginas.push(pagina);
      }
      kolom = [];
      pagina.kolommen.push(kolom);
      hoogteKolom = 0;
    }
    kolom.push(lay);
    hoogteKolom += lay.hoogte;
  }
  return paginas;
}
