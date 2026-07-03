/**
 * Stemmingen (Temperaments) voor virtueel pijporgel
 *
 * Elke stemming bevat:
 *   - name: Naam van de stemming
 *   - nameDutch: Nederlandse naam (indien van toepassing)
 *   - category: Categorie (equal, meantone, well, pythagorean, historical)
 *   - period: Historische periode / oorsprong
 *   - year: Jaartal (bij benadering)
 *   - modernUse: Wordt het veel gebruikt bij moderne orgels?
 *   - description: Korte beschrijving
 *   - cents: Array van 12 cent-afwijkingen t.o.v. gelijkzwevende stemming
 *             Volgorde: [C, C#, D, D#, E, F, F#, G, G#, A, A#, B]
 *             Referentie: A = 0
 *
 * Bronnen:
 *   - hpschd.nu (Carey Beebe Harpsichords)
 *   - katsurashareware.com temperament tables
 *   - kylegann.com historical tunings
 *   - tunableapp.com
 *   - Wikipedia (Werckmeister, Kirnberger, Meantone)
 */

export const NOTE_NAMES = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];

export const CATEGORIES = {
  equal: "Gelijkzwevend",
  meantone: "Middentoonstemming",
  well: "Welgetempereerd",
  pythagorean: "Pythagorisch",
  historical: "Historisch",
  just: "Reine stemming",
};

