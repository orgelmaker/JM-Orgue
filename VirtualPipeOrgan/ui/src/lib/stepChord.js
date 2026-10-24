// Stapinvoer-akkoord (0.9.22): bijhouden welke toetsen nu ingedrukt zijn en
// welke noten in deze aanslag horen, gesleuteld op kanaal ÉN toonhoogte.
//
// Tot 0.9.21 was de sleutel alleen het MIDI-nummer. Dezelfde noot op twee
// kanalen (c' met rechts én links, of manuaal en pedaal) werd dan één noot:
// de eerste NoteOff haalde hem uit "ingedrukt" terwijl het andere kanaal
// hem nog vasthield, en het akkoord werd te vroeg vastgelegd met één noot
// te weinig. Puur JS, zonder Svelte-afhankelijkheden, zodat het met Node te
// testen is (testscripts/stepChord.test.mjs).

/** Sleutel van één ingedrukte toets: kanaal + noot. Onbekend kanaal = 'x'. */
export function stapSleutel(channel, midi) {
  const ch = (channel == null || Number.isNaN(Number(channel))) ? 'x' : Number(channel);
  return `${ch}:${Number(midi)}`;
}

/**
 * Maak een akkoord-verzamelaar. `noteOn` registreert een toets, `noteOff`
 * laat hem los; is daarna niets meer ingedrukt, dan geeft `noteOff` de
 * verzamelde noten van deze aanslag terug (anders null).
 * Elke noot: { midi, channel, division, layer } — division/layer komen van
 * de backend (kanaal → divisie → doelbalk) en mogen null zijn.
 */
export function maakStapAkkoord() {
  let ingedrukt = new Map();
  let akkoord = new Map();
  return {
    noteOn(midi, channel = null, division = null, layer = null) {
      const k = stapSleutel(channel, midi);
      const noot = { midi: Number(midi), channel: channel == null ? null : Number(channel), division: division || null, layer: layer ?? null };
      ingedrukt.set(k, noot);
      akkoord.set(k, noot);
    },
    noteOff(midi, channel = null) {
      ingedrukt.delete(stapSleutel(channel, midi));
      if (ingedrukt.size > 0) return null;
      return this.commit();
    },
    commit() {
      const noten = Array.from(akkoord.values());
      akkoord = new Map();
      return noten;
    },
    reset() { ingedrukt = new Map(); akkoord = new Map(); },
    get aantalIngedrukt() { return ingedrukt.size; },
    get aantalInAkkoord() { return akkoord.size; },
  };
}

/**
 * Verdeel de noten van één aanslag over balken: [[laagId, [midi, …]], …].
 * Voorrang per noot: de doelbalk die de backend meegaf (kanaal → divisie →
 * armed), anders een balk waaraan de divisie is toegewezen, anders de
 * standaardbalk. Dubbele toonhoogtes binnen één balk vallen samen (dezelfde
 * noot op twee kanalen die tóch op één balk landen is één noot).
 */
export function groepeerPerBalk(noten, layers, standaardLaagId) {
  const groepen = new Map();
  for (const n of noten) {
    let lid = n.layer ?? null;
    if (lid == null && n.division) {
      const laag = (layers || []).find((l) => (l.divisions || []).includes(n.division));
      if (laag) lid = laag.id;
    }
    if (lid == null) lid = standaardLaagId;
    if (!groepen.has(lid)) groepen.set(lid, []);
    const g = groepen.get(lid);
    if (!g.includes(n.midi)) g.push(n.midi);
  }
  return [...groepen];
}

/**
 * Routeringsoverzicht voor de statusregel en de waarschuwingen (0.9.22).
 * Per balk: waarop hij vangt (eigen kanaal, divisies met hun ingeleerde
 * kanaal, of alleen als armed laag). `mappings` = [{division, channel}].
 * Geeft { regels: [{id, name, kanaal, divisies, bron}], waarschuwingen: [{soort, ...}] }
 * met soort 'ongebonden' (geen kanaal, geen divisie) of 'overlap' (twee
 * balken met hetzelfde kanaal — de eerste wint, de tweede vangt nooit).
 */
export function routeringOverzicht(layers, mappings, armedLayer) {
  const kanaalVanDivisie = (d) => {
    const m = (mappings || []).find((x) => x.division === d);
    return m && m.channel != null ? Number(m.channel) : null;
  };
  const regels = (layers || []).map((l) => {
    const kanaal = l.midi_channel == null ? null : Number(l.midi_channel);
    const divisies = (l.divisions || []).map((d) => ({ naam: d, kanaal: kanaalVanDivisie(d) }));
    let bron;
    if (kanaal != null) bron = 'kanaal';
    else if (divisies.length) bron = 'divisie';
    else bron = armedLayer === l.id ? 'armed' : 'niets';
    return { id: l.id, name: l.name, kanaal, divisies, bron };
  });
  const waarschuwingen = [];
  for (const r of regels) {
    if (r.bron === 'niets' || r.bron === 'armed') waarschuwingen.push({ soort: 'ongebonden', name: r.name, armed: r.bron === 'armed' });
  }
  // Effectief kanaal per balk (eigen kanaal, of het ingeleerde kanaal van
  // een divisie); twee balken op hetzelfde kanaal zonder eigen kanaal op
  // de tweede = overlap.
  const gezien = new Map();
  for (const r of regels) {
    const kanalen = r.kanaal != null ? [r.kanaal] : r.divisies.map((d) => d.kanaal).filter((k) => k != null);
    for (const k of kanalen) {
      if (gezien.has(k)) waarschuwingen.push({ soort: 'overlap', a: gezien.get(k), b: r.name, kanaal: k });
      else gezien.set(k, r.name);
    }
  }
  return { regels, waarschuwingen };
}
