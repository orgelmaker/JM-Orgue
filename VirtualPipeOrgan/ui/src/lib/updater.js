// Automatische update (0.7.40) — downloaden, installeren en herstarten met één knop.
//
// GEEN periodieke controle: zoekUpdate() wordt alleen aangeroepen bij het
// STARTEN van de app en via de knop "Controleer op updates" in Algemene
// Instellingen. De updater-plugin doet uit zichzelf niets; er staat hier
// bewust geen timer.
//
// Hoe het werkt:
//   1. check() haalt latest.json op bij de nieuwste GitHub-release
//      (endpoints staan in tauri.conf.json) en vergelijkt de versie.
//   2. download() haalt de installer op en controleert de minisign-handtekening
//      tegen de publieke sleutel in de binary. Klopt die niet, dan wordt er
//      NIETS geïnstalleerd en komt er een fout uit deze stap.
//   3. install() start de installer. Op Windows keert die aanroep nooit terug:
//      de plugin start de NSIS-setup (stil, met /UPDATE /R) en beëindigt het
//      proces met exit(0); de setup start de app daarna zelf weer op.
//      Op macOS/Linux moeten we zelf relaunch() aanroepen.
//
// Terugval: kan de plugin niets (geen latest.json, netwerkfout, macOS), dan
// valt de app terug op de oude route via de GitHub-API — melding met een knop
// naar de downloadpagina.

import { check } from '@tauri-apps/plugin-updater';
import { relaunch } from '@tauri-apps/plugin-process';
import { invoke } from '@tauri-apps/api/core';
import { checkForUpdate as checkViaGitHub, githubRepoUrl } from './github.js';

/** Draait de app op macOS? (webview-heuristiek; geen extra plugin nodig) */
export function isMacOS() {
  try {
    const p = `${navigator.userAgentData?.platform || ''} ${navigator.platform || ''} ${navigator.userAgent || ''}`;
    return /mac/i.test(p);
  } catch (e) {
    return false;
  }
}

// macOS bewust UIT: onze .app is niet ondertekend en niet genotariseerd (geen
// Apple Developer-account). De updater vervangt de .app ter plekke; staat hij
// in /Applications zonder schrijfrechten dan mislukt dat, en de
// Gatekeeper-ontheffing die de gebruiker ooit handmatig gaf (rechtsklik → Open
// of xattr -dr) moet na het vervangen opnieuw. Een half gelukte automatische
// update is daar erger dan één keer handmatig de dmg installeren.
export const autoUpdateOndersteund = () => !isMacOS();

// Ruime deadline voor de download (reqwest-timeout in de plugin: van verbinden
// tot en met het laatste brokje). Zonder deze grens blijft een verbinding die
// blijft hangen zonder fout de balk voor altijd op "Update downloaden..." laten
// staan en is de app alleen met een herstart te redden. 30 minuten is ruim: ook
// een installer van 100 MB haalt dat nog bij ~55 kB/s. Loopt hij af, dan gooit
// download() een fout, komt de nette melding in beeld en kan de gebruiker de
// downloadpagina gebruiken. Geen afbreekknop: die zou alleen de weergave
// terugzetten terwijl de download doorloopt (tweede poging = dubbel downloaden).
const DOWNLOAD_TIMEOUT_MS = 30 * 60 * 1000;

const releaseUrl = (version) => {
  const v = String(version || '').replace(/^v/, '');
  return v ? `${githubRepoUrl()}/releases/tag/v${v}` : `${githubRepoUrl()}/releases`;
};

/**
 * Controleer op een nieuwere versie.
 *
 * Geeft null als alles bij is, anders:
 *   { version, url, notes, update, auto }
 * waarbij `auto` true is als er met één knop bijgewerkt kan worden (`update`
 * is dan het handvat van de plugin). Bij auto=false is alleen de
 * downloadpagina beschikbaar.
 *
 * Faalt geluidloos: een update-controle mag de opstart nooit storen.
 */
export async function zoekUpdate(currentVersion) {
  if (autoUpdateOndersteund()) {
    try {
      const update = await check({ timeout: 15000 });
      if (update) {
        const version = String(update.version || '').replace(/^v/, '');
        return {
          version,
          url: releaseUrl(version),
          notes: update.body || '',
          update,
          auto: true,
        };
      }
      // null = latest.json gezien en we zijn bij. Tóch nog even de GitHub-API:
      // vlak na het publiceren van een release staan de installers (en dus
      // latest.json) er nog niet, terwijl de tag al wel bestaat.
    } catch (e) {
      console.warn('Update-controle via latest.json mislukt:', e);
    }
  }
  const upd = await checkViaGitHub(currentVersion);
  return upd ? { ...upd, notes: '', update: null, auto: false } : null;
}

/**
 * Download de update (met voortgang), sla de laatste stand op en installeer.
 *
 * onVoortgang({ fase: 'download'|'install', pct, gedaan, totaal }) — pct is
 * null zolang de servergrootte onbekend is (geen Content-Length).
 * voorInstalleren() — async callback die vlak vóór het afsluiten nog opslaat.
 *
 * Gooit bij een mislukte download/handtekening/installatie; de app blijft dan
 * gewoon draaien.
 */
export async function installeerUpdate(update, { onVoortgang, voorInstalleren } = {}) {
  let totaal = 0;
  let gedaan = 0;
  const meld = (fase, pct) => { try { onVoortgang?.({ fase, pct, gedaan, totaal }); } catch (e) {} };

  meld('download', null);
  await update.download((event) => {
    switch (event.event) {
      case 'Started':
        totaal = event.data?.contentLength || 0;
        gedaan = 0;
        meld('download', totaal ? 0 : null);
        break;
      case 'Progress':
        // LET OP: chunkLength is de grootte van DIT brokje, geen totaal.
        gedaan += event.data?.chunkLength || 0;
        meld('download', totaal ? Math.min(100, Math.round((gedaan / totaal) * 100)) : null);
        break;
      case 'Finished':
        if (!totaal) totaal = gedaan;
        gedaan = totaal;
        meld('download', 100);
        break;
    }
  }, { timeout: DOWNLOAD_TIMEOUT_MS });

  // Vanaf hier is de handtekening geverifieerd en staat de installer klaar.
  meld('install', 100);

  // Laatste stand opslaan (instellingen, geometrie, panelen) — de app sluit zo af.
  if (voorInstalleren) {
    try { await voorInstalleren(); } catch (e) { console.warn('Opslaan vóór de update mislukte:', e); }
  }
  // Audio-thread/driver netjes vrijgeven: de installer beëindigt het proces met
  // exit(0) (geen Drop-handlers) en zou anders een bezet ASIO/WASAPI-apparaat
  // achterlaten voor de opnieuw gestarte app.
  try { await invoke('prepare_for_update'); } catch (e) { console.warn('prepare_for_update mislukte:', e); }

  await update.install();

  // Windows: hier komen we nooit — de plugin heeft het proces al beëindigd en
  // de installer start de app zelf opnieuw op (/R). macOS/Linux: zelf doen.
  try { await relaunch(); } catch (e) { console.warn('relaunch mislukte:', e); }
}

/** "12,3 MB van 34,5 MB" — leesbare voortgang als er geen percentage is. */
export function formatteerBytes(n) {
  const mb = (n || 0) / (1024 * 1024);
  return `${mb.toFixed(1)} MB`;
}
