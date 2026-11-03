"""Reproduceerbare prestatiemeting van de import- en afspeelketen (werkt voor elk orgel, ook CODM).

Per orgel en scenario (enkel register / tutti met koppels / kort en lang spel) wordt gemeten:
  parserduur (CODM-preflight), laadtijd, piek-RAM, stemmenpiek, callbackbelasting p50/p95/p99/max
  (fractie van de buffertijd; > 1,0 = te laat), late callbacks, RT-drops, preload-tekorten,
  release-randhits en de bovengrens voor de MIDI->audio-vertraging.
Alles samen met computer, OS, uitvoerhost/-apparaat, samplerate en buffermaat, zodat een
tweede meting op dezelfde machine vergelijkbaar is.

Gebruik:
  python codm_benchmark.py                       eigen synthetische proeforgels (geen samples nodig)
  python codm_benchmark.py <orgel> [<orgel> ...] eigen paden (ook grote commerciele sets, lokaal)
Uitvoer: testen/codm-benchmark/benchmark-<datum>.md en .json

Eerlijke kanttekeningen (staan ook in het rapport):
  - De MIDI->audio-vertraging is een BOVENGRENS via HTTP en pollen; een echte meting vraagt een
    hardware-loopback.
  - MAX_POLYPHONY (32768) is een codeplafond, geen prestatieclaim: kijk naar p99 en late callbacks.
"""
import datetime, json, os, statistics, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from codm_meetlib import App, REPO, alle_stops, laad, machine_info, piek_ram_mb, req
import maak_codm_koppeltest, maak_codm_releasetest

UIT = os.path.join(REPO, 'testen', 'codm-benchmark')
NOTEN = [36, 43, 48, 52, 55, 60, 64, 67, 72, 76, 79, 84]


def percentiel(w, p):
    if not w:
        return None
    w = sorted(w)
    return round(w[min(len(w) - 1, int(round(p / 100 * (len(w) - 1))))], 4)


def draai(duur, gebeurtenissen):
    """Voer (tijd, functie)-gebeurtenissen uit en bemonster elke ~40 ms de slechtste callback."""
    totalen, stemmen = [], 0
    gebeurtenissen = sorted(gebeurtenissen, key=lambda g: g[0])
    t0 = time.perf_counter()
    req('/perf/slechtste')  # wissen
    i = 0
    while True:
        nu = time.perf_counter() - t0
        while i < len(gebeurtenissen) and gebeurtenissen[i][0] <= nu:
            gebeurtenissen[i][1]()
            i += 1
        p = req('/perf/slechtste')
        totalen.append(p['totaal'])
        stemmen = max(stemmen, p['stemmen'])
        if nu > duur and i >= len(gebeurtenissen):
            break
        time.sleep(0.04)
    return totalen, stemmen


def scenario(naam, org, stops, duur, gebeurtenissen):
    st0 = req('/status')
    tot, stemmen = draai(duur, gebeurtenissen)
    st1 = req('/status')
    req('/panic', 'POST')
    time.sleep(1.0)
    l0, l1 = st0['loader'], st1['loader']
    return {
        'scenario': naam,
        'stemmenpiek': stemmen,
        'callback_p50': percentiel(tot, 50), 'callback_p95': percentiel(tot, 95),
        'callback_p99': percentiel(tot, 99), 'callback_max': round(max(tot), 4) if tot else None,
        'late_callbacks': st1['render_overloads'] - st0['render_overloads'],
        'rt_drops': st1['rt_drops'] - st0['rt_drops'],
        'preload_tekorten': l1['preload_underruns'] - l0['preload_underruns'],
        'release_randhits': l1['release_edge_hits'] - l0['release_edge_hits'],
        'laadwachtrij_vol': l1['queue_full'] - l0['queue_full'],
        'laadlatentie_max_ms': round(l1['latency_max_ms'], 1),
        'monsters': len(tot),
    }


