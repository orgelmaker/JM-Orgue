// Headless component test with explicit IPC mocks. This does not pretend to
// validate the Rust compiler/audio; those require the isolated process test.
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
  output: { type: 'string', default: path.resolve(here, '../../testen/orgelbouwer-2026-11-26/ui') },
} });
let chromium;
try {
  ({ chromium } = await import(values.playwright ? pathToFileURL(path.resolve(values.playwright)).href : 'playwright'));
} catch (error) {
  throw new Error('Playwright ontbreekt. Installeer het pakket of geef --playwright <absoluut pad naar playwright/index.mjs> (of PLAYWRIGHT_MODULE).', { cause: error });
}
const { createServer } = await import(pathToFileURL(path.join(ui, 'node_modules/vite/dist/node/index.js')).href);
const output = path.resolve(values.output);
await fs.mkdir(output, { recursive: true });
const fileName = `.organ-project-test-${randomUUID()}.html`;
const harnessPath = path.join(ui, fileName);
const checks = [];
const report = name => { checks.push(name); console.log(`OK ${name}`); };
let server, browser, page;
const pageErrors = [];
const harness = `<!doctype html><html lang="nl"><head><meta charset="utf-8"><title>Orgelbouwer geïsoleerde componenttest</title><style>#header-host{flex-shrink:0}</style></head><body><main id="app"><div id="header-host"></div><div class="console" id="builder-host"></div></main><script type="module">
import OrganBuilder from '/src/components/OrganBuilder.svelte';
import Header from '/src/components/Header.svelte';
import '/src/styles.css';
const copy = value => JSON.parse(JSON.stringify(value));
const a = 'C:/fixture/Samen/Orgel A.organ', b = 'C:/fixture/Samen/Orgel B.organ';
const catalogs = {
 [a]: {path:a,name:'Atelier · Bron A',format:'grandorgue',fingerprint:'fingerprint-a',supports_original_pitch:true,warnings:[],stops:[
  {id:1,name:'Prestant 8',manual_id:1,manual_name:'Manuaal I',harmonic_number:8,first_midi_note:36,key_count:61,perspectives:[]},
  {id:2,name:'Octaaf 4',manual_id:1,manual_name:'Manuaal I',harmonic_number:16,first_midi_note:36,key_count:61,perspectives:[]}]},
 [b]: {path:b,name:'Kerkorgel · Bron B',format:'grandorgue',fingerprint:'fingerprint-b',supports_original_pitch:true,warnings:['Deze opname kan natuurlijke galm bevatten.'],stops:[
  {id:1,name:'Gedekt 8',manual_id:1,manual_name:'Manuaal II',harmonic_number:8,first_midi_note:36,key_count:61,perspectives:[]},
  {id:2,name:'Subbas 16',manual_id:0,manual_name:'Pedaal',harmonic_number:4,first_midi_note:36,key_count:30,perspectives:[]}]}
};
window.__test = {dialogs:[],storage:{},calls:[],loadEvents:[],consoleEvents:0,dirty:false,saving:false,organ:null};
window.__TAURI_INTERNALS__ = {invoke:async (command,args) => {
 const s=window.__test;s.calls.push({command,args:copy(args || {})});
 if(command==='plugin:dialog|open' || command==='plugin:dialog|save') {
  if(!s.dialogs.length) throw new Error('Onverwachte testdialoog: '+command);
  return s.dialogs.shift();
 }
 if(command==='inspect_organ_project_source') {
  if(!catalogs[args.path]) throw new Error('Onbekende testbron');return copy(catalogs[args.path]);
 }
 if(command==='save_organ_project') {
  await new Promise(resolve=>setTimeout(resolve,25));
  const saved=copy(args.project);saved.sources.forEach(source=>{source.resources=[{path:source.path+'.wav',bytes:123,modified_ns:'123'}];});
  s.storage[args.path]=copy(saved);return saved;
 }
 if(command==='read_organ_project') {if(!s.storage[args.path]) throw new Error('Testproject ontbreekt');return copy(s.storage[args.path]);}
 if(command==='get_project_wind_controls') return {controls:[],values:{},source_model_requested:true};
 throw new Error('Onverwachte IPC-opdracht: '+command);
}};
let component;
new Header({target:document.getElementById('header-host'),props:{showTabs:true,organLoaded:true,organName:'Mijn atelierorgel · eigen dispositie uit twee samplesets',activeView:'orgelbouwer',status:{audioRunning:false}}});
window.__mount = () => {
 if(component)component.$destroy();
 window.__test.dirty=false;window.__test.saving=false;
 component=new OrganBuilder({target:document.getElementById('builder-host'),props:{organInfo:window.__test.organ}});
 component.$on('dirty',event=>window.__test.dirty=event.detail);
 component.$on('saving',event=>window.__test.saving=event.detail);
 component.$on('load',event=>{const s=window.__test;s.loadEvents.push({path:event.detail.path,dirty:s.dirty,saving:s.saving});s.organ={id:event.detail.path};component.$set({organInfo:s.organ});});
 component.$on('console',()=>window.__test.consoleEvents++);
};
window.__mount();
</script></body></html>`;

