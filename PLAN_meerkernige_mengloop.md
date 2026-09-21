# Plan: de mengloop over meerdere kernen

Opgesteld 21 september 2026, op versie 0.7.48. Aanleiding: de meting bij de
polyfonie-verhoging liet zien dat het getal niet de grens is — de ene rekenkern
waarop de mengloop draait, is dat wel.

## 1. Wat de meting zegt

Gemeten op de ontwikkelmachine (Intel i9-10885H, 8 fysieke kernen / 16 logische,
WASAPI 48 kHz, 480 frames per callback = 10 ms budget) met Friesach en alle 44
registers getrokken, met het meetscript `testscripts/meet_mengloop.py`:

| Klinkende stemmen | Totaal | wind/tremulant | stemwerk | keten | aandeel stemwerk |
|---|---|---|---|---|---|
| 0 | 1,30 % | 0,95 % | 0,00 % | 0,36 % | — |
| 129 | 12,00 % | 0,97 % | 10,88 % | 0,44 % | 90,7 % |
| 260 | 23,45 % | 0,96 % | 22,18 % | 0,44 % | 94,6 % |
| 436 | 39,30 % | 0,96 % | 37,86 % | 0,44 % | 96,3 % |
| 576 | 56,45 % | 1,02 % | 54,98 % | 0,49 % | 97,4 % |
| 716 | 74,35 % | 1,07 % | 72,75 % | 0,55 % | 97,9 % |

Drie dingen vallen daaraan op.

**De kosten zijn recht evenredig met het aantal stemmen.** Ongeveer 0,10 % van
de buffertijd per stem, van 130 tot 716 stemmen zonder knik. Er is dus geen
cache-instorting meer (die was er vóór 0.7.36, toen de mengloop nog frame-major
was); wat er nu staat is een vaste prijs per stem per sample. Bij 80 %
belasting — de grens waarboven onderbrekingen dreigen — ligt het plafond op
deze machine rond de **770 stemmen**.

**Alles wat niet met stemmen te maken heeft, kost niets.** De vloer is 1,3 % van
de buffertijd: het windmodel en de tremulant-LFO's samen 0,95 %, en de hele
keten van zwelkast, routering, galm, EQ en limiter 0,36 %. In de zware situatie
is dus **97,9 % van de rekentijd per-stem-werk**.

**Meten vereist geduld.** De eerste meetronde gaf 0,17 % per stem en een grens
rond de 500 stemmen. Die liep terwijl de sampleset nog in de achtergrond
inlaadde — negentien seconden lang, waarin de belasting opgedreven werd en de
registratie aan het eind zelfs werd teruggezet. Het meetscript wacht nu tot de
registratie blijft staan én de belasting bij stilte laag is. Wie dit herhaalt:
doe dat ook.

Dat aandeel van 97,9 % is het belangrijkste getal van dit plan. Het bepaalt via
de wet van Amdahl wat meerkernig renderen maximaal kan opleveren: met zeven
werkers theoretisch een factor zes. In de praktijk wordt dat minder, omdat het
werk geheugengebonden is (elke stem leest uit zijn eigen samplebuffer van
megabytes). Juist daar helpen meer kernen wél goed: meer kernen betekent meer
gelijktijdig uitstaande geheugenverzoeken. Een realistische verwachting is
**een factor 3 tot 5**, oftewel van ~770 naar 2.300–3.800 stemmen.

## 2. Wat er precies parallel kan

De mengloop werkt sinds 0.7.36 in blokken van maximaal 256 frames
(`MIX_BLOCK`), met drie passen per blok:

1. **Pass 1 — wind en tremulant.** Per frame, per divisie (hoogstens 32).
   Goedkoop, en de modellen moeten in volgorde stappen. *Blijft serieel.*
2. **Pass 2 — de stemmen.** Elke stem leest zijn hele blok en telt op bij
   `blk_div_l[frame][divisie]` en `blk_div_r[...]`. *Dit is de 99 %.*
