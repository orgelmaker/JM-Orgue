// Luidsprekercatalogus (0.7.81): alleen de namen van de ~1100 luidsprekers
// waarvoor spinorama.org een parametrische correctie heeft. De correctie zelf
// wordt pas bij het kiezen opgehaald (Rust: fetch_speaker_eq, vastgepind op
// de commit in de catalogus) en hier geparseerd met eqImport.js. Zie
// scripts/bouw_luidspreker_catalogus.py en docs/LICENTIES_DERDEN.md.
import data from '../assets/luidsprekers-catalogus.json';
import { maakPresetZoeker } from './eqPresetsKern.js';

// De kern van de zoeker werkt op { id, merk, model }; de luidsprekeritems
// hebben daarnaast `pad` (mapnaam in datas/eq) en geen banden.
const zoeker = maakPresetZoeker({ ...data, presets: data.items });

export const luidsprekers = zoeker.presets;
export const luidsprekerBron = { bron: data.bron, url: data.url, site: data.site, commit: data.commit, gegenereerd: data.gegenereerd };
export const zoekLuidsprekers = zoeker.zoek;
export const luidsprekersPerMerk = zoeker.perMerk;
export const luidsprekerBijId = zoeker.bijId;