try {
  await fs.writeFile(harnessPath, harness, { flag: 'wx' });
  server = await createServer({ root: ui, configFile: path.join(ui, 'vite.config.js'),
    logLevel: 'error', server: { host: '127.0.0.1', port: 0, strictPort: false } });
  await server.listen();
  const address = server.httpServer.address();
  assert(address && typeof address === 'object');
  browser = await chromium.launch({ executablePath: values.chrome, headless: true });
  page = await browser.newPage({ viewport: { width: 1440, height: 1000 }, deviceScaleFactor: 1 });
  page.on('pageerror', error => pageErrors.push(String(error)));
  await page.goto(`http://127.0.0.1:${address.port}/${fileName}`, { waitUntil: 'networkidle' });
  await page.getByTestId('organ-builder').waitFor();
  assert(await page.getByTestId('organ-project-load').isDisabled());
  report('nieuw project blokkeert laden zonder gekozen bron en register');

  const queueDialog = value => page.evaluate(value => window.__test.dialogs.push(value), value);
  await queueDialog('C:/fixture/Samen/Orgel A.organ');
  await page.getByTestId('organ-project-source-1').getByRole('button', { name: 'Kies definitie…', exact: true }).click();
  await page.getByRole('heading', { name: 'Atelier · Bron A' }).waitFor();
  await queueDialog('C:/fixture/Samen/Orgel B.organ');
  await page.getByTestId('organ-project-source-2').getByRole('button', { name: 'Kies definitie…', exact: true }).click();
  await page.getByRole('heading', { name: 'Kerkorgel · Bron B' }).waitFor();
  report('twee brondefinities in dezelfde map kiezen en analyseren');

  await page.getByTestId('organ-project-name').fill('Mijn atelierorgel');
  await page.locator('.project-settings').getByRole('spinbutton').fill('442');
  const sourceA = page.getByTestId('organ-project-source-1');
  const sourceB = page.getByTestId('organ-project-source-2');
  await sourceA.getByLabel('Opgenomen A (Hz)').fill('415');
  await sourceB.getByLabel('Stemming', { exact: true }).selectOption('original');
  await sourceB.getByLabel('Extra (cent)').fill('10');
  assert(await sourceB.getByLabel('Opgenomen A (Hz)').isDisabled());

  await page.locator('.source-tabs').getByRole('button', { name: /Atelier/ }).click();
  await page.getByRole('checkbox', { name: 'Prestant 8 kiezen', exact: true }).check();
  await page.getByRole('combobox', { name: 'Doelklavier voor Prestant 8' }).selectOption('3');
  await page.locator('.source-tabs').getByRole('button', { name: /Kerkorgel/ }).click();
  await page.getByRole('checkbox', { name: 'Gedekt 8 kiezen', exact: true }).check();
  await page.getByRole('combobox', { name: 'Doelklavier voor Gedekt 8' }).selectOption('2');
  await page.getByRole('checkbox', { name: 'Subbas 16 kiezen', exact: true }).check();
  assert(!(await page.getByTestId('organ-project-load').isDisabled()));
  report('registers selecteren, verplaatsen en stembeleid onafhankelijk kiezen');

  const projectPath = 'C:/fixture/Projecten/Atelier.jmorgan';
  await queueDialog(projectPath);
  await page.getByTestId('organ-project-save').click();
  await page.waitForFunction(path => !!window.__test.storage[path] && !window.__test.saving, projectPath);
  const saved = await page.evaluate(path => window.__test.storage[path], projectPath);
  assert.equal(saved.name, 'Mijn atelierorgel');
  assert.equal(saved.target_a4_hz, 442);
  assert.equal(saved.sources[0].reference_a4_hz, 415);
  assert.equal(saved.sources[0].tuning_policy, 'normalize');
  assert.equal(saved.sources[1].tuning_policy, 'original');
  assert.equal(saved.sources[1].tuning_cents, 10);
  assert.equal(saved.stops.length, 3);
  assert.equal(saved.stops[0].division_id, 3);
  assert.equal(saved.stops[1].division_id, 2);
  assert.equal(saved.stops[2].division_id, 1);
  assert.equal(new Set(saved.stops.map(stop => stop.id)).size, 3);
  report('project bewaren met bronstemming, pedaal en stabiele registerdoelen');

  const dialogCallsBefore = await page.evaluate(() => window.__test.calls.filter(call => call.command.startsWith('plugin:dialog|')).length);
  await page.getByTestId('organ-project-name').fill('Nog niet opgeslagen');
  await page.getByRole('button', { name: 'Open project…', exact: true }).click();
  await page.getByText('Je hebt niet-opgeslagen wijzigingen. Wil je die verwerpen?', { exact: true }).waitFor();
  assert.equal(await page.evaluate(() => window.__test.calls.filter(call => call.command.startsWith('plugin:dialog|')).length), dialogCallsBefore);
  await page.getByRole('button', { name: 'Annuleren', exact: true }).click();
  assert.equal(await page.getByTestId('organ-project-name').inputValue(), 'Nog niet opgeslagen');
  await page.getByRole('button', { name: 'Wijzigingen herstellen', exact: true }).click();
  await queueDialog('C:/fixture/Samen/Orgel A.organ');
  await sourceA.getByRole('button', { name: 'Andere definitie…', exact: true }).click();
  await page.getByText(/Deze bron opnieuw koppelen en de huidige bronbestanden bij opslaan opnieuw vastleggen/).waitFor();
  await page.getByRole('button', { name: 'Annuleren', exact: true }).click();
  assert.equal(await page.evaluate(() => window.__test.dirty), false);
  report('annuleren beschermt een gewijzigd concept en de vastgelegde bronbestanden');

  await page.getByTestId('organ-project-load').click();
  await page.waitForFunction(() => window.__test.loadEvents.length === 1);
  const load = await page.evaluate(() => window.__test.loadEvents[0]);
  assert.deepEqual(load, { path: projectPath, dirty: false, saving: false });
  await page.getByRole('button', { name: 'Console indelen →' }).click();
  assert.equal(await page.evaluate(() => window.__test.consoleEvents), 1);
  report('laden pas na vrijgeven van dirty/saving-guard; Consolebouwer-event werkt');

  await page.getByRole('textbox', { name: 'Projectnaam voor Prestant 8' }).fill('Eigen Prestant');
  await page.getByRole('button', { name: 'Wijzigingen herstellen', exact: true }).click();
  assert.equal(await page.getByRole('textbox', { name: 'Projectnaam voor Prestant 8' }).inputValue(), '');
  assert.equal(await page.evaluate(() => window.__test.dirty), false);
  await queueDialog(projectPath);
  await page.getByRole('button', { name: 'Open project…', exact: true }).click();
  await page.getByText('Project geopend. Controleer de bronnen en laad het orgel wanneer je klaar bent.', { exact: true }).waitFor();
  assert.equal(await sourceA.getByLabel('Opgenomen A (Hz)').inputValue(), '415');
  assert.equal(await sourceB.getByLabel('Stemming', { exact: true }).inputValue(), 'original');
  assert.equal(await page.getByTestId('organ-project-name').inputValue(), 'Mijn atelierorgel');
  report('wijzigingen herstellen en opgeslagen project opnieuw openen');

  const before = await page.evaluate(() => window.__test.calls.filter(call => call.command === 'read_organ_project').length);
  await page.evaluate(() => window.__mount());
  await page.waitForFunction(before => window.__test.calls.filter(call => call.command === 'read_organ_project').length > before && !window.__test.saving, before);
  assert.equal(await page.getByTestId('organ-project-name').inputValue(), 'Mijn atelierorgel');
  assert.equal(await page.evaluate(() => window.__test.dialogs.length), 0);
  report('tab opnieuw openen leest het geladen project zonder bestandsdialoog');
  assert.deepEqual(pageErrors, []);
  await page.locator('.organ-builder').evaluate(element => element.scrollTop = 0);
  await page.screenshot({ path: path.join(output, 'orgelbouwer-project.png') });
  await page.locator('.organ-builder').evaluate(element => element.scrollTop = element.scrollHeight);
  await page.screenshot({ path: path.join(output, 'orgelbouwer-dispositie.png') });
  await page.setViewportSize({ width: 1024, height: 768 });
  assert(await page.locator('.organ-builder').evaluate(element => element.scrollWidth <= element.clientWidth + 1));
  await page.getByRole('textbox', { name: 'Projectnaam voor Prestant 8' }).scrollIntoViewIfNeeded();
  assert(await page.getByRole('textbox', { name: 'Projectnaam voor Prestant 8' }).isVisible());
  report('compact venster behoudt alle inhoud zonder horizontale overloop');
  for (const width of [1280, 1024]) {
    await page.setViewportSize({ width, height: 768 });
    const buttons = await page.locator('.header').evaluate(header => {
      const bounds = header.getBoundingClientRect();
      return { width: innerWidth, bottom: bounds.bottom, tabs: header.querySelectorAll('[role=tab]').length,
        buttons: [...header.querySelectorAll('button')].map(button => {
          const box = button.getBoundingClientRect();
          return { name: button.title || button.textContent.trim(), left: box.left, right: box.right, top: box.top, bottom: box.bottom };
        }) };
    });
    assert.equal(buttons.tabs, 5);
    for (const button of buttons.buttons) assert(button.left >= -1 && button.right <= buttons.width + 1 && button.top >= -1 && button.bottom <= buttons.bottom + 1,
      `Headerknop buiten beeld bij ${width}px: ${JSON.stringify(button)}`);
    for (let a = 0; a < buttons.buttons.length; a++) for (let b = a + 1; b < buttons.buttons.length; b++) {
      const first = buttons.buttons[a], second = buttons.buttons[b];
      assert(!(Math.min(first.right, second.right) - Math.max(first.left, second.left) > 1 &&
        Math.min(first.bottom, second.bottom) - Math.max(first.top, second.top) > 1),
        `Headerknoppen overlappen bij ${width}px: ${first.name} / ${second.name}`);
    }
    await page.locator('.organ-builder').evaluate(element => element.scrollTop = 0);
    await page.screenshot({ path: path.join(output, `orgelbouwer-header-${width}.png`) });
  }
  report('vijf tabs en alle headerknoppen zichtbaar zonder overlap op 1280 en 1024 px');
  const result = { purpose: 'isolated-organ-builder-component-test', headless: true, mocked_ipc: true,
    checks, page_errors: pageErrors, screenshot: path.join(output, 'orgelbouwer-project.png') };
  await fs.writeFile(path.join(output, 'result.json'), JSON.stringify(result, null, 2));
  console.log(JSON.stringify(result, null, 2));
} catch (error) {
  if (page) await page.screenshot({ path: path.join(output, 'failure.png'), fullPage: true }).catch(() => {});
  throw error;
} finally {
  if (browser) await browser.close();
  if (server) await server.close();
  await fs.unlink(harnessPath).catch(error => { if (error.code !== 'ENOENT') throw error; });
}
