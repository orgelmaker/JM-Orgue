// Apparaatkeuze-helper voor de audio-instellingen (gedeeld door App en PanelApp):
// kies het bewaarde apparaat als het nog bestaat, anders het default-apparaat
// van de host, anders het eerste uit de lijst.

// De opgeslagen uitgang (host, apparaat, buffer, samplerate) voor het
// instellingenscherm. De bron is de backend (`audio_config.json`, opdracht
// get_audio_output_prefs): dat is wat de app bij de start werkelijk gebruikt.
//
// Tot en met 0.9.54 hield de interface een eigen kopie bij in localStorage
// (jm-orgue-audio-host/-device/-buffer/-rate). Die wordt niet meer geschreven.
// Een lege host in het antwoord betekent de standaardhost. De oude sleutels
// blijven staan voor oudere versies, maar zeggen niets over de huidige keuze.
//
// `prefs` is het antwoord van de backend. Geeft altijd alle vier de velden;
// null = geen keuze (de aanroeper kiest dan zijn standaard).
export function opgeslagenUitgang(prefs) {
  const getal = (v) => { const n = Number(v); return Number.isFinite(n) && n > 0 ? Math.round(n) : null; };
  if (prefs) {
    return {
      host: prefs.host || prefs.default_host || null,
      device: prefs.device || null,
      buffer: getal(prefs.buffer_frames),
      rate: getal(prefs.sample_rate),
      bron: 'backend',
    };
  }
  return {
    host: null, device: null, buffer: null, rate: null, bron: 'onbekend',
  };
}

export function pickDevice(devices, savedName) {
  if (savedName && devices.some(d => d.name === savedName)) return savedName;
  const def = devices.find(d => d.is_default);
  return def ? def.name : (devices[0] ? devices[0].name : null);
}
