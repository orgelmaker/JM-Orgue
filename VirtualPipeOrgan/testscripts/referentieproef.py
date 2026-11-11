#!/usr/bin/env python3
"""Referentieproef (taak C3): een vaste MIDI-reeks, per register opgenomen, per noot gemeten en vergeleken.

Bouwt voort op `ab_vergelijk.py` en `docs/AB_KLANKPROTOCOL.md`: dezelfde definities (niveau = RMS over het
sustain-venster, aanzet = 10 -> 90 % van de omhullende in vensters van 10 ms, uitsterven = tijd tot -40 dB
na NoteOff) en dezelfde richtwaarden (1,5 dB, 5 cent, 20 %). Nieuw is dat de hele reeks automatisch gaat:

  midi        schrijf het vaste MIDI-bestand (en toon de tijdlijn)
  opnemen     laad een orgel in een draaiende app (`vpo-app --test-api`), trek per register alleen dat
              register, speel het MIDI-bestand af en bewaar de opname + manifest.json
  analyseer   meet elke noot van een opname (map of bestand), schrijf analyse-*.json
  vergelijk   vergelijk twee opnamen of twee mappen (JM-Orgue tegen JM-Orgue, of tegen een referentie)
  zelftest    controleert dit hulpmiddel zelf op synthetische opnamen met bekende waarden

Voorbeeld (regressie voor C1, C2 of C4: dezelfde set, voor en na de wijziging):

  python referentieproef.py opnemen --orgel "D:/Samplesets/.../x.Organ_Hauptwerk_xml" --uit voor --registers Principal
  ... wijziging bouwen, app herstarten ...
  python referentieproef.py opnemen --orgel "D:/Samplesets/.../x.Organ_Hauptwerk_xml" --uit na --registers Principal
  python referentieproef.py vergelijk voor na

Een referentie (GrandOrgue, Hauptwerk) neem je op met dezelfde `referentieproef.mid` en dezelfde registratie;
zie docs/AB_KLANKPROTOCOL.md, sectie 7. Zonder ffmpeg op PATH werkt het decoderen niet.
"""
import argparse
import datetime
import glob
import json
import math
import os
import platform
import re
import struct
import subprocess
import sys
import tempfile
import time
import urllib.request
import wave

import numpy as np

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from ab_vergelijk import laad as laad_mono, SR  # noqa: E402

BASIS = 'http://127.0.0.1:8765'

# Richtwaarden uit docs/AB_KLANKPROTOCOL.md, sectie 4: aanleiding om te luisteren, geen oordeel.
TOL_NIVEAU_DB = 1.5
TOL_CENT = 5.0
TOL_TIJD = 0.20
# De omhullende heeft vensters van 10 ms; onder deze aanzettijd zegt een verhouding niets.
MIN_AANZET_MS = 30.0

# De vaste reeks. Verander dit niet zonder de naam van het bestand en de documentatie mee te veranderen:
# opnamen van verschillende tijden zijn alleen vergelijkbaar bij dezelfde tijdlijn.
NOTEN = (36, 48, 60, 72, 84, 96)
HOUD_S = 2.5
DIEPTES_DB = (10, 20, 30, 40)  # uitsterven tot zoveel dB onder het sustain-niveau
MARGE_GRENSGEVAL_DB = 12.0  # zachter dan dit boven de ruis: aan/uit-verschil is een grensgeval
STACCATO_S = (0.04, 0.10, 0.30)
STACCATO_NOOT = 60
VOORLOOP_S = 1.5
TICKS_PER_S = 960  # 480 ticks per kwartnoot bij 120 bpm


def tijdlijn(gap_s=4.0, noten=NOTEN):
    """De noten van de proef met aan/uit/tot (seconden vanaf het begin van het bestand)."""
    t, items = VOORLOOP_S, []
    for n in noten:
        items.append(dict(soort='lang', noot=n, aan=t, uit=t + HOUD_S, tot=t + HOUD_S + gap_s))
        t += HOUD_S + gap_s
    gap_k = min(gap_s, 3.0)
    for h in STACCATO_S:
        items.append(dict(soort='staccato', noot=STACCATO_NOOT, aan=t, uit=t + h, tot=t + h + gap_k))
        t += h + gap_k
    return items, t


# ── MIDI ────────────────────────────────────────────────────────────────────

def _vlq(n):
    b = [n & 0x7F]
    n >>= 7
    while n:
        b.append((n & 0x7F) | 0x80)
        n >>= 7
    return bytes(reversed(b))


def schrijf_midi(pad, items, totaal_s):
    """Format 0, één spoor, kanaal 1, 120 bpm; NoteOff als NoteOn met velocity 0."""
    ev = []
    for it in items:
        ev.append((round(it['aan'] * TICKS_PER_S), 1, it['noot']))
        ev.append((round(it['uit'] * TICKS_PER_S), 0, it['noot']))
    ev.sort()  # bij gelijke tick eerst uit, dan aan
    spoor = bytearray(_vlq(0) + b'\xFF\x51\x03' + (500000).to_bytes(3, 'big'))
    laatste = 0
    for tick, aan, noot in ev:
        spoor += _vlq(tick - laatste) + bytes([0x90, noot, 100 if aan else 0])
        laatste = tick
    spoor += _vlq(max(0, round(totaal_s * TICKS_PER_S) - laatste)) + b'\xFF\x2F\x00'
    with open(pad, 'wb') as f:
        f.write(b'MThd' + struct.pack('>IHHH', 6, 0, 1, 480) + b'MTrk' + struct.pack('>I', len(spoor)) + bytes(spoor))


