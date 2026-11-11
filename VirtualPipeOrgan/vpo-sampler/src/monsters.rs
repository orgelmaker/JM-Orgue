//! Compacte opslag van samplewaarden (C2, fase 1).
//!
//! Een opname wordt bij het laden in `f32` ingelezen en bewerkt, precies zoals altijd. Daarna probeert
//! [`Monsters::compacteer_paar`] haar in de bitdiepte van de bron op te slaan: 16-bit (`i16`, de helft) of 24-bit
//! (drie bytes per monster, driekwart). Dat gebeurt **alleen** als elk monster bij het terugrekenen exact dezelfde
//! `f32` oplevert (vergeleken op de bits). Een opname die bij het laden is geresampled, genormaliseerd of gefilterd
//! voldoet daar niet aan en blijft `f32`. Het afspelen leest via [`Bron::at`], dat voor elk monster dezelfde `f32`
//! teruggeeft als vroeger in de reeks stond: de klank is daardoor bit-identiek.
//!
//! De omzetting schrijft in één nieuw blok van precies de goede grootte; het oude `f32`-blok gaat daarna vrij. Met de
//! standaard heap van Windows was dat in de app (veel threads, duizenden blokken van wisselende grootte) zeer traag, met
//! nieuwe blokken en ook met verkleinen ter plaatse (de lading van een set met 11 000 startbuffers duurde 20 tot 28 in
//! plaats van 6,5 s); daarom gebruikt de app mimalloc als geheugenbeheerder (zie `main.rs`).
//!
//! Terugrekenen: 16-bit is `x * 2^-15`, 24-bit `x * 2^-23`. Vermenigvuldigen met een macht van twee is in `f32`
//! exact en gelijk aan de deling `x as f32 / max_val` die de lezers gebruiken (`max_val = 2^(bits-1)`).

/// Schaal van 16-bit naar `f32`: 2^-15.
const SCHAAL_16: f32 = 1.0 / 32768.0;
/// Schaal van 24-bit naar `f32`: 2^-23.
const SCHAAL_24: f32 = 1.0 / 8388608.0;

/// Leesbare reeks samplewaarden voor de mengkern: lengte en de waarde op een positie. De mengkern is generiek
/// over dit kenmerk, zodat er per monster geen keuze tussen soorten nodig is.
pub trait Bron {
    fn n(&self) -> usize;
    fn at(&self, i: usize) -> f32;
}

impl Bron for [f32] {
    #[inline(always)]
    fn n(&self) -> usize {
        self.len()
    }
    #[inline(always)]
    fn at(&self, i: usize) -> f32 {
        self[i]
    }
}

impl Bron for Vec<f32> {
    #[inline(always)]
    fn n(&self) -> usize {
        self.len()
    }
    #[inline(always)]
    fn at(&self, i: usize) -> f32 {
        self[i]
    }
}

impl<const N: usize> Bron for [f32; N] {
    #[inline(always)]
    fn n(&self) -> usize {
        N
    }
    #[inline(always)]
    fn at(&self, i: usize) -> f32 {
        self[i]
    }
}

/// 16-bit-monsters als bron.
#[derive(Clone, Copy)]
pub struct I16Bron<'a>(pub &'a [i16]);

impl Bron for I16Bron<'_> {
    #[inline(always)]
    fn n(&self) -> usize {
        self.0.len()
    }
    #[inline(always)]
    fn at(&self, i: usize) -> f32 {
        self.0[i] as f32 * SCHAAL_16
    }
}

/// 24-bit-monsters (drie bytes per monster, little-endian) als bron. De bytes hebben na het laatste monster minstens één
/// reservebyte, zodat elk monster met één lezing van vier bytes kan worden opgehaald (de mengkern leest per uitvoerframe
/// vier monsters per kanaal; drie losse bytes per monster maakte het mengen merkbaar zwaarder).
#[derive(Clone, Copy)]
pub struct I24Bron<'a>(pub &'a [u8]);

impl Bron for I24Bron<'_> {
    #[inline(always)]
    fn n(&self) -> usize {
        (self.0.len() - 1) / 3
    }
    #[inline(always)]
    fn at(&self, i: usize) -> f32 {
        let j = 3 * i;
        // Eén grenscontrole voor het hele woord; de omzetting naar [u8; 4] valt daarna weg.
        let w = u32::from_le_bytes(self.0[j..j + 4].try_into().unwrap());
        (((w << 8) as i32) >> 8) as f32 * SCHAAL_24
    }
}

