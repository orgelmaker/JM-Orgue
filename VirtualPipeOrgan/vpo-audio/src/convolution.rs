//! Convolutiegalm via FFT: uniforme partitieconvolutie met tijdgespreid rekenwerk.
//!
//! Zie de toelichting bij [`ConvolutionReverb`] voor de werking, de reden van
//! de spreiding en de latentie op het natte pad.

use num_complex::Complex;
use realfft::{ComplexToReal, RealFftPlanner, RealToComplex};
use std::sync::Arc;

/// Partitieconvolutie-galm (mono) met gelijkmatig verdeeld rekenwerk.
///
/// # Werking
/// De impulsrespons (IR) wordt in `N` partities van `P = partition_size`
/// samples geknipt; elke partitie wordt (met nullen aangevuld tot `2·P`) naar
/// het frequentiedomein getransformeerd. Van de invoer wordt telkens een blok
/// van `P` samples verzameld en op dezelfde manier getransformeerd; de spectra
/// van de laatste `N` invoerblokken staan in een ringbuffer. Het spectrum van
/// uitvoerblok b is `Σ_k X[b-k]·H[k]` (één complexe MAC per IR-partitie),
/// terug naar het tijddomein via één IFFT en overlap-add van de tweede helft.
///
/// # Waarom tijdgespreid
/// De vorige versie deed op elke partitiegrens ál het werk in de ene
/// sample-aanroep waarin de laatste sample van het blok binnenkwam: FFT, dan
/// `N` complexe MACs over `P+1` bins, dan IFFT. Bij een IR van 6 s op 48 kHz
/// (P = 1024 → N = 282) is dat ~0,5 ms in één audio-callback, 47× per seconde.
/// Bij ASIO met 32 frames (0,67 ms per blok) is dat een gegarandeerde
/// overschrijding: continu kraken zodra convolutie met een eigen IR aanstaat;
/// bij 128 frames een periodieke piek.
///
/// Nu gebeurt op de partitiegrens alleen de FFT van het nieuwe invoerblok en
/// het opschuiven van de ringbuffer. De `N` MACs worden in gelijke porties van
/// [`macs_per_call`](Self::macs_per_call) partities over de eerstvolgende
/// `P-1` sample-aanroepen verdeeld; zodra de laatste portie klaar is (uiterlijk
/// in aanroep `P-2` van het blok, dus ruim vóór de volgende grens) volgt de
/// IFFT + overlap-add. Het maximum per sample-aanroep is daarmee
/// `max(FFT, IFFT + portie)` in plaats van `FFT + N·MAC + IFFT`: voor P = 256
/// een paar microseconden in plaats van een halve milliseconde. Het totale
/// rekenwerk per seconde is gelijk gebleven; het is alleen gelijkmatig.
///
/// # Latentie (uitsluitend het natte pad)
/// Een blok kan pas getransformeerd worden als de laatste sample ervan binnen
/// is (P samples), en de MACs lopen over het volgende blok (nog eens P
/// samples): de galm van sample n komt bij sample n + 2·P naar buiten, zie
/// [`latency_samples`](Self::latency_samples). De spreiding kost dus precies
/// één partitie extra ten opzichte van de oude versie (die had al P). Het droge
/// signaal wordt hier niet aangeraakt; voor een galmstaart van seconden is een
/// vaste voorvertraging van enkele milliseconden (P = 256 → 10,7 ms totaal op
/// 48 kHz, waarvan 5,3 ms door de spreiding) onhoorbaar.
///
/// Het hete pad ([`process_wet_sample`](Self::process_wet_sample)) alloceert
/// niet: alle buffers worden in [`new`](Self::new) aangemaakt en de FFT's
/// gebruiken vooraf gealloceerde scratch-buffers.
#[derive(Clone)]
pub struct ConvolutionReverb {
    /// FFT-lengte = 2·partition_size.
    fft_size: usize,

    /// Partitiegrootte P: aantal samples per invoer-/uitvoerblok.
    partition_size: usize,

    /// Aantal IR-partities N (≥ 1).
    num_partitions: usize,

    /// Aantal frequentiebins per spectrum: fft_size/2 + 1 = P + 1.
    bins: usize,

    /// Voorwaartse FFT.
    fft_forward: Arc<dyn RealToComplex<f32>>,

    /// Inverse FFT.
    fft_inverse: Arc<dyn ComplexToReal<f32>>,

    /// IR-spectra: per partitie `bins` waarden, reëel en imaginair apart
    /// (structure-of-arrays) zodat de MAC-lus door de compiler gevectoriseerd
    /// kan worden. Index: `k * bins + bin`.
    ir_re: Vec<f32>,
    ir_im: Vec<f32>,

