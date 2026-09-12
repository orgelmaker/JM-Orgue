# Test gestapelde ranks + hertemperen op de GO-test-ODF (TestRanks.organ).
#  1. stop "Gestapeld" (2 ranks) → noot 48: verwacht 2 stemmen (nu: 1)
#  2. stop "Enkel" → noot 60 opnemen, grondtoon meten (FFT) — met optioneel
#     temperament via POST /temperament {name} als dat endpoint bestaat.
import json, urllib.request, time, subprocess, sys, os
import numpy as np
B = "http://127.0.0.1:8765"
SP = os.path.dirname(os.path.abspath(__file__))
ODF = os.path.join(SP, "go_testodf", "TestRanks.organ")
def post(p, b=None, t=300):
    d = json.dumps(b).encode() if b is not None else b"{}"
    r = urllib.request.Request(B + p, data=d, method="POST", headers={"Content-Type": "application/json"})
    return json.loads(urllib.request.urlopen(r, timeout=t).read())
def get(p, t=30): return json.loads(urllib.request.urlopen(B + p, timeout=t).read())
def try_post(p, b=None):
    try: return post(p, b)
    except Exception as e: return {"error": str(e)[:80]}

def fundamental(mp3, sr=48000, seg=(0.6, 2.4)):
    raw = subprocess.run(["ffmpeg", "-v", "error", "-i", mp3, "-f", "f32le", "-ac", "1", "-ar", str(sr), "-"], capture_output=True).stdout
    x = np.frombuffer(raw, dtype=np.float32)[int(seg[0]*sr):int(seg[1]*sr)].astype(np.float64)
    x -= x.mean(); w = np.hanning(len(x)); X = np.abs(np.fft.rfft(x * w)); f = np.fft.rfftfreq(len(x), 1/sr)
    # grondtoon: hoogste piek tussen 50 en 1200 Hz met parabolische interpolatie
    lo, hi = np.searchsorted(f, 50), np.searchsorted(f, 1200)
    i = lo + int(np.argmax(X[lo:hi]))
    if 1 <= i < len(X)-1:
        a, b, c = np.log(X[i-1]+1e-12), np.log(X[i]+1e-12), np.log(X[i+1]+1e-12)
        d = 0.5*(a-c)/(a-2*b+c)
        return float((i+d) * sr / len(x))
    return float(f[i])

post("/load_organ", {"path": ODF}); time.sleep(4)
info = get("/organ")
stops = {s["name"]: s for d in info["divisions"] for s in d["stops"]}
print("stops:", {k: v["id"] for k, v in stops.items()})
for s in stops.values():
    if s.get("drawn"): post(f"/stops/{s['id']}/toggle")

# 1. gestapeld
post(f"/stops/{stops['Gestapeld']['id']}/toggle"); time.sleep(0.3)
post("/notes/48/on"); time.sleep(1.0)
vc = get("/status")["voice_count"]
post("/notes/48/off"); time.sleep(1.5)
post(f"/stops/{stops['Gestapeld']['id']}/toggle")
print(f"Gestapeld, 1 toets → stemmen: {vc} (verwacht 2 met gestapelde ranks)")

# 2. toonhoogte "Enkel" (Bourdon_16 met per-pijp PitchCorrection -20 ct in Rank001? nee: Rank003 = solo zonder correctie)
def meet(label, temperament=None):
    if temperament is not None:
        r = try_post("/temperament", {"name": temperament}); print(f"  temperament {temperament!r}: {r}")
    post(f"/stops/{stops['Enkel']['id']}/toggle"); time.sleep(0.3)
    out = os.path.join(SP, f"pitch_{label}.mp3")
    post("/record/start", {"path": out}); time.sleep(0.3)
    post("/notes/60/on"); time.sleep(2.6); post("/notes/60/off"); time.sleep(0.5)
    post("/record/stop"); post(f"/stops/{stops['Enkel']['id']}/toggle"); time.sleep(0.3)
    f0 = fundamental(out)
    cents = 1200*np.log2(f0/261.626) if f0 > 0 else float('nan')
    print(f"  {label}: f0 = {f0:.2f} Hz = {cents:+.1f} ct t.o.v. C4 (261.63 Hz)")
    return f0
meet("default")
for t in (sys.argv[1:] or []):
    meet(t.replace(" ", "_"), t)
