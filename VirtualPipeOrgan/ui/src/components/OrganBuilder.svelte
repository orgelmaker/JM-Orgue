<script>
  import { createEventDispatcher, onDestroy, onMount, tick } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import ProjectWindControls from './ProjectWindControls.svelte';
  import { newOrganProject, cloneOrganProject, chooseProjectStop, withoutProjectSource,
    addProjectDivision, organProjectError, sameOrganProjectPath, ORGAN_PROJECT_MAX_SOURCES,
    organProjectSourceLabel } from '../lib/organProject.js';

  export let organInfo = null;
  const dispatch = createEventDispatcher();
  let project = newOrganProject();
  let projectPath = '';
  let catalogs = {};
  let sourceErrors = {};
  let baselineProject = cloneOrganProject(project);
  let baselineCatalogs = {};
  let baselinePath = '';
  let baseline = JSON.stringify(project);
  let busy = false;
  $: locked = busy || !!pendingAction;
  let error = '';
  let message = '';
  let query = '';
  let activeSource = null;
  let pendingAction = null;
  let pendingRoot = '';
  let alive = true;
  let showSelected = false;
  $: sourceSlots = Array.from({ length: Math.min(ORGAN_PROJECT_MAX_SOURCES, project.sources.length + 1) }, (_, index) => index);
  $: dirty = JSON.stringify(project) !== baseline;
  $: dispatch('dirty', dirty);
  $: dispatch('saving', busy);
  $: validationError = organProjectError(project, catalogs);
  $: currentSource = project.sources.find(source => source.id === activeSource) || project.sources[0];
  $: currentCatalog = currentSource ? catalogs[currentSource.id] : null;
  $: visibleStops = (currentCatalog?.stops || []).filter(stop =>
    `${stop.name} ${stop.manual_name}`.toLocaleLowerCase().includes(query.toLocaleLowerCase()) &&
    (!showSelected || project.stops.some(selected => selected.source_id === currentSource?.id && selected.source_stop_id === stop.id)));
  $: loaded = !!projectPath && sameOrganProjectPath(organInfo?.id, projectPath);
  $: selectedSources = project.sources.filter(source => project.stops.some(stop => stop.source_id === source.id)).length;

  onDestroy(() => { alive = false; });
  onMount(() => {
    if (typeof organInfo?.id === 'string' && organInfo.id.toLocaleLowerCase().endsWith('.jmorgan')) openProject(organInfo.id);
  });
  const describeError = e => String(e?.message || e);
  const selectedStop = (sourceId, stopId) => project.stops.find(stop => stop.source_id === sourceId && stop.source_stop_id === stopId);
  const catalogStop = selected => catalogs[selected.source_id]?.stops.find(stop => stop.id === selected.source_stop_id);
  const sourceName = id => catalogs[id]?.name || 'Bronorgel';
  const shortPath = path => String(path || '').split(/[\\/]/).pop();
  const countSourceStops = id => project.stops.filter(stop => stop.source_id === id).length;
  const perspectives = catalog => [...new Set((catalog?.stops || []).flatMap(stop => stop.perspectives || []))];

  function changed() { project = { ...project }; error = ''; message = ''; }
  function editProject(patch) { project = { ...project, ...patch }; error = ''; message = ''; }
  function editSource(id, patch) { project = { ...project, sources: project.sources.map(source => source.id === id ? { ...source, ...patch } : source) }; error = ''; message = ''; }
  function editStop(id, patch) { project = { ...project, stops: project.stops.map(stop => stop.id === id ? { ...stop, ...patch } : stop) }; error = ''; message = ''; }
  function editDivision(id, patch) { project = { ...project, divisions: project.divisions.map(division => division.id === id ? { ...division, ...patch } : division) }; error = ''; message = ''; }
  function requestAction(action, text = 'Je hebt niet-opgeslagen wijzigingen. Wil je die verwerpen?') {
    if (busy) return;
    if (dirty) pendingAction = { action, text };
    else action();
  }
  function confirmAction() { const action = pendingAction?.action; pendingAction = null; action?.(); }
  export function resetDraft() {
    if (busy) return;
    project = cloneOrganProject(baselineProject); catalogs = cloneOrganProject(baselineCatalogs);
    projectPath = baselinePath; sourceErrors = {}; error = ''; message = ''; pendingAction = null;
    activeSource = project.sources[0]?.id || null; pendingRoot = '';
  }
  function makeNew() {
    project = newOrganProject(); projectPath = ''; catalogs = {}; sourceErrors = {};
    baselineProject = cloneOrganProject(project); baselineCatalogs = {}; baselinePath = '';
    baseline = JSON.stringify(project); activeSource = null; error = ''; message = ''; query = ''; pendingRoot = '';
  }
  async function inspect(path, packageRoot = null) {
    return invoke('inspect_organ_project_source', { path, packageRoot: packageRoot || null });
  }
  async function chooseSource(index) {
    if (busy) return;
    if (!project.sources[index] && project.sources.length >= ORGAN_PROJECT_MAX_SOURCES) {
      error = `Een project ondersteunt maximaal ${ORGAN_PROJECT_MAX_SOURCES} bronorgels.`; return;
    }
    error = ''; message = '';
    try {
      const { open } = await import('@tauri-apps/plugin-dialog');
      const path = await open({ title: `Kies bronorgel ${index + 1}`, multiple: false,
        filters: [{ name: 'Orgeldefinities', extensions: ['organ', 'Organ_Hauptwerk_xml', 'CustomOrgan_Hauptwerk_xml'] }] });
      if (!path || !alive) return;
      const existing = project.sources[index];
      if (project.sources.some((source, sourceIndex) => sourceIndex !== index && sameOrganProjectPath(source.path, path))) {
        error = 'Deze definitie is al als bron gekozen. Kies een andere brondefinitie.'; return;
      }
      busy = true;
      const packageRoot = existing?.package_root || (!existing && pendingRoot) || null;
      const catalog = await inspect(String(path), packageRoot);
      if (!alive) return;
      const same = existing && existing.fingerprint === catalog.fingerprint;
      const apply = () => {
        if (existing && !same) {
          project = withoutProjectSource(project, existing.id);
          delete catalogs[existing.id]; delete sourceErrors[existing.id];
        }
        const source = same ? { ...existing, path: catalog.path, package_root: packageRoot, resources: [] } : {
          id: globalThis.crypto.randomUUID(), path: catalog.path, fingerprint: catalog.fingerprint,
          package_root: packageRoot, reference_a4_hz: 440, tuning_policy: 'auto', tuning_cents: 0,
          perspective: null, resources: [],
        };
        const nextSources = [...project.sources];
        if (existing && same) nextSources[index] = source;
        else nextSources.splice(index, 0, source);
        project = { ...project, sources: nextSources };
        catalogs = { ...catalogs, [source.id]: catalog };
        sourceErrors = { ...sourceErrors, [source.id]: '' };
        if (!existing) pendingRoot = '';
        activeSource = source.id; error = ''; message = '';
      };
      if (same && existing.resources?.length) {
        pendingAction = { action: apply, text: 'Deze bron opnieuw koppelen en de huidige bronbestanden bij opslaan opnieuw vastleggen? De registerkoppelingen blijven behouden. Controleer eventuele wijzigingen aan de samples voordat je doorgaat.' };
      } else if (existing && !same && countSourceStops(existing.id)) {
        pendingAction = { action: apply, text: 'Dit is een andere bron. De geselecteerde registers uit de vorige bron worden uit dit project verwijderd. Doorgaan?' };
      } else apply();
    } catch (e) { if (alive) error = describeError(e); }
    finally { if (alive) busy = false; }
  }
  async function choosePackageRoot(index) {
    if (busy) return;
    try {
      const { open } = await import('@tauri-apps/plugin-dialog');
      const folder = await open({ title: 'Kies de map met bronpakketten', directory: true });
      if (!folder || !alive) return;
      const source = project.sources[index];
      if (!source) { pendingRoot = String(folder); return; }
      busy = true; error = '';
      const catalog = await inspect(source.path, String(folder));
      if (!alive) return;
      const apply = () => {
        catalogs = { ...catalogs, [source.id]: catalog };
        editSource(source.id, { package_root: String(folder), resources: [] });
        sourceErrors = { ...sourceErrors, [source.id]: '' };
      };
      if (source.resources?.length) pendingAction = { action: apply, text: 'Deze pakketmap gebruiken en de bijbehorende bronbestanden bij opslaan opnieuw vastleggen? Controleer of dit de juiste klankpakketten zijn.' };
      else apply();
    } catch (e) { if (alive) error = describeError(e); }
    finally { if (alive) busy = false; }
  }
  async function refreshSource(source) {
    if (busy) return;
    busy = true; error = '';
    try {
      const catalog = await inspect(source.path, source.package_root);
      if (!alive) return;
      catalogs = { ...catalogs, [source.id]: catalog };
      sourceErrors = { ...sourceErrors, [source.id]: source.fingerprint !== catalog.fingerprint ? 'De brondefinitie is gewijzigd. Koppel de bron opnieuw en controleer de selectie.' : '' };
    } catch (e) { if (alive) sourceErrors = { ...sourceErrors, [source.id]: describeError(e) }; }
    finally { if (alive) busy = false; }
  }
  async function openProject(existingPath = null) {
    if (busy) return;
    error = ''; message = '';
    try {
      let path = typeof existingPath === 'string' ? existingPath : null;
      if (!path) {
        const { open } = await import('@tauri-apps/plugin-dialog');
        path = await open({ title: 'Open orgelproject', multiple: false, filters: [{ name: 'JM-Orgue orgelproject', extensions: ['jmorgan'] }] });
      }
      if (!path || !alive) return;
      busy = true;
      const result = await invoke('read_organ_project', { path: String(path) });
      if (!Array.isArray(result?.sources) || result.sources.length < 1 || result.sources.length > ORGAN_PROJECT_MAX_SOURCES) {
        throw new Error(`Een project ondersteunt één tot ${ORGAN_PROJECT_MAX_SOURCES} bronorgels.`);
      }
      const nextCatalogs = {}, nextErrors = {};
      for (const source of result.sources) {
        try {
          const catalog = await inspect(source.path, source.package_root);
          nextCatalogs[source.id] = catalog;
          if (source.fingerprint !== catalog.fingerprint) nextErrors[source.id] = 'De brondefinitie is gewijzigd. Koppel de bron opnieuw en controleer de selectie.';
        } catch (e) { nextErrors[source.id] = describeError(e); }
      }
      if (!alive) return;
      project = result; projectPath = String(path); catalogs = nextCatalogs; sourceErrors = nextErrors;
      baselineProject = cloneOrganProject(project); baselineCatalogs = cloneOrganProject(catalogs); baselinePath = projectPath;
      baseline = JSON.stringify(project); activeSource = project.sources[0]?.id || null; query = ''; pendingRoot = '';
      message = 'Project geopend. Controleer de bronnen en laad het orgel wanneer je klaar bent.';
    } catch (e) { if (alive) error = describeError(e); }
    finally { if (alive) busy = false; }
  }
  async function saveProject(andLoad = false, saveAs = false) {
    if (busy) return;
    error = ''; message = '';
    const invalid = organProjectError(project, catalogs);
    if (invalid || Object.values(sourceErrors).some(Boolean)) { error = invalid || 'Los eerst de bronproblemen op.'; return; }
    try {
      let path = saveAs ? '' : projectPath;
      if (!path) {
        const { save } = await import('@tauri-apps/plugin-dialog');
        path = await save({ title: 'Bewaar orgelproject', defaultPath: `${project.name.trim().replace(/[<>:"/\\|?*]/g, '_') || 'Mijn orgel'}.jmorgan`,
          filters: [{ name: 'JM-Orgue orgelproject', extensions: ['jmorgan'] }] });
      }
      if (!path || !alive) return;
      busy = true;
      const saved = await invoke('save_organ_project', { path: String(path), project: cloneOrganProject(project) });
      if (!alive) return;
      project = saved; projectPath = String(path); baselineProject = cloneOrganProject(project);
      baselineCatalogs = cloneOrganProject(catalogs); baselinePath = projectPath; baseline = JSON.stringify(project);
      message = 'Project veilig opgeslagen. De oorspronkelijke samplesets zijn niet gewijzigd.';
      if (andLoad) {
        busy = false;
        await tick();
        if (alive) dispatch('load', { path: projectPath });
      }
    } catch (e) { if (alive) error = describeError(e); }
    finally { if (alive) busy = false; }
  }
  function toggle(sourceId, stop, checked) {
    try {
      const originalDivision = project.divisions.find(division => division.pedal === /pedal|pedaal/i.test(stop.manual_name));
      project = chooseProjectStop(project, sourceId, stop.id, checked, originalDivision?.id);
      error = ''; message = '';
    } catch (e) { error = describeError(e); }
  }
  // Meerdere registers tegelijk aan- of uitvinken: alle zichtbare, of een reeks met Shift.
  function setStops(stops, checked) {
    if (!currentSource) return;
    try {
      let next = project;
      for (const stop of stops) next = chooseProjectStop(next, currentSource.id, stop.id, checked,
        project.divisions.find(division => division.pedal === /pedal|pedaal/i.test(stop.manual_name))?.id);
      project = next; error = ''; message = '';
    } catch (e) { error = describeError(e); }
  }
  function selectVisible() { setStops(visibleStops, true); }
  function clearVisible() { setStops(visibleStops, false); }
  let lastStopId = null, shiftDown = false;
  function toggleRow(stop, checked) {
    const from = visibleStops.findIndex(item => item.id === lastStopId), to = visibleStops.findIndex(item => item.id === stop.id);
    if (shiftDown && from >= 0 && to >= 0 && from !== to) setStops(visibleStops.slice(Math.min(from, to), Math.max(from, to) + 1), checked);
    else toggle(currentSource.id, stop, checked);
    lastStopId = stop.id;
  }
  function removeSource(source) {
    pendingAction = { action: () => {
      project = withoutProjectSource(project, source.id);
      const { [source.id]: removedCatalog, ...remainingCatalogs } = catalogs;
      const { [source.id]: removedError, ...remainingErrors } = sourceErrors;
      catalogs = remainingCatalogs; sourceErrors = remainingErrors;
      if (activeSource === source.id) activeSource = project.sources[0]?.id || null;
      error = ''; message = '';
    }, text: `Bron “${sourceName(source.id)}” en zijn ${countSourceStops(source.id)} geselecteerde registers uit het project verwijderen?` };
  }
  function addDivision() { if (project.divisions.length < 8) { project = addProjectDivision(project); error = ''; message = ''; } }
  function removeDivision(id) {
    if (project.stops.some(stop => stop.division_id === id) || project.divisions.length <= 1) return;
    project = { ...project, divisions: project.divisions.filter(division => division.id !== id) }; error = ''; message = '';
  }
