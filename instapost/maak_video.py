"""Reel-video 1080x1920 van ~18 s met zachte pan over de carrousel-slides.

Draai zonder argumenten voor beide talen. Draai met "nl" of "en" voor één taal.

De carrousel-kaarten zijn 1080x1350 (feed). Voor Reels brengen we ze naar 1080x1920
door een geblurde achtergrond + de kaart in het midden, met zachte Ken-Burns-zoom
en cross-fades tussen de kaarten.
"""
import os, subprocess, sys
from PIL import Image, ImageFilter

HIER = os.path.dirname(os.path.abspath(__file__))
VIDEO = os.path.join(HIER, "video")

VOLGORDE = [
    "slide1_titel.png",
    "slide2_jmorgue.png",
    "slide3_vergelijking.png",
    "slide4_jmrec.png",
    "slide5a_jmrec_waarom.png",
    "slide5_slot.png",
]

W, H = 1080, 1920
SEC = 3.0
FADE = 0.5

def reel_achtergrond(bron_pad, uit_pad):
    k = Image.open(bron_pad).convert("RGB")
    achter = k.resize((W, H), Image.LANCZOS).filter(ImageFilter.GaussianBlur(40))
    pix = achter.load()
    for y in range(H):
        for x in range(W):
            r, g, b = pix[x, y]
            pix[x, y] = (r//3, g//3, b//3)
    kw, kh = k.size
    y_off = (H - kh) // 2
    achter.paste(k, (0, y_off))
    achter.save(uit_pad, quality=92)

def maak_voor_taal(taal):
    slides_dir = os.path.join(HIER, "slides") if taal == "nl" else os.path.join(HIER, "slides", "en")
    out_dir = VIDEO
    tmp_dir = os.path.join(VIDEO, "tmp", taal) if taal != "nl" else os.path.join(VIDEO, "tmp")
    os.makedirs(tmp_dir, exist_ok=True)
    os.makedirs(out_dir, exist_ok=True)

    naam_suffix = "" if taal == "nl" else "_en"
    final_uit = os.path.join(out_dir, f"jm_orgue_jm_rec_reel{naam_suffix}.mp4")

    # Stap 1: reel-achtergronden
    frames = []
    for naam in VOLGORDE:
        src = os.path.join(slides_dir, naam)
        if not os.path.exists(src):
            print(f"[{taal}] bronslide ontbreekt: {src}"); return
        dst = os.path.join(tmp_dir, naam.replace(".png", "_reel.jpg"))
        reel_achtergrond(src, dst)
        frames.append(dst)

    # Stap 2: per frame een korte video met zachte zoom
    stukjes = []
    for i, f in enumerate(frames):
        uit = os.path.join(tmp_dir, f"stuk{i:02d}.mp4")
        if i % 2 == 0:
            zoom = "zoom='min(zoom+0.0008,1.08)'"
        else:
            zoom = "zoom='if(lte(zoom,1.0),1.08,max(1.001,zoom-0.0008))'"
        cmd = [
            "ffmpeg", "-y", "-loop", "1", "-i", f,
            "-vf", f"scale=1080:1920:flags=lanczos,zoompan={zoom}:d=1:s=1080x1920:fps=30",
            "-t", str(SEC), "-r", "30",
            "-c:v", "libx264", "-pix_fmt", "yuv420p",
            "-preset", "medium", "-crf", "19",
            uit,
        ]
        print(f"[{taal}] stuk {i} …")
        r = subprocess.run(cmd, capture_output=True, text=True)
        if r.returncode != 0:
            print("ffmpeg stuk-fout:", r.stderr[-400:])
            raise SystemExit(1)
        stukjes.append(uit)

    # Stap 3: xfade-ketting tussen alle stukjes
    args = ["ffmpeg", "-y"]
    for s in stukjes:
        args += ["-i", s]
    offset = SEC - FADE
    delen = []
    laatste = "0:v"
    for i in range(1, len(stukjes)):
        label = f"vx{i}"
        off = (i)*offset
        delen.append(f"[{laatste}][{i}:v]xfade=transition=fade:duration={FADE}:offset={off:.3f}[{label}]")
        laatste = label
    fc = "; ".join(delen)
    args += ["-filter_complex", fc, "-map", f"[{laatste}]",
             "-c:v", "libx264", "-pix_fmt", "yuv420p",
             "-preset", "medium", "-crf", "19", "-r", "30",
             final_uit]
    print(f"[{taal}] stitch…")
    r = subprocess.run(args, capture_output=True, text=True)
    if r.returncode != 0:
        print("ffmpeg stitch-fout:", r.stderr[-600:])
        raise SystemExit(1)
    print(f"[{taal}] KLAAR: {final_uit}")

if __name__ == "__main__":
    talen = sys.argv[1:] or ["nl", "en"]
    for t in talen:
        maak_voor_taal(t)
