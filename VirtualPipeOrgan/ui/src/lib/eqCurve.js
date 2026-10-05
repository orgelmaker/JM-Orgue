// EQ-curve (0.7.79): pure functies die vpo_audio::effects letterlijk spiegelen
// (BiquadFilter::from_band_spec, respons_db, eq_raster, auto_preamp_db), zodat
// de grafiek precies tekent wat de audiothread doet. De gelijkheid wordt
// bewezen door testscripts/eq_curve_test.mjs tegen een fixture die een Rust-
// test wegschrijft (testscripts/testdata/eq_curve_fixture.json).
//
// Bandobject zoals de equalizer het gebruikt: { enabled, band_type, freq,
// gain_db, bandwidth, q (null = bandbreedte-route), channel (null = alle) }.

export const EQ_Q_MIN = 0.05;
export const EQ_Q_MAX = 20.0;
export const EQ_MAX_BANDEN = 32;
const LN2_2 = 0.5 * Math.LN2;

export const qToBw = (q) => (2 / Math.LN2) * Math.asinh(1 / (2 * Math.max(EQ_Q_MIN, q)));
export const bwToQ = (bw) => 1 / (2 * Math.sinh(0.5 * Math.LN2 * Math.max(0.01, bw)));

// De audiothread rekent de coëfficiënten in f32 (Rust). Bij smalle pieken en
// lage frequenties liggen de polen zó dicht bij de eenheidscirkel dat één ulp
// in a1/a2 al 0,1 dB scheelt. Daarom wordt hier elke bewerking op f32 afgerond
// (Math.fround), in dezelfde volgorde als in Rust: de grafiek toont dan
// dezelfde filters als die er klinken, niet een nauwkeuriger ideaalbeeld.
const r = Math.fround;
const F32_PI = r(Math.PI);
const F32_LN2_2 = r(0.5 * r(Math.LN2));

function shelf(laag, freq, gainDb, qOfNull, sr) {
  const a = r(Math.pow(10, r(gainDb / 40)));
  const w = r(r(r(2 * F32_PI) * freq) / sr);
  const s = r(Math.sin(w)), c = r(Math.cos(w));
  const alpha = qOfNull != null
    ? r(s / r(2 * qOfNull))
    : r(r(s / 2) * r(Math.sqrt(r(r(r(a + r(1 / a)) * r(r(1 / 0.9) - 1)) + 2))));
  const tsa = r(r(2 * r(Math.sqrt(a))) * alpha);
  const ap1 = r(a + 1), am1 = r(a - 1);
  const am1c = r(am1 * c), ap1c = r(ap1 * c);
  let b0, b1, b2, a0, a1, a2;
  if (laag) {
    b0 = r(a * r(r(ap1 - am1c) + tsa)); b1 = r(r(2 * a) * r(am1 - ap1c)); b2 = r(a * r(r(ap1 - am1c) - tsa));
    a0 = r(r(ap1 + am1c) + tsa); a1 = r(-2 * r(am1 + ap1c)); a2 = r(r(ap1 + am1c) - tsa);
  } else {
    b0 = r(a * r(r(ap1 + am1c) + tsa)); b1 = r(r(-2 * a) * r(am1 + ap1c)); b2 = r(a * r(r(ap1 + am1c) - tsa));
    a0 = r(r(ap1 - am1c) + tsa); a1 = r(2 * r(am1 - ap1c)); a2 = r(r(ap1 - am1c) - tsa);
  }
  return { b0: r(b0 / a0), b1: r(b1 / a0), b2: r(b2 / a0), a1: r(a1 / a0), a2: r(a2 / a0) };
}

// Spiegel van BiquadFilter::from_band_spec (genormaliseerd op a0, f32).
export function biquadCoeffs(band, sr) {
  const freq = r(Math.min(Math.max(r(Number(band.freq) || 1000), 10), r(sr * 0.45)));
  const bw = r(Math.min(Math.max(r(Number(band.bandwidth) || 1), 0.05), 8));
  const gain = r(Number(band.gain_db) || 0);
  const q = band.q == null ? null : r(Math.min(Math.max(r(Number(band.q)), EQ_Q_MIN), EQ_Q_MAX));
  if (band.band_type === 'lowshelf') return shelf(true, freq, gain, q, sr);
  if (band.band_type === 'highshelf') return shelf(false, freq, gain, q, sr);
  const w = r(r(r(2 * F32_PI) * freq) / sr);
  const s = r(Math.sin(w)), c = r(Math.cos(w));
  const alpha = q != null ? r(s / r(2 * q)) : r(s * r(Math.sinh(r(r(r(F32_LN2_2 * bw) * w) / s))));
  const m2c = r(-2 * c);
  let b0, b1, b2, a0, a1, a2;
  switch (band.band_type) {
    case 'peak': {
      const a = r(Math.pow(10, r(gain / 40)));
      const aa = r(alpha * a), ad = r(alpha / a);
      b0 = r(1 + aa); b1 = m2c; b2 = r(1 - aa); a0 = r(1 + ad); a1 = m2c; a2 = r(1 - ad);
      break;
    }
    case 'lowpass': {
      const omc = r(1 - c);
      b0 = r(omc / 2); b1 = omc; b2 = r(omc / 2); a0 = r(1 + alpha); a1 = m2c; a2 = r(1 - alpha);
      break;
    }
    case 'highpass': {
      const opc = r(1 + c);
      b0 = r(opc / 2); b1 = r(-opc); b2 = r(opc / 2); a0 = r(1 + alpha); a1 = m2c; a2 = r(1 - alpha);
      break;
    }
    case 'bandpass':
      b0 = alpha; b1 = 0; b2 = r(-alpha); a0 = r(1 + alpha); a1 = m2c; a2 = r(1 - alpha);
      break;
    default:
      return { b0: 1, b1: 0, b2: 0, a1: 0, a2: 0 };
  }
  return { b0: r(b0 / a0), b1: r(b1 / a0), b2: r(b2 / a0), a1: r(a1 / a0), a2: r(a2 / a0) };
}

