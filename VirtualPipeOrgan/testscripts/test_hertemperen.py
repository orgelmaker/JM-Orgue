# Hertemperen op gemeten pijptoonhoogte (0.7.38) — API-test tegen vpo-app --test-api (poort 8765).
#
# Gebruik:
#   python test_hertemperen.py                 -> TestRanks.organ (scratchpad go_testodf, Puttershoek-WAVs)
#   python test_hertemperen.py friesach        -> Friesach (HW Principal 8' + SW Vox celeste 8')
#   python test_hertemperen.py puttershoek     -> Bätz-Witte Puttershoek (JM-Rec, via /load_directory: regressie, 0 metingen)
#
# Meet per modus de grondtoon via peak_hz van POST /record/stop (FFT-piek, laatste ~2,7 s), genormaliseerd
# naar het dichtstbijzijnde veelvoud van de ET-doeltoon (k = round(peak / f_et)).
#
# Verwachtingen (±3 ct):
#   TestRanks (Samples = Puttershoek-WAVs: smpl unity = toets, fractie 0 -> gemeten = doel):
#     "Enkel"     (Rank003, geen PitchCorrection) noot 60/62:  Origineel 0, ET 0, middentoon +10.27 (C) / +3.42 (D)
#     "Gestapeld" (Rank001 per pijp PitchCorrection=-20; Rank002 = mp3 -> geen meting, valt terug op PitchTuning 0):
#                 noot 60/62: Origineel 0, ET -20, middentoon -20 + T[noot]
#                 (klinkt Rank002 na de stapel-ranks-feature mee, dan is de FFT-piek een mengsel: -20 en 0 ct)
#     GET /tuning: retune_pipes == retune_total - 54 (de 54 mp3-pijpen van Rank002 hebben geen meting)
#   Friesach: 1_11 (HW Principal 8') noot 60: Origineel +6.40, ET 0.00, middentoon +10.27;
#             noot 64: Origineel +7.21, ET 0.00, middentoon -3.42;
#             2_28 (SW Vox celeste 8') noot 60: Origineel +22.96, ET +15.62, middentoon +25.89
#             (ET-meting ~0 of ~+38.6 => teken van PitchCorrection verkeerd)
#   Puttershoek: retune_pipes == 0; ET == Origineel (±0.5 ct); middentoon = Origineel + T[noot]
import json, urllib.request, time, sys, os, math, subprocess, tempfile

B = "http://127.0.0.1:8765"
SP = os.path.dirname(os.path.abspath(__file__))

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

ODF_TEST = _testranks_odf()
ODF_FRIESACH = r"C:\Bronbestanden\JM-Orgue\Sample set homemade\Friesach_GrandOrgue\Friesach.organ"
DIR_PUTTERSHOEK = r"C:\Bronbestanden\JM-Orgue\Sample set homemade\Bätz-Witte Puttershoek"
REC = os.path.join(os.environ.get("TEMP", SP), "jm_tune.mp3")

MEANTONE = [10.27, -13.69, 3.42, 20.53, -3.42, 13.69, -10.27, 6.84, -17.11, 0, 17.11, -6.84]
MODES = [
    ("Origineel", {"name": "Original (as recorded)", "cents": [0] * 12, "retune": False}),
    ("ET",        {"name": "Equal Temperament", "cents": [0] * 12, "retune": True}),
    ("Middentoon", {"name": "Meantone 1/4", "cents": MEANTONE, "retune": True}),
]

def post(p, b=None, t=600):
    d = json.dumps(b).encode("utf-8") if b is not None else b"{}"
    r = urllib.request.Request(B + p, data=d, method="POST", headers={"Content-Type": "application/json"})
    return json.loads(urllib.request.urlopen(r, timeout=t).read())

def get(p, t=30):
    return json.loads(urllib.request.urlopen(B + p, timeout=t).read())

def f_et(note):
    return 440.0 * 2 ** ((note - 69) / 12)

def measure(stop_id, note, hold=3.0):
    post(f"/stops/{stop_id}/toggle"); time.sleep(0.3)
    post("/record/start", {"path": REC}); time.sleep(0.5)
    post(f"/notes/{note}/on?velocity=100"); time.sleep(hold); post(f"/notes/{note}/off"); time.sleep(0.3)
    r = post("/record/stop")
    post(f"/stops/{stop_id}/toggle"); time.sleep(0.4)
    peak = float(r.get("peak_hz") or 0.0)
    if peak <= 0:
        return float("nan"), peak
    # peak_hz is de STERKSTE piek, niet per se de grondtoon: vouw eerst hele
    # octaven weg (16'/4'-ranks klinken een octaaf onder/boven de nominale toon)
    # en deel daarna door de dichtstbijzijnde hele boventoon.
    ratio = peak / f_et(note)
    ratio /= 2 ** round(math.log2(ratio))
    k = max(1, round(ratio))
    return 1200 * math.log2(ratio / k), peak

def expected_cents(mode, base, note):
    return base + (MEANTONE[note % 12] if mode == "Middentoon" else 0.0)

