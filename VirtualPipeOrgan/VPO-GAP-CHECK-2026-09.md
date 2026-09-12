# Wat is er de laatste maanden verbeterd in Hauptwerk en GrandOrgue — en wat mist JM-Orgue nog?

Peildatum 2026-09-12, venster maart–september 2026. Bronnen: GitHub-releases/issues/PR's van GrandOrgue, Hauptwerk-forum en -documentatie, Sweelinq/Organteq-releasenotes, nieuwe VPO-projecten (Rusty Pipes, Aristide, MagnusOrgue, Organova), samplesetmakers (Grabowski, Sonus Paradisi, Pipeloops, OrganArt Media, Voxus). Elke claim is door een tweede agent tegen de bron én tegen de JM-Orgue-code gecontroleerd; correcties zijn verwerkt.

## 1. Kort antwoord

* **Hauptwerk:** geen nieuwe versie sinds 9.0.1 (dec 2024). Martin Dyde bevestigde in 2026 op het forum een "volgende grote versie" (nieuwe Qt-versie, fix voor de windmodel-bug die sinds v5 de toonhoogte de verkeerde kant op stuurt, VST3-versie van VST Link, native Apple-Silicon-build) — alles zonder datum, "a while off". Prijzen en iLok ongewijzigd. Er is dus niets nieuws in Hauptwerk dat wij missen; de pijnpunten van HW-gebruikers (VST2-only, Rosetta, iLok, Windows-updates die MIDI breken) zijn juist kansen.
* **GrandOrgue:** drie bugfix-releases (3.17.1 op 1 april, 3.17.2 op 8 juni, 3.17.3 op 11 augustus). Enige nieuwe functie: kanaaltoewijzing bij het afspelen van MIDI-bestanden zonder GO-header (pedaal eerst/laatst/MIDIInputNumber). Op master (bestemd voor 3.18, nog niet uitgebracht) werkt hoofdontwikkelaar oleg68 aan een grote engine-refactor (planaire buffers, gemergd 1 sept; verwerkingsketen en taak per windlade in open PR's) als opmaat voor pitch-tremulant, zwelkast-shelf-EQ en galm per windlade — **dingen die JM-Orgue al heeft**. Verder: crash-rapportage met stacktrace (gemergd 20 aug), native Windows-build, vier Oost-Europese vertalingen, en een externe PR (12 sept) voor een web-afstandsbediening (telefoon als setzer/pistons).
* **De echte gaten** zitten niet in de recente HW/GO-releases maar bij de samplesetkant en de trend "headless + tablet": meerdere microfoonperspectieven, hertemperen op gemeten pijptoonhoogte, LAN-afstandsbediening, en een reeks kleine gebruikswinsten (automatisch MIDI-archief, WAV-opname, kanaalmapping voor vreemde MIDI-bestanden, divisionale pistons, crash-rapport).

## 2. Wat JM-Orgue mist of maar half heeft (geprioriteerd)

Prioriteit 1 = eerst doen. Inspanning: klein = uren, middel = dagen, groot = weken.

| # | Functie | Bij wie | JM-Orgue nu | Waarde | Inspanning |
|---|---|---|---|---|---|
| 1 | **Meerdere microfoonperspectieven laden en mengen** (close/front/rear, per perspectief volume/uitgang) | HW (perspective ranks); samplesetmakers: Grabowski Munich/Kampen 6-kanaals, Sonus Paradisi surround en droge multikanaals sets (jun/jul 2026) | Gedeeltelijk: eigen sets met multi-mic-submappen gebruiken alleen de eerste submap (`custom_organ.rs` ~1211); HW-import "eerste rank die een noot levert"; **ook GO-ODF's met NumberOfRanks ≥ 2 verliezen alle ranks behalve de eerste per toets** (`grandorgue.rs` ~610, teller `stacked_dropped`) — dat raakt ook echte gestapelde koren (mixturen/cornetten als aparte ranks) | Hoog — directe synergie met JM-Rec (meerdere microfoons) en met moderne sets | Groot; stap 1 (klein): perspectiefkeuze per orgel (welke submap/rank); stap 2: gestapelde ranks per toets als extra voices met eigen gain |
| 1 | **Hertemperen op gemeten pijptoonhoogte** (tuned/detuned) | HW (bestaand); GO: RFC #2479 (apr 2026) — oleg68 stelt dat GO bij een niet-origineel temperament al hertempert op pijptoonhoogte | Gedeeltelijk: 52 temperamenten als 12 cent-offsets t.o.v. de nominale toon; GO-`PitchCorrection` wordt geparsed maar bewust niet toegepast (`commands.rs` ~1035); HW-tuning-velden alleen basispitch | Hoog voor wie met koor/blazers/piano samenspeelt met "detuned" geleverde sets | Middel: PitchCorrection per pijp/rank/stop doorgeven; hertempering = (doeltemperament − gemeten afwijking) in `c_pitch_mul`; schakelaar "Originele stemming / hertemperen" per orgel |
| 1 | **Web-/LAN-afstandsbediening** (telefoon/tablet als registers, setzer, pistons) | GO: open PR #2622 (12 sept 2026); Rusty Pipes (web-UI + REST, apr 2026); Aristide (headless daemon + console-client over HTTP); RPi-projecten | Ontbreekt: test-API bindt alleen 127.0.0.1, start alleen met `--test-api`, serveert geen HTML, geen token; alle endpoints (status, organ, stops toggle, notes, presets) bestaan al | Hoog — tablet op de speeltafel; sluit aan bij de trend "headless + tablet" | Middel: opt-in instelling "Afstandsbediening in het netwerk" (runtime aan/uit, LAN-bind, eenvoudig token/QR), één ingebedde HTML-pagina die de bestaande endpoints gebruikt |
| 1 | **Automatisch MIDI-archief op de achtergrond** (start bij eerste noot, stopt na stilte, `YYYY-MM-DD_HH-MM-SS.mid`) | Organteq (bestaand); GO-wens #2445 met prototype | Ontbreekt: alleen handmatige MIDI-opname (`capture_midi_event`) | Hoog voor thuisorganisten ("had ik dat maar opgenomen") | Klein–middel: tweede, onafhankelijke recorder in `state.rs`; SMF-writer bestaat al |
| 2 | **Lossless audio-opname (WAV)** naast MP3 | GO/HW (bestaand, niet recent) | Ontbreekt: `recorder.rs` alleen LAME-MP3 320 kbps | Midden — montage/CD/YouTube | Klein: `hound` zit al in de workspace |
| 2 | **Kanaaltoewijzing bij vreemde MIDI-bestanden** | GO 3.17.3 (PR #2563) | Ontbreekt: speler gaat door dezelfde ingeleerde kanaalmapping als live MIDI; bestand van internet landt op verkeerde klavieren of blijft stil, zonder uitleg | Midden | Klein–middel: kanalen in het bestand tellen, dialoog "pedaal eerst / pedaal laatst / ODF-nummer", onthouden per bestand |
| 2 | **Divisionale pistons** (combinaties per klavier) | GO 3.17.3 fixte divisionals zonder pedaal; HW standaard | Gedeeltelijk: enums/velden bestaan (`router.rs`, `organ.rs`) maar zijn ongebruikt; SetzerBar is uitsluitend generaal (8×1000) | Midden — kerkmusici met console-divisionals | Middel: presets per (divisie, niveau, nummer); let op dat presets nu dubbel leven (localStorage per orgel én `library.rs`) |
| 2 | **Crash-rapportage met stacktrace** | GO master (PR #2567, gemergd 20 aug) | Gedeeltelijk: logrotatie + "logbestand meesturen"; geen `std::panic::set_hook`, dus een panic op audio-/MIDI-/laadthread staat niet in het log | Midden — bugrapporten van testers bruikbaar maken | Klein: panic-hook in `main.rs` (thread, boodschap, backtrace → `CrashReports/`) |
| 2 | **Panic-knop zichtbaar op extra schermen** | GO 3.17.3 (panic ook vanuit losse panelen); HW | De facto aanwezig: het stop-icoon in de hoofdbalk doet AllNotesOff + held_notes wissen zonder de stream te stoppen — maar op een extra scherm is de hoofdbalk standaard verborgen, dus daar is de noodknop onzichtbaar; niet MIDI-inleerbaar als losse actie | Midden — tijdens een dienst | Klein: kleine "Panic"-knop in de werkbalk van elk scherm + inleerbare consoleknop |
| 2 | **Klavieruitbreiding buiten het gesamplede bereik** (61-toets klavier op 54-toets orgel) | Sweelinq 3.0 "extended keyboard compass" | Ontbreekt: ontbrekende pijpen zijn stil; per-divisie toetsenbereik bestaat al als MIDI-mapping (aanknopingspunt) | Midden — moderne MIDI-klavieren | Klein–middel: optie per orgel; buurpijp octaaf-/toonverschoven lenen |
| 2 | **Voicing/galm/EQ per uitgangsprofiel** (hoofdtelefoon ≠ luidsprekers) | HW 8+ (4 rank routing/voicing presets per orgel; forumthread 10 sept 2026) | Gedeeltelijk: profielen bevatten host/apparaat/buffer/kanalen (0.7.37), maar voicing/EQ/galm zijn orgelbreed | Midden — precies het HW-forumprobleem; JM-Orgue heeft de profielknop al | Middel: optionele per-profiel-overrides in `.jm-settings.json` |
| 3 | Instelbaar RAM-plafond + geheugenindicator | GO (memory limit; wens #2508) | Vast 1,5 GB (`SAMPLES_BYTES_CAP`) met eviction | Midden–laag | Klein |
| 3 | Uitgangsroutering per register (Subbas → subwoofer) | HW rank routing; GO audiogroep per pijp (#2620 draft) | Alleen per divisie | Midden voor kerkinstallaties | Middel |
| 3 | Meerdere loops per sample, willekeurig gekozen | GO (bestaand); OrganArt Media multi-loop-sets (HW-only) | Alle loops worden gelezen, alleen de langste gebruikt (`loader.rs` ~315/793) | Laag–midden | Klein–middel |
| 3 | Laden annuleren | GO (bestaand tijdens sample-laden; wens #2507 voor ODF-fase) | Voortgang wel, geen annuleerknop; generatieteller `max_generation` is herbruikbaar | Laag–midden | Klein |
| 3 | Setzer-nummer in één keer intypen | GO 3.17.0 (febr 2026, net buiten venster); HW | Cijferknoppen 0–9 + bank via ±10 bestaan (MIDI-inleerbaar); alleen "typ 347 + Enter" ontbreekt | Laag–midden | Klein |
| 3 | Sampleset installeren uit lokale zip/andere URL | GO `.orgpkg`; GrandOrgue Selector (mrt 2026) | Downloads alleen uit `samplesets.json` (GitHub); uitpakcode herbruikbaar | Laag–midden — JM-Rec-gebruikers delen sets | Klein |
| 3 | Galm-mix per divisie/windlade | GO per-windlade-galm aangekondigd na #2608/#2620 (nog geen PR); HW per perspectief | Eén galm; kanaalgewichten volgen wel de divisieroutering (0.7.33) | Midden–laag | Middel (wet-send per divisie is klein) |
| 4 | SysEx-inleren voor consoleknoppen (Johannus/Content) | GO 3.17.1 (Johannus-SysEx in "Listen") | MIDI-uit kent Johannus/JOHAS-SysEx; inleren kent alleen noot/CC/PC (matcher in `state.rs` ~1870) | Laag | Klein–middel |
| 4 | Linux/Raspberry Pi-builds | GO (deb/rpm/AppImage x86_64; aarch64/armhf als deb/rpm/tar.gz; Flathub); Rusty Pipes headless | CI alleen windows/macos | Laag–midden | Middel |
| 4 | VST3/AU met echte sample-engine | HW VST Link (VST2; VST3 gepland zonder datum); GO: geen plannen (GPL2) | `vpo-plugin` MVP met sinus-stemmen | Laag–midden | Groot |
| 4 | Meerdere audio-apparaten tegelijk | GO/HW | Bewust één stream (zie AUDIO-UITVOER-VERGELIJKING.md); GO's 3.17.3-hang bij twee apparaten (#2606) is op master gefixt | Laag–midden | Groot |
| 4 | Toegankelijkheid (focusring, screenreader) | GO discussie #2478 | Registerknoppen zijn native `<button>` met aria-label/aria-pressed (Tab/Space werken); geen zichtbare focusring op registers | Laag | Klein |
| 5 | Console-bitmaps/ODF-panelen | GO/HW | Bewust eigen touch-knoppen; plan ligt klaar (workflow 2026-09-08) | Laag | Groot |
| 5 | Android/iPad-versie | MagnusOrgue (Android alpha, aug 2026); Organova (iPad eind 2026) | — | Midden op termijn | Groot |
| 5 | Extra UI-talen (Pools zou passen bij Grabowski-gebruikers) | GO master: BY/LT/RU/UK (bestemd voor 3.18) | NL/EN/FR/DE volledig (±840 sleutels) | Laag | Klein per taal |
| 5 | Celeste-ontstemming in Hz | GO-wens #2584 | ±50 cent per pijp, geen Hz-modus | Laag | Klein |

## 3. Recente HW/GO-verbeteringen die JM-Orgue al heeft

* Planaire kanaal-major audiobuffers (GO PR #2575, 1 sept) — JM-Orgue 0.7.36 echte stereo planair L/R.
* Zwelkast met frequentieafhankelijk filter (GO #717, bestemd voor 3.18) — JM-Orgue sinds 0.2.0/0.6.4.
* Pitch-gebaseerde tremulant (GO #709, open sinds 2021) — JM-Orgue LFO met pitch-diepte + samplelaag.
* Galm die divisies/kanalen volgt (GO per-windlade-galm in voorbereiding) — JM-Orgue 0.7.33.
* Audiogroepen/kanalen live herindelen zonder herstart (GO #2620 draft) — JM-Orgue 0.7.37.
* Windmodel met juiste toonhoogte-polariteit (HW-bug v5–v9, fix pas in volgende grote versie) — JM-Orgue verlaagt de toonhoogte bij inzakkende wind.
* Orgelvolume per orgel bewaard, registers bedienen tijdens MIDI-afspelen, laadvoortgang, release-uitlijning/loop-wrap-fixes (GO 3.17.x) — allemaal aanwezig.
* Realtime voicing-GUI per pijp (GO #2389, open) — VoicingPanel sinds 0.3.0.
* Voice-stealing onder belasting (GO-wens #2509) — 0.7.36 staarten eerst, dan oudste, met belastingsmeter.
* Touchbediening zonder schermtoetsenbord-gedoe (GO 3.17.3) — grote touch-targets, 3 s vasthouden = inleren.
* MIDI-terugkoppeling naar lampen/Stream Deck/OLED (GO discussie #2530, trend 2026) — drie protocollen in `feedback.rs`.
* JSON-status/control-API (GO-wens #2498/#1563) — test-API + MCP-server.
* 96 kHz-uitvoer (Sweelinq 3.0 Premium) — ondersteund.
* Meer dan 20 setzer-geheugens (GO #2593) — 8 niveaus × 1000.
* Hauptwerk-sets laden zonder Hauptwerk, disk-streaming (Rusty Pipes) — onversleutelde HW-import sinds 0.5.0, streaming met preload sinds 0.6.4.
* Native Apple Silicon (HW: pas vóór macOS 28) — universal dmg sinds 0.7.30 (onondertekend; nog niet op een Mac getest).

## 4. Waar JM-Orgue voorloopt

Live notatie (MIDI → MusicXML met lagen, bewerken, stapinvoer, export); online bibliotheek met downloads in de app; speakers↔hoofdtelefoon-profielwissel met één knop of MIDI-knop, nu ook zonder herbouw op hetzelfde apparaat; audio-zelfherstel (watchdog, ASIO-herstart, app-herstart met behoud van registratie); Bluetooth-LE-MIDI in de app; MIDI-uit-terugkoppeling met drie protocollen; MP3-opname vanaf elk scherm; FDN-galm met presets naast convolutie; windmodel per windgroep en C/Cis-ladespreiding; extra vensters als volwaardige consoles; JM-Rec-koppeling plus ingebouwde sampleset-tools; test-API + MCP; gratis zonder iLok of abonnement; update-check en feedback met logbestand in de app; consoleknoppen incl. "computer afsluiten"; instelbare polyfonie tot 4096 met belastingsmeter; thema's, ronde trekregisters en koppelbalk.

## 5. Aanbevolen volgorde

1. **Gestapelde ranks / perspectieven** (gap 1a: eerst de import-drop van extra ranks bij GO-ODF's oplossen — dat is een klankfout bij mixturen/cornetten als aparte ranks; daarna perspectiefkeuze en menging). Grootste klankwinst, sluit aan bij JM-Rec.
2. **Hertemperen op pijptoonhoogte** (PitchCorrection toepassen + schakelaar).
3. **LAN-afstandsbediening** (opt-in, token, ingebedde pagina op de bestaande endpoints).
4. **Automatisch MIDI-archief**, **WAV-opname**, **kanaalmapping voor vreemde MIDI-bestanden**, **panic-knop op elk scherm**, **crash-rapport** — elk in uren tot een dag.
5. Daarna divisionale pistons, klavieruitbreiding en per-profiel-voicing.

Huishoudelijk: `CHANGELOG.md` stopt bij 0.7.2 (7 aug 2026); alles van 0.7.3 t/m 0.7.37 staat alleen in de git-log en de release-notes op GitHub — bijwerken zodat toekomstige analyses (en gebruikers) het terugvinden.

## 6. Bronverwijzingen (selectie)

* GrandOrgue releases: https://github.com/GrandOrgue/grandorgue/releases (3.17.1-1, 3.17.2-1, 3.17.3-1); PR #2563 (MIDI-kanaalmapping), #2567 (crash-rapportage), #2575 (planaire buffers), #2608/#2620 (verwerkingsketen/windlade-taak), #2622 (web-UI), issues #2445, #2479, #2507, #2508, #2509, #2584, #2593, #2606.
* Hauptwerk-forum: t=21868 (windmodel-bug, 2–3 mrt 2026), t=21936 (VST Link VST2/VST3, Rosetta, aug 2026), t=21963 (voicing per speakers/hoofdtelefoon, 10 sept 2026), t=21683 (geen native ARM-port).
* Sweelinq 3.0/3.1 releasenotes; Organteq-forum (apr 2026); Rusty Pipes releases; Aristide (github.com/macaquedev/aristide); MagnusOrgue; Console Craft 5.1 (11 sept 2026, GrandOrgue-export).
* Samplesets: Piotr Grabowski Munich St. Margaret (jun 2026), Sonus Paradisi droge multikanaals sets (jun/jul 2026), OrganArt Media Ducroquet-Cavaillé-Coll New Edition, Voxus Uithuizen (12 sept 2026).
