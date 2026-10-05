// Keuzelijsten van het notatievenster (0.7.83), gedeeld door de werkbalk en
// de wizard "Nieuw stuk".

/** Maatsoorten die de werkbalk en de wizard aanbieden (teller/noemer). */
export const MAATSOORTEN = ['2/4', '3/4', '4/4', '6/4', '2/2', '3/2', '6/8'];

/** "6/8" → [6, 8]; onbruikbare invoer → [4, 4]. De noemer is 2, 4 of 8. */
export function splitsMaatsoort(s) {
  const m = /^\s*(\d{1,2})\s*\/\s*(\d)\s*$/.exec(String(s ?? ''));
  if (!m) return [4, 4];
  const beats = Math.min(12, Math.max(1, Number(m[1])));
  const unit = [2, 4, 8].includes(Number(m[2])) ? Number(m[2]) : 4;
  return [beats, unit];
}

/** Lengte van één maat in microseconden bij dit tempo (kwarten per minuut). */
export function maatDuurUs(bpm, beats, unit) {
  const kwart = 60e6 / (Number(bpm) || 90);
  return kwart * (Number(beats) || 4) * (4 / (Number(unit) || 4));
}

/**
 * Sleutel van de notemap (0.7.86): part, maat, stem, positie in rastereenheden
 * binnen de maat en toon — dezelfde velden als de Rust-NoteRef.
 */
export function notemapSleutel(part, measure, voice, pos, midi) {
  return `${part}|${measure}|${voice}|${pos}|${midi}`;
}

/** OSMD-tijdstempel binnen de maat (in hele noten) → rastereenheden bij raster q. */
export function rasterPositie(tsWhole, q) {
  return Math.round((Number(tsWhole) || 0) * 4 * (Number(q) || 4));
}
