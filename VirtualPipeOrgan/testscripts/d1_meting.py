"""TIJDELIJK UITGESCHAKELD: deze meting moet eerst veilig geïsoleerd worden.

Vaste meting voor D1/F3 (mengkern zonder allocaties, orgelwissel als één generatie).

Vraagt een testbouw met `--features rt-toezicht` (anders is `allocaties` in /perf/slechtste null). Per scenario wordt
/perf/slechtste vooraf gewist en achteraf gelezen: allocaties in de callback, slechtste callback met segmenten.

Scenario's (op orgel A; de wissel gaat naar orgel B):
  spel       alle registers, toets 36–96 op kanaal 0–4 kort aan
  tremulant  akkoord vast, elke divisie tremulant aan/uit
  crescendo  akkoord vast, crescendo 0 → 30 → 0
  opname     MP3-opname aan, kort spelen, opname uit
  wissel     akkoord vast, orgel B laden terwijl er noten blijven komen; daarna vastgehouden noten en stemmen

De test-API deelt de gegevensmap van de gebruiker: dit script bewaart vooraf de JSON-bestanden uit die map en de
`.jm-settings.json` naast beide orgels, en zet ze na afloop terug.

Gebruik:
    python d1_meting.py --exe <vpo-app.exe> --a <definitie> --b <definitie> --uit <resultaat.json> [--kladmap <map>]
"""
import argparse
import glob
import json
import os
import shutil
import subprocess
import threading
import time
import urllib.request

API = 'http://127.0.0.1:8765'
GEGEVENS = os.path.join(os.environ['APPDATA'], 'nl.jm-orgue.app')


def api(methode, pad, body=None, timeout=600):
    data = None if body is None else json.dumps(body).encode('utf-8')
    req = urllib.request.Request(API + pad, data=data, method=methode, headers={'Content-Type': 'application/json'})
    with urllib.request.urlopen(req, timeout=timeout) as r:
        tekst = r.read().decode('utf-8')
        return json.loads(tekst) if tekst else None


def probeer(methode, pad, body=None):
    try:
        return api(methode, pad, body)
    except Exception as e:  # noqa: BLE001 — een scenario mag ontbreken (geen crescendo, geen tremulant)
        return {'fout': str(e)}


def wacht_op_api(seconden=60):
    eind = time.time() + seconden
    while time.time() < eind:
        try:
            return api('GET', '/version', timeout=3)
        except Exception:
            time.sleep(1)
    raise SystemExit('test-API niet bereikbaar')


def bewaar(paden, kladmap):
    kopie = {}
    for i, p in enumerate(paden):
        if os.path.isfile(p):
            doel = os.path.join(kladmap, 'bewaard_%d_%s' % (i, os.path.basename(p)))
            shutil.copy2(p, doel)
            kopie[p] = doel
    return kopie


def zet_terug(kopie, paden):
    for p in paden:
        if p in kopie:
            shutil.copy2(kopie[p], p)
        elif os.path.isfile(p):
            os.remove(p)  # bestond vooraf niet: door de meting ontstaan


def slechtste():
    s = api('GET', '/perf/slechtste')
    return {
        'allocaties': s.get('allocaties'),
        'aanvragen_per_plek': s.get('aanvragen_per_plek'),
        'totaal': s.get('totaal'),
        'segmenten': s.get('segmenten'),
        'stemmen': s.get('stemmen'),
        'stemstarts': s.get('stemstarts'),
        'commandos': s.get('commandos'),
    }


def akkoord(aan, kanalen=range(5), noten=(48, 52, 55, 60, 64, 67, 72)):
    for k in kanalen:
        for n in noten:
            pad = '/notes/%d/on?velocity=100&channel=%d' % (n, k) if aan else '/notes/%d/off?channel=%d' % (n, k)
            api('POST', pad)


def alle_registers():
    org = api('GET', '/organ')
    ids = [s['id'] for d in org['divisions'] for s in d['stops']]
    api('POST', '/stops/set', {'ids': ids})
    return org


def scenario_spel():
    alle_registers()
    for noot in range(36, 97):
        for k in range(5):
            api('POST', '/notes/%d/on?velocity=100&channel=%d' % (noot, k))
        time.sleep(0.1)
        for k in range(5):
            api('POST', '/notes/%d/off?channel=%d' % (noot, k))
    time.sleep(4)


def scenario_tremulant():
    org = alle_registers()
    akkoord(True)
    for d in org['divisions']:
        for aan in ('1', '0'):
            probeer('POST', '/tremulant?division=%s&active=%s' % (urllib.request.quote(d['name']), aan))
            time.sleep(0.3)
    akkoord(False)
    time.sleep(3)


