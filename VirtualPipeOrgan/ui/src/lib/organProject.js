// Project IDs survive editing and reordering. Counters are persisted so a
// removed stop can never silently become another console/preset target.
export const ORGAN_PROJECT_VERSION = 1;
export const ORGAN_PROJECT_MAX_STOPS = 2048;
export const ORGAN_PROJECT_MAX_SOURCES = 64;
export const cloneOrganProject = project => JSON.parse(JSON.stringify(project));

export function organProjectSourceLabel(index) {
  if (!Number.isSafeInteger(index) || index < 0) return '?';
  let number = index + 1, label = '';
  while (number > 0) {
    number -= 1;
    label = String.fromCharCode(65 + number % 26) + label;
    number = Math.floor(number / 26);
  }
  return label;
}

export function newOrganProject(id = globalThis.crypto.randomUUID()) {
  return {
    schema_version: ORGAN_PROJECT_VERSION, id, name: 'Mijn orgel', target_a4_hz: 440,
    sources: [], divisions: [
      { id: 1, name: 'Pedaal', midi_channel: 1, first_midi_note: 36, key_count: 30, pedal: true },
      { id: 2, name: 'Manuaal I', midi_channel: 2, first_midi_note: 36, key_count: 61, pedal: false },
      { id: 3, name: 'Manuaal II', midi_channel: 3, first_midi_note: 36, key_count: 61, pedal: false },
    ], stops: [], couplers: [], next_stop_id: 1, next_division_id: 4,
  };
}

export function chooseProjectStop(project, sourceId, sourceStopId, selected, divisionId = null) {
  const next = cloneOrganProject(project);
  const existing = next.stops.find(stop => stop.source_id === sourceId && stop.source_stop_id === sourceStopId);
  if (!selected) {
    next.stops = next.stops.filter(stop => stop !== existing);
  } else if (!existing) {
    if (!next.sources.some(source => source.id === sourceId)) throw new Error('Onbekende bron.');
    const division = next.divisions.find(division => division.id === divisionId) || next.divisions.find(division => !division.pedal) || next.divisions[0];
    if (!division) throw new Error('Voeg eerst een klavier toe.');
    const id = Math.max(next.next_stop_id || 1, ...next.stops.map(stop => stop.id + 1));
    if (id > 65535) throw new Error('De register-ID-ruimte van dit project is vol.');
    next.stops.push({ id, source_id: sourceId, source_stop_id: sourceStopId, division_id: division.id, name: null });
    next.next_stop_id = id + 1;
  }
  return next;
}

export function withoutProjectSource(project, sourceId) {
  const next = cloneOrganProject(project);
  next.sources = next.sources.filter(source => source.id !== sourceId);
  next.stops = next.stops.filter(stop => stop.source_id !== sourceId);
  return next;
}

export function addProjectDivision(project) {
  const next = cloneOrganProject(project);
  const id = Math.max(next.next_division_id || 1, ...next.divisions.map(division => division.id + 1));
  const usedChannels = new Set(next.divisions.map(division => division.midi_channel));
  const channel = Array.from({ length: 16 }, (_, index) => index + 1).find(channel => !usedChannels.has(channel)) || 1;
  next.divisions.push({ id, name: `Manuaal ${id - 1}`, midi_channel: channel, first_midi_note: 36, key_count: 61, pedal: false });
  next.next_division_id = id + 1;
  return next;
}

export function organProjectError(project, catalogs = null) {
  if (!project || project.schema_version !== ORGAN_PROJECT_VERSION) return 'Dit projectformaat wordt niet ondersteund.';
  if (!project.name?.trim()) return 'Geef het orgel een naam.';
  if (!Number.isFinite(project.target_a4_hz) || project.target_a4_hz < 300 || project.target_a4_hz > 550) return 'Kies een concerttoonhoogte tussen 300 en 550 Hz.';
  if (!Array.isArray(project.sources) || project.sources.length < 1 || project.sources.length > ORGAN_PROJECT_MAX_SOURCES) return `Kies één tot ${ORGAN_PROJECT_MAX_SOURCES} bronorgels.`;
  if (!project.stops.length) return 'Kies ten minste één register uit iedere bron.';
  if (project.stops.length > ORGAN_PROJECT_MAX_STOPS) return `Een project ondersteunt maximaal ${ORGAN_PROJECT_MAX_STOPS.toLocaleString('nl-NL')} registers.`;
  const sourceIds = new Set(project.sources.map(source => source.id));
  const stopIds = new Set(project.stops.map(stop => stop.id));
  if (sourceIds.size !== project.sources.length || stopIds.size !== project.stops.length) return 'Het project bevat dubbele IDs.';
  if (!project.divisions.length || project.divisions.length > 8) return 'Gebruik één tot acht klavieren.';
  if (project.divisions.filter(division => division.pedal).length > 1) return 'Kies maximaal één pedaalklavier.';
  for (const source of project.sources) {
    if (!project.stops.some(stop => stop.source_id === source.id)) return 'Kies ten minste één register uit iedere bron.';
    if (!Number.isFinite(source.reference_a4_hz) || source.reference_a4_hz < 300 || source.reference_a4_hz > 550) return 'Controleer de referentietoonhoogte van iedere bron (300–550 Hz).';
    if (!Number.isFinite(source.tuning_cents) || source.tuning_cents < -1200 || source.tuning_cents > 1200) return 'Controleer de broncorrecties (−1200 tot +1200 cent).';
    if (!['original', 'normalize', 'auto'].includes(source.tuning_policy)) return 'Kies een stembeleid per bron.';
    if (catalogs) {
      const catalog = catalogs[source.id];
      if (!catalog) return 'Analyseer alle brondefinities voordat je opslaat of laadt.';
      if (source.fingerprint !== catalog.fingerprint) return 'Een brondefinitie is gewijzigd. Koppel de bron opnieuw en controleer de selectie.';
      if (project.stops.some(stop => stop.source_id === source.id && !catalog.stops.some(item => item.id === stop.source_stop_id))) return 'Een geselecteerd register ontbreekt in de bron.';
    }
  }
  const names = new Set(), channels = new Set();
  for (const division of project.divisions) {
    const name = division.name.trim().toLocaleLowerCase();
    if (!name || names.has(name)) return 'Geef ieder klavier een eigen, niet-lege naam.';
    names.add(name);
    if (!Number.isInteger(division.midi_channel) || division.midi_channel < 1 || division.midi_channel > 16) return 'MIDI-kanalen moeten tussen 1 en 16 liggen.';
    if (channels.has(division.midi_channel)) return 'Geef ieder klavier een eigen MIDI-kanaal.';
    channels.add(division.midi_channel);
    if (!Number.isInteger(division.first_midi_note) || division.first_midi_note < 0 || !Number.isInteger(division.key_count) || division.key_count < 1 || division.first_midi_note + division.key_count > 128) return 'Controleer het toetsbereik van de klavieren (MIDI 0–127).';
  }
  if (project.stops.some(stop => !sourceIds.has(stop.source_id) || !project.divisions.some(division => division.id === stop.division_id))) return 'Een register verwijst naar een ontbrekende bron of een ontbrekend klavier.';
  return null;
}

export const sameOrganProjectPath = (left, right) => {
  if (typeof left !== 'string' || typeof right !== 'string') return false;
  const a = left.replaceAll('\\', '/'), b = right.replaceAll('\\', '/');
  return /^[a-z]:\//i.test(a) && /^[a-z]:\//i.test(b) ? a.toLocaleLowerCase() === b.toLocaleLowerCase() : a === b;
};
