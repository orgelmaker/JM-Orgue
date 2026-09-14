# Test: JM-Rec-import (bibliotheeknaam + orgelafbeelding) en de map-loader.
#
#  1. Friesach_GrandOrgue-MAP laden (bevat Friesach.organ) -> 44 stops + koppels
#  2. Puttershoek-map laden (geen .organ) -> gewone mapscan (20 stops)
#  3. JM-Rec-projectmap (hier zelf gemaakt): naam moet
#     "Kerknaam - Orgelbouwer - Plaats" worden, builder/location gevuld,
#     en orgel.jpg uit de projectmap moet als afbeelding gevonden worden
#  4. Kerknaam leeghalen in het manifest en opnieuw laden: de bestaande
#     bibliotheek-entry wordt VERVERST (zelfde id/pad, nieuwe naam)
#  5. Afbeelding-zoektocht op echte GO/HW-sets (paneelbeeld uit de ODF,
#     pakketfoto bij Hauptwerk)
#
# Starten: vpo-app.exe --test-api (poort 8765), daarna dit script draaien.
import json, urllib.request, time, os, subprocess, shutil, sys

B = "http://127.0.0.1:8765"
SCRATCH = os.path.join(os.path.dirname(os.path.abspath(__file__)), "jmrec_naamtest")
PUTT = r"C:\Bronbestanden\JM-Orgue\Sample set homemade\Bätz-Witte Puttershoek"
FOTO = r"C:\Bronbestanden\JM-Orgue\testen\JM-Orgue testorgel\orgel.jpg"

fouten = []


def post(p, b=None, t=240):
    d = json.dumps(b).encode() if b is not None else None
    r = urllib.request.Request(B + p, data=d, method="POST", headers={"Content-Type": "application/json"})
    return json.loads(urllib.request.urlopen(r, timeout=t).read())


def get(p, t=30):
    return json.loads(urllib.request.urlopen(B + p, timeout=t).read())


def check(label, ok, detail=""):
    print(f"{'OK  ' if ok else 'FOUT'} | {label}{(' | ' + detail) if detail else ''}")
    if not ok:
        fouten.append(label)


def lib_entry(padstuk):
    """Bibliotheek-entry waarvan het pad op `padstuk` eindigt (case-ongevoelig)."""
    for o in get("/library")["organs"]:
        if o["source_path"].lower().replace("/", "\\").endswith(padstuk.lower().replace("/", "\\")):
            return o
    return None


# ---------------------------------------------------------------- 1 en 2
for label, pad, verwacht_stops in [
    ("Friesach-MAP (met .organ)", r"C:\Bronbestanden\JM-Orgue\Sample set homemade\Friesach_GrandOrgue", 44),
    ("Puttershoek-map (zonder .organ)", PUTT, 20),
]:
    post("/load_directory", {"path": pad})
    info = get("/organ")
    n_stops = sum(len(d["stops"]) for d in info.get("divisions", []))
    n_coup = len(info.get("couplers") or [])
    check(f"{label}: {n_stops} registers (verwacht {verwacht_stops})",
          n_stops == verwacht_stops, f"'{info['name']}' | koppels={n_coup}")
    time.sleep(1)


# ---------------------------------------------------------------- JM-Rec-set bouwen
def maak_jmrec_set(kerk, bouwer, plaats, met_foto=True):
    """Bouw <scratch>/NaamTest/ zoals JM-Rec hem oplevert: <stem>.organ,
    <stem>.jm-rec.json en (optioneel) orgel.jpg. De samples komen via een
    NTFS-junction (GrandOrgue-ODF's gebruiken relatieve paden)."""
    os.makedirs(SCRATCH, exist_ok=True)
    link = os.path.join(SCRATCH, "Samples").replace("/", "\\")  # mklink wil backslashes
    if not os.path.exists(link):
        subprocess.run(["cmd", "/c", "mklink", "/J", link, PUTT], check=True, capture_output=True)

    reg = os.path.join(PUTT, "Hoofdwerk", "Prestant_8")
    noten = sorted(f for f in os.listdir(reg) if f.lower().endswith((".wav", ".mp3")) and f[:3].isdigit())[:12]
    eerste = int(noten[0][:3])
    n = len(noten)

    L = ["[Organ]",
         f"ChurchName={kerk or 'NaamTest'}",   # JM-Rec zet de MAPCODE als de kerknaam leeg is
         f"ChurchAddress={plaats}",
         f"OrganBuilder={bouwer}",
         "OrganComments=Opgenomen met JM-Rec v3.10",
         "RecordingDetails=44100 Hz, 16-bit, mp3",
         "NumberOfManuals=1", "HasPedals=N", "NumberOfEnclosures=0",
         "NumberOfTremulants=0", "NumberOfWindchestGroups=1", "",
         "[WindchestGroup001]", "Name=Main", "NumberOfEnclosures=0", "NumberOfTremulants=0", "",
         "[Manual001]", "Name=Hoofdwerk", "MIDIInputNumber=1",
         f"NumberOfLogicalKeys={n}", f"NumberOfAccessibleKeys={n}",
         f"FirstAccessibleKeyMIDINoteNumber={eerste}",
         "NumberOfStops=1", "Stop001=001", "NumberOfCouplers=0", "NumberOfDivisionals=0",
         "NumberOfTremulants=0", "NumberOfSwitches=0", "",
         "[Stop001]", "Name=Prestant 8", "FirstAccessiblePipeLogicalKeyNumber=1",
         f"NumberOfAccessiblePipes={n}", f"NumberOfLogicalPipes={n}",
         f"FirstMidiNoteNumber={eerste}", "WindchestGroup=001", "Percussive=N",
         "HarmonicNumber=8", "AmplitudeLevel=100"]
    for i, f in enumerate(noten, 1):
        L.append(f"Pipe{i:03d}=Samples\\Hoofdwerk\\Prestant_8\\{f}")
    L.append("")

    odf = os.path.join(SCRATCH, "NaamTest.organ")
    open(odf, "w", encoding="utf-8", newline="\r\n").write("\n".join(L))
    open(os.path.join(SCRATCH, "NaamTest.jm-rec.json"), "w", encoding="utf-8").write(
        json.dumps({"jm_rec_version": "3.10", "organ": "NaamTest",
                    "kerk": kerk, "plaats": plaats, "bouwer": bouwer}, ensure_ascii=False))
    doel = os.path.join(SCRATCH, "orgel.jpg")
    if met_foto and os.path.exists(FOTO) and not os.path.exists(doel):
        shutil.copyfile(FOTO, doel)
    return odf


