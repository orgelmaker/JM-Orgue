# Onderzoek: geheugengebruik bij grote samplesets — 24 september 2026

Aanleiding: een grote surround-set vulde bij het laden ruim 20 GB werkgeheugen,
en met de tremulant-opnamen erbij (0.7.55) kwam daar nog ruim een vijfde
overheen. Op een machine met 32 GB wordt dat krap. Vraag: valt er structureel
minder geheugen te gebruiken, en zo ja, waar zit de meeste winst?

Dit is een onderzoeksnotitie. Er is niets gebouwd; de opties hieronder zijn
afwegingen, geen beslissingen. De bevindingen zijn in twee ronden tot stand
gekomen: eerst een eigen doorloop met metingen, daarna vier onafhankelijke
lezingen van de code waarvan elke bevinding langs een weerleggingsronde is
gegaan. Waar die ronde mij corrigeerde, staat dat er met zoveel woorden bij.

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
| opslag | `Vec<f32>`, planair (links en rechts elk een eigen `Vec`) | `vpo-sampler/src/loader.rs:745` |
| afgekapt op | `preload_samples.min(total_frames - seg_start)` | `vpo-sampler/src/loader.rs:942` |
| gebouwd bij | aanzet én elk release-segment | `src-tauri/src/commands.rs:2215` |
| samplerate | die van het **bronbestand** | `src-tauri/src/audio.rs:968` e.v. |

De volledige opname wordt pas van schijf gestreamd zodra een noot voor het
eerst klinkt. Die volledig geladen samples staan wél onder een plafond van
1,5 GB met FIFO-opruiming (`src-tauri/src/audio.rs:3642`), en worden naar de
**uitgangs**-samplerate geresampled.

**De preload-map kent geen enkel plafond.** Er is geen kap, geen opruiming per
ingang en geen teller; de map wordt alleen in zijn geheel vervangen bij een
orgelwissel (`src-tauri/src/audio.rs:4412`). Wie aan de streamingkant draait —
de kap verlagen, agressiever opruimen — raakt dus hooguit een zesde van het
probleem.

> **Niemand meet deze kant.** `PreloadBuffer::bytes()` bestaat
> (`vpo-sampler/src/loader.rs:786`) maar wordt in de hele applicatie nul keer
> aangeroepen; álle `.bytes()`-aanroepen gaan over de gekapte 1,5 GB. Het model
> hieronder is dus afgeleid uit procesgeheugen, niet afgelezen. Zie §7.

### Gemeten model

Twee gecontroleerde laadbeurten, procesgeheugen vóór en na:

| set | unieke ingangen | werkgeheugen |
|---|---|---|
| tien registers uit een grote surround-set | 896 | 730 MB |
| een middelgrote Franse set | 3.812 | 3.042 MB |

Lineair daardoorheen: **0,793 MB per ingang, met een vaste voet van ongeveer
20 MB.** Theoretisch is een stereo-preload 96.000 × 4 bytes × 2 kanalen =
0,768 MB. De meting zit daar 3 % boven. Een onafhankelijke meting van de
WAV-koppen van een hele set (12.148 bestanden, vrijwel alle 24-bits 48 kHz
stereo, gemiddeld 4,94 s, en maar 3 % korter dan de preload) komt op 0,764 MB
per ingang. De drie getallen sluiten op elkaar aan:

| ingangen | verwacht werkgeheugen |
|---|---|
| 3.812 | 3,0 GB |
| 16.564 | 13,2 GB |
| 20.168 | 16,0 GB |
| 26.000 | 20,6 GB |

Twee dingen die dit model **niet** doet. Het geldt alleen met stereo laden aan;
met mono-downmix is alles de helft. En het beweegt niet mee met de
uitgangs-samplerate, terwijl de gestreamde samples dat wél doen: op een
96 kHz-uitgang passen er half zoveel pijpen onder dezelfde 1,5 GB, terwijl de
preload onveranderd blijft.

---

## 2. Waar het geheugen in gaat

De verdeling is eenzijdig. Per pijp staat er één aanzet en staan er meerdere
release-staarten. In de gemeten set: 9.625 van de 12.017 ingangen zijn
releases.

| onderdeel | aandeel |
|---|---|
| release-staarten | 75 – 80 % |
| aanzetten | 20 – 25 % |
| tremulant-opnamen (0.7.55) erbovenop | +18 % |

Vier vijfde van het werkgeheugen is dus nagalm die pas bij het loslaten van een
toets klinkt. Elke besparing die alleen de aanzetten aanpakt, raakt per
definitie hoogstens een vijfde.

---

## 3. De grootste piek zit in de orgelwissel, niet in het orgel

