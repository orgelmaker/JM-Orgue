// The real Svelte component in an owned, headless Chrome, with explicit Tauri
// mocks. Pneumatic integration, audio and persistence require the separate app
// background test; this test covers UI routing and asynchronous UI behavior.
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
  output: { type: 'string', default: path.resolve(here, '../../testen/orgelbouwer-kleppen-2026-11-26/ui') },
} });
const { chromium } = await import(values.playwright ? pathToFileURL(path.resolve(values.playwright)).href : 'playwright');
const { createServer } = await import(pathToFileURL(path.join(ui, 'node_modules/vite/dist/node/index.js')).href);
const output = path.resolve(values.output);
await fs.mkdir(output, { recursive: true });
const fileName = `.project-wind-controls-test-${randomUUID()}.html`;
const harnessPath = path.join(ui, fileName);
const checks = [], pageErrors = [];
const report = name => { checks.push(name); console.log(`OK ${name}`); };
let server, browser, page;
const organA = 'C:/fixtures/Dual.jmorgan';
const organB = 'C:/fixtures/Other.jmorgan';
const ids = {
  switchA: 'source-a-uuid:switch:7', switchB: 'source-b-uuid:switch:7',
  continuousA: 'source-a-uuid:continuous:9', continuousB: 'source-b-uuid:continuous:9',
  linkedA: 'source-a-uuid:switch:8', linkedB: 'source-b-uuid:switch:8',
  automaticA: 'source-a-uuid:continuous:301',
  other: 'source-c-uuid:switch:7',
};
const sourceNames = {
  'source-a-uuid': 'Bron A · Atelierorgel Sint-Martinus',
  'source-b-uuid': 'Bron B · Kathedraalorgel Notre-Dame',
  'source-c-uuid': 'Bron A · Nieuw orgel',
};
const stopNames = { 101: 'Prestant 8′ · Hoofdwerk', 201: 'Gedekt 8′', 202: 'Fluit 4′' };
const harness = `<!doctype html><html lang="nl"><head><meta charset="utf-8"><title>Isolated source wind controls</title>
<style>#host{padding:20px;width:100%;box-sizing:border-box;overflow:auto;flex:1;min-height:0}</style></head><body><main id="app"><div id="host"></div></main><script type="module">
import ProjectWindControls from '/src/components/ProjectWindControls.svelte';
import '/src/styles.css';
const copy=value=>JSON.parse(JSON.stringify(value));
const first=${JSON.stringify(organA)}, second=${JSON.stringify(organB)};
const ids=${JSON.stringify(ids)};
const sourceNames=${JSON.stringify(sourceNames)}, stopNames=${JSON.stringify(stopNames)};
const control=(id,source_id,kind,name,default_value,project_stop_ids=[])=>({id,source_id,source_control_id:kind==='continuous'?9:project_stop_ids.length?8:7,kind,name,default_value,project_stop_ids,travel_seconds:.01});
const controls=[
 control(ids.switchA,'source-a-uuid','switch','Ventilator A · zelfstandige schakelaar',0),
 control(ids.switchB,'source-b-uuid','switch','Ventilator B · zelfstandige schakelaar',1),
 control(ids.continuousA,'source-a-uuid','continuous','Regelklep A · aanvoer',.75),
 control(ids.continuousB,'source-b-uuid','continuous','Regelklep B · aanvoer',.25),
 control(ids.linkedA,'source-a-uuid','switch','Registertoevoer A',0,[101]),
 control(ids.linkedB,'source-b-uuid','switch','Registertoevoer B',1,[201,202]),
 {...control(ids.automaticA,'source-a-uuid','continuous','Balg A · automatische inlaat',.3),automatic:true},
];
window.__test={calls:[],organs:{
 [first]:{controls,values:{[ids.switchA]:0,[ids.switchB]:1,[ids.linkedA]:0,[ids.linkedB]:1,[ids.automaticA]:.3},source_model_requested:true},
 [second]:{controls:[control(ids.other,'source-c-uuid','switch','Nieuwe bronklep',1)],values:{[ids.other]:1},source_model_requested:true},
},nextFailure:'',nextWarning:'',holdGetsFor:'',pendingGets:[],holdNextSet:false,pendingSet:null};
window.__TAURI_INTERNALS__={invoke:async(command,args)=>{
 const s=window.__test;s.calls.push({command,args:copy(args||{})});
 if(command==='get_project_wind_controls'){
  const result=copy(s.organs[args.organId]);
  if(!result)throw new Error('Onbekend testorgel');
  if(s.holdGetsFor===args.organId)return await new Promise(resolve=>s.pendingGets.push(()=>resolve(result)));
  return result;
 }
 if(command==='set_project_wind_control'){
  const organ=s.organs[args.organId],item=organ?.controls.find(control=>control.id===args.controlId);
  if(!item)throw new Error('Verkeerde orgel- of controller-ID');
  if(item.automatic)throw new Error('Automatische bronregeling is alleen-lezen');
  if(item.project_stop_ids.length)throw new Error('Registerklep mag niet zelfstandig worden bediend');
  if(!Number.isFinite(args.value)||args.value<0||args.value>1)throw new Error('Ongeldige stand');
  if(s.holdNextSet){s.holdNextSet=false;return await new Promise((resolve,reject)=>s.pendingSet={resolve,reject});}
  if(s.nextFailure){const message=s.nextFailure;s.nextFailure='';throw new Error(message);}
  organ.values[args.controlId]=args.value;
  const warning=s.nextWarning;s.nextWarning='';return {warning};
 }
 throw new Error('Onverwachte IPC-opdracht: '+command);
}};
window.__component=new ProjectWindControls({target:document.getElementById('host'),props:{organId:first,sourceNames,stopNames}});
window.__setOrgan=id=>window.__component.$set({organId:id});
window.__ready=true;
</script></body></html>`;