    /// Ringbuffer met de spectra van de laatste N invoerblokken, zelfde
    /// indeling als `ir_re`/`ir_im`.
    hist_re: Vec<f32>,
    hist_im: Vec<f32>,

    /// Index in de ringbuffer van het nieuwste invoerblok.
    hist_pos: usize,

    /// Accumulator van de lopende, gespreide MAC (spectrum van het uitvoerblok
    /// in wording).
    acc_re: Vec<f32>,
    acc_im: Vec<f32>,

    /// Aantal IR-partities dat voor het lopende blok al verrekend is.
    /// `== num_partitions` betekent: alle MACs klaar én de IFFT is gedaan.
    macs_done: usize,

    /// Portiegrootte: aantal partitie-MACs per sample-aanroep.
    macs_per_call: usize,

    /// Aantal partitie-MACs in de laatste sample-aanroep (diagnose/tests).
    macs_in_last_call: usize,

    /// Aantal keer dat de partitiegrens nog openstaand MAC-werk moest afmaken.
    /// Hoort 0 te blijven (kan alleen bij P == 1); telt hoe vaak het vangnet
    /// in `on_partition_boundary` nodig was.
    noodbursts: u32,

    /// Tweede helft van de vorige IFFT-uitvoer (overlap-add), P samples.
    overlap: Vec<f32>,

    /// Uitvoerblok dat nu wordt uitgelezen (P samples).
    output_buffer: Vec<f32>,

    /// Volgende uitvoerblok, klaargezet door `finalize_block`; wordt op de
    /// partitiegrens met `output_buffer` verwisseld (swap, geen kopie).
    next_output: Vec<f32>,

    /// Positie binnen het lopende invoerblok (0..P).
    pos: usize,

    /// Werkbuffers voor FFT/IFFT, vooraf gealloceerd.
    fft_input: Vec<f32>,
    fft_output: Vec<Complex<f32>>,
    fft_scratch: Vec<Complex<f32>>,
    ifft_input: Vec<Complex<f32>>,
    ifft_output: Vec<f32>,
    ifft_scratch: Vec<Complex<f32>>,

    /// Wet/dry-mix (0.0 = droog, 1.0 = nat); alleen gebruikt door `process`.
    pub mix: f32,

    /// Uitgangsversterking van het natte signaal.
    pub gain: f32,
}

impl std::fmt::Debug for ConvolutionReverb {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConvolutionReverb")
            .field("num_partitions", &self.num_partitions)
            .field("partition_size", &self.partition_size)
            .field("latency_samples", &self.latency_samples())
            .field("macs_per_call", &self.macs_per_call)
            .field("noodbursts", &self.noodbursts)
            .finish()
    }
}

impl ConvolutionReverb {
    /// Aanbevolen partitiegrootte: 256 samples.
    ///
    /// Bij uniforme partitieconvolutie is het aantal complexe MACs per seconde
    /// onafhankelijk van de partitiegrootte: N·(P+1) MACs per P samples, met
    /// N ≈ IR-lengte / P, is ≈ IR-lengte MACs per sample — ongeacht P. Alleen de
    /// FFT-kosten (één FFT + één IFFT van 2·P punten per P samples) stijgen
    /// licht bij kleinere P, en die zijn verwaarloosbaar. Een kleinere partitie
    /// kost dus vrijwel niets extra, maar verkort de natte latentie
    /// evenredig: 256 samples = 5,3 ms per partitie op 48 kHz in plaats van
    /// 21 ms bij 1024 (totaal 10,7 ms in plaats van 42,7 ms, zie
    /// [`latency_samples`](Self::latency_samples)). Tegelijk wordt de
    /// spreidingsportie per sample-aanroep kleiner, dus ook de piekkosten.
    pub const fn aanbevolen_partitie() -> usize {
        256
    }

