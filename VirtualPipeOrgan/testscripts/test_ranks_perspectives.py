# Test gestapelde ranks + microfoonperspectieven via de test-API (poort 8765).
# Draaien met de app gestart met --test-api (release-build 0.7.38-werkboom).
#
#  A. TestRanks.organ (go_testodf): Stop001 "Gestapeld" = 2 ranks zonder
#     perspectief → /ranks 2 lagen, /status layered_stops == 1, 1 toets → 2 stemmen;
#     Stop002 "Enkel" → 1 stem. /perspectives leeg.
#  B. stacked_test.organ (gegenereerd, Friesach-WAVs): Rank001 "(front)" +
#     Rank002 "(rear)" → /perspectives [front aan/geladen, rear uit]; 1 toets →
#     1 stem; rear aan → reload_needed; herladen → 2 stemmen; gain -40 dB →
#     lagere piek; /settings/save → /settings/organ bevat perspectives.
#  C. MicTest-map (gegenereerd, kopieën van Friesach-WAVs): Hoofdwerk/Prestant_8/
#     Front|Rear → /perspectives [Front aan, Rear uit]; 1 stem; Rear aan +
#     herladen → 2 stemmen.
#  D. Saint-Jean-de-Luz (Hauptwerk, indien aanwezig): /perspectives
#     [front, rear, dry]; Bourdon 16 noot 48 → 1 stem; dry aan + herladen → 2.
#  E. Regressie Friesach.organ: /perspectives leeg, layered_stops 0, alle
#     stops alleen laag 0, 1 stem per toets.
import json, os, shutil, subprocess, sys, tempfile, time, urllib.request

B = "http://127.0.0.1:8765"
SP = os.path.dirname(os.path.abspath(__file__))
FRIESACH_DIR = r"C:\Bronbestanden\JM-Orgue\Sample set homemade\Friesach_GrandOrgue"
FRIESACH_ODF = os.path.join(FRIESACH_DIR, "Friesach.organ")
FR_P8 = os.path.join(FRIESACH_DIR, "Data - Friesach", "HW Principal 8", "A0")
FR_O4 = os.path.join(FRIESACH_DIR, "Data - Friesach", "HW Octave 4", "A0")
SJDL = r"C:\Bronbestanden\JM-Orgue\testen\SaintJeanDeLuz_Choeur_1_04.CompPkg.Hauptwerk\OrganDefinitions\Saint-Jean-de-Luz (choeur).Organ_Hauptwerk_xml"

def _testranks_odf():
    """Pad naar TestRanks.organ; genereert hem als hij nog niet bestaat.

    De ODF staat in %TEMP%\jm-orgue-testodf (buiten de repo, samen met de
    junction naar de Puttershoek-samples) en wordt gemaakt door
    testscripts/maak_go_testodf.py.
    """
    d = os.path.join(tempfile.gettempdir(), "jm-orgue-testodf")
    p = os.path.join(d, "TestRanks.organ")
    if not os.path.exists(p):
        gen = os.path.join(SP, "maak_go_testodf.py")
        print(f"TestRanks.organ ontbreekt, generator draaien: {gen}")
        subprocess.run([sys.executable, gen, d], check=True)
    return p

TESTRANKS = _testranks_odf()

def post(p, b=None, t=600):
    d = json.dumps(b).encode() if b is not None else b"{}"
    r = urllib.request.Request(B + p, data=d, method="POST", headers={"Content-Type": "application/json"})
    return json.loads(urllib.request.urlopen(r, timeout=t).read())
def get(p, t=30):
    return json.loads(urllib.request.urlopen(B + p, timeout=t).read())

FAILS = []
def check(cond, msg):
    print(("  PASS " if cond else "  FAIL ") + msg)
    if not cond:
        FAILS.append(msg)

def all_stops():
    info = get("/organ")
    return {s["name"]: s for d in info["divisions"] for s in d["stops"]}
def clear_stops():
    for s in all_stops().values():
        if s.get("drawn"):
            post(f"/stops/{s['id']}/toggle")
    time.sleep(0.2)
def voices_for(stop_name, note, hold=0.8):
    stops = all_stops()
    sid = stops[stop_name]["id"]
    post(f"/stops/{sid}/toggle"); time.sleep(0.3)
    post(f"/notes/{note}/on"); time.sleep(hold)
    st = get("/status")
    post(f"/notes/{note}/off"); time.sleep(0.2)
    rel = get("/status")["voice_count"]
    # Wachten tot de release-staarten echt uit zijn: opgenomen kerkakoestiek
    # duurt op natte sets (Friesach, SJDL) makkelijk 5-8 s, en sinds 0.7.39
    # worden verse staarten niet meer door het staartbudget weggekozen — een
    # vaste wachttijd van 2,5 s telde daardoor nog klinkende nagalm mee.
    after = rel
    for _ in range(60):
        time.sleep(0.25)
        after = get("/status")["voice_count"]
        if after == 0:
            break
    post(f"/stops/{sid}/toggle"); time.sleep(0.2)
    return st["voice_count"], max(st["peak_left"], st["peak_right"]), rel, after
