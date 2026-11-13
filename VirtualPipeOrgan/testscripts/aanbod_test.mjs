// Test van de vergelijking "al geïnstalleerd?" voor het blok "Beschikbaar om te
// downloaden" (ui/src/lib/aanbod.js), zonder DOM. Draait met `node` en in CI.
import { normNaam, normPad, isGeinstalleerd, nogTeDownloaden, waaromNiet } from '../ui/src/lib/aanbod.js';

let fouten = 0;
function check(cond, naam) {
  if (cond) { console.log('ok   ' + naam); }
  else { console.log('FOUT ' + naam); fouten++; }
}

const NFC = 'B\u00e4tz-Witte Puttershoek';          // één teken voor ä
const NFD = 'Ba\u0308tz-Witte Puttershoek';         // a + trema, zoals een Mac het schrijft
const set = { id: 'batz-witte-puttershoek', naam: NFC, map_naam: NFC, zip_url: 'https://github.com/x/y.zip' };
const orgel = (name, source_path, source_type = 'sample_directory') => ({ name, source_path, source_type });

// ---- schrijfwijze ----
check(normNaam(NFC) === normNaam(NFD), 'een ä als één of als twee tekens is dezelfde naam');
check(normNaam('Bätz-Witte Puttershoek') === normNaam('batz witte  PUTTERSHOEK'), 'hoofdletters, koppelteken en spaties tellen niet');
check(normNaam('batz-witte-puttershoek') === normNaam(NFC), 'het id van de set komt overeen met de naam');
check(normPad('C:\\Orgels\\' + NFD + '\\') === normPad('c:/orgels/' + NFC), 'pad: schuine strepen, hoofdletters, slotstreep en tekenvorm');

// ---- wel geïnstalleerd ----
check(isGeinstalleerd(set, [orgel(NFC, 'D:\\Sets\\' + NFC)], {}), 'zelfde naam en map');
check(isGeinstalleerd(set, [orgel(NFD, 'D:/Sets/' + NFD)], {}), 'Mac-schrijfwijze (a + trema) tegenover het manifest');
check(isGeinstalleerd(set, [orgel('Kerk - Bouwer - Plaats', 'D:\\Sets\\' + NFC)], {}), 'het orgel heet anders, de map draagt de naam van de set');
check(isGeinstalleerd(set, [orgel('Iets anders', 'D:\\Sets\\' + NFC + '\\OrganDefinitions\\a.Organ_Hauptwerk_xml', 'organ_file')], {}),
  'een definitie diep in de map van de set (niet alleen de laatste map telt)');
check(isGeinstalleerd(set, [orgel('Heel andere naam', 'E:\\Muziek\\Eigen map')], { [set.id]: 'E:\\Muziek\\Eigen map' }), 'precies op de plek waar de app de set neerzette');
check(isGeinstalleerd(set, [orgel('Heel andere naam', 'e:/muziek/eigen map/OrganDefinitions/a.xml', 'organ_file')], { [set.id]: 'E:\\Muziek\\Eigen map\\' }),
  'onder de plek waar de app de set neerzette, ongeacht schuine strepen en hoofdletters');

// ---- niet geïnstalleerd ----
check(!isGeinstalleerd(set, [], {}), 'lege bibliotheek');
check(!isGeinstalleerd(set, [orgel('Rotterdam', 'D:\\Sets\\Rotterdam_Hoofdorgel')], {}), 'andere orgels');
check(!isGeinstalleerd(set, [orgel('Heel andere naam', 'E:\\Muziek\\Andere map')], { [set.id]: 'E:\\Muziek\\Eigen map' }),
  'de plek van de download hoort bij een andere map');
check(!isGeinstalleerd(set, [orgel('Heel andere naam', 'E:\\Muziek\\Eigen map extra')], { [set.id]: 'E:\\Muziek\\Eigen map' }),
  'een map die met dezelfde letters begint is niet "onder" die plek');
check(!isGeinstalleerd(set, [orgel(NFC + ' (kopie)', 'D:\\Sets\\' + NFC + ' (kopie)')], {}), 'een kopie met een andere mapnaam is een ander orgel');

// ---- lege namen geven geen vals positief ----
{
  const zonderNaam = { id: '', naam: '', zip_url: 'https://github.com/x/y.zip' };
  check(!isGeinstalleerd(zonderNaam, [orgel('', ''), orgel(undefined, undefined)], {}), 'een set zonder naam telt nooit als geïnstalleerd');
  check(!isGeinstalleerd(set, [orgel('', ''), { }], {}), 'een orgel zonder naam of pad is niet de set');
  check(!isGeinstalleerd(set, null, null) && !isGeinstalleerd(set, undefined, undefined), 'geen bibliotheek');
}

// ---- de lijst ----
{
  const ander = { id: 'iets', naam: 'Iets Anders', map_naam: 'Iets Anders', zip_url: 'https://github.com/x/z.zip' };
  const zonderZip = { id: 'kapot', naam: 'Kapot', map_naam: 'Kapot' };
  const lijst = nogTeDownloaden([set, ander, zonderZip, null], [orgel(NFD, 'D:/Sets/' + NFD)], {});
  check(lijst.length === 1 && lijst[0].id === 'iets', 'alleen wat nog te downloaden is, met een zip-adres');
  check(nogTeDownloaden(undefined, [], {}).length === 0, 'geen manifest');
}

// ---- de uitleg voor het logboek ----
{
  const bijna = orgel('Batz Witte Puttershoek (kopie)', 'D:\\Sets\\Andere map');
  const uitleg = waaromNiet(set, [bijna, orgel('Rotterdam', 'D:\\Sets\\Rotterdam')], { [set.id]: 'E:\\Download\\Batz' });
  check(uitleg.includes("'batz-witte-puttershoek'") && uitleg.includes('batzwitteputtershoek'), 'de uitleg noemt de set en de namen waarmee vergeleken is');
  check(uitleg.includes('2 orgels in de bibliotheek'), 'de uitleg telt de orgels');
  check(uitleg.includes('E:\\Download\\Batz'), 'de uitleg noemt de plek van een eerdere download');
  check(uitleg.includes("'Batz Witte Puttershoek (kopie)'") && !uitleg.includes("'Rotterdam' in"), 'de uitleg noemt alleen orgels die erop lijken');
  // Een map in een Windows-pad telt ook als de naam van het orgel zelf anders is (beide soorten streep).
  const viaMap = waaromNiet(set, [orgel('Iets anders', 'D:\\Sets\\Batz-Witte Puttershoek 2\\OrganDefinitions\\a.xml')], {});
  check(viaMap.includes("'Iets anders'"), 'de uitleg vindt een orgel via een map in het pad');
  check(waaromNiet(set, [], {}).includes('dichtstbijzijnde orgels: geen') && waaromNiet(set, null, null).includes('0 orgels'), 'lege of ontbrekende bibliotheek geeft geen fout');
}

if (fouten) { console.log('\n' + fouten + ' fout(en)'); process.exit(1); }
console.log('\nalle controles goed');