def latentie_bovengrens(n=20):
    """Tijd van /midi/inject tot een piek in /status (bovengrens, incl. HTTP en pollen)."""
    uit = []
    for _ in range(n):
        req('/panic', 'POST')
        time.sleep(0.6)
        req('/status')  # pieken wissen
        t0 = time.perf_counter()
        req('/notes/60/on?velocity=100', 'POST')
        while time.perf_counter() - t0 < 1.0:
            if req('/status')['peak_left'] > 0.005:
                uit.append((time.perf_counter() - t0) * 1000)
                break
        req('/notes/60/off', 'POST')
    return {'n': len(uit), 'mediaan_ms': round(statistics.median(uit), 1) if uit else None,
            'max_ms': round(max(uit), 1) if uit else None}


def meet_orgel(pad):
    res = {'orgel': pad}
    # Parserduur (alleen CODM kent een preflight met een volledige parse + sampleresolutie).
    t = time.perf_counter()
    try:
        rep = req('/import/preflight', 'POST', {'path': pad.replace('\\', '/')})
        res['preflight_ms'] = round((time.perf_counter() - t) * 1000)
        res['verwacht_stops'] = rep.get('verwacht', {}).get('stops')
    except Exception:
        res['preflight_ms'] = None
    t = time.perf_counter()
    org = laad(pad)
    res['laadtijd_s'] = round(time.perf_counter() - t - 4.0, 1)  # laad() wacht 4 s na afloop
    st = req('/status')
    res['stops'] = sum(len(d['stops']) for d in org['divisions'])
    res['load_stappen_ms'] = st.get('load_stappen_ms')
    stops = alle_stops(org)
    ids = list(stops.values())
    koppels = [c['id'] for c in (org.get('couplers') or [])]

    sc = []
    # 1. enkel register
    req('/panic', 'POST')
    req('/stops/set', 'POST', {'ids': ids[:1]})
    req('/couplers/set', 'POST', {'ids': []})
    time.sleep(0.5)
    sc.append(scenario('enkel register', org, stops, 3.5, [
        (0.2, lambda: req('/notes/60/on?velocity=100', 'POST')),
        (3.0, lambda: req('/notes/60/off', 'POST')),
    ]))
    # 2. tutti met koppels
    req('/stops/set', 'POST', {'ids': ids})
    req('/couplers/set', 'POST', {'ids': koppels})
    time.sleep(0.5)
    sc.append(scenario('tutti + koppels', org, stops, 5.0, (
        [(0.2 + 0.05 * k, (lambda n=n: req(f'/notes/{n}/on?velocity=100', 'POST'))) for k, n in enumerate(NOTEN)]
        + [(4.0, (lambda n=n: req(f'/notes/{n}/off', 'POST'))) for n in NOTEN])))
    # 3. kort en lang spel
    ev = []
    for k in range(40):
        n = 48 + (k % 24)
        ev.append((0.2 + k * 0.125, (lambda n=n: req(f'/notes/{n}/on?velocity=100', 'POST'))))
        ev.append((0.2 + k * 0.125 + 0.08, (lambda n=n: req(f'/notes/{n}/off', 'POST'))))
    ev += [(5.6, (lambda n=n: req(f'/notes/{n}/on?velocity=100', 'POST'))) for n in (48, 55, 60, 64)]
    ev += [(8.6, (lambda n=n: req(f'/notes/{n}/off', 'POST'))) for n in (48, 55, 60, 64)]
    sc.append(scenario('kort en lang spel', org, stops, 9.5, ev))
    res['scenarios'] = sc
    req('/stops/set', 'POST', {'ids': ids[:1]})
    req('/couplers/set', 'POST', {'ids': []})
    time.sleep(0.5)
    res['midi_naar_audio'] = latentie_bovengrens()
    res['piek_ram_mb'] = piek_ram_mb()
    return res


