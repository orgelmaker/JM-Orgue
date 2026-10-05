// Controle van de gebundelde hoofdtelefoonpresets (0.7.77): de database laadt,
// de kop draagt licentie en commit, zoeken werkt zoals bedoeld en de K240-
// preset komt er exact uit zoals AutoEq hem geeft.
// Draait in CI (tests.yml) en lokaal: node testscripts/eq_presets_test.mjs
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import path from 'node:path';
import { maakPresetZoeker, presetNaarBanden, presetSamenvatting } from '../ui/src/lib/eqPresetsKern.js';

const hier = path.dirname(fileURLToPath(import.meta.url));
let fouten = 0;
function check(naam, ok, detail = '') {
  console.log((ok ? 'OK   ' : 'FOUT ') + naam + (detail ? '  ' + detail : ''));
  if (!ok) fouten += 1;
}

const data = JSON.parse(readFileSync(path.join(hier, '..', 'ui', 'src', 'assets', 'eq-presets.json'), 'utf8'));
const z = maakPresetZoeker(data);

check('minstens 500 presets', z.presets.length >= 500, String(z.presets.length));
check('kop: bron, meter, licentie, commit', data.bron === 'AutoEq' && data.meter === 'oratory1990'
  && /MIT License/.test(data.licentie) && /^[0-9a-f]{40}$/.test(data.commit));
check('ids uniek', new Set(z.presets.map((p) => p.id)).size === z.presets.length);
check('gesorteerd op merk, model', z.presets.every((p, i) => i === 0
  || (z.presets[i - 1].merk + ' ' + z.presets[i - 1].model).toLowerCase() <= (p.merk + ' ' + p.model).toLowerCase()));

const k = z.zoek('k240').map((p) => p.model);
check('k240 → bevat K240 Studio en K240 MKII', k.includes('AKG K240 Studio') && k.includes('AKG K240 MKII'), k.join(' | '));
check('dt770 → DT 770 Pro', z.zoek('dt770').some((p) => p.model === 'Beyerdynamic DT 770 Pro'));
check('dt-770 (koppelteken) → ook', z.zoek('dt-770').some((p) => p.model === 'Beyerdynamic DT 770 Pro'));
check('één teken → niets', z.zoek('k').length === 0);
check('twee woorden, beide moeten voorkomen', z.zoek('sennheiser 600').some((p) => p.model === 'Sennheiser HD 600')
  && z.zoek('sennheiser 600').every((p) => /sennheiser/i.test(p.merk + p.model)));
check('max 40 resultaten standaard', z.zoek('hd').length <= 40);
check('merk zoeken', z.zoek('akg').length >= 10);

const st = z.bijId('autoeq/oratory1990/over-ear/AKG K240 Studio');
check('bijId K240 Studio', !!st && st.merk === 'AKG');
const b = presetNaarBanden(st);
check('K240 Studio: 10 banden, preamp -6,2', b.length === 10 && st.preamp === -6.2, `${b.length} / ${st?.preamp}`);
check('K240 Studio band 1 = LSC 105 Hz +6,3 dB Q 0,70', b[0].band_type === 'lowshelf' && b[0].freq === 105 && b[0].gain_db === 6.3 && b[0].q === 0.7);
check('K240 Studio band 6 = HSC 10 kHz', b[5].band_type === 'highshelf' && b[5].freq === 10000);
check('banden: aan, kanaal null, bandbreedte uit Q', b.every((x) => x.enabled && x.channel === null && x.bandwidth > 0));
check('samenvatting', presetSamenvatting(st) === '10 · -6.2 dB', presetSamenvatting(st));

check('alle presets geldig', z.presets.every((p) => p.banden.length > 0 && p.banden.length <= 32
  && p.banden.every(([t, f, g, q]) => ['peak', 'lowshelf', 'highshelf'].includes(t) && f > 0 && f <= 20000 && Math.abs(g) <= 24 && q > 0 && q <= 20)));
check('alle presets: preamp ≤ 0', z.presets.every((p) => p.preamp <= 0));
const merken = z.perMerk();
check('perMerk: AKG, Beyerdynamic, Sennheiser aanwezig', ['AKG', 'Beyerdynamic', 'Sennheiser'].every((m) => merken.some((x) => x.merk === m)));
check('perMerk: som = alle presets', merken.reduce((n, m) => n + m.presets.length, 0) === z.presets.length);
check('bron-object', z.bron.commit === data.commit && z.bron.meter === 'oratory1990');

console.log(fouten ? `${fouten} FOUTEN` : 'alles OK');
process.exit(fouten ? 1 : 0);
