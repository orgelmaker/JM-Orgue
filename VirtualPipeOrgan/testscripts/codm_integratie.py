"""CODM-integratieproef: parse -> preflight -> sampleresolutie -> preload -> echte audio-engine -> NoteOff -> verslag.

Maakt zelf kleine CODM-proeforgels (sinussen, geen commerciele samples), start de app met
de test-API en controleert met opnames dat de engine doet wat de definitie zegt:
  1. importverslag / preflight (optioneel, met testen/codm/pitea-minimal als aanwezig)
  2. releases: kort/middel/lang op de toetsduurgrenzen, eerste cue-marker, losse release
  3. koppels: richting, 16'/8'/4', Unison Off, bas, melodie, rand, live wisselen
  4. schakelaars: DefaultToEngaged, buiten de combinaties, pistons
  5. willekeurige stemafwijking (aan/uit, tegen de voorspelde waarde)
  6. chimes: een WAV voor 37 toetsen op de juiste toonhoogte (optioneel, pitea-minimal)

Vereist: ffmpeg + numpy, een gebouwde app (target/release/vpo-app) en een audio-uitvoer.
Gebruik:  python codm_integratie.py [uitvoermap]
Afsluitcode 0 = alles geslaagd. Wat niet kan (ontbrekende optionele fixture) wordt met reden
overgeslagen en zo gemeld.
"""
import math, os, shutil, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import numpy as np
from codm_meetlib import (App, Resultaat, REPO, alle_stops, laad, opname, req, wacht_tot_stil, pcm)
import maak_codm_koppeltest, maak_codm_releasetest

R = Resultaat()
UIT = os.path.abspath(sys.argv[1]) if len(sys.argv) > 1 else os.path.join(REPO, 'testen', 'codm-integratie')
TMP = os.path.join(UIT, 'opname.mp3')


def pad(*d):
    return os.path.join(UIT, *d).replace('\\', '/')


def freq(n):
    return 440.0 * 2 ** ((n - 69) / 12.0)


# ----------------------------------------------------------------------------- 1. verslag
def verslag():
    mini = os.path.join(REPO, 'testen', 'codm', 'pitea-minimal', 'OrganDefinitions', 'Les-Pitea895-945.CustomOrgan_Hauptwerk_xml')
    if not os.path.exists(mini):
        R.overslaan('importverslag Piteå: testen/codm/pitea-minimal ontbreekt (optionele fixture)')
        return
    org = laad(pad('koppel', 'OrganDefinitions', 'Koppel.CustomOrgan_Hauptwerk_xml'))
    voor = sum(len(d['stops']) for d in org['divisions'])
    rep = req('/import/preflight', 'POST', {'path': mini.replace('\\', '/')})
    R.ok(rep['verwacht']['stops'] == 59 and rep['geladen']['stops'] == 1 and rep['onvolledig'] and not rep['niet_speelbaar'],
         f"Piteå met alleen pakket 537000: {rep['geladen']['stops']} van {rep['verwacht']['stops']} speelbaar, onvolledig")
    ontbr = {p['id']: p['naam'] for p in rep['pakketten'] if not p['gevonden']}
    R.ok(758 in ontbr and 759 in ontbr and ontbr[758] and ontbr[759], f'ontbrekende pakketten met naam: {ontbr}')
    na = req('/organ')
    R.ok(sum(len(d['stops']) for d in na['divisions']) == voor, 'preflight liet het huidige orgel ongemoeid')
    kapot = pad('kapot.CustomOrgan_Hauptwerk_xml')
    open(kapot, 'w', encoding='utf-8').write('<Hauptwerk FileFormat="CustomOrgan" FileFormatVersion="3.00"><ObjectList ObjectType="stop"><Stop>')
    rep = req('/import/preflight', 'POST', {'path': kapot})
    R.ok(rep['niet_speelbaar'] and any('geldige XML' in m['tekst'] for m in rep['meldingen']), 'kapot bestand: eigen begrijpelijke reden')
    try:
        req('/load_organ', 'POST', {'path': kapot})
        R.ok(False, 'laden van een kapot bestand had moeten falen')
    except Exception:
        R.ok(True, 'laden van een kapot bestand faalt netjes')
    na = req('/organ')
    R.ok(sum(len(d['stops']) for d in na['divisions']) == voor, 'vorig orgel blijft staan na een mislukte load')


