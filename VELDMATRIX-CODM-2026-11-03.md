# Veldmatrix CODM — JM-Orgue

**Peildatum:** 3 november 2026 · **Checkout:** `orgelmaker/JM-Orgue` master, commit `1171585` (0.9.41).
Het externe plan noemde `5272a67`; die commit bestaat hier niet. Deze matrix is opgesteld tegen de werkelijke code.

**Bronnen:** vier oorspronkelijke CODM-definities (`FileFormatVersion="3.00"`): Piteå (4 divisies, 59 stops, 79 StopRanks, 67 unieke ranks, 25 koppels) en drie Litomyšl-varianten (elk 4/56/74/65). Alle vier in `testen/codm/`; minimaal Piteå-pakket (native-ODF + één Chime-WAV) in `testen/codm/pitea-minimal/`.

**Statuslegenda:** ✅ ondersteund · 🔧 nieuw werk · ≈ benadering · ⛔ niet mogelijk zonder bron/samples.

---

## 1. Structuur en identiteit — grotendeels ✅, detectie 🔧

| Veld / aspect | Status | Opmerking | WP / toets |
|---|---|---|---|
| `FileFormat` / `FileFormatVersion` detectie | 🔧 | `is_codm()` doorzoekt alleen eerste 1024 bytes op `FileFormat="CustomOrgan"` (één quotevorm). Geen BOM/witruimte/enkele-quote/case. | B1 |
| Objectlijst, objecttags (hoofdletter-mix `<Rank>`/`<rank>`) | ✅ | Wordt al case-tolerant gelezen. | B1 |
| `DivisionCode`, `StopCode` (2DNN), `RankID`, `CouplerCode` | ✅ | Structuur + koppeldecodering werken. | B3, F1 |
| Dubbele `RankID` | ≈ | Nu stil `first wins` (Piteå RankID 33). Moet expliciete, gemelde regel worden. | B3 |
| `AlternateRankID` / `StopCodeToSwitchToAlternateRank` / `ExternalRankID` / `AlternateExternalRankID` | 🔧 | Veld aanwezig (79×/74×) maar leeg in deze vier bronnen → niet gebruikt. Compatibiliteitsdoel, synthetische fixture nodig. | B3, C |
| `RankTypeCode`/`ActionTypeCode`/`ActionEffectCode` ≠ 1/1/1 | 🔧 | Alle 301 StopRank-rijen zijn 1/1/1. Andere combinaties = synthetische fixture. | B3 |
| Unieke-sleutel-/verwijzingsvalidatie | 🔧 | Nog geen expliciete validatie met gerichte fout. | B3 |

## 2. Volledigheid, pakketten en importverslag — 🔧 (Werkpakket A)

| Veld / aspect | Status | Opmerking | WP |
|---|---|---|---|
| `RequiredInstallationPackage` naar rol (hoofd/release/trem/beeld) | 🔧 | `sample_pakketten()` inventariseert alleen hoofdrangpakketten; rol ontbreekt. | A1, B5 |
| Gestructureerd `ImportReport` (verwacht/geladen/overgeslagen + reden) | ✅ | Gebouwd (commit A+B, 0.9.42-kandidaat). | A1 |
| Preflight vóór vervangen: essentiële afhankelijkheid ontbreekt | 🔧 | Nu: zolang ≥1 stop bouwt = "geslaagd". Piteå met alleen 537000 → 1 register, lijkt gelukt. | A2 |
| Behoud vorig orgel + audio bij afbreken/falen | 🔧 | Te verifiëren en borgen. | A2 |
| Pakketroot-herstel via mapkiezer, per orgel opgeslagen | 🔧 | Bestaat niet. | A3 |
| Laadfasen (parse/controle/samplebuffer) + heropenbaar verslag | 🔧 | Nu generieke laadmelding. | A4 |

## 3. Samplekeuze, toonhoogte, gedeelde pijpen — deels ✅, kern 🔧 (Werkpakket C)

