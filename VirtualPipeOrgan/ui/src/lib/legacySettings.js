// Beslis alleen na bevestiging over een anoniem oud instellingenbestand.
// I/O en dialoogfuncties worden meegegeven zodat de keuzes getest kunnen worden.
export async function kiesOudeInstellingen({ inspecteer, bevestig, meldOntbrekend, waarschuw, stelUit, importeer, handmatig }) {
  let kandidaat;
  try {
    kandidaat = await inspecteer();
  } catch (e) {
    if (handmatig) throw e;
    waarschuw(e);
    return true;
  }
  if (!kandidaat) {
    if (handmatig) await meldOntbrekend();
    return !handmatig;
  }
  if ((kandidaat.has_settings || kandidaat.deferred) && !handmatig) return true;
  if (!(await bevestig(kandidaat, false))) {
    if (handmatig) return false;
    // Pas verdergaan zodra de keuze duurzaam is opgeslagen. Een fout gaat naar
    // de normale laadfoutmelding, zodat de gebruiker niet denkt dat dit onthouden is.
    await stelUit(kandidaat);
    return true;
  }
  if (kandidaat.has_settings && !(await bevestig(kandidaat, true))) return false;
  await importeer(kandidaat);
  return true;
}

// Een mislukte lading laat mogelijk de oude runtime actief. Blokkeer opslag
// daarvan totdat een nieuwe lading is geslaagd, óók bij opnieuw proberen.
export function maakMigratieBewaking() {
  let openstaand = false;
  return {
    importGeslaagd() { openstaand = true; },
    ladenGeslaagd() { openstaand = false; },
    get openstaand() { return openstaand; },
    kanOpslaan(orgelwissel, force = false) { return !openstaand && (!orgelwissel || force); },
  };
}

// Een ouder antwoord mag een nieuwe lading van hetzelfde orgel niet terugdraaien.
// Controleer zowel vóór een actie als na elk asynchroon antwoord de generatie.
export function bewaakOrgelHerstel(isHuidig) {
  return async (actie) => {
    const controleer = () => {
      if (!isHuidig()) throw new Error('Orgelinstellingen zijn tijdens het herstellen gewijzigd');
    };
    controleer();
    const uitkomst = await actie();
    controleer();
    return uitkomst;
  };
}

export async function voltooiOrgelHerstel(herstel, bewaking) {
  await herstel();
  bewaking.ladenGeslaagd();
}
