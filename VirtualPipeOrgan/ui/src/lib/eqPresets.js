// Gebundelde hoofdtelefoonpresets (AutoEq, oratory1990, over-ear) gebonden
// aan de kernfuncties. Zie eqPresetsKern.js en scripts/bouw_eq_presets.py.
import data from '../assets/eq-presets.json';
import { maakPresetZoeker, presetNaarBanden, presetSamenvatting } from './eqPresetsKern.js';

const zoeker = maakPresetZoeker(data);

export const presets = zoeker.presets;
export const presetBron = zoeker.bron;
export const zoekPresets = zoeker.zoek;
export const presetsPerMerk = zoeker.perMerk;
export const presetBijId = zoeker.bijId;
export { presetNaarBanden, presetSamenvatting };