# ---------------------------------------------------------------- 3
if not os.path.isdir(PUTT):
    print("OVERSLAAN | Puttershoek-samples niet gevonden, JM-Rec-naamtest kan niet draaien")
else:
    maak_jmrec_set("Hervormde Kerk", "Bätz-Witte", "Puttershoek")
    post("/load_directory", {"path": SCRATCH})
    info = get("/organ")
    check("JM-Rec-naam 'Kerk - Bouwer - Plaats'",
          info["name"] == "Hervormde Kerk - Bätz-Witte - Puttershoek", f"kreeg '{info['name']}'")

    e = lib_entry("NaamTest.organ")
    check("JM-Rec-set staat in de bibliotheek op het .organ-pad", e is not None)
    if e:
        eerste_id = e["id"]
        check("bibliotheek: naam", e["name"] == "Hervormde Kerk - Bätz-Witte - Puttershoek", e["name"])
        check("bibliotheek: bouwer", e["builder"] == "Bätz-Witte", e["builder"])
        check("bibliotheek: plaats (ChurchAddress, niet RecordingDetails)",
              e["location"] == "Puttershoek", e["location"])
        img = (e.get("image_path") or "").lower()
        check("bibliotheek: orgel.jpg uit de projectmap gevonden",
              img.endswith("orgel.jpg"), e.get("image_path"))
        check("handmatige vlag staat uit", e.get("image_manual") is False)

        # ------------------------------------------------------- 4
        time.sleep(1)
        maak_jmrec_set("", "Bätz-Witte", "Puttershoek")
        post("/load_directory", {"path": SCRATCH})
        info2 = get("/organ")
        e2 = lib_entry("NaamTest.organ")
        check("lege kerknaam -> 'Bouwer - Plaats'",
              info2["name"] == "Bätz-Witte - Puttershoek", f"kreeg '{info2['name']}'")
        check("bestaande entry wordt VERVERST (naam mee)",
              e2 is not None and e2["name"] == "Bätz-Witte - Puttershoek", e2 and e2["name"])
        check("id/pad blijft gelijk (opgeslagen instellingen blijven werken)",
              e2 is not None and e2["id"] == eerste_id, e2 and e2["id"])
        check("aantal bibliotheek-entries niet verdubbeld",
              sum(1 for o in get("/library")["organs"]
                  if o["source_path"].lower().endswith("naamtest.organ")) == 1)

# ---------------------------------------------------------------- 5
echte = [
    (r"C:\Bronbestanden\JM-Orgue\testen\GreenPositiv_GrandOrgue_1_2\GreenPositiv.organ", "background1.png"),
    (r"C:\Bronbestanden\JM-Orgue\testen\Burea_Funeral_Chapel\burea_gravkapell.organ", "background.png"),
    (r"C:\Bronbestanden\JM-Orgue\Sample set homemade\Friesach_GrandOrgue\Friesach.organ", "background.png"),
    (r"C:\Bronbestanden\JM-Orgue\testen\LedzinyStClement.CompPkg.Hauptwerk\OrganDefinitions\Ledziny st Clement.Organ_Hauptwerk_xml", "ph.jpg"),
]
for pad, verwacht in echte:
    if not os.path.exists(pad):
        print(f"OVERSLAAN | {os.path.basename(pad)} niet aanwezig")
        continue
    try:
        post("/load_organ", {"path": pad})
    except Exception as ex:
        check(f"laden {os.path.basename(pad)}", False, str(ex))
        continue
    e = lib_entry(os.path.basename(pad))
    img = (e.get("image_path") or "").lower() if e else ""
    check(f"afbeelding gevonden voor {os.path.basename(pad)} (verwacht *{verwacht})",
          img.endswith(verwacht), e.get("image_path") if e else "geen entry")
    time.sleep(1)

# Tweede keer de bibliotheek opvragen mag geen nieuwe zoektocht kosten:
t0 = time.time()
get("/library")
check("bibliotheek opvragen < 1 s (afbeelding komt uit de entry)", time.time() - t0 < 1.0,
      f"{(time.time() - t0) * 1000:.0f} ms")

print()
print(f"{'ALLES OK' if not fouten else str(len(fouten)) + ' FOUT(EN): ' + '; '.join(fouten)}")
sys.exit(1 if fouten else 0)
