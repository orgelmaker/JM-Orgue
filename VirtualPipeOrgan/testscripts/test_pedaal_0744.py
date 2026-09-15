# -*- coding: utf-8 -*-
"""Pedaalgedrag 0.7.44 via de test-API (vpo-app.exe --test-api, orgel geladen).

Controleert de reparaties uit het zwel/crescendo-onderzoek van 2026-09-15:
  F1  een zwelkoppeling die verdrongen (crescendo op dezelfde trede) of gewist
      wordt zet haar divisie weer OPEN (gain 1.0) in plaats van hem op de laatste
      pedaalstand te laten staan;
  F2  crescendo-koppeling wissen zet eerst trap 0 (trede-registers weg);
  F3  een crescendo-koppeling op een andere (kanaal, CC) erft geen oud bereik;
  F4  spiegelbeeld/bereik van de zwel is meteen hoorbaar;
  F7  dode zone van het crescendo is 8 (trede met speling houdt trap 1 niet vast);
  F8  crescendo-claims overleven een audiowissel (herlaad) en veren daarna terug;
  D   MIDI-CC-diagnoseregels in het log.
Gebruik: python testscripts/test_pedaal_0744.py
"""
import json, sys, time, urllib.request, urllib.error
B = "http://127.0.0.1:8765"
R = {"pass": [], "fail": []}
def ok(m):   print("  PASS", m); R["pass"].append(m)
def fail(m): print("  FAIL", m); R["fail"].append(m)
def check(c, m): (ok if c else fail)(m)
def post(p, b=None, t=120):
    d = json.dumps(b).encode() if b is not None else b"{}"
    r = urllib.request.Request(B + p, data=d, method="POST", headers={"Content-Type": "application/json"})
    return json.loads(urllib.request.urlopen(r, timeout=t).read())
def get(p, t=30):
    return json.loads(urllib.request.urlopen(B + p, timeout=t).read())
def cc(ch, num, val):
    post(f"/midi/inject?cc={num}&value={val}&channel={ch}"); time.sleep(0.2)
def gain(idx): return float(get("/division_volumes")["gains"][idx])
def swell(name):
    for s in get("/swell"):
        if s["division"] == name: return s
    return None
def cresc(): return get("/crescendo")
def drawn(): return set(get("/state")["drawn"])
def logs(n=600): return get(f"/logs?lines={n}").get("lines", [])

st = get("/status")
if not st.get("organ_loaded"): sys.exit("geen orgel geladen")
organ = get("/organ")
divs = organ["divisions"]
if len(divs) < 2: sys.exit("orgel met minstens twee divisies nodig")
SW = divs[1]["name"]; SWI = 1                    # zweldivisie (bv. Nevenwerk)
hw = [s["id"] for s in divs[0]["stops"]][:3]     # drie registers van de eerste divisie voor de trappen
if len(hw) < 3: sys.exit("eerste divisie heeft minder dan drie registers")
print(f"orgel: {organ.get('name')} | zweldivisie: {SW} | trapregisters: {hw}")

# Oorspronkelijke instellingen onthouden (worden aan het eind teruggezet + opgeslagen).
orig_cresc = cresc()
orig_swell = get("/swell")

# Schone uitgangstoestand.
for sid in get("/stops/drawn")["drawn_stops"]: post(f"/stops/{sid}/toggle")
post("/crescendo/binding", {"clear": True})
for d in divs: post("/swell/binding", {"division": d["name"], "clear": True})
post("/crescendo/config", {"stages": [[hw[0]], [hw[0], hw[1]], [hw[0], hw[1], hw[2]]], "enabled": True})
try: post("/settings/save")   # anders zet de herlaad (F8) de opgeslagen crescendo-instellingen van het orgel terug
except Exception as e: print("   (settings/save niet beschikbaar:", e, ")")
time.sleep(0.4)

print("\nF1 — verdrongen zwelkoppeling zet de kast open")
post("/swell/binding", {"division": SW, "channel": 1, "cc": 11, "min": 0, "max": 127})
cc(1, 11, 0)
check(gain(SWI) <= 0.01, f"zwel {SW} dicht na CC11=0 (gain {gain(SWI):.2f})")
r = post("/crescendo/binding", {"channel": 1, "cc": 11})
check(any(d.get("kind") == "swell" for d in r.get("displaced", [])), "crescendo op dezelfde trede verdringt de zwelkoppeling")
time.sleep(0.3)
check(swell(SW)["binding"] is None, "zwelkoppeling weg")
check(gain(SWI) >= 0.99, f"kast weer OPEN na verdringing (gain {gain(SWI):.2f})")

