# Polyfonie/tutti-test op Friesach: alle registers, 10-noots akkoorden legato
# (elke seconde wisselen) gedurende 30 s. Meet voice_count, render_load/peak
# en de kap; controleer het log op stealing-/overbelastingsmeldingen.
import json, urllib.request, time, threading, sys
import numpy as np
B = "http://127.0.0.1:8765"
def post(p, b=None, t=300):
    d = json.dumps(b).encode() if b is not None else b"{}"
    r = urllib.request.Request(B + p, data=d, method="POST", headers={"Content-Type": "application/json"})
    return json.loads(urllib.request.urlopen(r, timeout=t).read())
def get(p, t=30): return json.loads(urllib.request.urlopen(B + p, timeout=t).read())

cap = int(sys.argv[1]) if len(sys.argv) > 1 else None
stereo = (sys.argv[2].lower() != "mono") if len(sys.argv) > 2 else None
if cap: post("/polyphony", {"voices": cap})
if stereo is not None: post("/stereo_samples", {"on": stereo})
post("/load_organ", {"path": r"C:\Bronbestanden\JM-Orgue\Sample set homemade\Friesach_GrandOrgue\Friesach.organ"})
time.sleep(3)
info = get("/organ")
stops = [s for d in info["divisions"] for s in d["stops"]]
for s in stops:
    if not s.get("drawn"): post(f"/stops/{s['id']}/toggle")
for c in info.get("couplers", []):
    if c.get("coupler_type") == "unison" and not c.get("active"):
        try: post(f"/couplers/{c['id']}/toggle")
        except Exception: pass
time.sleep(1)
st0 = get("/status")
print(f"kap={st0.get('polyphony')} stereo={st0.get('stereo_samples')} sr={st0['sample_rate']} host={st0.get('audio_host')}")

chords = [[36, 43, 48, 52, 55, 60, 64, 67, 72, 76], [38, 45, 50, 53, 57, 62, 65, 69, 74, 77],
          [40, 47, 52, 55, 59, 64, 67, 71, 76, 79], [41, 48, 53, 57, 60, 65, 69, 72, 77, 81]]
samples = []
prev = []
t_end = time.time() + 30
i = 0
while time.time() < t_end:
    cur = chords[i % len(chords)]
    for n in cur: post(f"/notes/{n}/on")      # legato: eerst aan, dan oude los
    for n in prev:
        if n not in cur: post(f"/notes/{n}/off")
    prev = cur
    i += 1
    for _ in range(4):
        time.sleep(0.25)
        st = get("/status")
        samples.append((st["voice_count"], st.get("render_load", 0), st.get("render_peak", 0)))
for n in prev: post(f"/notes/{n}/off")
time.sleep(4)
vc = np.array([s[0] for s in samples]); ld = np.array([s[1] for s in samples]); pk = np.array([s[2] for s in samples])
print(f"stemmen: gem {vc.mean():.0f}, max {vc.max()} (kap {st0.get('polyphony')}) | belasting: gem {ld.mean()*100:.0f}%, max {ld.max()*100:.0f}%, piek {pk.max()*100:.0f}%")
logs = get("/logs?lines=400") if True else {}
txt = json.dumps(logs)
print("log: overbelast-meldingen:", txt.count("zwaar belast"), "| underrun/klik-meldingen:", txt.count("underrun") + txt.count("Underrun"))
