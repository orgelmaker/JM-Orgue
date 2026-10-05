// Import van EQ-presets (0.7.77): AutoEq / Equalizer APO-tekst, Sweelinq
// `.swes` en ons eigen JSON-formaat. Pure functies, zonder DOM of Tauri, zodat
// ze in node te testen zijn (testscripts/eq_import_test.mjs).
//
// Alle parsers leveren { banden, preamp, overgeslagen, kanaalregels, aan }:
//   banden      — bandobjecten zoals de equalizer ze gebruikt (q gezet, de
//                 bandbreedte er analoog uit afgeleid, channel null)
//   preamp      — voorversterking uit het bestand in dB, of null (de app
//                 rekent de Auto-waarde toch zelf uit)
//   overgeslagen — aantal regels/banden die niet te lezen waren
//   kanaalregels — "Channel:"-regels (Equalizer APO) die we negeren
//   aan         — stond de EQ in het bestand aan (alleen .swes kent dat)
// Bij een onbruikbaar bestand gooien ze een Error met een korte code
// ('geen_filters', 'afgekapt', 'onbekend_formaat'); de UI vertaalt die.

export const MAX_BANDEN = 32;

const TYPES = {
  PK: 'peak', PEQ: 'peak',
  LSC: 'lowshelf', LS: 'lowshelf', LSQ: 'lowshelf',
  HSC: 'highshelf', HS: 'highshelf', HSQ: 'highshelf',
  LP: 'lowpass', LPQ: 'lowpass',
  HP: 'highpass', HPQ: 'highpass',
  BP: 'bandpass',
};

const klem = (v, lo, hi) => Math.max(lo, Math.min(hi, v));

// Q ↔ bandbreedte (analoge RBJ-benadering; dezelfde formule als in Console
// en vpo_audio::q_naar_bandbreedte, in beide richtingen).
export const qNaarBw = (q) => (2 / Math.LN2) * Math.asinh(1 / (2 * Math.max(0.05, q)));
export const bwNaarQ = (bw) => 1 / (2 * Math.sinh(0.5 * Math.LN2 * Math.max(0.01, bw)));

export function maakBand(type, freq, gain, q, enabled = true) {
  const qq = klem(Number.isFinite(Number(q)) && Number(q) > 0 ? Number(q) : 0.7071, 0.05, 20);
  return {
    enabled: !!enabled,
    band_type: type,
    freq: klem(Number(freq) || 1000, 10, 20000),
    gain_db: klem(Number(gain) || 0, -24, 24),
    bandwidth: qNaarBw(qq),
    q: qq,
    channel: null,
  };
}

// --- Equalizer APO / AutoEq ParametricEQ.txt -------------------------------
// Voorbeeld:  Preamp: -6.2 dB
//             Filter 1: ON LSC Fc 105 Hz Gain 6.3 dB Q 0.70
//             Filter 3: ON PK Fc 4207 Hz Gain 5.2 dB BW Oct 0.5
// Spinorama schrijft 'Filter  1: ON PK Fc    44 Hz Gain +2.98 dB Q 2.89' (dubbele
// spaties, plusteken): daarom [-+]? en \s+ overal.
const RE_FILTER = /^Filter\s*\d*\s*:\s*(ON|OFF)\s+([A-Z]+)\s+Fc\s+([-+]?[\d.]+)\s*Hz(?:\s+Gain\s+([-+]?[\d.]+)\s*dB)?(?:\s+(Q|BW\s*Oct)\s+([-+]?[\d.]+))?/i;
const RE_PREAMP = /^Preamp:\s*([-+]?[\d.]+)\s*dB/i;

export function parseParametricEqTxt(tekst) {
  const banden = [];
  let preamp = null;
  let overgeslagen = 0;
  let kanaalregels = 0;
  for (const raw of String(tekst).split(/\r?\n/)) {
    const regel = raw.trim();
    if (!regel || regel.startsWith('#')) continue;
    const mp = RE_PREAMP.exec(regel);
    if (mp) { preamp = Number(mp[1]); continue; }
    if (/^Channel\s*:/i.test(regel)) { kanaalregels += 1; continue; }
    const m = RE_FILTER.exec(regel);
    if (!m) {
      if (/^Filter/i.test(regel)) overgeslagen += 1;
      continue;
    }
    const type = TYPES[m[2].toUpperCase()];
    if (!type) { overgeslagen += 1; continue; }
    let q = 0.7071;
    if (m[5] && m[6] !== undefined) q = /^BW/i.test(m[5]) ? bwNaarQ(Number(m[6])) : Number(m[6]);
    if (banden.length >= MAX_BANDEN) { overgeslagen += 1; continue; }
    banden.push(maakBand(type, m[3], m[4] ?? 0, q, m[1].toUpperCase() === 'ON'));
  }
  if (!banden.length) throw new Error('geen_filters');
  return { banden, preamp, overgeslagen, kanaalregels, aan: true };
}