# ----------------------------------------------------------------------------- 2. releases
def release_freqs(t_off, kandidaten):
    x = pcm(TMP)
    gezien = set()
    stap = int(0.05 * 48000)
    for i in range(int(max(0, t_off - 0.05) * 48000), min(len(x) - stap, int((t_off + 1.4) * 48000)), stap):
        seg = x[i:i + stap]
        if np.sqrt(np.mean(seg ** 2)) < 30:
            continue
        sp = np.abs(np.fft.rfft(seg * np.hanning(len(seg))))
        fr = np.fft.rfftfreq(len(seg), 1 / 48000)
        b = (fr > 300) & (fr < 3000)
        f = fr[b][np.argmax(sp[b])]
        for k in kandidaten:
            if abs(f - k) < 40:
                gezien.add(k)
    return gezien


def releases():
    org = laad(pad('release', 'OrganDefinitions', 'Mini.CustomOrgan_Hauptwerk_xml'))
    stops = alle_stops(org)
    sid = lambda s: [v for k, v in stops.items() if s in k][0]
    kand = [600, 900, 1300, 700, 1400]

    def proef(naam, houd_ms):
        req('/panic', 'POST'); time.sleep(0.8)
        req('/stops/set', 'POST', {'ids': [sid(naam)]}); time.sleep(0.2)
        req('/record/start', 'POST', {}); time.sleep(0.3)
        req('/notes/60/on?velocity=100', 'POST'); time.sleep(houd_ms / 1000.0)
        req('/notes/60/off', 'POST'); time.sleep(1.6)
        r = req('/record/stop', 'POST')
        shutil.copy(r['path'], TMP)
        return release_freqs(0.3 + houd_ms / 1000.0, kand)

    R.ok(proef('Gescheiden', 80) == {600}, 'korte noot (80 ms): korte release (600 Hz)')
    R.ok(proef('Gescheiden', 250) == {900}, 'middellange noot (250 ms): middellange release (900 Hz)')
    R.ok(proef('Gescheiden', 1000) == {1300}, 'lange noot (1000 ms): primaire release (1300 Hz)')
    R.ok(proef('MeerCues', 1200) == {700, 1400}, 'twee cue-markers: release start bij de EERSTE (700 Hz, daarna 1400 Hz)')


# ----------------------------------------------------------------------------- 3. koppels
HARM = {'Ped': 3, 'Gt': 5, 'Sw': 7}


def klinkt(kandidaten):
    x = pcm(TMP)[int(0.35 * 48000):int(1.15 * 48000)]
    N = len(x) * 4
    sp = np.abs(np.fft.rfft(x * np.hanning(len(x)), N))
    fr = np.fft.rfftfreq(N, 1 / 48000)
    uit = set()
    for div, n in kandidaten:
        f = HARM[div] * freq(n)
        if f > 5500:  # daarboven zaaien limiter/mp3 onechte harmonischen
            continue
        i = np.argmin(abs(fr - f))
        piek = sp[max(0, i - 6):i + 7].max()
        vloer = np.median(sp[(fr > f - 60) & (fr < f + 60)]) + 1e-9
        if piek / vloer > 40:
            uit.add((div, n))
    return uit


