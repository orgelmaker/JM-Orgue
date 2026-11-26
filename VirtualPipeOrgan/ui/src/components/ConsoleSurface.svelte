<script>
  import { createEventDispatcher, onMount } from 'svelte';
  import ConsoleControl from './ConsoleControl.svelte';
  import { boundedConsoleElement, consoleTargetMap } from '../lib/consoleLayout.js';
  import { snapConsoleElement } from '../lib/consoleSnap.js';
  import { t } from '../lib/i18n.js';
  export let layout = null;
  export let organInfo = null;
  export let pageId = null;
  export let editing = false;
  export let selectedId = null;
  export let inert = false;
  export let grid = 8;
  export let snapGuides = true;
  const dispatch = createEventDispatcher();
  let viewport, availableWidth = 800, drag = null, transient = null, suppressClick = false, guides = [];
  $: pages = layout?.pages || [];
  $: page = pages.find(p => p.id === pageId) || pages[0] || null;
  $: targets = consoleTargetMap(organInfo);
  $: scale = page ? Math.min(1, availableWidth / page.width) : 1;
  $: elements = page?.elements || [];
  $: texture = ['grain','oak','burl','ebony','linen','brushed','plain','custom'].includes(layout?.texture) ? layout.texture : 'grain';
  $: textureOpacity = Number.isFinite(layout?.texture_opacity) ? Math.max(0, Math.min(1, layout.texture_opacity)) : 0.65;
  $: textureUnit = Number.isFinite(layout?.texture_scale) ? Math.max(25, Math.min(400, layout.texture_scale)) / 100 : 1;
  $: textureImage = texture === 'custom' && /^data:image\/(?:png|jpeg|webp);base64,[A-Za-z0-9+/]+={0,2}$/.test(layout?.custom_texture?.data_url || '') ? `url("${layout.custom_texture.data_url}")` : null;
  onMount(() => {
    const observer = new ResizeObserver(entries => { availableWidth = entries[0].contentRect.width || 800; });
    observer.observe(viewport);
    return () => observer.disconnect();
  });
  function start(event, element, mode = 'move') {
    if (!editing || event.button !== 0) return;
    if (event.target.closest('button') == null) return;
    event.preventDefault();
    event.stopPropagation();
    event.currentTarget?.focus({ preventScroll: true });
    event.currentTarget?.setPointerCapture?.(event.pointerId);
    dispatch('select', { elementId: element.id });
    drag = { element, pageId: page.id, pointerId: event.pointerId, startX: event.clientX, startY: event.clientY, scale, mode, moved: false };
    transient = null; guides = [];
  }
  function move(event) {
    if (!drag || !page || drag.pageId !== page.id || event.pointerId !== drag.pointerId) return;
    const dx = (event.clientX - drag.startX) / drag.scale, dy = (event.clientY - drag.startY) / drag.scale;
    if (Math.abs(dx) + Math.abs(dy) < 3 && !drag.moved) return;
    drag.moved = true;
    const candidate = drag.mode === 'resize' ? { ...drag.element, width: drag.element.width + dx, height: drag.element.height + dy }
      : { ...drag.element, x: drag.element.x + dx, y: drag.element.y + dy };
    const result = snapConsoleElement(candidate, page, elements, { scale: drag.scale, grid, enabled: snapGuides, altKey: event.altKey, mode: drag.mode });
    transient = result.element; guides = result.guides;
  }
  function finish(event) {
    if (!drag || (event?.pointerId != null && event.pointerId !== drag.pointerId)) return;
    const moved = drag.moved, element = transient, mode = drag.mode;
    drag = null; transient = null; guides = [];
    if (moved && element) {
      suppressClick = true;
      if (mode === 'resize') dispatch('resize', { elementId: element.id, width: element.width, height: element.height });
      else dispatch('move', { elementId: element.id, x: element.x, y: element.y });
      setTimeout(() => { suppressClick = false; }, 0);
    }
  }
  function cancel() { drag = null; transient = null; guides = []; }
  function resizeKey(event, element) {
    if (!editing || !page || !['ArrowLeft','ArrowRight','ArrowUp','ArrowDown'].includes(event.key)) return;
    event.preventDefault(); event.stopPropagation();
    const step = event.shiftKey || event.altKey ? 1 : grid;
    const width = element.width + (event.key === 'ArrowLeft' ? -step : event.key === 'ArrowRight' ? step : 0);
    const height = element.height + (event.key === 'ArrowUp' ? -step : event.key === 'ArrowDown' ? step : 0);
    const next = snapConsoleElement({ ...element, width, height }, page, [], { mode: 'resize', enabled: false, altKey: true }).element;
    dispatch('resize', { elementId: element.id, width: next.width, height: next.height });
  }
  function activate(element) {
    if (suppressClick) return;
    if (editing) dispatch('select', { elementId: element.id });
    else if (!inert && element.kind !== 'label' && targets.has(`${element.kind}:${element.target_id}`))
      dispatch('toggle', { kind: element.kind, targetId: element.target_id });
  }
  function learn(element) {
    if (!editing && !inert && element.kind !== 'label' && targets.has(`${element.kind}:${element.target_id}`))
      dispatch('learn', { kind: element.kind, targetId: element.target_id });
  }
  function key(event, element) {
    if (!editing || !page || !['ArrowLeft','ArrowRight','ArrowUp','ArrowDown'].includes(event.key)) return;
    event.preventDefault(); event.stopPropagation();
    const step = event.shiftKey ? 1 : grid;
    const dx = event.key === 'ArrowLeft' ? -step : event.key === 'ArrowRight' ? step : 0;
    const dy = event.key === 'ArrowUp' ? -step : event.key === 'ArrowDown' ? step : 0;
    const next = boundedConsoleElement(element, page, { x: element.x + dx, y: element.y + dy });
    dispatch('select', { elementId: element.id });
    dispatch('move', { elementId: element.id, x: next.x, y: next.y });
  }