// Spiegel van BiquadFilter::respons_db.
export function magnitudeDb(co, f, sr) {
  const w = 2 * Math.PI * f / sr;
  const c1 = Math.cos(w), s1 = Math.sin(w), c2 = Math.cos(2 * w), s2 = Math.sin(2 * w);
  const nr = co.b0 + co.b1 * c1 + co.b2 * c2;
  const ni = -(co.b1 * s1 + co.b2 * s2);
  const dr = 1 + co.a1 * c1 + co.a2 * c2;
  const di = -(co.a1 * s1 + co.a2 * s2);
  const mag2 = (nr * nr + ni * ni) / Math.max(dr * dr + di * di, 1e-30);
  return 10 * Math.log10(Math.max(mag2, 1e-30));
}

export const bandDb = (band, f, sr) => magnitudeDb(biquadCoeffs(band, sr), f, sr);

// Banden die op kanaal `ch` werken (zoals banden_voor_kanaal): ingeschakeld
// en kanaal null of gelijk; maximaal 32. `ch` null = alleen de banden voor
// alle kanalen (de weergave "Alle").
export function bandenVoorKanaal(bands, ch) {
  const uit = [];
  for (const b of bands || []) {
    if (!b || !b.enabled) continue;
    if (ch == null ? b.channel != null : (b.channel != null && Number(b.channel) !== ch)) continue;
    uit.push(b);
    if (uit.length >= EQ_MAX_BANDEN) break;
  }
  return uit;
}

// Samengestelde respons in dB op één frequentie (banden + voorversterking).
export function compositeDb(bands, f, sr, ch = null, preampDb = 0) {
  let som = preampDb || 0;
  for (const b of bandenVoorKanaal(bands, ch)) som += bandDb(b, f, sr);
  return som;
}

// Dezelfde curve, voorgerekend met coëfficiënten (voor 256 punten).
export function compositeCurve(bands, freqs, sr, ch = null, preampDb = 0) {
  const cos = bandenVoorKanaal(bands, ch).map((b) => biquadCoeffs(b, sr));
  return freqs.map((f) => {
    let som = preampDb || 0;
    for (const co of cos) som += magnitudeDb(co, f, sr);
    return som;
  });
}

// Spiegel van eq_raster(n): n log-gespreide punten 20 Hz .. 20 kHz.
export function eqRaster(n) {
  n = Math.max(2, n);
  return Array.from({ length: n }, (_, i) => 20 * Math.pow(1000, i / (n - 1)));
}

export const logFreqs = (n, fmin = 20, fmax = 20000) =>
  Array.from({ length: n }, (_, i) => fmin * Math.pow(fmax / fmin, i / (n - 1)));

// Spiegel van auto_preamp_db: −(grootste opgetelde versterking) over het
// raster van 480 punten plus de centrumfrequenties, over alle kanalen; nooit
// positief en nooit −0.
export function autoPreampDb(bands, kanalen, sr) {
  const raster = eqRaster(480);
  const fMax = sr * 0.45;
  let piek = 0;
  for (let ch = 0; ch < Math.max(1, kanalen); ch++) {
    const lijst = bandenVoorKanaal(bands, ch);
    if (!lijst.length) continue;
    const cos = lijst.map((b) => biquadCoeffs(b, sr));
    const centra = lijst.map((b) => Math.min(Math.max(Number(b.freq) || 1000, 10), fMax));
    for (const f of raster.concat(centra)) {
      let r = 0;
      for (const co of cos) r += magnitudeDb(co, f, sr);
      if (Number.isFinite(r) && r > piek) piek = r;
    }
  }
  return piek > 0 ? -piek : 0;
}

// Sterkte (0.7.80): kopieën van de banden met de gain geschaald naar 0–100 %
// (spiegel van commands::schaal_specs). 100 geeft dezelfde objecten terug.
export function schaalBanden(bands, strength = 100) {
  const f = Math.min(100, Math.max(0, Number.isFinite(Number(strength)) ? Number(strength) : 100)) / 100;
  if (Math.abs(f - 1) < 1e-6) return bands || [];
  return (bands || []).map((b) => ({ ...b, gain_db: (Number(b.gain_db) || 0) * f }));
}
