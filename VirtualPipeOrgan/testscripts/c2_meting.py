"""Vaste meting voor C2 (compacte sample-opslag): geheugen, laadtijd en belasting van één orgel.

Start de app met de test-API, laadt het orgel, meet het geheugen van het proces, trekt alle registers en speelt
elke toets van 36 tot 96 op de kanalen 0 tot en met 4 kort aan (zodat de volledige opnamen geladen worden), en
meet daarna opnieuw. Het orgel wordt twee keer geladen in hetzelfde proces: de tweede keer is "warm" (de
bestanden staan dan in de bestandscache van het systeem).

LET OP: de test-API deelt de gegevensmap van de gebruiker. Maak vooraf een reservekopie van
`organ_library.json`, `audio_config.json` en de `.jm-settings.json` naast het orgel, en zet ze daarna terug.

Gebruik:
    python c2_meting.py --exe <pad naar vpo-app.exe> --orgel <definitie> --uit <resultaat.json>
"""
import argparse
import json
import subprocess
import time
import urllib.request

API = 'http://127.0.0.1:8765'


def api(methode, pad, body=None, timeout=600):
    data = None if body is None else json.dumps(body).encode('utf-8')
    req = urllib.request.Request(API + pad, data=data, method=methode, headers={'Content-Type': 'application/json'})
    with urllib.request.urlopen(req, timeout=timeout) as r:
        tekst = r.read().decode('utf-8')
        return json.loads(tekst) if tekst else None


def geheugen(pid):
    """Private bytes en working set van het proces, in MB (Windows)."""
    uit = subprocess.run(
        ['powershell', '-NoProfile', '-Command',
         '$p = Get-Process -Id %d; "$($p.PrivateMemorySize64) $($p.WorkingSet64)"' % pid],
        capture_output=True, text=True).stdout.split()
    return {'private_mb': int(uit[0]) // 1_000_000, 'workingset_mb': int(uit[1]) // 1_000_000}


def wacht_op_api(seconden=60):
    eind = time.time() + seconden
    while time.time() < eind:
        try:
            return api('GET', '/version', timeout=3)
        except Exception:
            time.sleep(1)
    raise SystemExit('test-API niet bereikbaar')


def speel_alles():
    org = api('GET', '/organ')
    ids = [s['id'] for d in org['divisions'] for s in d['stops']]
    api('POST', '/stops/set', {'ids': ids})
    for noot in range(36, 97):
        for kanaal in range(5):
            api('POST', '/notes/%d/on?velocity=100&channel=%d' % (noot, kanaal))
        time.sleep(0.25)
        for kanaal in range(5):
            api('POST', '/notes/%d/off?channel=%d' % (noot, kanaal))
    time.sleep(8)  # laders en staarten laten uitlopen
    api('POST', '/stops/set', {'ids': []})
    return len(ids)


def main():
    a = argparse.ArgumentParser()
    a.add_argument('--exe', required=True)
    a.add_argument('--orgel', required=True)
    a.add_argument('--uit', required=True)
    a = a.parse_args()
    proc = subprocess.Popen([a.exe, '--test-api'])
    try:
        versie = wacht_op_api()
        time.sleep(3)
        uitkomst = {'versie': versie, 'orgel': a.orgel, 'ladingen': []}
        uitkomst['leeg'] = geheugen(proc.pid)
        for ronde in ('eerste', 'tweede'):
            t0 = time.time()
            r = api('POST', '/load_organ', {'path': a.orgel})
            laadtijd = time.time() - t0
            time.sleep(5)
            na_laden = geheugen(proc.pid)
            regs = speel_alles()
            na_spelen = geheugen(proc.pid)
            status = api('GET', '/status')
            slechtste = api('GET', '/perf/slechtste')
            uitkomst['ladingen'].append({
                'ronde': ronde,
                'registers': r.get('stop_count'),
                'getrokken': regs,
                'laadtijd_s': round(laadtijd, 1),
                'na_laden': na_laden,
                'na_spelen': na_spelen,
                'status_loader': {k: v for k, v in status.items() if 'loader' in k or 'preload' in k or 'overload' in k or 'sample' in k},
                'slechtste_callback': slechtste,
            })
            print(ronde, 'laadtijd', round(laadtijd, 1), 's | na laden', na_laden, '| na spelen', na_spelen)
        with open(a.uit, 'w', encoding='utf-8') as f:
            json.dump(uitkomst, f, indent=2, ensure_ascii=False)
    finally:
        proc.kill()
        proc.wait(timeout=30)


if __name__ == '__main__':
    main()
