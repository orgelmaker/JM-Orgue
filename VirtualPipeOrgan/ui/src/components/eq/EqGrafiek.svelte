<script>
  // Equalizergrafiek (0.7.79): de samengestelde curve 20 Hz–20 kHz met
  // genummerde, sleepbare punten. Rekent met eqCurve.js (bewezen gelijk aan de
  // audiothread). Kent Tauri niet: de eigenaar krijgt `change` (detail
  // { live }) met gemuteerde bandobjecten, `select`, `add` ({ freq, gain_db })
  // en `remove` (index). Tijdens slepen hooguit ~20 `change`-events per
  // seconde (live: true) en één definitieve bij loslaten (live: false).
  import { createEventDispatcher, onDestroy } from 'svelte';
  import { t } from '../../lib/i18n.js';
  import { compositeCurve, bandDb, logFreqs, qToBw, bwToQ, EQ_MAX_BANDEN } from '../../lib/eqCurve.js';

  export let bands = [];
  export let selected = -1;
  export let sampleRate = 48000;
  export let preampDb = 0;
  export let enabled = true;
  export let channelView = null;   // null = "Alle", anders kanaalindex
  export let unit = 'oct';

  const dispatch = createEventDispatcher();
  const ML = 36, MR = 12, MT = 10, MB = 22;
  const FREQS = logFreqs(256);
  const RASTER = [20, 50, 100, 200, 500, 1000, 2000, 5000, 10000, 20000];

  let breedte = 600;
  let slepen = null;   // { idx, startX, startY, f0, g0, q0, dbPerPx, bewogen }
  $: hoogte = Math.max(180, Math.min(320, Math.round(breedte * 0.38)));
  $: binnenB = Math.max(50, breedte - ML - MR);
  $: binnenH = Math.max(50, hoogte - MT - MB);

  // y-as springt mee: ±12 standaard, ±20 boven 11 dB, ±24 boven 19 dB — maar
  // niet tijdens een sleep (reviewbevinding: anders verandert de schaal onder
  // de cursor en springt de gain; de as volgt pas bij loslaten).
  $: maxGain = Math.max(0, ...(bands || []).map((b) => Math.abs(Number(b.gain_db) || 0)), Math.abs(preampDb || 0));
  let bereik = 12;
  $: if (!slepen) bereik = maxGain > 19 ? 24 : maxGain > 11 ? 20 : 12;

  // Reactief ($:), niet const: Svelte neemt een const-closure niet mee als
  // afhankelijkheid, zodat raster, curve en punten na de breedtemeting en bij
  // een asverandering op de oude coördinaten bleven staan (reviewbevinding).
  $: xVan = (f) => ML + (Math.log10(Math.max(20, Math.min(20000, f)) / 20) / 3) * binnenB;
  $: fVan = (x) => 20 * Math.pow(10, 3 * Math.max(0, Math.min(1, (x - ML) / binnenB)));
  $: yVan = (db) => MT + (1 - (Math.max(-bereik, Math.min(bereik, db)) + bereik) / (2 * bereik)) * binnenH;
  $: dbVan = (y) => -bereik + (1 - Math.max(0, Math.min(1, (y - MT) / binnenH))) * 2 * bereik;

  // Curve zonder en met voorversterking (de tweede alleen als die ≠ 0).
  $: curve = compositeCurve(bands, FREQS, sampleRate, channelView, 0);
  $: curveMetPre = Math.abs(preampDb || 0) > 0.05 ? curve.map((v) => v + preampDb) : null;
  $: pad = curve.map((v, i) => `${i ? 'L' : 'M'}${xVan(FREQS[i]).toFixed(1)},${yVan(v).toFixed(1)}`).join('');
  $: padVul = pad + `L${xVan(20000).toFixed(1)},${yVan(0).toFixed(1)}L${xVan(20).toFixed(1)},${yVan(0).toFixed(1)}Z`;
  $: padPre = curveMetPre ? curveMetPre.map((v, i) => `${i ? 'L' : 'M'}${xVan(FREQS[i]).toFixed(1)},${yVan(v).toFixed(1)}`).join('') : '';
  $: gekozen = bands && selected >= 0 && selected < bands.length ? bands[selected] : null;
  $: padGekozen = gekozen && gekozen.enabled
    ? FREQS.map((f, i) => `${i ? 'L' : 'M'}${xVan(f).toFixed(1)},${yVan(bandDb(gekozen, f, sampleRate)).toFixed(1)}`).join('')
    : '';
  $: dbLijnen = bereik === 12 ? [-12, -6, 0, 6, 12] : bereik === 20 ? [-20, -10, 0, 10, 20] : [-24, -12, 0, 12, 24];

  const heeftGain = (b) => b.band_type === 'peak' || b.band_type === 'lowshelf' || b.band_type === 'highshelf';
  $: puntY = (b) => yVan(heeftGain(b) ? Number(b.gain_db) || 0 : 0);
  const bandQ = (b) => (b.q != null ? Number(b.q) : bwToQ(Number(b.bandwidth) || 1));
  const isShelf = (b) => b.band_type === 'lowshelf' || b.band_type === 'highshelf';

  function fmtF(f) { return f >= 1000 ? `${(f / 1000).toFixed(f >= 10000 ? 0 : 1)}k` : `${Math.round(f)}`; }
  function fmtTip(b) {
    const q = bandQ(b);
    const breed = unit === 'q' || isShelf(b) ? `Q ${q.toFixed(2)}` : `${qToBw(q).toFixed(2)} oct`;
    return `${fmtF(Number(b.freq))} Hz · ${heeftGain(b) ? `${b.gain_db > 0 ? '+' : ''}${Number(b.gain_db).toFixed(1)} dB · ` : ''}${breed}`;
  }

  // ---- slepen ----
  let svg;
  let tip = null;
  let laatsteLive = 0;
  let tikTijd = 0, tikIdx = -2;

  function pos(e) {
    const r = svg.getBoundingClientRect();
    return { x: e.clientX - r.left, y: e.clientY - r.top };
  }
  function meld(live) {
    const nu = performance.now();
    if (live && nu - laatsteLive < 50) return;   // ≤ 20/s tijdens slepen
    laatsteLive = nu;
    dispatch('change', { live });
  }
  function zetQ(b, q) {
    const lo = isShelf(b) ? 0.3 : 0.05, hi = isShelf(b) ? 2 : 20;
    const qq = Math.max(lo, Math.min(hi, q));
    b.q = qq; b.bandwidth = qToBw(qq);
  }
  function puntDown(e, idx) {
    if (!enabled) return;
    e.preventDefault();
    // preventDefault op pointerdown houdt de automatische focus tegen; de
    // toetsbediening hangt aan de svg, dus zelf focus geven.
    try { svg.focus({ preventScroll: true }); } catch (err) {}
    const b = bands[idx];
    selected = idx;
    dispatch('select', idx);
    const p = pos(e);
    slepen = { idx, startX: p.x, startY: p.y, f0: Number(b.freq) || 1000, g0: Number(b.gain_db) || 0, q0: bandQ(b),
               dbPerPx: (2 * bereik) / binnenH, bewogen: false, pointerId: e.pointerId };
    try { svg.setPointerCapture(e.pointerId); } catch (err) {}
    tip = { x: xVan(b.freq), y: puntY(b), tekst: fmtTip(b) };
  }
  function beweeg(e) {
    if (!slepen) return;
    const b = bands[slepen.idx];
    if (!b) return;
    const p = pos(e);
    const dx = p.x - slepen.startX, dy = p.y - slepen.startY;
    if (!slepen.bewogen && Math.abs(dx) < 3 && Math.abs(dy) < 3) return;
    slepen.bewogen = true;
    if (e.shiftKey) {
      // Verticaal slepen met Shift = breedte (Q), 80 px per factor 2.
      zetQ(b, slepen.q0 * Math.pow(2, -dy / 80));
    } else {
      const fijn = e.altKey ? 0.25 : 1;
      b.freq = Math.round(Math.max(20, Math.min(20000, fVan(xVan(slepen.f0) + dx * fijn))) * 10) / 10;
      if (heeftGain(b)) {
        // Vaste dB-per-pixel voor de hele sleep (de as is intussen bevroren).
        const g = slepen.g0 - dy * fijn * slepen.dbPerPx;
        b.gain_db = Math.round(Math.max(-24, Math.min(24, g)) * 10) / 10;
      }
    }
    bands = bands;
    tip = { x: xVan(b.freq), y: puntY(b), tekst: fmtTip(b) };
    meld(true);
  }
  function los(e) {
    if (!slepen) return;
    const s = slepen; slepen = null; tip = null;
    try { svg.releasePointerCapture(s.pointerId); } catch (err) {}
    if (s.bewogen) { laatsteLive = 0; meld(false); return; }
    // Niet bewogen = tik: twee tikken binnen 350 ms op hetzelfde punt → gain 0.
    const nu = performance.now();
    if (tikIdx === s.idx && nu - tikTijd < 350) {
      const b = bands[s.idx];
      if (b && heeftGain(b)) { b.gain_db = 0; bands = bands; laatsteLive = 0; meld(false); }
      tikIdx = -2;
    } else { tikIdx = s.idx; tikTijd = nu; }
  }
  function leegTik(e) {
    // Dubbeltik op een leeg stuk = nieuwe peak-band daar (max. 32).
    if (!enabled || slepen) return;
    const nu = performance.now();
    const p = pos(e);
    if (tikIdx === -1 && nu - tikTijd < 350) {
      tikIdx = -2;
      if ((bands || []).length < EQ_MAX_BANDEN) dispatch('add', { freq: Math.round(fVan(p.x)), gain_db: Math.round(dbVan(p.y) * 2) / 2 });
    } else { tikIdx = -1; tikTijd = nu; }
  }
  function wiel(e, idx) {
    if (!enabled) return;
    const b = bands[idx ?? selected];
    if (!b) return;
    e.preventDefault();
    if (idx != null) { selected = idx; dispatch('select', idx); }
    const stap = e.deltaY > 0 ? -1 : 1;
    if (e.ctrlKey) {
      if (heeftGain(b)) b.gain_db = Math.round(Math.max(-24, Math.min(24, (Number(b.gain_db) || 0) + 0.5 * stap)) * 10) / 10;
    } else {
      zetQ(b, bandQ(b) * Math.pow(2, stap / 4));
    }
    bands = bands;
    laatsteLive = 0; meld(false);
  }
  function toets(e) {
    if (!enabled) return;
    const n = (bands || []).length;
    if (e.key >= '1' && e.key <= '9') { const i = Number(e.key) - 1; if (i < n) { selected = i; dispatch('select', i); } e.preventDefault(); return; }
    if ((e.key === 'PageDown' || e.key === 'PageUp') && n) { selected = ((selected + (e.key === 'PageUp' ? -1 : 1)) + n) % n; dispatch('select', selected); e.preventDefault(); return; }
    const b = bands[selected];
    if (!b) return;
    let gedaan = true;
    const oct = e.shiftKey ? 0.25 : 1 / 24, dbStap = e.shiftKey ? 2 : 0.5;
    switch (e.key) {
      case 'ArrowLeft': b.freq = Math.round(Math.max(20, b.freq * Math.pow(2, -oct)) * 10) / 10; break;
      case 'ArrowRight': b.freq = Math.round(Math.min(20000, b.freq * Math.pow(2, oct)) * 10) / 10; break;
      case 'ArrowUp': if (heeftGain(b)) b.gain_db = Math.min(24, Math.round(((Number(b.gain_db) || 0) + dbStap) * 10) / 10); break;
      case 'ArrowDown': if (heeftGain(b)) b.gain_db = Math.max(-24, Math.round(((Number(b.gain_db) || 0) - dbStap) * 10) / 10); break;
      case '[': zetQ(b, bandQ(b) / Math.pow(2, 0.25)); break;
      case ']': zetQ(b, bandQ(b) * Math.pow(2, 0.25)); break;
      case '0': if (heeftGain(b)) b.gain_db = 0; break;
      case 'e': case 'E': b.enabled = !b.enabled; break;
      case 'Delete': case 'Backspace': dispatch('remove', selected); e.preventDefault(); return;
      default: gedaan = false;
    }
    if (!gedaan) return;
    e.preventDefault();
    bands = bands;
    laatsteLive = 0; meld(false);
  }
  onDestroy(() => { slepen = null; });