    /// Maak een convolutiegalm met de gegeven impulsrespons (mono) en
    /// partitiegrootte (bij voorkeur een macht van twee, zie
    /// [`aanbevolen_partitie`](Self::aanbevolen_partitie)).
    pub fn new(impulse_response: &[f32], partition_size: usize) -> Self {
        // Lege IR → één stille partitie. num_partitions = 0 gaf verderop een
        // index-out-of-bounds / rest-deling-door-nul-panic ín de audio-callback
        // (auditbevinding 13).
        let silent = [0.0f32];
        let impulse_response: &[f32] = if impulse_response.is_empty() { &silent } else { impulse_response };
        // Partitie 0 zou een FFT van lengte 0 geven: minimaal 1.
        let partition_size = partition_size.max(1);
        let fft_size = partition_size * 2;
        let bins = fft_size / 2 + 1;
        let num_partitions = (impulse_response.len() + partition_size - 1) / partition_size;

        let mut planner = RealFftPlanner::<f32>::new();
        let fft_forward = planner.plan_fft_forward(fft_size);
        let fft_inverse = planner.plan_fft_inverse(fft_size);

        let mut fft_input = vec![0.0f32; fft_size];
        let mut fft_output = vec![Complex::new(0.0f32, 0.0f32); bins];
        let mut fft_scratch = fft_forward.make_scratch_vec();
        let ifft_scratch = fft_inverse.make_scratch_vec();

        // IR partitioneren en transformeren (SoA-opslag).
        let mut ir_re = vec![0.0f32; num_partitions * bins];
        let mut ir_im = vec![0.0f32; num_partitions * bins];
        for k in 0..num_partitions {
            let start = k * partition_size;
            let end = (start + partition_size).min(impulse_response.len());
            // Nul-aanvulling: realfft gebruikt de invoer als kladruimte, dus
            // elke keer de hele buffer schoonmaken.
            fft_input.fill(0.0);
            fft_input[..end - start].copy_from_slice(&impulse_response[start..end]);
            // Kan alleen falen op verkeerde bufferlengtes, en die staan hier vast.
            fft_forward
                .process_with_scratch(&mut fft_input, &mut fft_output, &mut fft_scratch)
                .expect("FFT van IR-partitie: bufferlengtes kloppen niet");
            let base = k * bins;
            for (i, c) in fft_output.iter().enumerate() {
                ir_re[base + i] = c.re;
                ir_im[base + i] = c.im;
            }
        }
        fft_input.fill(0.0);

        Self {
            fft_size,
            partition_size,
            num_partitions,
            bins,
            fft_forward,
            fft_inverse,
            ir_re,
            ir_im,
            hist_re: vec![0.0; num_partitions * bins],
            hist_im: vec![0.0; num_partitions * bins],
            hist_pos: 0,
            acc_re: vec![0.0; bins],
            acc_im: vec![0.0; bins],
            // Nog geen blok in behandeling: "alles klaar".
            macs_done: num_partitions,
            macs_per_call: Self::bereken_macs_per_call(num_partitions, partition_size),
            macs_in_last_call: 0,
            noodbursts: 0,
            overlap: vec![0.0; partition_size],
            output_buffer: vec![0.0; partition_size],
            next_output: vec![0.0; partition_size],
            pos: 0,
            fft_input,
            fft_output,
            fft_scratch,
            ifft_input: vec![Complex::new(0.0, 0.0); bins],
            ifft_output: vec![0.0; fft_size],
            ifft_scratch,
            mix: 0.5,
            gain: 1.0,
        }
    }

    /// Portiegrootte: ceil(N / (P-1)) partitie-MACs per sample-aanroep, zodat
    /// alle N MACs uiterlijk in aanroep P-2 van het blok klaar zijn en de IFFT
    /// nooit in dezelfde aanroep valt als de FFT op de partitiegrens. Bij P == 1
    /// is er geen ruimte om te spreiden en gebeurt alles in die ene aanroep.
    fn bereken_macs_per_call(num_partitions: usize, partition_size: usize) -> usize {
        let slots = partition_size.saturating_sub(1).max(1);
        (num_partitions + slots - 1) / slots
    }

    /// Latentie van het NATTE pad in samples: 2·partition_size.
    ///
    /// Eén partitie omdat een invoerblok pas na de laatste sample
    /// getransformeerd kan worden (dat had de oude versie ook), plus één
    /// partitie omdat de vermenigvuldiging met de IR-partities over het
    /// volgende blok gespreid wordt. De galm van sample n komt bij sample
    /// n + latency_samples() naar buiten; het droge signaal blijft ongemoeid.
    /// Bij P = 256 op 48 kHz: 512 samples = 10,7 ms.
    pub fn latency_samples(&self) -> usize {
        2 * self.partition_size
    }

    /// Partitiegrootte P.
    pub fn partition_size(&self) -> usize {
        self.partition_size
    }

    /// Aantal IR-partities N.
    pub fn num_partitions(&self) -> usize {
        self.num_partitions
    }

    /// Aantal partitie-MACs dat per sample-aanroep hoogstens wordt uitgevoerd
    /// (de spreidingsportie).
    pub fn macs_per_call(&self) -> usize {
        self.macs_per_call
    }

