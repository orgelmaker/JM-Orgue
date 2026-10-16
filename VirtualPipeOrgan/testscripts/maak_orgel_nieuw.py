# Maakt een orgel "nieuw" voor een wizard-rooktest: verwijdert de opgeslagen
# instellingen uit organ_library.json en hernoemt de .jm-settings.json naast
# het orgel (reservekopieën met achtervoegsel .bak-wizardtest). Alleen
# uitvoeren met gesloten app. Gebruik: python maak_orgel_nieuw.py <deel-van-id>
import json, os, shutil, sys
lib = r"C:\Users\orgel\AppData\Roaming\nl.jm-orgue.app\organ_library.json"
deel = sys.argv[1].lower()
d = json.load(open(lib, encoding="utf-8"))
shutil.copy(lib, lib + ".bak-wizardtest")
keys = [k for k in d.get("settings", {}) if deel in k.lower()]
for k in keys:
    del d["settings"][k]
    pad = k
    map_ = pad if os.path.isdir(pad) else os.path.dirname(pad)
    side = os.path.join(map_, ".jm-settings.json")
    if os.path.exists(side):
        shutil.move(side, side + ".bak-wizardtest")
        print("zijbestand opzij:", side)
json.dump(d, open(lib, "w", encoding="utf-8"), indent=2, ensure_ascii=False)
print("instellingen verwijderd voor:", keys)
