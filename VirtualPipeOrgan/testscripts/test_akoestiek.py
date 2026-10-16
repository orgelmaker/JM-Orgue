# Functionele test akoestische configuratie (0.8.92): laadt het testorgel
# (maak_akoestiek_testorgel.py), controleert de samenvatting in /organ en
# meet dat de zwelkast per rank alleen de gebonden registers dempt.
#   python test_akoestiek.py "<pad>\Friesach-akoestiek-test.organ" <stop_in_kast> <stop_buiten>
import json, sys, time, urllib.request
B = "http://127.0.0.1:8765"
def post(p, b=None, t=600):
    d = json.dumps(b).encode() if b is not None else b"{}"
    r = urllib.request.Request(B + p, data=d, method="POST", headers={"Content-Type": "application/json"})
    return json.loads(urllib.request.urlopen(r, timeout=t).read())
def get(p, t=60): return json.loads(urllib.request.urlopen(B + p, timeout=t).read())

organ, in_kast, buiten = sys.argv[1], int(sys.argv[2]), int(sys.argv[3])
post("/load_organ", {"path": organ})
while not get("/status").get("organ_loaded"): time.sleep(0.5)
time.sleep(2)
info = get("/organ")
ac = info.get("acoustics")
assert ac, "geen acoustics in /organ"
print("acoustics:", json.dumps({k: ac[k] for k in ("format", "schema_version", "valid", "error", "ranks", "ranks_in_enclosure", "pipe_bindings", "bound_pipes", "mix_groups", "outputs")}))
print("enclosures:", ac["enclosures"]); print("irs:", [(i["name"], i["channels"], i["exists"]) for i in ac["impulse_responses"]]); print("warnings:", ac["warnings"])
assert ac["valid"], ac.get("error")
assert len(ac["enclosures"]) == 1 and ac["enclosures"][0]["slot"] == 0
assert ac["bound_pipes"] == ac["pipe_bindings"], "niet alle bindingen passen"

stops = {s["id"]: s for d in info["divisions"] for s in d["stops"]}
def dto_id(n):
    for sid in stops:
        if sid.endswith(f"_{n}"): return sid
    raise SystemExit(f"stop {n} niet gevonden")
def alles_uit():
    for d in get("/organ")["divisions"]:
        for s in d["stops"]:
            if s.get("drawn"): post(f"/stops/{s['id']}/toggle")

def piek_met(stop_n, kast_gain):
    post("/stops/set", {"ids": [dto_id(stop_n)]})   # exact deze registratie
    post("/swell", {"index": 128, "gain": kast_gain})
    time.sleep(0.3)
    post("/notes/60/on"); time.sleep(1.2)
    p = max(get("/status")["peak_left"], get("/status")["peak_right"])
    post("/notes/60/off"); time.sleep(1.5)
    return p

post("/master?db=-12")
open_k = piek_met(in_kast, 1.0); dicht_k = piek_met(in_kast, 0.0)
open_b = piek_met(buiten, 1.0); dicht_b = piek_met(buiten, 0.0)
post("/swell", {"index": 128, "gain": 1.0})
print(f"in kast: open {open_k:.4f} dicht {dicht_k:.4f} ({20*__import__('math').log10(max(dicht_k,1e-6)/max(open_k,1e-6)):+.1f} dB)")
print(f"buiten:  open {open_b:.4f} dicht {dicht_b:.4f} ({20*__import__('math').log10(max(dicht_b,1e-6)/max(open_b,1e-6)):+.1f} dB)")
assert open_k > 0.001 and open_b > 0.001, "geen geluid"
assert dicht_k < open_k * 0.5, "kast dempt het gebonden register niet"
assert dicht_b > open_b * 0.7, "kast dempt ten onrechte een register buiten de kast"
print("OK: zwelkast per rank werkt alleen op de gebonden ranks")
