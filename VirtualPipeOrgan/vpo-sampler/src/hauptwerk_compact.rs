//! Gecomprimeerde Hauptwerk-orgeldefinities.
//!
//! Hauptwerk kan een ODF "compacted" wegschrijven; bovenin het bestand staat
//! dan `<Control_FileIsCompacted_AlwaysSetThisToNIfEditingManually>Y</…>`. Elk
//! object wordt daarbij één regel met letter-aliassen in plaats van volledige
//! veldnamen:
//!
//! ```text
//! lang:    <Stop><StopID>1</StopID><Name>Gedekt 8</Name><DivisionID>5</DivisionID>…</Stop>
//! compact: <o><a>1</a><c>5</c><d>19001</d><e>2402</e><f>10</f><b>Gedekt 8</b></o>
//! ```
//!
//! De letters volgen de veldvolgorde van het Hauptwerk-schema (a = eerste veld,
//! b = tweede, …) en velden met hun standaardwaarde worden weggelaten. Die
//! volgorde verschilt echter per formaatversie: in versie 4 is het eerste veld
//! van `EnclosurePipe` de PipeID, in versie 5 de EnclosureID, en `Sample` heeft
//! in versie 5 meer velden dan in versie 4. Een vaste tabel is dus niet genoeg.
//!
//! Daarom leidt deze module de betekenis van de letters af uit het bestand
//! zelf: uit de verwijzingen tussen de objectlijsten (welke letter van een pijp
//! bevat getallen die allemaal voorkomen als RankID?), uit de vorm van de
//! waarden (tekst, bestandsnaam, getal in een bereik) en uit structuur (de
//! MIDI-noot is binnen één rang uniek). De schemavolgorde van versie 4 dient
//! daarbij alleen als voorkeur wanneer meerdere letters even goed passen.
//!
//! Wat eruit komt is een gewone [`FieldMap`] met de lange veldnamen, zodat de
//! rest van de importer niets van het verschil merkt.

use std::collections::{HashMap, HashSet};

use tracing::{info, warn};

/// Veldnaam → waarde, zoals de rest van de importer het verwacht.
pub(crate) type FieldMap = HashMap<String, String>;

/// Objecttypen die de importer leest, in de volgorde waarin ze van elkaar
/// afhangen: eerst de lijsten waar andere naar verwijzen.
pub(crate) const OBJECTTYPEN: &[&str] = &[
    "Division",
    "Keyboard",
    "Switch",
    "Rank",
    "Stop",
    "StopRank",
    "Pipe_SoundEngine01",
    "Pipe_SoundEngine01_Layer",
    "Sample",
    "Pipe_SoundEngine01_AttackSample",
    "Pipe_SoundEngine01_ReleaseSample",
    "Enclosure",
    "EnclosurePipe",
    "KeyAction",
];

/// Staat `Control_FileIsCompacted…` op `Y`? Dan zijn alle objectlijsten ná
/// `_General` in de korte `<o>`-vorm geschreven. (`_General` zelf blijft
/// altijd leesbaar, vandaar dat organaam en bouwer ook zonder deze module
/// doorkwamen.)
pub(crate) fn is_gecomprimeerd(xml: &str) -> bool {
    let kop: &str = &xml[..xml.len().min(20_000)];
    match kop.find("<Control_FileIsCompacted") {
        Some(i) => kop[i..]
            .find('>')
            .map(|j| kop[i + j + 1..].trim_start().starts_with('Y'))
            .unwrap_or(false),
        None => false,
    }
}

/// De rauwe objecten van één lijst: letter → waarde.
fn rauwe_objecten(xml: &str, object_type: &str) -> Vec<FieldMap> {
    let kop = format!("<ObjectList ObjectType=\"{}\">", object_type);
    let start = match xml.find(&kop) {
        Some(i) => i + kop.len(),
        None => return Vec::new(),
    };
    let eind = xml[start..]
        .find("</ObjectList>")
        .map(|i| start + i)
        .unwrap_or(xml.len());
    let body = &xml[start..eind];

    let mut uit = Vec::new();
    let mut pos = 0usize;
    while let Some(s) = body[pos..].find("<o>") {
        let obj_start = pos + s + 3;
        let e = match body[obj_start..].find("</o>") {
            Some(i) => obj_start + i,
            None => break,
        };
        uit.push(velden_van_object(&body[obj_start..e]));
        pos = e + 4;
    }
    uit
}

/// `<a>1</a><b>Gedekt 8</b>` → {a: "1", b: "Gedekt 8"}.
fn velden_van_object(body: &str) -> FieldMap {
    let mut map = FieldMap::new();
    let mut rest = body;
    while let Some(lt) = rest.find('<') {
        let na = &rest[lt + 1..];
        let gt = match na.find('>') {
            Some(j) => j,
            None => break,
        };
        let tag = &na[..gt];
        if tag.starts_with('/') || tag.is_empty() {
            rest = &na[gt + 1..];
            continue;
        }
        // Leeg veld `<a/>`: overslaan, dat is de standaardwaarde.
        if let Some(_naam) = tag.strip_suffix('/') {
            rest = &na[gt + 1..];
            continue;
        }
        let na_tag = &na[gt + 1..];
        let sluit = format!("</{}>", tag);
        match na_tag.find(&sluit) {
            Some(e) => {
                map.insert(tag.to_string(), ontsnap(&na_tag[..e]));
                rest = &na_tag[e + sluit.len()..];
            }
            None => break,
        }
    }
    map
}

/// De vijf XML-entiteiten die Hauptwerk gebruikt, plus `&#xNN;`.
fn ontsnap(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut uit = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        uit.push_str(&rest[..i]);
        let na = &rest[i..];
        let eind = match na.find(';') {
            Some(j) if j <= 10 => j,
            _ => {
                uit.push('&');
                rest = &na[1..];
                continue;
            }
        };
        let ent = &na[1..eind];
        let vervang = match ent {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            _ => ent
                .strip_prefix("#x")
                .and_then(|h| u32::from_str_radix(h, 16).ok())
                .or_else(|| ent.strip_prefix('#').and_then(|d| d.parse().ok()))
                .and_then(char::from_u32),
        };
        match vervang {
            Some(c) => uit.push(c),
            None => uit.push_str(&na[..=eind]),
        }
        rest = &na[eind + 1..];
    }
    uit.push_str(rest);
    uit
}

/// Alle letters die in een lijst voorkomen, in alfabetische volgorde.
fn letters(objs: &[FieldMap]) -> Vec<String> {
    let mut set: HashSet<&str> = HashSet::new();
    for o in objs {
        for k in o.keys() {
            set.insert(k.as_str());
        }
    }
    let mut v: Vec<String> = set.into_iter().map(|s| s.to_string()).collect();
    v.sort();
    v
}

/// Welk deel van de objecten heeft dit veld én voldoet het aan `pred`?
/// (Objecten zónder het veld tellen niet mee: een weggelaten veld is de
/// standaardwaarde, niet een tegenvoorbeeld.)
fn dekking(objs: &[FieldMap], letter: &str, pred: impl Fn(&str) -> bool) -> (usize, usize) {
    let mut aanwezig = 0usize;
    let mut goed = 0usize;
    for o in objs {
        if let Some(v) = o.get(letter) {
            aanwezig += 1;
            if pred(v) {
                goed += 1;
            }
        }
    }
    (goed, aanwezig)
}

