// Bewijs dat ui/src/lib/eqCurve.js (de grafiek) precies rekent wat
// vpo_audio::effects (de audiothread) doet: coëfficiënten en respons per band,
// de K240-keten en de Auto-voorversterking, tegen een fixture die de Rust-test
// schrijf_fixture_eq_curve heeft weggeschreven (testscripts/testdata).
// Draait in CI (tests.yml) en lokaal: node testscripts/eq_curve_test.mjs
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import path from 'node:path';
import { biquadCoeffs, magnitudeDb, compositeDb, autoPreampDb, qToBw, bwToQ, eqRaster, bandenVoorKanaal, schaalBanden } from '../ui/src/lib/eqCurve.js';

const hier = path.dirname(fileURLToPath(import.meta.url));
const fx = JSON.parse(readFileSync(path.join(hier, 'testdata', 'eq_curve_fixture.json'), 'utf8'));
let fouten = 0;
function check(naam, ok, detail = '') {
  console.log((ok ? 'OK   ' : 'FOUT ') + naam + (detail ? '  ' + detail : ''));
  if (!ok) fouten += 1;
}

// Per band: coëfficiënten binnen 1e-5 (Rust rekent in f32), |H| binnen 0,01 dB.
let maxCo = 0, maxDb = 0;
for (const b of fx.banden) {
  const band = { enabled: true, band_type: b.type, freq: b.freq, gain_db: b.gain_db, bandwidth: b.bandwidth, q: b.q, channel: null };
  const co = biquadCoeffs(band, b.sr);
  const js = [co.b0, co.b1, co.b2, co.a1, co.a2];
  for (let i = 0; i < 5; i++) maxCo = Math.max(maxCo, Math.abs(js[i] - b.coeffs[i]));
  fx.freqs.forEach((f, i) => { maxDb = Math.max(maxDb, Math.abs(magnitudeDb(co, f, b.sr) - b.db[i])); });
}
check(`coëfficiënten van ${fx.banden.length} banden = Rust (max afwijking ${maxCo.toExponential(2)})`, maxCo < 1e-5);
check(`respons van ${fx.banden.length} banden × ${fx.freqs.length} frequenties = Rust (max ${maxDb.toFixed(4)} dB)`, maxDb < 0.01);

// K240-keten met Auto-voorversterking.
const K240 = [
  ['lowshelf', 105, 6.3, 0.70], ['peak', 204, -3.2, 0.33], ['peak', 4207, 5.2, 3.07], ['peak', 1585, 5.4, 3.15],
  ['peak', 75, 2.9, 1.48], ['highshelf', 10000, 1.8, 0.70], ['peak', 6459, -2.3, 2.56], ['peak', 5428, 2.7, 3.81],
  ['peak', 2689, -1.7, 4.63], ['peak', 9322, -2.0, 2.55],
].map(([t, f, g, q]) => ({ enabled: true, band_type: t, freq: f, gain_db: g, bandwidth: qToBw(q), q, channel: null }));
const pre = autoPreampDb(K240, 2, fx.k240.sr);
check(`Auto-voorversterking K240 = Rust (${pre.toFixed(4)} vs ${fx.k240.auto_preamp_db.toFixed(4)})`, Math.abs(pre - fx.k240.auto_preamp_db) < 0.005);
let maxK = 0;
fx.freqs.forEach((f, i) => { maxK = Math.max(maxK, Math.abs(compositeDb(K240, f, fx.k240.sr, 0, pre) - fx.k240.db_met_preamp[i])); });
check(`K240-keten incl. preamp = Rust (max ${maxK.toFixed(4)} dB)`, maxK < 0.01);

// Hulpfuncties.
check('qToBw/bwToQ rondreis', [0.1, 0.7071, 1, 3, 10].every((q) => Math.abs(bwToQ(qToBw(q)) - q) < 1e-9));
check('eqRaster: 480 punten van 20 Hz tot 20 kHz', eqRaster(480).length === 480 && Math.abs(eqRaster(480)[0] - 20) < 1e-9 && Math.abs(eqRaster(480)[479] - 20000) < 1e-6);
check('autoPreampDb: alleen verzwakking → 0 (geen −0)', Object.is(autoPreampDb([{ enabled: true, band_type: 'peak', freq: 1000, gain_db: -6, bandwidth: 1, q: 1, channel: null }], 2, 48000), 0));
check('autoPreampDb: band op kanaal 1 telt mee', Math.abs(autoPreampDb([{ enabled: true, band_type: 'peak', freq: 1000, gain_db: 6, bandwidth: 1, q: 1, channel: 1 }], 2, 48000) + 6) < 0.01);
check('bandenVoorKanaal: weergave Alle laat kanaalbanden weg, kanaal 1 neemt ze mee',
  bandenVoorKanaal([{ enabled: true, channel: null }, { enabled: true, channel: 1 }, { enabled: false, channel: null }], null).length === 1
  && bandenVoorKanaal([{ enabled: true, channel: null }, { enabled: true, channel: 1 }, { enabled: true, channel: 0 }], 1).length === 2);

// Sterkte (0.7.80): 50 % halveert elke gain, de Auto-voorversterking rekent opnieuw, 0 % = vlak, 100 % = dezelfde objecten.
const half = schaalBanden(K240, 50);
check('schaalBanden 50 %: gains gehalveerd, bron ongemoeid', half.every((b, i) => Math.abs(b.gain_db - K240[i].gain_db / 2) < 1e-9) && K240[0].gain_db === 6.3);
check('schaalBanden 50 %: Auto-preamp opnieuw (≈ helft)', Math.abs(autoPreampDb(half, 2, 48000) - pre / 2) < 0.6, `${autoPreampDb(half, 2, 48000).toFixed(3)} vs ${(pre / 2).toFixed(3)}`);
check('schaalBanden 0 %: vlak', Math.abs(compositeDb(schaalBanden(K240, 0), 1000, 48000, 0, 0)) < 1e-9);
check('schaalBanden 100 %: dezelfde array', schaalBanden(K240, 100) === K240);

console.log(fouten ? `${fouten} FOUTEN` : 'alles OK');
process.exit(fouten ? 1 : 0);
