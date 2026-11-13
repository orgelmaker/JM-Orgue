import assert from 'node:assert/strict';
import { kiesOudeInstellingen, maakMigratieBewaking, bewaakOrgelHerstel, voltooiOrgelHerstel } from '../ui/src/lib/legacySettings.js';

const candidate = { organ_id: 'a.organ', legacy_path: '.jm-settings.json', has_settings: false, deferred: false };
async function run({ item = candidate, answers = [], handmatig = false, failure = null, deferFailure = null } = {}) {
  const calls = [];
  const result = await kiesOudeInstellingen({
    handmatig,
    inspecteer: async () => { if (failure) throw failure; return item; },
    bevestig: async (_, vervangen) => { calls.push(vervangen ? 'vervangen' : 'kiezen'); return answers.shift(); },
    meldOntbrekend: async () => calls.push('ontbreekt'),
    waarschuw: () => calls.push('waarschuwing'),
    stelUit: async (k) => { calls.push(`later:${k.organ_id}`); if (deferFailure) throw deferFailure; },
    importeer: async (k) => calls.push(`import:${k.organ_id}`),
  });
  return { result, calls };
}

assert.deepEqual(await run({ answers: [true] }), { result: true, calls: ['kiezen', 'import:a.organ'] });
assert.deepEqual(await run({ answers: [false] }), { result: true, calls: ['kiezen', 'later:a.organ'] });
assert.deepEqual(await run({ item: { ...candidate, deferred: true } }), { result: true, calls: [] });
assert.deepEqual(await run({ item: { ...candidate, deferred: true }, handmatig: true, answers: [true] }),
  { result: true, calls: ['kiezen', 'import:a.organ'] });
assert.deepEqual(await run({ item: { ...candidate, deferred: true }, handmatig: true, answers: [false] }),
  { result: false, calls: ['kiezen'] });
await assert.rejects(run({ answers: [false], deferFailure: Error('opslaan mislukt') }), /opslaan mislukt/);
assert.deepEqual(await run({ item: { ...candidate, has_settings: true } }), { result: true, calls: [] });
assert.deepEqual(await run({ item: { ...candidate, has_settings: true }, handmatig: true, answers: [true, true] }),
  { result: true, calls: ['kiezen', 'vervangen', 'import:a.organ'] });
assert.deepEqual(await run({ item: { ...candidate, has_settings: true }, handmatig: true, answers: [true, false] }),
  { result: false, calls: ['kiezen', 'vervangen'] });
assert.deepEqual(await run({ handmatig: true, item: null }), { result: false, calls: ['ontbreekt'] });
assert.deepEqual(await run({ failure: Error('leesfout') }), { result: true, calls: ['waarschuwing'] });
await assert.rejects(run({ handmatig: true, failure: Error('leesfout') }), /leesfout/);

// Een nieuwe UI-instantie leest de onthouden keuze uit de backend. Een ander
// orgel in dezelfde map blijft zijn eigen vraag krijgen; handmatig blijft mogelijk.
let persisted = '[]';
const calls = [];
const loadSession = () => {
  const deferred = new Set(JSON.parse(persisted));
  return (organId, handmatig = false) => kiesOudeInstellingen({
    handmatig,
    inspecteer: async () => ({ ...candidate, organ_id: organId, deferred: deferred.has(organId) }),
    bevestig: async () => { calls.push(`vraag:${organId}`); return false; },
    stelUit: async () => { deferred.add(organId); persisted = JSON.stringify([...deferred]); },
    importeer: async () => { throw Error('niet bevestigd'); },
  });
};
await loadSession()('a.organ');
assert.deepEqual(calls, ['vraag:a.organ']);
await loadSession()('a.organ');
assert.deepEqual(calls, ['vraag:a.organ']);
await loadSession()('b.organ');
assert.deepEqual(calls, ['vraag:a.organ', 'vraag:b.organ']);
await loadSession()('a.organ', true);
assert.deepEqual(calls, ['vraag:a.organ', 'vraag:b.organ', 'vraag:a.organ']);
// Import gevolgd door mislukte lading: ook een expliciete save vóór een retry
// mag de oude runtime niet opslaan. Pas succes van de nieuwe lading heft dit op.
const guard = maakMigratieBewaking();
assert.equal(guard.kanOpslaan(false), true);
guard.importGeslaagd();
assert.equal(guard.openstaand, true);
assert.equal(guard.kanOpslaan(false), false);
assert.equal(guard.kanOpslaan(true, true), false);
guard.ladenGeslaagd();
assert.equal(guard.openstaand, false);
assert.equal(guard.kanOpslaan(false), true);
assert.equal(guard.kanOpslaan(true), false);
assert.equal(guard.kanOpslaan(true, true), true);

// De backend kan al geladen zijn terwijl de UI nog een oud volume toont.
// Opslag blijft geblokkeerd tot juist ook dat asynchrone herstel klaar is.
let geefUiVrij;
guard.importGeslaagd();
const voltooid = voltooiOrgelHerstel(() => new Promise(resolve => { geefUiVrij = resolve; }), guard);
assert.equal(guard.kanOpslaan(false), false);
geefUiVrij();
await voltooid;
assert.equal(guard.kanOpslaan(false), true);
guard.importGeslaagd();
await assert.rejects(voltooiOrgelHerstel(async () => { throw Error('UI niet hersteld'); }, guard), /UI niet hersteld/);
assert.equal(guard.kanOpslaan(false), false);

// Een nieuwe generatie van hetzelfde orgel maakt een vertraagd oud antwoord
// ongeldig; een volgende actie van die oude run mag helemaal niet starten.
let generatie = 1;
const doeHerstel = bewaakOrgelHerstel(() => generatie === 1);
let geefAntwoord;
const oudAntwoord = doeHerstel(() => new Promise(resolve => { geefAntwoord = resolve; }));
generatie = 2;
geefAntwoord(-4);
await assert.rejects(oudAntwoord, /tijdens het herstellen gewijzigd/);
let gepusht = false;
await assert.rejects(doeHerstel(() => { gepusht = true; }), /tijdens het herstellen gewijzigd/);
assert.equal(gepusht, false);
console.log('legacy_settings_test: alle keuzes in orde');
