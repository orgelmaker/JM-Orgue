<script>
  import { createEventDispatcher, onDestroy } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { t, tx } from '../lib/i18n.js';
  import ConsoleSurface from './ConsoleSurface.svelte';
  import { alignConsoleArray, consoleArray } from '../lib/consoleSnap.js';
  import { boundedConsoleElement, cloneConsoleLayout, consoleTargets, consoleLayoutError,
    defaultConsoleLayout, missingConsoleTargets, nextConsoleId, normalizeConsoleLayout,
    CONSOLE_APPEARANCES, CONSOLE_THEMES, CONSOLE_TEXTURES, CONSOLE_TEXTURE_MAX_BYTES, CONSOLE_MAX_PAGES, consoleCustomTextureError, roundConsoleAppearance } from '../lib/consoleLayout.js';

  export let organInfo = null;
  export let initialLayout = null;
  export let organLoadEpoch = 0;
  const dispatch = createEventDispatcher();
  const organId = organInfo?.id;
  const epoch = organLoadEpoch;
  const makeDefault = () => defaultConsoleLayout(organInfo, { pageName: tx('console_builder.page'), couplersName: tx('console_builder.couplers') });
  let persisted = normalizeConsoleLayout(initialLayout);
  let invalid = initialLayout != null && persisted == null;
  let draft = cloneConsoleLayout(persisted) || makeDefault();
  let baseline = JSON.stringify(draft);
  let savedOnce = persisted != null;
  let forceDirty = false;
  let pageId = draft.pages[0].id;
  let selectedId = null;
  let history = [];
  let preview = false;
  let grid = 8;
  let search = '';
  let filter = 'all';
  let saving = false;
  let readingTexture = false;
  let textureRead = 0;
  let textureInput;
  let snapGuides = true;
  let wideCanvas = false;
  let error = '';
  let warning = '';
  let message = '';
  let alive = true;
  $: dirty = forceDirty || JSON.stringify(draft) !== baseline;
  $: dispatch('dirty', dirty);
  $: page = draft.pages.find(p => p.id === pageId) || draft.pages[0];
  $: selected = page.elements.find(e => e.id === selectedId) || null;
  $: targets = consoleTargets(organInfo);
  $: placed = new Set(draft.pages.flatMap(p => p.elements.filter(e => e.kind !== 'label').map(e => `${e.kind}:${e.target_id}`)));
  $: unplaced = targets.filter(target => !placed.has(`${target.kind}:${target.id}`));
  $: visibleTargets = targets.filter(target => (filter === 'all' || target.kind === filter) &&
    `${target.name} ${target.pitch} ${target.division}`.toLocaleLowerCase().includes(search.toLocaleLowerCase()));
  $: missing = missingConsoleTargets(draft, organInfo);
  $: validationError = consoleLayoutError(draft);
  $: selectedTarget = selected ? targets.find(target => target.kind === selected.kind && target.id === selected.target_id) : null;

  function mutate(change) {
    if (saving || readingTexture || invalid) return;
    history = [...history.slice(-39), cloneConsoleLayout(draft)];
    const next = cloneConsoleLayout(draft);
    change(next);
    draft = next;
    error = ''; warning = ''; message = '';
  }
  function undo() {
    if (!history.length || saving) return;
    draft = history[history.length - 1]; history = history.slice(0, -1);
    if (!draft.pages.some(p => p.id === pageId)) pageId = draft.pages[0].id;
    selectedId = null; error = ''; message = '';
  }
  export function resetDraft() {
    if (saving) return;
    if (readingTexture) { textureRead++; readingTexture = false; dispatch('saving', false); }
    draft = cloneConsoleLayout(persisted) || makeDefault();
    baseline = JSON.stringify(draft); forceDirty = false;
    history = []; selectedId = null; pageId = draft.pages[0].id;
    error = ''; warning = ''; message = ''; preview = false;
    invalid = initialLayout != null && persisted == null;
  }
  function arrangeAgain() {
    const next = makeDefault();
    next.theme = draft.theme; next.enabled = draft.enabled; next.name = draft.name;
    if (invalid) { invalid = false; forceDirty = true; draft = next; }
    else mutate(layout => { layout.pages = next.pages; });
    pageId = draft.pages[0].id; selectedId = null; preview = false;
  }
  async function apply() {
    if (saving || readingTexture || invalid || (draft.enabled && missing.length)) return;
    if (validationError) { error = tx('console_builder.invalid_draft'); return; }
    const snapshot = cloneConsoleLayout(draft);
    saving = true; error = ''; warning = ''; message = '';
    dispatch('saving', true);
    try {
      const result = await invoke('set_console_layout', { organId, layout: snapshot });
      if (!alive || organInfo?.id !== organId || organLoadEpoch !== epoch) return;
      persisted = snapshot; draft = cloneConsoleLayout(snapshot); baseline = JSON.stringify(snapshot);
      savedOnce = true; forceDirty = false; history = [];
      warning = result?.warning || ''; message = tx('console_builder.saved');
      dispatch('saved', { layout: cloneConsoleLayout(snapshot), organId });
    } catch (e) {
      if (alive) error = `${tx('console_builder.save_error')} ${String(e?.message || e)}`;
    } finally {
      if (alive) { saving = false; dispatch('saving', false); }
    }
  }
  function slot(width, height) {
    for (let y = 40; y + height <= page.height - 20; y += 112)
      for (let x = 32; x + width <= page.width - 20; x += 140)
        if (!page.elements.some(e => x < e.x + e.width + 8 && x + width + 8 > e.x && y < e.y + e.height + 8 && y + height + 8 > e.y)) return { x, y };
    return { x: 32, y: 40 };
  }
  function addTarget(target = null) {
    if (page.elements.length >= 512) return;
    const width = target ? 120 : 240, height = target ? 92 : 36;
    const position = slot(width, height), id = nextConsoleId(draft);
    mutate(layout => layout.pages.find(p => p.id === page.id).elements.push({
      id, kind: target?.kind || 'label', target_id: target?.id || null,
      label: target ? '' : tx('console_builder.new_label'), ...position, width, height,
      appearance: selected?.kind !== 'label' && selected?.appearance || 'rocker',
    }));
    selectedId = id;
  }
  function editSelected(patch) {
    if (!selected) return;
    const selectedIdNow = selected.id, pageIdNow = page.id;
    mutate(layout => {
      const p = layout.pages.find(p => p.id === pageIdNow);
      p.elements = p.elements.map(e => e.id === selectedIdNow ? boundedConsoleElement(e, p, patch) : e);
    });
  }
  function moveElement(detail) {
    selectedId = detail.elementId;
    const id = detail.elementId, currentPage = page.id;
    mutate(layout => {
      const p = layout.pages.find(p => p.id === currentPage);
      p.elements = p.elements.map(e => e.id === id ? boundedConsoleElement(e, p, { x: detail.x, y: detail.y }) : e);
    });
  }
  function appearancePatch(element, appearance) {
    return roundConsoleAppearance(appearance) ? { appearance, width: Math.max(48, Math.min(element.width, element.height)), height: Math.max(48, Math.min(element.width, element.height)) } : { appearance };
  }
  function resizeElement(detail) {
    selectedId = detail.elementId;
    const id = detail.elementId, currentPage = page.id;
    mutate(layout => {
      const p = layout.pages.find(p => p.id === currentPage);
      p.elements = p.elements.map(e => e.id === id ? boundedConsoleElement(e, p, { width: detail.width, height: detail.height }) : e);
    });
  }
  async function chooseTexture(event) {
    const file = event.currentTarget.files?.[0];
    event.currentTarget.value = '';
    if (!file || saving || readingTexture || invalid) return;
    const request = ++textureRead;
    if (file.size > CONSOLE_TEXTURE_MAX_BYTES || !['image/png', 'image/jpeg', 'image/webp'].includes(file.type)) {
      error = tx('console_builder.custom_texture_error'); return;
    }
    readingTexture = true; dispatch('saving', true); error = ''; message = '';
    try {
      const data = await new Promise((resolve, reject) => {
        const reader = new FileReader(); reader.onload = () => resolve(reader.result); reader.onerror = reject; reader.readAsDataURL(file);
      });
      const image = { name: [...file.name].slice(0, 120).join(''), data_url: data };
      if (consoleCustomTextureError(image)) throw new Error(tx('console_builder.custom_texture_error'));
      const dimensions = await new Promise((resolve, reject) => {
        const bitmap = new Image(); bitmap.onload = () => resolve([bitmap.naturalWidth, bitmap.naturalHeight]); bitmap.onerror = () => reject(new Error(tx('console_builder.custom_texture_error'))); bitmap.src = data;
      });
      if (dimensions.some(d => d > 8192)) throw new Error(tx('console_builder.custom_texture_dimension_error'));
      if (!alive || request !== textureRead || organInfo?.id !== organId || organLoadEpoch !== epoch) return;
      readingTexture = false;
      mutate(layout => { layout.custom_texture = image; layout.texture = 'custom'; });
    } catch (e) {
      if (alive && request === textureRead) error = e?.message || tx('console_builder.custom_texture_error');
    } finally {
      if (alive && request === textureRead) { readingTexture = false; dispatch('saving', saving); }
    }
  }
  function removeTexture() {
    mutate(layout => { delete layout.custom_texture; if (layout.texture === 'custom') layout.texture = 'grain'; });
  }
  function changePageSize(key, value) {
    const n = Number(value), min = key === 'width' ? 640 : 400, max = key === 'width' ? 2400 : 1600;
    const bounded = Math.max(min, Math.min(max, Number.isFinite(n) ? n : page[key]));
    mutate(layout => { const p = layout.pages.find(p => p.id === page.id); p[key] = bounded; p.elements = p.elements.map(e => boundedConsoleElement(e, p)); });
  }
  // Formaat van de geselecteerde bediening voor alle bedieningen (alle schermen).
  // Ronde vormen blijven rond: zij krijgen de kleinste van breedte en hoogte.
  function applySizeToAll() {
    if (!selected || selected.kind === 'label') return;
    const { width, height } = selected, round = Math.min(width, height);
    mutate(layout => layout.pages.forEach(p => {
      p.elements = p.elements.map(e => e.kind === 'label' ? e :
        boundedConsoleElement(e, p, roundConsoleAppearance(e.appearance) ? { width: round, height: round } : { width, height }));
    }));
  }
  // Rij of kolom rond de selectie: op één lijn en met gelijke tussenruimte.
  $: rowSize = selected ? consoleArray(selected, page.elements, 'x').length : 0;
  $: columnSize = selected ? consoleArray(selected, page.elements, 'y').length : 0;
  function alignArray(axis) {
    if (!selected) return;
    const patches = alignConsoleArray(selected, page.elements, page, axis);
    if (!patches) return;
    const currentPage = page.id;
    mutate(layout => {
      const p = layout.pages.find(p => p.id === currentPage);
      p.elements = p.elements.map(e => patches[e.id] ? boundedConsoleElement(e, p, patches[e.id]) : e);
    });
  }
  function removeSelected() {
    if (!selected) return;
    const id = selected.id;
    mutate(layout => { const p = layout.pages.find(p => p.id === page.id); p.elements = p.elements.filter(e => e.id !== id); });
    selectedId = null;
  }
  function duplicate() {
    if (!selected || page.elements.length >= 512) return;
    const copied = boundedConsoleElement({ ...selected, id: nextConsoleId(draft) }, page, { x: selected.x + 24, y: selected.y + 24 });
    mutate(layout => layout.pages.find(p => p.id === page.id).elements.push(copied));
    selectedId = copied.id;
  }
  function addPage() {
    if (draft.pages.length >= CONSOLE_MAX_PAGES) return;
    const id = nextConsoleId(draft, 'page');
    mutate(layout => layout.pages.push({ id, name: `${tx('console_builder.page')} ${layout.pages.length + 1}`, width: page.width, height: page.height, elements: [] }));
    pageId = id; selectedId = null;
  }
  function removePage() {
    if (draft.pages.length <= 1 || page.elements.length) return;
    const id = page.id;
    mutate(layout => { layout.pages = layout.pages.filter(p => p.id !== id); });
    pageId = draft.pages[0].id; selectedId = null;
  }
  function removeMissing() {
    const ids = new Set(missing.map(m => m.element.id));
    mutate(layout => layout.pages.forEach(p => { p.elements = p.elements.filter(e => !ids.has(e.id)); }));
    selectedId = null;
  }
  onDestroy(() => { alive = false; textureRead++; dispatch('dirty', false); dispatch('saving', false); });
