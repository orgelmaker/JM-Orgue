# -*- coding: utf-8 -*-
"""
Meetplan zweltrede + generaal crescendo (JM-Orgue 0.7.38) via de test-API (poort 8765).

Vereist de test-API-routes uit 0.7.38 (POST /midi/inject?cc=, GET /crescendo, POST /crescendo/config|binding|stage,
GET /swell, POST /swell/binding, POST /midi/mapping, GET /held_notes) — het script controleert dat en stopt anders.
Modelwijzigingen 0.7.38 (zie state.rs): Schmitt-hysterese H=2 CC rond elke trapgrens (crescendo_next_stage),
lege trap erft de lagere gevulde trap, koppels op lidmaatschap, zwel-smoothing tau=20 ms (audio.rs swell_dsp),
zwelstand hersteld na herlaad/wissel (SwellBinding.last_value), crescendo-matrix in OrganSettings.
Patroon: scratchpad/test_ranks_pitch.py (urllib + numpy + ffmpeg).

Wat dit script meet
  1. Zweltrede   : CC-sweep 0→127→0 (stap 1, ook met jitter ±2) terwijl een noot klinkt,
                   MP3 opnemen → RMS-envelope per 10 ms: monotonie, max sprong/stap,
                   eindwaarden (dicht = min_db, open = 0 dB), sprong 127→0 (ramp/klik).
  2. Crescendo   : per CC-waarde de trede aflezen (GET /state → crescendo.stage, drawn),
                   monotonie, dode zone, hysterese rond een grens, handmatige registers
                   blijven staan, trede 0 haalt alles weg, audio-dip/klik bij trapwissel.
  3. Inversie/bereik: pedaal 127→0 en bereik 20..110 voor zwel én crescendo.
  4. Persistentie: bindingen overleven /settings/save → /load_organ en /audio_output;
                   zwelstand na herlaad/wissel (audio vs. spiegel).
  5. Exclusiviteit: zwel en crescendo tegelijk op VERSCHILLENDE CC's (geen kruisreactie),
                   zelfde (kanaal, CC) via de handmatige route in beide volgordes (er blijft er
                   precies een over), zelfde CC-nummer op verschillende kanalen (beide werken),
                   opslaan + herladen met beide bindingen, en crescendo UIT met binding
                   (de CC blijft geclaimd - gekozen gedrag, de UI waarschuwt ervoor).
  6. Extra       : crescendo uit (enabled=false) = geen respons; MIDI-speler stuurt de
                   crescendo-CC door (sinds 0.7.38).

Feiten uit de code (0.7.37/0.7.38, alleen gelezen):
  - state.rs process_crescendo_cc: scaled = trunc(((v-lo)/span).clamp(0,1)*127); invert → 127-scaled;
    trede = 0 als m<4, anders min(N, (m-4)*N//124 + 1), met Schmitt-hysterese H=2 CC rond elke
    grens (crescendo_next_stage). Per drain-batch telt alleen
    de laatste crescendo-CC (coalescing) — bij HTTP-injectie (≥1 ms/request, MIDI-lus 100 µs)
    komt praktisch elke CC in een eigen batch.
  - state.rs handle_midi_message ControlChange: zwel normalized = clamp((v-min)/(max-min)); invert →
    1-normalized; → AppState.division_gains[idx] (GET /division_volumes) + AudioCommand::SetDivisionGain.
  - audio.rs: gain wordt per callback hard gelezen (geen ramp): db = min_db*(1-pos), vol = 10^(db/20),
    plus one-pole low-pass cutoff = cutoff_closed + pos*(20000-cutoff_closed). Default (-20 dB, 800 Hz).
  - audio.rs RegisterStopDivisionMap (elke orgel-load, ook na audio-wissel) zet division_gains audio-
    zijdig op 1.0; AppState-spiegel blijft staan; apply_dsp_after_backend_reload stuurt alleen
    SetSwellConfig, geen SetDivisionGain.
  - /notes/{n}/on stuurt direct AudioCommand::NoteOn en komt NIET in held_notes → een bijkomende trede
    voegt voor zo'n noot geen stem toe (sync_stop_voices_inner: held leeg → return). Daarom spelen we
    noten via /midi/inject (BLE-lus) met een MIDI-mapping per divisie.
  - Crescendo-matrix (stages/enabled/binding) zit sinds 0.7.38 in OrganSettings (.jm-settings.json),
    niet meer in localStorage; de backend herstelt hem bij een orgel-load zonder frontend-push.
    Zonder route POST /crescendo/config blijft stages=[] voor het testorgel → crescendo doet niets.
  - Exclusiviteit loopt sinds deze versie via EEN gedeelde helper (state.rs claim_pedal_cc): een
    zwelkoppeling verdringt de crescendo-koppeling op dezelfde (kanaal, CC) en omgekeerd, in ALLE
    paden (inleer-popup, handmatige invoer, zwel-vinkje, test-API). POST /crescendo/binding geeft de
    verdrongen koppelingen terug in het veld "displaced"; het log noemt ze met kanaal 1-based.
  - state.rs MIDI-lus (0.7.39): de lus wacht nu op ALLE kanalen tegelijk
    (crossbeam Select::ready_timeout) in plaats van thread::sleep(100 us) +
    cmd_rx.recv_timeout(2 ms). Die twee wachtjes duurden op Windows elk een hele
    klok-tik (~15,6 ms), zodat de lus maar ~30x per seconde rondliep en elk
    MIDI-bericht tot ~31 ms bleef liggen. Daardoor las dit script vlak bij een
    trapgrens nog de VORIGE pedaalstand (afwijkingen als (31, 1, 2)) en leek de
    zwelstand door een crescendo-CC te bewegen (beide CC's dragen in de dubbele
    sweep dezelfde waarde). Gemeten injectie -> trapwissel: voor min 10,2 /
    mediaan 29,3 / max 41,4 ms, erna min 0,7 / mediaan 11,0 / max 25,3 ms
    (waarvan ~9 ms HTTP). CRESC_MS = 12 ms is daarmee ruim genoeg.
  - /status heeft er render_overloads (cumulatief aantal callbacks > 80 % van de buffertijd) en
    rt_drops (gedropte realtime audio-commando's) bij — bruikbaar om haperen te kwantificeren.
"""
import json, os, sys, time, random, struct, subprocess, urllib.request, urllib.error
import numpy as np

B = "http://127.0.0.1:8765"
SP = os.path.dirname(os.path.abspath(__file__))
ODF_DIR = os.path.join(SP, "go_testodf")
SAMPLES_SRC = r"C:\Bronbestanden\JM-Orgue\Sample set homemade\Bätz-Witte Puttershoek"
OUT_JSON = os.path.join(SP, "cresc_zwel_result.json")

# ---------------------------------------------------------------------------
# Test-API-routes die dit script nodig heeft en die in 0.7.37 ONTBREKEN
# (voorstel, exact; allemaal route_test_only in test_api.rs):
ROUTES_NODIG = {
    "POST /midi/inject?cc=N&value=V&channel=C":
        "MidiMessage::ControlChange{channel,controller,value} → state.ble_message_tx (zelfde lus als noten; "
        "doorloopt coalescing + process_crescendo_cc + handle_midi_message). Bestaande note-parameters blijven.",
    "GET /crescendo":
        '{"enabled","stage","stages":N,"stage_stops":[[..]],"binding":{"channel","cc","min","max","invert"}|null,'
        '"active_stops":[..]} — leest crescendo_enabled/stage/stages/binding/active_stops.',
    "POST /crescendo/config  {stages:[[stop_id..]..], enabled:bool}":
        "= commands::set_crescendo_config (schrijft crescendo_stages + crescendo_enabled).",
    "POST /crescendo/binding {channel,cc,min?,max?,invert?} | {clear:true}":
        "= set_crescendo_binding_manual + set_crescendo_range + set_crescendo_invert (+ stage=0, "
        "swell-binding op dezelfde CC wissen, SetMasterExpression(1.0)); clear → None.",
    "GET /swell":
        '[{"division","index","position":division_gains[i],"binding":{..}|null,"min_db","cutoff"}] — '
        "position = AppState.division_gains (spiegel, zelfde als /division_volumes).",
    "POST /swell/binding {division,channel,cc,min?,max?,invert?} | {division,clear:true}":
        "= state.set_swell_binding_manual + set_swell_range/set_swell_invert; clear = clear_swell_binding.",
    "POST /midi/mapping {division, channel:int|null, first?:int, last?:int, transpose?:int}":
        "vervangt de MidiChannelMapping voor die divisie (state.set_midi_mappings) — nodig zodat "
        "geïnjecteerde noten geaccepteerd worden en in held_notes komen.",
    "GET /held_notes (optioneel)":
        '{"held":[[ch,note,vel]..]} — debug voor hangers na trapwissels.',
}
# Fallback zonder routes: .jm-settings.json naast een VERSE ODF-bestandsnaam (bibliotheek heeft dan nog
# geen entry → restore_organ_settings importeert het bestand): swell_bindings, crescendo_binding,
# midi_mappings, division_swell_configs zijn zo te seeden. crescendo stages NIET (alleen localStorage).

