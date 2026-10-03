// Geometrietest van de klavar-weergave (0.7.71), zonder DOM: xOfMidi,
// nearestKey, splitSystems, paginate en layoutSystem (verstrengelde
// akkoorden, stoptekens, stippen, telstrepen). Draait met `node`
// (ui/package.json is "type": "module", vandaar .mjs) en in CI.
import {
  MAAT, balkGeometrie, xOfMidi, nearestKey, isZwart, splitSystems, paginate, layoutSystem, hitTest, gridTimeAtY, staffAtX,
} from '../ui/src/lib/klavarLayout.js';

let fouten = 0;
function check(cond, naam) {
  if (cond) { console.log('ok   ' + naam); }
  else { console.log('FOUT ' + naam); fouten++; }
}
function dicht(a, b, eps = 1e-6) { return Math.abs(a - b) < eps; }

// ---- xOfMidi: witte toetsen op (k+½)·w, zwarte op (k+1)·w, dubbelruimte e/f en b/c ----
{
  const balk = balkGeometrie(48, 83, 1.6, 0); // c – b''
  const w = 1.6;
  check(dicht(xOfMidi(48, balk), 0.5 * w), 'c op een halve w');
  check(dicht(xOfMidi(49, balk), 1.0 * w), 'cis op k + 0,5');
  check(dicht(xOfMidi(50, balk), 1.5 * w), 'd');
  check(dicht(xOfMidi(52, balk), 2.5 * w), 'e');
  check(dicht(xOfMidi(53, balk), 3.5 * w), 'f: dubbele ruimte na e');
  check(dicht(xOfMidi(54, balk), 4.0 * w), 'fis');
  check(dicht(xOfMidi(59, balk), 6.5 * w), 'b');
  check(dicht(xOfMidi(60, balk), 7.5 * w), "c': dubbele ruimte na b, octaaf = 7 w");
  check(dicht(balk.breedte, 21 * w), 'breedte = aantal witte toetsen × w');
  check(isZwart(61) && !isZwart(60) && isZwart(70) && !isZwart(71), 'zwart/wit');
}

// ---- nearestKey: rondreis midi → x → midi, ook zwarte toetsen ----
{
  const balk = balkGeometrie(36, 95, 1.6, 7);
  let goed = true;
  for (let m = 36; m <= 95; m++) if (nearestKey(xOfMidi(m, balk), balk) !== m) goed = false;
  check(goed, 'nearestKey rondreis 36..95');
  check(nearestKey(xOfMidi(61, balk) + 0.3, balk) === 61, 'nearestKey: dicht bij cis blijft cis');
}

// ---- splitSystems: hele maten, laatste korter ----
{
  const model = { q: 4, measure_len: 16, beats_per_bar: 4, num_measures: 19 };
  const sys = splitSystems(model, 8);
  check(sys.length === 3 && sys[0].bars === 8 && sys[2].bars === 3, 'splitSystems 19 maten → 8+8+3');
  check(sys[1].t0 === 128 && sys[1].t1 === 256, 'systeemgrenzen in rastereenheden');
  check(splitSystems({ q: 4, measure_len: 16, beats_per_bar: 4, num_measures: 0 }, 8).length === 1, 'lege partituur: één systeem');
}

function model(noten, extra = {}) {
  return {
    generation: null, title: 'T', bpm: 60, q: 4, beats_per_bar: 4, measure_len: 16, num_measures: 2,
    legend: [], manual: { midi_min: 48, midi_max: 83, notes: noten }, pedal: null, ...extra,
  };
}
function noot(midi, start, end, hand = 'right', extra = {}) {
  return { id: null, layer_id: null, midi, start, end, hand, stop: false, dots: [], bar_crossings: [], beam: null, label: null, ...extra };
}

