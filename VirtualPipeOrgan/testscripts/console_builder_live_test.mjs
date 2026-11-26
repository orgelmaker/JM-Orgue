// Runtime integration test. Refuses the normal profile and real samplesets.
import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import path from 'node:path';
import { parseArgs } from 'node:util';

const { values } = parseArgs({ options: {
  base: { type: 'string', default: 'http://127.0.0.1:18766' },
  'fixture-dir': { type: 'string' },
  'verify-existing': { type: 'boolean', default: false },
  'verify-texture': { type: 'boolean', default: false },
} });
assert(!(values['verify-existing'] && values['verify-texture']), 'Kies één verificatiemodus');
assert(values['fixture-dir'], 'Geef --fixture-dir expliciet op');
const base = new URL(values.base);
assert(base.protocol === 'http:' && base.hostname === '127.0.0.1' && base.port,
  'Alleen een expliciete lokale test-API is toegestaan');
assert(!base.username && !base.password && base.pathname === '/' && !base.search && !base.hash);
const fixture = await fs.realpath(path.resolve(values['fixture-dir']));
const normalized = p => {
  const absolute = path.resolve(p).replaceAll('\\', '/');
  return process.platform === 'win32' ? absolute.toLowerCase() : absolute;
};
assert(normalized(fixture).endsWith('/testen/consolebouwer-2026-11-26/orgels'),
  'Dit script werkt uitsluitend in de eigen Consolebouwer-fixturemap');
await fs.access(path.join(fixture, '.jm-console-test-fixture'));
const profile = await fs.realpath(path.join(fixture, '..', 'profile'));
const sentinel = JSON.parse(await fs.readFile(path.join(profile, 'consolebouwer-test-profile.json'), 'utf8'));
assert.equal(sentinel.purpose, 'jm-orgue-consolebouwer-live-test', 'Testprofielmarker ontbreekt');
const ids = ['Console A.organ', 'Console B.organ'].map(name => path.join(fixture, name));
for (const id of ids) {
  assert.equal(normalized(await fs.realpath(id)), normalized(id), 'Een fixture mag niet naar een echte sampleset verwijzen');
  assert((await fs.stat(id)).isFile());
}
const checks = [];
const report = name => { checks.push(name); console.log(`OK ${name}`); };
async function request(route, body, expected = 200) {
  const response = await fetch(new URL(route, base), {
    method: body === undefined ? 'GET' : 'POST',
    headers: body === undefined ? {} : { 'Content-Type': 'application/json' },
    body: body === undefined ? undefined : JSON.stringify(body),
    signal: AbortSignal.timeout(120000),
  });
  const result = await response.json();
  assert.equal(response.status, expected, `${route}: ${JSON.stringify(result)}`);
  return result;
}
const environment = await request('/test/profile');
assert.equal(normalized(environment.app_data_dir), normalized(profile),
  'De draaiende app gebruikt niet dit geïsoleerde testprofiel; geen mutaties uitgevoerd');
report('draaiende app gebruikt het expliciete testprofiel');
async function load(id) {
  await request('/load_organ', { path: id });
  const organ = await request('/organ');
  assert.equal(normalized(organ.id), normalized(id));
  return organ;
}
const read = async id => (await request(`/settings/console_layout?organ_id=${encodeURIComponent(id)}`)).layout;
const apply = (id, layout, expected = 200) => request('/settings/console_layout', { organ_id: id, layout }, expected);
const clone = x => structuredClone(x);
function design(organ, suffix) {
  const stop = organ.divisions.flatMap(d => d.stops)[0];
  assert(stop, 'Fixture moet een register bevatten');
  return {
    version: 1, enabled: true, name: `Live console ${suffix}`, theme: suffix === 'A' ? 'walnut' : 'ivory',
    pages: [{ id: 'main', name: 'Hoofdscherm', width: 1200, height: 760, elements: [{
      id: 'register-1', kind: 'stop', target_id: stop.id, label: `Wipper ${suffix}`, x: suffix === 'A' ? 48 : 208, y: 80,
      width: 104, height: 92, appearance: 'rocker',
    }] }],
  };
}

