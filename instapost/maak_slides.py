"""Maak Instagram-slides (1080x1350, carrousel) in NL en EN.

Vijf vierkante kaarten voor de carrousel per taal:
 1. Titel: JM-Orgue + JM-Rec, met URL's
 2. JM-Orgue top (knippen + plakken uit screenshot)
 3. JM-Orgue vergelijkingstabel
 4. JM-Rec top (screenshot)
 5. "Waarom JM-Rec" + voordelen (screenshot)
 6. Eindkaart met beide links

Draai zonder argumenten voor beide talen. Draai met "nl" of "en" voor één taal.

De bron-screenshots staan in bron/ (NL) en bron/en/ (EN).
Resultaat in slides/ (NL) en slides/en/ (EN).
"""
import os, sys
from PIL import Image, ImageDraw, ImageFont

HIER = os.path.dirname(os.path.abspath(__file__))

# Kleuren
GOUD = (217, 164, 60)       # JM-Orgue
MINT = (73, 203, 180)       # JM-Rec
WIT = (245, 240, 232)
ZACHTWIT = (210, 205, 198)

W = 1080
H = 1350  # vierkant-portret carrousel-formaat (ratio 4:5)

# ---- fonts ----

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

# ---- vertalingen ----

TEKST = {
    "nl": {
        "titel": "JM-Orgue + JM-Rec",
        "sub1":  "Een virtueel pijporgel voor thuis.",
        "sub2":  "En een eigen opname-wizard erbij.",
        "org_titel":    "JM-Orgue",
        "org_regel1":   "Virtueel pijporgel",
        "org_regel2":   "voor Windows • macOS • Linux",
        "org_regel3":   "GrandOrgue- en",
        "org_regel4":   "Hauptwerk-samplesets",
        "org_regel5":   "7 talen · gratis",
        "org_url":      "orgelmaker.github.io",
        "org_pad":      "/JM-Orgue",
        "rec_titel":    "JM-Rec",
        "rec_regel1":   "Pijporgels samplen",
        "rec_regel2":   "zonder gedoe",
        "rec_regel3":   "Draadloos bedienen",
        "rec_regel4":   "via telefoon of tablet",
        "rec_regel5":   "Direct in JM-Orgue",
        "rec_url":      "orgelmaker.github.io",
        "rec_pad":      "/JM-Rec",
        "voet":         "Twee programma's. Eén workflow.",
        "voet_url":     "orgelmaker.github.io",

        "s2_titel":  "JM-Orgue",
        "s2_sub":    "Een virtueel pijporgel voor thuis.",
        "s2_url":    "orgelmaker.github.io/JM-Orgue",

        "s3_titel":  "Dezelfde mogelijkheden",
        "s3_sub":    "Alleen zonder dongle en zonder abonnement.",
        "s3_url":    "orgelmaker.github.io/JM-Orgue",

        "s4_titel":  "JM-Rec",
        "s4_sub":    "Pijporgels samplen, zonder gedoe.",
        "s4_url":    "orgelmaker.github.io/JM-Rec",

        "s5_titel":  "Een echte wizard",
        "s5_sub":    "Draadloos bedienen. Direct klaar voor JM-Orgue.",
        "s5_url":    "orgelmaker.github.io/JM-Rec",

        "slot_titel":   "Probeer het.",
        "slot_sub1":    "Zonder dongle. Zonder abonnement.",
        "slot_sub2":    "Gewoon downloaden.",
        "tags":         "#virtueelpijporgel #hauptwerk #grandorgue",
        "tags2":        "#kerkorgel #orgelmuziek",
    },
    "en": {
        "titel": "JM-Orgue + JM-Rec",
        "sub1":  "A virtual pipe organ for home.",
        "sub2":  "And its own recording wizard.",
        "org_titel":    "JM-Orgue",
        "org_regel1":   "Virtual pipe organ",
        "org_regel2":   "for Windows • macOS • Linux",
        "org_regel3":   "GrandOrgue and",
        "org_regel4":   "Hauptwerk sample sets",
        "org_regel5":   "7 languages · free",
        "org_url":      "orgelmaker.github.io",
        "org_pad":      "/JM-Orgue",
        "rec_titel":    "JM-Rec",
        "rec_regel1":   "Sample pipe organs",
        "rec_regel2":   "no fuss",
        "rec_regel3":   "Wireless control",
        "rec_regel4":   "from phone or tablet",
        "rec_regel5":   "Opens straight in JM-Orgue",
        "rec_url":      "orgelmaker.github.io",
        "rec_pad":      "/JM-Rec",
        "voet":         "Two programs. One workflow.",
        "voet_url":     "orgelmaker.github.io",

        "s2_titel":  "JM-Orgue",
        "s2_sub":    "A virtual pipe organ for home.",
        "s2_url":    "orgelmaker.github.io/JM-Orgue",

        "s3_titel":  "Same capabilities",
        "s3_sub":    "Just without a dongle or a subscription.",
        "s3_url":    "orgelmaker.github.io/JM-Orgue",

        "s4_titel":  "JM-Rec",
        "s4_sub":    "Sample pipe organs, no fuss.",
        "s4_url":    "orgelmaker.github.io/JM-Rec",

        "s5_titel":  "A real wizard",
        "s5_sub":    "Wireless control. Opens straight in JM-Orgue.",
        "s5_url":    "orgelmaker.github.io/JM-Rec",

        "slot_titel":   "Give it a try.",
        "slot_sub1":    "No dongle. No subscription.",
        "slot_sub2":    "Just download it.",
        "tags":         "#virtualpipeorgan #hauptwerk #grandorgue",
        "tags2":        "#churchorgan #organmusic",
    },
}