/// De letter waarvan de waarden verwijzen naar `doelen` (een verzameling
/// ID's uit een andere lijst). Vereist dat vrijwel elke waarde raak is en dat
/// het veld bij de meeste objecten voorkomt; bij gelijkspel wint de letter die
/// in `voorkeur` het vroegst staat.
fn kies_verwijzing(
    objs: &[FieldMap],
    doelen: &HashSet<u32>,
    uitsluiten: &[&str],
    voorkeur: &[&str],
) -> Option<String> {
    if doelen.is_empty() {
        return None;
    }
    let mut beste: Option<(String, f32, usize)> = None;
    for l in letters(objs) {
        if uitsluiten.contains(&l.as_str()) {
            continue;
        }
        let (goed, aanwezig) = dekking(objs, &l, |v| {
            v.trim().parse::<u32>().map(|n| doelen.contains(&n)).unwrap_or(false)
        });
        if aanwezig == 0 {
            continue;
        }
        let kwaliteit = goed as f32 / aanwezig as f32;
        if kwaliteit < 0.95 || aanwezig * 2 < objs.len() {
            continue;
        }
        let rang = voorkeur.iter().position(|p| *p == l).unwrap_or(usize::MAX);
        let beter = match &beste {
            None => true,
            Some((_, k, r)) => kwaliteit > *k + 0.01 || ((kwaliteit - *k).abs() <= 0.01 && rang < *r),
        };
        if beter {
            beste = Some((l, kwaliteit, rang));
        }
    }
    beste.map(|(l, _, _)| l)
}

/// Ziet deze waarde eruit als vrije tekst (een naam)?
fn is_tekst(v: &str) -> bool {
    let t = v.trim();
    !t.is_empty()
        && t.parse::<f64>().is_err()
        && t != "Y"
        && t != "N"
        && !t.contains('/')
        && !t.contains('\\')
}

/// De letter met de naam van het object.
///
/// Doorslaggevend is niet hoeveel waarden tekst zijn, maar hoeveel
/// VERSCHILLENDE: een naam is per object anders, terwijl een vast opschrift
/// als "1. layer: Pipe tone" bij elke rang hetzelfde staat. Zonder dat
/// onderscheid koos deze functie bij Rotterdam de laagbeschrijving in plaats
/// van de rangnaam. De drempel is bewust ruim, want een enkel register dat
/// puur "16" heet telt niet als tekst.
fn kies_tekst(objs: &[FieldMap], uitsluiten: &[&str], voorkeur: &[&str]) -> Option<String> {
    let mut beste: Option<(String, usize, usize)> = None;
    for l in letters(objs) {
        if uitsluiten.contains(&l.as_str()) {
            continue;
        }
        let (goed, aanwezig) = dekking(objs, &l, |v| is_tekst(v));
        if aanwezig * 2 < objs.len() || goed * 5 < aanwezig * 3 {
            continue;
        }
        let uniek: HashSet<&str> = objs
            .iter()
            .filter_map(|o| o.get(&l))
            .filter(|v| is_tekst(v))
            .map(|v| v.trim())
            .collect();
        let rang = voorkeur.iter().position(|p| *p == l).unwrap_or(usize::MAX);
        let beter = match &beste {
            None => true,
            Some((_, u, r)) => uniek.len() > *u || (uniek.len() == *u && rang < *r),
        };
        if beter {
            beste = Some((l, uniek.len(), rang));
        }
    }
    beste.map(|(l, _, _)| l)
}

/// De letter met een bestandsnaam (een pad of iets met een extensie).
fn kies_bestandsnaam(objs: &[FieldMap]) -> Option<String> {
    letters(objs).into_iter().find(|l| {
        let (goed, aanwezig) = dekking(objs, l, |v| {
            let t = v.trim().to_ascii_lowercase();
            t.ends_with(".wav") || t.ends_with(".wv") || (t.contains('/') && t.len() > 4)
        });
        aanwezig > 0 && goed * 10 >= aanwezig * 9
    })
}

/// De letter waarvan de waarden gehele getallen in `[lo, hi]` zijn.
fn kies_getal(
    objs: &[FieldMap],
    lo: i64,
    hi: i64,
    uitsluiten: &[&str],
    voorkeur: &[&str],
) -> Option<String> {
    let kandidaten: Vec<String> = letters(objs)
        .into_iter()
        .filter(|l| !uitsluiten.contains(&l.as_str()))
        .filter(|l| {
            let (goed, aanwezig) = dekking(objs, l, |v| {
                v.trim().parse::<i64>().map(|n| n >= lo && n <= hi).unwrap_or(false)
            });
            aanwezig > 0 && goed == aanwezig
        })
        .collect();
    voorkeur
        .iter()
        .find(|p| kandidaten.iter().any(|k| k == *p))
        .map(|p| p.to_string())
        .or_else(|| kandidaten.into_iter().next())
}

/// De MIDI-noot van een pijp: binnen één rang is elke noot uniek, en de
/// waarden liggen in het MIDI-bereik. Dat is een veel scherper kenmerk dan
/// "een getal tussen 0 en 127", want dat zijn meerdere velden.
fn kies_midi_noot(pijpen: &[FieldMap], rank_letter: &str) -> Option<String> {
    let mut beste: Option<(String, usize)> = None;
    for l in letters(pijpen) {
        if l == rank_letter || l == "a" {
            continue;
        }
        let mut per_rang: HashMap<&str, Vec<i64>> = HashMap::new();
        let mut buiten_bereik = false;
        for p in pijpen {
            let (Some(r), Some(v)) = (p.get(rank_letter), p.get(&l)) else { continue };
            match v.trim().parse::<i64>() {
                Ok(n) if (0..=127).contains(&n) => per_rang.entry(r.as_str()).or_default().push(n),
                _ => {
                    buiten_bereik = true;
                    break;
                }
            }
        }
        if buiten_bereik || per_rang.is_empty() {
            continue;
        }
        let uniek = per_rang
            .values()
            .filter(|v| {
                let set: HashSet<i64> = v.iter().copied().collect();
                set.len() == v.len()
            })
            .count();
        let totaal: usize = per_rang.values().map(|v| v.len()).sum();
        if uniek * 10 < per_rang.len() * 9 {
            continue;
        }
        if beste.as_ref().map(|(_, t)| totaal > *t).unwrap_or(true) {
            beste = Some((l, totaal));
        }
    }
    beste.map(|(l, _)| l)
}

/// Een veld dat binnen één rang constant is (voetmaat, stemming) en in een
/// plausibel bereik ligt.
fn kies_rangconstante(
    pijpen: &[FieldMap],
    rank_letter: &str,
    lo: i64,
    hi: i64,
    uitsluiten: &[&str],
    voorkeur: &[&str],
) -> Option<String> {
    let kandidaten: Vec<String> = letters(pijpen)
        .into_iter()
        .filter(|l| l != rank_letter && l != "a" && !uitsluiten.contains(&l.as_str()))
        .filter(|l| {
            let mut per_rang: HashMap<&str, HashSet<i64>> = HashMap::new();
            let mut aanwezig = 0usize;
            for p in pijpen {
                let (Some(r), Some(v)) = (p.get(rank_letter), p.get(l)) else { continue };
                match v.trim().parse::<i64>() {
                    Ok(n) if (lo..=hi).contains(&n) => {
                        aanwezig += 1;
                        per_rang.entry(r.as_str()).or_default().insert(n);
                    }
                    _ => return false,
                }
            }
            if aanwezig * 2 < pijpen.len() || per_rang.is_empty() {
                return false;
            }
            let constant = per_rang.values().filter(|s| s.len() == 1).count();
            constant * 10 >= per_rang.len() * 8
        })
        .collect();
    voorkeur
        .iter()
        .find(|p| kandidaten.iter().any(|k| k == *p))
        .map(|p| p.to_string())
        .or_else(|| kandidaten.into_iter().next())
}

