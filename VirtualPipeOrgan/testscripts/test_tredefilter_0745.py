# -*- coding: utf-8 -*-
"""Tredefilter 0.7.45 via de test-API (vpo-app.exe --test-api, orgel geladen).

Controleert wat het filter moet doen met het signaal dat op het testorgel is
gemeten (jm-midimon, 2026-09-16): rustwiebel van +-2, losse uitschieters van
30..70 eenheden, en berichten om de 5 ms.

  T1  losse uitschieter haalt de zwelkast niet;
  T2  rustwiebel laat de kast stilstaan;
  T3  een echte beweging komt gewoon aan;
  T4  na een stilte geldt een nieuwe stand meteen (midibestand/test-API);
  T5  helemaal dicht en helemaal open worden exact gehaald.

Gebruik: python testscripts/test_tredefilter_0745.py
"""
import json, sys, time, urllib.request
B = "http://127.0.0.1:8765"
R = {"pass": [], "fail": []}
def ok(m):   print("  PASS", m); R["pass"].append(m)
def fail(m): print("  FAIL", m); R["fail"].append(m)
def check(c, m): (ok if c else fail)(m)
def post(p, b=None, t=30):
    d = json.dumps(b).encode() if b is not None else b"{}"
    r = urllib.request.Request(B + p, data=d, method="POST", headers={"Content-Type": "application/json"})
    return json.loads(urllib.request.urlopen(r, timeout=t).read())
def get(p, t=30): return json.loads(urllib.request.urlopen(B + p, timeout=t).read())
def gain(idx): return float(get("/division_volumes")["gains"][idx])

# Snel injecteren: geen pauze, zodat de berichten binnen het filtervenster
# vallen. Met een pauze van >= 150 ms geldt elke waarde meteen (dat is T4).
KAN, CC = 15, 20
def snel(reeks):
    for v in reeks:
        post(f"/midi/inject?cc={CC}&value={v}&channel={KAN}")
def rust(): time.sleep(0.4)

st = get("/status")
if not st.get("organ_loaded"): sys.exit("geen orgel geladen")
divs = get("/organ")["divisions"]
if len(divs) < 2: sys.exit("orgel met minstens twee divisies nodig")
SW, SWI = divs[1]["name"], 1
print(f"zweldivisie: {SW} (kanaal {KAN+1}, CC{CC})")

orig = get("/swell")
post("/swell/binding", {"division": SW, "channel": KAN, "cc": CC, "min": 0, "max": 127, "invert": False})
time.sleep(0.3)

print("\nT1 losse uitschieter")
snel([60] * 6); rust()
basis = gain(SWI)
snel([60, 61, 59, 120, 60, 61])          # 120 = overspraak van de buurtrede
piek = gain(SWI)
snel([60] * 6); rust()
check(abs(piek - basis) < 0.06, f"uitschieter 120 gaf {piek:.3f} t.o.v. {basis:.3f}")

print("\nT2 rustwiebel")
snel([100] * 6); rust()
standen = []
for i in range(24):
    snel([[98, 100, 102, 100][i % 4]])
    standen.append(round(gain(SWI), 4))
check(max(standen) - min(standen) <= 0.02,
      f"kast beweegt {max(standen)-min(standen):.3f} bij een wiebel van +-2")

print("\nT3 echte beweging")
snel(range(0, 128, 4)); rust(); snel([127] * 5); rust()
check(gain(SWI) > 0.93, f"na open trappen staat de kast op {gain(SWI):.3f}")
snel(range(127, -1, -4)); rust(); snel([0] * 5); rust()
check(gain(SWI) < 0.07, f"na dicht trappen staat de kast op {gain(SWI):.3f}")

print("\nT4 losse waarden met pauzes ertussen")
goed = True
for v, verwacht in ((127, 1.0), (0, 0.0), (64, 64 / 127)):
    post(f"/midi/inject?cc={CC}&value={v}&channel={KAN}"); time.sleep(0.35)
    g = gain(SWI)
    if abs(g - verwacht) > 0.02:
        goed = False; print(f"    {v} -> {g:.3f}, verwacht {verwacht:.3f}")
check(goed, "elke losse waarde geldt meteen (midibestand, test-API)")

print("\nT5 uiterste standen")
snel([0] * 200); rust()
dicht = gain(SWI)
snel([127] * 200); rust()
open_ = gain(SWI)
check(dicht <= 0.001 and open_ >= 0.999, f"dicht {dicht:.3f} / open {open_:.3f}")

# Oorspronkelijke zwelkoppelingen terugzetten.
post("/swell/binding", {"division": SW, "clear": True})
for s in orig:
    if s.get("channel") is not None:
        post("/swell/binding", {"division": s["division"], "channel": s["channel"],
                                "cc": s.get("cc_num", s.get("cc")), "min": s.get("min_val", 0),
                                "max": s.get("max_val", 127), "invert": bool(s.get("invert"))})

print(f"\n{len(R['pass'])} pass, {len(R['fail'])} fail")
for m in R["fail"]: print("  FAIL", m)
sys.exit(1 if R["fail"] else 0)