def lees_midi_noten(pad):
    """Voor de zelftest: (seconde, aan?, noot) uit een door `schrijf_midi` gemaakt bestand."""
    data = open(pad, 'rb').read()
    assert data[:4] == b'MThd'
    i = data.index(b'MTrk') + 8
    t, uit = 0, []
    while i < len(data):
        n = 0
        while True:
            b = data[i]
            i += 1
            n = (n << 7) | (b & 0x7F)
            if not b & 0x80:
                break
        t += n
        if data[i] == 0xFF:
            lengte = data[i + 2]
            i += 3 + lengte
            continue
        status, noot, vel = data[i], data[i + 1], data[i + 2]
        i += 3
        if status == 0x90:
            uit.append((t / TICKS_PER_S, vel > 0, noot))
    return uit


# ── Test-API ────────────────────────────────────────────────────────────────

def api(methode, pad, body=None, timeout=900):
    data = None if body is None else json.dumps(body).encode()
    if methode == 'POST' and data is None:
        data = b''
    req = urllib.request.Request(BASIS + pad, data=data, method=methode, headers={'Content-Type': 'application/json'})
    with urllib.request.urlopen(req, timeout=timeout) as r:
        tekst = r.read().decode()
    try:
        return json.loads(tekst)
    except ValueError:
        return tekst


def veilig(naam):
    return re.sub(r'[^A-Za-z0-9]+', '_', naam).strip('_')[:40] or 'register'


def cmd_midi(a):
    items, totaal = tijdlijn(a.gap)
    os.makedirs(os.path.dirname(os.path.abspath(a.pad)), exist_ok=True)
    schrijf_midi(a.pad, items, totaal)
    print(f'{a.pad}: {len(items)} noten, {totaal:.1f} s')
    for it in items:
        print(f"  {it['soort']:9} noot {it['noot']:3}  aan {it['aan']:6.2f}  uit {it['uit']:6.2f}  tot {it['tot']:6.2f}")


def cmd_opnemen(a):
    try:
        versie = api('GET', '/version')
    except Exception as e:
        sys.exit(f'De test-API antwoordt niet ({e}). Start de app met: vpo-app.exe --test-api')
    items, totaal = tijdlijn(a.gap)
    os.makedirs(a.uit, exist_ok=True)
    midi = os.path.abspath(os.path.join(a.uit, 'referentieproef.mid'))
    schrijf_midi(midi, items, totaal)

    for opdracht in a.voor or []:
        methode, pad, *rest = opdracht.split(' ', 2)
        api(methode.upper(), pad, json.loads(rest[0]) if rest else None)
    api('POST', '/load_organ', {'path': a.orgel})
    if not a.galm_aan:  # na het laden: het laden kan de bewaarde galminstelling van het orgel terugzetten
        api('POST', '/reverb', {'mix': 0.0})
    org = api('GET', '/organ')
    status = api('GET', '/status')
    stops = [(d['name'], s) for d in org['divisions'] if not d.get('is_pedal') for s in d['stops']]
    if a.registers:
        gekozen = []
        for zoek in a.registers:
            gekozen += [(dn, s) for dn, s in stops if zoek.lower() in s['name'].lower()][:1]
    else:
        gekozen = stops[:a.max]
    if not gekozen:
        sys.exit('Geen register gevonden. Beschikbaar: ' + ', '.join(s['name'] for _, s in stops[:30]))
    deelen = [d['name'] for d in org['divisions']]

    manifest = {
        'datum': datetime.datetime.now().isoformat(timespec='seconds'),
        'app_versie': versie.get('version') if isinstance(versie, dict) else versie,
        'orgel': org.get('name'), 'orgel_pad': a.orgel,
        'orgel_vlaggen': status.get('orgel_vlaggen'), 'sample_rate': status.get('sample_rate'),
        'tuning': api('GET', '/tuning'), 'galm_uit': not a.galm_aan, 'voor_opdrachten': a.voor or [],
        'tijdlijn': {'gap_s': a.gap, 'items': items, 'totaal_s': totaal},
        'machine': {'os': platform.platform(), 'python': platform.python_version()},
        'registers': [],
    }
    for nr, (divisie, s) in enumerate(gekozen, 1):
        api('POST', '/panic')
        api('POST', '/stops/set', {'ids': [s['id']]})
        api('POST', '/couplers/set', {'ids': []})
        for dn in deelen:  # alleen de divisie van dit register hoort kanaal 1 te volgen
            api('POST', '/midi/mapping', {'division': dn, 'channel': 0 if dn == divisie else 15})
        time.sleep(0.4)
        pad = os.path.abspath(os.path.join(a.uit, f"{nr:02d}-{veilig(s['name'])}.mp3"))
        api('POST', '/record/start', {'path': pad})
        api('POST', '/midi/player/play', {'path': midi})
        gezien, einde = False, time.time() + totaal + 20
        while time.time() < einde:
            st = api('GET', '/midi/player/status')
            gezien = gezien or st.get('state') == 'playing'
            if (gezien and st.get('state') != 'playing') or st.get('position_ms', 0) >= st.get('duration_ms', 1e12) - 30:
                break
            time.sleep(0.25)
        time.sleep(0.8)
        stop = api('POST', '/record/stop')
        api('POST', '/panic')
        manifest['registers'].append({'nr': nr, 'naam': s['name'], 'divisie': divisie, 'voet': s.get('pitch'),
                                      'opname': os.path.basename(pad), 'rms': stop.get('rms') if isinstance(stop, dict) else None})
        print(f"[{nr}/{len(gekozen)}] {s['name']} ({divisie}) -> {os.path.basename(pad)}")
    with open(os.path.join(a.uit, 'manifest.json'), 'w', encoding='utf-8') as f:
        json.dump(manifest, f, indent=1, ensure_ascii=False)
    print(f'manifest: {os.path.join(a.uit, "manifest.json")}')


