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

## RAR-pakketten uitpakken: UnRAR

JM-Orgue kan RAR-archieven (ook meerdelig) met een sampleset uitpakken. Dat gebeurt met de
UnRAR-broncode van RARLAB, via de Rust-bibliotheek `unrar` / `unrar_sys` (versie 0.5.8, MIT of Apache-2.0,
https://github.com/muja/unrar.rs) die de UnRAR-code meebouwt. JM-Orgue gebruikt UnRAR alleen om uit te pakken: er is
geen RAR-archiver in gemaakt en het RAR-compressiealgoritme is niet nagebouwd. De UnRAR-broncode is ongewijzigd
(zoals de bibliotheek hem meelevert); de licentie daarvan luidt:

```
      The source code of UnRAR utility is freeware. This means:

   1. All copyrights to RAR and the utility UnRAR are exclusively
      owned by the author - Alexander Roshal.

   2. UnRAR source code may be used in any software to handle
      RAR archives without limitations free of charge, but cannot be
      used to develop RAR (WinRAR) compatible archiver and to
      re-create RAR compression algorithm, which is proprietary.
      Distribution of modified UnRAR source code in separate form
      or as a part of other software is permitted, provided that
      full text of this paragraph, starting from "UnRAR source code"
      words, is included in license, or in documentation if license
      is not available, and in source code comments of resulting package.

   3. The UnRAR utility may be freely distributed. It is allowed
      to distribute UnRAR inside of other software packages.

   4. THE RAR ARCHIVER AND THE UnRAR UTILITY ARE DISTRIBUTED "AS IS".
      NO WARRANTY OF ANY KIND IS EXPRESSED OR IMPLIED.  YOU USE AT
      YOUR OWN RISK. THE AUTHOR WILL NOT BE LIABLE FOR DATA LOSS,
      DAMAGES, LOSS OF PROFITS OR ANY OTHER KIND OF LOSS WHILE USING
      OR MISUSING THIS SOFTWARE.

   5. Installing and using the UnRAR utility signifies acceptance of
      these terms and conditions of the license.

   6. If you don't agree with terms of the license you must remove
      UnRAR files from your storage devices and cease to use the
      utility.

      Thank you for your interest in RAR and UnRAR.

                                            Alexander L. Roshal
```
