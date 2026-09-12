# Audio-uitvoer: hoe andere orgelsoftware het doet, en wat JM-Orgue daarvan overneemt

Onderzoek 2026-09-12 (bronnen: GrandOrgue-broncode en -help, Hauptwerk User Guide 5.0.1 en forum, Sweelinq-helpcenter, Organteq-handleiding, jOrgan/Aeolus-documentatie, Reaper/Ableton/Cubase-handleidingen, Microsoft Core Audio-documentatie, cpal 0.15.3-bron). Aanleiding: "bij geluidskaartselectie komen maar 2 kanalen beschikbaar per klavier".

## 1. Wat er in JM-Orgue mis was (en wat 0.7.37 verandert)

| Onderdeel | Vóór 0.7.37 | 0.7.37 |
|---|---|---|
| Kanaalaantal in de UI (chips per klavier, EQ-kanaal) | Eén keer opgevraagd bij het aanmaken van het scherm, nooit ververst. Bij een ASIO-voorkeur start de app op WASAPI (2 kanalen) en wisselt ~10 s later naar ASIO — de chips bleven op 2. | Volgt de lopende stream via de bestaande 100-ms-statuspoll (`StatusDto.channels`), in hoofdvenster én extra schermen. |
| `query_audio_channel_count` / `query_supported_sample_rates` | Enumereerden een **verse** cpal-host. Onder ASIO laadt die de driver opnieuw en exit hem bij het opruimen — de spelende driver werd geraakt bij elk extra scherm en bij "Ondersteunde sample rates". Bij een fout bleef de UI op 2. | Lezen de lopende stream; onder ASIO geen enumeratie meer. |
| Uitgangspaar (0.7.36) vs. chips | Twee verschillende bronnen: de paarkeuze las wél het stream-aantal, de chips niet — ze konden elkaar tegenspreken. | Eén bron. |
| Kanaalnamen | 7.1-namen (Center, LFE, Achter L …) — onzin op een 8-uits interface. | Links/Rechts bij stereo, anders "Uitgang N". |
| Klavier op een uitgang die het huidige apparaat niet heeft (bv. 3–4 na een wissel naar een stereo-hoofdtelefoon) | Stil teruggevallen op 1–2, onzichtbaar. | Gele hint onder de chips ("uitgang bestaat niet op dit apparaat → speelt op 1–2"). |
| Handmatige kanaalwijziging terwijl een profiel actief is | Profielkopie bleef oud; de volgende wissel zette de oude kanalen terug. | Profielkopie wordt bijgewerkt (ook vanuit extra schermen). |
| Profielwissel naar hetzelfde apparaat (hoofdtelefoon = ander paar op dezelfde interface) | Volledige stream-herbouw + orgel-herlaad. | Alleen de kanaal-override; direct en klikvrij. |
| Audio-uitvoer-blok | Verspreide, deels Engelse/Nederlandse teksten. | Regel "N uitgangen beschikbaar op de nu geopende audio-uitgang (host)" + uitleg per host; alle teksten via i18n (NL/EN/FR/DE). |

**Wat de stream zelf betreft is niets veranderd — en dat is juist.** De engine opent de stream met het aantal kanalen dat cpal als standaard meldt:

* **ASIO:** `ASIOGetChannels().outs` = **alle** uitgangen van de driver (8 op een GIGAPORT eX). De engine routeerde al tot 64 kanalen.
* **WASAPI (shared mode):** het mix-formaat van het endpoint = de luidsprekerconfiguratie in Windows (stereo = 2, 7.1 = 8). cpal 0.15 kent geen exclusive mode en accepteert in shared mode geen ander aantal. Een interface die zich in Windows als losse stereo-apparaten aanmeldt ("GIGAPORT 1/2", "3/4", …) kán via WASAPI dus nooit 8 kanalen in één stream geven — daarvoor is ASIO de juiste weg (of één 7.1-endpoint).

De UI legt dit nu uit in het Audio-uitvoer-blok, met bij WASAPI de suggestie om ASIO te kiezen.

## 2. Hoe de anderen het doen

### GrandOrgue
* **Model:** twee lagen. Registers/windladen/pijpen kiezen een *audio group* (per orgel, in de preset); elke audio group is een stereo mengbuffer. In de globale instellingen staat een boom *Audio Output → Device → Channel 1..n → "groep – left/right: x dB"*: een volledige mengmatrix met een gain per (kanaal, groep, kant).
* **Apparaten:** één device in de praktijk; meerdere zijn technisch toevoegbaar maar GO waarschuwt expliciet ("Using more than one audio interface is currently not supported" — geen kloksynchronisatie).
* **Kanaaltelling:** het device meldt zijn maximum, maar de gebruiker bouwt het aantal zelf op ("Add" per kanaal); de stream opent met exact dat aantal. Minder dan het maximum kan, meer geeft een open-fout.
* **Hoofdtelefoon:** geen concept, geen profielen, geen snelle wissel. Ander apparaat kiezen = audio sluiten en heropenen; al het openen van de apparatenlijst sluit de lopende audio (ASIO kan niet twee keer open). Pseudo-profielen alleen via `GrandOrgue -i <naam>` (aparte config) — dus twee snelkoppelingen.
* **Latency:** sample rate en samples-per-buffer globaal; per device een "desired latency"; werkelijke latency in "Sound Output State"; "Revert to Default" naar stereo.