# ── Analyse ─────────────────────────────────────────────────────────────────

def _omhullende_db(x, dt=0.01):
    n = int(dt * SR)
    k = len(x) // n
    env = np.sqrt((x[:k * n].reshape(k, n) ** 2).mean(axis=1))
    return 20 * np.log10(np.maximum(env, 1e-7))


def zoek_offset(x, items, bereik_s=3.0, dt=0.01):
    """Seconden dat de opname later (+) of eerder (-) begint dan de nominale tijdlijn.

    Gezocht wordt het verschuiven waarbij het niveau binnen de aangehouden noten het meest boven het niveau
    erbuiten uitsteekt; werkt voor elke opname, ook als de referentie met een willekeurige voorloop begon.
    Geeft (offset, gelukt)."""
    e = _omhullende_db(x, dt)
    if len(e) < 20:
        return 0.0, False
    e = np.clip(e, e.max() - 60.0, None)
    if e.max() - e.min() < 12.0:
        return 0.0, False
    e = (e - e.mean()) / (e.std() + 1e-9)
    m = int(max(it['tot'] for it in items) / dt)
    ind = np.zeros(m, bool)
    for it in items:
        ind[int(it['aan'] / dt):max(int(it['aan'] / dt) + 1, int(it['uit'] / dt))] = True
    binnen, buiten = np.nonzero(ind)[0], np.nonzero(~ind)[0]
    beste, beste_lag = -1e9, 0
    for lag in range(-int(bereik_s / dt), int(bereik_s / dt) + 1):
        i1, i0 = binnen + lag, buiten + lag
        k1, k0 = (i1 >= 0) & (i1 < len(e)), (i0 >= 0) & (i0 < len(e))
        if k1.sum() < 0.8 * len(binnen) or k0.sum() < 10:
            continue
        score = e[i1[k1]].mean() - e[i0[k0]].mean()
        if score > beste:
            beste, beste_lag = score, lag
    return beste_lag * dt, True


def verfijn_offset(x, items, offset, ruis_db, drempel_db=12.0):
    """Verfijn de grove verschuiving op het eerste inzetmoment: het eerste venster van 2 ms waarin het niveau
    `drempel_db` boven de ruis komt, bij de eerste noot die klinkt. De grove zoektocht (contrast binnen/buiten
    de noten) schuift een halve aanzettijd door, omdat het begin van de aanzet nog stil is. Inzet = het moment
    dat de klank begint; dat is wat A en B moeten delen, niet het moment van het NoteOn-bericht."""
    n = int(0.002 * SR)
    for it in items:
        a0 = max(0, int((it['aan'] + offset - 0.3) * SR))
        a1 = min(len(x), int((it['aan'] + offset + 0.3) * SR))
        k = (a1 - a0) // n
        if k < 5:
            continue
        e = 20 * np.log10(np.maximum(np.sqrt((x[a0:a0 + k * n].reshape(k, n) ** 2).mean(axis=1)), 1e-7))
        hit = np.nonzero(e > ruis_db + drempel_db)[0]
        if len(hit) and hit[0] > 0:  # al luid bij het begin van het venster = staart van de vorige noot
            return (a0 + int(hit[0]) * n) / SR - it['aan']
    return offset


