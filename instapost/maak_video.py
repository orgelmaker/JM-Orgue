"""Reel-video 1080x1920 van ~18 seconden met zachte pan over de carrousel-slides.

De carrousel-kaarten zijn 1080x1350 (feed). Voor Reels brengen we ze naar 1080x1920
door een geblurde achtergrond + de kaart in het midden, met zachte Ken-Burns-zoom
en cross-fades tussen de kaarten.
"""
import os, subprocess
from PIL import Image, ImageFilter

HIER = os.path.dirname(os.path.abspath(__file__))
SLIDES = os.path.join(HIER, "slides")
VIDEO = os.path.join(HIER, "video")
TMP = os.path.join(VIDEO, "tmp")
os.makedirs(TMP, exist_ok=True)

VOLGORDE = [
    "slide1_titel.png",
    "slide2_jmorgue.png",
    "slide3_vergelijking.png",
    "slide4_jmrec.png",
    "slide5a_jmrec_waarom.png",
    "slide5_slot.png",
]

W, H = 1080, 1920
SEC = 3.0  # per kaart
FADE = 0.5

def reel_achtergrond(bron_pad, uit_pad):
    """1080x1920 achtergrond: geblurde kopie van de kaart met de scherpe kaart erbovenop."""
    k = Image.open(bron_pad).convert("RGB")
    # Blur-achtergrond: schaal kaart naar 1080x1920 + sterke blur
    achter = k.resize((W, H), Image.LANCZOS).filter(ImageFilter.GaussianBlur(40))
    # Donkerder maken
    pix = achter.load()
    for y in range(H):
        for x in range(W):
            r, g, b = pix[x, y]
            pix[x, y] = (r//3, g//3, b//3)
    # Scherpe kaart gecentreerd (1080x1350 → bovenaan 1080x1350, met ruimte boven+onder)
    kw, kh = k.size
    y_off = (H - kh) // 2
    achter.paste(k, (0, y_off))
    achter.save(uit_pad, quality=92)

if __name__ == "__main__":
    # Stap 1: reel-achtergronden klaar zetten
    frames = []
    for naam in VOLGORDE:
        src = os.path.join(SLIDES, naam)
        dst = os.path.join(TMP, naam.replace(".png", "_reel.jpg"))
        reel_achtergrond(src, dst)
        frames.append(dst)

    # Stap 2: per frame een korte video met zachte zoom (Ken Burns)
    stukjes = []
    for i, f in enumerate(frames):
        uit = os.path.join(TMP, f"stuk{i:02d}.mp4")
        # Zoom in de ene, zoom uit de volgende — kleine beweging
        if i % 2 == 0:
            zoom = "zoom='min(zoom+0.0008,1.08)'"
        else:
            zoom = "zoom='if(lte(zoom,1.0),1.08,max(1.001,zoom-0.0008))'"
        # Gebruik zoompan met een stilstaande kaart. Omdat zoompan één pixel-flikker
        # geeft op statische beelden, benaderen we met een vaste frame-rate.
        cmd = [
            "ffmpeg", "-y", "-loop", "1", "-i", f,
            "-vf", f"scale=1080:1920:flags=lanczos,zoompan={zoom}:d=1:s=1080x1920:fps=30",
            "-t", str(SEC),
            "-r", "30",
            "-c:v", "libx264", "-pix_fmt", "yuv420p",
            "-preset", "medium", "-crf", "19",
            uit,
        ]
        print("stuk", i, "...")
        r = subprocess.run(cmd, capture_output=True, text=True)
        if r.returncode != 0:
            print("ffmpeg stuk-fout:", r.stderr[-400:])
            raise SystemExit(1)
        stukjes.append(uit)

    # Stap 3: xfade-ketting tussen alle stukjes
    # Omdat xfade altijd 2 ingangen combineert, bouwen we een filter-complex.
    args = ["ffmpeg", "-y"]
    for s in stukjes:
        args += ["-i", s]
    # Elke stukje duurt SEC seconden; crossfade van FADE sec eruit, dus volgende begint SEC-FADE later.
    offset = SEC - FADE
    # Bouw: [0:v][1:v] xfade dissolve offset=offset [v01];
    #       [v01][2:v] xfade dissolve offset=(2*offset) [v02]; enz.
    delen = []
    laatste = "0:v"
    for i in range(1, len(stukjes)):
        label = f"vx{i}"
        off = (i)*offset
        delen.append(f"[{laatste}][{i}:v]xfade=transition=fade:duration={FADE}:offset={off:.3f}[{label}]")
        laatste = label
    fc = "; ".join(delen)
    uit = os.path.join(VIDEO, "jm_orgue_jm_rec_reel.mp4")
    args += ["-filter_complex", fc, "-map", f"[{laatste}]",
             "-c:v", "libx264", "-pix_fmt", "yuv420p",
             "-preset", "medium", "-crf", "19", "-r", "30",
             uit]
    print("stitch…")
    r = subprocess.run(args, capture_output=True, text=True)
    if r.returncode != 0:
        print("ffmpeg stitch-fout:", r.stderr[-600:])
        raise SystemExit(1)
    print("KLAAR:", uit)