3. **Pass 3 — de keten per frame.** Zwelkast, routering naar kanalen, galm, EQ,
   limiter. Stateful en volgordegevoelig. *Blijft serieel.*

Pass 2 is bijna ideaal parallel materiaal:

- elke stem raakt alleen **zijn eigen** toestand (positie, envelope, rate);
- alles wat hij daarnaast leest is **binnen het blok onveranderlijk**: de
  wind/trem-tabellen uit pass 1, de pan-waarden, de divisie-instellingen;
- het enige gedeelde schrijfdoel zijn de twee opteltabellen.

Die opteltabellen zijn de hele puzzel. De oplossing is de klassieke: **elke
werker krijgt zijn eigen opteltabel en achteraf worden ze bij elkaar geteld.**
Met alleen de divisies die het orgel echt heeft (meestal drie of vier, hoogstens
acht) is zo'n tabel 256 × 8 × 4 bytes × 2 kanalen = 16 kB per werker. Met acht
werkers 128 kB, en de optelling erna kost enkele tienduizenden optellingen per
5 ms — verwaarloosbaar.

## 3. Ontwerp

### 3.1 Een eigen werkerspool, geen rayon

Rayon zit al in het project, maar is hier ongeschikt: de globale pool wordt
gedeeld met het laden van samples, doet work-stealing met onvoorspelbare
latentie, en draait op gewone prioriteit. Een audio-callback die op zo'n pool
wacht, wacht op de willekeur van het besturingssysteem.

Dus: een **eigen pool van N−1 werkerthreads**, aangemaakt zodra de audio-stream
gebouwd wordt en opgeruimd als hij sluit. Nooit een thread starten of geheugen
reserveren ín een callback.

- Op Windows krijgen de werkers dezelfde behandeling als de audiothread:
  `SetThreadPriority(..., THREAD_PRIORITY_TIME_CRITICAL)` via de al aanwezige
  `windows-sys`-afhankelijkheid, en bij voorkeur `AvSetMmThreadCharacteristics`
  met "Pro Audio" zodat de multimediascheduler ze kent.
- Elke werker zet bij het starten **zelf FTZ/DAZ** (denormals naar nul). De
  audiothread doet dat nu per callback; een werker die het niet doet, loopt bij
  uitklinkende staarten tegen dezelfde honderdvoudige vertraging aan.

### 3.2 De barrière

Per blok:

1. de audiothread verdeelt de stemmen met `chunks_mut` in N aaneengesloten
   stukken — disjunct, dus geen aliasing;
2. hij verhoogt een generatieteller en maakt de werkers wakker;
3. elke werker verwerkt zijn stuk in zijn eigen opteltabellen;
4. de audiothread doet ondertussen zijn eigen stuk;
5. hij wacht tot alle werkers klaar zijn en telt de tabellen op.

Voor het wachten: **eerst kort spinnen** (`core::hint::spin_loop()`, enkele
microseconden), daarna pas parkeren. Spinnen is bij een blok van 5 ms goedkoop
en houdt de latentie in de orde van honderden nanoseconden; parkeren als vangnet
voorkomt dat een verdrongen werker de hele machine laat draaien.

De `&mut [PlayingVoice]`-stukken gaan als rauwe pointer plus lengte naar de
werkers, in een omhulsel met een expliciete veiligheidsnotitie: de audiothread
geeft de stukken pas vrij ná de barrière, dus er bestaat nooit twee keer een
verwijzing naar dezelfde stem.

### 3.3 Wanneer NIET parallel

Meerkernig heeft alleen zin als er genoeg te verdelen valt. Twee drempels:

- **onder ~64 klinkende stemmen** blijft alles op één kern; de barrière kost dan
  meer dan hij oplevert;
- **bij heel kleine buffers** (ASIO op 32 frames = 0,67 ms) wordt de barrière
  relatief duur. Blokken van 256 frames vallen dan samen met de callback, dus er
  is één barrière per callback; dat is te doen, maar de drempel gaat daar
  omhoog.

Beide drempels worden gemeten, niet geraden (zie fase 1).