</script>

<section class="console-builder" aria-label={$t('console_builder.title')}>
  <div class="builder-heading">
    <div><span class="builder-eyebrow">JM-ORGUE · {$t('console_builder.organ')}</span><h1>{$t('console_builder.title')}</h1><p>{$t('console_builder.subtitle')}</p></div>
    <div class="organ-plaque">{organInfo?.name}</div>
  </div>

  {#if invalid}
    <div class="builder-notice error" role="alert"><span>{$t('console_builder.invalid')}</span><button class="btn btn-secondary" type="button" on:click={arrangeAgain}>{$t('console_builder.invalid_action')}</button></div>
  {:else}
    <div class="builder-commandbar">
      <div class="builder-mode" role="group" aria-label={$t('console_builder.title')}>
        <button type="button" class:active={!preview} on:click={() => { preview = false; }} aria-pressed={!preview}>{$t('console_builder.design')}</button>
        <button type="button" class:active={preview} on:click={() => { preview = true; }} aria-pressed={preview}>{$t('console_builder.preview')}</button>
      </div>
      <span class="draft-state" class:dirty>{dirty ? $t('console_builder.unsaved') : savedOnce ? $t('console_builder.unchanged') : $t('console_builder.new_design')}</span>
      <div class="builder-actions">
        <button type="button" class="btn btn-secondary btn-sm" disabled={!history.length || saving} on:click={undo}>↶ {$t('console_builder.undo')}</button>
        <button type="button" class="btn btn-secondary btn-sm" disabled={(!dirty && !error) || saving} on:click={resetDraft}>{$t('console_builder.cancel')}</button>
        <button type="button" class="btn btn-primary btn-sm" disabled={saving || readingTexture || (draft.enabled && missing.length > 0) || !!validationError || (!dirty && savedOnce)} on:click={apply}>{saving ? $t('console_builder.saving') : $t('console_builder.apply')}</button>
      </div>
    </div>
    {#if error}<div class="builder-notice error" role="alert">{error}</div>{/if}
    {#if validationError}<div class="builder-notice error" role="alert">{$t('console_builder.invalid_draft')}</div>{/if}
    {#if warning}<div class="builder-notice warning" role="status">{warning}</div>{/if}
    {#if message}<div class="builder-notice success" role="status">{message}</div>{/if}
    {#if missing.length}<div class="builder-notice warning" role="status"><span>{$t('console_builder.missing').replace('{n}', missing.length)}</span><button type="button" class="btn btn-secondary btn-sm" disabled={saving} on:click={removeMissing}>{$t('console_builder.remove_missing')}</button></div>{/if}

    <div class="builder-settings">
      <label class="console-enabled"><input type="checkbox" checked={draft.enabled} disabled={saving} on:change={e => mutate(layout => { layout.enabled = e.target.checked; })}><span>{$t('console_builder.enabled')}</span></label>
      <label>{$t('console_builder.name')}<input value={draft.name} maxlength="120" disabled={saving || preview} on:change={e => mutate(layout => { layout.name = e.target.value; })}></label>
      <label>{$t('console_builder.theme')}<select value={draft.theme} disabled={saving} on:change={e => mutate(layout => { layout.theme = e.target.value; })}>{#each CONSOLE_THEMES as theme}<option value={theme}>{$t(`console_builder.${theme}`)}</option>{/each}</select></label>
      <button type="button" class="btn btn-secondary btn-sm" disabled={saving || preview} title={$t('console_builder.reset_hint')} on:click={arrangeAgain}>{$t('console_builder.reset')}</button>
    </div>

    <div class="builder-materials">
      <label>{$t('console_builder.texture')}<select value={draft.texture || 'grain'} disabled={saving || readingTexture} on:change={e => mutate(layout => { layout.texture = e.target.value; })}>
        {#each CONSOLE_TEXTURES as texture}<option value={texture} disabled={texture === 'custom' && !draft.custom_texture}>{$t(`console_builder.texture_${texture}`)}</option>{/each}
      </select></label>
      <label>{$t('console_builder.texture_opacity')} <output>{Math.round((draft.texture_opacity ?? 0.65) * 100)}%</output><input type="range" min="0" max="1" step="0.05" value={draft.texture_opacity ?? 0.65} disabled={saving || readingTexture} on:change={e => mutate(layout => { layout.texture_opacity = Number(e.target.value); })}></label>
      <label>{$t('console_builder.texture_scale')} <output>{draft.texture_scale ?? 100}%</output><input type="range" min="25" max="400" step="25" value={draft.texture_scale ?? 100} disabled={saving || readingTexture} on:change={e => mutate(layout => { layout.texture_scale = Number(e.target.value); })}></label>
      <div class="custom-material">
        <input class="texture-file-input" type="file" accept="image/png,image/jpeg,image/webp" bind:this={textureInput} on:change={chooseTexture} disabled={saving || readingTexture} aria-label={$t('console_builder.custom_texture_choose')}>
        <button type="button" class="btn btn-secondary btn-sm" disabled={saving || readingTexture} on:click={() => textureInput.click()}>{readingTexture ? $t('console_builder.texture_loading') : $t('console_builder.custom_texture_choose')}</button>
        {#if draft.custom_texture}<button type="button" class="btn btn-ghost btn-sm" disabled={saving || readingTexture} on:click={removeTexture}>{$t('console_builder.custom_texture_remove')}</button>{/if}
        <small>{draft.custom_texture?.name || $t('console_builder.custom_texture_hint')}</small>
      </div>
    </div>

    <div class="builder-workspace" class:preview class:wide={wideCanvas}>
      {#if !preview && !wideCanvas}
        <aside class="builder-palette">
          <h2>{$t('console_builder.palette')}</h2>
          <input type="search" bind:value={search} placeholder={$t('console_builder.search')} aria-label={$t('console_builder.search')}>
          <select bind:value={filter} aria-label={$t('console_builder.palette')}><option value="all">{$t('console_builder.all')}</option><option value="stop">{$t('console_builder.stops')}</option><option value="coupler">{$t('console_builder.couplers')}</option></select>
          <div class="palette-list">
            {#each visibleTargets as target (`${target.kind}:${target.id}`)}
              {@const isPlaced = placed.has(`${target.kind}:${target.id}`)}
              <button type="button" class="palette-control" class:placed={isPlaced} disabled={saving || isPlaced || page.elements.length >= 512} on:click={() => addTarget(target)} title={isPlaced ? $t('console_builder.placed') : $t('console_builder.add')}>
                <span class="palette-symbol" aria-hidden="true">{target.kind === 'coupler' ? '↔' : '＋'}</span><span><strong>{target.name} {target.pitch}</strong><small>{target.division}</small></span><span class="placed-check" aria-hidden="true">{isPlaced ? '✓' : ''}</span>
              </button>
            {/each}
          </div>
          <button type="button" class="btn btn-secondary btn-sm" disabled={saving || page.elements.length >= 512} on:click={() => addTarget()}>{$t('console_builder.add_label')}</button>
          <span class="palette-count">{$t('console_builder.unplaced').replace('{n}', unplaced.length)}</span>
        </aside>
      {/if}

      <div class="builder-stage">
        <div class="stage-heading"><span>{preview ? $t('console_builder.preview_mode') : $t('console_builder.design_mode')}</span><span>{page.width} × {page.height}</span></div>
        {#if !preview}<div class="canvas-tools"><label><input type="checkbox" bind:checked={snapGuides}>{$t('console_builder.snap_guides')}</label><span>{$t('console_builder.snap_hint')}</span><button type="button" class="btn btn-ghost btn-sm" on:click={() => wideCanvas = !wideCanvas}>{$t(wideCanvas ? 'console_builder.normal_canvas' : 'console_builder.wide_canvas')}</button></div>{/if}
        <ConsoleSurface layout={draft} {organInfo} {pageId} switcher editing={!preview && !saving && !readingTexture} inert={preview || saving || readingTexture} {selectedId} {grid} {snapGuides}
          on:pageChange={e => { pageId = e.detail.pageId; selectedId = null; }}
          on:select={e => { selectedId = e.detail.elementId; }}
          on:move={e => moveElement(e.detail)} on:resize={e => resizeElement(e.detail)} />
        {#if !preview}<p class="editor-hint">{$t('console_builder.editor_hint')} {$t('console_builder.screens_hint')}</p>{/if}
        <div class="page-controls">
          <label class="console-enabled"><input type="checkbox" checked={!!draft.screen_switcher} disabled={saving} on:change={e => mutate(layout => { if (e.target.checked) layout.screen_switcher = true; else delete layout.screen_switcher; })}><span>{$t('console_builder.screen_switcher')}</span></label>
          <label>{$t('console_builder.page_name')}<input value={page.name} maxlength="120" disabled={preview || saving} on:change={e => mutate(layout => { layout.pages.find(p => p.id === page.id).name = e.target.value; })}></label>
          {#if !preview}<button type="button" class="btn btn-secondary btn-sm" disabled={saving || draft.pages.length >= CONSOLE_MAX_PAGES} on:click={addPage}>＋ {$t('console_builder.add_page')}</button><button type="button" class="btn btn-ghost btn-sm" disabled={saving || page.elements.length > 0 || draft.pages.length <= 1} on:click={removePage}>{$t('console_builder.remove_page')}</button>{/if}
        </div>
      </div>

      {#if !preview && !wideCanvas}
        <aside class="builder-properties">
          <h2>{$t('console_builder.properties')}</h2>
          {#if selected}
            <div class="selection-name"><span>{selected.kind === 'label' ? $t('console_builder.label_element') : (selectedTarget?.division || $t('console_builder.missing_control'))}</span><strong>{selected.label || selectedTarget?.name || selected.target_id}</strong></div>
            <label>{$t('console_builder.label')}<input value={selected.label} maxlength="160" placeholder={$t('console_builder.original_label')} disabled={saving} on:change={e => editSelected({ label: e.target.value })}></label>
            {#if selected.kind !== 'label'}
              <label>{$t('console_builder.appearance')}<select value={selected.appearance} disabled={saving} on:change={e => editSelected(appearancePatch(selected, e.target.value))}>{#each CONSOLE_APPEARANCES as appearance}<option value={appearance}>{$t(`console_builder.${appearance.replaceAll('-', '_')}`)}</option>{/each}</select></label>
              <button type="button" class="btn btn-secondary btn-sm" disabled={saving} on:click={() => { const appearance = selected.appearance; mutate(layout => layout.pages.forEach(p => { p.elements = p.elements.map(e => e.kind === 'label' ? e : boundedConsoleElement(e, p, appearancePatch(e, appearance))); })); }}>{$t('console_builder.apply_style')}</button>
            {/if}
            <div class="geometry-fields">
              <label>{$t('console_builder.x')}<input type="number" min="0" max={page.width - selected.width} step={grid} value={selected.x} disabled={saving} on:change={e => editSelected({ x: Number(e.target.value) })}></label>
              <label>{$t('console_builder.y')}<input type="number" min="0" max={page.height - selected.height} step={grid} value={selected.y} disabled={saving} on:change={e => editSelected({ y: Number(e.target.value) })}></label>
              <label>{$t('console_builder.width')}<input type="number" min={selected.kind === 'label' ? 16 : 48} max={page.width} value={selected.width} disabled={saving} on:change={e => editSelected({ width: Number(e.target.value) })}></label>
              <label>{$t('console_builder.height')}<input type="number" min={selected.kind === 'label' ? 16 : 40} max={page.height} value={selected.height} disabled={saving} on:change={e => editSelected({ height: Number(e.target.value) })}></label>
            </div>
            {#if selected.kind !== 'label'}<button type="button" class="btn btn-secondary btn-sm" disabled={saving} on:click={applySizeToAll}>{$t('console_builder.apply_size')}</button>{/if}
            <div class="array-tools">
              <button type="button" class="btn btn-secondary btn-sm" disabled={saving || rowSize < 2} title={$t('console_builder.align_hint')} on:click={() => alignArray('x')}>{$t('console_builder.align_row').replace('{n}', rowSize)}</button>
              <button type="button" class="btn btn-secondary btn-sm" disabled={saving || columnSize < 2} title={$t('console_builder.align_hint')} on:click={() => alignArray('y')}>{$t('console_builder.align_column').replace('{n}', columnSize)}</button>
            </div>
            <button type="button" class="btn btn-secondary btn-sm" disabled={saving || page.elements.length >= 512} on:click={duplicate}>{$t('console_builder.duplicate')}</button>
            <button type="button" class="btn btn-ghost btn-sm remove-control" disabled={saving} on:click={removeSelected}>{$t('console_builder.remove')}</button>
          {:else}<p class="no-selection">{$t('console_builder.no_selection')}</p>{/if}
          <div class="page-properties"><h3>{$t('console_builder.page_size')}</h3><div class="geometry-fields"><label>{$t('console_builder.width')}<input type="number" min="640" max="2400" step="40" value={page.width} disabled={saving} on:change={e => changePageSize('width', e.target.value)}></label><label>{$t('console_builder.height')}<input type="number" min="400" max="1600" step="40" value={page.height} disabled={saving} on:change={e => changePageSize('height', e.target.value)}></label></div><label>{$t('console_builder.grid')}<select bind:value={grid}><option value={1}>1 px</option><option value={8}>8 px</option><option value={16}>16 px</option></select></label></div>
        </aside>
      {/if}
    </div>
  {/if}
</section>

<style>
  .console-builder{width:100%;padding:24px 24px 32px;color:var(--text);background:var(--bg-dark);box-sizing:border-box;overflow:auto;min-height:100%}
  .builder-heading{display:flex;justify-content:space-between;align-items:center;gap:24px;margin:0 0 20px}
  .builder-heading h1{font:500 27px/1.2 Georgia,serif;margin:4px 0 5px}
  .builder-heading p{color:var(--text-secondary);font-size:13px;margin:0}
  .builder-eyebrow{font-size:10px;letter-spacing:.18em;color:var(--text-muted);font-weight:600}
  .organ-plaque{font:500 16px/1.25 Georgia,serif;max-width:35%;padding:10px 20px;border:1px solid var(--stop-porcelain-border);border-radius:6px;background:var(--bg-elevated);color:var(--text-secondary);text-align:center;box-shadow:var(--shadow-sm);overflow-wrap:anywhere}
  .builder-commandbar{position:sticky;top:0;z-index:8;display:flex;align-items:center;gap:16px;flex-wrap:wrap;padding:10px 0 14px;background:var(--bg-dark);border-bottom:1px solid var(--stop-porcelain-border)}
  .builder-mode{display:inline-flex;padding:3px;gap:3px;background:var(--bg-panel);border:1px solid var(--stop-porcelain-border);border-radius:7px}
  .builder-mode button{font:inherit;font-size:12px;font-weight:500;padding:7px 15px;color:var(--text-secondary);border:none;background:transparent;border-radius:4px;cursor:pointer}
  .builder-mode button.active{background:var(--primary);color:var(--text-on-primary);box-shadow:var(--shadow-sm)}
  .draft-state{font-size:12px;color:var(--text-muted)}
  .draft-state.dirty{color:var(--text-secondary)}
  .draft-state.dirty:before{content:'●';color:var(--primary);margin-right:7px}
  .builder-actions{display:flex;gap:8px;margin-left:auto;flex-wrap:wrap}
  .builder-settings{display:flex;align-items:end;gap:16px;flex-wrap:wrap;margin:16px 0}
  .builder-settings>label:not(.console-enabled){min-width:140px;max-width:240px;flex:1}
  .builder-settings>.console-enabled{align-self:center;display:flex;align-items:center;gap:8px;flex-direction:row;margin-right:6px;font-size:12px}
  .console-builder label{display:flex;flex-direction:column;gap:5px;font-size:11px;font-weight:500;color:var(--text-secondary);min-width:0}
  .console-builder input:not([type=checkbox]),.console-builder select{width:100%;min-width:0;padding:7px 8px;min-height:32px;border:1px solid var(--stop-porcelain-border);border-radius:5px;background:var(--bg-elevated);color:var(--text);font:inherit;font-size:12px;font-weight:400;box-sizing:border-box}
  .console-builder input[type=checkbox]{accent-color:var(--primary);width:16px;height:16px;margin:0}
  .builder-workspace{display:grid;grid-template-columns:190px minmax(0,1fr) 188px;align-items:start;gap:16px}
  .builder-workspace.preview,.builder-workspace.wide{grid-template-columns:minmax(0,1fr)}
  .builder-materials{display:grid;grid-template-columns:minmax(150px,1fr) minmax(100px,.7fr) minmax(100px,.7fr) minmax(220px,1.4fr);gap:16px;align-items:center;margin:0 0 20px;padding:14px;background:var(--bg-panel);border:1px solid var(--stop-porcelain-border);border-radius:8px}
  .builder-materials output{display:inline;color:var(--text)}
  .builder-materials input[type=range]{padding:0;accent-color:var(--primary)}
  .custom-material{display:flex;align-items:center;gap:6px;flex-wrap:wrap;min-width:0}
  .custom-material small{display:block;flex-basis:100%;font-size:11px;color:var(--text-secondary);overflow-wrap:anywhere}
  .texture-file-input{display:none}
  .canvas-tools{display:flex;gap:10px;align-items:center;flex-wrap:wrap;margin:0 0 10px;color:var(--text-secondary);font-size:11px}
  .canvas-tools label{display:flex;flex-direction:row;align-items:center;gap:6px}
  .canvas-tools button{margin-left:auto}
  .builder-workspace aside{background:var(--bg-panel);border:1px solid var(--stop-porcelain-border);border-radius:8px;padding:13px;display:flex;flex-direction:column;gap:12px;min-width:0}
  .builder-workspace h2{font-size:13px;line-height:1.3;margin:0;font-weight:600}
  .palette-list{max-height:440px;overflow-y:auto;display:flex;flex-direction:column;gap:2px;margin:0 -5px;padding:0 2px}
  .palette-control{display:flex;align-items:center;gap:7px;padding:8px 3px;border:0;border-bottom:1px solid var(--stop-porcelain-border);text-align:left;background:transparent;color:var(--text);cursor:pointer;width:100%;min-width:0;font:inherit}
  .palette-control>span:nth-child(2){flex:1;min-width:0}
  .palette-control strong{display:block;font-size:11px;font-weight:500;line-height:1.3;overflow-wrap:anywhere}
  .palette-control small{display:block;font-size:10px;color:var(--text-muted);margin-top:3px;overflow-wrap:anywhere}
  .palette-symbol{color:var(--primary);font-size:16px}
  .palette-control:disabled{opacity:.55;cursor:default}
  .palette-control.placed .palette-symbol{color:var(--text-muted)}
  .placed-check{color:var(--text-muted);font-size:11px}
  .palette-count{font-size:11px;color:var(--text-muted)}
  .builder-stage{min-width:0}
  .stage-heading{display:flex;justify-content:space-between;gap:10px;margin:0 0 10px;color:var(--text-muted);font-size:10px;flex-wrap:wrap}
  .editor-hint{font-size:11px;color:var(--text-muted);margin:12px 0}
  .page-controls{display:flex;align-items:end;gap:10px;flex-wrap:wrap;margin-top:12px}
  .page-controls label{flex:1;min-width:130px;max-width:230px}
  .geometry-fields{display:grid;grid-template-columns:1fr 1fr;gap:10px}
  .selection-name{border-left:3px solid var(--primary);padding-left:9px;overflow-wrap:anywhere}
  .selection-name span{display:block;font-size:10px;color:var(--text-muted);margin-bottom:3px}
  .selection-name strong{font-size:12px;font-weight:500}
  .no-selection{font-size:12px;color:var(--text-muted);margin:0;line-height:1.5}
  .page-properties{display:flex;flex-direction:column;gap:10px;border-top:1px solid var(--stop-porcelain-border);padding-top:13px;margin-top:2px}
  .page-properties h3{font-size:11px;color:var(--text-secondary);margin:0;font-weight:500}
  .remove-control{color:var(--error)}
  .builder-notice{display:flex;align-items:center;justify-content:space-between;gap:12px;flex-wrap:wrap;padding:12px 14px;margin:12px 0;border-radius:6px;font-size:12px;line-height:1.5;background:var(--bg-panel);border:1px solid var(--stop-porcelain-border)}
  .builder-notice.error{border-left:4px solid var(--error);color:var(--text)}
  .builder-notice.warning{border-left:4px solid var(--warning);color:var(--text)}
  .builder-notice.success{border-left:4px solid var(--success);color:var(--text)}
  @media(max-width:1150px){.console-builder{padding:16px}.builder-workspace{grid-template-columns:165px minmax(0,1fr) 170px;gap:12px}.builder-workspace aside{padding:10px}.builder-actions{margin-left:0}.organ-plaque{max-width:40%}}
  @media(max-width:850px){.builder-workspace{grid-template-columns:160px minmax(0,1fr)}.builder-properties{grid-column:1/-1;display:grid!important;grid-template-columns:repeat(3,minmax(0,1fr))}.builder-properties>h2{grid-column:1/-1}.page-properties{grid-column:1/-1}.organ-plaque{display:none}}
  @media(max-width:850px){.builder-materials{grid-template-columns:1fr 1fr}}
  @media(pointer:coarse){.console-builder input:not([type=checkbox]),.console-builder select{min-height:44px;font-size:16px}.builder-mode button,.palette-control{min-height:44px}}
</style>
