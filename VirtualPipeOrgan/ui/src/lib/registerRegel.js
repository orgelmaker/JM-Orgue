// De regel onder een registernaam (0.7.66): de voetmaat, en bij een mengwerk
// het aantal koren, opgemaakt zoals organisten het in hun taal schrijven.
//
// Het aantal komt als { min, max } uit de backend (StopDto.koren); `pitch`
// blijft alleen de voetmaat, want die wordt elders als getal gelezen
// (sorteren, crescendo). De opmaak komt uit de vertaalsleutel
// stops.ranks_format: {n} = Arabisch ("4", "4-6"), {r} = Romeins ("IV", "IV-VI").
//   nl "{n} st."   → "4 st." / "4-6 st."
//   de "{n}fach"   → "4fach"
//   en/fr/… "{r}"  → "IV"

const ROMEINS = ['', 'I', 'II', 'III', 'IV', 'V', 'VI', 'VII', 'VIII', 'IX', 'X', 'XI', 'XII'];

export function romeins(n) {
  return ROMEINS[n] || String(n);
}

export function korenTekst(koren, fmt) {
  if (!koren || !koren.min) return '';
  const een = koren.min === koren.max;
  const n = een ? `${koren.min}` : `${koren.min}-${koren.max}`;
  const r = een ? romeins(koren.min) : `${romeins(koren.min)}-${romeins(koren.max)}`;
  return (fmt || '{r}').replace('{n}', n).replace('{r}', r);
}

// "4 st.", "4-5 st. 2 2/3'", "8'", of "" als er niets bekend is.
export function registerRegel(stop, fmt) {
  const k = korenTekst(stop?.koren, fmt);
  const p = (stop?.pitch || '').toString().trim();
  return [k, p].filter(Boolean).join(' ');
}