let first, second;
if (values['verify-texture']) {
  // Keep the GUI's current organ, registration and successful image import.
  // The only POST requests in this mode are intentionally invalid layouts.
  const organ = await request('/organ');
  assert(organ && ids.some(id => normalized(id) === normalized(organ.id)), 'Laad eerst Console A of B in het testprofiel');
  const id = ids.find(candidate => normalized(candidate) === normalized(organ.id));
  const current = await read(id);
  assert.equal(current?.texture, 'custom', 'Importeer eerst de fixturetextuur via Consolebouwer');
  const imagePath = path.join(fixture, 'eigen-testtextuur.png');
  assert.equal(normalized(await fs.realpath(imagePath)), normalized(imagePath));
  const imageBytes = await fs.readFile(imagePath);
  assert(imageBytes.length > 0 && imageBytes.length <= 2 * 1024 * 1024);
  const expectedDataUrl = `data:image/png;base64,${imageBytes.toString('base64')}`;
  assert.equal(current.custom_texture?.data_url, expectedDataUrl, 'De GUI heeft niet de gekozen fixtureafbeelding opgeslagen');
  report('GUI-import bewaart precies de gekozen eigen PNG-textuur');
  const libraryPath = path.join(profile, 'organ_library.json');
  const diskBefore = JSON.parse(await fs.readFile(libraryPath, 'utf8'));
  const storedLayout = (library, organId) => Object.entries(library.settings || {})
    .find(([key]) => normalized(key) === normalized(organId))?.[1]?.console_layout;
  assert.deepEqual(storedLayout(diskBefore, id), current, 'De textuur moet ook duurzaam in de bibliotheek staan');
  first = storedLayout(diskBefore, ids[0]); second = storedLayout(diskBefore, ids[1]);
  report('eigen textuur staat identiek in de bibliotheek op schijf');
  const drawnBefore = await request('/stops/drawn');
  const oversized = Buffer.alloc(2 * 1024 * 1024 + 1);
  imageBytes.subarray(0, 8).copy(oversized);
  const cases = [
    ['SVG-type', l => { l.custom_texture.data_url = `data:image/svg+xml;base64,${Buffer.from('<svg/>').toString('base64')}`; }],
    ['verkeerde bestandssignatuur', l => { l.custom_texture.data_url = expectedDataUrl.replace('image/png', 'image/jpeg'); }],
    ['afbeelding boven 2 MiB', l => { l.custom_texture.data_url = `data:image/png;base64,${oversized.toString('base64')}`; }],
    ['ongeldige base64', l => { l.custom_texture.data_url = 'data:image/png;base64,iVBORw0KGgo=!'; }],
    ['textuurschaal boven maximum', l => { l.texture_scale = 401; }],
    ['textuurdekking boven maximum', l => { l.texture_opacity = 2; }],
  ];
  for (const [name, mutate] of cases) {
    const invalid = clone(current); mutate(invalid);
    await apply(id, invalid, 400);
    assert.deepEqual(await read(id), current, `${name} mag het geldige GUI-ontwerp niet wijzigen`);
    report(`${name} veilig afgewezen met behoud van de eigen textuur`);
  }
  const diskAfter = JSON.parse(await fs.readFile(libraryPath, 'utf8'));
  for (const organId of ids) assert.deepEqual(storedLayout(diskAfter, organId), storedLayout(diskBefore, organId));
  assert.equal(normalized((await request('/organ')).id), normalized(id), 'De verificatie mag niet van orgel wisselen');
  assert.deepEqual(await request('/stops/drawn'), drawnBefore);
  report('orgelkeuze, registratie en beide opgeslagen consoleontwerpen blijven behouden');
} else if (values['verify-existing']) {
  for (const [i, id] of ids.entries()) {
    await load(id);
    const layout = await read(id);
    assert(layout?.version === 1 && layout.pages.length > 0, 'Bewaard consoleontwerp ontbreekt');
    if (i === 0) first = layout; else second = layout;
    report(`bewaard ontwerp ${i ? 'B' : 'A'} teruggelezen na laden`);
  }
  assert.notDeepEqual(first, second, 'A en B moeten aantoonbaar eigen ontwerpen hebben');
  await load(ids[0]);
  assert.deepEqual(await read(ids[0]), first);
  report('A → B → A behoudt twee afzonderlijke ontwerpen');
} else {
  const organA = await load(ids[0]);
  await request('/settings/save', {});
  const before = await request('/settings/organ');
  const drawnBefore = await request('/stops/drawn');
  first = design(organA, 'A');
  await apply(ids[0], first);
  assert.deepEqual(await read(ids[0]), first);
  assert.deepEqual(await request('/stops/drawn'), drawnBefore, 'Ontwerp opslaan mag geen registers activeren');
  const after = await request('/settings/organ');
  for (const key of ['master_volume_db', 'presets', 'midi_mappings', 'pipe_voicings', 'eq', 'temperament']) {
    assert.deepEqual(after[key], before[key], `${key} gewijzigd door Consolebouwer`);
  }
  report('A opgeslagen met behoud van registratie, volume, MIDI en klankinstellingen');
  for (const mutate of [
    l => { l.version = 99; },
    l => { l.pages[0].elements[0].x = 2400; },
    l => { l.pages[0].elements[0].target_id = 'bestaat-niet'; },
    l => { l.pages[0].elements.push(clone(l.pages[0].elements[0])); },
  ]) {
    const invalid = clone(first); mutate(invalid);
    await apply(ids[0], invalid, 400);
    assert.deepEqual(await read(ids[0]), first, 'Afgewezen ontwerp mag opslag niet wijzigen');
  }
  await apply(ids[1], first, 400);
  assert.deepEqual(await read(ids[0]), first);
  report('verkeerde versie, geometrie, register, dubbele ID en verkeerd orgel veilig afgewezen');
  await request('/settings/save', {});
  assert.deepEqual(await read(ids[0]), first);
  report('autosave behoudt toegepast ontwerp');
  await apply(ids[0], null);
  await request('/settings/save', {});
  assert.equal(await read(ids[0]), null);
  await apply(ids[0], first);
  report('bewuste reset wordt door autosave niet teruggedraaid');
  const organB = await load(ids[1]);
  second = design(organB, 'B');
  await apply(ids[1], second);
  await request('/settings/save', {});
  await apply(ids[0], first, 400);
  assert.deepEqual(await read(ids[1]), second);
  await load(ids[0]);
  assert.deepEqual(await read(ids[0]), first);
  await load(ids[1]);
  assert.deepEqual(await read(ids[1]), second);
  report('twee definities in dezelfde map behouden eigen ontwerpen bij orgelwissels');
  const sidecars = (await fs.readdir(fixture)).filter(n => /^\.jm-settings-organ-[a-f0-9]+\.json$/.test(n));
  const portable = await Promise.all(sidecars.map(async name => JSON.parse(await fs.readFile(path.join(fixture, name), 'utf8'))));
  for (const [i, id] of ids.entries()) {
    const saved = portable.find(p => p.naam === path.basename(id));
    assert(saved, `Eigen zijbestand voor ${id} ontbreekt`);
    assert.deepEqual(saved.instellingen.console_layout, i === 0 ? first : second);
  }
  report('afzonderlijke zijbestanden bevatten de juiste definitie en het juiste ontwerp');
  await load(ids[0]);
}
await fs.writeFile(path.join(profile, values['verify-texture'] ? 'consolebouwer-live-texture-result.json' : 'consolebouwer-live-result.json'), JSON.stringify({
  time: new Date().toISOString(), processId: environment.process_id,
  mode: values['verify-texture'] ? 'verify-texture' : values['verify-existing'] ? 'verify-existing' : 'write-and-verify', checks,
  designs: [first?.name ?? null, second?.name ?? null],
}, null, 2));
console.log(`console_builder_live_test: ${checks.length} controles geslaagd`);
