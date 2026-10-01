// Toetsenbordhulp voor de vensters (App, PanelApp).
//
// Moeten globale toetsen (F1-F3, F11, Escape, toetsenbordnoten) wijken voor
// het element met de focus? Alleen bij echte tekstinvoer. Een schuif of
// vinkje met focus (bijvoorbeeld de volumeschuif in de setzerbalk na één tik)
// schakelde anders F11 en Escape uit tot je ergens anders klikte (0.7.65).
const GEEN_TEKST = new Set(['range', 'checkbox', 'radio', 'button', 'submit', 'reset', 'color', 'file']);

export function isTekstveld(el) {
  if (!el) return false;
  if (el.isContentEditable) return true;
  if (el.tagName === 'TEXTAREA') return true;
  if (el.tagName === 'INPUT') return !GEEN_TEKST.has((el.type || 'text').toLowerCase());
  return false;
}
