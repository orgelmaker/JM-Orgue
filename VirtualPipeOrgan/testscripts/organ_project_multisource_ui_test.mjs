// Real OrganBuilder + ProjectWindControls, owned headless Chrome and explicit
// Tauri mocks. Source compilation, sample playback and disk safety are covered
// by the separate native background tests, never inferred from this harness.
import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { randomUUID } from 'node:crypto';
import { parseArgs } from 'node:util';
const here = path.dirname(fileURLToPath(import.meta.url));
const ui = path.resolve(here, '../ui');
const { values } = parseArgs({ options: {
  playwright: { type: 'string', default: process.env.PLAYWRIGHT_MODULE },
  chrome: { type: 'string', default: 'C:/Program Files/Google/Chrome/Application/chrome.exe' },
  output: { type: 'string', default: path.resolve(here, '../../testen/orgelbouwer-meerbronnen-2026-11-26/ui') },
} });
const { chromium } = await import(values.playwright ? pathToFileURL(path.resolve(values.playwright)).href : 'playwright');
const { createServer } = await import(pathToFileURL(path.join(ui, 'node_modules/vite/dist/node/index.js')).href);
const output = path.resolve(values.output); await fs.mkdir(output, { recursive: true });
const fileName = `.organ-project-multisource-${randomUUID()}.html`, harnessPath = path.join(ui, fileName);
const projectPath = 'C:/fixture/Projects/Many.jmorgan';
const paths = { reordered: 'C:/fixture/Projects/Reordered.jmorgan', n27: 'C:/fixture/Projects/27.jmorgan', n64: 'C:/fixture/Projects/64.jmorgan', n65: 'C:/fixture/Projects/65.jmorgan' };
const sourcePath = number => `C:/fixture/Sets/Source-${String(number).padStart(3, '0')}.organ`;
const sourceName = number => `Bronorgel ${number} · afzonderlijke opname`;
const stopName = number => `Register bron ${number}`;
const checks = [], pageErrors = [];
const report = name => { checks.push(name); console.log(`OK ${name}`); };
let server, browser, page;
const harness = `<!doctype html><html lang="nl"><head><meta charset="utf-8"><title>Multi-source organ builder</title></head><body><main id="app"><div id="builder" class="console"></div></main><script type="module">
import OrganBuilder from '/src/components/OrganBuilder.svelte';
import {newOrganProject} from '/src/lib/organProject.js';
import '/src/styles.css';
const copy=value=>JSON.parse(JSON.stringify(value));
const paths=${JSON.stringify(paths)};
const sourcePath=number=>'C:/fixture/Sets/Source-'+String(number).padStart(3,'0')+'.organ';
const sourceName=number=>'Bronorgel '+number+' · afzonderlijke opname';
const stopName=number=>'Register bron '+number;
const catalogs=Object.fromEntries(Array.from({length:65},(_,index)=>{const number=index+1,p=sourcePath(number);return[p,{
 path:p,name:sourceName(number),format:'grandorgue',fingerprint:'fingerprint-'+number,supports_original_pitch:true,warnings:[],
 stops:[{id:1,name:stopName(number),manual_id:1,manual_name:'Manuaal I',harmonic_number:8,first_midi_note:36,key_count:61,perspectives:[]}],
}];}));
const fixture=count=>{
 const project=newOrganProject();project.name='Atelier met '+count+' bronnen';
 project.sources=Array.from({length:count},(_,i)=>({id:crypto.randomUUID(),path:sourcePath(i+1),fingerprint:'fingerprint-'+(i+1),package_root:'C:/fixture/Packages/Set-'+(i+1),reference_a4_hz:400+i,tuning_policy:i%2?'original':'normalize',tuning_cents:i/10,perspective:null,resources:[{path:sourcePath(i+1)+'.wav',bytes:100+i,modified_ns:String(100+i)}]}));
 project.stops=project.sources.map((source,index)=>({id:index+1,source_id:source.id,source_stop_id:1,division_id:2,name:null}));
 project.next_stop_id=count+1;return project;
};
window.__test={dialogs:[],calls:[],saved:[],inspectFailures:{},storage:{[paths.n27]:fixture(27),[paths.n64]:fixture(64),[paths.n65]:fixture(65)},loads:[],dirty:false,saving:false,organ:null};
window.__TAURI_INTERNALS__={invoke:async(command,args)=>{
 const s=window.__test;s.calls.push({command,args:copy(args||{})});
 if(command==='plugin:dialog|open'||command==='plugin:dialog|save'){
  if(!s.dialogs.length)throw new Error('Onverwachte testdialoog '+command);return s.dialogs.shift();
 }
 if(command==='inspect_organ_project_source'){
  if(s.inspectFailures[args.path])throw new Error(s.inspectFailures[args.path]);
  if(!catalogs[args.path])throw new Error('Onbekende fixturebron');return copy(catalogs[args.path]);
 }
 if(command==='read_organ_project'){
  if(!s.storage[args.path])throw new Error('Onbekend testproject');return copy(s.storage[args.path]);
 }
 if(command==='save_organ_project'){
  await new Promise(resolve=>setTimeout(resolve,15));
  const saved=copy(args.project);saved.sources.forEach((source,index)=>{if(!source.resources.length)source.resources=[{path:source.path+'.wav',bytes:100+index,modified_ns:String(100+index)}];});
  s.storage[args.path]=copy(saved);s.saved.push({path:args.path,project:copy(saved)});return saved;
 }
 if(command==='get_project_wind_controls'){
  const project=s.storage[args.organId];return{controls:(project?.sources||[]).map((source,index)=>({id:source.id+':continuous:9',source_id:source.id,source_control_id:9,kind:'continuous',name:'Bronklep '+(index+1),default_value:.5,project_stop_ids:[],automatic:false,travel_seconds:.01})),values:{},source_model_requested:true};
 }
 throw new Error('Onverwachte IPC '+command);
}};
let component;
window.__mount=()=>{
 component?.$destroy();component=new OrganBuilder({target:document.getElementById('builder'),props:{organInfo:window.__test.organ}});
 component.$on('dirty',event=>window.__test.dirty=event.detail);
 component.$on('saving',event=>window.__test.saving=event.detail);
 component.$on('load',event=>{const s=window.__test,project=s.storage[event.detail.path];s.loads.push({path:event.detail.path,dirty:s.dirty,saving:s.saving});s.organ={id:event.detail.path,divisions:[{stops:project.stops.map(stop=>({id:'project_'+stop.id,name:stop.name||catalogs[project.sources.find(source=>source.id===stop.source_id).path].stops[0].name}))}]};component.$set({organInfo:s.organ});});
};window.__mount();
</script></body></html>`;

