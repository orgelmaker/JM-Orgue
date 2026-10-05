// Controle van de EQ-presetimport (0.7.77): AutoEq/Equalizer APO-tekst,
// Sweelinq .swes (echt bestand als fixture + zelfgebouwde bytes) en eigen JSON.
// Draait in CI (tests.yml) en lokaal: node testscripts/eq_import_test.mjs
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import path from 'node:path';
import { parseParametricEqTxt, parseSwes, parseEigenJson, herkenEnParse, bwNaarQ, exportParametricEqTxt, exportEigenJson } from '../ui/src/lib/eqImport.js';

const hier = path.dirname(fileURLToPath(import.meta.url));
let fouten = 0;
function check(naam, ok, detail = '') {
  console.log((ok ? 'OK   ' : 'FOUT ') + naam + (detail ? '  ' + detail : ''));
  if (!ok) fouten += 1;
}

// AutoEq oratory1990 "AKG K240 Studio" ParametricEQ.txt
const K240 = `Preamp: -6.2 dB
Filter 1: ON LSC Fc 105 Hz Gain 6.3 dB Q 0.70
Filter 2: ON PK Fc 204 Hz Gain -3.2 dB Q 0.33
Filter 3: ON PK Fc 4207 Hz Gain 5.2 dB Q 3.07
Filter 4: ON PK Fc 1585 Hz Gain 5.4 dB Q 3.15
Filter 5: ON PK Fc 75 Hz Gain 2.9 dB Q 1.48
Filter 6: ON HSC Fc 10000 Hz Gain 1.8 dB Q 0.70
Filter 7: ON PK Fc 6459 Hz Gain -2.3 dB Q 2.56
Filter 8: ON PK Fc 5428 Hz Gain 2.7 dB Q 3.81
Filter 9: ON PK Fc 2689 Hz Gain -1.7 dB Q 4.63
Filter 10: ON PK Fc 9322 Hz Gain -2.0 dB Q 2.55
`;
const r = parseParametricEqTxt(K240);
check('K240: preamp -6,2', r.preamp === -6.2, String(r.preamp));
check('K240: 10 banden', r.banden.length === 10, String(r.banden.length));
check('K240: types', r.banden[0].band_type === 'lowshelf' && r.banden[1].band_type === 'peak' && r.banden[5].band_type === 'highshelf');
check('K240: Q exact', r.banden[2].q === 3.07 && r.banden[8].q === 4.63);
check('K240: freq/gain', r.banden[3].freq === 1585 && r.banden[3].gain_db === 5.4);
check('K240: bandbreedte uit Q', Math.abs(r.banden[0].bandwidth - (2 / Math.LN2) * Math.asinh(1 / 1.4)) < 1e-9);
check('K240: alle banden aan, kanaal null', r.banden.every((b) => b.enabled && b.channel === null));

const off = parseParametricEqTxt('Filter 1: OFF PK Fc 100 Hz Gain 1 dB Q 1');
check('OFF → enabled false', off.banden.length === 1 && off.banden[0].enabled === false);

// Q = 2^(N/2) / (2^N - 1): één octaaf = 1,414, twee octaven = 0,667.
const bw = parseParametricEqTxt(['Filter 1: ON PK Fc 1000 Hz Gain 3 dB BW Oct 1.0', 'Filter 2: ON PK Fc 1000 Hz Gain 3 dB BW Oct 2'].join('\n'));
check('BW Oct 1,0 → Q 1,414', Math.abs(bw.banden[0].q - bwNaarQ(1.0)) < 1e-9 && Math.abs(bw.banden[0].q - 1.4142) < 0.001, String(bw.banden[0].q));
check('BW Oct 2,0 → Q 0,667', Math.abs(bw.banden[1].q - 0.6667) < 0.001, String(bw.banden[1].q));

