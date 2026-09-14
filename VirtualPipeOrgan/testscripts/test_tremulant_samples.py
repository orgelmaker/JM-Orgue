# Test: externe sampleset met tremulant-OPNAMEN geeft een tremulantregister dat
# bij activeren écht de tremulant-samples speelt (attack én release).
#
# Draaien met de app gestart met --test-api (poort 8765) en met de test-ODF's uit
# maak_go_testtrem.py (wordt automatisch gedraaid als ze ontbreken):
#
#   python test_tremulant_samples.py [pad naar TestTrem.organ]
#
# De test-ODF geeft elke stand van pijp n een eigen toonhoogte, zodat de FFT-piek
# van POST /record/stop (peak_hz) zegt WELKE opname klinkt:
#     droge attack    f       (Prestant_8, 8')
#     trem-attack    2f       (Fluit_4, 4')
#     droge release  1,26f    (Prestant_8, noot n+4; GEEN octaaf: dat deelt zijn
#                              tweede boventoon met de grondtoon van de aanslag)
#     trem-release   1,5f     (Prestant_8, noot n+7)
#
# Onderdelen:
#   A. /organ: divisie has_tremulant + tremulant_kind="wave"; stop Prestant
#      has_tremulant=true, stop Enkel false.
#   B. POST /tremulant: spiegelt in /state.tremulant en schakelt N registers.
#   C. Attack: droog = f, met tremulant = 2f — ook bij de TWEEDE aanslag (na de
#      achtergrond-full-load), want droog en trem hebben een eigen samplesleutel.
#   D. Release: droog = 1,26f, met tremulant = 1,5f.
#   E. Crossfade tijdens het klinken: tremulant aanzetten wisselt de laag live.
#   F. Regressie: stop Enkel (geen trem-opnamen) verandert niet; TestTremWc.organ
#      (tremulant alléén op de windlade) geeft tóch een tremulantregister;
#      POST /settings/trem?enabled=0 wordt nu wél overgenomen.
import json, os, subprocess, sys, tempfile, time, urllib.request

B = "http://127.0.0.1:8765"
SP = os.path.dirname(os.path.abspath(__file__))
DEF_DIR = os.path.join(tempfile.gettempdir(), "jm-orgue-testodf")  # zelfde default als de generator
ODF = sys.argv[1] if len(sys.argv) > 1 else os.path.join(DEF_DIR, "TestTrem.organ")
ODF_WC = os.path.join(os.path.dirname(ODF), "TestTremWc.organ")
REC = os.path.join(tempfile.gettempdir(), "jm_trem_test.mp3")
NOTE = 60          # c' — ligt midden in het bereik van de test-ODF
TOL = 0.08         # 8 % marge op de verhouding tussen gemeten pieken


def post(p, b=None, t=600):
    d = json.dumps(b).encode("utf-8") if b is not None else b"{}"
    r = urllib.request.Request(B + p, data=d, method="POST",
                               headers={"Content-Type": "application/json"})
    return json.loads(urllib.request.urlopen(r, timeout=t).read())


def get(p, t=30):
    return json.loads(urllib.request.urlopen(B + p, timeout=t).read())


FAILS = []


def check(cond, msg):
    print(("  PASS " if cond else "  FAIL ") + msg)
    if not cond:
        FAILS.append(msg)


def ratio_ok(measured, base, expected, tol=TOL):
    if base <= 0 or measured <= 0:
        return False
    return abs(measured / base / expected - 1.0) <= tol


def ratio_ok_octaaf(measured, base, expected, tol=TOL):
    """Zoals ratio_ok, maar een octaaf ernaast telt ook als goed.

    `peak_hz` is de STERKSTE FFT-piek, niet per se de grondtoon: bij een
    Prestant-opname is de tweede boventoon vaak luider dan de grondtoon. Voor de
    vraag 'welke OPNAME klinkt hier' is een octaaf-afwijking daarom geen fout —
    zolang de gemeten verhouding maar niet ook bij een andere kandidaat-opname
    past. Daarom staat de droge release een grote terts hoger (1,26f: ook ×2 of
    ÷2 valt niet samen met f, 1,5f of 2f).
    """
    return any(ratio_ok(measured, base, expected * f, tol) for f in (0.5, 1.0, 2.0))


def all_stops():
    return {s["name"]: s for d in get("/organ")["divisions"] for s in d["stops"]}


