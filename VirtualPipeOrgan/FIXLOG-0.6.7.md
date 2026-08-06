# Fixlog 0.6.7 — verwerking auditbevindingen (start 2026-07-23)

Nummering = AUDIT-2026-07-18.md. Status: TODO / BEZIG / KLAAR / DEELS / BEWUST-NIET (met reden).

- [KLAAR (0.6.6)] **1** [hoog] `src-tauri/src/commands.rs:3584` — Early return bij ontbrekende settings-entry: master volume en reverb van het vorige orgel lekken in het nieuwe orgel en worden opgeslagen
- [KLAAR (0.6.6)] **2** [hoog] `src-tauri/src/commands.rs:3591` — MIDI-mappings, preset-, swell- en crescendo-bindings lekken naar het volgende orgel en worden daar gepersisteerd
- [DEELS] **3** [hoog] `src-tauri/src/audio.rs:1542` — tracing-logging (mutex + stderr-write) in de realtime audio-callback
  - DEELS — warn nu max 1× per pijp; load-time info-regels in de callback blijven (begrensd)
- [KLAAR] **4** [hoog] `src-tauri/src/state.rs:1205` — MIDI-thread houdt loaded_organ_info-write vast over blokkerende 3s-sends naar de audio-queue
- [KLAAR] **5** [hoog] `src-tauri/src/silence.rs:314` — WAV-herschrijven bij trim gooit cue-release-markers (en extra metadata) stil weg
- [KLAAR] **6** [hoog] `ui/src/components/Console.svelte:911` — Windgroep-keuze wordt bij elke organInfo-wijziging teruggedraaid door apply_saved_wind_groups en vervolgens als teruggedraaide waarde opgeslagen
- [KLAAR] **7** [hoog] `ui/src/components/Console.svelte:2465` — Na audio-uitgang/profielwissel worden per-orgel DSP-instellingen nooit opnieuw toegepast (guards keyen op orgel-id/naam)
- [KLAAR] **8** [hoog] `ui/src/components/SetzerBar.svelte:58` — switchMemoryLevel leest stale reactive storageKey en loadPresets reset niet: presets lekken tussen geheugenniveaus en orgels
- [KLAAR] **9** [hoog] `ui/src/components/SetzerBar.svelte:105` — Geen trigger-poller buiten de orgel-view: MIDI-thread blokkeert op vol kanaal + replay van oude piston-acties
- [KLAAR] **10** [hoog] `ui/src/components/RegisterPanel.svelte:454` — Panel-SetzerBar steelt MIDI-preset-triggers: acties vallen stil of overschrijven presets
- [KLAAR] **11** [hoog] `vpo-midi/src/player.rs:279` — Afspeelklok wordt nooit opnieuw verankerd: seek, pauze en snelheidswissel springen en veroorzaken event-vloed
- [KLAAR] **12** [hoog] `src-tauri/src/audio.rs:1916` — IR-laden (disk-I/O + volledige FFT-partitionering) draait ín de realtime audio-callback
- [KLAAR] **13** [hoog] `vpo-audio/src/convolution.rs:61` — Lege impulsrespons: num_partitions = 0 → panic (index out of bounds / modulo 0) in de audio-callback
- [KLAAR] **14** [hoog] `src-tauri/src/audio.rs:2405` — NaN/Inf-vangnet zit ná de stateful effecten: filterstaat blijft permanent vergiftigd
- [KLAAR] **15** [hoog] `vpo-sampler/src/loader.rs:669` — Panic (deling door nul / index-OOB) in load_wav_preload_segment op ADPCM- of misvormde WAV
- [KLAAR] **16** [hoog] `vpo-sampler/src/custom_organ.rs:331` — export_organ_file gooit per-pijp MIDI-noot weg: gedeelde/discant-registers en gaten verschuiven alle toetsen
- [KLAAR] **17** [hoog] `src-tauri/src/commands.rs:3980` — .jm-settings.json naast het orgel wordt bij laden grotendeels genegeerd en bij de eerste autosave overschreven (dataverlies bij verse AppData)
- [KLAAR] **18** [hoog] `src-tauri/src/commands.rs:3693` — Backend-geinitieerde herlaad (noodherstel / uitgestelde ASIO-wissel) past DSP-instellingen nooit toe op de nieuwe audio-thread
- [KLAAR] **19** [hoog] `src-tauri/src/state.rs:2275` — expand_coupler_routes cascadeert zelf-koppels: octaafkoppel op eigen klavier klinkt tot 8 octaven
- [KLAAR (0.6.6)] **20** [hoog] `src-tauri/src/state.rs:1174` — Crescendo-pedaal trekt handmatig getrokken registers stilletjes weg
- [BEWUST-NIET] **21** [hoog] `src-tauri/src/audio.rs:1534` — Tremulant- en droge samples delen één key in de full-sample-map: verkeerde laag blijft klinken
  - BEWUST-NIET (nu) — vergt key-space-redesign trem/droog in de audio-thread; te riskant zonder gehoortest