## 4. Instelbaar per pc, en vanzelf goed bij de eerste start

Dit is uitdrukkelijk geen vast getal in de code.

### 4.1 De instelling

Eén instelling in Algemene instellingen → Audio: **"Rekenkernen voor het
mengen"**, met de keuzes 1 (uit, precies het huidige gedrag) tot het aantal
fysieke kernen. Opgeslagen in de audio-voorkeuren, dus per pc en niet per orgel.
Ernaast de bestaande belastingsmeter, die nu pas echt iets te vertellen heeft.

### 4.2 Wat de app zelf kiest bij de eerste start

Bij de allereerste start (nog geen voorkeur opgeslagen) stelt de app zelf in:

1. **Fysieke kernen tellen,** niet logische. Hyperthreading levert voor
   geheugengebonden werk weinig en kan de latentie juist verslechteren. Op
   Windows via `GetLogicalProcessorInformationEx` uit `windows-sys`, dat al
   binnengehaald wordt.
2. **Eén kern vrijlaten** voor het besturingssysteem, de UI en het laden van
   samples — dat laatste draait bewust op lage prioriteit en moet wél
   vooruitkomen.
3. **Aftoppen op 8.** Daarboven wint het geheugen het van de kernen, en de
   barrière wordt langer dan hij waard is.

Dus: `kernen = min(8, max(1, fysieke_kernen - 1))`. Op de ontwikkelmachine
(8 fysiek) wordt dat 7; op een tweekerns laptop 1 — daar verandert er dus
niets, en dat is de bedoeling.

### 4.3 De meetknop

Naast de instelling komt **"Meet mijn pc"**: een routine die met een
synthetische belasting (stemmen die uit echte preload-buffers lezen, zodat het
geheugengedrag klopt) het aantal stemmen opzoekt waarbij de rendertijd op 80 %
van de buffertijd uitkomt, en dat doet voor 1 kern en voor de ingestelde
hoeveelheid. Hij rapporteert twee getallen — "met 1 kern ~500 stemmen, met 7
kernen ~2.100 stemmen" — en biedt aan de polyfonie-instelling daarop te zetten.

Die meting duurt enkele seconden en draait niet vanzelf bij de eerste start:
geluid produceren zonder dat iemand erom vroeg is onaangenaam. De automatische
keuze uit 4.2 is de standaard; de meetknop is er om hem te onderbouwen of bij
te stellen. Wie hem draait, krijgt het resultaat opgeslagen zodat de melding
"uw pc haalt ongeveer N stemmen" naast de polyfonie-instelling kan staan.

## 5. Wat er kan misgaan, en hoe we het merken

**De optelvolgorde verandert.** Drijvende-kommaoptelling is niet associatief:
zodra de stemmen over werkers verdeeld worden, is de som niet meer bit-identiek
aan die van één kern. Het verschil is in de orde van 1e-7 relatief — volstrekt
onhoorbaar — maar het is wel een breuk met de belofte uit 0.7.36 dat de
mengloop-verbouwing bit-exact was. Aanpak: de deeltabellen worden **in vaste
werkervolgorde** opgeteld, zodat het resultaat bij gelijk aantal kernen
reproduceerbaar is. In de changelog komt te staan dat het geluid bij een ander
aantal kernen in de laatste decimaal verschilt. Bestaande tests die op
bit-gelijkheid leunen (de mono/stereo-test) draaien met 1 kern.

**Denormals in een werker.** Zie 3.1: elke werker zet zelf FTZ/DAZ. Zonder dat
verschijnt het probleem pas bij lange uitklinkende staarten, dus laat en moeilijk
te herleiden. Er komt een test die dat vastlegt.

**Een verdrongen werker houdt de hele callback op.** De barrière is zo snel als
de traagste werker. Daarom de prioriteitsinstelling, het vrijlaten van een kern,
en de aftopping. Meetpunt: de bestaande teller van zware callbacks
(`render_overload_count`) moet bij gelijke belasting niet oplopen.

