# Onderzoek: geheugengebruik bij grote samplesets — 24 september 2026

Aanleiding: een grote surround-set vulde bij het laden ruim 20 GB werkgeheugen,
en met de tremulant-opnamen erbij (0.7.55) kwam daar nog ruim een vijfde
overheen. Op een machine met 32 GB wordt dat krap. Vraag: valt er structureel
minder geheugen te gebruiken, en zo ja, waar zit de meeste winst?

Dit is een onderzoeksnotitie. Er is niets gebouwd; de opties hieronder zijn
afwegingen, geen beslissingen.

Alle paden zijn relatief ten opzichte van
`C:\Bronbestanden\JM-Orgue\VirtualPipeOrgan\`.

---

## 1. Wat er na het laden werkelijk in het geheugen staat

Bij het laden van een orgel wordt **niet** elke opname volledig ingelezen. Dat
is een bewuste keuze van eerder (`src-tauri/src/commands.rs:1867` e.v.): grote
sets liepen daarop vast. In plaats daarvan komt er per **unieke (bestandspad,
segment)-combinatie** één preload-buffer in het geheugen:

| eigenschap | waarde | vindplaats |
|---|---|---|
| lengte | `PRELOAD_SAMPLES` = 96.000 frames (2 s bij 48 kHz) | `src-tauri/src/audio.rs:23` |
| opslag | `Vec<f32>`, planair (links en rechts apart) | `vpo-sampler/src/loader.rs:745` |
| afgekapt op | `preload_samples.min(total_frames - seg_start)` | `vpo-sampler/src/loader.rs:942` |
| gebouwd bij | aanzet én elk release-segment | `src-tauri/src/commands.rs:2215` |

De volledige opname wordt pas van schijf gestreamd zodra een noot voor het
eerst klinkt, en die volledig geladen samples staan al onder een plafond van
1,5 GB met FIFO-opruiming (`src-tauri/src/audio.rs:3642`). Lang spelen laat het
geheugen dus niet onbegrensd oplopen; dat is eerder al opgelost.

**Het resident geheugen is daarmee vrijwel volledig de som van de preloads.**

### Gemeten model

Twee gecontroleerde laadbeurten, geheugen van het proces gemeten vóór en na:

| set | unieke ingangen | werkgeheugen |
|---|---|---|
| tien registers uit een grote surround-set | 896 | 730 MB |
| een middelgrote Franse set | 3.812 | 3.042 MB |

Lineair daardoorheen: **0,793 MB per ingang, met een vaste voet van ongeveer
20 MB.** Theoretisch is een stereo-preload 96.000 × 4 bytes × 2 kanalen =
0,768 MB. De meting zit daar 3 % boven — dat is de allocator-overhead plus de
uitlijningstabel van 1 KB per release. Het model klopt dus, en je kunt het
geheugen van tevoren uitrekenen:

| ingangen | verwacht werkgeheugen |
|---|---|
| 3.812 | 3,0 GB |
| 16.564 | 13,2 GB |
| 20.168 | 16,0 GB |
| 26.000 | 20,6 GB |

---

## 2. Waar het geheugen in gaat

De verdeling is verrassend eenzijdig. Bij de grote surround-set zijn er 4.141
pijpen en 16.564 ingangen: **per pijp één aanzet en drie release-staarten.**

| onderdeel | aandeel |
|---|---|
| release-staarten | 75 % |
| aanzetten | 25 % |
| tremulant-opnamen (0.7.55) erbovenop | +18 % (3.604 van 20.168) |

Driekwart van het werkgeheugen is dus nagalm die pas bij het loslaten van een
toets klinkt. Elke zoektocht naar besparing die de aanzetten aanpakt en de
releases laat staan, pakt per definitie hoogstens een kwart aan.

---

## 3. Wat er al is

Twee knoppen bestaan al en werken:

- **"Stereo-samples afspelen" uit** mengt bij het láden naar mono
  (`vpo-sampler/src/loader.rs:100`, `split_planar`). Dat halveert het
  geheugen onmiddellijk. Het kost het ruimtebeeld van stereo-opnamen.
- **Microfoonposities**: alleen de ingeschakelde positie laadt. De instelling
  "Alle posities in het geheugen houden" doet precies wat hij zegt en
  vermenigvuldigt het geheugen met het aantal posities.

En het plafond van 1,5 GB op volledig geladen samples voorkomt dat uren spelen
het geheugen laat groeien.

---

## 4. Twee ideeën die op metingen zijn afgevallen

Beide klonken goed en leveren niets op. Ze staan hier zodat niemand ze nog een
keer bedenkt.

**Preload afkappen op het lusbegin.** Een aanhoudende pijp speelt de aanzet en
gaat daarna heen en weer tussen `loop_start` en `loop_end`. Alles ná `loop_end`
zou dan nooit gelezen worden. Gemeten over 29.597 WAV-bestanden in `testen/`:
dat scheelt **1 %**. De reden is dat de lus meestal pas ná de preload begint —
`loop_end` ligt mediaan op 4,19 s, het 90e percentiel op 7,32 s.

**Kortere preload voor hoge pijpen.** Hoge pijpen zwingen sneller in, dus zou
een kortere aanloop volstaan. Het lusbegin per toetsgroep:

| toetsgroep | mediaan lusbegin | 90e percentiel |
|---|---|---|
| groot octaaf en lager | 2,21 s | 3,56 s |
| klein octaaf | 2,15 s | 3,68 s |
| c' t/m b' | 2,17 s | 4,07 s |
| c'' t/m b'' | 2,07 s | 4,03 s |
| c''' en hoger | 1,68 s | 3,68 s |

Het verschil tussen de laagste en de hoogste groep is een halve seconde. Te
weinig om er een mechanisme voor te bouwen.

**Wat deze meting wél leert:** slechts **45 %** van de opnamen haalt zijn
lusbegin binnen 2 seconden. De huidige preload van 2 s is dus al krap bemeten
als brug naar de achtergrondlading. De aanzet-preload zou ik daarom niet
inkorten.

---

## 5. Wat wel kan, op volgorde van opbrengst

### 5.1 De preload in 16 bits opslaan in plaats van 32

**Opbrengst: 50 %.** Ongeveer 8 GB op een grote set. Verreweg de grootste knop.

De bronbestanden zijn overwegend 24-bits (gemeten over de lokale sets: 24-bits
48 kHz overheerst, met daarnaast 16-bits 44,1 kHz en 32-bits 48 kHz). Opslaan
als `i16` betekent dus kwantisatieruis toevoegen aan een 24-bits bron. Met een
schaalfactor per buffer (het maximum van de buffer) is dat in de praktijk bijna
niet hoorbaar, en het is precies wat andere virtuele orgels als
"geheugencompressie" aanbieden.

**Risico en werk: groot.** Het raakt het leespad van elke stem, en dat pad is
in 0.7.49 juist over meerdere rekenkernen verdeeld en zorgvuldig geoptimaliseerd.
Een conversie per sample in de mengloop kost rekentijd die daar niet zomaar is.

### 5.2 Een kortere preload alleen voor de release-staarten

**Opbrengst: ongeveer een derde van het totaal** als de release-preload van 2 s
naar 1 s gaat (75 % van het geheugen × de helft).

Het risico is milder dan bij de aanzet. `clamp_release_to_buffer`
(`src-tauri/src/audio.rs:1043`) richt de uitfade al op wat er werkelijk in het
geheugen staat, juist om een tik op de bufferrand te voorkomen. De gevolgen van
een kortere release-preload zijn dus: de **eerste** keer dat een bepaalde pijp
wordt losgelaten is de staart korter dan bedoeld, daarna niet meer. Geen tik,
geen gat.

Het werk is bovendien klein: `load_wav_preload_segment` neemt de lengte al als
parameter, dus aanzet en release kunnen zonder structurele ingreep elk hun
eigen waarde krijgen (`src-tauri/src/commands.rs:2215`).

**Te onderzoeken vóór dit gebouwd wordt:** hoe lang een release werkelijk
doorklinkt voordat de achtergrondlading er is, op een trage schijf. Dat is een
meting, geen schatting.

### 5.3 Tremulant-opnamen pas laden bij het inschakelen

**Opbrengst: 18 %** bij sets met veel tremmed registers.

Sinds 0.7.55 laden ze altijd mee (`src-tauri/src/commands.rs:2162`). Een
divisie waarvan de tremulant nooit aan gaat betaalt daar nu wel voor. Laden bij
het eerste inschakelen kost een laadmoment; op de achtergrond laden zodra het
orgel klaar is, kost geen wachttijd maar ook geen geheugen zolang je de
tremulant niet gebruikt.

### 5.4 Het plafond van 1,5 GB instelbaar maken

**Opbrengst: geen** op de basis, maar het geeft sturing. Een machine met weinig
geheugen kan omlaag, een grote machine kan meer gespeelde samples vasthouden en
laadt daardoor minder vaak opnieuw van schijf.

Het is nu een constante in de audio-thread (`src-tauri/src/audio.rs:3642`).

---

## 6. Samengevat

- Het geheugen is voorspelbaar: **0,79 MB per unieke (bestand, segment)**.
- **Driekwart daarvan zijn release-staarten.** Daar ligt de winst.
- De aanzet-preload van 2 s is al krap; niet inkorten.
- De grootste enkele knop is 16-bits opslag, maar die raakt de mengloop.
- De veiligste noemenswaardige winst is een kortere preload voor releases.