| Veld / aspect | Status | Opmerking | WP |
|---|---|---|---|
| `Samples_MainSampleFileFolder`, `MIDINoteNumIncrementFromPipesToSamples` | ✅ | Gelezen. | C |
| `Samples_SampleMIDINoteNumOfFirst/LastAvailableSample`, `MIDINoteNumIncBetweenSamples` | 🔧 | Niet gelezen → geen weet van sample-gaten/sprongen. | C1 |
| `Samples_RankBasePitch64ftHarmNumIfAssumedTunedToConcertPitch` | 🔧 | Niet gelezen; nodig voor juiste nominale samplepitch. | C1, C2 |
| Gedeelde WAV over 37 virtuele Chimes-pijpen (`061-c#.wav`) | 🔧 | WAV heeft geen `smpl`-pitch; fallback neemt doelnoot → **alle 37 toetsen klinken gelijk**. Bronnoot-transpositie herstellen, los van ±600 ct-plausibiliteitsgrens (>6 halve tonen moet kunnen). | C1, C2 |
| Transpositie in `Origineel` én andere stemmingen | 🔧 | Samplekeuze-transpositie scheiden van stemming/intonatie. | C1 |
| Zelfde fysieke rangpijp via twee stops = één klinkende pijp | ≈ | Refcount per `(stop_id, pipe_num)` dekt directe route + koppel naar zelfde stop, niet identiteit over twee stops. Alleen aanpassen als fixture/native-ODF het bevestigt. | C3 |
| Gedeelde buffer + eigen stem/volume per virtuele pijp | 🔧 | Buffer delen, instellingen gescheiden houden. | C3 |

## 4. Releases en articulatie — deels ✅ (Werkpakket D)

| Veld / aspect | Status | Opmerking | WP |
|---|---|---|---|
| Korte/middellange release + grenzen (`MaxNoteTimeMsFor…`) | ✅ | Gelezen; aanslag-release standaard sinds 0.9.40. | D1 |
| Aparte primaire release (`Samples_ReleaseSamplesStoredInSeparateFilesIfNotPercussive`, `RelSampleFileFolder…`) | 🔧 | Veld aanwezig (68×/65×); alleen 24-bit wet Litomyšl heeft aparte primaire release. Rol per variant onderscheiden. | D1 |
| `IsPercussive` → geen release | ✅/🔧 | Veld gelezen; gedrag verifiëren. | D1 |
| WAV-cue-semantiek per formaat (begin/eind release) | 🔧 | Loader neemt hoogste `cue` volgens GO-regel. CODM-regel verifiëren met echte/synthetische WAV; GO-route niet blind wijzigen. | D2 |
| `ReverbTailTruncation_*` | 🔧 | Aanwezig (135×/130×), niet gelezen; begintransiënt behouden. | D3 |

## 5. Tremulant, zwelkast, wind — grotendeels 🔧 (Werkpakket E)

| Veld / aspect | Status | Opmerking | WP |
|---|---|---|---|
| `Trem_WaveformSampleFileFolder` + `Trem_*` samplevelden | 🔧 | 37 (Piteå) / 34–35 (Litomyšl) rangen hebben golfvormtremulant. Nu: `FrequencyHz` → simpele LFO met vaste `amp_mod_depth=18`. Apart **golfvormgestuurd** type nodig; golfvorm niet als attack-WAV spelen. | E1 |
| Ontbrekend trembestand | ≈ | Moet als verminderde dekking in verslag; LFO-fallback alleen expliciet. | E1 |
| `Encl_EnclosureCode` | ✅ | Gelezen. | E2 |
| `Encl_FiltParamWhenOpen/Clsd…`, `Encl_MIDINoteNumOfFirst/LastEnclosedPipe…` | 🔧 | 272×/260× aanwezig; per-pijp filter/demping ontbreekt (`windchest_group` te grof). | E2 |
| `WindModel_*`, `WindSupply_DivisionCodeOfWindchest`, `MaxFlowRandomisationPct` | ≈ | Naar bestaand JM-windmodel vertalen, stabiele seed per pijp; benadering melden. | E3 |
| `MaxFrequencyRandomisationPct`, `MaxDepthRandomisationPct` (stemafwijking) | ≈ | Naar JM-stemmodel. | E3 |
| `Position_*` / afstand, reverb-truncatie als klankveld | ≈ | Alleen toepassen bij versiebevestigde betekenis; anders verminderde dekking melden. | E4 |

## 6. Bediening, combinaties, presentatie — deels ✅ (Werkpakket F)

| Veld / aspect | Status | Opmerking | WP |
|---|---|---|---|
| Gewone/16'/4'/bas/melodie/Unison Off-koppels | ✅ | Werken sinds 0.9.39. Richting/transpositie/grenzen/ketens hertesten met 25 Piteå- + 19 Litomyšl-koppelcodes. | F1 |
| `DefaultToEngaged` | 🔧 | 86×/77× aanwezig, niet gelezen. | F2 |
| `StoreInDivisionalCombinations` / `StoreInGeneralCombinations` | 🔧 | Aanwezig; Piteå sluit 16 van 25 koppels uit divisional uit. Respecteren in setzer/herstel. | F2 |
| `AccessibleForMIDIInputAndOutput`, `SwitchIsLatching`, `…FromWhichToCopyState` | 🔧 | Niet gelezen. | F2 |
| `shortcutpiston` als actie op doelobject | 🔧 | Objecttype aanwezig; niet verwerkt. | F2 |
| `CustomDisplay*`, beeld-/toets-/tekststijlen, `BackgroundImage…` | ≈/⛔ | Optionele presentatie. Grafische console pas bij aanwezige beelden + licentie; volledige bediening moet zonder beelden werken. Ontbrekend beeldpakket los melden van ontbrekende samples. | F3 |