**Kleine buffers.** Zie 3.3; de drempels worden gemeten.

**Hangende stemmen bij een orgelwissel.** De pool leeft zolang de stream leeft.
Bij een audio-wissel wordt de stream afgebroken en opnieuw gebouwd; de pool moet
netjes mee sluiten, anders blijven er threads hangen die naar een verdwenen
stemmenlijst wijzen. Dit is het echte gevaar van dit plan en krijgt daarom een
eigen fase met een test die honderd keer achter elkaar wisselt.

## 6. Fasering

Elke fase is los af te ronden en te verifiëren.

**Fase 1 — meten waar de tijd heen gaat. ✅ AF (21 september 2026).** Tijdmeting
per pas, altijd aan (zes kloklezingen per blok, ~0,003 % van de buffertijd),
zichtbaar in de test-API als `mix_pass_load`. Het meetscript
`testscripts/meet_mengloop.py` zet de tabel uit paragraaf 1 neer. *Geen
gedragsverandering.*

**Fase 2 — de opteltabellen loskoppelen. ✅ AF (21 september 2026).** De stemmen
worden in `MENG_STUKKEN` aaneengesloten stukken verdeeld (instelbaar 1–8,
standaard 1), elk met een eigen opteltabel, waarna `reduceer_deeltabellen()` ze
in vaste stukvolgorde optelt. Stuk 0 schrijft rechtstreeks in de hoofdtabel, dus
bij één stuk gebeurt er letterlijk niets extra's en blijft het resultaat
bit-identiek. Onder `MENG_DREMPEL_STEMMEN` (64) blijft alles op één stuk.

Gemeten prijs van het verdelen, bij 436 stemmen, drie ronden afgewisseld:

| Stukken | Totale belasting | waarvan reductie |
|---|---|---|
| 1 | 39,00 % | 0,00 % |
| 2 | 40,00 % | 0,02 % |
| 4 | 39,10 % | 0,06 % |
| 8 | 39,60 % | 0,14 % |

De reductie kost dus **ruim een tiende procent van de buffertijd bij acht
stukken**, en het totaal blijft binnen de ruis van de machine (±1 %). Afgezet
tegen de 97,9 % die daarmee parallel te maken wordt, is dat verwaarloosbaar.

Zes unittests dekken de reductie: één stuk verandert niets, alle stukken en
divisies komen erbij, divisies boven het aantal van dit orgel blijven
onaangeroerd, de optelvolgorde ligt vast, de instelling wordt geklemd en de
drempel is zinnig.

*Nog open voor fase 3:* een offline render-pad om twee uitvoeringen
bit-voor-bit te vergelijken. De niveaumeters zijn daarvoor te grof gebleken —
ze schommelen per akkoord meer dan het verschil dat we zoeken. Zonder zo'n pad
steunt de gelijkheid op de unittests en op de redenering dat `chunks_mut` elke
stem precies één keer raakt; dat is genoeg voor fase 2, maar niet voor fase 3.

**Fase 3 — de pool en de barrière. ✅ AF (21 september 2026).** Een eigen
werkerspool (`src-tauri/src/mengpool.rs`) met vaste threads op
`THREAD_PRIORITY_TIME_CRITICAL` en eigen FTZ/DAZ. De audiothread schrijft per
blok een taak per werker, hoogt de generatieteller op (Release), doet zijn eigen
stuk en wacht spinnend tot iedereen klaar meldt (Acquire). Werkers wachten eerst
spinnend (2 ms) en parkeren daarna, zodat een stil orgel geen kernen laat
rondtollen en wakker maken tijdens het spelen geen systeemaanroep kost.

**Afwijking van het ontwerp:** de pool hangt aan de *instelling*, niet aan de
audiostream. Dat is eenvoudiger én veiliger — er bestaat altijd precies één pool
en een audiowissel hoeft geen threads op te ruimen. De callback pakt hem met
`try_read`; lukt dat niet (de pool wordt net herbouwd), dan mengt die callback
op één kern.

