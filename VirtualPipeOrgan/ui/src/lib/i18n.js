// Simpele i18n implementatie voor JM-Orgue.
// 4 locales: nl (default), en, fr, de.
//
// Gebruik:
//   import { t, locale, setLocale } from '../lib/i18n.js';
//   <h1>{$t('app.title')}</h1>
//   $: setLocale('en');
//
// Locale files zitten in src/lib/locales/{nl,en,fr,de}.json
// Onbekende key → toont de key zelf (zichtbaar in UI, makkelijk te debuggen).
// Onbekende locale → fallback naar nl.

import { writable, derived, get } from 'svelte/store';
import nl from './locales/nl.json';
import en from './locales/en.json';
import fr from './locales/fr.json';
import de from './locales/de.json';

const dictionaries = { nl, en, fr, de };
export const AVAILABLE_LOCALES = ['nl', 'en', 'fr', 'de'];
export const LOCALE_LABELS = {
  nl: 'Nederlands',
  en: 'English',
  fr: 'Français',
  de: 'Deutsch',
};
const DEFAULT_LOCALE = 'nl';
const STORAGE_KEY = 'jm-orgue-locale';

function getInitialLocale() {
  try {
    const saved = localStorage.getItem(STORAGE_KEY);
    if (saved && AVAILABLE_LOCALES.includes(saved)) return saved;
  } catch (e) {}
  // Fallback: bekijk browser-taal
  try {
    const browserLang = (navigator.language || 'nl').slice(0, 2);
    if (AVAILABLE_LOCALES.includes(browserLang)) return browserLang;
  } catch (e) {}
  return DEFAULT_LOCALE;
}

export const locale = writable(getInitialLocale());

export function setLocale(newLocale) {
  if (!AVAILABLE_LOCALES.includes(newLocale)) return;
  locale.set(newLocale);
  try { localStorage.setItem(STORAGE_KEY, newLocale); } catch (e) {}
  // Update <html lang="..."> attribute voor screen readers + spell check
  try { document.documentElement.setAttribute('lang', newLocale); } catch (e) {}
}

/** Vertaal een dotted key naar string voor huidige locale. */
function translate(key, currentLocale) {
  const dict = dictionaries[currentLocale] || dictionaries[DEFAULT_LOCALE];
  const parts = key.split('.');
  let cur = dict;
  for (const p of parts) {
    if (cur && typeof cur === 'object' && p in cur) {
      cur = cur[p];
    } else {
      // Fallback naar default
      if (currentLocale !== DEFAULT_LOCALE) {
        return translate(key, DEFAULT_LOCALE);
      }
      return key; // toont de key zelf als zichtbare placeholder
    }
  }
  return typeof cur === 'string' ? cur : key;
}

/** Reactive store: in template als `$t('key.path')`. */
export const t = derived(locale, ($locale) => (key) => translate(key, $locale));

/** Imperatieve versie — voor non-Svelte contexts (logs, alert messages). */
export function tx(key) {
  return translate(key, get(locale));
}

// Init <html lang>
try { document.documentElement.setAttribute('lang', getInitialLocale()); } catch (e) {}
