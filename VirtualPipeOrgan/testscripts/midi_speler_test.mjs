// Test van de afspeelknop naast de MIDI-knop (ui/src/lib/midiSpeler.js), zonder DOM. Draait met `node` en in CI.
import { bestandsNaam, speeltNu, afspeelStappen } from '../ui/src/lib/midiSpeler.js';

let fouten = 0;
function check(cond, naam) {
  if (cond) { console.log('ok   ' + naam); }
  else { console.log('FOUT ' + naam); fouten++; }
}
const is = (a, b) => JSON.stringify(a) === JSON.stringify(b);
const stil = { state: 'stopped' };
const speelt = { state: 'playing' };
const pauze = { state: 'paused' };
const A = 'C:\\Muziek\\a.mid';
const B = 'D:\\Anders\\b.mid';

// ---- bestandsnaam ----
check(bestandsNaam('C:\\Muziek\\Bach\\toccata.mid') === 'toccata.mid', 'naam uit een Windows-pad');
check(bestandsNaam('/Users/x/Muziek/toccata.mid') === 'toccata.mid', 'naam uit een pad met schuine strepen');
check(bestandsNaam('') === '' && bestandsNaam(null) === '' && bestandsNaam(undefined) === '', 'geen pad geeft een lege naam');

// ---- wat toont de knop ----
check(speeltNu(speelt, A, A) === true, 'speelt het eigen bestand: Stop');
check(speeltNu(speelt, B, A) === false, 'speelt een ander bestand: Afspelen');
check(speeltNu(speelt, null, A) === false, 'nog niets gestart door de knop: Afspelen');
check(speeltNu(stil, A, A) === false, 'gestopt: Afspelen');
check(speeltNu(pauze, A, A) === false, 'gepauzeerd: Afspelen (een klik hervat)');
check(speeltNu(speelt, A, null) === false && speeltNu(undefined, A, A) === false, 'geen bestand of geen status: Afspelen');

// ---- wat doet een klik ----
check(is(afspeelStappen(stil, null, A), ['speel']), 'stil: het bestand afspelen');
check(is(afspeelStappen(speelt, A, A), ['stop']), 'speelt het eigen bestand: stoppen');
check(is(afspeelStappen(pauze, A, A), ['hervat']), 'eigen bestand gepauzeerd: hervatten');
check(is(afspeelStappen(speelt, B, A), ['stop', 'speel']), 'een ander bestand speelt: eerst stoppen, dan het eigen bestand');
check(is(afspeelStappen(pauze, B, A), ['stop', 'speel']), 'een ander bestand is gepauzeerd: niet hervatten maar het eigen bestand starten');
check(is(afspeelStappen(speelt, null, A), ['stop', 'speel']), 'er speelt iets van elders: eerst stoppen');
check(is(afspeelStappen(undefined, null, A), ['speel']), 'geen status: gewoon afspelen');
// Een nieuw geopend bestand na een afgespeeld bestand: de speler is stil, dus direct afspelen.
check(is(afspeelStappen(stil, A, B), ['speel']), 'ander bestand geopend na een afgespeeld bestand: afspelen');

if (fouten) { console.log('\n' + fouten + ' fout(en)'); process.exit(1); }
console.log('\nalle controles goed');
