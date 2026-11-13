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

/** De genormaliseerde namen waaronder een set uit het manifest in de bibliotheek kan staan. */
function setNamen(s) {
  const namen = new Set([normNaam(s?.naam), normNaam(s?.map_naam || s?.naam), normNaam(s?.id)]);
  namen.delete('');
  return namen;
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
  const namen = setNamen(s);
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

/**
 * Waarom wordt deze set als "nog te downloaden" aangeboden? Eén regel voor het logboek, zodat een set die
 * onterecht blijft staan (andere schrijfwijze, andere map) te herleiden is: de namen waarmee vergeleken is,
 * de plek van een eerdere download, en de orgels in de bibliotheek die er het dichtst bij komen.
 */
export function waaromNiet(s, organen, gedownload) {
  const namen = [...setNamen(s)];
  const lijst = organen || [];
  const dicht = [];
  for (const o of lijst) {
    const kandidaten = [normNaam(o?.name), ...String(o?.source_path ?? '').split(/[\\/]/).map(normNaam)].filter((x) => x.length >= 4);
    const raak = kandidaten.some((k) => namen.some((n) => {
      if (k.includes(n) || n.includes(k)) return true;
      let i = 0;
      while (i < k.length && i < n.length && k[i] === n[i]) i++;
      return i >= 6;
    }));
    if (raak) dicht.push(`'${o?.name ?? ''}' in '${o?.source_path ?? ''}'`);
  }
  const plek = s?.id && gedownload ? gedownload[s.id] : '';
  return `set '${s?.naam ?? ''}' (id '${s?.id ?? ''}', map '${s?.map_naam ?? ''}') niet herkend als geïnstalleerd; ` +
    `vergeleken met de namen [${namen.join(', ')}]; ${lijst.length} orgels in de bibliotheek; ` +
    `plek van een eerdere download: ${plek ? `'${plek}'` : 'geen'}; ` +
    `dichtstbijzijnde orgels: ${dicht.length ? dicht.slice(0, 3).join('; ') : 'geen'}`;
}
