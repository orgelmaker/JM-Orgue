"""Maak Instagram-slides (1080x1350, carrousel) + reel-filmpje (1080x1920).

Vijf vierkante kaarten voor de carrousel:
 1. Titel: JM-Orgue + JM-Rec, met URL's
 2. JM-Orgue top (knippen + plakken uit screenshot)
 3. JM-Orgue vergelijkingstabel
 4. JM-Rec top
 5. JM-Rec waarom + link

Daarnaast: filmpje 15 s voor Reels (1080x1920) met zachte pan over de screenshots.
"""
import os, subprocess
from PIL import Image, ImageDraw, ImageFont, ImageFilter

HIER = os.path.dirname(os.path.abspath(__file__))
BRON = os.path.join(HIER, "bron")
SLIDES = os.path.join(HIER, "slides")
VIDEO = os.path.join(HIER, "video")
TMP = os.path.join(VIDEO, "tmp")
for d in (SLIDES, VIDEO, TMP): os.makedirs(d, exist_ok=True)

# Kleuren
GOUD = (217, 164, 60)       # JM-Orgue
MINT = (73, 203, 180)       # JM-Rec
ZWART = (12, 12, 15)
WIT = (245, 240, 232)
ZACHTWIT = (210, 205, 198)

# Font (Windows systeem)
def font(grootte, bold=False):
    paden = [
        r"C:\Windows\Fonts\segoeuib.ttf" if bold else r"C:\Windows\Fonts\segoeui.ttf",
        r"C:\Windows\Fonts\arial.ttf",
    ]
    for p in paden:
        if os.path.exists(p):
            return ImageFont.truetype(p, grootte)
    return ImageFont.load_default()

def titel_font(g):
    for p in (r"C:\Windows\Fonts\georgiab.ttf", r"C:\Windows\Fonts\georgia.ttf"):
        if os.path.exists(p):
            return ImageFont.truetype(p, g)
    return font(g, bold=True)

def tekst_in_breedte(d, tekst, maxbreedte, f):
    """Woordomslag: geef lijst regels terug."""
    woorden = tekst.split()
    regels, huidige = [], ""
    for w in woorden:
        proef = (huidige + " " + w).strip()
        bb = d.textbbox((0,0), proef, font=f)
        if bb[2]-bb[0] <= maxbreedte:
            huidige = proef
        else:
            if huidige: regels.append(huidige)
            huidige = w
    if huidige: regels.append(huidige)
    return regels

def kaart(w, h, achter=ZWART):
    img = Image.new("RGB", (w, h), achter)
    return img

def gradient(w, h, kleur1, kleur2):
    img = Image.new("RGB", (w, h), kleur1)
    for y in range(h):
        t = y / max(1, h-1)
        r = int(kleur1[0] + (kleur2[0]-kleur1[0]) * t)
        g = int(kleur1[1] + (kleur2[1]-kleur1[1]) * t)
        b = int(kleur1[2] + (kleur2[2]-kleur1[2]) * t)
        for x in range(w):
            img.putpixel((x, y), (r, g, b))
    return img

# Snelle verticale gradiënt met PIL (zonder NumPy)
def vert_gradient(w, h, k1, k2):
    rij = Image.new("RGB", (w, 1), k1)
    uit = Image.new("RGB", (w, h))
    for y in range(h):
        t = y / max(1, h-1)
        kl = tuple(int(k1[i] + (k2[i]-k1[i]) * t) for i in range(3))
        rij = Image.new("RGB", (w, 1), kl)
        uit.paste(rij, (0, y))
    return uit

W = 1080
H = 1350  # vierkant-portret carrousel-formaat (ratio 4:5)
VW = 1080
VH = 1920  # reel

def plak_gecentreerd(canvas, afb, bb=(0.08, 0.22, 0.92, 0.95)):
    """Plak afbeelding gecentreerd binnen genormaliseerde bb (links, boven, rechts, onder)."""
    cw, ch = canvas.size
    x0, y0, x1, y1 = int(bb[0]*cw), int(bb[1]*ch), int(bb[2]*cw), int(bb[3]*ch)
    vlak_w, vlak_h = x1-x0, y1-y0
    aw, ah = afb.size
    schaal = min(vlak_w/aw, vlak_h/ah)
    nw, nh = int(aw*schaal), int(ah*schaal)
    r = afb.resize((nw, nh), Image.LANCZOS)
    x = x0 + (vlak_w - nw)//2
    y = y0 + (vlak_h - nh)//2
    canvas.paste(r, (x, y))
    return (x, y, x+nw, y+nh)