### Hauptwerk
* **Model:** één driver (ASIO aanbevolen), álle kanalen van dat apparaat; rank → 4 perspectives → mixer-busgroepen → primary buses → master mix buses; elke bus krijgt een uitgangskanaal(paar). Kanalen hernoembaar/hermapbaar.
* **Meerdere apparaten:** "It isn't possible to use several different audio interfaces or drivers simultaneously."
* **Hoofdtelefoon:** standaard een vaste *Mstr mix bus 2 "Stereo mix 2 (Headphones)"*: een mixdown die je aan een eigen kanaalpaar van dezelfde interface koppelt en die altijd meeloopt. Een snelle speakers↔hoofdtelefoon-wissel bestaat niet (Martin Dyde: mixerpreset niet per MIDI-piston wisselbaar; enhancement request voor mute/unmute van busgroepen). Gebruikers lossen het op met de routingsoftware van de interface of een hoofdtelefoonversterker op een vast paar.

### Sweelinq / Organteq
* Sweelinq: één apparaat (ASIO op Windows), daarna per "audio output" kanalen kiezen met een **testsignaal** per uitgang; EQ-presets voor hoofdtelefoons.
* Organteq: apparaattype (ASIO / "Windows low latency" = WASAPI), 2 of 8 uitgangskanalen (8 = elke divisie een eigen stereo-uitgang) plus een routingmatrix.

### DAW-conventies (Reaper, Ableton, Cubase)
* Reaper: "Output Range" exposeert alle kanalen; de master gaat naar meerdere hardware-outputs die elk apart gemute kunnen worden — wissel = mute-toggle.
* Ableton: Main Out- en Cue Out-kiezer op de master (min. 4 uitgangen).
* Cubase Control Room: tot 4 monitor-sets + phones-kanaal, exclusieve A/B-schakeling; "Switching between monitors that share device ports is seamless".

### Platformfeiten
* ASIO: één driver per proces; de host kiest de kanaalsubset. Bluetooth-hoofdtelefoons hebben vrijwel nooit ASIO → WASAPI, hogere latency.
* WASAPI-clients volgen het Windows-standaardapparaat niet vanzelf; daarvoor is `IMMNotificationClient::OnDefaultDeviceChanged` nodig (cpal issue #740).

## 3. Beste praktijk en de keuze voor JM-Orgue

1. **Eén apparaat tegelijk open houden** (Hauptwerk, Sweelinq, Organteq, DAW-ASIO). GrandOrgue's multi-device is "early state" en drift-gevoelig. JM-Orgue blijft bij één stream.
2. **Alle uitgangskanalen van dat apparaat aanbieden** (Reaper Output Range, Hauptwerk device-channels). JM-Orgue: gedaan — ASIO alle driver-uitgangen, WASAPI het endpoint-aantal, UI volgt de stream.
3. **Uitgangsdoelen als benoemde kanaalparen** (Cubase monitor-sets, Reaper hardware outputs). JM-Orgue heeft dat al in de vorm van de profielen Speakers/Hoofdtelefoon (host + apparaat + buffer + kanalen per klavier) én de per-klavier-kanaalkeuze; 0.7.36 voegde de uitgangspaar-keuze toe.
4. **De wissel in twee gevallen:**
   * hoofdtelefoon = ander paar op **hetzelfde** apparaat → alleen de routing wisselen, zonder de stream te heropenen (Cubase "seamless"). **0.7.37: gedaan.**
   * hoofdtelefoon = **ander** apparaat (USB-DAC, Bluetooth) → profielwissel met sluiten/heropenen; JM-Orgue's uitgestelde wissel, registratie-herstel en ASIO-zelfherstart dekken dit al.
   Beide zitten achter dezelfde Hoofdtelefoon-knop (ook MIDI-leerbaar) — precies wat Hauptwerk-gebruikers vragen maar niet hebben.
5. **Nog niet gedaan, mogelijk later:**
   * testsignaal per uitgang (Sweelinq) om fysieke uitgangen te identificeren;
   * kanaalnamen/aliassen (Hauptwerk, Reaper);
   * Hauptwerk-stijl "hoofdtelefoon altijd gevoed" als optie (alleen luidsprekers muten);
   * `IMMNotificationClient` voor aan-/afkoppelen van USB/Bluetooth-apparaten;
   * dB-gain per (klavier, uitgang) à la GrandOrgue's mengmatrix — bewust niet: te veel knoppen voor een organist; de per-klavier-pan en de master-limiter volstaan.

## 4. Verificatie 0.7.37
* Test-API: `GET /status` geeft `channels`, `audio_host`, `audio_device`, `buffer_frames`; `GET/POST /division_channels` toont/zet de uitgangen per klavier.
* Op de ontwikkel-pc (Realtek, WASAPI 2 ch / ASIO4ALL 2 ch): chips 1–2 en "2 uitgangen beschikbaar (ASIO)" na de wissel, zonder herstart; log meldt "kanalen: 2" bij het openen.
* Op het testorgel (GIGAPORT eX via ASIO) hoort na de uitgestelde ASIO-wissel "kanalen: 8" in het log te staan en verschijnen 8 chips per klavier — ook als de instellingen al openstonden.
