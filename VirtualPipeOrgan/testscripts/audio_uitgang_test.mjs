// Controle van de opgeslagen audio-uitgang voor het instellingenscherm: de
// backend (audio_config.json) is de bron; de oude kopie in localStorage telt
// ook wanneer oude kopieën nog in localStorage staan.
// Draait in CI (tests.yml) en lokaal: node testscripts/audio_uitgang_test.mjs
import { opgeslagenUitgang, pickDevice } from '../ui/src/lib/audioDevices.js';

let fouten = 0;
function check(naam, ok) {
  console.log(`${ok ? 'OK  ' : 'FOUT'} ${naam}`);
  if (!ok) fouten++;
}
const gelijk = (a, b) => JSON.stringify(a) === JSON.stringify(b);
const opslag = (inhoud) => ({ getItem: (k) => (k in inhoud ? inhoud[k] : null) });

const oud = opslag({
  'jm-orgue-audio-host': 'WASAPI',
  'jm-orgue-audio-device': 'Luidsprekers (oud)',
  'jm-orgue-audio-buffer': '512',
  'jm-orgue-audio-rate': '44100',
});

// De backend wint, ook als localStorage iets anders zegt (het geval waar dit om gaat).
check('backend wint van een afwijkende localStorage',
  gelijk(opgeslagenUitgang({ host: 'ASIO', device: 'ASIO4ALL v2', buffer_frames: 256, sample_rate: 48000 }, oud),
    { host: 'ASIO', device: 'ASIO4ALL v2', buffer: 256, rate: 48000, bron: 'backend' }));

// Buffer en samplerate "standaard" (null) in de backend blijven standaard; de oude
// waarden uit localStorage sijpelen er niet doorheen.
check('standaard buffer en samplerate blijven standaard',
  gelijk(opgeslagenUitgang({ host: 'WASAPI', device: null, buffer_frames: null, sample_rate: null }, oud),
    { host: 'WASAPI', device: null, buffer: null, rate: null, bron: 'backend' }));

// Een lege host in de backend betekent de standaard, óók als er oude sleutels zijn.
check('standaardhost gebruikt geen verouderde lokale keuze',
  gelijk(opgeslagenUitgang({ host: null, device: null, buffer_frames: null, sample_rate: null }, oud),
    { host: null, device: null, buffer: null, rate: null, bron: 'backend' }));
check('standaardhost behoudt overige backendkeuzes',
  gelijk(opgeslagenUitgang({ host: null, device: 'Actief', buffer_frames: 256, sample_rate: 96000 }, oud),
    { host: null, device: 'Actief', buffer: 256, rate: 96000, bron: 'backend' }));
check('de standaardhost van de backend wint van verouderde lokale keuze',
  opgeslagenUitgang({ host: null, default_host: 'ALSA' }, oud).host === 'ALSA');
check('een gekozen host wint van de standaardhost',
  opgeslagenUitgang({ host: 'ASIO', default_host: 'WASAPI' }, oud).host === 'ASIO');
check('onbekende voorkeur gebruikt geen verouderde lokale keuze',
  gelijk(opgeslagenUitgang(null, oud), { host: null, device: null, buffer: null, rate: null, bron: 'onbekend' }));
check('ongeldige getallen worden null',
  opgeslagenUitgang({ buffer_frames: 'abc', sample_rate: -1 }).buffer === null &&
  opgeslagenUitgang({ buffer_frames: 'abc', sample_rate: -1 }).rate === null);
check('nul uit de backend is standaard (null)',
  opgeslagenUitgang({ host: 'WASAPI', buffer_frames: 0, sample_rate: 0 }, opslag({})).buffer === null);

// Samen met pickDevice: een opgeslagen apparaat dat niet meer bestaat valt op het standaardapparaat.
const lijst = [{ name: 'A', is_default: false }, { name: 'B', is_default: true }];
check('opgeslagen apparaat bestaat nog', pickDevice(lijst, 'A') === 'A');
check('opgeslagen apparaat weg: standaardapparaat', pickDevice(lijst, 'Luidsprekers (oud)') === 'B');

if (fouten) {
  console.error(`${fouten} controle(s) mislukt`);
  process.exit(1);
}
console.log('audio_uitgang_test: alles in orde');
