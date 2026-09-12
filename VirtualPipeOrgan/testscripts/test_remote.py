# Integratietest afstandsbediening in het netwerk (0.7.38).
# Vereist: vpo-app gestart met --test-api (poort 8765), bij voorkeur met een
# geladen orgel én het hoofdvenster op het Orgel-tabblad (voor de /action-tests).
# Gebruik:  python test_remote.py            (poort 8766)
#           python test_remote.py 8790       (andere remote-poort)
import json, sys, time, urllib.request, urllib.error, socket

TEST = "http://127.0.0.1:8765"
RPORT = int(sys.argv[1]) if len(sys.argv) > 1 else 8766
REMOTE = f"http://127.0.0.1:{RPORT}"
fails = 0

def req(url, method="GET", body=None, headers=None, t=20):
    """Geeft (status, headers, tekst). Netwerkfout → status 0."""
    data = None
    h = {"Content-Type": "application/json"}
    if headers: h.update(headers)
    if body is not None:
        data = json.dumps(body).encode()
    r = urllib.request.Request(url, data=data, method=method, headers=h)
    try:
        with urllib.request.urlopen(r, timeout=t) as resp:
            return resp.status, dict(resp.headers), resp.read().decode("utf-8", "replace")
    except urllib.error.HTTPError as e:
        return e.code, dict(e.headers), e.read().decode("utf-8", "replace")
    except (urllib.error.URLError, socket.error, ConnectionError) as e:
        return 0, {}, str(e)

class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, *a, **k): return None
_noredir = urllib.request.build_opener(NoRedirect)

def req_noredirect(url):
    try:
        with _noredir.open(urllib.request.Request(url), timeout=20) as resp:
            return resp.status, dict(resp.headers)
    except urllib.error.HTTPError as e:
        return e.code, dict(e.headers)
    except Exception as e:
        return 0, {"err": str(e)}

def J(text):
    try: return json.loads(text)
    except Exception: return None

def check(naam, ok, info=""):
    global fails
    print(("  OK   " if ok else "  FOUT ") + naam + (f"  [{info}]" if info else ""))
    if not ok: fails += 1

def get_json(url, **kw):
    s, h, t = req(url, **kw); return s, J(t)

# ---------- 0. test-API bereikbaar ----------
s, st = get_json(TEST + "/status")
if s != 200:
    print("Test-API niet bereikbaar op 8765 — start vpo-app --test-api"); sys.exit(2)
print(f"Test-API ok: orgel geladen={st.get('organ_loaded')} audio={st.get('audio_running')}")

# ---------- 1. inschakelen via test-API ----------
print("1. /remote/enable")
s, dto = get_json(TEST + "/remote/enable", method="POST", body={"enabled": True, "port": RPORT})
check("enable → 200", s == 200, f"{s} {dto}")
if s != 200: sys.exit(1)
check("running", dto.get("running") is True)
TOKEN = dto.get("token", "")
check("token 12 tekens", len(TOKEN) == 12 and all(c in "abcdefghjkmnpqrstuvwxyz23456789" for c in TOKEN), TOKEN)
urls = dto.get("urls", [])
check("urls[0] eindigt op /r/<token>/", bool(urls) and urls[0].endswith(f":{RPORT}/r/{TOKEN}/"), str(urls))
check("qr_svg begint met <svg", (dto.get("qr_svg") or "").lstrip().startswith("<svg") or (dto.get("qr_svg") or "").startswith("<?xml"), (dto.get("qr_svg") or "")[:30])
s, st2 = get_json(TEST + "/remote/status")
check("status: zelfde token", s == 200 and st2.get("token") == TOKEN)
R = f"{REMOTE}/r/{TOKEN}"

# ---------- 2. token-loos = 401, redirect ----------
print("2. token")
s, _, _ = req(REMOTE + "/state"); check("zonder token → 401", s == 401, str(s))
s, _, _ = req(REMOTE + "/r/verkeerd/state"); check("fout token → 401", s == 401, str(s))
s, h = req_noredirect(R); check("/r/<token> → 302 naar /r/<token>/", s == 302 and h.get("Location") == f"/r/{TOKEN}/", f"{s} {h.get('Location')}")

# ---------- 3. HTML + header-variant ----------
print("3. pagina en /state")
s, h, body = req(R + "/")
check("GET / → 200 text/html", s == 200 and "text/html" in h.get("Content-Type", ""), f"{s} {h.get('Content-Type')}")
check("pagina bevat viewport-meta en fetch(", '<meta name="viewport"' in body and "fetch(" in body)
check("Cache-Control: no-store", h.get("Cache-Control") == "no-store")
s, state = get_json(REMOTE + "/state", headers={"X-Remote-Token": TOKEN})
check("header-token → 200", s == 200, str(s))
for k in ("organ_loaded", "drawn", "couplers", "tremulant", "master_db", "voices", "setzer", "crescendo"):
    check(f"/state heeft veld {k}", isinstance(state, dict) and k in state)
