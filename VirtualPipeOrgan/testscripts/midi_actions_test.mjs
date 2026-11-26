import test from 'node:test';
import assert from 'node:assert/strict';
import { midiActionTargets } from '../ui/src/lib/midiActions.js';

test('2048 project register actions stay distinct from extended couplers and pistons', () => {
  const stops = Array.from({ length: 2048 }, (_, i) => ({ id: `project_${i + 1}`, midi_action_code: 65536 + i + 1 }));
  const organ = { divisions: [{ name: 'Manuaal', stops }], couplers: [
    { id: 'coupler_1', midi_action_code: 262144 }, { id: 'coupler_512', midi_action_code: 262655 },
  ], schakelaars: { pistons: [{ index: 0, midi_action_code: 262656 }] } };
  const map = midiActionTargets(organ);
  assert.equal(map.size, 2051);
  assert.equal(map.get(65537).item.id, 'project_1');
  assert.equal(map.get(67584).item.id, 'project_2048');
  assert.equal(map.get(262655).kind, 'coupler');
  assert.equal(map.get(262656).kind, 'piston');
});

test('legacy object action values resolve unchanged and reserved MIDI functions are not object IDs', () => {
  const organ = { divisions: [{ stops: [{ id: 'stop', midi_action_code: 150 }] }],
    couplers: [{ id: 'coupler', midi_action_code: 100 }], schakelaars: { pistons: [{ index: 0, midi_action_code: 200 }] } };
  const map = midiActionTargets(organ);
  assert.equal(map.get(150).kind, 'stop'); assert.equal(map.get(100).kind, 'coupler');
  assert.equal(map.get(200).kind, 'piston'); assert.equal(map.has(42), false);
});

test('duplicate action IDs never operate an arbitrary first object', () => {
  const map = midiActionTargets({ divisions: [{ stops: [{ id: 'a', midi_action_code: 255 }, { id: 'b', midi_action_code: 255 }] }],
    couplers: [{ id: 'c', midi_action_code: 255 }] });
  assert.equal(map.has(255), false);
});