def tekstkader(d, xy, tekst, f, kleur, uitlijn="midden", maxbreedte=None):
    x, y = xy
    regels = tekst_in_breedte(d, tekst, maxbreedte, f) if maxbreedte else [tekst]
    y_off = y
    for r in regels:
        bb = d.textbbox((0,0), r, font=f)
        bw = bb[2]-bb[0]; bh = bb[3]-bb[1]
        if uitlijn == "midden":
            d.text((x - bw//2, y_off), r, font=f, fill=kleur)
        elif uitlijn == "links":
            d.text((x, y_off), r, font=f, fill=kleur)
        y_off += bh + 10
    return y_off

# ---- SLIDE 1: titel ----
def slide_titel():
    img = vert_gradient(W, H, (18, 16, 20), (28, 24, 30))
    d = ImageDraw.Draw(img)
    # Boven: kleine streep
    d.rectangle([W//2-60, 140, W//2+60, 148], fill=GOUD)
    tekstkader(d, (W//2, 180), "JM-Orgue + JM-Rec", titel_font(78), WIT, "midden")
    tekstkader(d, (W//2, 290), "Een virtueel pijporgel voor thuis.", font(36), ZACHTWIT, "midden")
    tekstkader(d, (W//2, 350), "En een eigen opname-wizard erbij.", font(36), ZACHTWIT, "midden")

    # Twee kleurblokken naast elkaar
    kw, kh = 410, 540
    gx = (W - 2*kw - 60)//2
    gy = 450
    # JM-Orgue blok
    d.rounded_rectangle([gx, gy, gx+kw, gy+kh], radius=28, fill=(28, 23, 15), outline=GOUD, width=3)
    tekstkader(d, (gx+kw//2, gy+40), "JM-Orgue", titel_font(60), GOUD, "midden")
    tekstkader(d, (gx+kw//2, gy+130), "Virtueel pijporgel", font(28), WIT, "midden")
    tekstkader(d, (gx+kw//2, gy+170), "voor Windows • macOS • Linux", font(26), ZACHTWIT, "midden")
    tekstkader(d, (gx+kw//2, gy+230), "GrandOrgue- en", font(26), ZACHTWIT, "midden")
    tekstkader(d, (gx+kw//2, gy+265), "Hauptwerk-samplesets", font(26), ZACHTWIT, "midden")
    tekstkader(d, (gx+kw//2, gy+330), "7 talen · gratis", font(28), WIT, "midden")
    d.rectangle([gx+40, gy+kh-110, gx+kw-40, gy+kh-102], fill=GOUD)
    tekstkader(d, (gx+kw//2, gy+kh-80), "orgelmaker.github.io", font(24), WIT, "midden")
    tekstkader(d, (gx+kw//2, gy+kh-50), "/JM-Orgue", titel_font(28), GOUD, "midden")

    # JM-Rec blok
    gx2 = gx + kw + 60
    d.rounded_rectangle([gx2, gy, gx2+kw, gy+kh], radius=28, fill=(14, 25, 23), outline=MINT, width=3)
    tekstkader(d, (gx2+kw//2, gy+40), "JM-Rec", titel_font(60), MINT, "midden")
    tekstkader(d, (gx2+kw//2, gy+130), "Pijporgels samplen", font(28), WIT, "midden")
    tekstkader(d, (gx2+kw//2, gy+170), "zonder gedoe", font(26), ZACHTWIT, "midden")
    tekstkader(d, (gx2+kw//2, gy+230), "Draadloos bedienen", font(26), ZACHTWIT, "midden")
    tekstkader(d, (gx2+kw//2, gy+265), "via telefoon of tablet", font(26), ZACHTWIT, "midden")
    tekstkader(d, (gx2+kw//2, gy+330), "Direct in JM-Orgue", font(28), WIT, "midden")
    d.rectangle([gx2+40, gy+kh-110, gx2+kw-40, gy+kh-102], fill=MINT)
    tekstkader(d, (gx2+kw//2, gy+kh-80), "orgelmaker.github.io", font(24), WIT, "midden")
    tekstkader(d, (gx2+kw//2, gy+kh-50), "/JM-Rec", titel_font(28), MINT, "midden")

    # Voetnoot
    tekstkader(d, (W//2, H-110), "Twee programma's. Eén workflow.", font(32), ZACHTWIT, "midden")
    tekstkader(d, (W//2, H-60), "orgelmaker.github.io", font(26), GOUD, "midden")

    img.save(os.path.join(SLIDES, "slide1_titel.png"))
    return img

# ---- SLIDE 2+3: JM-Orgue screenshots in kaart ----
def slide_screenshot(bron_pad, naam, titel, sub, kleur, url):
    img = vert_gradient(W, H, (18, 16, 20), (28, 24, 30))
    d = ImageDraw.Draw(img)
    # Kop
    d.rectangle([W//2-50, 90, W//2+50, 97], fill=kleur)
    tekstkader(d, (W//2, 125), titel, titel_font(64), WIT, "midden")
    tekstkader(d, (W//2, 215), sub, font(32), ZACHTWIT, "midden")

    # Afbeelding
    try:
        afb = Image.open(bron_pad).convert("RGB")
        # Lichte afgeronde kader
        bb = plak_gecentreerd(img, afb, (0.07, 0.23, 0.93, 0.90))
        d.rounded_rectangle(bb, radius=18, outline=kleur, width=4)
    except Exception as e:
        print("Fout bij", bron_pad, e)

    # Voet URL
    tekstkader(d, (W//2, H-80), url, font(32), kleur, "midden")
    img.save(os.path.join(SLIDES, naam))

# ---- SLIDE 5: afsluiten ----
def slide_slot():
    img = vert_gradient(W, H, (18, 16, 20), (28, 24, 30))
    d = ImageDraw.Draw(img)
    tekstkader(d, (W//2, 320), "Probeer het.", titel_font(96), WIT, "midden")
    tekstkader(d, (W//2, 460), "Zonder dongle. Zonder abonnement.", font(34), ZACHTWIT, "midden")
    tekstkader(d, (W//2, 520), "Gewoon downloaden.", font(34), ZACHTWIT, "midden")

    # Twee URL-knoppen
    kw, kh = 760, 110
    kx = (W-kw)//2
    gy1 = 700
    d.rounded_rectangle([kx, gy1, kx+kw, gy1+kh], radius=22, fill=(28, 23, 15), outline=GOUD, width=3)
    tekstkader(d, (W//2, gy1+16), "JM-Orgue", font(28), GOUD, "midden")
    tekstkader(d, (W//2, gy1+56), "orgelmaker.github.io/JM-Orgue", titel_font(32), WIT, "midden")
    gy2 = gy1 + kh + 40
    d.rounded_rectangle([kx, gy2, kx+kw, gy2+kh], radius=22, fill=(14, 25, 23), outline=MINT, width=3)
    tekstkader(d, (W//2, gy2+16), "JM-Rec", font(28), MINT, "midden")
    tekstkader(d, (W//2, gy2+56), "orgelmaker.github.io/JM-Rec", titel_font(32), WIT, "midden")

    tekstkader(d, (W//2, H-150), "#virtueelpijporgel #hauptwerk #grandorgue", font(24), ZACHTWIT, "midden")
    tekstkader(d, (W//2, H-110), "#kerkorgel #orgelmuziek", font(24), ZACHTWIT, "midden")

    img.save(os.path.join(SLIDES, "slide5_slot.png"))

if __name__ == "__main__":
    slide_titel()
    slide_screenshot(os.path.join(BRON, "jm-orgue-site-top.jpg"),
                     "slide2_jmorgue.png",
                     "JM-Orgue", "Een virtueel pijporgel voor thuis.", GOUD,
                     "orgelmaker.github.io/JM-Orgue")
    slide_screenshot(os.path.join(BRON, "jm-orgue-site-vergelijking.jpg"),
                     "slide3_vergelijking.png",
                     "Dezelfde mogelijkheden", "Alleen zonder dongle en zonder abonnement.", GOUD,
                     "orgelmaker.github.io/JM-Orgue")
    slide_screenshot(os.path.join(BRON, "jm-rec-site-top.jpg"),
                     "slide4_jmrec.png",
                     "JM-Rec", "Pijporgels samplen, zonder gedoe.", MINT,
                     "orgelmaker.github.io/JM-Rec")
    slide_screenshot(os.path.join(BRON, "jm-rec-waarom.jpg"),
                     "slide5a_jmrec_waarom.png",
                     "Een echte wizard", "Draadloos bedienen. Direct klaar voor JM-Orgue.", MINT,
                     "orgelmaker.github.io/JM-Rec")
    slide_slot()
    print("klaar met slides:", sorted(os.listdir(SLIDES)))