- [KLAAR] **22** [hoog] `vpo-sampler/src/grandorgue.rs:690` — PipeNNNReleaseCount onbegrensd: Vec::with_capacity op ODF-waarde crasht de app (alloc-abort)
- [KLAAR (0.6.6)] **23** [middel] `src-tauri/src/state.rs:2620` — num_wind_groups en ccis_spread worden bij orgelwissel niet gereset: lekken naar het volgende orgel en worden daar opgeslagen
- [KLAAR] **24** [middel] `src-tauri/src/state.rs:1059` — LearnKeyboardRange gooit NoteOffs weg → klinkende noten blijven eeuwig hangen
- [KLAAR] **25** [middel] `src-tauri/src/silence.rs:330` — Sample-bestanden worden in-place herschreven zonder temp+rename en zonder rollback
- [KLAAR] **26** [middel] `src-tauri/src/main.rs:149` — Race-guard van uitgestelde ASIO-herlaad leest current_organ_id buiten de load_switch_gate (TOCTOU)
- [BEWUST-NIET] **27** [middel] `src-tauri/src/commands.rs:2082` — Kanaalwijziging onder actief uitvoerprofiel wordt nergens persistent en wordt stil teruggedraaid
  - BEWUST-ZO — profiel-kanalen zijn per ontwerp een tijdelijke laag (0.6.5, gedocumenteerd): profiel opnieuw opslaan maakt ze blijvend
- [KLAAR] **28** [middel] `ui/src/components/Console.svelte:1057` — Tremulant-default-reactive draait bij orgelwissel met de stale map van het vórige orgel en persisteert die in het nieuwe orgel
- [KLAAR] **29** [middel] `ui/src/App.svelte:126` — Directe organInfo-updates werken de poll-dedup-cache niet bij: permanente registerknop-desync en gemiste autosave
- [KLAAR] **30** [middel] `ui/src/App.svelte:828` — Presets-sleutel-mismatch (zonder vs met -m-suffix): library-persistentie van setzer-presets is een dode lus
- [KLAAR] **31** [middel] `vpo-midi/src/device.rs:74` — Poortselectie op naam: bij twee identiek genaamde MIDI-interfaces wordt twee keer dezelfde poort verbonden en blijft de tweede dood
- [BEWUST-NIET] **32** [middel] `vpo-midi/src/ble.rs:391` — Wegvallende BLE-verbinding wordt nooit gedetecteerd: connected-state blijft 'verbonden', geen reconnect
  - BEWUST-NIET (nu) — BLE-disconnect-detectie vergt btleplug-event-architectuur
- [KLAAR] **33** [middel] `vpo-midi/src/player.rs:242` — Race in load_and_play: oude playerthread kan de stop-vlag missen en blijft naast de nieuwe thread spelen
- [KLAAR] **34** [middel] `vpo-audio/src/convolution.rs:114` — IR-samplerate wordt volledig genegeerd: galm speelt op verkeerde snelheid af
- [KLAAR] **35** [middel] `src-tauri/src/commands.rs:944` — ODF-looppunt-override bereikt alleen de preload-buffer, nooit de full sample: hoorbare loop-desync op het upgrademoment
- [KLAAR] **36** [middel] `vpo-sampler/src/loader.rs:350` — IO-fout halverwege decode wordt stil naar nullen gemapt: noot valt zonder enige melding stil na de preload->full-upgrade
- [DEELS] **37** [middel] `vpo-sampler/src/custom_organ.rs:894` — check_new_structure classificeert oude-structuur sets met alleen multi-mic/bas-dis-submappen als nieuwe structuur: stops worden divisions
  - DEELS — bas/dis/trem-varianten herkend als oude structuur; pure multi-mic-sets blijven ambigu
- [KLAAR] **38** [middel] `vpo-sampler/src/custom_organ.rs:310` — Export schrijft HarmonicNumber als 32/voet terwijl de importer 64/harmonic verwacht: alle voetmaten verdubbeld na round-trip
- [KLAAR] **39** [middel] `vpo-sampler/src/grandorgue.rs:571` — Misvormde REF-nummers vallen terug op 0 en resolven stil naar de verkeerde pijp (eerste pedaalstop)
- [BEWUST-NIET] **40** [middel] `src-tauri/src/commands.rs:3841` — Met een actief uitvoerprofiel worden handmatige routing-wijzigingen nergens persistent: weg na herstart
  - BEWUST-ZO — zelfde ontwerpkeuze als hierboven
