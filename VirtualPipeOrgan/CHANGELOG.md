# Changelog

Alle belangrijke wijzigingen van JM-Orgue worden hier bijgehouden.

Format gebaseerd op [Keep a Changelog](https://keepachangelog.com/),
versies volgen [Semantic Versioning](https://semver.org/).

## [0.6.1] - 2026-07-07

### Opgelost — de speakers↔hoofdtelefoon-wissel werkt nu echt

Wortel-oorzaak gevonden met probes op de console-PC: **ASIO4ALL kan binnen één
app-sessie maar één keer initialiseren** (elke her-init faalt blijvend met
"No audio output device found"), en het láden van de ASIO-driver claimt de
WASAPI-endpoints exclusief — waardoor de app met zijn eigen draaiende stream
(en de watchdog die hem elke 2 s herbouwde) om het apparaat vocht.

- 🎧 **ASIO-driver wordt één keer geladen en daarna procesbreed hergebruikt**: elke volgende wissel naar ASIO bouwt een nieuwe stream op de al geladen driver — heen en weer wisselen tussen ASIO-speakers en een WASAPI-hoofdtelefoon werkt nu onbeperkt vaak
- 🚪 **Wissels van/naar ASIO breken de oude uitgang éérst volledig af** (met echte wacht op de audio-thread) vóór de nieuwe wordt gebouwd, met een retry-ladder voor het asynchroon vrijgeven van apparaten; kiest de gebruiker expliciet WASAPI op het apparaat dat de idle ASIO-driver bezet houdt, dan wordt de driver vrijgegeven en opnieuw geprobeerd
- 📋 **Apparaatlijsten laden geen drivers meer**: de ASIO-lijst in Instellingen komt nu uit het register (namen-scan) — het openen van de instellingen doodt de lopende WASAPI-stream niet meer (dat was de mysterieuze watchdog-herbouw bij elke start)
- 🗣️ **Eerlijke wisselresultaten**: een wissel meldt nu gestructureerd wat er echt gebeurd is (gelukt / vorige uitgang hersteld / op standaardapparaat teruggevallen, mét Nederlandse uitleg). De stille "fallback-gemeld-als-succes" — geluid op het verkeerde apparaat terwijl de knop hoofdtelefoon toont — is weg
- 🎯 **Strikte apparaatkeuze**: een gevraagd apparaat dat er niet (meer) is geeft nu een duidelijke fout in plaats van stilletjes naar het standaardapparaat te routeren; bij falen wordt de vórige uitgang hersteld (niet zomaar de default)
- 🧊 **De wissel bevriest de app niet meer**: het wisselcommando draait asynchroon buiten de UI-thread
- 🩹 De echte foutoorzaak wordt gemeld i.p.v. een misleidende "Audio stream did not start within 8s" (apparaatresolutie-fouten bereikten de melding nooit)
- 🖱️ De "Activeer"-knop bij uitvoerprofielen is niet langer uitgeschakeld voor het actieve profiel — opnieuw activeren is een legitiem herstelmiddel; het afgeleide actieve profiel overschrijft de opgeslagen keuze niet meer

### Gewijzigd — nagalm-presets

- 🏛️ **Vaste ruimte-presets zonder schuifregelaars**: kies je Kleine Kapel t/m Concertzaal, dan geldt die klank zoals ontworpen — de schuiven zijn verborgen
- 🎚️ **Nieuwe preset "Eigen (per orgel)"**: mét schuiven (RT60, pre-delay, demping, ruimte), en die instelling wordt per orgel vastgelegd. Oude opslag met bijgestelde schuiven wordt automatisch als "Eigen" herkend zodat je klank behouden blijft
- 💾 **Eigen impuls-respons (convolutie-galm) blijft nu bewaard**: het gekozen IR-bestand wordt per orgel opgeslagen en na een herstart, orgelwissel of audio-wissel automatisch herladen (voorheen moest je hem elke keer opnieuw laden)

### Toegevoegd — alle MIDI-koppelingen handmatig te bewerken

- 🛠️ **Nieuw blok "MIDI-koppelingen (handmatig bewerken)"** in Orgel-Instellingen: álle koppelingen (setzer-knoppen, SET/GC/±1/±10/geheugens, tremulanten, EQ- en crescendo-schakelaar, afsluiten, speakers↔hoofdtelefoon, koppels én registers) in één tabel — type (noot/CC/program change), kanaal, nummer en CC-drempel direct aan te passen, per koppeling te verwijderen (✕), en onderaan handmatig toe te voegen met doel-keuzelijst. Inleren blijft daarnaast gewoon werken (max 4 signalen per doel)
- ⌨️ **Toetsenbereik handmatig**: per klavier zijn de laagste/hoogste MIDI-noot nu direct in te typen (naast de bestaande twee-toetsen-inleer)
- 🦶 **Zwelpedaal en crescendopedaal handmatig**: MIDI-kanaal en CC-nummer per divisie/crescendo direct in te vullen (naast "Leer pedaal"); de crescendo-pedaalkoppeling heeft nu ook een wisknop in de UI



### Toegevoegd

- 🎧 **Snelle wissel speakers ↔ hoofdtelefoon**: sla twee uitvoerprofielen op (host + apparaat + buffer, bijv. luidsprekers via ASIO en een hoofdtelefoon via WASAPI) en wissel met één knop in de kopbalk — of via een ingeleerde MIDI-knop

### Opgelost (wissel-robuustheid, n.a.v. "geen geluid op speakers")

- 🔇 **Mislukte wissel kan het geluid niet meer meenemen**: de profielknop wordt alléén omgezet als de wissel écht slaagt, en bij falen keert de app automatisch terug naar het vorige (werkende) profiel met een duidelijke foutmelding; een wissel naar een afwezig apparaat (hoofdtelefoon uit/los) wordt vooraf geweigerd i.p.v. stilletjes naar een ander apparaat te routeren
- 🔒 **ASIO-exclusiviteit bij het wisselen**: gaat de wissel van ASIO naar WASAPI mis doordat ASIO het apparaat nog exclusief vasthoudt, dan wordt de oude stream eerst netjes gesloten en opnieuw geprobeerd (daarna default host als laatste redmiddel)
- 🧭 **De knop toont altijd de échte uitgang**: het actieve profiel wordt afgeleid uit wat er werkelijk speelt (ook na een app-herstart of een handmatige wijziging in Instellingen) — staat het geluid op de hoofdtelefoon, dan zie je dat meteen in de kopbalk
- 🚑 **Audio-noodherstel**: krijgt de interne watchdog de stream vijf keer op rij niet herbouwd (bv. apparaat verdwenen en de vervanger heeft een ander formaat), dan wordt de complete audio-uitvoer automatisch opnieuw opgebouwd en het orgel herladen — geen blijvende stilte meer die alleen een herstart oploste; het noodherstel herhaalt zichzelf zolang nodig en pakt ook een volledig weggevallen audio-uitvoer op
- ✅ **Een wissel telt pas als geslaagd wanneer er écht audio stroomt**: een stream die wel "start" maar nooit audio levert (het beruchte stille ASIO4ALL-gedrag) wordt nu herkend en als fout gemeld, zodat de app op de werkende uitgang blijft in plaats van stil te vallen; ook een herbouwde stream wordt pas als hersteld beschouwd zodra er callbacks binnenkomen
- 🛟 **Voorkeuren overleven storingen**: valt de app tijdelijk terug op de standaard-uitgang (nood-fallback), dan blijft je opgeslagen voorkeur (bv. ASIO) onaangetast; een tijdelijke storing kan je vaste instelling niet meer wissen
- 🧵 Diverse vastloop-scenario's rond het wisselen gedicht (blokkerende afsluitcommando's op een volle wachtrij, zombie-audiothreads na een time-out, en een race tussen het noodherstel en een handmatige wissel)
- 🎹 **MIDI-kanaal handmatig instellen werkt nu écht**: de kanaal-keuzelijst (1-16) stuurde intern het verkeerde kanaalnummer (één te hoog), waardoor het klavier zweeg na een handmatige keuze en een geleerd kanaal als lege keuzelijst verscheen — weergave en techniek zijn nu correct gekoppeld
- 🚪 **"Laatst geopende orgel automatisch laden bij starten" is nu instelbaar** (Algemene Instellingen, standaard aan) — los van de bestaande optie "Laatste registratie herstellen bij openen"
- 🔗 **Koppels koppelen door (transitief)**: is Pedaal→Hoofdwerk actief én Hoofdwerk→Nevenwerk, dan klinkt het pedaal nu ook door op het Nevenwerk; octaafkoppels sommeren netjes over de keten (4' + 16' = unisono) en kringen worden veilig afgekapt. T.A. op de doeldivisie onderdrukt ook meegekoppelde tongwerken
- 🎼 **GrandOrgue-import volgens de officiële semantiek** (nieuw importspec-onderzoek):
  - **Same-file releases**: bij sets waar de kerkakoestiek ná de loop in hetzelfde WAV-bestand zit (cue-marker-model) speelt het loslaten nu écht de opgenomen ruimte af; aparte release-bestanden starten voortaan op hun cue-marker zoals GrandOrgue doet
  - **Inschalingshiërarchie compleet**: AmplitudeLevel/Gain/PitchTuning van álle niveaus (orgel × windkast × stop/rank × pijp) worden nu gesommeerd — voorheen ontbraken het windkast- en rank-niveau
  - **PitchCorrection niet meer onterecht toegepast**: GrandOrgue gebruikt die alleen bij hertemperen — een Voix Céleste zweeft nu zoals opgenomen, niet dubbel
  - **smpl-loops exact**: het loop-einde is inclusief (GrandOrgue-semantiek, was 1 frame te kort), alle loop-records worden gevalideerd en de langste geldige wint; ODF-loops winnen nu van de WAV-loops (was andersom)
  - **Release-fade toonhoogte-afhankelijk** zoals GrandOrgue: lage pijpen ~184 ms, hoge ~6 ms (was vast 42 ms) — bassen sterven natuurlijker uit
  - 8-bit PCM-samples worden nu ook door de preload-loader geaccepteerd

### Eerder toegevoegd (2026-07-04)

- 🎛️ **Vrije EQ per uitgangskanaal** (GrandOrgue-stijl): onbeperkt banden toevoegen, per band type (piek, laag-/hoogdoorlaat, banddoorlaat, lage/hoge shelf), frequentie, gain, bandbreedte (octaven) en doelkanaal ("Alle kanalen" of één specifiek fysiek kanaal). Oude 3-bands instellingen worden automatisch gemigreerd
- ⛪ **Stereo-galm die altijd werkt**: de algoritmische galm is nu echt stereo (gedecorreleerde L/R-staart), ~12 dB krachtiger gekalibreerd, en de engine valt automatisch terug op de algoritmische galm zolang er geen IR-bestand geladen is (voorheen: convolutie zonder IR = helemaal geen galm). Slider-wijzigingen (rt60/demping/mix) wissen de klinkende galmstaart niet meer
- 🩺 **Audio-zelfherstel**: valt het uitvoerapparaat weg of stopt de stream (bv. bluetooth-koptelefoon die verbindt, apparaatwissel), dan herbouwt de audio-watchdog de stream binnen ~2 s automatisch — voorheen bleef de app geluidloos tot een herstart
- ⏱️ **Actuele audio-status + geschatte latentie** zichtbaar in Algemene Instellingen (host, apparaat, samplerate, kanalen, buffer, ≈ms)

### Opgelost

- 📊 **Laad-voortgang bij het openen van een orgel**: de voortgangsbalk bleef altijd op 0% staan, waardoor een grote (koude) GrandOrgue-set eruitzag als een vastgelopen app — de balk loopt nu echt mee met het aantal geladen samples
- 💾 **Instellingen naast Hauptwerk-orgels**: het `.jm-settings.json`-bestand naast het orgel werd bij Hauptwerk-orgels (en `.Organ`-bestanden met hoofdletter) nooit geschreven of teruggelezen (het orgelbestand werd als map behandeld); ook de console-afbeelding werd daardoor niet gevonden
- 🎹 **Registreren tijdens het spelen werkt nu overal direct**: een register bijtrekken laat ingedrukte toetsen meteen meeklinken (ook via actieve koppels en melodie-/baskoppels) en wegtrekken laat pijpen direct zwijgen — nu ook bij setzer-/preset-oproepen en het generaal-crescendo, niet alleen bij losse registerklikken
- 🔇 **Tikken in loops verholpen** (o.a. Puttershoek): drie oorzaken gefixt — (1) één sample stilte per loop-omloop wanneer het looppunt op het buffereinde lag (alle preload-buffers en fallback-loops), (2) een hoorbare positiesprong op het moment dat de volledige sample de preload-buffer overneemt (aanloop-trim werd niet gecompenseerd), (3) de FFT-resampler-vertraging werd niet gecompenseerd waardoor 44,1 kHz-sets in de tijd verschoven t.o.v. hun looppunten
- ✂️ **Kniptool bewaart looppunten**: stilte wegknippen gooide de smpl-loop-chunk weg en liet looppunten verschuiven; loops worden nu meegeschoven en teruggeschreven
- ⚡ **Minder aanslagvertraging**: resampler-delay-compensatie (44,1 kHz-sets spreken enkele ms eerder aan) en een strakkere aanslag-envelope (~2 ms i.p.v. ~4 ms)
- 🚦 Eén `ReleaseStop`-audiocommando per weggetrokken register i.p.v. een NoteOff per pijp — een preset-wissel die veel registers wegtrekt kan de command-queue niet meer verstoppen

## [0.5.2] - 2026-07-03

### Opgelost

- 🖱️ **Sorteerlijst-verslepen werkt nu echt**: de registersorteerlijst in Instellingen gebruikte nog native browser-drag (verbodsteken in WebView2); nu hetzelfde betrouwbare pointer-mechanisme als de registerknoppen — met de muis overal op de rij, met touch via het ☰-handvat

## [0.5.1] - 2026-07-03

### Opgelost

- 📚 **Dubbele samplesets in de bibliotheek**: verschillende pad-notaties (forward/backslash) leverden dubbele entries op; paden worden nu genormaliseerd en bestaande duplicaten worden bij het starten automatisch samengevoegd (instellingen blijven behouden)

## [0.5.0] - 2026-07-03

### Toegevoegd

- 🖱️ **Registerknoppen verslepen**: herorden registers direct op het orgelscherm met de muis (klikken blijft trekken; ▲▼ en de sorteerlijst blijven bestaan)
- ↕️ **Verticale indeling vult nu echt kolommen**: van boven naar beneden, kolom vol → volgende kolom; horizontaal blijft rijen vullen
- 🦶 **Pedaalregisters** krijgen geen onterecht "(Bas)"-label meer (bereik-heuristiek geldt alleen voor manualen)
- 🎼 **Echte release-samples** (GrandOrgue + Hauptwerk): bij het loslaten van een toets klinkt nu de opgenomen ruimteakoestiek van de sampleset in plaats van een synthetische fade van 42 ms
- 🎚️ **ODF-inschaling**: Gain/AmplitudeLevel/PitchTuning/PitchCorrection uit de sampleset worden nu toegepast (registerbalans en stemming zoals de maker bedoelde) — als aparte laag naast je eigen intonatie
- 🪗 **Hauptwerk-sets openbaar via de bestandskiezer** (`.Organ_Hauptwerk_xml`), inclusief zwelkasten, tremulant-samplelaag en release-samples; versleutelde sets geven een duidelijke melding
- 🔔 **Percussive-registers** (klok/glockenspiel) spelen one-shot en klinken uit bij note-off, zoals GrandOrgue

### Opgelost

- 🐛 **REF-pijpverwijzingen** resolven nu manual-relatief — herstelt stille toetsen (o.a. de 12 laagste van Bureå's Salicional 8')
- 🎹 **Zwelkast-detectie** is structureel (windchest→enclosure) i.p.v. naam-match — zwelkasten verschijnen nu ook als de windchest-naam afwijkt
- 🌊 **ODF-tremulantparameters** (snelheid/diepte) worden als default overgenomen
- 🎵 MIDI-velocity beïnvloedt het volume niet meer (orgelpijpen klinken altijd vol)
- 🔁 De eerste seconden van een noot gebruiken nu de echte looppunten i.p.v. een noodloop over de attack

- 🔊 **ASIO**: uitgestelde ASIO-wissel bij een opgeslagen ASIO-voorkeur (start op WASAPI, automatische wissel ±10 s na de app-start) — voorkomt dat ASIO4ALL stil blijft na een koude start
- 🔊 **ASIO→ASIO-wissel** sluit nu eerst de oude stream voordat de nieuwe wordt geopend (ASIO-drivers zijn single-client)
- 📊 **Audio-callback-watchdog** in het log — meldt wanneer audio-callbacks stoppen (init én periodiek elke 30 s)
- 🎹 **MIDI-opname**: stopknop stopt de capture nu echt — noten tijdens de opslagdialoog belanden niet meer in het .mid-bestand
- 🎚️ **MIDI-volumepedaal (CC7/CC11)** stuurt nu een correct dB-bereik (was: lineaire waarde als dB geïnterpreteerd, waardoor het pedaal feitelijk niets deed)
- 🩹 **Frontend**: crescendo-stap aanklikken past nu echt de registers toe; SetzerBar-MIDI-acties (tremulant/EQ/afsluiten) werken weer; opgeslagen voicings zichtbaar na herladen; knopgrootte/layout van het registervenster springt niet meer terug; versienummer in de statusbalk is dynamisch

## [0.4.0] - 2026-06-24

### Toegevoegd

#### Opname & weergave
- 🔴 **MIDI-opname** (Standard MIDI File) — neem je spel op als `.mid` in `Documenten/JM-Orgue-opnames`; klein, herbruikbaar, transponeerbaar
- ▶️ **Opname afspelen** — één klik en je hoort jezelf terug op hetzelfde orgel
- 🎧 **MP3-opname** van de live audio-uitvoer (320 kbps stereo, lock-vrij vanuit de audio-thread)

#### Per-orgel instellingen-persistentie
- 💾 **Elk orgel onthoudt z'n eigen instellingen** in `.jm-settings.json`: mastervolume, temperament + fijnstemming, reverb (type/mix/ruimte-preset), 3-bands EQ, per-divisie DSP (zwelkast, tremulant-LFO, stereo-pan, wind-groepen) en UI-voorkeuren (koppel- en zwel-zichtbaarheid, layout, knopgrootte). Geen gelekte instellingen meer tussen orgels

#### Sampleset-tools
- ✂️ **Silence-trim** — detecteert en verwijdert aanloopstilte uit WAV/MP3-samples in een sampleset (originelen naar backup-map)
- 🔁 **Loop-tool** — vindt automatisch een sustain-loop per WAV, bakt een equal-power crossfade op de naad en schrijft de loop points naar de `smpl` chunk

#### Instellingen & UI
- 🌍 **Meertalige UI** (NL/EN/FR/DE) door de hele app, realtime wisselbaar via Sfeer & Layout
- 🗂️ **Instellingen gesplitst** — Algemene Instellingen (audio, MIDI, sfeer, taal) gescheiden van Orgel-Instellingen (reverb, EQ, voicing per orgel); drie tabs Orgel · Orgel-Instellingen · Algemene Instellingen (F1/F2/F3)
- 🖱️ **Universele MIDI-learn** — overal via rechtermuisklik (desktop) of 3 sec vasthouden (touchscreen)

#### Realisme & routing
- 🌬️ **Wind-groepen** — 1 tot 8 windvoorzieningen vrij toewijsbaar aan divisies (pedaal apart, twee manualen op één balg, etc.)
- 🎹 **C/Cis-spreiding** — gescheiden C- en Cis-laden realistisch nagebootst, per orgel instelbaar
- 🔈 **Audiokanaal-verdeling per werk** — stuur elk klavier naar eigen fysieke uitgangen (multi-kanaals opstelling)

#### Developer / testen
- 🧪 **Test-API** — start de app met `--test-api` (standaard poort 8765, alleen 127.0.0.1) voor een HTTP-API t.b.v. testen en automatisering
- 🎚️ **Optionele ASIO-buildfeature** — `cargo build --release -p vpo-app --features asio` compileert cpal's ASIO-backend voor lage latency op Windows

### Gewijzigd

- ⚡ **Snellere sampleset-loading** — bestaande (Hauptwerk/GrandOrgue-)sets laden in seconden; 16 GB RAM volstaat voor grote orgels

## [0.3.0] - 2026-05-05

### Toegevoegd

#### Sfeer & Layout
- 🎨 **Vrij instelbaar layout-systeem** — alle kleuren, lettertype, achtergrond per token aanpasbaar via Instellingen
- 🎨 **5 ingebouwde presets** — Basis (Witte orgelbouwer), Klassiek hout (Eiken), Modern minimalistisch, Nacht (Donker), Avond (Warm donker)
- 💾 **Eigen sferen opslaan** — naam geven en als preset bewaren, met live preview
- 🖼️ **Achtergrond textuur** — eigen afbeelding als achtergrond mogelijk
- 📤 **Sfeer exporteren** als JSON

#### Audio & MIDI
- 🎼 **Per-pijp voicing UI** — interactieve sliders voor volume (±12 dB) en stemming (±50 ¢) per pijp, met visuele indicatoren voor aangepaste pijpen
- 💾 **Voicing persistentie** — automatisch opgeslagen in `.jm-settings.json` per orgel, hersteld bij laden
- ▶️ **MIDI file playback** — `.mid` / `.midi` / `.smf` bestanden afspelen door het orgel met play/pause/stop, seek-balk en snelheidsregelaar (0.5×–2×)
- 🎵 **Sample rate display** — actuele rate + ondersteunde rates van het audio-device, met helptekst voor 96 kHz instelling
- 🎵 **52 historische temperamenten** — Lambert (1774), Barnes-Bach (1979) en Vallotti-Young toegevoegd

#### UX
- 📱 **Touchscreen-optimalisatie systematisch** doorgevoerd via design tokens (`--touch-target`) over alle knoppen, sliders, inputs, lijsten, context menus

### Gewijzigd

- 🎨 **Donker / licht thema toggle vervangen** door vrij instelbaar systeem met presets — meer flexibiliteit voor de gebruiker
- 🎨 **Volledige design tokens migratie** — alle hardcoded kleuren in `styles.css` en componenten omgezet naar CSS custom properties zodat sfeer-aanpassingen consistent doorwerken
- 📦 **CSS architectuur** — één centrale token-set in `:root`, geen losse hex-waarden meer in selectors

### Opgelost

- 🐛 Sample rate stond niet meer expliciet in de UI — nu zichtbaar in Instellingen
- 🐛 Sommige kleur-aanpassingen via Instellingen pakten niet overal door — alle componenten gebruiken nu tokens

## [0.2.0] - 2026-03-28

### Toegevoegd

#### Audio Engine
- 🌬️ **Windmodel per divisie** — reservoir, demping, max drukverlies
- 🎵 **Tremulant LFO synthese** als fallback wanneer geen tremulant-samples beschikbaar
- ⛪ **FDN algoritmische galm** met 6 ruimtepresets (Kapel → Gotische Kathedraal)
- 🎚️ **3-band Parametric EQ** (low shelf + mid peak + high shelf)
- 🔇 **Aanpasbare zwelkast** — volume + frequentie-afhankelijk filter
- 🎼 **Per-pijp voicing** — volume (±20dB) + pitch (±100ct) per individuele pijp
- 🎵 **96 kHz HD audio** support
- 📊 **Per-divisie stereo panning** (constant-power) met persistente waardes

#### MIDI
- 🔵 **Bluetooth LE MIDI** — directe BLE GATT verbinding via `btleplug`
- 🔄 **Auto-connect** alle USB MIDI bij opstarten
- 📝 **MIDI file recording** (Standard MIDI File writer)
- 🎚️ **Activity counter** voor BLE messages

#### Registers & Koppels
- 🎛️ **Drag & drop register sortering** per divisie
- ⬆️ **Superoctaaf** (4', +12)
- ⬇️ **Suboctaaf / Octave Grave** (16', -12)
- 🔇 **Tongwerken Af** per divisie
- 🎼 **Melodiekoppel** (alleen hoogste noot)
- 🎚️ **Baskoppel** (alleen laagste noot, naar pedaal)
- 🎚️ **Generaal Crescendo** met live LED-indicator
- 💾 **8 memory levels × 1000 presets** = 8.000 combinaties
- ⏯️ **Held-notes re-trigger** bij stop toggle

#### Stemmingen
- 🎵 **50 historische temperamenten** (was 33)
- ✏️ **Custom temperament editor** met 12 cent-input fields
- 🎯 **Per-noot tuning** in audio engine

#### UI/UX
- 🌙 **Donker / licht thema** toggle (zon/maan icoon)
- 📱 **Touchscreen-optimalisatie** (`@media pointer: coarse`)
- ⌨️ **Keyboard shortcuts** — F1/F2 navigatie, Esc voor orgel-selectie, arrow keys tussen tabs
- ♿ **ARIA accessibility labels** op stops, koppels, tabs
- 📚 **Orgel bibliotheek** met persistente settings
- 💾 **Auto-save** instellingen in orgelmap + AppData
- 📤 **Export / import** settings als JSON
- 🪟 **Apart register-paneel** venster

#### Bestandsformaten
- 📄 **`.organ` file export** — genereer ODF vanuit sample directory
- 🎵 **MP3 sample support** (naast WAV)
- 🔁 **Loop point extraction** uit WAV smpl chunk
- 🔄 **Auto-resample** naar audio device sample rate

### Gewijzigd

- ✏️ Verwijzingen naar externe orgelsoftware uit UI (algemener gemaakt)
- 🎨 Nieuwe orgelbouwer-stijl UI (crème/bladgoud thema)
- 📁 File logging in `%APPDATA%\nl.jm-orgue.app\jm-orgue.log`
- 🔧 Bundle config voor macOS (.dmg) en Linux (.AppImage/.deb) klaar
- 🎯 Sample rate is nu device-gestuurd (geen hardcoded 48000)

### Opgelost

- 🐛 BLE MIDI HRESULT 0x80000013 ("object closed") door subscribe-retry mechanisme
- 🐛 BLE MIDI deadlock tijdens learn — nieuwe `learn_wait_settled` helpers
- 🐛 BLE MIDI berichten kwamen niet binnen → notification stream wordt nu na subscribe geopend
- 🐛 Crescendo CC werd niet verwerkt — `process_crescendo_cc` toegevoegd
- 🐛 Status check zag BLE-verbonden apparaten niet als "verbonden"
- 🐛 Sample upgrade van preload→full had positie-conversie fout
- 🐛 Custom temperament editor sloot niet correct
- 🐛 Pan slider behield value 0 ongeacht localStorage

## [0.1.0] - 2025-09-17

### Toegevoegd

- Initial release: basis sample playback, MIDI input, .organ ODF parsing
- Convolutie reverb met IR loading
- Setzer met MIDI learn (basis 100 presets)
- Sample directory scanner
- Per-divisie zwelkast (alleen volume)
- 33 temperamenten
- Coupler systeem (unison)
