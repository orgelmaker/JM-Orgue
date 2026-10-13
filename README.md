<div align="center">

# JM-Orgue

**Een virtueel pijporgel voor thuis.**

Sluit een orgelklavier op de computer aan, kies een sampleset en speel.
JM-Orgue leest opnamen van echte orgels en geeft ze terug met registers,
koppels, zwelkasten, tremulanten, galm en een generaal crescendo — zoals
aan een echte speeltafel.

[![downloads](https://img.shields.io/github/downloads/orgelmaker/JM-Orgue/total.svg?label=downloads&color=1f6feb&style=for-the-badge&cacheSeconds=3600)](https://github.com/orgelmaker/JM-Orgue/releases)
[![laatste versie](https://img.shields.io/github/v/release/orgelmaker/JM-Orgue?label=laatste%20versie&color=238636&style=for-the-badge)](https://github.com/orgelmaker/JM-Orgue/releases/latest)
[![platforms](https://img.shields.io/badge/platforms-Windows%20%C2%B7%20macOS%20%C2%B7%20Linux-6e40c9?style=for-the-badge)](#downloaden)
[![licentie](https://img.shields.io/badge/licentie-vrij%20gebruiken%20%26%20doorgeven-8957e5?style=for-the-badge)](#licentie)

<img src="screenshots/02_orgel_registers.png" alt="JM-Orgue aan het werk" width="820">

<sub>De speeltafel in de app: registers per divisie, koppels en crescendo bij de hand.</sub>

</div>

---

## Downloaden

Deze links wijzen altijd naar de nieuwste versie. Ze blijven werken; je hoeft
niet te zoeken welke de laatste is.

<table>
<tr>
<th align="left">Systeem</th>
<th align="left">Bestand</th>
<th align="left"></th>
</tr>
<tr>
<td><b>Windows</b></td>
<td><a href="https://github.com/orgelmaker/JM-Orgue/releases/latest/download/JM-Orgue-setup.exe">JM-Orgue-setup.exe</a></td>
<td>installeert voor jezelf, geen beheerdersrechten nodig</td>
</tr>
<tr>
<td><b>Windows</b></td>
<td><a href="https://github.com/orgelmaker/JM-Orgue/releases/latest/download/JM-Orgue.msi">JM-Orgue.msi</a></td>
<td>installeert voor de hele computer</td>
</tr>
<tr>
<td><b>macOS</b></td>
<td><a href="https://github.com/orgelmaker/JM-Orgue/releases/latest/download/JM-Orgue.dmg">JM-Orgue.dmg</a></td>
<td>universal, Intel en Apple Silicon</td>
</tr>
<tr>
<td><b>Linux</b></td>
<td><a href="https://github.com/orgelmaker/JM-Orgue/releases/latest/download/JM-Orgue.AppImage">JM-Orgue.AppImage</a></td>
<td>uitvoerbaar maken en starten</td>
</tr>
<tr>
<td><b>Linux</b></td>
<td><a href="https://github.com/orgelmaker/JM-Orgue/releases/latest/download/JM-Orgue.deb">JM-Orgue.deb</a></td>
<td>Debian, Ubuntu en verwanten</td>
</tr>
</table>

Alle versies met hun wijzigingen staan bij de
[releases](https://github.com/orgelmaker/JM-Orgue/releases).

> **Bijwerken.** Op Windows meldt de app zelf dat er een nieuwe versie is en
> installeert die op verzoek, waarna hij opnieuw opstart. Op macOS en Linux
> haal je de nieuwe versie van deze pagina.
>
> **Eerste keer op macOS:** de app is niet ondertekend bij Apple. Open hem
> de eerste keer via rechtsklik → Openen.

---

## Samplesets

JM-Orgue leest drie soorten:

- **Eigen mappen** — een map per register, opnamen genoemd naar de toets.
  Zo eenvoudig als het klinkt, en bedoeld voor wie zelf opneemt.
- **GrandOrgue** (`.organ`) — rechtstreeks, zonder omzetstap.
- **Hauptwerk** (`.Organ_Hauptwerk_xml`) — ook rechtstreeks, inclusief
  microfoonposities, zwelkasten, release-opnamen en tremulant-opnamen.

Gesloten formaten werken niet: Hauptwerks eigen `.hbw`, `.pkg` en `.iwh`, en
Sweelinqs `.swop`. Zo'n set wordt geweigerd met een melding, niet met een
orgel zonder geluid.

Waar je vrij te gebruiken sets vindt, staat in
[docs/SAMPLE_SETS.md](VirtualPipeOrgan/docs/SAMPLE_SETS.md) en in de app zelf,
onderaan de orgelbibliotheek.

---

## Wat het doet

<table>
<tr>
<td width="33%" valign="top">

### Spelen
Registers, koppels, zwelkasten, tremulant en een generaal crescendo, te
bedienen met de muis, met een aanraakscherm of met de knoppen van je eigen
speeltafel via MIDI.

</td>
<td width="33%" valign="top">

### Klinken
Galm en toonregeling per uitgang, keuze uit temperaturen, een windmodel
dat de druk laat meebewegen met wat je speelt, en microfoonposities
waartussen je live wisselt.

</td>
<td width="33%" valign="top">

### Uitvoer
ASIO of WASAPI op Windows, CoreAudio op macOS, ALSA op Linux. Meerdere
uitgangen tegelijk, met de divisies verdeeld over de kanalen.

</td>
</tr>
<tr>
<td valign="top">

### Opnemen
MIDI en MP3, met een archief dat vanzelf meeschrijft.

</td>
<td valign="top">

### Noteren
Wat je speelt verschijnt als notenschrift in een eigen venster. Dit
onderdeel is nog alfa en niet nagelopen.

</td>
<td valign="top">

### Zeven talen
Nederlands, Engels, Duits, Frans, Spaans, Italiaans en Pools.

</td>
</tr>
</table>

---

## Wat je nodig hebt

Een computer met een geluidskaart, en een MIDI-klavier of speeltafel als je
meer wilt dan met de muis spelen. Voor grote samplesets telt vooral
werkgeheugen: een set van duizenden pijpen vraagt er tientallen gigabytes van.
De app laadt niet alles ineens in, maar de ruimte moet er wel zijn.

Bouwen vanaf de broncode staat in
[BUILDING.md](VirtualPipeOrgan/BUILDING.md).

---

## Vragen of problemen

Open een [issue](https://github.com/orgelmaker/JM-Orgue/issues), of gebruik de
feedbackknop in de app onder Algemene Instellingen. Die stuurt het logboek
mee, wat zoeken een stuk korter maakt.

---

## Licentie

**Het programma mag je vrij gebruiken en doorgeven.** Installeren op zoveel
computers als je wilt, aan anderen geven, op een website zetten: dat mag,
zolang je de officiële installer ongewijzigd doorgeeft, er geen geld voor
vraagt en de licentie meegaat.

**De broncode staat hier uitsluitend ter inzage.** Kopiëren, wijzigen of in
andere software opnemen mag niet zonder schriftelijke toestemming, en de app
verkopen evenmin.

Samplesets vallen hier niet onder; daarvoor gelden de voorwaarden van wie ze
gemaakt heeft. Zie [LICENSE](LICENSE) voor de volledige tekst; een verzoek om
toestemming doe je via een issue.