def koppels():
    org = laad(pad('koppel', 'OrganDefinitions', 'Koppel.CustomOrgan_Hauptwerk_xml'))
    K = {' '.join(c['name'].split()): c['id'] for c in org['couplers']}
    stops = alle_stops(org)
    KAN = {'Pedal': 0, 'Great': 1, 'Swell': 2}
    for div, ch in KAN.items():
        req('/midi/mapping', 'POST', {'division': div, 'channel': ch})

    def sleutel(z):
        z = z.replace(" 8'", '')
        return K[z] if z in K else [v for k, v in K.items() if z in k][0]

    def geval(titel, koppelnamen, toetsen, verwacht, stemmen=None):
        req('/panic', 'POST'); time.sleep(0.8)
        req('/stops/set', 'POST', {'ids': list(stops.values())})
        req('/couplers/set', 'POST', {'ids': [sleutel(k) for k in koppelnamen]}); time.sleep(0.3)
        st = opname(TMP, 1.0, kanaal_noten=[(KAN[d], n) for d, n in toetsen])
        kand = {(d, n + s) for d in HARM for (_, n) in toetsen for s in (-12, 0, 12)}
        kand = [(d, n) for d, n in kand if 36 <= n <= 96]
        gevonden = klinkt(kand)
        extra = '' if stemmen is None else f', stemmen {st} (verwacht {stemmen})'
        R.ok(gevonden == set(verwacht) and (stemmen is None or st == stemmen),
             f'{titel}: klinkt {sorted(gevonden)}{extra}')

    geval('Great 60, geen koppel', [], [('Great', 60)], [('Gt', 60)])
    geval("Sw to Gt 8'", ["Sw to Gt 8'"], [('Great', 60)], [('Gt', 60), ('Sw', 60)])
    geval("Sw to Gt 4'", ["Sw to Gt 4'"], [('Great', 60)], [('Gt', 60), ('Sw', 72)])
    geval("Sw to Gt 16'", ["Sw to Gt 16'"], [('Great', 60)], [('Gt', 60), ('Sw', 48)])
    geval("Gt Unison Off + Sw to Gt 8'", ['Gt Unison Off', "Sw to Gt 8'"], [('Great', 60)], [('Sw', 60)])
    geval("Gt to Ped 8' (pedaaltoets)", ["Gt to Ped 8'"], [('Pedal', 48)], [('Ped', 48), ('Gt', 48)])
    geval('Bass: laagste Great-toets op Pedal', ['Bass'], [('Great', 60), ('Great', 64)], [('Gt', 60), ('Gt', 64), ('Ped', 60)])
    geval('Swell Melody: hoogste Great-toets op Swell', ['Swell Melody'], [('Great', 60), ('Great', 64)], [('Gt', 60), ('Gt', 64), ('Sw', 64)])
    geval("Rand: Great 96 + Sw 4' (108 bestaat niet)", ["Sw to Gt 4'"], [('Great', 96)], [], stemmen=1)
    geval("Rand: Great 36 + Sw 16' (24 bestaat niet)", ["Sw to Gt 16'"], [('Great', 36)], [('Gt', 36)])
    # Vanaf 0.9.44: CODM-koppels doen niet aan de GO-stijl keten (gids: source keyboard -> destination
    # key action). Op een pedaaltoets klinken Ped en Gt (via Gt->Ped), maar Sw niet - er zijn geen Sw-toetsen.
    geval("Geen keten: Sw to Gt + Gt to Ped, pedaaltoets -> Sw klinkt NIET", ["Sw to Gt 8'", "Gt to Ped 8'"], [('Pedal', 48)],
          [('Gt', 48), ('Ped', 48)])

    req('/panic', 'POST'); time.sleep(0.8)
    req('/stops/set', 'POST', {'ids': list(stops.values())}); req('/couplers/set', 'POST', {'ids': []})
    req('/midi/inject?note=60&velocity=100&channel=1', 'POST'); time.sleep(0.5)
    v1 = req('/status')['voice_count']
    req('/couplers/set', 'POST', {'ids': [sleutel("Sw to Gt 8'")]}); time.sleep(0.5)
    v2 = req('/status')['voice_count']
    req('/couplers/set', 'POST', {'ids': [sleutel('Gt Unison Off'), sleutel("Sw to Gt 4'")]}); time.sleep(0.5)
    v3 = req('/status')['voice_count']
    req('/midi/inject?note=60&on=0&channel=1', 'POST')
    stil = wacht_tot_stil()
    R.ok(v2 > v1 and stil is not None, f'live koppelen bij ingedrukte toets: stemmen {v1} -> {v2} -> {v3}, stil na loslaten ({stil} s): geen hangers')


