// Isolated, headless SetzerBar test. MIDI IPC is mocked; Rust queue/persistence
// and real MIDI injection are verified separately by the app background probe.
import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { randomUUID } from 'node:crypto';
import { parseArgs } from 'node:util';
const here = path.dirname(fileURLToPath(import.meta.url)), ui = path.resolve(here, '../ui');
const { values } = parseArgs({ options: {
  playwright: { type: 'string', default: process.env.PLAYWRIGHT_MODULE },
  chrome: { type: 'string', default: 'C:/Program Files/Google/Chrome/Application/chrome.exe' },
  output: { type: 'string', default: path.resolve(here, '../../testen/orgelbouwer-uitbreiding-2026-11-26/midi-ui') },
} });
const { chromium } = await import(values.playwright ? pathToFileURL(path.resolve(values.playwright)).href : 'playwright');
const { createServer } = await import(pathToFileURL(path.join(ui, 'node_modules/vite/dist/node/index.js')).href);
const output = path.resolve(values.output); await fs.mkdir(output, { recursive: true });
const fileName = `.midi-actions-test-${randomUUID()}.html`, harnessPath = path.join(ui, fileName);
const checks = [], errors = [];
let server, browser;
const harness = `<!doctype html><html><head><meta charset="utf-8"><title>Isolated wide MIDI actions</title></head><body><main id="app"><div id="bar"></div><div id="surface" style="overflow:auto;min-height:0;flex:1"></div></main><script type="module">
import SetzerBar from '/src/components/SetzerBar.svelte';
import ConsoleSurface from '/src/components/ConsoleSurface.svelte';
import {defaultConsoleLayout} from '/src/lib/consoleLayout.js';
import '/src/styles.css';
const stops = Array.from({length:2048},(_,i)=>({id:'project_'+(i+1),name:'Register '+(i+1),midi_action_code:65537+i,drawn:false}));
window.__test={queue:[],calls:[],external:[],surfaceToggles:[],ready:false};
window.__TAURI_INTERNALS__={invoke:async(command,args)=>{
 if(command==='poll_preset_trigger') return window.__test.queue.shift()??null;
 if(command==='get_preset_bindings') return [];
 if(command==='get_crescendo_state') return [false,0,0];
 if(command==='get_crescendo_claims') return [];
 if(command==='report_setzer_state') return null;
 window.__test.calls.push({command,args:JSON.parse(JSON.stringify(args||{}))});return null;
}};
window.__organ={id:'C:/fixtures/Large.jmorgan',divisions:[{name:'Manual',stops}],couplers:Array.from({length:512},(_,i)=>({
 id:i===0?'coupler-first':i===511?'coupler-last':'coupler-'+i,name:i===0?'First':i===511?'Last':'Coupler '+i,midi_action_code:262144+i,active:false})),
 schakelaars:{pistons:[{index:0,naam:'Piston',midi_action_code:262656}]}};
window.__component=new SetzerBar({target:document.getElementById('bar'),props:{organInfo:window.__organ,consumeMidiTriggers:true}});
window.__component.$on('externalAction',event=>window.__test.external.push(event.detail.actionCode));
window.__layout=defaultConsoleLayout(window.__organ);
window.__surface=new ConsoleSurface({target:document.getElementById('surface'),props:{organInfo:window.__organ,layout:window.__layout}});
window.__surface.$on('pageChange',event=>window.__surface.$set({pageId:event.detail.pageId}));
window.__surface.$on('toggle',event=>window.__test.surfaceToggles.push(event.detail));
window.__test.ready=true;
</script></body></html>`;
try {
  await fs.writeFile(harnessPath, harness, { flag: 'wx' });
  server = await createServer({ root: ui, configFile: path.join(ui, 'vite.config.js'), logLevel: 'error', server: {host:'127.0.0.1',port:0} });
  await server.listen();
  browser = await chromium.launch({ executablePath: values.chrome, headless: true });
  const page = await browser.newPage({viewport:{width:1440,height:900}});
  page.on('pageerror', error=>errors.push(String(error)));
  await page.goto(`http://127.0.0.1:${server.httpServer.address().port}/${fileName}`, {waitUntil:'networkidle'});
  await page.waitForFunction(()=>window.__test?.ready);
  await page.evaluate(()=>window.__test.queue.push(65537,67584,262144,262655,262656,24,42));
  await page.waitForFunction(()=>window.__test.calls.length===5 && window.__test.external.length===2);
  const actual = await page.evaluate(()=>window.__test);
  assert.deepEqual(actual.calls, [
    {command:'toggle_stop',args:{stopId:'project_1'}}, {command:'toggle_stop',args:{stopId:'project_2048'}},
    {command:'toggle_coupler',args:{couplerId:'coupler-first'}}, {command:'toggle_coupler',args:{couplerId:'coupler-last'}},
    {command:'piston_druk',args:{index:0,ingedrukt:true}},
  ]);
  checks.push('first and 2048th stop dispatch without truncation');
  checks.push('extended couplers and pistons dispatch to their own type');
  assert.deepEqual(actual.external,[24,42]); checks.push('reserved tremulant and shutdown actions keep their legacy dispatch');
  const layoutInfo = await page.evaluate(()=>({pages:window.__layout.pages.length,targets:window.__layout.pages.flatMap(p=>p.elements).filter(e=>e.kind!=='label').length,
    lastStopPage:window.__layout.pages.find(p=>p.elements.some(e=>e.target_id==='project_2048')).id,
    lastCouplerPage:window.__layout.pages.find(p=>p.elements.some(e=>e.target_id==='coupler-last')).id}));
  assert.equal(layoutInfo.targets,2560); assert(layoutInfo.pages>8 && layoutInfo.pages<=64);
  await page.locator('.console-pages select').selectOption(layoutInfo.lastStopPage);
  await page.getByRole('button',{name:/^Register 2048 /}).click();
  await page.locator('.console-pages select').selectOption(layoutInfo.lastCouplerPage);
  await page.getByRole('button',{name:/^Last /}).click();
  assert.deepEqual(await page.evaluate(()=>window.__test.surfaceToggles),[{kind:'stop',targetId:'project_2048'},{kind:'coupler',targetId:'coupler-last'}]);
  checks.push('all 2560 console controls paginate; stop 2048 and coupler 512 can be selected and clicked');
  await page.screenshot({path:path.join(output,'large-console-last-page.png')});
  await page.evaluate(()=>{
    window.__organ.couplers.push({id:'ambiguous',name:'Ambiguous',midi_action_code:65537});
    window.__component.$set({organInfo:{...window.__organ}});
  });
  // Queue on a later browser task, after Svelte applies the changed DTO map.
  await page.evaluate(()=>window.__test.queue.push(65537));
  await page.waitForFunction(()=>window.__test.queue.length===0);
  assert.equal((await page.evaluate(()=>window.__test.calls)).length,5);
  checks.push('ambiguous codes do not operate a guessed object');
  assert.deepEqual(errors,[]);
  await fs.writeFile(path.join(output,'result.json'),JSON.stringify({headless:true,mocked_ipc:true,checks,page_errors:errors},null,2));
  console.log(JSON.stringify({checks,page_errors:errors},null,2));
} finally {
  if(browser) await browser.close();
  if(server) await server.close();
  await fs.unlink(harnessPath).catch(error=>{if(error.code!=='ENOENT')throw error;});
}
