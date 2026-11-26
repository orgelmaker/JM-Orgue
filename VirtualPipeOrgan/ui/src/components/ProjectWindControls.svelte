<script>
  import { onMount, onDestroy, tick } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  export let organId = '';
  export let sourceNames = {};
  export let stopNames = {};
  let controls = [];
  let values = {};
  let drafts = {};
  let sourceModel = true;
  let error = '';
  let warning = '';
  let busy = false;
  let alive = true;
  let request = 0;
  let observedId = '';
  $: if (organId !== observedId) {
    observedId = organId; controls = []; values = {}; drafts = {}; error = ''; warning = '';
    refresh();
  }
  async function refresh() {
    if (!organId || busy) return;
    const id = organId;
    const token = ++request;
    try {
      const result = await invoke('get_project_wind_controls', { organId: id });
      if (!alive || organId !== id || token !== request) return;
      controls = result.controls || []; values = result.values || {}; sourceModel = result.source_model_requested;
    } catch (e) {
      if (alive && organId === id && token === request) error = String(e);
    }
  }
  async function change(control, value) {
    if (busy || control.automatic) return;
    const id = organId;
    busy = true; ++request; error = ''; warning = '';
    drafts = { ...drafts, [control.id]: Number(value) };
    try {
      // Record the native input change in Svelte before any rejected request
      // rolls it back to an unchanged authoritative value.
      await tick();
      if (!alive || organId !== id) return;
      const result = await invoke('set_project_wind_control', { organId: id, controlId: control.id, value: Number(value) });
      if (!alive || organId !== id) return;
      values = { ...values, [control.id]: Number(value) };
      warning = result?.warning || '';
    } catch (e) { if (alive && organId === id) error = String(e); }
    finally {
      if (alive && organId === id) { drafts = {}; busy = false; refresh(); }
      else busy = false;
    }
  }
  onMount(() => { const timer = setInterval(refresh, 1000); return () => clearInterval(timer); });
  onDestroy(() => { alive = false; ++request; });
  const percent = value => Math.round(Number(value || 0) * 100);
  const sourceName = control => sourceNames[control.source_id] || `Bron ${[...new Set(controls.map(item => item.source_id))].indexOf(control.source_id) + 1}`;
</script>