def scenario_crescendo():
    """Crescendo-achtig: met een vastgehouden akkoord steeds één register erbij en daarna weer eraf (NoteOn-fan-out
    en ReleaseStop, het werk van een trapwissel). Los van een ingesteld crescendo, dat niet elke set heeft."""
    org = api('GET', '/organ')
    ids = [s['id'] for d in org['divisions'] for s in d['stops']]
    api('POST', '/stops/set', {'ids': []})
    akkoord(True)
    for n in list(range(0, len(ids) + 1)) + list(range(len(ids), -1, -1)):
        api('POST', '/stops/set', {'ids': ids[:n]})
        time.sleep(0.08)
    akkoord(False)
    time.sleep(3)


def scenario_opname(kladmap):
    alle_registers()
    probeer('POST', '/record/start', {'path': os.path.join(kladmap, 'd1_opname.mp3')})
    akkoord(True)
    time.sleep(2)
    akkoord(False)
    time.sleep(1)
    probeer('POST', '/record/stop')
    time.sleep(2)


def scenario_wissel(orgel_b):
    alle_registers()
    akkoord(True)
    stop = threading.Event()
    gestuurd = [0]

    def noten_tijdens_laden():
        n = 0
        while not stop.is_set():
            noot = 40 + (n % 40)
            try:
                api('POST', '/notes/%d/on?velocity=100&channel=0' % noot, timeout=5)
                time.sleep(0.02)
                api('POST', '/notes/%d/off?channel=0' % noot, timeout=5)
                gestuurd[0] += 1
            except Exception:
                pass
            n += 1

    t = threading.Thread(target=noten_tijdens_laden, daemon=True)
    t.start()
    t0 = time.time()
    r = api('POST', '/load_organ', {'path': orgel_b})
    laadtijd = time.time() - t0
    time.sleep(1)
    stop.set()
    t.join(timeout=10)
    akkoord(False)
    time.sleep(4)
    status = api('GET', '/status')
    return {
        'laadtijd_s': round(laadtijd, 1),
        'registers_b': r.get('stop_count') if isinstance(r, dict) else None,
        'noten_tijdens_laden': gestuurd[0],
        'vastgehouden_na': probeer('GET', '/held_notes'),
        'stemmen_na_4s': status.get('voice_count', status.get('voices')),
    }


def main():
    raise SystemExit(
        'Deze meting is tijdelijk uitgeschakeld: de bestaande reservekopie mist de '
        'nieuwe orgelbestanden en kan instellingen achterlaten of verliezen. '
        'Werk het script eerst bij voor een volledig geïsoleerde testomgeving.'
    )
    a = argparse.ArgumentParser()
    a.add_argument('--exe', required=True)
    a.add_argument('--a', required=True)
    a.add_argument('--b', required=True)
    a.add_argument('--uit', required=True)
    a.add_argument('--kladmap', default=os.environ.get('TEMP', '.'))
    a = a.parse_args()

    paden = glob.glob(os.path.join(GEGEVENS, '*.json'))
    for orgel in (a.a, a.b):
        paden += glob.glob(os.path.join(os.path.dirname(orgel), '*.jm-settings.json'))
        paden += glob.glob(os.path.join(os.path.dirname(os.path.dirname(orgel)), '*.jm-settings.json'))
    kopie = bewaar(paden, a.kladmap)

    proc = subprocess.Popen([a.exe, '--test-api'])
    uitkomst = {'exe': a.exe, 'a': a.a, 'b': a.b, 'scenarios': {}}
    try:
        uitkomst['versie'] = wacht_op_api()
        time.sleep(3)
        api('POST', '/load_organ', {'path': a.a})
        time.sleep(4)
        api('GET', '/perf/slechtste')  # wissen: het laden zelf telt hier niet
        for naam, fn in (('spel', scenario_spel), ('tremulant', scenario_tremulant), ('crescendo', scenario_crescendo),
                         ('opname', lambda: scenario_opname(a.kladmap))):
            api('POST', '/stops/set', {'ids': []})
            time.sleep(2)
            api('GET', '/perf/slechtste')
            fn()
            uitkomst['scenarios'][naam] = slechtste()
            print(naam, json.dumps(uitkomst['scenarios'][naam]['allocaties']), 'plek', uitkomst['scenarios'][naam]['aanvragen_per_plek'], 'totaal', uitkomst['scenarios'][naam]['totaal'])
        api('GET', '/perf/slechtste')
        extra = scenario_wissel(a.b)
        uitkomst['scenarios']['wissel'] = dict(slechtste(), **extra)
        print('wissel', json.dumps(uitkomst['scenarios']['wissel']['allocaties']), 'plek', uitkomst['scenarios']['wissel']['aanvragen_per_plek'], extra)
        with open(a.uit, 'w', encoding='utf-8') as f:
            json.dump(uitkomst, f, indent=2, ensure_ascii=False)
    finally:
        proc.kill()
        proc.wait(timeout=30)
        time.sleep(1)
        zet_terug(kopie, paden)


if __name__ == '__main__':
    main()
