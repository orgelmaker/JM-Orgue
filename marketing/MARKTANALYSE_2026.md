# Marktanalyse — JM-Orgue & JM-Rec positie (mei 2026)

## 🎯 De vraag

Waar staan we in het landschap van virtuele pijporgel-software, en is onze focus (eigen kerkorgel samplen + thuis afspelen) de juiste?

## 🏛️ Concurrentie-overzicht

| Software | Type | Prijs | Doelgroep | Sterk in | Zwak in |
|----------|------|-------|-----------|----------|---------|
| **Hauptwerk** | Sample-based, professioneel | €230 één-malig + dure sets | Professionele organisten, instituten | Authenticiteit, ondersteuning, ruim sample-aanbod | Hoge RAM-eisen (16-64 GB), lange laadtijden, complex |
| **Sweelinq** | Sample + modelled reverb | €11-13/maand abonnement | NL/BE organisten, thuispraktijk | Gebruiksvriendelijk, touch-bediening, NL focus | Abonnement, beperkt sample-aanbod, online vereist |
| **GrandOrgue** | Sample-based, open source | Gratis | Hobbyisten, DIY-bouwers, Linux-gebruikers | Volledig gratis, breed sample-aanbod (ODF format) | UI gedateerd, complexere setup, eigen sampling vraagt aparte tools |
| **jOrgan / Aeolus** | Synthesis-based | Gratis | Experimentele projecten | Lichtgewicht, flexibel | Geen echte orgel-realisme |
| **OrganTeq** | Modal physical modelling | Plug-in prijs | DAW-gebruikers, producers | Plugin format, klein RAM | Niet authentic per orgel |
| **JM-Orgue** | Sample-based + JM-Rec | Gratis, open source | **NL organisten die hun eigen kerkorgel willen** | Eigen sampling tool (JM-Rec), snelle laad-techniek, NL-eerst | Vroege fase, beperkt aantal supported features t.o.v. Hauptwerk |

## 🔍 Wat zegt de community? (forums)

### Hauptwerk forum (engelstalig)
- **"Slow loading times"** is een terugkerend thema — Hauptwerk 8 heeft hier expliciet aan gewerkt
- **RAM-eisen** (16-64 GB) zijn een drempel — vooral op laptops een issue
- **Sample creation forum** bestaat, maar het is "een hobby voor de paar die het volhouden"

### MPS Orgelseite (Duitstalig)
- Lange thread "Software für gesampelte Orgeln Murks?" — gebruikers zoeken alternatieven
- Discussies over **GrandOrgue + Audacity + GOODF** als pad voor eigen samples, maar veel barrières
- Vraag: "Wie detailliert muss eine Orgel für Hausgebrauch simuliert werden?" — antwoord: minder dan men denkt

### Jeux d'Orgues forum (Frans)
- Title-thread: **"Le parcours du combattant"** (de hindernisbaan) — een gevestigde uitdrukking voor het samplen van eigen orgel
- Behoefte aan **simpeler workflow** is duidelijk
- Goede sample-banks bestaan maar **vragen weken werk** om te maken

### Orgelnieuws.nl + Refoforum (Nederlands)
- "Wat raden jullie aan: Hauptwerk, Sweelinq of GrandOrgue?" — veel beginners overweldigd
- Sweelinq wordt **steeds populairder** in NL (Sonarte, Oostendorp distributies)
- Behoefte aan **simpele toegang tot het EIGEN orgel** wordt expliciet genoemd

## ✅ Waar we het goed doen (USP's bevestigd door forums)

