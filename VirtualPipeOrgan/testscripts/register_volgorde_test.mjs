// Controle van het overnemen van de registervolgorde uit het orgelbestand
// (remote_layout.stop_order) voor een orgel zonder lokale volgorde.
// Draait in CI (tests.yml) en lokaal: node testscripts/register_volgorde_test.mjs
import { volgordeUitOrgelbestand } from '../ui/src/lib/registerVolgorde.js';

let fouten = 0;
function check(naam, ok) {
  console.log(`${ok ? 'OK  ' : 'FOUT'} ${naam}`);
  if (!ok) fouten++;
}
const gelijk = (a, b) => JSON.stringify(a) === JSON.stringify(b);
const div = (name, ...ids) => ({ name, stops: ids.map((id) => ({ id })) });

const orgel = [div('Hoofdwerk', 'h1', 'h2', 'h3'), div('Pedaal', 'p1', 'p2')];

// Niet gesorteerd: de gepubliceerde volgorde is die van de set; niets overnemen.
check('ongesorteerd orgel neemt niets over',
  gelijk(volgordeUitOrgelbestand({ Hoofdwerk: ['h1', 'h2', 'h3'], Pedaal: ['p1', 'p2'] }, orgel), {}));

// Eén divisie gesorteerd: alleen die divisie.
check('alleen de gesorteerde divisie komt mee',
  gelijk(volgordeUitOrgelbestand({ Hoofdwerk: ['h3', 'h1', 'h2'], Pedaal: ['p1', 'p2'] }, orgel), { Hoofdwerk: ['h3', 'h1', 'h2'] }));

// Het scherm toont soms minder registers (effectregisters verborgen): de volgorde
// van de getoonde registers telt, een weggelaten register maakt het niet "gesorteerd".
check('weggelaten register telt niet als sorteren',
  gelijk(volgordeUitOrgelbestand({ Hoofdwerk: ['h1', 'h3'] }, orgel), {}));
check('weggelaten register, wel gesorteerd',
  gelijk(volgordeUitOrgelbestand({ Hoofdwerk: ['h3', 'h1'] }, orgel), { Hoofdwerk: ['h3', 'h1'] }));

// Registers die de set niet meer heeft vallen weg; dubbelen ook.
check('verdwenen en dubbele registers vallen weg',
  gelijk(volgordeUitOrgelbestand({ Hoofdwerk: ['weg', 'h2', 'h2', 'h1'] }, orgel), { Hoofdwerk: ['h2', 'h1'] }));
check('alleen verdwenen registers: niets',
  gelijk(volgordeUitOrgelbestand({ Hoofdwerk: ['weg1', 'weg2'] }, orgel), {}));

// Divisies die het orgel niet (meer) heeft worden genegeerd.
check('onbekende divisie genegeerd',
  gelijk(volgordeUitOrgelbestand({ Bovenwerk: ['b2', 'b1'] }, orgel), {}));

// Robuust tegen ontbrekende of kapotte invoer (een .jm-settings.json kan van elders komen).
check('geen opgeslagen volgorde', gelijk(volgordeUitOrgelbestand(null, orgel), {}));
check('geen divisies', gelijk(volgordeUitOrgelbestand({ Hoofdwerk: ['h2', 'h1'] }, null), {}));
check('verkeerde typen', gelijk(volgordeUitOrgelbestand({ Hoofdwerk: 'h2,h1', Pedaal: [1, 2] }, orgel), {}));
check('divisie zonder registers', gelijk(volgordeUitOrgelbestand({ Leeg: ['x'] }, [{ name: 'Leeg' }]), {}));

// Nieuwe bestanden bewaren expliciete keuzes ook als die gelijk zijn aan de set.
check('expliciete natuurlijke volgorde blijft een keuze',
  gelijk(volgordeUitOrgelbestand({ Hoofdwerk: ['h1', 'h2', 'h3'] }, orgel, { Hoofdwerk: ['h1', 'h2', 'h3'] }), { Hoofdwerk: ['h1', 'h2', 'h3'] }));
check('expliciet lege keuze onderdrukt de oude schermvolgorde',
  gelijk(volgordeUitOrgelbestand({ Hoofdwerk: ['h3', 'h1', 'h2'] }, orgel, {}), {}));
check('expliciete keuze verwijdert verdwenen en dubbele registers',
  gelijk(volgordeUitOrgelbestand(null, orgel, { Hoofdwerk: ['weg', 'h1', 'h1', 'h2'] }), { Hoofdwerk: ['h1', 'h2'] }));

if (fouten) {
  console.error(`${fouten} controle(s) mislukt`);
  process.exit(1);
}
console.log('register_volgorde_test: alles in orde');
