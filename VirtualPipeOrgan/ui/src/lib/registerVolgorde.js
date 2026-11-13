// Registervolgorde uit het orgelbestand overnemen.
//
// De eigen registervolgorde (slepen in Instellingen → Registersortering) staat in
// localStorage van de webview. Het hoofdvenster publiceert de getoonde volgorde
// ook als `remote_layout.stop_order`, en die komt zo in de instellingen van het
// orgel (bibliotheek en `.jm-settings.json` naast de set). Tot en met 0.9.54
// werd die alleen door de afstandsbediening gelezen: wie een set met zijn
// `.jm-settings.json` op een andere pc zette, of de webview-gegevens kwijt was,
// begon weer met de volgorde van de set.
//
// Deze functie bepaalt wat er uit het orgelbestand over te nemen is voor een
// orgel dat op deze pc nog GEEN eigen volgorde heeft. Een lokale volgorde wint
// altijd; daar beslist de aanroeper over (sleutel aanwezig = niet overnemen).
//
// `opgeslagen`: remote_layout.stop_order, { divisienaam: [registerId, ...] }.
// `expliciet`: alleen de door de gebruiker gekozen volgordes, indien beschikbaar.
// Oudere bestanden hebben alleen de volledige schermvolgorde; daaruit kan een
// expliciete keuze gelijk aan de setvolgorde niet meer worden teruggevonden.
export function volgordeUitOrgelbestand(opgeslagen, divisies, expliciet = null) {
  const uit = {};
  const heeftExpliciet = expliciet !== null && typeof expliciet === 'object' && !Array.isArray(expliciet);
  if (heeftExpliciet) opgeslagen = expliciet;
  if (!opgeslagen || typeof opgeslagen !== 'object' || !Array.isArray(divisies)) return uit;
  for (const d of divisies) {
    const lijst = opgeslagen[d && d.name];
    if (!Array.isArray(lijst) || lijst.length === 0) continue;
    const vanSet = ((d && d.stops) || []).map((s) => s.id);
    const bestaat = new Set(vanSet);
    const gezien = new Set();
    const bewaard = [];
    for (const id of lijst) {
      if (typeof id === 'string' && bestaat.has(id) && !gezien.has(id)) { gezien.add(id); bewaard.push(id); }
    }
    if (bewaard.length < (heeftExpliciet ? 1 : 2)) continue;
    // Dezelfde registers in de volgorde van de set: is dat gelijk, dan is er niet gesorteerd.
    const natuurlijk = vanSet.filter((id) => gezien.has(id));
    if (!heeftExpliciet && bewaard.every((id, i) => id === natuurlijk[i])) continue;
    uit[d.name] = bewaard;
  }
  return uit;
}