print("\nF1 — gewiste zwelkoppeling zet de kast open")
post("/swell/binding", {"division": SW, "channel": 1, "cc": 12, "min": 0, "max": 127})
cc(1, 12, 0)
check(gain(SWI) <= 0.01, f"zwel dicht na CC12=0 (gain {gain(SWI):.2f})")
post("/swell/binding", {"division": SW, "clear": True}); time.sleep(0.4)
check(gain(SWI) >= 0.99, f"kast weer OPEN na wissen (gain {gain(SWI):.2f})")

print("\nF4 — spiegelbeeld meteen hoorbaar")
post("/swell/binding", {"division": SW, "channel": 1, "cc": 12, "min": 0, "max": 127, "invert": False})
cc(1, 12, 100)
g1 = gain(SWI)
check(abs(g1 - 100 / 127) < 0.02, f"gain volgt CC12=100 ({g1:.3f})")
post("/swell/binding", {"division": SW, "channel": 1, "cc": 12, "invert": True}); time.sleep(0.2)
g2 = gain(SWI)
check(abs(g2 - (1 - 100 / 127)) < 0.02, f"na spiegelbeeld meteen {g2:.3f} zonder pedaalbeweging")
post("/swell/binding", {"division": SW, "clear": True}); time.sleep(0.3)

print("\nF3 — andere trede erft geen bereik/spiegel")
post("/crescendo/binding", {"channel": 0, "cc": 20, "min": 20, "max": 100, "invert": True})
b = cresc()["binding"]
check(b and b["min"] == 20 and b["max"] == 100 and b["invert"] is True, f"eerste koppeling met eigen bereik: {b}")
post("/crescendo/binding", {"channel": 0, "cc": 21})
b = cresc()["binding"]
check(b and b["cc"] == 21 and b["min"] == 0 and b["max"] == 127 and b["invert"] is False, f"nieuwe trede begint op 0..127 zonder spiegel: {b}")
post("/crescendo/binding", {"channel": 0, "cc": 21, "min": 10, "max": 110})
b = cresc()["binding"]
check(b and b["min"] == 10 and b["max"] == 110, f"zelfde trede houdt/neemt bereik: {b}")

print("\nF7 — dode zone 8")
post("/crescendo/binding", {"channel": 0, "cc": 20, "min": 0, "max": 127, "invert": False})
cc(0, 20, 0)
cc(0, 20, 5);  check(cresc()["stage"] == 0, f"CC=5 blijft trap 0 (nu {cresc()['stage']})")
cc(0, 20, 8);  check(cresc()["stage"] == 0, f"CC=8 blijft trap 0 door de hysterese vanaf 0 (nu {cresc()['stage']})")
cc(0, 20, 10); check(cresc()["stage"] == 1, f"CC=10 geeft trap 1 (nu {cresc()['stage']})")
cc(0, 20, 127); check(cresc()["stage"] == 3, f"CC=127 geeft trap 3 (nu {cresc()['stage']})")
cc(0, 20, 0);  check(cresc()["stage"] == 0, f"CC=0 terug naar trap 0 (nu {cresc()['stage']})")
check(not (set(hw) & drawn()), "trede-registers weg op trap 0")

print("\nF2 — crescendo-koppeling wissen zet eerst trap 0")
cc(0, 20, 127)
check(cresc()["stage"] == 3 and set(hw) <= drawn(), "trap 3 actief met drie trede-registers")
post("/crescendo/binding", {"clear": True}); time.sleep(0.2)
c = cresc()
check(c["binding"] is None and c["stage"] == 0 and not c["active_stops"], f"koppeling weg, trap 0, geen claims ({c['stage']}, {c['active_stops']})")
check(not (set(hw) & drawn()), "trede-registers weggetrokken bij het wissen")

print("\nF8 — claims overleven een audiowissel")
post("/crescendo/binding", {"channel": 0, "cc": 20, "min": 0, "max": 127, "invert": False})
try: post("/settings/save")
except Exception: pass
post(f"/stops/{hw[2]}/toggle"); time.sleep(0.1)          # hw[2] handmatig getrokken
cc(0, 20, 64)
c = cresc()
check(c["stage"] == 2 and set(c["active_stops"]) == {hw[0], hw[1]}, f"trap 2: claims {c['active_stops']}, handmatig {hw[2]}")
st0 = get("/status")
r = post("/audio_output", {"host": st0["audio_host"], "device": st0["audio_device"], "buffer_frames": 256}, t=180)
check(r.get("ok") is True and r.get("player_rebuilt") is True, f"audiowissel met herlaad: {str(r)[:100]}")
time.sleep(1.0)
c = cresc()
check(c["stage"] == 2 and set(c["active_stops"]) == {hw[0], hw[1]}, f"na herlaad: trap {c['stage']}, claims {c['active_stops']}")
check(set(hw) <= drawn(), "alle drie registers getrokken na herlaad")
cc(0, 20, 0)
d = drawn()
check(hw[0] not in d and hw[1] not in d and hw[2] in d, f"terugveren: trede-registers weg, handmatige blijft ({sorted(d & set(hw))})")
post("/audio_output", {"host": st0["audio_host"], "device": st0["audio_device"], "buffer_frames": st0.get("buffer_frames") or 512}, t=180)
time.sleep(0.5)

