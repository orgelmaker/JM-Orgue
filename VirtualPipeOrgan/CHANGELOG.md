# Changelog

Alle belangrijke wijzigingen van JM-Orgue worden hier bijgehouden.

Format gebaseerd op [Keep a Changelog](https://keepachangelog.com/),
versies volgen [Semantic Versioning](https://semver.org/).

## [0.7.0] - 2026-08-06

### Nieuw — volledige hoofdbalk op extra schermen

- 🪟 **Extra registerschermen kunnen nu de volledige hoofdbalk tonen**: orgelnaam, de tabbladen Orgel / Orgel-instellingen / Algemene instellingen (die werken écht in het extra venster), de profielwissel speakers↔hoofdtelefoon en audio start/stop. Aan/uit te zetten per scherm met de knop "Balk" in de werkbalk (en het pijltje in de balk zelf om te verbergen); de keuze wordt per scherm en per orgel onthouden. Standaard staat de balk uit — het vertrouwde compacte scherm blijft de start
- 🔁 De extra schermen zijn onder de motorkap nu volwaardige consoles: dezelfde registerweergave als het hoofdscherm (inclusief knop-verslepen en MIDI-inleren via rechtermuisklik), met eigen divisiekeuze en indeling per scherm. Acties die het hele programma aangaan (orgel laden/sluiten, profielwissel, nieuw scherm, afsluiten) lopen netjes via het hoofdvenster
- 🎛️ Wijzigingen in knopgrootte, knopvorm, zwelkast-/koppelzichtbaarheid en registervolgorde volgen nu binnen een seconde in álle vensters (ook het hoofdvenster volgt de extra schermen)

### Nieuw — muziek noteren (bladmuziek uit je eigen spel)

- 🎼 **Nieuwe knop "Noteren"** naast de opnameknoppen: je MIDI-opname wordt omgezet naar notenschrift en getoond in een eigen notatievenster — volledig in de app, geen extern programma nodig. Elke divisie krijgt een eigen notenbalk (Pedaal in de bassleutel), gelijktijdige noten worden akkoorden, rusten en overbindingen worden ingevuld
- 🖨️ Vanuit het notatievenster: **afdrukken** (kies "Microsoft Print to PDF" voor een PDF-bestand) en **opslaan als MusicXML** (te openen in elk notatieprogramma). Tempo, maatsoort, toonsoort, kwantisatieraster en titel zijn achteraf bij te stellen; de weergave volgt direct
- 🎚️ **Vrij instelbare balk-indeling**: per notenbalk vink je aan welke divisies erop komen (bv. Hoofdwerk + Nevenwerk samen op één balk, Pedaal apart), met eigen naam en bassleutel-keuze per balk; balken zijn toe te voegen en te verwijderen
- ℹ️ Eerlijke verwachting: vrij ingespeeld werk kwantiseert het mooist wanneer je in een vast tempo speelde; kies anders een grover raster. Bewerken op nootniveau volgt in een latere versie (onderzocht: een inbouwbare open-source editor à la MuseScore bestaat niet; weergave draait op OpenSheetMusicDisplay, BSD-licentie)

### Bekende beperkingen (bewust geaccepteerd)

- Instellingen-schuiven die tegelijk in twee vensters zichtbaar zijn synchroniseren niet real-time; het extra scherm ververst bij het openen van het tabblad
- Een profielwissel die de audio opnieuw opbouwt sluit en heropent de extra schermen (bestaand gedrag)
- Notatie toont één stem per notenbalk: overlappend spel wordt ingekort tot de volgende inzet

## [0.6.14] - 2026-07-29

### Opgelost — audio-wissel (speakers ↔ hoofdtelefoon) liep vast

- 🔒 **Permanente deadlock gedicht**: de uitgestelde ASIO-wissel en het audio-noodherstel namen de load/wissel-vergrendeling en riepen daarbinnen het orgel-herladen aan, dat dezélfde vergrendeling opnieuw nam — die staat dan voorgoed op slot, waarna elke audio-wissel bleef hangen en de eerstvolgende orgel-load of autosave de hele app bevroor
- 🧊 **UI bevriest niet meer tijdens laden/wisselen**: orgel laden, samples-map laden en het automatische instellingen-opslaan draaien nu op een achtergrond-thread (zoals de audio-wissel zelf al deed) in plaats van op de venster-thread — de voortgangsbalk loopt door en de vensters blijven reageren
- 🔁 **Status-poll kon de wissel doodhouden**: de 100ms-statuspoll nam dezelfde lees-vergrendeling twee keer binnen één expressie; zodra de wissel stond te wachten om te schrijven was dat een blijvende blokkade van de UI-thread — herschreven naar één vergrendeling

### Opgelost — hangers bij de generaal crescendo

- 🎹 **Koppels in crescendotrappen lieten stemmen wees worden**: ging een koppel uit terwijl toetsen ingedrukt waren, dan kregen de gekoppelde stemmen nooit meer een note-off (note-off rekent met de níeuwe koppelstand) — permanente hangers. Koppelwissels (crescendo, presets, koppelknoppen) synchroniseren nu de stemmen: weggevallen routes zwijgen, bijgekomen routes klinken direct mee
- 👻 **Spooktoetsen opgeruimd**: de lijst "ingedrukte toetsen" werd nooit gewist bij panic/all-notes-off, orgelwissel of het stoppen van MIDI-afspelen; één achtergebleven toets wekte bij elke crescendotrap opnieuw stemmen tot leven die nooit meer stopten. Alle panic-/wissel-routes wissen de lijst nu mee
- 🦶 **Pedaal-sweeps overbelastten de MIDI-verwerking niet meer**: bij een snelle crescendobeweging telt per verwerkingsronde alleen de laatste pedaalstand (tussentrappen worden overgeslagen) — voorheen kon de stroom trapwissel-commando's de MIDI-thread seconden blokkeren waardoor binnenkomende note-offs verloren gingen (ook een bron van hangers)
- 🎚️ **Trap-/preset-wissel kon een register laten doorklinken**: de vergelijking "wat is er gewijzigd" keek alleen naar de UI-spiegel; kruiste de crescendotrede het commando, dan miste een weggevallen register zijn release — er wordt nu tegen beide administraties gediffed

### Opgelost — zweltrede en extra schermen

- 💡 **Zwelkast-lampjes lopen weer soepel mee bij meerdere schermen**: elk extra registerscherm bouwde elke 300 ms zijn volledige beeld opnieuw op (de wijzigings-check van het hoofdvenster ontbrak daar) — nu wordt alleen bij échte wijzigingen ververst
- 🔄 **Omgekeerd gemonteerde zweltrede via het oude inleerpad** zette de inversie-vlag niet (lampjes en volume liepen andersom) — gefixt
- 🪟 **Extra schermen kwamen na een herstart niet allemaal terug**: bij het heropenen kregen twee schermen die binnen dezelfde milliseconde openden hetzelfde venster-label, waardoor het tweede stil faalde en ook nog uit de onthouden lijst werd gewist. Elk scherm bewaart zijn positie nu ook bij het sluiten

## [0.6.13] - 2026-07-25

### Nieuw — extra registerschermen worden per orgel onthouden

- 🪟 **Extra schermen (2e, 3e, …) komen terug zoals je ze achterliet**: per orgel wordt onthouden welke schermen open stonden, met per scherm de gekozen divisies én de vensterpositie/-grootte. Bij het (opnieuw) laden van het orgel — ook na een herstart — gaan ze automatisch weer open. Zelf een scherm sluiten haalt het uit het onthouden lijstje; een orgelwissel of het afsluiten van de app deed dat voorheen óók (per ongeluk) — dat is gefixt

### Nieuw — opname- en bedieningsknoppen op de extra schermen

- 🔴 **MP3- en MIDI-opname kunnen nu vanaf elk registerscherm** gestart en gestopt worden (de status loopt op alle schermen synchroon), inclusief "Opname afspelen" na een MIDI-opname. Ook "Nieuw scherm" en "Afsluiten" staan nu op de extra schermen

### Nieuw — ronde registerknoppen

- ⚪ **Keuze tussen rechthoekige en ronde registerknoppen** (klassieke trekregisters): knop "Rond"/"Recht" in de werkbalk van het orgelscherm. De keuze wordt per orgel bewaard en geldt automatisch ook voor de extra schermen

## [0.6.12] - 2026-07-24

### Nieuw — natuurlijke galm bij staccato (onderzoek fase C)

- 🌫️ **Release-galm wordt geschaald naar de nootduur**, zoals GrandOrgue: bij een noot die tijdens de aanslag al wordt losgelaten start de release zachter (20-100%, afhankelijk van hoe ver de pijp "op spraak" was), en bij korte noten wordt de galmstaart actief afgebouwd — in de opname is de kerkgalm dan immers nog niet volledig opgebouwd ("time to full reverb", 100-350 ms afhankelijk van de release-lengte). Staccato op natte sets klinkt daardoor niet meer alsof elke tik de volle kathedraalstaart dumpt; lange noten behouden hun volledige, natuurlijke uitklank

## [0.6.11] - 2026-07-24

### Nieuw — release-fase-uitlijning voor natte samplesets (onderzoek fase B)

- 🎼 **Release-samples starten nu fase-uitgelijnd op het klinkende signaal**, zoals GrandOrgue dat doet. Bij het laden wordt per release-sample een uitlijningstabel gebouwd (16×16 amplitude×helling → startpositie); elke spelende stem onthoudt zijn laatste twee golfvorm-waarden, en bij het loslaten begint de release op de positie die daarop aansluit — in plaats van altijd op het begin. Dit haalt de fase-tik weg die je op sets als Friesach bij élke losgelaten toets kon horen, zowel bij losse noten als bij het wegtrekken van registers

## [0.6.10] - 2026-07-24

### Verbeterd — treden inleren nu ook met stap-voor-stap-popup

- 🦶 **Zweltrede en generaal crescendo leren nu net als de klavieren**: popup zegt "zet de trede in de LAAGSTE stand en houd hem stil", wordt groen zodra de stand herkend is (met CC, kanaal en waarde), vraagt dan de hoogste stand en bevestigt. Een omgekeerd gemonteerde trede wordt automatisch herkend (inversie), en time-outs geven een duidelijke melding met "Opnieuw"

### Opgelost — presets sprongen alle kanten op tijdens het spelen

- 🎛️ **Preset-knoppen op noot onthouden nu ook het MIDI-kanaal.** Een piston ingeleerd op "noot 20" vuurde voorheen óók wanneer je toets 20 op een klavier speelde — vandaar het continue preset-wisselen tijdens het spelen. Nieuw ingeleerde preset-knoppen reageren alleen nog op hun eigen kanaal (bestaande koppelingen: één keer opnieuw inleren)

### Nieuw — koppels in de generaal crescendo

- 🔗 **De crescendo-matrix heeft nu ook rijen voor de koppels**: vink per stap aan welke koppels erbij komen. De trede claimt — net als bij registers — geen koppels die je zelf al had ingeschakeld

### Onderzoek — externe samplesets (GrandOrgue-vergelijking)

- 🔬 De GrandOrgue-broncode is bestudeerd om onze afspeelopzet voor natte GO-sets te toetsen; volledig verslag in `ONDERZOEK-EXTERNE-SETS.md`. Kern: GrandOrgue lijnt release-samples in FASE uit op het klinkende signaal (wij starten op de cue → tikken bij loslaten) en schaalt release-galm bij staccato ("time to full reverb" — verklaart de "rare galm"). Voorgestelde fasering staat in het verslag; als eerste stap is de release-budget-fade verzacht naar 250 ms (GrandOrgue-niveau)

## [0.6.9] - 2026-07-24

### Verbeterd — klavier inleren met echte stap-voor-stap-popup

- 🎹 **Nieuw inleerproces voor klavieren**: een popup vraagt "druk de LAAGSTE toets in", wordt groen zodra de toets herkend is (met nootnaam en kanaal), vraagt dan de hoogste toets en bevestigt het geleerde bereik. Time-outs en een tweede toets op een ander kanaal geven een duidelijke foutmelding met "Opnieuw" — het oude proces raadde met een timer en gaf geen terugkoppeling

### Opgelost — zweltrede inleren na een eerdere crescendo-koppeling

- 🦶 **Zwel- en crescendo-inleer zijn nu wederzijds exclusief (laatst ingeleerd wint).** Was een trede eerder als generaal crescendo ingeleerd en leerde je hem daarna als zwelkast, dan bleef de oude crescendo-koppeling de CC "claimen" en deed de zwelkast niets. Bij het inleren van een zwelkast wordt een crescendo-koppeling op dezelfde CC nu gewist, en andersom

### Opgelost — pistons op het klavierkanaal klonken nog als noten

- 🎛️ **Het ingeleerde toetsbereik filtert nu ook**: pistons/setzerknoppen die MIDI-noten sturen op hetzélfde kanaal als een klavier (maar buiten het ingeleerde laagste-hoogste-bereik) klinken niet meer als orgelnoten en zijn gewoon inleerbaar als preset-knop. Leer wel eerst het klavierbereik in (met de nieuwe popup), anders is er geen bereik om op te filteren

### Opgelost — Friesach: kraken en vastlopen bij vol werk loslaten

- 🌊 **Release-staarten begrensd tot een budget (160 tegelijk).** Bij het loslaten van vol werk op een natte GO-set startte per pijp × register een extra release-stem (de opgenomen kerkakoestiek): 200+ nieuwe stemmen in één klap, ruim 600 totaal — de mixer verzoop, het kraakte en de afbouw duurde heel lang. Boven het budget maakt de stilste bestaande staart nu versneld plaats; eigen droge sets (Puttershoek) hebben geen release-samples en vallen zoals voorheen direct terug naar 0

## [0.6.8] - 2026-07-23

### Opgelost — niets ingeleerd = geen respons (treden én pistons)

- 🦶 **Een zwel-/crescendotrede doet nu niets totdat je hem inleert.** Consoles sturen hun treden standaard als CC7/CC11 (volume/expressie) en een oude "gemaks"-fallback liet elke onbekende CC7/11 meteen het mastervolume regelen — met drie treden had je dus drie ongevraagde volumeregelaars die elkaar overschreven, zonder dat er iets was ingeleerd. Die fallback is verwijderd: een trede krijgt pas een functie als jij hem inleert als zwelkast (per klavier) of als generaal crescendo
- 🎹 **Piston-/setzerknoppen die MIDI-noten sturen klinken niet meer als orgelnoten.** Een klavier zonder ingeleerde koppeling accepteerde voorheen "alle kanalen", waardoor knopdrukken van de console als noten doorklonken. Een klavier reageert nu pas op fysieke MIDI zodra het is ingeleerd (in de MIDI-koppelingen-editor kun je desgewenst nog expliciet "alle kanalen" kiezen). Het schermklavier en de computertoetsen werken gewoon zonder inleren; let op: ook MIDI-bestanden afspelen vereist ingeleerde klavieren op het orgel

### Opgelost — haperen/tikken bij veel registers op grote samplesets

- 🚿 **De logstorm per toetsaanslag is gedempt**: elke noot schreef 1 + (aantal klinkende registers) regels naar console én logbestand — bij vol werk op Friesach honderden regels per seconde, die de MIDI-verwerking lieten stotteren precies op de drukke momenten. Per-noot-logging staat nu standaard uit (alleen nog op trace-niveau voor diagnose)
- ⚙️ **Achtergrond-sampleladers draaien onder normale prioriteit**: het decoderen van volledige samples (vol werk aanslaan/loslaten op een natte set) verdrong de audio-verwerking van de CPU — een bekende bron van tikken. De audio-thread heeft nu altijd voorrang; samples laden hooguit iets langzamer bij, wat de preload-buffers opvangen

## [0.6.7] - 2026-07-23

Grote onderhoudsronde: 53 van de 63 bevindingen uit de code-audit van 18 juli verwerkt
(zie AUDIT-2026-07-18.md en FIXLOG-0.6.7.md in de repo; 3 deels, 7 bewust gelaten met reden).

### Audio-betrouwbaarheid

- 🎛️ **Geen zware operaties meer in de realtime audio-lus**: eigen galm-IR's worden voortaan buiten de audio-thread gedecodeerd en voorbereid (geen haper meer bij het laden), grote datastructuren worden niet meer gekopieerd of gealloceerd in de callback, en de "geen sample"-melding logt nog maar één keer per pijp
- 🔇 **Een corrupt sample kan de filters niet meer blijvend vergiftigen**: niet-eindige waarden worden nu vóór de zwelkast-/galm-/EQ-keten afgevangen
- 🎧 **Eigen galm-IR's klinken correct**: samplerate wordt geresampled (galm speelde te snel/langzaam af), 32-bit IR's hebben geen omgekeerde polariteit meer, en een leeg IR-bestand kan de app niet meer laten crashen
- 🔉 **Voice-stealing zonder klik**: bij het 512-stemmen-plafond krijgt de stilste stem een korte fade in plaats van een harde knip
- 📻 **Kanaal-routering valt terug op het voorste paar** wanneer de geconfigureerde kanalen niet op het apparaat bestaan (droog signaal verdween geluidloos terwijl de galm doorklonk)
- 🌊 **ODF-looppunten gelden nu ook voor de volledig geladen sample** (loop versprong hoorbaar op het moment dat de achtergrond-load klaar was)

### Stabiliteit en samenspel van threads

- 🧵 Diverse blokkades en races opgelost: recv-timeouts richting de MIDI-thread (app bevroor als je iets deed tijdens een leer-modus), lock-volgordes gelijkgetrokken (deadlock-scenario), instellingen-opslag geserialiseerd met orgel-loads, en de audio-wissel houdt zijn lock niet meer 2 s vast
- 🎹 **Klavier-inleren laat klinkende noten niet meer eeuwig hangen** (NoteOffs worden nu doorgestuurd tijdens het leren)
- 🚨 **Na een audio-noodherstel of de uitgestelde ASIO-start worden alle DSP-instellingen (volume, galm, EQ, routing, pans) weer toegepast** — voorheen speelde het orgel dan op kale defaults
- 💾 **.jm-settings.json naast het orgel wordt nu echt gebruikt**: op een verse pc/AppData worden alle instellingen (inleren, presets, voicing) daaruit geïmporteerd in plaats van stilletjes overschreven

### GrandOrgue-import en eigen orgels

- 🛡️ **Corrupte orgelbestanden kunnen de app niet meer crashen**: caps op ODF-tellers (ReleaseCount e.d. gaven een ~200 GB-allocatie), ADPCM/misvormde WAVs worden netjes geweigerd i.p.v. een paniek, misvormde REF-regels wijzen niet meer stil naar de verkeerde pijp, en leesfouten in samples worden gelogd
- 🔁 **Eigen-orgel-export is round-trip-correct**: pijpen behouden hun echte toets (discant-registers verschoven twee octaven), voetmaten verdubbelen niet meer, en bas/discant-mappen worden weer als één register herkend
- 🎚️ **Octaafkoppel op het eigen klavier klinkt één octaaf** — niet tot acht octaven doorgestapeld

### Bediening

- 🎬 **MIDI-afspelen**: zoeken, pauzeren en snelheid wisselen springen niet meer en veroorzaken geen noten-vloed
- 🎛️ **Setzer**: geheugenniveau-wissel laadt het juiste niveau, presets lekken niet meer tussen niveaus/orgels, setzer-presets komen nu wél in de per-orgel-opslag terecht, en opgespaarde piston-drukken van tijdens een orgel-load worden niet meer in één keer afgespeeld
- 🪟 **Extra registerpanelen** kapen geen MIDI-preset-acties meer van het hoofdvenster en tonen na een orgelwissel geen leeg paneel meer
- 🌬️ Windgroep-keuze wordt niet meer teruggedraaid terwijl je hem instelt; na een uitgang-/profielwissel worden per-orgel DSP-instellingen opnieuw toegepast; het wijzigen van het aantal crescendostappen wist de matrix niet meer
- 🔌 Twee identiek genaamde MIDI-interfaces verbinden nu allebei; een tweede app-instantie wist het log van de draaiende instantie niet meer; de sampleset-trimtool bewaart cue/release-markers en schrijft crashveilig

## [0.6.6] - 2026-07-20

### Opgelost — instellingen lekten door naar het volgende orgel (o.a. zweltreden)

- 🎚️ **Zwel-, MIDI-klavier-, preset- en crescendo-koppelingen worden bij een orgelwissel volledig gewist** en daarna alleen gevuld met wat het níeuwe orgel zelf heeft opgeslagen. Voorheen bleven de koppelingen van het vorige orgel actief — een zweltrede kon dan het verkeerde klavier aansturen ("er gebeurt iets met het geluid") — en werden ze via de autosave zelfs blijvend in het nieuwe orgel weggeschreven. Elk orgel heeft nu strikt zijn eigen koppelingen (eenmalig per orgel inleren; daarna onthouden)
- 🔊 **Mastervolume en galm van het vorige orgel lekken niet meer in een vers orgel**: een orgel zonder eigen instellingen start nu écht op de standaardwaarden (-6 dB; GO/Hauptwerk-imports droog) in plaats van op de stand van het vorige orgel — en die lek-stand wordt dus ook niet meer als "instelling" van het nieuwe orgel opgeslagen
- 🌬️ Ook het aantal windgroepen en de C/Cis-spreiding gaan bij een orgelwissel terug naar de standaard (vielen buiten de bestaande reset)

### Gewijzigd — generaal crescendo respecteert handmatige registratie

- 🦶 **De crescendotrede claimt geen registers meer die je zelf al getrokken had** (GrandOrgue-gedrag): bij het terugveren van de trede verdwijnen alleen de registers die de trede zélf bijtrok — je eigen registratie blijft onaangetast staan

## [0.6.5] - 2026-07-18

### Opgelost — generaal-crescendo maakte het orgel zachter

- 🎚️ **De crescendotrede dempt het volume niet meer.** Stuurt de trede CC 7 of CC 11 (volume/expressie — gebruikelijk bij veel consoles), dan werd dezelfde pedaalbeweging óók door de algemene expressie-fallback verwerkt: registers kwamen erbij, maar het mastervolume ging tegelijk fors omlaag ("registers erbij, geluid zachter"). Een als crescendo ingeleerde pedaal is nu exclusief crescendo — hij doet niets meer met zwelkast- of expressievolume, ook niet tijdens het afspelen van een MIDI-opname
- 🦶 **Crescendostand start altijd op 0 bij het laden van een orgel** (de teller bleef voorheen op de oude trap hangen, waardoor de eerste pedaalbeweging binnen dezelfde trap niets deed)
- 🔁 **Na het (opnieuw) inleren van de crescendotrede wordt het mastervolume expliciet teruggezet**: tijdens het inleren is de koppeling tijdelijk gewist en kan een CC7/11-pedaal het volume nog dempen via de expressie-fallback; omdat die CC daarna exclusief crescendo is, bleef die demping anders blijvend hangen (gevonden bij de code-audit)

### Opgelost — audio-wissel: registerknoppen bleven "uit" staan

- 🎛️ **Na een speakers↔hoofdtelefoon-wissel tonen de registerknoppen weer de echte stand.** De registratie wérd al hersteld (sinds 0.6.3), maar het orgel-overzicht dat de herlaad teruggaf was gemaakt vóór dat herstel — alle knoppen oogden "uit", en de verversing corrigeerde dat niet omdat de herstelde stand identiek was aan die van vóór de wissel. Wie dan een knop aanklikte zette het register juist wéér uit — het "registers vallen uit"-gevoel. De load geeft nu de stand ná het herstel terug

### Nieuw — kanaalkeuze wisselt mee met de uitvoerprofielen

- 🔀 **Uitvoerprofielen (speakers/hoofdtelefoon) slaan nu ook de kanaalkeuze per klavier op.** Zo speel je op dezelfde ASIO-interface met speakers op eigen kanalen én een hoofdtelefoon op een ander kanalenpaar — de routering wisselt met één knop (of MIDI-knop) mee. "Huidige selectie opslaan" legt host, apparaat, buffer én de kanaalkeuze per klavier vast (op klaviernaam; ook bij een ander orgel met dezelfde klaviernamen toegepast)
- 🧷 De profiel-kanalen gelden als tijdelijke override: de per-orgel opgeslagen kanaalroutering blijft onaangetast bewaard en komt terug zodra je geen profiel (of een profiel zonder kanalen) actief hebt. Handmatige kanaal-wijzigingen terwijl een profiel actief is werken direct en blijven binnen dat profiel gelden; sla het profiel opnieuw op om ze blijvend te maken

### Gewijzigd — schone start bij het laden van een orgel

- 🧹 **Koppels starten nu ook uit.** "Laatste registratie herstellen bij openen" (standaard uit) gold al voor de registers, maar de koppels kwamen onvoorwaardelijk terug uit de vorige sessie. Nu vallen registers én koppels onder dezelfde keuze: optie uit = volledig schone start, optie aan = alles komt terug. (Een audio-wissel behoudt de live registratie uiteraard wél — dat is een herlaad van hetzelfde orgel, geen nieuwe start)

## [0.6.4] - 2026-07-14

### Opgelost — klavieren inleren blijft nu bewaard

- 🎹 **Ingeleerde klavieren en MIDI-kanalen zijn na een herstart gewoon terug.** Twee oorzaken gevonden: (1) de backend herstelde de koppelingen wél bij het laden van een orgel, maar de UI ververste de lijst nooit — je zág dus "leeg" en leerde onnodig opnieuw in; (2) geleerde koppelingen werden alleen bij een nette afsluiting opgeslagen (met een tijdslimiet die dat kon afbreken) — nu worden ze direct na het inleren automatisch bewaard, net als de registratie
- 🔌 **MIDI-klavieren activeren zichzelf**: een console die pas ná de app wordt ingeschakeld (of waarvan de eerste verbindingspoging tijdens de USB-handshake mislukt) wordt nu elke 5 seconden opnieuw geprobeerd — handmatig MIDI "aan/uit zetten" is niet meer nodig

### Opgelost — zwelkasten uit de orgeldefinitie werken nu

- 🎚️ **GO-/Hauptwerk-zwelkasten worden herkend**: moderne orgelbestanden zetten de windlade-koppeling op de rank in plaats van op het register, waardoor de zwelkast-detectie (windlade → Enclosure) nooit aansloeg. Friesach's Schwellwerk heeft nu gewoon zijn zweltrede in de app, precies zoals de orgeldefinitie voorschrijft

### Verbeterd — klankkwaliteit GO-sets

- 🌊 **Fase-uitgelijnde loops óók in de eerste seconden van elke noot**: de anti-tik-optimalisatie van looppunten draaide alleen op de volledige samples, niet op de preload-buffers waar elke noot zijn eerste ~2 seconden op speelt. Nu overal; en een loop die door de stilte-trim beschadigd raakte wordt herkend i.p.v. stilletjes verschoven
- 🎼 **Vloeiender interpolatie (4-punts Hermite i.p.v. lineair)**: een 48kHz-set op een 44,1kHz-apparaat (en elke tremulant-/temperament-verstemming) klinkt merkbaar schoner op heldere registers — minder aliasing-ruis
- ⚡ **Drie parallelle sample-laders i.p.v. één**: na het loslaten van een vol akkoord op een natte set worden de release-staarten drie keer zo snel bijgeladen
- 🎛️ **Limiter verfijnd** (plafond -0,26 dB, gedoseerde attack van 1,5 ms): één piek trekt niet langer het hele orgel hoorbaar omlaag. Crescendo-meting op Friesach: gelijkmatig stijgend van 1 register tot vol werk, geen "wegzakken" meer

### Gewijzigd — standaardinstellingen voor externe samplesets

- 🌫️ **GO-/Hauptwerk-imports starten voortaan zónder toegevoegde galm** (de kerkakoestiek zit al in de opnamen; de standaard 30% "Dorpskerk" er bovenop gaf een troebel dubbel-galm-geluid). Eigen droge samplemappen behouden de vertrouwde standaardgalm; een zelf ingestelde galm blijft altijd bewaard. Alle andere effecten (EQ, windmodel, C/Cis-spreiding, tremulant-LFO) stonden al correct uit

## [0.6.3] - 2026-07-13

### Opgelost — vol werk: geen vervorming en wegzakkend geluid meer

- 🎛️ **Echte master-limiter in plaats van tanh-vervorming.** Bij een vol werk (honderden stemmen tegelijk) kwam de mix-som ver boven het maximum en drukte de oude tanh-verzadiging per kanaal het signaal plat: hoorbare vervorming ("overstuur"), platgeslagen dynamiek ("gedempt" geluid — meer registers erbij deed niets meer) en een instortend stereobeeld (elk kanaal vervormde onafhankelijk). De nieuwe limiter verlaagt bij een piek de tótale gain (attack direct, release ~250 ms): geen vervorming, de onderlinge balans en het stereobeeld blijven intact, en na de piek veert het niveau vanzelf terug. De MP3-opname gebruikt dezelfde limiter, zodat opnames klinken zoals er gespeeld werd
- 🌬️ **Windmodel-demping herijkt** (alleen relevant als het windmodel aanstaat): de oude formule zat al bij ~3 stemmen op de maximale inzakking, waardoor het model als botte volumedemper werkte bij vollere registraties. De inzakking loopt nu vloeiend op met het aantal stemmen en nadert het maximum pas asymptotisch bij een echt vol werk

### Opgelost — geheugengroei tijdens lang spelen (gekraak/bevriezing)

- 🧠 **RAM-plafond op volledig geladen samples (~1,5 GB, oudste eerst weg).** Elke gespeelde pijp + release bleef voorheen voorgoed in het geheugen: uren spelen op een grote GO-set liet het geheugen tot vele gigabytes groeien → paging → gekraak. Een verdrongen sample is altijd veilig (klinkende stemmen houden hun eigen kopie vast) en komt bij een volgende noot gewoon terug via preload + achtergrondlaad
- 🧹 **Grote geheugenopruiming nooit meer op de audio-thread**: bij een orgelwissel na lang spelen werden gigabytes aan samples ín de audio-callback vrijgegeven — de audio stond dan seconden stil, de watchdog sloeg aan en de app kon volledig vastlopen (live gereproduceerd). Oude sample-verzamelingen gaan nu naar een aparte opruim-thread
- ⏱️ **Commandoverwerking per callback begrensd** (max 256): een tutti-burst wordt over enkele callbacks uitgesmeerd (~5 ms, onhoorbaar) in plaats van één callback over zijn realtime-deadline te duwen

### Opgelost — hoofdtelefoon/kanalen-wissel: registratie blijft nu staan

- 🎚️ **Een audio-uitvoer-wissel gooit de getrokken registers niet meer weg.** De verplichte orgel-herlaad na elke wissel (speakers↔hoofdtelefoon, WASAPI↔ASIO) wiste de registratie en herstelde die alleen wanneer de losse "herstel laatste registratie"-optie aanstond. Nu wordt de live registratie (registers + koppels) vlak vóór de wissel vastgelegd en na de herlaad van hetzélfde orgel automatisch teruggezet — onafhankelijk van die optie
- 🔀 **Race gefixt waarbij het noodherstel/de uitgestelde ASIO-wissel het verkéérde orgel kon terugladen**: laadde je zelf net een ander orgel terwijl het herstel liep, dan werd jouw keuze overschreven door het oude orgel-id. Beide herlaadpaden controleren nu eerst of hun orgel-id nog actueel is
- 🔉 **WASAPI→WASAPI-wissels breken de oude stream nu echt af** (begrensd wachten) in plaats van hem zwevend achter te laten — geen kort dubbel geluid of apparaat-contentie meer direct na de wissel

### Opgelost — GrandOrgue-sets: kwaliteit en stabiliteit

- 🎼 **Loop-naad-tikken op veel GO-sets verholpen**: de fase-uitlijning van looppunten draaide alleen in het standaard-WAV-pad; bestanden met een afwijkende fmt-header (o.a. álle Piotr Grabowski-sets) sloegen die optimalisatie over en hielden hoorbare tikken op de loop-naad. De juiste parser wordt nu vooraf gekozen (fmt-probe) en de loop-optimalisatie draait in álle paden
- 🗝️ **Meerkorige registers worden nu op toets-positie gelegd** (RankFirstAccessibleKeyNumber gerespecteerd): gesplicede koren (geleende bas, repeterende kwint) komen op de juiste toetsen terecht, óók als de ranks niet netjes aansluitend in de ODF staan; een misvormde REF-regel schuift niet langer alle volgende pijpen een toets op. Écht gestapelde koren (zelfde toetsbereik dubbel) spelen nog één koor — dat staat nu expliciet met een waarschuwing in het log
- 🔇 **Stille toetsen zijn nu traceerbaar**: een onopgeloste REF-verwijzing (de bekende stille-toetsen-oorzaak) logt voortaan precies welk register en welke pijp geen sample kreeg; een REF-keten met een cyclus crasht de app niet meer (dieptebegrenzer)
- 🧟 **"Zombie"-stemmen verholpen**: bij een volle achtergrondlaad-wachtrij (tutti loslaten op een natte set) werden laadverzoeken stil gedropt en bleef een stem eindeloos zijn 2-seconden-attackfragment loopen. Wachtrij vergroot (64→1024), verzoeken worden per pijp ontdubbeld en een gedropt verzoek blokkeert een nieuwe poging niet meer
- ⚡ **Zelfde bestand niet meer dubbel gedecodeerd**: REF-gedeelde ranks en same-file-releases gebruiken nu een pad-cache in de achtergrondlader (zelfde geheugen-Arc gedeeld) — minder CPU-pieken bij het loslaten van akkoorden en minder RAM
- 🔁 **Orgelwissel-race gedicht**: een achtergrond-load die pas ná een orgelwissel afrondde kon zijn sample onder een hergebruikte pijp-key van het níeuwe orgel registreren (verkeerde pijp/samplerate). Laadresultaten dragen nu een generatie-stempel en verlopen resultaten worden genegeerd
- 🎛️ **Callback-integratie begrensd**: klaargekomen achtergrond-loads worden met maximaal 16 per audio-callback verwerkt zodat een burst de realtime-deadline niet kan overschrijden
- 🎧 **Extensible-float-WAVs klinken niet meer als ruis** in het preload-pad (WAVE_FORMAT_EXTENSIBLE-subformaat wordt nu gelezen); 8-bit-normalisatie gelijkgetrokken tussen de parsers
- 🚿 **Logspam bij GO-sets weg**: de twee waarschuwingen per bestand (duizenden per orgel-load) zijn gedempt; dit kon bovendien beheerd gestarte processen laten bevriezen zodra de stderr-buffer volliep
- ⌨️ **Tutti-bursts bevriezen de app niet meer**: het commandokanaal naar de audio-thread is vergroot (512→4096) — een vol akkoord op veel registers met koppels stuurde meer commando's dan de wachtrij aankon, waardoor UI/MIDI-aanroepen seconden blokkeerden
- 🧪 Test-API overleeft nu een panic in een handler (alleen relevant voor geautomatiseerd testen)

## [0.6.2] - 2026-07-08

### Opgelost — kraken/haperen bij veel registers (vooral GrandOrgue-sets)

Wortel-oorzaak: de audio-mengloop deed **per klinkende pijp, per sample**
opnieuw werk dat in werkelijkheid constant is — vier HashMap-opzoekingen plus
`powf`/`sin`/`cos`. Bij een volle registratie (honderden gelijktijdige voices
op 44,1 kHz) liep dat op tot tientallen miljoenen bewerkingen per seconde op
één kern, waardoor de render-thread de buffer-deadline miste → buffer-underruns
→ hoorbaar gekraak. Gemeten op Friesach: 13 registers + een akkoord zaten al op
~50–70% van één kern; ná de fix voegt hetzelfde spel vrijwel niets meer toe
boven de rusttoestand.

- ⚡ **Per-voice constanten worden nu één keer per audio-callback voorgerekend** i.p.v. per sample: divisie-index, gestapelde voicing (gebruiker + ODF-intonatie) en de tremulant-keuze worden gecachet op de voice. Wind- en tremulant-modulatie worden per divisie (max 32) berekend i.p.v. per voice, en de constant-power panning-cos/sin per divisie één keer per callback. Het hete pad bevat zo geen HashMap-opzoekingen of transcendente functies meer per voice per sample. Live bijregelen van voicing/pan/temperament blijft werken (wordt de eerstvolgende callback opgepikt)
- 🧯 **Harde polyfonie-grens (512 voices) met voice-stealing**: het echte audiopad had géén limiet — de voice-lijst kon onbeperkt groeien (blijvende noten, koppels, tutti + release-staarten) en zo de render-thread over de deadline duwen. Zit de lijst vol, dan wordt de zachtste (bij voorkeur al uitklinkende) voice gestolen
- 🔇 **Logging uit de audio-callback gehaald**: de per-noot `debug!`-regels (die strings opmaakten en op een met de MIDI-thread gedeelde bestands-mutex konden blokkeren) zijn weg uit het NoteOn/NoteOff/one-shot-pad
- 🌫️ **Galm wordt overgeslagen bij mix 0**: de FDN/convolutie-galm draaide voorheen elke sample door, óók als de galm helemaal dicht stond — nu wordt dat rekenwerk overgeslagen
- 🧵 **Geen heap-allocaties meer per audio-callback**: de zes config-momentopnamen (divisie-gains/pans/routing/C-Cis/wind-groepen/zwelkast) werden elke callback gekloond. Ze worden nu als read-guard vastgehouden (die config wordt alleen in de commandofase van diezelfde callback geschreven, dus zonder concurrentie). Op ASIO4ALL, dat callbacks zeer frequent aanroept, scheelt dit merkbaar allocatie-jitter op de audio-thread — een directe bron van gekraak
- 🩺 **Audio-watchdog rijdt kortstondige ASIO-glitches uit**: een enkele stream-fout onder piekbelasting (typisch een ASIO4ALL-underrun waarbij de callbacks gewoon doorlopen) brak voorheen meteen de stream af en kon escaleren naar een volledige herstart + orgel-herlaad — een korte hapering werd zo een langere stilte. De watchdog wacht nu ~0,8 s en herbouwt alleen als de callbacks ook echt stoppen; bij een tijdelijke glitch gebeurt er niets

### Diagnose-notitie: de CPU-basislast komt van ASIO4ALL, niet van de app

Gemeten op de console-PC: met een orgel geladen maar zónder te spelen gebruikt de app op **WASAPI ~0–3% van één kern**, maar op **ASIO4ALL ~50% van één kern** — een vaste last die ASIO4ALL zelf veroorzaakt (kernel-streaming-wrapper, zwaarder bij een kleine buffer). Op een 16-kern-machine is dat ~3% van het totaal en de audio-thread zit ruim onder zijn deadline, dus dit is geen crackle-oorzaak op zich, maar het eet wél de marge op. Wie lagere CPU/meer marge wil: een grotere ASIO4ALL-buffer, een echte ASIO-driver van de geluidskaart, of WASAPI (hogere latency).

### Nog niet in deze versie (bekende resterende punten)

- De release-samples van GrandOrgue-sets worden bij de eerste keer spelen per toets gedecodeerd via een trage handmatige WAV-parser (de snelle decoders falen op deze bestanden); dat geeft korte CPU-pieken bij het loslaten tot de cache warm is (op een multi-core-machine door reservekernen opgevangen). Nog te doen: eenmalig decoderen/ontdubbelen of releases vooraf inladen.

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
