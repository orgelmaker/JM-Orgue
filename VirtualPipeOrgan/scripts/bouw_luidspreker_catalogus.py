#!/usr/bin/env python3
"""Bouwt ui/src/assets/luidsprekers-catalogus.json: de NAMEN van alle luidsprekers
waarvoor het spinorama-project (Pierre Aubert, GPL-3; metingen van ASR, Erin's
Audio Corner, fabrikanten e.a.) een parametrische correctie heeft
(datas/eq/<Merk Model>/iir-autoeq.txt). De correctie zelf wordt NIET gebundeld:
de app haalt hem per model op het moment van kiezen op (commands::fetch_speaker_eq),
vastgepind op de commit in deze catalogus. Handmatig draaien bij een release.
Gebruik: python scripts/bouw_luidspreker_catalogus.py  (vanuit VirtualPipeOrgan;
vereist `gh`, ingelogd).
"""
import datetime, io, json, os, subprocess, sys

REPO = "pierreaubert/spinorama"
UIT = os.path.join(os.path.dirname(__file__), "..", "ui", "src", "assets", "luidsprekers-catalogus.json")
MEERWOORD = ("Acoustic Energy", "Adam Audio", "Ascend Acoustics", "Audio Physic", "Bowers & Wilkins", "Buchardt Audio",
             "Dali", "Dutch & Dutch", "Elac", "Focal", "Genelec", "Harbeth", "JBL", "Kali", "KEF", "Klipsch", "Monitor Audio",
             "Neumann", "Polk Audio", "PSB", "Q Acoustics", "Revel", "Sonus Faber", "Wharfedale", "Yamaha",
             "Ascend", "Emotiva", "Dynaudio", "Infinity", "Micca", "Philharmonic Audio", "RSL", "Selah Audio",
             "Sony", "Tannoy", "Triangle", "Verdant Audio", "Zu Audio", "Eve Audio", "IK Multimedia", "Fluid Audio",
             "Mackie", "PreSonus", "M-Audio", "Behringer", "Pioneer DJ", "Edifier", "Focal Shape", "HEDD Audio",
             "Amphion", "Barefoot Sound", "Unity Audio", "Bang & Olufsen", "Devialet", "Vanatoo", "SVS", "Hsu Research")

def gh_json(route):
    uit = subprocess.run(["gh", "api", route], capture_output=True, text=True, encoding="utf-8")
    if uit.returncode != 0:
        print(uit.stderr, file=sys.stderr); sys.exit(1)
    return json.loads(uit.stdout)

def merk_van(naam):
    for m in sorted(MEERWOORD, key=len, reverse=True):
        if naam.lower().startswith(m.lower() + " "):
            return naam[:len(m)]
    return naam.split(" ")[0]

def main():
    commit = gh_json(f"repos/{REPO}/commits/master")["sha"]
    datas = gh_json(f"repos/{REPO}/contents/datas?ref={commit}")
    eq_sha = next(e["sha"] for e in datas if e["name"] == "eq" and e["type"] == "dir")
    boom = gh_json(f"repos/{REPO}/git/trees/{eq_sha}")
    if boom.get("truncated"):
        print("tree afgekapt!", file=sys.stderr); sys.exit(1)
    mappen = sorted(e["path"] for e in boom["tree"] if e["type"] == "tree")
    items = [{"id": "spinorama/" + m, "merk": merk_van(m), "model": m, "pad": m} for m in mappen]
    items.sort(key=lambda p: (p["merk"].lower(), p["model"].lower()))
    uit = {
        "bron": "spinorama", "url": f"https://github.com/{REPO}", "site": "https://www.spinorama.org",
        "commit": commit, "gegenereerd": datetime.date.today().isoformat(),
        "bestand": "datas/eq/{pad}/iir-autoeq.txt",
        "toelichting": "Alleen de namen zijn gebundeld; de correctie wordt per model opgehaald van de vastgepinde commit.",
        "items": items,
    }
    os.makedirs(os.path.dirname(UIT), exist_ok=True)
    with io.open(UIT, "w", encoding="utf-8", newline="\n") as f:
        json.dump(uit, f, ensure_ascii=False, separators=(",", ":")); f.write("\n")
    print(f"geschreven {os.path.normpath(UIT)}: {len(items)} luidsprekers, {len(set(i['merk'] for i in items))} merken, commit {commit[:10]}")
    for naam in ("Adam A7V", "Adam T7V", "Genelec 8030C", "Neumann KH 120 II"):
        print(("OK   " if any(i["model"] == naam for i in items) else "MIST ") + naam)

if __name__ == "__main__":
    main()
