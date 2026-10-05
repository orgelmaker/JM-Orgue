// Notemap-sleutel (0.7.86): dezelfde velden als de Rust-NoteRef, en de
// OSMD-tijdstempel (hele noten) → rastereenheden. Draait met: node testscripts/notemap_test.mjs
import { notemapSleutel, rasterPositie } from '../ui/src/lib/notatieKeuzes.js';
let fouten = 0;
function check(naam, ok) { console.log((ok ? 'ok   ' : 'FOUT ') + naam); if (!ok) fouten++; }
check('sleutel is stabiel', notemapSleutel(0, 3, 2, 8, 60) === '0|3|2|8|60');
check('sleutel onderscheidt stem', notemapSleutel(0, 3, 1, 8, 60) !== notemapSleutel(0, 3, 2, 8, 60));
// Vaste tabel: tijdstempel in hele noten, raster q → positie.
const tabel = [[0, 4, 0], [0.25, 4, 4], [0.125, 4, 2], [0.0625, 4, 1], [0.5, 2, 4], [0.75, 8, 24], [1 / 3, 4, 5]];
for (const [ts, q, pos] of tabel) check(`rasterPositie(${ts}, ${q}) = ${pos}`, rasterPositie(ts, q) === pos);
check('rondreis over een maat 4/4 bij q=4', [...Array(16).keys()].every(p => rasterPositie(p / 16, 4) === p));
check('onzin → 0', rasterPositie(undefined, undefined) === 0);
if (fouten) { console.error(`${fouten} fout(en)`); process.exit(1); }
console.log('alle notemap-tests slagen');
