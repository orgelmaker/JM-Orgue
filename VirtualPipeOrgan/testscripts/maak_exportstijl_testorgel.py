# Testorgel in de stijl van een geëxporteerde definitie (0.8.94): registers met
# eigen pijpregels (geen Rank-secties), GEEN zwelkasten in de .organ, een
# motor-register "Blower", toetsmechaniek als "Key action <klavier> Attack/
# Release" en registerklikken als "Stop attack/release noise <naam>". De
# zwelkast staat alleen in de akoestische configuratie (rank-gebonden), met
# pijpbindingen pipe_section = StopNNN en het pijpnummer in logical_pipe.
#
# Bouwt op de pijpopnamen van een bestaande GrandOrgue-set met Rank-secties
# (Friesach): twee registers per klavier uit de eerste twee manualen.
#   python maak_exportstijl_testorgel.py "<pad>\Friesach.organ"
# Schrijft Exportstijl-test.organ + JM-ACOUSTICS-exporttest.json in dezelfde map.
import json, os, re, sys

odf = sys.argv[1]
folder = os.path.dirname(odf)
text = open(odf, encoding="utf-8-sig", errors="replace").read()

def sectie(naam):
    m = re.search(r"^\[" + re.escape(naam) + r"\]\s*\n(.*?)(?=^\[|\Z)", text, re.S | re.M)
    d = {}
    if m:
        for line in m.group(1).splitlines():
            if "=" in line and not line.startswith(";"):
                k, v = line.split("=", 1); d[k.strip()] = v.strip()
    return d

def stops_van(manual):
    man = sectie(manual)
    return [int(man[k]) for k in sorted(man) if re.fullmatch(r"Stop\d{3}", k)]

def pijpen_van_stop(n):
    """[(logisch pijpnummer, pad)] van de eerste rank van een register."""
    s = sectie(f"Stop{n:03d}")
    rank = int(s["Rank001"]); first = int(s.get("Rank001FirstPipeNumber", "1"))
    count = int(s.get("Rank001PipeCount", s.get("NumberOfAccessiblePipes", "0")))
    r = sectie(f"Rank{rank:03d}")
    uit = []
    for i in range(count):
        pad = r.get(f"Pipe{first + i:03d}")
        if pad and pad.upper() != "DUMMY": uit.append((i + 1, pad))
    return s.get("Name", f"Stop {n}").strip(), uit, int(r.get("FirstMidiNoteNumber", "36")) + first - 1

# twee registers per klavier: Hoofdwerk (Manual001) en Zwelwerk (Manual002)
gekozen = []
for manual, label in (("Manual001", "Hoofdwerk"), ("Manual002", "Zwelwerk")):
    n = 0
    for sid in stops_van(manual):
        s = sectie(f"Stop{sid:03d}")
        if "Rank001" not in s: continue
        naam, pijpen, eerste = pijpen_van_stop(sid)
        if len(pijpen) < 40: continue
        gekozen.append((label, naam.split(".", 1)[-1].strip().lstrip("HSP ").strip(), pijpen, eerste))
        n += 1
        if n == 2: break
assert len(gekozen) == 4, gekozen

KEYS = 56
out, stops_txt, bindings, ranks = [], [], [], []
stop_nr = 0
manual_stops = {1: [], 2: []}
def stop_sectie(regels):
    global stop_nr
    stop_nr += 1
    stops_txt.append(f"[Stop{stop_nr:03d}]\n" + "\n".join(f"{k}={v}" for k, v in regels) + "\n")
    return stop_nr

for label, naam, pijpen, eerste in gekozen:
    mid = 1 if label == "Hoofdwerk" else 2
    regels = [("Name", naam), ("HarmonicNumber", "8"), ("WindchestGroup", str(mid)), ("NumberOfLogicalPipes", str(KEYS)),
              ("FirstAccessiblePipeLogicalKeyNumber", "1"), ("AmplitudeLevel", "100"), ("AcceptsRetuning", "Y"), ("Percussive", "N")]
    per_nr = dict(pijpen)
    for p in range(1, KEYS + 1):
        pad = per_nr.get(p)
        if pad:
            regels += [(f"Pipe{p:03d}", pad), (f"Pipe{p:03d}Gain", "-6"), (f"Pipe{p:03d}IsTremulant", "0")]
        else:
            regels.append((f"Pipe{p:03d}", "DUMMY"))
    sid = stop_sectie(regels)
    manual_stops[mid].append(sid)
    rid = f"R{sid}"
    ranks.append({"id": rid, "name": f"{label} - {naam}", "enclosure_id": "E1" if mid == 2 else None, "audio_group_id": "G1"})
    for p in range(1, KEYS + 1):
        if p in per_nr:
            bindings.append({"stop_section": f"Stop{sid:03d}", "pipe_section": f"Stop{sid:03d}", "layer_index": 0,
                             "logical_pipe": p, "keyboard_midi_note": 35 + p, "sample_midi_note": 35 + p,
                             "source_rank_id": rid, "attack_path": per_nr[p].replace("\\", "/"), "trem_attack_path": None,
                             "baked_audio_group_gain_db": -6.0})

