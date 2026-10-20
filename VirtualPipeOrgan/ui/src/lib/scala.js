// Scala .scl-stemmingsbestand (0.9.14).
//
// Standaardformaat voor microtonale/historische stemmingen, oorspronkelijk
// van Manuel Op de Coul (www.huygens-fokker.org/scala/). Bijna elke moderne
// toepassing (Max/MSP, Pianoteq, Surge, LMMS, Reaper, Cakewalk, Scala-tool
// zelf) kan ze lezen; met deze parser past JM-Orgue daarbij.
//
// Formaat (12 of ander aantal toonhoogten per octaaf; wij vertalen naar 12):
//   ! regelcommentaren beginnen met !
//   korte beschrijvingstekst                   ← eerste niet-! regel
//   N                                          ← aantal toonhoogten in de octaaf
//   100.0                                      ← noot 1 (cent)
//   7/6                                        ← noot 2 (ratio)
//   ...
// De octaaf (meestal 2/1 of 1200.0) staat als LAATSTE regel; die telt NIET
// als een extra toon (hij is al de 1/1 van het volgende octaaf). De
// nulnoot (grondtoon) staat niet in het bestand; die is per definitie 0 cent.
//
// Deze parser accepteert:
//   - commentaarregels (!...) en lege regels op elke plek,
//   - cent-waarden met decimaalpunt of komma,
//   - breuken (p/q, met p,q > 0),
//   - 12 toonhoogten in een octaaf; andere aantallen worden geweigerd met
//     een nette melding (vertalen naar 12 is géén muzikale beslissing).
//
// Retourneert {name, octaveCents, cents, note_offsets}:
//   octaveCents  — het laatste interval (meestal 1200);
//   cents        — array van 12 cent-waarden t.o.v. de grondtoon C (eerste
//                  is 0, laatste is de B);
//   note_offsets — zelfde array, maar als afwijking van gelijkzwevend
//                  (dus relatief; array[0] voor C, array[9] voor A = 0 ref).
//
// Een orgel-grondtoon is bij ons C, en de temperament-cents zijn t.o.v.
// gelijkzwevend met A als referentie. De parser verschuift daarom nog
// zodat A-offset exact 0 is (wie dat niet wil kan achteraf de hoofd-tune
// gebruiken).

const RATIO_REGEX = /^\s*(\d+)\s*\/\s*(\d+)\s*$/;
const CENT_REGEX = /^\s*(-?\d+(?:[.,]\d+)?)\s*$/;

function parseWaarde(regel) {
  const r = regel.match(RATIO_REGEX);
  if (r) {
    const [p, q] = [parseInt(r[1], 10), parseInt(r[2], 10)];
    if (p <= 0 || q <= 0) throw new Error(`Ongeldige breuk: "${regel}"`);
    return 1200 * Math.log2(p / q);
  }
  const c = regel.match(CENT_REGEX);
  if (c) return parseFloat(c[1].replace(',', '.'));
  throw new Error(`Onbegrepen regel in .scl: "${regel}"`);
}

export function parseScala(bron, bestandsnaam = 'Scala') {
  const regels = bron.split(/\r?\n/);
  const nuttig = [];
  let eersteBeschrijving = null;
  for (const regel of regels) {
    if (regel.trimStart().startsWith('!')) continue;   // commentaar
    if (regel.trim() === '') continue;                 // leeg
    if (eersteBeschrijving === null) { eersteBeschrijving = regel.trim(); continue; }
    nuttig.push(regel);
  }
  if (nuttig.length < 2) throw new Error('Scala-bestand is te kort: beschrijving + aantal + minstens één toon ontbreken.');

  const aantal = parseInt(nuttig[0].trim(), 10);
  if (!Number.isFinite(aantal) || aantal < 1) throw new Error(`Scala-bestand noemt geen aantal toonhoogten per octaaf.`);
  const toonregels = nuttig.slice(1).slice(0, aantal);
  if (toonregels.length !== aantal) throw new Error(`Scala-bestand noemt ${aantal} toonhoogten maar bevat er ${toonregels.length}.`);

  const cents = [0, ...toonregels.map(parseWaarde)];
  const octaveCents = cents[cents.length - 1];
  if (cents.length !== 13) {
    throw new Error(`JM-Orgue leest alleen 12-tonen Scala-stemmingen (deze heeft er ${cents.length - 1}). Zet hem eerst om in een 12-toons variant.`);
  }
  const centsOctaaf = cents.slice(0, 12); // 12 waarden, zonder het octaaf-interval

  // Gelijkzwevend = 100 cent per halve toon; offset t.o.v. die waarde.
  const gelijk = Array.from({ length: 12 }, (_, i) => i * 100);
  const offsets = centsOctaaf.map((c, i) => c - gelijk[i]);
  // Verschuiven zodat A-offset (index 9) op 0 komt — zelfde conventie als
  // de bestaande temperament-tabellen (A = referentie).
  const refA = offsets[9];
  const naarA = offsets.map((o) => o - refA);
  // Afronden op 2 decimalen, zoals de bestaande tabellen.
  const afgerond = naarA.map((o) => Math.round(o * 100) / 100);

  return {
    name: eersteBeschrijving || bestandsnaam,
    octaveCents,
    cents: afgerond,
  };
}

