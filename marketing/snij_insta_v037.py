# Instagram-uitsneden voor post v037.
#
# Alle slides op hetzelfde vierkante carrouselformaat (1:1 = 1080x1080).
# Instagram snijdt een carrousel bij op de verhouding van de eerste slide, dus
# gelijke verhoudingen voorkomt dat de rest wordt afgekapt. Vierkant i.p.v.
# staand: de schermafbeeldingen zijn breed, en in een staand vlak wordt het
# beeld geen millimeter groter — er komt alleen lege ruimte bij.
#
# Per slide: vensterrand en Windows-titelbalk eraf, een korte kop in Georgia
# (het lettertype dat de app voor registernamen gebruikt), en daaronder de
# schermafbeelding zo groot mogelijk, met dunne gouden rand en zachte schaduw
# op de crèmekleurige achtergrond van de app.
from PIL import Image, ImageFilter, ImageDraw, ImageFont

BRON = r'C:\Bronbestanden\JM-Orgue\screenshots\instagram_v037'
ZIJDE = 1080
MARGE = 30
ACHTERGROND = (240, 235, 224)   # #f0ebe0 — hoofdachtergrond van de app
RAND = (176, 152, 86)           # gedempt goud, zoals de divisiebalken
INKT = (43, 36, 26)
INKT_SUB = (122, 105, 74)
SCHADUW = (120, 104, 70, 70)

KOP = ImageFont.truetype(r'C:\Windows\Fonts\georgiab.ttf', 50)
SUB = ImageFont.truetype(r'C:\Windows\Fonts\georgia.ttf', 27)

# (bestand, uitsnede uit de bron, kop, onderschrift)
SLIDES = [
    # Vier kaarten i.p.v. zes: dan zijn de orgelnamen op een telefoon leesbaar.
    ('slide1_bibliotheek', (8, 32, 807, 378),
     'De bibliotheek',
     'Elk orgel met foto, orgelbouwer en plaats \u2014 in zeven talen'),
    ('slide2_speeltafel', (8, 32, 1408, 932),
     'De speeltafel',
     'Friesach: 44 registers, vier werken, koppels en setzer'),
    # Chrome-titelbalk en vensterrand eraf: oogt als een telefoonscherm.
    ('slide3_afstandsbediening', (2, 31, 404, 877),
     'Je telefoon als registreerscherm',
     'Dezelfde registratie, live in je eigen wifi-netwerk'),
    # Alleen de linkerkolom (master, stemming, nagalm): op vol formaat is de
    # regel 'gemeten pijptoonhoogtes (2.392 van 2.392 pijpen)' leesbaar.
    ('slide4_stemming_klank', (28, 100, 610, 754),
     'Stemming en klank',
     'Origineel zoals opgenomen \u2014 of elk temperament, per pijp gemeten'),
    ('slide5_talen_audio', (8, 32, 1408, 932),
     'Voor iedereen',
     'Zeven talen, eigen kleuren en je hele geluidskaart'),
]


def gecentreerd(tek, tekst, y, font, kleur):
    b = tek.textbbox((0, 0), tekst, font=font)
    tek.text(((ZIJDE - (b[2] - b[0])) // 2 - b[0], y), tekst, font=font, fill=kleur)
    return b[3] - b[1]


def maak(naam, vak, kop, sub):
    beeld = Image.open(f'{BRON}\\{naam}.png').convert('RGB').crop(vak)
    bb, bh = beeld.size

    doek = Image.new('RGB', (ZIJDE, ZIJDE), ACHTERGROND)
    tek = ImageDraw.Draw(doek)
    y = 46
    y += gecentreerd(tek, kop, y, KOP, INKT) + 22
    y += gecentreerd(tek, sub, y, SUB, INKT_SUB) + 30
    tek.line([(ZIJDE // 2 - 80, y), (ZIJDE // 2 + 80, y)], fill=RAND, width=3)
    y += 34

    ruimte_b, ruimte_h = ZIJDE - 2 * MARGE, ZIJDE - y - MARGE
    schaal = min(ruimte_b / bb, ruimte_h / bh, 2.0)
    nb, nh = int(bb * schaal), int(bh * schaal)
    beeld = beeld.resize((nb, nh), Image.LANCZOS)

    x = (ZIJDE - nb) // 2
    yb = y + int(0.42 * (ruimte_h - nh))   # iets boven het midden oogt rustiger

    schaduw = Image.new('RGBA', (ZIJDE, ZIJDE), (0, 0, 0, 0))
    ImageDraw.Draw(schaduw).rectangle([x - 6, yb - 4, x + nb + 6, yb + nh + 10], fill=SCHADUW)
    doek = Image.alpha_composite(doek.convert('RGBA'), schaduw.filter(ImageFilter.GaussianBlur(14))).convert('RGB')

    doek.paste(beeld, (x, yb))
    ImageDraw.Draw(doek).rectangle([x - 1, yb - 1, x + nb, yb + nh], outline=RAND, width=2)

    doek.save(f'{BRON}\\{naam}_vierkant.png')
    return (bb, bh), (nb, nh), round(schaal, 2)


for naam, vak, kop, sub in SLIDES:
    bron_fmt, nieuw_fmt, s = maak(naam, vak, kop, sub)
    print(f'{naam:28s} {bron_fmt[0]}x{bron_fmt[1]} -> {nieuw_fmt[0]}x{nieuw_fmt[1]} '
          f'(x{s}, beeld vult {round(100 * nieuw_fmt[1] / ZIJDE)}% van de hoogte)')
print(f'\nAlle slides: {ZIJDE}x{ZIJDE} (1:1)')