# Toetsmechaniek: per klavier Attack + Release, één klik per toets (kort pijp-
# fragment uit de set; het gaat om de route en het volume, niet om de klank).
klik = gekozen[0][2][-1][1]
for mid, label in ((1, "Hoofdwerk"), (2, "Zwelwerk")):
    for suffix in ("Attack", "Release"):
        regels = [("Name", f"Key action {label} {suffix}"), ("AcceptsRetuning", "N"), ("Percussive", "Y"),
                  ("NumberOfLogicalPipes", str(KEYS))]
        regels += [(f"Pipe{p:03d}", klik) for p in range(1, KEYS + 1)]
        manual_stops[mid].append(stop_sectie(regels))
# Registerklik voor het eerste register
eerste_naam = gekozen[0][1]
for label in ("attack", "release"):
    stop_sectie([("Name", f"Stop {label} noise {eerste_naam}"), ("AcceptsRetuning", "N"), ("Percussive", "Y"),
                 ("NumberOfLogicalPipes", "1"), ("Pipe001", klik)])
# Motor: een lange, geloopte pijpopname als lopend bijgeluid
motor = gekozen[1][2][0][1]
stop_sectie([("Name", "Blower"), ("AcceptsRetuning", "N"), ("NumberOfLogicalPipes", "1"), ("Pipe001", motor)])

out.append("; Testorgel in exportstijl (maak_exportstijl_testorgel.py)\n")
out.append("[Organ]\nChurchName=Exportstijl-test\nChurchAddress=Test\nOrganBuilder=Test\nOrganBuildDate=2026\n"
           "RecordingDetails=testorgel\nNumberOfManuals=2\nHasPedals=N\nNumberOfWindchestGroups=2\nNumberOfTremulants=0\n"
           "NumberOfEnclosures=0\nGain=-6\nNumberOfReversiblePistons=0\nNumberOfGenerals=0\nNumberOfDivisionalCouplers=0\n"
           f"NumberOfStops={stop_nr}\nNumberOfRanks=0\nNumberOfPanels=0\n")
for mid, label in ((1, "Hoofdwerk"), (2, "Zwelwerk")):
    ids = manual_stops[mid]
    out.append(f"[Manual{mid:03d}]\nName={label}\nMIDIInputNumber={mid}\nNumberOfLogicalKeys={KEYS}\nNumberOfAccessibleKeys={KEYS}\n"
               f"FirstAccessibleKeyMIDINoteNumber=36\nFirstAccessibleKeyLogicalKeyNumber=1\nNumberOfStops={len(ids)}\n"
               "NumberOfCouplers=0\nNumberOfTremulants=0\n" + "".join(f"Stop{i + 1:03d}={sid:03d}\n" for i, sid in enumerate(ids)))
    out.append(f"[WindchestGroup{mid:03d}]\nName={label}\nNumberOfEnclosures=0\nNumberOfTremulants=0\n")
json_naam = "JM-ACOUSTICS-exporttest.json"
out.append(f"[JMOrgueAcoustics]\nFormat=jm-orgue.acoustics\nSchemaVersion=1\nConfigurationFile={json_naam}\nActivation=ExportOnly\n")
organ_pad = os.path.join(folder, "Exportstijl-test.organ")
open(organ_pad, "w", encoding="utf-8").write("".join(out) + "".join(stops_txt))

cfg = {
    "format": "jm-orgue.acoustics", "schema_version": 1, "definition_revision": 1,
    "source": {"package_id": "TEST", "organ_name": "Exportstijl-test", "organ_definition_md5": "", "metadata_path": "", "metadata_md5": "", "manifest_path": ""},
    "activation": {"mode": "export_only", "automatic_dsp_activation": False, "fallback": "Use the .organ definition.", "unverified": []},
    "compatibility": {"audio_group_gain_already_in_organ_pipe_gain": True, "swell_processing_in_organ": False, "spatial_processing_in_organ": False, "effect_on_loops_in_organ": True, "effect_off_samples_in_organ": False},
    "summary": {"enclosures": 1, "impulse_responses": 0, "audio_groups": 1, "mix_groups": 0, "outputs": 0, "effect_registers": 0, "pipe_bindings": len(bindings), "unresolved_references": 0},
    "enclosures": [{"id": "E1", "name": "Zwelwerk"}],
    "ranks": ranks,
    "sliders": [{"id": "S1", "name": "Zwel Expr.", "enclosure_targets": [{"enclosure_id": "E1"}]}],
    "audio_groups": [{"id": "G1", "name": "Orgel"}],
    "impulse_responses": [], "mix_groups": [], "outputs": [],
    "sound_assets": [], "sound_switches": [], "stops": [], "pipe_bindings": bindings, "unresolved_references": [],
}
open(os.path.join(folder, json_naam), "w", encoding="utf-8").write(json.dumps(cfg, indent=1, ensure_ascii=False))
print(json.dumps({"organ": organ_pad, "stops": [(g[0], g[1], len(g[2])) for g in gekozen], "bindings": len(bindings), "stops_totaal": stop_nr}, ensure_ascii=False))
