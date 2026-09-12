# API-test automatisch MIDI-archief (0.7.38) — draaien tegen vpo-app.exe --test-api (poort 8765).
#
#   python test_midi_archive.py [--organ <pad naar .organ>]
#
# Scenario (plan_archief.json 'tests' + correcties uit de toetsing):
#  1. orgel geladen (of laden: --organ, anders eerste bibliotheek-item)
#  2. config: enabled=1, silence=3 (MIN_SILENCE_SECS-clamp), min_notes=4, min_secs=0, dir=scratch-map
#  3. register trekken
#  4. 4 noten injecteren via /midi/inject (BLE-lus → wordt gearchiveerd)
#  5. status: archiving=true, event_count=8; /status midi_archiving=true
#  6. wachten (4 s + poll-lus) → archiving=false, files_written=1, last_file bestaat, naam-regex
#  7. /midi/archive/list → files[0].path == last_file, size_bytes > 60
#  8. afspelen via /midi/player/play → state=playing, position_ms loopt op (voice_count>0 alleen
#     als het orgel een ingeleerde kanaal-mapping heeft — /settings/organ midi_mappings)
#  9. afspelen wordt NIET gearchiveerd: files_written blijft 1
# 10. min-noten-filter: 1 noot → afgekeurd, files_written blijft 1
# 11. flush: 4 noten + POST /midi/archive/flush → file != null, files_written=2
# 12. enabled=0 → status enabled=false; noten → archiving blijft false
# 13. persistentie: %APPDATA%\nl.jm-orgue.app\midi_archive.json bestaat
# Aan het eind worden de oorspronkelijke instellingen teruggezet en het register losgelaten.
import json, os, re, sys, time, urllib.parse, urllib.request

B = "http://127.0.0.1:8765"
SP = os.path.dirname(os.path.abspath(__file__))

def post(p, b=None, t=300):
    d = json.dumps(b).encode() if b is not None else b"{}"
    r = urllib.request.Request(B + p, data=d, method="POST", headers={"Content-Type": "application/json"})
    return json.loads(urllib.request.urlopen(r, timeout=t).read())

def get(p, t=30):
    return json.loads(urllib.request.urlopen(B + p, timeout=t).read())

results = []
def check(label, ok, detail=""):
    results.append((label, bool(ok)))
    print(("  OK   " if ok else "  FAIL ") + label + (f"  [{detail}]" if detail else ""))
    return ok

def wait_archive(expect_files, max_s=3.0):
    """Poll-lus (max. max_s, elke 0,25 s) tot archiving=false én files_written == expect_files."""
    st = None
    t0 = time.time()
    while time.time() - t0 < max_s:
        st = get("/midi/archive/status")
        if not st["archiving"] and st["files_written"] == expect_files:
            return st
        time.sleep(0.25)
    return st

def inject(note, on, ch):
    return post(f"/midi/inject?note={note}&on={1 if on else 0}&channel={ch}")

def play_notes(notes, ch):
    for n in notes:
        inject(n, True, ch); time.sleep(0.25)
        inject(n, False, ch); time.sleep(0.1)

def norm(p):
    return os.path.normcase(os.path.normpath(p)).replace("\\\\?\\", "")

# ---------- 1. orgel ----------
args = sys.argv[1:]
organ_path = None
if "--organ" in args:
    organ_path = args[args.index("--organ") + 1]
st = get("/status")
if not st["organ_loaded"]:
    if organ_path is None:
        lib = get("/library")["organs"]
        if not lib:
            print("Geen orgel geladen en bibliotheek leeg — geef --organ <pad>."); sys.exit(2)
        organ_path = lib[0]["source_path"]
    print("Orgel laden:", organ_path)
    post("/load_organ", {"path": organ_path})
    for _ in range(120):
        time.sleep(1)
        if get("/status")["organ_loaded"]: break
check("orgel geladen", get("/status")["organ_loaded"])

# Kanaal-mapping (voorwaarde voor 'voice_count > 0' bij afspelen; zie toetsing).
ch = 0
has_mapping = False
lo_note, hi_note = 36, 96
try:
    s = get("/settings/organ")
    maps = s.get("midi_mappings") or []
    for m in maps:
        if m.get("channel") is not None:
            ch = int(m["channel"]); has_mapping = True
            if m.get("first_midi_note") is not None: lo_note = int(m["first_midi_note"])
            if m.get("last_midi_note") is not None: hi_note = int(m["last_midi_note"])
            break
except Exception as e:
    print("  (settings/organ niet leesbaar:", str(e)[:60], ")")
print(f"  kanaal voor injectie: {ch} (mapping aanwezig: {has_mapping}, bereik {lo_note}-{hi_note})")
NOTES = [n for n in (60, 64, 67, 72) if lo_note <= n <= hi_note] or [lo_note + i * 4 for i in range(4)]

# ---------- oorspronkelijke instellingen bewaren ----------
orig = get("/midi/archive/status")
print("  oorspronkelijk:", {k: orig.get(k) for k in ("enabled", "dir", "dir_is_default", "silence_secs", "min_notes", "min_secs")})

# ---------- 2. config ----------
scratch = os.path.join(SP, "archief_test_" + time.strftime("%Y%m%d-%H%M%S"))
cfg = post("/midi/archive/config?enabled=1&silence=3&min_notes=4&min_secs=0&dir=" + urllib.parse.quote(scratch, safe=""))
check("config enabled=true", cfg.get("enabled") is True, str(cfg))
check("config dir=scratch", norm(cfg.get("dir", "")) == norm(scratch), cfg.get("dir"))
check("config silence=3 (clamp)", cfg.get("silence_secs") == 3)
check("config dir_is_default=false", cfg.get("dir_is_default") is False)