</script>

<section class="organ-builder" data-testid="organ-builder" aria-label="Orgelbouwer">
  <header class="project-heading">
    <div><span class="eyebrow">JE EIGEN INSTRUMENT</span><h1>Orgelbouwer</h1><p>Combineer registers uit je samplesets en geef ze samen een eigen speeltafel.</p></div>
    <div class="project-tally"><strong>{project.stops.length}</strong><span>registers uit {selectedSources} bronnen</span></div>
  </header>

  <div class="project-toolbar">
    <button type="button" class="btn btn-secondary btn-sm" disabled={locked} on:click={() => requestAction(makeNew)}>Nieuw</button>
    <button type="button" class="btn btn-secondary btn-sm" disabled={locked} on:click={() => requestAction(openProject)}>Open project…</button>
    <span class:dirty class="draft-state">{busy ? 'Bezig…' : dirty ? 'Niet opgeslagen' : projectPath ? 'Opgeslagen' : 'Nieuw project'}</span>
    <div class="toolbar-actions">
      <button type="button" class="btn btn-secondary btn-sm" disabled={locked || !dirty} on:click={resetDraft}>Wijzigingen herstellen</button>
      <button type="button" class="btn btn-secondary btn-sm" disabled={locked || !!validationError || Object.values(sourceErrors).some(Boolean)} on:click={() => saveProject(false, true)}>Opslaan als…</button>
      <button type="button" class="btn btn-secondary btn-sm" data-testid="organ-project-save" disabled={locked || !!validationError || Object.values(sourceErrors).some(Boolean)} on:click={() => saveProject()}>Opslaan</button>
      <button type="button" class="btn btn-primary btn-sm" data-testid="organ-project-load" disabled={locked || !!validationError || Object.values(sourceErrors).some(Boolean)} on:click={() => saveProject(true)}>Opslaan en laden</button>
    </div>
  </div>

  {#if pendingAction}<div class="notice warning" role="alert"><span>{pendingAction.text}</span><div><button type="button" class="btn btn-secondary btn-sm" on:click={() => pendingAction = null}>Annuleren</button><button type="button" class="btn btn-primary btn-sm" on:click={confirmAction}>Doorgaan</button></div></div>{/if}
  {#if error}<div class="notice error" role="alert">{error}</div>{/if}
  {#if message}<div class="notice success" role="status">{message}</div>{/if}

  <div class="project-settings">
    <label class="project-name">Naam van je orgel<input value={project.name} maxlength="120" disabled={locked} on:input={e => editProject({ name: e.target.value })} data-testid="organ-project-name"></label>
    <label>Gezamenlijke concerttoonhoogte A<input type="number" value={project.target_a4_hz} min="300" max="550" step="0.1" disabled={locked} on:input={e => editProject({ target_a4_hz: Number(e.target.value) })}><span class="field-hint">Hz · voor bronnen met ‘Gelijkmaken’</span></label>
    {#if loaded && !dirty}<button type="button" class="btn btn-primary btn-sm console-link" disabled={locked} on:click={() => dispatch('console')}>Console indelen →</button>{/if}
  </div>

  <div class="sources-heading"><div><h2>Samplesets</h2><span class="field-hint">{project.sources.length} van maximaal {ORGAN_PROJECT_MAX_SOURCES} bronnen · eigen stemming en pakketmap per bron</span></div><button type="button" class="btn btn-secondary btn-sm" disabled={locked || project.sources.length >= ORGAN_PROJECT_MAX_SOURCES} on:click={() => chooseSource(project.sources.length)}>Bron toevoegen…</button></div>
  <div class="sources">
    {#each sourceSlots as index}
      {@const source = project.sources[index]}
      {@const catalog = source ? catalogs[source.id] : null}
      <article class="source-card" class:empty={!source} data-testid={`organ-project-source-${index + 1}`}>
        <div class="source-top"><span class="source-letter">{organProjectSourceLabel(index)}</span><div><span class="eyebrow">BRON {index + 1}</span><h2>{catalog?.name || (source ? shortPath(source.path) : 'Voeg een sampleset toe')}</h2></div>{#if source}<span class="source-count">{countSourceStops(source.id)} gekozen</span>{/if}</div>
        {#if source}<p class="source-path" title={source.path}>{source.path}</p>{:else}<p class="empty-description">Open de orgeldefinitie. De samples en instellingen van het bronorgel blijven behouden.</p>{/if}
        <div class="source-buttons"><button type="button" class="btn btn-secondary btn-sm" disabled={locked} on:click={() => chooseSource(index)}>{source ? 'Andere definitie…' : 'Kies definitie…'}</button><button type="button" class="btn btn-ghost btn-sm" disabled={locked} on:click={() => choosePackageRoot(index)}>Pakketmap…</button>{#if source}<button type="button" class="btn btn-ghost btn-sm" disabled={locked} on:click={() => refreshSource(source)}>Controleer</button><button type="button" class="btn btn-ghost btn-sm remove" aria-label={`Bron ${index + 1} verwijderen`} disabled={locked} on:click={() => removeSource(source)}>×</button>{/if}</div>
        {#if !source && pendingRoot}<p class="field-hint source-path">Pakketmap: {pendingRoot}</p>{/if}
        {#if source}
          <div class="source-tuning">
            <label>Stemming<select aria-label="Stemming" value={source.tuning_policy} disabled={locked} on:change={e => editSource(source.id, { tuning_policy: e.target.value })}><option value="auto">Automatisch gelijkstemmen</option><option value="normalize">Concerttoonhoogte gelijkmaken</option><option value="original">Oorspronkelijk behouden</option></select></label>
            <label>Opgenomen A (Hz)<input type="number" min="300" max="550" step="0.1" value={source.reference_a4_hz} disabled={locked || source.tuning_policy !== 'normalize'} on:input={e => editSource(source.id, { reference_a4_hz: Number(e.target.value) })}></label>
            <label>Extra (cent)<input type="number" min="-1200" max="1200" step="0.1" value={source.tuning_cents} disabled={locked} on:input={e => editSource(source.id, { tuning_cents: Number(e.target.value) })}></label>
          </div>
          <p class="field-hint tuning-hint">{source.tuning_policy === 'auto' ? 'Elke pijp wordt op zijn gemeten toonhoogte bijgestemd naar de gezamenlijke concerttoonhoogte, zodat bronnen onderling zuiver zijn. Bewust zwevende stemmen houden hun zweving.' : source.tuning_policy === 'normalize' ? 'De algemene toonhoogte verschuift; het karakter van de stemming en celestezweving blijven behouden. Vul de echte bronreferentie in: zonder de juiste waarde blijven bronnen onderling verschillen.' : 'De oorspronkelijke toonhoogte en stemming blijven behouden. Verschillende bronnen kunnen daardoor onderling zweven.'}</p>
          {#if perspectives(catalog).length}<label class="perspective">Opnamepositie<select value={source.perspective || ''} disabled={locked} on:change={e => editSource(source.id, { perspective: e.target.value || null })}><option value="">Standaardpositie per register</option>{#each perspectives(catalog) as perspective}<option value={perspective}>{perspective}</option>{/each}</select></label>{/if}
          {#if sourceErrors[source.id]}<div class="source-error" role="alert">{sourceErrors[source.id]}</div>{/if}
          {#if catalog?.warnings?.length}<details class="source-warnings"><summary>{catalog.warnings.length} aandachtspunt{catalog.warnings.length === 1 ? '' : 'en'} bij deze bron</summary><ul>{#each catalog.warnings as warning}<li>{warning}</li>{/each}</ul></details>{/if}
        {/if}
      </article>
    {/each}
  </div>

  <div class="acoustics-note"><span aria-hidden="true">◌</span><p><strong>Luister ook naar de ruimte.</strong> Droge of directe opnamen combineren het best. Opgenomen galm blijft hoorbaar; deze versie gebruikt één gezamenlijke galmregeling. Gebruik ‘Pakketmap’ alleen als een bron losse klankpakketten nodig heeft.</p></div>

  {#if loaded}<ProjectWindControls organId={organInfo.id}
    sourceNames={Object.fromEntries(project.sources.map((source, index) => [source.id, `${organProjectSourceLabel(index)} · ${sourceName(source.id)}`]))}
    stopNames={Object.fromEntries((organInfo.divisions || []).flatMap(division => division.stops.map(stop => [stop.internal_stop_id, stop.name])))} />{/if}

  <div class="project-workspace">
    <section class="register-panel">
      <div class="panel-heading"><div><span class="eyebrow">1 · KIES DE KLANKEN</span><h2>Registers uit de bronnen</h2></div><span class="small-count">{currentCatalog?.stops.length || 0} beschikbaar</span></div>
      {#if project.sources.length}
        {#if project.sources.length <= 4}
          <div class="source-tabs" role="group" aria-label="Bron kiezen">{#each project.sources as source, index}<button type="button" class:active={currentSource?.id === source.id} disabled={locked} on:click={() => { activeSource = source.id; query = ''; }}><span>{organProjectSourceLabel(index)}</span>{sourceName(source.id)}</button>{/each}</div>
        {:else}
          <label class="source-picker">Bronorgel<select aria-label="Bron voor registerkeuze" value={currentSource?.id || ''} disabled={locked} on:change={e => { activeSource = e.target.value; query = ''; }}>{#each project.sources as source, index}<option value={source.id}>{organProjectSourceLabel(index)} · {sourceName(source.id)} · {countSourceStops(source.id)} gekozen</option>{/each}</select></label>
        {/if}
        <div class="register-tools"><label class="search"><span class="sr-only">Zoek register</span><input type="search" placeholder="Zoek register of bronklavier…" bind:value={query}></label><label class="check-label"><input type="checkbox" bind:checked={showSelected}> Alleen gekozen</label><button type="button" class="btn btn-ghost btn-sm" disabled={locked || !visibleStops.length} on:click={selectVisible}>Kies zichtbare</button><button type="button" class="btn btn-ghost btn-sm" disabled={locked || !visibleStops.length} on:click={clearVisible}>Wis zichtbare</button><span class="field-hint">Shift + klik: reeks aan of uit</span></div>
        <div class="register-scroll"><table><thead><tr><th class="check-column"><span class="sr-only">Kiezen</span></th><th>Bronregister</th><th>Doelklavier</th></tr></thead><tbody>
          {#each visibleStops as stop (stop.id)}
            {@const selected = selectedStop(currentSource.id, stop.id)}
            <tr class:selected={!!selected}><td><input type="checkbox" aria-label={`${stop.name} kiezen`} checked={!!selected} disabled={locked} on:click={e => { shiftDown = e.shiftKey; }} on:change={e => toggleRow(stop, e.target.checked)}></td><td><strong>{stop.name}</strong><small>{stop.manual_name} · MIDI {stop.first_midi_note}–{Math.min(127, stop.first_midi_note + stop.key_count - 1)}</small></td><td><select aria-label={`Doelklavier voor ${stop.name}`} value={selected?.division_id || ''} disabled={locked || !selected} on:change={e => editStop(selected.id, { division_id: Number(e.target.value) })}><option value="" disabled>Kies eerst</option>{#each project.divisions as division}<option value={division.id}>{division.name}</option>{/each}</select></td></tr>
          {:else}<tr><td colspan="3" class="empty-table">{currentCatalog ? 'Geen registers voor deze zoekopdracht.' : 'Deze bron kon nog niet worden geanalyseerd.'}</td></tr>{/each}
        </tbody></table></div>
      {:else}<div class="workspace-empty"><span aria-hidden="true">♫</span><h3>Begin met een sampleset</h3><p>Voeg daarna zoveel bronnen toe als je nodig hebt en kies hier hun registers, met opnamen en releases.</p></div>{/if}
    </section>

    <aside class="instrument-panel">
      <div class="panel-heading"><div><span class="eyebrow">2 · BOUW JE INSTRUMENT</span><h2>Klavieren en dispositie</h2></div></div>
      <div class="division-list">
        {#each project.divisions as division (division.id)}
          {@const assigned = project.stops.filter(stop => stop.division_id === division.id)}
          <article class="division-card">
            <div class="division-title"><label><span class="sr-only">Klaviernaam</span><input aria-label="Klaviernaam" value={division.name} maxlength="60" disabled={locked} on:input={e => editDivision(division.id, { name: e.target.value })}></label><button type="button" class="remove-division" aria-label={`${division.name} verwijderen`} title={assigned.length ? 'Verplaats eerst de registers van dit klavier' : 'Klavier verwijderen'} disabled={locked || !!assigned.length || project.divisions.length <= 1} on:click={() => removeDivision(division.id)}>×</button></div>
            <div class="division-fields"><label>MIDI<input type="number" min="1" max="16" value={division.midi_channel} disabled={locked} on:input={e => editDivision(division.id, { midi_channel: Number(e.target.value) })}></label><label>Eerste toets<input type="number" min="0" max="127" value={division.first_midi_note} disabled={locked} on:input={e => editDivision(division.id, { first_midi_note: Number(e.target.value) })}></label><label>Toetsen<input type="number" min="1" max="128" value={division.key_count} disabled={locked} on:input={e => editDivision(division.id, { key_count: Number(e.target.value) })}></label></div>
            <label class="check-label pedal-choice"><input type="checkbox" checked={!!division.pedal} disabled={locked} on:change={e => { project = { ...project, divisions: project.divisions.map(item => ({ ...item, pedal: item.id === division.id ? e.target.checked : e.target.checked ? false : item.pedal })) }; changed(); }}> Pedaalklavier</label>
            {#if assigned.length}<ul class="disposition">{#each assigned as selected (selected.id)}<li><span class="source-dot">{organProjectSourceLabel(project.sources.findIndex(source => source.id === selected.source_id))}</span><label><span class="sr-only">Naam van {catalogStop(selected)?.name || 'register'}</span><input aria-label={`Projectnaam voor ${catalogStop(selected)?.name || 'register'}`} placeholder={catalogStop(selected)?.name || `Register ${selected.source_stop_id}`} value={selected.name || ''} maxlength="120" disabled={locked} on:input={e => editStop(selected.id, { name: e.target.value.trim() ? e.target.value : null })}></label><button type="button" aria-label={`${catalogStop(selected)?.name || 'Register'} uit selectie verwijderen`} disabled={locked} on:click={() => { project = chooseProjectStop(project, selected.source_id, selected.source_stop_id, false); error = ''; message = ''; }}>×</button></li>{/each}</ul>{:else}<p class="empty-division">Nog geen registers toegewezen</p>{/if}
          </article>
        {/each}
      </div>
      <button type="button" class="btn btn-secondary btn-sm add-division" disabled={locked || project.divisions.length >= 8} on:click={addDivision}>+ Klavier toevoegen</button>
      <p class="field-hint">Brontoetsbereiken blijven behouden. Een groter doelklavier maakt geen ontbrekende pijpen aan. Na laden kun je de registerknoppen in de Consolebouwer plaatsen.</p>
    </aside>
  </div>
  <footer class="project-footer"><span>{validationError || (loaded && !dirty ? 'Dit project is geladen. Je kunt de console indelen en spelen.' : 'Het project is klaar om op te slaan en te laden.')}</span>{#if projectPath}<small title={projectPath}>{projectPath}</small>{/if}</footer>
</section>

<style>
  .organ-builder{padding:24px 24px 32px;background:var(--bg-dark);color:var(--text);box-sizing:border-box;min-height:100%;overflow:auto}
  .project-heading{display:flex;justify-content:space-between;gap:20px;align-items:center;margin-bottom:20px}
  .eyebrow{color:var(--text-muted);font-size:10px;font-weight:600;letter-spacing:.15em}
  h1{font:500 29px/1.2 Georgia,serif;margin:4px 0 7px}h2{font:500 18px/1.25 Georgia,serif;margin:4px 0 0}h3{font:500 17px/1.3 Georgia,serif;margin:8px 0}
  .project-heading p{font-size:13px;color:var(--text-secondary);margin:0;line-height:1.5}
  .project-tally{border:1px solid var(--stop-porcelain-border);background:var(--bg-panel);border-radius:8px;display:flex;align-items:center;gap:12px;padding:10px 16px;min-width:125px}
  .project-tally strong{font:400 32px/1 Georgia,serif;color:var(--primary)}.project-tally span{font-size:11px;color:var(--text-secondary);max-width:85px;line-height:1.5}
  .project-toolbar{position:sticky;top:0;z-index:7;background:var(--bg-dark);border-bottom:1px solid var(--stop-porcelain-border);display:flex;gap:8px;align-items:center;flex-wrap:wrap;padding:10px 0 14px}
  .toolbar-actions{display:flex;gap:8px;flex-wrap:wrap;margin-left:auto}.draft-state{color:var(--text-muted);font-size:11px;margin-left:6px}.draft-state.dirty:before{content:'●';color:var(--primary);padding-right:6px}
  .project-settings{display:flex;align-items:start;gap:20px;margin:20px 0;flex-wrap:wrap}.project-name{flex:1;min-width:220px}.project-settings>label:not(.project-name){width:230px}.console-link{align-self:center}
  label{display:flex;flex-direction:column;gap:5px;color:var(--text-secondary);font-size:11px;font-weight:500;min-width:0}
  input:not([type=checkbox]),select{box-sizing:border-box;width:100%;min-width:0;border:1px solid var(--stop-porcelain-border);background:var(--bg-elevated);color:var(--text);border-radius:5px;padding:8px;font:inherit;font-size:12px;font-weight:400;line-height:1.3;min-height:34px}
  input:focus,select:focus{outline:2px solid var(--primary);outline-offset:1px}input[type=checkbox]{accent-color:var(--primary);width:16px;height:16px;margin:0;flex-shrink:0}.project-name input{font:500 19px/1.3 Georgia,serif}
  .sources-heading{display:flex;align-items:center;justify-content:space-between;gap:14px;flex-wrap:wrap;margin:20px 0 12px}.sources-heading h2{margin-top:0}.sources-heading button{flex-shrink:0}.source-picker{margin-bottom:12px}.field-hint{font-size:10px;color:var(--text-muted);line-height:1.6;font-weight:400}.sources{display:grid;grid-template-columns:1fr 1fr;gap:18px}
  .source-card{position:relative;min-width:0;background:var(--bg-panel);border:1px solid var(--stop-porcelain-border);border-radius:10px;padding:17px 18px;border-top:3px solid var(--primary);box-shadow:var(--shadow-sm)}
  .source-card.empty{border-top-color:var(--stop-porcelain-border);background:var(--bg-dark)}.source-top{display:flex;align-items:center;gap:11px}.source-top>div{min-width:0;flex:1}.source-top h2{overflow-wrap:anywhere;font-size:18px}
  .source-letter{width:34px;height:34px;border-radius:50%;display:flex;align-items:center;justify-content:center;background:var(--accent-soft);color:var(--primary);font:500 19px/1 Georgia,serif;flex-shrink:0}
  .source-count{font-size:10px;color:var(--text-muted);white-space:nowrap}.source-path{font-size:10px;color:var(--text-muted);overflow-wrap:anywhere;line-height:1.5;margin:12px 0}.empty-description{font-size:12px;color:var(--text-secondary);line-height:1.7;max-width:400px;min-height:34px;margin:15px 0}
  .source-buttons{display:flex;gap:6px;align-items:center;flex-wrap:wrap;margin-bottom:14px}.source-buttons .remove{margin-left:auto;font-size:20px;color:var(--text-muted);padding:0 7px;min-width:26px}
  .source-tuning{display:grid;grid-template-columns:minmax(120px,2fr) minmax(75px,1fr) minmax(65px,.8fr);gap:10px;border-top:1px solid var(--stop-porcelain-border);padding-top:14px}.tuning-hint{margin:8px 0 0}.perspective{max-width:300px;margin-top:12px}
  .source-error{font-size:11px;color:var(--error);border-left:3px solid var(--error);padding-left:8px;line-height:1.5;margin-top:12px}.source-warnings{font-size:11px;line-height:1.6;color:var(--text-secondary);margin-top:12px}.source-warnings summary{cursor:pointer}.source-warnings ul{padding-left:17px}
  .acoustics-note{display:flex;gap:12px;align-items:center;padding:13px 17px;margin:18px 0;background:var(--accent-soft-hover);border:1px solid var(--stop-porcelain-border);border-radius:8px;color:var(--text-secondary)}.acoustics-note>span{font:400 28px/1 Georgia,serif;color:var(--primary)}.acoustics-note p{margin:0;font-size:11px;line-height:1.7}.acoustics-note strong{font-weight:600}
  .project-workspace{display:grid;grid-template-columns:minmax(0,1.3fr) minmax(330px,1fr);gap:18px;align-items:start}.register-panel,.instrument-panel{min-width:0;border:1px solid var(--stop-porcelain-border);background:var(--bg-panel);border-radius:10px;padding:16px}
  .panel-heading{display:flex;gap:12px;align-items:center;justify-content:space-between;margin-bottom:15px}.small-count{font-size:10px;color:var(--text-muted);white-space:nowrap}.source-tabs{display:flex;flex-wrap:wrap;gap:5px;padding:3px;background:var(--bg-dark);border:1px solid var(--stop-porcelain-border);border-radius:7px;margin-bottom:12px}
  .source-tabs button{display:flex;gap:7px;align-items:center;flex:1 1 40%;min-width:0;background:transparent;border:0;border-radius:4px;padding:8px;color:var(--text-secondary);font:inherit;font-size:11px;font-weight:500;line-height:1.4;text-align:left;cursor:pointer;overflow-wrap:anywhere}.source-tabs button.active{background:var(--bg-elevated);box-shadow:var(--shadow-sm);color:var(--text)}.source-tabs button span{font-family:Georgia,serif;color:var(--primary);border:1px solid var(--stop-porcelain-border);border-radius:4px;padding:0 5px}
  .register-tools{display:flex;gap:9px;align-items:center;flex-wrap:wrap;margin-bottom:10px}.search{flex:1;min-width:170px}.check-label{display:flex;flex-direction:row;align-items:center;gap:7px;font-size:10px;font-weight:400}.register-scroll{overflow:auto;max-height:550px;margin:0 -5px}
  table{border-collapse:collapse;width:100%;font-size:11px;text-align:left}th{position:sticky;top:0;z-index:1;background:var(--bg-panel);color:var(--text-muted);font-size:10px;font-weight:500;padding:10px 7px;border-bottom:1px solid var(--stop-porcelain-border)}td{border-bottom:1px solid var(--stop-porcelain-border);padding:10px 7px;vertical-align:middle}.check-column{width:22px}td:nth-child(2){min-width:110px}td:nth-child(3){min-width:100px;width:35%}td strong{font-size:12px;font-weight:500;display:block;overflow-wrap:anywhere}td small{font-size:10px;display:block;margin-top:5px;color:var(--text-muted)}tr.selected td{background:var(--accent-soft-hover)}td select{font-size:11px;padding:6px}.empty-table{text-align:center;color:var(--text-muted);padding:25px 12px}
  .workspace-empty{text-align:center;color:var(--text-secondary);padding:50px 24px}.workspace-empty>span{font:400 38px/1 Georgia,serif;color:var(--primary)}.workspace-empty p{font-size:12px;line-height:1.7;max-width:330px;margin:0 auto}
  .division-list{display:flex;flex-direction:column;gap:12px}.division-card{padding:12px;border:1px solid var(--stop-porcelain-border);background:var(--bg-dark);border-radius:7px}.division-title{display:flex;gap:8px;align-items:center;margin-bottom:10px}.division-title label{flex:1}.division-title input{font:500 15px/1.3 Georgia,serif;background:transparent;border-color:transparent;padding:3px 0;min-height:28px}.division-title input:focus{background:var(--bg-elevated);padding:3px 5px}.remove-division{border:0;background:transparent;color:var(--text-muted);font:inherit;font-size:20px;font-weight:400;line-height:1;cursor:pointer;padding:4px}.remove-division:disabled{opacity:.35;cursor:default}.division-fields{display:grid;grid-template-columns:1fr 1fr 1fr;gap:9px}.division-fields label{font-size:10px}.division-fields input{font-size:11px;min-height:30px;padding:6px}.pedal-choice{margin-top:10px}.disposition{list-style:none;padding:0;margin:12px 0 0}.disposition li{display:flex;align-items:center;gap:6px;border-top:1px solid var(--stop-porcelain-border);padding:6px 0}.source-dot{color:var(--primary);font:500 11px/1 Georgia,serif;border:1px solid var(--stop-porcelain-border);border-radius:3px;padding:3px 4px}.disposition label{flex:1}.disposition input{font-size:11px;background:transparent;border-color:transparent;padding:4px 3px;min-height:26px}.disposition input:focus{background:var(--bg-elevated);border-color:var(--stop-porcelain-border)}.disposition input::placeholder{color:var(--text-secondary);opacity:1}.disposition button{border:0;background:transparent;color:var(--text-muted);cursor:pointer;font-size:17px}.empty-division{font-size:10px;color:var(--text-muted);margin:12px 0 0;font-style:italic}.add-division{width:100%;margin:12px 0 0}.instrument-panel>.field-hint{margin:12px 0 0}
  .notice{display:flex;justify-content:space-between;gap:12px;flex-wrap:wrap;padding:12px 14px;margin:12px 0;font-size:12px;line-height:1.6;border:1px solid var(--stop-porcelain-border);border-left:4px solid var(--warning);border-radius:6px;background:var(--bg-panel)}.notice>div{display:flex;gap:8px}.notice.error{border-left-color:var(--error)}.notice.success{border-left-color:var(--success)}.project-footer{display:flex;justify-content:space-between;gap:12px;flex-wrap:wrap;padding:16px 0;font-size:11px;color:var(--text-secondary)}.project-footer small{color:var(--text-muted);font-size:10px;overflow-wrap:anywhere}.sr-only{position:absolute;width:1px;height:1px;padding:0;margin:-1px;overflow:hidden;clip:rect(0,0,0,0);white-space:nowrap;border:0}
  @media(max-width:1100px){.organ-builder{padding:16px}.toolbar-actions{margin-left:0}.project-workspace{grid-template-columns:minmax(0,1.15fr) minmax(300px,1fr)}.source-tuning{grid-template-columns:1fr 1fr}.source-tuning>label:first-child{grid-column:1/-1}.source-count{display:none}}
  @media(max-width:850px){.sources,.project-workspace{grid-template-columns:1fr}.project-tally{display:none}.source-tuning{grid-template-columns:2fr 1fr 1fr}.source-tuning>label:first-child{grid-column:auto}.project-toolbar{position:static}.project-settings>label:not(.project-name){width:210px}.register-scroll{max-height:420px}}
  @media(pointer:coarse){input:not([type=checkbox]),select{min-height:44px;font-size:16px}.source-tabs button{min-height:44px}.disposition button,.remove-division{min-width:40px;min-height:40px}.disposition input,.division-fields input{min-height:40px;font-size:14px}}
</style>