// --- Sweelinq .swes ---------------------------------------------------------
// Protobuf-achtig: herhaald veld 1 (length-delimited) = band { 2: f32
// frequentie, 3: f32 gain dB, 4: f32 Q, 5: varint aan }, daarna veld 2 varint
// = EQ aan. Structuur afgeleid uit één voorbeeldbestand; onbekende velden
// worden per wire-type overgeslagen, een afgekapt bestand geeft 'afgekapt'.
function leesVarint(view, pos) {
  let r = 0, shift = 0, b;
  do {
    if (pos >= view.byteLength) throw new Error('afgekapt');
    b = view.getUint8(pos++);
    r += (b & 0x7f) * 2 ** shift;
    shift += 7;
    if (shift > 63) throw new Error('onbekend_formaat');
  } while (b & 0x80);
  return [r, pos];
}

function leesVelden(bytes) {
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  const uit = [];
  let pos = 0;
  while (pos < bytes.byteLength) {
    let tag;
    [tag, pos] = leesVarint(view, pos);
    const nr = Math.floor(tag / 8), wt = tag & 7;
    if (wt === 0) { let v; [v, pos] = leesVarint(view, pos); uit.push({ nr, wt, val: v }); }
    else if (wt === 5) { if (pos + 4 > bytes.byteLength) throw new Error('afgekapt'); uit.push({ nr, wt, val: view.getFloat32(pos, true) }); pos += 4; }
    else if (wt === 1) { if (pos + 8 > bytes.byteLength) throw new Error('afgekapt'); uit.push({ nr, wt, val: view.getFloat64(pos, true) }); pos += 8; }
    else if (wt === 2) {
      let len; [len, pos] = leesVarint(view, pos);
      if (pos + len > bytes.byteLength) throw new Error('afgekapt');
      uit.push({ nr, wt, val: bytes.subarray(pos, pos + len) }); pos += len;
    }
    else throw new Error('onbekend_formaat');
  }
  return uit;
}

export function parseSwes(bytes) {
  const b = bytes instanceof Uint8Array ? bytes : new Uint8Array(bytes);
  const banden = [];
  let aan = true;
  for (const v of leesVelden(b)) {
    if (v.nr === 1 && v.wt === 2) {
      let freq = null, gain = 0, q = 1, enabled = true;
      for (const x of leesVelden(v.val)) {
        if (x.nr === 2 && x.wt === 5) freq = x.val;
        else if (x.nr === 3 && x.wt === 5) gain = x.val;
        else if (x.nr === 4 && x.wt === 5) q = x.val;
        else if (x.nr === 5 && x.wt === 0) enabled = x.val !== 0;
      }
      if (freq != null && Number.isFinite(freq) && freq > 0 && banden.length < MAX_BANDEN) {
        banden.push(maakBand('peak', freq, gain, q, enabled));
      }
    } else if (v.nr === 2 && v.wt === 0) {
      aan = v.val !== 0;
    }
  }
  if (!banden.length) throw new Error('geen_filters');
  return { banden, preamp: null, overgeslagen: 0, kanaalregels: 0, aan };
}

// --- Eigen JSON (export uit JM-Orgue, of het eq-blok uit .jm-settings.json) --
export function parseEigenJson(tekst) {
  let d;
  try { d = JSON.parse(String(tekst)); } catch (e) { throw new Error('onbekend_formaat'); }
  const lijst = Array.isArray(d) ? d : (Array.isArray(d?.bands) ? d.bands : (Array.isArray(d?.eq?.bands) ? d.eq.bands : null));
  if (!lijst) throw new Error('onbekend_formaat');
  const banden = [];
  let overgeslagen = 0;
  for (const b of lijst) {
    const type = TYPES[String(b?.band_type || b?.type || '').toUpperCase()] || String(b?.band_type || b?.type || '').toLowerCase();
    if (!['peak', 'lowshelf', 'highshelf', 'lowpass', 'highpass', 'bandpass'].includes(type)) { overgeslagen += 1; continue; }
    if (banden.length >= MAX_BANDEN) { overgeslagen += 1; continue; }
    const q = b.q != null ? Number(b.q) : bwNaarQ(Number(b.bandwidth) || 1);
    const band = maakBand(type, b.freq, b.gain_db ?? b.gain ?? 0, q, b.enabled !== false);
    if (b.channel !== undefined && b.channel !== null && Number.isInteger(Number(b.channel))) band.channel = Number(b.channel);
    banden.push(band);
  }
  if (!banden.length) throw new Error('geen_filters');
  const bron = Array.isArray(d) ? {} : (d.eq || d);
  const preamp = Number.isFinite(Number(bron.preamp_db)) ? Number(bron.preamp_db) : null;
  return { banden, preamp, overgeslagen, kanaalregels: 0, aan: bron.enabled !== false, preampAuto: bron.preamp_auto };
}