**Eén gevaar dat de meting blootlegde en dat gerepareerd is:** het opruimen van
een oude pool joint zeven threads, en dat mag nooit in een audio-callback
gebeuren. De callback houdt zijn kopie hooguit één callback vast, dus de
instelling wacht nu tot zij de laatste eigenaar is voordat ze de oude pool
loslaat.

*Gemeten winst* (i9-10885H, 8 fysieke kernen, WASAPI 48 kHz, 480 frames,
Friesach met alle 44 registers):

| Stemmen | 1 kern | 8 stukken | winst |
|---|---|---|---|
| 428 | 35,5 % | 10,3 % | 3,5× |
| 744 | 76,1 % | 34,0 % | 2,2× |
| 1.024 | 178,9 % | 48,0 % | 3,7× |
| 1.088 | 170,0 % | 50,1 % | 3,4× |

En per aantal stukken bij 436 stemmen: 1 stuk 37,5 %, 2 stukken 20,5 %,
4 stukken 12,4 %, 8 stukken 9,8 % — de reductie kost daarbij 0,02 %, 0,11 % en
0,28 %. Dat is **een factor 3,4 tot 3,7**, precies in de voorspelde bandbreedte.
Bij 1.024 stemmen zakt de belasting van onspeelbaar (179 %) naar comfortabel
(48 %); de polyfonie-kap is daarmee eerder bindend dan de rekenkracht.

*Verificatie:* 202 tests groen, waaronder vier die bewijzen dat verdeeld mengen
hetzelfde geluid geeft (200 stemmen, 1 tegen 2/3/4/8 stukken, verschil onder een
honderdduizendste van het niveau) en drie voor de pool zelf. Stresstest: 200 keer
de pool herbouwen terwijl 576 stemmen klinken — 576 stemmen erna, één extra
zware callback, nul gedropte realtime-commando's, en evenveel threads als ervoor
(60 → 60), dus geen lek.

*Het offline render-pad is niet gebouwd.* In plaats daarvan bewijst een
unittest de gelijkheid rechtstreeks op de mengfunctie, met echte stemmen die uit
echte preload-buffers lezen. Dat is scherper dan een WAV-vergelijking en het
draait mee in de gewone testronde.

**Fase 4 — de instelling en de automatische keuze (een dag).** De
voorkeursinstelling, de kernendetectie, de standaardwaarde bij de eerste start,
de teksten in zeven talen. *Verificatie: op deze machine komt er 7 uit; de
instelling overleeft een herstart.*

**Fase 5 — de meetknop (een dag).** Zie 4.3.

**Fase 6 — meten en verantwoorden (een halve dag).** Dezelfde tabel als in
paragraaf 1, nu met 1 tot 8 kernen naast elkaar, in de changelog en in het
vergelijkingsrapport.

## 7. Wat dit niet oplost

- **Het laden van samplesets** wordt er niet sneller van; dat draait al
  meerkernig via rayon en is schijfgebonden.
- **De galm, EQ en limiter** blijven op één kern. Ze kosten samen 1,4 %, dus
  daar valt niets te halen.
- **Hauptwerks 32.768 stemmen** blijft op een huis-pc buiten bereik, met of
  zonder dit plan. Het verschil is dat het getal daarna eerlijk in de buurt komt
  van wat de machine kan, in plaats van een factor vier ernaast te liggen.
- **De latentie** verandert niet. Dit plan gaat over hoeveel er in een buffer
  past, niet over hoe klein die buffer kan zijn.

## 8. Voorstel

Fase 1 en 2 zijn risicoloos en leveren meteen de cijfers die de rest
onderbouwen. Fase 3 is het echte werk en raakt het hart van de audiothread; die
zou ik pas beginnen als 0.7.47 op het testorgel bevestigd is — wat inmiddels zo
is — en met ruimte om hem apart te testen vóór de release van oktober.

Als de oktoberdatum krap wordt, is fase 1 t/m 2 een prima tussenstand: geen
gedragsverandering, wel de voorbereiding klaar.