/// De letter die naar de alternatieve rang wijst: de "tremmed" opname waar
/// de tremulant het register naartoe zet.
///
/// `kies_verwijzing` helpt hier niet, want dit veld staat maar bij een handvol
/// registers (Rotterdam: 32 van de 201). Het kenmerk is een ander: de rangen
/// waar het naar wijst worden nérgens als gewone rang gebruikt. Dat zijn
/// precies de tremulant-rangen, en geen enkel ander veld doet dat.
fn kies_alternatieve_rang(
    stopranks: &[FieldMap],
    rank_letter: &str,
    rang_ids: &HashSet<u32>,
) -> Option<String> {
    let gewone: HashSet<&str> = stopranks
        .iter()
        .filter_map(|o| o.get(rank_letter))
        .map(|v| v.trim())
        .collect();

    let mut beste: Option<(usize, String)> = None;
    for l in letters(stopranks) {
        if l == rank_letter || l == "a" {
            continue;
        }
        let mut treffers = 0usize;
        let mut goed = true;
        for o in stopranks {
            let Some(v) = o.get(&l).map(|v| v.trim()).filter(|v| !v.is_empty()) else { continue };
            // Een geldig rangnummer, niet de eigen rang, en een rang die
            // nergens gewoon gespeeld wordt.
            let geldig = v
                .parse::<u32>()
                .map(|n| rang_ids.contains(&n))
                .unwrap_or(false);
            if !geldig || gewone.contains(v) {
                goed = false;
                break;
            }
            treffers += 1;
        }
        if goed && treffers > 0 && beste.as_ref().map_or(true, |(t, _)| treffers > *t) {
            beste = Some((treffers, l));
        }
    }
    beste.map(|(_, l)| l)
}

/// Het veld dat zegt tot welke toetsduur een release hoort.
///
/// Een set met een korte, een middellange en een lange release heeft per laag
/// drie van die grenzen, en juist daarin verschillen ze: 167 ms, 335 ms, en
/// een die ontbreekt omdat "geen grens" de standaardwaarde is. Andere
/// getalvelden op een release — de crossfade-lengte, het pakketnummer — zijn
/// binnen één laag vaak juist gelijk. Daar houden we ze uit elkaar.
///
/// Op bereik kiezen werkt hier niet: de crossfade-lengte (62-112) en het
/// vaste `4` vallen in hetzelfde bereik als een grens in milliseconden.
fn kies_releasegrens(
    releases: &[FieldMap],
    laag_letter: &str,
    uitsluiten: &[&str],
) -> Option<String> {
    let mut groepen: HashMap<&str, Vec<&FieldMap>> = HashMap::new();
    for r in releases {
        if let Some(l) = r.get(laag_letter) {
            groepen.entry(l.as_str()).or_default().push(r);
        }
    }
    let meervoudig: Vec<&Vec<&FieldMap>> = groepen.values().filter(|g| g.len() > 1).collect();
    // Eén release per laag: dan is er niets te kiezen en hoort er ook geen
    // grens te zijn.
    if meervoudig.len() < 3 {
        return None;
    }

    let mut beste: Option<(i64, String)> = None;
    for l in letters(releases) {
        if uitsluiten.contains(&l.as_str()) {
            continue;
        }
        let (goed, aanwezig) = dekking(releases, &l, |v| {
            matches!(v.trim().parse::<i64>(), Ok(n) if (1..=600_000).contains(&n))
        });
        if aanwezig == 0 || goed != aanwezig {
            continue;
        }
        let mut score: i64 = 0;
        for groep in &meervoudig {
            let waarden: Vec<&String> = groep.iter().filter_map(|r| r.get(&l)).collect();
            let ontbreekt = groep.len() - waarden.len();
            let uniek: HashSet<&String> = waarden.iter().copied().collect();
            // Hooguit één release zonder waarde (dat is de lange, met de
            // standaardwaarde), en de rest allemaal verschillend.
            if !waarden.is_empty() && ontbreekt <= 1 && uniek.len() == waarden.len() {
                score += 1;
            } else {
                score -= 1;
            }
        }
        // Een ruime meerderheid van de lagen moet kloppen; anders is het een
        // veld dat er toevallig een paar keer op lijkt.
        if score * 2 > meervoudig.len() as i64
            && beste.as_ref().map_or(true, |(b, _)| score > *b)
        {
            beste = Some((score, l));
        }
    }
    beste.map(|(_, l)| l)
}

/// De weggelaten standaardwaarde van de MIDI-noot van een pijp.
///
/// In een gecomprimeerde ODF staat een veld er niet wanneer het zijn
/// standaardwaarde heeft — en die is voor de MIDI-noot niet 0 maar 60, de c'.
/// Zonder dat besef ontbreekt in bijna elke rang precies één pijp: juist de
/// c' midden in het klavier. We leiden de waarde af uit de gaten: rangen met
/// één pijp zónder notenveld horen precies één ontbrekende noot in hun bereik
/// te hebben, en die is overal dezelfde.
fn standaard_midi_noot(
    pijpen: &[FieldMap],
    rank_letter: &str,
    noot_letter: &str,
) -> Option<u32> {
    let mut bekend: HashMap<&str, HashSet<u32>> = HashMap::new();
    let mut zonder: HashMap<&str, usize> = HashMap::new();
    for p in pijpen {
        let Some(r) = p.get(rank_letter).map(|s| s.as_str()) else { continue };
        match p.get(noot_letter).and_then(|v| v.trim().parse::<u32>().ok()) {
            Some(n) => {
                bekend.entry(r).or_default().insert(n);
            }
            None => *zonder.entry(r).or_default() += 1,
        }
    }
    let mut stemmen: HashMap<u32, usize> = HashMap::new();
    for (rang, n_zonder) in &zonder {
        if *n_zonder != 1 {
            continue;
        }
        let Some(noten) = bekend.get(rang) else { continue };
        let (Some(&lo), Some(&hi)) = (noten.iter().min(), noten.iter().max()) else { continue };
        let gaten: Vec<u32> = (lo..=hi).filter(|n| !noten.contains(n)).collect();
        if gaten.len() == 1 {
            *stemmen.entry(gaten[0]).or_default() += 1;
        }
    }
    stemmen
        .into_iter()
        .max_by_key(|(_, n)| *n)
        .filter(|(_, n)| *n >= 3)
        .map(|(waarde, _)| waarde)
}