check("setzer.data heeft 10 vlaggen", isinstance(state, dict) and len(state.get("setzer", {}).get("data", [])) == 10)

# ---------- 8. whitelist ----------
print("8. whitelist")
s, _, _ = req(R + "/notes/60/on", method="POST"); check("POST /notes/60/on → 404", s == 404, str(s))
s, _, _ = req(R + "/load_organ", method="POST", body={"path": "x"}); check("POST /load_organ → 404", s == 404, str(s))
s, _, _ = req(R + "/logs"); check("GET /logs → 404", s == 404, str(s))
s, _, _ = req(R + "/remote/status"); check("GET /remote/status via remote → 404", s == 404, str(s))
s, _, _ = req(TEST + "/status"); check("test-API /status werkt nog", s == 200)
s, _, _ = req(TEST + "/stops/drawn"); check("test-API /stops/drawn werkt nog", s == 200)
s, _, _ = req(TEST + "/settings/mirror"); check("test-API /settings/mirror werkt nog", s == 200)

# ---------- 4-7: alleen met geladen orgel ----------
s, organ = get_json(TEST + "/organ")
if not organ:
    print("Geen orgel geladen — tests 4-7 (register/koppel/master/acties) overgeslagen; laad een orgel en draai opnieuw.")
    s, _, _ = req(R + "/action/24", method="POST"); check("action/24 zonder orgel → 403", s == 403, str(s))
else:
    print("4. register")
    sid = organ["divisions"][0]["stops"][0]["id"]
    s, r = get_json(R + f"/stops/{urllib.request.quote(sid, safe='')}/toggle", method="POST")
    check("toggle → 200 active=true", s == 200 and r.get("active") is True and r.get("stop_id") == sid, f"{s} {r}")
    s, d = get_json(TEST + "/stops/drawn"); check("8765 /stops/drawn bevat id", sid in d.get("drawn_stops", []))
    s, st = get_json(R + "/state"); check("8766 /state.drawn bevat id", sid in st.get("drawn", []))
    s, og = get_json(TEST + "/organ"); check("8765 /organ drawn-vlag actueel", any(x["id"] == sid and x["drawn"] for dv in og["divisions"] for x in dv["stops"]))
    s, r = get_json(R + f"/stops/{urllib.request.quote(sid, safe='')}/toggle", method="POST")
    check("toggle terug → active=false", s == 200 and r.get("active") is False, f"{s} {r}")
    s, _, _ = req(R + "/stops/bestaat_niet_xyz/toggle", method="POST"); check("onbekende id → 404", s == 404, str(s))

    print("5. koppel")
    cps = organ.get("couplers") or []
    if cps:
        cid = cps[0]["id"]
        s, r = get_json(R + f"/couplers/{urllib.request.quote(cid, safe='')}/toggle", method="POST")
        check("koppel toggle → active=true", s == 200 and r.get("active") is True, f"{s} {r}")
        s, og = get_json(TEST + "/organ"); check("8765 /organ couplers[0].active", any(c["id"] == cid and c["active"] for c in og["couplers"]))
        s, st = get_json(R + "/state"); check("/state.couplers bevat id", cid in st.get("couplers", []))
        s, r = get_json(R + f"/couplers/{urllib.request.quote(cid, safe='')}/toggle", method="POST"); check("koppel terug uit", s == 200 and r.get("active") is False)
    else:
        print("  (orgel heeft geen koppels — overgeslagen)")
    s, _, _ = req(R + "/couplers/bestaat_niet/toggle", method="POST"); check("onbekende koppel → 404", s == 404, str(s))

    print("6. master")
    s, st0 = get_json(R + "/state"); vorige = st0.get("master_db", -6)
    s, r = get_json(R + "/master?db=-12", method="POST"); check("master -12 → {db:-12}", s == 200 and r.get("db") == -12, f"{s} {r}")
    s, st = get_json(R + "/state"); check("/state.master_db == -12", st.get("master_db") == -12, str(st.get("master_db")))
    s, r = get_json(R + "/master?db=99", method="POST"); check("db=99 geclamped op 6", s == 200 and r.get("db") == 6, f"{r}")
    s, _, _ = req(R + "/master", method="POST"); check("zonder db → 400", s == 400, str(s))
    req(R + f"/master?db={vorige}", method="POST")

    print("7. acties/setzer (hoofdvenster op Orgel-tabblad)")
    s, r = get_json(R + "/action/42", method="POST"); check("action/42 (afsluiten) → 403", s == 403, str(s))
    s, r = get_json(R + "/action/200", method="POST"); check("action/200 → 403", s == 403, str(s))
    ndiv = len(organ["divisions"])
    s, r = get_json(R + f"/action/{24 + ndiv}", method="POST"); check(f"action/{24+ndiv} (divisie buiten bereik) → 403", s == 403, str(s))
    s, r = get_json(R + "/action/10", method="POST")
    if s == 409:
        print("  409: orgelscherm niet actief — open het Orgel-tabblad in JM-Orgue en draai de test opnieuw voor de setzer-controles.")
        check("409 heeft foutmelding", "Orgel" in (r or {}).get("error", ""))
    else:
        check("action/10 (SET) → queued", s == 200 and r.get("queued") is True, f"{s} {r}")
        time.sleep(0.4)
        s, st = get_json(R + "/state"); check("set_mode true na SET", st["setzer"]["set_mode"] is True, str(st["setzer"]))
        # eerst een register trekken: een lege registratie zet dataFlags niet
        get_json(R + f"/stops/{urllib.request.quote(sid, safe='')}/toggle", method="POST"); time.sleep(0.4)
        get_json(R + "/action/3", method="POST"); time.sleep(0.5)
        s, st = get_json(R + "/state")
        check("na SET+3: set_mode false, preset 3, data[3] true", st["setzer"]["set_mode"] is False and st["setzer"]["preset"] == 3 and st["setzer"]["data"][3] is True, str(st["setzer"]))
        get_json(R + "/action/11", method="POST"); time.sleep(0.5)
        s, st = get_json(R + "/state")
        check("na GC: drawn en couplers leeg, preset -1", st["drawn"] == [] and st["couplers"] == [] and st["setzer"]["preset"] == -1, str(st["setzer"]) + str(st["drawn"]))
        get_json(R + "/action/17", method="POST"); time.sleep(0.5)
        s, st = get_json(R + "/state"); check("na M2 (17): level 2", st["setzer"]["level"] == 2, str(st["setzer"]))
        get_json(R + "/action/16", method="POST"); time.sleep(0.3)
        if organ["divisions"][0].get("has_tremulant"):
            s, st0 = get_json(R + "/state"); t0 = st0["tremulant"][0]
            get_json(R + "/action/24", method="POST"); time.sleep(0.5)
            s, st = get_json(R + "/state"); check("tremulant[0] gewisseld", st["tremulant"][0] != t0, f"{t0} → {st['tremulant'][0]}")
            get_json(R + "/action/24", method="POST")