// ---- layoutSystem: akkoordlijn, verstrengeling, stop, stippen ----
{
  const m = model([
    noot(60, 0, 4, 'right', { id: 1 }), noot(64, 0, 4, 'right', { id: 2 }), noot(67, 0, 4, 'right', { id: 3 }),
    noot(62, 0, 4, 'left', { id: 4 }),                       // verstrengeld: tussen 60 en 67
    noot(72, 8, 12, 'right', { id: 5, stop: true, dots: [10] }),
    noot(48, 8, 24, 'left', { id: 6, bar_crossings: [16] }),  // kruist de maatstreep op 16
  ]);
  const sys = splitSystems(m, 1);
  const lay = layoutSystem(m, sys[0]);
  check(lay.noten.length === 6, 'noten van het eerste systeem');
  check(lay.akkoordlijnen.length === 0, 'verstrengeld akkoord: geen doorgetrokken stok');
  const n60 = lay.noten.find(n => n.midi === 60);
  const n62 = lay.noten.find(n => n.midi === 62);
  check(Math.abs(n60.stokX1 - n60.stokX0) <= MAAT.stokKort * MAAT.w + 1e-9 && n60.stokX1 <= n62.cx - 0.5 * MAAT.w + 1e-9,
    'verstrengeling: korte stokken, en nooit door de vreemde kop');
  check(lay.stops.length === 1 && dicht(lay.stops[0].y, MAAT.randBoven + 3 * MAAT.kwart), 'stopteken op het einde (12 eenheden = 3 kwarten)');
  check(lay.stippen.length === 1 && dicht(lay.stippen[0].y, MAAT.randBoven + 2.5 * MAAT.kwart), 'doorklinkstip op het rastermoment');
  check(lay.telnummers.length === 4 && lay.telstrepen.filter(t => t.gestreept).length === 3, 'eerste maat: telnummers en gestreepte telstrepen');
  const n64 = lay.noten.find(n => n.midi === 64);
  check(!n64.zwart && n64.cy > n64.yStok, 'witte kop hangt onder de stok');
  const lay2 = layoutSystem(m, sys[1]);
  check(lay2.noten.length === 0 && lay2.telnummers.length === 0, 'tweede systeem: geen noten die daar beginnen, geen telnummers');
  check(lay2.telstrepen.length > 0 && lay2.telstrepen.every(t => !t.gestreept), 'latere maat: korte streepjes');
  check(lay2.stippen.length === 1 && lay2.stippen[0].y < MAAT.randBoven + 2, 'stip onder de bovenste maatstreep bij een kruisende noot');
  // Akkoord zonder verstrengeling: één lijn met uitsteek aan de handzijde.
  const m2 = model([noot(60, 0, 4, 'right'), noot(64, 0, 4, 'right')]);
  const l2 = layoutSystem(m2, splitSystems(m2, 1)[0]);
  check(l2.akkoordlijnen.length === 1 && l2.akkoordlijnen[0].x1 > l2.noten[1].cx, 'akkoordlijn steekt rechts uit');
  check(l2.noten.every(n => n.inAkkoord), 'akkoordnoten zonder eigen stok');
  // Hit-test en klik-naar-tijd.
  const hit = hitTest(l2, l2.noten[0].cx + 0.3, l2.noten[0].cy, 1.5 * MAAT.w);
  check(hit && hit.midi === 60, 'hitTest vindt de dichtstbijzijnde kop');
  check(gridTimeAtY(l2, MAAT.randBoven + 1.6 * MAAT.kwart) === 6, 'gridTimeAtY snapt op het raster (1,6 kwart → 6 zestienden)');
}

// ---- gekruiste handen op dezelfde inzet: stokken raken elkaar nooit ----
{
  const w = MAAT.w;
  // (a) losse noten: rechts e' (64) en links f' (65)
  let m = model([noot(64, 0, 4, 'right'), noot(65, 0, 4, 'left')]);
  let lay = layoutSystem(m, splitSystems(m, 1)[0]);
  let n64 = lay.noten.find(n => n.midi === 64), n65 = lay.noten.find(n => n.midi === 65);
  check(n64.stokX1 <= n65.cx - 0.5 * w + 1e-9 && n64.stokX1 > n64.cx, 'gekruist (a): rechterstok stopt vóór de linkerkop');
  check(n65.stokX1 >= n64.cx + 0.5 * w - 1e-9 && n65.stokX1 < n65.cx, 'gekruist (a): linkerstok stopt vóór de rechterkop');
  // (b) rechts akkoord c'+e', links g'
  m = model([noot(60, 0, 4, 'right'), noot(64, 0, 4, 'right'), noot(67, 0, 4, 'left')]);
  lay = layoutSystem(m, splitSystems(m, 1)[0]);
  const n67 = lay.noten.find(n => n.midi === 67);
  check(lay.akkoordlijnen.length === 1 && lay.akkoordlijnen[0].x1 <= n67.cx - 0.5 * w + 1e-9, 'gekruist (b): akkoordlijn stopt vóór de linkerkop');
  check(n67.stokX1 >= lay.noten.find(n => n.midi === 64).cx + 0.5 * w - 1e-9, 'gekruist (b): linkerstok stopt vóór het akkoord');
  // (c) verstrengeld: rechts c'+g' met links e' ertussen
  m = model([noot(60, 0, 4, 'right'), noot(67, 0, 4, 'right'), noot(64, 0, 4, 'left')]);
  lay = layoutSystem(m, splitSystems(m, 1)[0]);
  const c60 = lay.noten.find(n => n.midi === 60), l64 = lay.noten.find(n => n.midi === 64);
  check(c60.stokX1 <= l64.cx - 0.5 * w + 1e-9, 'gekruist (c): korte rechterstok stopt vóór de linkerkop');
  check(l64.stokX1 >= c60.cx + 0.5 * w - 1e-9, 'gekruist (c): linkerstok stopt vóór de rechterkop');
}