const lp = parseParametricEqTxt('Filter 1: ON LP Fc 8000 Hz\nFilter 2: ON HPQ Fc 40 Hz Q 0.9');
check('LP/HPQ zonder gain', lp.banden.length === 2 && lp.banden[0].band_type === 'lowpass' && lp.banden[1].band_type === 'highpass' && lp.banden[1].q === 0.9);

const veel = ['Preamp: 0 dB', 'Channel: L', 'Filter 1: ON XX Fc 1 Hz', 'kapot', '# commentaar']
  .concat(Array.from({ length: 40 }, (_, i) => `Filter ${i + 2}: ON PK Fc ${100 + i} Hz Gain 1 dB Q 1`)).join('\r\n');
const v = parseParametricEqTxt(veel);
check('cap 32 banden', v.banden.length === 32, String(v.banden.length));
check('kanaalregel geteld', v.kanaalregels === 1);
check('onbekend type + overschot overgeslagen', v.overgeslagen === 9, String(v.overgeslagen));

const klem = parseParametricEqTxt('Filter 1: ON PK Fc 50000 Hz Gain 40 dB Q 99');
check('klemming f/gain/Q', klem.banden[0].freq === 20000 && klem.banden[0].gain_db === 24 && klem.banden[0].q === 20);

let gooide = false;
try { parseParametricEqTxt('hallo'); } catch (e) { gooide = e.message === 'geen_filters'; }
check('geen filters → geen_filters', gooide);

// Sweelinq .swes: het echte bestand (fixture) …
const swes = new Uint8Array(readFileSync(path.join(hier, 'testdata', 'preset_dt770.swes')));
const s = parseSwes(swes);
check('swes: 11 peaks', s.banden.length === 11 && s.banden.every((b) => b.band_type === 'peak'), String(s.banden.length));
const verwacht = [37.8, 61.4, 85.5, 114.5, 220, 364, 777, 1360, 2150, 3474, 9285];
check('swes: frequenties', s.banden.every((b, i) => Math.abs(b.freq - verwacht[i]) < 0.1), s.banden.map((b) => b.freq.toFixed(1)).join(','));
check('swes: gains en Q binnen bereik', s.banden.every((b) => Math.abs(b.gain_db) <= 24 && b.q >= 0.05 && b.q <= 20));
check('swes: aan', s.aan === true);
check('swes: geen preamp in het bestand', s.preamp === null);
let afgekapt = false;
try { parseSwes(swes.subarray(0, swes.length - 5)); } catch (e) { afgekapt = e.message === 'afgekapt'; }
check('swes afgekapt → afgekapt', afgekapt);

// … en zelfgebouwde bytes met een onbekend veld (6, varint) en EQ uit.
function f32(x) { const b = new Uint8Array(4); new DataView(b.buffer).setFloat32(0, x, true); return [...b]; }
const band = [0x15, ...f32(1000), 0x1d, ...f32(-3), 0x25, ...f32(2), 0x28, 1, 0x30, 7];
const bytes = new Uint8Array([0x0a, band.length, ...band, 0x10, 0]);
const z = parseSwes(bytes);
check('swes zelfgebouwd: 1 band, onbekend veld overgeslagen, aan=false',
  z.banden.length === 1 && z.banden[0].freq === 1000 && z.banden[0].gain_db === -3 && z.banden[0].q === 2 && z.aan === false,
  JSON.stringify(z.banden[0]));
let leeg = false;
try { parseSwes(new Uint8Array([0x10, 1])); } catch (e) { leeg = e.message === 'geen_filters'; }
check('swes zonder banden → geen_filters', leeg);

// Eigen JSON
const j = parseEigenJson(JSON.stringify({ enabled: true, preamp_db: -2, preamp_auto: false, bands: [
  { enabled: true, band_type: 'lowshelf', freq: 100, gain_db: 3, bandwidth: 1.0, q: null, channel: 1 },
  { enabled: false, type: 'PK', freq: 2000, gain: -1, q: 2 },
] }));
check('json: banden, kanaal, preamp', j.banden.length === 2 && j.banden[0].channel === 1 && j.preamp === -2
  && Math.abs(j.banden[0].q - bwNaarQ(1.0)) < 1e-9 && j.banden[1].band_type === 'peak' && j.banden[1].enabled === false);