# ---- tekenprimitieven ----

def tekst_in_breedte(d, tekst, maxbreedte, f):
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

def vert_gradient(w, h, k1, k2):
    uit = Image.new("RGB", (w, h))
    for y in range(h):
        t = y / max(1, h-1)
        kl = tuple(int(k1[i] + (k2[i]-k1[i]) * t) for i in range(3))
        rij = Image.new("RGB", (w, 1), kl)
        uit.paste(rij, (0, y))
    return uit

def plak_gecentreerd(canvas, afb, bb=(0.08, 0.22, 0.92, 0.95)):
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

# ---- slide-fabrikant per taal ----

def slide_titel(T, uitpad):
    img = vert_gradient(W, H, (18, 16, 20), (28, 24, 30))
    d = ImageDraw.Draw(img)
    d.rectangle([W//2-60, 140, W//2+60, 148], fill=GOUD)
    tekstkader(d, (W//2, 180), T["titel"], titel_font(78), WIT, "midden")
    tekstkader(d, (W//2, 290), T["sub1"], font(36), ZACHTWIT, "midden")
    tekstkader(d, (W//2, 350), T["sub2"], font(36), ZACHTWIT, "midden")

    kw, kh = 410, 540
    gx = (W - 2*kw - 60)//2
    gy = 450
    d.rounded_rectangle([gx, gy, gx+kw, gy+kh], radius=28, fill=(28, 23, 15), outline=GOUD, width=3)
    tekstkader(d, (gx+kw//2, gy+40), T["org_titel"], titel_font(60), GOUD, "midden")
    tekstkader(d, (gx+kw//2, gy+130), T["org_regel1"], font(28), WIT, "midden")
    tekstkader(d, (gx+kw//2, gy+170), T["org_regel2"], font(26), ZACHTWIT, "midden")
    tekstkader(d, (gx+kw//2, gy+230), T["org_regel3"], font(26), ZACHTWIT, "midden")
    tekstkader(d, (gx+kw//2, gy+265), T["org_regel4"], font(26), ZACHTWIT, "midden")
    tekstkader(d, (gx+kw//2, gy+330), T["org_regel5"], font(28), WIT, "midden")
    d.rectangle([gx+40, gy+kh-110, gx+kw-40, gy+kh-102], fill=GOUD)
    tekstkader(d, (gx+kw//2, gy+kh-80), T["org_url"], font(24), WIT, "midden")
    tekstkader(d, (gx+kw//2, gy+kh-50), T["org_pad"], titel_font(28), GOUD, "midden")

    gx2 = gx + kw + 60
    d.rounded_rectangle([gx2, gy, gx2+kw, gy+kh], radius=28, fill=(14, 25, 23), outline=MINT, width=3)
    tekstkader(d, (gx2+kw//2, gy+40), T["rec_titel"], titel_font(60), MINT, "midden")
    tekstkader(d, (gx2+kw//2, gy+130), T["rec_regel1"], font(28), WIT, "midden")
    tekstkader(d, (gx2+kw//2, gy+170), T["rec_regel2"], font(26), ZACHTWIT, "midden")
    tekstkader(d, (gx2+kw//2, gy+230), T["rec_regel3"], font(26), ZACHTWIT, "midden")
    tekstkader(d, (gx2+kw//2, gy+265), T["rec_regel4"], font(26), ZACHTWIT, "midden")
    tekstkader(d, (gx2+kw//2, gy+330), T["rec_regel5"], font(28), WIT, "midden")
    d.rectangle([gx2+40, gy+kh-110, gx2+kw-40, gy+kh-102], fill=MINT)
    tekstkader(d, (gx2+kw//2, gy+kh-80), T["rec_url"], font(24), WIT, "midden")
    tekstkader(d, (gx2+kw//2, gy+kh-50), T["rec_pad"], titel_font(28), MINT, "midden")

    tekstkader(d, (W//2, H-110), T["voet"], font(32), ZACHTWIT, "midden")
    tekstkader(d, (W//2, H-60), T["voet_url"], font(26), GOUD, "midden")

    img.save(uitpad)

def slide_screenshot(bron_pad, uitpad, titel, sub, kleur, url):
    img = vert_gradient(W, H, (18, 16, 20), (28, 24, 30))
    d = ImageDraw.Draw(img)
    d.rectangle([W//2-50, 90, W//2+50, 97], fill=kleur)
    tekstkader(d, (W//2, 125), titel, titel_font(64), WIT, "midden")
    tekstkader(d, (W//2, 215), sub, font(32), ZACHTWIT, "midden")

    try:
        afb = Image.open(bron_pad).convert("RGB")
        bb = plak_gecentreerd(img, afb, (0.07, 0.23, 0.93, 0.90))
        d.rounded_rectangle(bb, radius=18, outline=kleur, width=4)
    except Exception as e:
        print("Fout bij", bron_pad, e)

    tekstkader(d, (W//2, H-80), url, font(32), kleur, "midden")
    img.save(uitpad)

def slide_slot(T, uitpad):
    img = vert_gradient(W, H, (18, 16, 20), (28, 24, 30))
    d = ImageDraw.Draw(img)
    tekstkader(d, (W//2, 320), T["slot_titel"], titel_font(96), WIT, "midden")
    tekstkader(d, (W//2, 460), T["slot_sub1"], font(34), ZACHTWIT, "midden")
    tekstkader(d, (W//2, 520), T["slot_sub2"], font(34), ZACHTWIT, "midden")

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

    tekstkader(d, (W//2, H-150), T["tags"], font(24), ZACHTWIT, "midden")
    tekstkader(d, (W//2, H-110), T["tags2"], font(24), ZACHTWIT, "midden")

    img.save(uitpad)

def maak_voor_taal(taal):
    T = TEKST[taal]
    bron = os.path.join(HIER, "bron") if taal == "nl" else os.path.join(HIER, "bron", "en")
    slides = os.path.join(HIER, "slides") if taal == "nl" else os.path.join(HIER, "slides", "en")
    os.makedirs(slides, exist_ok=True)

    slide_titel(T, os.path.join(slides, "slide1_titel.png"))
    slide_screenshot(os.path.join(bron, "jm-orgue-site-top.jpg"),
                     os.path.join(slides, "slide2_jmorgue.png"),
                     T["s2_titel"], T["s2_sub"], GOUD, T["s2_url"])
    slide_screenshot(os.path.join(bron, "jm-orgue-site-vergelijking.jpg"),
                     os.path.join(slides, "slide3_vergelijking.png"),
                     T["s3_titel"], T["s3_sub"], GOUD, T["s3_url"])
    slide_screenshot(os.path.join(bron, "jm-rec-site-top.jpg"),
                     os.path.join(slides, "slide4_jmrec.png"),
                     T["s4_titel"], T["s4_sub"], MINT, T["s4_url"])
    slide_screenshot(os.path.join(bron, "jm-rec-waarom.jpg"),
                     os.path.join(slides, "slide5a_jmrec_waarom.png"),
                     T["s5_titel"], T["s5_sub"], MINT, T["s5_url"])
    slide_slot(T, os.path.join(slides, "slide5_slot.png"))
    print(f"[{taal}] klaar — {len(os.listdir(slides))} slides in {slides}")

if __name__ == "__main__":
    talen = sys.argv[1:] or ["nl", "en"]
    for t in talen:
        if t not in TEKST:
            print(f"onbekende taal: {t}; keuzes: {list(TEKST)}")
            continue
        maak_voor_taal(t)
