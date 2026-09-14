# Maakt een minimale GrandOrgue-ODF (rank-referentievorm) met Puttershoek-samples:
#  - Stop001 "Gestapeld": 2 ranks (twee registers) → per toets 2 pijpen (test gestapelde ranks)
#  - Stop002 "Enkel": 1 rank
#  - Rank002 krijgt PitchCorrection=+30 cent (gemeten afwijking) op rankniveau,
#    en Rank001 per pijp PitchCorrection=-20 (test hertemperen)
# Uitvoer: %TEMP%\jm-orgue-testodf\TestRanks.organ (of de map die je als argument geeft).
# De sample-paden in de ODF zijn relatief t.o.v. de junction 'Samples' die hier
# naast wordt aangemaakt; de testmap ligt bewust BUITEN de repo.
import os, re, sys, tempfile
SRC = r"C:\Bronbestanden\JM-Orgue\Sample set homemade\Bätz-Witte Puttershoek"
OUT = sys.argv[1] if len(sys.argv) > 1 else os.path.join(tempfile.gettempdir(), "jm-orgue-testodf")
os.makedirs(OUT, exist_ok=True)

def notes_in(reg_dir):
    m = {}
    for f in os.listdir(reg_dir):
        if f.lower().endswith((".wav", ".mp3")) and f[:3].isdigit():
            m[int(f[:3])] = f
    return m

hw = os.path.join(SRC, "Hoofdwerk")
regs = sorted(d for d in os.listdir(hw) if os.path.isdir(os.path.join(hw, d)))
cand = [(r, notes_in(os.path.join(hw, r))) for r in regs]
cand = [(r, n) for r, n in cand if len(n) >= 20]
# Twee ACHTVOETS registers kiezen, geen 16'- of cornetrank. Een 16' klinkt een
# octaaf onder de nominale toon en een cornet heeft ingebouwde tertsen/kwinten:
# de sterkste FFT-piek zegt dan niet meer welke toonhoogte de pijp heeft, en
# juist daarop meet test_hertemperen.py.
acht = [(r, n) for r, n in cand if r.endswith("_8")]
if len(acht) >= 2:
    (r1, n1), (r2, n2) = acht[0], acht[1]
else:
    r1, n1 = cand[0]; r2, n2 = cand[1]
common = sorted(set(n1) & set(n2))
first, last = common[0], common[-1]
print(f"ranks: {r1} ({len(n1)} noten), {r2} ({len(n2)} noten), gemeenschappelijk {first}-{last} ({len(common)})")

rel = lambda reg, fn: "..\\..\\..\\..\\..\\..\\..\\..\\Bronbestanden\\JM-Orgue\\Sample set homemade\\Bätz-Witte Puttershoek\\Hoofdwerk\\" + reg + "\\" + fn
# eenvoudiger: absolute paden zijn in GO-ODF's toegestaan? Niet officieel; JM-Orgue's parser joint relatief t.o.v. de ODF-map.
# Daarom: een symlink/junction naar de Puttershoek-map naast de ODF.
link = os.path.join(OUT, "Samples").replace("/", "\\")
if not os.path.exists(link):
    import subprocess
    subprocess.run(["cmd", "/c", "mklink", "/J", link, SRC], check=True, capture_output=True)
rel = lambda reg, fn: "Samples\\Hoofdwerk\\" + reg + "\\" + fn

n = len(common)
L = ["[Organ]", "ChurchName=TestRanks", "ChurchAddress=Scratch", "OrganBuilder=JM-Orgue test", "NumberOfManuals=1", "HasPedals=N",
     "NumberOfEnclosures=0", "NumberOfTremulants=0", "NumberOfWindchestGroups=1", "NumberOfRanks=5", "",
     "[WindchestGroup001]", "Name=Main", "NumberOfEnclosures=0", "NumberOfTremulants=0", "",
     "[Manual001]", "Name=Hoofdwerk", "MIDIInputNumber=1", f"NumberOfLogicalKeys={n}", f"NumberOfAccessibleKeys={n}",
     f"FirstAccessibleKeyMIDINoteNumber={first}", "NumberOfStops=4",
     "Stop001=001", "Stop002=002", "Stop003=003", "Stop004=004",
     "NumberOfCouplers=0", "NumberOfDivisionals=0", "NumberOfTremulants=0", "NumberOfSwitches=0", "",
     "[Stop001]", "Name=Gestapeld", "NumberOfRanks=2", "Rank001=001", "Rank002=002",
     "FirstAccessiblePipeLogicalKeyNumber=1", f"NumberOfAccessiblePipes={n}", "",
     "[Stop002]", "Name=Enkel", "NumberOfRanks=1", "Rank001=003",
     "FirstAccessiblePipeLogicalKeyNumber=1", f"NumberOfAccessiblePipes={n}", "",
     # Twee ENKELVOUDIGE registers met elk één soort toonhoogtecorrectie: alleen
     # daar is een toonhoogtemeting eenduidig (bij het gestapelde register
     # klinken twee pijpen tegelijk en zegt de sterkste piek niets over één rank).
     "[Stop003]", "Name=Enkel pijp-PC", "NumberOfRanks=1", "Rank001=004",
     "FirstAccessiblePipeLogicalKeyNumber=1", f"NumberOfAccessiblePipes={n}", "",
     "[Stop004]", "Name=Enkel rank-PC", "NumberOfRanks=1", "Rank001=005",
     "FirstAccessiblePipeLogicalKeyNumber=1", f"NumberOfAccessiblePipes={n}", ""]
def harmonic_for(reg):
    m = re.search(r"_(\d+)(?:st)?$", reg)
    ft = int(m.group(1)) if m else 8
    return max(1, round(64 / ft))  # 16'→4, 8'→8, 4'→16, 2'→32
def rank(idx, name, reg, notes, rank_pc=None, pipe_pc=None):
    L.extend([f"[Rank{idx:03d}]", f"Name={name}", "WindchestGroup=001", f"FirstMidiNoteNumber={first}",
              f"NumberOfLogicalPipes={n}", "Percussive=N", f"HarmonicNumber={harmonic_for(reg)}", "AmplitudeLevel=100"])
    if rank_pc is not None: L.append(f"PitchCorrection={rank_pc}")
    for i, note in enumerate(common, 1):
        L.append(f"Pipe{i:03d}={rel(reg, notes[note])}")
        if pipe_pc is not None: L.append(f"Pipe{i:03d}PitchCorrection={pipe_pc}")
    L.append("")
rank(1, r1, r1, n1, pipe_pc=-20)
rank(2, r2, r2, n2, rank_pc=30)
rank(3, r1 + " solo", r1, n1)
rank(4, r1 + " pijp-PC", r1, n1, pipe_pc=-20)
rank(5, r2 + " rank-PC", r2, n2, rank_pc=30)
odf = os.path.join(OUT, "TestRanks.organ")
open(odf, "w", encoding="utf-8", newline="\r\n").write("\n".join(L))
print("ODF:", odf, "| ranks:", r1, "+", r2, "| noten", first, "-", last)