Dit was de verrassing van het onderzoek, en het is de goedkoopste winst.

Bij het laden van een nieuw orgel worden eerst alle nieuwe preload-buffers
gebouwd (`src-tauri/src/commands.rs:2207`) en pas daarna in één keer aan de
audio-thread overhandigd (`commands.rs:2341`, `RegisterPreloadBuffers`). Het
oude orgel blijft tot dat moment volledig resident. Er gaat geen `ClearSamples`
aan vooraf.

**De piek bij een wissel is dus het oude orgel plus het nieuwe.** Gemeten
tijdens dit onderzoek: 7.565 MB vlak vóór het laden van een set die daarna
3.042 MB bleek te vragen. Bij het wisselen tussen twee grote sets is dat het
verschil tussen passen en niet passen.

Het oude orgel eerst loslaten kost een regel. De prijs is dat een mislukte
laadpoging je zonder orgel achterlaat in plaats van met het vorige, en dat is
precies de reden dat het nu níet zo staat. Dat is een afweging waard, geen
vanzelfsprekendheid.

Wel goed nieuws uit dezelfde meting: ná de wissel zakte het procesgeheugen naar
de 3,0 GB die het model voor de nieuwe set voorspelt. Het vrijgegeven geheugen
komt dus werkelijk terug bij het besturingssysteem; dat was niet vanzelfsprekend
(zie §7, punt 3).

---

## 4. Wat er al is

- **"Stereo-samples afspelen" uit** mengt bij het láden naar mono
  (`vpo-sampler/src/loader.rs:100`). Dat halveert het geheugen onmiddellijk en
  kost het ruimtebeeld van stereo-opnamen. Dit is vandaag de enige knop met
  werkelijke omvang.
- **Microfoonposities**: alleen de ingeschakelde positie laadt. "Alle posities
  in het geheugen houden" vermenigvuldigt het verbruik met het aantal posities.
- Het plafond van 1,5 GB voorkomt dat uren spelen het geheugen laat groeien.

---

## 5. Ideeën die op metingen zijn afgevallen

Ze staan hier zodat niemand ze nog een keer bedenkt.

**Preload afkappen op het lusbegin.** Alles ná `loop_end` zou nooit gelezen
worden. Gemeten over 29.597 WAV-bestanden: dat scheelt **1 %**. De lus begint
meestal pas ná de preload — `loop_end` ligt mediaan op 4,19 s.

**Kortere preload voor hoge pijpen.** Het lusbegin per toetsgroep:

| toetsgroep | mediaan lusbegin | 90e percentiel |
|---|---|---|
| groot octaaf en lager | 2,21 s | 3,56 s |
| klein octaaf | 2,15 s | 3,68 s |
| c' t/m b' | 2,17 s | 4,07 s |
| c'' t/m b'' | 2,07 s | 4,03 s |
| c''' en hoger | 1,68 s | 3,68 s |

Een halve seconde verschil tussen de uitersten. Te weinig voor een mechanisme.

**Per register laden zodra het register getrokken wordt.** Klinkt aantrekkelijk,
maar het verplaatst de laadtijd naar het moment dat je speelt, en een setzer of
generaal crescendo trekt tientallen registers tegelijk.

**Van de releases alleen de standaardvariant resident houden.** Dan verliest de
sampler juist de keuze op toetsduur die in 0.7.53 is ingebouwd, en die keuze
valt bij de kórte releases — precies de variant die je zou weglaten.

**Wat deze metingen wél leren:** slechts **45 %** van de opnamen haalt zijn
lusbegin binnen 2 seconden. De preload van 2 s is dus al krap bemeten als brug
naar de achtergrondlading.

---

## 6. Wat wel kan, op volgorde van opbrengst

### 6.1 De preload in 16 bits opslaan in plaats van 32

**Opbrengst: ongeveer 45 %** van de residente basis. Bij 20.000 ingangen gaat
het van 16 GB naar ruim 8 GB. Verreweg de grootste knop.

De bronnen zijn overwegend 24-bits, dus `i16` voegt kwantisatieruis toe. Met een
schaalfactor per buffer is dat in de praktijk onder de hoorbaarheidsgrens, en
het is precies wat andere virtuele orgels als "geheugencompressie" aanbieden.
Gepakte 24 bits geeft 25 % en is volledig verliesvrij.

Het is geen 50 % omdat niet alle bytes sampledata zijn: er zit een vaste voet
per buffer en een uitlijningstabel per release in.

**Risico en werk: groot.** Het raakt het leespad van elke stem, en dat pad is in
0.7.49 over meerdere rekenkernen verdeeld en zorgvuldig geoptimaliseerd.

### 6.2 Het oude orgel loslaten vóór het nieuwe laadt

**Opbrengst: de volledige omvang van het vorige orgel op het wisselmoment.**
Geen besparing in ruststand, maar wel het verschil tussen wel en niet kunnen
wisselen tussen twee grote sets. Zie §3. Klein werk, echte afweging.

### 6.3 Een kortere preload voor de release-staarten

**Opbrengst: fors** — daar zit vier vijfde van het geheugen. Maar:

> **Correctie op mijn eerste ronde.** Ik schreef dat het risico mild is, omdat
> `clamp_release_to_buffer` (`src-tauri/src/audio.rs:1043`) de uitfade al op de
> residente lengte richt. Dat klopt, maar het is niet het hele verhaal. Er
> bestaat een diagnoseteller `RELEASE_PRELOAD_EDGE_HITS`
> (`src-tauri/src/audio.rs:572`, geteld op `:1209`) die juist bijhoudt hoe vaak
> een release-staart het einde van zijn preload haalt terwijl de volledige WAV
> er nog niet is. Het commentaar erbij zegt waarom die teller bestaat:
> "laadlatentie bij grote akkoorden". Het gebeurt vandaag dus al bij tutti, en
> de preload halveren vermenigvuldigt precies dat. Afgekapte galmstaarten zijn
> het symptoom waar 0.7.33 juist voor is gemaakt.

Dit blijft de moeite waard, maar niet als losse ingreep. Het hoort samen met
6.1 (dan houd je de lengte en win je op de breedte) of met een meting die
aantoont dat de marge er is. Zie §7, punt 2.

### 6.4 Tremulant-opnamen pas laden bij het inschakelen

**Opbrengst: 18 % in ruststand, maar 0 % op de piek.** De droge buffers blijven
staan als de tremulant aangaat, dus de tremulant-buffers komen er bovenop. Of
een set op de machine past, verandert er niet door; hoeveel hij in rust kost
wel.

Er is bovendien geen mechanisme om preloads ná het laden bij te laden: de map
wordt alleen in zijn geheel vervangen. Dat zou er eerst moeten komen.

### 6.5 Het plafond van 1,5 GB instelbaar maken

**Opbrengst: enkele procenten.** Het raakt de 16 GB niet. Wel zinvol als
sturing, maar niet als antwoord op deze vraag.

---

## 7. Wat eerst gemeten moet worden

Drie metingen die goedkoop zijn en de beslissingen hierboven scherper maken dan
welke redenering ook.

1. **Meet de preload-kant écht.** Eén regel bij `commands.rs:2272` die
   `path_buffers.values().map(|b| b.bytes()).sum()` logt, plus een histogram van
   `attack_data.len()`, maakt het model uit §1 controleerbaar in plaats van
   afgeleid. Dit had de eerste stap moeten zijn, niet de laatste.

2. **Meet de marge, niet de rand.** `RELEASE_PRELOAD_EDGE_HITS` telt alleen
   staarten die eroverheen gaan; hij staat op nul zolang je het net haalt, en
   zegt dus niets over hoeveel speling er is. Wat je nodig hebt is `self.position`
   op het moment van `upgrade_to_full` (`src-tauri/src/audio.rs:892`): een
   histogram over 0,25 / 0,5 / 1 / 2 s zegt welk deel van de stemmen al vóór
   één seconde was overgestapt. Dát getal beslist of 6.3 veilig is.

3. **Controleer de allocator.** Elk kanaal is een eigen `Vec<f32>` van hoogstens
   384.000 bytes (`loader.rs:747` en `:749`), net onder de drempel waarboven de
   Windows-heap geheugen aan het besturingssysteem teruggeeft. De meting uit §3
   suggereert dat het wél terugkomt, maar dat was één waarneming bij een wissel
   van groot naar klein. Blijkt het anders te liggen, dan is de goedkoopste
   ingreep niet mínder bytes maar ándere: beide kanalen in één allocatie van
   768.000 bytes, die wel boven de drempel uitkomt.

---

## 8. Samengevat

- Het geheugen is voorspelbaar: **0,79 MB per unieke (bestand, segment)**, en de
  preload-kant kent geen enkel plafond.
- **Vier vijfde daarvan zijn release-staarten.** Daar ligt de winst, maar niet
  zonder de marge eerst te meten.
- De **orgelwissel verdubbelt de piek**; dat is de goedkoopste echte ingreep.
- De grootste enkele knop is 16-bits opslag, met ongeveer 45 %, maar die raakt
  de mengloop.
- De aanzet-preload van 2 s is al krap. Niet inkorten.
- Begin met meten (§7). De helft van wat hierboven staat is nu beredeneerd waar
  het gemeten had kunnen zijn.
