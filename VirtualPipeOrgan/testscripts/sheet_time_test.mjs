// Klik → rastertijd (0.7.87): rondreis en randgevallen van xToGridTime.
// Draait met: node testscripts/sheet_time_test.mjs
import { xToGridTime, gridTimeToX, celBreedte, snapGridTime } from '../ui/src/lib/sheetGeometry.js';
let fouten = 0;
function check(naam, ok) { console.log((ok ? 'ok   ' : 'FOUT ') + naam); if (!ok) fouten++; }

// Lege maat, 16 eenheden over 160 px: lineair.
check('lege maat: begin', xToGridTime([], 100, 260, 100, 16) === 0);
check('lege maat: midden', xToGridTime([], 100, 260, 180, 16) === 8);
check('lege maat: vlak voor het einde', xToGridTime([], 100, 260, 259, 16) === 15);
check('lege maat: voorbij het einde klemt', xToGridTime([], 100, 260, 400, 16) === 15);
check('lege maat: vóór het begin', xToGridTime([], 100, 260, 10, 16) === 0);
// Vier kwarten op 120/160/200/240 px (t = 0, 4, 8, 12).
const kwarten = [{ x: 120, t: 0 }, { x: 160, t: 4 }, { x: 200, t: 8 }, { x: 240, t: 12 }];
check('op een inzet', xToGridTime(kwarten, 100, 260, 160, 16) === 4);
check('tussen twee inzetten: de achtste ertussen', xToGridTime(kwarten, 100, 260, 180, 16) === 6);
check('vóór de eerste inzet blijft tel 1', xToGridTime(kwarten, 100, 260, 110, 16) === 0);
check('na de laatste inzet: naar de maatstreep toe', xToGridTime(kwarten, 100, 260, 250, 16) === 14);
check('rondreis over de inzetten', kwarten.every(p => xToGridTime(kwarten, 100, 260, p.x, 16) === p.t));
// Ongeordende en vreemde invoer.
check('ongeordende entries', xToGridTime([kwarten[2], kwarten[0]], 100, 260, 160, 16) === 4);
check('entries buiten de maat genegeerd', xToGridTime([{ x: 50, t: 3 }, { x: 300, t: 2 }], 100, 260, 180, 16) === 8);
check('ongeldige breedte → 0', xToGridTime(kwarten, 260, 100, 180, 16) === 0);
check('celbreedte', Math.abs(celBreedte(100, 260, 16) - 10) < 1e-9);
// Omgekeerd: rastertijd → x, met dezelfde ankers (caret op de inzet van de klik).
check('x van inzet 2', gridTimeToX(kwarten, 100, 260, 4, 16) === 160);
check('x van tel 1 = eerste inzet, niet de maatstreep', gridTimeToX(kwarten, 100, 260, 0, 16) === 120);
check('x tussen inzetten', gridTimeToX(kwarten, 100, 260, 6, 16) === 180);
check('x na de laatste inzet', gridTimeToX(kwarten, 100, 260, 14, 16) === 250);
check('lege maat lineair', gridTimeToX([], 100, 260, 8, 16) === 180);
check('rondreis klik → tijd → x', [120, 160, 180, 200, 240].every(x => gridTimeToX(kwarten, 100, 260, xToGridTime(kwarten, 100, 260, x, 16), 16) === x));
// Klik in stapinvoer: op de nootwaarde of een bestaande inzet (0.7.87).
check('kwart in lege maat: 7 → tel 3 (8)', snapGridTime(7, [], 4, 16) === 8);
check('kwart in lege maat: 5 → tel 2 (4)', snapGridTime(5, [], 4, 16) === 4);
check('gelijke afstand → vroegste', snapGridTime(6, [], 4, 16) === 4);
check('achtste: 7 → 6', snapGridTime(7, [], 2, 16) === 6);
check('zestiende blijft', snapGridTime(7, [], 1, 16) === 7);
check('bestaande inzet wint als dichterbij', snapGridTime(7, [{ x: 0, t: 6 }], 4, 16) === 6);
check('hele noot in 3/4 → maatbegin', snapGridTime(11, [], 16, 12) === 0);
check('voorbij het einde klemt', snapGridTime(40, [], 4, 16) === 12);
if (fouten) { console.error(`${fouten} fout(en)`); process.exit(1); }
console.log('alle klik-op-tel-tests slagen');
