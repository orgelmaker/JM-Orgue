# -*- coding: utf-8 -*-
"""Levende wind 0.7.51 via de test-API (vpo-app.exe --test-api, orgel geladen).

De klacht was: "ik mis nog iets waardoor je dat Hollandse levendige effect
krijgt". Dit script meet aan de echte audiomotor, met een echt geladen orgel,
de drie dingen die dat effect maken:

  T1  één register met een akkoord: de wind ademt NIET (< 0,3 % inzakking);
  T2  het pleno wél: statisch een paar procent, kwadratisch gegroeid;
  T3  pedaal-schrik: een 16'-pijp onder een liggend discantakkoord laat de
      lade van het manuaal binnen ~50 ms dippen (3-8 %) en die veert terug;
  T4  loslaten van de baspijp geeft een opstoot (druk boven de rustdruk);
  T5  staartfix: na het loslaten van een akkoord staat de balg binnen een
      halve seconde weer op volle druk (0.7.50 bleef seconden ingezakt);
  T6  Neutraal is milder dan Hollands, en model uit is exact vlak;
  T7  de karikatuurgrens: nergens dieper dan 8 % kortstondig / 5 % statisch.

Gebruik: python testscripts/test_windmodel_0751.py
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

def wind():
    return get("/wind")

def lade(div):
    """(druk, dip) van de lade van divisie `div`."""
    d = wind()["divisions"][div]
    return d["pressure"], d["dip"]

def balg(groep):
    g = wind()["groups"][groep]
    return g["pressure"], g["verbruik"], g["voices"]

def zet_wind(aan, karakter=1, **extra):
    q = "&".join("%s=%s" % (k, v) for k, v in extra.items())
    post("/settings/wind?enabled=%d&karakter=%d%s" % (1 if aan else 0, karakter, ("&" + q) if q else ""))
    time.sleep(0.25)

def noten(reeks, aan=True):
    for n in reeks:
        post("/notes/%d/%s" % (n, "on" if aan else "off"))

def volg_lade(div, seconden, stap=0.01):
    """Laagste en hoogste ladedruk over `seconden`, gepolld; plus de laagste
    vastgehouden dip die de meter in die tijd liet zien."""
    lo, hi, dip, t0 = 9.0, 0.0, 9.0, time.time()
    while time.time() - t0 < seconden:
        p, d = lade(div)
        lo, hi, dip = min(lo, p), max(hi, p), min(dip, d)
        time.sleep(stap)
    return lo, hi, dip

def groepen(pedaal_bij_manuaal):
    """Identiteit (elke divisie een eigen balg), of het pedaal op de balg van
    het manuaal: het kistorgel-/gedeelde-balg-geval waarin het pedaal het
    manuaal laat schrikken. Op grote orgels met gescheiden kanalen is dat een
    keuze bij het klavier, geen standaard."""
    for d in range(len(divs)):
        g = MAN if (pedaal_bij_manuaal and d == PED) else d
        post("/settings/wind_group?division=%d&group=%d" % (d, g))
    time.sleep(0.2)

st = get("/status")
if not st.get("organ_loaded"):
    sys.exit("geen orgel geladen")
organ = get("/organ")
divs = organ["divisions"]
print("orgel: %s (%d divisies)" % (organ.get("name"), len(divs)))

# Manuaal met de meeste registers, en het pedaal (laagste laagste toets).
MAN = max(range(len(divs)), key=lambda i: len(divs[i].get("stops", [])))
PED = min(range(len(divs)), key=lambda i: min([s["first_midi_note"] for s in divs[i]["stops"]] or [127]))
if PED == MAN:
    PED = next((i for i in range(len(divs)) if i != MAN), MAN)
man_stops = divs[MAN]["stops"]
ped_stops = divs[PED]["stops"]
def ids(stops, filt=None, n=99):
    out = [s["id"] for s in stops if (filt is None or filt(s))]
    return out[:n]
def voet(s):
    t = (s.get("pitch") or "8'").replace("'", "").strip().split(" ")[0]
    try: return float(t)
    except: return 8.0
# Eén zacht 8'-register voor de nul-test; het pleno; een 16' (of het laagste) in het pedaal.
een_register = ids(man_stops, lambda s: voet(s) == 8.0, 1) or ids(man_stops, None, 1)
pleno = ids(man_stops, lambda s: voet(s) >= 2.0)
ped_16 = ids(ped_stops, lambda s: voet(s) >= 16.0, 2) or ids(ped_stops, None, 1)
hoog = ids(man_stops, lambda s: voet(s) <= 2.0, 1) or een_register
print("manuaal %s (groep %d), pedaal %s (groep %d)" % (divs[MAN]["name"], MAN + 1, divs[PED]["name"], PED + 1))
print("nul-test met %s; pleno %d registers; pedaal %s; discant op %s" % (een_register, len(pleno), ped_16, hoog))

post("/panic")
groepen(False)

print("\nT1 één register, een akkoord: de wind ademt niet")
zet_wind(True, 1)
post("/stops/set", {"ids": een_register})
time.sleep(0.3)
noten([60, 64, 67, 72])
time.sleep(1.5)
p1, _ = lade(MAN)
b1, q1, v1 = balg(MAN)
post("/panic"); time.sleep(1.0)
check(1.0 - p1 < 0.004, "één register: lade %.2f %% (verbruik %.2f, %d pijpen)" % (p1 * 100, q1, v1))

print("\nT2 het pleno ademt wel")
post("/stops/set", {"ids": pleno})
time.sleep(0.5)
noten([48, 52, 55, 60])
time.sleep(2.0)
p2, _ = lade(MAN)
b2, q2, v2 = balg(MAN)
check(1.0 - b2 > 0.01, "pleno: balg %.2f %% bij verbruik %.1f (%d pijpen)" % (b2 * 100, q2, v2))
check(1.0 - b2 < 0.06, "pleno statisch onder de 6 %%: %.2f %%" % ((1 - b2) * 100))
post("/panic"); time.sleep(1.0)

print("\nT3 pedaal-schrik onder een liggend discantakkoord (pedaal op de balg van het manuaal)")
groepen(True)
post("/stops/set", {"ids": hoog + ped_16})
time.sleep(0.5)
noten([72, 76, 79])            # liggend akkoord op het discantregister
time.sleep(1.2)
rust, _ = lade(MAN)
noten([36])                    # 16'-C in het pedaal
lo3, hi3, dip_hold = volg_lade(MAN, 0.5)
dip3 = rust - lo3
check(dip3 > 0.02, "de lade van het manuaal dipt %.2f %% (rust %.2f %%)" % (dip3 * 100, rust * 100))
check(dip3 < 0.09, "en niet dieper dan de karikatuurgrens: %.2f %%" % (dip3 * 100))
check(rust - dip_hold > 0.015, "de vastgehouden dip in de meter laat het zien: %.2f %%" % ((rust - dip_hold) * 100))
time.sleep(0.6)
# De baspijp klinkt nog: de balg draagt dan terecht een statische inzakking.
# De LADE (de snelle laag) hoort na een seconde wel weer op rust te staan.
na3 = wind()["divisions"][MAN]["lade"]
check(na3 > 0.985, "en de lade veert terug: %.2f %% na 1 s (balg draagt de statische inzakking van de klinkende bas)" % (na3 * 100))

print("\nT4 loslaten van de bas geeft een opstoot")
noten([36], aan=False)
lo4, hi4, _ = volg_lade(MAN, 0.4)
check(hi4 > rust + 0.004, "opstoot tot %.2f %% (rust %.2f %%)" % (hi4 * 100, rust * 100))
check(hi4 < 1.045, "maar begrensd: %.2f %%" % (hi4 * 100))
post("/panic"); time.sleep(1.0)

print("\nT5 staartfix: na loslaten staat de balg snel weer vol")
groepen(False)
post("/stops/set", {"ids": pleno})
time.sleep(0.5)
noten([48, 52, 55, 60, 64, 67])
time.sleep(2.0)
b5, q5, v5 = balg(MAN)
noten([48, 52, 55, 60, 64, 67], aan=False)
time.sleep(0.5)
b5b, q5b, v5b = balg(MAN)
check(q5 > 1.0 and q5b == 0.0, "verbruik %.1f tijdens, %.1f een halve seconde na het loslaten (%d -> %d speelstemmen)" % (q5, q5b, v5, v5b))
check(b5b > 0.99, "balg %.2f %% na een halve seconde (was tijdens %.2f %%)" % (b5b * 100, b5 * 100))
post("/panic"); time.sleep(1.0)

print("\nT6 Neutraal milder dan Hollands; uit is vlak")
groepen(True)
post("/stops/set", {"ids": hoog + ped_16})
time.sleep(0.4)
def schrik(karakter):
    zet_wind(True, karakter)
    noten([72, 76, 79]); time.sleep(1.0)
    rust, _ = lade(MAN)
    noten([36])
    lo, _, _ = volg_lade(MAN, 0.4)
    post("/panic"); time.sleep(0.8)
    return rust - lo
d_h = schrik(1)
d_n = schrik(0)
check(d_n < d_h, "Hollands dipt %.2f %%, Neutraal %.2f %%" % (d_h * 100, d_n * 100))
zet_wind(False)
noten([72, 76, 79, 36]); time.sleep(1.5)
lo6, hi6, _ = volg_lade(MAN, 0.4)
post("/panic")
check(lo6 >= 0.9999 and hi6 <= 1.0001, "model uit: lade exact vlak (%.4f..%.4f)" % (lo6, hi6))

print("\nT8 CPU: pas 1 bij een stil orgel")
time.sleep(1.0)
mpl = get("/status").get("mix_pass_load", {})
check(mpl.get("wind_trem", 1.0) < 0.03, "wind/tremulant-pas stil: %.2f %% van de buffertijd" % (100 * mpl.get("wind_trem", 1.0)))

print("\nT7 karikatuurgrens over alles wat gemeten is")
check(dip3 <= 0.08 and d_h <= 0.08 and (1 - b2) <= 0.05, "kortstondig <= 8 %%, statisch <= 5 %%")

post("/panic")
print("\n%d geslaagd, %d mislukt" % (len(R["pass"]), len(R["fail"])))
# Terug naar elke divisie een eigen balg, zodat de opgeslagen instellingen van
# het orgel niet met de meetopstelling achterblijven.
groepen(False)
for m in R["fail"]:
    print("  FAIL", m)
sys.exit(1 if R["fail"] else 0)