try {
  await fs.writeFile(harnessPath, harness, { flag: 'wx' });
  server = await createServer({ root: ui, configFile: path.join(ui, 'vite.config.js'), logLevel: 'error', server: { host: '127.0.0.1', port: 0, strictPort: false } });
  await server.listen();
  browser = await chromium.launch({ executablePath: values.chrome, headless: true });
  page = await browser.newPage({ viewport: { width: 1440, height: 1000 }, deviceScaleFactor: 1 });
  page.setDefaultTimeout(15_000); page.on('pageerror', error => pageErrors.push(String(error)));
  await page.goto(`http://127.0.0.1:${server.httpServer.address().port}/${fileName}`, { waitUntil: 'networkidle' });
  await page.getByTestId('organ-builder').waitFor();
  const source = number => page.getByTestId(`organ-project-source-${number}`);
  const chosenCards = () => page.locator('.source-card:not(.empty)');
  const queueDialog = value => page.evaluate(value => window.__test.dialogs.push(value), value);
  const saved = () => page.evaluate(() => window.__test.saved.at(-1).project);
  async function save(load = false) {
    const before = await page.evaluate(() => window.__test.saved.length);
    await page.getByTestId(load ? 'organ-project-load' : 'organ-project-save').click();
    await page.waitForFunction(before => window.__test.saved.length > before && !window.__test.saving, before);
    return saved();
  }
  async function selectStop(number) { await page.getByRole('checkbox', { name: `${stopName(number)} kiezen`, exact: true }).check(); }
  async function openProject(project) {
    const previousReads = await page.evaluate(project => window.__test.calls.filter(call => call.command === 'read_organ_project' && call.args.path === project).length, project);
    await queueDialog(project);
    await page.getByRole('button', { name: 'Open project…', exact: true }).click();
    await page.waitForFunction(({ project, previousReads }) => window.__test.calls.filter(call => call.command === 'read_organ_project' && call.args.path === project).length > previousReads && !window.__test.saving, { project, previousReads });
  }
  async function expectDisposition(number, letter) {
    const row = page.getByRole('textbox', { name: `Projectnaam voor ${stopName(number)}`, exact: true }).locator('xpath=ancestor::li');
    assert.equal(await row.locator('.source-dot').textContent(), letter);
  }
  assert.equal(await chosenCards().count(), 0);
  assert.equal(await page.locator('.source-card.empty').count(), 1);
  await queueDialog(sourcePath(1)); await source(1).getByRole('button', { name: 'Kies definitie…', exact: true }).click();
  await source(1).getByRole('heading', { name: sourceName(1), exact: true }).waitFor();
  await selectStop(1); await source(1).getByLabel('Opgenomen A (Hz)').fill('415');
  await queueDialog(projectPath); const one = await save();
  assert.equal(one.sources.length, 1); assert.equal(one.stops.length, 1); assert.equal(one.sources[0].reference_a4_hz, 415);
  assert.equal(await page.locator('.source-card.empty').count(), 1);
  report('one source can be selected and saved; exactly one empty next-source card remains');

  for (const number of [2, 3, 4]) {
    await queueDialog(sourcePath(number));
    await page.getByRole('button', { name: 'Bron toevoegen…', exact: true }).click();
    await source(number).getByRole('heading', { name: sourceName(number), exact: true }).waitFor();
    await selectStop(number);
  }
  await source(3).getByLabel('Stemming', { exact: true }).selectOption('original');
  await source(3).getByLabel('Extra (cent)').fill('-5');
  assert(await source(3).getByLabel('Opgenomen A (Hz)').isDisabled());
  await source(4).getByLabel('Opgenomen A (Hz)').fill('466'); await source(4).getByLabel('Extra (cent)').fill('10');
  assert.equal(await page.locator('.source-tabs button').count(), 4);
  for (const [index, letter] of ['A', 'B', 'C', 'D'].entries()) assert.equal(await source(index + 1).locator('.source-letter').textContent(), letter);
  const four = await save(true);
  assert.equal(four.sources.length, 4); assert.equal(new Set(four.sources.map(item => item.id)).size, 4);
  assert.equal(new Set(four.stops.map(item => item.id)).size, 4); assert(four.stops.every(item => item.source_stop_id === 1));
  assert.equal(four.sources[2].tuning_policy, 'original'); assert.equal(four.sources[2].tuning_cents, -5);
  assert.equal(four.sources[3].reference_a4_hz, 466); assert.equal(four.sources[3].tuning_cents, 10);
  assert(four.sources.every(item => item.resources.length === 1));
  assert.deepEqual(await page.evaluate(() => window.__test.loads.at(-1)), { path: projectPath, dirty: false, saving: false });
  for (let number = 1; number <= 4; number++) await expectDisposition(number, ['A', 'B', 'C', 'D'][number - 1]);
  await page.getByTestId('project-wind-controls').waitFor();
  const fourthWind = page.locator(`[data-control-id="${four.sources[3].id}:continuous:9"] .control-footer small`);
  await fourthWind.filter({ hasText: `D · ${sourceName(4)}` }).waitFor();
  report('three and four sources use separate UUIDs, stable stop IDs, tuning, resources, load guards and wind labels');

  await source(2).getByRole('button', { name: 'Bron 2 verwijderen', exact: true }).click();
  await page.getByText(new RegExp(`Bron “${sourceName(2)}`)).waitFor();
  await page.getByRole('button', { name: 'Doorgaan', exact: true }).click();
  assert.equal(await chosenCards().count(), 3);
  await queueDialog(sourcePath(5)); await page.getByRole('button', { name: 'Bron toevoegen…', exact: true }).click();
  await source(4).getByRole('heading', { name: sourceName(5), exact: true }).waitFor(); await selectStop(5);
  const replaced = await save(true);
  assert.deepEqual(replaced.sources.slice(0, 3).map(item => item.id), [four.sources[0].id, four.sources[2].id, four.sources[3].id]);
  assert(!four.sources.some(item => item.id === replaced.sources[3].id));
  assert(!replaced.stops.some(item => item.source_id === four.sources[1].id));
  assert(replaced.stops.find(item => item.source_id === replaced.sources[3].id).id > Math.max(...four.stops.map(item => item.id)));
  assert.equal(replaced.next_stop_id, 6);
  for (const [number, letter] of [[1, 'A'], [3, 'B'], [4, 'C'], [5, 'D']]) await expectDisposition(number, letter);
  await page.locator(`[data-control-id="${replaced.sources[1].id}:continuous:9"] .control-footer small`).filter({ hasText: `B · ${sourceName(3)}` }).waitFor();
  report('removing a middle source preserves surviving identities; adding a source never reuses removed register IDs');

  await page.evaluate(({ projectPath, reordered }) => { const project = structuredClone(window.__test.storage[projectPath]); project.sources = [project.sources[3], project.sources[0], project.sources[2], project.sources[1]]; window.__test.storage[reordered] = project; }, { projectPath, reordered: paths.reordered });
  await openProject(paths.reordered);
  for (const [index, number] of [5, 1, 4, 3].entries()) assert.match(await source(index + 1).locator('h2').textContent(), new RegExp(sourceName(number)));
  for (const [number, letter] of [[5, 'A'], [1, 'B'], [4, 'C'], [3, 'D']]) await expectDisposition(number, letter);
  const reordered = await save(true);
  assert.deepEqual(new Set(reordered.sources.map(item => item.id)), new Set(replaced.sources.map(item => item.id)));
  assert.deepEqual(new Set(reordered.stops.map(item => item.id)), new Set(replaced.stops.map(item => item.id)));
  await page.locator(`[data-control-id="${reordered.sources[3].id}:continuous:9"] .control-footer small`).filter({ hasText: `D · ${sourceName(3)}` }).waitFor();
  report('reordered projects retain every UUID/stop ID and update cards, disposition and wind labels');

  await page.evaluate(sourcePath => window.__test.inspectFailures[sourcePath] = 'Deze testbron is tijdelijk onleesbaar', sourcePath(4));
  await source(3).getByRole('button', { name: 'Controleer', exact: true }).click();
  await source(3).getByText('Deze testbron is tijdelijk onleesbaar', { exact: true }).waitFor();
  assert(await page.getByTestId('organ-project-save').isDisabled());
  await source(3).getByRole('button', { name: 'Bron 3 verwijderen', exact: true }).click();
  await page.getByRole('button', { name: 'Doorgaan', exact: true }).click();
  assert(!(await page.getByTestId('organ-project-save').isDisabled()));
  const withoutFailed = await save();
  assert.equal(withoutFailed.sources.length, 3);
  assert(!withoutFailed.sources.some(item => item.path === sourcePath(4)));
  assert(!withoutFailed.stops.some(item => item.source_id === reordered.sources[2].id));
  await page.evaluate(({ fixturePath, fixture }) => { window.__test.inspectFailures = {}; window.__test.storage[fixturePath] = fixture; }, { fixturePath: paths.reordered, fixture: reordered });
  await openProject(paths.reordered);
  report('removing an unreadable source clears its error and restores safe project saving');

  for (const count of [27, 64]) {
    const fixturePath = paths[`n${count}`];
    const before = await page.evaluate(() => window.__test.calls.length);
    await openProject(fixturePath);
    assert.equal(await chosenCards().count(), count);
    assert.equal(await source(26).locator('.source-letter').textContent(), 'Z');
    assert.equal(await source(27).locator('.source-letter').textContent(), 'AA');
    if (count === 64) assert.equal(await source(64).locator('.source-letter').textContent(), 'BL');
    assert.equal(await page.locator('.source-tabs button').count(), 0);
    const picker = page.getByRole('combobox', { name: 'Bron voor registerkeuze', exact: true });
    assert.equal(await picker.locator('option').count(), count);
    const fixture = await page.evaluate(fixturePath => window.__test.storage[fixturePath], fixturePath);
    await picker.selectOption(fixture.sources.at(-1).id);
    assert(await page.getByRole('checkbox', { name: `${stopName(count)} kiezen`, exact: true }).isChecked());
    await expectDisposition(count, count === 27 ? 'AA' : 'BL');
    const inspections = await page.evaluate(before => window.__test.calls.slice(before).filter(call => call.command === 'inspect_organ_project_source'), before);
    assert.equal(inspections.length, count);
    for (let number = 1; number <= count; number++) assert(inspections.some(call => call.args.path === sourcePath(number) && call.args.packageRoot === `C:/fixture/Packages/Set-${number}`));
    const persisted = await save();
    assert.equal(persisted.sources.length, count); assert.equal(persisted.stops.length, count);
    assert.deepEqual(persisted.sources, fixture.sources); assert.deepEqual(persisted.stops, fixture.stops);
    assert.equal(await page.locator('.source-card.empty').count(), count === 64 ? 0 : 1);
    assert.equal(await page.getByRole('button', { name: 'Bron toevoegen…', exact: true }).isDisabled(), count === 64);
    report(`${count} source project inspects and persists every source; labels and register picker extend beyond Z`);
  }

  const before65 = await page.evaluate(() => window.__test.calls.length);
  const savesBefore65 = await page.evaluate(() => window.__test.saved.length);
  await openProject(paths.n65);
  await page.locator('.notice.error').filter({ hasText: /64/ }).waitFor();
  assert.equal(await chosenCards().count(), 64, 'invalid oversized project must leave the previous project intact');
  assert.equal(await page.evaluate(() => window.__test.saved.length), savesBefore65);
  assert.equal((await page.evaluate(before => window.__test.calls.slice(before).filter(call => call.command === 'inspect_organ_project_source'), before65)).length, 0);
  report('unexpected 65-source payload is rejected before inspection or mutation, preserving the previous project');

  await openProject(paths.reordered);
  await page.getByTestId('organ-project-load').click();
  await page.waitForFunction(path => window.__test.loads.at(-1)?.path === path && !window.__test.saving, paths.reordered);
  await page.getByTestId('project-wind-controls').waitFor();
  for (const width of [1280, 1024, 600, 420]) {
    await page.setViewportSize({ width, height: 900 });
    const sizes = await page.getByTestId('organ-builder').evaluate(element => ({ ownWidth: element.clientWidth, ownScroll: element.scrollWidth, viewport: innerWidth, pageScroll: document.documentElement.scrollWidth }));
    assert(sizes.ownScroll <= sizes.ownWidth + 1 && sizes.pageScroll <= sizes.viewport + 1, JSON.stringify({ width, sizes }));
  }
  await page.getByTestId('organ-builder').evaluate(element => element.scrollTop = 0);
  await page.screenshot({ path: path.join(output, 'multisource-narrow.png') });
  await page.setViewportSize({ width: 1440, height: 1000 });
  await page.getByTestId('organ-builder').evaluate(element => element.scrollTop = 0);
  await page.screenshot({ path: path.join(output, 'multisource-wide.png') });
  report('four-source builder and human wind labels remain readable without horizontal overflow down to 420 px');
  assert.deepEqual(pageErrors, []);
  assert.equal(await page.evaluate(() => window.__test.dialogs.length), 0);
  await fs.writeFile(path.join(output, 'result.json'), JSON.stringify({ headless: true, mocked_ipc: true, checks, page_errors: pageErrors }, null, 2));
  console.log(JSON.stringify({ checks, page_errors: pageErrors }, null, 2));
} catch (error) {
  if (page) await page.screenshot({ path: path.join(output, 'failure.png') }).catch(() => {});
  await fs.writeFile(path.join(output, 'result.json'), JSON.stringify({ headless: true, mocked_ipc: true, checks, page_errors: pageErrors, failure: String(error) }, null, 2));
  throw error;
} finally {
  if (browser) await browser.close();
  if (server) await server.close();
  await fs.unlink(harnessPath).catch(error => { if (error.code !== 'ENOENT') throw error; });
}