# ---------------------------------------------------------------------------
# Drempels (dB tenzij anders vermeld)
TH = dict(
    zwel_eind_tol=1.5,        # |Δ(open−dicht) in band <400 Hz − |min_db|| ≤ 1.5
    # Zipper-toets: niet tegen een vaste drempel (de omhullende van een echte
    # pijpopname springt zelf al tot ~0,95 dB per 10 ms, zie RUST_MS) maar tegen
    # DEZELFDE opname in rust, vlak vóór de sweep. Vergeleken wordt het 98e
    # percentiel: een zipper van de zwelgain raakt elk frame en tilt dat percentiel
    # op, terwijl één uitschieter van de sampleset dat niet doet.
    zwel_stap_extra=0.3,      # p98 |Δenv| tijdens de sweep ≤ p98 in rust + 0.3 dB
    zwel_stap_extra_jitter=0.5,
    zwel_mono_tol=0.6,        # schending monotonie op 50 ms-gemiddelde
    zwel_mono_max_schend=3,   # aantal toegestane schendingen
    zwel_sprong_warn=9.0,     # sprong 127→0: tau=20 ms → max 20·(1−e^−0.5)=7.9 dB in het eerste 10 ms-frame; > 9 = geen ramp (WARN)
    klik_ratio=6.0,           # klikdetector: 5 ms-blok hp-piek / mediaan omliggende 500 ms
    cresc_hyst_pass=2,        # flips per 20 oscillaties rond een grens: ≤2 pass
    cresc_hyst_fail=10,       # ≥10 = "geen hysterese" (FAIL)
    # Trapwissel met klinkende noot (zie test_cresc_stage_audio): gemeten in de band
    # rond de grondtoon van de KLINKENDE 8' (60–300 Hz), waar het bijgetrokken 2-voets
    # register niets bijdraagt. Niveau over vensters van ~1,2 s (langer dan de loop van
    # de opname) en de grootste sprong per 10 ms (natuurlijk 0,53 dB; een onderbroken of
    # herstarte stem gaf in de negatieve proef 4,6 dB).
    cresc_f0_niveau=1.5,      # klinkende pijp mag niet van niveau veranderen
    cresc_f0_stap=1.0,        # ... en geen sprong maken groter dan de natuurlijke golf
    cresc_extra_db=6.0,       # bijgetrokken 2' moet in de band >600 Hz erbij komen (gemeten +16,6)
    persist_niveau_tol=1.5,   # zwelstand na herlaad/wissel: |gemeten − verwacht| ≤ 1.5
    spiegel_tol=0.01,         # division_gains exact (0.0 / 1.0 / 0.5±0.01)
)
SWEEP_MS = 15      # ms per CC-stap
CRESC_MS = 12      # ms per CC-stap crescendo (uitlezen erna)
# Rust-/meetvenster rond een sweep. De pijpopnamen van deze sampleset zijn echte
# opnamen met een LOOP van ~1 s; binnen die lus zwelt de amplitude ~±1,7 dB aan en
# af (gemeten: f0-omhullende van Prestant 8' loopt tussen −29,9 en −26,6 dB, periode
# ~1,05 s, maximale stap slechts 0,53 dB per 10 ms). Elk niveau moet daarom over een
# venster van minstens één loopperiode gemeten worden, anders meet je de lus.
RUST_MS = 1200

RESULT = {"pass": [], "fail": [], "warn": [], "info": []}
def ok(msg):   print("  PASS", msg); RESULT["pass"].append(msg)
def fail(msg): print("  FAIL", msg); RESULT["fail"].append(msg)
def warn(msg): print("  WARN", msg); RESULT["warn"].append(msg)
def info(msg): print("  info", msg); RESULT["info"].append(msg)
def check(cond, msg): (ok if cond else fail)(msg)

# ---------------------------------------------------------------------------
# HTTP
def post(p, b=None, t=300):
    d = json.dumps(b).encode() if b is not None else b"{}"
    r = urllib.request.Request(B + p, data=d, method="POST", headers={"Content-Type": "application/json"})
    return json.loads(urllib.request.urlopen(r, timeout=t).read())
def get(p, t=30):
    return json.loads(urllib.request.urlopen(B + p, timeout=t).read())
def try_post(p, b=None):
    try: return post(p, b)
    except urllib.error.HTTPError as e: return {"error": e.code, "body": e.read().decode(errors="replace")[:200]}
    except Exception as e: return {"error": str(e)[:120]}
def try_get(p):
    try: return get(p)
    except urllib.error.HTTPError as e: return {"error": e.code}
    except Exception as e: return {"error": str(e)[:120]}
def has_route(p):
    r = try_get(p)
    return not (isinstance(r, dict) and r.get("error") == 404)

class RouteMissing(Exception): pass

def send_cc(ch, cc, val):
    r = try_post(f"/midi/inject?cc={cc}&value={val}&channel={ch}")
    if "error" in r or r.get("cc") is None and r.get("ok") is not True:
        raise RouteMissing(f"/midi/inject zonder CC-ondersteuning: {r}")
    return r
def note_on_midi(ch, note, vel=100):  return post(f"/midi/inject?note={note}&on=1&velocity={vel}&channel={ch}")
def note_off_midi(ch, note):          return post(f"/midi/inject?note={note}&on=0&channel={ch}")
def swell_binding(name):
    for s_ in get("/swell"):
        if s_["division"] == name: return s_["binding"]
    return None
def overload_counters():
    st = get("/status")
    return int(st.get("render_overloads") or 0), int(st.get("rt_drops") or 0)
def cresc_state():
    s = get("/state")["crescendo"]; return s["enabled"], s["stage"], s["stages"]
def drawn_set():  return set(get("/state")["drawn"])
def swell_pos(idx): return float(get("/division_volumes")["gains"][idx])
def voices():     return get("/status")["voice_count"]
def log_lines(n=400): return get(f"/logs?lines={n}").get("lines", [])
def count_log(prefix, lines): return sum(1 for l in lines if prefix in l)

# ---------------------------------------------------------------------------
# Verwachtingsmodellen (exact zoals de Rust-code)
def mapped_value(v, lo=0, hi=127, invert=False):
    l, h = min(lo, hi), max(lo, hi); span = max(h - l, 1)
    scaled = int(min(max((v - l) / span, 0.0), 1.0) * 127)   # f32 → as u8 = truncatie
    return 127 - scaled if invert else scaled