/// Verwijzing naar de monsters van één kanaal, per soort.
#[derive(Clone, Copy)]
pub enum BronRef<'a> {
    F32(&'a [f32]),
    I16(I16Bron<'a>),
    I24(I24Bron<'a>),
}

/// De monsters van één kanaal.
#[derive(Debug, Clone, PartialEq)]
pub enum Monsters {
    F32(Vec<f32>),
    /// `len` monsters van 16 bit, opgeslagen in de bytes van `buf` (het oorspronkelijke f32-blok, verkleind).
    I16 { buf: Vec<f32>, len: usize },
    /// `len` monsters van 24 bit (drie bytes per monster, little-endian, met teken) in de bytes van `buf`.
    I24 { buf: Vec<f32>, len: usize },
}

fn als_i16(buf: &[f32], len: usize) -> &[i16] {
    &bytemuck::cast_slice::<f32, i16>(buf)[..len]
}

/// De 24-bit-bytes met de reservebyte erachter (zie [`I24Bron`]).
fn als_i24(buf: &[f32], len: usize) -> &[u8] {
    &bytemuck::cast_slice::<f32, u8>(buf)[..len * 3 + 1]
}

impl Default for Monsters {
    fn default() -> Self {
        Monsters::F32(Vec::new())
    }
}

impl From<Vec<f32>> for Monsters {
    fn from(v: Vec<f32>) -> Self {
        Monsters::F32(v)
    }
}

/// Soort opslag, voor tellingen en tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Soort {
    F32,
    I16,
    I24,
}

impl Monsters {
    pub fn len(&self) -> usize {
        match self {
            Monsters::F32(v) => v.len(),
            Monsters::I16 { len, .. } | Monsters::I24 { len, .. } => *len,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn soort(&self) -> Soort {
        match self {
            Monsters::F32(_) => Soort::F32,
            Monsters::I16 { .. } => Soort::I16,
            Monsters::I24 { .. } => Soort::I24,
        }
    }

    /// Bytes in het geheugen (bij compacte opslag afgerond op hele f32-woorden).
    pub fn bytes(&self) -> usize {
        match self {
            Monsters::F32(v) | Monsters::I16 { buf: v, .. } | Monsters::I24 { buf: v, .. } => v.len() * 4,
        }
    }

    pub fn bron(&self) -> BronRef<'_> {
        match self {
            Monsters::F32(v) => BronRef::F32(v),
            Monsters::I16 { buf, len } => BronRef::I16(I16Bron(als_i16(buf, *len))),
            Monsters::I24 { buf, len } => BronRef::I24(I24Bron(als_i24(buf, *len))),
        }
    }

    /// De waarde op positie `i` als `f32` (buiten de mengkern; daar gaat het via [`Bron`]).
    #[inline]
    pub fn at(&self, i: usize) -> f32 {
        match self {
            Monsters::F32(v) => v[i],
            Monsters::I16 { buf, len } => I16Bron(als_i16(buf, *len)).at(i),
            Monsters::I24 { buf, len } => I24Bron(als_i24(buf, *len)).at(i),
        }
    }

    /// De reeks als `f32`-plak, als hij als `f32` is opgeslagen.
    pub fn als_f32(&self) -> Option<&[f32]> {
        match self {
            Monsters::F32(v) => Some(v),
            _ => None,
        }
    }

    /// De reeks als `f32` (een kopie bij compacte opslag). Alleen buiten de audiothread gebruiken.
    pub fn naar_f32(&self) -> std::borrow::Cow<'_, [f32]> {
        match self {
            Monsters::F32(v) => std::borrow::Cow::Borrowed(v),
            _ => std::borrow::Cow::Owned((0..self.len()).map(|i| self.at(i)).collect()),
        }
    }

    /// Bewerkbare `f32`-reeks. Is de opslag compact, dan wordt die eerst (exact) teruggezet naar `f32`. Alleen bij
    /// het laden gebruiken, nooit in de audiothread.
    pub fn f32_mut(&mut self) -> &mut Vec<f32> {
        if !matches!(self, Monsters::F32(_)) {
            let v: Vec<f32> = (0..self.len()).map(|i| self.at(i)).collect();
            *self = Monsters::F32(v);
        }
        match self {
            Monsters::F32(v) => v,
            _ => unreachable!(),
        }
    }

    /// De reeks als eigen `f32`-vector (zonder kopie als ze al `f32` is).
    pub fn into_f32(self) -> Vec<f32> {
        match self {
            Monsters::F32(v) => v,
            ander => (0..ander.len()).map(|i| ander.at(i)).collect(),
        }
    }

    /// Overtollige capaciteit vrijgeven.
    pub fn shrink_to_fit(&mut self) {
        match self {
            Monsters::F32(v) | Monsters::I16 { buf: v, .. } | Monsters::I24 { buf: v, .. } => v.shrink_to_fit(),
        }
    }

    /// Inkorten tot `n` monsters, in dezelfde soort.
    pub fn truncate(&mut self, n: usize) {
        match self {
            Monsters::F32(v) => v.truncate(n),
            Monsters::I16 { buf, len } => {
                *len = (*len).min(n);
                buf.truncate((*len * 2).div_ceil(4));
            }
            Monsters::I24 { buf, len } => {
                *len = (*len).min(n);
                buf.truncate((*len * 3 + 1).div_ceil(4));
            }
        }
    }

    /// Een deel `[van, tot)` als nieuwe reeks in dezelfde soort.
    pub fn deel(&self, van: usize, tot: usize) -> Monsters {
        match self {
            Monsters::F32(v) => Monsters::F32(v[van..tot].to_vec()),
            ander => {
                // Een (zeldzaam) deel van een compacte reeks: via f32, dan opnieuw compact.
                let mut m = Monsters::F32((van..tot).map(|i| ander.at(i)).collect());
                Monsters::compacteer_paar(&mut m, None);
                m
            }
        }
    }

    /// Past de hele reeks exact in 16-bit? Alleen controleren, zonder geheugen te vragen; zonder takken per monster
    /// (vectoriseerbaar), in blokken zodat een reeks die niet past snel wordt opgegeven.
    fn past_16(v: &[f32]) -> bool {
        v.chunks(4096).all(|blok| {
            blok.iter().fold(true, |ok, &x| {
                let q = (x * 32768.0) as i32;
                ok & (-32768..=32767).contains(&q) & (((q as f32) * SCHAAL_16).to_bits() == x.to_bits())
            })
        })
    }

    /// Idem voor 24-bit.
    fn past_24(v: &[f32]) -> bool {
        v.chunks(4096).all(|blok| {
            blok.iter().fold(true, |ok, &x| {
                let q = (x * 8388608.0) as i32;
                ok & (-8388608..=8388607).contains(&q) & (((q as f32) * SCHAAL_24).to_bits() == x.to_bits())
            })
        })
    }

    /// Omzetten naar 16-bit in één nieuw blok van precies de goede grootte (hele f32-woorden); het oude f32-blok gaat
    /// daarna vrij. Alleen aanroepen als [`Self::past_16`] waar is.
    fn naar_16(v: Vec<f32>) -> Monsters {
        let n = v.len();
        let mut buf = vec![0f32; (n * 2).div_ceil(4)];
        {
            let uit = &mut bytemuck::cast_slice_mut::<f32, i16>(&mut buf[..])[..n];
            for (o, &x) in uit.iter_mut().zip(&v) {
                *o = (x * 32768.0) as i32 as i16;
            }
        }
        Monsters::I16 { buf, len: n }
    }

    /// Idem naar 24-bit (drie bytes per monster, little-endian).
    fn naar_24(v: Vec<f32>) -> Monsters {
        let n = v.len();
        // Minstens één reservebyte na het laatste monster (zie I24Bron).
        let mut buf = vec![0f32; (n * 3 + 1).div_ceil(4)];
        {
            let uit = &mut bytemuck::cast_slice_mut::<f32, u8>(&mut buf[..])[..n * 3];
            for (o, &x) in uit.chunks_exact_mut(3).zip(&v) {
                let b = ((x * 8388608.0) as i32).to_le_bytes();
                o.copy_from_slice(&b[..3]);
            }
        }
        Monsters::I24 { buf, len: n }
    }

    /// Zet links (en rechts) samen om naar de kleinste soort die voor BEIDE kanalen exact is. Lukt dat niet, of is
    /// de opslag al compact, dan verandert er niets. Geeft de soort van links terug.
    pub fn compacteer_paar(links: &mut Monsters, rechts: Option<&mut Monsters>) -> Soort {
        Self::compacteer_paar_tot(links, rechts, Soort::I24)
    }

    /// Als [`Self::compacteer_paar`], maar hoogstens tot de gegeven soort: `F32` = niets omzetten, `I16` = alleen
    /// 16-bit, `I24` = 16- of 24-bit (de instelling "samples in het geheugen").
    pub fn compacteer_paar_tot(links: &mut Monsters, rechts: Option<&mut Monsters>, max: Soort) -> Soort {
        if max == Soort::F32 {
            return links.soort();
        }
        let Monsters::F32(l) = &*links else { return links.soort() };
        if l.is_empty() {
            return Soort::F32;
        }
        let r: Option<&Vec<f32>> = match rechts.as_deref() {
            None => None,
            Some(Monsters::F32(r)) => Some(r),
            // Rechts is al compact en links niet: zo laten.
            Some(_) => return Soort::F32,
        };
        // Eerst controleren welke soort voor beide kanalen past (zonder geheugen te vragen), dan één keer omzetten.
        let soort = if Self::past_16(l) && r.is_none_or(|r| Self::past_16(r)) {
            Soort::I16
        } else if max == Soort::I24 && Self::past_24(l) && r.is_none_or(|r| Self::past_24(r)) {
            Soort::I24
        } else {
            return Soort::F32;
        };
        let neem = |m: &mut Monsters| match std::mem::take(m) {
            Monsters::F32(v) => v,
            _ => unreachable!("alleen f32 wordt omgezet"),
        };
        let lv = neem(links);
        *links = if soort == Soort::I16 { Self::naar_16(lv) } else { Self::naar_24(lv) };
        if let Some(rechts) = rechts {
            let rv = neem(rechts);
            *rechts = if soort == Soort::I16 { Self::naar_16(rv) } else { Self::naar_24(rv) };
        }
        soort
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Zoals de lezers het doen: `x as f32 / max_val`.
    fn van_16(x: i16) -> f32 {
        x as f32 / (1i64 << 15) as f32
    }
    fn van_24(x: i32) -> f32 {
        x as f32 / (1i64 << 23) as f32
    }

    #[test]
    fn zestien_bit_wordt_exact_teruggelezen() {
        let bron: Vec<i16> = vec![0, 1, -1, 32767, -32768, 1234, -4321, 2, -2];
        let f: Vec<f32> = bron.iter().map(|&x| van_16(x)).collect();
        let mut m = Monsters::F32(f.clone());
        assert_eq!(Monsters::compacteer_paar(&mut m, None), Soort::I16);
        assert_eq!(m.bytes(), (bron.len() * 2).div_ceil(4) * 4, "afgerond op hele woorden");
        for (i, &x) in f.iter().enumerate() {
            assert_eq!(m.at(i).to_bits(), x.to_bits(), "monster {i}");
        }
        let b = match m.bron() { BronRef::I16(b) => b, _ => unreachable!() };
        assert_eq!(b.n(), f.len());
    }

    #[test]
    fn vierentwintig_bit_wordt_exact_teruggelezen_ook_met_teken() {
        let bron: Vec<i32> = vec![0, 1, -1, 8388607, -8388608, 123456, -654321, 40000, -40000];
        let f: Vec<f32> = bron.iter().map(|&x| van_24(x)).collect();
        let mut m = Monsters::F32(f.clone());
        assert_eq!(Monsters::compacteer_paar(&mut m, None), Soort::I24);
        assert_eq!(m.bytes(), (bron.len() * 3 + 1).div_ceil(4) * 4, "afgerond op hele woorden, met een reservebyte");
        assert_eq!(m.len(), bron.len());
        for (i, &x) in f.iter().enumerate() {
            assert_eq!(m.at(i).to_bits(), x.to_bits(), "monster {i}");
        }
    }

    #[test]
    fn de_instelling_begrenst_de_soort() {
        let f16: Vec<f32> = [1i16, -2, 3].iter().map(|&x| van_16(x)).collect();
        let f24: Vec<f32> = [100000i32, -3].iter().map(|&x| van_24(x)).collect();
        // f32 = niets omzetten.
        let mut a = Monsters::F32(f16.clone());
        assert_eq!(Monsters::compacteer_paar_tot(&mut a, None, Soort::F32), Soort::F32);
        // Alleen 16-bit: 16-bit gaat om, 24-bit blijft f32.
        let mut b = Monsters::F32(f16.clone());
        assert_eq!(Monsters::compacteer_paar_tot(&mut b, None, Soort::I16), Soort::I16);
        let mut c = Monsters::F32(f24.clone());
        assert_eq!(Monsters::compacteer_paar_tot(&mut c, None, Soort::I16), Soort::F32);
        assert_eq!(c.naar_f32().as_ref(), &f24[..]);
        // 16- en 24-bit.
        let mut d = Monsters::F32(f24.clone());
        assert_eq!(Monsters::compacteer_paar_tot(&mut d, None, Soort::I24), Soort::I24);
    }

    #[test]
    fn bewerkte_waarden_blijven_f32() {
        // Na een gain of resampling zijn de waarden geen veelvoud van 2^-23 meer.
        let mut m = Monsters::F32(vec![0.1, 0.2, -0.3]);
        assert_eq!(Monsters::compacteer_paar(&mut m, None), Soort::F32);
        assert!(matches!(m, Monsters::F32(_)));
        // -0.0 kan uit een bewerking komen; terugrekenen gaf +0.0, dus niet compact maken.
        let mut nul = Monsters::F32(vec![van_16(5), -0.0]);
        assert_eq!(Monsters::compacteer_paar(&mut nul, None), Soort::F32);
        // Leeg: niets te doen.
        let mut leeg = Monsters::F32(vec![]);
        assert_eq!(Monsters::compacteer_paar(&mut leeg, None), Soort::F32);
    }

    #[test]
    fn stereo_kiest_de_soort_die_voor_beide_kanalen_past() {
        let l: Vec<f32> = [1i16, 2, 3].iter().map(|&x| van_16(x)).collect();
        let r24: Vec<f32> = [100000i32, 3, -5].iter().map(|&x| van_24(x)).collect();
        let (mut ml, mut mr) = (Monsters::F32(l.clone()), Monsters::F32(r24.clone()));
        assert_eq!(Monsters::compacteer_paar(&mut ml, Some(&mut mr)), Soort::I24);
        assert_eq!((ml.soort(), mr.soort()), (Soort::I24, Soort::I24));
        for i in 0..3 {
            assert_eq!(ml.at(i).to_bits(), l[i].to_bits());
            assert_eq!(mr.at(i).to_bits(), r24[i].to_bits());
        }
        // Rechts niet exact: beide blijven f32.
        let (mut ml2, mut mr2) = (Monsters::F32(l.clone()), Monsters::F32(vec![0.123, 0.0, 0.0]));
        assert_eq!(Monsters::compacteer_paar(&mut ml2, Some(&mut mr2)), Soort::F32);
        assert_eq!((ml2.soort(), mr2.soort()), (Soort::F32, Soort::F32));
        // Beide 16-bit.
        let (mut a, mut b) = (Monsters::F32(l.clone()), Monsters::F32(l.clone()));
        assert_eq!(Monsters::compacteer_paar(&mut a, Some(&mut b)), Soort::I16);
    }

    #[test]
    fn bewerken_inkorten_en_delen_houden_de_waarden() {
        let f: Vec<f32> = [10i16, -20, 30, -40, 50].iter().map(|&x| van_16(x)).collect();
        let mut m = Monsters::F32(f.clone());
        Monsters::compacteer_paar(&mut m, None);
        assert_eq!(m.deel(1, 4).naar_f32().as_ref(), &f[1..4]);
        m.truncate(3);
        assert_eq!(m.len(), 3);
        assert_eq!(m.naar_f32().as_ref(), &f[..3]);
        // Bewerken zet exact terug naar f32.
        let v = m.f32_mut();
        assert_eq!(v.as_slice(), &f[..3]);
        assert_eq!(m.soort(), Soort::F32);
        let mut m24 = Monsters::F32([7i32, -9].iter().map(|&x| van_24(x)).collect());
        Monsters::compacteer_paar(&mut m24, None);
        assert_eq!(m24.deel(1, 2).at(0).to_bits(), van_24(-9).to_bits());
        assert_eq!(m24.clone().into_f32(), vec![van_24(7), van_24(-9)]);
    }

    #[test]
    fn de_bronnen_voor_de_mengkern_geven_dezelfde_waarden() {
        let f: Vec<f32> = [3i16, -7, 11].iter().map(|&x| van_16(x)).collect();
        let mut m = Monsters::F32(f.clone());
        Monsters::compacteer_paar(&mut m, None);
        match m.bron() {
            BronRef::I16(b) => {
                for i in 0..3 {
                    assert_eq!(b.at(i).to_bits(), f[i].to_bits());
                }
            }
            _ => panic!("verwacht 16-bit"),
        }
        let als: &[f32] = &f;
        assert_eq!(als.at(1).to_bits(), f[1].to_bits());
        assert_eq!(Bron::n(als), 3);
    }
}
