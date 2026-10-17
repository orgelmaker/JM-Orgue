// Controle van de update-melding (0.8.96): een nieuwe release telt pas als
// hij klaar is, d.w.z. latest.json staat bij de bestanden. Vlak na het
// publiceren bestaat alleen de tag nog; dan geen melding en geen downloadknop.
// Draait in CI (tests.yml) en lokaal: node testscripts/update_klaar_test.mjs
import { releaseKlaar, checkForUpdate } from '../ui/src/lib/github.js';

let fouten = 0;
function check(naam, ok) {
  console.log(`${ok ? 'OK  ' : 'FOUT'} ${naam}`);
  if (!ok) fouten++;
}

const asset = (name, state = 'uploaded') => ({ name, state });
check('zonder bestanden niet klaar', !releaseKlaar({ tag_name: 'v9.9.9', assets: [] }));
check('alleen installers, nog geen latest.json', !releaseKlaar({ assets: [asset('JM-Orgue-setup.exe'), asset('JM-Orgue.dmg')] }));
check('latest.json nog aan het uploaden', !releaseKlaar({ assets: [asset('latest.json', 'starter')] }));
check('concept telt niet', !releaseKlaar({ draft: true, assets: [asset('latest.json')] }));
check('voorlopige versie telt niet', !releaseKlaar({ prerelease: true, assets: [asset('latest.json')] }));
check('met latest.json klaar', releaseKlaar({ assets: [asset('JM-Orgue-setup.exe'), asset('latest.json')] }));

// checkForUpdate met een nagebootste GitHub-API
async function metRelease(rel, huidig) {
  globalThis.fetch = async () => ({ ok: true, status: 200, json: async () => rel });
  return checkForUpdate(huidig);
}
check('nieuwere tag zonder latest.json: geen melding',
  (await metRelease({ tag_name: 'v0.9.0', html_url: 'x', assets: [asset('JM-Orgue-setup.exe')] }, '0.8.95')) === null);
const r = await metRelease({ tag_name: 'v0.9.0', html_url: 'https://example/rel', assets: [asset('latest.json')] }, '0.8.95');
check('nieuwere tag mét latest.json: melding', r && r.version === '0.9.0' && r.url === 'https://example/rel');
check('zelfde versie: geen melding',
  (await metRelease({ tag_name: 'v0.8.95', assets: [asset('latest.json')] }, '0.8.95')) === null);

if (fouten) { console.error(`${fouten} fout(en)`); process.exit(1); }
console.log('alles OK');