def expected_stage(v, n, lo=0, hi=127, invert=False):
    """Trap zonder hysterese (crescendo_stage_for)."""
    m = mapped_value(v, lo, hi, invert)
    if m < 4: return 0
    return min(n, (m - 4) * n // 124 + 1)
def stage_bounds(stage, n):
    ms = [m for m in range(128) if expected_stage(m, n) == stage]
    return (ms[0], ms[-1]) if ms else (0, 0)
HYST = 2  # wordt in setup() overschreven door GET /crescendo["hysteresis"]
def next_stage(m, current, n, h=None):
    """Schmitt-beslissing exact als state.rs crescendo_next_stage: None = blijven staan."""
    h = HYST if h is None else h
    target = expected_stage(m, n)
    if target == current: return None
    if current > n: return target
    if target > current:
        _, hi_ = stage_bounds(current, n)
        return target if (m == 127 or m > min(255, hi_ + h)) else None
    if target == 0: return 0
    lo_, _ = stage_bounds(current, n)
    return target if m + h < lo_ else None
class CrescModel:
    """Stateful verwachtingsmodel (trap volgt de hysterese)."""
    def __init__(self, n, lo=0, hi=127, invert=False, stage=0):
        self.n, self.lo, self.hi, self.inv, self.stage = n, lo, hi, invert, stage
    def feed(self, v):
        ns = next_stage(mapped_value(v, self.lo, self.hi, self.inv), self.stage, self.n)
        if ns is not None: self.stage = ns
        return self.stage
def cc_up(stage, n):
    """CC-waarde die vanuit elke lagere trap zeker naar `stage` gaat (bovengrens van de band)."""
    return 127 if stage >= n else stage_bounds(stage, n)[1]
def cc_down(stage, n):
    """CC-waarde die vanuit elke hogere trap zeker naar `stage` gaat (ondergrens van de band)."""
    return 0 if stage == 0 else stage_bounds(stage, n)[0]
def expected_swell(v, lo=0, hi=127, invert=False):
    rng = max(hi - lo, 1.0)
    x = min(max((v - lo) / rng, 0.0), 1.0)
    return 1.0 - x if invert else x
def stage_boundaries(n, lo=0, hi=127, invert=False):
    """Eerste CC-waarde per trede (oplopend v)."""
    b, prev = {}, None
    for v in range(128):
        s = expected_stage(v, n, lo, hi, invert)
        if s != prev: b[v] = s; prev = s
    return b

# ---------------------------------------------------------------------------
# Audio-analyse
def decode_mono(mp3, sr=48000):
    raw = subprocess.run(["ffmpeg", "-v", "error", "-i", mp3, "-f", "f32le", "-ac", "1", "-ar", str(sr), "-"],
                         capture_output=True).stdout
    return np.frombuffer(raw, dtype=np.float32).astype(np.float64), sr
def lowpass_fft(x, sr, fc=400.0):
    X = np.fft.rfft(x); f = np.fft.rfftfreq(len(x), 1 / sr); X[f > fc] = 0
    return np.fft.irfft(X, n=len(x))
def bandpass_fft(x, sr, f1, f2):
    X = np.fft.rfft(x); f = np.fft.rfftfreq(len(x), 1 / sr); X[(f < f1) | (f > f2)] = 0
    return np.fft.irfft(X, n=len(x))
def env_db(x, sr, win_ms=10):
    w = int(sr * win_ms / 1000); n = len(x) // w
    fr = x[:n * w].reshape(n, w)
    rms = np.sqrt((fr ** 2).mean(axis=1)) + 1e-9
    return 20 * np.log10(rms)                      # index i ↔ t = i*win_ms
def smooth(e, k=5):
    return np.convolve(e, np.ones(k) / k, mode="same")
def max_step(e, a_ms, b_ms, win_ms=10):
    seg = e[int(a_ms // win_ms):int(b_ms // win_ms)]
    return float(np.max(np.abs(np.diff(seg)))) if len(seg) > 2 else 0.0
def step_pct(e, a_ms, b_ms, p=98, win_ms=10):
    """Percentiel van |stap per frame| — ongevoelig voor een enkele uitschieter van de
    opname zelf, maar juist gevoelig voor een zipper (die raakt ELK frame)."""
    seg = e[int(a_ms // win_ms):int(b_ms // win_ms)]
    return float(np.percentile(np.abs(np.diff(seg)), p)) if len(seg) > 2 else 0.0
def mono_violations(e, a_ms, b_ms, direction, tol, win_ms=10):
    seg = smooth(e[int(a_ms // win_ms):int(b_ms // win_ms)])
    d = np.diff(seg)
    return int(np.sum(d > tol)) if direction == "down" else int(np.sum(d < -tol))
def level(e, a_ms, b_ms, win_ms=10, fn=np.median):
    return float(fn(e[int(a_ms // win_ms):int(b_ms // win_ms)]))
def click_ratio(x, sr, t_ms, span_ms=60, blk_ms=5, ctx_ms=500):
    """Hoogdoorlaat (1e verschil) → 5 ms-blokpieken; ratio piek in ±span rond t_ms t.o.v. mediaan ±ctx."""
    hp = np.diff(x); blk = int(sr * blk_ms / 1000)
    n = len(hp) // blk; pk = np.abs(hp[:n * blk]).reshape(n, blk).max(axis=1)
    i = int(t_ms / blk_ms); s = int(span_ms / blk_ms); c = int(ctx_ms / blk_ms)
    win = pk[max(0, i - s): i + s]; ctx = pk[max(0, i - c): i + c]
    return float(win.max() / (np.median(ctx) + 1e-12)) if len(win) and len(ctx) else 0.0
def find_edge(e, t_cmd_ms, search=(-100, 300), win_ms=10):
    """Grootste niveauverandering (50 ms-gemiddelde) rond het commandotijdstip → ms."""
    a, b = int((t_cmd_ms + search[0]) // win_ms), int((t_cmd_ms + search[1]) // win_ms)
    seg = smooth(e[max(0, a):b]); d = np.abs(np.diff(seg))
    return (max(0, a) + int(np.argmax(d))) * win_ms if len(d) else t_cmd_ms

# ---------------------------------------------------------------------------
# Test-ODF: Hoofdwerk (7 registers, open) + Nevenwerk (3 registers, in zwelkast Enclosure001)
HW_REGS = ["Prestant_8", "Roerfluit_8", "Octaaf_4", "Quint_3", "Woudfluit_2", "Mixtuur_4st", "Trompet_8"]
NW_REGS = ["Holfluit_8", "Salicet_4", "Gemshoorn_2"]
def notes_in(reg_dir):
    m = {}
    for f in os.listdir(reg_dir):
        if f.lower().endswith((".wav", ".mp3")) and f[:3].isdigit(): m[int(f[:3])] = f
    return m
def _filter_regs(regs, mname):
    base = os.path.join(SAMPLES_SRC, mname)
    return [r for r in regs if os.path.isdir(os.path.join(base, r)) and len(notes_in(os.path.join(base, r))) >= 20]
HW_REGS = _filter_regs(HW_REGS, "Hoofdwerk"); NW_REGS = _filter_regs(NW_REGS, "Nevenwerk")
def make_test_odf(name):
    os.makedirs(ODF_DIR, exist_ok=True)
    link = os.path.join(ODF_DIR, "Samples")
    if not os.path.exists(link):
        subprocess.run(["cmd", "/c", "mklink", "/J", link.replace("/", "\\"), SAMPLES_SRC], check=True, capture_output=True)
    manuals = [("Hoofdwerk", HW_REGS, 1), ("Nevenwerk", NW_REGS, 2)]   # (naam, registers, windchest)
    L = ["[Organ]", "ChurchName=TestCresc", "ChurchAddress=Scratch", "OrganBuilder=JM-Orgue test",
         "NumberOfManuals=2", "HasPedals=N", "NumberOfEnclosures=1", "NumberOfTremulants=0",
         "NumberOfWindchestGroups=2", f"NumberOfRanks={len(HW_REGS) + len(NW_REGS)}", "",
         "[Enclosure001]", "Name=Zwelkast", "AmpMinimumLevel=20", "MIDIInputNumber=0", "",
         "[WindchestGroup001]", "Name=Hoofdwerk", "NumberOfEnclosures=0", "NumberOfTremulants=0", "",
         "[WindchestGroup002]", "Name=Nevenwerk", "NumberOfEnclosures=1", "Enclosure001=001", "NumberOfTremulants=0", ""]
    stop_no, rank_no, ranks, ranges = 0, 0, [], {}
    for mi, (mname, regs, wc) in enumerate(manuals, 1):
        base = os.path.join(SAMPLES_SRC, mname)
        notes = {r: notes_in(os.path.join(base, r)) for r in regs}
        common = sorted(set.intersection(*[set(n) for n in notes.values()]))
        first, n = common[0], len(common); ranges[mname] = (first, common[-1])
        stops = list(range(stop_no + 1, stop_no + 1 + len(regs)))
        L += [f"[Manual{mi:03d}]", f"Name={mname}", f"MIDIInputNumber={mi}", f"NumberOfLogicalKeys={n}",
              f"NumberOfAccessibleKeys={n}", f"FirstAccessibleKeyMIDINoteNumber={first}",
              f"NumberOfStops={len(regs)}"] + [f"Stop{i:03d}={s:03d}" for i, s in enumerate(stops, 1)] + \
             ["NumberOfCouplers=0", "NumberOfDivisionals=0", "NumberOfTremulants=0", "NumberOfSwitches=0", ""]
        for r, s in zip(regs, stops):
            rank_no += 1
            L += [f"[Stop{s:03d}]", f"Name={r}", "NumberOfRanks=1", f"Rank001={rank_no:03d}",
                  "FirstAccessiblePipeLogicalKeyNumber=1", f"NumberOfAccessiblePipes={n}", ""]
            R = [f"[Rank{rank_no:03d}]", f"Name={r}", f"WindchestGroup={wc:03d}", f"FirstMidiNoteNumber={first}",
                 f"NumberOfLogicalPipes={n}", "Percussive=N", "HarmonicNumber=8", "AmplitudeLevel=100"]
            R += [f"Pipe{i:03d}=Samples\\{mname}\\{r}\\{notes[r][nt]}" for i, nt in enumerate(common, 1)]
            ranks += R + [""]
        stop_no += len(regs)
    odf = os.path.join(ODF_DIR, name)
    open(odf, "w", encoding="utf-8", newline="\r\n").write("\n".join(L + ranks))
    return odf, ranges

def seed_settings_file(odf_dir, swell_div, swell_idx, sw, cr):
    """Fallback zonder routes: .jm-settings.json naast de ODF (alleen geïmporteerd bij een orgel-id
    ZONDER bibliotheek-entry → gebruik een verse ODF-naam)."""
    s = {"presets": {}, "preset_bindings": [],
         "swell_bindings": [{"division_name": swell_div, "division_index": swell_idx, "channel": sw["ch"],
                             "cc_num": sw["cc"], "min_val": sw.get("min", 0), "max_val": sw.get("max", 127),
                             "invert": sw.get("invert", False)}],
         "midi_mappings": [{"division": "Hoofdwerk", "channel": 0, "transpose": 0, "first_midi_note": None, "last_midi_note": None},
                           {"division": "Nevenwerk", "channel": 1, "transpose": 0, "first_midi_note": None, "last_midi_note": None}],
         "crescendo_binding": {"channel": cr["ch"], "cc_num": cr["cc"], "min_val": cr.get("min", 0),
                               "max_val": cr.get("max", 127), "invert": cr.get("invert", False)},
         "division_swell_configs": [{"division": swell_div, "min_db": -20.0, "filter_cutoff": 800.0}]}
    open(os.path.join(odf_dir, ".jm-settings.json"), "w", encoding="utf-8").write(json.dumps(s, indent=2))

# ---------------------------------------------------------------------------
# Minimale SMF-schrijver (fallback voor zwel via /midi/player/play; 1 tick = 1 ms)
def vlq(n):
    out = [n & 0x7F]; n >>= 7
    while n: out.append((n & 0x7F) | 0x80); n >>= 7
    return bytes(reversed(out))
def write_smf(path, events):
    """events: [(t_ms, bytes)] absoluut; tempo 1 000 000 µs/kwartnoot, PPQ 1000 → tick = ms."""
    trk = b"\x00\xFF\x51\x03" + (1_000_000).to_bytes(3, "big"); last = 0
    for t, b in sorted(events, key=lambda e: e[0]):
        trk += vlq(int(t - last)) + b; last = int(t)
    trk += b"\x00\xFF\x2F\x00"
    open(path, "wb").write(b"MThd" + struct.pack(">IHHH", 6, 0, 1, 1000) + b"MTrk" + struct.pack(">I", len(trk)) + trk)

# ---------------------------------------------------------------------------
# Setup
class Ctx: pass
def setup():
    c = Ctx()
    c.routes = {"cresc": has_route("/crescendo"), "swell": has_route("/swell")}
    c.have_routes = c.routes["cresc"] and c.routes["swell"]
    c.sw = {"ch": 1, "cc": 11}       # zweltrede Nevenwerk: kanaal 2 (0-based 1), CC 11
    c.cr = {"ch": 0, "cc": 20}       # crescendotrede: kanaal 1 (0-based 0), CC 20
    name = "TestCresc.organ" if c.have_routes else time.strftime("TestCresc_%H%M%S.organ")
    c.odf, c.ranges = make_test_odf(name)
    if not c.have_routes:
        seed_settings_file(ODF_DIR, "Nevenwerk", 1, c.sw, c.cr)
        info("geen /crescendo of /swell-route: seeding via .jm-settings.json + verse ODF-naam " + name)
    # MIDI-archief uit (geïnjecteerde noten zouden anders takes schrijven)
    c.archive_was = try_get("/midi/archive/status").get("enabled")
    if c.archive_was: try_post("/midi/archive/config?enabled=0")
    try_post("/settings/wind?enabled=0")
    post("/load_organ", {"path": c.odf}); time.sleep(4.0)
    o = get("/organ"); c.div_names = [d["name"] for d in o["divisions"]]
    c.idx = {d["name"]: i for i, d in enumerate(o["divisions"])}
    c.stops = {s["name"]: s["id"] for d in o["divisions"] for s in d["stops"]}
    check(c.idx.get("Nevenwerk") == 1, f"divisievolgorde {c.div_names} (Nevenwerk = index 1)")
    check(any(d["name"] == "Nevenwerk" and d.get("has_swell") for d in o["divisions"]), "Nevenwerk herkend als zwelkast (has_swell)")
    for s in (x for d in o["divisions"] for x in d["stops"] if x.get("drawn")): post(f"/stops/{s['id']}/toggle")
    if c.have_routes:
        post("/midi/mapping", {"division": "Hoofdwerk", "channel": 0})
        post("/midi/mapping", {"division": "Nevenwerk", "channel": 1})
        post("/swell/binding", {"division": "Nevenwerk", **{k: v for k, v in c.sw.items() if k != "ch"}, "channel": c.sw["ch"]})
        post("/crescendo/binding", {"channel": c.cr["ch"], "cc": c.cr["cc"], "min": 0, "max": 127, "invert": False})
    else:
        so = try_get("/settings/organ")
        if not so.get("swell_bindings"): sys.exit("seed niet geïmporteerd (bibliotheek-entry bestond al?) — stop")
    # Matrix: cumulatief, 7 treden over de Hoofdwerk-registers (stage k = eerste k registers)
    c.N = len(HW_REGS)
    c.stages = [[c.stops[r] for r in HW_REGS[:k]] for k in range(1, c.N + 1)]
    r = try_post("/crescendo/config", {"stages": c.stages, "enabled": True})
    if "error" in r: sys.exit(f"POST /crescendo/config ontbreekt ({r}) — crescendo-tests onmogelijk; zie ROUTES_NODIG")
    time.sleep(1.2)   # frontend-push (localStorage, leeg) bij orgelwissel afwachten, dan opnieuw zetten
    post("/crescendo/config", {"stages": c.stages, "enabled": True})
    en, st, n = cresc_state(); check(en and n == c.N and st == 0, f"crescendo-config actief: enabled={en} stages={n} stage={st}")
    global HYST
    HYST = int(try_get("/crescendo").get("hysteresis", 2)); info(f"hysterese H={HYST} CC-eenheden (backend)")
    c.min_db = -20.0
    for s in try_get("/settings/mirror").get("swells", []):
        if s["division"] == "Nevenwerk": c.min_db = float(s["min_db"])
    info(f"zwel Nevenwerk min_db={c.min_db} (audio-default −20 als geen mirror-entry)")
    return c

def record(path, body):
    post("/record/start", {"path": path}); t0 = time.perf_counter(); time.sleep(0.3)
    marks = body(lambda: (time.perf_counter() - t0) * 1000.0)
    time.sleep(0.4); r = post("/record/stop"); return r, marks

# ---------------------------------------------------------------------------
# 1. Zweltrede
def sweep(ch, cc, seq, ms):
    for v in seq: send_cc(ch, cc, v); time.sleep(ms / 1000.0)

def test_swell_sweep(c, jitter=False):
    label = "zwel-sweep" + ("-jitter" if jitter else "")
    print(f"\n== {label}")
    post(f"/stops/{c.stops['Holfluit_8']}/toggle")
    send_cc(c.sw["ch"], c.sw["cc"], 127); note_on_midi(1, 60); time.sleep(1.5)
    def body(now):
        m = {}
        down = list(range(127, -1, -1)); up = list(range(0, 128))
        if jitter:
            rnd = random.Random(7)
            down = [min(127, max(0, v + rnd.randint(-2, 2))) for v in down]; down[-1] = 0
            up = [min(127, max(0, v + rnd.randint(-2, 2))) for v in up]; up[-1] = 127
        time.sleep(RUST_MS / 1000.0); m["open0"] = now()
        m["down0"] = now(); sweep(c.sw["ch"], c.sw["cc"], down, SWEEP_MS); m["down1"] = now()
        m["pos_dicht"] = swell_pos(c.idx["Nevenwerk"])
        time.sleep((RUST_MS + 150) / 1000.0); m["closed1"] = now()
        m["up0"] = now(); sweep(c.sw["ch"], c.sw["cc"], up, SWEEP_MS); m["up1"] = now()
        m["pos_open"] = swell_pos(c.idx["Nevenwerk"])
        time.sleep((RUST_MS + 150) / 1000.0); m["open1"] = now()
        return m
    out = os.path.join(SP, f"{label}.mp3")
    r, m = record(out, body)
    note_off_midi(1, 60); time.sleep(1.0); post(f"/stops/{c.stops['Holfluit_8']}/toggle")
    check(r.get("rms", 0) > 1e-4, f"opname bevat audio (rms={r.get('rms', 0):.4f}, {r.get('seconds', 0):.1f} s)")
    x, sr = decode_mono(out); e_bb = env_db(x, sr); e_lp = env_db(lowpass_fft(x, sr, 400.0), sr)
    # Vensters van RUST_MS: een echte pijpopname loopt (zie RUST_MS) met een periode
    # van ~1 s ±1,7 dB in amplitude. Korte vensters (de oude 400 ms) meten die golf
    # in plaats van de zwelstand — daar kwam de oude FAIL "weer open na terugsweep
    # +1,5 dB" vandaan.
    open_db = level(e_lp, m["open0"] - RUST_MS, m["open0"]); closed_db = level(e_lp, m["down1"] + 150, m["closed1"])
    open2_db = level(e_lp, m["up1"] + 150, m["open1"]); delta = open_db - closed_db
    check(abs(delta - abs(c.min_db)) <= TH["zwel_eind_tol"], f"dicht t.o.v. open (band<400 Hz): −{delta:.1f} dB (verwacht {c.min_db:+.0f} ±{TH['zwel_eind_tol']})")
    check(abs(open2_db - open_db) <= 1.0, f"weer open na terugsweep: {open2_db - open_db:+.1f} dB t.o.v. eerste open-niveau")
    bb_delta = level(e_bb, m["open0"] - RUST_MS, m["open0"]) - level(e_bb, m["down1"] + 150, m["closed1"])
    info(f"breedband dicht t.o.v. open: −{bb_delta:.1f} dB (incl. low-pass 800 Hz; ≥ {abs(c.min_db) - 1.5:.1f} verwacht)")
    check(abs(m["pos_dicht"] - 0.0) <= TH["spiegel_tol"] and abs(m["pos_open"] - 1.0) <= TH["spiegel_tol"],
          f"spiegel division_gains: dicht={m['pos_dicht']:.3f} open={m['pos_open']:.3f}")
    extra = TH["zwel_stap_extra_jitter"] if jitter else TH["zwel_stap_extra"]
    stap_rust = step_pct(e_bb, m["open0"] - RUST_MS, m["open0"])
    sd, su = step_pct(e_bb, m["down0"], m["down1"]), step_pct(e_bb, m["up0"], m["up1"])
    check(max(sd, su) <= stap_rust + extra,
          f"geen zipper tijdens de sweep: p98 sprong per 10 ms omlaag {sd:.2f} / omhoog {su:.2f} dB "
          f"(zelfde opname in rust {stap_rust:.2f} + {extra})")
    info(f"absolute uitschieters per 10 ms: rust {max_step(e_bb, m['open0'] - RUST_MS, m['open0']):.2f}, "
         f"omlaag {max_step(e_bb, m['down0'], m['down1']):.2f}, omhoog {max_step(e_bb, m['up0'], m['up1']):.2f} dB "
         f"(golf van de opname zelf, niet van de zwel)")
    vd = mono_violations(e_lp, m["down0"], m["down1"], "down", TH["zwel_mono_tol"])
    vu = mono_violations(e_lp, m["up0"], m["up1"], "up", TH["zwel_mono_tol"])
    if not jitter:
        check(vd <= TH["zwel_mono_max_schend"] and vu <= TH["zwel_mono_max_schend"],
              f"monotonie (50 ms-gemiddelde, tol {TH['zwel_mono_tol']} dB): schendingen omlaag {vd}, omhoog {vu}")
    else:
        info(f"jitter: monotonie-schendingen omlaag {vd}, omhoog {vu} (niet beoordeeld; jitter ±2 = ±0.3 dB)")
    return m

def test_swell_jump(c):
    print("\n== zwel-sprong 127→0→127 in één CC (ramp/klik)")
    post(f"/stops/{c.stops['Holfluit_8']}/toggle")
    send_cc(c.sw["ch"], c.sw["cc"], 127); note_on_midi(1, 60); time.sleep(1.5)
    def body(now):
        time.sleep(0.6); t1 = now(); send_cc(c.sw["ch"], c.sw["cc"], 0)
        time.sleep(0.8); t2 = now(); send_cc(c.sw["ch"], c.sw["cc"], 127); time.sleep(0.8)
        return {"t1": t1, "t2": t2}
    out = os.path.join(SP, "zwel-sprong.mp3"); r, m = record(out, body)
    note_off_midi(1, 60); time.sleep(1.0); post(f"/stops/{c.stops['Holfluit_8']}/toggle")
    x, sr = decode_mono(out); e = env_db(x, sr)
    for k, want in (("t1", "omlaag"), ("t2", "omhoog")):
        te = find_edge(e, m[k]); st = max_step(e, te - 30, te + 40)
        cr = click_ratio(x, sr, te)
        (warn if st > TH["zwel_sprong_warn"] else ok)(f"sprong {want}: {st:.1f} dB binnen één 10 ms-frame (rand @ {te:.0f} ms; > {TH['zwel_sprong_warn']} = geen ramp)")
        check(cr <= TH["klik_ratio"], f"klikdetector {want}: ratio {cr:.1f} (≤ {TH['klik_ratio']})")

def test_swell_range_invert(c):
    print("\n== zwel bereik 20..110 en inversie (spiegel division_gains)")
    idx = c.idx["Nevenwerk"]
    for lo, hi, inv in ((20, 110, False), (20, 110, True), (0, 127, True)):
        if c.have_routes:
            post("/swell/binding", {"division": "Nevenwerk", "channel": c.sw["ch"], "cc": c.sw["cc"], "min": lo, "max": hi, "invert": inv})
        else:
            warn("geen /swell/binding-route: bereik/inversie zwel niet instelbaar via API — overgeslagen"); return
        bad = []
        for v in (0, 19, 20, 65, 110, 111, 127):
            send_cc(c.sw["ch"], c.sw["cc"], v); time.sleep(0.03)
            got, want = swell_pos(idx), expected_swell(v, lo, hi, inv)
            if abs(got - want) > TH["spiegel_tol"]: bad.append((v, round(got, 3), round(want, 3)))
        check(not bad, f"zwel bereik {lo}..{hi} invert={inv}: afwijkingen {bad}")
    post("/swell/binding", {"division": "Nevenwerk", "channel": c.sw["ch"], "cc": c.sw["cc"], "min": 0, "max": 127, "invert": False})
    send_cc(c.sw["ch"], c.sw["cc"], 127)

# ---------------------------------------------------------------------------
# 2. Crescendo
def set_cresc_binding(c, lo=0, hi=127, inv=False):
    if c.have_routes:
        post("/crescendo/binding", {"channel": c.cr["ch"], "cc": c.cr["cc"], "min": lo, "max": hi, "invert": inv})
        return True
    if (lo, hi, inv) != (0, 127, False): warn("geen /crescendo/binding-route: bereik/inversie crescendo niet instelbaar — overgeslagen")
    return (lo, hi, inv) == (0, 127, False)

def test_cresc_mapping(c, lo=0, hi=127, inv=False, manual=()):
    label = f"crescendo-mapping lo={lo} hi={hi} invert={inv}" + (f" handmatig={list(manual)}" if manual else "")
    print(f"\n== {label}")
    if not set_cresc_binding(c, lo, hi, inv): return
    manual_ids = {c.stops[r] for r in manual}
    send_cc(c.cr["ch"], c.cr["cc"], 127 if inv else 0); time.sleep(0.1)
    for r in manual: post(f"/stops/{c.stops[r]}/toggle")
    time.sleep(0.1)
    before = count_log("Crescendo: kanaal ", log_lines(2000))
    seq = list(range(0, 128)) + list(range(126, -1, -1))
    model = CrescModel(c.N, lo, hi, inv, stage=cresc_state()[1])
    mism, drawn_bad, changes_up, changes_down, prev = [], [], 0, 0, None
    for i, v in enumerate(seq):
        send_cc(c.cr["ch"], c.cr["cc"], v); time.sleep(CRESC_MS / 1000.0)
        _, st, _ = cresc_state(); want = model.feed(v)
        if st != want: mism.append((v, st, want))
        if prev is not None and st != prev:
            if i < 128: changes_up += 1
            else: changes_down += 1
            exp_drawn = manual_ids | set(c.stages[st - 1] if st > 0 else [])
            got = drawn_set()
            if got != exp_drawn: drawn_bad.append((v, st, sorted(got - exp_drawn), sorted(exp_drawn - got)))
        prev = st
    check(not mism, f"trede per CC-waarde exact volgens model (met hysterese H={HYST}): {len(mism)} afwijkingen {mism[:6]}")
    check(changes_up == c.N and changes_down == c.N, f"aantal trapwissels omhoog {changes_up} / omlaag {changes_down} (verwacht {c.N}/{c.N})")
    check(not drawn_bad, f"getrokken set na elke wissel = handmatig ∪ trede: {len(drawn_bad)} fouten {drawn_bad[:4]}")
    dz = [expected_stage(v, c.N, lo, hi, inv) for v in range(0, 4)]
    info(f"dode zone model: CC0..3 → {dz}; grenzen: {stage_boundaries(c.N, lo, hi, inv)}")
    # De sweep eindigt op CC 0. Bij een GEINVERTEERDE koppeling (het pedaal stuurt
    # 127 -> 0) is CC 0 juist de VOLLE trede en niet de ruststand; de oude
    # verwachting "trede 0 na de sweep" was daar dus fout — hij las de correct
    # ingetrapte trede als fout af. Ga daarom eerst expliciet naar de ruststand
    # (de pedaalwaarde die op pedaalstand 0 uitkomt) en controleer die.
    rust = hi if inv else lo
    send_cc(c.cr["ch"], c.cr["cc"], rust); time.sleep(CRESC_MS / 1000.0 + 0.05)
    _, st_end, _ = cresc_state(); got = drawn_set()
    check(st_end == 0 and got == manual_ids,
          f"na terugtreden naar de ruststand (CC {rust}): trede {st_end}, drawn={sorted(got)} (verwacht alleen handmatig {sorted(manual_ids)})")
    after = count_log("Crescendo: kanaal ", log_lines(2000))
    info(f"log 'Crescendo: kanaal N CCxx' regels tijdens sweep: {after - before} (verwacht {2 * c.N})")
    for r in manual: post(f"/stops/{c.stops[r]}/toggle")
    set_cresc_binding(c)

def test_cresc_hysteresis(c):
    print("\n== crescendo hysterese rond een trapgrens")
    vb = stage_bounds(3, c.N)[0]   # eerste CC van trede 3 (zonder hysterese)
    send_cc(c.cr["ch"], c.cr["cc"], cc_up(2, c.N)); time.sleep(0.05)
    check(cresc_state()[1] == 2, f"uitgangspositie trede 2 (CC {cc_up(2, c.N)})")
    send_cc(c.cr["ch"], c.cr["cc"], vb - 1); time.sleep(0.05)
    before = count_log("Crescendo: kanaal ", log_lines(2000)); flips = 0; prev = cresc_state()[1]
    for i in range(40):
        send_cc(c.cr["ch"], c.cr["cc"], vb if i % 2 == 0 else vb - 1); time.sleep(0.03)
        st = cresc_state()[1]; flips += (st != prev); prev = st
    logs = count_log("Crescendo: kanaal ", log_lines(2000)) - before
    msg = f"20 oscillaties CC {vb - 1}↔{vb}: {flips} trapwissels (log {logs}); pass ≤ {TH['cresc_hyst_pass']}, fail ≥ {TH['cresc_hyst_fail']}"
    (ok if flips <= TH["cresc_hyst_pass"] else fail if flips >= TH["cresc_hyst_fail"] else warn)(msg)
    send_cc(c.cr["ch"], c.cr["cc"], 0); time.sleep(0.1)

def test_cresc_stage_audio(c):
    """Trapwissel terwijl een noot klinkt: loopt de KLINKENDE pijp ongestoord door en
    is het bijgetrokken register echt hoorbaar?

    Waarom niet meer met twee 8-voets registers (de oude opzet: Prestant 8' klinkt,
    de trap voegt Roerfluit 8' toe): twee pijpen op dezelfde nominale toonhoogte
    zweven tegen elkaar in. Het gemeten niveau schommelt dan om natuurkundige
    redenen — met beide registers constant aan, ZONDER enige trapwissel, meet
    dezelfde statistiek een mediane "dip" van −1,3 dB en uitschieters tot −3,8 dB.
    Daar bovenop loopt elke opname van deze sampleset met een periode van ~1 s
    waarin de amplitude ~±1,7 dB aanzwelt en afneemt (Prestant 8' alleen, zonder
    wissel: dips tot −3,3 dB). De oude drempels (dip ≥ −1 dB, stijging ≥ +1 dB,
    piek ≤ +1,5 dB) maten dus de sampleset en niet de app; ze konden niet slagen.

    Wat wél scherp meet — en ook echt uitslaat als de klinkende stem opnieuw wordt
    gestart of even wordt onderbroken (gecontroleerd met een negatieve proef waarin
    de trap de 8' juist WEGtrok: sprong 4,6 dB/10 ms en −55 dB niveau):
      - band 60–300 Hz = alleen de grondtoon van de klinkende 8' (G3 = 196 Hz);
        het bijgetrokken 2-voets register (784 Hz) heeft daar geen energie. Die band
        moet onveranderd doorlopen (≤ 1,5 dB) en mag geen sprong maken die groter is
        dan de natuurlijke golf van dezelfde opname vlak vóór de wissel.
      - band >600 Hz = alleen het bijgetrokken 2'. Dat moet er hoorbaar bij komen
        (gemeten +16,6 dB) en bij het terugtreden weer verdwijnen.
    """
    print("\n== crescendo trapwissel met klinkende noot (loopt de klinkende pijp door?)")
    extra = next((r for r in HW_REGS if r.endswith("_2")), None)
    if extra is None:
        warn("geen 2-voets register in de testset — trapwissel-audio overgeslagen"); return
    grond, NOOT, n = HW_REGS[0], 55, 2
    # Eigen 2-trapsmatrix: trede 1 = de klinkende 8', trede 2 = diezelfde 8' + het 2'.
    post("/crescendo/config", {"stages": [[c.stops[grond]], [c.stops[grond], c.stops[extra]]], "enabled": True})
    time.sleep(0.15)
    v1, v2, v1_down = cc_up(1, n), cc_up(2, n), cc_down(1, n)
    send_cc(c.cr["ch"], c.cr["cc"], 0); time.sleep(0.1)
    send_cc(c.cr["ch"], c.cr["cc"], v1); time.sleep(0.15)
    check(drawn_set() == {c.stops[grond]}, f"trede 1 trekt alleen {grond}")
    note_on_midi(0, NOOT); time.sleep(1.5); n1 = voices()
    def body(now):
        time.sleep(1.2); t1 = now(); send_cc(c.cr["ch"], c.cr["cc"], v2)
        time.sleep(0.4); nv = voices(); time.sleep(1.4)
        t2 = now(); send_cc(c.cr["ch"], c.cr["cc"], v1_down); time.sleep(1.8)
        return {"t1": t1, "t2": t2, "n_up": nv}
    out = os.path.join(SP, "cresc-trapwissel.mp3"); r, m = record(out, body)
    time.sleep(0.5); n3 = voices()
    note_off_midi(0, NOOT); time.sleep(1.5); send_cc(c.cr["ch"], c.cr["cc"], 0)
    check(m["n_up"] == n1 + 1, f"stemmen: {n1} → {m['n_up']} na trede omhoog (verwacht +1: {extra} speelt de vastgehouden toets mee)")
    check(n3 <= n1, f"stemmen na trede omlaag + release: {n3} (≤ {n1})")
    x, sr = decode_mono(out)
    e0 = env_db(bandpass_fft(x, sr, 60, 300), sr)        # alleen de klinkende 8'
    eh = env_db(bandpass_fft(x, sr, 600, 12000), sr)     # alleen het bijgetrokken 2'
    te, te2 = find_edge(eh, m["t1"]), find_edge(eh, m["t2"])
    ref0, refh = level(e0, te - 1100, te - 100), level(eh, te - 1100, te - 100)
    d_op = level(e0, te + 300, te + 1300) - ref0
    d_neer = level(e0, te2 + 300, te2 + 1300) - ref0
    check(abs(d_op) <= TH["cresc_f0_niveau"],
          f"trede omhoog: de klinkende {grond} houdt zijn niveau ({d_op:+.2f} dB in de band 60–300 Hz, ≤ {TH['cresc_f0_niveau']})")
    check(abs(d_neer) <= TH["cresc_f0_niveau"],
          f"trede omlaag: de klinkende {grond} houdt zijn niveau ({d_neer:+.2f} dB)")
    rust_stap = max_step(e0, te - 1100, te - 200)
    s_op, s_neer = max_step(e0, te - 100, te + 300), max_step(e0, te2 - 100, te2 + 300)
    check(max(s_op, s_neer) <= max(TH["cresc_f0_stap"], rust_stap + 0.3),
          f"geen onderbreking van de klinkende pijp: grootste sprong {max(s_op, s_neer):.2f} dB/10 ms "
          f"(zelfde opname zonder wissel: {rust_stap:.2f})")
    h_op = level(eh, te + 300, te + 1300) - refh
    h_neer = level(eh, te2 + 300, te2 + 1300) - refh
    check(h_op >= TH["cresc_extra_db"],
          f"trede omhoog: {extra} komt er hoorbaar bij ({h_op:+.1f} dB boven 600 Hz, ≥ +{TH['cresc_extra_db']})")
    check(abs(h_neer) <= TH["cresc_f0_niveau"],
          f"trede omlaag: {extra} is weer weg ({h_neer:+.1f} dB boven 600 Hz t.o.v. vóór de wissel)")
    check(click_ratio(x, sr, te) <= TH["klik_ratio"], f"klikdetector trede omhoog: ratio {click_ratio(x, sr, te):.1f}")
    check(click_ratio(x, sr, te2) <= TH["klik_ratio"], f"klikdetector trede omlaag: ratio {click_ratio(x, sr, te2):.1f}")
    info(f"trapwissel gemeten op randen {te:.0f} / {te2:.0f} ms; 8' {d_op:+.2f}/{d_neer:+.2f} dB, "
         f"2' {h_op:+.1f}/{h_neer:+.1f} dB")
    post("/crescendo/config", {"stages": c.stages, "enabled": True}); time.sleep(0.1)

def test_cresc_disabled_and_exclusive(c):
    print("\n== crescendo uit / CC exclusief")
    post("/crescendo/config", {"stages": c.stages, "enabled": False}); time.sleep(0.05)
    send_cc(c.cr["ch"], c.cr["cc"], 127); time.sleep(0.05)
    _, st, _ = cresc_state(); check(st == 0 and not drawn_set(), f"enabled=false: CC127 → trede {st}, drawn={sorted(drawn_set())} (verwacht 0/leeg)")
    send_cc(c.cr["ch"], c.cr["cc"], 0); post("/crescendo/config", {"stages": c.stages, "enabled": True}); time.sleep(0.05)
    if not c.have_routes: warn("exclusiviteitstest vereist /crescendo/binding — overgeslagen"); return
    # crescendo op dezelfde CC/kanaal als de zwel → zwel mag niet reageren, binding zwel is gewist
    post("/crescendo/binding", {"channel": c.sw["ch"], "cc": c.sw["cc"]}); time.sleep(0.05)
    idx = c.idx["Nevenwerk"]; p0 = swell_pos(idx)
    send_cc(c.sw["ch"], c.sw["cc"], 0); time.sleep(0.05)
    check(abs(swell_pos(idx) - p0) <= 1e-6, f"zwel reageert niet op een als crescendo geclaimde CC (pos {p0:.2f} → {swell_pos(idx):.2f})")
    sw = [s for s in get("/swell") if s["division"] == "Nevenwerk"][0]
    check(sw["binding"] is None, "zwel-binding op dezelfde CC gewist (laatst ingeleerd wint)")
    send_cc(c.sw["ch"], c.sw["cc"], 0); time.sleep(0.05); send_cc(c.cr["ch"], c.cr["cc"], 0)
    # herstel
    post("/crescendo/binding", {"channel": c.cr["ch"], "cc": c.cr["cc"]})
    post("/swell/binding", {"division": "Nevenwerk", "channel": c.sw["ch"], "cc": c.sw["cc"]})
    send_cc(c.sw["ch"], c.sw["cc"], 127); time.sleep(0.05)

def _zet_basisbindingen(c):
    """Zwel op c.sw, crescendo op c.cr, crescendo aan — de uitgangsstand van de suite."""
    post("/swell/binding", {"division": "Nevenwerk", "channel": c.sw["ch"], "cc": c.sw["cc"], "min": 0, "max": 127, "invert": False})
    post("/crescendo/binding", {"channel": c.cr["ch"], "cc": c.cr["cc"], "min": 0, "max": 127, "invert": False})
    post("/crescendo/config", {"stages": c.stages, "enabled": True})
    send_cc(c.cr["ch"], c.cr["cc"], 0); send_cc(c.sw["ch"], c.sw["cc"], 127); time.sleep(0.1)

def test_beide_treden_tegelijk(c):
    """Beide bindingen tegelijk op VERSCHILLENDE CC's: geen kruisreactie (de kernklacht
    'als het een werkt, werkt het ander niet'). Daarna dezelfde test met hetzelfde
    CC-nummer op twee verschillende kanalen."""
    print("\n== beide treden tegelijk (verschillende CC's)")
    if not c.have_routes:
        warn("vereist /swell/binding + /crescendo/binding — overgeslagen"); return
    idx = c.idx["Nevenwerk"]
    _zet_basisbindingen(c)
    check(swell_binding("Nevenwerk") is not None and try_get("/crescendo").get("binding") is not None,
          "beide bindingen staan tegelijk in de backend")
    # Akkoord op Hoofdwerk vasthouden: een trapwissel start/stopt dan echt stemmen
    # (dat is precies de burst die bij kleine buffers het haperen gaf).
    ov0, drops0 = overload_counters()
    for n in (55, 60, 64): note_on_midi(0, n)
    time.sleep(0.5)
    model = CrescModel(c.N)
    fout_pos, fout_stage = [], []
    for v in list(range(0, 128, 4)) + list(range(124, -1, -4)):
        # (a) zwel-CC: alleen de zwelstand mag bewegen
        st_voor = cresc_state()[1]
        send_cc(c.sw["ch"], c.sw["cc"], v); time.sleep(CRESC_MS / 1000.0)
        st_na = cresc_state()[1]
        if st_na != st_voor: fout_stage.append(("zwel-cc", v, st_voor, st_na))
        pos = swell_pos(idx)
        if abs(pos - expected_swell(v)) > 0.02: fout_pos.append(("zwel-cc", v, round(pos, 3)))
        # (b) crescendo-CC: alleen de trap mag bewegen
        pos_voor = swell_pos(idx)
        verwacht = model.feed(v)
        send_cc(c.cr["ch"], c.cr["cc"], v); time.sleep(CRESC_MS / 1000.0)
        st_na = cresc_state()[1]
        if st_na != verwacht: fout_stage.append(("cresc-cc", v, verwacht, st_na))
        pos_na = swell_pos(idx)
        if abs(pos_na - pos_voor) > TH["spiegel_tol"]: fout_pos.append(("cresc-cc", v, round(pos_na, 3)))
    for n in (55, 60, 64): note_off_midi(0, n)
    time.sleep(0.4)
    check(not fout_pos, f"zwelstand volgt uitsluitend CC{c.sw['cc']} ({len(fout_pos)} afwijkingen: {fout_pos[:4]})")
    check(not fout_stage, f"crescendotrap volgt uitsluitend CC{c.cr['cc']} ({len(fout_stage)} afwijkingen: {fout_stage[:4]})")
    ov1, drops1 = overload_counters()
    info(f"tijdens de dubbele sweep: {ov1 - ov0} zware callbacks erbij, {drops1 - drops0} gedropte realtime-commando's "
         f"(buffer {get('/status').get('buffer_frames')} frames)")
    send_cc(c.cr["ch"], c.cr["cc"], 0); time.sleep(0.1)

    # Zelfde CC-nummer, verschillend kanaal: geen van beide claimt de ander.
    print("== zelfde CC-nummer op twee kanalen")
    r = try_post("/swell/binding", {"division": "Nevenwerk", "channel": 1, "cc": 7})
    r2 = try_post("/crescendo/binding", {"channel": 0, "cc": 7})
    check(not (r2.get("displaced") or []), "crescendo op kanaal 1/CC7 verdringt de zwel op kanaal 2/CC7 NIET")
    check(swell_binding("Nevenwerk") is not None and try_get("/crescendo").get("binding") is not None,
          "beide bindingen bestaan naast elkaar op hetzelfde CC-nummer, ander kanaal")
    send_cc(1, 7, 0); time.sleep(0.08)
    check(abs(swell_pos(idx)) <= TH["spiegel_tol"], f"zwel volgt kanaal 2/CC7 (pos {swell_pos(idx):.2f})")
    send_cc(0, 7, 127); time.sleep(0.08)
    check(cresc_state()[1] == c.N, f"crescendo volgt kanaal 1/CC7 (trede {cresc_state()[1]})")
    send_cc(0, 7, 0); time.sleep(0.08)
    _zet_basisbindingen(c)

def test_handmatige_exclusiviteit(c):
    """Zelfde (kanaal, CC) via de HANDMATIGE route, in BEIDE volgordes: er mag er precies
    een overblijven. Vóór de fix leverde deze route twee koppelingen op waarvan de zwel
    stilzwijgend dood was (de crescendo-claim keert eerder terug)."""
    print("\n== handmatige route: zelfde CC in beide volgordes")
    if not c.have_routes:
        warn("vereist /swell/binding + /crescendo/binding — overgeslagen"); return
    CH, CC = 2, 21          # kanaal 3, ongebruikte CC
    # (a) eerst zwel, daarna crescendo
    post("/swell/binding", {"division": "Nevenwerk", "channel": CH, "cc": CC})
    r = try_post("/crescendo/binding", {"channel": CH, "cc": CC})
    verdrongen = r.get("displaced") or []
    check(swell_binding("Nevenwerk") is None, "zwel→crescendo: de zwelkoppeling op dezelfde CC is verdrongen")
    check((try_get("/crescendo").get("binding") or {}).get("cc") == CC, "zwel→crescendo: de crescendo-koppeling staat er")
    check(any(d.get("kind") == "swell" for d in verdrongen),
          f"backend meldt de verdrongen zwelkoppeling terug (displaced={verdrongen})")
    # (b) omgekeerd: crescendo staat er al, nu handmatig een zwel op dezelfde CC
    post("/swell/binding", {"division": "Nevenwerk", "channel": CH, "cc": CC})
    check(try_get("/crescendo").get("binding") is None, "crescendo→zwel: de crescendo-koppeling is verdrongen")
    check((swell_binding("Nevenwerk") or {}).get("cc") == CC, "crescendo→zwel: de zwelkoppeling staat er")
    # functioneel: de trede stuurt nu echt de zwelkast
    idx = c.idx["Nevenwerk"]
    send_cc(CH, CC, 0); time.sleep(0.08)
    check(abs(swell_pos(idx)) <= TH["spiegel_tol"], f"de trede werkt als zwelkast (pos {swell_pos(idx):.2f})")
    lines = log_lines(300)
    check(count_log("die trede is nu het generaal crescendo", lines) >= 1,
          "logregel over de verdrongen zwelkoppeling aanwezig")
    check(count_log("die trede is nu een zwelkast", lines) >= 1,
          "logregel over de verdrongen crescendo-koppeling aanwezig")
    _zet_basisbindingen(c)

def test_persistentie_beide_bindingen(c):
    """Opslaan + herladen met BEIDE bindingen (op verschillende CC's): identiek terug en
    allebei functioneel, zonder frontend-push."""
    print("\n== opslaan + herladen met beide bindingen")
    if not c.have_routes:
        warn("vereist /swell/binding + /crescendo/binding — overgeslagen"); return
    _zet_basisbindingen(c)
    sw0, cr0 = swell_binding("Nevenwerk"), try_get("/crescendo").get("binding")
    post("/settings/save"); time.sleep(0.3)
    post("/load_organ", {"path": c.odf}); time.sleep(4.0)
    sw1, cr1 = swell_binding("Nevenwerk"), try_get("/crescendo").get("binding")
    for veld in ("channel", "cc", "min", "max", "invert"):
        check(sw0 and sw1 and sw0.get(veld) == sw1.get(veld), f"zwelbinding veld {veld} identiek na herlaad ({sw0} → {sw1})")
        check(cr0 and cr1 and cr0.get(veld) == cr1.get(veld), f"crescendobinding veld {veld} identiek na herlaad ({cr0} → {cr1})")
    en, st, n = cresc_state()
    check(en and n == c.N, f"crescendo-config uit OrganSettings hersteld: enabled={en} stages={n} (zonder frontend-push)")
    idx = c.idx["Nevenwerk"]
    send_cc(c.sw["ch"], c.sw["cc"], 0); time.sleep(0.08)
    check(abs(swell_pos(idx)) <= TH["spiegel_tol"], "zweltrede functioneel na herlaad")
    send_cc(c.cr["ch"], c.cr["cc"], 127); time.sleep(0.08)
    check(cresc_state()[1] == c.N, f"crescendotrede functioneel na herlaad (trede {cresc_state()[1]})")
    _zet_basisbindingen(c)

def test_crescendo_uit_met_binding(c):
    """A3: crescendo UIT (of zonder trappen) terwijl er wel een crescendo-binding staat.
    Gekozen gedrag (optie a uit het dossier): de claim blijft staan, dus die trede doet
    dan helemaal niets — ook niet als zwelkast. De UI waarschuwt daarvoor; deze test legt
    het gedrag vast zodat het niet ongemerkt verandert."""
    print("\n== crescendo uit met binding (dode trede)")
    if not c.have_routes:
        warn("vereist /swell/binding + /crescendo/binding — overgeslagen"); return
    CH, CC = c.cr["ch"], c.cr["cc"]
    idx = c.idx["Nevenwerk"]
    _zet_basisbindingen(c)
    post("/crescendo/config", {"stages": c.stages, "enabled": False}); time.sleep(0.08)
    # Zwel handmatig op DEZELFDE CC: de (uitgeschakelde) crescendo-koppeling wijkt.
    post("/swell/binding", {"division": "Nevenwerk", "channel": CH, "cc": CC})
    check(try_get("/crescendo").get("binding") is None,
          "een zwelkoppeling verdringt ook een crescendo-koppeling die uit staat")
    send_cc(CH, CC, 64); time.sleep(0.08)
    pos_mid = swell_pos(idx)
    check(abs(pos_mid - expected_swell(64)) <= 0.02, f"de trede werkt nu als zwelkast (pos {pos_mid:.2f})")
    # Crescendo-koppeling terug op dezelfde CC: de zwel wijkt en de trede is dood.
    r = try_post("/crescendo/binding", {"channel": CH, "cc": CC})
    check(any(d.get("kind") == "swell" for d in (r.get("displaced") or [])),
          "crescendo-koppeling meldt de verdrongen zwelkoppeling")
    check(swell_binding("Nevenwerk") is None, "zwelkoppeling verdrongen door de crescendo-koppeling")
    send_cc(CH, CC, 0); time.sleep(0.08); send_cc(CH, CC, 127); time.sleep(0.08)
    check(abs(swell_pos(idx) - pos_mid) <= TH["spiegel_tol"],
          f"crescendo uit + binding = dode trede: zwelstand blijft {pos_mid:.2f} (nu {swell_pos(idx):.2f})")
    check(cresc_state()[1] == 0, f"crescendo uit: trede blijft 0 (nu {cresc_state()[1]})")
    # Zelfde geval met crescendo AAN maar lege matrix.
    post("/crescendo/config", {"stages": [[] for _ in range(c.N)], "enabled": True}); time.sleep(0.08)
    send_cc(CH, CC, 127); time.sleep(0.08)
    check(not drawn_set(), f"lege matrix: CC127 trekt geen registers (drawn={sorted(drawn_set())})")
    send_cc(CH, CC, 0); time.sleep(0.08)
    _zet_basisbindingen(c)

def test_player_cc(c):
    print("\n== MIDI-speler: CC-events (zwel volgt? crescendo volgt?)")
    mid = os.path.join(SP, "cresc_zwel_player.mid"); idx = c.idx["Nevenwerk"]
    ev = [(0, bytes([0xB0 | c.sw["ch"], c.sw["cc"], 127])), (200, bytes([0xB0 | c.sw["ch"], c.sw["cc"], 0])),
          (400, bytes([0xB0 | c.cr["ch"], c.cr["cc"], 127])), (900, bytes([0xB0 | c.cr["ch"], c.cr["cc"], 0])),
          (1100, bytes([0xB0 | c.sw["ch"], c.sw["cc"], 127]))]
    write_smf(mid, ev)
    send_cc(c.sw["ch"], c.sw["cc"], 127); send_cc(c.cr["ch"], c.cr["cc"], 0); time.sleep(0.05)
    post("/midi/player/play", {"path": mid}); time.sleep(0.3); p_mid = swell_pos(idx)
    time.sleep(0.35); st_mid = cresc_state()[1]; time.sleep(0.8); post("/midi/player/stop")
    check(abs(p_mid - 0.0) <= TH["spiegel_tol"], f"speler: zwel-CC toegepast (pos {p_mid:.2f})")
    check(st_mid == c.N, f"speler: crescendo-CC 127 → trede {st_mid} (verwacht {c.N}; speler-lus past de trede toe sinds 0.7.38)")
    send_cc(c.cr["ch"], c.cr["cc"], 0); send_cc(c.sw["ch"], c.sw["cc"], 127)

# ---------------------------------------------------------------------------
# 4. Persistentie
def snapshot_bindings():
    """Koppelingen zonder de LIVE pedaalstand. `last_value` in een swell_binding is
    geen instelling maar de laatst ontvangen CC-waarde: die hoort juist mee te
    bewegen met het pedaal en wordt met opzet over een herlaad/audio-wissel heen
    bewaard (zo staat de zwelkast na het herladen nog op dezelfde stand). De oude
    vergelijking nam hem mee en vergeleek een snapshot van vóór een pedaalbeweging
    met eentje van erna — dat moest wel mislukken. Dat de stand zelf klopt, wordt
    apart gecontroleerd (zwelstand hersteld na herlaad / na audio-wissel)."""
    s = get("/settings/organ")
    sw = [{k: v for k, v in b.items() if k != "last_value"} for b in (s.get("swell_bindings") or [])]
    return {"swell": sw, "cresc": s.get("crescendo_binding")}

def swell_last_value():
    s = get("/settings/organ")
    return next((b.get("last_value") for b in (s.get("swell_bindings") or [])), None)

def measure_swell_level(c, label):
    """Noot op Nevenwerk opnemen → niveau (band<400 Hz) — voor vergelijking open/dicht over herlaad/wissel."""
    post(f"/stops/{c.stops['Holfluit_8']}/toggle"); note_on_midi(1, 60); time.sleep(1.2)
    out = os.path.join(SP, f"persist-{label}.mp3"); r, _ = record(out, lambda now: time.sleep(1.0))
    note_off_midi(1, 60); time.sleep(0.8); post(f"/stops/{c.stops['Holfluit_8']}/toggle")
    x, sr = decode_mono(out); e = env_db(lowpass_fft(x, sr, 400.0), sr)
    return level(e, 400, 1300)

def test_persistence(c):
    print("\n== persistentie: save → herlaad orgel → audio-wissel")
    idx = c.idx["Nevenwerk"]
    send_cc(c.sw["ch"], c.sw["cc"], 127); time.sleep(0.05); ref_open = measure_swell_level(c, "open")
    post("/settings/save"); snap0 = snapshot_bindings()
    check(snap0["swell"] and snap0["cresc"], f"opgeslagen: swell={snap0['swell']} cresc={snap0['cresc']}")
    # zwelstand halverwege zetten vóór herlaad (audio én spiegel op ~0.315 → −13.7 dB)
    send_cc(c.sw["ch"], c.sw["cc"], 40); time.sleep(0.05); pos_before = swell_pos(idx)
    exp_db = c.min_db * (1 - expected_swell(40))
    post("/load_organ", {"path": c.odf}); time.sleep(4.0)
    snap1 = snapshot_bindings(); check(snap1 == snap0, "bindingen gelijk na herlaad (GET /settings/organ)")
    en, st, n = cresc_state(); check(en and n == c.N, f"na herlaad backend crescendo uit OrganSettings: enabled={en} stages={n} (verwacht {c.N}, zonder frontend-push)")
    pos_after = swell_pos(idx)
    check(abs(pos_after - pos_before) <= TH["spiegel_tol"], f"zwelstand (spiegel) hersteld na herlaad: {pos_before:.3f} → {pos_after:.3f}")
    check(swell_last_value() == 40, f"bewaarde pedaalstand volgt het pedaal, niet het opgeslagen bestand (last_value={swell_last_value()}, verwacht 40)")
    lvl = measure_swell_level(c, "na-herlaad") - ref_open
    info(f"zwelspiegel vóór/na herlaad: {pos_before:.3f}/{pos_after:.3f}; audio-niveau na herlaad {lvl:+.1f} dB (spiegel verwacht {exp_db:+.1f}, open = 0)")
    check(abs(lvl - (c.min_db * (1 - pos_after))) <= TH["persist_niveau_tol"],
          f"na herlaad: audio ({lvl:+.1f} dB) komt overeen met spiegel-stand {pos_after:.2f} (RegisterStopDivisionMap reset audio-gains naar 1.0?)")
    post("/crescendo/config", {"stages": c.stages, "enabled": True}); time.sleep(0.1)
    send_cc(c.sw["ch"], c.sw["cc"], 0); time.sleep(0.05)
    check(abs(swell_pos(idx)) <= TH["spiegel_tol"], "zwelbinding functioneel na herlaad (CC0 → 0.0)")
    send_cc(c.cr["ch"], c.cr["cc"], 127); time.sleep(0.05)
    check(cresc_state()[1] == c.N, f"crescendobinding functioneel na herlaad (CC127 → trede {cresc_state()[1]})")
    send_cc(c.cr["ch"], c.cr["cc"], 0); time.sleep(0.05)
    # audio-wissel: zelfde host/apparaat, andere buffer → player_rebuilt + herlaad + apply_dsp_after_backend_reload
    st0 = get("/status"); buf = st0.get("buffer_frames") or 512; other = 256 if buf != 256 else 512
    send_cc(c.sw["ch"], c.sw["cc"], 40); time.sleep(0.05)
    r = post("/audio_output", {"host": st0["audio_host"], "device": st0["audio_device"], "buffer_frames": other}); time.sleep(4.0)
    info(f"audio-wissel: ok={r.get('ok')} rebuilt={r.get('player_rebuilt')} reloaded={r.get('reloaded_organ') is not None} {r.get('message')}")
    if r.get("player_rebuilt"):
        snap2 = snapshot_bindings(); check(snap2 == snap0, "bindingen gelijk na audio-wissel")
        pos_sw = swell_pos(idx); lvl2 = measure_swell_level(c, "na-wissel") - ref_open
        check(abs(pos_sw - expected_swell(40)) <= TH["spiegel_tol"], f"zwelstand (spiegel) hersteld na audio-wissel: {pos_sw:.3f}")
        check(abs(lvl2 - (c.min_db * (1 - pos_sw))) <= TH["persist_niveau_tol"],
              f"na audio-wissel: audio ({lvl2:+.1f} dB) komt overeen met spiegel-stand {pos_sw:.2f}")
        time.sleep(1.5); post("/crescendo/config", {"stages": c.stages, "enabled": True}); time.sleep(0.1)
        send_cc(c.cr["ch"], c.cr["cc"], 127); time.sleep(0.05)
        check(cresc_state()[1] == c.N, f"crescendo functioneel na audio-wissel (trede {cresc_state()[1]})")
        send_cc(c.cr["ch"], c.cr["cc"], 0)
    else:
        warn("audio-wissel gaf geen herbouw (zelfde apparaat?) — wissel-test niet uitgevoerd")
    post("/audio_output", {"host": st0["audio_host"], "device": st0["audio_device"], "buffer_frames": buf}); time.sleep(3.0)
    send_cc(c.sw["ch"], c.sw["cc"], 127)

# ---------------------------------------------------------------------------
def main():
    print("JM-Orgue zwel/crescendo-meetplan —", get("/version"))
    c = setup()
    try:
        test_swell_sweep(c, jitter=False)
        test_swell_sweep(c, jitter=True)
        test_swell_jump(c)
        test_swell_range_invert(c)
        test_cresc_mapping(c)
        test_cresc_mapping(c, manual=(HW_REGS[-1],))          # handmatig register blijft staan
        test_cresc_mapping(c, lo=20, hi=110, inv=False)
        test_cresc_mapping(c, lo=0, hi=127, inv=True)          # pedaal stuurt 127→0
        test_cresc_hysteresis(c)
        test_cresc_stage_audio(c)
        test_cresc_disabled_and_exclusive(c)
        test_beide_treden_tegelijk(c)
        test_handmatige_exclusiviteit(c)
        test_crescendo_uit_met_binding(c)
        test_persistentie_beide_bindingen(c)
        test_player_cc(c)
        test_persistence(c)
    except RouteMissing as e:
        fail(f"route ontbreekt: {e}")
    finally:
        try_post("/panic"); try_post("/settings/wind?enabled=1")
        if c.archive_was: try_post("/midi/archive/config?enabled=1")
    print(f"\n== resultaat: {len(RESULT['pass'])} pass, {len(RESULT['fail'])} fail, {len(RESULT['warn'])} warn")
    for f in RESULT["fail"]: print("  FAIL", f)
    for w in RESULT["warn"]: print("  WARN", w)
    json.dump({"thresholds": TH, "routes_nodig": ROUTES_NODIG, **RESULT}, open(OUT_JSON, "w", encoding="utf-8"), indent=2, ensure_ascii=False)
    print("json:", OUT_JSON)

if __name__ == "__main__":
    main()
