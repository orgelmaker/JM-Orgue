# macOS en Linux: waar staan we, en hoe testen we het?

Peildatum 16 september 2026, versie 0.7.44. Aanleiding: de vraag of we macOS
kunnen testen en of een Linux-versie mogelijk is.

## Kort antwoord

**Beide platformen werken verder dan verwacht.** De bouwstraat bouwt en start de
app nu op macOS én Linux, met werkende audio. Wat ontbreekt is niet de code maar
de *hand aan de knoppen*: een echte Mac of Linux-pc met een orgelklavier eraan.

## Wat de bouwstraat bewijst

Nieuwe workflow `.github/workflows/platform-check.yml`, handmatig te starten via
Actions → "Platformcontrole (macOS en Linux)" → Run workflow. Hij bouwt de
frontend, draait alle Rust-tests, bouwt de app, start hem met de test-API en
vraagt de status op.

Uitkomst van de laatste run (16 september 2026):

| | macOS | Linux |
|---|---|---|
| Frontend bouwen | ✅ | ✅ |
| Alle tests (161 + 5) | ✅ | ✅ |
| App bouwen | ✅ | ✅ |
| App start en antwoordt | ✅ | ✅ (venster via Xvfb) |
| Audio draait | ✅ CoreAudio, 48 kHz, 2 kanalen | ✅ ALSA, 44,1 kHz, 2 kanalen |
| Belasting bij stilte | 0,4 % | 0,2 % |

Twee dingen die deze controle meteen opleverde:

1. **Webadressen openen werkte alleen op Windows.** `open_external_url` riep
   altijd `rundll32` aan, dus op een Mac of Linux-pc zou geen enkele knop met een
   link werken: de feedbackknop, de releasepagina, de handleiding. Nu per
   platform: `rundll32`, `open` of `xdg-open`.
2. **Eén test viel om op beide platformen.** De JM-Rec-naamtest gaf een
   hardgecodeerd `C:\X\PuttBatz\PuttBatz.organ` mee; buiten Windows is dat geen
   mappad maar één bestandsnaam. De functie zelf was al draagbaar; de test is nu
   platformneutraal.

Verder bleek de rest van de code netjes afgeschermd: de registervermelding voor
automatisch starten, het afsluiten van de computer en de threadprioriteit van de
sample-laders staan al achter `#[cfg(target_os = ...)]` en vallen buiten Windows
stilletjes weg. ASIO is een aparte buildoptie en wordt buiten Windows niet
meegecompileerd.

## Wat de bouwstraat NIET bewijst

- **Spelen.** Er is geen sampleset op de bouwserver, dus er is nog nooit een noot
  geklonken op een Mac of Linux-pc.
- **MIDI.** Geen klavier, geen zweltrede, geen pistons. CoreMIDI (macOS) en ALSA
  MIDI (Linux) zijn nooit met echte hardware aangeraakt.
- **Het scherm.** Niemand heeft de app op die platformen zien draaien; de
  vensters, de knopmaten en de lettertypen kunnen er anders uitzien.
- **Latentie.** De bouwserver gebruikt een virtuele geluidskaart. Wat een echte
  interface doet, weten we niet.

## macOS: hoe testen we het echt?

Vier wegen, van goedkoop naar duur.

1. **Een Mac-bezitter uit de kring.** Verreweg het snelste. Nodig: de dmg, een
   sampleset, en een testblad met wat te proberen. Let op: de app is niet
   ondertekend, dus de eerste keer openen gaat via rechtsklik → Openen (en op
   nieuwere macOS: Systeeminstellingen → Privacy en beveiliging → "Toch openen").
   Vermeld dat erbij, anders denkt de tester dat de app stuk is.
2. **Een oproep bij de release.** Zet in de aankondiging een regel: "wie een Mac
   heeft en wil meetesten, meld je". Dat levert testers op die gemotiveerd zijn
   en die je daarna kunt bedanken in de app.
3. **Een Mac per uur in de cloud** (bijvoorbeeld Scaleway of MacInCloud, enkele
   tientjes per maand als je hem af en toe gebruikt; let op dat AWS een minimum
   van 24 uur per Mac rekent). Daarmee test je het scherm, het laden van een
   sampleset en de bijwerkknop. MIDI-hardware kan niet.
4. **Zelf een tweedehands Mac mini.** Als macOS een ondersteund platform wordt,
   is dit uiteindelijk onvermijdelijk: je hebt een machine nodig om elke release
   te controleren en om te ondertekenen.

### Wat er nog bij komt kijken voordat macOS "uitgebracht" heet

- **Ondertekenen en notariseren.** Zonder Apple Developer Program (ongeveer 99
  dollar per jaar) waarschuwt macOS bij elke installatie. De in-app-update werkt
  technisch al — het macOS-updatebestand en de handtekening rollen al uit de
  bouwstraat en staan in `latest.json` — maar een niet-ondertekende app die
  zichzelf bijwerkt, loopt opnieuw tegen die waarschuwing aan.
- **Een testronde met een klavier**, zoals we die voor Windows op het testorgel
  doen.

## Linux: is het mogelijk?

**Ja, en het staat dichterbij dan je zou denken.** De app compileert, start en
speelt audio af. Er is bovendien al aan gedacht: in `tauri.conf.json` staan de
instellingen voor een deb-pakket en een AppImage klaar.

Wat er nog moet gebeuren:

- **Een bouwtaak toevoegen** aan `release-build.yml` (Ubuntu-runner plus de
  pakketten die de platformcontrole al installeert), die een deb en een AppImage
  oplevert. Dat is een half uur werk, maar het is wel een belofte: vanaf dan
  verwachten mensen dat het blijft werken.
- **Kiezen welke distributies je noemt.** Tauri 2 gebruikt webkit2gtk 4.1; dat
  zit in Ubuntu 22.04 en nieuwer en in vergelijkbare distributies. Een AppImage
  vangt een deel van die verschillen op, maar niet alles.
- **Geluid uitzoeken op een echte machine.** ALSA werkt; de meeste Linux-pc's
  draaien tegenwoordig PipeWire, dat zich als ALSA voordoet. Voor lage latentie
  is dat waarschijnlijk prima, maar dat moet gemeten worden — niet aangenomen.
- Automatisch starten bij aanmelden en de computer afsluiten doen op Linux niets;
  die knoppen zouden daar verborgen of anders ingevuld moeten worden.

## Aanbeveling

1. **Houd de platformcontrole handmatig** zoals nu, en draai hem vóór elke
   release. Hij kost ongeveer tien minuten en vangt precies wat hij vandaag ving:
   code die stilzwijgend Windows-only is geworden.
2. **Zoek één Mac-tester en één Linux-tester** voor de release. Dat kost niets en
   levert meer op dan welke bouwserver ook.
3. **Beloof nog niets.** Zolang er geen mens op die platformen heeft gespeeld,
   blijft de aankondiging spreken over een gewone Windows-pc. Wat de bouwstraat
   laat zien is een goede basis, geen uitgebrachte versie.
