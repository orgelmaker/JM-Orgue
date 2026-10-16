# Startbuffer-proef (0.8.92): laadt een groot orgel met een gegeven startbuffer
# (ms), trekt alle registers, speelt 60 s wisselende 10-noots akkoorden en
# meet RAM, laadtijd, stemmen, render-belasting en de laadtellers
# (startbuffer-tekorten, wachtrij vol, laadlatentie).
#
# Gebruik: python test_preload_rotterdam.py <ms> [seconden] [orgelpad]
# Vereist een draaiende app met --test-api (poort 8765).
import json, subprocess, sys, time, urllib.request

B = "http://127.0.0.1:8765"
def post(p, b=None, t=600):
    d = json.dumps(b).encode() if b is not None else b"{}"
    r = urllib.request.Request(B + p, data=d, method="POST", headers={"Content-Type": "application/json"})
    return json.loads(urllib.request.urlopen(r, timeout=t).read())
def get(p, t=60): return json.loads(urllib.request.urlopen(B + p, timeout=t).read())
def ram_mb():
    out = subprocess.run(["powershell", "-NoProfile", "-Command",
        "(Get-Process vpo-app | Measure-Object PrivateMemorySize64 -Sum).Sum"], capture_output=True, text=True).stdout.strip()
    try: return int(out) / 1e6
    except ValueError: return -1

ms = int(sys.argv[1]) if len(sys.argv) > 1 else 2000
seconds = int(sys.argv[2]) if len(sys.argv) > 2 else 60
organ = sys.argv[3] if len(sys.argv) > 3 else r"D:\HauptwerkSampleSetsAndComponents\OrganDefinitions\Rotterdam - Laurenskerk, Hoofdorgel Surround.Organ_Hauptwerk_xml"
# Belasting (0.8.92): aantal registers (0 = alle), noten per akkoord, seconden per akkoord.
max_stops = int(sys.argv[4]) if len(sys.argv) > 4 else 0
chord_n = int(sys.argv[5]) if len(sys.argv) > 5 else 10
period = float(sys.argv[6]) if len(sys.argv) > 6 else 1.0

print(f"== startbuffer {ms} ms, {seconds} s, RAM voor load {ram_mb():.0f} MB")
post("/preload_ms", {"ms": ms})
t0 = time.time()
post("/load_organ", {"path": organ})
while not get("/status").get("organ_loaded"): time.sleep(0.5)
laadtijd = time.time() - t0
post("/motor", {"on": True})   # vers proces: motor staat uit en zonder motor klinkt niets
time.sleep(3)
ram_na_load = ram_mb()
st = get("/status")
print(f"geladen in {laadtijd:.1f} s; RAM na load {ram_na_load:.0f} MB; sr={st['sample_rate']} host={st.get('audio_host')} loader={st.get('loader')}")
persp = get("/perspectives")
# Alleen de eerste positie (argv[7] = "1pos"): maakt 2000 en 200 ms vergelijkbaar
# wanneer de geheugenrem bij 2000 ms maar één positie toelaat.
if len(sys.argv) > 7 and sys.argv[7] == "1pos" and any(p["enabled"] for p in persp[1:]):
    for p in persp[1:]:
        if p["enabled"]: post("/perspectives", {"name": p["name"], "enabled": False})
    t0 = time.time(); post("/load_organ", {"path": organ})
    while not get("/status").get("organ_loaded"): time.sleep(0.5)
    laadtijd = time.time() - t0; time.sleep(3); ram_na_load = ram_mb()
    post("/motor", {"on": True})
    print(f"herladen met 1 positie in {laadtijd:.1f} s; RAM na load {ram_na_load:.0f} MB")
    persp = get("/perspectives")
print("perspectieven:", [(p['name'], p['enabled'], p.get('loaded')) for p in persp])

info = get("/organ")
stops = [s for d in info["divisions"] for s in d["stops"]]
if max_stops: stops = stops[:max_stops]
post("/stops/set", {"ids": [s["id"] for s in stops]})
print(f"registers getrokken: {len(stops)}, akkoord {chord_n} noten, elke {period} s")
for c in info.get("couplers", []):
    if c.get("coupler_type") == "unison" and not c.get("active"):
        try: post(f"/couplers/{c['id']}/toggle")
        except Exception: pass
time.sleep(1)
l0 = get("/status")["loader"]

chords = [[36, 43, 48, 52, 55, 60, 64, 67, 72, 76], [38, 45, 50, 53, 57, 62, 65, 69, 74, 77],
          [40, 47, 52, 55, 59, 64, 67, 71, 76, 79], [41, 48, 53, 57, 60, 65, 69, 72, 77, 81]]
prev = []; i = 0; peak_ram = ram_na_load; peak_voices = 0; peak_load = 0.0
t_end = time.time() + seconds
while time.time() < t_end:
    cur = chords[i % len(chords)][:chord_n]
    for n in cur: post(f"/notes/{n}/on")
    for n in prev:
        if n not in cur: post(f"/notes/{n}/off")
    prev = cur; i += 1
    time.sleep(period)
    st = get("/status")
    peak_voices = max(peak_voices, st["voice_count"]); peak_load = max(peak_load, st.get("render_peak", 0))
    if i % 5 == 0: peak_ram = max(peak_ram, ram_mb())
for n in prev: post(f"/notes/{n}/off")
time.sleep(4)
st = get("/status"); l1 = st["loader"]
print(f"stemmen piek {peak_voices}, render piek {peak_load*100:.0f}%, overloads {st.get('render_overloads')}, RAM piek {peak_ram:.0f} MB, RAM na {ram_mb():.0f} MB")
print(f"lader: tekorten +{l1['preload_underruns']-l0['preload_underruns']}, release-rand +{l1['release_edge_hits']-l0['release_edge_hits']}, "
      f"verzoeken +{l1['requests']-l0['requests']}, klaar +{l1['done']-l0['done']}, wachtrij vol +{l1['queue_full']-l0['queue_full']}, "
      f"herhaald +{l1['retries']-l0['retries']}, latentie gem {l1['latency_avg_ms']:.0f} ms / max {l1['latency_max_ms']:.0f} ms")
print(json.dumps({"ms": ms, "laadtijd_s": round(laadtijd, 1), "ram_na_load_mb": round(ram_na_load), "ram_piek_mb": round(peak_ram),
                  "stemmen_piek": peak_voices, "render_piek": round(peak_load, 3), "underruns": l1['preload_underruns']-l0['preload_underruns'],
                  "queue_full": l1['queue_full']-l0['queue_full'], "lat_avg_ms": round(l1['latency_avg_ms']), "lat_max_ms": round(l1['latency_max_ms'])}))
