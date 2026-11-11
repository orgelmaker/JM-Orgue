// Welke sets uit het samplesets-manifest zijn al geïnstalleerd?
//
// Het blok "Beschikbaar om te downloaden" in de bibliotheek toont de sets uit het
// manifest die nog niet in de bibliotheek staan. De vergelijking moet niet struikelen
// over schrijfwijze: een Mac schrijft "ä" als twee tekens (a + trema) in een mapnaam,
// het manifest als één; hoofdletters, koppeltekens en spaties kunnen verschillen; en
// een orgel kan heten wat de maker wil terwijl de map de naam van het manifest draagt.

/** Naam om te vergelijken: zonder accenten, hoofdletters en leestekens. */
export function normNaam(x) {
  return String(x ?? '')
    .normalize('NFD')
    .replace(/[̀-ͯ]/g, '')
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '');
}

/** Pad om te vergelijken: vaste schrijfwijze van de letters, schuine strepen en hoofdletters. */
export function normPad(x) {
  return String(x ?? '')
    .normalize('NFC')
    .replace(/\\/g, '/')
    .replace(/\/+$/, '')
    .toLowerCase();
}

/**
 * Staat deze set uit het manifest al in de bibliotheek?
 *
 * Ja als een orgel in de bibliotheek:
 *  - de naam van de set heeft (naam, mapnaam of id in het manifest), of
 *  - ergens in zijn pad een map heeft met die naam (een Hauptwerk-set staat
 *    bijvoorbeeld in `<set>/OrganDefinitions/…`), of
 *  - staat op of onder de plek waar de app deze set zelf neerzette (`gedownload`).
 *
 * @param {{id?: string, naam?: string, map_naam?: string}} s  een set uit het manifest
 * @param {Array<{name?: string, source_path?: string}>} organen  de bibliotheek
 * @param {Record<string,string>} gedownload  manifest-id → pad waar de app de set neerzette
 */
export function isGeinstalleerd(s, organen, gedownload) {
  const namen = new Set([normNaam(s?.naam), normNaam(s?.map_naam || s?.naam), normNaam(s?.id)]);
  namen.delete('');
  const plek = s?.id && gedownload ? normPad(gedownload[s.id]) : '';
  for (const o of organen || []) {
    if (namen.has(normNaam(o?.name))) return true;
    const pad = normPad(o?.source_path);
    if (pad.split('/').filter(Boolean).some((deel) => namen.has(normNaam(deel)))) return true;
    if (plek && (pad === plek || pad.startsWith(plek + '/'))) return true;
  }
  return false;
}

/** De sets die nog te downloaden zijn. */
export function nogTeDownloaden(manifestSets, organen, gedownload) {
  return (manifestSets || []).filter((s) => s && s.naam && s.zip_url && !isGeinstalleerd(s, organen, gedownload));
}
