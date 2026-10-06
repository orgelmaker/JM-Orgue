<script>
  // Klavar-weergave (0.7.71): tekent het KlavarModel uit de backend als SVG,
  // per systeem (een blok hele maten) één <svg> in mm, verdeeld in pagina's
  // en kolommen (klavarLayout.paginate). Op het scherm staan de systemen onder
  // elkaar; bij afdrukken staan de kolommen naast elkaar (zie de printregels).
  //
  // Klikken, slepen en stapinvoer worden hier afgehandeld (stopPropagation)
  // en als events naar het notatievenster gestuurd:
  //   select    { id, shift }         — noot aangeklikt
  //   drag      { id, semitones }     — noot horizontaal naar een andere toets gesleept
  //   stepclick { midi, staff, gridTime } — klik op een lege plek in stapinvoer
  // Elke noot kent haar event-ID (data-id); er is geen OSMD-correlatie nodig.
  import { createEventDispatcher, onDestroy } from 'svelte';
  import { t } from '../lib/i18n.js';
  import { MAAT, PX_PER_MM, splitSystems, layoutSystem, paginate, hitTest, gridTimeAtY, staffAtX, nearestKey, nuLijnY, openNoten, pasBereikToe } from '../lib/klavarLayout.js';

  export let model = null;
  export let selectionIds = new Set();
  export let cursorId = null;
  export let zoom = 1;
  export let stepMode = false;
  export let gum = false;      // kleverige gum (0.7.87): klik = verwijderen, ook in stapinvoer
  export let recording = false;
  export let live = false;
  // Inspelen (0.7.72): het opnametijdstip in rastereenheden (nu-lijn) en de
  // toetsen die nu ingedrukt zijn [{ layerId, midi, start }].
  export let nowGrid = null;
  export let openNotes = [];
  // Vast bereik (0.7.73): { manual: [lo, hi], pedal: [lo, hi] } of null (automatisch).
  export let bereik = null;

  const dispatch = createEventDispatcher();

  // Tijdens het inspelen loopt het papier twee maten vóór op de nu-lijn.
  // `extraMaten` is een getal: Svelte herrekent de lay-out alleen als het
  // verandert (eens per maat), niet bij elke stap van de nu-lijn.
  $: extraMaten = model && recording && nowGrid != null && model.measure_len > 0
    ? Math.max(0, Math.floor(nowGrid / model.measure_len) + 2 - model.num_measures) : 0;
  $: basisModel = model && bereik ? pasBereikToe(model, bereik) : model;
  $: effModel = basisModel && extraMaten ? { ...basisModel, num_measures: basisModel.num_measures + extraMaten } : basisModel;
  $: layouts = effModel ? splitSystems(effModel).map(s => layoutSystem(effModel, s, { verbergLaatsteStop: recording })) : [];
  $: paginas = paginate(layouts);
  $: kpx = PX_PER_MM * zoom;
  // Lijnen nooit dunner dan één schermpixel.
  $: minStroke = 1 / kpx;
  $: sw = (mm) => Math.max(mm, minStroke);

  // `vertaal` ($t) als argument, zodat de markup bij een taalwissel meeverandert.
  function handLabel(l, vertaal) {
    if (l.hand === 'pedal') return vertaal('notation.klavar_pedal_label');
    if (l.split_midi != null) return 'R+L';
    return l.hand === 'left' ? 'L' : 'R';
  }

  // Klikpunt → mm in de SVG.
  function svgPunt(svg, e) {
    const m = svg.getScreenCTM();
    if (!m) return null;
    const p = svg.createSVGPoint();
    p.x = e.clientX; p.y = e.clientY;
    const q = p.matrixTransform(m.inverse());
    return { x: q.x, y: q.y };
  }
  function balkVoor(lay, staff) { return staff === 'pedal' && lay.pedaal ? lay.pedaal : lay.manuaal; }

  let drag = null;
  let dragConsumedClick = false;
  function onMouseDown(e, lay, svg) {
    if (!live || stepMode || e.button !== 0) return;
    const p = svgPunt(svg, e);
    if (!p) return;
    const hit = hitTest(lay, p.x, p.y, 1.5 * MAAT.w);
    if (!hit || hit.id == null) return;
    e.stopPropagation();
    drag = { id: hit.id, midi: hit.midi, balk: hit.balk, lay, svg, startX: e.clientX, moved: false };
    window.addEventListener('mousemove', onDragMove);
    window.addEventListener('mouseup', onDragUp);
  }
  function onDragMove(e) {
    if (drag && Math.abs(e.clientX - drag.startX) > 5) drag.moved = true;
  }
  function onDragUp(e) {
    window.removeEventListener('mousemove', onDragMove);
    window.removeEventListener('mouseup', onDragUp);
    const d = drag; drag = null;
    if (!d || !d.moved) return;
    dragConsumedClick = true;
    setTimeout(() => { dragConsumedClick = false; }, 0);
    const p = svgPunt(d.svg, e);
    if (!p) return;
    const doel = nearestKey(p.x, balkVoor(d.lay, d.balk));
    const delta = Math.max(-24, Math.min(24, doel - d.midi));
    if (delta) dispatch('drag', { id: d.id, semitones: delta });
  }
  onDestroy(() => {
    window.removeEventListener('mousemove', onDragMove);
    window.removeEventListener('mouseup', onDragUp);
  });
  function onClick(e, lay, svg) {
    e.stopPropagation();
    if (!live || dragConsumedClick) return;
    const p = svgPunt(svg, e);
    if (!p) return;
    if (stepMode && !gum) {
      const staff = staffAtX(lay, p.x);
      const midi = nearestKey(p.x, balkVoor(lay, staff));
      dispatch('stepclick', { midi, staff, gridTime: gridTimeAtY(lay, p.y), shift: e.shiftKey });
      return;
    }
    const hit = hitTest(lay, p.x, p.y, 1.5 * MAAT.w);
    if (!hit || hit.id == null) return;
    dispatch('select', { id: hit.id, shift: e.shiftKey || e.ctrlKey || e.metaKey });
  }