def toonhoogte(x, f_et):
    """Grondtoon (Hz) = de LAAGSTE sterke deeltoon (binnen 15 dB van de sterkste, tussen f_et/2,3 en 8 kHz),
    parabolisch geïnterpoleerd, en de afwijking in cent ten opzichte van de gelijkzwevende toon `f_et`, gevouwen
    op een octaaf. De sterkste piek nemen was niet stabiel: in de bas wisselt die tussen de 1e en de 2e deeltoon
    en die liggen door onharmonische deeltonen ~4 ct uit elkaar (gemeten: 6 ct tussen twee identieke opnamen).
    `partieel` = de grondtoon ligt niet op een octaaf van de noot (een quint- of terspijp): geen cent-waarde."""
    n = len(x)
    if n < 2048:
        return None, None, True
    nfft = 1 << int(math.ceil(math.log2(n * 4)))
    sp = np.abs(np.fft.rfft(x * np.hanning(n), nfft))
    fr = np.fft.rfftfreq(nfft, 1 / SR)
    band = (fr >= max(30.0, f_et / 2.3)) & (fr < 8000)
    if not band.any() or sp[band].max() <= 0:
        return None, None, True
    top = sp[band].max()
    lok = np.zeros(len(sp), bool)
    lok[1:-1] = (sp[1:-1] > sp[:-2]) & (sp[1:-1] >= sp[2:])
    kand = np.nonzero(lok & band & (sp >= 0.178 * top))[0]
    k = int(kand[0]) if len(kand) else int(np.argmax(np.where(band, sp, 0.0)))
    if 1 <= k < len(sp) - 1 and sp[k - 1] > 0 and sp[k + 1] > 0 and sp[k] > 0:
        a, b, c = math.log(sp[k - 1]), math.log(sp[k]), math.log(sp[k + 1])
        d = 0.5 * (a - c) / (a - 2 * b + c) if (a - 2 * b + c) != 0 else 0.0
    else:
        d = 0.0
    f0 = float(fr[k] + d * (fr[1] - fr[0]))
    cent = 1200 * math.log2(f0 / f_et)
    gevouwen = ((cent + 600) % 1200) - 600
    return f0, gevouwen, abs(gevouwen) > 50


def zwaartepunt(x):
    """Spectraal zwaartepunt (Hz) tussen 40 Hz en 8 kHz."""
    sp = np.abs(np.fft.rfft(x * np.hanning(len(x))))
    fr = np.fft.rfftfreq(len(x), 1 / SR)
    band = (fr > 40) & (fr < 8000)
    return float((fr[band] * sp[band]).sum() / max(sp[band].sum(), 1e-12))


def _glijdend(x, venster=0.010, stap=0.002):
    """Vermogen per venster (10 ms, om de 2 ms), de stap in samples en de vensterlengte in samples.
    Tijd van venster i = (i * stap + venster / 2)."""
    n, st = int(venster * SR), int(stap * SR)
    if len(x) < n:
        return np.array([]), st, n
    c = np.concatenate(([0.0], np.cumsum(x.astype(np.float64) ** 2)))
    idx = np.arange(0, len(x) - n + 1, st)
    return (c[idx + n] - c[idx]) / n, st, n


def meet_item(x, it, offset, ruis_db):
    """Eén noot meten. Definities zoals in docs/AB_KLANKPROTOCOL.md, met twee verfijningen die de herhaalbaarheids-
    proef afdwong: aanzet en uitsterven zijn gerelateerd aan het GEMETEN sustain-niveau (niet aan het maximum van
    de eerste halve seconde), en uitsterven wordt op meerdere diepten gemeld, omdat een zachte opname -40 dB onder
    de ruisvloer laat. Een diepte telt alleen als haar doel minstens 6 dB boven de ruis ligt."""
    ruis_p = 10 ** (ruis_db / 10)
    i0 = max(0, int((it['aan'] + offset - 0.4) * SR))
    i1 = min(len(x), int((it['tot'] + offset) * SR))
    seg = x[i0:i1]
    aan = it['aan'] + offset - i0 / SR
    uit = it['uit'] + offset - i0 / SR
    lang = it['uit'] - it['aan'] >= 1.0
    res = {'soort': it['soort'], 'noot': it['noot'], 'aan': it['aan'], 'uit': it['uit']}
    pw, st, n = _glijdend(seg)
    if len(pw) < 10:
        res.update(stil=True, fout='opname te kort')
        return res
    t = (np.arange(len(pw)) * st + n / 2) / SR
    sus = None
    if lang:
        sus = seg[int((aan + 1.2) * SR):int((uit - 0.1) * SR)]
        p = float(np.mean(sus ** 2)) if len(sus) > 2048 else 0.0
    else:
        m = (t >= aan) & (t <= uit + 0.05)
        p = float(pw[m].max()) if m.any() else 0.0
    if p < 4 * ruis_p:
        res.update(stil=True, niveau_db=round(10 * math.log10(max(p, 1e-14)), 2))
        return res
    # Vanaf hier rekenen we met het vermogen ZONDER ruis: een opname die dicht bij de ruisvloer zit (een zachte
    # set, of een lage uitgangsstand) lijkt anders trager uit te sterven en langzamer aan te zetten dan dezelfde
    # klank op een hoger niveau. Gemeten: JM-Orgue met en zonder de niveaus van de set gaf zo x1,24 uitsterven.
    ps = p - ruis_p
    pw = np.maximum(pw - ruis_p, 1e-14)
    res.update(stil=False, niveau_db=round(10 * math.log10(ps), 2), marge_db=round(10 * math.log10(p / ruis_p), 1))
    if lang:
        f_et = 440.0 * 2 ** ((it['noot'] - 69) / 12.0)
        f0, cent, partieel = toonhoogte(sus, f_et)
        res.update(grondtoon_hz=None if f0 is None else round(f0, 3),
                   cent_ten_opzichte_van_et=None if cent is None else round(cent, 2), partieel=partieel,
                   zwaartepunt_hz=round(zwaartepunt(sus), 1))
        # Aanzet: 10 -> 90 % van het sustain-amplitude (1 % -> 81 % van het vermogen).
        a_s, a_e = int(np.searchsorted(t, aan - 0.35)), int(np.searchsorted(t, aan + 1.5))
        w = pw[a_s:a_e]
        i10 = np.nonzero(w >= 0.01 * ps)[0]
        i90 = np.nonzero(w >= 0.81 * ps)[0]
        # De 10 %-drempel moet minstens 6 dB boven de ruis liggen, anders meet je de ruis.
        if (0.01 * ps >= 4 * ruis_p and len(w) and w[0] < 0.01 * ps and len(i10) and len(i90)
                and i90[0] >= i10[0]):
            res['aanzet_ms'] = round((i90[0] - i10[0]) * st / SR * 1000, 1)
        else:
            res['aanzet_ms'] = None
    # Uitsterven. Het nulpunt is NIET het nominale NoteOff: dat hangt van de uitlijning af en die weer van het
    # niveau (gemeten: 26 ms verschil tussen twee opnamen van dezelfde klank op 20 dB van elkaar, genoeg voor een
    # schijnverschil van x1,24). Het nulpunt is het eerste moment dat het niveau 3 dB onder het sustain zakt,
    # dus de tijden zijn 'van -3 dB tot -d dB' en hangen alleen van de klank zelf af.
    if lang:
        i_zoek = int(np.searchsorted(t, uit - 0.3))
    else:
        m = (t >= aan) & (t <= uit + 0.3)
        i_zoek = int(np.nonzero(m)[0][np.argmax(pw[m])]) if m.any() else 0
    half = np.nonzero(pw[i_zoek:] <= 0.5 * ps)[0]
    uitst = {str(d): None for d in DIEPTES_DB}
    if len(half):
        i_half = i_zoek + int(half[0])
        for diepte in DIEPTES_DB:
            doel = ps * 10 ** (-diepte / 10)
            if doel < 4 * ruis_p:
                continue
            idx = np.nonzero(pw[i_half:] <= doel)[0]
            if len(idx):
                uitst[str(diepte)] = round(float(t[i_half + idx[0]] - t[i_half]) * 1000, 1)
    res['uitsterf_ms'] = uitst
    return res


