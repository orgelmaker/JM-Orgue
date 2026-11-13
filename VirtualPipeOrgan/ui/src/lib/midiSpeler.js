// De afspeelknop van de werkbalk (naast de MIDI-knop): wat hij toont en wat een klik doet.
//
// De MIDI-speler is één speler voor het hele programma; de speler onder Routing gebruikt hem ook. De knop onthoudt daarom
// welk bestand hij zelf gestart heeft (`gestartPad`) en behandelt een ander bestand dat in de speler speelt als "niet
// van mij": een klik start dan zijn eigen bestand, eerst na het stoppen van het andere.

/** Bestandsnaam uit een pad, met schuine strepen van beide soorten. */
export function bestandsNaam(pad) {
  return pad ? String(pad).split(/[\\/]/).pop() : '';
}

/** Speelt de speler nu het bestand van de knop? Dan toont de knop Stop. */
export function speeltNu(status, gestartPad, pad) {
  return !!pad && status?.state === 'playing' && gestartPad === pad;
}

/**
 * De stappen bij een klik op de knop, in volgorde:
 *  - 'stop'   de speler stoppen (en alle noten stil);
 *  - 'hervat' verder na een pauze (alleen als het bestand van de knop gepauzeerd is);
 *  - 'speel'  het bestand van de knop laden en afspelen.
 */
export function afspeelStappen(status, gestartPad, pad) {
  const eigen = !!pad && gestartPad === pad;
  const stand = status?.state || 'stopped';
  if (stand === 'playing' && eigen) return ['stop'];
  if (stand === 'paused' && eigen) return ['hervat'];
  return stand === 'stopped' ? ['speel'] : ['stop', 'speel'];
}
