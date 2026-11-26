import test from 'node:test';
import assert from 'node:assert/strict';
import { newOrganProject, chooseProjectStop, withoutProjectSource, addProjectDivision,
  organProjectError, sameOrganProjectPath, ORGAN_PROJECT_MAX_SOURCES,
  organProjectSourceLabel } from '../ui/src/lib/organProject.js';

const fixture = () => ({ ...newOrganProject('b866fd99-29e2-493f-a57c-d8d1c005831b'), sources: [
  { id: 'source-a', path: 'C:/sets/a.organ', fingerprint: 'a', reference_a4_hz: 415, tuning_policy: 'normalize', tuning_cents: 0 },
  { id: 'source-b', path: 'C:/sets/b.organ', fingerprint: 'b', reference_a4_hz: 440, tuning_policy: 'original', tuning_cents: 10 },
] });
const catalogs = { 'source-a': { fingerprint: 'a', stops: [{ id: 1 }] }, 'source-b': { fingerprint: 'b', stops: [{ id: 1 }] } };

test('two source stops with the same native ID remain distinct project targets', () => {
  const original = fixture();
  const first = chooseProjectStop(original, 'source-a', 1, true, 2);
  const both = chooseProjectStop(first, 'source-b', 1, true, 3);
  assert.equal(original.stops.length, 0);
  assert.deepEqual(both.stops.map(stop => [stop.id, stop.source_id, stop.division_id]), [[1, 'source-a', 2], [2, 'source-b', 3]]);
  assert.equal(organProjectError(both, catalogs), null);
  assert.deepEqual(both.sources, original.sources);
});

test('deleted stop IDs are never reused after save/reopen or source removal', () => {
  let project = chooseProjectStop(fixture(), 'source-a', 1, true);
  project = chooseProjectStop(project, 'source-b', 1, true);
  const removedId = project.stops[1].id;
  project = chooseProjectStop(project, 'source-b', 1, false);
  project = JSON.parse(JSON.stringify(project));
  project = chooseProjectStop(project, 'source-b', 2, true);
  assert(project.stops[1].id > removedId);
  const counter = project.next_stop_id;
  project = withoutProjectSource(project, 'source-b');
  assert.equal(project.next_stop_id, counter);
  assert.deepEqual(project.stops.map(stop => stop.source_id), ['source-a']);
});

test('repeated selection keeps the existing ID and mapping', () => {
  const selected = chooseProjectStop(fixture(), 'source-a', 1, true, 3);
  const repeated = chooseProjectStop(selected, 'source-a', 1, true, 2);
  assert.deepEqual(repeated, selected);
});

test('division IDs are monotonic even after an empty division is removed', () => {
  let project = addProjectDivision(fixture());
  const removedId = project.divisions.at(-1).id;
  project.divisions.pop();
  project = addProjectDivision(JSON.parse(JSON.stringify(project)));
  assert(project.divisions.at(-1).id > removedId);
  assert.deepEqual(project.divisions.slice(0, 3), fixture().divisions);
});

test('a new division uses an available MIDI channel when the highest is 16', () => {
  const project = fixture();
  project.divisions[2].midi_channel = 16;
  const next = addProjectDivision(project);
  assert.equal(next.divisions.at(-1).midi_channel, 3);
  assert.equal(new Set(next.divisions.map(division => division.midi_channel)).size, 4);
});

test('changed, missing and incomplete sources block save/load in the editor', () => {
  let project = chooseProjectStop(fixture(), 'source-a', 1, true);
  assert.match(organProjectError(project, catalogs), /iedere bron/);
  project = chooseProjectStop(project, 'source-b', 1, true);
  assert.match(organProjectError(project, { ...catalogs, 'source-b': { ...catalogs['source-b'], fingerprint: 'changed' } }), /gewijzigd/);
  assert.match(organProjectError(project, { 'source-a': catalogs['source-a'] }), /Analyseer/);
  assert.match(organProjectError(project, { ...catalogs, 'source-b': { fingerprint: 'b', stops: [] } }), /ontbreekt/);
});

