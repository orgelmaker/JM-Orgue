"""Gedeelde hulpfuncties voor de CODM-integratieproef en -benchmark.

Praat met de test-API van JM-Orgue (poort 8765, start met `--test-api`) en analyseert de
opname met ffmpeg + numpy. Alles werkt op eigen, zelf gesynthetiseerde fixtures; er zijn
geen commerciele samples nodig.
"""
import json, math, os, platform, shutil, subprocess, sys, time, urllib.request

HIER = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.abspath(os.path.join(HIER, os.pardir, os.pardir))
APP_MAP = os.path.abspath(os.path.join(HIER, os.pardir))
API = 'http://127.0.0.1:8765'


def req(pad, methode='GET', body=None, timeout=120):
    data = json.dumps(body).encode() if body is not None else (b'' if methode == 'POST' else None)
    r = urllib.request.Request(API + pad, data=data, method=methode, headers={'Content-Type': 'application/json'})
    t = urllib.request.urlopen(r, timeout=timeout).read()
    try:
        return json.loads(t)
    except Exception:
        return t


def app_draait():
    try:
        req('/status', timeout=2)
        return True
    except Exception:
        return False


class App:
    """Start (of hergebruik) een app met test-API."""

    def __init__(self, exe=None):
        self.exe = exe or os.path.join(APP_MAP, 'target', 'release', 'vpo-app.exe' if os.name == 'nt' else 'vpo-app')
        self.proces = None

    def __enter__(self):
        if app_draait():
            print('(bestaande app met test-API hergebruikt)')
            return self
        if not os.path.exists(self.exe):
            raise SystemExit(f'App niet gevonden: {self.exe} (bouw eerst: cargo build --release -p vpo-app)')
        self.proces = subprocess.Popen([self.exe, '--test-api'], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        for _ in range(60):
            if app_draait():
                return self
            time.sleep(1)
        raise SystemExit('App met test-API startte niet binnen 60 s')

    def __exit__(self, *a):
        if self.proces:
            self.proces.terminate()
            try:
                self.proces.wait(10)
            except Exception:
                self.proces.kill()


class Resultaat:
    def __init__(self):
        self.fouten = 0
        self.regels = []

    def ok(self, voorwaarde, tekst):
        self.regels.append(('OK   ' if voorwaarde else 'FOUT ') + tekst)
        print(self.regels[-1])
        if not voorwaarde:
            self.fouten += 1
        return voorwaarde

    def overslaan(self, tekst):
        self.regels.append('SKIP ' + tekst)
        print(self.regels[-1])


def laad(pad, timeout=600):
    req('/load_organ', 'POST', {'path': pad.replace('\\', '/')}, timeout=timeout)
    time.sleep(3)
    req('/motor', 'POST', {'on': True})
    time.sleep(1)
    return req('/organ')


def alle_stops(org):
    return {s['name']: s['id'] for d in org['divisions'] for s in d['stops']}


def pcm(mp3):
    import numpy as np
    raw = subprocess.run(['ffmpeg', '-v', 'quiet', '-i', mp3, '-ac', '1', '-ar', '48000', '-f', 's16le', '-'],
                         capture_output=True).stdout
    return np.frombuffer(raw, np.int16).astype(float)


def opname(tmp, houd=1.0, noten=(), kanaal_noten=()):
    """Neem `houd` seconden op terwijl noten ingedrukt zijn. `noten` = /notes-aanslagen,
    `kanaal_noten` = (kanaal, noot) via de MIDI-route (koppels werken alleen daarover)."""
    req('/record/start', 'POST', {})
    time.sleep(0.2)
    for n in noten:
        req(f'/notes/{n}/on?velocity=100', 'POST')
    for kanaal, n in kanaal_noten:
        req(f'/midi/inject?note={n}&velocity=100&channel={kanaal}', 'POST')
    time.sleep(houd * 0.7)
    stemmen = req('/status')['voice_count']
    time.sleep(houd * 0.3 + 0.2)
    r = req('/record/stop', 'POST')
    for n in noten:
        req(f'/notes/{n}/off', 'POST')
    for kanaal, n in kanaal_noten:
        req(f'/midi/inject?note={n}&on=0&channel={kanaal}', 'POST')
    shutil.copy(r['path'], tmp)
    return stemmen


def wacht_tot_stil(max_s=12):
    t0 = time.time()
    while time.time() - t0 < max_s:
        if req('/status')['voice_count'] == 0:
            return round(time.time() - t0, 1)
        time.sleep(0.25)
    return None


def machine_info():
    info = {'platform': platform.platform(), 'processor': platform.processor(), 'python': platform.python_version()}
    try:
        if os.name == 'nt':
            out = subprocess.run(['powershell', '-NoProfile', '-Command',
                                  "(Get-CimInstance Win32_Processor | Select -First 1 Name,NumberOfCores,NumberOfLogicalProcessors | ConvertTo-Json -Compress); "
                                  "[math]::Round((Get-CimInstance Win32_ComputerSystem).TotalPhysicalMemory/1GB,1)"],
                                 capture_output=True, text=True, timeout=30).stdout.strip().splitlines()
            if out:
                info['cpu'] = json.loads(out[0])
                info['ram_gb'] = out[1] if len(out) > 1 else ''
    except Exception:
        pass
    return info


def piek_ram_mb(naam='vpo-app'):
    """Piek-werkgeheugen (MB) van het app-proces, of None."""
    try:
        if os.name == 'nt':
            uit = subprocess.run(['powershell', '-NoProfile', '-Command',
                                  f"(Get-Process {naam} -ErrorAction SilentlyContinue | Measure-Object PeakWorkingSet64 -Maximum).Maximum"],
                                 capture_output=True, text=True, timeout=30).stdout.strip()
            return round(int(uit) / 1e6) if uit else None
        pid = subprocess.run(['pgrep', '-f', naam], capture_output=True, text=True).stdout.split()[0]
        for l in open(f'/proc/{pid}/status'):
            if l.startswith('VmHWM'):
                return round(int(l.split()[1]) / 1000)
    except Exception:
        return None
    return None
