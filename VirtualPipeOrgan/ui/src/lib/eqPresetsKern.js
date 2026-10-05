// Hoofdtelefoonpresets (0.7.77): pure functies over de gebundelde database
// (ui/src/assets/eq-presets.json, gebouwd door scripts/bouw_eq_presets.py uit
// AutoEq). Geen JSON-import hier, zodat node-tests de data zelf kunnen
// inlezen; eqPresets.js bindt deze functies aan de bundel.
import { qNaarBw } from './eqImport.js';

const norm = (s) => String(s || '').toLowerCase().replace(/[\s\-_.()]/g, '');

export function maakPresetZoeker(data) {
  const presets = Array.isArray(data?.presets) ? data.presets : [];
  const index = presets.map((p) => ({ p, n: norm(p.merk + ' ' + p.model) }));

  // Zoeken vanaf 2 tekens; elk woord moet voorkomen (spaties, koppeltekens
  // en punten tellen niet: "k240" vindt "AKG K240 Studio", "dt770" de DT 770).
  function zoek(tekst, max = 40) {
    const woorden = String(tekst || '').toLowerCase().split(/\s+/).map(norm).filter(Boolean);
    if (!woorden.length || woorden.join('').length < 2) return [];
    return index.filter((x) => woorden.every((w) => x.n.includes(w))).slice(0, max).map((x) => x.p);
  }

  function perMerk() {
    const m = new Map();
    for (const p of presets) {
      if (!m.has(p.merk)) m.set(p.merk, []);
      m.get(p.merk).push(p);
    }
    return [...m.entries()].map(([merk, lijst]) => ({ merk, presets: lijst }));
  }

  const bijId = (id) => presets.find((p) => p.id === id) || null;

  return {
    presets, zoek, perMerk, bijId,
    bron: {
      bron: data?.bron, url: data?.url, meter: data?.meter, vormfactor: data?.vormfactor,
      commit: data?.commit, gegenereerd: data?.gegenereerd, licentie: data?.licentie,
    },
  };
}

// Preset → bandobjecten van de equalizer (Q native; bandbreedte analoog afgeleid).
export function presetNaarBanden(preset) {
  return (preset?.banden || []).map(([type, fc, gain, q]) => ({
    enabled: true, band_type: type, freq: fc, gain_db: gain, bandwidth: qNaarBw(q), q, channel: null,
  }));
}

// Korte samenvatting voor in de kiezer: "10 banden · −6,2 dB".
export function presetSamenvatting(preset, dbLabel = 'dB') {
  const n = preset?.banden?.length || 0;
  const pre = Number(preset?.preamp) || 0;
  return `${n} · ${pre > 0 ? '+' : ''}${pre.toFixed(1)} ${dbLabel}`;
}