## 7. Betrouwbaarheid, meting, documentatie — 🔧 (Werkpakket G)

| Aspect | Status | Opmerking | WP |
|---|---|---|---|
| Deterministische integratieproef parse→preflight→resolutie→preload→render→los→rapport | 🔧 | Met zelfgemaakte mini-WAV's; geen commerciële pakketten in repo. | G1 |
| Corpustest vier CODM-XML's van `ignored` → optioneel met skipreden | 🔧 | Bestaat als `ignored` (regel ~949). **Let op:** die test verwacht 0 Litomyšl-koppels, plan zegt 19 — hercontroleren. | G1 |
| Benchmark (parserduur, RAM, stemmenpiek, p95/p99/max callback, dropouts, MIDI-latentie) | 🔧 | `MAX_POLYPHONY=32768` is codeplafond, geen prestatieclaim. | G2 |
| A/B-klankprotocol vs. referentiespeler | ⛔ | Vereist complete originele set + referentie-opname; zonder commerciële samples = "nog niet klinkend gevalideerd". | G3 |
| README/handleiding/vertalingen: CODM, pakketmap, `.hbw`, verslag lezen | 🔧 | Hoofd-README noemt CODM niet. | G4 |

---

## Harde grenzen
- Beveiligde `.hbw` en binaire/versleutelde definities: herkend, niet decodeerbaar — niet omzeilen.
- Piteå-pakketten 758/759 en Litomyšl commerciële samples niet aanwezig → geen klinkende validatie, geen "volledig ondersteund".
- Geen omgebouwde SWOP-definities als bewijs of fixture.

## Voorgestelde uitvoervolgorde (plan §5)
**A → B** (importveiligheid + robuuste parser/validatie) · dan **C → D** (toonhoogte/gedeelde pijpen + releases) · dan **E → F** (klankvelden + bediening) · ten slotte **G** (tests/benchmark/docs). A–D vóór klank/UI, anders lijkt een half geladen of verkeerd gestemde set ten onrechte bruikbaar.

---

## Voortgang (bijgewerkt na werkpakket D)

| WP | Stand | Commit |
|---|---|---|
| A+B | gebouwd; open: A4 (laadfasen/annuleren), B4 (compacte native-ODF-veldmapping) | ab8a986 |
| C | gebouwd en gemeten: chimes +1200 ct/octaaf (voorheen alle gelijk) | 07e4a19 |
| D1 | korte/middel/lang op de grenzen (≤ grens), aparte primaire release; gemeten 600/900/1300 Hz-releases | zie D-commit |
| D2 | CODM-release in het hoofdbestand start bij de EERSTE cue (GO: hoogste cue blijft); losse release start op frame 0; geen marker/loop → waarschuwing | zie D-commit |
| D3 | `ReverbTailTruncation_*` gelezen; in alle vier de bronnen modus 1 (geen truncatie) → geen effect; modus 2/3 wordt per rang als "niet beschikbaar" gemeld, niet toegepast | zie D-commit |
| E1 | golfvormtremulant: bestandsformaat niet openbaar gedocumenteerd en geen trembestanden beschikbaar → ⛔ niet te implementeren/valideren; LFO-benadering expliciet gemeld in het verslag, ontbrekende trembestanden per rang gemeld | zie E-commit |
| E2 | zwelkast/trem per hele rang werkt (bronnen: alle 38 ingesloten rangen identieke filterwaarden, nergens een deelbereik). Per-rang/per-pijp-routing binnen één stop NIET gebouwd: vraagt de per-pijp-kastenroute van JM-ACOUSTICS (UI-gekoppeld); nu gemeld als waarschuwing | zie E-commit |
| E3 | `Pitch_MaxRandomTuningErrorHz` toegepast (vast per pijp, aan/uit via `/settings/codm_random`), gemeten −7,4 ct = verwacht; windflowvariatie en trem-dieptevariatie niet toegepast (gemeld) | zie E-commit |