    /// Aantal partitie-MACs dat in de laatste sample-aanroep is uitgevoerd.
    pub fn macs_in_last_call(&self) -> usize {
        self.macs_in_last_call
    }

    /// Hoe vaak de partitiegrens nog openstaand MAC-werk in één keer moest
    /// afmaken. Hoort 0 te zijn; > 0 betekent dat de spreiding niet paste.
    pub fn noodbursts(&self) -> u32 {
        self.noodbursts
    }

    /// Load impulse response from WAV file. `target_sample_rate` is de
    /// engine-samplerate: een IR op een andere rate wordt lineair geresampled —
    /// voorheen werd de rate volledig genegeerd en klonk de galm te snel/langzaam
    /// met verschoven spectrum (auditbevinding 30).
    pub fn from_wav(path: &std::path::Path, partition_size: usize, target_sample_rate: u32) -> Result<Self, String> {
        let reader = hound::WavReader::open(path)
            .map_err(|e| format!("Failed to open WAV file: {}", e))?;

        let spec = reader.spec();
        let samples: Vec<f32> = match spec.sample_format {
            hound::SampleFormat::Float => {
                reader.into_samples::<f32>()
                    .map(|s| s.unwrap_or(0.0))
                    .collect()
            }
            hound::SampleFormat::Int => {
                // i64-shift: `1 << 31` als i32 is overflow — een 32-bit int-IR
                // kreeg daardoor i32::MIN als deler en een omgekeerde polariteit
                // (auditbevinding 56).
                let max_val = (1i64 << (spec.bits_per_sample - 1)) as f32;
                reader.into_samples::<i32>()
                    .map(|s| s.unwrap_or(0) as f32 / max_val)
                    .collect()
            }
        };

        // Convert to mono if stereo
        let mono: Vec<f32> = if spec.channels == 2 {
            samples.chunks(2)
                .map(|chunk| (chunk[0] + chunk.get(1).unwrap_or(&0.0)) * 0.5)
                .collect()
        } else {
            samples
        };

        if mono.is_empty() {
            return Err("IR-bestand bevat geen samples".to_string());
        }

        // Lineaire resample naar de engine-rate (voor een galm-IR ruim voldoende).
        let mono: Vec<f32> = if spec.sample_rate != target_sample_rate && spec.sample_rate > 0 {
            let ratio = spec.sample_rate as f64 / target_sample_rate as f64;
            let out_len = ((mono.len() as f64) / ratio).max(1.0) as usize;
            (0..out_len)
                .map(|i| {
                    let src = i as f64 * ratio;
                    let i0 = src.floor() as usize;
                    let frac = (src - i0 as f64) as f32;
                    let a = mono.get(i0).copied().unwrap_or(0.0);
                    let b = mono.get(i0 + 1).copied().unwrap_or(a);
                    a + (b - a) * frac
                })
                .collect()
        } else {
            mono
        };

        Ok(Self::new(&mono, partition_size))
    }

    /// Verwerk een blok samples met dry/wet-mix (`mix`).
    pub fn process(&mut self, input: &[f32], output: &mut [f32]) {
        for (i, &sample) in input.iter().enumerate() {
            let wet = self.process_wet_sample(sample);
            output[i] = sample * (1.0 - self.mix) + wet * self.mix;
        }
    }

    /// Verwerk één sample en geef alleen het WET-signaal terug (gain toegepast,
    /// mix niet — die past de aanroeper toe). Zo kan de engine de galm als
    /// additief effect bij het droge signaal mengen zonder dry/wet-truc.
    ///
    /// Per aanroep: sample opslaan, uitvoer lezen, één portie van het
    /// gespreide MAC-werk voor het vorige blok, en op de partitiegrens de FFT
    /// van het zojuist volle blok. Allocatievrij.
    #[inline]
    pub fn process_wet_sample(&mut self, sample: f32) -> f32 {
        let pos = self.pos;
        self.fft_input[pos] = sample;

        // Uitvoer van twee blokken terug (zie latency_samples).
        let wet = self.output_buffer[pos] * self.gain;

        // Eén portie MAC-werk voor het blok dat op de vorige grens is
        // getransformeerd; na de laatste portie meteen IFFT + overlap-add.
        self.spread_step();

        self.pos = pos + 1;
        if self.pos >= self.partition_size {
            self.on_partition_boundary();
            self.pos = 0;
        }

        wet
    }

