# -*- coding: utf-8 -*-
"""Bewaakt dat de bovenbalk in elke taal op één rij past (lengtebewaking).

De bovenbalk (ui/src/components/Header.svelte) toont drie tabs en de knop om
tussen luidsprekers en hoofdtelefoon te wisselen. Dit script telt per taal de
tekens van die labels en faalt als een vertaling zoveel langer is dat de rij
waarschijnlijk niet meer past. Het meet GEEN pixels: dat vraagt een echt venster.
Het is een vangnet tegen een te lange vertaling, niet een bewijs van de indeling.

De grenzen zijn de huidige langste taal (Duits: 60 tekens in totaal) met ongeveer
10 % marge; een label dat zelf langer is dan MAX_LABEL valt ook op.

Gebruik: python testscripts/check_bovenbalk_lengte.py   (afsluitcode 1 bij een fout)
"""
import io
import json
import os
import sys

if hasattr(sys.stdout, "reconfigure"):
    sys.stdout.reconfigure(encoding="utf-8")

TALEN = ["nl", "en", "fr", "de", "pl", "it", "es"]
# Labels die altijd zichtbaar in de bovenbalk staan (de organisatienaam en het
# merkteken zijn taalonafhankelijk). De profielknop toont één van de twee.
TABS = ["nav.orgel", "nav.orgel_instellingen", "nav.algemene_instellingen"]
PROFIELKNOP = ["settings.audio_profile_headphones", "settings.audio_profile_speakers"]
MAX_TOTAAL = 66   # tekens, alle labels samen
MAX_LABEL = 28    # tekens, één label

BASIS = os.path.join(os.path.dirname(os.path.abspath(__file__)), os.pardir,
                     "ui", "src", "lib", "locales")


def haal(d, pad):
    for k in pad.split("."):
        d = d[k]
    return d


def main():
    fouten = []
    print("%-4s %6s  %s" % ("taal", "totaal", "langste label"))
    for taal in TALEN:
        with io.open(os.path.join(BASIS, taal + ".json"), encoding="utf-8") as f:
            d = json.load(f)
        labels = [haal(d, k) for k in TABS]
        profiel = max((haal(d, k) for k in PROFIELKNOP), key=len)
        alles = labels + [profiel]
        totaal = sum(len(x) for x in alles)
        langste = max(alles, key=len)
        print("%-4s %6d  %s (%d)" % (taal, totaal, langste, len(langste)))
        if totaal > MAX_TOTAAL:
            fouten.append("%s: bovenbalk-labels samen %d tekens (maximum %d)" % (taal, totaal, MAX_TOTAAL))
        for x in alles:
            if len(x) > MAX_LABEL:
                fouten.append("%s: label '%s' is %d tekens (maximum %d)" % (taal, x, len(x), MAX_LABEL))
    if fouten:
        print("\nTE LANG:")
        for f in fouten:
            print("  -", f)
        sys.exit(1)
    print("\nOK: de bovenbalk-labels blijven binnen de lengtegrens in alle %d talen." % len(TALEN))


if __name__ == "__main__":
    main()
