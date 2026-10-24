// Node-test voor ui/src/lib/stepChord.js (0.9.22): `node testscripts/stepChord.test.mjs`
import assert from 'node:assert/strict';
import { maakStapAkkoord, groepeerPerBalk, routeringOverzicht, stapSleutel } from '../ui/src/lib/stepChord.js';

let n = 0;
function test(naam, f) { f(); n += 1; console.log('ok -', naam); }

test('sleutel bevat kanaal en noot', () => {
  assert.equal(stapSleutel(0, 60), '0:60');
  assert.equal(stapSleutel(null, 60), 'x:60');
  assert.notEqual(stapSleutel(0, 60), stapSleutel(1, 60));
});

test('dezelfde noot op twee kanalen blijft twee noten en wacht op beide NoteOffs', () => {
  const a = maakStapAkkoord();
  a.noteOn(60, 0, 'Hoofdwerk', 1);
  a.noteOn(60, 1, 'Zwelwerk', 2);
  assert.equal(a.aantalIngedrukt, 2);
  // Eerste kanaal loslaten: nog niets vastleggen (0.9.21-bug: hier werd al gecommit met één noot).
  assert.equal(a.noteOff(60, 0), null);
  assert.equal(a.aantalIngedrukt, 1);
  const noten = a.noteOff(60, 1);
  assert.ok(Array.isArray(noten));
  assert.equal(noten.length, 2);
  assert.deepEqual(noten.map(x => x.channel).sort(), [0, 1]);
});

test('gewoon akkoord op één kanaal: drie noten, één commit', () => {
  const a = maakStapAkkoord();
  a.noteOn(60, 0); a.noteOn(64, 0); a.noteOn(67, 0);
  assert.equal(a.noteOff(64, 0), null);
  assert.equal(a.noteOff(60, 0), null);
  const noten = a.noteOff(67, 0);
  assert.deepEqual(noten.map(x => x.midi).sort((p, q) => p - q), [60, 64, 67]);
  assert.equal(a.aantalInAkkoord, 0);
});

test('groepering: doelbalk van de backend gaat voor, dan divisie, dan standaard', () => {
  const layers = [{ id: 1, divisions: ['Hoofdwerk'] }, { id: 2, divisions: ['Zwelwerk'] }, { id: 3, divisions: ['Pedaal'] }];
  const noten = [
    { midi: 60, channel: 0, division: 'Hoofdwerk', layer: 2 }, // backend zegt balk 2 (eigen kanaal) → wint van divisie
    { midi: 64, channel: 1, division: 'Zwelwerk', layer: null }, // via divisie → 2
    { midi: 36, channel: 2, division: null, layer: null },       // niets → standaard
    { midi: 60, channel: 5, division: 'Zwelwerk', layer: null }, // zelfde toonhoogte, landt ook op 2 → samenvallen
  ];
  const g = groepeerPerBalk(noten, layers, 1);
  const m = new Map(g);
  assert.deepEqual(m.get(2).sort((p, q) => p - q), [60, 64]);
  assert.deepEqual(m.get(1), [36]);
});

test('routering: overzicht en waarschuwingen', () => {
  const layers = [
    { id: 1, name: 'Rechts', midi_channel: 0, divisions: [] },
    { id: 2, name: 'Links', midi_channel: null, divisions: ['Zwelwerk'] },
    { id: 3, name: 'Pedaal', midi_channel: null, divisions: [] },
    { id: 4, name: 'Extra', midi_channel: 0, divisions: [] },
  ];
  const mappings = [{ division: 'Zwelwerk', channel: 1 }];
  const { regels, waarschuwingen } = routeringOverzicht(layers, mappings, 3);
  assert.equal(regels[0].bron, 'kanaal');
  assert.equal(regels[1].bron, 'divisie');
  assert.equal(regels[1].divisies[0].kanaal, 1);
  assert.equal(regels[2].bron, 'armed');
  const soorten = waarschuwingen.map(w => w.soort);
  assert.ok(soorten.includes('ongebonden'));
  const overlap = waarschuwingen.find(w => w.soort === 'overlap');
  assert.ok(overlap && overlap.a === 'Rechts' && overlap.b === 'Extra' && overlap.kanaal === 0);
});

console.log(`${n} tests geslaagd`);