def run(cases, expect, relatief=False):
    """cases: [(label, stop_id, note, base_cents_per_mode)], expect: dict mode->fn(base)->cents

    Met `relatief=True` gelden de verwachte waarden TEN OPZICHTE VAN de gemeten
    stand in "Origineel". Dat is nodig voor sets die zelf niet op a=440 staan: de
    Puttershoek-opnamen klinken ~30 cent hoog, en "Origineel (zoals opgenomen)"
    hoort die stand juist te laten staan. Getoetst wordt dan de VERSCHUIVING die
    een echt temperament aanbrengt: de opgegeven PitchCorrection plus de
    temperament-afwijking van die toon.
    """
    fails = 0
    for label, stop_id, note, base in cases:
        print(f"\n== {label} (stop {stop_id}, noot {note}) ==")
        nul = 0.0
        if relatief:
            post("/temperament", MODES[0][1])
            nul, _ = measure(stop_id, note)
            print(f"  (referentie 'Origineel': {nul:+7.2f} ct — verwachtingen gelden hierop)")
        for mode, body in MODES:
            r = post("/temperament", body)
            assert r.get("retune") == body["retune"], r
            t = get("/tuning")
            assert t["retune"] == body["retune"], t
            cents, peak = measure(stop_id, note)
            exp = expected_cents(mode, base[mode], note) + nul
            ok = abs(cents - exp) <= 3.0
            fails += 0 if ok else 1
            print(f"  {mode:11s}: peak {peak:9.3f} Hz -> {cents:+7.2f} ct  (verwacht {exp:+7.2f})  {'OK' if ok else 'FOUT'}")
    return fails

def stops_by_name():
    info = get("/organ")
    return {s["name"]: s for d in info["divisions"] for s in d["stops"]}, info

def prepare():
    post("/panic")
    post("/reverb", {"mix": 0.0})
    post("/settings/wind?enabled=0")
    for s in (get("/stops/drawn").get("drawn_stops") or []):
        post(f"/stops/{s}/toggle")

def main():
    which = (sys.argv[1] if len(sys.argv) > 1 else "testranks").lower()
    if which == "friesach":
        post("/load_organ", {"path": ODF_FRIESACH}); time.sleep(6)
    elif which == "puttershoek":
        post("/load_directory", {"path": DIR_PUTTERSHOEK}); time.sleep(6)
    else:
        post("/load_organ", {"path": ODF_TEST}); time.sleep(4)
    prepare()
    tuning = get("/tuning")
    print("GET /tuning:", tuning)
    stops, info = stops_by_name()
    print("stops:", {k: v["id"] for k, v in stops.items()})
    fails = 0

    if which == "friesach":
        assert tuning["retune_total"] > 0 and tuning["retune_pipes"] >= 0.9 * tuning["retune_total"], tuning
        principal = next(v["id"] for k, v in stops.items() if "Principal 8" in k and v["id"].startswith("1_"))
        celeste = next(v["id"] for k, v in stops.items() if "celeste" in k.lower())
        fails += run([
            ("HW Principal 8' c'", principal, 60, {"Origineel": 6.40, "ET": 0.0, "Middentoon": 0.0}),
            ("HW Principal 8' e'", principal, 64, {"Origineel": 7.21, "ET": 0.0, "Middentoon": 0.0}),
            ("SW Vox celeste 8' c'", celeste, 60, {"Origineel": 22.96, "ET": 15.62, "Middentoon": 15.62}),
        ], None)
    elif which == "puttershoek":
        assert tuning["retune_pipes"] == 0, tuning
        sid = next(iter(stops.values()))["id"]
        # Zonder metingen: ET == Origineel; middentoon = Origineel + T[noot]. Basis = gemeten Origineel.
        post("/temperament", MODES[0][1]); base, _ = measure(sid, 60)
        print(f"Origineel-basis noot 60: {base:+.2f} ct")
        fails += run([("eerste register c'", sid, 60, {"Origineel": base, "ET": base, "Middentoon": base})], None)
    else:
        # TestRanks: 3 ranks x 54 pijpen; Rank002 (mp3) heeft geen meting.
        print(f"retune_pipes={tuning['retune_pipes']} retune_total={tuning['retune_total']} (verwacht total-54 metingen)")
        if tuning["retune_pipes"] != tuning["retune_total"] - 54:
            print("  LET OP: aantal metingen wijkt af van de verwachting")
        enkel = stops["Enkel"]["id"]
        pijp_pc = stops["Enkel pijp-PC"]["id"]; rank_pc = stops["Enkel rank-PC"]["id"]
        fails += run([
            ("Enkel c' (geen PC)", enkel, 60, {"Origineel": 0.0, "ET": 0.0, "Middentoon": 0.0}),
            ("Enkel d' (geen PC)", enkel, 62, {"Origineel": 0.0, "ET": 0.0, "Middentoon": 0.0}),
            # PitchCorrection per pijp (-20 ct) en per rank (+30 ct), elk op een
            # ENKELVOUDIG register: "Origineel" laat de opname staan, een echt
            # temperament rekent de opgegeven afwijking weg.
            ("Enkel pijp-PC c' (-20 ct)", pijp_pc, 60, {"Origineel": 0.0, "ET": -20.0, "Middentoon": -20.0}),
            ("Enkel pijp-PC d' (-20 ct)", pijp_pc, 62, {"Origineel": 0.0, "ET": -20.0, "Middentoon": -20.0}),
            ("Enkel rank-PC c' (+30 ct)", rank_pc, 60, {"Origineel": 0.0, "ET": 30.0, "Middentoon": 30.0}),
        ], None, relatief=True)

    # Persistentie: ET+retune opslaan, herladen, /tuning moet retune=true tonen.
    post("/temperament", MODES[1][1]); post("/settings/save")
    so = get("/settings/organ")
    tsav = (so.get("temperament") or {}) if isinstance(so, dict) else {}
    print("\nopgeslagen temperament:", tsav)
    if tsav.get("retune") is not True:
        print("  FOUT: retune niet als true opgeslagen"); fails += 1
    # Terug naar Origineel zodat de set niet met ET-retune achterblijft.
    post("/temperament", MODES[0][1]); post("/settings/save")
    print(f"\nKlaar: {fails} afwijking(en) > 3 ct.")
    sys.exit(1 if fails else 0)

if __name__ == "__main__":
    main()
