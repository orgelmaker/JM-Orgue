// Application action IDs are wider than MIDI bytes. Resolve by the advertised
// DTO identity, never by a range that could mistake a large coupler for a stop.
// Ambiguous IDs are deliberately unavailable rather than operating two objects.
export function midiActionTargets(organ) {
  const targets = new Map(), ambiguous = new Set();
  const add = (kind, item, division = null) => {
    const code = item.midi_action_code;
    if (!Number.isSafeInteger(code) || code < 100 || code > 0xffffffff) return;
    if (targets.has(code) || ambiguous.has(code)) {
      targets.delete(code); ambiguous.add(code); return;
    }
    targets.set(code, { kind, item, division });
  };
  for (const division of organ?.divisions || [])
    for (const stop of division.stops || []) add('stop', stop, division);
  for (const coupler of organ?.couplers || []) add('coupler', coupler);
  for (const piston of organ?.schakelaars?.pistons || []) add('piston', piston);
  return targets;
}