def analyseer_bestand(pad, items):
    x = laad_mono(pad)
    offset, gelukt = zoek_offset(x, items)
    # Ruis uit de voorloop (vóór de eerste noot), met de gevonden verschuiving.
    a, b = int(max(0.0, offset + 0.15) * SR), int(max(0.2, offset + VOORLOOP_S - 0.15) * SR)
    ruis_db = 20 * math.log10(max(float(np.sqrt(np.mean(x[a:b] ** 2))) if b > a else 1e-7, 1e-7))
    if gelukt:
        offset = verfijn_offset(x, items, offset, ruis_db)
    return {'bron': os.path.basename(pad), 'duur_s': round(len(x) / SR, 2), 'offset_s': round(offset, 3),
            'offset_gevonden': gelukt, 'ruis_db': round(ruis_db, 2),
            'noten': [meet_item(x, it, offset, ruis_db) for it in items]}


def _opnamen_in(pad):
    if os.path.isdir(pad):
        return sorted(p for ext in ('mp3', 'wav', 'flac') for p in glob.glob(os.path.join(pad, f'*.{ext}')))
    return [pad]


def _tijdlijn_van(pad, gap):
    if os.path.isdir(pad) and os.path.exists(os.path.join(pad, 'manifest.json')):
        return json.load(open(os.path.join(pad, 'manifest.json'), encoding='utf-8'))['tijdlijn']['items']
    return tijdlijn(gap)[0]


def cmd_analyseer(a):
    items = _tijdlijn_van(a.pad, a.gap)
    uit_map = a.pad if os.path.isdir(a.pad) else os.path.dirname(os.path.abspath(a.pad))
    for opname in _opnamen_in(a.pad):
        res = analyseer_bestand(opname, items)
        doel = os.path.join(uit_map, 'analyse-' + os.path.splitext(os.path.basename(opname))[0] + '.json')
        with open(doel, 'w', encoding='utf-8') as f:
            json.dump(res, f, indent=1)
        klinkend = sum(1 for n in res['noten'] if not n['stil'])
        print(f"{res['bron']}: {klinkend}/{len(res['noten'])} noten klinken, verschuiving {res['offset_s']:+.2f} s, "
              f"ruis {res['ruis_db']:.1f} dBFS -> {os.path.basename(doel)}")


# ── Vergelijken ─────────────────────────────────────────────────────────────

def _analyses(pad, gap):
    """Lijst van (sleutel, analyse) voor een bestand of map; analyseert zo nodig eerst."""
    if os.path.isfile(pad) and pad.endswith('.json'):
        return [(os.path.basename(pad), json.load(open(pad, encoding='utf-8')))]
    items = _tijdlijn_van(pad, gap)
    uit = []
    for opname in _opnamen_in(pad):
        sleutel = re.sub(r'^\d+-', '', os.path.splitext(os.path.basename(opname))[0])
        uit.append((sleutel, analyseer_bestand(opname, items)))
    return uit


def _mediaan(w):
    return float(np.median(w)) if w else None