# ---------- 13. persistentie ----------
appdata = os.environ.get("APPDATA", "")
pref_file = os.path.join(appdata, "nl.jm-orgue.app", "midi_archive.json")
check("midi_archive.json geschreven", os.path.isfile(pref_file), pref_file)
if os.path.isfile(pref_file):
    with open(pref_file, encoding="utf-8") as f:
        pj = json.load(f)
    check("midi_archive.json enabled=true", pj.get("enabled") is True)

# ---------- 3. register ----------
info = get("/organ")
stops = [s for d in info["divisions"] for s in d["stops"]]
stop = stops[0] if stops else None
toggled = False
if stop and not stop.get("drawn"):
    post(f"/stops/{stop['id']}/toggle"); toggled = True; time.sleep(0.3)
check("register getrokken", stop is not None, stop["name"] if stop else "geen stops")

# ---------- 4./5. vier noten → take loopt ----------
play_notes(NOTES, ch)
st = get("/midi/archive/status")
check("archiving=true na 4 noten", st["archiving"], str(st))
check("event_count=8", st["event_count"] == 8, str(st["event_count"]))
check("/status midi_archiving=true", get("/status").get("midi_archiving") is True)

# ---------- 6. stilte → bestand ----------
time.sleep(4)
st = wait_archive(1)
check("archiving=false na stilte", st and not st["archiving"], str(st))
check("files_written=1", st and st["files_written"] == 1, str(st and st["files_written"]))
last = (st or {}).get("last_file")
check("last_file eindigt op .mid en bestaat", last and last.lower().endswith(".mid") and os.path.isfile(last), str(last))
check("bestandsnaam-regex", last and re.match(r"^\d{4}-\d{2}-\d{2}_\d{2}-\d{2}-\d{2}(_.*)?\.mid$", os.path.basename(last)) is not None, os.path.basename(last or ""))
check("bestand in scratch-map", last and norm(os.path.dirname(last)) == norm(scratch))
check("last_error leeg", not (st or {}).get("last_error"), str((st or {}).get("last_error")))

# ---------- 7. lijst ----------
lst = get("/midi/archive/list")["files"]
check("list: 1 bestand", len(lst) == 1, str(len(lst)))
check("list[0].path == last_file", lst and norm(lst[0]["path"]) == norm(last or ""))
check("list[0].size_bytes > 60", lst and lst[0]["size_bytes"] > 60, str(lst and lst[0]["size_bytes"]))

# ---------- 8. afspelen ----------
r = post("/midi/player/play", {"path": last})
check("player/play ok", r.get("ok") is True, str(r))
time.sleep(0.5)
ps1 = get("/midi/player/status")
check("player state=playing", ps1["state"] == "playing", str(ps1))
vc = get("/status")["voice_count"]
if has_mapping:
    check("voice_count > 0 tijdens afspelen", vc > 0, str(vc))
else:
    print(f"  (geen kanaal-mapping: voice_count={vc} niet geasserteerd)")
time.sleep(0.5)
ps2 = get("/midi/player/status")
check("position_ms loopt op", ps2["position_ms"] > ps1["position_ms"], f"{ps1['position_ms']} → {ps2['position_ms']}")
time.sleep(1.5)
post("/midi/player/stop")
check("player gestopt", get("/midi/player/status")["state"] == "stopped")

# ---------- 9. afspelen niet gearchiveerd ----------
time.sleep(4)
st = wait_archive(1)
check("afspelen niet gearchiveerd (files_written=1)", st and st["files_written"] == 1 and not st["archiving"], str(st))

# ---------- 10. min-noten-filter ----------
play_notes(NOTES[:1], ch)
check("take gestart met 1 noot", get("/midi/archive/status")["archiving"])
time.sleep(4)
st = wait_archive(1)
check("1 noot afgekeurd (files_written blijft 1)", st and st["files_written"] == 1 and not st["archiving"], str(st))

# ---------- 11. flush ----------
play_notes(NOTES, ch)
fl = post("/midi/archive/flush")
check("flush file != null", fl.get("ok") and fl.get("file"), str(fl))
st = get("/midi/archive/status")
check("files_written=2 na flush", st["files_written"] == 2 and not st["archiving"], str(st))
check("flush-bestand bestaat", fl.get("file") and os.path.isfile(fl["file"]))
lst = get("/midi/archive/list")["files"]
check("list: 2 bestanden, nieuwste eerst", len(lst) == 2 and norm(lst[0]["path"]) == norm(fl.get("file") or ""))

# ---------- 12. uitzetten ----------
cfg = post("/midi/archive/config?enabled=0")
check("config enabled=false", cfg.get("enabled") is False)
check("status enabled=false", get("/midi/archive/status")["enabled"] is False)
play_notes(NOTES[:2], ch)
check("uit: archiving blijft false", get("/midi/archive/status")["archiving"] is False)

# ---------- herstel ----------
if toggled:
    post(f"/stops/{stop['id']}/toggle")
restore = "/midi/archive/config?enabled={}&silence={}&min_notes={}&min_secs={}&dir={}".format(
    1 if orig.get("enabled") else 0, orig.get("silence_secs", 20), orig.get("min_notes", 4), orig.get("min_secs", 5),
    "" if orig.get("dir_is_default", True) else urllib.parse.quote(orig.get("dir", ""), safe=""))
r = post(restore)
check("instellingen hersteld", r.get("enabled") == bool(orig.get("enabled")) and r.get("dir_is_default") == orig.get("dir_is_default", True), str(r))
print("  archiefbestanden van deze test staan in:", scratch)

failed = [l for l, ok in results if not ok]
print(f"\n{len(results) - len(failed)}/{len(results)} checks OK" + (f"; GEFAALD: {failed}" if failed else ""))
sys.exit(1 if failed else 0)
