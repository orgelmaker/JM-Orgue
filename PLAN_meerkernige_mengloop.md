# Plan: de mengloop over meerdere kernen

Opgesteld 21 september 2026, op versie 0.7.48. Aanleiding: de meting bij de
polyfonie-verhoging liet zien dat het getal niet de grens is — de ene rekenkern
waarop de mengloop draait, is dat wel.

## 1. Wat de meting zegt

Gemeten op de ontwikkelmachine (Intel i9-10885H, 8 fysieke kernen / 16 logische,
WASAPI 48 kHz, 480 frames per callback = 10 ms budget) met Friesach en alle 44
registers getrokken:

| Klinkende stemmen | Belasting van de buffertijd |
|---|---|
| 0 (stilte) | 1,4 % |
| 210 | 29 % |
| 424 | 68 % |
| 828 | 139 % — over de deadline |
| 1.088 | 179–195 % |

Twee dingen vallen daaraan op.

**De kosten zijn recht evenredig met het aantal stemmen.** Ongeveer 0,17 % van
de buffertijd per stem, van 200 tot 1.100 stemmen zonder knik. Er is dus geen
cache-instorting meer (die was er vóór 0.7.36, toen de mengloop nog frame-major
was); wat er nu staat is een vaste prijs per stem per sample.

**Alles wat niet met stemmen te maken heeft, kost niets.** De vloer — zwelkast,
routering, galm, EQ, limiter, windmodel, tremulant — is 1,4 % van de buffertijd.
In de zware situatie is dus **99 % van de rekentijd per-stem-werk**.

Dat laatste getal is het belangrijkste van dit plan. Het bepaalt via de wet van
Amdahl wat meerkernig renderen maximaal kan opleveren: met acht kernen
theoretisch een factor 7,5. In de praktijk wordt dat minder, omdat het werk
geheugengebonden is (elke stem leest uit zijn eigen samplebuffer van megabytes).
Juist daar helpen meer kernen wél goed: meer kernen betekent meer gelijktijdig
uitstaande geheugenverzoeken. Een realistische verwachting is **een factor 3 tot
5**, oftewel van ~500 naar 1.500–2.500 stemmen die een gewone pc volhoudt.

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

**Fase 1 — meten waar de tijd heen gaat (een halve dag).** Tijdmeting per pas
(1/2/3) achter een vlag, zodat we zwart op wit hebben dat pass 2 de 99 % is en
hoe duur een barrière op deze machine werkelijk is. Levert de drempels uit 3.3.
*Geen gedragsverandering.*

**Fase 2 — de opteltabellen loskoppelen (een dag).** Pass 2 schrijft naar een
*array van* opteltabellen in plaats van naar één, gevolgd door een
reductiestap — maar nog steeds op één thread. Hiermee is de structuur klaar
zonder dat er één thread bij komt, en is het verschil in optelvolgorde
afzonderlijk te beoordelen. *Verificatie: het geluid is meetbaar gelijk (de
bestaande FFT-tests), de belasting is onveranderd.*

**Fase 3 — de pool en de barrière (twee tot drie dagen).** Werkers, prioriteit,
FTZ/DAZ, spin-en-park, het opruimen bij een streamwissel. Vast op het door de
gebruiker ingestelde aantal, standaard nog 1. *Verificatie: honderd
audiowissels achter elkaar zonder achterblijvende threads; de rookproef en alle
bestaande tests groen.*

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
