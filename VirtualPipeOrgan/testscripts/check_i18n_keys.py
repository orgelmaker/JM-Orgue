"""Controleert of alle vertalingen dezelfde sleutels hebben als het Nederlands.

Het pad wordt afgeleid van de plek van DIT script (testscripts/ ligt naast ui/),
zodat het zowel op de ontwikkel-pc als op een CI-runner (Linux, ander pad) werkt.
Draaien kan vanuit elke map: python VirtualPipeOrgan/testscripts/check_i18n_keys.py
"""
import json, sys, io, os
base = os.path.join(os.path.dirname(os.path.abspath(__file__)), os.pardir,
                    "ui", "src", "lib", "locales") + os.sep
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
