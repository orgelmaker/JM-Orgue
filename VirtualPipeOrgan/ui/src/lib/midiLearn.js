// Svelte action voor uniforme MIDI-learn UX.
//
// Gebruik:
//   <button use:midiLearn={{ actionCode: 16, onTrigger: () => startLearn(16) }}>...
//
// Gedrag:
// - Rechtermuis (contextmenu)        → roept onTrigger aan
// - Touch long-press (3 sec ingedrukt) → roept onTrigger aan
// - Touch waarbij gebruiker vinger verschuift > 10px → annuleert
//
// Optionele extra opties:
//   pressDelayMs   — long-press duur (default 3000)
//   moveThresholdPx — beweging-tolerantie (default 10)
//   onTrigger      — callback wanneer learn moet starten

const LONG_PRESS_MS_DEFAULT = 3000;
const MOVE_THRESHOLD_DEFAULT = 10;

export function midiLearn(node, params = {}) {
  let opts = normalize(params);
  let timer = null;
  let startX = 0;
  let startY = 0;
  let triggered = false;

  function fire() {
    if (triggered) return;
    triggered = true;
    try { opts.onTrigger?.(); } catch (e) { console.error('midiLearn onTrigger error:', e); }
    // Klein haptisch signaal op touch — als browser het ondersteunt
    if (navigator.vibrate) {
      try { navigator.vibrate(40); } catch (e) {}
    }
  }

  function clearTimer() {
    if (timer) {
      clearTimeout(timer);
      timer = null;
    }
  }

  function onContextMenu(e) {
    e.preventDefault();
    e.stopPropagation();
    triggered = false;
    fire();
  }

  function onTouchStart(e) {
    if (e.touches.length !== 1) return;
    triggered = false;
    startX = e.touches[0].clientX;
    startY = e.touches[0].clientY;
    clearTimer();
    timer = setTimeout(() => {
      fire();
    }, opts.pressDelayMs);
  }

  function onTouchMove(e) {
    if (!timer) return;
    const t = e.touches[0];
    if (!t) return;
    const dx = t.clientX - startX;
    const dy = t.clientY - startY;
    if (Math.hypot(dx, dy) > opts.moveThresholdPx) {
      clearTimer();
    }
  }

  function onTouchEnd() {
    clearTimer();
  }

  node.addEventListener('contextmenu', onContextMenu);
  node.addEventListener('touchstart', onTouchStart, { passive: true });
  node.addEventListener('touchmove', onTouchMove, { passive: true });
  node.addEventListener('touchend', onTouchEnd);
  node.addEventListener('touchcancel', onTouchEnd);

  return {
    update(newParams) {
      opts = normalize(newParams);
    },
    destroy() {
      clearTimer();
      node.removeEventListener('contextmenu', onContextMenu);
      node.removeEventListener('touchstart', onTouchStart);
      node.removeEventListener('touchmove', onTouchMove);
      node.removeEventListener('touchend', onTouchEnd);
      node.removeEventListener('touchcancel', onTouchEnd);
    },
  };
}

function normalize(p) {
  return {
    pressDelayMs: p.pressDelayMs ?? LONG_PRESS_MS_DEFAULT,
    moveThresholdPx: p.moveThresholdPx ?? MOVE_THRESHOLD_DEFAULT,
    onTrigger: p.onTrigger,
  };
}
