# Changelog

Alle belangrijke wijzigingen van JM-Orgue worden hier bijgehouden.

Format gebaseerd op [Keep a Changelog](https://keepachangelog.com/),
versies volgen [Semantic Versioning](https://semver.org/).

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