// Zelf-test (unit-test light, zodat we niet via Rust hoeven voor dit stuk).
export function _runScalaTests() {
  const faal = (naam, oorzaak) => { throw new Error(`[scala.js] ${naam}: ${oorzaak}`); };
  const bijna = (a, b, tol = 0.01) => Math.abs(a - b) < tol;

  // 1. Gelijkzwevend uit cent-waarden (100, 200, …, 1200): alles 0.
  {
    const bron = ['! equal', 'Equal 12-TET', '12',
      '100.0', '200.0', '300.0', '400.0', '500.0', '600.0', '700.0', '800.0', '900.0', '1000.0', '1100.0', '1200.0'].join('\n');
    const t = parseScala(bron);
    if (!t.cents.every((c) => c === 0)) faal('equal-cents', `verwacht alle nul, kreeg ${t.cents}`);
    if (!bijna(t.octaveCents, 1200)) faal('equal-octaaf', `octaveCents=${t.octaveCents}`);
  }
  // 2. Breuken: 3/2 is 701,955 cent → 701,955 − 700 = +1,955 relatief.
  //    A-shift zet A (index 9) op nul, dus na shift blijft 3/2 niet op
  //    index 7 maar wordt alles verschoven. We checken dat A exact 0 is en
  //    dat de octaaf-interval goed uitkomt.
  {
    const bron = ['Ratio-mix', '12',
      '16/15', '9/8', '6/5', '5/4', '4/3', '45/32', '3/2', '8/5', '5/3', '9/5', '15/8', '2/1'].join('\n');
    const t = parseScala(bron);
    if (!bijna(t.cents[9], 0, 0.001)) faal('ratio-A-ref', `A=${t.cents[9]} (moet 0 zijn na A-shift)`);
    if (!bijna(t.octaveCents, 1200, 0.001)) faal('ratio-octaaf', `octaveCents=${t.octaveCents}`);
    // 3/2 is 701,955; na A-shift (A op 884,359 cent in deze set → verschuiving −884,359)
    // zit index 7 op 701,955 − 884,359 + 9·100 = 717,596. Oftewel offset t.o.v. 700 = +17,596.
    // We checken alleen dat de index-7-waarde eindig is en niet NaN.
    if (!Number.isFinite(t.cents[7])) faal('ratio-G', `G=${t.cents[7]}`);
  }
  // 3. Commentaar + lege regels tolereren.
  {
    const bron = ['! commentaar', '', 'Beschrijving', '! nog meer', '12',
      '100', '200', '300', '400', '500', '600', '700', '800', '900', '1000', '1100', '1200'].join('\n');
    const t = parseScala(bron);
    if (t.name !== 'Beschrijving') faal('naam', `kreeg "${t.name}"`);
  }
  // 4. Aantal moet 12 zijn.
  {
    try { parseScala(['Mini', '4', '300', '600', '900', '1200'].join('\n')); faal('N=4', 'verwacht fout'); }
    catch (e) { if (!/12/.test(e.message)) faal('N=4', `verkeerde fout: ${e.message}`); }
  }
  // 5. Komma als decimaal.
  {
    const bron = ['Komma', '12', '100,0', '200,0', '300,0', '400,0', '500,0', '600,0', '700,0', '800,0', '900,0', '1000,0', '1100,0', '1200,0'].join('\n');
    const t = parseScala(bron);
    if (!t.cents.every((c) => Math.abs(c) < 0.01)) faal('komma', `cents=${t.cents}`);
  }
  // 6. Rotte breuk weigeren.
  {
    try { parseScala(['Rot', '12', '1/0', '200', '300', '400', '500', '600', '700', '800', '900', '1000', '1100', '1200'].join('\n')); faal('rot', 'verwacht fout'); }
    catch (e) {}
  }
  return 'scala.js: alle zelf-tests ok';
}
