# Neutraal testorgel voor de akoestische configuratie (0.8.92, plan §9 A):
# kopieert een GrandOrgue-definitie naar <naam>-akoestiek-test.organ met een
# [JMOrgueAcoustics]-sectie en schrijft een JM-ACOUSTICS.json ernaast met één
# zwelkast (eerste twee registers van het eerste manuaal), één rank buiten de
# kast, een audiogroep, één impulsrespons (een bestaand WAV uit de set),
# één mengroute en één uitgang. Gebruik:
#   python maak_akoestiek_testorgel.py "<pad>\Friesach.organ"
import json, re, struct, sys, os

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

man = sectie("Manual001")
stops = [int(man[k]) for k in sorted(man) if re.fullmatch(r"Stop\d{3}", k)]
assert len(stops) >= 3, "manuaal 1 heeft te weinig registers"
in_kast = stops[:2]; buiten = stops[2]
def pijpen(n): return int(sectie(f"Stop{n:03d}").get("NumberOfAccessiblePipes", "0"))
def naam(n): return sectie(f"Stop{n:03d}").get("Name", f"Stop {n}")

# Een bestaand WAV als "impulsrespons" (functionele test, geen echte IR).
ir_rel = None
for root, _, files in os.walk(folder):
    for f in files:
        if f.lower().endswith(".wav"):
            ir_rel = os.path.relpath(os.path.join(root, f), folder).replace("\\", "/"); break
    if ir_rel: break
assert ir_rel, "geen WAV gevonden"
with open(os.path.join(folder, ir_rel), "rb") as fh:
    hdr = fh.read(64)
ch = struct.unpack_from("<H", hdr, 22)[0]; rate = struct.unpack_from("<I", hdr, 24)[0]; bits = struct.unpack_from("<H", hdr, 34)[0]

ranks = [{"id": f"R{n}", "name": naam(n), "enclosure_id": "E1", "audio_group_id": "G1"} for n in in_kast]
ranks.append({"id": f"R{buiten}", "name": naam(buiten), "enclosure_id": None, "audio_group_id": "G1"})
bindings = []
for n in in_kast + [buiten]:
    for p in range(1, pijpen(n) + 1):
        bindings.append({"stop_section": f"Stop{n:03d}", "pipe_section": f"Pipe{p:03d}", "layer_index": 0,
                         "logical_pipe": p, "keyboard_midi_note": 35 + p, "sample_midi_note": 35 + p,
                         "source_rank_id": f"R{n}", "attack_path": "", "trem_attack_path": None,
                         "baked_audio_group_gain_db": 0.0})
cfg = {
    "format": "jm-orgue.acoustics", "schema_version": 1, "definition_revision": 1,
    "source": {"package_id": "TEST", "organ_name": "Akoestiek-testorgel", "organ_definition_md5": "", "metadata_path": "", "metadata_md5": "", "manifest_path": ""},
    "activation": {"mode": "export_only", "automatic_dsp_activation": False, "fallback": "Use the .organ definition when no compatible reader is available.", "unverified": ["IR channel roles", "mix laws"]},
    "compatibility": {"audio_group_gain_already_in_organ_pipe_gain": True, "swell_processing_in_organ": False, "spatial_processing_in_organ": False, "effect_on_loops_in_organ": True, "effect_off_samples_in_organ": False},
    "summary": {"enclosures": 1, "impulse_responses": 1, "audio_groups": 1, "mix_groups": 1, "outputs": 1, "effect_registers": 0, "pipe_bindings": len(bindings), "unresolved_references": 0},
    "enclosures": [{"id": "E1", "name": "Testkast"}],
    "ranks": ranks,
    "sliders": [{"id": "S1", "name": "Zweltrede", "enclosure_targets": [{"enclosure_id": "E1"}]}],
    "audio_groups": [{"id": "G1", "name": "Hoofdwerk", "source_gain_db": -19.0}],
    "impulse_responses": [{"id": "IR1", "name": "Test-IR", "path": ir_rel, "source_path": ir_rel, "source_gain_db": 0.0,
                           "wav": {"channels": ch, "sample_rate_hz": rate, "bits_per_sample": bits, "format_tag": 1, "frames": 48000, "channel_roles": "unverified"}}],
    "mix_groups": [{"id": "M1", "name": "Kerk", "impulse_response_id": "IR1", "inputs": [{"audio_group_id": "G1"}]}],
    "outputs": [{"id": "O1", "name": "Stereo", "inputs": [{"mix_group_id": "M1"}]}],
    "sound_assets": [], "sound_switches": [], "stops": [], "pipe_bindings": bindings, "unresolved_references": [],
}
json_naam = "JM-ACOUSTICS-test.json"
open(os.path.join(folder, json_naam), "w", encoding="utf-8").write(json.dumps(cfg, indent=1, ensure_ascii=False))
base = os.path.splitext(os.path.basename(odf))[0]
uit = os.path.join(folder, f"{base}-akoestiek-test.organ")
extra = f"\n[JMOrgueAcoustics]\nFormat=jm-orgue.acoustics\nSchemaVersion=1\nConfigurationFile={json_naam}\nActivation=ExportOnly\n"
open(uit, "w", encoding="utf-8").write(text + extra)
print(json.dumps({"organ": uit, "json": os.path.join(folder, json_naam), "in_kast": in_kast, "buiten": buiten,
                  "pijpen": [pijpen(n) for n in in_kast + [buiten]], "ir": ir_rel}))
