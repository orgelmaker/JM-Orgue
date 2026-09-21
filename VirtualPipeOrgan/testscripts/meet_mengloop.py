# -*- coding: utf-8 -*-
"""Meting van de mengloop (meerkernig plan, fase 1 t/m 3).

Draait op een lopende vpo-app.exe --test-api met een orgel geladen. Meet:
  A. de verdeling van de rendertijd over de passen bij oplopende belasting --
     dat bepaalt via de wet van Amdahl wat meerkernig maximaal kan opleveren;
  B. wat het verdelen in stukken oplevert: sinds fase 3 doen echte
     werkerthreads de stukken 1.., dus dit is de winst van meerkernig mengen
     (en de reductie is de prijs).

LET OP -- twee valkuilen die de eerste metingen bedierven:

  * Het laden van een sampleset loopt na `/load_organ` nog minuten door in de
    achtergrond, en als het klaar is zet het de registratie terug op nul.
    Meten tijdens die tijd geeft zowel te hoge belastingen als plotseling
    verdwijnende stemmen. `wacht_tot_rustig()` wacht daarom tot de registratie
    blijft staan en de belasting bij stilte laag is.
  * De belasting schommelt van callback tot callback. Elke stand wordt daarom
    afgewisseld en meerdere keren gemeten, en we nemen de mediaan.

Gebruik: python testscripts/meet_mengloop.py
"""
import json
import statistics
import sys
import time
import urllib.request

B = "http://127.0.0.1:8765"


def post(pad, body=None):
    data = json.dumps(body).encode() if body is not None else b"{}"
    req = urllib.request.Request(B + pad, data=data, method="POST",
                                 headers={"Content-Type": "application/json"})
    return urllib.request.urlopen(req, timeout=300).read()


def get(pad):
    return json.loads(urllib.request.urlopen(B + pad, timeout=30).read())


def wacht_tot_rustig(alle_stops, tijdslimiet=900):
    """Wacht tot het laden klaar is.

    Een halve controle is niet genoeg: de registratie laat zich tijdens het
    laden gewoon zetten, en pas als het laden klaar is (bij een grote set na
    twintig seconden) wordt hij teruggezet. We eisen daarom dat de registratie
    TWEE opeenvolgende vensters overleeft.
    """
    t0 = time.time()
    goed = 0
    while time.time() - t0 < tijdslimiet:
        if goed == 0:
            post("/stops/set", {"ids": alle_stops})
        time.sleep(8.0)
        st = get("/status")
        rustig = st["render_load"] < 0.10 and st["voice_count"] == 0
        if st["drawn_stop_count"] == len(alle_stops) and rustig:
            goed += 1
            if goed >= 2:
                return st
        else:
            goed = 0
            print("   ... nog bezig met laden (registers %d/%d, belasting %.0f%%)"
                  % (st["drawn_stop_count"], len(alle_stops), st["render_load"] * 100))
    sys.exit("sampleset is na %d s nog niet klaar met laden" % tijdslimiet)


def meet(noten, stukken, lezingen=4):
    post("/audio/mix_chunks?n=%d" % stukken)
    post("/panic")
    time.sleep(1.0)
    for n in noten:
        post("/notes/%d/on?velocity=100" % n)
    time.sleep(2.0)
    rijen = []
    for _ in range(lezingen):
        rijen.append(get("/status"))
        time.sleep(0.4)
    for n in noten:
        post("/notes/%d/off" % n)
    post("/panic")
    time.sleep(0.6)
    med = lambda f: statistics.median([f(x) for x in rijen])
    return {
        "stemmen": med(lambda x: x["voice_count"]),
        "totaal": med(lambda x: x["render_load"]) * 100,
        "wind_trem": med(lambda x: x["mix_pass_load"]["wind_trem"]) * 100,
        "stemwerk": med(lambda x: x["mix_pass_load"]["voices"]) * 100,
        "keten": med(lambda x: x["mix_pass_load"]["chain"]) * 100,
        "reductie": med(lambda x: x["mix_pass_load"]["reduce"]) * 100,
    }


def main():
    st = get("/status")
    if not st.get("organ_loaded"):
        sys.exit("geen orgel geladen")
    organ = get("/organ")
    alle = [s["id"] for d in organ["divisions"] for s in d["stops"]]
    print("orgel: %s (%d registers), %d Hz, %d frames per callback"
          % (organ.get("name"), len(alle), st["sample_rate"], st.get("render_frames") or 0))
    print("wachten tot de sampleset klaar is met laden...")
    wacht_tot_rustig(alle)
    print("   klaar.\n")

    print("A. Verdeling van de rendertijd over de passen (1 stuk)")
    print("   %-7s %-9s %-9s %-10s %-9s %-8s" %
          ("noten", "stemmen", "totaal", "wind/trem", "stemwerk", "keten"))
    for n_noten in (0, 3, 6, 10, 14, 18):
        noten = list(range(48, 48 + n_noten * 2, 2))
        r = meet(noten, 1)
        if n_noten > 0 and r["stemmen"] == 0:
            print("   %-7d  GEEN STEMMEN — meting overgeslagen" % n_noten)
            continue
        deel = (r["stemwerk"] / r["totaal"] * 100) if r["totaal"] > 0.01 else 0.0
        print("   %-7d %-9d %-9.2f %-10.2f %-9.2f %-8.2f  stemwerk %5.1f%%"
              % (n_noten, r["stemmen"], r["totaal"], r["wind_trem"], r["stemwerk"],
                 r["keten"], deel))

    print("\nB. Winst van het verdelen over werkerthreads")
    print("   drie ronden afgewisseld, mediaan per aantal stukken")
    noten = list(range(48, 48 + 10 * 2, 2))
    metingen = {k: [] for k in (1, 2, 4, 8)}
    reducties = {k: [] for k in (1, 2, 4, 8)}
    for _ in range(3):
        for stukken in (1, 2, 4, 8):
            r = meet(noten, stukken, lezingen=3)
            if r["stemmen"] > 0:
                metingen[stukken].append(r["totaal"])
                reducties[stukken].append(r["reductie"])
    print("   %-7s %-9s %-10s %-12s" % ("stukken", "totaal", "reductie", "verschil"))
    basis = statistics.median(metingen[1]) if metingen[1] else 0.0
    for k in (1, 2, 4, 8):
        if not metingen[k]:
            print("   %-7d  geen bruikbare meting" % k)
            continue
        m = statistics.median(metingen[k])
        red = statistics.median(reducties[k])
        print("   %-7d %-9.2f %-10.2f %+.2f%% t.o.v. 1 stuk" % (k, m, red, m - basis))

    post("/audio/mix_chunks?n=1")
    print("\n(terug op 1 stuk)")


if __name__ == "__main__":
    main()