def clear_stops():
    for s in all_stops().values():
        if s.get("drawn"):
            post(f"/stops/{s['id']}/toggle")
    time.sleep(0.2)


def draw(name):
    sid = all_stops()[name]["id"]
    if not all_stops()[name].get("drawn"):
        post(f"/stops/{sid}/toggle")
    time.sleep(0.3)
    return sid


def load(path):
    r = post("/load_organ", {"path": path})
    time.sleep(1.5)
    return r


def trem(active):
    r = post(f"/tremulant?division=Hoofdwerk&active={1 if active else 0}")
    time.sleep(0.4)
    return r


def peak_sustain(note=NOTE, hold=1.6):
    """Toonhoogte van de KLINKENDE noot: opname loopt tijdens het aanhouden."""
    post("/record/start", {"path": REC})
    time.sleep(0.3)
    post(f"/notes/{note}/on?velocity=100")
    time.sleep(hold)
    r = post("/record/stop")
    post(f"/notes/{note}/off")
    time.sleep(0.6)
    return float(r.get("peak_hz") or 0.0)


def peak_release(note=NOTE, hold=1.5, tail=1.4):
    """Toonhoogte ná loslaten: opname start vlak vóór de note-off."""
    post(f"/notes/{note}/on?velocity=100")
    time.sleep(hold)
    post("/record/start", {"path": REC})
    time.sleep(0.15)
    post(f"/notes/{note}/off")
    time.sleep(tail)
    r = post("/record/stop")
    time.sleep(0.4)
    return float(r.get("peak_hz") or 0.0)


# ---------------------------------------------------------------- testdata
if not os.path.exists(ODF):
    gen = os.path.join(SP, "maak_go_testtrem.py")
    print(f"Test-ODF ontbreekt, generator draaien: {gen}")
    subprocess.run([sys.executable, gen, os.path.dirname(ODF)], check=True)

print(f"ODF: {ODF}")
load(ODF)
clear_stops()

# ---------------------------------------------------------------- A: DTO
print("A. Registervlaggen uit de ODF")
org = get("/organ")
div = next((d for d in org["divisions"] if d["name"] == "Hoofdwerk"), None)
check(div is not None, "divisie Hoofdwerk aanwezig")
check(bool(div and div.get("has_tremulant")), f"divisie has_tremulant: {div and div.get('has_tremulant')}")
check(div is not None and div.get("tremulant_kind") == "wave",
      f"divisie tremulant_kind == 'wave' (geen synth-LFO-default): {div and div.get('tremulant_kind')}")
stops = all_stops()
check(bool(stops.get("Prestant", {}).get("has_tremulant")),
      f"stop Prestant has_tremulant: {stops.get('Prestant', {}).get('has_tremulant')}")
check(not stops.get("Enkel", {}).get("has_tremulant", False),
      "stop Enkel (geen trem-opnamen) has_tremulant == false")

# ---------------------------------------------------------------- B: schakelen
print("B. POST /tremulant schakelt en spiegelt")
r_on = trem(True)
check(int(r_on.get("stops", 0)) >= 1, f"/tremulant aan → {r_on.get('stops')} registers geschakeld")
st = get("/state")
check(bool(st.get("tremulant", [False])[0]), f"/state.tremulant[0] == true: {st.get('tremulant')}")
r_off = trem(False)
check(not get("/state").get("tremulant", [True])[0], "/state.tremulant[0] weer false na uit")
check(int(r_off.get("stops", 0)) >= 1, "uitzetten schakelt dezelfde registers")

# ---------------------------------------------------------------- C: attack
print("C. Attack-opname volgt de tremulantstand")
draw("Prestant")
trem(False)
f_dry = peak_sustain()
check(f_dry > 0, f"droge grondtoon gemeten: {f_dry:.1f} Hz")
trem(True)
f_trem = peak_sustain()
check(ratio_ok(f_trem, f_dry, 2.0),
      f"met tremulant klinkt de trem-opname (2f): {f_trem:.1f} Hz vs {f_dry:.1f} Hz "
      f"(verhouding {f_trem / f_dry if f_dry else 0:.2f}, verwacht 2,00)")
# Tweede aanslag ná de achtergrond-full-load: mag NIET terugvallen op de droge
# sample (de bug waarbij de gedeelde samples-map de stand vergat).
time.sleep(3.0)
f_trem2 = peak_sustain()
check(ratio_ok(f_trem2, f_dry, 2.0),
      f"tweede aanslag blijft de trem-opname: {f_trem2:.1f} Hz (verhouding "
      f"{f_trem2 / f_dry if f_dry else 0:.2f})")
