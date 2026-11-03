# Eindrapport CODM-ontwikkelplan (3 november 2026)

**Basis:** JM-Orgue 0.9.41 (`1171585`). **Stand:** zes commits lokaal (`ab8a986` A+B, `07e4a19` C, `d68ba52` D,
`88cfd25` E, `fb16096` F) plus het G-werk dat dit rapport meelevert. Niets is gepusht of uitgebracht.
Gedetailleerde veldstatus: `VELDMATRIX-CODM-2026-11-03.md`.

## 1. Wat gebouwd is

| pakket | resultaat |
|---|---|
| **A** | `ImportReport` (formaat, versie, pakketten met rol en naam, verwacht/geladen, overgeslagen registers met reden, meldingen met ernst); preflight vóór het laden met de keuze pakketmap kiezen / gedeeltelijk laden / annuleren; vorig orgel blijft actief; pakketmap per orgel onthouden; verslag opnieuw te openen; 7 talen |
| **B** | echte kopanalyse (BOM, quotes, volgorde), wellgevormdheid met regel/kolom, versiemelding, validatie van sleutels en verwijzingen, dubbele RankID expliciet gemeld, samplemap kan het pakket niet verlaten |
| **C** | samplekeuze-transpositie los van stemming en plausibiliteitsgrens: 37 chimes uit één WAV op de juiste toon |
| **D** | release in het hoofdbestand start bij de eerste cue; losse releases beginnen op frame 0; ontbrekende marker/loop wordt gemeld; `ReverbTailTruncation` gelezen en als niet beschikbaar gemeld |
| **E** | willekeurige stemafwijking per pijp; verslag meldt golfvormtremulant, deelbereiken, gemengde zwelkast per register, windflowvariatie |
| **F** | koppels getoetst (golden + engine); `DefaultToEngaged`, `StoreInGeneralCombinations`, shortcutpistons |
| **G** | integratieproef (CI en engine), benchmark, A/B-protocol met meethulp, documentatie en handleiding in 7 talen |

## 2. Beslissingen en benaderingen

- **Voorcontrole zonder markers.** De preflight leest de markers van de samplebestanden niet (op de proefset 1007 ms → 4 ms);
  die komen bij het echte laden in het verslag.
- **Stemafwijking** (`Pitch_MaxRandomTuningErrorHz`): vast per pijp, afgeleid van rang en noot, kans ~ u²
  (gewogen naar zuiver). De Hauptwerk-kansverdeling is niet gedocumenteerd: benadering, aan/uit via de test-API.
