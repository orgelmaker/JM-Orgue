# Handleiding JM-Orgue 0.8.98

De handleiding bestaat in alle zeven talen van het programma, met schermafdrukken in
die taal. Open de PDF om te lezen of te printen.

| Taal | PDF |
|---|---|
| Nederlands | [JM-Orgue-handleiding.nl.pdf](JM-Orgue-handleiding.nl.pdf) |
| English | [JM-Orgue-handleiding.en.pdf](JM-Orgue-handleiding.en.pdf) |
| Français | [JM-Orgue-handleiding.fr.pdf](JM-Orgue-handleiding.fr.pdf) |
| Deutsch | [JM-Orgue-handleiding.de.pdf](JM-Orgue-handleiding.de.pdf) |
| Polski | [JM-Orgue-handleiding.pl.pdf](JM-Orgue-handleiding.pl.pdf) |
| Italiano | [JM-Orgue-handleiding.it.pdf](JM-Orgue-handleiding.it.pdf) |
| Español | [JM-Orgue-handleiding.es.pdf](JM-Orgue-handleiding.es.pdf) |

## Opbouw

- `HANDLEIDING.<taal>.html` is de bron van elke taal; `handleiding.css` is de gedeelde opmaak.
- `afbeeldingen/<taal>/` bevat de schermafdrukken in de taal van het programma.
- De PDF maak je door de HTML af te drukken naar PDF (bijvoorbeeld met Edge:
  `msedge --headless=new --no-pdf-header-footer --print-to-pdf=uit.pdf bestand.html`).