def vergelijk_paar(a, b, uitlijnen):
    rijen, niv, cents, aanzet, uitst = [], [], [], [], []
    stil_verschil = 0
    for na, nb in zip(a['noten'], b['noten']):
        rij = {'soort': na['soort'], 'noot': na['noot']}
        if na['stil'] and nb['stil']:
            rij['opm'] = 'stil in beide'
        elif na['stil'] != nb['stil']:
            klinkend = nb if na['stil'] else na
            if klinkend.get('marge_db', 99.0) < MARGE_GRENSGEVAL_DB:
                rij['opm'] = 'grensgeval: bijna stil in één opname'  # geen vlag: hangt van de ruis af
            else:
                rij['opm'] = 'klinkt alleen in ' + ('B' if na['stil'] else 'A')
                stil_verschil += 1
        else:
            rij['d_niveau'] = nb['niveau_db'] - na['niveau_db']
            # Een staccatonoot haalt zijn top niet: haar niveau hangt van de aanzet af, dus telt niet mee
            # in het niveau en de balans van het register (de rij blijft wel in de tabel).
            if na['soort'] == 'lang':
                niv.append(rij['d_niveau'])
            ca, cb = na.get('cent_ten_opzichte_van_et'), nb.get('cent_ten_opzichte_van_et')
            if ca is not None and cb is not None and not na.get('partieel') and not nb.get('partieel'):
                d = ((cb - ca + 600) % 1200) - 600
                rij['d_cent'] = d
                cents.append(d)
            wa, wb = na.get('aanzet_ms'), nb.get('aanzet_ms')
            if wa and wb and wa >= MIN_AANZET_MS and wb >= MIN_AANZET_MS:
                rij['v_aanzet_ms'] = wb / wa
                aanzet.append(wb / wa)
            # Uitsterven op de grootste diepte die beide opnamen boven hun ruis konden meten.
            for diepte in reversed(DIEPTES_DB):
                ua, ub = na.get('uitsterf_ms', {}).get(str(diepte)), nb.get('uitsterf_ms', {}).get(str(diepte))
                if ua and ub and ua > 0 and ub > 0:
                    rij['v_uitsterf'], rij['uitsterf_diepte'] = ub / ua, diepte
                    uitst.append(ub / ua)
                    break
        rijen.append(rij)
    med_niv = _mediaan(niv) or 0.0
    afw = [d - med_niv for d in niv]
    samenvatting = {
        'noten_vergeleken': len(niv), 'noten_stil_verschil': stil_verschil,
        'niveau_mediaan_db': None if not niv else med_niv,
        'niveau_spreiding_db': None if not niv else max(afw) - min(afw),
        'cent_mediaan': _mediaan(cents), 'cent_max_abs': max((abs(c) for c in cents), default=None),
        'aanzet_verhouding_mediaan': _mediaan(aanzet), 'uitsterf_verhouding_mediaan': _mediaan(uitst),
    }
    vlaggen = []
    if niv:
        if not uitlijnen and abs(med_niv) > TOL_NIVEAU_DB:
            vlaggen.append(f'niveau {med_niv:+.1f} dB')
        if max(abs(d) for d in afw) > TOL_NIVEAU_DB:
            vlaggen.append(f'niveaubalans tussen noten: tot {max(abs(d) for d in afw):.1f} dB van de mediaan')
    if cents and samenvatting['cent_max_abs'] > TOL_CENT:
        vlaggen.append(f"toonhoogte tot {samenvatting['cent_max_abs']:.1f} ct")
    for naam, lijst in (('aanzet', aanzet), ('uitsterven', uitst)):
        if lijst and abs(_mediaan(lijst) - 1.0) > TOL_TIJD:
            vlaggen.append(f'{naam} x{_mediaan(lijst):.2f}')
    if stil_verschil:
        vlaggen.append(f'{stil_verschil} noot/noten klinken in maar één opname')
    return rijen, samenvatting, vlaggen


def _fmt(w, f='{:+.1f}'):
    return '-' if w is None else f.format(w)


def cmd_vergelijk(a):
    A, B = _analyses(a.a, a.gap), _analyses(a.b, a.gap)
    if len(A) != len(B):
        sys.exit(f'Aantal opnamen verschilt ({len(A)} tegen {len(B)}); vergelijk twee gelijke reeksen.')
    bdict = dict(B)
    uit, regels, ver_te_veel = [], [], 0
    regels.append(f'# Referentieproef: {a.a} (A) tegen {a.b} (B)\n')
    regels.append(f'Richtwaarden (docs/AB_KLANKPROTOCOL.md): {TOL_NIVEAU_DB} dB, {TOL_CENT:g} ct, {TOL_TIJD * 100:.0f} %. '
                  'Verschil is B min A; een vlag is een aanleiding om te luisteren, geen oordeel.\n')
    regels.append('| register | noten | niveau (dB) | balans (dB) | cent max | aanzet | uitsterven | vlaggen |')
    regels.append('|---|---:|---:|---:|---:|---:|---:|---|')
    details = []
    for i, (sleutel, ana) in enumerate(A):
        bk = sleutel if sleutel in bdict else B[i][0]
        rijen, s, vl = vergelijk_paar(ana, bdict[bk], a.niveau_uitlijnen)
        ver_te_veel += bool(vl)
        regels.append(f"| {sleutel} | {s['noten_vergeleken']} | {_fmt(s['niveau_mediaan_db'])} | "
                      f"{_fmt(s['niveau_spreiding_db'], '{:.1f}')} | {_fmt(s['cent_max_abs'], '{:.1f}')} | "
                      f"{_fmt(s['aanzet_verhouding_mediaan'], 'x{:.2f}')} | {_fmt(s['uitsterf_verhouding_mediaan'], 'x{:.2f}')} | "
                      f"{'; '.join(vl) or 'binnen de richtwaarden'} |")
        uit.append({'register': sleutel, 'samenvatting': s, 'vlaggen': vl, 'noten': rijen})
        d = [f'\n### {sleutel}\n', '| soort | noot | Δniveau dB | Δcent | aanzet B/A | uitsterven B/A | opm |', '|---|---:|---:|---:|---:|---:|---|']
        for r in rijen:
            d.append(f"| {r['soort']} | {r['noot']} | {_fmt(r.get('d_niveau'))} | {_fmt(r.get('d_cent'))} | "
                     f"{_fmt(r.get('v_aanzet_ms'), '{:.2f}')} | "
                     f"{_fmt(r.get('v_uitsterf'), '{:.2f}') + (' (-' + str(r['uitsterf_diepte']) + ' dB)' if 'uitsterf_diepte' in r else '')} | {r.get('opm', '')} |")
        details += d
    regels += details
    tekst = '\n'.join(regels) + '\n'
    print(tekst)
    if a.uit:
        os.makedirs(a.uit, exist_ok=True)
        open(os.path.join(a.uit, 'vergelijking.md'), 'w', encoding='utf-8').write(tekst)
        json.dump(uit, open(os.path.join(a.uit, 'vergelijking.json'), 'w', encoding='utf-8'), indent=1)
    if a.streng and ver_te_veel:
        sys.exit(1)