trem(False)
time.sleep(0.5)
f_dry2 = peak_sustain()
check(ratio_ok(f_dry2, f_dry, 1.0), f"weer droog na uitzetten: {f_dry2:.1f} Hz")

# ---------------------------------------------------------------- D: release
print("D. Release-opname volgt de tremulantstand")
trem(False)
f_rel_dry = peak_release()
check(ratio_ok_octaaf(f_rel_dry, f_dry, 1.26),
      f"droge release (1,26f): {f_rel_dry:.1f} Hz (verhouding "
      f"{f_rel_dry / f_dry if f_dry else 0:.2f}, verwacht 1,26 of een octaaf daarvan)")
trem(True)
f_rel_trem = peak_release()
check(ratio_ok(f_rel_trem, f_dry, 1.5),
      f"tremulant-release (1,5f): {f_rel_trem:.1f} Hz (verhouding "
      f"{f_rel_trem / f_dry if f_dry else 0:.2f}, verwacht 1,50)")
trem(False)

# ---------------------------------------------------------------- E: crossfade
print("E. Tremulant aanzetten tijdens het klinken (crossfade)")
post("/record/start", {"path": REC})
post(f"/notes/{NOTE}/on?velocity=100")
time.sleep(0.8)
trem(True)
time.sleep(1.6)
r = post("/record/stop")
f_cross = float(r.get("peak_hz") or 0.0)
vc = get("/status")["voice_count"]
post(f"/notes/{NOTE}/off")
# De release in deze test-ODF is een VOLLEDIGE noot-opname van enkele seconden
# (geen korte staart), dus even geduld voordat de stemmen weg mogen zijn.
for _ in range(50):
    time.sleep(0.2)
    if get("/status")["voice_count"] == 0:
        break
check(ratio_ok(f_cross, f_dry, 2.0),
      f"na de crossfade domineert de trem-opname: {f_cross:.1f} Hz (verhouding "
      f"{f_cross / f_dry if f_dry else 0:.2f})")
check(vc <= 4, f"geen lekkende stemmen tijdens de crossfade: {vc} stemmen")
check(get("/status")["voice_count"] == 0, "alle stemmen weg na loslaten")
trem(False)
clear_stops()

# ---------------------------------------------------------------- F: regressie
print("F. Regressie en randgevallen")
draw("Enkel")
trem(False)
e_dry = peak_sustain()
trem(True)
e_trem = peak_sustain()
check(e_dry > 0 and ratio_ok(e_trem, e_dry, 1.0),
      f"stop zonder trem-opnamen klinkt onveranderd: {e_dry:.1f} → {e_trem:.1f} Hz")
trem(False)
clear_stops()

logs = "\n".join(get("/logs?lines=600").get("lines", []))
check("Tremulant-samplelaag" in logs,
      "logregel 'Tremulant-samplelaag: N pijpen in M registers' aanwezig")
check("Tremulant ON" in logs and "Tremulant OFF" in logs,
      "SetTremulant-logregels (ON/OFF) aanwezig")

if os.path.exists(ODF_WC):
    print("  TestTremWc.organ (tremulant alléén op de windlade)")
    load(ODF_WC)
    d2 = next((d for d in get("/organ")["divisions"] if d["name"] == "Hoofdwerk"), None)
    check(bool(d2 and d2.get("has_tremulant")),
          f"windlade-tremulant geeft tóch has_tremulant: {d2 and d2.get('has_tremulant')}")
    check(d2 is not None and d2.get("tremulant_kind") == "wave",
          f"tremulant_kind == 'wave': {d2 and d2.get('tremulant_kind')}")
    load(ODF)
    clear_stops()

# POST /settings/trem?enabled=0 werd voorheen genegeerd (stond hard op true).
post("/settings/trem?division=Hoofdwerk&rate=7&enabled=0")
mirror = get("/settings/mirror")
row = next((t for t in mirror.get("tremulants", []) if t["division"] == "Hoofdwerk"), None)
check(row is not None and row.get("enabled") is False,
      f"/settings/trem?enabled=0 overgenomen in de mirror: {row}")
post("/settings/trem?division=Hoofdwerk&rate=6&enabled=1")

print()
if FAILS:
    print(f"{len(FAILS)} FAILS:")
    for f in FAILS:
        print("  - " + f)
    sys.exit(1)
print("Alles geslaagd.")
