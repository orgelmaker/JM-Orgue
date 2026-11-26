"""Maakt de ingebouwde Consolebouwer-texturen (ui/src/assets/textures/*.jpg).

- grain, oak, ebony: grijswaarde-versie van CC0-houtfoto's van ambientCG (Wood027, Wood058, Wood051),
  1K Color-kaart uit de zip, contrast uitgerekt.
- burl, linen, brushed: procedureel (tegelbare FFT-ruis), geen externe bron.
De afbeeldingen worden in de app over de themakleur gelegd (mix-blend-mode), daarom grijs.

Gebruik: python maak_consoletexturen.py <map-met-Wood027_1K-JPG.zip,...> [uitmap]
"""
import io, os, sys, zipfile
import numpy as np
from PIL import Image, ImageOps, ImageFilter

BRON = sys.argv[1]
UIT = sys.argv[2] if len(sys.argv) > 2 else os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "ui", "src", "assets", "textures")
os.makedirs(UIT, exist_ok=True)
N = 512


def foto(code, draai=False):
    z = zipfile.ZipFile(os.path.join(BRON, f"{code}_1K-JPG.zip"))
    im = Image.open(io.BytesIO(z.read(f"{code}_1K-JPG_Color.jpg"))).convert("L")
    if draai: im = im.transpose(Image.ROTATE_90)       # nerf horizontaal
    return im


def norm(im, cut=1.5, mid=128, spreiding=1.0):
    """Grijs rond het midden (zodat overlay de themakleur behoudt), contrast uitgerekt."""
    im = ImageOps.autocontrast(im, cutoff=cut)
    a = np.asarray(im, np.float32)
    a = (a - a.mean()) * spreiding + mid
    return Image.fromarray(np.clip(a, 0, 255).astype(np.uint8))


def opslaan(im, naam):
    im = im.resize((N, N), Image.LANCZOS)
    im.save(os.path.join(UIT, naam + ".jpg"), quality=86, optimize=True, progressive=True)
    print(naam, os.path.getsize(os.path.join(UIT, naam + ".jpg")) // 1024, "KB")


def ruis(sx, sy, seed, n=N):
    r = np.random.default_rng(seed)
    f = np.fft.fft2(r.standard_normal((n, n)))
    fx = np.fft.fftfreq(n)[None, :]; fy = np.fft.fftfreq(n)[:, None]
    a = np.real(np.fft.ifft2(f * np.exp(-((fx * sx) ** 2 + (fy * sy) ** 2) * 6.0)))
    return (a - a.mean()) / (a.std() + 1e-9)


def naar_img(a, spreiding=42):
    return Image.fromarray(np.clip(128 + a * spreiding, 0, 255).astype(np.uint8))


# houtfoto's
opslaan(norm(foto("Wood027"), cut=0.3, spreiding=1.25), "grain")
opslaan(norm(foto("Wood058", draai=True), cut=0.3, spreiding=1.15), "oak")
opslaan(norm(foto("Wood051"), cut=0.3, spreiding=0.8), "ebony")

# wortelnoot (vlammig, grillig)
y = np.arange(N)[:, None] / N
warp = ruis(120, 40, 51) * 1.8 + ruis(50, 14, 52) * 0.6
ring = np.abs(np.sin(((y * 6 + warp * 0.16) % 1.0) * np.pi)) ** 1.5
burl = 0.40 * ring + 0.35 * (ruis(260, 2.5, 53) * .5 + .5) + 0.30 * (ruis(60, 1.6, 54) * .5 + .5) + 0.10 * ruis(260, 260, 55)
burl = (burl - burl.mean()) / burl.std()
opslaan(naar_img(burl, 30), "burl")

# linnen: weefsel met slubs (kettingdraden verticaal, inslag horizontaal)
x = np.arange(N)[None, :]; yy = np.arange(N)[:, None]
draad = 8
kett = 0.5 + 0.5 * np.sin(2 * np.pi * x / draad) * (1 + 0.35 * ruis(3, 90, 61))
inslag = 0.5 + 0.5 * np.sin(2 * np.pi * yy / draad) * (1 + 0.35 * ruis(90, 3, 62))
kruis = ((x // (draad // 2) + yy // (draad // 2)) % 2)
lin = np.where(kruis == 0, kett, inslag) + 0.45 * ruis(1.5, 1.5, 63) + 0.35 * ruis(60, 3, 64) + 0.35 * ruis(3, 60, 65)
lin = (lin - lin.mean()) / lin.std()
opslaan(naar_img(lin, 26), "linen")

# geborsteld metaal: fijne horizontale strepen + zachte glans
st = ruis(240, 1.2, 71) * 1.0 + ruis(120, 0.8, 72) * 0.6 + ruis(500, 2.5, 73) * 0.45
st = (st - st.mean()) / st.std()
opslaan(naar_img(st, 28), "brushed")