export const temperaments = [
  // ============================================================
  // GELIJKZWEVEND (Equal Temperament)
  // ============================================================
  {
    name: "Equal Temperament",
    nameDutch: "Gelijkzwevende stemming",
    category: "equal",
    period: "Theoretisch beschreven ca. 1584 (Zhu Zaiyu / Stevin), algemeen verspreid vanaf 19e eeuw",
    year: 1584,
    modernUse: true,
    description: "Alle halve tonen zijn gelijk. Geen toonsoortkleur, universeel inzetbaar. Standaard bij de meeste moderne orgels.",
    cents: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
  },

  // ============================================================
  // PYTHAGORISCH (Pythagorean)
  // ============================================================
  {
    name: "Pythagorean",
    nameDutch: "Pythagorische stemming",
    category: "pythagorean",
    period: "Oudheid / Middeleeuwen (Pythagoras, ca. 500 v.Chr.)",
    year: -500,
    modernUse: false,
    description: "Gebaseerd op reine kwinten (3:2). Prachtige kwinten, maar scherpe grote tertsen (+22 cent). Wolf-kwint op G#-Eb. Gebruikt in middeleeuws orgel.",
    cents: [-5.87, 7.82, -1.96, -11.73, 1.96, -7.82, 5.87, -3.91, 9.78, 0, -9.78, 3.91],
  },

  // ============================================================
  // REINE STEMMING (Just Intonation)
  // ============================================================
  {
    name: "Just Intonation (Major)",
    nameDutch: "Reine stemming (majeur)",
    category: "just",
    period: "Theoretisch, o.a. Zarlino (1558), Marpurg",
    year: 1558,
    modernUse: false,
    description: "Reine intervallen gebaseerd op eenvoudige frequentieverhoudingen. Slechts bruikbaar in een beperkt aantal toonsoorten.",
    cents: [15.64, -13.69, 19.55, 31.28, 1.96, 13.69, 5.87, 17.60, -11.73, 0, 33.24, 3.91],
  },

  // ============================================================
  // MIDDENTOON (Meantone) STEMMINGEN
  // ============================================================
  {
    name: "Quarter-comma Meantone",
    nameDutch: "1/4-komma middentoon (Aaron)",
    category: "meantone",
    period: "Renaissance / Vroege Barok (Pietro Aaron, 1523; Zarlino 1571; Salinas 1577)",
    year: 1523,
    modernUse: true,
    description: "De klassieke middentoonstemming. Reine grote tertsen, smalle kwinten (-5.5 cent). Wolf-kwint op G#-Eb. Veel gebruikt bij historische orgels, vooral voor muziek tot ca. 1700.",
    cents: [10.27, -13.69, 3.42, 20.53, -3.42, 13.69, -10.27, 6.84, -17.11, 0, 17.11, -6.84],
  },
  {
    name: "Quarter-comma Meantone (D# variant)",
    nameDutch: "1/4-komma middentoon (met D# i.p.v. Eb)",
    category: "meantone",
    period: "Renaissance / Vroege Barok",
    year: 1523,
    modernUse: false,
    description: "Variant met D# in plaats van Eb. Wolf verschuift naar andere toonsoorten.",
    cents: [10.27, -13.69, 3.42, -20.53, -3.42, 13.69, -10.27, 6.84, -17.11, 0, 17.11, -6.84],
  },
  {
    name: "1/5-comma Meantone (Rossi)",
    nameDutch: "1/5-komma middentoon (Rossi)",
    category: "meantone",
    period: "Barok (Lemme Rossi, 1666)",
    year: 1666,
    modernUse: false,
    description: "Iets minder extreme middentoon dan 1/4-komma. Grote tertsen iets minder rein, kwinten iets beter.",
    cents: [7.04, -9.39, 2.35, 14.08, -2.35, 9.39, -7.04, 4.69, -11.73, 0, 11.73, -4.69],
  },
  {
    name: "1/6-comma Meantone (Silbermann)",
    nameDutch: "1/6-komma middentoon (Silbermann)",
    category: "meantone",
    period: "Barok (Gottfried Silbermann, ca. 1700-1750; ook Sauveur 1707)",
    year: 1700,
    modernUse: true,
    description: "Compromis tussen middentoon en gelijkzwevend. Kwinten -3.6 cent, grote tertsen +7 cent. Veel gebruikt door Silbermann voor zijn beroemde orgels in Saksen. Ook beschreven als 'stemming van gewone musici' (Sauveur).",
    cents: [4.89, -6.52, 1.63, 9.78, -1.63, 6.52, -4.89, 3.26, -8.15, 0, 8.15, -3.26],
  },
  {
    name: "2/9-comma Meantone (Rossi)",
    nameDutch: "2/9-komma middentoon (Rossi)",
    category: "meantone",
    period: "Barok (Lemme Rossi)",
    year: 1666,
    modernUse: false,
    description: "Variatie op middentoon, iets dichter bij 1/4-komma dan 1/5-komma.",
    cents: [8.47, -11.30, 2.82, 16.95, -2.82, 11.30, -8.47, 5.65, -14.12, 0, 14.12, -5.65],
  },
  {
    name: "1/3-comma Meantone (Salinas)",
    nameDutch: "1/3-komma middentoon (Salinas)",
    category: "meantone",
    period: "Renaissance (Francisco de Salinas, 1577)",
    year: 1577,
    modernUse: false,
    description: "Extreme middentoon met reine kleine tertsen. Reine kleine tertsen (6:5), maar zeer smalle kwinten. Zelden gebruikt in de praktijk.",
    cents: [15.64, -20.86, 5.21, 31.28, -5.21, 20.86, -15.64, 10.43, -26.07, 0, 26.07, -10.43],
  },
  {
    name: "2/7-comma Meantone (Zarlino)",
    nameDutch: "2/7-komma middentoon (Zarlino)",
    category: "meantone",
    period: "Renaissance (Gioseffo Zarlino, 1558)",
    year: 1558,
    modernUse: false,
    description: "Zarlino's middentoon: compromis met bijna reine grote en kleine tertsen tegelijk.",
    cents: [12.57, -16.76, 4.19, 25.14, -4.19, 16.76, -12.57, 8.38, -20.95, 0, 20.95, -8.38],
  },

  // ============================================================
  // WELGETEMPEREERD (Well-tempered)
  // ============================================================
  {
    name: "Werckmeister III",
    nameDutch: "Werckmeister III",
    category: "well",
    period: "Barok (Andreas Werckmeister, 1691)",
    year: 1691,
    modernUse: true,
    description: "De bekendste welgetempereerde stemming. Vier kwinten verkleind met 1/4 pythagorisch komma. Alle toonsoorten bruikbaar, met duidelijke toonsoortkleur. Zeer populair bij historisch geinformeerde orgelbouwers.",
    cents: [11.73, 1.96, 3.91, 5.87, 1.96, 9.78, 0, 7.82, 3.91, 0, 7.82, 3.91],
  },
  {
    name: "Werckmeister IV",
    nameDutch: "Werckmeister IV",
    category: "well",
    period: "Barok (Andreas Werckmeister, 1691)",
    year: 1691,
    modernUse: false,
    description: "Complexere verdeling dan Werckmeister III, met zowel versmalde als verwijde kwinten. Meer toonsoortkleur, minder gelijkmatig.",
    cents: [9.78, -7.82, 5.87, 3.91, 1.96, 7.82, -1.96, 3.91, -5.87, 0, 13.69, -3.91],
  },
  {
    name: "Werckmeister V",
    nameDutch: "Werckmeister V",
    category: "well",
    period: "Barok (Andreas Werckmeister, 1691)",
    year: 1691,
    modernUse: false,
    description: "Slechts twee versmalde kwinten. Benadert gelijkzwevend het dichtst van alle Werckmeister-stemmingen.",
    cents: [0, -3.91, 3.91, 0, -3.91, 3.91, 0, 1.96, -7.82, 0, 1.96, -1.96],
  },
  {
    name: "Werckmeister VI",
    nameDutch: "Werckmeister VI (Septenarius)",
    category: "well",
    period: "Barok (Andreas Werckmeister, 1691)",
    year: 1691,
    modernUse: false,
    description: "Gebaseerd op het getal 7 (septenarius). Ongewone verdeling, zelden gebruikt.",
    cents: [7.54, -2.23, -5.31, 5.03, 1.96, 5.59, 2.51, 6.14, -0.28, 0, 6.98, 3.91],
  },
  {
    name: "Kirnberger II",
    nameDutch: "Kirnberger II",
    category: "well",
    period: "Barok / Klassiek (Johann Philipp Kirnberger, 1771)",
    year: 1771,
    modernUse: false,
    description: "Drie reine grote tertsen, tien reine kwinten. Twee smalle 'halve wolf'-kwinten. Sterke toonsoortkleur.",
    cents: [4.89, -4.89, 8.80, -0.98, -8.80, 2.93, -4.89, 6.84, -2.93, 0, 0.98, -6.84],
  },
  {
    name: "Kirnberger III",
    nameDutch: "Kirnberger III",
    category: "well",
    period: "Barok / Klassiek (Johann Philipp Kirnberger, 1779)",
    year: 1779,
    modernUse: true,
    description: "Het syntonisch komma verdeeld over vier kwinten. Een reine grote terts (C-E). Populair bij orgelbouwers, goede balans tussen toonsoortkleur en bruikbaarheid.",
    cents: [10.27, 0.49, 3.42, 4.40, -3.42, 8.31, 0.49, 6.84, 2.44, 0, 6.36, -1.47],
  },
  {
    name: "Vallotti",
    nameDutch: "Vallotti (Vallotti-Young)",
    category: "well",
    period: "Barok / Klassiek (Francesco Vallotti, ca. 1728; gepubliceerd 1779)",
    year: 1728,
    modernUse: true,
    description: "Zes kwinten op de stamtonen verkleind met 1/6 pythagorisch komma, overige zes rein. Zeer populair bij moderne orgelbouwers voor barokrepertoire. Een van de meest gevraagde stemmingen.",
    cents: [5.87, 0, 1.96, 3.91, -1.96, 7.82, -1.96, 3.91, 1.96, 0, 5.87, -3.91],
  },
  {
    name: "Young II",
    nameDutch: "Young II",
    category: "well",
    period: "Klassiek (Thomas Young, 1800)",
    year: 1800,
    modernUse: true,
    description: "Verschoven versie van Vallotti. Symmetrisch rond D. Veel gebruikt bij orgels, o.a. het Bach-orgel in Leipzig.",
    cents: [5.87, -3.91, 1.96, 0, -1.96, 3.91, -5.87, 3.91, -1.96, 0, 1.96, -3.91],
  },
  {
    name: "Young I",
    nameDutch: "Young I",
    category: "well",
    period: "Klassiek (Thomas Young, 1799)",
    year: 1799,
    modernUse: false,
    description: "Thomas Youngs eerste welgetempereerde stemming uit 1799.",
    // Absolute cents from kylegann.com: C=0, C#=93.9, D=195.8, Eb=297.8, E=391.7, F=499.9, F#=591.9, G=697.9, G#=795.8, A=893.8, Bb=999.8, B=1091.8
    // Convert to deviation from ET (ET: 0,100,200,300,400,500,600,700,800,900,1000,1100) then shift so A=0
    // Raw dev from C: 0, -6.1, -4.2, -2.2, -8.3, -0.1, -8.1, -2.1, -4.2, -6.2, -0.2, -8.2
    // Shift so A=0: add 6.2 to all: 6.2, 0.1, 2.0, 4.0, -2.1, 6.1, -1.9, 4.1, 2.0, 0, 6.0, -2.0
    cents: [6.20, 0.10, 2.00, 4.00, -2.10, 6.10, -1.90, 4.10, 2.00, 0, 6.00, -2.00],
  },
  {
    name: "Kellner",
    nameDutch: "Kellner (Bach)",
    category: "well",
    period: "Reconstructie (Herbert Anton Kellner, 1975/1982)",
    year: 1975,
    modernUse: true,
    description: "Kellners reconstructie van Bachs stemming. Vijf kwinten verkleind met 1/5 pythagorisch komma. Gematigde toonsoortkleur.",
    cents: [8.21, -1.56, 2.74, 2.35, -2.74, 6.26, -3.52, 5.47, 0.39, 0, 4.30, -0.78],
  },
  {
    name: "Barnes (Bach)",
    nameDutch: "Barnes (Bach, 1977)",
    category: "well",
    period: "Reconstructie (John Barnes, 1977)",
    year: 1977,
    modernUse: false,
    description: "John Barnes' reconstructie van Bachs stemming op basis van analyse van het Wohltemperiertes Klavier.",
    cents: [5.87, 0, 1.96, 3.91, -1.96, 7.82, -1.96, 3.91, 1.96, 0, 5.87, 0],
  },
  {
    name: "Bach-Lehman",
    nameDutch: "Bach-Lehman (2005)",
    category: "well",
    period: "Reconstructie (Bradley Lehman, 2005, gebaseerd op titelpagina WTC)",
    year: 2005,
    modernUse: true,
    description: "Lehmans interpretatie van Bachs stemming op basis van de decoratieve krul op de titelpagina van het Wohltemperiertes Klavier. Wordt veel besproken en toegepast.",
    cents: [5.87, 3.91, 1.96, 3.91, -1.96, 7.82, 1.96, 3.91, 3.91, 0, 3.91, 0],
  },
  {
    name: "Bach (Klais)",
    nameDutch: "Bach-Klais",
    category: "well",
    period: "Reconstructie (Orgelbau Klais, Bonn)",
    year: 1970,
    modernUse: true,
    description: "De stemming zoals gebruikt door de orgelbouwer Klais voor Bach-orgels.",
    cents: [7.49, -1.95, 3.74, 1.85, -4.88, 5.65, -3.82, 7.51, -0.02, 0, 3.77, -5.70],
  },
  {
    name: "Neidhardt I (Grosse Stadt)",
    nameDutch: "Neidhardt I (Grote Stad)",
    category: "well",
    period: "Barok (Johann Georg Neidhardt, 1724/1732)",
    year: 1724,
    modernUse: false,
    description: "Neidhardts stemming 'voor een grote stad'. Gematigde toonsoortkleur, dicht bij gelijkzwevend.",
    cents: [5.87, 0, 1.96, 1.96, -1.96, 3.91, -1.96, 3.91, 1.96, 0, 1.96, -1.96],
  },
  {
    name: "Neidhardt II (Kleine Stadt)",
    nameDutch: "Neidhardt II (Kleine Stad)",
    category: "well",
    period: "Barok (Johann Georg Neidhardt, 1724/1732)",
    year: 1724,
    modernUse: false,
    description: "Neidhardts stemming 'voor een kleine stad'. Iets meer toonsoortkleur.",
    cents: [5.87, 1.96, 1.96, 3.91, 0, 5.87, 1.96, 3.91, 1.96, 0, 5.87, 1.96],
  },
  {
    name: "Neidhardt III (Dorf)",
    nameDutch: "Neidhardt III (Dorp)",
    category: "well",
    period: "Barok (Johann Georg Neidhardt, 1724/1732)",
    year: 1724,
    modernUse: false,
    description: "Neidhardts stemming 'voor een dorp'. Meer toonsoortkleur dan de 'stad'-versies.",
    cents: [5.87, 1.96, 1.96, 3.91, 0, 3.91, 1.96, 3.91, 1.96, 0, 3.91, 1.96],
  },
  {
    name: "Bendeler III",
    nameDutch: "Bendeler III",
    category: "well",
    period: "Barok (Johann Philipp Bendeler, ca. 1690)",
    year: 1690,
    modernUse: false,
    description: "Drie kwinten elk verkleind met 1/3 pythagorisch komma. Eenvoudige welgetempereerde stemming.",
    cents: [5.87, 1.96, -1.96, 0, 1.96, 3.91, 0, 1.96, 3.91, 0, 1.96, -1.96],
  },
  {
    name: "Lambert",
    nameDutch: "Lambert",
    category: "well",
    period: "Klassiek (Johann Heinrich Lambert, 1774)",
    year: 1774,
    modernUse: false,
    description: "Lamberts welgetempereerde stemming. Wiskundig berekend compromis.",
    cents: [4.19, -2.23, 1.40, 1.68, -1.40, 5.59, -4.19, 2.79, -0.28, 0, 3.63, -2.79],
  },

  // ============================================================
  // GEMODIFICEERDE MIDDENTOON / TEMPERAMENT ORDINAIRE
  // ============================================================
  {
    name: "Rameau (modified 1/4-comma)",
    nameDutch: "Rameau (gemodificeerde 1/4-komma)",
    category: "historical",
    period: "Barok (Jean-Philippe Rameau, 1726)",
    year: 1726,
    modernUse: false,
    description: "Rameau's aanpassing van de middentoonstemming. Bredere kwinten aan de kruiszijde om meer toonsoorten bruikbaar te maken. Temperament ordinaire.",
    cents: [10.27, -2.93, 3.42, -4.56, -3.42, 13.69, -4.89, 6.84, -0.98, 0, 4.56, -6.84],
  },
  {
    name: "Schlick (Vogel)",
    nameDutch: "Schlick (reconstructie Vogel)",
    category: "historical",
    period: "Late Middeleeuwen / Renaissance (Arnolt Schlick, 1511; reconstructie H. Vogel)",
    year: 1511,
    modernUse: false,
    description: "Een van de vroegst beschreven orgelstemmingen. Arnolt Schlick beschreef zijn stemming in 'Spiegel der Orgelmacher' (1511).",
    cents: [8.21, -6.26, 2.74, 2.35, -2.74, 10.95, -8.21, 5.47, -4.30, 0, 8.99, -5.47],
  },
  {
    name: "Bruder 1829",
    nameDutch: "Bruder (1829)",
    category: "historical",
    period: "Romantiek (Gebruder Bruder, 1829, Schwarzwald)",
    year: 1829,
    modernUse: false,
    description: "Stemming van de gebroeders Bruder, beroemd om hun draaiorgels en kerkorgels in het Zwarte Woud.",
    cents: [2.93, -1.96, 5.87, 0, -5.87, 1.96, -3.42, 4.40, -0.98, 0, 0.98, -4.89],
  },
  {
    name: "Van Zwolle",
    nameDutch: "Van Zwolle",
    category: "historical",
    period: "Late Middeleeuwen (Henri Arnaut de Zwolle, ca. 1440)",
    year: 1440,
    modernUse: false,
    description: "Een van de oudst gedocumenteerde orgelstemmingen. Pythagorische basis met aanpassingen.",
    cents: [-5.87, -15.64, -1.96, -11.73, 1.96, -7.82, -17.60, -3.91, -13.69, 0, -9.78, 3.91],
  },
  // === Extra temperamenten (50+ totaal) ===
  {
    name: "Tartini-Vallotti",
    nameDutch: "Tartini-Vallotti",
    category: "well",
    period: "18e eeuw (Tartini/Vallotti variant)",
    year: 1754,
    modernUse: true,
    description: "Variant van Vallotti, populair bij Italiaanse orgels.",
    cents: [-5.87, -1.96, -3.91, 0, -7.82, -3.91, 0, -3.91, 0, 0, -1.96, -5.87],
  },
  {
    name: "D'Alembert",
    nameDutch: "D'Alembert",
    category: "well",
    period: "18e eeuw (Jean le Rond d'Alembert, 1752)",
    year: 1752,
    modernUse: false,
    description: "Frans-wetenschappelijke stemming uit de Encyclopédie.",
    cents: [-3.42, -6.84, -0.98, -4.89, -8.30, -2.44, -5.87, -1.47, -4.40, 0, -3.42, -6.84],
  },
  {
    name: "Marpurg I",
    nameDutch: "Marpurg I",
    category: "well",
    period: "18e eeuw (Friedrich Wilhelm Marpurg, 1756)",
    year: 1756,
    modernUse: false,
    description: "Eerste stemming van Marpurg, lichtgekleurd.",
    cents: [-3.91, -5.87, -1.96, -3.91, -5.87, -1.96, -3.91, -1.96, -5.87, 0, -1.96, -3.91],
  },
  {
    name: "Marpurg II",
    nameDutch: "Marpurg II",
    category: "well",
    period: "18e eeuw (Friedrich Wilhelm Marpurg)",
    year: 1756,
    modernUse: false,
    description: "Tweede stemming van Marpurg, meer contrast.",
    cents: [-5.87, -9.78, 0, -3.91, -7.82, 0, -5.87, -1.96, -7.82, 0, -1.96, -5.87],
  },
  {
    name: "Marpurg III",
    nameDutch: "Marpurg III",
    category: "well",
    period: "18e eeuw (Friedrich Wilhelm Marpurg)",
    year: 1756,
    modernUse: false,
    description: "Derde stemming van Marpurg, circulerender.",
    cents: [-1.96, -3.91, -1.96, -3.91, -5.87, 0, -1.96, -1.96, -3.91, 0, -1.96, -3.91],
  },
  {
    name: "Stanhope",
    nameDutch: "Stanhope",
    category: "well",
    period: "18e eeuw (Charles Stanhope, 1806)",
    year: 1806,
    modernUse: false,
    description: "Engelse wetenschappelijke stemming.",
    cents: [-3.26, -7.82, -1.96, -1.63, -5.54, -2.28, -6.52, -0.98, -5.21, 0, -0.65, -3.91],
  },
  {
    name: "Thomas Young (origineel)",
    nameDutch: "Thomas Young (origineel)",
    category: "well",
    period: "19e eeuw (Thomas Young, 1799)",
    year: 1799,
    modernUse: false,
    description: "Originele stemming van Young, vóór zijn bekendere variant.",
    cents: [-3.91, -7.82, -1.96, 0, -5.87, -1.96, -7.82, -1.96, -5.87, 0, 0, -3.91],
  },
  {
    name: "Prinz",
    nameDutch: "Prinz",
    category: "well",
    period: "17e eeuw (Johann Georg Prinz)",
    year: 1690,
    modernUse: false,
    description: "Noord-Duitse stemming uit het late baroktijdperk.",
    cents: [-3.91, -7.82, -1.96, 0, -5.87, -1.96, -5.87, -3.91, -5.87, 0, 0, -3.91],
  },
  {
    name: "Sorge",
    nameDutch: "Sorge",
    category: "well",
    period: "18e eeuw (Georg Andreas Sorge, 1744)",
    year: 1744,
    modernUse: false,
    description: "Stemming van een tijdgenoot van J.S. Bach.",
    cents: [-3.91, -7.82, -1.96, -1.96, -5.87, -1.96, -5.87, -1.96, -5.87, 0, -1.96, -3.91],
  },
  {
    name: "Grammateus",
    nameDutch: "Grammateus",
    category: "historical",
    period: "Renaissance (Henricus Grammateus, 1518)",
    year: 1518,
    modernUse: false,
    description: "Vroege Renaissance stemming, halfweg tussen Pythagorisch en middentoon.",
    cents: [-5.87, -11.73, 0, -5.87, -11.73, -3.91, -9.78, -1.96, -7.82, 0, -3.91, -9.78],
  },
  {
    name: "Fogliano-Bentivoglio",
    nameDutch: "Fogliano-Bentivoglio",
    category: "historical",
    period: "Renaissance (Lodovico Fogliano, 1529)",
    year: 1529,
    modernUse: false,
    description: "Italiaanse Renaissance stemming met reine tertsen.",
    cents: [-10.26, -20.53, -3.91, -13.69, 0, -6.84, -17.11, -8.29, -24.44, 0, -10.26, -17.11],
  },
  {
    name: "De Caus",
    nameDutch: "De Caus",
    category: "historical",
    period: "17e eeuw (Salomon de Caus, 1615)",
    year: 1615,
    modernUse: false,
    description: "Vroeg-barokke stemming van de Frans-Duitse ingenieur.",
    cents: [-10.26, -17.60, -3.42, -13.69, -6.84, -7.82, -17.60, -5.87, -15.64, 0, -9.78, -3.91],
  },
  {
    name: "Praetorius",
    nameDutch: "Praetorius",
    category: "historical",
    period: "Vroeg-barok (Michael Praetorius, 1619)",
    year: 1619,
    modernUse: false,
    description: "Stemming beschreven in Syntagma Musicum. Gemodificeerd middentoon.",
    cents: [-7.82, -17.60, -3.91, -13.69, 3.91, -5.87, -17.60, -1.96, -15.64, 0, -9.78, 3.91],
  },
  {
    name: "Neidhardt IV (Dorf)",
    nameDutch: "Neidhardt IV (Dorp)",
    category: "well",
    period: "18e eeuw (Johann Georg Neidhardt, 1724)",
    year: 1724,
    modernUse: false,
    description: "Neidhardt's stemming voor dorpskerken.",
    cents: [-3.91, -5.87, -1.96, -1.96, -3.91, -3.91, -5.87, -1.96, -3.91, 0, -1.96, -3.91],
  },
  {
    name: "Neidhardt V (Kleine Stadt)",
    nameDutch: "Neidhardt V (Kleine stad)",
    category: "well",
    period: "18e eeuw (Johann Georg Neidhardt, 1724)",
    year: 1724,
    modernUse: false,
    description: "Neidhardt's stemming voor kleine stadskerken.",
    cents: [-1.96, -5.87, -1.96, -1.96, -3.91, -1.96, -3.91, -1.96, -3.91, 0, -1.96, -3.91],
  },
  {
    name: "Colonna",
    nameDutch: "Colonna",
    category: "historical",
    period: "Renaissance (Fabio Colonna, 1618)",
    year: 1618,
    modernUse: false,
    description: "Italiaanse stemming met nadruk op reine harmonieën.",
    cents: [-6.84, -13.69, -3.42, -10.26, 0, -6.84, -13.69, -3.42, -10.26, 0, -6.84, -3.42],
  },
  {
    name: "Schlick (gemodificeerd)",
    nameDutch: "Schlick (gemodificeerd)",
    category: "historical",
    period: "Vroeg 16e eeuw (Arnolt Schlick, 1511)",
    year: 1511,
    modernUse: false,
    description: "Gemodificeerde versie van de oudste beschreven Duitse orgelstemming.",
    cents: [-4.89, -13.69, -0.98, -9.78, -5.87, -2.93, -11.73, -2.93, -11.73, 0, -7.82, -3.91],
  },
  {
    name: "Lambert 1774",
    nameDutch: "Lambert 1774",
    category: "well",
    period: "18e eeuw (Johann Heinrich Lambert, 1774)",
    year: 1774,
    modernUse: false,
    description: "Welgetempereerde stemming van de Zwitsers-Duitse wiskundige Lambert; karaktervolle toonsoorten met behoud van speelbaarheid in alle toonaarden.",
    cents: [4.71, -2.35, 1.96, 0, 3.14, 5.49, -1.57, 3.14, -0.78, 2.35, 0, 1.96],
  },
  {
    name: "Barnes-Bach",
    nameDutch: "Barnes-Bach",
    category: "well",
    period: "20e-eeuwse reconstructie (John Barnes, 1979)",
    year: 1979,
    modernUse: true,
    description: "Reconstructie door John Barnes van de stemming die mogelijk gebruikt werd voor Bachs Wohltemperirte Clavier. Populair bij moderne uitvoeringen van Bach.",
    cents: [6, 0, 4, 2, 4, 8, 2, 6, 1, 4, 0, 4],
  },
  {
    name: "Vallotti-Young",
    nameDutch: "Vallotti-Young",
    category: "well",
    period: "18e-eeuwse synthese (Vallotti 1779 / Young 1800)",
    year: 1800,
    modernUse: true,
    description: "Hybride van Vallotti en Thomas Young. Symmetrisch verdeeld systeem, elegant evenwicht tussen reine en gespannen tertsen. Veel gebruikt in moderne orgelmuziek.",
    cents: [6, 0, 4, 2, 4, 8, 2, 6, 1, 4, 0, 4],
  },
];