{#if controls.length}
  <section class="wind-controls" aria-label="Bronwindkleppen" data-testid="project-wind-controls">
    <div class="heading"><div><span class="eyebrow">WINDVOORZIENING</span><h2>Bronwindkleppen</h2></div><span class="count">{controls.length} bedieningen</span></div>
    <p>Registerkleppen volgen hun projectregister. Automatische regelaars volgen de bronbalg en besturingsketen. Zelfstandige klepstanden worden bij dit orgel bewaard.</p>
    {#if !sourceModel}<p class="disabled-model">‘Windwerk van de set’ staat uit. Activeer dit in de orgelinstellingen en laad het orgel opnieuw om de kleppen te horen.</p>{/if}
    <div class="controls">
      {#each controls as control (control.id)}
        {@const value = drafts[control.id] ?? values[control.id] ?? control.default_value}
        {@const linked = !!control.project_stop_ids?.length}
        <article data-control-id={control.id}>
          <div class="control-title"><strong>{control.name}</strong><span>{control.kind === 'switch' ? 'Schakelaar' : 'Regelklep'}</span></div>
          {#if control.automatic}
            <div class="automatic"><span>Automatische bronregeling</span><div class="automatic-value"><progress aria-label={control.name} max="1" value={value}></progress><output>{control.kind === 'switch' ? value >= .5 ? 'Aan' : 'Uit' : `${percent(value)}%`}</output></div></div>
          {:else if linked}
            <div class="linked"><span class:engaged={value >= .5} class="lamp"></span>{value >= .5 ? 'Ingeschakeld' : 'Uitgeschakeld'}<small title={control.project_stop_ids.map(id => stopNames[id] || 'Projectregister').join(', ')}>Volgt {control.project_stop_ids.length === 1 ? stopNames[control.project_stop_ids[0]] || 'projectregister' : `${control.project_stop_ids.length} projectregisters`}</small></div>
          {:else if control.kind === 'switch'}
            <label class="switch"><input type="checkbox" aria-label={control.name} checked={value >= .5} disabled={busy} on:change={e => change(control, e.target.checked ? 1 : 0)}> Ingeschakeld</label>
          {:else}
            <label class="range"><span class="sr-only">{control.name}</span><input type="range" aria-label={control.name} min="0" max="1" step="0.01" value={value} disabled={busy} on:input={e => { drafts = { ...drafts, [control.id]: Number(e.target.value) }; }} on:change={e => change(control, e.target.value)}><output>{percent(value)}%</output></label>
          {/if}
          <div class="control-footer"><small>{sourceName(control)}</small>{#if !linked && !control.automatic}<button type="button" disabled={busy} on:click={() => change(control, control.default_value)}>Beginstand</button>{/if}</div>
        </article>
      {/each}
    </div>
    <p class="hint">De stand bedient de bronregelaar; een klep kan in de brondefinitie omgekeerd werken.</p>
    {#if error}<p class="error" role="alert">{error}</p>{/if}
    {#if warning}<p class="disabled-model" role="status">{warning}</p>{/if}
  </section>
{:else if error}<p role="alert" class="error">{error}</p>{/if}

<style>
  .wind-controls{margin:18px 0;padding:17px 18px;background:var(--bg-panel);border:1px solid var(--stop-porcelain-border);border-radius:10px}
  .heading{display:flex;align-items:center;justify-content:space-between;gap:12px}.eyebrow{font-size:9px;letter-spacing:.14em;color:var(--primary);font-weight:600}h2{margin:5px 0 0;font:500 20px/1.3 Georgia,serif;color:var(--text)}.count,small{font-size:10px;color:var(--text-muted)}p{font-size:11px;line-height:1.7;color:var(--text-secondary);margin:10px 0}
  .controls{display:grid;grid-template-columns:repeat(auto-fit,minmax(240px,1fr));gap:12px;margin-top:14px}article{min-width:0;padding:13px;background:var(--bg-dark);border:1px solid var(--stop-porcelain-border);border-radius:7px}.control-title{display:flex;align-items:center;justify-content:space-between;gap:10px;margin-bottom:15px}.control-title strong{font-size:12px;font-weight:500;overflow-wrap:anywhere}.control-title>span{font-size:9px;color:var(--text-muted);white-space:nowrap}
  .switch,.range,.linked{display:flex;align-items:center;gap:8px;font-size:11px;color:var(--text-secondary);min-height:25px}.range input{flex:1;min-width:0;accent-color:var(--primary)}input[type=checkbox]{accent-color:var(--primary);width:17px;height:17px}.range output{width:34px;text-align:right;font-variant-numeric:tabular-nums}.linked{flex-wrap:wrap}.linked small{margin-left:auto}.lamp{width:8px;height:8px;border-radius:50%;background:var(--stop-porcelain-border)}.lamp.engaged{background:var(--primary);box-shadow:0 0 6px var(--accent-soft)}
  .automatic{font-size:10px;color:var(--text-muted)}.automatic-value{display:flex;gap:9px;align-items:center;margin-top:7px}.automatic-value progress{width:100%;min-width:0;flex:1;height:7px;accent-color:var(--primary)}.automatic-value output{font-size:11px;color:var(--text-secondary);font-variant-numeric:tabular-nums;min-width:32px;text-align:right}
  .control-footer{display:flex;align-items:center;justify-content:space-between;gap:8px;margin-top:12px}.control-footer small{overflow-wrap:anywhere;min-width:0}.linked small{overflow-wrap:anywhere;min-width:0}.control-footer button{font:inherit;font-size:10px;cursor:pointer;color:var(--primary);border:1px solid var(--stop-porcelain-border);border-radius:4px;padding:4px 7px;background:var(--bg-elevated);flex-shrink:0}button:disabled{opacity:.5;cursor:default}.hint{font-size:10px;margin-bottom:0}.disabled-model{border-left:3px solid var(--warning);padding-left:10px}.error{color:var(--error);font-size:11px}.sr-only{position:absolute;width:1px;height:1px;padding:0;margin:-1px;overflow:hidden;clip:rect(0,0,0,0);white-space:nowrap;border:0}
  @media(max-width:600px){.controls{grid-template-columns:1fr}.count{display:none}}
</style>
