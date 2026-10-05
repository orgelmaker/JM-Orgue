# Licenties van derden

## Hoofdtelefoonpresets: AutoEq

De ingebouwde hoofdtelefoonpresets (`ui/src/assets/eq-presets.json`) zijn afgeleid van het AutoEq-project van Jaakko Pasanen (https://github.com/jaakkopasanen/AutoEq, commit `7ae0f56d53074872b028649617a22bbb4232feb7`), metingen door oratory1990. AutoEq is uitgebracht onder de MIT-licentie:

```
MIT License

Copyright (c) 2018-2022 Jaakko Pasanen

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

## Luidsprekercatalogus: spinorama

De lijst met luidsprekernamen in `ui/src/assets/luidsprekers-catalogus.json`
komt uit het spinorama-project van Pierre Aubert (https://github.com/pierreaubert/spinorama,
https://www.spinorama.org), code onder GPL-3. De parametrische correctie per
model (`datas/eq/<model>/iir-autoeq.txt`) wordt **niet** met JM-Orgue
meegeleverd: de app haalt dat ene bestand op het moment van kiezen op van de in
de catalogus vastgepinde commit, zoals een gebruiker het ook van de site zou
downloaden. De onderliggende metingen komen van verschillende bronnen (Audio
Science Review, Erin's Audio Corner, fabrikanten en anderen); per model staat
de bron op spinorama.org. Zo'n preset corrigeert de luidspreker zelf, niet de
kamer.