print("\nD — MIDI-CC-diagnose in het log")
post("/swell/binding", {"division": SW, "channel": 1, "cc": 12, "min": 0, "max": 127})
cc(1, 12, 64); cc(0, 20, 64); cc(5, 77, 3)
L = logs()
check(any("MIDI-CC kanaal 2 CC12 = 64" in l and "zwelkast" in l for l in L), "zwel-CC gelogd met divisie en percentage")
check(any("MIDI-CC kanaal 1 CC20 = 64" in l and "generaal crescendo" in l for l in L), "crescendo-CC gelogd")
check(any("MIDI-CC kanaal 6 CC77 = 3" in l and "geen zwel-/crescendokoppeling" in l for l in L), "ongebonden CC gelogd als 'geen koppeling'")

print("\nB2 — na General Cancel begint het crescendo opnieuw vanaf de bodem")
cc(0, 20, 127)
check(cresc()["stage"] == 3 and set(hw) <= drawn(), "trap 3 met drie trede-registers")
post("/stops/set", {"ids": []})                      # General Cancel: alles uit, claims worden handregistratie
time.sleep(0.2)
c = cresc()
check(c["stage"] == 0 and not c["active_stops"] and not (set(hw) & drawn()), f"na GC: trap 0, geen claims, niets getrokken ({c['stage']})")
cc(0, 20, 64)
check(cresc()["stage"] == 0 and not (set(hw) & drawn()), "trede nog op 64: telt niet mee, geen registers bij")
cc(0, 20, 0)
cc(0, 20, 64)
check(cresc()["stage"] == 2 and {hw[0], hw[1]} <= drawn(), f"na terug op 0 telt de trede weer: trap {cresc()['stage']}")
cc(0, 20, 0)
check(not (set(hw) & drawn()), "en veert weer terug")

print("\nB2 — zelfde met een koppel in de matrix (UI zet eerst registers, dan koppels)")
koppels = [c["id"] for c in (organ.get("couplers") or [])]
if not koppels:
    print("   (orgel zonder koppels: overgeslagen)")
else:
    kp = koppels[0]
    post("/couplers/set", {"ids": []})
    post("/crescendo/config", {"stages": [[hw[0]], [hw[0], hw[1]], [hw[0], hw[1], kp]], "enabled": True})
    cc(0, 20, 127)
    c = cresc()
    check(c["stage"] == 3 and kp in c["active_stops"], f"trap 3 claimt ook het koppel {kp}: {c['active_stops']}")
    post("/stops/set", {"ids": []}); post("/couplers/set", {"ids": []}); time.sleep(0.2)
    c = cresc()
    check(c["stage"] == 0 and not c["active_stops"], f"na GC met koppel: trap 0 en geen claims ({c['stage']}, {c['active_stops']})")
    cc(0, 20, 64)
    check(cresc()["stage"] == 0 and not (set(hw) & drawn()), "trede op 64 na GC: geen registers bij (ook met koppel in de matrix)")
    cc(0, 20, 0); cc(0, 20, 64)
    check(cresc()["stage"] == 2, f"na terug op 0 telt de trede weer (trap {cresc()['stage']})")
    cc(0, 20, 0)
    post("/crescendo/config", {"stages": [[hw[0]], [hw[0], hw[1]], [hw[0], hw[1], hw[2]]], "enabled": True})

# Opruimen + oorspronkelijke instellingen terug (en opslaan, zie /settings/save hierboven)
cc(0, 20, 0)
post("/crescendo/binding", {"clear": True})
post("/swell/binding", {"division": SW, "clear": True})
for sid in get("/stops/drawn")["drawn_stops"]: post(f"/stops/{sid}/toggle")
post("/crescendo/config", {"stages": orig_cresc["stage_stops"], "enabled": orig_cresc["enabled"], "num_stages": orig_cresc["num_stages"]})
ob = orig_cresc["binding"]
if ob: post("/crescendo/binding", {"channel": ob["channel"], "cc": ob["cc"], "min": ob["min"], "max": ob["max"], "invert": ob["invert"]})
for sdiv in orig_swell:
    b = sdiv["binding"]
    if b: post("/swell/binding", {"division": sdiv["division"], "channel": b["channel"], "cc": b["cc"], "min": b["min"], "max": b["max"], "invert": b["invert"]})
try: post("/settings/save")
except Exception: pass
print(f"\nRESULTAAT: {len(R['pass'])} pass / {len(R['fail'])} fail")
if R["fail"]: print("FOUT:", R["fail"])
sys.exit(1 if R["fail"] else 0)