# ── Zelftest op synthetische opnamen ────────────────────────────────────────

def _synth(items, totaal, offset, niveau_db, cent, aanzet_s, uitsterf_s, ruis_db, zaad):
    """Opname met bekende waarden: tone + 2e deel, lineaire aanzet, exponentiële uitklinking naar -40 dB in
    `uitsterf_s`, ruis. `offset` seconden stilte vooraf."""
    rng = np.random.default_rng(zaad)
    n = int((totaal + offset + 1.0) * SR)
    y = rng.normal(0, 10 ** (ruis_db / 20), n)
    tau = uitsterf_s / math.log(100.0)
    amp = 10 ** (niveau_db / 20) * math.sqrt(2.0 / 1.09)
    for it in items:
        f = 440.0 * 2 ** ((it['noot'] - 69) / 12.0) * 2 ** (cent / 1200)
        a0, u0, e0 = it['aan'] + offset, it['uit'] + offset, it['tot'] + offset
        t = np.arange(int(a0 * SR), min(n, int(e0 * SR))) / SR
        env = np.where(t < u0, np.minimum(1.0, (t - a0) / aanzet_s), np.exp(-(t - u0) / tau))
        if u0 - a0 < aanzet_s:  # korte noot: bereikt zijn top niet helemaal
            env = np.where(t < u0, (t - a0) / aanzet_s, ((u0 - a0) / aanzet_s) * np.exp(-(t - u0) / tau))
        y[int(a0 * SR):int(a0 * SR) + len(t)] += amp * env * (np.sin(2 * np.pi * f * t) + 0.3 * np.sin(4 * np.pi * f * t))
    return np.clip(y, -1, 1)


def _schrijf_wav(pad, y):
    with wave.open(pad, 'wb') as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(SR)
        w.writeframes((y * 32767).astype('<i2').tobytes())


