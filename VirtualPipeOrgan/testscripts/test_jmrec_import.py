# Test: map-loader gebruikt de .organ-definitie als die in de map staat.
#  - Friesach_GrandOrgue-MAP laden (bevat Friesach.organ) -> verwacht 44 stops + koppels
#  - Puttershoek-map laden (geen .organ) -> verwacht de gewone mapscan (20 stops)
import json, urllib.request, time
B = "http://127.0.0.1:8765"
def post(p, b=None, t=240):
    d = json.dumps(b).encode() if b is not None else None
    r = urllib.request.Request(B+p, data=d, method="POST", headers={"Content-Type": "application/json"})
    return json.loads(urllib.request.urlopen(r, timeout=t).read())
def get(p, t=30): return json.loads(urllib.request.urlopen(B+p, timeout=t).read())

for label, pad, verwacht_stops in [
    ("Friesach-MAP (met .organ)", r"C:\Bronbestanden\JM-Orgue\Sample set homemade\Friesach_GrandOrgue", 44),
    ("Puttershoek-map (zonder .organ)", r"C:\Bronbestanden\JM-Orgue\Sample set homemade\Bätz-Witte Puttershoek", 20),
]:
    r = post("/load_directory", {"path": pad})
    info = get("/organ")
    n_stops = sum(len(d["stops"]) for d in info.get("divisions", []))
    n_coup = len(info.get("couplers") or [])
    echte = [c for c in (info.get("couplers") or []) if c.get("coupler_type") in ("unison", "sub", "super")]
    ok = n_stops == verwacht_stops
    print(f"{label}: geladen als '{info['name']}' | id={info['id']} | stops={n_stops} (verwacht {verwacht_stops}) "
          f"| koppels={n_coup} | {'OK' if ok else 'FOUT'}")
    time.sleep(1)