/// Eerste toets, aantal toetsen en octaafsprong van een StopRank horen bij
/// elkaar: samen bepalen ze welke pijp onder welke toets komt. We kiezen de
/// combinatie waarbij de meeste toetsen een pijp opleveren die in die rang
/// ook echt bestaat.
///
/// Dat is hier geen luxe. In de Rotterdamse set staat een veld dat overal `1`
/// is precies in het bereik van een octaafsprong; een tabel op bereik alleen
/// koos dat veld en verschoof het hele orgel een halve toon omhoog — met
/// samples die bestaan, dus zonder dat er iets zou opvallen behalve de
/// toonhoogte.
fn kies_toetsbereik(
    stopranks: &[FieldMap],
    rank_letter: &str,
    noten_per_rang: &HashMap<u32, HashSet<u32>>,
) -> (Option<String>, Option<String>, Option<String>) {
    let getal_kandidaten = |lo: i64, hi: i64| -> Vec<String> {
        letters(stopranks)
            .into_iter()
            .filter(|l| l != rank_letter && l != "a")
            .filter(|l| {
                let (goed, aanwezig) =
                    dekking(stopranks, l, |v| matches!(v.trim().parse::<i64>(), Ok(n) if (lo..=hi).contains(&n)));
                aanwezig > 0 && goed == aanwezig
            })
            .collect()
    };
    // `None` = het veld ontbreekt en de standaardwaarde geldt.
    let eerste_kand: Vec<Option<String>> = std::iter::once(None)
        .chain(getal_kandidaten(12, 108).into_iter().map(Some))
        .collect();
    let aantal_kand: Vec<Option<String>> = std::iter::once(None)
        .chain(getal_kandidaten(1, 128).into_iter().map(Some))
        .collect();
    let inc_kand: Vec<Option<String>> = std::iter::once(None)
        .chain(getal_kandidaten(-48, 48).into_iter().map(Some))
        .collect();

    let haal = |o: &FieldMap, l: &Option<String>| -> Option<i64> {
        l.as_ref().and_then(|l| o.get(l)).and_then(|v| v.trim().parse::<i64>().ok())
    };

    let mut beste: Option<(i64, usize, Option<String>, Option<String>, Option<String>)> = None;
    for eerste in &eerste_kand {
        for aantal in &aantal_kand {
            if aantal.is_some() && aantal == eerste {
                continue;
            }
            for inc in &inc_kand {
                if inc.is_some() && (inc == eerste || inc == aantal) {
                    continue;
                }
                let mut score: i64 = 0;
                for sr in stopranks {
                    let Some(rid) = sr.get(rank_letter).and_then(|v| v.trim().parse::<u32>().ok()) else { continue };
                    let Some(noten) = noten_per_rang.get(&rid) else { continue };
                    let f = haal(sr, eerste).unwrap_or(EERSTE_TOETS_STANDAARD);
                    let i = haal(sr, inc).unwrap_or(0);
                    // Zonder expliciet aantal beslaat de StopRank de hele rang.
                    let n = haal(sr, aantal)
                        .unwrap_or_else(|| hele_rang(noten, f, i));
                    for k in 0..n {
                        let noot = f + k + i;
                        if (0..=127).contains(&noot) && noten.contains(&(noot as u32)) {
                            score += 1;
                        } else {
                            score -= 1;
                        }
                    }
                }
                // Bij gelijke dekking wint de eenvoudigste verklaring: geen
                // octaafsprong en geen expliciete eerste toets.
                let eenvoud = usize::from(inc.is_none()) + usize::from(eerste.is_none());
                let beter = match &beste {
                    None => true,
                    Some((s0, e0, _, _, _)) => score > *s0 || (score == *s0 && eenvoud > *e0),
                };
                if beter {
                    beste = Some((score, eenvoud, eerste.clone(), aantal.clone(), inc.clone()));
                }
            }
        }
    }
    match beste {
        Some((_, _, e, a, i)) => (e, a, i),
        None => (None, None, None),
    }
}

/// De standaard-eerste toets van een StopRank: C groot (MIDI 36).
pub(crate) const EERSTE_TOETS_STANDAARD: i64 = 36;

/// Hoeveel toetsen een StopRank zonder expliciet aantal beslaat: tot en met
/// de hoogste pijp van zijn rang.
pub(crate) fn hele_rang(noten: &HashSet<u32>, eerste: i64, inc: i64) -> i64 {
    match noten.iter().max() {
        Some(&hoogste) => (hoogste as i64 - inc - eerste + 1).max(0),
        None => 0,
    }
}

/// De ID's uit de eerste kolom van een lijst (`a` is altijd het eerste veld
/// van de sleutel).
fn id_verzameling(objs: &[FieldMap]) -> HashSet<u32> {
    objs.iter()
        .filter_map(|o| o.get("a"))
        .filter_map(|v| v.trim().parse::<u32>().ok())
        .collect()
}

