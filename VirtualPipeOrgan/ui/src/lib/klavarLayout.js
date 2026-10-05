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
  labelTekst: 2.0,     // manuaallabels
  balk: 0.35,          // dikte van een balk × w
  toonsoortRuimte: 6,  // extra ruimte boven de eerste maat voor het toonsoortteken
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
export function yOfGrid(t, q, top = MAAT.randBoven) { return top + (t / q) * MAAT.kwart; }

/** Rastermoment bij een y, gesnapt op het raster en geklemd op het systeem. */
export function gridTimeAtY(layout, y) {
  const t = Math.round(((y - layout.top) / MAAT.kwart) * layout.q);
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
  // Het eerste systeem heeft boven de eerste maat ruimte voor het toonsoortteken.
  const top = sys.index === 0 ? MAAT.randBoven + MAAT.toonsoortRuimte : MAAT.randBoven;
  const yg = (t) => yOfGrid(t, q, top);
  const hoogte = top + (sys.bars * measureLen / q) * MAAT.kwart + MAAT.randOnder;
  const xLinks = pedaal ? pedaal.x0 : manuaal.x0;
  const xRechts = manuaal.x0 + manuaal.breedte;

  const lay = {
    sys, q, measureLen, beats, w, top, breedte, hoogte, pedaal, manuaal, xLinks, xRechts,
    lijnen: [], maatstrepen: [], telstrepen: [], telnummers: [], maatnummers: [],
    noten: [], akkoordlijnen: [], stops: [], stippen: [], balken: [], labels: [], toonsoort: null,
  };

  // Lijnen van de balken (over de hele systeemhoogte).
  for (const l of lijnen(manuaal, true)) lay.lijnen.push({ ...l, y0: 0, y1: hoogte });
  if (pedaal) for (const l of lijnen(pedaal, false)) lay.lijnen.push({ ...l, y0: 0, y1: hoogte });

  // Maatstrepen (ook aan het eind van het systeem) en maatnummers.
  for (let i = 0; i <= sys.bars; i++) {
    const t = i * measureLen;
    const y = yg(t);
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
      // Telstreep per tel van de maatsoort (0.7.83: bij 6/8 per achtste).
      const t = i * measureLen + b * (measureLen / beats);
      const y = yg(t);
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
        const yStok = yg(n.start - sys.t0);
        const zwart = isZwart(n.midi);
        const noot = {
          key: n.id != null ? String(n.id) : naam + '-' + i,
          id: n.id, layerId: n.layer_id, midi: n.midi, hand: n.hand, balk: naam,
          start: n.start, end: n.end,
          cx, cy: zwart ? yStok - r : yStok + r, r, yStok, zwart, dir,
          stokX0: cx, stokX1: cx + dir * MAAT.stokLengte * balk.w,
          inAkkoord: false, beam: n.beam ?? null, label: n.label ?? null,
        };
        lay.noten.push(noot);
        const g = naam + '/' + n.start + '/' + n.hand;
        if (!groepen.has(g)) groepen.set(g, []);
        groepen.get(g).push(noot);
        // Doorklinkstippen in dit systeem.
        for (const t of n.dots || []) {
          if (t >= sys.t0 && t < sys.t1) {
            // Op een maatstreep zou de stip óp de streep liggen: er net onder.
            const y = yg(t - sys.t0) + (t % measureLen === 0 ? MAAT.stip * balk.w * 1.5 : 0);
            lay.stippen.push({ x: cx, y, r: (MAAT.stip * balk.w) / 2, id: n.id });
          }
        }
      } else {
        // Een noot uit een eerder systeem die hier nog klinkt: stippen op
        // latere inzetten in dit systeem.
        for (const t of n.dots || []) {
          if (t >= sys.t0 && t < sys.t1) {
            // Op een maatstreep zou de stip óp de streep liggen: er net onder.
            const y = yg(t - sys.t0) + (t % measureLen === 0 ? MAAT.stip * balk.w * 1.5 : 0);
            lay.stippen.push({ x: cx, y, r: (MAAT.stip * balk.w) / 2, id: n.id });
          }
        }
      }
      // Stip onder de bovenste maatstreep als de noot de systeemgrens kruist
      // (tenzij daar al een stip van een inzet staat).
      if ((n.bar_crossings || []).includes(sys.t0) && n.start < sys.t0 && !(n.dots || []).includes(sys.t0)) {
        lay.stippen.push({ x: cx, y: top + MAAT.stip * balk.w * 1.5, r: (MAAT.stip * balk.w) / 2, id: n.id });
      }
      // Stopteken op het (legato-afgekapte) einde, in het systeem waar dat
      // valt; bij een witte kop nooit in de kop zelf (een zestiende is korter
      // dan kop + chevron).
      const eind = n.end_cut != null ? n.end_cut : n.end;
      if (n.stop && eind > sys.t0 && eind <= sys.t1) {
        const onderdruk = verbergLaatste && laatstePer.get(naam + '/' + n.hand) === n.start;
        if (!onderdruk) {
          const h = MAAT.stopH * balk.w;
          let y = yg(eind - sys.t0);
          if (inSysteem && !isZwart(n.midi)) y = Math.max(y, yg(n.start - sys.t0) + MAAT.kop * balk.w + h);
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

  // Uiteinde van de stok aan de handzijde per (balk, start, hand): de
  // akkoordlijn als er een is, anders de stok van de buitenste noot.
  const stokUiteinde = (n) => {
    const a = lay.akkoordlijnen.find(l => l.balk === n.balk && l.start === n.start && l.hand === n.hand);
    if (a) return n.dir > 0 ? a.x1 : a.x0;
    let x = n.stokX1;
    for (const m of lay.noten) {
      if (m.balk === n.balk && m.start === n.start && m.hand === n.hand && !m.inAkkoord) {
        x = n.dir > 0 ? Math.max(x, m.stokX1) : Math.min(x, m.stokX1);
      }
    }
    return x;
  };

  // Balken (0.7.72): één schuine streep door de stokuiteinden van de noten
  // met hetzelfde balknummer (model: per tel, per hand), in tijdvolgorde.
  const perBalk = new Map();
  for (const n of lay.noten) {
    if (n.beam == null) continue;
    const k = n.balk + '/' + n.hand + '/' + n.beam;
    if (!perBalk.has(k)) perBalk.set(k, new Map());
    const inz = perBalk.get(k);
    if (!inz.has(n.start)) inz.set(n.start, { x: stokUiteinde(n), y: n.yStok });
  }
  for (const [k, inz] of perBalk) {
    if (inz.size < 2) continue;
    const balkW = k.startsWith('pedal') ? pedaal.w : manuaal.w;
    const punten = [...inz.entries()].sort((a, b) => a[0] - b[0]).map(e => e[1]);
    lay.balken.push({ punten, dikte: MAAT.balk * balkW });
  }

  // Manuaallabels (0.7.72): naast het stokuiteinde, aan de handzijde.
  for (const n of lay.noten) {
    if (!n.label) continue;
    const balkW = n.balk === 'pedal' ? pedaal.w : manuaal.w;
    const x = stokUiteinde(n) + n.dir * 0.5 * balkW;
    lay.labels.push({ x, y: n.yStok - 0.3 * balkW, tekst: n.label, anchor: n.dir > 0 ? 'start' : 'end' });
  }

  // Toonsoortteken (0.7.72), alleen in het eerste systeem: een kop op de
  // grondtoon (in het octaaf c'–b') in een cirkel (majeur) of ruit (mineur).
  if (sys.index === 0 && model.key_root != null) {
    const midi = 60 + (model.key_root % 12);
    lay.toonsoort = {
      x: xOfMidi(midi, manuaal), y: MAAT.randBoven + MAAT.toonsoortRuimte * 0.45,
      r: (MAAT.kop * w) / 2, zwart: isZwart(midi), ring: 1.15 * w, minor: !!model.minor,
    };
  }
  return lay;
}

/**
 * De y van de "nu"-lijn tijdens het inspelen (0.7.72) in dit systeem, of
 * null als het tijdstip (rastereenheden, mag een breuk zijn) erbuiten valt.
 */
export function nuLijnY(lay, nowGrid) {
  if (nowGrid == null || !Number.isFinite(nowGrid)) return null;
  const t = nowGrid - lay.sys.t0;
  if (t < 0 || t >= lay.sys.t1 - lay.sys.t0) return null;
  return yOfGrid(t, lay.q, lay.top);
}

/**
 * Aangehouden toetsen (note-on zonder note-off) tijdens het inspelen
 * (0.7.72): kop en stok op het inzetmoment, zonder stopteken, zodat een noot
 * er staat zodra hij klinkt en niet pas bij het loslaten. De balk en de hand
 * volgen de legenda van de laag; een toets buiten het bereik van de balk
 * wacht tot het loslaten (dan rekt het model het bereik op).
 * `open` = [{ layerId, midi, channel, start }] met start in rastereenheden;
 * de sleutel neemt het kanaal mee (dezelfde toets van twee manualen op één
 * balk zijn twee aangehouden toetsen).
 */
export function openNoten(lay, model, open) {
  const out = [];
  if (!model || !open || !open.length) return out;
  for (const o of open) {
    const t = o.start - lay.sys.t0;
    if (t < 0 || t >= lay.sys.t1 - lay.sys.t0) continue;
    const leg = (model.legend || []).find(l => l.layer_id === o.layerId) || null;
    const pedaal = leg?.hand === 'pedal';
    const balk = pedaal ? lay.pedaal : lay.manuaal;
    if (!balk || o.midi < balk.min || o.midi > balk.max) continue;
    const hand = pedaal ? 'pedal'
      : leg?.split_midi != null ? (o.midi < leg.split_midi ? 'left' : 'right')
      : (leg?.hand === 'left' ? 'left' : 'right');
    const dir = hand === 'right' ? 1 : -1;
    const zwart = isZwart(o.midi);
    const yStok = yOfGrid(t, lay.q, lay.top);
    const r = (MAAT.kop * balk.w) / 2;
    const cx = xOfMidi(o.midi, balk);
    out.push({
      key: o.layerId + '/' + (o.channel ?? '') + '/' + o.midi + '/' + o.start, midi: o.midi, hand,
      cx, cy: zwart ? yStok - r : yStok + r, r, yStok, zwart,
      stokX0: cx, stokX1: cx + dir * MAAT.stokLengte * balk.w,
    });
  }
  return out;
}

/**
 * Vast bereik (0.7.73): verbreed de balken tot het klavier van het orgel,
 * `bereik` = { manual: [lo, hi], pedal: [lo, hi] } (een ontbrekende balk of
 * een null-paar blijft zoals hij is). Nooit smaller dan het automatische
 * bereik, zodat een noot buiten het klavier (transpositie, koppel) toch een
 * plek houdt.
 */
export function pasBereikToe(model, bereik) {
  if (!model || !bereik) return model;
  const breed = (staff, b) => (!staff || !b) ? staff
    : { ...staff, midi_min: Math.min(staff.midi_min, b[0]), midi_max: Math.max(staff.midi_max, b[1]) };
  return { ...model, manual: breed(model.manual, bereik.manual), pedal: breed(model.pedal, bereik.pedal) };
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
