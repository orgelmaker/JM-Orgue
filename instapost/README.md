# Instagram-post: JM-Orgue + JM-Rec

**Doel:** één post die beide sites (GitHub Pages) laat zien en de twee
programma's samen presenteert.

## Inhoud van deze map

```
instapost/
  bron/                        bron-screenshots van beide sites
    jm-orgue-site-top.jpg
    jm-orgue-site-vergelijking.jpg
    jm-rec-site-top.jpg
    jm-rec-waarom.jpg
  slides/                      1080 × 1350 (feed-carrousel)
    slide1_titel.png            — titelslide met beide blokken
    slide2_jmorgue.png          — JM-Orgue top (screenshot)
    slide3_vergelijking.png     — vergelijkingstabel JM-Orgue
    slide4_jmrec.png            — JM-Rec top (screenshot)
    slide5a_jmrec_waarom.png    — Waarom JM-Rec (voordelen)
    slide5_slot.png             — eindkaart met beide links
  video/
    jm_orgue_jm_rec_reel.mp4    — 1080 × 1920, ~15,5 s, voor Reels/Stories
    tmp/                        — tussenstukken (mag weg)
  caption-nl.txt                — caption Nederlands
  caption-en.txt                — caption Engels
  maak_slides.py                — bron: Python-script dat de slides maakt
  maak_video.py                 — bron: Python-script dat het filmpje bouwt
```

## Posten

**Feed-carrousel (NL):** post slides 1–6 in deze volgorde met caption-nl.txt.

**Reels/Stories:** post `video/jm_orgue_jm_rec_reel.mp4`
(1080 × 1920, 15,5 s) met dezelfde caption. Instagram knipt vanzelf naar
30 s en zal de video onder "Reels" tonen.

## Links in de post

- https://orgelmaker.github.io/JM-Orgue
- https://orgelmaker.github.io/JM-Rec

## Opnieuw maken

Beide sites in de browser zetten op telefoon-formaat (1080 breed), de
screenshots in `bron/` vervangen en:

```bash
python maak_slides.py
python maak_video.py
```

Het filmpje blijft ongeveer 15 s. Pas `SEC` of `VOLGORDE` in
`maak_video.py` aan voor langer/korter of een andere volgorde.