try {
  await fs.writeFile(harnessPath, harness, { flag: 'wx' });
  server = await createServer({ root: ui, configFile: path.join(ui, 'vite.config.js'),
    logLevel: 'error', server: { host: '127.0.0.1', port: 0 } });
  await server.listen();
  browser = await chromium.launch({ executablePath: values.chrome, headless: true });
  page = await browser.newPage({ viewport: { width: 1200, height: 900 }, deviceScaleFactor: 1 });
  page.setDefaultTimeout(10_000);
  page.on('pageerror', error => pageErrors.push(String(error)));
  await page.goto(`http://127.0.0.1:${server.httpServer.address().port}/${fileName}`, { waitUntil: 'networkidle' });
  await page.getByTestId('project-wind-controls').waitFor();
  // Direct attribute selection is scoped by the component's stable source ID.
  const control = id => page.locator(`[data-control-id="${id}"]`);
  const writes = () => page.evaluate(() => window.__test.calls.filter(call => call.command === 'set_project_wind_control'));
  const waitWrites = count => page.waitForFunction(count => window.__test.calls.filter(call => call.command === 'set_project_wind_control').length === count, count);
  assert.equal(await page.locator('article').count(), 7);
  assert.equal(await control(ids.switchA).locator('.control-footer small').textContent(), sourceNames['source-a-uuid']);
  assert.equal(await control(ids.switchB).locator('.control-footer small').textContent(), sourceNames['source-b-uuid']);
  assert.doesNotMatch(await page.getByTestId('project-wind-controls').textContent(), /source-[abc]-uuid|bron-ID/);
  assert(!(await control(ids.switchA).getByRole('checkbox').isChecked()));
  assert(await control(ids.switchB).getByRole('checkbox').isChecked());
  assert.equal(await control(ids.continuousA).getByRole('slider').inputValue(), '0.75');
  assert.equal(await control(ids.continuousB).getByRole('slider').inputValue(), '0.25');
  assert.match(await control(ids.continuousA).textContent(), /75%/);
  report('two sources with colliding raw IDs keep human source names, stable identities and source defaults');

  const automated = control(ids.automaticA);
  assert.equal(await automated.locator('input,button').count(), 0);
  assert.match(await automated.textContent(), /Automatische bronregeling.*30%/s);
  await page.evaluate(({organ, id}) => { window.__test.organs[organ].values[id] = .81; }, {organ: organA, id: ids.automaticA});
  await page.waitForFunction(id => Math.abs(document.querySelector(`[data-control-id="${id}"] progress`).value - .81) < .001, ids.automaticA);
  assert.match(await automated.textContent(), /81%/);
  assert.equal((await writes()).length, 0);
  report('automatic feedback values poll from audio and have no manual controls or reset action');

  await control(ids.switchA).getByRole('checkbox').check();
  await waitWrites(1);
  await page.waitForFunction(id => window.__test.organs['C:/fixtures/Dual.jmorgan'].values[id] === 1, ids.switchA);
  assert.deepEqual((await writes())[0], { command: 'set_project_wind_control', args: { organId: organA, controlId: ids.switchA, value: 1 } });
  await control(ids.switchB).getByRole('checkbox').uncheck();
  await waitWrites(2);
  assert.deepEqual((await writes())[1].args, { organId: organA, controlId: ids.switchB, value: 0 });
  assert(await control(ids.switchA).getByRole('checkbox').isChecked());
  report('standalone switches dispatch the stable source ID and current organ ID independently');

  const slider = control(ids.continuousA).getByRole('slider');
  await slider.evaluate(element => { element.value = '0.37'; element.dispatchEvent(new Event('input', { bubbles: true })); });
  assert.match(await control(ids.continuousA).textContent(), /37%/);
  assert.equal((await writes()).length, 2);
  await slider.dispatchEvent('change');
  await waitWrites(3);
  assert.deepEqual((await writes())[2].args, { organId: organA, controlId: ids.continuousA, value: 0.37 });
  assert.equal(await control(ids.continuousB).getByRole('slider').inputValue(), '0.25');
  report('continuous preview stays local until commit and dispatches a numeric normalized value');

  await control(ids.continuousA).getByRole('button', { name: 'Beginstand', exact: true }).click();
  await waitWrites(4);
  assert.deepEqual((await writes())[3].args, { organId: organA, controlId: ids.continuousA, value: 0.75 });
  await page.waitForFunction(id => document.querySelector('[data-control-id="' + id + '"] input').value === '0.75', ids.continuousA);
  await control(ids.switchB).getByRole('button', { name: 'Beginstand', exact: true }).click();
  await waitWrites(5);
  assert.deepEqual((await writes())[4].args, { organId: organA, controlId: ids.switchB, value: 1 });
  report('Beginstand restores each source default for switch and continuous controls');

  assert.equal(await control(ids.linkedA).locator('input,button').count(), 0);
  assert.match(await control(ids.linkedA).textContent(), /Uitgeschakeld.*Volgt Prestant 8′ · Hoofdwerk/s);
  assert.equal(await control(ids.linkedA).locator('.linked small').getAttribute('title'), stopNames[101]);
  assert.match(await control(ids.linkedB).textContent(), /Ingeschakeld.*Volgt 2 projectregisters/s);
  assert.equal(await control(ids.linkedB).locator('.linked small').getAttribute('title'), `${stopNames[201]}, ${stopNames[202]}`);
  await page.evaluate(({ organA, ids }) => { window.__test.organs[organA].values[ids.linkedA] = 1; window.__test.organs[organA].values[ids.linkedB] = 0; }, { organA, ids });
  await page.waitForFunction(id => document.querySelector('[data-control-id="' + id + '"] .lamp').classList.contains('engaged'), ids.linkedA);
  assert(!(await control(ids.linkedB).locator('.lamp').evaluate(element => element.classList.contains('engaged'))));
  assert.equal((await writes()).length, 5);
  report('linked controls are read-only and polling follows external register changes independently');

  await page.evaluate(() => window.__test.nextFailure = 'Opslaan van klepstand mislukt');
  // click(), because a correctly immediate rejection can restore checked=true
  // before Playwright's uncheck() performs its native final-state assertion.
  await control(ids.switchA).getByRole('checkbox').click();
  await waitWrites(6);
  await page.getByRole('alert').filter({ hasText: 'Opslaan van klepstand mislukt' }).waitFor();
  await page.waitForFunction(id => document.querySelector('[data-control-id="' + id + '"] input').checked, ids.switchA);
  assert(!(await control(ids.switchA).getByRole('checkbox').isDisabled()));
  report('failed changes show feedback, restore authoritative state and release controls');

  await page.evaluate(({ organA }) => { window.__test.organs[organA].source_model_requested = false; window.__test.nextWarning = 'Stand bewaard; windmodel uitgeschakeld'; }, { organA });
  await control(ids.switchA).getByRole('checkbox').uncheck();
  await waitWrites(7);
  await page.getByRole('status').filter({ hasText: 'Stand bewaard; windmodel uitgeschakeld' }).waitFor();
  await page.getByText(/‘Windwerk van de set’ staat uit/).waitFor();
  assert.equal(await page.getByRole('alert').count(), 0);
  report('disabled source model and saved-but-inactive changes show explicit feedback');

  await page.evaluate(organA => window.__test.holdGetsFor = organA, organA);
  await page.waitForFunction(() => window.__test.pendingGets.length > 0);
  await page.evaluate(organB => window.__setOrgan(organB), organB);
  await control(ids.other).waitFor();
  await page.evaluate(() => { window.__test.holdGetsFor = ''; for (const release of window.__test.pendingGets.splice(0)) release(); });
  await page.waitForTimeout(100);
  assert.equal(await page.locator('article').count(), 1);
  assert.equal(await control(ids.switchA).count(), 0);
  assert(await control(ids.other).getByRole('checkbox').isChecked());
  report('late read responses from a previous organ cannot replace the current controls');

  await page.evaluate(() => window.__test.holdNextSet = true);
  await control(ids.other).getByRole('checkbox').uncheck();
  await page.waitForFunction(() => !!window.__test.pendingSet);
  await page.evaluate(organA => window.__setOrgan(organA), organA);
  await page.evaluate(() => { window.__test.pendingSet.reject(new Error('Verouderde fout uit vorig orgel')); window.__test.pendingSet = null; });
  await control(ids.switchA).waitFor();
  assert.equal(await page.getByRole('alert').count(), 0);
  assert.equal(await control(ids.other).count(), 0);
  report('late write failures from the previous organ do not contaminate the next organ');

  await page.evaluate(() => { document.getElementById('host').scrollTop = 0; window.scrollTo(0, 0); });
  await page.screenshot({ path: path.join(output, 'project-wind-controls-wide.png') });
  for (const width of [600, 360, 320]) {
    await page.setViewportSize({ width, height: 900 });
    const geometry = await page.getByTestId('project-wind-controls').evaluate(element => ({
      hostWidth: element.parentElement.clientWidth, hostScroll: element.parentElement.scrollWidth,
      viewport: innerWidth, pageScroll: document.documentElement.scrollWidth,
      ownWidth: element.clientWidth, ownScroll: element.scrollWidth,
      articleRects: [...element.querySelectorAll('article')].map(article => ({ left: article.getBoundingClientRect().left, right: article.getBoundingClientRect().right })),
    }));
    assert(geometry.hostScroll <= geometry.hostWidth + 1, JSON.stringify({ width, geometry }));
    assert(geometry.pageScroll <= geometry.viewport + 1, JSON.stringify({ width, geometry }));
    assert(geometry.ownScroll <= geometry.ownWidth + 1, JSON.stringify({ width, geometry }));
    assert(geometry.articleRects.every(rect => rect.left >= -1 && rect.right <= width + 1));
  }
  await page.evaluate(() => { document.getElementById('host').scrollTop = 0; window.scrollTo(0, 0); });
  await page.screenshot({ path: path.join(output, 'project-wind-controls-narrow.png') });
  report('600, 360 and 320 px layouts keep every control within the viewport without horizontal overflow');
  assert.deepEqual(pageErrors, []);
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