# ----------------------------------------------------------------------------- 4. schakelaars
def schakelaars():
    run = 'run%d' % int(time.time())
    doel = pad('koppel', run, 'OrganDefinitions')
    os.makedirs(doel, exist_ok=True)
    shutil.copy(pad('koppel', 'OrganDefinitions', 'Schakel.CustomOrgan_Hauptwerk_xml'), doel + '/Schakel.CustomOrgan_Hauptwerk_xml')
    org = laad(doel + '/Schakel.CustomOrgan_Hauptwerk_xml')
    req('/panic', 'POST')
    sch = req('/pistons')
    actief = [' '.join(c['name'].split()) for c in org['couplers'] if c['active']]
    drawn = [s['name'] for d in org['divisions'] for s in d['stops'] if s['drawn']]
    R.ok(actief == ["Sw to Gt 4'"], f'DefaultToEngaged koppel bij het eerste laden: {actief}')
    R.ok(len(drawn) == 1 and 'Gamba' in drawn[0], f'DefaultToEngaged register: {drawn}')
    R.ok(len(sch['pistons']) == 3 and len(org['couplers']) == 2, 'drie pistons en geen extra registers of koppels')
    R.ok(len(sch['koppels_buiten_combinatie']) == 1, f"koppel buiten de combinaties: {sch['koppels_buiten_combinatie']}")

    def koppel_aan(naam):
        o = req('/organ')
        for c in o['couplers']:
            if (naam == '8' and ' '.join(c['name'].split()) == 'Sw to Gt') or (naam == '4' and "4'" in c['name']):
                return c['active']

    req('/pistons/0/druk?aan=1', 'POST'); req('/pistons/0/druk?aan=0', 'POST')
    a = koppel_aan('8')
    req('/pistons/0/druk?aan=1', 'POST'); req('/pistons/0/druk?aan=0', 'POST')
    R.ok(a is True and koppel_aan('8') is False, 'omschakelende piston: 1e druk aan, 2e druk uit')
    req('/pistons/1/druk?aan=1', 'POST'); req('/pistons/1/druk?aan=0', 'POST')
    R.ok(koppel_aan('4') is True, 'vasthoud-piston op een koppel dat al aan stond: blijft aan')
    req('/couplers/set', 'POST', {'ids': []}); time.sleep(0.2)
    req('/pistons/1/druk?aan=1', 'POST'); a = koppel_aan('4')
    req('/pistons/1/druk?aan=0', 'POST'); b = koppel_aan('4')
    R.ok(a is True and b is False, f'vasthouden: aan zolang ingedrukt ({a}), uit na loslaten ({b})')
    req('/pistons/2/druk?aan=1', 'POST'); req('/pistons/2/druk?aan=0', 'POST')
    o = req('/organ')
    R.ok([s['drawn'] for d in o['divisions'] for s in d['stops'] if 'Gamba' in s['name']][0] is False, 'piston op een register schakelt het om')


# ----------------------------------------------------------------------------- 5. willekeurige stemming
def willekeurig(rank, noot, max_hz, doel):
    M = 0xFFFFFFFFFFFFFFFF
    z = (((rank << 32) | noot) + 0x9E3779B97F4A7C15) & M
    z = ((z ^ (z >> 30)) * 0xBF58476D1CE4E5B9) & M
    z = ((z ^ (z >> 27)) * 0x94D049BB133111EB) & M
    z ^= z >> 31
    u = (z >> 11) / float(1 << 53)
    teken = 1.0 if z & 1 == 0 else -1.0
    f = 440 * 2 ** ((doel - 6900) / 1200)
    return 1200 * math.log2(max(f + teken * max_hz * u * u, 0.01) / f)


