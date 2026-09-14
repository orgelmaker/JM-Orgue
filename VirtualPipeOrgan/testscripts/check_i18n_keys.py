import json, sys, io
base = r"C:/Bronbestanden/JM-Orgue/VirtualPipeOrgan/ui/src/lib/locales/"
def flat(d, p=""):
    out = set()
    for k, v in d.items():
        key = f"{p}.{k}" if p else k
        if isinstance(v, dict): out |= flat(v, key)
        else: out.add(key)
    return out
sets = {}
for l in ["nl", "en", "fr", "de", "pl", "it", "es"]:
    with io.open(base + l + ".json", encoding="utf-8") as f:
        sets[l] = flat(json.load(f))
ref = sets["nl"]
ok = True
for l in ["en", "fr", "de", "pl", "it", "es"]:
    missing = ref - sets[l]; extra = sets[l] - ref
    if missing or extra:
        ok = False
        print(l, "MISSING:", sorted(missing), "EXTRA:", sorted(extra))
print("midi_archive keys nl:", len([k for k in ref if k.startswith("midi_archive.")]))
print("status_bar.archiving in nl:", "status_bar.archiving" in ref, "status_bar.archiving_title" in ref)
print("OK" if ok else "MISMATCH")
sys.exit(0 if ok else 1)
