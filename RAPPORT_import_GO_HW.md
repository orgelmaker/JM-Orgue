# JM-Orgue: GrandOrgue- en Hauptwerk-import, inschaling en loop-realisme

Alle paden hieronder zijn relatief t.o.v. `C:\Bronbestanden\JM-Orgue\VirtualPipeOrgan\`. Getest tegen de echte sets in `C:\Bronbestanden\JM-Orgue\testen\` (GrandOrgue: Bureå gravkapell + GreenPositiv; Hauptwerk: Ledziny St. Clement + Saint-Jean-de-Luz Choeur). De kernclaims zijn handmatig in de code nagelezen.

**Kort antwoord op je vier vragen:**
1. **GrandOrgue:** de structuur (manualen, stops, koppels, voetmaten, bas/discant-splits) komt goed binnen, maar van alle pijp-attributen bereikt **alleen het samplepad** de audio-engine. Plus één echte bug die toetsen stil maakt.
2. **Hauptwerk:** de importer zelf is netjes gebouwd, maar **via de UI kun je geen Hauptwerk-orgel openen** — het hele pad is in de praktijk onbereikbaar.
3. **Inschaling:** nee. Gain, AmplitudeLevel, PitchTuning en PitchCorrection worden geparset (of niet eens) maar **nergens toegepast**. Registerbalans en stemming wijken hoorbaar af van hoe de sampleset bedoeld is.
4. **Loop/crossfade:** de sustain-loop is verrassend goed — beter zelfs dan je zou verwachten. Maar de **release** (loslaten van een toets) is het grootste realisme-gat: 42 ms kunstmatige fade in plaats van de opgenomen kerkakoestiek.

---

## 1. Hoe gaat hij om met GrandOrgue-bestanden?

**Route:** `commands.rs:508` (`do_load_organ`) → `vpo-sampler/src/grandorgue.rs:666` (`OrganDefinition::load`) → `OdfParser::parse` (grandorgue.rs:213). Let op: `GrandOrgueLoader`/`pool.rs`/`streaming.rs` in vpo-sampler zijn **dode code** — de live keten is `load_audio_preload` (loader.rs) + achtergrondstreaming in audio.rs.

### Wat goed werkt
- **Encoding:** INI-parsing met Latin-1-fallback (grandorgue.rs:218-222) — Bureå's 'Rörflöjt' parset zonder crash.
- **Structuur:** [Organ]-metadata, manualen incl. MIDI-mapping en toetsbereik, [Stop]-refs, koppels ([Coupler] DestinationManual/Keyshift → sub/super/unison, grandorgue.rs:612-634 + commands.rs:2337-2384 — Bureå's Man/Ped werkt).
- **Beide pijpformaten:** modern rank-formaat ([Rank] + FirstPipeNumber/PipeCount, grandorgue.rs:424-486) én oud inline-formaat (Pipe999=, grandorgue.rs:489-525). Bas/discant-splits via FirstAccessiblePipeLogicalKeyNumber kloppen (Bureå Salicional-discant start correct op MIDI 60).
- **Voetmaten:** HarmonicNumber → 16', 8', ook mutaties zoals 1 1/3' (grandorgue.rs:731-759).
- **Noise-filter:** blower/key action/ambient worden correct géén registerknop (grandorgue.rs:128-139 + commands.rs:572/647) — conform je eigen designkeuze. In GreenPositiv worden 14 van 19 "stops" terecht weggefilterd.
- **REF-dedup:** gedeelde samples worden 1× geladen en via Arc gedeeld (commands.rs:585-622).

### Wat we missen — en dit is fors

**Bug (stille toetsen): REF-pipe-borrowing verkeerd geresolved.** `REF:mm:ss:pp` gebruikt een *manual-relatieve* stopindex, maar `resolve_reference` zoekt op *globale* stop-id en negeert het manual-veld (grandorgue.rs:692-698: `find(|s| s.id == *stop)`, `manual: _`). In Bureå verwijst Stop101 Salicional 8' bas naar `REF:001:003:001..` = de 3e stop van Manual001 (Stop103 Gedackt). Er bestaat geen globale Stop003 → resolve levert None → **de 12 laagste toetsen (MIDI 36-47) van Salicional 8' zijn volledig stil** (burea_gravkapell.organ:376 e.v.).

**Alleen het pad overleeft het laden.** commands.rs:577-580 destructureert `PipeDef::Sample { path, .. }` — amplitude_level en pitch_correction die de parser wél invult worden weggegooid. Gevolgen: zie sectie 3.

**PipeNNNPitchTuning wordt niet eens geparset** (alleen PitchCorrection, grandorgue.rs:475/511; bovendien als `i32` terwijl PitchTuning een float in cents is — `"4.79062".parse::<i32>()` zou zelfs falen). GreenPositiv heeft 212 PitchTuning-regels; **28 pijpen spelen ≥1 semitoon vals (tot +899,7 cent = 9 semitonen**, GreenPositiv.organ:4598) omdat toppipes hergebruikte lagere samples zijn — de topoctaaf van Flet kryty 8', Flet 4', Pryncypał 2' en Kwinta speelt de verkeerde toonhoogte. Plus 83 pijpen met kleine detuning (±5-90 ct).

**Release-samples worden volledig genegeerd** — geen enkele `Release`-key wordt geparset. GreenPositiv heeft 497 Release-regels (kerkakoestiek per pijp, GreenPositiv.organ:1457-1461). Zie sectie 4 voor het hoorbare gevolg.

**Verdere gaten (met testset-bewijs):**
| Gat | Gevolg | Bewijs |
|---|---|---|
| Zweldetectie eist naam-match windchest = divisienaam | Bureå: Comment='Enclosed Manual wind-chest' ≠ 'Manual' → has_swell=false, zwelkast verschijnt niet | commands.rs:541-547 + .organ:181-190 |
| Tremulant-parameters geparset maar ongebruikt | Bureå Period=175 ms (5,7 Hz)/AmpModDepth=20% vervangen door generieke LFO 6 Hz | grandorgue.rs:591-609 vs audio.rs:704-706 |
| Multi-attack (Attack999) niet geparset | Geen round-robin; herhaalde noten machinaal identiek (GreenPositiv: 653 Attack-regels) | grandorgue.rs:451-477 |
| Percussive geparset maar genegeerd | Klok/glockenspiel zou eindeloos loopen i.p.v. één keer uitklinken | grandorgue.rs:404 + audio.rs:283-286 |
| ODF-looppunten (Loop999Start/End) niet geparset | Sets zonder smpl-chunk vallen terug op de ¾-fallback-loop | loader.rs:115-172 is de enige loop-bron |
| Rank-eenhedenmix | PipeNGain (dB!) belandt in het amplitude_level-veld (procent-semantiek) | grandorgue.rs:474 vs 515 |
| Switch/Function-logica, ReversiblePiston, Panel-secties niet geparset | Voor deze twee sets beperkt; relevant voor grotere sets met switch-geschakelde stops | GreenPositiv.organ:99-113 |

---

## 2. Hoe gaat hij om met Hauptwerk-bestanden?

**Route:** commands.rs:519-522 detecteert Hauptwerk via `is_hauptwerk_path` en roept `load_hauptwerk` aan (vpo-sampler/src/hauptwerk.rs); daarna dezelfde preload/streaming/DTO-route als GrandOrgue.

### Wat goed werkt
De importer zelf is degelijker dan je zou verwachten voor een "minimale" vertaling:
- **Eigen XML-parser zonder dependency** met entity-decoding ('Subbaß', 'Flûte' — hauptwerk.rs:433-477, met tests) en self-closing tags.
- **Multi-package-resolutie:** zowel numerieke ('002219') als benoemde package-mappen, geverifieerd op beide testsets (hauptwerk.rs:525-576).
- **Correcte MIDI-mapping** per StopRank incl. gaten (PipeDef::Empty houdt toets→pijp uitgelijnd, hauptwerk.rs:172-184), footage via dezelfde 64'-conventie, koppels uit KeyAction-routing (hauptwerk.rs:248-298), naam-schoonmaak ('P  Subbass 16' → 'Subbass 16').
- **Noise-filtering is hier structureel robuust:** Hauptwerk modelleert blower/tracker-noise als ranks die níét aan Stop-objecten hangen, dus ze worden nooit een registerknop — in lijn met je projectbesluit.
- **Loops** komen uit de WAV-smpl-chunks (Hauptwerk embedt die), dus sustain-loops werken.

### Wat we missen

**Functioneel het ergst: je kunt via de UI geen Hauptwerk-orgel openen.** De filepicker 'Open orgelbestand' filtert op `extensions: ['organ']` (ui/src/components/Console.svelte:1530-1533, door mij bevestigd) — `*.Organ_Hauptwerk_xml` valt daarbuiten. Het hele importpad is alleen via de test-API bereikbaar. Consistent hiermee: `organ_library.json` bevat nul Hauptwerk-orgels. Daarbovenop: de **herlaadroutine na een audio-device-wissel** routeert op '.organ'-suffix (ui/src/App.svelte:331-336 + src-tauri/src/test_api.rs:283-289), dus een geladen Hauptwerk-orgel wordt naar de mappen-scanner gestuurd en raakt kwijt.

**Klankgaten:**
- **Release-samples expliciet genegeerd** (hauptwerk.rs:20-22 documenteert dit zelf; `Pipe_SoundEngine01_ReleaseSample` wordt nooit geëxtraheerd). Ledziny: 504 releases, SJDL: 13.578 releases incl. R0/R1/R2 per toetsduur. Hauptwerk-sets ontlenen hun karakter hier grotendeels aan.
- **Zwelkast niet geparset:** SJDL heeft 2 Enclosures + 1956 EnclosurePipes (Récit expressif); de importer levert lege lijsten (hauptwerk.rs:330-331) → nooit een zweltrede.
- **Tremmed-laag genegeerd:** SJDL heeft op 1920 pijpen een 2e laag met tremulant-opnamen; `or_insert` houdt alleen de eerste laag (hauptwerk.rs:84-89), en `number_of_tremulants: 0` → nooit een tremulant, hoewel de engine trem-buffers wél ondersteunt (audio.rs:929).
- **Mic-perspectieven:** eerste StopRank in documentvolgorde wint (hauptwerk.rs:159); SJDL's rear/dry zijn onbereikbaar. Geen dubbel geluid — dat is goed — maar ook geen keuze.
- **Per-pijp gain/pitch uit de XML genegeerd** (AmpLvl_LevelAdjustDecibels, Pitch_ExactSamplePitch — alles op 100/0, hauptwerk.rs:177-195).
- **Fragiele detectie:** `is_hauptwerk_path` is een substring-check op 'hauptwerk' in het pad (hauptwerk.rs:338-340) — een `.organ`-bestand in een map met 'Hauptwerk' in de naam wordt misrouted. Detectie op de extensie `.Organ_Hauptwerk_xml` is robuust.
- **Encrypted sets:** geen detectie; elke sample-preload faalt stil met een log-warn (commands.rs:606-609) → orgel verschijnt met stille registerknoppen zonder uitleg.

---

## 3. Wordt alles goed ingeschaald (volume/pitch)?

**Nee — dit is naast de releases het grootste eerlijke antwoord.** De hele intonatielaag van de sampleset-maker wordt genegeerd:

- **Stop-AmplitudeLevel geparset maar ongebruikt** (grandorgue.rs:403 → weggegooid op commands.rs:578). Bureå: Subbas 16' staat op 240, Principal 2' op 80 — de onderlinge registerbalans wijkt **tot ~9,5 dB** af van hoe GrandOrgue dit orgel laat klinken.
- **[Organ] Gain niet geparset** (parser leest alleen AmplitudeLevel, grandorgue.rs:311). GreenPositiv: Gain=14 → het hele orgel 14 dB zachter dan bedoeld; orgels onderling inconsistent in je bibliotheek.
- **Per-pijp gain genegeerd.** GreenPositiv Kwinta 1 1/3': alle 44 pijpen Gain=-4 (geleende Pryncypał-rank die zachter moest) → klinkt 4 dB te hard.
- **Pitch: er wordt niet gerepitcht.** 1:1 pijp→sample; de rate-verhouding corrigeert alleen device-samplerate (audio.rs:156-157). PitchTuning (zie sectie 1) en stop-PitchCorrection (Bureå Voix Celeste 11 ct zweving, .organ:577) doen niets.
- **Wat wél goed is:** offline resampling naar device-rate is hoogwaardig (rubato FFT, loader.rs:282-293); MIDI-velocity schaalt helaas het volume (audio.rs:145) — een orgelpijp klinkt altijd vol, GrandOrgue negeert velocity terecht; en alle stereo samples worden **mono gedownmixt** ((l+r)*0.5, loader.rs:245-247) met een mono-galmbus (audio.rs:1383, diffuse L=R op 1401-1404) — het opgenomen ruimtelijke beeld van natte sets gaat verloren. De altijd-actieve tanh-softclip (audio.rs:1412) comprimeert en kleurt vol werk hoorbaar.

**Laaghangend fruit:** audio.rs heeft al een werkend per-pijp voicing-mechanisme (`SetPipeVoicing`: volume_db + pitch_cents, audio.rs:1096-1101, toegepast op 1277-1298). ODF-Gain/AmplitudeLevel/PitchTuning kunnen bij het laden naar ditzelfde mechanisme gerouteerd worden zonder de audio-thread te herontwerpen. Let bij het fixen op de eenheden: AmplitudeLevel is 0-1000 (%, 100=neutraal), Gain is dB, ze stapelen hiërarchisch (organ × stop × rank × pipe), en PitchTuning is een float in cents (het huidige veld is i32).

---

## 4. Is de crossfade/loop-functie realistisch?

**De sustain-loop: ja, verrassend goed.** Dit is het sterkste stuk van de engine:
- smpl-chunk-looppunten worden gelezen (loader.rs:115-172) en gebruikt;
- op de loopnaad geen harde sprong maar een **equal-power (sin/cos) crossfade** van 256 tot 24.000 samples ≈ tot 500 ms @48k (audio.rs:304-390) — dat maskeert ook matige looppunten, toleranter dan GrandOrgue's default;
- daarbovenop een **offline fase-optimalisatie** van loop_end (SSD-match, loader.rs:73-112);
- beide met regressietests op klikvrijheid (audio.rs:402-443, loader.rs:904-933).

Twee kanttekeningen: (a) in de **eerste ~2 s van elke noot** (preload-fase, tot de background-load klaar is) worden de échte looppunten genegeerd en loopt de voice over het laatste ¾ van het attack-buffer (audio.rs:290-296) — bij trage disk of veel gelijktijdige noten hoor je de attack-transient periodiek terugkeren; (b) de lange naadcrossfade geeft bij strijkende registers met inharmonische boventonen een licht chorus-effect in de 0,5 s vóór de naad.

**De release: nee, en dit is hét hoorbare gat.** Bij het loslaten van een toets klinkt geen opgenomen akoestiek maar een **synthetische lineaire fade van ~42 ms** (envelope_speed 0.0005/sample, audio.rs:212-216 — door mij bevestigd). Geen release-samples (GO noch HW), geen toetsduur-afhankelijke keuze (MaxKeyPressTime / R0-R2-mappen), geen release-alignment. Elke noot eindigt droog en abrupt; alleen de (mono) galmbus maskeert dit deels. Dit is precies waar GrandOrgue en Hauptwerk hun realisme vandaan halen — natte sets als GreenPositiv en SJDL ontlenen hun karakter grotendeels aan de release-staart.

**Tremulant-afspelen is relatief goed geregeld** (echte trem-samples met 140 ms crossfade op klinkende voices + LFO-fallback, audio.rs:194-223/943-987/1269-1296) — alleen bereiken de ODF-parameters en de HW-tremmed-laag dit mechanisme dus niet.

**loop_tool.rs (je eigen loop-generator):** degelijk middenklasse-werk — autocorrelatie-pitchdetectie met octaafcorrectie, zero-crossing loopstart, SSD-fase-match, gebakken equal-power crossfade, correcte smpl-schrijver met backups. Prima voor droge huisopnamen (de doelgroep). Tekortkomingen: de zoekgrens 55-1500 Hz **mist 16'/32'-pedaalfundamenten** (16' C = 32,7 Hz → de tool pakt een harmonische → langzame zweving in lage pedaalsustains, loop_tool.rs:133-134), max looplengte 1,2 s is te kort voor natte samples, en één loop i.p.v. multi-loop. Geen Hauptwerk/LoopAuditioneer-niveau, maar dat hoeft ook niet.

---

## Geprioriteerde verbeterlijst

**Hoog (hoorbaar of blokkerend):**
1. **REF-bug fixen** — resolve manual-relatief i.p.v. globale stop-id (grandorgue.rs:692-698). Herstelt 12 stille toetsen in Bureå. *Effort: S*
2. **Hauptwerk bereikbaar maken** — filepicker-extensies uitbreiden met `Organ_Hauptwerk_xml` (Console.svelte:1532) + herlaad-routing op source_type i.p.v. '.organ'-suffix (App.svelte:332, test_api.rs:284). *Effort: S*
3. **ODF-inschaling toepassen** — PitchTuning parsen (als float!), [Organ] Gain parsen, eenhedenmix rank-branch fixen, en alles via het bestaande SetPipeVoicing-mechanisme naar de engine routeren (commands.rs:578 niet langer alleen `path`). Herstelt registerbalans (tot 9,5 dB) en 28 valse pijpen (tot 9 semitonen). *Effort: M*
4. **Release-samples afspelen** — GO Release999* + MaxKeyPressTime en HW Pipe_SoundEngine01_ReleaseSample parsen, bij note-off crossfaden naar de release-buffer (ReleaseCrossfadeLength). Grootste klankwinst van alles; raakt loader + audio-thread. *Effort: L*

**Middel:**
5. **Preload-fase echte looppunten gebruiken** — get_loop_points wordt al opgehaald (audio.rs:245-250) maar de PreloadOnly-tak negeert ze (audio.rs:290-296). Weg met de foute loop in de eerste ~2 s. *Effort: S*
6. **Zwelkast:** GO-detectie via de al geparste windchest→enclosure-refs i.p.v. naam-match (commands.rs:541-547; fixt Bureå), en HW Enclosure/EnclosurePipe parsen (fixt SJDL Récit). Neem meteen AmpMinimumLevel mee. *Effort: M*
7. **Tremulant:** ODF Period/AmpModDepth → LFO-parameters (Bureå 5,7 Hz/20%), en HW tremmed-laag → de bestaande RegisterTremulantBuffers-route. *Effort: M*
8. **Stereo behouden** — mono-downmix in de loader schrappen en het pad t/m de galm stereo maken; grootste ruimtelijke winst, maar raakt alles. *Effort: L*
9. **Velocity negeren** (of optioneel maken) — orgelpijpen zijn niet aanslaggevoelig (audio.rs:145). *Effort: S*
10. **Percussive respecteren** — one-shot i.p.v. ¾-fallback-loop (grandorgue.rs:404 + audio.rs:283-286). *Effort: S*
11. **Headroom:** tanh-softclip vervangen door limiter met polyfonie-bewuste gain; mono-galm → stereo (samen met punt 8). *Effort: M*

**Laag:**
12. GO ODF-looppunten (Loop999Start/End) parsen als smpl-chunk ontbreekt. *Effort: S*
13. `is_hauptwerk_path` op extensie i.p.v. pad-substring (hauptwerk.rs:338-340). *Effort: S*
14. Encrypted/onleesbare HW-sets detecteren en een duidelijke foutmelding geven i.p.v. stille registers. *Effort: S*
15. Multi-attack round-robin (GO Attack999, HW meerdere AttackSamples). *Effort: M*
16. Mic-perspectief-keuze bij HW-import (nu wint 'front' toevallig). *Effort: M*
17. loop_tool: zoekgrens verlagen tot ~16 Hz en max looplengte verruimen voor pedaal/natte samples (loop_tool.rs:22, 133-134). *Effort: S*
18. RankFirstAccessibleKeyNumber lezen (nu kloppen GreenPositiv's segmenten toevallig); preload→full upgrade: onset-trim compenseren (audio.rs:175-192). *Effort: S*
19. Switch/Function-drawstop-logica voor grotere GO-sets. *Effort: L*

**Eerlijke slotsom:** de fundamenten zijn goed — robuuste parsers, correcte structuur-import, een loop-engine die klikvrij is met dubbele bewijsvoering in tests, en een noise-filter dat je designkeuze netjes afdwingt. Maar wat er nu speelt is de *sampleset zonder de intonatie en zonder de akoestiek van de maker*: alles even hard, sommige pijpen vals of stil, en elke noot eindigt droog. De vier hoog-items (waarvan twee S-effort) dichten het grootste deel van dat gat.