/// De vertaaltabel van één gecomprimeerde ODF.
pub(crate) struct GecomprimeerdeOdf {
    /// objecttype → (letter → veldnaam)
    tabel: HashMap<String, HashMap<String, &'static str>>,
    /// Welke MIDI-noten elke rang heeft — nodig om een StopRank zonder
    /// expliciet aantal toetsen tot het einde van zijn rang te laten lopen.
    noten_per_rang: HashMap<u32, HashSet<u32>>,
    /// Velden die bij het inlezen aangevuld worden als ze ontbreken, omdat
    /// een weggelaten veld zijn standaardwaarde heeft: objecttype →
    /// (veldnaam, waarde).
    standaarden: HashMap<String, Vec<(&'static str, String)>>,
}

impl GecomprimeerdeOdf {
    /// Leidt de tabel af uit het bestand. Geeft `None` terug wanneer de ODF
    /// niet gecomprimeerd is (dan werkt de gewone lange-vorm-lezer).
    pub fn detecteer(xml: &str) -> Option<Self> {
        if !is_gecomprimeerd(xml) {
            return None;
        }
        let rauw: HashMap<&str, Vec<FieldMap>> = OBJECTTYPEN
            .iter()
            .map(|t| (*t, rauwe_objecten(xml, t)))
            .collect();
        let leeg: Vec<FieldMap> = Vec::new();
        let lijst = |t: &str| rauw.get(t).unwrap_or(&leeg);

        let div_ids = id_verzameling(lijst("Division"));
        let kbd_ids = id_verzameling(lijst("Keyboard"));
        let switch_ids = id_verzameling(lijst("Switch"));
        let rank_ids = id_verzameling(lijst("Rank"));
        let pipe_ids = id_verzameling(lijst("Pipe_SoundEngine01"));
        let layer_ids = id_verzameling(lijst("Pipe_SoundEngine01_Layer"));
        let sample_ids = id_verzameling(lijst("Sample"));
        let enc_ids = id_verzameling(lijst("Enclosure"));

        let mut tabel: HashMap<String, HashMap<String, &'static str>> = HashMap::new();
        let mut standaarden: HashMap<String, Vec<(&'static str, String)>> = HashMap::new();
        let mut zet = |typ: &str, paren: Vec<(Option<String>, &'static str)>| {
            let map: HashMap<String, &'static str> = paren
                .into_iter()
                .filter_map(|(l, naam)| l.map(|l| (l, naam)))
                .collect();
            tabel.insert(typ.to_string(), map);
        };

        // ── Division: a = ID, de naam is het enige tekstveld ──
        let d = lijst("Division");
        zet(
            "Division",
            vec![
                (Some("a".into()), "DivisionID"),
                (kies_tekst(d, &["a"], &["b"]), "Name"),
            ],
        );

        // ── Stop: a = ID, naam, en de verwijzing naar de divisie ──
        let st = lijst("Stop");
        zet(
            "Stop",
            vec![
                (Some("a".into()), "StopID"),
                (kies_tekst(st, &["a"], &["b"]), "Name"),
                (kies_verwijzing(st, &div_ids, &["a"], &["c"]), "DivisionID"),
            ],
        );

        // ── Rank: a = ID, naam ──
        let rk = lijst("Rank");
        zet(
            "Rank",
            vec![
                (Some("a".into()), "RankID"),
                (kies_tekst(rk, &["a"], &["b"]), "Name"),
            ],
        );

        // ── Pijp: a = ID, de rang, de MIDI-noot en de voetmaat van de rang ──
        let pp = lijst("Pipe_SoundEngine01");
        let pp_rank = kies_verwijzing(pp, &rank_ids, &["a"], &["b"]);
        let pp_noot = pp_rank.as_deref().and_then(|r| kies_midi_noot(pp, r));
        let mut pp_uit: Vec<&str> = vec!["a"];
        if let Some(l) = &pp_rank {
            pp_uit.push(l.as_str());
        }
        if let Some(l) = &pp_noot {
            pp_uit.push(l.as_str());
        }
        // De 64-voets harmonische: 8' = 8, 4' = 16, 2' = 32, 2⅔' = 24 — per
        // rang constant, en zelden boven een paar honderd.
        let pp_voet = pp_rank
            .as_deref()
            .and_then(|r| kies_rangconstante(pp, r, 1, 512, &pp_uit, &["f"]));
        zet(
            "Pipe_SoundEngine01",
            vec![
                (Some("a".into()), "PipeID"),
                (pp_rank.clone(), "RankID"),
                (pp_noot.clone(), "NormalMIDINoteNumber"),
                (pp_voet.clone(), "Pitch_Tempered_RankBasePitch64ftHarmonicNum"),
            ],
        );

        // ── StopRank: a = StopID; de rang, en het toetsbereik dat we tegen de
        //    zojuist herkende pijpen valideren ──
        let sr = lijst("StopRank");
        let sr_rank = kies_verwijzing(sr, &rank_ids, &["a"], &["d", "e"]);
        // De MIDI-noot heeft een standaardwaarde die niet 0 is (zie
        // `standaard_midi_noot`). Die moet hiervóór bekend zijn: de rangen
        // waarvan we het toetsbereik afleiden missen anders precies die ene
        // pijp, en dan valt de laatste toets van een bashelft eraf. Bij de
        // Chamade van de Laurenskerk was dat juist de centrale c.
        let standaard_noot = match (&pp_rank, &pp_noot) {
            (Some(rl), Some(nl)) => standaard_midi_noot(lijst("Pipe_SoundEngine01"), rl, nl),
            _ => None,
        };
        if let Some(v) = standaard_noot {
            info!("Hauptwerk: weggelaten MIDI-noot betekent {} (de standaardwaarde)", v);
            standaarden.insert(
                "Pipe_SoundEngine01".to_string(),
                vec![("NormalMIDINoteNumber", v.to_string())],
            );
        }

        let mut noten_per_rang: HashMap<u32, HashSet<u32>> = HashMap::new();
        if let (Some(rl), Some(nl)) = (&pp_rank, &pp_noot) {
            for p in lijst("Pipe_SoundEngine01") {
                let Some(r) = p.get(rl).and_then(|v| v.trim().parse::<u32>().ok()) else {
                    continue;
                };
                let noot = p
                    .get(nl)
                    .and_then(|v| v.trim().parse::<u32>().ok())
                    .or(standaard_noot);
                if let Some(n) = noot {
                    noten_per_rang.entry(r).or_default().insert(n);
                }
            }
        }
        let (sr_eerste, sr_aantal, sr_inc) = match &sr_rank {
            Some(rl) => kies_toetsbereik(sr, rl, &noten_per_rang),
            None => (None, None, None),
        };
        let mut sr_uit: Vec<&str> = vec!["a"];
        for l in [&sr_rank, &sr_eerste, &sr_aantal, &sr_inc].into_iter().flatten() {
            sr_uit.push(l.as_str());
        }
        // De tremmed rang waar de tremulant het register naartoe zet. Staat
        // maar bij een handvol registers, dus die wordt apart herkend.
        let sr_alt = match &sr_rank {
            Some(rl) => kies_alternatieve_rang(sr, rl, &rank_ids),
            None => None,
        };
        if let Some(l) = &sr_alt {
            info!("Hauptwerk: letter '{}' van StopRank is de tremulant-rang", l);
        }
        zet(
            "StopRank",
            vec![
                (Some("a".into()), "StopID"),
                (kies_tekst(sr, &sr_uit, &["b"]), "Name"),
                (sr_rank.clone(), "RankID"),
                (sr_eerste.clone(), "MIDINoteNumOfFirstMappedDivisionInputNode"),
                (sr_aantal.clone(), "NumberOfMappedDivisionInputNodes"),
                (sr_inc.clone(), "MIDINoteNumIncrementFromDivisionToRank"),
                (sr_alt.clone(), "AlternateRankID"),
            ],
        );

        // ── Laag: a = ID, de pijp, en het laagnummer (1 = hoofdlaag) ──
        let ly = lijst("Pipe_SoundEngine01_Layer");
        let ly_pipe = kies_verwijzing(ly, &pipe_ids, &["a"], &["b"]);
        let mut ly_uit: Vec<&str> = vec!["a"];
        if let Some(l) = &ly_pipe {
            ly_uit.push(l.as_str());
        }
        zet(
            "Pipe_SoundEngine01_Layer",
            vec![
                (Some("a".into()), "LayerID"),
                (ly_pipe.clone(), "PipeID"),
                (kies_getal(ly, 1, 8, &ly_uit, &["c"]), "PipeLayerNumber"),
            ],
        );

        // ── Sample: a = ID, de bestandsnaam en het installatiepakket ──
        let sm = lijst("Sample");
        let sm_bestand = kies_bestandsnaam(sm);
        let mut sm_uit: Vec<&str> = vec!["a"];
        if let Some(l) = &sm_bestand {
            sm_uit.push(l.as_str());
        }
        zet(
            "Sample",
            vec![
                (Some("a".into()), "SampleID"),
                (sm_bestand.clone(), "SampleFilename"),
                (kies_getal(sm, 1, 999_999, &sm_uit, &["b"]), "InstallationPackageID"),
            ],
        );

        // ── Attack- en release-samples: laag → sample ──
        // `a` is hier een eigen volgnummer dat toevallig ook een geldige
        // sample-ID kan zijn; daarom sluiten we die uit.
        for (typ, extra) in [
            ("Pipe_SoundEngine01_AttackSample", false),
            ("Pipe_SoundEngine01_ReleaseSample", true),
        ] {
            let ls = lijst(typ);
            let l_layer = kies_verwijzing(ls, &layer_ids, &["a"], &["b"]);
            let mut uit: Vec<&str> = vec!["a"];
            if let Some(l) = &l_layer {
                uit.push(l.as_str());
            }
            let l_sample = kies_verwijzing(ls, &sample_ids, &uit, &["c"]);
            let mut paren = vec![(l_layer.clone(), "LayerID"), (l_sample.clone(), "SampleID")];
            if extra {
                if let Some(l) = &l_sample {
                    uit.push(l.as_str());
                }
                // Tot welke toetsduur deze release hoort. Zonder dit veld
                // pakt de sampler bij elke toetsduur dezelfde release en
                // vervalt het verschil tussen een korte tik en een lange
                // noot.
                let duur = l_layer
                    .as_deref()
                    .and_then(|laag| kies_releasegrens(ls, laag, &uit));
                if duur.is_none() {
                    info!(
                        "Hauptwerk: geen releasegrens herkend op {} — alle releases gelden voor elke toetsduur",
                        typ
                    );
                }
                paren.push((duur, "ReleaseSelCriteria_LatestKeyReleaseTimeMs"));
            }
            zet(typ, paren);
        }

        // ── Zwelkast: a = ID, naam; de pijpenlijst verwijst beide kanten op ──
        let en = lijst("Enclosure");
        zet(
            "Enclosure",
            vec![
                (Some("a".into()), "EnclosureID"),
                (kies_tekst(en, &["a"], &["b"]), "Name"),
            ],
        );
        let ep = lijst("EnclosurePipe");
        // In versie 4 staat de pijp voorop, in versie 5 de kast: welke van de
        // twee het is, blijkt uit de ID's zelf.
        let ep_enc = kies_verwijzing(ep, &enc_ids, &[], &["b", "a"]);
        let mut ep_uit: Vec<&str> = Vec::new();
        if let Some(l) = &ep_enc {
            ep_uit.push(l.as_str());
        }
        zet(
            "EnclosurePipe",
            vec![
                (ep_enc.clone(), "EnclosureID"),
                (kies_verwijzing(ep, &pipe_ids, &ep_uit, &["a", "b"]), "PipeID"),
            ],
        );

        // ── Koppels: klavier → divisie, met de schakelaar die hem aanzet ──
        let ka = lijst("KeyAction");
        let ka_div = kies_verwijzing(ka, &div_ids, &["a"], &["c", "d"]);
        let mut ka_uit: Vec<&str> = vec!["a"];
        if let Some(l) = &ka_div {
            ka_uit.push(l.as_str());
        }
        zet(
            "KeyAction",
            vec![
                (
                    kies_verwijzing(ka, &kbd_ids, &[], &["a"]).or(Some("a".into())),
                    "SourceKeyboardID",
                ),
                (ka_div.clone(), "DestDivisionID"),
                (kies_tekst(ka, &ka_uit, &["d", "e"]), "Name"),
                (kies_verwijzing(ka, &switch_ids, &ka_uit, &["e", "f"]), "ConditionSwitchID"),
            ],
        );

        let odf = GecomprimeerdeOdf { tabel, noten_per_rang, standaarden };
        odf.log_samenvatting(&rauw);
        Some(odf)
    }

    fn log_samenvatting(&self, rauw: &HashMap<&str, Vec<FieldMap>>) {
        let mut onvolledig: Vec<String> = Vec::new();
        for (typ, verwacht) in [
            ("Stop", &["StopID", "Name", "DivisionID"][..]),
            ("StopRank", &["StopID", "RankID"][..]),
            ("Pipe_SoundEngine01", &["PipeID", "RankID", "NormalMIDINoteNumber"][..]),
            ("Pipe_SoundEngine01_Layer", &["LayerID", "PipeID"][..]),
            ("Pipe_SoundEngine01_AttackSample", &["LayerID", "SampleID"][..]),
            ("Sample", &["SampleID", "SampleFilename"][..]),
        ] {
            if rauw.get(typ).map(|v| v.is_empty()).unwrap_or(true) {
                continue;
            }
            let gevonden = self.tabel.get(typ);
            for v in verwacht {
                let ok = gevonden
                    .map(|m| m.values().any(|n| n == v))
                    .unwrap_or(false);
                if !ok {
                    onvolledig.push(format!("{}.{}", typ, v));
                }
            }
        }
        if onvolledig.is_empty() {
            info!("Hauptwerk: gecomprimeerde orgeldefinitie, alle sleutelvelden herkend");
        } else {
            warn!(
                "Hauptwerk: gecomprimeerde orgeldefinitie, deze velden zijn niet herkend: {}",
                onvolledig.join(", ")
            );
        }
    }

    /// Leest één objectlijst en vertaalt de letters naar de lange veldnamen.
    /// Letters zonder betekenis blijven staan (ze doen geen kwaad en helpen
    /// bij het uitzoeken van een nieuwe formaatversie).
    pub(crate) fn objecten(&self, xml: &str, object_type: &str) -> Vec<FieldMap> {
        let vertaal = self.tabel.get(object_type);
        let vul_aan = self.standaarden.get(object_type);
        rauwe_objecten(xml, object_type)
            .into_iter()
            .map(|o| {
                let mut uit: FieldMap = match vertaal {
                    None => o,
                    Some(t) => o
                        .into_iter()
                        .map(|(k, v)| match t.get(&k) {
                            Some(naam) => (naam.to_string(), v),
                            None => (k, v),
                        })
                        .collect(),
                };
                if let Some(paren) = vul_aan {
                    for (naam, waarde) in paren {
                        uit.entry(naam.to_string()).or_insert_with(|| waarde.clone());
                    }
                }
                // Een StopRank zonder expliciet aantal toetsen loopt tot het
                // einde van zijn rang. Dat aantal staat alleen bij de
                // uitzonderingen in het bestand (St. Anne's noteert het bij 8
                // van de 30); zonder deze aanvulling krijgt de rest nul
                // toetsen en blijft het hele orgel leeg.
                if object_type == "StopRank"
                    && !uit.contains_key("NumberOfMappedDivisionInputNodes")
                {
                    if let Some(noten) = uit
                        .get("RankID")
                        .and_then(|v| v.trim().parse::<u32>().ok())
                        .and_then(|r| self.noten_per_rang.get(&r))
                    {
                        let eerste = uit
                            .get("MIDINoteNumOfFirstMappedDivisionInputNode")
                            .and_then(|v| v.trim().parse::<i64>().ok())
                            .unwrap_or(EERSTE_TOETS_STANDAARD);
                        let inc = uit
                            .get("MIDINoteNumIncrementFromDivisionToRank")
                            .and_then(|v| v.trim().parse::<i64>().ok())
                            .unwrap_or(0);
                        let n = hele_rang(noten, eerste, inc);
                        if n > 0 {
                            uit.insert(
                                "NumberOfMappedDivisionInputNodes".to_string(),
                                n.to_string(),
                            );
                        }
                    }
                }
                uit
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Bouwt een gecomprimeerde ODF zoals versie 5 die schrijft: objecten als
    /// `<o>` met letter-aliassen, de naam achteraan, en de zwelkast vóór de
    /// pijp (andersom dan versie 4).
    ///
    /// `noten` zijn de MIDI-noten van de rang; `aantal_veld` zet desgewenst
    /// een expliciet aantal toetsen op de StopRank.
    fn compacte_odf(noten: &[u32], aantal_veld: Option<u32>) -> String {
        // Drie rangen: het afleiden van een weggelaten standaardwaarde vraagt
        // meer dan één voorbeeld, anders is een gat in één rang net zo goed
        // een pijp die er werkelijk niet is.
        const RANGEN: [u32; 3] = [904, 905, 906];
        let mut xml = String::new();
        xml.push_str(r#"<?xml version="1.0" encoding="UTF-8"?>"#);
        xml.push_str(r#"<Hauptwerk FileFormat="Organ" FileFormatVersion="5.00300">"#);
        xml.push_str("<ObjectList ObjectType=\"_General\"><_General>");
        xml.push_str("<Identification_Name>Test</Identification_Name>");
        xml.push_str("<Control_FileIsCompacted_AlwaysSetThisToNIfEditingManually>Y</Control_FileIsCompacted_AlwaysSetThisToNIfEditingManually>");
        xml.push_str("</_General></ObjectList>");
        xml.push_str(r#"<ObjectList ObjectType="Division"><o><a>5</a><b>Borstwerk</b></o></ObjectList>"#);

        xml.push_str(r#"<ObjectList ObjectType="Rank">"#);
        for (i, r) in RANGEN.iter().enumerate() {
            xml.push_str(&format!(
                "<o><a>{}</a><b>Regaal {} (rear)</b><c>1. layer: Pipe tone</c></o>",
                r,
                i + 1
            ));
        }
        xml.push_str("</ObjectList>");

        xml.push_str(r#"<ObjectList ObjectType="Stop">"#);
        for (i, _) in RANGEN.iter().enumerate() {
            xml.push_str(&format!(
                "<o><a>{}</a><c>5</c><d>1900{}</d><b>Regaal {}</b></o>",
                90 + i,
                i,
                i + 1
            ));
        }
        xml.push_str("</ObjectList>");

        xml.push_str(r#"<ObjectList ObjectType="StopRank">"#);
        for (i, r) in RANGEN.iter().enumerate() {
            xml.push_str(&format!("<o><a>{}</a><d>{}</d><f>1</f><g>1</g>", 90 + i, r));
            if let Some(n) = aantal_veld {
                xml.push_str(&format!("<i>{}</i>", n));
            }
            xml.push_str(&format!("<b>Regaal {} (rear)</b></o>", i + 1));
        }
        xml.push_str("</ObjectList>");

        xml.push_str(r#"<ObjectList ObjectType="Pipe_SoundEngine01">"#);
        for r in RANGEN.iter() {
            for n in noten {
                // Noot 60 is de standaardwaarde en wordt daarom weggelaten —
                // precies zoals Hauptwerk het schrijft.
                let d = if *n == 60 { String::new() } else { format!("<d>{}</d>", n) };
                xml.push_str(&format!("<o><a>{}{}</a><b>{}</b>{}<f>8</f></o>", r, n, r, d));
            }
        }
        xml.push_str("</ObjectList>");

        xml.push_str(r#"<ObjectList ObjectType="Pipe_SoundEngine01_Layer">"#);
        for r in RANGEN.iter() {
            for n in noten {
                xml.push_str(&format!("<o><a>{}{}0</a><b>{}{}</b></o>", r, n, r, n));
            }
        }
        xml.push_str("</ObjectList>");

        xml.push_str(r#"<ObjectList ObjectType="Sample">"#);
        for r in RANGEN.iter() {
            for n in noten {
                xml.push_str(&format!(
                    "<o><a>7{}{}</a><b>899</b><d>2</d><c>BorstRe/Re_regal{}/{:03}.wav</c></o>",
                    r, n, r, n
                ));
            }
        }
        xml.push_str("</ObjectList>");

        xml.push_str(r#"<ObjectList ObjectType="Pipe_SoundEngine01_AttackSample">"#);
        let mut volgnummer = 1;
        for r in RANGEN.iter() {
            for n in noten {
                xml.push_str(&format!(
                    "<o><a>{}</a><b>{}{}0</b><c>7{}{}</c></o>",
                    volgnummer, r, n, r, n
                ));
                volgnummer += 1;
            }
        }
        xml.push_str("</ObjectList>");

        xml.push_str(r#"<ObjectList ObjectType="Enclosure"><o><a>998</a><c>998</c><b>Swell</b></o></ObjectList>"#);
        xml.push_str(&format!(
            r#"<ObjectList ObjectType="EnclosurePipe"><o><a>998</a><b>904{}</b></o></ObjectList>"#,
            noten[0]
        ));
        xml.push_str("</Hauptwerk>");
        xml
    }

    /// Een rang van 56 toetsen vanaf C groot, met de c' (60) weggelaten omdat
    /// dat de standaardwaarde is.
    fn volle_rang() -> Vec<u32> {
        (36..=91).collect()
    }

    fn veld(objs: &[FieldMap], i: usize, naam: &str) -> String {
        objs[i].get(naam).cloned().unwrap_or_default()
    }

    #[test]
    fn herkent_een_gecomprimeerd_bestand() {
        let xml = compacte_odf(&volle_rang(), Some(56));
        assert!(is_gecomprimeerd(&xml));
        assert!(!is_gecomprimeerd(
            r#"<Hauptwerk><Control_FileIsCompacted_AlwaysSetThisToNIfEditingManually>N</x>"#
        ));
        assert!(!is_gecomprimeerd(r#"<Hauptwerk FileFormat="Organ"></Hauptwerk>"#));
    }

    #[test]
    fn vertaalt_de_letters_naar_veldnamen() {
        let xml = compacte_odf(&volle_rang(), Some(56));
        let odf = GecomprimeerdeOdf::detecteer(&xml).expect("gecomprimeerd");

        let stops = odf.objecten(&xml, "Stop");
        assert_eq!(veld(&stops, 0, "StopID"), "90");
        assert_eq!(veld(&stops, 0, "Name"), "Regaal 1");
        assert_eq!(veld(&stops, 0, "DivisionID"), "5");

        // De rangnaam is het gevarieerde tekstveld, niet het vaste opschrift
        // "1. layer: Pipe tone" dat bij elke rang hetzelfde staat.
        let ranks = odf.objecten(&xml, "Rank");
        assert_eq!(veld(&ranks, 0, "Name"), "Regaal 1 (rear)");

        let sr = odf.objecten(&xml, "StopRank");
        assert_eq!(veld(&sr, 0, "RankID"), "904");
        assert_eq!(veld(&sr, 0, "NumberOfMappedDivisionInputNodes"), "56");
        // Geen octaafsprong: die staat er niet, dus hij is er ook niet.
        assert_eq!(veld(&sr, 0, "MIDINoteNumIncrementFromDivisionToRank"), "");

        let pijpen = odf.objecten(&xml, "Pipe_SoundEngine01");
        assert_eq!(veld(&pijpen, 0, "PipeID"), "90436");
        assert_eq!(veld(&pijpen, 0, "RankID"), "904");
        assert_eq!(veld(&pijpen, 0, "NormalMIDINoteNumber"), "36");
        assert_eq!(veld(&pijpen, 0, "Pitch_Tempered_RankBasePitch64ftHarmonicNum"), "8");

        let lagen = odf.objecten(&xml, "Pipe_SoundEngine01_Layer");
        assert_eq!(veld(&lagen, 0, "LayerID"), "904360");
        assert_eq!(veld(&lagen, 0, "PipeID"), "90436");

        let att = odf.objecten(&xml, "Pipe_SoundEngine01_AttackSample");
        assert_eq!(veld(&att, 0, "LayerID"), "904360");
        assert_eq!(veld(&att, 0, "SampleID"), "790436");

        let smp = odf.objecten(&xml, "Sample");
        assert_eq!(veld(&smp, 0, "SampleID"), "790436");
        assert_eq!(veld(&smp, 0, "SampleFilename"), "BorstRe/Re_regal904/036.wav");
        assert_eq!(veld(&smp, 0, "InstallationPackageID"), "899");
    }

    /// Een veld dat ontbreekt heeft zijn standaardwaarde, en die is voor de
    /// MIDI-noot 60 — niet 0. Zonder dat mist elke rang precies de c'.
    #[test]
    fn de_weggelaten_middelste_c_komt_terug() {
        let xml = compacte_odf(&volle_rang(), Some(56));
        let odf = GecomprimeerdeOdf::detecteer(&xml).expect("gecomprimeerd");
        let pijpen = odf.objecten(&xml, "Pipe_SoundEngine01");
        let noten: HashSet<u32> = pijpen
            .iter()
            .filter(|p| p.get("RankID").map(|r| r == "904").unwrap_or(false))
            .filter_map(|p| p.get("NormalMIDINoteNumber"))
            .filter_map(|v| v.parse().ok())
            .collect();
        assert_eq!(noten.len(), 56, "alle 56 toetsen horen een noot te hebben");
        assert!(noten.contains(&60), "de weggelaten c' hoort aangevuld te worden");
    }

    /// Staat het aantal toetsen er niet, dan beslaat de StopRank zijn hele
    /// rang. St. Anne's noteert het maar bij 8 van de 30 registers.
    #[test]
    fn zonder_aantal_toetsen_loopt_de_stoprank_tot_het_einde_van_de_rang() {
        let xml = compacte_odf(&volle_rang(), None);
        let odf = GecomprimeerdeOdf::detecteer(&xml).expect("gecomprimeerd");
        let sr = odf.objecten(&xml, "StopRank");
        assert_eq!(veld(&sr, 0, "NumberOfMappedDivisionInputNodes"), "56");
    }

    /// Versie 5 zet de zwelkast vóór de pijp, versie 4 andersom. De kant
    /// waarop het staat mag niet uitmaken.
    #[test]
    fn zwelkast_en_pijp_worden_niet_verwisseld() {
        let xml = compacte_odf(&volle_rang(), Some(56));
        let odf = GecomprimeerdeOdf::detecteer(&xml).expect("gecomprimeerd");
        let ep = odf.objecten(&xml, "EnclosurePipe");
        assert_eq!(veld(&ep, 0, "EnclosureID"), "998");
        assert_eq!(veld(&ep, 0, "PipeID"), "90436");
    }

    /// De grens waarop de sampler de korte, middellange of lange release
    /// kiest. Het onderscheidende kenmerk is niet het getalbereik — de
    /// crossfade-lengte en het vaste pakketnummer vallen er ook in — maar dat
    /// de releases van dezelfde laag er juist in verschillen.
    #[test]
    fn de_releasegrens_wordt_gekozen_op_wat_de_lagen_onderscheidt() {
        // Per laag drie releases: kort, middellang, en een lange zonder grens
        // (die heeft de standaardwaarde en staat er dus niet).
        let mut releases: Vec<FieldMap> = Vec::new();
        for laag in 0..8u32 {
            for (i, grens) in [Some(167), Some(335), None].into_iter().enumerate() {
                let mut r = FieldMap::new();
                r.insert("a".into(), format!("{}{}", laag, i));
                r.insert("b".into(), laag.to_string());
                r.insert("c".into(), format!("7{}{}", laag, i));
                // Vast pakketnummer: binnen een laag overal gelijk.
                r.insert("d".into(), "4".to_string());
                // Crossfade-lengte: bijna gelijk binnen een laag.
                r.insert("n".into(), if i == 1 { "62" } else { "64" }.to_string());
                if let Some(g) = grens {
                    r.insert("q".into(), g.to_string());
                }
                releases.push(r);
            }
        }
        // `e` staat op één enkel object en mag de keuze niet winnen.
        releases[0].insert("e".into(), "3".to_string());

        assert_eq!(
            kies_releasegrens(&releases, "b", &["a", "c"]).as_deref(),
            Some("q")
        );
    }

    /// Eén release per laag: dan valt er niets te kiezen en hoort er geen
    /// grens uit te komen in plaats van een willekeurig getalveld.
    #[test]
    fn zonder_meerdere_releases_per_laag_komt_er_geen_grens() {
        let mut releases: Vec<FieldMap> = Vec::new();
        for laag in 0..8u32 {
            let mut r = FieldMap::new();
            r.insert("a".into(), laag.to_string());
            r.insert("b".into(), laag.to_string());
            r.insert("c".into(), format!("7{}", laag));
            r.insert("n".into(), "64".to_string());
            releases.push(r);
        }
        assert_eq!(kies_releasegrens(&releases, "b", &["a", "c"]), None);
    }

    /// De tremmed rang staat maar bij een handvol registers, dus op dekking
    /// valt hij niet te vinden. Het kenmerk is dat hij wijst naar rangen die
    /// nergens gewoon gespeeld worden.
    #[test]
    fn de_tremulant_rang_wordt_herkend_aan_zijn_eigen_rangen() {
        let rangen: HashSet<u32> = [1u32, 2, 3, 901, 902].into_iter().collect();
        let mut sr: Vec<FieldMap> = Vec::new();
        for (i, (rang, alt)) in [(1u32, Some(901u32)), (2, Some(902)), (3, None)]
            .into_iter()
            .enumerate()
        {
            let mut o = FieldMap::new();
            o.insert("a".into(), (i + 1).to_string());
            o.insert("d".into(), rang.to_string());
            // Een veld dat wél een getal is maar geen rang: mag niet winnen.
            o.insert("f".into(), "7".to_string());
            if let Some(a) = alt {
                o.insert("p".into(), a.to_string());
            }
            sr.push(o);
        }
        assert_eq!(kies_alternatieve_rang(&sr, "d", &rangen).as_deref(), Some("p"));

        // Wijst een veld naar rangen die óók gewoon gespeeld worden, dan is
        // het geen tremulant-rang.
        for o in sr.iter_mut() {
            o.insert("p".into(), "2".to_string());
        }
        assert_eq!(kies_alternatieve_rang(&sr, "d", &rangen), None);
    }

    #[test]
    fn xml_entiteiten_worden_ontsnapt() {
        assert_eq!(ontsnap("Fl&#xFB;te"), "Flûte");
        assert_eq!(ontsnap("a &amp; b"), "a & b");
        assert_eq!(ontsnap("Subba&#xDF;"), "Subbaß");
        assert_eq!(ontsnap("gewoon"), "gewoon");
    }
}

#[cfg(test)]
mod diagnose {
    use super::*;

    /// Toont per objecttype welke letter welk veld werd, voor een echte
    /// gecomprimeerde set. Zet `JM_HW_TEST_ODF` en draai met
    /// `--ignored --nocapture`.
    #[test]
    #[ignore]
    fn toon_aliassen() {
        let Ok(p) = std::env::var("JM_HW_TEST_ODF") else { return };
        let xml = std::fs::read_to_string(&p).expect("lezen");
        println!("gecomprimeerd: {}", is_gecomprimeerd(&xml));
        let Some(odf) = GecomprimeerdeOdf::detecteer(&xml) else { return };
        let mut typen: Vec<&String> = odf.tabel.keys().collect();
        typen.sort();
        for t in typen {
            let m = &odf.tabel[t];
            let mut paren: Vec<(&String, &&str)> = m.iter().collect();
            paren.sort();
            let n = rauwe_objecten(&xml, t).len();
            println!("{:32} ({:6} objecten)  {}", t, n,
                paren.iter().map(|(l, v)| format!("{}={}", l, v)).collect::<Vec<_>>().join("  "));
        }
    }
}
