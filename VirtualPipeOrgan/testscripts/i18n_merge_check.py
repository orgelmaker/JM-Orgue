# 1) Sleutels uit scratchpad/i18n_keys_*.json samenvoegen in de vier locales
#    (bestaande sleutels niet overschrijven, tenzij --overschrijf).
# 2) Dekkingscontrole: elke $t('…')/tx('…')-sleutel in ui/src moet in alle
#    vier locales bestaan; placeholders {x} moeten per taal gelijk zijn.
import os, re, json, glob, sys, collections
ROOT = r"C:\Bronbestanden\JM-Orgue\VirtualPipeOrgan\ui\src"
SP = r"C:\Users\orgel\AppData\Local\Temp\claude\C--Bronbestanden-JM-Orgue\e7aa5637-5756-42e5-b452-fede78474ae9\scratchpad"
LANGS = ["nl", "en", "fr", "de"]
overschrijf = "--overschrijf" in sys.argv

def flat(d, prefix=""):
    out = {}
    for k, v in d.items():
        key = f"{prefix}.{k}" if prefix else k
        if isinstance(v, dict): out.update(flat(v, key))
        else: out[key] = v
    return out

locales = {}
for lang in LANGS:
    p = os.path.join(ROOT, "lib", "locales", f"{lang}.json")
    locales[lang] = json.load(open(p, encoding="utf-8"), object_pairs_hook=collections.OrderedDict)

# ── 1. samenvoegen ──
added = collections.Counter(); skipped = collections.Counter()
for f in sorted(glob.glob(os.path.join(SP, "i18n_keys_*.json"))):
    try:
        data = json.load(open(f, encoding="utf-8"))
    except Exception as e:
        print(f"!! {os.path.basename(f)}: onleesbaar ({e})"); continue
    for lang in LANGS:
        src = data.get(lang) or {}
        for group, keys in src.items():
            if not isinstance(keys, dict): continue
            dst = locales[lang].setdefault(group, collections.OrderedDict())
            for k, v in keys.items():
                if k in dst and not overschrijf:
                    if dst[k] != v: skipped[lang] += 1
                    continue
                dst[k] = v; added[lang] += 1
    print(f"{os.path.basename(f)}: verwerkt")
for lang in LANGS:
    p = os.path.join(ROOT, "lib", "locales", f"{lang}.json")
    with open(p, "w", encoding="utf-8", newline="\n") as fh:
        json.dump(locales[lang], fh, ensure_ascii=False, indent=2); fh.write("\n")
    print(f"{lang}: +{added[lang]} sleutels (bestaand behouden: {skipped[lang]}), totaal {len(flat(locales[lang]))}")

# ── 2. dekking ──
flats = {lang: flat(locales[lang]) for lang in LANGS}
key_re = re.compile(r"""(?:\$t|\btx|\bt)\(\s*['"]([a-zA-Z0-9_.]+)['"]""")
used = collections.defaultdict(set)
for f in glob.glob(os.path.join(ROOT, "**", "*.svelte"), recursive=True) + glob.glob(os.path.join(ROOT, "**", "*.js"), recursive=True):
    txt = open(f, encoding="utf-8").read()
    for m in key_re.finditer(txt):
        used[m.group(1)].add(os.path.relpath(f, ROOT))
missing = collections.defaultdict(list)
for key, files in sorted(used.items()):
    for lang in LANGS:
        if key not in flats[lang]:
            missing[lang].append((key, sorted(files)[0]))
print(f"\nGebruikte sleutels in code: {len(used)}")
for lang in LANGS:
    print(f"  ontbreekt in {lang}: {len(missing[lang])}")
    for key, f in missing[lang][:40]:
        print(f"    - {key}  ({f})")
# sleutelsets onderling
union = set().union(*[set(fl) for fl in flats.values()])
for lang in LANGS:
    diff = sorted(union - set(flats[lang]))
    if diff: print(f"  {lang} mist t.o.v. unie: {len(diff)} → {diff[:15]}")
# placeholders
ph = re.compile(r"\{[a-zA-Z_]+\}")
for key in sorted(union):
    sets = {lang: set(ph.findall(str(flats[lang].get(key, "")))) for lang in LANGS if key in flats[lang]}
    if len({frozenset(v) for v in sets.values()}) > 1:
        print(f"  placeholder-verschil {key}: {sets}")