    /// Eén portie van de gespreide MAC; sluit het blok af zodra alle
    /// partities verrekend zijn.
    #[inline]
    fn spread_step(&mut self) {
        let done = self.macs_done;
        if done >= self.num_partitions {
            self.macs_in_last_call = 0;
            return;
        }
        let end = (done + self.macs_per_call).min(self.num_partitions);
        self.mac_range(done, end);
        self.macs_in_last_call = end - done;
        self.macs_done = end;
        if end == self.num_partitions {
            self.finalize_block();
        }
    }

    /// acc += Σ_{k in from..to} X[hist_pos - k] · H[k] (complex, per bin).
    #[inline]
    fn mac_range(&mut self, from: usize, to: usize) {
        let bins = self.bins;
        let n = self.num_partitions;
        let hist_pos = self.hist_pos;
        let Self { acc_re, acc_im, hist_re, hist_im, ir_re, ir_im, .. } = self;
        let acc_re = &mut acc_re[..bins];
        let acc_im = &mut acc_im[..bins];
        for k in from..to {
            let h = if hist_pos >= k { hist_pos - k } else { hist_pos + n - k };
            let xr = &hist_re[h * bins..(h + 1) * bins];
            let xi = &hist_im[h * bins..(h + 1) * bins];
            let hr = &ir_re[k * bins..(k + 1) * bins];
            let hi = &ir_im[k * bins..(k + 1) * bins];
            // Alle zes slices zijn precies `bins` lang: de compiler kan de
            // bounds-checks weglaten en de lus vectoriseren.
            for i in 0..bins {
                let (a, b, c, d) = (xr[i], xi[i], hr[i], hi[i]);
                acc_re[i] += a * c - b * d;
                acc_im[i] += a * d + b * c;
            }
        }
    }

    /// IFFT van de accumulator + overlap-add naar `next_output`; maakt de
    /// accumulator leeg voor het volgende blok.
    fn finalize_block(&mut self) {
        for (c, (&re, &im)) in self.ifft_input.iter_mut().zip(self.acc_re.iter().zip(self.acc_im.iter())) {
            *c = Complex::new(re, im);
        }
        // realfft eist een zuiver reële DC- en Nyquist-bin. Door de
        // vermenigvuldiging van reële bins is dat al zo; expliciet nullen maakt
        // het onafhankelijk van afronding.
        self.ifft_input[0].im = 0.0;
        let last = self.bins - 1;
        self.ifft_input[last].im = 0.0;
        // Kan alleen falen op verkeerde bufferlengtes (staan vast) of op die
        // imaginaire delen (zojuist genuld; de transformatie wordt dan tóch
        // uitgevoerd). Geen unwrap in het audiopad.
        let _ = self
            .fft_inverse
            .process_with_scratch(&mut self.ifft_input, &mut self.ifft_output, &mut self.ifft_scratch);

        let norm = 1.0 / self.fft_size as f32;
        let p = self.partition_size;
        let (head, tail) = self.ifft_output.split_at(p);
        for ((out, ov), (&h, &t)) in self.next_output.iter_mut().zip(self.overlap.iter_mut()).zip(head.iter().zip(tail.iter())) {
            *out = *ov + h * norm;
            *ov = t * norm;
        }
        self.acc_re.fill(0.0);
        self.acc_im.fill(0.0);
    }

    /// Partitiegrens: uitvoerblok wisselen, FFT van het volle invoerblok,
    /// ringbuffer opschuiven en de spreiding voor dit blok starten.
    fn on_partition_boundary(&mut self) {
        // Vangnet: staat er tóch nog MAC-werk open (alleen mogelijk bij P == 1),
        // maak het nu af — liever een burst dan een fout spectrum.
        if self.macs_done < self.num_partitions {
            self.noodbursts = self.noodbursts.saturating_add(1);
            let done = self.macs_done;
            self.mac_range(done, self.num_partitions);
            self.macs_in_last_call += self.num_partitions - done;
            self.macs_done = self.num_partitions;
            self.finalize_block();
        }

        // Het tijdens dit blok klaargezette uitvoerblok wordt nu uitleesbaar.
        std::mem::swap(&mut self.output_buffer, &mut self.next_output);

        // FFT van het invoerblok; de tweede helft is nul (na elke FFT wordt de
        // hele buffer geleegd, want realfft gebruikt de invoer als kladruimte).
        let _ = self
            .fft_forward
            .process_with_scratch(&mut self.fft_input, &mut self.fft_output, &mut self.fft_scratch);
        self.fft_input.fill(0.0);

        // Ringbuffer opschuiven en het nieuwe spectrum opslaan.
        self.hist_pos = (self.hist_pos + 1) % self.num_partitions;
        let base = self.hist_pos * self.bins;
        for (i, c) in self.fft_output.iter().enumerate() {
            self.hist_re[base + i] = c.re;
            self.hist_im[base + i] = c.im;
        }

        // Spreiding voor dit blok starten.
        self.macs_done = 0;
    }