- [KLAAR] **41** [middel] `src-tauri/src/state.rs:1162` — crescendo_stage/crescendo_active_stops raken desync met UI-pad en herinleren; gelijke-trap-early-out maskeert dit
- [KLAAR] **42** [middel] `src-tauri/src/audio.rs:2317` — Divisie-routing naar niet-bestaande kanalen: droog signaal geluidloos weg, galm klinkt wél door
- [KLAAR] **43** [middel] `src-tauri/src/commands.rs:3466` — Stale tremulant-buffers en stops_with_trem_samples overleven een orgelwissel (custom-map-pad)
- [KLAAR] **44** [middel] `src-tauri/src/library.rs:333` — Settings-keys zijn hoofdlettergevoelig terwijl bibliotheek-dedup hoofdletter-ongevoelig is: instellingen splitsen of raken zoek per pad-notatie
- [KLAAR] **45** [middel] `vpo-sampler/src/grandorgue.rs:624` — RankNNNFirstAccessibleKeyNumber onbegrensd: gigantische dense pijplijst (OOM-abort) en u32-overflow in first_key + i
- [KLAAR] **46** [middel] `ui/src/components/Console.svelte:1351` — loadAudioSettingsForOrgan heeft geen stale-guard: na een snelle orgelwissel pusht de oude run instellingen van orgel A in de mirrors/audio van orgel B
- [KLAAR] **47** [middel] `src-tauri/src/commands.rs:3847` — Lock-volgorde-inversie division-locks ↔ loaded_organ_info: 3-thread-deadlock via wachtende writer
- [KLAAR] **48** [middel] `src-tauri/src/commands.rs:1160` — Race: loaded_organ_info gepubliceerd vóór current_organ_id — frontend kan settings van het vórige orgel op het nieuwe toepassen
- [KLAAR] **49** [middel] `src-tauri/src/state.rs:2410` — recv() zonder timeout naar de MIDI-thread die tot een minuut in een leer-handler zit
- [KLAAR] **50** [laag] `src-tauri/src/main.rs:59` — Tweede app-instantie trunceert het logbestand van de draaiende instantie
- [BEWUST-NIET] **51** [laag] `src-tauri/src/recorder.rs:84` — Na dood van de encoder-thread blijft de recorder 'recording' melden en telt hij drops niet meer
  - BEWUST-NIET (nu) — laag/niche (encoder-thread-dood)
- [BEWUST-NIET] **52** [laag] `src-tauri/src/loop_tool.rs:359` — crate_has_smpl zoekt 'smpl' in ruwe bytes inclusief PCM-audiodata: false positives
  - BEWUST-NIET (nu) — laag/niche (false positives smpl-scan)
- [KLAAR] **53** [laag] `ui/src/components/Console.svelte:3153` — Wijzigen van het aantal crescendo-stappen wist de complete crescendo-matrix en persisteert dat direct, zonder bevestiging of herstelmogelijkheid
- [KLAAR] **54** [laag] `vpo-midi/src/ble.rs:454` — BLE-parser slikt SysEx/system-common alleen als statusbyte in: payloadbytes worden daarna als running-status-events geemit
- [KLAAR] **55** [laag] `vpo-audio/src/convolution.rs:126` — 32-bit int-WAV als IR: `1 << 31` wordt i32::MIN → polariteit van de hele galm omgedraaid
- [KLAAR] **56** [laag] `src-tauri/src/commands.rs:3758` — do_save_organ_settings draait buiten de load_switch_gate: race met een lopende load kan de instellingen van het oude orgel corrumperen
- [KLAAR] **57** [laag] `src-tauri/src/audio.rs:454` — Voice-stealing op het 512-plafond kapt een klinkende sustain-voice zonder fade af (klik)
- [KLAAR] **58** [laag] `src-tauri/src/audio.rs:1677` — RegisterOdfVoicings kloont een duizenden-entries HashMap in de audio-callback
- [DEELS] **59** [laag] `vpo-audio/src/reverb_fdn.rs:157` — FdnReverb::configure en ChannelEq::build alloceren op de audio-thread
  - DEELS — FDN alloceert niet meer (capaciteit vooraf); EQ-chain-rebuild alloceert nog bij sliderwijziging
- [KLAAR] **60** [laag] `ui/src/components/RegisterPanel.svelte:127` — selectedDivisions niet gereset bij orgelwissel in panel-venster: leeg paneel
- [BEWUST-NIET] **61** [laag] `src-tauri/src/state.rs:987` — Opgenomen crescendo-CC doet bij MIDI-afspelen helemaal niets meer
  - BEWUST-ZO — gevolg van crescendo-exclusiviteit (0.6.5): afspeel-CC doet niets, dat is juist
- [KLAAR] **62** [laag] `src-tauri/src/state.rs:1924` — Registratie-snapshot lekt op de foutpaden van switch_audio_output_inner
- [KLAAR] **63** [laag] `src-tauri/src/state.rs:1971` — if-let-temporary houdt audio_player.write vast tijdens shutdown_and_wait(2 s)

## Naschrift 0.6.8 (2026-07-23)

- Bevinding **3** alsnog volledig: de per-noot info-logging in de MIDI-thread
  ("MIDI NoteOn" + "Playing stop" per register per toets) staat nu op trace
  (default uit) — dit bleek de hoofdoorzaak van haperen bij vol werk.
- Restpunt "GO release-decode-pieken" (0.6.2-memory) aangepakt: de 3
  sample-loaderthreads draaien onder THREAD_PRIORITY_BELOW_NORMAL.
- De CC7/11-master-expressie-fallback is volledig VERWIJDERD (treden deden
  ongevraagd volume zonder inleren; melding gebruiker 23-07).