</script>

<svelte:window on:pointermove={move} on:pointerup={finish} on:pointercancel={cancel} on:blur={cancel} />

<div class="console-surface theme-{layout?.theme || 'walnut'}" class:editing style="--console-scale:{scale};max-width:{page?.width || 1200}px;margin-inline:auto">
  {#if pages.length > 1}
    <nav class="console-pages" aria-label={$t('console_builder.page')}>
      {#if pages.length > 8}
        <label>{$t('console_builder.page')}
          <select value={page?.id} on:change={(event) => dispatch('pageChange', { pageId: event.currentTarget.value })}>
            {#each pages as p}<option value={p.id}>{p.name}</option>{/each}
          </select>
        </label>
        <span>{pages.findIndex(p => p.id === page?.id) + 1} / {pages.length}</span>
      {:else}
      {#each pages as p}<button type="button" class:current={p.id === page?.id} on:click={() => dispatch('pageChange', { pageId: p.id })} aria-pressed={p.id === page?.id}>{p.name}</button>{/each}
      {/if}
    </nav>
  {/if}
  <div class="console-viewport" bind:this={viewport} style:height={page ? `${page.height * scale}px` : '80px'}>
    {#if page}
      <div class="console-board" class:grid={editing} style="width:{page.width}px;height:{page.height}px;transform:scale({scale});--console-grid:{grid}px" role="group" aria-label={page.name}>
        <div class="board-texture texture-{texture}" style="opacity:{textureOpacity};--texture-unit:{textureUnit};{textureImage ? `background-image:${textureImage}` : ''}" aria-hidden="true"></div>
        <div class="board-trim" aria-hidden="true"></div>
        {#each elements as element (element.id)}
          {@const shown = transient?.id === element.id ? transient : element}
          <div class="console-element" class:is-label={element.kind === 'label'} style="left:{shown.x}px;top:{shown.y}px;width:{shown.width}px;height:{shown.height}px;font-size:{Math.max(12, Math.min(17, element.width / 8))}px">
            <ConsoleControl {element} target={targets.get(`${element.kind}:${element.target_id}`)} {editing} {inert}
              selected={selectedId === element.id} on:activate={() => activate(element)} on:learn={() => learn(element)}
              on:pointerdown={(e) => start(e.detail, element)} on:keydown={(e) => key(e.detail, element)} />
            {#if editing && selectedId === element.id}
              <button type="button" class="resize-handle" aria-label={$t('console_builder.resize_control')} title={$t('console_builder.resize_control')}
                on:pointerdown={event => start(event, element, 'resize')} on:keydown={event => resizeKey(event, element)} on:click|stopPropagation><span aria-hidden="true">↘</span></button>
            {/if}
          </div>
        {/each}
        {#if editing}{#each guides as guide}<div class="snap-guide guide-{guide.axis}" style={guide.axis === 'x' ? `left:${guide.position}px;top:${guide.from}px;height:${guide.to - guide.from}px` : `top:${guide.position}px;left:${guide.from}px;width:${guide.to - guide.from}px`} aria-hidden="true"></div>{/each}{/if}
        {#if elements.length === 0}<div class="empty-console">{$t('console_builder.empty_page')}</div>{/if}
      </div>
    {/if}
  </div>
</div>

<style>
  .console-surface{--console-case:#302016;--console-case-edge:#1c130c;--console-face:#d7b77d;--console-face-top:#f2dfba;--console-face-border:#b49768;--console-ink:#2e2113;--console-light:#ffd36b;--console-select:#ffd36b;--console-label:#f5e1bc;--console-reed:#812319;--console-draw-face:#292019;--console-draw-highlight:#66503b;--console-draw-ink:#f5e8cb;--console-draw-edge:#987553;--console-draw-reed:#ffb2a2;--console-wood:#68452d;--console-edge:#281a11;--console-grain:#c5945524;width:100%;min-width:0}
  .theme-ivory{--console-case:#c5bda9;--console-case-edge:#8c826e;--console-face:#eee8d9;--console-face-top:#fffdf4;--console-face-border:#bcb19b;--console-ink:#2b2a26;--console-label:#4b4032;--console-wood:#d6cbb7;--console-edge:#928674;--console-grain:#fff7dd30;--console-select:#855511;--console-draw-face:#e8dfc9;--console-draw-highlight:#fffaf0;--console-draw-ink:#332b22;--console-draw-edge:#9a835d;--console-draw-reed:#8f2c1f}
  .theme-slate{--console-case:#1e242b;--console-case-edge:#0f1318;--console-face:#d0d5d6;--console-face-top:#eaf0f0;--console-face-border:#8c999e;--console-ink:#202b32;--console-light:#b9e1ff;--console-label:#e0eaf1;--console-wood:#303b43;--console-edge:#121b22;--console-grain:#b4d2ed08;--console-select:#a7dfff;--console-draw-face:#202a32;--console-draw-highlight:#4c616c;--console-draw-ink:#e5eff4;--console-draw-edge:#8198a3}
  .console-pages{display:flex;gap:6px;flex-wrap:wrap;padding:0 0 10px}
  .console-pages label{display:flex;gap:8px;align-items:center}
  .console-pages select{max-width:260px;font:inherit;padding:6px 10px;background:var(--bg-elevated);color:var(--text);border:1px solid var(--border-color,var(--stop-porcelain-border));border-radius:5px}
  .console-pages > span{align-self:center;color:var(--text-secondary);font-size:12px}
  .console-pages button{font:inherit;background:var(--bg-elevated);color:var(--text-secondary);border:1px solid var(--border-color,var(--stop-porcelain-border));padding:6px 14px;border-radius:5px;cursor:pointer}
  .console-pages button.current{background:var(--primary);color:var(--text-on-primary);border-color:var(--primary)}
  .console-viewport{position:relative;width:100%;min-width:0;overflow:hidden;border-radius:8px;box-shadow:0 4px 16px #0004}
  .console-board{position:relative;transform-origin:top left;background-color:var(--console-wood);background-image:linear-gradient(90deg,#0002,transparent 18%,#fff1 45%,#0001 80%);box-shadow:inset 0 0 0 8px var(--console-edge),inset 0 0 0 10px #ffffff13,inset 0 0 20px 12px #0004;border-radius:8px;isolation:isolate}
  .board-texture{position:absolute;inset:10px;z-index:0;pointer-events:none;border-radius:3px;background-repeat:repeat}
  .texture-grain{background-image:repeating-linear-gradient(91deg,transparent 0 calc(9px * var(--texture-unit)),var(--console-grain) calc(10px * var(--texture-unit)),transparent calc(12px * var(--texture-unit)) calc(18px * var(--texture-unit))),repeating-linear-gradient(88deg,transparent 0 calc(49px * var(--texture-unit)),#150b052b calc(50px * var(--texture-unit)),transparent calc(54px * var(--texture-unit)) calc(81px * var(--texture-unit)))}
  .texture-oak{background-image:repeating-linear-gradient(89deg,#4c260522 0 calc(2px * var(--texture-unit)),transparent calc(3px * var(--texture-unit)) calc(9px * var(--texture-unit)),#ffe2a422 calc(11px * var(--texture-unit)),transparent calc(12px * var(--texture-unit)) calc(24px * var(--texture-unit))),radial-gradient(ellipse at 70% 35%,#2410014a 0,transparent 12%,#ffe1ac18 19%,transparent 24%);background-size:auto,calc(220px * var(--texture-unit)) calc(650px * var(--texture-unit))}
  .texture-burl{background-image:repeating-radial-gradient(ellipse at 23% 62%,#e8a65a26 0 calc(3px * var(--texture-unit)),#2e100b3d calc(5px * var(--texture-unit)),transparent calc(8px * var(--texture-unit)) calc(15px * var(--texture-unit))),radial-gradient(ellipse at 78% 22%,#fff0c326,transparent 40%);background-size:calc(230px * var(--texture-unit)) calc(170px * var(--texture-unit)),calc(390px * var(--texture-unit)) calc(240px * var(--texture-unit))}
  .texture-ebony{background-color:#100e0ed9;background-image:repeating-linear-gradient(91deg,#e8d5b518 0 calc(1px * var(--texture-unit)),transparent calc(2px * var(--texture-unit)) calc(10px * var(--texture-unit)),#0505066b calc(12px * var(--texture-unit)),transparent calc(14px * var(--texture-unit)) calc(25px * var(--texture-unit)))}
  .texture-linen{background-image:repeating-linear-gradient(0deg,#fff5dc26 0 calc(1px * var(--texture-unit)),#281a2029 calc(1px * var(--texture-unit)) calc(2px * var(--texture-unit)),transparent calc(2px * var(--texture-unit)) calc(5px * var(--texture-unit))),repeating-linear-gradient(90deg,#fff5dc26 0 calc(1px * var(--texture-unit)),#281a2029 calc(1px * var(--texture-unit)) calc(2px * var(--texture-unit)),transparent calc(2px * var(--texture-unit)) calc(5px * var(--texture-unit)))}
  .texture-brushed{background-color:#a5abb529;background-image:repeating-linear-gradient(0deg,#ffffff36 0 calc(.7px * var(--texture-unit)),#111b202b calc(.8px * var(--texture-unit)) calc(1.4px * var(--texture-unit)),transparent calc(1.5px * var(--texture-unit)) calc(4px * var(--texture-unit))),linear-gradient(110deg,transparent,#ffffff2b 40%,#00000019 65%,transparent)}
  .texture-plain{background-image:none}
  .texture-custom{background-size:calc(260px * var(--texture-unit)) auto}
  .board-trim{position:absolute;inset:17px;z-index:1;border:1px solid #ffffff16;border-radius:3px;pointer-events:none}
  .console-board.grid:after{content:'';position:absolute;inset:10px;z-index:1;pointer-events:none;background-image:radial-gradient(#ffffff24 .8px,transparent .8px);background-size:var(--console-grid) var(--console-grid)}
  .console-element{position:absolute;min-width:0;z-index:2}
  .console-element:has(:global(.selected)){z-index:3}
  .resize-handle{position:absolute;right:calc(-3px / var(--console-scale,1));bottom:calc(-3px / var(--console-scale,1));display:grid;place-items:center;width:calc(20px / var(--console-scale,1));height:calc(20px / var(--console-scale,1));padding:0;border:1px solid #32210f;background:var(--console-select);color:#281c0e;border-radius:4px;z-index:5;font:500 calc(14px / var(--console-scale,1))/1 sans-serif;cursor:nwse-resize;touch-action:none}
  .resize-handle:focus-visible{outline:3px solid var(--console-select);outline-offset:3px}
  .snap-guide{position:absolute;z-index:4;pointer-events:none;background:#80e1ff;box-shadow:0 0 0 calc(.5px / var(--console-scale,1)) #152d3e99}
  .guide-x{width:calc(1px / var(--console-scale,1))}
  .guide-y{height:calc(1px / var(--console-scale,1))}
  .empty-console{position:absolute;inset:30px;z-index:2;display:grid;place-items:center;color:var(--console-label);font-size:18px}
  @media(pointer:coarse){.console-pages button{min-height:44px}}
</style>
