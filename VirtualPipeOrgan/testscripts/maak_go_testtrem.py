# Maakt twee GrandOrgue-test-ODF's met een GOLFVORM-tremulant (TremulantType=Wave)
# op de Puttershoek-samples (junction "Samples" in de uitvoermap):
#
#   TestTrem.organ    tremulant op [WindchestGroup001] én [Manual001]
#   TestTremWc.organ  tremulant ALLEEN op [WindchestGroup001] (GrandOrgue past de
#                     tremulant via de windlade toe; de manual-verwijzing is voor
#                     de divisionals) — test of de divisie dan tóch een
#                     tremulantregister krijgt.
#
# Stop001 "Prestant" heeft per pijp vier verschillende toonhoogtes, zodat een
# FFT-piek (POST /record/stop → peak_hz) ondubbelzinnig zegt WELKE opname klinkt:
#
#   attack  droog   IsTremulant=0   Prestant_8 (8')      noot n      →   f
#   attack  trem    IsTremulant=1   Fluit_4    (4')      noot n      →  2f
#   release droog   IsTremulant=0   Prestant_8           noot n+4    → 1,26f
#   release trem    IsTremulant=1   Prestant_8           noot n+7    → 1,5f
#
# Stop002 "Enkel" heeft alleen droge samples (geen attacks, geen releases) en moet
# dus ongewijzigd blijven klinken — de regressiecontrole.
#
# Gebruik:  python maak_go_testtrem.py [uitvoermap]
# Standaard uitvoermap: %TEMP%\jm-orgue-testodf (bewust NIET in de repo).
import os, subprocess, sys, tempfile

SRC = r"C:\Bronbestanden\JM-Orgue\Sample set homemade\Bätz-Witte Puttershoek"
OUT = sys.argv[1] if len(sys.argv) > 1 else os.path.join(tempfile.gettempdir(), "jm-orgue-testodf")
# mklink wil Windows-padscheidingstekens (een pad met / geeft "verkeerde syntaxis").
OUT = os.path.normpath(OUT).replace("/", "\\")
os.makedirs(OUT, exist_ok=True)

link = OUT + "\\Samples"  # expliciet: onder MSYS-Python is os.sep "/"
if not os.path.exists(link):
    r = subprocess.run(["cmd", "/c", "mklink", "/J", link, SRC], capture_output=True, text=True)
    if r.returncode != 0 or not os.path.exists(link):
        sys.exit(f"junction maken mislukt ({link} -> {SRC}): {r.stdout.strip()} {r.stderr.strip()}")


def notes_in(reg):
    d = os.path.join(SRC, "Hoofdwerk", reg)
    return {int(f[:3]): f for f in os.listdir(d)
            if f.lower().endswith((".wav", ".mp3")) and f[:3].isdigit()}


dry, trem = notes_in("Prestant_8"), notes_in("Fluit_4")
common = sorted(set(dry) & set(trem))
first, n = common[0], len(common)
lo, hi = min(dry), max(dry)


def rel(reg, fn):
    return "Samples\\Hoofdwerk\\" + reg + "\\" + fn


def wav_note(note):
    """Prestant_8-bestand van `note`, geklemd op het bereik van het register.
    Releases moeten WAV zijn: de release-loader leest geen mp3."""
    return dry[min(max(note, lo), hi)]


def stop_block(L, idx, name, with_trem):
    L.extend([f"[Stop{idx:03d}]", f"Name={name}", "NumberOfRanks=0", "WindchestGroup=001",
              "FirstAccessiblePipeLogicalKeyNumber=1", f"NumberOfAccessiblePipes={n}",
              f"NumberOfLogicalPipes={n}", f"FirstMidiNoteNumber={first}",
              "Percussive=N", "HarmonicNumber=8", "AmplitudeLevel=100"])
    for i, note in enumerate(common, 1):
        L.append(f"Pipe{i:03d}={rel('Prestant_8', dry[note])}")
        if not with_trem:
            continue
        # Droge attack + tremulant-attack (kwartoon hoger register: 4' = 2f).
        L.append(f"Pipe{i:03d}IsTremulant=0")
        L.append(f"Pipe{i:03d}AttackCount=1")
        L.append(f"Pipe{i:03d}Attack001={rel('Fluit_4', trem[note])}")
        L.append(f"Pipe{i:03d}Attack001IsTremulant=1")
        # Twee releases: één voor de droge stand (octaaf lager) en één voor de
        # tremulantstand (kwint hoger). Beide MaxKeyPressTime=-1 (default).
        L.append(f"Pipe{i:03d}ReleaseCount=2")
        L.append(f"Pipe{i:03d}Release001={rel('Prestant_8', wav_note(note + 4))}")
        L.append(f"Pipe{i:03d}Release001IsTremulant=0")
        L.append(f"Pipe{i:03d}Release001MaxKeyPressTime=-1")
        L.append(f"Pipe{i:03d}Release002={rel('Prestant_8', wav_note(note + 7))}")
        L.append(f"Pipe{i:03d}Release002IsTremulant=1")
        L.append(f"Pipe{i:03d}Release002MaxKeyPressTime=-1")
    L.append("")


def build(church, manual_trem):
    L = ["[Organ]", f"ChurchName={church}", "ChurchAddress=Scratch",
         "OrganBuilder=JM-Orgue test", "NumberOfManuals=1", "HasPedals=N",
         "NumberOfEnclosures=0", "NumberOfTremulants=1", "NumberOfWindchestGroups=1",
         "NumberOfRanks=0", "",
         "[Tremulant001]", "Name=Tremulant", "TremulantType=Wave", "",
         "[WindchestGroup001]", "Name=Main", "NumberOfEnclosures=0",
         "NumberOfTremulants=1", "Tremulant001=001", "",
         "[Manual001]", "Name=Hoofdwerk", "MIDIInputNumber=1",
         f"NumberOfLogicalKeys={n}", f"NumberOfAccessibleKeys={n}",
         f"FirstAccessibleKeyMIDINoteNumber={first}", "NumberOfStops=2",
         "Stop001=001", "Stop002=002", "NumberOfCouplers=0", "NumberOfDivisionals=0"]
    if manual_trem:
        L.extend(["NumberOfTremulants=1", "Tremulant001=001"])
    else:
        L.append("NumberOfTremulants=0")
    L.extend(["NumberOfSwitches=0", ""])
    stop_block(L, 1, "Prestant", True)
    stop_block(L, 2, "Enkel", False)
    return "\n".join(L)


for fname, manual_trem in (("TestTrem.organ", True), ("TestTremWc.organ", False)):
    p = os.path.join(OUT, fname)
    with open(p, "w", encoding="utf-8", newline="\r\n") as f:
        f.write(build(os.path.splitext(fname)[0], manual_trem))
    print("ODF:", p)

print(f"noten {first}-{common[-1]} ({n}); Prestant_8-bereik {lo}-{hi}")