def load(path, kind="organ"):
    r = post("/load_organ" if kind == "organ" else "/load_directory", {"path": path})
    time.sleep(1.5)
    return r
def persp_map():
    return {p["name"]: p for p in get("/perspectives")}

# ---------------------------------------------------------------- A: TestRanks
print("A. TestRanks.organ (gestapeld zonder perspectief)")
load(TESTRANKS)
clear_stops()
ranks = get("/ranks")
by_name = {s["name"]: s for s in ranks["stops"]}
g = by_name.get("Gestapeld"); e = by_name.get("Enkel")
check(g is not None and len(g["layers"]) == 2, f"Gestapeld heeft 2 lagen: {g and [(l['index'], l['perspective'], l['pipes_nonempty']) for l in g['layers']]}")
check(g is not None and all(l["perspective"] is None for l in g["layers"]), "lagen zonder perspectief (echte ranks)")
check(e is not None and len(e["layers"]) == 1, "Enkel heeft 1 laag")
check(ranks["stacked_stops"] == 1 and get("/status").get("layered_stops") == 1, "stacked_stops/layered_stops == 1")
check(get("/perspectives") == [], "/perspectives leeg")
vc, _, rel, after = voices_for("Gestapeld", 48)
check(vc == 2, f"Gestapeld 1 toets → {vc} stemmen (verwacht 2)")
check(after == 0, f"na loslaten weer 0 stemmen (rel-piek {rel}, resterend {after})")
vc, _, _, _ = voices_for("Enkel", 48)
check(vc == 1, f"Enkel 1 toets → {vc} stem (verwacht 1)")
# Gebruikers-voicing op de kale pijp: geldt voor beide lagen (alleen rooktest: geen crash)

# ---------------------------------------------------------------- B: stacked_test.organ met perspectieven
print("B. stacked_test.organ (front/rear als ranks)")
odf = os.path.join(SP, "stacked_test.organ")
def pipes(d):
    return "\n".join(f"Pipe{i:03}={os.path.join(d, f)}" for i, f in enumerate(["036-c.wav", "037-c#.wav", "038-d.wav"], 1))
open(odf, "w", encoding="utf-8").write(f"""[Organ]
ChurchName=StackedTest
NumberOfManuals=1
[Manual001]
Name=Hoofdwerk
NumberOfStops=1
Stop001=101
[Stop101]
Name=Test 8'
NumberOfRanks=2
Rank001=001
Rank001PipeCount=3
Rank001FirstAccessibleKeyNumber=1
Rank002=002
Rank002PipeCount=3
Rank002FirstAccessibleKeyNumber=1
[Rank001]
Name=Test 8' (front)
NumberOfLogicalPipes=3
{pipes(FR_P8)}
[Rank002]
Name=Test 8' (rear)
NumberOfLogicalPipes=3
{pipes(FR_O4)}
""")
load(odf); clear_stops()
pm = persp_map()
check(list(pm) == ["front", "rear"], f"/perspectives labels: {list(pm)}")
check(pm.get("front", {}).get("enabled") and pm.get("front", {}).get("loaded"), "front aan + geladen")
check(not pm.get("rear", {}).get("enabled") and not pm.get("rear", {}).get("loaded"), "rear standaard uit")
r = get("/ranks")["stops"][0]
check([l["perspective"] for l in r["layers"]] == ["front", "rear"], f"/ranks lagen: {[l['perspective'] for l in r['layers']]}")
check(get("/status")["layered_stops"] == 0, "layered_stops 0 (alleen perspectief-lagen)")
vc1, peak1, _, _ = voices_for("Test 8'", 36)
check(vc1 == 1, f"alleen front: {vc1} stem")
rr = post("/perspectives", {"name": "rear", "enabled": True})
check(rr.get("reload_needed") is True, f"rear aan → reload_needed: {rr}")
load(odf); clear_stops()
pm = persp_map()
check(pm["rear"]["loaded"] and pm["rear"]["enabled"], "na herladen: rear geladen")
vc2, peak2, rel2, after2 = voices_for("Test 8'", 36)
check(vc2 == 2, f"front+rear: {vc2} stemmen")
check(after2 == 0, f"na loslaten 0 stemmen (release-piek {rel2})")
post("/perspectives", {"name": "rear", "gain_db": -40})
vc3, peak3, _, _ = voices_for("Test 8'", 36)
# Rear is hier een 4'-rank (HW Octave 4) naast een 8' front: die bepaalt de PIEK
# nauwelijks, dus "minstens 15 % zachter" was een verkeerde maatstaf. De juiste
# controle is dat -40 dB de rear-laag hoorbaar wegneemt: de piek moet terug naar
# het niveau van front alleen (peak1), terwijl de stem wél blijft bestaan.
stil_genoeg = abs(peak3 - peak1) <= max(0.004, peak1 * 0.06) and peak3 <= peak2
check(vc3 == 2 and stil_genoeg,
      f"rear -40 dB: {vc3} stemmen (verwacht 2), piek {peak3:.3f} terug op front-alleen {peak1:.3f} "
      f"(met rear op 0 dB: {peak2:.3f})")
