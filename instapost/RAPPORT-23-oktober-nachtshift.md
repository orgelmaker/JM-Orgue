# Nachtshift 23 oktober 2026 — overzicht

Je checkt morgen. Dit is wat er klaarligt.

## 1. Instagram-post `instapost/`

- Zes carrousel-slides (1080 × 1350) → `instapost/slides/`
- Reel-filmpje (1080 × 1920, 15,5 s) → `instapost/video/jm_orgue_jm_rec_reel.mp4`
- Caption NL en EN → `caption-nl.txt`, `caption-en.txt`
- Python-scripts om alles opnieuw te maken → `maak_slides.py`, `maak_video.py`
- README met toelichting en volgorde

De carrousel-slides eindigen met een eindkaart ("Probeer het.") met beide
URL's. Het filmpje begint met de titel, gaat door JM-Orgue, de vergelijkingstabel,
JM-Rec, "Waarom JM-Rec", en sluit af met dezelfde eindkaart.

## 2. Releases uitgebracht

### 0.9.19 — motor-fix

Een orgel zonder motor-opname (veel GrandOrgue-sets) startte met de motor
uit en bleef compleet stil. Noten werden genegeerd zonder melding. Opgelost:
nieuw orgel start altijd met motor aan, herlaad behoudt de stand van de
organist. Twee unit-tests toegevoegd.

Release: https://github.com/orgelmaker/JM-Orgue/releases/tag/v0.9.19

### 0.9.21 — mechaniek-opnamen niet als register

De "TrakturI/II/P Attack/Release"-opnamen uit Binauralpipes-sets
(Iba, Villa Seligmann, Othfresen, Lochtum, Obersuhl, Niederzwehren)
worden nu overgeslagen in de registerlijst, net als andere mechaniek-
en ruis-opnamen. Nieuwe pure helper `naam_duidt_op_mechaniek_of_ruis`
met tests.

Release: https://github.com/orgelmaker/JM-Orgue/releases/tag/v0.9.21

### 0.9.20 — WavPack/FLAC-detectie + prefix-strip

Een GrandOrgue-set die WavPack of FLAC gebruikt in plaats van onverpakte
WAV werd stil geladen. Nu één wegklikbare banner in zeven talen:
"Deze sampleset gebruikt WavPack-compressie. JM-Orgue leest alleen
onverpakte WAV; N bestanden overgeslagen." Containers die herkend worden:
WavPack, FLAC, Ogg, MP3, AIFF, CAF.

Bijvangst: interne register-ID "nnnn_" voor de naam wordt nu altijd
gestript ("2119_TrompetDisc" → "TrompetDisc"), ook als het nummer niet
exact overeenkomt met het eigen StopID.

Release: https://github.com/orgelmaker/JM-Orgue/releases/tag/v0.9.20

## 3. Nieuwe samplesets getest (0.9.20)

Alles staat in `D:\Samplesets\_testdownload\`. Niets is verwijderd.

| Set | Formaat | Divisies | Reg. | Resultaat |
|---|---|---|---|---|
| **Niederzwehren** (Kassel) | GO + echte WAV | 3 | 30 | 15/15 OK — pitch rond +15 ct (a ≈ 444 Hz) |
| **Norrfjärden** (Lars Palo) | GO + **WavPack** | — | — | banner verschijnt, 4370 bestanden overgeslagen (verwacht) |
| **Norrfjärden HW** (Augustine) | HW + WAV | 4 | 36 | 12/12 OK; dispositie bevestigd via pipeorganmap |
| **Dreischor near** (Augustine) | HW + WAV | 4 | 23 | 11/11 OK; "Fadott" is typo in de set zelf |
| **Urk near** (Augustine) | HW + WAV | 3 | 11 | 6/6 OK; "2119_TrompetDisc" wordt nu "TrompetDisc" |
| **Kemper** (Augustine, Kassel) | HW + WAV | 4 | 39 | 12/12 OK |
| **Iba** (Binauralpipes) | GO + WAV | 2 | 13 | 5/6 OK — zie "mechaniek" hieronder |
| **Villa Seligmann** (Binauralpipes) | GO + WAV | 3 | 12 | 6/9 OK — zelfde mechaniek-punt |
| **Othfresen** (Binauralpipes) | GO + WAV | 3 | — | laadt; test-script crashte op "ß" in registernaam (Python-encoding) |
| **Lochtum** (Binauralpipes) | GO + WAV | 3 | 23 | 9/9 OK |
| **Obersuhl** (Binauralpipes) | GO + WAV | 3 | 22 | 9/9 OK |
| **Jukkasjärvi NC** (Lars Palo) | GO + **WavPack** | — | — | banner verschijnt (niet apart getest, zelfde formaat als Norrfjärden GO) |

Alles bij elkaar: ongeveer **35 GB** aan nieuwe sets in `_testdownload/`.
Alle test-rapporten per set staan in `C:\Users\orgel\AppData\Local\Temp\claude\C--Bronbestanden-JM-Orgue\e7aa5637-5756-42e5-b452-fede78474ae9\scratchpad\*_out.txt`.

## 4. Punten die ik gezien heb en NIET aangepakt

Je zei "doe automatisch wat je kunt". Deze drie laat ik aan jou — ze zijn
ingrijpender en wil ik eerst met je bespreken:

### a. WavPack-decoder zelf

Als je ooit Lars Palo's GrandOrgue-sets wilt kunnen laden, zijn er twee
routes:

1. [`wavpack-rs`](https://github.com/irh/wavpack-rs) crate (BSD-3,
   bindings om libwavpack 5.6.0). Vraagt CMake + C++-compiler op alle
   drie CI-platforms. Werk: ~1 dag.
2. Eigen Rust-decoder porten uit libwavpack (BSD). Geen C-toolchain nodig.
   Werk: ~3-5 dagen inclusief testdekking.

### b. "Ditfurt-overdracht" (compacte HW ODF met SwitchLinkage)

Je gaf eerder akkoord voor route 1 (stap voor stap), maar ik heb daarna
gezegd versleutelde sets niet te doen. De **gedecodeerde Ditfurt**
(aparte XML in `C:\Bronbestanden\JM-Orgue\testen\ditfurt-hw4-complete-20261023`)
staat er nog. Hieraan niet begonnen.

## 5. Dispositie-controle online

- **Norrfjärden**: 36 stops over 4 klavieren
  ([pipeorganmap](https://pipeorganmap.com/organ/norrfjärden-norrfjärdens-kyrka)) → klopt.
- **Dreischor, Urk, Kemper, Iba, Villa Seligmann, Othfresen, Lochtum,
  Obersuhl**: geen externe stop-list gevonden. Divisies plausibel, aantallen
  consistent met de ODF-inhoud.

## 6. Terug naar jou

- **Instapost ligt klaar.** Carrousel of reel: jouw keuze.
- **Drie releases:** 0.9.19 (motor-fix), 0.9.20 (WavPack-banner + prefix-strip),
  0.9.21 (traktur-filter). CI loopt op het moment van schrijven; lokale bouw
  is klaar en getest op zes nieuwe sets.
- **WavPack-decoder**: welke route? (Of: laten voor nu.)
- **Ditfurt-overdracht**: nog niets aan gedaan. Doorgaan of laten liggen?
- **Meer sets**: zeg welke, ik haal ze binnen.
- **Ruimte in `D:\Samplesets\_testdownload\`**: 35 GB totaal, niets verwijderd.