def stemming():
    mini = pad('release', 'OrganDefinitions', 'Mini.CustomOrgan_Hauptwerk_xml')

    def meet(aan):
        req('/settings/codm_random?on=' + ('1' if aan else '0'), 'POST')
        org = laad(mini)
        sid = [v for k, v in alle_stops(org).items() if 'MeerCues' in k][0]
        uit = {}
        for noot in (60, 61):
            req('/panic', 'POST'); time.sleep(0.8)
            req('/stops/set', 'POST', {'ids': [sid]}); time.sleep(0.2)
            opname(TMP, 1.6, noten=[noot])
            x = pcm(TMP)[int(0.5 * 48000):int(1.3 * 48000)]
            sp = np.abs(np.fft.rfft(x * np.hanning(len(x)), len(x) * 16))
            fr = np.fft.rfftfreq(len(x) * 16, 1 / 48000)
            b = (fr > 150) & (fr < 700)
            uit[noot] = fr[b][np.argmax(sp[b])]
        return uit

    uit, aan = meet(False), meet(True)
    req('/settings/codm_random?on=1', 'POST')
    for noot in (60, 61):
        gemeten = 1200 * math.log2(aan[noot] / uit[noot])
        voorspeld = willekeurig(2, noot, 5.0, 100.0 * noot)
        R.ok(abs(gemeten - voorspeld) < 1.5, f'willekeurige stemafwijking noot {noot}: {gemeten:+.1f} ct (voorspeld {voorspeld:+.1f})')


# ----------------------------------------------------------------------------- 6. chimes
def chimes():
    odf = os.path.join(REPO, 'testen', 'codm', 'pitea-minimal', 'OrganDefinitions', 'Les-Pitea895-945.CustomOrgan_Hauptwerk_xml')
    if not os.path.exists(odf):
        R.overslaan('chimes: testen/codm/pitea-minimal ontbreekt (optionele fixture)')
        return
    org = laad(odf)
    sid = org['divisions'][0]['stops'][0]['id']
    res = {}
    for noot in (60, 72, 84):
        req('/panic', 'POST'); time.sleep(0.6)
        req('/stops/set', 'POST', {'ids': [sid]}); time.sleep(0.2)
        opname(TMP, 1.8, noten=[noot])
        x = pcm(TMP)[int(0.3 * 48000):int(1.5 * 48000)]
        sp = np.abs(np.fft.rfft(x * np.hanning(len(x))))
        fr = np.fft.rfftfreq(len(x), 1 / 48000)
        b = (fr > 60) & (fr < 6000)
        res[noot] = fr[b][np.argmax(sp[b])]
    R.ok(abs(1200 * math.log2(res[72] / res[60]) - 1200) < 30 and abs(1200 * math.log2(res[84] / res[72]) - 1200) < 30,
         f'chimes: 37 toetsen uit een WAV, octaven kloppen ({res[60]:.0f}/{res[72]:.0f}/{res[84]:.0f} Hz)')


def main():
    os.makedirs(UIT, exist_ok=True)
    maak_codm_releasetest.main(os.path.join(UIT, 'release'))
    maak_codm_koppeltest.main(os.path.join(UIT, 'koppel'))
    with App():
        for naam, f in (('verslag', verslag), ('releases', releases), ('koppels', koppels), ('schakelaars', schakelaars),
                        ('stemming', stemming), ('chimes', chimes)):
            print(f'--- {naam}')
            try:
                f()
            except Exception as e:  # een crash is een fout, geen stil overslaan
                R.ok(False, f'{naam}: onverwachte fout {e!r}')
    print(f'\nFOUTEN: {R.fouten}')
    sys.exit(1 if R.fouten else 0)


if __name__ == '__main__':
    main()