# ---------- 9. uit/aan/poort/token ----------
print("9. uit/aan/nieuw token")
s, dto = get_json(TEST + "/remote/enable", method="POST", body={"enabled": False})
check("disable → running false", s == 200 and dto.get("running") is False, f"{s} {dto}")
s, _, _ = req(R + "/state"); check("na uit: verbinding geweigerd", s == 0, str(s))
s, dto = get_json(TEST + "/remote/enable", method="POST", body={"enabled": True, "port": RPORT})
check("direct weer aan → running true (kick + bind-retry)", s == 200 and dto.get("running") is True, f"{s} {dto if s != 200 else ''}")
s, _, _ = req(R + "/state"); check("oude token werkt weer", s == 200, str(s))
s, dto = get_json(TEST + "/remote/new_token", method="POST")
NEW = dto.get("token", "") if dto else ""
check("nieuw token ≠ oud", s == 200 and NEW and NEW != TOKEN, f"{s} {NEW}")
s, _, _ = req(R + "/state"); check("oud token → 401", s == 401, str(s))
s, _, _ = req(f"{REMOTE}/r/{NEW}/state"); check("nieuw token → 200", s == 200, str(s))
s, dto = get_json(TEST + "/remote/enable", method="POST", body={"enabled": True, "port": 80})
check("poort 80 → fout", s != 200 and "1024" in json.dumps(dto), f"{s} {dto}")
s, dto = get_json(TEST + "/remote/enable", method="POST", body={"enabled": True, "port": 8765})
check("poort 8765 (test-API) → fout", s != 200 and "test-API" in json.dumps(dto), f"{s} {dto}")
s, st2 = get_json(TEST + "/remote/status"); check("na fout: nog steeds actief op oude poort", st2.get("running") is True and st2.get("port") == RPORT, str(st2.get('port')))

print()
print("KLAAR — fouten:", fails)
print("Handmatig: telefoon in hetzelfde wifi → QR scannen in Algemene Instellingen; app herstarten → 10 (persistentie) controleren met GET", f"{REMOTE}/r/{NEW}/state")
sys.exit(1 if fails else 0)