- **Samplekeuze-transpositie:** de bestandsnoot geldt als nominale toon (plus de basisharmonische bij "gestemd
  aangenomen"); een `smpl`-pitch in de WAV wint in de hertemper-stand.
- **Golfvormtremulant:** niet gelezen, geen eigen decoder: het bestandsformaat is niet openbaar gedocumenteerd en er zijn
  geen trembestanden. De tremulant klinkt als eenvoudige modulatie en dat staat in het verslag.
- **Combinaties:** JM kent alleen algemene combinaties; `StoreInDivisionalCombinations=N` wordt gemeld, niet toegepast.
- **Koppelketens:** ongewijzigd (JM schakelt doorkoppelend, GrandOrgue-gedrag). Zie open punten.

## 3. Testmatrix (plan §4) met resultaat

| fixture / scenario | resultaat |
|---|---|
| Piteå 3.00, alleen pakket 537000 | ✅ 59 verwacht / 1 speelbaar, 758 en 759 met naam ontbrekend, vorig orgel blijft; chimes: 37 pijpen, één WAV, +1200 ct per octaaf gemeten, uiteinden −1300/+2300 ct in de unit-test |
| Piteå 3.00 met synthetische 758/759 | ⛔ niet gebouwd. Wel: structuur zonder samples (4/59/79/67/25) in de corpustest |
| Litomyšl droog / 16-bit wet / 24-bit wet | ✅ structuur 4/56/74/65/19 per variant en pakketlijsten 322 / 319–321 / 311–318 (corpustest). De aparte primaire release van alleen de 24-bit-variant is uit de definitie geteld, niet als aparte test vastgelegd |
| Synthetische CODM-miniaturen | gedeeltelijk ✅: andere quotes/witruimte, dubbele RankID, onbekende versie, gemengde zwelkast, golfvormtremulant, pakket buiten de root, meer cues, geen markers, losse releases. ⛔ niet apart getest: ontbrekende RankID, externe rang, alternate rank, andere actiecodes (die worden wel gemeld in het verslag) |
| GrandOrgue en native/compacte Hauptwerk | ✅ alle bestaande tests groen (vpo-sampler 159, vpo-app 431); Giubiasco laadt zonder dialoog |
| Runtime | ✅ NoteOn/NoteOff met direct + koppel + wissel, geen hangers (engine). ⛔ dezelfde fysieke rangpijp via twee registers: niet aangetoond, runtime niet gewijzigd |
| App en performance | ✅ preflight laat het orgel ongemoeid, kapot bestand faalt netjes, verslag zichtbaar, benchmark reproduceerbaar. Dialoog en setzer **niet in de UI doorgeklikt** |

**Commando's (alle groen op 3 november):** `cargo test --offline --workspace` (vpo-sampler 159, vpo-app 431, vpo-audio 49, rest groen),
`python VirtualPipeOrgan/testscripts/check_i18n_keys.py`, `npx vite build`; ook `cargo test --locked -p vpo-sampler --lib` slaagt
(de nieuwe afhankelijkheid `roxmltree` stond al in de lock en `Cargo.lock` is gecommit).
Engine-integratie: `python VirtualPipeOrgan/testscripts/codm_integratie.py` (33 controles, 0 fouten). Niet in CI
(vraagt audio-uitvoer, ffmpeg, numpy).

## 4. Prestatie (machine: Intel i9-10885H, 31,8 GB, Windows 11, WASAPI Realtek, 48 kHz, buffer door het apparaat bepaald)

Volledig rapport: `testen/codm-benchmark/benchmark-*.md`. Samenvatting, per orgel een verse app:

| orgel | registers | laadtijd | piek-RAM | stemmen (tutti) | callback p99 / max | late callbacks |
|---|---|---|---|---|---|---|
| eigen CODM-proeforgel | 3 | 0,1 s | 208 MB | 48 | 0,059 / 0,071 | 0 |
| GrandOrgue-set ter vergelijking | 22 | 6,6 s | 3823 MB | 437 | 0,29 / 0,295 | 0 |

De proeforgels zijn te klein om de limiet te laten zien; ze bewijzen het meetgereedschap. Een grote CODM-set is er niet.
De MIDI→audio-waarde (32 ms op de proeforgel) is een bovengrens via HTTP en pollen, geen echte vertraging.
`MAX_POLYPHONY` (32768) is een codeplafond en geen prestatieclaim.

## 5. A/B-klankvergelijking

Protocol en meethulp zijn gebouwd (`VirtualPipeOrgan/docs/AB_KLANKPROTOCOL.md`, `testscripts/ab_vergelijk.py`).
**Er is geen vergelijking uitgevoerd**: dat vraagt een complete originele set met al haar pakketten én een
referentiespeler. Uitkomst voor elke CODM-set: nog niet klinkend gevalideerd.

## 6. Resterende blokkades en open besluiten

| punt | reden |
|---|---|
| Golfvormtremulant | formaat niet openbaar gedocumenteerd, geen bestanden |
| Zwelkast/tremulant per rang binnen één register | vraagt de per-pijp-kastenroute van JM-ACOUSTICS en een bron die het gebruikt |
| Reverb tail truncation, windflowvariatie | geen bron gebruikt het (modus 1 overal); schaling niet verifieerbaar |
| **Koppelketens (besluit gevraagd)** | JM koppelt door (GrandOrgue); de CODM-gids beschrijft géén keten. Eén aanpassing voor CODM als je dat wilt |
| A4 laadfasen en annuleerknop | niet bewezen dat elke fase veilig kan stoppen |
| B4 compacte native-ODF-veldmapping met onzekerheden | niet gedaan |
| Piston-knop en grafische console (F3) | niet gebouwd |
| Dezelfde rangpijp via twee registers | semantiek niet aangetoond |
| UI-doorklik van dialoog en setzer | backend en build getest, schermen niet bediend |
| Echte Piteå-/Litomyšl-samples | pakketten 758/759 en de commerciële samples ontbreken |
| Cue-semantiek bij echte natte releases | alleen gids en eigen proefbestanden |

## 7. Gewijzigde bestanden (sinds 0.9.41, voor de G-wijzigingen nog te committen)
- `VELDMATRIX-CODM-2026-11-03.md`
- `VirtualPipeOrgan/Cargo.lock`
- `VirtualPipeOrgan/src-tauri/src/commands.rs`
- `VirtualPipeOrgan/src-tauri/src/import_pakketroot.rs`
- `VirtualPipeOrgan/src-tauri/src/main.rs`
- `VirtualPipeOrgan/src-tauri/src/state.rs`
- `VirtualPipeOrgan/src-tauri/src/test_api.rs`
- `VirtualPipeOrgan/testscripts/maak_codm_koppeltest.py`
- `VirtualPipeOrgan/testscripts/maak_codm_releasetest.py`
- `VirtualPipeOrgan/ui/src/App.svelte`
- `VirtualPipeOrgan/ui/src/components/ImportVerslag.svelte`
- `VirtualPipeOrgan/ui/src/components/SetzerBar.svelte`
- `VirtualPipeOrgan/ui/src/lib/locales/de.json`
- `VirtualPipeOrgan/ui/src/lib/locales/en.json`
- `VirtualPipeOrgan/ui/src/lib/locales/es.json`
- `VirtualPipeOrgan/ui/src/lib/locales/fr.json`
- `VirtualPipeOrgan/ui/src/lib/locales/it.json`
- `VirtualPipeOrgan/ui/src/lib/locales/nl.json`
- `VirtualPipeOrgan/ui/src/lib/locales/pl.json`
- `VirtualPipeOrgan/vpo-sampler/Cargo.toml`
- `VirtualPipeOrgan/vpo-sampler/src/grandorgue.rs`
- `VirtualPipeOrgan/vpo-sampler/src/hauptwerk.rs`
- `VirtualPipeOrgan/vpo-sampler/src/hauptwerk_codm.rs`
- `VirtualPipeOrgan/vpo-sampler/src/import_report.rs`
- `VirtualPipeOrgan/vpo-sampler/src/lib.rs`
- `VirtualPipeOrgan/vpo-sampler/src/loader.rs`

Nog te committen (G): `VirtualPipeOrgan/testscripts/codm_*.py`, `ab_vergelijk.py`, `docs/AB_KLANKPROTOCOL.md`, README, `SAMPLE_SETS.md`, `TEST_API.md`, de zeven handleidingen (HTML + PDF), de snelle voorcontrole en de pijplijntest.
