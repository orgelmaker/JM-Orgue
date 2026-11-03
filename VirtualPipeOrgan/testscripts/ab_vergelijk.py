"""Vergelijk twee opnames van dezelfde noot (JM-Orgue en een referentiespeler) op meetbare grootheden.

Hoort bij docs/AB_KLANKPROTOCOL.md. Per opname:
  niveau (RMS dB over het sustain-venster), toonhoogte (FFT-piek + afwijking in cent t.o.v. de andere),
  aanzettijd (10 -> 90 % van de omhullende), uitsterftijd na NoteOff (tijd tot -40 dB t.o.v. het sustain-niveau),
  spectrale zwaartepunt (Hz) en de verhouding boventonen/grondtoon.

Gebruik:
  python ab_vergelijk.py <a.wav|mp3> <b.wav|mp3> --aan 0.5 --uit 2.5 [--sustain-venster 0.8 2.2]
Tijden (s) zijn die van NoteOn en NoteOff in BEIDE opnames; lijn de opnames dus eerst uit op de
aanzet (dezelfde MIDI-opname, hetzelfde startmoment). De uitvoer is een tabel; een groot verschil
is een aanwijzing voor een luistercontrole, geen oordeel op zich.
"""
import argparse, math, subprocess, sys

import numpy as np

SR = 48000


def laad(pad):
    raw = subprocess.run(['ffmpeg', '-v', 'quiet', '-i', pad, '-ac', '1', '-ar', str(SR), '-f', 's16le', '-'],
                         capture_output=True).stdout
    return np.frombuffer(raw, np.int16).astype(float) / 32768.0


def db(x):
    return 20 * math.log10(max(x, 1e-9))


def omhullende(x, venster=0.01):
    n = int(venster * SR)
    k = len(x) // n
    return np.sqrt((x[:k * n].reshape(k, n) ** 2).mean(axis=1)), venster


def meet(x, aan, uit, sv):
    a0, a1 = int(sv[0] * SR), int(sv[1] * SR)
    sus = x[a0:a1]
    r = {'niveau_db': db(np.sqrt(np.mean(sus ** 2)))}
    sp = np.abs(np.fft.rfft(sus * np.hanning(len(sus)), len(sus) * 4))
    fr = np.fft.rfftfreq(len(sus) * 4, 1 / SR)
    band = (fr > 40) & (fr < 8000)
    f0 = fr[band][np.argmax(sp[band])]
    r['grondtoon_hz'] = f0
    r['zwaartepunt_hz'] = float((fr[band] * sp[band]).sum() / sp[band].sum())
    boven = sp[(fr > 1.5 * f0) & (fr < 8000)].sum()
    r['boventonen_db'] = db(boven / max(sp[(fr > 0.8 * f0) & (fr < 1.2 * f0)].sum(), 1e-9))
    env, dt = omhullende(x)
    i_aan = int(aan / dt)
    top = env[i_aan:i_aan + int(0.5 / dt)].max()
    t10 = next((i for i in range(i_aan, len(env)) if env[i] >= 0.1 * top), None)
    t90 = next((i for i in range(i_aan, len(env)) if env[i] >= 0.9 * top), None)
    r['aanzet_ms'] = (t90 - t10) * dt * 1000 if t10 is not None and t90 is not None else None
    i_uit = int(uit / dt)
    ref = env[max(i_aan, i_uit - int(0.3 / dt)):i_uit].mean()
    doel = ref * 10 ** (-40 / 20)
    t_stil = next((i for i in range(i_uit, len(env)) if env[i] <= doel), None)
    r['uitsterf_40db_ms'] = (t_stil - i_uit) * dt * 1000 if t_stil is not None else None
    return r


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('a'); ap.add_argument('b')
    ap.add_argument('--aan', type=float, required=True)
    ap.add_argument('--uit', type=float, required=True)
    ap.add_argument('--sustain-venster', type=float, nargs=2)
    o = ap.parse_args()
    sv = o.sustain_venster or [o.aan + 0.3, o.uit - 0.2]
    ra, rb = meet(laad(o.a), o.aan, o.uit, sv), meet(laad(o.b), o.aan, o.uit, sv)
    print(f'{"":22}{"A":>14}{"B":>14}{"verschil B-A":>16}')
    for k, eenheid in (('niveau_db', 'dB'), ('grondtoon_hz', 'Hz'), ('zwaartepunt_hz', 'Hz'), ('boventonen_db', 'dB'),
                       ('aanzet_ms', 'ms'), ('uitsterf_40db_ms', 'ms')):
        a, b = ra[k], rb[k]
        v = '-' if a is None or b is None else f'{b - a:+.1f} {eenheid}'
        fa = '-' if a is None else f'{a:.1f}'
        fb = '-' if b is None else f'{b:.1f}'
        print(f'{k:22}{fa:>14}{fb:>14}{v:>16}')
    cent = 1200 * math.log2(rb['grondtoon_hz'] / ra['grondtoon_hz'])
    print(f'{"toonhoogteverschil":22}{"":>14}{"":>14}{cent:>+13.1f} ct')


if __name__ == '__main__':
    main()
