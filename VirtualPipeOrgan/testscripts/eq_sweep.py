#!/usr/bin/env python3
"""End-to-end-bewijs van de uitgangscorrectie (0.7.78): een sinus loopt DOOR de
hele keten (orgel-EQ, uitgangscorrectie, limiter) en de niveaumeter moet dan
precies de berekende respons (GET /audio/eq_response) laten zien.

Vereist: de app met --test-api (poort 8765), audio actief, geen registers
getrokken. Gebruik:  python testscripts/eq_sweep.py [profiel]  (standaard: de
actieve profielsoort). Zet tijdelijk een testcorrectie op dat profiel en zet
daarna alles terug zoals het was.

Werkwijze: sinus op -20 dBFS (de begrenzer grijpt dan niet in, ook niet bij
+6 dB), kanaal 0 (de niveaumeters meten alleen kanaal 0 en 1), frequenties
50 Hz..10 kHz. De meter is de piek per audio-callback, exponentieel
gladgestreken (0,95 per callback): frequenties onder sample_rate/(2·frames)
worden overgeslagen en de inlooptijd schaalt met de buffergrootte (zie
meter_grenzen). Negatieve controle: correctie uit → vlak binnen 0,15 dB.
"""
import json, math, sys, time, urllib.parse, urllib.request

API = 'http://127.0.0.1:8765'
LEVEL_DB = -20.0
# 50 Hz .. 10 kHz: de piekmeter neemt per callback (10 ms bij 480 frames) het
# maximum van de samples. Onder ~40 Hz past er geen hele periode in een
# callback en boven ~12 kHz valt de top tussen twee samples; de gemeten piek
# zakt dan 0,2-0,9 dB zonder dat de EQ iets doet (gemeten bij de bouw van
# 0.7.78: 20 Hz -0,9 dB, 30 Hz -0,2 dB, 16 kHz -0,6 dB, alles ertussen 0,00).
FREQS = [50, 80, 105, 150, 204, 300, 500, 800, 1585, 2500, 4207, 6500, 8000, 10000]
TESTBANDEN = [  # een herkenbare curve: laag omhoog, kuil rond 204 Hz, piek rond 4,2 kHz, top omlaag
    {'type': 'lowshelf', 'freq': 105, 'gain_db': 6.0, 'q': 0.7},
    {'type': 'peak', 'freq': 204, 'gain_db': -4.0, 'q': 0.5},
    {'type': 'peak', 'freq': 4207, 'gain_db': 5.0, 'q': 3.0},
    {'type': 'highshelf', 'freq': 10000, 'gain_db': -5.0, 'q': 0.7},
]

def get(p):
    with urllib.request.urlopen(API + p, timeout=10) as r:
        return json.loads(r.read().decode())

def post(p, body=None):
    data = json.dumps(body).encode() if body is not None else b''
    req = urllib.request.Request(API + p, data=data, headers={'Content-Type': 'application/json'}, method='POST')
    with urllib.request.urlopen(req, timeout=30) as r:
        t = r.read().decode()
        return json.loads(t) if t else {}

def meter_grenzen():
    """De niveaumeter is de max |sample| PER CALLBACK, daarna exponentieel
    gladgestreken (0,95 per callback). Daaruit volgen twee grenzen die van de
    buffergrootte afhangen: (1) onder sample_rate/(2·frames) bevat niet elke
    callback een top van de sinus → te lage piek (bij 480 frames: < 50 Hz);
    (2) de inlooptijd is 150 callbacks (0,95^150 ≈ 0,05 % rest), bij 10 ms-
    callbacks 1,5 s, bij 1024 frames/48 kHz 3,2 s."""
    st = get('/status')
    sr = st.get('sample_rate') or 48000
    frames = st.get('render_frames') or st.get('buffer_frames') or int(sr * 0.010)
    return sr / (2.0 * frames), max(1.2, 150.0 * frames / sr), frames

def meet(freq, settle):
    post('/audio/test_signal?' + urllib.parse.urlencode({'channel': 0, 'kind': 2, 'level_db': LEVEL_DB, 'freq': freq}))
    time.sleep(settle)
    v = []
    for _ in range(10):
        v.append(get('/status')['peak_left']); time.sleep(0.03)
    piek = sum(v) / len(v)
    return 20 * math.log10(max(piek, 1e-9)) - LEVEL_DB

def sweep(label, tolerantie):
    fouten = 0
    f_min, settle, frames = meter_grenzen()
    print(f'--- {label}  (callback {frames} frames: meetbaar vanaf {f_min:.0f} Hz, inlooptijd {settle:.1f} s)')
    for f in FREQS:
        if f < f_min:
            print(f"SKIP {f:6d} Hz  halve periode langer dan één callback; de piekmeter is een piek per callback")
            continue
        gemeten = meet(f, settle)
        verwacht = get(f'/audio/eq_response?freq={f}')['total_db']
        ok = abs(gemeten - verwacht) <= tolerantie
        fouten += 0 if ok else 1
        print(f"{'OK  ' if ok else 'FOUT'} {f:6d} Hz  gemeten {gemeten:+6.2f} dB  berekend {verwacht:+6.2f} dB  verschil {gemeten - verwacht:+5.2f}")
    return fouten

def main():
    st = get('/status')
    if not st.get('audio_running'):
        print('audio draait niet'); return 2
    if st.get('voice_count', 0) > 0:
        print('er klinken nog stemmen; eerst alles loslaten'); return 2
    eq = get('/audio/eq_state')
    profiel = sys.argv[1] if len(sys.argv) > 1 else eq.get('output_profile')
    if not profiel:
        print('geen profielsoort actief; geef er een op (speakers|headphones)'); return 2
    oud = get(f'/settings/output_eq?profile={profiel}')
    oud_actief = eq.get('output_profile')
    fouten = 0
    try:
        post(f'/settings/output_eq/activate?profile={profiel}')
        # Negatieve controle: correctie uit → vlak.
        post(f'/settings/output_eq?profile={profiel}', {'enabled': False, 'preamp_db': 0, 'bands': []})
        fouten += sweep('correctie uit (negatieve controle, ±0,15 dB)', 0.15)
        # Testcurve met Auto-voorversterking.
        r = post(f'/settings/output_eq?profile={profiel}', {'enabled': True, 'preamp_db': 'auto', 'bands': TESTBANDEN})
        print(f"testcurve gezet: {len(r['bands'])} banden, voorversterking {r['preamp_db']:+.2f} dB")
        fouten += sweep('testcurve door de keten (±0,5 dB)', 0.5)
    finally:
        post('/audio/test_signal')  # uit
        # Alles terug: de oude correctie van dit profiel en de oude actieve soort.
        post(f'/settings/output_eq?profile={profiel}', {
            'enabled': oud['enabled'], 'preamp_db': 'auto' if oud['preamp_auto'] else oud['preamp_db'],
            'preset_id': oud.get('preset_id'), 'preset_naam': oud.get('preset_naam'),
            'bands': [{'band_type': b['band_type'], 'freq': b['freq'], 'gain_db': b['gain_db'], 'bandwidth': b['bandwidth'],
                       'q': b.get('q'), 'channel': b.get('channel'), 'enabled': b['enabled']} for b in oud['bands']],
        })
        post('/settings/output_eq/activate?profile=' + (oud_actief or 'none'))
    print('RESULTAAT:', 'alles OK' if fouten == 0 else f'{fouten} afwijkingen')
    return 1 if fouten else 0

if __name__ == '__main__':
    sys.exit(main())
