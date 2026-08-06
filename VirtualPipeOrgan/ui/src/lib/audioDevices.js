// Apparaatkeuze-helper voor de audio-instellingen (gedeeld door App en PanelApp):
// kies het bewaarde apparaat als het nog bestaat, anders het default-apparaat
// van de host, anders het eerste uit de lijst.

export function pickDevice(devices, savedName) {
  if (savedName && devices.some(d => d.name === savedName)) return savedName;
  const def = devices.find(d => d.is_default);
  return def ? def.name : (devices[0] ? devices[0].name : null);
}