def cmd_zelftest(_a):
    fouten = []

    def eis(voorwaarde, tekst):
        print(('ok    ' if voorwaarde else 'FOUT  ') + tekst)
        if not voorwaarde:
            fouten.append(tekst)

    with tempfile.TemporaryDirectory() as tmp:
        items, totaal = tijdlijn(2.0, noten=(48, 60, 72))
        # 1. Het MIDI-bestand geeft precies de tijdlijn terug.
        mid = os.path.join(tmp, 'x.mid')
        schrijf_midi(mid, items, totaal)
        gelezen = lees_midi_noten(mid)
        verwacht = sorted([(it['aan'], True, it['noot']) for it in items] + [(it['uit'], False, it['noot']) for it in items])
        eis(len(gelezen) == len(verwacht) and all(abs(g[0] - v[0]) < 0.0012 and g[1:] == v[1:] for g, v in zip(gelezen, verwacht)),
            'MIDI-bestand = tijdlijn (binnen 1 tick)')

        # 2. Opnamen met bekende waarden
        a_wav, b_wav, a2_wav, c_wav = (os.path.join(tmp, f'{n}.wav') for n in 'a b a2 c'.split())
        _schrijf_wav(a_wav, _synth(items, totaal, 0.0, -20.0, 0.0, 0.060, 1.0, -70.0, 1))
        _schrijf_wav(a2_wav, _synth(items, totaal, 0.0, -20.0, 0.0, 0.060, 1.0, -70.0, 2))
        _schrijf_wav(b_wav, _synth(items, totaal, 0.0, -26.0, 7.0, 0.090, 1.6, -70.0, 3))
        _schrijf_wav(c_wav, _synth(items, totaal, 0.37, -20.0, 0.0, 0.060, 1.0, -70.0, 4))

        ana = {k: analyseer_bestand(p, items) for k, p in (('a', a_wav), ('a2', a2_wav), ('b', b_wav), ('c', c_wav))}
        eis(all(not n['stil'] for n in ana['a']['noten']), 'alle noten van de synthetische opname klinken')
        eis(abs(ana['a']['offset_s']) < 0.02, f"verschuiving 0 gevonden ({ana['a']['offset_s']:+.3f} s)")
        eis(abs(ana['c']['offset_s'] - 0.37) < 0.02, f"verschuiving 0,37 s gevonden ({ana['c']['offset_s']:+.3f} s)")
        lang = [n for n in ana['a']['noten'] if n['soort'] == 'lang']
        fouten_cent = [abs(n['cent_ten_opzichte_van_et']) for n in lang]
        eis(max(fouten_cent) < 1.0, f'toonhoogte binnen 1 ct van gelijkzwevend (max {max(fouten_cent):.2f})')
        uit_ms = [n['uitsterf_ms']['40'] for n in lang]
        eis(all(u and abs(u - 925) < 60 for u in uit_ms), f'uitsterven van -3 tot -40 dB rond 925 ms ({uit_ms})')
        aanz = [n['aanzet_ms'] for n in lang]
        eis(all(v and abs(v - 48) < 12 for v in aanz), f'aanzet 10-90 % rond 48 ms bij een aanzet van 60 ms ({aanz})')

        # 3. Twee opnamen met bekende verschillen: de vergelijking vindt ze, en een gelijke opname niet.
        _, s, vl = vergelijk_paar(ana['a'], ana['b'], False)
        eis(abs(s['niveau_mediaan_db'] + 6.0) < 0.3, f"niveauverschil -6 dB gemeten ({s['niveau_mediaan_db']:+.2f})")
        eis(s['cent_max_abs'] is not None and abs(s['cent_mediaan'] - 7.0) < 1.0, f"toonhoogteverschil +7 ct gemeten ({s['cent_mediaan']})")
        eis(all(any(v.startswith(k) for v in vl) for k in ('niveau -', 'toonhoogte', 'aanzet', 'uitsterven')),
            f'vlaggen voor niveau, toonhoogte, aanzet en uitsterven: {vl}')
        eis(not any('balans' in v for v in vl), 'geen niveaubalansvlag: alle lange noten zakken even veel')
        eis(s['uitsterf_verhouding_mediaan'] is not None and abs(s['uitsterf_verhouding_mediaan'] - 1.6) < 0.15,
            f"uitsterven x1,6 gemeten ({s['uitsterf_verhouding_mediaan']})")
        eis(s['aanzet_verhouding_mediaan'] is not None and abs(s['aanzet_verhouding_mediaan'] - 1.5) < 0.15,
            f"aanzet x1,5 gemeten ({s['aanzet_verhouding_mediaan']})")
        _, s0, vl0 = vergelijk_paar(ana['a'], ana['a2'], False)
        eis(not vl0, f'twee gelijke opnamen (andere ruis): geen vlaggen ({vl0})')
        _, s1, vl1 = vergelijk_paar(ana['a'], ana['c'], False)
        eis(not vl1, f'dezelfde opname met 0,37 s voorloop: geen vlaggen ({vl1})')
    if fouten:
        sys.exit(f'{len(fouten)} controle(s) mislukt')
    print('zelftest geslaagd')


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest='cmd', required=True)
    p = sub.add_parser('midi')
    p.add_argument('pad', nargs='?', default='testdata/referentieproef.mid')
    p.add_argument('--gap', type=float, default=4.0)
    p.set_defaults(f=cmd_midi)
    p = sub.add_parser('opnemen')
    p.add_argument('--orgel', required=True)
    p.add_argument('--uit', required=True)
    p.add_argument('--registers', nargs='*', help='delen van registernamen; zonder: de eerste --max registers')
    p.add_argument('--max', type=int, default=3)
    p.add_argument('--gap', type=float, default=4.0, help='seconden stilte na elke noot (natte sets: 6-8)')
    p.add_argument('--voor', action='append', help='API-opdracht vóór het laden, bv. "POST /orgel_vlaggen {\\"path\\": \\"<orgelpad>\\", \\"niveaus_van_set\\": false}"')
    p.add_argument('--galm-aan', action='store_true', help='galm van JM-Orgue laten staan (standaard droog: mix 0)')
    p.set_defaults(f=cmd_opnemen)
    p = sub.add_parser('analyseer')
    p.add_argument('pad')
    p.add_argument('--gap', type=float, default=4.0)
    p.set_defaults(f=cmd_analyseer)
    p = sub.add_parser('vergelijk')
    p.add_argument('a')
    p.add_argument('b')
    p.add_argument('--gap', type=float, default=4.0)
    p.add_argument('--uit', help='map voor vergelijking.md en vergelijking.json')
    p.add_argument('--niveau-uitlijnen', action='store_true', help='niveauverschil tussen A en B buiten beschouwing (referentie met ander uitgangsniveau)')
    p.add_argument('--streng', action='store_true', help='afsluitcode 1 als een register een vlag heeft')
    p.set_defaults(f=cmd_vergelijk)
    p = sub.add_parser('zelftest')
    p.set_defaults(f=cmd_zelftest)
    a = ap.parse_args()
    a.f(a)


if __name__ == '__main__':
    main()