// ---- stip op een maatstreep ligt er net onder; geen dubbele systeemstip ----
{
  const m = model([noot(48, 8, 40, 'left', { id: 1, dots: [16, 20], bar_crossings: [16, 32] })], { num_measures: 3 });
  const sys = splitSystems(m, 1);
  const l2 = layoutSystem(m, sys[1]); // t0 = 16
  const opStreep = l2.stippen.filter(d => Math.abs(d.y - MAAT.randBoven) < 1e-6);
  check(opStreep.length === 0, 'geen stip óp de bovenste maatstreep');
  check(l2.stippen.length === 2, 'stip van de inzet op 16 (net onder de streep) + stip op 20, geen extra systeemstip');
  const l3 = layoutSystem(m, sys[2]); // t0 = 32: wél een systeemstip
  check(l3.stippen.length === 1 && l3.stippen[0].y > MAAT.randBoven, 'systeemstip bij een kruisende noot zonder inzet');
}

// ---- stopteken nooit in een open kop ----
{
  const m = model([noot(60, 0, 1, 'right', { id: 1, stop: true })]); // een zestiende
  const lay = layoutSystem(m, splitSystems(m, 1)[0]);
  const n = lay.noten[0];
  check(lay.stops.length === 1 && lay.stops[0].y - lay.stops[0].h >= n.yStok + MAAT.kop * MAAT.w - 1e-9, 'stopteken van een zestiende begint onder de kop');
}

// ---- pedaalbalk links, staffAtX ----
{
  const m = model([noot(60, 0, 4)], { pedal: { midi_min: 36, midi_max: 59, notes: [noot(36, 0, 4, 'pedal', { id: 9 })] } });
  const lay = layoutSystem(m, splitSystems(m, 1)[0]);
  check(lay.pedaal && lay.pedaal.x0 < lay.manuaal.x0, 'pedaalbalk links van de manuaalbalk');
  check(staffAtX(lay, lay.pedaal.x0 + 1) === 'pedal' && staffAtX(lay, lay.manuaal.x0 + 1) === 'manual', 'staffAtX');
  const p = lay.noten.find(n => n.balk === 'pedal');
  check(p && p.dir === -1 && p.stokX1 < p.stokX0, 'pedaalnoot: stok naar links');
  check(lay.lijnen.some(l => l.gestreept && l.midi === 61) && !lay.lijnen.some(l => l.gestreept && l.midi < 48), 'stippellijnen alleen bij cis\'/dis\' van het manuaal');
}

// ---- paginate: kolommen en leesvolgorde ----
{
  const smal = Array.from({ length: 5 }, (_, i) => ({ sys: { index: i }, breedte: 70, hoogte: 100 }));
  const p = paginate(smal);
  check(p.length === 2 && p[0].kolommen.length === 2 && p[0].kolommen[0].length === 2 && p[0].kolommen[1].length === 2 && p[1].kolommen[0].length === 1,
    'paginate: twee kolommen, 2+2 op pagina 1, 1 op pagina 2');
  const volgorde = p.flatMap(pg => pg.kolommen.flatMap(k => k.map(l => l.sys.index)));
  check(volgorde.join(',') === '0,1,2,3,4', 'leesvolgorde per kolom');
  const breed = Array.from({ length: 3 }, (_, i) => ({ sys: { index: i }, breedte: 120, hoogte: 100 }));
  check(paginate(breed)[0].kolommen.length === 1, 'brede systemen: één kolom');
  const hoog = [{ sys: { index: 0 }, breedte: 70, hoogte: 300 }];
  check(paginate(hoog).length === 1 && paginate(hoog)[0].kolommen[0].length === 1, 'een te hoog systeem krijgt toch een kolom');
}

if (fouten) { console.log(`${fouten} fout(en)`); process.exit(1); }
console.log('alle klavar-geometrietests slagen');