// Naam voor in de knop: bestandsnaam zonder map, extensie en het Sweelinq-
// voorvoegsel "preset ".
export function presetNaamUitPad(pad) {
  const base = String(pad || '').split(/[\\/]/).pop() || '';
  return base.replace(/\.(txt|swes|json)$/i, '').replace(/^preset\s+/i, '').trim() || base;
}

// Herken het formaat aan extensie en inhoud en parse. `bytes` = Uint8Array
// (of gewone array van bytes, zoals Tauri een Vec<u8> aanlevert).
export function herkenEnParse(pad, bytes) {
  const b = bytes instanceof Uint8Array ? bytes : Uint8Array.from(bytes || []);
  const ext = (String(pad || '').match(/\.([a-z0-9]+)$/i) || [, ''])[1].toLowerCase();
  const tekst = () => new TextDecoder('utf-8').decode(b);
  let r;
  if (ext === 'swes') r = parseSwes(b);
  else if (ext === 'json') r = parseEigenJson(tekst());
  else {
    const t = tekst();
    if (/^\s*[\[{]/.test(t)) r = parseEigenJson(t);
    else if (/Filter\s*\d*\s*:|Preamp:/i.test(t)) r = parseParametricEqTxt(t);
    else if (b.length > 0 && b[0] === 0x0a) r = parseSwes(b);
    else throw new Error('onbekend_formaat');
  }
  return { naam: presetNaamUitPad(pad), ...r };
}

// --- Exporteren (0.7.80) ----------------------------------------------------
// AutoEq / Equalizer APO-tekst: alleen peak (PK), lowshelf (LSC) en highshelf
// (HSC) hebben daar een vorm; andere typen worden weggelaten en geteld.
// Uitgeschakelde banden gaan als OFF mee. Terug-importeren geeft dezelfde
// banden (rondreis-test in testscripts/eq_import_test.mjs).
const TXT_TYPES = { peak: 'PK', lowshelf: 'LSC', highshelf: 'HSC' };
export function exportParametricEqTxt(banden, preampDb = 0) {
  const regels = [`Preamp: ${(Number(preampDb) || 0).toFixed(1)} dB`];
  let n = 0, overgeslagen = 0;
  for (const b of banden || []) {
    const type = TXT_TYPES[b.band_type];
    if (!type) { overgeslagen += 1; continue; }
    n += 1;
    const q = b.q != null ? Number(b.q) : bwNaarQ(Number(b.bandwidth) || 1);
    regels.push(`Filter ${n}: ${b.enabled === false ? 'OFF' : 'ON'} ${type} Fc ${Number(b.freq).toFixed(Number.isInteger(Number(b.freq)) ? 0 : 1)} Hz Gain ${(Number(b.gain_db) || 0).toFixed(1)} dB Q ${q.toFixed(2)}`);
  }
  return { tekst: regels.join('\n') + '\n', overgeslagen };
}

// Eigen JSON: volledig (alle typen, kanalen, Q én bandbreedte).
export function exportEigenJson(banden, preampDb = null, preampAuto = true, naam = null) {
  const uit = {
    jm_orgue_eq: 1,
    naam: naam || null,
    enabled: true,
    preamp_db: preampDb == null ? null : Number(preampDb),
    preamp_auto: !!preampAuto,
    bands: (banden || []).map((b) => ({
      enabled: b.enabled !== false, band_type: b.band_type, freq: Number(b.freq), gain_db: Number(b.gain_db) || 0,
      bandwidth: Number(b.bandwidth) || qNaarBw(Number(b.q) || 1), q: b.q == null ? null : Number(b.q),
      channel: b.channel == null ? null : Number(b.channel),
    })),
  };
  return JSON.stringify(uit, null, 2) + '\n';
}