test('unsafe klavier ranges and source tuning inputs cannot be committed', () => {
  let project = chooseProjectStop(chooseProjectStop(fixture(), 'source-a', 1, true), 'source-b', 1, true);
  project.divisions[0].key_count = 128;
  assert.match(organProjectError(project, catalogs), /toetsbereik/);
  project.divisions[0].key_count = 30;
  project.sources[0].reference_a4_hz = NaN;
  assert.match(organProjectError(project, catalogs), /referentietoonhoogte/);
  project.sources[0].reference_a4_hz = 415;
  project.sources[1].tuning_policy = 'unknown';
  assert.match(organProjectError(project, catalogs), /stembeleid/);
});

test('path matching accepts Windows notation and preserves Unix case sensitivity', () => {
  assert(sameOrganProjectPath('C:\\Orgels\\Mijn.jmorgan', 'c:/orgels/mijn.jmorgan'));
  assert(!sameOrganProjectPath('/orgels/A.jmorgan', '/orgels/a.jmorgan'));
  assert(!sameOrganProjectPath(null, 'C:/orgel.jmorgan'));
});

test('large dispositions allow 2048 persistent register targets and reject the next', () => {
  let project = fixture();
  project.stops = Array.from({ length: 2048 }, (_, index) => ({ id: index + 1,
    source_id: index % 2 ? 'source-a' : 'source-b', source_stop_id: index + 1, division_id: 2, name: null }));
  project.next_stop_id = 2049;
  assert.equal(organProjectError(project), null);
  project = chooseProjectStop(project, 'source-a', 2049, true);
  assert.match(organProjectError(project), /2.048 registers/);
});

test('one through 64 selected sources are valid and the next is rejected without truncation', () => {
  let project = newOrganProject();
  assert.match(organProjectError(project), /één tot 64/);
  for (let index = 0; index < ORGAN_PROJECT_MAX_SOURCES; index++) {
    project.sources.push({ ...fixture().sources[0], id: `source-${index}`, path: `C:/sets/${index}.organ` });
    project = chooseProjectStop(project, `source-${index}`, 1, true);
    assert.equal(organProjectError(project), null);
  }
  assert.equal(project.sources.length, 64);
  assert.equal(new Set(project.stops.map(stop => stop.id)).size, 64);
  project.sources.push({ ...fixture().sources[0], id: 'source-64', path: 'C:/sets/64.organ' });
  assert.match(organProjectError(project), /één tot 64/);
  assert.equal(project.sources.length, 65);
});

test('many-source reorder and middle-source replacement preserve other register targets', () => {
  let project = fixture();
  project.sources.push({ ...project.sources[0], id: 'source-c', path: 'C:/sets/c.organ' },
    { ...project.sources[1], id: 'source-d', path: 'C:/sets/d.organ' });
  for (const source of project.sources) project = chooseProjectStop(project, source.id, 1, true);
  const original = project.stops.map(stop => ({ ...stop }));
  project.sources.reverse();
  assert.deepEqual(project.stops, original);
  project = withoutProjectSource(project, 'source-b');
  project = JSON.parse(JSON.stringify(project));
  project.sources.push({ ...project.sources[0], id: 'source-e', path: 'C:/sets/e.organ' });
  project = chooseProjectStop(project, 'source-e', 1, true);
  assert.deepEqual(project.stops.slice(0, -1), original.filter(stop => stop.source_id !== 'source-b'));
  assert(project.stops.at(-1).id > Math.max(...original.map(stop => stop.id)));
  assert.equal(organProjectError(project), null);
});

test('source labels distinguish C, Z, AA and BL within the supported 64 sources', () => {
  assert.deepEqual([0, 1, 2, 25, 26, 51, 52, 63].map(organProjectSourceLabel), ['A', 'B', 'C', 'Z', 'AA', 'AZ', 'BA', 'BL']);
});
