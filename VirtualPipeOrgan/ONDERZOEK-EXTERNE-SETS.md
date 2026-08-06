# Onderzoek: externe samplesets (GrandOrgue-vergelijking) — 24 juli 2026

Aanleiding: op externe (natte) GO-sets zoals Friesach hapert het bij veel registers,
klinken loops/loslaten niet schoon ("tikken") en klinkt de galm(staart) raar, terwijl
eigen droge sets (Puttershoek) schoon spelen. Voor dit onderzoek is de actuele
GrandOrgue-broncode (github.com/GrandOrgue/grandorgue, GPL-2.0) bestudeerd, met name
`src/grandorgue/sound/` (GOSoundOrganEngine, GOSoundReleaseAlignTable, GOSoundStream,
GOSoundDefs).

## Hoe GrandOrgue het doet — en wat wij (nog) niet doen

### 1. Release-uitlijning (GOSoundReleaseAlignTable) — ONS GROOTSTE GAT

GO start een release-sample **niet op frame 0**, maar op de plek in de release die
qua golfvorm aansluit op wat er nú klinkt:

- Bij het laden wordt per release-sample een tabel gebouwd over het eerste stuk
  (tot de periode van de laagste pijp): voor elke combinatie van (amplitude-bucket
  × helling-bucket) de eerste samplepositie met die waarde (16×16 buckets).
- Elke spelende stem onthoudt zijn laatste 2 samples (BLOCK_HISTORY).
- Bij loslaten: lookup (huidige amplitude, huidige helling) → startpositie in de
  release; de release begint dus fase-uitgelijnd op het klinkende signaal.

**Wij** starten de release op de cue-marker/frame 0 met een korte crossfade op de
stervende sustain. Elke phase-mismatch is een hoorbare discontinuïteit — dé bron van
de "tikken bij loslaten" op natte sets. Droge sets hebben geen release-samples,
vandaar dat Puttershoek schoon is.

### 2. Geschaalde releases bij staccato (m_IsScaledReleases) — verklaart "galm raar"

GO schaalt de release-versterking op basis van hoe lang de noot geklonken heeft:

- Noot losgelaten tijdens de attack (staccato): release-gain ×0,2..1,0 (kwadratische
  curve over de geschatte attackduur; bas ~500 ms, discant ~50 ms).
- "Time to full reverb"-model: de kerkgalm in de release is pas volledig opgebouwd
  na 100–350 ms (afhankelijk van de release-lengte). Voor kortere noten wordt de
  release-STAART actief weggefaded (decay 200 ms–6 s, evenredig met de nootduur).

**Wij** spelen bij elke NoteOff de volledige release op het huidige envelope-niveau:
een staccato-akkoord dumpt de volle kerkstaart van elke pijp → onnatuurlijk veel en
"rare" galm. Dit is geen reverb-instelling maar release-gedrag.

### 3. Polyfonie-beheer

- GO: harde limiet via sampler-pool (standaard duizenden; per-sampler kosten laag
  door int-decodering + SIMD) en een **zachte limiet**: daarboven krijgen releases
  die al >2,75 s spelen een fade van 370 ms (`StartDecreasingVolume(MsToSamples(370))`).
- Optioneel per set: **release tail truncation** (GetReleaseTail) — gebruiker kapt
  release-staarten op bv. 3 s af; GO fade't ze dan netjes.

**Wij** (sinds 0.6.9): budget van 160 gelijktijdige release-staarten; daarboven
maakt de stilste plaats. Fade is per dit onderzoek verzacht van 30 ms naar 250 ms
(GO-niveau). Onze per-voice-kosten (f32 + hermite + per-voice effects) zijn hoger
dan die van GO — het budget blijft dus nodig totdat de mixkosten omlaag kunnen.

### 4. Loops en crossfades

- GO bakt loop-crossfades bij het LADEN in de sampledata (REMAINING_AFTER_CROSSFADE
  256), ondersteunt meerdere loops per sample en kiest per afspeelbeurt.
- Wij optimaliseren looppunten bij het laden (fase-uitlijning) en crossfaden bij
  het afspelen. Sinds 0.6.7 kent ook de full-sample-load de ODF-looppunten.
  Verschil is acceptabel; meerdere loops per sample (variatie) ontbreekt bij ons.

### 5. Galm bij natte sets

GO voegt NIETS toe aan natte sets — de akoestiek zit volledig in attack- en
release-samples. Onze droog-start voor GO-imports (0.6.4) is dus correct; de "rare
galm" komt uit punt 2 (staccato-release-gedrag), niet uit onze reverb. Advies:
app-galm op 0 laten voor GO-sets (is al de default).

## Aanbevolen fasering

- **Fase A (gedaan in 0.6.10)**: release-budget-fade 250 ms; koppels/presets/
  inleer-fixes los van dit onderzoek.
- **Fase B — release-uitlijning (grootste klankwinst, ~2-3 dagdelen)**:
  1. Bij het laden per release-preload een 16×16 align-tabel bouwen (poorten van
     GOSoundReleaseAlignTable::ComputeTable naar Rust, over de preload-buffer).
  2. PlayingVoice onthoudt zijn laatste 2 gemixte samples (al bijna gratis).
  3. In NoteOff/ReleaseStop: startpositie uit de tabel i.p.v. cue/0.
  4. Cache de tabel naast de preload zodat herladen goedkoop blijft.
- **Fase C — geschaalde releases (~1 dagdeel)**: sample-teller in de engine als
  klok; PlayingVoice krijgt starttijd; bij release de GO-formules toepassen
  (attack-schaal + time-to-full-reverb-decay). Lost "galm raar" bij staccato op.
- **Fase D — optioneel**: release tail truncation als gebruikersinstelling per
  orgel (kap bv. op 4 s); meerdere loops per sample; per-voice-kosten verlagen
  (minder per-sample werk) zodat het budget omhoog kan.

Bron: GrandOrgue-broncode (GPL-2.0 of later), bestudeerd 2026-07-24; de hierboven
beschreven technieken zijn algoritmisch geherformuleerd, geen code-overname.