    /// Maak de galmstaat schoon (geen staart van het vorige signaal).
    pub fn reset(&mut self) {
        self.hist_re.fill(0.0);
        self.hist_im.fill(0.0);
        self.acc_re.fill(0.0);
        self.acc_im.fill(0.0);
        self.overlap.fill(0.0);
        self.output_buffer.fill(0.0);
        self.next_output.fill(0.0);
        self.fft_input.fill(0.0);
        self.pos = 0;
        self.hist_pos = 0;
        self.macs_done = self.num_partitions;
        self.macs_in_last_call = 0;
    }
}

/// Stereo convolution reverb
pub struct StereoConvolutionReverb {
    left: ConvolutionReverb,
    right: ConvolutionReverb,
}

impl StereoConvolutionReverb {
    pub fn new(ir_left: &[f32], ir_right: &[f32], partition_size: usize) -> Self {
        Self {
            left: ConvolutionReverb::new(ir_left, partition_size),
            right: ConvolutionReverb::new(ir_right, partition_size),
        }
    }

    pub fn set_mix(&mut self, mix: f32) {
        self.left.mix = mix;
        self.right.mix = mix;
    }

    pub fn set_gain(&mut self, gain: f32) {
        self.left.gain = gain;
        self.right.gain = gain;
    }

    /// Latentie van het natte pad in samples (zie [`ConvolutionReverb::latency_samples`]).
    pub fn latency_samples(&self) -> usize {
        self.left.latency_samples()
    }

    pub fn process(&mut self, input_left: &[f32], input_right: &[f32],
                   output_left: &mut [f32], output_right: &mut [f32]) {
        self.left.process(input_left, output_left);
        self.right.process(input_right, output_right);
    }