post("/perspectives", {"name": "rear", "gain_db": 0})
post("/settings/save"); time.sleep(0.5)
so = get("/settings/organ")
sp = {p["name"]: p for p in so.get("perspectives", [])}
check(sp.get("front", {}).get("enabled") is True and sp.get("rear", {}).get("enabled") is True, f"opgeslagen perspectives: {so.get('perspectives')}")
# terug naar default (rear uit) voor een schone volgende run
post("/perspectives", {"name": "rear", "enabled": False}); post("/settings/save")

# ---------------------------------------------------------------- C: MicTest-map
print("C. MicTest-map (submappen als perspectieven)")
mic = os.path.join(SP, "MicTest")
shutil.rmtree(mic, ignore_errors=True)
for pos, src in (("Front", FR_P8), ("Rear", FR_O4)):
    d = os.path.join(mic, "Hoofdwerk", "Prestant_8", pos)
    os.makedirs(d)
    for f in ("036-c.wav", "037-c#.wav", "072-c.wav"):
        s = os.path.join(src, f)
        if os.path.exists(s):
            shutil.copy(s, d)
load(mic, "dir"); clear_stops()
pm = persp_map()
check(list(pm) == ["Front", "Rear"], f"/perspectives: {list(pm)}")
check(pm.get("Front", {}).get("loaded") and not pm.get("Rear", {}).get("loaded"), "Front geladen, Rear niet")
stop_name = next(iter(all_stops()))
vc, _, _, _ = voices_for(stop_name, 36)
check(vc == 1, f"{stop_name}: {vc} stem")
post("/perspectives", {"name": "Rear", "enabled": True})
load(mic, "dir"); clear_stops()
vc, _, _, after = voices_for(stop_name, 36)
check(vc == 2, f"Rear aan: {vc} stemmen")
check(after == 0, "na loslaten 0 stemmen")
post("/perspectives", {"name": "Rear", "enabled": False}); post("/settings/save")

# ---------------------------------------------------------------- D: SJDL (optioneel)
if os.path.exists(SJDL):
    print("D. Saint-Jean-de-Luz (Hauptwerk)")
    load(SJDL); clear_stops()
    pm = persp_map()
    check(list(pm) == ["front", "rear", "dry"], f"/perspectives: {list(pm)}")
    check(pm.get("front", {}).get("loaded") and not pm.get("rear", {}).get("loaded") and not pm.get("dry", {}).get("loaded"), "alleen front geladen")
    check(all(p["pipe_count"] > 600 for p in pm.values()), f"pipe_count ≈ 654: {[p['pipe_count'] for p in pm.values()]}")
    names = all_stops()
    check(not any("(front)" in n for n in names), "stopnamen zonder '(front)'")
    bname = next(n for n in names if n.startswith("Bourdon 16"))
    vc, _, _, _ = voices_for(bname, 48)
    check(vc == 1, f"{bname} noot 48: {vc} stem")
    post("/perspectives", {"name": "dry", "enabled": True})
    load(SJDL); clear_stops()
    vc, _, rel, after = voices_for(bname, 48)
    check(vc == 2, f"front+dry: {vc} stemmen")
    check(after == 0, f"na loslaten 0 (release-piek {rel})")
    logs = get("/logs?lines=200")["lines"]
    check(not any("No sample or preload" in l for l in logs), "geen 'No sample' in log")
    post("/perspectives", {"name": "dry", "enabled": False}); post("/settings/save")
else:
    print("D. SJDL niet aanwezig — overgeslagen")

# ---------------------------------------------------------------- E: Friesach regressie
if os.path.exists(FRIESACH_ODF):
    print("E. Friesach regressie")
    load(FRIESACH_ODF); clear_stops()
    check(get("/perspectives") == [], "/perspectives leeg")
    r = get("/ranks")
    check(all(len(s["layers"]) == 1 for s in r["stops"]), "alle stops alleen laag 0")
    check(get("/status")["layered_stops"] == 0, "layered_stops 0")
    names = all_stops()
    pn = next(n for n in names if n.startswith("Principal 8"))
    vc, _, _, after = voices_for(pn, 48)
    check(vc == 1 and after == 0, f"{pn}: {vc} stem, na loslaten {after}")
else:
    print("E. Friesach niet aanwezig — overgeslagen")

print()
print("RESULTAAT:", "ALLES GROEN" if not FAILS else f"{len(FAILS)} FOUT(EN): " + "; ".join(FAILS))
sys.exit(1 if FAILS else 0)
