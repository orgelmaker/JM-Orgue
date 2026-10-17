# Handleiding JM-Orgue 0.8.93

De handleiding bestaat in alle zeven talen van het programma, met schermafdrukken in
die taal. Open de PDF om te lezen of te printen.

| Taal | PDF |
|---|---|
| Nederlands | [JM-Orgue-handleiding-0.8.93.nl.pdf](JM-Orgue-handleiding-0.8.93.nl.pdf) |
| English | [JM-Orgue-handleiding-0.8.93.en.pdf](JM-Orgue-handleiding-0.8.93.en.pdf) |
| Français | [JM-Orgue-handleiding-0.8.93.fr.pdf](JM-Orgue-handleiding-0.8.93.fr.pdf) |
| Deutsch | [JM-Orgue-handleiding-0.8.93.de.pdf](JM-Orgue-handleiding-0.8.93.de.pdf) |
| Polski | [JM-Orgue-handleiding-0.8.93.pl.pdf](JM-Orgue-handleiding-0.8.93.pl.pdf) |
| Italiano | [JM-Orgue-handleiding-0.8.93.it.pdf](JM-Orgue-handleiding-0.8.93.it.pdf) |
| Español | [JM-Orgue-handleiding-0.8.93.es.pdf](JM-Orgue-handleiding-0.8.93.es.pdf) |

## Opbouw

- `HANDLEIDING.<taal>.html` is de bron van elke taal; `handleiding.css` is de gedeelde opmaak.
- `afbeeldingen/<taal>/` bevat de schermafdrukken in de taal van het programma.
- De PDF maak je door de HTML af te drukken naar PDF (bijvoorbeeld met Edge:
  `msedge --headless=new --no-pdf-header-footer --print-to-pdf=uit.pdf bestand.html`).
