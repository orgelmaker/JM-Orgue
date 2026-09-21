# -*- coding: utf-8 -*-
"""Windmodel 0.7.50 via de test-API (vpo-app.exe --test-api, orgel geladen).

De klacht was: "ik merk er nauwelijks iets van, werkt het wel?". Dit script
meet dat aan de echte audiomotor, met een echt geladen orgel:

  T1  zonder klinkende pijpen blijft de druk vol;
  T2  een akkoord met registers laat de druk zakken (hoorbaar = meer dan 3 cent);
  T3  meer pijpen = verder inzakken;
  T4  toetsen los: de druk komt terug op vol;
  T5  een grotere maximale daling geeft ook echt meer inzakking;
  T6  model uit = altijd volle druk;
  T7  de balg veert na (slap gedempt schiet hij over de rustdruk heen).

Gebruik: python testscripts/test_windmodel_0750.py
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

def wind(groep=None):
    """(druk, stemmen, cent) van een windgroep."""
    g = get("/wind")["groups"][GROEP if groep is None else groep]
    return g["pressure"], g["voices"], g["cents"]

def zet_wind(aan, reservoir=0.5, demping=0.7, sag=0.10):
    post(f"/settings/wind?enabled={1 if aan else 0}&reservoir={reservoir}"
         f"&damping={demping}&sag={sag}")
    time.sleep(0.2)

def noten(reeks, aan=True):
    for n in reeks:
        post(f"/notes/{n}/{'on' if aan else 'off'}")

def rust(s=1.2):
    """Wachten tot de balg tot stilstand is gekomen."""
    time.sleep(s)

st = get("/status")
if not st.get("organ_loaded"):
    sys.exit("geen orgel geladen")
organ = get("/organ")
divs = organ["divisions"]
print(f"orgel: {organ.get('name')} ({len(divs)} divisies)")

# De divisie met de meeste registers pakken. De windgroep van een divisie is
# standaard gelijk aan het divisienummer, dus divisie-index = groep-index.
GROEP = max(range(len(divs)), key=lambda i: len(divs[i].get("stops", [])))
stops = [x["id"] for x in divs[GROEP].get("stops", [])]
if len(stops) < 3:
    sys.exit("geen divisie met genoeg registers voor deze meting")
post("/panic")
post("/stops/set", {"ids": stops[:8]})
time.sleep(0.5)
print(f"divisie {divs[GROEP]['name']} = windgroep {GROEP + 1}, "
      f"{min(8, len(stops))} van {len(stops)} registers getrokken")

print("\nT1 stille windlade")
zet_wind(True)
rust()
druk, stemmen, cent = wind()
check(stemmen == 0 and druk > 0.999,
      f"stil: {stemmen} stemmen, druk {druk*100:.2f} %")

print("\nT2 akkoord met wind aan")
noten([48, 52, 55, 60, 64, 67])
rust(1.5)
druk2, stemmen2, cent2 = wind()
check(stemmen2 > 0, f"{stemmen2} klinkende pijpen op de balg")
check(cent2 < -3.0, f"inzakking {druk2*100:.2f} % = {cent2:.1f} cent (hoorbaar vanaf ~3 cent)")

print("\nT3 meer pijpen, verder inzakken")
noten([36, 40, 43, 72, 76, 79])
rust(1.5)
druk3, stemmen3, cent3 = wind()
check(stemmen3 > stemmen2, f"{stemmen3} pijpen tegen {stemmen2}")
check(druk3 < druk2 - 0.001, f"druk {druk3*100:.2f} % tegen {druk2*100:.2f} %")

print("\nT4 toetsen los")
post("/panic")
rust(2.0)
druk4, stemmen4, _ = wind()
check(stemmen4 == 0 and druk4 > 0.999, f"terug op {druk4*100:.2f} %")

print("\nT5 grotere maximale daling")
zet_wind(True, sag=0.25)
noten([48, 52, 55, 60, 64, 67])
rust(1.5)
druk5, _, cent5 = wind()
post("/panic"); rust(1.5)
check(cent5 < cent2 - 2.0,
      f"25 % geeft {cent5:.1f} cent tegen {cent2:.1f} cent bij 10 %")

print("\nT6 model uit")
zet_wind(False)
noten([48, 52, 55, 60, 64, 67])
rust(1.5)
druk6, _, _ = wind()
post("/panic"); rust(1.0)
check(druk6 > 0.9999, f"uit: druk {druk6*100:.2f} %")

print("\nT7 naveren van de balg")
# Slap gedempt hoort de balg na het loslaten over de rustdruk heen te schieten.
zet_wind(True, demping=0.05, sag=0.20)
noten([48, 52, 55, 60, 64, 67])
rust(2.0)
post("/panic")
hoogste = 0.0
t0 = time.time()
while time.time() - t0 < 1.5:
    hoogste = max(hoogste, wind()[0])
    time.sleep(0.02)
zet_wind(False)
check(hoogste > 1.0005, f"overschot na loslaten: {hoogste*100:.3f} %")

post("/panic")
print(f"\n{len(R['pass'])} geslaagd, {len(R['fail'])} mislukt")
for m in R["fail"]:
    print("  FAIL", m)
sys.exit(1 if R["fail"] else 0)
