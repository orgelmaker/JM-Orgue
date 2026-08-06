// Per-scherm-toestand van extra vensters (divisiekeuze, vensterpositie/-grootte,
// layout, hoofdbalk aan/uit), bewaard per scherm × orgel in localStorage.
//
// Sleutel: `jm-orgue-panel-state-<nr>-<orgelhash>` — zelfde hash-schema als de
// organUiKey's in App/Console. Single-writer-afspraak: alléén het venster met
// dat nummer schrijft zijn eigen sleutel; het hoofdvenster leest hem uitsluitend
// bij het (her)openen (positie/grootte). savePanelState merge-patcht zodat
// oudere velden ({divisions,x,y,width,height}) intact blijven wanneer nieuwe
// velden (layout, showHeader) erbij komen.

export function hashCode(str) {
  let h = 0;
  for (let i = 0; i < (str || '').length; i++) {
    h = ((h << 5) - h) + str.charCodeAt(i);
    h |= 0;
  }
  return Math.abs(h).toString(36);
}

// Per-orgel localStorage-sleutel (zelfde vorm als organUiKey in App/Console).
export function organUiKey(base, organId) {
  return organId ? base + '-' + hashCode(organId) : base;
}

export function panelStateKey(organId, panelNumber) {
  return organUiKey(`jm-orgue-panel-state-${panelNumber}`, organId);
}

// Lees de toestand van scherm <panelNumber> bij orgel <organId>; null als er
// niets (geldigs) bewaard is.
export function loadPanelState(organId, panelNumber) {
  if (panelNumber == null || !organId) return null;
  try {
    const st = JSON.parse(localStorage.getItem(panelStateKey(organId, panelNumber)) || 'null');
    return (st && typeof st === 'object') ? st : null;
  } catch (e) {
    return null;
  }
}

// Merge-patch: alleen de meegegeven velden wijzigen, de rest blijft staan.
export function savePanelState(organId, panelNumber, patch) {
  if (panelNumber == null || !organId) return;
  try {
    const cur = loadPanelState(organId, panelNumber) || {};
    localStorage.setItem(panelStateKey(organId, panelNumber), JSON.stringify({ ...cur, ...patch }));
  } catch (e) { /* opslag vol/geblokkeerd → stil overslaan */ }
}