### 1. JM-Rec voor eigen sampling — gat in markt
Niemand biedt momenteel een **één-klik tool voor de gewone organist** om zijn eigen kerkorgel op te nemen. De alternatieven zijn:
- Audacity + handmatige bestandsnaamgeving + GOODF (technisch, uren werk)
- Inhuren van professionele sample-makers (€€€)
- Hauptwerk-eigen tools (alleen voor pro's)

JM-Rec → automatische mapstructuur + `.organ` definitie = **uniek**.

### 2. Snelle laad-techniek — bewezen pijn
Preload-buffer aanpak (attack direct → rest in achtergrond) is een concreet voordeel. Hauptwerk-gebruikers klagen er actief over; Hauptwerk 8 (2026) heeft dit als hoofdfeature aangepakt — we lopen dus mee op de juiste curve.

### 3. Gratis + open source
- Hauptwerk = duur, propriëtair
- Sweelinq = abonnement (€132/jaar)
- GrandOrgue = gratis maar UI gedateerd
- **JM-Orgue = gratis + moderne UI + open source** = goede positie

### 4. Native Hauptwerk-set compatibility (op de roadmap voor v0.3)
Mensen hebben geld geïnvesteerd in Hauptwerk-sets. Die ook kunnen gebruiken is enorm aantrekkelijk.

### 5. Touchscreen-first design
Sweelinq markt zich expliciet op touch-bediening. Wij hebben dat ook (`--touch-target`, long-press learn). Concurrentie-pariteit.

## ⚠️ Waar we kwetsbaar zijn

### 1. Sweelinq overlapt grotendeels met onze doelgroep
NL-organisten, eigen kerkorgel, thuispraktijk. Hun voordeel: gefocuste marketing, distributiekanaal (Oostendorp), bestaande klantenbasis. **Maar:** Sweelinq biedt geen eigen-orgel-sampling — dat is ons concrete onderscheid.

### 2. Sample-set ecosysteem is nog leeg
Hauptwerk en GrandOrgue hebben honderden sets. Wij hebben er een handvol. **Mitigation:** Hauptwerk-set compatibility + JM-Rec workflow voor users-as-creators.

### 3. Naam-herkenning
Niemand kent JM-Orgue. Sweelinq + Hauptwerk hebben jaren marketing achter zich.

### 4. Plugin-ecosysteem (VST/AU)
Hauptwerk + Sweelinq hebben dit nog niet goed; wij hebben er een MVP voor. **Daar liggen kansen** voor productie-organisten die DAW gebruiken.

## 🎯 Conclusie: zijn we op de goede weg?

**Ja**, maar met focus-aanpassingen:

### Houden:
1. ✅ **JM-Rec als kernproduct** — dit is het unieke verkooppunt
2. ✅ **Snelle laadtijd / preload-buffer** — bewezen voordeel
3. ✅ **Gratis + open source** — strategisch onderscheidend
4. ✅ **Touchscreen + moderne UI** — niveau van Sweelinq matchen

### Versterken:
1. 🔥 **Hauptwerk-set compatibility** — direct meer gebruikers
2. 🔥 **Multitaal UI** (EN/FR/DE) — opent buitenlandse markten waar Sweelinq niet sterk is
3. 🔥 **VST/AU plugin** — niche maar zonder concurrentie
4. 🔥 **Marketing/community-bouw** — Instagram, organforum.com, MPS Orgelseite presence

### Overwegen:
- Aparte distributie partner voor sample-sets? (zoals Sonarte voor Sweelinq)
- Premium feature laag (bv. sample-streamen of cloud-sync) voor toekomstig inkomstenmodel
- Samenwerking met sample-makers (Piotr Grabowski, Sonus Paradisi vrije sets) voor curatie

## 📚 Bronnen geraadpleegd

- [Hauptwerk forum](http://forum.hauptwerk.com/) — sample creation, loading time threads
- [MPS Orgelseite](https://mps-orgelseite.de/home/forum/) — Duitstalig, DIY-community
- [Jeux d'Orgues forum](https://www.jeuxdorgues.com/forum/) — Frans, "parcours du combattant" thread
- [Orgelnieuws.nl](https://www.orgelnieuws.nl/) — NL nieuws over virtuele orgels
- [Refoforum virtueel orgel](https://refoforum.nl/forum/viewtopic.php?t=15744) — NL gebruikersdiscussie
- [The Organ Forum (organforum.com)](https://organforum.com/) — internationaal
- [Lars virtual pipe organ — sampleset creation](https://familjenpalo.se/vpo/sampleset-creation/) — GO sample creation guide
- [GOODF (GrandOrgue ODF designer)](https://github.com/GrandOrgue/grandorgue/discussions/173) — concurrent tool voor .organ
- [Sweelinq](https://www.sweelinq.com/) — abonnement-concurrent
- [Hauptwerk](https://www.hauptwerk.com/) — marktleider
- [Sonarte (NL Sweelinq distributie)](https://www.sonarte.nl/systemen/sweelinq-software/)

---
*Opgesteld: mei 2026, op basis van publieke forum-discussies en software-vendor sites.*