</script>

<div class="eq-graph-wrap" bind:clientWidth={breedte}>
  <!-- svelte-ignore a11y-no-noninteractive-tabindex a11y-no-noninteractive-element-interactions -->
  <svg bind:this={svg} class="eq-graph" class:uit={!enabled} width={breedte} height={hoogte} viewBox="0 0 {breedte} {hoogte}"
    tabindex="0" role="application" aria-label={$t('eq.graph_help')}
    on:pointermove={beweeg} on:pointerup={los} on:pointercancel={los} on:keydown={toets}
    on:wheel|nonpassive={(e) => { if (document.activeElement === svg && selected >= 0 && (e.target === svg || e.target.classList?.contains('eq-vlak'))) wiel(e, null); }}>
    <!-- raster -->
    {#each dbLijnen as d}
      <line x1={ML} x2={breedte - MR} y1={yVan(d)} y2={yVan(d)} class="eq-lijn" class:nul={d === 0} />
      <text x={ML - 4} y={yVan(d) + 3} class="eq-as" text-anchor="end">{d > 0 ? '+' : ''}{d}</text>
    {/each}
    {#each RASTER as f}
      <line x1={xVan(f)} x2={xVan(f)} y1={MT} y2={MT + binnenH} class="eq-lijn" class:zwak={![100, 1000, 10000].includes(f)} />
      <text x={xVan(f)} y={hoogte - 6} class="eq-as" text-anchor="middle">{fmtF(f)}</text>
    {/each}
    <!-- klikvlak voor dubbeltik = nieuwe band -->
    <rect class="eq-vlak" x={ML} y={MT} width={binnenB} height={binnenH} fill="transparent" on:pointerup={leegTik} />
    <!-- curve -->
    <path d={padVul} class="eq-vul" />
    <path d={pad} class="eq-curve" />
    {#if padPre}<path d={padPre} class="eq-curve-pre" />{/if}
    {#if padGekozen}<path d={padGekozen} class="eq-curve-band" />{/if}
    <!-- punten -->
    {#each bands as b, i (i)}
      {@const cx = xVan(Number(b.freq) || 1000)}
      {@const cy = puntY(b)}
      <g class="eq-punt" class:gekozen={i === selected} class:bandUit={!b.enabled}
        on:pointerdown={(e) => puntDown(e, i)} on:wheel|nonpassive={(e) => wiel(e, i)}
        role="button" tabindex="-1" aria-label={$t('eq.aria_band').replace('{n}', i + 1).replace('{info}', fmtTip(b))}>
        <circle class="eq-hit" {cx} {cy} r="18" />
        <circle class="eq-bol" {cx} {cy} r="9" />
        <text x={cx} y={cy + 3.5} class="eq-nr" text-anchor="middle">{i + 1}</text>
        {#if b.channel != null}<text x={cx + 9} y={cy - 7} class="eq-kanaal">{Number(b.channel) + 1}</text>{/if}
      </g>
    {/each}
    {#if tip}
      <g class="eq-tip" transform="translate({Math.min(breedte - 120, Math.max(ML, tip.x - 55))},{tip.y < MT + 40 ? tip.y + 16 : tip.y - 34})">
        <rect width="118" height="20" rx="4" />
        <text x="59" y="14" text-anchor="middle">{tip.tekst}</text>
      </g>
    {/if}
  </svg>
</div>

<style>
  .eq-graph-wrap { width: 100%; }
  .eq-graph { display: block; touch-action: none; user-select: none; border: 1px solid var(--accent-soft-2); border-radius: var(--radius-sm); background: var(--bg-darkest); outline: none; }
  .eq-graph:focus-visible { box-shadow: 0 0 0 2px var(--gold-border); }
  .eq-graph.uit { opacity: 0.45; }
  .eq-lijn { stroke: var(--accent-soft-2); stroke-width: 1; }
  .eq-lijn.zwak { opacity: 0.45; }
  .eq-lijn.nul { stroke: var(--text-secondary, var(--text-muted)); }
  .eq-as { fill: var(--text-muted); font-size: 9px; }
  .eq-vul { fill: var(--primary); opacity: 0.18; }
  .eq-curve { fill: none; stroke: var(--primary); stroke-width: 2; }
  .eq-curve-pre { fill: none; stroke: var(--text-muted); stroke-width: 1; stroke-dasharray: 3 3; }
  .eq-curve-band { fill: none; stroke: var(--led-blue, #4a90d9); stroke-width: 1; stroke-dasharray: 4 3; }
  .eq-punt { cursor: grab; }
  .eq-hit { fill: transparent; }
  .eq-bol { fill: var(--bg-elevated); stroke: var(--primary); stroke-width: 1.5; }
  .eq-punt.gekozen .eq-bol { fill: var(--primary); stroke: var(--gold-border); stroke-width: 2; }
  .eq-punt.bandUit .eq-bol { stroke-dasharray: 2 2; opacity: 0.5; }
  .eq-nr { fill: var(--text); font-size: 10px; font-weight: 700; pointer-events: none; }
  .eq-punt.gekozen .eq-nr { fill: var(--bg-darkest); }
  .eq-kanaal { fill: var(--text-muted); font-size: 8px; pointer-events: none; }
  .eq-tip rect { fill: var(--bg-elevated); stroke: var(--accent-soft-2); }
  .eq-tip text { fill: var(--text); font-size: 10px; }
  @media (pointer: coarse) {
    .eq-bol { r: 13; }
    .eq-hit { r: 26; }
    .eq-nr { font-size: 12px; }
  }
</style>
