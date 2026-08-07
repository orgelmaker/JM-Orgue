// Audio-uitvoerprofielen (speakers/hoofdtelefoon): pure helpers, gedeeld door
// het hoofdvenster (App.svelte, dat de profielen beheert en wisselt) en de
// extra schermen (PanelApp, die alleen tonen welk profiel actief is).

export const AUDIO_PROFILES_KEY = 'jm-orgue-audio-profiles';

// Profielen uit localStorage; ontbrekend/corrupt → lege defaults.
export function loadAudioProfiles() {
  try {
    return {
      speakers: null, headphones: null, active: null,
      ...JSON.parse(localStorage.getItem(AUDIO_PROFILES_KEY) || '{}'),
    };
  } catch (e) {
    return { speakers: null, headphones: null, active: null };
  }
}

// Hoort deze (echt spelende) uitgang bij profiel p?
export function profileMatchesOutput(p, host, device) {
  if (!p || !p.host) return false;
  if (p.host !== host) return false;
  // Profiel zonder expliciet apparaat = default van die host → host-match volstaat.
  return !p.device || p.device === device;
}

// Bepaal welk profiel bij de actuele uitgang hoort. Er is bewust géén derde
// "profielloos"-toestand meer (0.7.3): kan de uitgang niet eenduidig aan één
// profiel worden gekoppeld — onbekend apparaat, of beide profielen identiek —
// dan blijft de huidige `active`-stand gewoon staan.
export function deriveProfileFromOutput(profiles, host, device) {
  if (!host) return profiles.active;
  const hp = profileMatchesOutput(profiles.headphones, host, device);
  const sp = profileMatchesOutput(profiles.speakers, host, device);
  if (hp && !sp) return 'headphones';
  if (sp && !hp) return 'speakers';
  return profiles.active;   // geen eenduidige match → laat staan
}
