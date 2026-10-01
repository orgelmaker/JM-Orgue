// Vensterstand en volledig scherm van de extra registerschermen (0.7.64).
//
// Werkt met elke Tauri-vensterhandle: getCurrentWindow() in het paneel zelf,
// of de WebviewWindow die het hoofdvenster bij het openen in handen heeft.
// Alles in FYSIEKE pixels (0.7.21): eenduidig over meerdere schermen met een
// verschillende schaal.
//
// Wat volledig scherm op Windows doet (tao): setFullscreen(true) kiest het
// scherm waar het venster op dat moment het meest op staat, bewaart de
// vensterplaatsing en zet die bij uitzetten terug. Daaruit volgen de regels
// hieronder: eerst positioneren, dán volledig scherm; en een venster op
// volledig scherm nooit als "vensterstand" bewaren of terugzetten (de valkuil
// van 0.7.61 bij het hoofdvenster).
//
// Het hoofdvenster gebruikt deze module (nog) niet; App.svelte heeft zijn
// eigen, uitgebreid geteste variant. Overzetten is een aparte stap.

const BUITEN_BEELD = -30000; // Windows meldt een geminimaliseerd venster op -32000

// De huidige stand als patch voor de panel-state, of null als er niets te
// bewaren valt: geminimaliseerd, volledig scherm of buiten beeld.
export async function leesVensterstand(win) {
  if (await win.isMinimized()) return null;
  if (await win.isFullscreen()) return null;
  const pos = await win.outerPosition();
  if (pos.x < BUITEN_BEELD || pos.y < BUITEN_BEELD) return null;
  if (await win.isMaximized()) {
    // Alleen de vlag plus een anker op het scherm; px/py/pw/ph blijven de
    // gewone stand waar "herstellen" op terugvalt.
    return { maximized: true, mpx: pos.x, mpy: pos.y };
  }
  const size = await win.innerSize();
  return { px: pos.x, py: pos.y, pw: size.width, ph: size.height, maximized: false };
}

// Volledig scherm aan of uit. Geeft true als het lukte.
export async function zetVolledigScherm(win, aan) {
  try {
    await win.setFullscreen(aan);
    return true;
  } catch (e) {
    console.warn('Volledig scherm wisselen mislukte:', e);
    return false;
  }
}

// Een bewaarde stand toepassen, en daarna desgewenst volledig scherm.
// Geeft terug of het venster nu volledig scherm is.
//
// Volledig scherm alleen als het scherm waar het venster hoort er ook is.
// Ontbreekt dat scherm (uitgezet, of bij het opstarten van de speeltafel nog
// niet actief), dan kiest Windows het dichtstbijzijnde, en dan zou het paneel
// schermvullend over het hoofdvenster komen. Dan liever als gewoon venster
// openen; de wens blijft bewaard voor de volgende keer.
export async function pasVensterstandToe(win, st, { minW = 200, minH = 150, volledigScherm = false } = {}) {
  if (!st) return false;
  const { PhysicalPosition, PhysicalSize } = await import('@tauri-apps/api/dpi');
  const getal = (v) => (typeof v === 'number' && v > BUITEN_BEELD) ? v : null;
  let anker = null;

  if (st.maximized) {
    // Eerst op het scherm van de maximalisatie zetten, dán maximaliseren:
    // maximize() pakt het scherm waar het venster op dat moment staat. +64
    // zodat het punt ruim binnen de monitor valt.
    const ax = getal(st.mpx) ?? getal(st.px);
    const ay = getal(st.mpy) ?? getal(st.py);
    if (ax !== null && ay !== null) {
      anker = { x: ax + 64, y: ay + 64 };
      await win.setPosition(new PhysicalPosition(anker.x, anker.y));
    }
    await win.maximize();
  } else if (getal(st.px) !== null && getal(st.py) !== null) {
    await win.setPosition(new PhysicalPosition(st.px, st.py));
    anker = { x: st.px + 64, y: st.py + 64 };
    if (st.pw >= minW && st.ph >= minH) {
      anker = { x: st.px + Math.round(st.pw / 2), y: st.py + Math.round(st.ph / 2) };
      const apply = () => win.setSize(new PhysicalSize(Math.min(st.pw, 16000), Math.min(st.ph, 16000)));
      await apply();
      // Cross-DPI-controle: verhuist het venster naar een scherm met een
      // andere schaal, dan kan de asynchrone herschaling de maat overschrijven.
      // Eén keer controleren — maar NIET als het venster inmiddels volledig
      // scherm staat: dan trok dit het terug naar de oude maat (0.7.61).
      setTimeout(async () => {
        try {
          if (await win.isFullscreen()) return;
          const cur = await win.innerSize();
          if (Math.abs(cur.width - st.pw) > 4 || Math.abs(cur.height - st.ph) > 4) await apply();
        } catch (e) {}
      }, 250);
    }
  }

  if (!volledigScherm) return false;
  if (anker) {
    try {
      const { monitorFromPoint } = await import('@tauri-apps/api/window');
      if (!(await monitorFromPoint(anker.x, anker.y))) return false;
    } catch (e) { /* controle niet mogelijk: gewoon doorgaan */ }
  }
  return zetVolledigScherm(win, true);
}