</script>

<div class="klavar" class:klavar-live={live} class:klavar-stap={stepMode} style="--kpx:{kpx}">
  {#if model}
    {#each paginas as pagina, pi}
      <section class="klavar-pagina" class:eerste={pi === 0}>
        <header class="klavar-kop">
          <h2>{model.title}</h2>
          <span class="klavar-tempo">{$t('notation.klavar_tempo').replace('{bpm}', Math.round(model.bpm))}</span>
          <span class="klavar-legenda">{$t('notation.klavar_legend')}:
            {#each model.legend as l, i}{i ? ' · ' : ' '}{l.name} ({handLabel(l, $t)}){/each}
          </span>
        </header>
        <div class="klavar-kolommen">
          {#each pagina.kolommen as kolom}
            <div class="klavar-kolom">
              {#each kolom as lay (lay.sys.index)}
                <svg
                  class="klavar-systeem"
                  viewBox="0 0 {lay.breedte} {lay.hoogte}"
                  style="--wmm:{lay.breedte}; --hmm:{lay.hoogte}"
                  on:click={(e) => onClick(e, lay, e.currentTarget)}
                  on:mousedown={(e) => onMouseDown(e, lay, e.currentTarget)}
                  role="img"
                  aria-label={$t('notation.view_klavar')}
                >
                  <!-- Toetslijnen -->
                  {#each lay.lijnen as l}
                    <line class="k-lijn" x1={l.x} x2={l.x} y1={l.y0} y2={l.y1}
                      stroke-width={sw(l.dik ? MAAT.lijnDik : MAAT.lijnDun)}
                      stroke-dasharray={l.gestreept ? '1.5 1' : null} />
                  {/each}
                  <!-- Telstrepen en maatstrepen -->
                  {#each lay.telstrepen as s}
                    <line class="k-tel" x1={s.x0} x2={s.x1} y1={s.y} y2={s.y}
                      stroke-width={sw(MAAT.telstreep)} stroke-dasharray={s.gestreept ? '1 1' : null} />
                  {/each}
                  {#each lay.maatstrepen as m}
                    <line class="k-maat" x1={m.x0} x2={m.x1} y1={m.y} y2={m.y} stroke-width={sw(m.dikte)} />
                    {#if m.dubbel}
                      <line class="k-maat" x1={m.x0} x2={m.x1} y1={m.y + m.dikte * 3} y2={m.y + m.dikte * 3} stroke-width={sw(m.dikte)} />
                    {/if}
                  {/each}
                  {#each lay.telnummers as n}
                    <text class="k-tekst" x={n.x} y={n.y} text-anchor="end" font-size={MAAT.tekst}>{n.tekst}</text>
                  {/each}
                  {#each lay.maatnummers as n}
                    <text class="k-tekst k-maatnummer" x={n.x} y={n.y} font-size={MAAT.tekst * 0.85}>{n.tekst}</text>
                  {/each}
                  <!-- Toonsoortteken boven de eerste maat (0.7.72) -->
                  {#if lay.toonsoort}
                    {#if lay.toonsoort.minor}
                      <path class="k-toonsoort" d="M {lay.toonsoort.x} {lay.toonsoort.y - lay.toonsoort.ring} L {lay.toonsoort.x + lay.toonsoort.ring} {lay.toonsoort.y} L {lay.toonsoort.x} {lay.toonsoort.y + lay.toonsoort.ring} L {lay.toonsoort.x - lay.toonsoort.ring} {lay.toonsoort.y} Z" stroke-width={sw(MAAT.stok)} />
                    {:else}
                      <circle class="k-toonsoort" cx={lay.toonsoort.x} cy={lay.toonsoort.y} r={lay.toonsoort.ring} stroke-width={sw(MAAT.stok)} />
                    {/if}
                    <circle class="k-kop" class:zwart={lay.toonsoort.zwart} cx={lay.toonsoort.x} cy={lay.toonsoort.y} r={lay.toonsoort.r} stroke-width={sw(MAAT.kopLijn)} />
                  {/if}
                  <!-- Balken per tel (0.7.72) -->
                  {#each lay.balken as b}
                    <polyline class="k-balk" points={b.punten.map(p => p.x + ',' + p.y).join(' ')} stroke-width={sw(b.dikte)} />
                  {/each}
                  <!-- Akkoordlijnen, stokken en koppen -->
                  {#each lay.akkoordlijnen as a}
                    <line class="k-stok k-v{a.voice || 1}" x1={a.x0} x2={a.x1} y1={a.y} y2={a.y} stroke-width={sw(MAAT.stok)} />
                  {/each}
                  {#each lay.noten as n (n.key)}
                    <g class="k-noot" data-id={n.id} class:selected={n.id != null && selectionIds.has(n.id)} class:cursor={n.id != null && n.id === cursorId}>
                      {#if !n.inAkkoord}
                        <line class="k-stok k-v{n.voice || 1}" x1={n.stokX0} x2={n.stokX1} y1={n.yStok} y2={n.yStok} stroke-width={sw(MAAT.stok)} />
                      {/if}
                      <circle class="k-kop" class:zwart={n.zwart} cx={n.cx} cy={n.cy} r={n.r} stroke-width={sw(MAAT.kopLijn)} />
                      {#if n.fermata}
                        <text class="k-label k-fermate" x={n.cx} y={n.cy - n.r * 1.4} text-anchor="middle" font-size={MAAT.labelTekst}>𝄐</text>
                      {/if}
                    </g>
                  {/each}
                  <!-- Stoptekens en doorklinkstippen -->
                  {#each lay.stops as s}
                    <path class="k-stop" d="M {s.x - s.b / 2} {s.y - s.h} L {s.x} {s.y} L {s.x + s.b / 2} {s.y - s.h}" stroke-width={sw(MAAT.stok)} />
                  {/each}
                  {#each lay.stippen as d}
                    <circle class="k-stip" cx={d.x} cy={d.y} r={d.r} />
                  {/each}
                  <!-- Manuaallabels bij een wissel van laag (0.7.72) -->
                  {#each lay.labels as l}
                    <text class="k-label" x={l.x} y={l.y} text-anchor={l.anchor} font-size={MAAT.labelTekst}>{l.tekst}</text>
                  {/each}
                  <!-- Aanwijzingen (0.7.88) -->
                  {#each lay.teksten as tk}
                    <text class="k-label k-aanwijzing" x={tk.x} y={tk.y} text-anchor={tk.anchor} font-size={MAAT.labelTekst}>{tk.tekst}</text>
                  {/each}
                  <!-- Inspelen (0.7.72): aangehouden toetsen (grijs) en de nu-lijn -->
                  {#each openNoten(lay, effModel, openNotes) as o (o.key)}
                    <g class="k-noot k-open">
                      <line class="k-stok" x1={o.stokX0} x2={o.stokX1} y1={o.yStok} y2={o.yStok} stroke-width={sw(MAAT.stok)} />
                      <circle class="k-kop" class:zwart={o.zwart} cx={o.cx} cy={o.cy} r={o.r} stroke-width={sw(MAAT.kopLijn)} />
                    </g>
                  {/each}
                  {#if nowGrid != null}
                    {@const yNu = nuLijnY(lay, nowGrid)}
                    {#if yNu != null}
                      <line class="k-nu" x1={lay.xLinks - 1.5} x2={lay.xRechts + 1.5} y1={yNu} y2={yNu} stroke-width={sw(0.3)} />
                    {/if}
                  {/if}
                </svg>
              {/each}
            </div>
          {/each}
        </div>
      </section>
    {/each}
  {/if}
</div>

<style>
  .klavar { display: block; }
  .klavar-pagina { display: block; }
  .klavar-kop { display: none; margin: 0 0 0.5rem; color: #222; }
  .klavar-pagina.eerste .klavar-kop { display: block; }
  .klavar-kop h2 { margin: 0 0 0.15rem; font-size: 1.1rem; font-weight: 600; }
  .klavar-tempo { margin-right: 1rem; font-size: 0.85rem; }
  .klavar-legenda { font-size: 0.8rem; color: #444; }
  .klavar-kolommen { display: block; }
  .klavar-kolom { display: block; }
  .klavar-systeem {
    display: block;
    width: calc(var(--wmm) * var(--kpx) * 1px);
    height: calc(var(--hmm) * var(--kpx) * 1px);
    overflow: visible;
    user-select: none;
  }
  /* Op het scherm sluiten de systemen van een kolom naadloos aan: de
     onderrand van het ene en de bovenrand van het volgende (samen 4 mm)
     overlappen, zodat de dubbele maatstreep op de grens samenvalt. */
  .klavar-kolom > .klavar-systeem + .klavar-systeem { margin-top: calc(-4 * var(--kpx) * 1px); }
  .klavar-live .klavar-systeem { cursor: pointer; }
  .klavar-stap .klavar-systeem { cursor: crosshair; }
  .k-lijn { stroke: #000; shape-rendering: crispEdges; }
  .k-tel { stroke: #000; }
  .k-maat { stroke: #000; }
  .k-tekst { fill: #333; font-family: Georgia, 'Times New Roman', serif; }
  .k-stok { stroke: #000; stroke-linecap: butt; }
  /* Stemkleuren (0.7.86), alleen op het scherm. */
  .k-stok.k-v2 { stroke: #2a6fdb; }
  .k-stok.k-v3 { stroke: #2e8b57; }
  .k-stok.k-v4 { stroke: #e07b00; }
  @media print { .k-stok.k-v2, .k-stok.k-v3, .k-stok.k-v4 { stroke: #000; } }
  .k-kop { fill: #fff; stroke: #000; }
  .k-kop.zwart { fill: #000; }
  .k-stop { fill: none; stroke: #000; stroke-linejoin: miter; }
  .k-stip { fill: #000; }
  .k-balk { fill: none; stroke: #000; stroke-linejoin: round; stroke-linecap: butt; }
  .k-label { fill: #333; font-family: Georgia, 'Times New Roman', serif; font-style: italic; }
  .k-toonsoort { fill: none; stroke: #000; }
  /* Inspelen: aangehouden toetsen grijs, de nu-lijn in het rood van de opnameknop. */
  .k-open .k-kop { stroke: #888; }
  .k-open .k-kop.zwart { fill: #888; }
  .k-open .k-stok { stroke: #888; }
  .k-nu { stroke: #c0392b; opacity: 0.85; }
  /* Selectie en cursor in het bestaande blauw van de selectie-overlay. */
  .k-noot.selected .k-kop { stroke: rgb(40, 110, 240); stroke-width: 0.45; }
  .k-noot.selected .k-kop.zwart { fill: rgb(40, 110, 240); }
  .k-noot.selected .k-stok { stroke: rgb(40, 110, 240); }
  .k-noot.cursor .k-kop { filter: drop-shadow(0 0 0.6px rgba(40, 110, 240, 0.9)); }

  @media print {
    .k-nu, .k-open { display: none; }
    .klavar-pagina { break-after: page; page-break-after: always; }
    .klavar-pagina:last-child { break-after: auto; page-break-after: auto; }
    .klavar-kop { display: block; }
    .klavar-kolommen { display: flex; gap: 10mm; align-items: flex-start; }
    .klavar-kolom { display: block; }
    .klavar-kolom > .klavar-systeem + .klavar-systeem { margin-top: 0; }
    .klavar-systeem {
      break-inside: avoid; page-break-inside: avoid;
      width: calc(var(--wmm) * 1mm);
      height: calc(var(--hmm) * 1mm);
    }
  }
</style>
