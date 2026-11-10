# -*- coding: utf-8 -*-
"""F1: herhaald laden geeft het geheugen van de vorige lading terug.

Laadt een groot orgel (A) en een heel klein orgel (B) om en om en meet na elke
lading het privégeheugen (commit) van het app-proces, zodra dat tot rust is
gekomen. Controleert:

  1. na het kleine orgel is het geheugen van het grote terug bij het systeem
     (B < 50 % van A);
  2. opnieuw laden van hetzelfde grote orgel groeit niet (A volgende rondes
     binnen +10 % van de eerste);
  3. idem voor het kleine orgel (+25 %, het is klein en de basis telt mee).

Gebruik (Windows, PowerShell nodig voor de meting):
  python testscripts/geheugen_herhaald_laden.py <vpo-app.exe> [rondes] [orgel-A] [orgel-B]

Draait precies één app; stopt als er al een vpo-app.exe draait. Eindigt met
afsluitcode 1 als een controle faalt.
"""
import json
import os
import subprocess
import sys
import time
import urllib.request

B = "http://127.0.0.1:8765"
HIER = os.path.dirname(os.path.abspath(__file__))
STANDAARD_A = ("D:/Samplesets/Kolonics_Brasov/OrganDefinitions/"
               "Kolonics Brasov lite.Organ_Hauptwerk_xml")
STANDAARD_B = os.path.join(HIER, "go_testodf", "TestCresc.organ")


def post(pad, body=None):
    data = json.dumps(body).encode() if body is not None else b"{}"
    req = urllib.request.Request(B + pad, data=data, method="POST",
                                 headers={"Content-Type": "application/json"})
    return json.loads(urllib.request.urlopen(req, timeout=600).read() or b"{}")


def get(pad):
    return json.loads(urllib.request.urlopen(B + pad, timeout=30).read())


def aantal_apps():
    uit = subprocess.run(['tasklist', '/FI', 'IMAGENAME eq vpo-app.exe', '/NH'],
                         capture_output=True, text=True).stdout
    return sum(1 for r in uit.splitlines() if 'vpo-app.exe' in r)


def prive_mb(pid):
    """Privégeheugen (commit) van het proces in MB, via PowerShell."""
    uit = subprocess.run(
        ['powershell', '-NoProfile', '-Command',
         '(Get-Process -Id %d).PrivateMemorySize64' % pid],
        capture_output=True, text=True).stdout.strip()
    return int(uit) / 1e6


def tot_rust(pid, max_s=180):
    """Wacht tot het privégeheugen niet meer beweegt (twee metingen 6 s
    uit elkaar binnen 2 %) en geef de laatste waarde."""
    vorig = prive_mb(pid)
    t0 = time.time()
    while time.time() - t0 < max_s:
        time.sleep(6)
        nu = prive_mb(pid)
        if abs(nu - vorig) <= 0.02 * max(vorig, 1.0):
            return nu
        vorig = nu
    return vorig


def main():
    if len(sys.argv) < 2:
        sys.exit(__doc__)
    exe = sys.argv[1]
    rondes = int(sys.argv[2]) if len(sys.argv) > 2 else 3
    orgel_a = sys.argv[3] if len(sys.argv) > 3 else STANDAARD_A
    orgel_b = sys.argv[4] if len(sys.argv) > 4 else STANDAARD_B
    if aantal_apps():
        sys.exit('er draait al een vpo-app.exe; stop die eerst')
    p = subprocess.Popen([exe, '--test-api'], cwd=os.path.dirname(exe))
    meting = []   # (label, MB)
    try:
        for _ in range(60):
            try:
                urllib.request.urlopen(B + '/version', timeout=2).read()
                break
            except Exception:
                time.sleep(1)
        else:
            sys.exit('de app start niet')
        print('na het starten: %.0f MB' % prive_mb(p.pid))
        for r in range(rondes):
            for label, pad in (('A', orgel_a), ('B', orgel_b)):
                t0 = time.time()
                post('/load_organ', {'path': pad})
                mb = tot_rust(p.pid)
                g = get('/status').get('geheugen', {})
                naam = get('/organ').get('name')
                print('ronde %d %s %-28s %7.0f MB privé  (laden+rust %3.0f s; resident-cache %s MB, plafond %s MB)' % (
                    r + 1, label, (naam or '?')[:28], mb, time.time() - t0,
                    g.get('resident_mb'), g.get('plafond_mb')), flush=True)
                meting.append((label, mb))
    finally:
        subprocess.run(['taskkill', '/PID', str(p.pid), '/T', '/F'], capture_output=True)
        time.sleep(3)

    a = [mb for l, mb in meting if l == 'A']
    b = [mb for l, mb in meting if l == 'B']
    fouten = []
    for i, (ma, mb_) in enumerate(zip(a, b), 1):
        if not mb_ < 0.5 * ma:
            fouten.append('ronde %d: na het kleine orgel nog %.0f MB, grote was %.0f MB (niet < 50 %%)' % (i, mb_, ma))
    for i, ma in enumerate(a[1:], 2):
        if ma > 1.10 * a[0]:
            fouten.append('ronde %d: groot orgel %.0f MB tegen %.0f MB bij de eerste lading (> +10 %%)' % (i, ma, a[0]))
    for i, mb_ in enumerate(b[1:], 2):
        if mb_ > 1.25 * b[0]:
            fouten.append('ronde %d: klein orgel %.0f MB tegen %.0f MB bij de eerste lading (> +25 %%)' % (i, mb_, b[0]))
    if fouten:
        print('\nMISLUKT:')
        for f in fouten:
            print('  -', f)
        sys.exit(1)
    print('\nGESLAAGD: het geheugen van de vorige lading komt terug en groeit niet.')


if __name__ == '__main__':
    main()
