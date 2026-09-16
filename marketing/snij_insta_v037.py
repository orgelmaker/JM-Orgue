# Instagram-uitsneden voor post v037 (acht slides).
#
# Alle slides op hetzelfde vierkante carrouselformaat (1:1 = 1080x1080).
# Instagram snijdt een carrousel bij op de verhouding van de eerste slide, dus
# gelijke verhoudingen voorkomt dat de rest wordt afgekapt. Vierkant i.p.v.
# staand: de schermafbeeldingen zijn breed, en in een staand vlak wordt het
# beeld geen millimeter groter — er komt alleen lege ruimte bij.
#
# Per slide: vensterrand en titelbalk eraf, een korte kop in Georgia (het
# lettertype dat de app voor registernamen gebruikt), en daaronder de
# schermafbeelding zo groot mogelijk, met dunne gouden rand en zachte schaduw
# op de crèmekleurige achtergrond van de app.
#
# Draaien vanuit de repo-root:  python marketing/snij_insta_v037.py
from PIL import Image, ImageFilter, ImageDraw, ImageFont

MAP = r'C:\Bronbestanden\JM-Orgue\screenshots\instagram_v037'
ZIJDE = 1080
MARGE = 30
ACHTERGROND = (240, 235, 224)   # #f0ebe0 — hoofdachtergrond van de app
RAND = (176, 152, 86)           # gedempt goud, zoals de divisiebalken
INKT = (43, 36, 26)
INKT_SUB = (122, 105, 74)
SCHADUW = (120, 104, 70, 70)

KOP = ImageFont.truetype(r'C:\Windows\Fonts\georgiab.ttf', 50)
SUB = ImageFont.truetype(r'C:\Windows\Fonts\georgia.ttf', 27)

# (uitvoernaam, bronbestand, uitsnede, kop, onderschrift)
SLIDES = [
    # Vier kaarten i.p.v. zes: dan zijn de orgelnamen op een telefoon leesbaar.
    ('slide1_bibliotheek', 'bron_bibliotheek', (8, 32, 807, 378),
     'De bibliotheek',
     'Elk orgel met foto, orgelbouwer en plaats \u2014 in zeven talen'),
    ('slide2_speeltafel', 'bron_speeltafel', (8, 32, 1408, 932),
     'De speeltafel',
     'Friesach: 44 registers, vier werken, koppels en setzer'),
    # Werkbalk meenemen: die laat het bewerken en exporteren zien.
    ('slide3_notatie', 'bron_notatie', (0, 8, 1260, 950),
     'Bladmuziek uit je eigen spel',
     'Elk werk op zijn eigen balk \u2014 terugspelen, opslaan als PDF of MIDI'),
    # Chrome-titelbalk en schuifbalk eraf: oogt als een telefoonscherm.
    ('slide4_telefoon', 'bron_telefoon', (2, 31, 404, 877),
     'Je telefoon als registreerscherm',
     'Dezelfde registratie, live in je eigen wifi-netwerk'),
    # Alleen de linkerkolom (master, stemming, nagalm): op vol formaat is de
    # regel 'gemeten pijptoonhoogtes (2.392 van 2.392 pijpen)' leesbaar.
    ('slide5_stemming', 'bron_orgelinstellingen', (28, 100, 610, 754),
     'Stemming en klank',
     'Origineel zoals opgenomen \u2014 of een historisch temperament'),
    # Rechterkolom, het blok van één klavier.
    ('slide6_klavieren', 'bron_orgelinstellingen', (660, 355, 1215, 732),
     'Elk klavier zijn eigen weg',
     'Eigen MIDI-kanaal, eigen zwelkast, eigen luidsprekers'),
    ('slide7_console', 'bron_console_midi', (28, 348, 612, 878),
     'Je eigen speeltafel praat mee',
     'Registerlampen, displays en pistons van je eigen console'),
    ('slide8_talen', 'bron_talen_audio', (8, 32, 1408, 932),
     'Voor iedereen',
     'Zeven talen, eigen kleuren, en bijwerken met één klik'),
]


def gecentreerd(tek, tekst, y, font, kleur):
    b = tek.textbbox((0, 0), tekst, font=font)
    tek.text(((ZIJDE - (b[2] - b[0])) // 2 - b[0], y), tekst, font=font, fill=kleur)
    return b[3] - b[1]


def maak(naam, bron, vak, kop, sub):
    beeld = Image.open(f'{MAP}\\{bron}.png').convert('RGB').crop(vak)
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

    doek.save(f'{MAP}\\{naam}_vierkant.png')
    return (bb, bh), (nb, nh), round(schaal, 2)


for naam, bron, vak, kop, sub in SLIDES:
    bron_fmt, nieuw_fmt, s = maak(naam, bron, vak, kop, sub)
    print(f'{naam:22s} {bron_fmt[0]:>4}x{bron_fmt[1]:<4} -> {nieuw_fmt[0]:>4}x{nieuw_fmt[1]:<4} '
          f'(x{s}, beeld vult {round(100 * nieuw_fmt[1] / ZIJDE):>2}% van de hoogte)')
print(f'\n{len(SLIDES)} slides van {ZIJDE}x{ZIJDE} (1:1)')