    pub fn reset(&mut self) {
        self.left.reset();
        self.right.reset();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Eenvoudige pseudo-willekeurige generator (geen extra crate nodig),
    /// uniform in [-1, 1).
    fn lcg(seed: &mut u64) -> f32 {
        *seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((*seed >> 40) as f32 / (1u64 << 24) as f32) * 2.0 - 1.0
    }

    fn ruis(seed: u64, n: usize, schaal: f32) -> Vec<f32> {
        let mut s = seed;
        (0..n).map(|_| lcg(&mut s) * schaal).collect()
    }

    /// Directe (naïeve) convolutie in f64; alleen de eerste x.len() uitvoerwaarden.
    fn naieve_convolutie(x: &[f32], h: &[f32]) -> Vec<f64> {
        (0..x.len())
            .map(|n| {
                let kmax = n.min(h.len() - 1);
                (0..=kmax).map(|k| h[k] as f64 * x[n - k] as f64).sum()
            })
            .collect()
    }

    /// Voer `x` door `rev`, gevolgd door genoeg nullen om de staart te lezen,
    /// en vergelijk (na compensatie van de latentie) met de naïeve convolutie.
    fn vergelijk_met_naief(rev: &mut ConvolutionReverb, x: &[f32], ir: &[f32], tol: f64) {
        let lat = rev.latency_samples();
        let mut uit = Vec::with_capacity(x.len() + lat);
        for &s in x {
            uit.push(rev.process_wet_sample(s));
        }
        for _ in 0..lat {
            uit.push(rev.process_wet_sample(0.0));
        }
        // Vóór de latentie komt er niets naar buiten.
        for (n, v) in uit[..lat].iter().enumerate() {
            assert!(v.abs() < 1e-7, "uitvoer vóór de latentie niet nul: sample {n} = {v}");
        }
        let referentie = naieve_convolutie(x, ir);
        let mut max_fout = 0.0f64;
        for n in 0..x.len() {
            let fout = (uit[n + lat] as f64 - referentie[n]).abs();
            max_fout = max_fout.max(fout);
            assert!(
                fout < tol,
                "sample {n}: gespreid {} vs naïef {} (fout {fout}, P={}, N={})",
                uit[n + lat], referentie[n], rev.partition_size(), rev.num_partitions()
            );
        }
        assert!(max_fout < tol, "maximale fout {max_fout}");
    }

    fn ceil_div(a: usize, b: usize) -> usize {
        (a + b - 1) / b
    }

    /// (a) korte willekeurige IR (veelvoud van de partitie) × willekeurig signaal.
    #[test]
    fn gespreide_uitvoer_gelijk_aan_naieve_convolutie() {
        let p = 16;
        let ir = ruis(1, 3 * p, 0.5);
        let x = ruis(2, 20 * p + 7, 1.0);
        let mut rev = ConvolutionReverb::new(&ir, p);
        assert_eq!(rev.num_partitions(), 3);
        vergelijk_met_naief(&mut rev, &x, &ir, 1e-4);
    }

    /// (b) IR-lengte die geen veelvoud van de partitie is: korter dan één
    /// partitie, en langer met rest. Ook een geval met N > P-1, zodat er
    /// meerdere MACs per aanroep nodig zijn.
    #[test]
    fn ir_lengte_geen_veelvoud_van_partitie() {
        // Korter dan één partitie.
        let p = 16;
        let ir = ruis(3, 5, 0.5);
        let x = ruis(4, 10 * p, 1.0);
        let mut rev = ConvolutionReverb::new(&ir, p);
        assert_eq!(rev.num_partitions(), 1);
        vergelijk_met_naief(&mut rev, &x, &ir, 1e-4);

        // Drie partities plus een rest.
        let ir = ruis(5, 3 * p + 5, 0.5);
        let x = ruis(6, 15 * p + 3, 1.0);
        let mut rev = ConvolutionReverb::new(&ir, p);
        assert_eq!(rev.num_partitions(), 4);
        vergelijk_met_naief(&mut rev, &x, &ir, 1e-4);

        // Kleine partitie, veel partities (N = 13 > P-1 = 7 → 2 MACs per aanroep).
        let p = 8;
        let ir = ruis(7, 100, 0.3);
        let x = ruis(8, 60 * p + 1, 1.0);
        let mut rev = ConvolutionReverb::new(&ir, p);
        assert_eq!(rev.num_partitions(), 13);
        assert_eq!(rev.macs_per_call(), 2);
        vergelijk_met_naief(&mut rev, &x, &ir, 1e-4);
        assert_eq!(rev.noodbursts(), 0);
    }

    /// (c) reset() maakt de staat schoon: geen staart uit het vorige signaal,
    /// en daarna weer exact hetzelfde gedrag als een verse instantie.
    #[test]
    fn reset_maakt_staat_schoon() {
        let p = 16;
        let ir = ruis(9, 3 * p + 5, 0.5);
        let x = ruis(10, 12 * p + 3, 1.0);
        let mut rev = ConvolutionReverb::new(&ir, p);
        // Signaal erin, midden in een blok stoppen.
        for &s in &x {
            rev.process_wet_sample(s);
        }
        rev.reset();
        // Nullen erin → nullen eruit (geen staart).
        for n in 0..6 * p {
            let v = rev.process_wet_sample(0.0);
            assert_eq!(v, 0.0, "staart na reset bij sample {n}: {v}");
        }
        // Na reset gedraagt de galm zich als een verse instantie.
        rev.reset();
        vergelijk_met_naief(&mut rev, &x, &ir, 1e-4);
    }

    /// (d) geen burst: per sample-aanroep nooit meer dan macs_per_call
    /// (= ceil(N/(P-1))) partitie-MACs, en dat is ≤ ceil(N/P) + 1. Getest met
    /// een kleine partitie én met de aanbevolen 256 bij een IR van 6 s op 48 kHz.
    #[test]
    fn geen_burst_per_sample_aanroep() {
        for &(p, ir_len) in &[(8usize, 100usize), (16, 53), (256, 6 * 48_000)] {
            let ir = ruis(11, ir_len, 0.1);
            let mut rev = ConvolutionReverb::new(&ir, p);
            let n = rev.num_partitions();
            let grens = ceil_div(n, p) + 1;
            assert!(rev.macs_per_call() <= grens, "P={p}: portie {} > {grens}", rev.macs_per_call());
            let mut max_gezien = 0;
            let mut s = 12u64;
            for aanroep in 0..8 * p {
                rev.process_wet_sample(lcg(&mut s));
                let m = rev.macs_in_last_call();
                max_gezien = max_gezien.max(m);
                assert!(m <= rev.macs_per_call(), "P={p} aanroep {aanroep}: {m} MACs > portie {}", rev.macs_per_call());
                assert!(m <= grens, "P={p} aanroep {aanroep}: {m} MACs > ceil(N/P)+1 = {grens}");
            }
            assert_eq!(rev.noodbursts(), 0, "P={p}: partitiegrens moest MAC-werk afmaken");
            // Het werk wordt ook echt gedaan (niet per ongeluk nul).
            assert!(max_gezien >= 1, "P={p}: geen enkele MAC uitgevoerd");
        }
    }

    /// Latentie, aanbevolen partitie en portieformule.
    #[test]
    fn latentie_en_aanbevolen_partitie() {
        assert_eq!(ConvolutionReverb::aanbevolen_partitie(), 256);
        let rev = ConvolutionReverb::new(&[1.0; 1000], 256);
        assert_eq!(rev.latency_samples(), 512);
        assert_eq!(rev.num_partitions(), 4);
        assert_eq!(rev.macs_per_call(), 1);
        let rev = ConvolutionReverb::new(&[1.0; 6 * 48_000], 256);
        assert_eq!(rev.num_partitions(), 1125);
        assert_eq!(rev.macs_per_call(), ceil_div(1125, 255));
        let stereo = StereoConvolutionReverb::new(&[1.0; 10], &[1.0; 10], 64);
        assert_eq!(stereo.latency_samples(), 128);
    }

    /// Lege IR en partitie 0 geven geen panic en een stille uitvoer.
    #[test]
    fn lege_ir_en_partitie_nul_zijn_veilig() {
        let mut rev = ConvolutionReverb::new(&[], 16);
        for _ in 0..100 {
            assert_eq!(rev.process_wet_sample(1.0), 0.0);
        }
        let mut rev = ConvolutionReverb::new(&[1.0, 0.5], 0);
        assert_eq!(rev.partition_size(), 1);
        for _ in 0..100 {
            rev.process_wet_sample(1.0);
        }
    }

    /// gain werkt op het natte signaal; process() past de mix toe.
    #[test]
    fn gain_en_mix() {
        let p = 8;
        let ir = vec![1.0f32];
        let x = ruis(13, 6 * p, 1.0);
        let lat = 2 * p;
        let mut rev = ConvolutionReverb::new(&ir, p);
        rev.gain = 2.0;
        let mut uit = Vec::new();
        for &s in &x {
            uit.push(rev.process_wet_sample(s));
        }
        for _ in 0..lat {
            uit.push(rev.process_wet_sample(0.0));
        }
        for n in 0..x.len() {
            assert!((uit[n + lat] - 2.0 * x[n]).abs() < 1e-5);
        }
        // mix 0 → puur droog.
        let mut rev = ConvolutionReverb::new(&ir, p);
        rev.mix = 0.0;
        let mut out = vec![0.0f32; x.len()];
        rev.process(&x, &mut out);
        assert_eq!(out, x);
    }

    /// Benchmark (met `--ignored --nocapture`): kosten per sample-aanroep bij
    /// een IR van 6 s op 48 kHz, voor P = 1024 (oude instelling) en 256.
    /// Referentie oude implementatie (zelfde meting, 2026-09-15):
    ///   P=1024 N=282:  max 502 µs (burst op elke grens), gemiddeld 0,51 µs
    ///   P=256  N=1125: max 526 µs, p99.9 480 µs,          gemiddeld 1,9 µs
    #[test]
    #[ignore]
    fn bench_kosten_per_sample_aanroep() {
        use std::time::Instant;
        let sr = 48_000usize;
        let mut s = 12345u64;
        let ir: Vec<f32> = (0..6 * sr)
            .map(|i| lcg(&mut s) * (-(i as f32) / (sr as f32)).exp())
            .collect();
        for &p in &[1024usize, 256] {
            let mut rev = ConvolutionReverb::new(&ir, p);
            let mut s2 = 7u64;
            for _ in 0..8 * p {
                rev.process_wet_sample(lcg(&mut s2));
            }
            let calls = 16 * p;
            let mut durs = Vec::with_capacity(calls);
            let mut sink = 0.0f32;
            for _ in 0..calls {
                let x = lcg(&mut s2);
                let t = Instant::now();
                sink += rev.process_wet_sample(x);
                durs.push(t.elapsed());
            }
            durs.sort();
            let n = durs.len();
            let som: std::time::Duration = durs.iter().sum();
            let boven = |us: u64| durs.iter().filter(|d| d.as_micros() as u64 >= us).count();
            println!(
                "NIEUW P={} N={} portie={} max={:?} p99.9={:?} mediaan={:?} gem={:?} som/{}calls={:?} >=5µs:{} >=20µs:{} >=100µs:{} noodbursts={} (sink {})",
                p, rev.num_partitions(), rev.macs_per_call(), durs[n - 1], durs[n * 999 / 1000], durs[n / 2],
                som / n as u32, n, som, boven(5), boven(20), boven(100), rev.noodbursts(), sink
            );
        }
    }
}