let jf = false;
try { parseEigenJson('{"iets":1}'); } catch (e) { jf = e.message === 'onbekend_formaat'; }
check('json zonder banden → onbekend_formaat', jf);

// Herkenning op extensie en inhoud
const h = herkenEnParse('C:\\x\\preset Beyerdynamic DT770 Pro.swes', swes);
check('herken: .swes + naam zonder voorvoegsel', h.banden.length === 11 && h.naam === 'Beyerdynamic DT770 Pro', h.naam);
const h2 = herkenEnParse('AKG K240 Studio ParametricEQ.txt', new TextEncoder().encode(K240));
check('herken: .txt', h2.banden.length === 10 && h2.naam === 'AKG K240 Studio ParametricEQ');
const h3 = herkenEnParse('iets.dat', [...new TextEncoder().encode(K240)]);
check('herken: tekstinhoud zonder bekende extensie (gewone array)', h3.banden.length === 10);
const h4 = herkenEnParse('x.bin', bytes);
check('herken: swes-bytes zonder extensie', h4.banden.length === 1);
let onb = false;
try { herkenEnParse('x.dat', new TextEncoder().encode('zomaar tekst')); } catch (e) { onb = e.message === 'onbekend_formaat'; }
check('onbekend → onbekend_formaat', onb);

// Exporteren (0.7.80): rondreis tekst → banden → tekst → banden is gelijk.
const uit = exportParametricEqTxt(r.banden, r.preamp);
check('export txt: preamp-regel en 10 filters', uit.tekst.startsWith('Preamp: -6.2 dB\n') && (uit.tekst.match(/^Filter \d+: ON/gm) || []).length === 10 && uit.overgeslagen === 0);
const terug = parseParametricEqTxt(uit.tekst);
check('export txt: rondreis gelijk', terug.preamp === -6.2 && terug.banden.length === 10
  && terug.banden.every((b, i) => b.band_type === r.banden[i].band_type && Math.abs(b.freq - r.banden[i].freq) < 0.05
    && Math.abs(b.gain_db - r.banden[i].gain_db) < 0.05 && Math.abs(b.q - r.banden[i].q) < 0.005));
const gemengd = exportParametricEqTxt([
  { enabled: false, band_type: 'peak', freq: 100, gain_db: -2, q: 1 },
  { enabled: true, band_type: 'lowpass', freq: 8000, gain_db: 0, q: 0.7 },
  { enabled: true, band_type: 'highshelf', freq: 10000, gain_db: 1.5, bandwidth: 1.0, q: null },
], 0);
check('export txt: OFF-band, lowpass weggelaten, Q uit bandbreedte', /^Filter 1: OFF PK Fc 100 Hz/m.test(gemengd.tekst) && /^Filter 2: ON HSC Fc 10000 Hz Gain 1.5 dB Q 1.41/m.test(gemengd.tekst) && gemengd.overgeslagen === 1, gemengd.tekst);
const jsonUit = exportEigenJson(s.banden, null, true, 'DT770');
const jsonTerug = parseEigenJson(jsonUit);
check('export json: rondreis 11 banden, naam, Auto', jsonTerug.banden.length === 11 && JSON.parse(jsonUit).naam === 'DT770' && jsonTerug.preampAuto === true
  && jsonTerug.banden.every((b, i) => Math.abs(b.freq - s.banden[i].freq) < 1e-6 && Math.abs(b.q - s.banden[i].q) < 1e-6));

console.log(fouten ? `${fouten} FOUTEN` : 'alles OK');
process.exit(fouten ? 1 : 0);
