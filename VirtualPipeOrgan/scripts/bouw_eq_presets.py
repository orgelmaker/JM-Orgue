#!/usr/bin/env python3
"""Bouwt ui/src/assets/eq-presets.json uit het AutoEq-project (MIT, © Jaakko
Pasanen): alle over-ear-modellen gemeten door oratory1990, als parametrische
EQ (Equalizer APO-syntaxis). Handmatig draaien bij een release; AutoEq wijzigt
zelden. Gebruik: python scripts/bouw_eq_presets.py  (vanuit VirtualPipeOrgan)

Vereist: `gh` (ingelogd) voor de mappenlijst; de bestanden zelf komen van
raw.githubusercontent.com.
"""
import concurrent.futures, datetime, io, json, os, re, subprocess, sys, urllib.parse, urllib.request

REPO = "jaakkopasanen/AutoEq"
PAD = "results/oratory1990/over-ear"
UIT = os.path.join(os.path.dirname(__file__), "..", "ui", "src", "assets", "eq-presets.json")
LICENTIE_UIT = os.path.join(os.path.dirname(__file__), "..", "docs", "LICENTIES_DERDEN.md")

# Merken van meer dan één woord (mapnaam begint ermee).
def gh_json(route):
    # Zonder --paginate: de contents-API geeft een map in één antwoord (tot 1000
    # items) en bij een enkel object plakt --paginate meerdere JSON's aaneen.
    uit = subprocess.run(["gh", "api", route], capture_output=True, text=True, encoding="utf-8")
    if uit.returncode != 0:
        print(uit.stderr, file=sys.stderr); sys.exit(1)
    return json.loads(uit.stdout)

def merk_van(naam):
    for m in ("Austrian Audio", "Dan Clark Audio", "Master & Dynamic", "Monolith by Monoprice", "Bang & Olufsen",
              "Bowers & Wilkins", "Cooler Master", "Gold Planar", "House of Marley", "Mark Levinson", "Steven Slate Audio",
              "Allen & Heath", "Adam Audio", "Audio Zenith", "Final Audio", "Meze Audio", "Massdrop x", "Drop +"):
        if naam.startswith(m + " "):
            return m
    return naam.split(" ")[0]

RE_FILTER = re.compile(r"^Filter\s*\d+\s*:\s*(ON|OFF)\s+(PK|LSC|HSC)\s+Fc\s+([\d.]+)\s*Hz\s+Gain\s+(-?[\d.]+)\s*dB(?:\s+Q\s+([\d.]+))?", re.I)
RE_PREAMP = re.compile(r"^Preamp:\s*(-?[\d.]+)\s*dB", re.I)

def parse(tekst):
    preamp = 0.0; banden = []
    for regel in tekst.splitlines():
        m = RE_PREAMP.match(regel.strip())
        if m:
            preamp = float(m.group(1)); continue
        m = RE_FILTER.match(regel.strip())
        if m and m.group(1).upper() == "ON":
            soort = {"PK": "peak", "LSC": "lowshelf", "HSC": "highshelf"}[m.group(2).upper()]
            q = float(m.group(5)) if m.group(5) else 0.7071
            banden.append([soort, float(m.group(3)), float(m.group(4)), q])
    return preamp, banden

def haal(naam, sha):
    url = f"https://raw.githubusercontent.com/{REPO}/{sha}/{PAD}/" + urllib.parse.quote(f"{naam}/{naam} ParametricEQ.txt")
    try:
        with urllib.request.urlopen(url, timeout=30) as r:
            return naam, r.read().decode("utf-8")
    except Exception as e:
        return naam, None

def main():
    sha = gh_json(f"repos/{REPO}/commits/master")["sha"]
    mappen = [e["name"] for e in gh_json(f"repos/{REPO}/contents/{PAD}") if e["type"] == "dir"]
    print(f"commit {sha[:10]}, {len(mappen)} modellen")
    licentie = urllib.request.urlopen(f"https://raw.githubusercontent.com/{REPO}/{sha}/LICENSE", timeout=30).read().decode("utf-8")
    presets = []; mislukt = []
    with concurrent.futures.ThreadPoolExecutor(max_workers=8) as pool:
        for naam, tekst in pool.map(lambda n: haal(n, sha), mappen):
            if tekst is None:
                mislukt.append(naam); continue
            preamp, banden = parse(tekst)
            if not banden:
                mislukt.append(naam + " (geen filters)"); continue
            presets.append({"id": "autoeq/oratory1990/over-ear/" + naam, "merk": merk_van(naam), "model": naam,
                            "preamp": preamp, "banden": banden})
    presets.sort(key=lambda p: (p["merk"].lower(), p["model"].lower()))
    uit = {
        "bron": "AutoEq", "url": f"https://github.com/{REPO}", "meter": "oratory1990", "vormfactor": "over-ear",
        "commit": sha, "gegenereerd": datetime.date.today().isoformat(),
        "licentie": licentie.strip(),
        "presets": presets,
    }
    os.makedirs(os.path.dirname(UIT), exist_ok=True)
    with io.open(UIT, "w", encoding="utf-8", newline="\n") as f:
        json.dump(uit, f, ensure_ascii=False, separators=(",", ":"))
        f.write("\n")
    print(f"geschreven {os.path.normpath(UIT)}: {len(presets)} presets, {os.path.getsize(UIT)//1024} kB; mislukt: {mislukt}")
    for naam in ("AKG K240 Studio", "AKG K240 MKII", "Beyerdynamic DT 770 Pro", "Sennheiser HD 600"):
        aanwezig = any(p["model"] == naam for p in presets)
        print(("OK   " if aanwezig else "MIST ") + naam)
    # Licentietekst voor docs/LICENTIES_DERDEN.md
    os.makedirs(os.path.dirname(LICENTIE_UIT), exist_ok=True)
    kop = ("# Licenties van derden\n\n## Hoofdtelefoonpresets: AutoEq\n\nDe ingebouwde hoofdtelefoonpresets "
           "(`ui/src/assets/eq-presets.json`) zijn afgeleid van het AutoEq-project van Jaakko Pasanen "
           f"(https://github.com/{REPO}, commit `{sha}`), metingen door oratory1990. AutoEq is uitgebracht onder de "
           "MIT-licentie:\n\n```\n" + licentie.strip() + "\n```\n")
    with io.open(LICENTIE_UIT, "w", encoding="utf-8", newline="\n") as f:
        f.write(kop)
    print("geschreven", os.path.normpath(LICENTIE_UIT))

if __name__ == "__main__":
    main()