def rapport(resultaten, info, audio):
    r = ['# Prestatiemeting import en afspelen', '',
         f'Datum: {datetime.datetime.now():%Y-%m-%d %H:%M}', '',
         '## Machine en instellingen', '',
         f"- Computer: {info.get('cpu', info.get('processor'))}, RAM {info.get('ram_gb', '?')} GB",
         f"- OS: {info['platform']}",
         f"- Audio: host `{audio['audio_host']}`, apparaat `{audio['audio_device']}`, {audio['sample_rate']} Hz, "
         f"{audio['channels']} kanalen, buffer "
         + (f"{audio['buffer_frames']} frames ({1000.0 * audio['buffer_frames'] / max(audio['sample_rate'], 1):.2f} ms)"
            if audio['buffer_frames'] else 'door het apparaat bepaald (niet opgegeven)'),
         f"- Polyfonie-doel nu: {audio['polyphony']} stemmen (het codeplafond MAX_POLYPHONY is 32768: een plafond, geen prestatieclaim)", '']
    for o in resultaten:
        r += [f"## {os.path.basename(o['orgel'])}", '',
              f"{o['stops']} registers; preflight/parse {o.get('preflight_ms')} ms; laadtijd {o['laadtijd_s']} s; "
              f"piek-RAM {o['piek_ram_mb']} MB; MIDI->audio bovengrens (HTTP+poll) mediaan {o['midi_naar_audio']['mediaan_ms']} ms "
              f"/ max {o['midi_naar_audio']['max_ms']} ms (n={o['midi_naar_audio']['n']})", '',
              '| scenario | stemmen | p50 | p95 | p99 | max | late | RT-drops | preload-tekort | rand-hits |',
              '|---|---|---|---|---|---|---|---|---|---|']
        for s in o['scenarios']:
            r.append(f"| {s['scenario']} | {s['stemmenpiek']} | {s['callback_p50']} | {s['callback_p95']} | {s['callback_p99']} | "
                     f"{s['callback_max']} | {s['late_callbacks']} | {s['rt_drops']} | {s['preload_tekorten']} | {s['release_randhits']} |")
        r.append('')
    r += ['Callbackbelasting = rendertijd van de zwaarste callback per bemonsterd interval als fractie van de buffertijd '
          '(1,0 = de buffer is precies op). "late" = callbacks over de buffertijd sinds de scenariostart.', '',
          'De MIDI->audio-waarde is een bovengrens (HTTP + pollen) en hangt af van de aanzet van het sample: een zachte aanzet '
          'geeft een hogere waarde dan een directe. Niet gemeten: de echte vertraging (vraagt een hardware-loopback) en dropouts '
          'op het apparaat zelf (alleen zichtbaar aan de driver). Vergelijk altijd met dezelfde machine, instellingen en buffermaat.']
    return '\n'.join(r)


def main():
    os.makedirs(UIT, exist_ok=True)
    paden = sys.argv[1:]
    if not paden:
        maak_codm_koppeltest.main(os.path.join(UIT, 'koppel'))
        maak_codm_releasetest.main(os.path.join(UIT, 'release'))
        paden = [os.path.join(UIT, 'koppel', 'OrganDefinitions', 'Koppel.CustomOrgan_Hauptwerk_xml'),
                 os.path.join(UIT, 'release', 'OrganDefinitions', 'Mini.CustomOrgan_Hauptwerk_xml')]
    resultaten = []
    audio = {}
    for p in paden:  # per orgel een verse app: dan is de RAM-piek van dat orgel alleen
        with App():
            resultaten.append(meet_orgel(p))
            audio = req('/status')
        time.sleep(2)
    info = machine_info()
    stempel = datetime.datetime.now().strftime('%Y%m%d-%H%M%S')
    json.dump({'machine': info, 'audio': {k: audio[k] for k in ('audio_host', 'audio_device', 'sample_rate', 'channels', 'buffer_frames', 'polyphony')},
               'resultaten': resultaten}, open(os.path.join(UIT, f'benchmark-{stempel}.json'), 'w', encoding='utf-8'), indent=2, ensure_ascii=False)
    md = rapport(resultaten, info, audio)
    open(os.path.join(UIT, f'benchmark-{stempel}.md'), 'w', encoding='utf-8').write(md)
    print(md)


if __name__ == '__main__':
    main()