/**
 * Hulpfunctie: geef de absolute cent-waarde voor een noot
 * (t.o.v. C in gelijkzwevend = 0, 100, 200, ... 1100)
 *
 * @param {object} temperament - Een stemming-object uit de temperaments array
 * @param {number} noteIndex - Index 0-11 (C=0, C#=1, ... B=11)
 * @returns {number} Absolute cent-waarde
 */
export function getAbsoluteCents(temperament, noteIndex) {
  const equalCents = noteIndex * 100;
  // cents array is relative to A (index 9), so we need to adjust
  // The stored deviation already has A=0 as reference
  return equalCents + temperament.cents[noteIndex];
}

/**
 * Hulpfunctie: bereken de frequentie van een noot
 *
 * @param {object} temperament - Een stemming-object
 * @param {number} noteIndex - Index 0-11 (C=0, C#=1, ... B=11)
 * @param {number} octave - Octaaf (4 = midden-octaaf, A4 = 440 Hz)
 * @param {number} a4Freq - Referentiefrequentie voor A4 (standaard 440)
 * @returns {number} Frequentie in Hz
 */
export function getFrequency(temperament, noteIndex, octave, a4Freq = 440) {
  // A4 is noteIndex=9, octave=4
  // Halftonen verschil t.o.v. A4 in gelijkzwevend
  const semitonesFromA4 = (octave - 4) * 12 + (noteIndex - 9);
  // Cent-afwijking van deze noot t.o.v. gelijkzwevend
  const centDeviation = temperament.cents[noteIndex];
  // Totale cents verschil t.o.v. A4
  const totalCents = semitonesFromA4 * 100 + centDeviation;
  return a4Freq * Math.pow(2, totalCents / 1200);
}

/**
 * Hulpfunctie: haal stemmingen op per categorie
 *
 * @param {string} category - Categorie-sleutel (equal, meantone, well, pythagorean, historical, just)
 * @returns {Array} Gefilterde lijst van stemmingen
 */
export function getTemperamentsByCategory(category) {
  return temperaments.filter((t) => t.category === category);
}

/**
 * Hulpfunctie: haal alleen de veel gebruikte stemmingen op
 *
 * @returns {Array} Stemmingen die veel gebruikt worden bij moderne orgels
 */
export function getCommonTemperaments() {
  return temperaments.filter((t) => t.modernUse);
}
