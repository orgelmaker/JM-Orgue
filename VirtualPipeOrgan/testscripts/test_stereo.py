# Stereo-bewijs: Friesach (stereo 24-bit WAV) laden, vast register + akkoord,
# MP3-opname via de test-API, decoderen met ffmpeg en L/R vergelijken.
# Verwacht ná 0.7.36: correlatie(L,R) duidelijk < 1 en RMS(L−R) ruim boven de
# vloer. Ter vergelijking: Puttershoek (mono) → L en R identiek (pan 0).
import json, urllib.request, time, subprocess, sys, os
import numpy as np
B = "http://127.0.0.1:8765"
def post(p, b=None, t=300):
    d = json.dumps(b).encode() if b is not None else b"{}"
    r = urllib.request.Request(B + p, data=d, method="POST", headers={"Content-Type": "application/json"})
    return json.loads(urllib.request.urlopen(r, timeout=t).read())
def get(p, t=30): return json.loads(urllib.request.urlopen(B + p, timeout=t).read())

def decode(mp3):
    raw = subprocess.run(["ffmpeg", "-v", "error", "-i", mp3, "-f", "f32le", "-ac", "2", "-"], capture_output=True).stdout
    a = np.frombuffer(raw, dtype=np.float32).reshape(-1, 2)
    return a[:, 0], a[:, 1]

def meet(label, organ_path, is_dir, stop_pick, notes, out):
    post("/load_directory" if is_dir else "/load_organ", {"path": organ_path})
    time.sleep(2)
    info = get("/organ")
    stops = [s for d in info["divisions"] for s in d["stops"]]
    chosen = [s for s in stops if stop_pick(s)][:1] or stops[:1]
    for s in stops:
        if s.get("drawn"): post(f"/stops/{s['id']}/toggle")
    for s in chosen: post(f"/stops/{s['id']}/toggle")
    time.sleep(0.3)
    post("/record/start", {"path": out})
    time.sleep(0.4)
    for n in notes: post(f"/notes/{n}/on")
    time.sleep(3.0)
    st = get("/status")
    for n in notes: post(f"/notes/{n}/off")
    time.sleep(1.5)
    post("/record/stop")
    for s in chosen: post(f"/stops/{s['id']}/toggle")
    time.sleep(0.5)
    l, r = decode(out)
    sr = 48000
    seg = slice(int(0.8 * sr), int(3.0 * sr))
    l, r = l[seg].astype(np.float64), r[seg].astype(np.float64)
    corr = float(np.corrcoef(l, r)[0, 1]) if l.std() > 0 and r.std() > 0 else float("nan")
    rms = lambda x: float(np.sqrt(np.mean(x * x)) + 1e-12)
    print(f"{label}: register={chosen[0]['name']!r} | RMS L={rms(l):.4f} R={rms(r):.4f} | RMS(L-R)={rms(l - r):.4f} "
          f"| corr(L,R)={corr:.3f} | status peak L/R={st['peak_left']:.3f}/{st['peak_right']:.3f} | stereo_samples={st.get('stereo_samples')}")
    return corr, rms(l - r) / max(rms(l), 1e-9)

SP = os.path.dirname(os.path.abspath(__file__))
c1, d1 = meet("Friesach (stereo-set)", r"C:\Bronbestanden\JM-Orgue\Sample set homemade\Friesach_GrandOrgue\Friesach.organ", False,
              lambda s: "Principal" in s["name"] or "Prinzipal" in s["name"], [60, 64, 67], os.path.join(SP, "stereo_friesach.mp3"))
c2, d2 = meet("Puttershoek (mono-set)", r"C:\Bronbestanden\JM-Orgue\Sample set homemade\Bätz-Witte Puttershoek", True,
              lambda s: "Prestant" in s["name"], [60, 64, 67], os.path.join(SP, "stereo_puttershoek.mp3"))
print("OORDEEL:", "STEREO KOMT DOOR" if (c1 < 0.97 and d1 > 0.1) else "GEEN STEREO?!", "| mono-set identiek L/R:", "OK" if d2 < 0.02 else f"AFWIJKING {d2:.3f}")
