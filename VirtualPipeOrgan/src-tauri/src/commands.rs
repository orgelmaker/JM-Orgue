//! Tauri commands - API between frontend and backend

use std::path::Path;
use tauri::State;
use serde::{Serialize, Deserialize};
use tracing::{info, warn};

use crate::audio::AudioCommand;
use crate::state::AppState;
use crate::library::{self, OrganLibraryEntry, OrganSettings, PresetBindingSaved, SwellBindingSaved, CrescendoBindingSaved, DivisionTremulantSaved, MidiMappingSaved, PresetData};
use vpo_sampler::{OrganDefinition, clean_stop_name, clean_division_name};

/// Audio device info for frontend
#[derive(Debug, Serialize)]
pub struct AudioDeviceDto {
    pub name: String,
    pub is_default: bool,
    pub sample_rates: Vec<u32>,
    pub channels: Vec<u16>,
}

/// MIDI device info for frontend
#[derive(Debug, Serialize)]
pub struct MidiDeviceDto {
    pub name: String,
    pub is_input: bool,
    pub is_output: bool,
    /// True if device name suggests Bluetooth/BLE MIDI
    pub is_bluetooth: bool,
}

/// Detect Bluetooth/BLE MIDI devices by name patterns
fn is_bluetooth_midi(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.contains("bluetooth") ||
    lower.contains("ble") ||
    lower.contains("bt-") ||
    lower.contains("bt midi") ||
    lower.contains("esp32") ||
    lower.contains("widi") ||
    lower.contains("midiair") ||
    lower.contains("cme") ||
    lower.contains("yamaha md-bt") ||
    lower.contains("mi.1")
}

/// MIDI channel mapping for frontend
#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct MidiMappingDto {
    pub division: String,
    pub channel: Option<u8>,
    pub transpose: i8,
    pub first_midi_note: Option<u8>,
    pub last_midi_note: Option<u8>,
    pub short_octave: bool,
}

/// Coupler info for frontend
#[derive(Debug, Clone, Serialize)]
pub struct CouplerDto {
    pub id: String,
    pub name: String,
    pub source_division: String,
    pub destination_division: String,
    pub active: bool,
    pub display_in_division: String,
    pub midi_action_code: u8,
    /// "unison", "super", "sub", "ta" (tongwerken af)
    pub coupler_type: String,
    /// Pitch offset in semitones: 0=unison, +12=super, -12=sub
    pub pitch_offset: i32,
}

/// Organ info for frontend
#[derive(Debug, Clone, Serialize)]
pub struct OrganInfoDto {
    pub id: String,
    pub name: String,
    pub builder: String,
    pub location: String,
    pub year: Option<String>,
    pub stop_count: usize,
    pub divisions: Vec<DivisionDto>,
    pub couplers: Option<Vec<CouplerDto>>,
    /// Hertemperen: aantal speelbare pijpen met een plausibele gemeten
    /// toonhoogte (smpl-chunk/ODF/HW) — 0 bij JM-Rec/eigen sets.
    #[serde(default)]
    pub retune_pipes: usize,
    /// Totaal aantal speelbare pijp-keys (exclusief release-keys).
    #[serde(default)]
    pub retune_total: usize,
    /// Microfoonperspectieven van dit orgel (0.7.38); leeg = één perspectief.
    #[serde(default)]
    pub perspectives: Vec<PerspectiveDto>,
    /// Aantal registers met ≥1 echte extra rank (laag zonder perspectief,
    /// bv. mixtuurkoren) — die klinken altijd volledig.
    #[serde(default)]
    pub layered_stops: usize,
    /// Aantal pijpen met een RELEASE-opname. Een release is de uitklank van de
    /// pijp ín de ruimte: staat die erin, dan heeft de sampleset de akoestiek
    /// van de kerk zelf al meegenomen ("nat"). De frontend zet daar geen
    /// kunstmatige galm overheen (zie de nagalm-instellingen).
    #[serde(default)]
    pub release_pipes: usize,
}

/// Microfoonperspectief voor de frontend (Orgel-Instellingen → Perspectieven).
#[derive(Debug, Clone, Serialize)]
pub struct PerspectiveDto {
    pub name: String,
    /// Gewenst: laden bij de volgende (her)load.
    pub enabled: bool,
    pub gain_db: f32,
    /// Nu daadwerkelijk geladen.
    pub loaded: bool,
    pub pipe_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct DivisionDto {
    pub name: String,
    pub display_name: String,
    pub stops: Vec<StopDto>,
    /// True when the source organ defines a tremulant for this division (ODF
    /// tremulant ref or trem sample folders). The UI uses it to make the
    /// LFO-tremulant available by default so the tremulant isn't "missing".
    #[serde(default)]
    pub has_tremulant: bool,
    /// Soort tremulant van deze divisie, voor de UI-default van de synth-LFO:
    /// `"wave"` = de sampleset heeft echte tremulant-OPNAMEN (GrandOrgue
    /// `TremulantType=Wave`), `"samples"` = trem-opnamen zonder ODF-tremulant
    /// (Hauptwerk "tremmed"-laag, JM-Rec `_trem`-map), `"synth"` = alleen een
    /// ODF-tremulant met LFO-parameters. `None` = geen tremulant.
    #[serde(default)]
    pub tremulant_kind: Option<String>,
    /// True when this division sits in a swell box (GrandOrgue Enclosure). The UI
    /// enables the swell box (expression) for it by default.
    #[serde(default)]
    pub has_swell: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct StopDto {
    pub id: String,
    pub name: String,
    pub pitch: String,
    pub drawn: bool,
    pub color: Option<String>,
    /// Whether this stop has tremulant samples available
    pub has_tremulant: bool,
    /// MIDI action code for this stop (for MIDI learn, 150+)
    pub midi_action_code: u8,
    /// Internal stop ID (numeric) for audio lookup
    pub internal_stop_id: u32,
    /// First MIDI note this stop responds to
    pub first_midi_note: u8,
    /// Last MIDI note this stop responds to
    pub last_midi_note: u8,
    /// Whether this is a reed (tongwerk) stop — for T.A. coupler
    #[serde(skip_serializing)]
    pub is_reed: bool,
}

/// Division volume info for frontend
#[derive(Debug, Clone, Serialize)]
pub struct DivisionVolumeDto {
    pub division: String,
    pub volume: f32,
}

/// Swell binding info for frontend
#[derive(Debug, Clone, Serialize)]
pub struct SwellBindingDto {
    pub division: String,
    pub channel: u8,
    pub cc_num: u8,
    pub min_val: u8,
    pub max_val: u8,
    pub invert: bool,
}

/// Application status
#[derive(Debug, Serialize)]
pub struct StatusDto {
    pub audio_running: bool,
    pub midi_connected: bool,
    pub organ_loaded: bool,
    pub voice_count: usize,
    pub peak_left: f32,
    pub peak_right: f32,
    pub sample_rate: u32,
    /// Actual audio host/driver in use (e.g. "WASAPI", "ASIO")
    pub audio_host: String,
    /// Actual audio output device in use
    pub audio_device: String,
    /// Actual buffer size in frames (0 = driver default)
    pub buffer_frames: u32,
    /// Actual number of output channels of the open stream
    pub channels: u16,
    /// Ingestelde polyfonie-kap (max. gelijktijdige stemmen).
    pub polyphony: usize,
    /// Belasting van de render-thread (0..1+, EMA) en piek.
    pub render_load: f32,
    pub render_peak: f32,
    /// Aantal callbacks dat sinds het starten van de stream over 80 % van zijn
    /// buffertijd ging (cumulatief). Loopt deze op tijdens een crescendo-sweep,
    /// dan is de buffer te klein voor het werk per trapwissel.
    pub render_overloads: u64,
    /// Gedropte realtime audio-commando's (volle queue) — sluit een volle
    /// commandowachtrij uit als oorzaak van haperen/ontbrekende noten.
    pub rt_drops: u64,
    /// Stereo-samples als twee kanalen laden (instelling).
    pub stereo_samples: bool,
    /// Automatisch MIDI-archief legt op dit moment een take vast.
    pub midi_archiving: bool,
    /// Audio-uitgang definitief (uitgestelde ASIO-wissel afgerond of niet
    /// nodig); de frontend wacht hierop met het autoladen van het laatste orgel.
    pub audio_ready: bool,
    /// Naam van de ASIO-driver die in deze sessie niet meer kan starten; de UI
    /// toont dan de balk met de herstart-knop. None = geen advies.
    pub asio_restart_advice: Option<String>,
    /// Interne orgel-herladingen sinds de start (ASIO-wissel, noodherstel).
    pub backend_reloads: u32,
    /// Werkelijke framegrootte van de laatste audio-callback (ook wanneer de
    /// driver zijn eigen buffer aanhoudt en buffer_frames 0 is).
    pub render_frames: u32,
    /// Welk deel van de rendertijd het mengen van de stemmen is (0..1). Dat is
    /// precies het werk dat over de rekenkernen verdeeld wordt; de rest (wind,
    /// tremulant, zwelkast, galm, EQ, limiter) blijft op één kern.
    pub mix_voice_load: f32,
}

// ============ Commands ============

/// Polyfonie-kap instellen (256..=4096), direct actief én opgeslagen in de
/// audio-voorkeuren. Hoger = meer CPU op de (ene) render-kern; de status
/// toont de belasting zodat de gebruiker onder ~80% kan blijven.
#[tauri::command]
pub fn set_polyphony(state: State<AppState>, voices: u32) -> Result<usize, String> {
    let n = (voices as usize).clamp(crate::audio::MIN_LIVE_VOICES, crate::audio::MAX_LIVE_VOICES);
    crate::audio::set_polyphony_target(n);
    let mut prefs = crate::state::load_audio_prefs(&state.app_data_dir);
    prefs.polyphony = Some(n as u32);
    crate::state::save_audio_prefs(&state.app_data_dir, &prefs);
    info!("Polyfonie-kap ingesteld op {} stemmen", n);
    Ok(n)
}

/// Stand van één windgroep (balg) voor de meter in de instellingen.
#[derive(Debug, Clone, Serialize)]
pub struct WindGroupStatusDto {
    /// Groep-index (0-based).
    pub group: u8,
    /// Balgdruk nu, als fractie van de volle druk.
    pub pressure: f32,
    /// Klinkende pijpen op deze groep.
    pub voices: u32,
    /// Gewogen windverbruik (8'-C-eenheden; een 16'-C is 4, een 2'-c''' 0,03).
    pub verbruik: f32,
    /// Toonhoogteverschil van de referentiepijp (8'-prestant c') in cent.
    pub cents: f32,
}

/// Stand van één lade (divisie): de snelle laag, met de vastgehouden dip.
#[derive(Debug, Clone, Serialize)]
pub struct WindDivisionStatusDto {
    pub division: u8,
    /// Druk op de lade nu (balg × lade), fractie.
    pub pressure: f32,
    /// Laagste druk van de laatste ~0,3 s, zodat een schrik van 50-150 ms
    /// ook op het scherm te zien is.
    pub dip: f32,
    /// Wat die druk doet met een prestant 8' c', een fluit c''' en een
    /// tongwerk, in cent.
    pub cents_prestant: f32,
    pub cents_fluit: f32,
    pub cents_tongwerk: f32,
}

#[derive(Debug, Clone, Serialize)]
pub struct WindStatusDto {
    pub groups: Vec<WindGroupStatusDto>,
    pub divisions: Vec<WindDivisionStatusDto>,
}

/// Live winddruk per groep en per lade. Antwoordt op "werkt het windmodel
/// eigenlijk wel?": zolang je speelt is hier te zien hoe ver balg en lade
/// inzakken en wat dat per pijpfamilie in cent scheelt. Alleen opgevraagd
/// zolang het instellingenscherm open staat.
#[tauri::command]
pub fn get_wind_status(state: State<AppState>) -> WindStatusDto {
    let drukken = crate::audio::wind_drukken();
    let stemmen = crate::audio::wind_stemmen();
    let verbruik = crate::audio::wind_verbruik();
    let laden = crate::audio::wind_laden();
    let dips = crate::audio::wind_dips();
    let n_div = state.loaded_organ_info.read().as_ref().map(|o| o.divisions.len()).unwrap_or(0).min(32);
    let toewijzing = state.division_wind_groups.read().clone();
    let cents = vpo_audio::WIND_CENTS_PER_EENHEID;
    let groups = (0..8usize)
        .map(|g| {
            let p = drukken.get(g).copied().unwrap_or(1.0);
            WindGroupStatusDto {
                group: g as u8,
                pressure: p,
                voices: stemmen.get(g).copied().unwrap_or(0),
                verbruik: verbruik.get(g).copied().unwrap_or(0.0),
                cents: (p - 1.0) * cents,
            }
        })
        .collect();
    let divisions = (0..n_div)
        .map(|d| {
            let g = toewijzing.get(d).copied().unwrap_or(d as u8) as usize;
            let balg = drukken.get(g).copied().unwrap_or(1.0);
            let p = balg * laden.get(d).copied().unwrap_or(1.0);
            let dip = balg * dips.get(d).copied().unwrap_or(1.0);
            WindDivisionStatusDto {
                division: d as u8,
                pressure: p,
                dip,
                cents_prestant: (p - 1.0) * cents,
                cents_fluit: (p - 1.0) * cents * 1.75,
                cents_tongwerk: 0.0,
            }
        })
        .collect();
    WindStatusDto { groups, divisions }
}

/// Hoeveel rekenkernen de mengloop gebruikt (0.7.49). 1 = alles op de
/// audiothread, zoals vóór deze versie. Meer kernen verdelen het stemwerk —
/// dat is 97 % van de rendertijd bij veel klinkende pijpen.
///
/// De pool wordt hier herbouwd (buiten de audiothread) en de keuze wordt per
/// pc bewaard, niet per orgel.
#[tauri::command]
pub fn set_mix_cores(state: State<AppState>, cores: u32) -> Result<usize, String> {
    let n = (cores as usize).clamp(1, crate::audio::MAX_MENG_STUKKEN);
    crate::audio::set_meng_stukken(n);
    let mut prefs = crate::state::load_audio_prefs(&state.app_data_dir);
    prefs.mix_cores = Some(n as u32);
    crate::state::save_audio_prefs(&state.app_data_dir, &prefs);
    Ok(n)
}

/// Stand van zaken voor de instelling: gekozen kernen, wat deze pc heeft, en
/// wat de app zou aanraden.
#[derive(Debug, Clone, Serialize)]
pub struct MixCoresDto {
    /// Nu in gebruik.
    pub cores: usize,
    /// Fysieke rekenkernen van deze pc.
    pub physical: usize,
    /// Hoogste keuze die de app aanbiedt.
    pub max: usize,
    /// Wat de app zelf zou kiezen.
    pub recommended: usize,
}

#[tauri::command]
pub fn get_mix_cores() -> MixCoresDto {
    MixCoresDto {
        cores: crate::audio::meng_stukken(),
        physical: crate::mengpool::fysieke_kernen(),
        max: crate::audio::MAX_MENG_STUKKEN,
        recommended: crate::mengpool::aanbevolen_kernen(),
    }
}

/// Luidspreker-testsignaal op één uitgangskanaal (0.7.47). `channel = None`
/// zet hem uit. `kind`: 0 = roze ruis, 1 = sinus 440 Hz. `level_db` is het
/// piekniveau t.o.v. volle schaal (-60..0 dB; standaard -20 dB, luid genoeg om
/// te horen en zacht genoeg om niet te schrikken).
///
/// Waarom: wie zes of acht kanalen aansluit weet daarna nog niet welke stekker
/// in welke kast zit. Dit stuurt geluid naar precies één uitgang.
#[tauri::command]
pub fn set_output_test_signal(channel: Option<u8>, kind: Option<u8>, level_db: Option<f32>) -> Result<(), String> {
    match channel {
        Some(ch) => {
            let db = level_db.unwrap_or(-20.0).clamp(-60.0, 0.0);
            let gain = 10f32.powf(db / 20.0);
            crate::audio::set_test_signal(Some(ch), kind.unwrap_or(0), gain);
            info!("Testsignaal AAN op kanaal {} ({}, {:.0} dB)", ch + 1,
                if kind.unwrap_or(0) == 1 { "sinus 440 Hz" } else { "roze ruis" }, db);
        }
        None => {
            crate::audio::set_test_signal(None, 0, 0.0);
            info!("Testsignaal uit");
        }
    }
    Ok(())
}

/// Stereo-samples als twee kanalen laden (aan) of naar mono mengen (uit).
/// Geldt voor het volgende (her)laden van een orgel.
#[tauri::command]
pub fn set_stereo_samples(state: State<AppState>, on: bool) -> Result<(), String> {
    vpo_sampler::set_stereo_loading(on);
    let mut prefs = crate::state::load_audio_prefs(&state.app_data_dir);
    prefs.stereo_samples = Some(on);
    crate::state::save_audio_prefs(&state.app_data_dir, &prefs);
    info!("Stereo-samples laden: {}", if on { "aan" } else { "uit (mono-mix)" });
    Ok(())
}

// ============ Gestapelde ranks en microfoonperspectieven (0.7.38) ============

use crate::state::{PerspectiveRuntime, RankSummary, RankLayerSummary};
use crate::library::PerspectiveSaved;

/// Opgeslagen instellingen van een orgel: bibliotheek (exact, dan case-/
/// slash-ongevoelig — zelfde orgel, andere pad-notatie, audit 49), anders de
/// `.jm-settings.json` naast het orgel; die wordt dan in de bibliotheek
/// geïmporteerd zodat álle herstel-paden hem daarna zien (auditbevinding 17).
fn saved_settings_for(state: &AppState, organ_id: &str) -> Option<OrganSettings> {
    let from_lib = {
        let lib = state.organ_library.read();
        lib.settings.get(organ_id).cloned().or_else(|| {
            let want = organ_id.replace('/', "\\").to_lowercase();
            lib.settings.iter()
                .find(|(k, _)| k.replace('/', "\\").to_lowercase() == want)
                .map(|(_, v)| v.clone())
        })
    };
    if from_lib.is_some() {
        return from_lib;
    }
    let from_file = organ_settings_dir(organ_id)
        .map(|d| d.join(".jm-settings.json"))
        .filter(|p| p.exists())
        .and_then(|p| std::fs::read_to_string(&p).ok())
        .and_then(|j| serde_json::from_str::<OrganSettings>(&j).ok())?;
    info!("Instellingen geïmporteerd uit .jm-settings.json naast het orgel (geen bibliotheek-entry)");
    state.organ_library.write().settings.insert(organ_id.to_string(), from_file.clone());
    state.save_library();
    Some(from_file)
}

/// Perspectief-voorkeuren (naam, aan/uit, gain) voor de komende load. Bij een
/// herlaad van HETZELFDE orgel de runtime-staat (de gebruiker heeft net op een
/// vinkje geklikt; de 800 ms-autosave kan later komen dan de "Orgel opnieuw
/// laden"-klik → geen save/reload-race), anders de opgeslagen instellingen.
fn perspective_prefs_for(state: &AppState, organ_id: &str) -> Vec<PerspectiveSaved> {
    let same = state.current_organ_id.read().as_deref() == Some(organ_id);
    if same {
        let rt = state.perspectives.read();
        if !rt.is_empty() {
            return rt.iter()
                .map(|p| PerspectiveSaved { name: p.name.clone(), enabled: p.enabled, gain_db: p.gain_db })
                .collect();
        }
    }
    saved_settings_for(state, organ_id).map(|s| s.perspectives).unwrap_or_default()
}

/// Laadplan voor perspectieven uit waarnemingen `(label, is-laag-0, #pijpen)`
/// in stop-/laagvolgorde. Labels in eerste-voorkomen-volgorde; `enabled` =
/// opgeslagen waarde, anders aan als het label ergens als PRIMAIR perspectief
/// (laag 0) voorkomt — labels die alléén als extra laag voorkomen staan
/// standaard uit, anders klinkt een ongelabelde laag 0 dubbel met een
/// gelabelde extra laag. Hoogstens 15 labels (gain-slots 1..15); daarboven
/// warn + overslaan (die lagen worden niet geladen). `loaded` blijft false;
/// de laadroute zet hem gelijk aan `enabled`.
fn plan_perspectives_from<I>(obs: I, saved: &[PerspectiveSaved]) -> Vec<PerspectiveRuntime>
where
    I: IntoIterator<Item = (String, bool, usize)>,
{
    let mut out: Vec<PerspectiveRuntime> = Vec::new();
    let mut primary_seen: Vec<bool> = Vec::new();
    let mut skipped: Vec<String> = Vec::new();
    for (label, primary, pipes) in obs {
        if let Some(i) = out.iter().position(|p| p.name == label) {
            out[i].pipe_count += pipes;
            primary_seen[i] |= primary;
        } else if out.len() < crate::audio::MAX_LAYERS - 1 {
            out.push(PerspectiveRuntime {
                name: label,
                enabled: false,
                gain_db: 0.0,
                loaded: false,
                slot: (out.len() + 1) as u8,
                pipe_count: pipes,
            });
            primary_seen.push(primary);
        } else if !skipped.contains(&label) {
            skipped.push(label);
        }
    }
    if !skipped.is_empty() {
        warn!("Meer dan 15 perspectieven: {:?} worden niet geladen", skipped);
    }
    for (i, p) in out.iter_mut().enumerate() {
        match saved.iter().find(|s| s.name == p.name) {
            Some(s) => {
                p.enabled = s.enabled;
                p.gain_db = s.gain_db.clamp(-40.0, 12.0);
            }
            None => p.enabled = primary_seen[i],
        }
    }
    out
}

/// Laadplan voor een GrandOrgue/Hauptwerk-definitie (niet-ruis-stops).
fn plan_perspectives(definition: &OrganDefinition, saved: &[PerspectiveSaved]) -> Vec<PerspectiveRuntime> {
    use vpo_sampler::PipeDef;
    let obs = definition.stops.iter()
        .filter(|s| !s.is_noise_or_mechanical())
        .flat_map(|s| s.all_layers().filter_map(|(idx, persp, pipes)| {
            persp.map(|p| (
                p.to_string(),
                idx == 0,
                pipes.iter().filter(|x| !matches!(x, PipeDef::Empty)).count(),
            ))
        }).collect::<Vec<_>>());
    plan_perspectives_from(obs, saved)
}

/// Laadplan voor een eigen sample-map (labels = submapnamen).
fn plan_perspectives_custom(organ: &CustomOrgan, saved: &[PerspectiveSaved]) -> Vec<PerspectiveRuntime> {
    let obs = organ.divisions.iter().flat_map(|d| d.stops.iter()).flat_map(|s| {
        let mut v: Vec<(String, bool, usize)> = Vec::new();
        if let Some(p) = &s.primary_perspective {
            v.push((p.clone(), true, s.pipes.len()));
        }
        for p in &s.perspectives {
            v.push((p.name.clone(), false, p.pipes.len()));
        }
        v
    });
    plan_perspectives_from(obs, saved)
}

fn perspective_dtos(list: &[PerspectiveRuntime]) -> Vec<PerspectiveDto> {
    list.iter().map(|p| PerspectiveDto {
        name: p.name.clone(),
        enabled: p.enabled,
        gain_db: p.gain_db,
        loaded: p.loaded,
        pipe_count: p.pipe_count,
    }).collect()
}

/// Lagen-samenvatting van een GO/HW-stop (diagnose: /ranks, layered_stops).
fn rank_summary_of(stop: &vpo_sampler::StopDef, dto_id: String) -> RankSummary {
    use vpo_sampler::PipeDef;
    RankSummary {
        stop_id: stop.id,
        dto_id,
        name: clean_stop_name(&stop.name),
        layers: stop.all_layers().map(|(idx, persp, pipes)| RankLayerSummary {
            index: idx,
            name: if idx == 0 {
                stop.name.clone()
            } else {
                stop.layers.iter().find(|l| l.index == idx).map(|l| l.name.clone()).unwrap_or_default()
            },
            perspective: persp.map(|s| s.to_string()),
            pipes_nonempty: pipes.iter().filter(|x| !matches!(x, PipeDef::Empty)).count(),
        }).collect(),
    }
}

/// Spiegel de runtime-perspectieven naar de bewaarde DTO, zodat de 300 ms-
/// organ-poll van de frontend (en GET /organ) geen verouderde enabled/gain-
/// waarden terugzet over de zojuist gezette (zelfde patroon als de drawn-vlaggen).
fn sync_perspective_dto(state: &AppState) {
    let dto = perspective_dtos(&state.perspectives.read());
    if let Some(o) = state.loaded_organ_info.write().as_mut() {
        o.perspectives = dto;
    }
}

/// Kern (niet-Tauri; ook de test-API): perspectief aan/uit. Retourneert true
/// als herladen nodig is (enabled != loaded voor ≥1 perspectief). Slaat NIET
/// direct op: de reguliere autosave bewaart `state.perspectives`, en een
/// herlaad van hetzelfde orgel leest de runtime-staat (perspective_prefs_for).
/// Koppels die de app uit de divisie-indeling kan afleiden maar die in de
/// huidige koppellijst ontbreken (0.7.47). Een sampleset die zelf koppels
/// meebrengt onderdrukte tot nu toe de afgeleide lijst volledig: had het orgel
/// geen sub- of superoctaafkoppel, dan was die ook niet te krijgen. Dit geeft
/// precies de ontbrekende terug, zodat de organist ze kan bijschakelen.
///
/// "Ontbreekt" wordt op betekenis bepaald (bron, doel, soort), niet op id: een
/// ODF-koppel met een eigen naam is hetzelfde koppel als het afgeleide.
fn ontbrekende_koppels(huidig: &[CouplerDto], divisions: &[DivisionDto]) -> Vec<CouplerDto> {
    let bestaat = |k: &CouplerDto| huidig.iter().any(|h| {
        h.source_division == k.source_division
            && h.destination_division == k.destination_division
            && h.coupler_type == k.coupler_type
            && h.pitch_offset == k.pitch_offset
    });
    generate_couplers(divisions).into_iter().filter(|k| !bestaat(k)).collect()
}

/// Stelt de koppellijst van het geladen orgel opnieuw samen: de koppels van de
/// sampleset plus de bijgeschakelde extra's. Actiecodes van de originele
/// koppels blijven ongemoeid (ingeleerde MIDI-knoppen mogen niet verspringen);
/// de extra's krijgen codes bóven de hoogste bestaande.
fn koppellijst_herbouwen(state: &AppState) {
    let basis = state.base_couplers.read().clone();
    if basis.is_empty() { return; }
    let gewenst = state.extra_coupler_ids.read().clone();
    let divisions = match state.loaded_organ_info.read().as_ref() {
        Some(o) => o.divisions.clone(),
        None => return,
    };
    let mut lijst = basis.clone();
    let mut code = lijst.iter().map(|c| c.midi_action_code).max().unwrap_or(100).saturating_add(1);
    for mut k in ontbrekende_koppels(&basis, &divisions) {
        if !gewenst.contains(&k.id) { continue; }
        k.midi_action_code = code;
        code = code.saturating_add(1);
        lijst.push(k);
    }
    // Koppels die zijn weggehaald mogen niet actief blijven staan.
    let ids: Vec<String> = lijst.iter().map(|c| c.id.clone()).collect();
    let nog_actief: Vec<String> = state.get_active_couplers().into_iter()
        .filter(|a| ids.contains(a)).collect();
    if let Some(o) = state.loaded_organ_info.write().as_mut() {
        o.couplers = Some(lijst);
    }
    state.set_active_couplers(nog_actief);
}

/// De extra koppels die voor dit orgel te krijgen zijn, met de stand of ze nu
/// bijgeschakeld zijn (`active` wordt hier gebruikt als "staat aan in de lijst").
#[tauri::command]
pub fn get_extra_coupler_options(state: State<AppState>) -> Vec<CouplerDto> {
    let basis = state.base_couplers.read().clone();
    let divisions = match state.loaded_organ_info.read().as_ref() {
        Some(o) => o.divisions.clone(),
        None => return Vec::new(),
    };
    let gekozen = state.extra_coupler_ids.read().clone();
    ontbrekende_koppels(&basis, &divisions).into_iter()
        .map(|mut k| { k.active = gekozen.contains(&k.id); k })
        .collect()
}

/// Welke extra koppels wil de organist erbij? Meteen actief, geen herlaad.
#[tauri::command]
pub fn set_extra_couplers(state: State<AppState>, ids: Vec<String>) -> Result<(), String> {
    *state.extra_coupler_ids.write() = ids;
    koppellijst_herbouwen(&state);
    let n = state.extra_coupler_ids.read().len();
    info!("Extra koppels: {} bijgeschakeld", n);
    Ok(())
}

/// Gain waarmee een geladen maar uitgeschakelde opnamepositie wordt stilgezet.
/// Ruim onder de hoorgrens; de samples blijven staan zodat aanzetten geen
/// herlaad kost.
pub const PERSPECTIEF_STIL_DB: f32 = -120.0;

pub fn do_set_perspective_enabled(state: &AppState, name: &str, enabled: bool) -> Result<bool, String> {
    let (slot, gain_db, loaded, reload_needed) = {
        let mut list = state.perspectives.write();
        let p = list.iter_mut().find(|p| p.name == name)
            .ok_or_else(|| format!("Onbekend perspectief: {}", name))?;
        p.enabled = enabled;
        let (slot, gain_db, loaded) = (p.slot, p.gain_db, p.loaded);
        // Herladen is alléén nodig voor een positie die de organist WIL horen
        // maar die niet in het geheugen staat. Uitzetten kan altijd live: de
        // samples blijven staan en de positie wordt stilgezet (0.7.47; tot dan
        // vroeg elke wijziging om een herlaad).
        (slot, gain_db, loaded, list.iter().any(|p| p.enabled && !p.loaded))
    };
    if loaded {
        let g = if enabled { gain_db } else { PERSPECTIEF_STIL_DB };
        state.send_audio_command(AudioCommand::SetPerspectiveGain { slot, gain_db: g });
    }
    sync_perspective_dto(state);
    info!("Opnamepositie '{}' {} ({}, herladen nodig: {})", name,
        if enabled { "aan" } else { "uit" },
        if loaded { "live" } else { "niet geladen" }, reload_needed);
    Ok(reload_needed)
}

/// Kern: live volume van een perspectief (clamp -40..+12 dB), zonder herlaad.
pub fn do_set_perspective_gain(state: &AppState, name: &str, gain_db: f32) -> Result<(), String> {
    let g = if gain_db.is_finite() { gain_db.clamp(-40.0, 12.0) } else { 0.0 };
    let (slot, enabled) = {
        let mut list = state.perspectives.write();
        let p = list.iter_mut().find(|p| p.name == name)
            .ok_or_else(|| format!("Onbekend perspectief: {}", name))?;
        p.gain_db = g;
        (p.slot, p.enabled)
    };
    // Een uitgeschakelde (maar geladen) positie blijft stil: het schuifje zet
    // alleen de waarde klaar voor als hij weer aangaat.
    let te_sturen = if enabled { g } else { PERSPECTIEF_STIL_DB };
    state.send_audio_command(AudioCommand::SetPerspectiveGain { slot, gain_db: te_sturen });
    sync_perspective_dto(state);
    Ok(())
}

#[tauri::command]
pub fn get_perspectives(state: State<AppState>) -> Vec<PerspectiveDto> {
    perspective_dtos(&state.perspectives.read())
}

/// Zet een perspectief aan/uit; true = herladen nodig. Geen schijf-I/O
/// (de UI triggert de autosave via de bestaande debounce).
#[tauri::command]
pub fn set_perspective_enabled(state: State<AppState>, name: String, enabled: bool) -> Result<bool, String> {
    do_set_perspective_enabled(&state, &name, enabled)
}

/// Doorlopend klavier (0.7.47): toetsen buiten het opgenomen bereik van een
/// register spelen de pijp een octaaf hoger of lager in plaats van te zwijgen.
/// Alle klinkende noten gaan uit bij het omzetten: NoteOn en NoteOff moeten
/// dezelfde pijp uitrekenen, anders blijft er een noot hangen.
#[tauri::command]
pub fn set_continuous_keyboard(state: State<AppState>, enabled: bool) -> Result<(), String> {
    crate::state::DOORLOPEND_KLAVIER.store(enabled, std::sync::atomic::Ordering::Relaxed);
    state.send_audio_command(AudioCommand::AllNotesOff);
    state.held_notes.write().clear();
    info!("Doorlopend klavier: {}", if enabled { "aan" } else { "uit" });
    Ok(())
}

/// Staat het doorlopende klavier aan?
#[tauri::command]
pub fn get_continuous_keyboard() -> bool {
    crate::state::DOORLOPEND_KLAVIER.load(std::sync::atomic::Ordering::Relaxed)
}

/// Alle opnameposities in het geheugen houden (0.7.47), zodat wisselen
/// ogenblikkelijk gaat. Kost geheugen: ruwweg maal het aantal posities. Werkt
/// vanaf de volgende (her)load, dus geeft `true` terug als er nu nog posities
/// ontbreken en een herlaad dus nog nodig is.
#[tauri::command]
pub fn set_load_all_perspectives(state: State<AppState>, enabled: bool) -> Result<bool, String> {
    *state.load_all_perspectives.write() = enabled;
    let ontbreekt = state.perspectives.read().iter().any(|p| !p.loaded);
    info!("Alle opnameposities laden: {} (herladen nodig: {})",
        if enabled { "aan" } else { "uit" }, enabled && ontbreekt);
    Ok(enabled && ontbreekt)
}

/// Staat "alle opnameposities laden" aan voor het geladen orgel?
#[tauri::command]
pub fn get_load_all_perspectives(state: State<AppState>) -> bool {
    *state.load_all_perspectives.read()
}

/// Live volume van een perspectief in dB.
#[tauri::command]
pub fn set_perspective_gain(state: State<AppState>, name: String, gain_db: f32) -> Result<(), String> {
    do_set_perspective_gain(&state, &name, gain_db)
}

#[tauri::command]
pub fn get_audio_devices(state: State<AppState>) -> Result<Vec<AudioDeviceDto>, String> {
    let devices = state.list_audio_devices();
    Ok(devices.into_iter().map(|d| AudioDeviceDto {
        name: d.name,
        is_default: d.is_default,
        sample_rates: d.sample_rates,
        channels: d.channels,
    }).collect())
}

/// List available audio hosts/drivers (e.g. "WASAPI"; "ASIO" when built with the asio feature).
#[tauri::command]
pub fn list_audio_hosts(state: State<AppState>) -> Vec<String> {
    state.list_audio_hosts()
}

/// List output devices for a given host (None = default host).
#[tauri::command]
pub fn list_output_devices(state: State<AppState>, host: Option<String>) -> Result<Vec<AudioDeviceDto>, String> {
    let devices = state.list_audio_devices_for_host(host.as_deref());
    Ok(devices.into_iter().map(|d| AudioDeviceDto {
        name: d.name,
        is_default: d.is_default,
        sample_rates: d.sample_rates,
        channels: d.channels,
    }).collect())
}

/// Switch the audio output host/device/buffer and persist the choice.
/// Retourneert een eerlijke `SwitchOutcome`: de frontend herlaadt het orgel
/// wanneer `player_rebuilt` en markeert het profiel pas actief bij `switched`.
/// Async + spawn_blocking: de wissel kan (met retry-ladder en orgel-herlaad)
/// seconden duren en mag de webview-IPC niet bevriezen.
#[tauri::command]
pub async fn set_audio_output(
    state: State<'_, AppState>,
    host: Option<String>,
    device: Option<String>,
    buffer_frames: Option<u32>,
    // sample_rate: gevraagde samplerate in Hz (0.7.48); None = apparaatstandaard.
    sample_rate: Option<u32>,
) -> Result<crate::state::SwitchOutcome, String> {
    let st = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        st.switch_audio_output(crate::audio::AudioOutputConfig {
            host_name: host,
            device_name: device,
            buffer_frames,
            sample_rate,
        })
    })
    .await
    .map_err(|e| format!("audio-wissel-taak mislukt: {}", e))?
}

/// Scan a sample folder for leading silence in WAV/MP3 files (read-only report).
#[tauri::command]
pub async fn scan_sampleset_silence(directory: String, threshold_db: Option<f32>) -> Result<crate::silence::ScanReport, String> {
    let thr = threshold_db.unwrap_or(-60.0);
    tokio::task::spawn_blocking(move || crate::silence::scan(&directory, thr))
        .await
        .map_err(|e| format!("scan task failed: {}", e))
}

/// Trim leading silence from all WAV/MP3 files in a sample folder.
/// Originals are backed up to a sibling "<name>_backup" folder first.
#[tauri::command]
pub async fn trim_sampleset_silence(directory: String, threshold_db: Option<f32>, preroll_ms: Option<f32>) -> Result<crate::silence::TrimReport, String> {
    let thr = threshold_db.unwrap_or(-60.0);
    let pre = preroll_ms.unwrap_or(5.0);
    tokio::task::spawn_blocking(move || crate::silence::trim(&directory, thr, pre))
        .await
        .map_err(|e| format!("trim task failed: {}", e))
}

/// Scan a sample folder: which WAVs lack a loop and which can get one (read-only).
#[tauri::command]
pub async fn scan_sampleset_loops(directory: String) -> Result<crate::loop_tool::LoopScanReport, String> {
    tokio::task::spawn_blocking(move || crate::loop_tool::scan(&directory))
        .await
        .map_err(|e| format!("loop scan task failed: {}", e))
}

/// Detect a sustain loop in each WAV, bake a crossfade at the seam, and write the
/// loop into the WAV `smpl` chunk. Originals are backed up to "<name>_loopbackup".
/// `only_missing` (default true) skips files that already have a loop.
#[tauri::command]
pub async fn apply_sampleset_loops(directory: String, only_missing: Option<bool>) -> Result<crate::loop_tool::LoopApplyReport, String> {
    let only = only_missing.unwrap_or(true);
    tokio::task::spawn_blocking(move || crate::loop_tool::apply(&directory, only))
        .await
        .map_err(|e| format!("loop apply task failed: {}", e))
}

#[tauri::command]
pub fn get_midi_devices(state: State<AppState>) -> Result<Vec<MidiDeviceDto>, String> {
    let devices = state.list_midi_devices();
    info!("Found {} MIDI devices", devices.len());
    Ok(devices.into_iter().map(|d| {
        let bt = is_bluetooth_midi(&d.name);
        info!("  MIDI device: {} (input={}, output={}, bluetooth={})", d.name, d.is_input, d.is_output, bt);
        MidiDeviceDto {
            name: d.name,
            is_input: d.is_input,
            is_output: d.is_output,
            is_bluetooth: bt,
        }
    }).collect())
}

// === Bluetooth LE MIDI commands ===

/// Bluetooth LE MIDI device DTO for frontend
#[derive(Debug, Clone, Serialize)]
pub struct BleMidiDeviceDto {
    pub id: String,
    pub name: String,
    pub address: String,
    pub rssi: Option<i16>,
    pub connected: bool,
}

/// Initialize BLE MIDI manager (lazy)
fn ensure_ble_manager(state: &State<AppState>) -> Result<(), String> {
    let mut ble = state.ble_midi.write();
    if ble.is_none() {
        let manager = vpo_midi::BleMidiManager::new(state.ble_message_tx.clone())?;
        *ble = Some(manager);
        info!("BLE MIDI manager initialized");
    }
    Ok(())
}

/// Scan for BLE MIDI devices for ~5 seconds
/// Async to avoid blocking the UI during the scan duration.
#[tauri::command]
pub async fn scan_ble_midi(state: State<'_, AppState>) -> Result<Vec<BleMidiDeviceDto>, String> {
    ensure_ble_manager(&state)?;
    // Clone manager handle so we can move into spawn_blocking
    let ble_handle = state.ble_midi.clone();
    let result = tokio::task::spawn_blocking(move || {
        let ble = ble_handle.read();
        let manager = ble.as_ref().ok_or("BLE manager not initialized")?;
        manager.scan(5000)
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))??;

    Ok(result.into_iter().map(|d| BleMidiDeviceDto {
        id: d.id, name: d.name, address: d.address, rssi: d.rssi, connected: d.connected,
    }).collect())
}

/// Connect to a BLE MIDI device by ID
#[tauri::command]
pub async fn connect_ble_midi(state: State<'_, AppState>, device_id: String) -> Result<String, String> {
    ensure_ble_manager(&state)?;
    let ble_handle = state.ble_midi.clone();
    tokio::task::spawn_blocking(move || {
        let ble = ble_handle.read();
        let manager = ble.as_ref().ok_or("BLE manager not initialized")?;
        manager.connect(&device_id)
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))?
}

/// Disconnect a BLE MIDI device
#[tauri::command]
pub async fn disconnect_ble_midi(state: State<'_, AppState>, device_id: String) -> Result<(), String> {
    let ble_handle = state.ble_midi.clone();
    tokio::task::spawn_blocking(move || {
        let ble = ble_handle.read();
        if let Some(manager) = ble.as_ref() {
            manager.disconnect(&device_id)?;
        }
        Ok::<(), String>(())
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))?
}

/// Get list of currently connected BLE MIDI devices
#[tauri::command]
pub fn get_connected_ble_midi(state: State<AppState>) -> Vec<(String, String)> {
    let ble = state.ble_midi.read();
    ble.as_ref().map(|m| m.connected_devices()).unwrap_or_default()
}

/// Get the BLE MIDI message counter (for activity indicator)
#[tauri::command]
pub fn get_ble_midi_count(state: State<AppState>) -> u64 {
    state.ble_message_count.load(std::sync::atomic::Ordering::Relaxed)
}

/// Refresh MIDI device list and reconnect any newly available devices
/// Useful after pairing a Bluetooth MIDI device while the app is running
#[tauri::command]
pub fn refresh_midi_devices(state: State<AppState>) -> Result<usize, String> {
    info!("Refreshing MIDI device list...");
    // Reconnect to all available devices (this will pick up newly paired BLE devices)
    let count = state.connect_all_midi()?;
    info!("Refreshed: {} MIDI devices connected", count);
    Ok(count)
}

#[tauri::command]
pub fn start_audio(_state: State<AppState>, device_name: Option<String>) -> Result<(), String> {
    if let Some(name) = device_name {
        info!("Starting audio with device: {}", name);
    }
    // Audio is already running (started in AppState::new())
    info!("Audio already running");
    Ok(())
}

#[tauri::command]
pub fn stop_audio(state: State<AppState>) -> Result<(), String> {
    state.send_audio_command(AudioCommand::AllNotesOff);
    // Ook de ingedrukte-toetsen-administratie wissen: een spooktoets die hier
    // blijft staan wordt bij elke volgende crescendotrap/registerwissel opnieuw
    // tot leven gewekt (sync start er stemmen voor die nooit meer een NoteOff
    // krijgen — cumulatieve hangers).
    state.held_notes.write().clear();
    info!("Audio stopped (all notes off)");
    Ok(())
}

/// Vlak vóór het starten van de update-installer (0.7.40).
///
/// De updater-plugin start de installer en beëindigt het proces daarna met
/// `std::process::exit(0)` — dat draait GEEN Drop-handlers. Zonder deze stap
/// houdt het stervende proces het ASIO/WASAPI-endpoint vast terwijl de
/// installer de exe vervangt en de app herstart: precies het zombie-scenario
/// van sessie 2026-07-18 (nieuwe instantie zonder geluid, apparaat bezet).
///
/// Async + spawn_blocking: `shutdown_and_wait` blokkeert tot ~1,5 s en een
/// sync-command draait op de UI-thread (les 2026-07-29).
#[tauri::command]
pub async fn prepare_for_update(state: State<'_, AppState>) -> Result<(), String> {
    let st = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        // Lopende MIDI-take nog wegschrijven: de app sluit hierna niet via het
        // kruisje, dus de close-handler in App.svelte komt er niet meer aan te pas.
        if st.midi_archive.archiving.load(std::sync::atomic::Ordering::Relaxed) {
            if let Some(p) = st.midi_archive_flush(std::time::Duration::from_millis(800)) {
                info!("MIDI-archief vóór de update geschreven: {:?}", p);
            }
        }
        st.send_audio_command(AudioCommand::AllNotesOff);
        st.held_notes.write().clear();
        let guard = st.audio_player.read();
        if let Some(p) = guard.as_ref() {
            p.shutdown_and_wait(std::time::Duration::from_millis(1500));
            // Vangnet: mislukt het starten van de installer alsnog, dan blijft
            // de app draaien mét een dode audio-thread. Met deze vlag bouwt de
            // bestaande noodherstel-thread (main.rs) de player opnieuw op.
            //
            // De vlag gaat BEWUST pas na 10 seconden aan. Meteen zetten liet de
            // noodherstel-thread (die elke 3 s kijkt) de hele player én alle
            // samples opnieuw opbouwen precies terwijl de installer de
            // bestanden vervangt: zware schijf- en geheugendruk op het moment
            // dat het proces juist netjes moet afsluiten. Bij een geslaagde
            // update is het proces binnen die 10 seconden allang weg en gebeurt
            // er dus niets.
            let vlag = p.restart_needed.clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_secs(10));
                vlag.store(true, std::sync::atomic::Ordering::Relaxed);
            });
        }
        drop(guard);
        info!("Klaar voor de update-installer: audio-thread gestopt, apparaat vrij");
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))
}

/// Frontend is opgestart (0.7.43): startsein voor de uitgestelde ASIO-wissel
/// in main.rs, die tot nu toe op een vaste 10 s na de processtart wachtte.
#[tauri::command]
pub fn frontend_ready(state: State<AppState>) {
    info!("Frontend gereed gemeld (startsein voor de uitgestelde ASIO-wissel)");
    let (lock, cv) = &*state.frontend_ready;
    *lock.lock() = true;
    cv.notify_all();
}

/// Herstart op verzoek van de gebruiker om de ASIO-driver terug te krijgen
/// (balk "JM-Orgue herstarten"). Tot 0.7.42 deed de backend dit ongevraagd.
#[tauri::command]
pub async fn restart_for_asio(state: State<'_, AppState>) -> Result<(), String> {
    let st = state.inner().clone();
    tokio::task::spawn_blocking(move || st.herstart_voor_asio())
        .await
        .map_err(|e| format!("Task join error: {}", e))?
}

#[tauri::command]
pub fn connect_midi(state: State<AppState>, device_name: String) -> Result<(), String> {
    info!("Connecting to MIDI device: {}", device_name);
    state.connect_midi(&device_name)?;
    info!("Connected to MIDI device: {}", device_name);
    Ok(())
}

#[tauri::command]
pub fn connect_all_midi(state: State<AppState>) -> Result<usize, String> {
    info!("Connecting to all MIDI devices...");
    let count = state.connect_all_midi()?;
    info!("Connected to {} MIDI devices", count);
    Ok(count)
}

#[tauri::command]
pub fn disconnect_midi(state: State<AppState>) -> Result<(), String> {
    info!("Disconnecting MIDI devices");
    state.disconnect_midi();
    Ok(())
}

/// Open een https-URL in de standaardbrowser van de gebruiker (feedback-knop,
/// release-pagina bij een update). Bewust via rundll32 FileProtocolHandler:
/// geen shell-parsing van de URL (cmd /C start zou &-tekens interpreteren) en
/// geen extra plugin-dependency. Alleen https:// toegestaan.
#[tauri::command]
pub fn open_external_url(url: String) -> Result<(), String> {
    if !url.starts_with("https://") {
        return Err("Alleen https-URL's kunnen geopend worden".into());
    }
    // Per platform de eigen opener. Windows bewust via rundll32 (geen
    // shell-parsing van &-tekens in de URL); macOS `open`, Linux `xdg-open`.
    #[cfg(target_os = "windows")]
    let mut cmd = {
        let mut c = std::process::Command::new("rundll32");
        c.args(["url.dll,FileProtocolHandler", &url]);
        c
    };
    #[cfg(target_os = "macos")]
    let mut cmd = {
        let mut c = std::process::Command::new("open");
        c.arg(&url);
        c
    };
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let mut cmd = {
        let mut c = std::process::Command::new("xdg-open");
        c.arg(&url);
        c
    };
    cmd.spawn().map_err(|e| format!("Browser openen mislukt: {}", e))?;
    Ok(())
}

// ===== MIDI-uit terugkoppeling (fase 1): registerlampen/display =====

/// Beschikbare MIDI-uitgangspoorten (voor de terugkoppel-poortkeuze).
#[tauri::command]
pub fn feedback_list_outputs(state: State<AppState>) -> Vec<String> {
    state.feedback.lock().list_outputs()
}

/// Eén register-slot uit de UI: id (stop/koppel) + nootnummer (NoteOnOff).
#[derive(serde::Deserialize)]
pub struct FeedbackSlotDto {
    pub id: String,
    #[serde(default)]
    pub note: u8,
}

/// Protocol + poort + kanaal + slot-lijst instellen. `protocol`: "off"/"note"/
/// "lcd"/"johannus". Volgorde van `slots` = register-index (LCD/Johannus).
#[tauri::command]
pub fn feedback_configure(
    state: State<AppState>,
    protocol: String,
    port: Option<String>,
    channel: u8,
    slots: Vec<FeedbackSlotDto>,
    organ_name: String,
) -> Result<(), String> {
    let proto = crate::feedback::FeedbackProtocol::parse(&protocol);
    let slots = slots.into_iter()
        .map(|s| crate::feedback::FeedbackSlot { id: s.id, note: s.note })
        .collect();
    state.feedback.lock().configure(proto, port, channel, slots, organ_name)
}

/// Nieuwe actieve registratie (stop- + koppel-id's) — diff'd en gebatcht gestuurd.
#[tauri::command]
pub fn feedback_apply(state: State<AppState>, active_ids: Vec<String>) {
    state.feedback.lock().apply(&active_ids);
}

/// Alles uit op de console (bij Johannus incl. JOHAS-reset).
#[tauri::command]
pub fn feedback_all_off(state: State<AppState>) {
    state.feedback.lock().all_off();
}

#[tauri::command]
pub fn set_midi_mapping(state: State<AppState>, division: String, channel: Option<u8>, transpose: i8) -> Result<(), String> {
    info!("Setting MIDI mapping for {}: channel={:?}, transpose={}", division, channel, transpose);
    state.set_midi_mapping(&division, channel, transpose);
    Ok(())
}

/// Kort octaaf (C/E) aan of uit voor één klavier (0.7.47). Zie
/// `MidiChannelMapping::short_octave`.
#[tauri::command]
pub fn set_short_octave(state: State<AppState>, division: String, enabled: bool) -> Result<(), String> {
    info!("Kort octaaf voor {}: {}", division, if enabled { "aan" } else { "uit" });
    state.set_short_octave(&division, enabled);
    Ok(())
}

#[tauri::command]
pub fn get_midi_mappings(state: State<AppState>) -> Vec<MidiMappingDto> {
    state.get_midi_mappings().into_iter().map(|m| MidiMappingDto {
        division: m.division,
        channel: m.channel,
        transpose: m.transpose,
        first_midi_note: m.first_midi_note,
        last_midi_note: m.last_midi_note,
        short_octave: m.short_octave,
    }).collect()
}

#[tauri::command]
pub fn learn_midi_channel(state: State<AppState>, division: String) -> Result<Option<u8>, String> {
    info!("Learning MIDI channel for division: {}", division);
    state.learn_midi_channel(&division)
}

/// Result of learning a keyboard range
#[derive(Debug, Clone, Serialize)]
pub struct LearnedKeyboardRangeDto {
    pub channel: u8,
    pub first_note: u8,
    pub last_note: u8,
    pub transpose: i8,
}

/// Learn keyboard range for a division (two-note)
/// Press lowest key, then highest key
#[tauri::command]
pub fn learn_keyboard_range(state: State<AppState>, division: String, first_sample_note: u8) -> Result<Option<LearnedKeyboardRangeDto>, String> {
    info!("Learning keyboard range for division: {} (first_sample_note={})", division, first_sample_note);
    info!("Press the LOWEST key on your keyboard, then the HIGHEST key");

    let result = state.learn_keyboard_range(&division, first_sample_note)?;

    Ok(result.map(|r| LearnedKeyboardRangeDto {
        channel: r.channel,
        first_note: r.first_note,
        last_note: r.last_note,
        transpose: r.transpose,
    }))
}

/// Wacht op een pedaalstand (voor de stapsgewijze trede-inleer-popup): de
/// gebruiker beweegt de trede en houdt hem stil → (kanaal, cc, waarde).
/// `channel`/`cc_num` beperken fase 2 tot dezelfde trede.
#[tauri::command]
pub fn learn_pedal_position(state: State<AppState>, channel: Option<u8>, cc_num: Option<u8>) -> Result<Option<(u8, u8, u8)>, String> {
    let filter = match (channel, cc_num) {
        (Some(ch), Some(cc)) => Some((ch, cc)),
        _ => None,
    };
    state.learn_pedal_position(filter)
}

/// Pas een via de popup ingeleerde ZWELTREDE toe. Inversie wordt automatisch
/// gedetecteerd (laagste stand gaf hogere CC-waarde dan hoogste stand).
/// Wederzijds exclusief met de generaal crescendo (laatst ingeleerd wint) —
/// via de gedeelde helper `claim_pedal_cc`; de verdrongen koppeling gaat terug
/// naar de UI zodat de popup meldt wat er is overgenomen.
#[tauri::command]
pub fn apply_learned_swell(state: State<AppState>, division: String, division_index: u8, channel: u8, cc_num: u8, low_val: u8, high_val: u8) -> Result<Vec<crate::state::DisplacedPedalBinding>, String> {
    use crate::state::{PedalCcKind, SwellBinding};
    let (min_val, max_val, invert) = if low_val <= high_val {
        (low_val, high_val, false)
    } else {
        (high_val, low_val, true)
    };
    let verdrongen = state.claim_pedal_cc(PedalCcKind::Swell, channel, cc_num);
    let binding = SwellBinding {
        division_name: division.clone(),
        division_index,
        channel,
        cc_num,
        min_val,
        max_val,
        invert,
        // De trede staat nu in de hoogste stand (laatste inleerstap).
        last_value: Some(high_val),
    };
    let mut bindings = state.swell_bindings.write();
    bindings.retain(|b| b.division_name != division);
    bindings.push(binding);
    info!("Zweltrede ingeleerd via popup: {} → CC{} kanaal {} bereik {}-{}{}",
        division, cc_num, channel + 1, min_val, max_val, if invert { " (geïnverteerd)" } else { "" });
    Ok(verdrongen)
}

/// Pas een via de popup ingeleerde CRESCENDOTREDE toe (zelfde inversie-detectie).
/// Verdringt zwelkoppelingen op dezelfde (kanaal, CC) via de gedeelde helper en
/// meldt ze terug aan de UI.
#[tauri::command]
pub fn apply_learned_crescendo(state: State<AppState>, channel: u8, cc_num: u8, low_val: u8, high_val: u8) -> Result<Vec<crate::state::DisplacedPedalBinding>, String> {
    use crate::state::PedalCcKind;
    let (min_val, max_val, invert) = if low_val <= high_val {
        (low_val, high_val, false)
    } else {
        (high_val, low_val, true)
    };
    *state.crescendo_binding.write() = Some((channel, cc_num, min_val, max_val, invert));
    // Wederzijds exclusief (gedeelde helper, logt zelf) + expressie-reset
    // (zie learn_crescendo_pedal).
    let verdrongen = state.claim_pedal_cc(PedalCcKind::Crescendo, channel, cc_num);
    // Via de kern: claims los, trede-registers weg, DTO-vlaggen en voice-sync
    // (een ruwe stage-write liet trede-registers met lege teller staan).
    let _ = state.apply_crescendo_stage(0);
    state.send_audio_command(AudioCommand::SetMasterExpression(1.0));
    info!("Crescendotrede ingeleerd via popup: CC{} kanaal {} bereik {}-{}{}",
        cc_num, channel + 1, min_val, max_val, if invert { " (geïnverteerd)" } else { "" });
    Ok(verdrongen)
}

/// Wacht op één toetsaanslag (voor de stapsgewijze klavier-inleer-popup).
/// Geeft (kanaal, noot) terug, of None bij time-out (20 s).
#[tauri::command]
pub fn learn_keyboard_note(state: State<AppState>) -> Result<Option<(u8, u8)>, String> {
    state.learn_single_note()
}

/// Pas een via de popup ingeleerd toetsbereik toe: zelfde transpose-logica als
/// het klassieke twee-noten-inleren (laagste noot → first_sample_note).
#[tauri::command]
pub fn apply_learned_keyboard_range(state: State<AppState>, division: String, channel: u8, low: u8, high: u8, first_sample_note: u8) -> Result<LearnedKeyboardRangeDto, String> {
    use crate::state::MidiChannelMapping;
    let (low, high) = if low <= high { (low, high) } else { (high, low) };
    let transpose = (first_sample_note as i16 - low as i16) as i8;
    let mut mappings = state.get_midi_mappings();
    if let Some(m) = mappings.iter_mut().find(|m| m.division == division) {
        m.channel = Some(channel);
        m.first_midi_note = Some(low);
        m.last_midi_note = Some(high);
        m.transpose = transpose;
    } else {
        mappings.push(MidiChannelMapping {
            division: division.clone(),
            channel: Some(channel),
            transpose,
            first_midi_note: Some(low),
            last_midi_note: Some(high),
            short_octave: false,
        });
    }
    state.set_midi_mappings(mappings);
    info!("Klavier ingeleerd via popup: {} ch={} bereik {}-{} transpose={}", division, channel + 1, low, high, transpose);
    Ok(LearnedKeyboardRangeDto { channel, first_note: low, last_note: high, transpose })
}

/// Preset binding info for frontend
#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct PresetBindingDto {
    pub preset_num: u8,
    pub trigger_type: String,  // "note", "cc", "program"
    pub trigger_value: String, // human readable description
}

#[tauri::command]
pub fn get_preset_bindings(state: State<AppState>) -> Vec<PresetBindingDto> {
    use crate::state::MidiPresetTrigger;

    state.get_preset_bindings().into_iter().map(|b| {
        let (trigger_type, trigger_value) = match b.trigger {
            MidiPresetTrigger::Note { channel, note } => {
                let ch = channel.map_or("elk".to_string(), |c| format!("{}", c + 1));
                ("note".to_string(), format!("Note {} (kan. {})", note, ch))
            }
            MidiPresetTrigger::ControlChange { channel, controller, value } => {
                let ch = channel.map_or("Any".to_string(), |c| format!("{}", c + 1));
                ("cc".to_string(), format!("CC{} >= {} (ch {})", controller, value, ch))
            }
            MidiPresetTrigger::ProgramChange { channel, program } => {
                let ch = channel.map_or("Any".to_string(), |c| format!("{}", c + 1));
                ("program".to_string(), format!("PC{} (ch {})", program, ch))
            }
            MidiPresetTrigger::SysEx { ref data } => {
                ("sysex".to_string(), format!("SysEx {}", crate::library::sysex_naar_hex(data)))
            }
            MidiPresetTrigger::ControlChangeBit { channel, controller, bit } => {
                let ch = channel.map_or("Any".to_string(), |c| format!("{}", c + 1));
                ("ccbit".to_string(), format!("CC{} bit {} (ch {})", controller, bit, ch))
            }
        };
        PresetBindingDto {
            preset_num: b.preset_num,
            trigger_type,
            trigger_value,
        }
    }).collect()
}

#[tauri::command]
pub fn learn_preset_binding(state: State<AppState>, preset_num: u8) -> Result<Option<String>, String> {
    info!("Learning MIDI binding for preset: {}", preset_num);
    let result = state.learn_preset_binding(preset_num)?;
    Ok(result.map(|_| "Learned".to_string()))
}

#[tauri::command]
pub fn clear_preset_binding(state: State<AppState>, preset_num: u8) {
    info!("Clearing all MIDI bindings for action: {}", preset_num);
    state.clear_preset_bindings_for(preset_num);
}

#[tauri::command]
pub fn poll_preset_trigger(state: State<AppState>) -> Option<u8> {
    // Heartbeat voor de afstandsbediening: zolang SetzerBar (Orgel-tabblad)
    // gemount is komt deze poll elke 100 ms; /action weigert anders met 409.
    *state.preset_poll_seen.write() = Some(std::time::Instant::now());
    state.poll_preset_trigger()
}

/// Setzerstand van het hoofdvenster spiegelen (voor GET /state van de
/// afstandsbediening). Tauri 2 zet het camelCase-argument setMode om.
#[tauri::command]
pub fn report_setzer_state(state: State<AppState>, level: u8, preset: i32, set_mode: bool, data: Vec<bool>) {
    *state.setzer_mirror.write() = crate::state::SetzerMirror { level, preset, set_mode, data };
}

// ============ Handmatig beheer van MIDI-koppelingen ============
// get_preset_bindings geeft alleen een mens-leesbare string; voor de
// bewerken-UI zijn de structurele velden nodig. PresetBindingSaved (het
// opslagformaat) is daarvoor het natuurlijke DTO.

fn preset_binding_to_saved(b: &crate::state::MidiPresetBinding) -> PresetBindingSaved {
    use crate::state::MidiPresetTrigger;
    match b.trigger {
        MidiPresetTrigger::Note { channel, note } => PresetBindingSaved {
            preset_num: b.preset_num,
            trigger_type: "note".to_string(),
            note: Some(note),
            channel,
            controller: None,
            value: None,
            program: None,
            sysex_hex: None,
            bit: None,
        },
        MidiPresetTrigger::ControlChange { channel, controller, value } => PresetBindingSaved {
            preset_num: b.preset_num,
            trigger_type: "cc".to_string(),
            note: None,
            channel,
            controller: Some(controller),
            value: Some(value),
            program: None,
            sysex_hex: None,
            bit: None,
        },
        MidiPresetTrigger::ProgramChange { channel, program } => PresetBindingSaved {
            preset_num: b.preset_num,
            trigger_type: "program".to_string(),
            note: None,
            channel,
            controller: None,
            value: None,
            program: Some(program),
            sysex_hex: None,
            bit: None,
        },
        MidiPresetTrigger::SysEx { ref data } => PresetBindingSaved {
            preset_num: b.preset_num,
            trigger_type: "sysex".to_string(),
            note: None,
            channel: None,
            controller: None,
            value: None,
            program: None,
            sysex_hex: Some(crate::library::sysex_naar_hex(data)),
            bit: None,
        },
        MidiPresetTrigger::ControlChangeBit { channel, controller, bit } => PresetBindingSaved {
            preset_num: b.preset_num,
            trigger_type: "ccbit".to_string(),
            note: None,
            channel,
            controller: Some(controller),
            value: None,
            program: None,
            sysex_hex: None,
            bit: Some(bit),
        },
    }
}

fn saved_to_preset_binding(b: &PresetBindingSaved) -> Result<crate::state::MidiPresetBinding, String> {
    use crate::state::{MidiPresetBinding, MidiPresetTrigger};
    let trigger = match b.trigger_type.as_str() {
        "note" => MidiPresetTrigger::Note {
            channel: b.channel.map(|c| c.min(15)),
            note: b.note.ok_or("Nootnummer ontbreekt")?.min(127),
        },
        "cc" => MidiPresetTrigger::ControlChange {
            channel: b.channel.map(|c| c.min(15)),
            controller: b.controller.ok_or("CC-nummer ontbreekt")?.min(127),
            value: b.value.unwrap_or(64).min(127),
        },
        "program" => MidiPresetTrigger::ProgramChange {
            channel: b.channel.map(|c| c.min(15)),
            program: b.program.ok_or("Programmanummer ontbreekt")?.min(127),
        },
        "sysex" => MidiPresetTrigger::SysEx {
            data: crate::library::hex_naar_sysex(
                b.sysex_hex.as_deref().ok_or("SysEx-inhoud ontbreekt")?)?,
        },
        "ccbit" => MidiPresetTrigger::ControlChangeBit {
            channel: b.channel.map(|c| c.min(15)),
            controller: b.controller.ok_or("CC-nummer ontbreekt")?.min(127),
            bit: b.bit.unwrap_or(0).min(6),
        },
        other => return Err(format!("Onbekend triggertype '{}'", other)),
    };
    Ok(MidiPresetBinding { preset_num: b.preset_num, trigger })
}

/// Alle actie-koppelingen structureel (voor de handmatige bewerken-UI).
#[tauri::command]
pub fn get_preset_bindings_full(state: State<AppState>) -> Vec<PresetBindingSaved> {
    state.get_preset_bindings().iter().map(preset_binding_to_saved).collect()
}

/// Voeg handmatig een koppeling toe (zelfde regels als inleren: max 4 per
/// actie, duplicaten geweigerd). Retourneert de bijgewerkte volledige lijst.
#[tauri::command]
pub fn add_preset_binding_manual(state: State<AppState>, binding: PresetBindingSaved) -> Result<Vec<PresetBindingSaved>, String> {
    let b = saved_to_preset_binding(&binding)?;
    info!("Handmatige MIDI-koppeling toevoegen voor actie {}: {:?}", b.preset_num, b.trigger);
    state.add_preset_binding(b)?;
    Ok(state.get_preset_bindings().iter().map(preset_binding_to_saved).collect())
}

/// Verwijder één koppeling: de `index`-de (0-based) binding van deze actiecode.
#[tauri::command]
pub fn remove_preset_binding(state: State<AppState>, preset_num: u8, index: usize) -> Result<Vec<PresetBindingSaved>, String> {
    info!("MIDI-koppeling #{} voor actie {} verwijderen", index + 1, preset_num);
    state.remove_preset_binding_at(preset_num, index)?;
    Ok(state.get_preset_bindings().iter().map(preset_binding_to_saved).collect())
}

/// Vervang één koppeling in-place (bewerken in de UI).
#[tauri::command]
pub fn update_preset_binding(state: State<AppState>, preset_num: u8, index: usize, binding: PresetBindingSaved) -> Result<Vec<PresetBindingSaved>, String> {
    let b = saved_to_preset_binding(&binding)?;
    info!("MIDI-koppeling #{} voor actie {} bijwerken naar {:?}", index + 1, preset_num, b.trigger);
    state.replace_preset_binding_at(preset_num, index, b)?;
    Ok(state.get_preset_bindings().iter().map(preset_binding_to_saved).collect())
}

/// Zet handmatig kanaal + CC voor de zwelkast van een divisie (het handmatige
/// alternatief voor 'Leer pedaal'). Kanaal is 0-based (wire), zoals overal.
/// Dezelfde exclusiviteit als de inleer-popup (claim_pedal_cc): een crescendo
/// op deze CC vervalt en wordt aan de UI gemeld — vroeger ontstonden hier twee
/// koppelingen op dezelfde CC waarvan er stilletjes één dood was.
#[tauri::command]
pub fn set_swell_binding_manual(state: State<AppState>, division: String, division_index: u8, channel: u8, cc_num: u8) -> Result<Vec<crate::state::DisplacedPedalBinding>, String> {
    info!("Handmatige zwelkast-koppeling voor {}: kanaal {} CC{}", division, channel + 1, cc_num);
    Ok(state.set_swell_binding_manual(&division, division_index, channel.min(15), cc_num.min(127)))
}

/// Zet handmatig kanaal + CC voor de generaal-crescendo. Bestaand bereik en
/// invert blijven behouden. Verdringt zwelkoppelingen op dezelfde (kanaal, CC)
/// via de gedeelde helper en meldt ze terug aan de UI.
#[tauri::command]
pub fn set_crescendo_binding_manual(state: State<AppState>, channel: u8, cc_num: u8) -> Result<Vec<crate::state::DisplacedPedalBinding>, String> {
    use crate::state::PedalCcKind;
    let (ch, cc) = (channel.min(15), cc_num.min(127));
    info!("Handmatige crescendo-koppeling: kanaal {} CC{}", ch + 1, cc);
    {
        let mut b = state.crescendo_binding.write();
        // Bereik en spiegelbeeld horen bij een fysieke trede: alleen behouden
        // als (kanaal, CC) gelijk blijft; een andere trede begint op 0..127
        // zonder spiegel (0.7.44 — erfde eerder het bereik van de vorige trede).
        let (mn, mx, inv) = match *b {
            Some((och, occ, mn, mx, inv)) if och == ch && occ == cc => (mn, mx, inv),
            _ => (0, 127, false),
        };
        *b = Some((ch, cc, mn, mx, inv));
    }
    let verdrongen = state.claim_pedal_cc(PedalCcKind::Crescendo, ch, cc);
    // Deze CC is vanaf nu exclusief crescendo; een eerdere expressie-demping
    // van dezelfde pedaal (CC7/11-fallback) zou anders blijvend hangen.
    state.send_audio_command(AudioCommand::SetMasterExpression(1.0));
    Ok(verdrongen)
}

/// Zet handmatig het toetsenbereik (laagste/hoogste MIDI-noot) van een divisie.
#[tauri::command]
pub fn set_midi_mapping_range(state: State<AppState>, division: String, first_midi_note: Option<u8>, last_midi_note: Option<u8>) -> Result<(), String> {
    info!("Handmatig toetsenbereik voor {}: {:?}..{:?}", division, first_midi_note, last_midi_note);
    state.set_midi_mapping_range(&division, first_midi_note.map(|n| n.min(127)), last_midi_note.map(|n| n.min(127)));
    Ok(())
}

/// Async + spawn_blocking (zoals set_audio_output): een load duurt bij een
/// grote set 30-60 s en nam voorheen als sync-command de UI-thread in beslag —
/// alle vensters "Niet reagerend", geen voortgangs-events, en samen met de
/// load_switch_gate bevroor dat de hele app tijdens audio-wissels.
#[tauri::command]
pub async fn load_organ(state: State<'_, AppState>, path: String) -> Result<OrganInfoDto, String> {
    let st = state.inner().clone();
    tokio::task::spawn_blocking(move || do_load_organ(&st, &path))
        .await
        .map_err(|e| format!("orgel-laadtaak mislukt: {}", e))?
}

/// Herbruikbare load-organ logic die zowel vanuit Tauri commands als vanuit de test API
/// kan worden aangeroepen. Neemt een `&AppState` (niet `State<AppState>`).
/// GrandOrgue AmplitudeLevel (procent; 100 = neutraal, 0-1000) → dB.
/// 0 of afwezig behandelen we als "niet gezet" → neutraal (0 dB).
fn ampl_to_db(ampl: f32) -> f32 {
    if ampl > 0.0 && (ampl - 100.0).abs() > f32::EPSILON {
        20.0 * (ampl / 100.0).log10()
    } else {
        0.0
    }
}

/// MIDI-noot van pijp-index 0 van een stop. Hauptwerk zet de eerste noot
/// direct (first_midi_note); GrandOrgue: eerste toets van het manual +
/// (FirstAccessiblePipeLogicalKeyNumber − 1) — zo komt een discant-helft op
/// bv. MIDI 60 i.p.v. op de 36 van het manual. Zonder manual (stop niet in
/// een stoplijst): terugval op 36.
fn stop_first_midi(definition: &OrganDefinition, stop: &vpo_sampler::StopDef) -> u32 {
    if let Some(n) = stop.first_midi_note {
        return n;
    }
    let manual_first = definition
        .manuals
        .iter()
        .find(|m| m.stop_ids.contains(&stop.id))
        .map(|m| m.first_accessible_key_midi_note)
        .unwrap_or(36);
    manual_first + stop.first_accessible_pipe_logical_key.saturating_sub(1)
}

/// Kies de default/langste release van een pijp: MaxKeyPressTime `None` of `-1`
/// betekent "geen limiet" (GrandOrgue-conventie) en wint; anders de langste
/// tijd. Releases die naar het attack-bestand zelf wijzen worden hier
/// overgeslagen — dat geval dekt de same-file-release-route (cue-marker).
/// Toetsduur-afhankelijke selectie (R0/R1/R2) is een latere uitbreiding.
fn pick_default_release<'a>(
    releases: &'a [vpo_sampler::ReleaseDef],
    attack_path: &Path,
) -> Option<&'a vpo_sampler::ReleaseDef> {
    releases
        .iter()
        .filter(|r| r.path != attack_path)
        .max_by_key(|r| match r.max_key_press_time_ms {
            None | Some(-1) => i64::MAX,
            Some(t) => t as i64,
        })
}

/// Manifest dat JM-Rec naast de `.organ` schrijft (`<stem>.jm-rec.json`) —
/// alleen de velden die de bibliotheeknaam bepalen. Dit is de PRIMAIRE bron:
/// het bevat precies wat de gebruiker in de wizard invoerde. De ODF-velden
/// zijn de terugval, want JM-Rec schrijft de MAPCODE als `ChurchName` zodra de
/// kerknaam leeg bleef (bv. "PuttBätz").
#[derive(Debug, Clone, Default, Deserialize)]
struct JmRecManifest {
    #[serde(default)]
    kerk: String,
    #[serde(default)]
    plaats: String,
    #[serde(default)]
    bouwer: String,
}

/// Lees `<stem>.jm-rec.json` naast een orgelbestand (None als het er niet is
/// of niet te lezen valt).
fn read_jm_rec_manifest(odf_path: &Path) -> Option<JmRecManifest> {
    let stem = odf_path.file_stem()?.to_string_lossy().to_string();
    let manifest = odf_path.with_file_name(format!("{}.jm-rec.json", stem));
    let txt = std::fs::read_to_string(&manifest).ok()?;
    serde_json::from_str::<JmRecManifest>(&txt).ok()
}

/// De JM-Rec-naamregel: "Kerknaam - Orgelbouwer - Plaats", lege delen weg.
/// None als er niets bruikbaars overblijft.
fn jm_rec_naam(kerk: &str, bouwer: &str, plaats: &str) -> Option<String> {
    let delen: Vec<&str> = [kerk.trim(), bouwer.trim(), plaats.trim()]
        .into_iter()
        .filter(|d| !d.is_empty())
        .collect();
    if delen.is_empty() {
        None
    } else {
        Some(delen.join(" - "))
    }
}

/// Naam/bouwer/plaats uit een JM-Rec-manifest in een SAMPLE-MAP: een
/// projectmap waarvan de gebruiker (nog) geen .organ exporteerde, die dus via
/// de mapscan geladen wordt. Zonder dit zou zo'n set als mapcode ("PuttBätz")
/// in de bibliotheek komen.
fn jm_rec_identity_for_dir(dir: &Path) -> Option<(String, String, String)> {
    let manifest = std::fs::read_dir(dir).ok()?.flatten().map(|e| e.path()).find(|p| {
        p.file_name()
            .map(|n| n.to_string_lossy().to_lowercase().ends_with(".jm-rec.json"))
            .unwrap_or(false)
    })?;
    let m: JmRecManifest = serde_json::from_str(&std::fs::read_to_string(&manifest).ok()?).ok()?;
    let naam = jm_rec_naam(&m.kerk, &m.bouwer, &m.plaats)?;
    Some((naam, m.bouwer.trim().to_string(), m.plaats.trim().to_string()))
}

/// Naam, bouwer en plaats voor bibliotheek + kopregel.
///
/// * JM-Rec-set (manifest ernaast óf `OrganComments` begint met "Opgenomen met
///   JM-Rec"): **"Kerknaam - Orgelbouwer - Plaats"**. Lege delen vallen weg, en
///   zónder manifest valt een `ChurchName` die gelijk is aan de bestandsstam
///   (de mapcode van JM-Rec) ook weg.
/// * Andere sets (GrandOrgue/Hauptwerk): naam en bouwer ongewijzigd — die namen
///   bevatten de plaats vaak al ("Lędziny, St. Clement").
///
/// `location` is voor álle orgeldefinities de PLAATS (`ChurchAddress`, bij
/// Hauptwerk `OrganInfo_Location`); alleen als die leeg is valt hij terug op
/// `RecordingDetails` — dat stond er vroeger altijd ("Recorded by ...").
pub fn display_identity(odf_path: &Path, organ: &vpo_sampler::OrganInfo) -> (String, String, String) {
    let address = organ.church_address.trim();
    let location = if address.is_empty() {
        organ.recording_details.trim().to_string()
    } else {
        address.to_string()
    };

    let manifest = read_jm_rec_manifest(odf_path);
    let is_jm_rec = manifest.is_some() || organ.organ_comments.starts_with("Opgenomen met JM-Rec");
    if !is_jm_rec {
        return (organ.church_name.clone(), organ.organ_builder.clone(), location);
    }

    let (kerk, bouwer, plaats) = match &manifest {
        Some(m) => (m.kerk.trim().to_string(), m.bouwer.trim().to_string(), m.plaats.trim().to_string()),
        None => {
            let stem = odf_path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
            let kerk = organ.church_name.trim();
            // Mapcode als kerknaam: niet tonen.
            let kerk = if kerk.eq_ignore_ascii_case(&stem) { "" } else { kerk };
            (kerk.to_string(), organ.organ_builder.trim().to_string(), address.to_string())
        }
    };

    let naam = jm_rec_naam(&kerk, &bouwer, &plaats).unwrap_or_else(|| organ.church_name.clone());
    let builder = if bouwer.is_empty() { organ.organ_builder.clone() } else { bouwer };
    let plek = if plaats.is_empty() { location } else { plaats };
    (naam, builder, plek)
}

/// Is dit orgel-id een orgel-definitiebestand (GrandOrgue .organ of Hauptwerk
/// .Organ_Hauptwerk_xml) — en dus voor do_load_organ — of een sample-map (voor
/// do_load_samples_from_directory)? Gebruikt door alle herlaad-routes.
pub fn is_organ_file_id(id: &str) -> bool {
    let l = id.to_lowercase();
    l.ends_with(".organ") || l.ends_with(".organ_hauptwerk_xml")
}

/// Map waarin de per-orgel `.jm-settings.json` hoort: naast het orgelbestand
/// (GrandOrgue én Hauptwerk, hoofdletter-ongevoelig) of de sample-map zelf.
/// De oude check (`ends_with(".organ")`, hoofdlettergevoelig) behandelde een
/// Hauptwerk-bestand — en elk `.Organ`-bestand — als map, waardoor instellingen
/// naast zulke orgels nooit werden weggeschreven of teruggelezen.
fn organ_settings_dir(organ_id: &str) -> Option<std::path::PathBuf> {
    if is_organ_file_id(organ_id) {
        std::path::Path::new(organ_id).parent().map(|p| p.to_path_buf())
    } else {
        Some(std::path::PathBuf::from(organ_id))
    }
}

/// Welke soort tremulant meldt deze divisie aan de UI?
///
/// * `"wave"` — er zijn ECHTE trem-opnamen én de ODF noemt het een
///   golfvormtremulant (`TremulantType=Wave`). De UI zet de synth-LFO dan uit:
///   de tremulant zit al in het geluid.
/// * `"samples"` — echte trem-opnamen zonder die vlag (o.a. Hauptwerk-"tremmed").
/// * `"synth"` — de ODF kent wel een tremulant, maar er is geen enkele opname:
///   alleen de synthetische LFO kan hem dan laten horen.
/// * `None` — deze divisie heeft geen tremulant.
///
/// De vlag alléén was eerder genoeg voor `"wave"`. Een set die
/// `TremulantType=Wave` zet maar waarvan wij geen trem-opname laden (bv. omdat
/// de pijpen geen `IsTremulant=1`-attack hebben) hield daardoor helemaal géén
/// tremulant over: de LFO stond uit en er viel niets te horen.
fn tremulant_kind_voor_divisie(
    heeft_trem_opnamen: bool,
    odf_zegt_wave: bool,
    heeft_odf_tremulant: bool,
) -> Option<&'static str> {
    if heeft_trem_opnamen {
        if odf_zegt_wave { Some("wave") } else { Some("samples") }
    } else if heeft_odf_tremulant {
        Some("synth")
    } else {
        None
    }
}

/// Stuur laad-voortgang naar de frontend (LoadingOverlay). Headless (test-API
/// vóór Tauri-setup) is er geen AppHandle — dan stilletjes overslaan.
fn emit_load_progress(state: &AppState, loaded: usize, total: usize, message: &str) {
    if let Some(app) = state.app_handle.read().as_ref() {
        use tauri::Emitter;
        let _ = app.emit("load-progress", serde_json::json!({
            "loaded": loaded,
            "total": total,
            "message": message,
        }));
    }
}

pub fn do_load_organ(state: &AppState, path: &str) -> Result<OrganInfoDto, String> {
    // Serialiseer t.o.v. audio-wissels en andere loads (zie load_switch_gate):
    // een wissel midden in een load zou de net-geregistreerde preload-buffers
    // weggooien en het herladen missen (current_organ_id nog niet gezet).
    let _gate = state.load_switch_gate.lock();
    do_load_organ_locked(state, path)
}

/// Variant ZONDER gate — voor aanroepers die de load_switch_gate al bezitten
/// (uitgestelde ASIO-wissel en audio-noodherstel in main.rs). parking_lot-
/// mutexen zijn niet re-entrant: dezelfde gate op dezelfde thread opnieuw
/// nemen is een PERMANENTE deadlock. Dat gebeurde hier: main.rs nam de gate
/// en riep do_load_organ aan, die hem opnieuw nam → audio-wissel voorgoed
/// vast en elke volgende load/autosave bevroor de hele app.
pub fn do_load_organ_locked(state: &AppState, path: &str) -> Result<OrganInfoDto, String> {
    // Watchdog pauzeren voor de duur van de load (RAII: ook bij een vroege
    // ?-return gaat de vlag weer uit). Een zware load kan ASIO4ALL-callbacks
    // tijdelijk laten haperen; zonder pauze escaleerde de watchdog dat tot een
    // volledige audio-herstart waarbij ASIO soms verloren ging (→ WASAPI).
    struct WatchdogHoldGuard(Option<std::sync::Arc<std::sync::atomic::AtomicBool>>);
    impl Drop for WatchdogHoldGuard {
        fn drop(&mut self) {
            if let Some(f) = &self.0 { f.store(false, std::sync::atomic::Ordering::Relaxed); }
        }
    }
    let _watchdog_hold = {
        let guard = state.audio_player.read();
        let flag = guard.as_ref().map(|p| p.watchdog_hold.clone());
        if let Some(f) = &flag { f.store(true, std::sync::atomic::Ordering::Relaxed); }
        WatchdogHoldGuard(flag)
    };

    // Normaliseer de pad-notatie (test-API/scripts leveren soms forward
    // slashes): orgel-id, bibliotheek en per-orgel-settings zien zo altijd
    // dezelfde vorm — voorkomt dubbele bibliotheek-entries.
    let path = &path.replace('/', "\\");
    info!("Loading organ from: {}", path);

    let odf_path = Path::new(path);
    // Hauptwerk sets (*.Organ_Hauptwerk_xml) use a different format; route them to
    // the Hauptwerk importer, which translates to the same OrganDefinition so the
    // streaming-load + DTO + noise-filter below are shared with GrandOrgue.
    let mut definition = if vpo_sampler::is_hauptwerk_path(odf_path) {
        info!("Detected Hauptwerk organ definition");
        vpo_sampler::load_hauptwerk(odf_path)
            .map_err(|e| format!("Failed to parse Hauptwerk ODF: {}", e))?
    } else {
        OrganDefinition::load(odf_path)
            .map_err(|e| format!("Failed to parse ODF: {}", e))?
    };

    // Tidy division/manual names (strip "N. " ordinals like "2. Hauptwerk"). The
    // name is the routing key for divisions AND couplers, so cleaning it here keeps
    // everything downstream consistent. (Hauptwerk names are already clean → no-op.)
    for manual in &mut definition.manuals {
        manual.name = clean_division_name(&manual.name);
    }

    info!("Parsed organ: {} by {}", definition.organ.church_name, definition.organ.organ_builder);

    // Divisions sitting in a swell box: een divisie is 'enclosed' wanneer de
    // meerderheid van haar échte registers op een windchest staat die een
    // Enclosure refereert. Structureel (geen naam-match): fixt sets waar de
    // windchest-naam afwijkt van de divisienaam (bv. Bureå "Enclosed Manual
    // wind-chest") en dekt ook de Hauptwerk-import (enclosure-per-windchest).
    let enclosed_divs: std::collections::HashSet<String> = definition.manuals.iter()
        .filter(|m| {
            let (enclosed, total) = m.stop_ids.iter()
                .filter_map(|sid| definition.stops.iter().find(|s| s.id == *sid))
                .filter(|s| !s.is_noise_or_mechanical())
                .fold((0usize, 0usize), |(e, t), s| {
                    let enc = definition.windchests.iter()
                        .any(|w| w.number == s.windchest_group && !w.enclosure_ids.is_empty());
                    (e + enc as usize, t + 1)
                });
            total > 0 && enclosed * 2 >= total
        })
        .map(|m| m.name.clone())
        .collect();
    if !enclosed_divs.is_empty() {
        info!("Swell-enclosed divisions: {:?}", enclosed_divs);
    }

    // Load samples via het preload + achtergrond-stream-mechanisme (zelfde route
    // als de custom sample-mappen, do_load_samples_from_directory). We laden NIET
    // eager elke sample volledig in RAM — dat liet grote GrandOrgue-sets vastlopen
    // (honderden lange samples ineens → geheugendruk/swapping → app onresponsief).
    // In plaats daarvan: alleen de attack (~PRELOAD_SAMPLES frames) per uniek
    // bestand inlezen voor directe respons; de audio-thread streamt de volledige
    // sample op de achtergrond zodra een noot gespeeld wordt.
    // Hertemper-indicator voor de DTO: aantal pijpen met plausibele gemeten
    // toonhoogte / aantal speelbare pijpen (gevuld in het laadblok hieronder).
    let retune_pipes: usize;
    let retune_total: usize;
    // Pijpen met een release-opname (= opgenomen kerkakoestiek). Zie
    // OrganInfoDto::release_pipes.
    let release_pipes: usize;
    // Perspectieven-plan (gevuld in het laadblok; ná reset_organ_scoped_state
    // in state.perspectives gezet — anders wist de reset hem direct weer).
    let persp: Vec<PerspectiveRuntime>;
    // Registers met échte tremulant-opnamen (GO `IsTremulant=1`, Hauptwerk
    // "tremmed"-laag): vult StopDto.has_tremulant en de engine-set die de
    // synth-LFO voor die registers uitschakelt.
    let mut stops_with_trem: std::collections::HashSet<u32> = std::collections::HashSet::new();
    {
        use std::collections::{HashMap, HashSet};
        use std::path::PathBuf;
        use std::sync::Arc;
        use rayon::prelude::*;
        use vpo_sampler::{load_audio_preload, load_wav_preload_segment, PipeDef, PreloadBuffer, PreloadSegment};
        use crate::audio::PRELOAD_SAMPLES;

        /// Welke uitsnede van een bestand geladen moet worden — attacks en
        /// release-segmenten van hetzélfde bestand zijn verschillende buffers.
        #[derive(Clone, Copy, PartialEq, Eq, Hash)]
        enum SegKey {
            Attack,
            /// `require_cue`: same-file release (moet loop+cue hebben, anders
            /// bestaat er geen release); anders apart release-bestand (start op
            /// cue-marker als die er is, anders frame 0).
            Release { cue_override: Option<u32>, require_cue: bool },
        }

        // (stop_id, pipe_num) → concreet samplepad. Referenties (REF:...) worden
        // hier al opgelost zodat de achtergrond-loader rechtstreeks van het bestand
        // kan streamen. pipe_num = pijp-index+1 (zoals de NoteOn-route verwacht:
        // midi_note - first_midi_note + 1).
        let mut load_tasks: Vec<((u32, u32), PathBuf)> = Vec::new();
        // Percussieve stops (one-shot: klok e.d.) en ODF-looppunten per bestand.
        // GrandOrgue-regel: zodra de ODF een loop opgeeft, winnen die van de
        // smpl-chunk in de WAV (override, geen fallback).
        let mut percussive_stops: HashSet<u32> = HashSet::new();
        let mut odf_loops: HashMap<PathBuf, (u32, u32)> = HashMap::new();
        // ODF-intonatie per pijp: de volledige GrandOrgue-hiërarchie
        // Organ × Windchest × Stop/Rank × Pipe. AmplitudeLevel% en Gain-dB
        // stapelen als dB-som; PitchTuning (cents) wordt gesommeerd.
        // PitchCorrection blijft hier bewust BUITEN: dat is in GrandOrgue een
        // bewuste, OPGETELDE afwijking (zwever van een celeste) die alléén in
        // hertemper-modus meetelt — zie `retune_inputs` hieronder / retune.rs.
        // In "Origineel" telt alléén PitchTuning (anders zweeft een Voix
        // Céleste dubbel).
        let mut odf_voicings: HashMap<(u32, u32), (f32, f32)> = HashMap::new();
        // Hertemperen op gemeten pijptoonhoogte: per speelbare pijp de invoer
        // voor retune::retune_cents + het attack-pad (voor de smpl-meting uit
        // de preload-buffer) + de HW-terugval (originele pijptoon).
        let mut retune_inputs: Vec<((u32, u32), PathBuf, crate::retune::PipePitchInput, Option<f32>)> = Vec::new();
        // Door de sampleset opgegeven crossfade-duur per release-key (ms):
        // GrandOrgue `ReleaseCrossfadeLength`, 1-3000 ms.
        let mut release_xfade: HashMap<(u32, u32), u32> = HashMap::new();
        // Aantal pijpen met een aparte tremulant-opname (voor de logregel).
        let mut trem_pipe_count = 0usize;
        // Release-samples (opgenomen kerkakoestiek), onder de gemarkeerde key
        // (pipe_num | RELEASE_PIPE_FLAG). Twee vormen: een apart release-bestand,
        // of het release-segment ná de loop in het attack-bestand zelf
        // (cue-marker, GrandOrgue-model voor oudere natte sets).
        let mut release_tasks: Vec<((u32, u32), PathBuf, SegKey)> = Vec::new();
        // Toetsduur-release-metadata: (stop, pijp) → gesorteerde (max_ms, key).
        let mut release_meta: Vec<((u32, u32), Vec<(i32, u32)>)> = Vec::new();

        /// Zet de release-bestanden van één pijp om in laadtaken + toetsduur-meta.
        /// `key_pipe` is de pijpsleutel (met laagbits en, voor de tremulantstand,
        /// `TREM_FLAG`); de release-keys krijgen daar `RELEASE_PIPE_FLAG` en —
        /// voor de kortere MaxKeyPressTime-varianten — een index in bits 24-26 bij.
        ///
        /// Alle losse release-bestanden (R0/R1/…, toetsduur-varianten): oplopend
        /// op MaxKeyPressTime, de default (-1/None, langste) achteraan op de
        /// klassieke index-loze key — sets met één release gedragen zich exact als
        /// voorheen. De audio-thread kiest per NoteOff op de gespeelde duur
        /// (RegisterReleaseMeta, 0.7.26). Zonder losse bestanden: het
        /// release-segment ná de loop in het attack-bestand zelf (cue-marker).
        #[allow(clippy::too_many_arguments)]
        fn push_pipe_releases(
            stop_id: u32,
            key_pipe: u32,
            attack_path: &PathBuf,
            attack_cue: Option<u32>,
            load_release: Option<bool>,
            percussive: bool,
            releases: &[vpo_sampler::ReleaseDef],
            release_tasks: &mut Vec<((u32, u32), PathBuf, SegKey)>,
            release_meta: &mut Vec<((u32, u32), Vec<(i32, u32)>)>,
            release_xfade: &mut HashMap<(u32, u32), u32>,
        ) {
            let default_key = (stop_id, key_pipe | crate::audio::RELEASE_PIPE_FLAG);
            let mut file_releases: Vec<&vpo_sampler::ReleaseDef> = releases
                .iter()
                .filter(|r| r.path != *attack_path)
                .collect();
            if !file_releases.is_empty() {
                file_releases.sort_by_key(|r| match r.max_key_press_time_ms {
                    None | Some(-1) => i64::MAX,
                    Some(t) => t as i64,
                });
                file_releases.truncate(8); // idx past in 3 bits + default
                let n = file_releases.len();
                let mut meta: Vec<(i32, u32)> = Vec::with_capacity(n);
                for (idx, rel) in file_releases.iter().enumerate() {
                    let kp = if idx + 1 == n {
                        default_key.1 // default/langste → klassieke key
                    } else {
                        key_pipe | crate::audio::RELEASE_PIPE_FLAG | (((idx as u32) + 1) << 24)
                    };
                    release_tasks.push((
                        (stop_id, kp),
                        rel.path.clone(),
                        SegKey::Release { cue_override: rel.cue_point, require_cue: false },
                    ));
                    if let Some(ms) = rel.crossfade_ms {
                        release_xfade.insert((stop_id, kp), ms);
                    }
                    meta.push((
                        match rel.max_key_press_time_ms { None | Some(-1) => -1, Some(t) => t },
                        kp,
                    ));
                }
                if meta.len() > 1 {
                    release_meta.push(((stop_id, key_pipe), meta));
                }
            } else if !percussive && load_release.unwrap_or(true) {
                // Geen apart release-bestand: probeer het release-segment uit het
                // attack-bestand zelf (loop + cue). De loader slaat dit stil over
                // als het bestand geen cue-marker/loop heeft.
                release_tasks.push((
                    default_key,
                    attack_path.clone(),
                    SegKey::Release { cue_override: attack_cue, require_cue: true },
                ));
            }
        }

        let organ_db = ampl_to_db(definition.organ.amplitude_level) + definition.organ.gain_db;
        let organ_cents = definition.organ.pitch_tuning_cents;
        // Windchest-niveau inschaling per groepsnummer: (dB, PitchTuning,
        // PitchCorrection).
        let windchest_voicing: HashMap<u32, (f32, f32, f32)> = definition.windchests.iter()
            .map(|w| (w.number, (ampl_to_db(w.amplitude_level) + w.gain_db, w.pitch_tuning_cents, w.pitch_correction_cents)))
            .collect();
        // Perspectieven (0.7.38): welke labels laden (opgeslagen/runtime-
        // voorkeur; standaard alleen het primaire) en het laadplan per stop
        // voor de NoteOn-fan-out in de audio-thread. Echte extra ranks
        // (laag zonder perspectief) worden altijd geladen.
        let saved_persp = perspective_prefs_for(state, path);
        let mut persp_plan = plan_perspectives(&definition, &saved_persp);
        // "Alles laden" (0.7.47): ook de uitgeschakelde posities komen in het
        // geheugen, zodat wisselen ogenblikkelijk is in plaats van een herlaad.
        // Uitgeschakelde posities worden na het laden stilgezet (gain), niet
        // weggelaten.
        let alles_laden = *state.load_all_perspectives.read();
        let enabled_labels: HashSet<String> = persp_plan.iter()
            .filter(|p| alles_laden || p.enabled)
            .map(|p| p.name.clone())
            .collect();
        let mut rank_layout: HashMap<u32, Vec<(u8, u8)>> = HashMap::new();
        let mut skipped_layers = 0usize;
        for stop in &definition.stops {
            // Sla mechaniek-/ruisopnamen over (klep-/registergeluid, blaasbalg,
            // ambient) — die horen geen speelbaar register te zijn.
            if stop.is_noise_or_mechanical() {
                continue;
            }
            if stop.percussive {
                percussive_stops.insert(stop.id);
            }
            let (wc_db, wc_cents, _wc_pc) = windchest_voicing.get(&stop.windchest_group)
                .copied()
                .unwrap_or((0.0, 0.0, 0.0));
            let stop_db = ampl_to_db(stop.amplitude_level) + stop.gain_db;
            let stop_cents = stop.pitch_tuning_cents;
            for (layer_idx, layer_persp, layer_pipes) in stop.all_layers() {
            if let Some(p) = layer_persp {
                if !enabled_labels.contains(p) {
                    skipped_layers += 1;
                    continue;
                }
            }
            let slot = persp_plan.iter()
                .position(|x| Some(x.name.as_str()) == layer_persp)
                .map(|i| i as u8 + 1)
                .unwrap_or(0);
            let mut layer_any = false;
            for (pipe_idx, pipe) in layer_pipes.iter().enumerate() {
                // Engine-sleutel: pijpindex in bits 0-15, laag in bits 16-23.
                let pipe_num = crate::audio::layer_key((pipe_idx + 1) as u32, layer_idx);
                let resolved_opt = definition.resolve_reference(pipe);
                if resolved_opt.is_none() && matches!(pipe, PipeDef::Reference { .. }) {
                    // Onopgeloste REF was voorheen VOLLEDIG stil — de toets deed
                    // het gewoon niet, zonder één logregel (de bekende
                    // "stille toetsen"-klacht bij GO-imports). Nu benoemen we
                    // precies welke pijp van welk register geen sample kreeg.
                    warn!(
                        "Stop {} ('{}') pijp {}: REF-verwijzing onopgelost — deze toets blijft stil ({:?})",
                        stop.id, stop.name, pipe_num, pipe
                    );
                }
                if let Some(resolved) = resolved_opt {
                    if let PipeDef::Sample { path, extra } = resolved {
                        let effective_percussive = extra.percussive.unwrap_or(stop.percussive);
                        if extra.percussive == Some(true) {
                            percussive_stops.insert(stop.id);
                        }
                        // Is de PRIMAIRE opname de tremulantvariant (ODF
                        // `{key}IsTremulant=1`), dan staat de droge in een extra
                        // attack — die wint dan als attack-bestand van deze pijp.
                        let (path, attack_loop, attack_cue, attack_load_release) = match &extra.dry_attack {
                            Some(a) => (&a.path, (a.loop_start, a.loop_end), a.cue_point, a.load_release),
                            None => (path, (extra.loop_start, extra.loop_end), extra.cue_point, extra.load_release),
                        };
                        if let (Some(ls), Some(le)) = attack_loop {
                            if le > ls {
                                odf_loops.entry(path.clone()).or_insert((ls, le));
                            }
                        }
                        let pipe_db = extra.amplitude_level.map(ampl_to_db).unwrap_or(0.0)
                            + extra.gain_db.unwrap_or(0.0);
                        let total_db = organ_db + wc_db + stop_db + pipe_db;
                        let total_cents = organ_cents + wc_cents + stop_cents
                            + extra.pitch_tuning_cents.unwrap_or(0.0);
                        if total_db.abs() > 0.01 || total_cents.abs() > 0.01 {
                            odf_voicings.insert((stop.id, pipe_num), (total_db, total_cents));
                        }
                        // Hertemper-invoer met EIGENAAR-semantiek: bij een REF
                        // (Octaaf 4' → Prestant 8') gelden toets, HarmonicNumber,
                        // PitchCorrection en windchest van de eigenaar-stop, zodat
                        // de geleende pijp exact dezelfde correctie krijgt als het
                        // origineel (GO: GOReferencePipe speelt de eigenaar-pijp).
                        if let Some((owner, owner_idx, _)) = definition.resolve_reference_owner(stop, pipe_idx, pipe) {
                            let owner_wc_pc = windchest_voicing.get(&owner.windchest_group).map(|v| v.2).unwrap_or(0.0);
                            let key_midi = extra.key_midi_note
                                .unwrap_or(stop_first_midi(&definition, owner) + owner_idx as u32);
                            let odf_measured = extra.sample_pitch_cents.or(extra
                                .midi_key_number
                                .map(|k| 100.0 * k as f32 + extra.midi_pitch_fraction.unwrap_or(0.0)));
                            retune_inputs.push((
                                (stop.id, pipe_num),
                                path.clone(),
                                crate::retune::PipePitchInput {
                                    key_midi,
                                    harmonic: extra.harmonic_number.unwrap_or(owner.harmonic_number).max(1),
                                    measured_cents: odf_measured,
                                    pitch_correction_cents: definition.organ.pitch_correction_cents
                                        + owner_wc_pc
                                        + owner.pitch_correction
                                        + extra.pitch_correction_cents.unwrap_or(0.0),
                                    keep_pct: extra.retune_keep_pct.unwrap_or(0.0),
                                    original_pitch_tuning_cents: total_cents,
                                },
                                extra.original_pitch_cents,
                            ));
                        }
                        push_pipe_releases(
                            stop.id, pipe_num, path, attack_cue, attack_load_release,
                            effective_percussive, &extra.releases,
                            &mut release_tasks, &mut release_meta, &mut release_xfade,
                        );
                        load_tasks.push(((stop.id, pipe_num), path.clone()));
                        // Tremulant-OPNAME van deze pijp (GrandOrgue
                        // `{key}Attack{jjj}IsTremulant=1`, Hauptwerk "tremmed"-laag):
                        // een gewone preload onder dezelfde sleutel + TREM_FLAG.
                        // Attack, achtergrond-load, full sample en release staan zo
                        // per tremulantstand apart — NoteOn kiest de sleutel op de
                        // stand van het register.
                        if let Some(trem) = &extra.tremulant_attack {
                            let trem_pipe = pipe_num | crate::audio::TREM_FLAG;
                            if let (Some(ls), Some(le)) = (trem.loop_start, trem.loop_end) {
                                if le > ls {
                                    odf_loops.entry(trem.path.clone()).or_insert((ls, le));
                                }
                            }
                            // Releases van de tremulantstand. Is `tremulant_releases`
                            // leeg (geen trem-specifieke releases), dan blijft alleen
                            // de same-file-terugval over en valt NoteOff verder
                            // vanzelf terug op de droge release van dezelfde pijp.
                            push_pipe_releases(
                                stop.id, trem_pipe, &trem.path, trem.cue_point, trem.load_release,
                                effective_percussive, &extra.tremulant_releases,
                                &mut release_tasks, &mut release_meta, &mut release_xfade,
                            );
                            load_tasks.push(((stop.id, trem_pipe), trem.path.clone()));
                            stops_with_trem.insert(stop.id);
                            trem_pipe_count += 1;
                        }
                        layer_any = true;
                    }
                }
            }
            if layer_any {
                rank_layout.entry(stop.id).or_default().push((layer_idx, slot));
            }
            } // for layer
        }
        let multi_layer_stops = rank_layout.values().filter(|v| v.len() > 1).count();
        if multi_layer_stops > 0 || skipped_layers > 0 {
            info!(
                "Lagen: {} stops met meerdere lagen geladen, {} perspectief-lagen overgeslagen (uitgeschakeld); perspectieven: {:?}",
                multi_layer_stops, skipped_layers,
                persp_plan.iter().map(|p| format!("{}={}", p.name, if p.enabled { "aan" } else { "uit" })).collect::<Vec<_>>()
            );
        }

        // GrandOrgue-stops delen vaak één rank: meerdere (stop,pijp)-keys wijzen
        // dan naar hetzelfde bestand. Laad elke UNIEKE (bestand, segment)-combinatie
        // maar één keer en deel de Arc<PreloadBuffer> over alle keys (minder
        // disk-I/O en minder geheugen).
        let mut seen: HashSet<(PathBuf, SegKey)> = HashSet::new();
        let unique_loads: Vec<(PathBuf, SegKey)> = load_tasks
            .iter()
            .map(|(_, p)| (p.clone(), SegKey::Attack))
            .chain(release_tasks.iter().map(|(_, p, seg)| (p.clone(), *seg)))
            .filter(|entry| seen.insert(entry.clone()))
            .collect();

        info!(
            "GrandOrgue: {} pijp-keys, {} unieke (bestand, segment)-loads — attack-preloads laden...",
            load_tasks.len(),
            unique_loads.len()
        );
        let start_time = std::time::Instant::now();
        // Voortgang naar de frontend: zonder events blijft de laad-overlay op 0%
        // staan en oogt een grote set (30-60s koud van schijf) als vastgelopen.
        let progress_total = unique_loads.len();
        let progress_count = std::sync::atomic::AtomicUsize::new(0);
        emit_load_progress(state, 0, progress_total, "Samples laden");
        let mut path_buffers: HashMap<(PathBuf, SegKey), Arc<PreloadBuffer>> = unique_loads
            .par_iter()
            .filter_map(|entry| {
                let (p, seg) = entry;
                let n = progress_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                if n % 100 == 0 || n == progress_total {
                    emit_load_progress(state, n, progress_total, "Samples laden");
                }
                let result = match seg {
                    SegKey::Attack => load_audio_preload(p, PRELOAD_SAMPLES),
                    SegKey::Release { cue_override, require_cue } => load_wav_preload_segment(
                        p,
                        PRELOAD_SAMPLES,
                        PreloadSegment::Release { cue_override: *cue_override, require_cue: *require_cue },
                    ),
                };
                match result {
                    Ok(buf) => Some((entry.clone(), Arc::new(buf))),
                    Err(e) => {
                        // Same-file release zonder cue/loop is geen fout: dat
                        // bestand heeft simpelweg geen release-segment.
                        if !matches!(seg, SegKey::Release { require_cue: true, .. }) {
                            warn!("Failed to preload {:?}: {}", p, e);
                        }
                        None
                    }
                }
            })
            .collect();

        // ODF-looppunten: GrandOrgue-regel — heeft de ODF een loop, dan winnen
        // die van de smpl-chunk (override). Looppunten zijn bestandsframes; de
        // buffer is onset-getrimd, dus verschuiven met trim_start. De Arc's zijn
        // hier nog uniek (net gebouwd), dus get_mut werkt.
        let mut odf_loops_applied = 0usize;
        for (path, (ls, le)) in &odf_loops {
            if let Some(buf) = path_buffers.get_mut(&(path.clone(), SegKey::Attack)) {
                if let Some(b) = Arc::get_mut(buf) {
                    let trim = b.trim_start as u64;
                    let (ls, le) = (*ls as u64, *le as u64);
                    // Loop-begin vóór de trim = loopgebied aangetast → geen
                    // (verschoven) looppunten zetten; de engine-fallback is
                    // dan betrouwbaarder dan een fout loop-begin.
                    if ls >= trim && le > ls {
                        let a = ls - trim;
                        let mut e = le - trim;
                        // Zelfde fase-uitlijning als het smpl-pad: de eerste
                        // ~2 s van elke noot loopt op deze buffer.
                        if (e as usize) <= b.attack_data.len() {
                            if let Some(better) = vpo_sampler::optimize_loop_end_lr(
                                &b.attack_data, b.attack_right.as_deref(), a as usize, e as usize,
                            ) {
                                e = better as u64;
                            }
                        }
                        b.loop_start = Some(a);
                        b.loop_end = Some(e);
                        odf_loops_applied += 1;
                    }
                }
            }
        }
        if odf_loops_applied > 0 {
            info!("ODF-looppunten toegepast op {} bestanden (override op smpl)", odf_loops_applied);
        }
        info!(
            "Loaded {} unieke preload-buffers in {:.2}s",
            path_buffers.len(),
            start_time.elapsed().as_secs_f32()
        );

        // Hertemper-tabel: per pijp de correctie in cents die in hertemper-
        // modus de ODF-PitchTuning VERVANGT. Meting (prioriteit): ODF/HW-
        // sampleveld > smpl-chunk van het attack-bestand > HW originele
        // pijptoon. Onplausibele metingen (> 600 ct t.o.v. PitchTuning, GO's
        // waarschuwingsgrens) vallen af — die pijp blijft op PitchTuning.
        let mut odf_retune: HashMap<(u32, u32), f32> = HashMap::new();
        let mut retune_rejected = 0usize;
        for (key, path, mut inp, original_cents) in retune_inputs {
            if inp.measured_cents.is_none() {
                let target = crate::retune::target_cents(inp.key_midi, inp.harmonic);
                inp.measured_cents = path_buffers
                    .get(&(path, SegKey::Attack))
                    .and_then(|b| crate::retune::measured_from_smpl_for_target(b.smpl_unity_note, b.smpl_pitch_fraction_cents, target))
                    .or(original_cents)
                    // Geen meting (MP3-pijpen, WAV zonder smpl): zoals GrandOrgue
                    // de pijp als nominaal zuiver beschouwen — de temperament-
                    // offsets gelden dan t.o.v. de nominale toon en de ODF-
                    // PitchCorrection wordt wél toegepast. Anders viel bij
                    // JM-Rec-sets in MP3 het hele hertemperen stilletjes weg.
                    .or(Some(target));
            }
            let had_measurement = inp.measured_cents.is_some();
            match crate::retune::retune_cents(&inp) {
                Some(c) => { odf_retune.insert(key, c); }
                None if had_measurement => retune_rejected += 1,
                None => {}
            }
        }
        retune_pipes = odf_retune.len();
        retune_total = load_tasks.len();
        // Unieke FYSIEKE pijpen met release-opname: toetsduur-varianten,
        // tremulantstand en perspectief-lagen van dezelfde pijp tellen als één
        // (base_pipe haalt die bits eraf).
        release_pipes = release_tasks.iter()
            .map(|(key, _, _)| (key.0, crate::audio::base_pipe(key.1)))
            .collect::<std::collections::HashSet<_>>()
            .len();
        info!(
            "Hertemperen: {} van {} pijpen met gemeten toonhoogte ({} onplausibel verworpen)",
            retune_pipes, retune_total, retune_rejected
        );

        // Fan-out naar per-key buffers (gedeelde Arc). Release-keys (gemarkeerd
        // met RELEASE_PIPE_FLAG) gaan mee in dezelfde preload-map: NoteOff zoekt
        // ze daar op en de background-load/upgrade werkt er ongewijzigd mee
        // (PreloadBuffer.trim_start = segment-start voor release-uitsneden).
        let preload_buffers: HashMap<(u32, u32), Arc<PreloadBuffer>> = load_tasks
            .iter()
            .map(|(key, p)| (key, p.clone(), SegKey::Attack))
            .chain(release_tasks.iter().map(|(key, p, seg)| (key, p.clone(), *seg)))
            .filter_map(|(key, p, seg)| path_buffers.get(&(p, seg)).map(|buf| (*key, buf.clone())))
            .collect();
        let release_count = preload_buffers.keys()
            .filter(|(_, pn)| pn & crate::audio::RELEASE_PIPE_FLAG != 0)
            .count();
        if release_count > 0 {
            info!("Release-samples: {} pijpen met opgenomen kerkakoestiek bij note-off", release_count);
        }

        info!("Registering {} preload buffers for instant playback", preload_buffers.len());
        {
            let player_lock = state.audio_player.read();
            if let Some(ref player) = *player_lock {
                player
                    .send_command(crate::audio::AudioCommand::RegisterPreloadBuffers(Arc::new(preload_buffers)))
                    .map_err(|e| format!("Failed to register preload buffers: {}", e))?;
                // Altijd sturen (ook leeg): vervangt de sets van het vorige orgel.
                let _ = player.send_command(crate::audio::AudioCommand::RegisterPercussiveStops(percussive_stops));
                if trem_pipe_count > 0 {
                    info!(
                        "Tremulant-samplelaag: {} pijpen in {} registers (echte trem-opnamen)",
                        trem_pipe_count, stops_with_trem.len()
                    );
                }
                // Altijd sturen (ook leeg): vervangt de set van het vorige orgel.
                let _ = player.send_command(crate::audio::AudioCommand::RegisterTremStops(stops_with_trem.clone()));
                if !release_xfade.is_empty() {
                    info!("ReleaseCrossfadeLength: {} releases met eigen crossfade-duur", release_xfade.len());
                }
                let _ = player.send_command(crate::audio::AudioCommand::RegisterReleaseCrossfade(Arc::new(release_xfade)));
                if !odf_voicings.is_empty() {
                    info!("ODF-intonatie: {} pijpen met gain/pitch uit de sampleset", odf_voicings.len());
                }
                let _ = player.send_command(crate::audio::AudioCommand::RegisterOdfVoicings(Arc::new(odf_voicings)));
                // Altijd sturen (ook leeg): vervangt de hertemper-tabel van het
                // vorige orgel (botsende (stop_id, pipe_num)-keys!).
                let _ = player.send_command(crate::audio::AudioCommand::RegisterOdfRetune(Arc::new(odf_retune)));
                // Ook de ODF-looppunten registreren: de achtergrond-full-sample-
                // load gebruikt dan dezelfde loop als de preload (geen versprong
                // op het upgrademoment bij WAVs zonder/met afwijkende smpl-chunk).
                let _ = player.send_command(crate::audio::AudioCommand::RegisterOdfLoops(Arc::new(odf_loops.clone())));
                // Toetsduur-releases (altijd sturen, ook leeg — vervangt de map
                // van het vorige orgel): NoteOff kiest hiermee de R0/R1-variant
                // die bij de gespeelde duur hoort (MaxKeyPressTime, 0.7.26).
                if !release_meta.is_empty() {
                    info!("Toetsduur-releases: {} pijpen met meerdere release-varianten", release_meta.len());
                }
                let _ = player.send_command(crate::audio::AudioCommand::RegisterReleaseMeta(
                    release_meta.iter().cloned().collect()));
                // Laadplan per stop (altijd sturen, ook leeg — vervangt de map
                // van het vorige orgel) + live gains per perspectief-slot.
                let _ = player.send_command(crate::audio::AudioCommand::RegisterRankLayout(Arc::new(rank_layout)));
                for p in &persp_plan {
                    // Geladen maar uitgeschakeld = stil. Zo kan het aanzetten
                    // later ogenblikkelijk door alleen de gain te herstellen.
                    let g = if p.enabled { p.gain_db } else { PERSPECTIEF_STIL_DB };
                    let _ = player.send_command(crate::audio::AudioCommand::SetPerspectiveGain { slot: p.slot, gain_db: g });
                }
            }
        }
        for p in persp_plan.iter_mut() {
            p.loaded = alles_laden || p.enabled;
        }
        persp = persp_plan;
    }

    // GrandOrgue past een tremulant via de WINDCHEST toe; de [Manual]-
    // verwijzing gebruikt hij alleen voor de divisionals. Een ODF die zijn
    // tremulant alléén op de windchest noemt gaf daardoor has_tremulant=false.
    // Neem dus beide bronnen mee.
    let manual_trem_ids = |manual: &vpo_sampler::ManualDef| -> Vec<u32> {
        let mut ids: Vec<u32> = manual.tremulant_ids.clone();
        for sid in &manual.stop_ids {
            if let Some(stop) = definition.stops.iter().find(|s| s.id == *sid) {
                if let Some(wc) = definition.windchests.iter().find(|w| w.number == stop.windchest_group) {
                    for t in &wc.tremulant_ids {
                        if !ids.contains(t) {
                            ids.push(*t);
                        }
                    }
                }
            }
        }
        ids
    };

    // Build organ info for UI
    let mut divisions: Vec<DivisionDto> = Vec::new();
    let mut stop_action_code: u8 = 150; // Stops use action codes 150-255
    let mut skipped_noise = 0usize;
    let mut rank_summary: Vec<RankSummary> = Vec::new();

    for manual in &definition.manuals {
        let mut stops: Vec<StopDto> = Vec::new();

        for stop_id in &manual.stop_ids {
            if let Some(stop) = definition.stops.iter().find(|s| s.id == *stop_id) {
                // Mechaniek/ruis (klep-/registergeluid, blaasbalg, ambient) is geen
                // speelbaar register — niet als registerknop tonen.
                if stop.is_noise_or_mechanical() {
                    skipped_noise += 1;
                    continue;
                }
                rank_summary.push(rank_summary_of(stop, format!("{}_{}", manual.number, stop.id)));
                let pitch = OrganDefinition::harmonic_to_footage(stop.harmonic_number);
                let color = get_stop_color(&stop.name);

                // MIDI range. Hauptwerk sets the stop's own first note directly.
                // For GrandOrgue, a stop can start partway up the keyboard (bass/
                // discant-split stops): its first pipe sounds at the manual's first
                // key + (FirstAccessiblePipeLogicalKeyNumber - 1). Honoring that puts
                // a discant half at e.g. MIDI 60 instead of wrongly at the manual's 36.
                let first_midi = stop_first_midi(&definition, stop) as u8;
                let last_midi = first_midi.saturating_add(stop.number_of_pipes.saturating_sub(1) as u8);

                stops.push(StopDto {
                    id: format!("{}_{}", manual.number, stop.id),
                    name: clean_stop_name(&stop.name),
                    pitch,
                    drawn: false,
                    color: Some(color.to_string()),
                    // Echte tremulant-opnamen van dit register (gevuld in het
                    // laadblok). Stond hier hard op false — daardoor stuurde
                    // set_tremulant nooit een SetTremulant en was ook het
                    // Hauptwerk-"tremmed"-pad in de praktijk dood.
                    has_tremulant: stops_with_trem.contains(&stop.id),
                    midi_action_code: stop_action_code,
                    internal_stop_id: stop.id,
                    first_midi_note: first_midi,
                    last_midi_note: last_midi,
                    is_reed: is_reed_stop(&stop.name),
                });
                stop_action_code = stop_action_code.saturating_add(1);
            }
        }

        let display_name = if manual.number == 0 {
            "Pedaal".to_string()
        } else {
            format!("{} ({})", manual.name, roman_numeral(manual.number as usize))
        };

        // Tremulant van deze divisie: een ODF-verwijzing (manual óf windchest)
        // en/of registers met echte trem-opnamen.
        let trem_ids = manual_trem_ids(manual);
        let stops_have_trem = stops.iter().any(|s| s.has_tremulant);
        let trem_wave = trem_ids.iter().any(|tid| {
            definition.tremulants.iter().any(|t| t.number == *tid && t.wave)
        });
        let tremulant_kind = tremulant_kind_voor_divisie(stops_have_trem, trem_wave, !trem_ids.is_empty());
        if trem_wave && !stops_have_trem {
            warn!(
                "Manuaal '{}': TremulantType=Wave maar geen enkele trem-opname geladen → synthetische tremulant",
                manual.name
            );
        }
        divisions.push(DivisionDto {
            name: manual.name.clone(),
            display_name,
            stops,
            // ODF defines a tremulant for this manual (or its windchest), or the
            // sample set has real tremulant recordings for one of its stops.
            has_tremulant: !trem_ids.is_empty() || stops_have_trem,
            tremulant_kind: tremulant_kind.map(str::to_string),
            // Division sits in a swell box (its windchest references an Enclosure).
            has_swell: enclosed_divs.contains(&manual.name),
        });
    }

    let stop_count: usize = divisions.iter().map(|d| d.stops.len()).sum();
    if skipped_noise > 0 {
        info!("Skipped {} mechaniek/ruis-'stops' (klep/register/blaasbalg/ambient) — {} echte registers", skipped_noise, stop_count);
    }
    // Prefer the organ's real couplers (from the ODF/Hauptwerk file); only fall
    // back to generic ones when the source defines none.
    let real_couplers = build_couplers_from_definition(&definition);
    let couplers = if real_couplers.is_empty() {
        generate_couplers(&divisions)
    } else {
        info!("Using {} real couplers from the organ definition", real_couplers.len());
        real_couplers
    };

    // Naam/bouwer/plaats: JM-Rec-sets krijgen "Kerknaam - Orgelbouwer - Plaats",
    // en `location` is voortaan de PLAATS (ChurchAddress) i.p.v. RecordingDetails.
    let (display_name, display_builder, display_location) =
        display_identity(odf_path, &definition.organ);
    let organ_info = OrganInfoDto {
        id: path.to_string(),
        name: display_name,
        builder: display_builder,
        location: display_location,
        year: if definition.organ.organ_build_date.is_empty() {
            None
        } else {
            Some(definition.organ.organ_build_date.clone())
        },
        stop_count,
        divisions,
        couplers: Some(couplers),
        retune_pipes,
        retune_total,
        perspectives: perspective_dtos(&persp),
        layered_stops: rank_summary.iter().filter(|r| r.is_stacked()).count(),
        release_pipes,
    };

    // Koppels zoals de sampleset ze levert onthouden: vertrekpunt voor het
    // bij- en afschakelen van extra koppels (0.7.47).
    *state.base_couplers.write() = organ_info.couplers.clone().unwrap_or_default();
    state.extra_coupler_ids.write().clear();

    // Reset bij orgelwissel: stop alle klinkende noten en wis de getrokken registratie +
    // crescendo-stops + koppels, zodat geen geluid of registratie van het vórige orgel
    // achterblijft (anders het symptoom "geluid is actief maar de knop niet"). De
    // "laatste stand" van dít orgel wordt hierna via restore_organ_settings hersteld.
    state.send_audio_command(AudioCommand::AllNotesOff);
    state.drawn_stops.write().clear();
    // Spooktoetsen van het vorige orgel wissen: die zouden anders bij elke
    // registerwissel/crescendotrap van het nieuwe orgel stemmen starten die
    // nooit meer een NoteOff krijgen (hangers).
    state.held_notes.write().clear();
    *state.crescendo_active_stops.write() = Vec::new();
    *state.crescendo_stage.write() = 0;
    crate::state::CRESCENDO_HERSTART_VANAF_NUL.store(false, std::sync::atomic::Ordering::Relaxed);
    crate::state::zwel_nazending_leeg();
    crate::state::trede_filter_leeg();
    state.set_active_couplers(Vec::new());
    reset_organ_scoped_state(state);
    // Perspectieven + lagen-samenvatting van dít orgel (altijd zetten, ook
    // leeg — vervangt die van het vorige orgel).
    *state.perspectives.write() = persp;
    *state.rank_summary.write() = rank_summary;

    // Build stop→division map and register with audio thread
    let stop_div_map = build_stop_division_map(&organ_info.divisions);
    state.send_audio_command(AudioCommand::RegisterStopDivisionMap(stop_div_map));
    let (wind_profielen, wind_pleno) = build_stop_wind_profiles(&organ_info.divisions);
    state.send_audio_command(AudioCommand::RegisterStopWindProfiles(std::sync::Arc::new(wind_profielen), wind_pleno));
    state.reset_division_gains(organ_info.divisions.len());
    state.reset_division_settings(organ_info.divisions.len());

    // ODF-tremulantparameters als default voor de per-divisie LFO: rate uit
    // Period (ms), diepte uit AmpModDepth (%). Eventueel per orgel opgeslagen
    // gebruikersinstellingen overschrijven dit daarna via de normale restore.
    for (div_idx, manual) in definition.manuals.iter().enumerate() {
        if let Some(tid) = manual_trem_ids(manual).first() {
            if let Some(trem) = definition.tremulants.iter().find(|t| t.number == *tid) {
                if trem.wave {
                    // TremulantType=Wave: de set heeft echte tremulant-opnamen;
                    // GrandOrgue leest Period/AmpModDepth dan niet en er hoort
                    // geen synth-LFO overheen.
                    info!("ODF-tremulant '{}': golfvorm (echte opnamen) — geen synth-LFO", manual.name);
                } else if trem.period > 1.0 {
                    let rate = (1000.0 / trem.period).clamp(0.5, 12.0);
                    let amp_depth = if trem.amp_mod_depth > 0.0 {
                        trem.amp_mod_depth.clamp(1.0, 50.0)
                    } else {
                        10.0
                    };
                    info!("ODF-tremulant '{}': {:.1} Hz, amp-diepte {:.0}%", manual.name, rate, amp_depth);
                    state.send_audio_command(AudioCommand::SetTremulantLFO {
                        division_index: div_idx as u8,
                        active: false,
                        rate,
                        amp_depth,
                        pitch_depth: 15.0,
                    });
                }
            }
        }
    }

    // current_organ_id EERST zetten, dan pas de DTO publiceren: de 300ms-poll
    // van de frontend kan het nieuwe orgel anders zien terwijl get_organ_settings
    // nog de OUDE id leest — en dan instellingen van het vorige orgel toepassen
    // (auditbevinding 41).
    {
        let mut id = state.current_organ_id.write();
        *id = Some(path.to_string());
    }
    {
        let mut current = state.loaded_organ_info.write();
        *current = Some(organ_info.clone());
    }

    {
        let mut def = state.organ_definition.write();
        *def = Some(definition);
    }

    {
        // We houden geen volledig in-RAM LoadedOrgan meer aan (samples streamen nu
        // via preload-buffers + achtergrond-load). Wis een eventueel eerder geladen
        // orgel zodat zijn samples vrijkomen.
        let mut loaded = state.loaded_organ.write();
        *loaded = None;
    }

    info!("Organ loaded: {} stops in {} divisions", stop_count, organ_info.divisions.len());

    // Add to library if not already present
    add_or_refresh_library_entry(state, &organ_info, path, "organ_file");

    // current_organ_id is al vóór de DTO-publicatie gezet (auditbevinding 41).
    restore_organ_settings(state, path);
    apply_pending_registration(state, &organ_info);
    // Zwelstand terugzetten (RegisterStopDivisionMap zette de audio op open).
    apply_swell_positions(state);

    Ok(fill_live_registration_flags(state, organ_info))
}

/// Vul de drawn/active-vlaggen van een zojuist gebouwde OrganInfoDto vanuit de
/// backend-state. De DTO wordt gebouwd vóórdat restore_organ_settings en
/// apply_pending_registration draaien; zonder deze sync geeft de load-command
/// een "alles uit"-beeld terug terwijl de registratie wél hersteld is. De
/// organInfo-poll in de frontend dedupliceert op JSON-inhoud en corrigeerde dit
/// niet wanneer de herstelde stand gelijk is aan die van vóór de audio-wissel.
fn fill_live_registration_flags(state: &AppState, mut organ_info: OrganInfoDto) -> OrganInfoDto {
    {
        let drawn = state.drawn_stops.read();
        for div in organ_info.divisions.iter_mut() {
            for s in div.stops.iter_mut() {
                s.drawn = drawn.contains(&s.id);
            }
        }
    }
    let active = state.get_active_couplers();
    if let Some(ref mut couplers) = organ_info.couplers {
        for c in couplers.iter_mut() {
            c.active = active.contains(&c.id);
        }
    }
    // Ook de bewaarde DTO bijwerken zodat directe lezers (test-API /organ)
    // dezelfde vlaggen zien; get_organ_info vult ze sowieso per aanroep.
    {
        let mut current = state.loaded_organ_info.write();
        *current = Some(organ_info.clone());
    }
    organ_info
}

/// Pas na een BACKEND-geïnitieerde herlaad (noodherstel of de uitgestelde
/// ASIO-wissel, main.rs) de per-orgel DSP toe op de verse audio-thread. De
/// frontend doet dit normaal bij een load, maar merkt deze herlaad niet op —
/// de organ-poll dedupliceert op JSON en de DTO is identiek. Zonder dit bleef
/// de nieuwe audio-thread op alle defaults staan (auditbevinding 18).
pub fn apply_dsp_after_backend_reload(state: &AppState) {
    let n = apply_saved_output_channels_inner(state);
    if let Some(db) = *state.master_volume_db.read() {
        state.send_audio_command(AudioCommand::SetMasterGain(db));
    }
    if let Some(t) = state.temperament_settings.read().clone() {
        if let Some(cents) = t.custom_cents {
            state.send_audio_command(AudioCommand::SetTemperament {
                note_offsets: cents, fine_tune: t.fine_tune_cents,
                // Bestand van vóór het retune-veld = Origineel (geen klankverandering).
                retune: t.retune.unwrap_or(false),
            });
        }
    }
    if let Some(r) = state.reverb_settings.read().clone() {
        state.send_audio_command(AudioCommand::SetAlgorithmicReverb {
            preset: None, rt60: r.rt60, pre_delay_ms: r.pre_delay_ms,
            damping: r.damping, room_size: r.room_size, mix: r.mix,
        });
        let algorithmic = r.reverb_type != "convolution";
        if !algorithmic {
            if let Some(ref irp) = r.ir_path {
                let path = std::path::PathBuf::from(irp);
                if path.exists() {
                    let target_rate = state.audio_player.read().as_ref()
                        .map(|p| *p.sample_rate.read()).filter(|&x| x > 0).unwrap_or(44100);
                    if let Ok(rev) = vpo_audio::ConvolutionReverb::from_wav(&path, vpo_audio::ConvolutionReverb::aanbevolen_partitie(), target_rate) {
                        state.send_audio_command(AudioCommand::LoadImpulseResponse(Box::new(rev)));
                    }
                }
            }
        }
        state.send_audio_command(AudioCommand::SetReverbType { algorithmic });
    }
    // Per-divisie pan + zwel-config uit de mirrors (op divisienaam).
    let organ = state.loaded_organ_info.read();
    if let Some(ref o) = *organ {
        for (name, pan) in state.division_pans.read().iter() {
            if let Some(idx) = o.divisions.iter().position(|d| &d.name == name) {
                state.send_audio_command(AudioCommand::SetDivisionPan {
                    division_index: idx as u8, pan: *pan,
                });
            }
        }
        for (name, (min_db, cutoff)) in state.division_swell_configs.read().iter() {
            if let Some(idx) = o.divisions.iter().position(|d| &d.name == name) {
                state.send_audio_command(AudioCommand::SetSwellConfig {
                    division_index: idx as u8, min_db: *min_db, filter_cutoff_closed: *cutoff,
                });
            }
        }
    }
    drop(organ);
    // Laatst bekende zwelstand per divisie (de verse audio-thread staat op open).
    apply_swell_positions(state);
    info!("DSP hersteld na backend-herlaad ({} kanaal-routes)", n);
}

/// Wis bij een orgelwissel álle orgel-gebonden koppelingen en DSP-spiegels,
/// vóórdat restore_organ_settings de opgeslagen waarden van het NIEUWE orgel
/// terugzet. Zonder deze reset erfde (en persisteerde via de autosave!) een
/// orgel zonder eigen instellingen de MIDI-klavier-mappings, zwel-/preset-/
/// crescendo-koppelingen, het mastervolume en de galm van het vórige orgel —
/// de zwelpedaal stuurde dan bv. via een geërfde division_index het verkeerde
/// klavier aan (audit 2026-07-18, bevindingen 1, 2 en 23).
fn reset_organ_scoped_state(state: &AppState) {
    state.set_midi_mappings(Vec::new());
    state.set_preset_bindings_replace(Vec::new());
    // Laatste zwelstanden van het VORIGE orgel (loaded_organ_info is hier nog
    // het oude) bewaren voor een herlaad/wissel van hetzelfde orgel.
    {
        let prev_id = state.loaded_organ_info.read().as_ref().map(|o| o.id.clone());
        let stash: Vec<(String, u8)> = state.get_swell_bindings().iter()
            .filter_map(|b| b.last_value.map(|v| (b.division_name.clone(), v)))
            .collect();
        *state.swell_last_stash.write() = prev_id.filter(|_| !stash.is_empty()).map(|id| (id, stash));
    }
    state.set_swell_bindings_replace(Vec::new());
    *state.crescendo_binding.write() = None;
    // Crescendo-matrix + aan/uit zijn per orgel (stop-IDs verschillen); zonder
    // deze reset duwde een pedaalbeweging in het venster tussen wissel en
    // restore stop-IDs van het vórige orgel in drawn_stops.
    state.crescendo_stages.write().clear();
    *state.crescendo_enabled.write() = false;
    *state.crescendo_num_stages.write() = 15;
    // DSP-spiegels: None = "dit orgel heeft nog niets gezet". De frontend past
    // bij load zijn defaults toe; bewust geen audio-commando's hier (zelfde
    // afspraak als in restore_organ_settings).
    *state.master_volume_db.write() = None;
    *state.temperament_settings.write() = None;
    *state.reverb_settings.write() = None;
    *state.eq_settings.write() = None;
    // Globale C/Cis-spreiding en windgroep-aantal terug naar default (deze
    // vielen buiten reset_division_settings en lekten door).
    *state.num_wind_groups.write() = 8;
    *state.ccis_spread.write() = (0.7, 0.6, false);
    state.send_audio_command(AudioCommand::SetCcisSpread { strength: 0.7, falloff: 0.6, swap: false });
}

/// Herstel de live registratie die vlak vóór een audio-uitvoer-wissel is
/// vastgelegd (AppState::pending_registration_restore). Alleen wanneer
/// hetzélfde orgel opnieuw geladen is; een load van een ander orgel laat het
/// snapshot bewust vallen. Onafhankelijk van de "herstel laatste registratie"-
/// toggle: dit gaat om de LIVE stand van zojuist, niet om een opgeslagen stand.
fn apply_pending_registration(state: &AppState, organ_info: &OrganInfoDto) {
    let pending = state.pending_registration_restore.write().take();
    // Trap altijd verbruiken (ook bij een vroege return: een ander orgel of
    // geen snapshot betekent dat er niets te herstellen valt).
    let trap = state.pending_crescendo_stage.swap(0, std::sync::atomic::Ordering::Relaxed);
    let Some((snap_organ, stops, couplers)) = pending else { return; };
    if snap_organ.is_none() || snap_organ != *state.current_organ_id.read() {
        return;
    }
    let valid_stops: std::collections::HashSet<&str> = organ_info.divisions.iter()
        .flat_map(|d| d.stops.iter().map(|s| s.id.as_str()))
        .collect();
    let restored: Vec<String> = stops.into_iter()
        .filter(|s| valid_stops.contains(s.as_str()))
        .collect();
    let valid_couplers: std::collections::HashSet<&str> = organ_info.couplers.as_ref()
        .map(|cl| cl.iter().map(|c| c.id.as_str()).collect())
        .unwrap_or_default();
    let restored_couplers: Vec<String> = couplers.into_iter()
        .filter(|c| valid_couplers.contains(c.as_str()))
        .collect();
    if !restored.is_empty() || !restored_couplers.is_empty() {
        info!(
            "Registratie hersteld na audio-wissel: {} registers, {} koppels",
            restored.len(), restored_couplers.len()
        );
        if !restored.is_empty() {
            *state.drawn_stops.write() = restored;
        }
        if !restored_couplers.is_empty() {
            state.set_active_couplers(restored_couplers);
        }
    }
    // Crescendotrap van vóór de wissel opnieuw toepassen: het snapshot bevatte
    // de trede-registers bewust niet (zie registratie_snapshot_zonder_crescendo),
    // zodat ze hier weer als claims terugkomen en straks gewoon terugveren.
    // Alleen als het crescendo (na het herstel van de instellingen) aan staat;
    // uitgeschakeld hoort de trap op 0 zonder claims.
    if trap > 0 && *state.crescendo_enabled.read() {
        *state.crescendo_stage.write() = 0;
        state.crescendo_active_stops.write().clear();
        let (bij, _) = state.apply_crescendo_stage(trap);
        info!("Crescendotrap {} hersteld na audio-wissel ({} trede-registers)", trap, bij.len());
    }
}

/// Gecachte orgel-DTO met actuele drawn-/koppelvlaggen (gedeeld door het
/// Tauri-commando get_organ_info en GET /organ van test-API/afstandsbediening).
pub fn organ_info_merged(state: &AppState) -> Option<OrganInfoDto> {
    let organ = state.loaded_organ_info.read();
    match organ.clone() {
        Some(mut o) => {
            // Fill coupler active states from backend
            let active = state.get_active_couplers();
            if let Some(ref mut couplers) = o.couplers {
                for c in couplers.iter_mut() {
                    c.active = active.contains(&c.id);
                }
            }
            // Fill stop drawn states from the audio-authoritative drawn set, so the
            // UI's engaged display always matches what is actually sounding. (The
            // cached DTO can otherwise drift out of sync — e.g. its drawn flags are
            // rebuilt to false on a reload while the drawn set / audio keep playing.)
            {
                let drawn = state.drawn_stops.read();
                for div in o.divisions.iter_mut() {
                    for s in div.stops.iter_mut() {
                        s.drawn = drawn.contains(&s.id);
                    }
                }
            }
            Some(o)
        }
        None => None,
    }
}

#[tauri::command]
pub fn get_organ_info(state: State<AppState>) -> Result<Option<OrganInfoDto>, String> {
    Ok(organ_info_merged(&state))
}

#[tauri::command]
pub fn set_stop(state: State<AppState>, stop_id: String, active: bool) -> Result<(), String> {
    let changed = {
        let mut organ = state.loaded_organ_info.write();
        let Some(ref mut o) = *organ else {
            return Err(format!("Stop not found: {}", stop_id));
        };
        let mut found = false;
        let mut changed = false;
        for division in &mut o.divisions {
            if let Some(stop) = division.stops.iter_mut().find(|s| s.id == stop_id) {
                changed = stop.drawn != active;
                stop.drawn = active;
                found = true;
                break;
            }
        }
        if !found {
            return Err(format!("Stop not found: {}", stop_id));
        }
        changed
    };

    state.set_stop_drawn(&stop_id, active);
    if changed {
        // Direct hoorbaar tijdens het spelen (zie toggle_stop).
        state.sync_stop_voices(&stop_id, active);
    }
    info!("Stop {} set to {}", stop_id, active);
    Ok(())
}

/// Register omschakelen (kern van het Tauri-commando toggle_stop; ook gebruikt
/// door de test-API en de afstandsbediening). Werkt de DTO-vlag, de drawn-set
/// en de klinkende stemmen bij.
pub fn toggle_stop_inner(state: &AppState, stop_id: &str) -> Result<bool, String> {
    let drawn = {
        let mut organ = state.loaded_organ_info.write();
        let Some(ref mut o) = *organ else {
            return Err(format!("Stop not found: {}", stop_id));
        };
        let mut found = None;
        for division in &mut o.divisions {
            if let Some(stop) = division.stops.iter_mut().find(|s| s.id == stop_id) {
                stop.drawn = !stop.drawn;
                found = Some(stop.drawn);
                break;
            }
        }
        match found {
            Some(d) => d,
            None => return Err(format!("Stop not found: {}", stop_id)),
        }
    }; // organ-write-lock vrijgeven vóór sync (die neemt zelf read-locks)

    state.set_stop_drawn(stop_id, drawn);
    // Handmatig omgezet → geen trede-claim meer (zie note_manual_change).
    state.note_manual_change(stop_id);
    // Direct hoorbaar tijdens het spelen: AAN speelt ingedrukte toetsen (incl.
    // koppels) meteen door dit register, UIT laat alle pijpen direct los.
    state.sync_stop_voices(stop_id, drawn);
    info!("Stop {} → {}", stop_id, if drawn { "AAN" } else { "UIT" });
    Ok(drawn)
}

#[tauri::command]
pub fn toggle_stop(state: State<AppState>, stop_id: String) -> Result<bool, String> {
    toggle_stop_inner(&state, &stop_id)
}

/// Set all drawn stops at once (for setzer/preset recall)
/// Stops in the list become drawn=true, all others become drawn=false
#[tauri::command]
pub fn set_drawn_stops(state: State<AppState>, stop_ids: Vec<String>) -> Result<(), String> {
    set_drawn_stops_inner(&state, stop_ids)
}

/// Kern van set_drawn_stops (setzer-oproep / General Cancel), ook voor de
/// test-API (POST /stops/set).
pub fn set_drawn_stops_inner(state: &AppState, stop_ids: Vec<String>) -> Result<(), String> {
    // Verzamel de wijzigingen onder de write-lock, synchroniseer daarna de
    // klinkende voices per gewijzigd register: een preset-oproep of setzer
    // tijdens het spelen moet direct hoorbaar zijn (bijgetrokken registers
    // spelen de ingedrukte toetsen mee, weggetrokken registers zwijgen).
    // De diff kijkt naar BEIDE bronnen: de UI-spiegel (organ_info.drawn) én de
    // échte routeringsbron (drawn_stops). Die twee kunnen uiteenlopen wanneer
    // de crescendotrede (MIDI-thread) en dit command elkaar kruisen; diffen op
    // alleen de spiegel miste dan een ReleaseStop → register bleef doorklinken
    // zolang de toets lag. Voices alleen (her)starten als de échte bron
    // wijzigde, anders verdubbelt een al-klinkend register zijn stemmen.
    let currently: std::collections::HashSet<String> =
        state.drawn_stops.read().iter().cloned().collect();
    let mut turned_on: Vec<(String, bool)> = Vec::new(); // (id, voices starten?)
    let mut turned_off: Vec<String> = Vec::new();
    {
        let mut organ = state.loaded_organ_info.write();
        let Some(ref mut o) = *organ else {
            return Err("Geen orgel geladen".into());
        };
        for division in &mut o.divisions {
            for stop in &mut division.stops {
                let should_draw = stop_ids.contains(&stop.id);
                let flag_changed = stop.drawn != should_draw;
                let truth_changed = currently.contains(&stop.id) != should_draw;
                if flag_changed {
                    stop.drawn = should_draw;
                }
                if flag_changed || truth_changed {
                    if should_draw {
                        turned_on.push((stop.id.clone(), truth_changed));
                    } else {
                        turned_off.push(stop.id.clone());
                    }
                }
            }
        }
    }
    for id in &turned_off {
        state.set_stop_drawn(id, false);
        state.note_manual_change(id);
        // ReleaseStop is onschuldig als het register al stil was.
        state.sync_stop_voices(id, false);
    }
    for (id, start_voices) in &turned_on {
        state.set_stop_drawn(id, true);
        state.note_manual_change(id);
        if *start_voices {
            state.sync_stop_voices(id, true);
        }
    }
    // Alles wat de preset expliciet bevat is handregistratie — ook registers
    // die de trede al had geclaimd (anders trekt terugveren een preset-register
    // weg).
    for id in &stop_ids {
        state.note_manual_change(id);
    }
    // Zijn daarmee álle trede-claims handregistratie geworden terwijl de trap
    // nog op N stond, dan begint het crescendo opnieuw vanaf de bodem (0.7.44):
    // anders trok het terugnemen van de trede de registers van de lagere trap
    // bíj in plaats van weg. (Ook aan het eind van set_active_couplers: een
    // koppel-claim verdwijnt pas daar.)
    state.crescendo_herstart_indien_claims_leeg();
    info!("set_drawn_stops: {} aan, {} uit (van {} gevraagd)",
          turned_on.len(), turned_off.len(), stop_ids.len());
    Ok(())
}

/// Play a note for a specific stop
#[tauri::command]
pub fn play_note(state: State<AppState>, stop_id: String, note: u8, velocity: f32) -> Result<(), String> {
    // Parse stop_id (format: "manual_stopid")
    let parts: Vec<&str> = stop_id.split('_').collect();
    if parts.len() != 2 {
        return Err(format!("Invalid stop_id format: {}", stop_id));
    }

    let internal_stop_id: u32 = parts[1].parse()
        .map_err(|_| format!("Invalid stop ID: {}", parts[1]))?;

    let definition = state.organ_definition.read();

    // Get manual's first MIDI note
    let manual_first_midi = definition.as_ref()
        .and_then(|def| def.manuals.iter()
            .find(|m| m.stop_ids.contains(&internal_stop_id))
            .map(|m| m.first_accessible_key_midi_note))
        .unwrap_or(36);

    // Calculate pipe number (1-based)
    let pipe_num = if note as u32 >= manual_first_midi {
        (note as u32) - manual_first_midi + 1
    } else {
        return Ok(()); // Note below range
    };

    info!("play_note: stop={}, note={}, manual_first_midi={}, pipe_num={}", internal_stop_id, note, manual_first_midi, pipe_num);

    state.send_audio_command(AudioCommand::NoteOn {
        stop_id: internal_stop_id,
        pipe_num,
        midi_note: note,
        velocity,
    });

    Ok(())
}

/// Stop a note for a specific stop
#[tauri::command]
pub fn stop_note(state: State<AppState>, stop_id: String, note: u8) -> Result<(), String> {
    let parts: Vec<&str> = stop_id.split('_').collect();
    if parts.len() != 2 {
        return Err(format!("Invalid stop_id format: {}", stop_id));
    }

    let internal_stop_id: u32 = parts[1].parse()
        .map_err(|_| format!("Invalid stop ID: {}", parts[1]))?;

    let definition = state.organ_definition.read();

    // Get manual's first MIDI note
    let manual_first_midi = definition.as_ref()
        .and_then(|def| def.manuals.iter()
            .find(|m| m.stop_ids.contains(&internal_stop_id))
            .map(|m| m.first_accessible_key_midi_note))
        .unwrap_or(36);

    // Calculate pipe number (1-based)
    if note as u32 >= manual_first_midi {
        let pipe_num = (note as u32) - manual_first_midi + 1;

        state.send_audio_command(AudioCommand::NoteOff {
            stop_id: internal_stop_id,
            pipe_num,
        });
    }

    Ok(())
}

/// Play a note for all drawn stops
#[tauri::command]
pub fn play_note_all_stops(state: State<AppState>, note: u8, velocity: f32) -> Result<(), String> {
    let organ = state.loaded_organ_info.read();
    let definition = state.organ_definition.read();
    let drawn_stops = state.drawn_stops.read();

    info!("play_note_all_stops: note={}, velocity={}, drawn_stops={:?}", note, velocity, *drawn_stops);

    if let (Some(ref o), Some(ref def)) = (&*organ, &*definition) {
        for division in &o.divisions {
            for stop in &division.stops {
                if drawn_stops.contains(&stop.id) {
                    // Get stop definition for pipe range info
                    let stop_def = def.get_stop(stop.internal_stop_id);

                    // Get manual's first MIDI note (usually 36 for manuals, lower for pedals)
                    let manual_first_midi = def.manuals.iter()
                        .find(|m| m.stop_ids.contains(&stop.internal_stop_id))
                        .map(|m| m.first_accessible_key_midi_note)
                        .unwrap_or(36);

                    // Calculate pipe number (1-based): pipe 1 = first MIDI note of the manual
                    let pipe_num = if note as u32 >= manual_first_midi {
                        (note as u32) - manual_first_midi + 1
                    } else {
                        continue; // Note is below this stop's range
                    };

                    // Check if pipe exists
                    let num_pipes = stop_def.map(|s| s.number_of_pipes).unwrap_or(61);
                    if pipe_num > num_pipes {
                        continue; // Note is above this stop's range
                    }

                    info!("Triggering stop {} (internal_id={}), note={}, manual_first_midi={}, pipe_num={}",
                          stop.id, stop.internal_stop_id, note, manual_first_midi, pipe_num);

                    state.send_audio_command(AudioCommand::NoteOn {
                        stop_id: stop.internal_stop_id,
                        pipe_num,
                        midi_note: note,
                        velocity,
                    });
                }
            }
        }
    }

    Ok(())
}

/// Stop a note for all drawn stops
#[tauri::command]
pub fn stop_note_all_stops(state: State<AppState>, note: u8) -> Result<(), String> {
    let organ = state.loaded_organ_info.read();
    let definition = state.organ_definition.read();
    let drawn_stops = state.drawn_stops.read();

    if let (Some(ref o), Some(ref def)) = (&*organ, &*definition) {
        for division in &o.divisions {
            for stop in &division.stops {
                if drawn_stops.contains(&stop.id) {
                    // Get manual's first MIDI note
                    let manual_first_midi = def.manuals.iter()
                        .find(|m| m.stop_ids.contains(&stop.internal_stop_id))
                        .map(|m| m.first_accessible_key_midi_note)
                        .unwrap_or(36);

                    // Calculate pipe number (1-based)
                    if note as u32 >= manual_first_midi {
                        let pipe_num = (note as u32) - manual_first_midi + 1;

                        state.send_audio_command(AudioCommand::NoteOff {
                            stop_id: stop.internal_stop_id,
                            pipe_num,
                        });
                    }
                }
            }
        }
    }

    Ok(())
}

/// Hoofdvolume zetten (kern van set_master_volume; ook voor de afstandsbediening).
pub fn set_master_volume_inner(state: &AppState, db: f32) {
    state.send_audio_command(AudioCommand::SetMasterGain(db));
    // Onthoud voor per-orgel opslag (save_current_organ_settings leest dit veld).
    *state.master_volume_db.write() = Some(db);
}

#[tauri::command]
pub fn set_master_volume(state: State<AppState>, db: f32) -> Result<(), String> {
    set_master_volume_inner(&state, db);
    Ok(())
}

#[tauri::command]
pub fn set_reverb_mix(state: State<AppState>, mix: f32) -> Result<(), String> {
    state.send_audio_command(AudioCommand::SetReverbMix(mix.clamp(0.0, 1.0)));
    Ok(())
}

/// Configure algorithmic FDN reverb
#[tauri::command]
pub fn set_algorithmic_reverb(state: State<AppState>, preset: Option<u8>, rt60: f32, pre_delay_ms: f32, damping: f32, room_size: f32, mix: f32) -> Result<(), String> {
    state.send_audio_command(AudioCommand::SetAlgorithmicReverb { preset, rt60, pre_delay_ms, damping, room_size, mix });
    Ok(())
}

/// Crescendo-matrix + aan/uit (+ optioneel kolomaantal) zetten. De backend is
/// de bron van waarheid (per orgel opgeslagen); de huidige trap wordt direct
/// opnieuw toegepast zodat een editor-wijziging live hoorbaar is, een krimp
/// van de matrix de trap clampt en uitschakelen de trede-registers wegtrekt.
#[tauri::command]
pub fn set_crescendo_config(state: State<AppState>, stages: Vec<Vec<String>>, enabled: bool, num_stages: Option<u8>) -> Result<(), String> {
    set_crescendo_config_inner(&state, stages, enabled, num_stages);
    Ok(())
}

pub fn set_crescendo_config_inner(state: &AppState, mut stages: Vec<Vec<String>>, enabled: bool, num_stages: Option<u8>) {
    if let Some(n) = num_stages {
        *state.crescendo_num_stages.write() = n.max(1);
    } else if !stages.is_empty() {
        // Zonder expliciet aantal (test-API, oudere aanroepers) bepaalt de
        // matrix het aantal trappen.
        *state.crescendo_num_stages.write() = stages.len().clamp(1, 255) as u8;
    }
    // Matrix tot num_stages padden met lege trappen (een lege trap erft de
    // dichtstbijzijnde lagere), zodat pedaal (len()) en balk/editor
    // (num_stages) hetzelfde bereik hebben.
    let n = *state.crescendo_num_stages.read() as usize;
    if stages.len() < n {
        stages.resize(n, Vec::new());
    }
    *state.crescendo_stages.write() = stages;
    set_crescendo_enabled_inner(state, enabled);
}

/// Alleen aan/uit (piston 41, checkbox, paneel) — zonder de (mogelijk
/// verouderde) matrix van dat venster mee te sturen.
#[tauri::command]
pub fn set_crescendo_enabled(state: State<AppState>, enabled: bool) -> Result<(), String> {
    set_crescendo_enabled_inner(&state, enabled);
    Ok(())
}

/// Kern van aan/uit: uit → trap 0 (trede-registers weg, claims leeg);
/// aan → de huidige trap (geclamped) opnieuw toepassen.
pub fn set_crescendo_enabled_inner(state: &AppState, enabled: bool) {
    let was = *state.crescendo_enabled.read();
    *state.crescendo_enabled.write() = enabled;
    if !enabled {
        if was || !state.crescendo_active_stops.read().is_empty() || *state.crescendo_stage.read() != 0 {
            state.apply_crescendo_stage(0);
        }
    } else {
        let stage = *state.crescendo_stage.read();
        state.apply_crescendo_stage(stage);
    }
}

/// Trap zetten vanuit de UI (klik op een trap/kolomkop) of de test-API:
/// 0 = uit, 1..N. Zelfde kern als het pedaal; de aanroeper ververst daarna
/// alleen get_organ_info.
#[tauri::command]
pub fn set_crescendo_stage(state: State<AppState>, stage: u8) -> Result<(), String> {
    set_crescendo_stage_inner(&state, stage)
}

pub fn set_crescendo_stage_inner(state: &AppState, stage: u8) -> Result<(), String> {
    if !*state.crescendo_enabled.read() {
        return Err("Crescendo staat uit".to_string());
    }
    if state.crescendo_stages.read().is_empty() {
        return Err("Geen crescendo-stappen ingesteld".to_string());
    }
    // Een bewuste trapkeuze (UI/piston) heft de "eerst terug naar 0"-stand
    // na een setzer op.
    crate::state::CRESCENDO_HERSTART_VANAF_NUL.store(false, std::sync::atomic::Ordering::Relaxed);
    state.apply_crescendo_stage(stage);
    Ok(())
}

/// Crescendo-config voor de frontend (bron van waarheid: backend).
#[derive(Debug, Clone, Serialize)]
pub struct CrescendoConfigDto {
    pub enabled: bool,
    pub stage: u8,
    pub stages: Vec<Vec<String>>,
    pub num_stages: u8,
    /// Registers/koppels die de trede nu bijgetrokken heeft (claims).
    pub active_stops: Vec<String>,
    /// Pedaalbinding (channel, cc, min, max, invert) — zodat andere vensters
    /// een elders ingeleerde trede zien zonder extra IPC.
    pub binding: Option<(u8, u8, u8, u8, bool)>,
}

/// Get current crescendo config
#[tauri::command]
pub fn get_crescendo_config(state: State<AppState>) -> CrescendoConfigDto {
    crescendo_config_dto(&state)
}

pub fn crescendo_config_dto(state: &AppState) -> CrescendoConfigDto {
    CrescendoConfigDto {
        enabled: *state.crescendo_enabled.read(),
        stage: *state.crescendo_stage.read(),
        stages: state.crescendo_stages.read().clone(),
        num_stages: *state.crescendo_num_stages.read(),
        active_stops: state.crescendo_active_stops.read().clone(),
        binding: *state.crescendo_binding.read(),
    }
}

/// Registers/koppels die de crescendotrede nu geclaimd heeft — de setzer-SET
/// trekt ze af zodat een preset geen trede-registers bevat.
#[tauri::command]
pub fn get_crescendo_claims(state: State<AppState>) -> Vec<String> {
    state.crescendo_active_stops.read().clone()
}

/// Lopende trede-inleer afbreken (Annuleren/Opnieuw in de popup). Async:
/// de blokkerende learn_pedal_position houdt de main-thread bezet, dus dit
/// commando moet op een eigen taak lopen om überhaupt aan te komen.
#[tauri::command]
pub async fn cancel_pedal_learn(state: State<'_, AppState>) -> Result<(), String> {
    state.learn_cancel.store(true, std::sync::atomic::Ordering::Relaxed);
    state.midi_tx.send(crate::state::MidiCommand::CancelLearn).map_err(|e| e.to_string())?;
    Ok(())
}

/// Lightweight poll for crescendo live state (enabled, current stage, total stages)
#[tauri::command]
pub fn get_crescendo_state(state: State<AppState>) -> (bool, u8, u8) {
    let enabled = *state.crescendo_enabled.read();
    let stage = *state.crescendo_stage.read();
    let total = state.crescendo_stages.read().len() as u8;
    (enabled, stage, total)
}

/// Learn crescendo pedal MIDI binding.
/// Stille leer-modus: tijdens het luisteren staat de bestaande crescendo-binding
/// even uit en stoppen we alle klinkende noten + crescendo-stops. Anders zou de
/// bewegende pedaal tijdens het leren een al-gebonden crescendo activeren
/// (registers worden gestoken/teruggetrokken), wat het settle-detectie-algoritme
/// verstoort. De oude binding wordt PAS vervangen bij een geslaagde inleer: bij
/// een time-out of afbreken komt hij ongeschonden terug (voorheen was je hem
/// dan kwijt).
#[tauri::command]
pub fn learn_crescendo_pedal(state: State<AppState>) -> Result<Option<(u8, u8)>, String> {
    use crate::state::PedalCcKind;
    // Stille leer-modus: trede-registers netjes wegtrekken (zelfde kern als
    // het pedaal, incl. voice-sync en DTO-vlaggen) en de binding parkeren.
    let vorige = state.crescendo_binding.write().take();
    state.apply_crescendo_stage(0);
    state.send_audio_command(AudioCommand::AllNotesOff);

    let (tx, rx) = crossbeam_channel::bounded(1);
    state.midi_tx.send(crate::state::MidiCommand::LearnCrescendoPedal(tx))
        .map_err(|e| {
            *state.crescendo_binding.write() = vorige;
            e.to_string()
        })?;
    match rx.recv_timeout(std::time::Duration::from_secs(20)) {
        Ok(result) => {
            if let Some((ch, cc)) = result {
                // Behoud bestaande invert-keuze als die er was; nieuwe binding
                // (channel, cc, min=0, max=127, invert=false)
                *state.crescendo_binding.write() = Some((ch, cc, 0, 127, false));
                // Wederzijds exclusief via de gedeelde helper: een zwel-koppeling
                // op dezelfde CC zou door de crescendo-claim toch dood zijn.
                let _ = state.claim_pedal_cc(PedalCcKind::Crescendo, ch, cc);
                // Tijdens het inleren was de binding gewist, dus een CC7/11-pedaal
                // heeft dan via de expressie-fallback het mastervolume gedempt.
                // Nu de CC exclusief crescendo is komt er nooit meer een
                // expressie-update op deze CC — zet de demping expliciet terug.
                state.send_audio_command(AudioCommand::SetMasterExpression(1.0));
            } else {
                // Time-out in de MIDI-thread: oude koppeling terug.
                *state.crescendo_binding.write() = vorige;
            }
            Ok(result)
        }
        Err(_) => {
            *state.crescendo_binding.write() = vorige;
            Ok(None)
        }
    }
}

/// Clear crescendo pedal binding
#[tauri::command]
pub fn clear_crescendo_binding(state: State<AppState>) {
    // Eerst trap 0 (claims los, trede-registers weg), dan de koppeling weg:
    // anders bleven de door de trede getrokken registers als "handmatig"
    // staan, zonder trede om ze nog terug te nemen (0.7.44).
    let _ = state.apply_crescendo_stage(0);
    *state.crescendo_binding.write() = None;
}

/// Start MIDI recording
#[tauri::command]
pub fn start_midi_recording(state: State<AppState>) {
    let mut rec = state.midi_recording.write();
    *rec = Some(crate::state::MidiRecording {
        events: Vec::new(),
        start_time: std::time::Instant::now(),
        active: true,
        stopped_elapsed: None,
    });
    info!("MIDI recording started");
}

/// Status van de huidige MIDI-opname — voor de UI-poll (live teller events + tijd).
#[derive(serde::Serialize)]
pub struct MidiRecordingStatus {
    pub recording: bool,
    pub event_count: usize,
    pub seconds: f64,
}

/// Geef de live status van de MIDI-opname terug (loopt er een opname, hoeveel events, hoe lang).
#[tauri::command]
pub fn midi_recording_status(state: State<AppState>) -> MidiRecordingStatus {
    let rec = state.midi_recording.read();
    match rec.as_ref() {
        Some(r) => MidiRecordingStatus {
            recording: r.active,
            event_count: r.events.len(),
            // Na stop: bevroren duur i.p.v. eeuwig doorlopende wandkloktijd.
            seconds: r.stopped_elapsed.unwrap_or_else(|| r.start_time.elapsed().as_secs_f64()),
        },
        None => MidiRecordingStatus { recording: false, event_count: 0, seconds: 0.0 },
    }
}

/// Stop MIDI recording
#[tauri::command]
pub fn stop_midi_recording(state: State<AppState>) -> usize {
    // Zet de capture echt uit (anders belanden noten die tijdens de opslag-
    // dialoog gespeeld worden alsnog in het .mid-bestand); de events blijven
    // beschikbaar voor save/afspelen tot clear of een nieuwe opname.
    let mut rec = state.midi_recording.write();
    let count = match rec.as_mut() {
        Some(r) => {
            if r.active {
                r.active = false;
                r.stopped_elapsed = Some(r.start_time.elapsed().as_secs_f64());
            }
            r.events.len()
        }
        None => 0,
    };
    info!("MIDI recording stopped, {} events captured", count);
    count
}

/// Stel een standaard-opslagpad voor een MIDI-opname voor: tijdgestempeld .mid in
/// Documenten/JM-Orgue-opnames (zelfde map als de MP3-opnames). Maakt de map alvast aan
/// zodat de opslagdialoog er direct naartoe kan navigeren.
#[tauri::command]
pub fn suggest_midi_recording_path() -> String {
    let dir = recordings_default_dir();
    let _ = std::fs::create_dir_all(&dir);
    make_timestamped_recording_path("mid").to_string_lossy().to_string()
}

/// Codeer opgenomen events naar een Standard MIDI File (formaat 0, 480 PPQ, vast 120 BPM).
/// `events` = (timestamp_us, status, data1, data2, channel) zoals vastgelegd in de MIDI-thread.
/// Byte-identiek aan `encode_smf_with_name(events, None)`.
pub fn encode_smf(events: &[(u64, u8, u8, u8, u8)]) -> Vec<u8> {
    encode_smf_with_name(events, None)
}

/// Als `encode_smf`, optioneel met een Track-Name-meta (FF 03) direct na de
/// tempo-meta — het automatische MIDI-archief zet daar de orgelnaam in. midly
/// slaat dit meta-event bij het afspelen/noteren gewoon over.
pub fn encode_smf_with_name(events: &[(u64, u8, u8, u8, u8)], name: Option<&str>) -> Vec<u8> {
    let mut data: Vec<u8> = Vec::new();

    // Header chunk: MThd
    data.extend_from_slice(b"MThd");
    data.extend_from_slice(&6u32.to_be_bytes()); // chunk length
    data.extend_from_slice(&0u16.to_be_bytes()); // format 0
    data.extend_from_slice(&1u16.to_be_bytes()); // 1 track
    data.extend_from_slice(&480u16.to_be_bytes()); // 480 ticks per quarter note

    // Track chunk
    let mut track_data: Vec<u8> = Vec::new();

    // Tempo: 120 BPM = 500000 us/quarter
    track_data.extend_from_slice(&[0x00, 0xFF, 0x51, 0x03]);
    track_data.extend_from_slice(&500000u32.to_be_bytes()[1..4]);

    // Optionele Track-Name-meta (FF 03). Afkappen op TEKENS, niet op bytes,
    // zodat een UTF-8-teken ('Bätz') nooit gesplitst wordt.
    if let Some(n) = name {
        let n: String = n.chars().take(120).collect();
        let b = n.as_bytes();
        if !b.is_empty() {
            track_data.extend_from_slice(&[0x00, 0xFF, 0x03]);
            write_var_len(&mut track_data, b.len() as u32);
            track_data.extend_from_slice(b);
        }
    }

    let mut prev_tick: u32 = 0;
    for &(timestamp_us, status, data1, data2, _ch) in events {
        // Convert microseconds to ticks (at 120 BPM, 480 ticks/quarter = 960 ticks/ms)
        let tick = (timestamp_us as f64 / 1_000_000.0 * 120.0 / 60.0 * 480.0) as u32;
        let delta = tick.saturating_sub(prev_tick);
        prev_tick = tick;

        // Write variable-length delta time
        write_var_len(&mut track_data, delta);
        // Write MIDI event
        track_data.push(status);
        track_data.push(data1);
        if status & 0xF0 != 0xC0 && status & 0xF0 != 0xD0 {
            track_data.push(data2);
        }
    }

    // End of track
    track_data.extend_from_slice(&[0x00, 0xFF, 0x2F, 0x00]);

    // Track header
    data.extend_from_slice(b"MTrk");
    data.extend_from_slice(&(track_data.len() as u32).to_be_bytes());
    data.extend_from_slice(&track_data);

    data
}

/// Save MIDI recording as SMF (Standard MIDI File)
#[tauri::command]
pub fn save_midi_recording(state: State<AppState>, path: String) -> Result<(), String> {
    let rec = state.midi_recording.read();
    let recording = rec.as_ref().ok_or("Geen opname actief")?;

    if recording.events.is_empty() {
        return Err("Geen events opgenomen".into());
    }

    let data = encode_smf(&recording.events);

    // Zorg dat de doelmap bestaat (bv. JM-Orgue-opnames bij eerste gebruik).
    if let Some(parent) = std::path::Path::new(&path).parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::write(&path, &data)
        .map_err(|e| format!("Kan niet schrijven: {}", e))?;

    info!("MIDI recording saved to {} ({} bytes)", path, data.len());
    Ok(())
}

/// Clear MIDI recording
#[tauri::command]
pub fn clear_midi_recording(state: State<AppState>) {
    *state.midi_recording.write() = None;
}

// ============ Automatisch MIDI-archief ============
// Zie midi_archive.rs. De instellingen staan in <app_data_dir>/midi_archive.json;
// de live status komt uit de atomics in state.midi_archive. Gedeelde logica
// (config-DTO opbouwen/toepassen) wordt ook door de test-API gebruikt.

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MidiArchiveConfigDto {
    pub enabled: bool,
    /// Feitelijke archiefmap (eigen keuze of standaard).
    pub dir: String,
    /// true = er is geen eigen map gekozen (standaardmap in gebruik).
    pub dir_is_default: bool,
    pub silence_secs: u32,
    pub min_notes: u32,
    pub min_secs: u32,
}

#[derive(Debug, Serialize, Clone)]
pub struct MidiArchiveStatusDto {
    pub enabled: bool,
    pub archiving: bool,
    pub event_count: u32,
    /// Verstreken tijd van de lopende take (0 als er geen take loopt).
    pub seconds: f64,
    pub files_written: u32,
    pub last_file: Option<String>,
    pub last_error: Option<String>,
}

/// Huidige archief-instellingen als DTO (uit midi_archive.json).
pub(crate) fn midi_archive_config_dto(state: &AppState) -> MidiArchiveConfigDto {
    let prefs = crate::midi_archive::load_prefs(&state.app_data_dir);
    MidiArchiveConfigDto {
        enabled: prefs.enabled,
        dir: prefs.effective_dir().to_string_lossy().to_string(),
        dir_is_default: prefs.dir.is_none(),
        silence_secs: prefs.silence_secs,
        min_notes: prefs.min_notes,
        min_secs: prefs.min_secs,
    }
}

/// Nieuwe archief-instellingen toepassen: begrenzen, map aanmaken, bewaren en
/// naar de MIDI-thread doorzetten. Uitzetten terwijl een take loopt: de
/// MIDI-thread sluit die zelf af via ArchiveRecorder::poll (enabled=false).
pub(crate) fn apply_midi_archive_config(state: &AppState, cfg: MidiArchiveConfigDto) -> Result<MidiArchiveConfigDto, String> {
    let dir = cfg.dir.trim();
    let prefs = crate::midi_archive::MidiArchivePrefs {
        enabled: cfg.enabled,
        dir: if dir.is_empty() { None } else { Some(dir.to_string()) },
        silence_secs: cfg.silence_secs,
        min_notes: cfg.min_notes,
        min_secs: cfg.min_secs,
    }.clamped();
    std::fs::create_dir_all(prefs.effective_dir())
        .map_err(|e| format!("Archiefmap niet aan te maken: {}", e))?;
    crate::midi_archive::save_prefs(&state.app_data_dir, &prefs);
    state.midi_archive.apply(&prefs);
    info!(
        "MIDI-archief: enabled={} dir={:?} stilte={}s min_noten={} min_lengte={}s",
        prefs.enabled, prefs.effective_dir(), prefs.silence_secs, prefs.min_notes, prefs.min_secs
    );
    Ok(midi_archive_config_dto(state))
}

/// Live status van het archief als DTO (ook door de test-API gebruikt).
pub(crate) fn midi_archive_status_dto(state: &AppState) -> MidiArchiveStatusDto {
    use std::sync::atomic::Ordering;
    let sh = &state.midi_archive;
    let archiving = sh.archiving.load(Ordering::Relaxed);
    let seconds = if archiving {
        let started = sh.take_started_epoch_ms.load(Ordering::Relaxed);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        now.saturating_sub(started) as f64 / 1000.0
    } else {
        0.0
    };
    MidiArchiveStatusDto {
        enabled: sh.enabled.load(Ordering::Relaxed),
        archiving,
        event_count: sh.event_count.load(Ordering::Relaxed),
        seconds,
        files_written: sh.files_written.load(Ordering::Relaxed),
        last_file: sh.last_file.read().as_ref().map(|p| p.to_string_lossy().to_string()),
        last_error: sh.last_error.read().clone(),
    }
}

#[tauri::command]
pub fn get_midi_archive_config(state: State<AppState>) -> MidiArchiveConfigDto {
    midi_archive_config_dto(&state)
}

/// JS: invoke('set_midi_archive_config', { config })
#[tauri::command]
pub fn set_midi_archive_config(state: State<AppState>, config: MidiArchiveConfigDto) -> Result<MidiArchiveConfigDto, String> {
    apply_midi_archive_config(&state, config)
}

#[tauri::command]
pub fn midi_archive_status(state: State<AppState>) -> MidiArchiveStatusDto {
    midi_archive_status_dto(&state)
}

/// De 10 nieuwste archiefbestanden (nieuwste eerst).
#[tauri::command]
pub fn midi_archive_list(state: State<AppState>) -> Vec<crate::midi_archive::ArchiveFileInfo> {
    let dir = state.midi_archive.dir.read().clone();
    crate::midi_archive::list_recent(&dir, 10)
}

/// Verwijder een archiefbestand — alleen .mid-bestanden direct in de archiefmap.
#[tauri::command]
pub fn midi_archive_delete(state: State<AppState>, path: String) -> Result<(), String> {
    let dir = state.midi_archive.dir.read().clone();
    let file = std::path::Path::new(&path);
    if !crate::midi_archive::is_inside_archive_dir(&dir, file) {
        return Err("Alleen bestanden in de archiefmap kunnen verwijderd worden".into());
    }
    std::fs::remove_file(file).map_err(|e| format!("Verwijderen mislukt: {}", e))?;
    let mut last = state.midi_archive.last_file.write();
    if last.as_ref().map(|p| p.as_path() == file).unwrap_or(false) {
        *last = None;
    }
    info!("MIDI-archief: bestand verwijderd: {}", path);
    Ok(())
}

/// Archiefmap in de Verkenner openen (zonder shell-parsing, zoals open_external_url).
#[tauri::command]
pub fn midi_archive_open_dir(state: State<AppState>) -> Result<(), String> {
    let dir = state.midi_archive.dir.read().clone();
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("Archiefmap niet aan te maken: {}", e))?;
    std::process::Command::new("explorer")
        .arg(dir.as_os_str())
        .spawn()
        .map_err(|e| format!("Verkenner openen mislukt: {}", e))?;
    Ok(())
}

/// Lopende take nu afsluiten en wegschrijven (afsluiten app); antwoord = pad of null.
/// Async + spawn_blocking (patroon set_audio_output): het wachten op de
/// MIDI-thread (tot 1,5 s, bv. midden in een leermodus) mag de UI-thread
/// niet blokkeren.
#[tauri::command]
pub async fn midi_archive_flush(state: State<'_, AppState>) -> Result<Option<String>, String> {
    let st = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        st.midi_archive_flush(std::time::Duration::from_millis(1500))
            .map(|p| p.to_string_lossy().to_string())
    })
    .await
    .map_err(|e| format!("archief-flush-taak mislukt: {}", e))
}

// ============ Muzieknotatie (MIDI → MusicXML) ============

/// Zet een MIDI-bestand (de eigen opname of een willekeurige .mid) om naar
/// MusicXML voor het notatievenster. De noten worden per divisie op een eigen
/// notenbalk gezet via de kanaal→divisie-mapping van het geladen orgel
/// (inclusief transpose → klinkende toonhoogte; Pedaal krijgt een bassleutel).
/// Noten op kanalen zonder mapping komen op een eigen balk "Kanaal N".
#[tauri::command]
pub fn convert_midi_to_musicxml(
    state: State<AppState>,
    path: String,
    options: crate::notation::NotationOptions,
) -> Result<String, String> {
    use crate::notation::{NoteEv, Staff};

    let bytes = std::fs::read(&path).map_err(|e| format!("Kan MIDI-bestand niet lezen: {}", e))?;
    let raw = crate::notation::extract_notes(&bytes)?;
    if raw.is_empty() {
        return Err("Geen noten gevonden in het MIDI-bestand".into());
    }

    // Stap 1: noten per divisie verzamelen (kanaal→divisie via de mappings,
    // transpose → klinkende toonhoogte). Onbekende kanalen krijgen een eigen
    // "Kanaal N"-groep.
    let mut group_names: Vec<String> = Vec::new(); // in orgel-volgorde
    let mut is_pedal: HashMap<String, bool> = HashMap::new();
    {
        let organ = state.loaded_organ_info.read();
        if let Some(ref o) = *organ {
            for d in &o.divisions {
                let lname = d.name.to_lowercase();
                is_pedal.insert(d.name.clone(), lname.contains("pedaal") || lname.contains("pedal"));
                group_names.push(d.name.clone());
            }
        }
    }
    let mut notes_of: HashMap<String, Vec<NoteEv>> = HashMap::new();
    let mappings = state.midi_mappings.read().clone();
    for n in raw {
        // Eerste mapping (in orgel-volgorde) die dit kanaal + deze noot
        // accepteert bepaalt de divisie — zelfde regel als het spelen zelf.
        let target = mappings.iter().find(|m| m.accepts(n.channel, n.note));
        let (group, midi) = match target {
            Some(m) => (m.division.clone(), (n.note as i16 + m.transpose as i16).clamp(0, 127) as u8),
            None => (format!("Kanaal {}", n.channel + 1), n.note),
        };
        if !group_names.contains(&group) {
            group_names.push(group.clone());
        }
        notes_of.entry(group).or_default()
            .push(NoteEv { midi, start_sec: n.start_sec, end_sec: n.end_sec });
    }

    // Stap 2: balk-indeling. Met een door de gebruiker samengestelde indeling
    // (notatievenster: vinkjes per balk) worden de aangevinkte divisies per
    // balk samengevoegd; anders automatisch één balk per divisie/kanaal.
    let mut staves: Vec<Staff> = Vec::new();
    match options.staves.as_ref().filter(|s| !s.is_empty()) {
        Some(specs) => {
            for spec in specs {
                let mut notes: Vec<NoteEv> = Vec::new();
                for div in &spec.divisions {
                    if let Some(ns) = notes_of.get(div) {
                        notes.extend(ns.iter().cloned());
                    }
                }
                let bass = spec.bass_clef.unwrap_or_else(|| {
                    spec.divisions.iter().any(|d| *is_pedal.get(d).unwrap_or(&false))
                });
                let name = match &spec.name {
                    Some(n) if !n.trim().is_empty() => n.clone(),
                    _ => spec.divisions.join(" + "),
                };
                staves.push(Staff { name, bass_clef: bass, notes });
            }
        }
        None => {
            for group in &group_names {
                let Some(notes) = notes_of.get(group) else { continue; };
                staves.push(Staff {
                    name: group.clone(),
                    bass_clef: *is_pedal.get(group).unwrap_or(&false),
                    notes: notes.clone(),
                });
            }
        }
    }

    crate::notation::build_musicxml(&staves, &options)
}

/// Sla MusicXML op (doel gekozen via de save-dialoog in de frontend).
#[tauri::command]
pub fn save_musicxml(path: String, xml: String) -> Result<(), String> {
    if let Some(parent) = std::path::Path::new(&path).parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::write(&path, xml.as_bytes()).map_err(|e| format!("Kan niet schrijven: {}", e))?;
    info!("MusicXML opgeslagen naar {}", path);
    Ok(())
}

// ============ Live-notatie: Score-lifecycle + edits ============
// Elke Score is een aparte partituur (één per notatievenster). De opname is
// gebonden aan de armed_score + armed_layer + armed_take. Wijzigingen aan de
// Score gaan via editing-commands; de UI ontvangt bij elke wijziging een
// generation-bump + `score-changed`-event (de frontend regenereert dan de XML).

fn with_score_mut<T>(
    state: &State<AppState>,
    score_id: u32,
    f: impl FnOnce(&mut crate::notation::Score) -> T,
) -> Result<T, String> {
    let mut scores = state.notation_scores.write();
    match scores.get_mut(&score_id) {
        Some(sc) => Ok(f(sc)),
        None => Err(format!("Score {} niet gevonden", score_id)),
    }
}

async fn emit_score_changed(app: tauri::AppHandle, score_id: u32, generation: u64) {
    use tauri::Emitter;
    let _ = app.emit("jm-orgue:notation:score-changed", serde_json::json!({
        "score": score_id, "generation": generation,
    }));
}

/// Nieuwe (lege) Score aanmaken; geeft de score-ID terug.
#[tauri::command]
pub fn notation_new_score(state: State<AppState>, app: tauri::AppHandle) -> Result<u32, String> {
    let id = {
        let mut next = state.notation_next_id.write();
        let id = *next;
        *next = next.saturating_add(1);
        id
    };
    let mut sc = crate::notation::Score::new(id);
    // Startlagen: één laag per divisie in het geladen orgel (Pedaal → bas).
    // Zonder orgel: één generieke laag.
    {
        let organ = state.loaded_organ_info.read();
        if let Some(ref o) = *organ {
            for d in &o.divisions {
                let lname = d.name.to_lowercase();
                let bass = lname.contains("pedaal") || lname.contains("pedal");
                sc.add_layer(d.name.clone(), Some(bass));
                // Standaard-routering: elke balk krijgt zijn eigen divisie, zodat
                // inspelen meteen per divisie op de juiste balk landt (0.7.14).
                if let Some(l) = sc.layers.last_mut() { l.divisions = vec![d.name.clone()]; }
            }
        }
        if sc.layers.is_empty() {
            sc.add_layer("Balk 1".into(), Some(false));
        }
    }
    // Eerste laag standaard armed → gebruiker kan direct opnemen.
    sc.armed_layer = sc.layers.first().map(|l| l.id);
    state.notation_scores.write().insert(id, sc);
    // AppHandle-spiegel voor de MIDI-thread: eenmalig doorgeven.
    *state.notation_app_handle.write() = Some(app.clone());
    Ok(id)
}

/// Score-snapshot uitlezen (voor de UI: LayerBar + selectie-model).
#[tauri::command]
pub fn notation_get_score(state: State<AppState>, score_id: u32) -> Result<crate::notation::Score, String> {
    let scores = state.notation_scores.read();
    scores.get(&score_id).cloned().ok_or_else(|| format!("Score {} niet gevonden", score_id))
}

/// MusicXML voor het gerenderde beeld (zichtbare takes van alle lagen).
#[tauri::command]
pub fn notation_get_musicxml(state: State<AppState>, score_id: u32) -> Result<String, String> {
    let scores = state.notation_scores.read();
    let sc = scores.get(&score_id).ok_or_else(|| format!("Score {} niet gevonden", score_id))?;
    crate::notation::build_musicxml_from_score(sc)
}

/// Nieuwe laag toevoegen. Geeft de laag-ID terug.
#[tauri::command]
pub fn notation_add_layer(state: State<AppState>, score_id: u32, name: String, bass_clef: Option<bool>) -> Result<u32, String> {
    with_score_mut(&state, score_id, |sc| sc.add_layer(name, bass_clef))
}

/// Extra take onder een laag toevoegen (overdub). Geeft de nieuwe take-ID terug.
#[tauri::command]
pub fn notation_add_take(state: State<AppState>, score_id: u32, layer_id: u32) -> Result<u32, String> {
    with_score_mut(&state, score_id, |sc| sc.add_take(layer_id))?
        .ok_or_else(|| "Laag niet gevonden".into())
}

/// Zet of wis armed voor een laag (opname-doel). Slechts één laag tegelijk.
#[tauri::command]
pub fn notation_arm_layer(state: State<AppState>, score_id: u32, layer_id: Option<u32>) -> Result<(), String> {
    with_score_mut(&state, score_id, |sc| {
        sc.armed_layer = layer_id.filter(|id| sc.layers.iter().any(|l| l.id == *id));
    })
}

/// Zichtbaarheid van een take (overdub-vinkje).
#[tauri::command]
pub fn notation_set_take_visible(state: State<AppState>, score_id: u32, layer_id: u32, take_id: u32, visible: bool) -> Result<(), String> {
    with_score_mut(&state, score_id, |sc| {
        if let Some(l) = sc.layers.iter_mut().find(|l| l.id == layer_id) {
            if let Some(t) = l.takes.iter_mut().find(|t| t.id == take_id) {
                t.visible = visible;
                sc.bump_gen();
            }
        }
    })
}

/// Live-opname starten voor deze score: armed_score = score_id, starttijd
/// wordt door de MIDI-thread bij de eerste NoteOn gezet (geen count-in).
#[tauri::command]
pub fn notation_start_recording(state: State<AppState>, score_id: u32) -> Result<(), String> {
    // Verse open-notes + starttijd (die krijgt zijn waarde bij de eerste NoteOn).
    state.notation_open_notes.write().clear();
    *state.notation_start_time.write() = None;
    *state.notation_armed_score.write() = Some(score_id);
    Ok(())
}

/// Live-opname stoppen. Open notes worden op "nu" afgesloten (paniek-safe).
#[tauri::command]
pub fn notation_stop_recording(state: State<AppState>) -> Result<(), String> {
    use tauri::Emitter;
    let handle = state.notation_app_handle.read().clone();
    let start = state.notation_start_time.read().clone();
    let armed = state.notation_armed_score.read().clone();
    if let (Some(handle), Some(start), Some(score_id)) = (handle, start, armed) {
        let end_us = std::time::Instant::now().saturating_duration_since(start).as_micros() as u64;
        // Alle openstaande NoteOns netjes afsluiten (voorkomt eeuwig-hangende
        // pedaalnoten in de notatie).
        let open: Vec<((u32, u32, u32, u8, u8), u64)> = state.notation_open_notes.write().drain().collect();
        let mut scores = state.notation_scores.write();
        for ((sid, lid, tid, ch, note), start_us) in open {
            if sid != score_id { continue; }
            if let Some(sc) = scores.get_mut(&sid) {
                let ev_id = sc.new_event_id();
                if let Some(layer) = sc.layers.iter_mut().find(|l| l.id == lid) {
                    if let Some(take) = layer.takes.iter_mut().find(|t| t.id == tid) {
                        let end_us = end_us.max(start_us + 20_000);
                        take.events.push(crate::notation::LayerEv {
                            id: ev_id, midi: note, start_us, end_us,
                            channel: ch, locked: false,
                        });
                        let _ = handle.emit("jm-orgue:notation:note-added", serde_json::json!({
                            "score": sid, "layer": lid, "take": tid,
                            "id": ev_id, "midi": note, "channel": ch,
                            "start_us": start_us, "end_us": end_us,
                        }));
                    }
                }
                sc.bump_gen();
            }
        }
    }
    *state.notation_armed_score.write() = None;
    Ok(())
}

/// Bewerk-commando's toepassen (Delete/Transpose/…); geeft de nieuwe generation
/// terug zodat de UI weet dat regenereren nodig is.
#[tauri::command]
pub fn notation_delete_events(state: State<AppState>, app: tauri::AppHandle, score_id: u32, event_ids: Vec<u64>) -> Result<u64, String> {
    let events_to_delete = with_score_mut(&state, score_id, |sc| {
        // Verzamel (layer, take, LayerEv) voor het commando.
        let mut out = Vec::new();
        for id in &event_ids {
            if let Some((li, ti, ei)) = sc.locate(*id) {
                let l_id = sc.layers[li].id;
                let t_id = sc.layers[li].takes[ti].id;
                out.push((l_id, t_id, sc.layers[li].takes[ti].events[ei].clone()));
            }
        }
        out
    })?;
    if events_to_delete.is_empty() { return Err("Geen geldige notatie-events geselecteerd".into()); }
    let gen = with_score_mut(&state, score_id, |sc| {
        let cmd = crate::notation::EditCommand::DeleteEvents { events: events_to_delete };
        if let Some(inv) = cmd.apply(sc) {
            sc.push_undo(inv);
        }
        sc.generation
    })?;
    tauri::async_runtime::spawn(emit_score_changed(app, score_id, gen));
    Ok(gen)
}

#[tauri::command]
pub fn notation_transpose(state: State<AppState>, app: tauri::AppHandle, score_id: u32, event_ids: Vec<u64>, semitones: i8) -> Result<u64, String> {
    let gen = with_score_mut(&state, score_id, |sc| {
        let cmd = crate::notation::EditCommand::Transpose { ids: event_ids, semitones };
        if let Some(inv) = cmd.apply(sc) {
            sc.push_undo(inv);
        }
        sc.generation
    })?;
    tauri::async_runtime::spawn(emit_score_changed(app, score_id, gen));
    Ok(gen)
}

/// Duur van geselecteerde noten wijzigen (halveren/verdubbelen/punt): de UI
/// rekent per event het nieuwe absolute eind (µs) uit en stuurt (id, end_us).
/// De backend zet het via SetEnds (exacte undo). Render kwantiseert de nieuwe
/// duur naar het raster.
#[tauri::command]
pub fn notation_set_durations(state: State<AppState>, app: tauri::AppHandle, score_id: u32, ends: Vec<(u64, u64)>) -> Result<u64, String> {
    let gen = with_score_mut(&state, score_id, |sc| {
        let cmd = crate::notation::EditCommand::SetEnds { items: ends };
        if let Some(inv) = cmd.apply(sc) {
            sc.push_undo(inv);
        }
        sc.generation
    })?;
    tauri::async_runtime::spawn(emit_score_changed(app, score_id, gen));
    Ok(gen)
}

/// Eén te plakken noot (absolute tijden al door de UI berekend: cursor-relatief
/// en op het doelbalk-raster gesnapt).
#[derive(serde::Deserialize)]
pub struct PasteNote {
    pub midi: u8,
    pub start_us: u64,
    pub end_us: u64,
    #[serde(default)]
    pub channel: u8,
}

/// Plak noten in een laag (armed take, of de eerste zichtbare take). De backend
/// kent verse id's toe en voegt ze via InsertEvents in (undo = verwijderen).
#[tauri::command]
pub fn notation_paste(state: State<AppState>, app: tauri::AppHandle, score_id: u32, layer_id: u32, notes: Vec<PasteNote>) -> Result<u64, String> {
    if notes.is_empty() { return Err("Klembord is leeg".into()); }
    let gen = with_score_mut(&state, score_id, |sc| {
        // Doeltake bepalen: armed take, anders de eerste zichtbare, anders de eerste.
        let take_id = sc.layers.iter().find(|l| l.id == layer_id).and_then(|l| {
            l.armed_take
                .or_else(|| l.takes.iter().find(|t| t.visible).map(|t| t.id))
                .or_else(|| l.takes.first().map(|t| t.id))
        });
        let take_id = match take_id { Some(t) => t, None => return sc.generation };
        let mut events = Vec::with_capacity(notes.len());
        for n in notes.iter() {
            let id = sc.new_event_id();
            let start_us = n.start_us;
            let end_us = n.end_us.max(start_us + 1000);
            events.push((layer_id, take_id, crate::notation::LayerEv {
                id, midi: n.midi, start_us, end_us, channel: n.channel, locked: true,
            }));
        }
        let cmd = crate::notation::EditCommand::InsertEvents { events };
        if let Some(inv) = cmd.apply(sc) {
            sc.push_undo(inv);
        }
        sc.generation
    })?;
    tauri::async_runtime::spawn(emit_score_changed(app, score_id, gen));
    Ok(gen)
}

#[tauri::command]
pub fn notation_set_tolerance(state: State<AppState>, app: tauri::AppHandle, score_id: u32, tolerance_pct: u8) -> Result<u64, String> {
    let gen = with_score_mut(&state, score_id, |sc| {
        let cmd = crate::notation::EditCommand::SetTolerance { old: sc.tolerance_pct, new: tolerance_pct };
        if let Some(inv) = cmd.apply(sc) {
            sc.push_undo(inv);
        }
        sc.generation
    })?;
    tauri::async_runtime::spawn(emit_score_changed(app, score_id, gen));
    Ok(gen)
}

#[tauri::command]
pub fn notation_set_bpm(state: State<AppState>, app: tauri::AppHandle, score_id: u32, bpm: f64) -> Result<u64, String> {
    let gen = with_score_mut(&state, score_id, |sc| {
        let cmd = crate::notation::EditCommand::SetBpm { old: sc.bpm, new: bpm.max(20.0).min(300.0) };
        if let Some(inv) = cmd.apply(sc) {
            sc.push_undo(inv);
        }
        sc.generation
    })?;
    tauri::async_runtime::spawn(emit_score_changed(app, score_id, gen));
    Ok(gen)
}

#[tauri::command]
pub fn notation_set_key(state: State<AppState>, app: tauri::AppHandle, score_id: u32, key_fifths: i8) -> Result<u64, String> {
    let gen = with_score_mut(&state, score_id, |sc| {
        let cmd = crate::notation::EditCommand::SetKey { old: sc.key_fifths, new: key_fifths };
        if let Some(inv) = cmd.apply(sc) {
            sc.push_undo(inv);
        }
        sc.generation
    })?;
    tauri::async_runtime::spawn(emit_score_changed(app, score_id, gen));
    Ok(gen)
}

/// Eén balk-specificatie uit de wizard van het notatievenster.
#[derive(serde::Deserialize)]
pub struct LayerSpecDto {
    pub name: String,
    pub bass_clef: bool,
    #[serde(default)]
    pub divisions: Vec<String>,
}

/// Wizard: vervang de balkindeling van een nog lege score (aantal balken,
/// naam, sleutel, divisie-routering). Alleen toegestaan zolang er geen noten
/// zijn — daarna zou vervangen werk weggooien; wijzigingen gaan dan via
/// notation_set_layer_divisions / notation_add_layer.
#[tauri::command]
pub fn notation_configure_layers(state: State<AppState>, app: tauri::AppHandle, score_id: u32, layers: Vec<LayerSpecDto>) -> Result<u64, String> {
    if layers.is_empty() { return Err("Minstens één balk is nodig".into()); }
    let gen = with_score_mut(&state, score_id, |sc| {
        let has_events = sc.layers.iter().any(|l| l.takes.iter().any(|t| !t.events.is_empty()));
        if has_events { return Err("De balkindeling kan alleen gewijzigd worden zolang de partituur leeg is".to_string()); }
        sc.layers.clear();
        for spec in layers {
            sc.add_layer(spec.name, Some(spec.bass_clef));
            if let Some(l) = sc.layers.last_mut() { l.divisions = spec.divisions; }
        }
        sc.armed_layer = sc.layers.first().map(|l| l.id);
        Ok(sc.generation)
    })??;
    tauri::async_runtime::spawn(emit_score_changed(app, score_id, gen));
    Ok(gen)
}

/// Divisie-routering van één balk wijzigen (LayerBar-chips). Geen notatie-
/// wijziging → geen undo/generation-bump.
#[tauri::command]
pub fn notation_set_layer_divisions(state: State<AppState>, score_id: u32, layer_id: u32, divisions: Vec<String>) -> Result<(), String> {
    with_score_mut(&state, score_id, |sc| {
        if let Some(l) = sc.layers.iter_mut().find(|l| l.id == layer_id) {
            l.divisions = divisions;
        }
    })
}

/// Selectie in de tijd verschuiven (µs, mag negatief; klemt op 0). De UI stuurt
/// een raster-gesnapte delta (Shift+←/→ of slepen); via SetTimes → exacte undo.
#[tauri::command]
pub fn notation_shift_events(state: State<AppState>, app: tauri::AppHandle, score_id: u32, event_ids: Vec<u64>, delta_us: i64) -> Result<u64, String> {
    if event_ids.is_empty() || delta_us == 0 { return Err("Niets te verschuiven".into()); }
    let gen = with_score_mut(&state, score_id, |sc| {
        let mut items: Vec<(u64, u64, u64)> = Vec::new();
        for id in &event_ids {
            if let Some((li, ti, ei)) = sc.locate(*id) {
                let ev = &sc.layers[li].takes[ti].events[ei];
                let start = (ev.start_us as i64 + delta_us).max(0) as u64;
                let end = (ev.end_us as i64 + delta_us).max(1000) as u64;
                items.push((*id, start, end));
            }
        }
        let cmd = crate::notation::EditCommand::SetTimes { items };
        if let Some(inv) = cmd.apply(sc) {
            sc.push_undo(inv);
        }
        sc.generation
    })?;
    tauri::async_runtime::spawn(emit_score_changed(app, score_id, gen));
    Ok(gen)
}

/// Maatsoort (x/4) van een live-score wijzigen — met undo, zoals bpm/toonsoort.
#[tauri::command]
pub fn notation_set_meter(state: State<AppState>, app: tauri::AppHandle, score_id: u32, beats_per_bar: u8) -> Result<u64, String> {
    let gen = with_score_mut(&state, score_id, |sc| {
        let cmd = crate::notation::EditCommand::SetMeter { old: sc.beats_per_bar, new: beats_per_bar };
        if let Some(inv) = cmd.apply(sc) {
            sc.push_undo(inv);
        }
        sc.generation
    })?;
    tauri::async_runtime::spawn(emit_score_changed(app, score_id, gen));
    Ok(gen)
}

/// Titel van een live-score (boven de partituur + bestandsnaam-suggestie).
#[tauri::command]
pub fn notation_set_title(state: State<AppState>, app: tauri::AppHandle, score_id: u32, title: String) -> Result<u64, String> {
    let gen = with_score_mut(&state, score_id, |sc| {
        sc.title = title;
        sc.bump_gen();
        sc.generation
    })?;
    tauri::async_runtime::spawn(emit_score_changed(app, score_id, gen));
    Ok(gen)
}

/// "Opslaan als MIDI…": exporteer de zichtbare takes als .mid-bestand.
#[tauri::command]
pub fn notation_export_midi(state: State<AppState>, score_id: u32, path: String) -> Result<(), String> {
    let bytes = {
        let scores = state.notation_scores.read();
        let sc = scores.get(&score_id).ok_or_else(|| format!("Score {} niet gevonden", score_id))?;
        crate::notation::score_to_smf_bytes(sc)?
    };
    std::fs::write(&path, bytes).map_err(|e| format!("MIDI-bestand opslaan mislukt: {}", e))
}

/// Stapinvoer aan/uit voor een score. Met stapinvoer aan stuurt de midi-thread
/// step-note-events naar het notatievenster (de toets klinkt gewoon mee);
/// het venster plaatst de noot op de invoercursor. Live-opname gaat altijd vóór.
#[tauri::command]
pub fn notation_set_step_input(state: State<AppState>, score_id: u32, enabled: bool) -> Result<(), String> {
    *state.notation_step_input.write() = if enabled { Some(score_id) } else { None };
    Ok(())
}

/// Stapinvoer/muisplaatsing: voeg één noot of akkoord toe op een absolute tijd
/// met een expliciete duur (beide µs). Gaat via InsertEvents → undo werkt.
/// Geeft (generation, nieuwe event-ids) terug zodat de stapinvoer de zojuist
/// geplaatste noot kan alteren (↑/↓ = kruis/mol, MuseScore-conventie).
#[tauri::command]
pub fn notation_insert_notes(state: State<AppState>, app: tauri::AppHandle, score_id: u32, layer_id: u32, notes: Vec<u8>, start_us: u64, dur_us: u64) -> Result<(u64, Vec<u64>), String> {
    if notes.is_empty() { return Err("Geen noten om in te voegen".into()); }
    let (gen, ids) = with_score_mut(&state, score_id, |sc| {
        let take_id = sc.layers.iter().find(|l| l.id == layer_id).and_then(|l| {
            l.armed_take
                .or_else(|| l.takes.iter().find(|t| t.visible).map(|t| t.id))
                .or_else(|| l.takes.first().map(|t| t.id))
        });
        let take_id = match take_id { Some(t) => t, None => return (sc.generation, Vec::new()) };
        let dur = dur_us.max(1000);
        let mut events = Vec::with_capacity(notes.len());
        let mut ids = Vec::with_capacity(notes.len());
        for midi in notes.iter() {
            let id = sc.new_event_id();
            ids.push(id);
            events.push((layer_id, take_id, crate::notation::LayerEv {
                id, midi: *midi, start_us, end_us: start_us + dur, channel: 0, locked: true,
            }));
        }
        let cmd = crate::notation::EditCommand::InsertEvents { events };
        if let Some(inv) = cmd.apply(sc) {
            sc.push_undo(inv);
        }
        (sc.generation, ids)
    })?;
    tauri::async_runtime::spawn(emit_score_changed(app, score_id, gen));
    Ok((gen, ids))
}

/// Metronoom-configuratie per score (aan/uit + aantal count-in-tellen). Puur
/// een afspeel-hulp in het notatievenster (aparte WebAudio-klik, geen orgelpijp)
/// en dus géén notatie-wijziging: geen undo, geen generation-bump — de UI houdt
/// zijn eigen toggle-stand bij en persist't hem hiermee in het Score-model.
#[tauri::command]
pub fn notation_set_metronome(state: State<AppState>, score_id: u32, click_on: bool, count_in_beats: u8) -> Result<(), String> {
    with_score_mut(&state, score_id, |sc| {
        sc.metronome = crate::notation::MetronomeCfg { click_on, count_in_beats: count_in_beats.min(8) };
    })?;
    Ok(())
}

#[tauri::command]
pub fn notation_undo(state: State<AppState>, app: tauri::AppHandle, score_id: u32) -> Result<u64, String> {
    let gen = with_score_mut(&state, score_id, |sc| {
        if let Some(cmd) = sc.pop_undo() {
            if let Some(redo_cmd) = cmd.apply(sc) {
                sc.push_redo(redo_cmd);
            }
        }
        sc.generation
    })?;
    tauri::async_runtime::spawn(emit_score_changed(app, score_id, gen));
    Ok(gen)
}

/// Importeer een MIDI-bestand in een bepaalde take van een score. De noten
/// worden toegewezen aan lagen via de kanaal→divisie-mapping (zelfde regel als
/// bij het spelen); noten op een niet-gemapt kanaal komen in de opgegeven
/// standaard-laag (`fallback_layer`). Als `into_take` `None` is, wordt een
/// nieuwe take aangemaakt in de armed laag. Gebruikt voor "Openen…" in het
/// notatievenster: het bestand komt binnen zonder de bestaande opname te wissen.
#[tauri::command]
pub fn notation_import_midi(
    state: State<AppState>,
    app: tauri::AppHandle,
    score_id: u32,
    path: String,
    into_take: Option<u32>,
) -> Result<u64, String> {
    let bytes = std::fs::read(&path).map_err(|e| format!("Kan MIDI-bestand niet lezen: {}", e))?;
    let raw = crate::notation::extract_notes(&bytes)?;
    if raw.is_empty() {
        return Err("Geen noten gevonden in het MIDI-bestand".into());
    }

    // Kanaal → divisienaam (voor de layer-toewijzing).
    let mappings = state.midi_mappings.read().clone();

    let gen = {
        let mut scores = state.notation_scores.write();
        let sc = scores.get_mut(&score_id).ok_or_else(|| format!("Score {} niet gevonden", score_id))?;

        // Bepaal doelbalk voor elke divisie-naam (bestaande laag met die naam,
        // anders eerste laag als fallback zodat er niets verloren gaat).
        let mut division_to_layer: HashMap<String, u32> = HashMap::new();
        for layer in &sc.layers {
            division_to_layer.entry(layer.name.clone()).or_insert(layer.id);
        }
        let fallback_layer = sc.layers.first().map(|l| l.id).ok_or("Score heeft geen enkele laag")?;

        // Doel-take per laag: bij expliciet `into_take` gaat álles daar naartoe
        // (op de laag waar die take bij hoort); anders per laag een nieuwe take
        // "Import (bestandsnaam)".
        let file_stem = std::path::Path::new(&path).file_stem()
            .and_then(|s| s.to_str()).unwrap_or("import").to_string();
        let mut take_for_layer: HashMap<u32, u32> = HashMap::new();

        if let Some(tid) = into_take {
            // Zoek de laag waar deze take bij hoort en gebruik hem als algemene doelbak.
            let owner_layer = sc.layers.iter()
                .find(|l| l.takes.iter().any(|t| t.id == tid))
                .map(|l| l.id)
                .ok_or("Take niet gevonden in deze score")?;
            take_for_layer.insert(owner_layer, tid);
        }

        for n in raw {
            // Kanaal → divisienaam (eerste mapping die deze noot accepteert).
            let target_layer = mappings.iter()
                .find(|m| m.accepts(n.channel, n.note))
                .and_then(|m| division_to_layer.get(&m.division).copied())
                .unwrap_or(fallback_layer);

            // Voor deze laag een take reserveren (bij ontbreken van into_take:
            // maak per laag een nieuwe take zodat de import naast bestaande
            // opnames staat en er niets over elkaar heen komt).
            let take_id = match take_for_layer.get(&target_layer) {
                Some(id) => *id,
                None => {
                    // Nieuwe take aanmaken via sc.add_take (die bump't gen).
                    let new_take = sc.add_take(target_layer).ok_or("Kan geen take toevoegen")?;
                    // Naam mooier maken dan "Take N": pak de zojuist toegevoegde take en hernoem.
                    if let Some(layer) = sc.layers.iter_mut().find(|l| l.id == target_layer) {
                        if let Some(take) = layer.takes.iter_mut().find(|t| t.id == new_take) {
                            take.name = format!("Import ({})", file_stem);
                        }
                    }
                    take_for_layer.insert(target_layer, new_take);
                    new_take
                }
            };

            // Klinkende toonhoogte via de mapping-transpose, gelijk aan spelen.
            let transpose = mappings.iter()
                .find(|m| m.accepts(n.channel, n.note))
                .map(|m| m.transpose).unwrap_or(0);
            let midi = (n.note as i16 + transpose as i16).clamp(0, 127) as u8;

            let ev_id = sc.new_event_id();
            let start_us = (n.start_sec * 1_000_000.0) as u64;
            let end_us = ((n.end_sec * 1_000_000.0) as u64).max(start_us + 20_000);
            if let Some(layer) = sc.layers.iter_mut().find(|l| l.id == target_layer) {
                if let Some(take) = layer.takes.iter_mut().find(|t| t.id == take_id) {
                    take.events.push(crate::notation::LayerEv {
                        id: ev_id, midi, start_us, end_us,
                        channel: n.channel, locked: false,
                    });
                }
            }
        }
        sc.bump_gen();
        sc.generation
    };
    tauri::async_runtime::spawn(emit_score_changed(app, score_id, gen));
    Ok(gen)
}

#[tauri::command]
pub fn notation_redo(state: State<AppState>, app: tauri::AppHandle, score_id: u32) -> Result<u64, String> {
    let gen = with_score_mut(&state, score_id, |sc| {
        if let Some(cmd) = sc.pop_redo() {
            if let Some(undo_cmd) = cmd.apply(sc) {
                sc.push_undo_no_clear_redo(undo_cmd);
            }
        }
        sc.generation
    })?;
    tauri::async_runtime::spawn(emit_score_changed(app, score_id, gen));
    Ok(gen)
}

/// Helper: write variable-length quantity
fn write_var_len(buf: &mut Vec<u8>, mut value: u32) {
    if value == 0 {
        buf.push(0);
        return;
    }
    let mut bytes = Vec::new();
    while value > 0 {
        bytes.push((value & 0x7F) as u8);
        value >>= 7;
    }
    bytes.reverse();
    for (i, b) in bytes.iter().enumerate() {
        if i < bytes.len() - 1 {
            buf.push(b | 0x80);
        } else {
            buf.push(*b);
        }
    }
}

/// Set per-pipe voicing adjustment (also stored in state for persistence)
#[tauri::command]
pub fn set_pipe_voicing(state: State<AppState>, stop_id: u32, pipe_num: u32, volume_db: f32, pitch_cents: f32) -> Result<(), String> {
    let v_db = volume_db.clamp(-20.0, 20.0);
    let p_cents = pitch_cents.clamp(-100.0, 100.0);
    // Store in state
    {
        let mut voicings = state.pipe_voicings.write();
        if v_db == 0.0 && p_cents == 0.0 {
            voicings.remove(&(stop_id, pipe_num));
        } else {
            voicings.insert((stop_id, pipe_num), (v_db, p_cents));
        }
    }
    // Apply to audio thread
    state.send_audio_command(AudioCommand::SetPipeVoicing {
        stop_id, pipe_num,
        volume_db: v_db,
        pitch_cents: p_cents,
    });
    Ok(())
}

/// Get all pipe voicings for current organ
#[tauri::command]
pub fn get_pipe_voicings(state: State<AppState>) -> Vec<(u32, u32, f32, f32)> {
    state.pipe_voicings.read()
        .iter()
        .map(|(&(s, p), &(v, c))| (s, p, v, c))
        .collect()
}

/// Pas opgeslagen divisie-naar-wind-groep toewijzing toe vanuit OrganSettings
#[tauri::command]
pub fn apply_saved_wind_groups(state: State<AppState>) -> Result<usize, String> {
    let organ_id = match state.current_organ_id.read().clone() {
        Some(id) => id,
        None => return Ok(0),
    };
    let (saved_groups, saved_num) = {
        let lib = state.organ_library.read();
        match lib.settings.get(&organ_id) {
            Some(s) => (s.division_wind_groups.clone(), s.num_wind_groups),
            None => (Vec::new(), 0),
        }
    };
    let organ = state.loaded_organ_info.read();
    let organ = match organ.as_ref() {
        Some(o) => o,
        None => return Ok(0),
    };
    // Eerst aantal groepen restoren (indien opgeslagen, niet 0)
    if saved_num >= 1 && saved_num <= 8 {
        *state.num_wind_groups.write() = saved_num;
    }
    let mut applied = 0;
    for entry in &saved_groups {
        if let Some(idx) = organ.divisions.iter().position(|d| d.name == entry.division) {
            let grp = entry.group.min(31);
            {
                let mut wg = state.division_wind_groups.write();
                if idx < wg.len() {
                    wg[idx] = grp;
                }
            }
            state.send_audio_command(AudioCommand::SetDivisionWindGroup {
                division_index: idx as u8,
                group: grp,
            });
            applied += 1;
        }
    }
    Ok(applied)
}

/// Migreer een oude output-pair-index (0..3) naar fysieke kanalen (gangbare 7.1-ordening).
fn migrate_pair_to_channels(pair: u8) -> Vec<u16> {
    match pair {
        1 => vec![4, 5], // Achter L/R
        2 => vec![6, 7], // Zij L/R
        3 => vec![2, 3], // Center/LFE als paar
        _ => vec![0, 1], // Voor L/R
    }
}

/// Pas opgeslagen divisie-naar-output-kanaal routing + C/Cis-spreiding toe vanuit OrganSettings
#[tauri::command]
pub fn apply_saved_output_channels(state: State<AppState>) -> Result<usize, String> {
    Ok(apply_saved_output_channels_inner(&state))
}

/// Kern van apply_saved_output_channels, ook aanroepbaar zonder Tauri-State
/// (backend-geïnitieerde herlaad in main.rs).
pub fn apply_saved_output_channels_inner(state: &AppState) -> usize {
    let organ_id = match state.current_organ_id.read().clone() {
        Some(id) => id,
        None => return 0,
    };
    let (saved_routing, saved_ccis, saved_spread) = {
        let lib = state.organ_library.read();
        match lib.settings.get(&organ_id) {
            Some(s) => (s.division_output_pairs.clone(), s.division_ccis.clone(), s.ccis_spread.clone()),
            None => (Vec::new(), Vec::new(), None),
        }
    };
    let organ = state.loaded_organ_info.read();
    let organ = match organ.as_ref() {
        Some(o) => o,
        None => return 0,
    };
    let mut applied = 0;
    for entry in &saved_routing {
        if let Some(idx) = organ.divisions.iter().position(|d| d.name == entry.division) {
            // Nieuw model = vrije kanaallijst; oude opslag: migreer pair → kanalen.
            let channels = entry.channels.clone().unwrap_or_else(|| migrate_pair_to_channels(entry.pair));
            {
                let mut chans = state.division_output_channels.write();
                if idx < chans.len() { chans[idx] = channels.clone(); }
            }
            state.send_audio_command(AudioCommand::SetDivisionOutputChannels {
                division_index: idx as u8,
                channels,
            });
            applied += 1;
        }
    }
    // Kanaal-override van het actieve uitvoerprofiel wint van de per-orgel
    // opgeslagen routing: als laatste toepassen, alleen voor divisienamen die
    // in dit orgel bestaan (andere divisies houden hun opgeslagen routing).
    let override_channels = state.profile_channel_override.read().clone();
    if let Some(ov) = override_channels {
        for (division, channels) in &ov {
            if let Some(idx) = organ.divisions.iter().position(|d| &d.name == division) {
                {
                    let mut chans = state.division_output_channels.write();
                    if idx < chans.len() { chans[idx] = channels.clone(); }
                }
                state.send_audio_command(AudioCommand::SetDivisionOutputChannels {
                    division_index: idx as u8,
                    channels: channels.clone(),
                });
                applied += 1;
            }
        }
        info!("Profiel-kanaaloverride toegepast op {} divisies", ov.len());
    }
    // C/Cis globale parameters
    if let Some(sp) = saved_spread {
        *state.ccis_spread.write() = (sp.strength, sp.falloff, sp.swap);
        state.send_audio_command(AudioCommand::SetCcisSpread {
            strength: sp.strength, falloff: sp.falloff, swap: sp.swap,
        });
    }
    // C/Cis per-divisie aan/uit
    for entry in &saved_ccis {
        if let Some(idx) = organ.divisions.iter().position(|d| d.name == entry.division) {
            {
                let mut en = state.division_ccis_enabled.write();
                if idx < en.len() { en[idx] = entry.enabled; }
            }
            state.send_audio_command(AudioCommand::SetDivisionCcis {
                division_index: idx as u8, enabled: entry.enabled,
            });
        }
    }
    applied
}

/// Apply saved voicings from organ settings to state + audio thread
#[tauri::command]
pub fn apply_saved_voicings(state: State<AppState>) -> Result<usize, String> {
    let organ_id = match state.current_organ_id.read().clone() {
        Some(id) => id,
        None => return Ok(0),
    };

    let saved = {
        let lib = state.organ_library.read();
        lib.settings.get(&organ_id)
            .map(|s| s.pipe_voicings.clone())
            .unwrap_or_default()
    };

    {
        let mut voicings = state.pipe_voicings.write();
        voicings.clear();
        for v in &saved {
            voicings.insert((v.stop_id, v.pipe_num), (v.volume_db, v.pitch_cents));
        }
    }

    for v in &saved {
        state.send_audio_command(AudioCommand::SetPipeVoicing {
            stop_id: v.stop_id,
            pipe_num: v.pipe_num,
            volume_db: v.volume_db,
            pitch_cents: v.pitch_cents,
        });
    }

    Ok(saved.len())
}

/// Reset voicing for a single pipe
#[tauri::command]
pub fn reset_pipe_voicing(state: State<AppState>, stop_id: u32, pipe_num: u32) -> Result<(), String> {
    state.pipe_voicings.write().remove(&(stop_id, pipe_num));
    state.send_audio_command(AudioCommand::SetPipeVoicing {
        stop_id, pipe_num,
        volume_db: 0.0,
        pitch_cents: 0.0,
    });
    Ok(())
}

/// Reset voicing for all pipes of a stop
#[tauri::command]
pub fn reset_stop_voicing(state: State<AppState>, stop_id: u32) -> Result<(), String> {
    let pipes_to_reset: Vec<u32> = {
        let voicings = state.pipe_voicings.read();
        voicings.keys()
            .filter(|&&(s, _)| s == stop_id)
            .map(|&(_, p)| p)
            .collect()
    };
    {
        let mut voicings = state.pipe_voicings.write();
        for &p in &pipes_to_reset {
            voicings.remove(&(stop_id, p));
        }
    }
    for &p in &pipes_to_reset {
        state.send_audio_command(AudioCommand::SetPipeVoicing {
            stop_id, pipe_num: p,
            volume_db: 0.0,
            pitch_cents: 0.0,
        });
    }
    Ok(())
}

/// Route een divisie naar een vrije set fysieke output-kanalen.
/// `channels`: lijst kanaalindices (leeg = standaard voorste paar 0/1). Opeenvolgende
/// kanalen vormen (L,R)-paren; een los laatste kanaal krijgt mono. Zo kan een klavier
/// uit 1 t/m alle beschikbare kanalen klinken (eigen ideale mapping).
#[tauri::command]
pub fn set_division_output_channels(state: State<AppState>, division: String, channels: Vec<u16>) -> Result<(), String> {
    set_division_output_channels_inner(&state, &division, channels)
}

/// Kern van set_division_output_channels, ook aanroepbaar zonder Tauri-State
/// (test-API: POST /division_channels).
pub fn set_division_output_channels_inner(state: &AppState, division: &str, channels: Vec<u16>) -> Result<(), String> {
    let organ = state.loaded_organ_info.read();
    if let Some(ref o) = *organ {
        if let Some(idx) = o.divisions.iter().position(|d| d.name == division) {
            {
                let mut chans = state.division_output_channels.write();
                if idx < chans.len() { chans[idx] = channels.clone(); }
            }
            // Handmatige wijziging terwijl een profiel-kanaaloverride actief is:
            // de override mee-updaten, anders draait de eerstvolgende orgel-load
            // de wijziging stilletjes terug naar de oude profielwaarde.
            {
                let mut ov = state.profile_channel_override.write();
                if let Some(ref mut entries) = *ov {
                    if let Some(e) = entries.iter_mut().find(|(name, _)| name == division) {
                        e.1 = channels.clone();
                    } else {
                        entries.push((division.to_string(), channels.clone()));
                    }
                }
            }
            state.send_audio_command(AudioCommand::SetDivisionOutputChannels {
                division_index: idx as u8,
                channels,
            });
            return Ok(());
        }
    }
    Err(format!("Division not found: {}", division))
}

/// Lees alle divisie output-kanaal instellingen (in volgorde van de divisies)
#[tauri::command]
pub fn get_division_output_channels(state: State<AppState>) -> Vec<Vec<u16>> {
    state.division_output_channels.read().clone()
}

/// Zet (of wis) de kanaal-override van het actieve uitvoerprofiel:
/// lijst van (divisienaam, kanaallijst). Wordt door de frontend gepusht bij
/// het wisselen van uitvoerprofiel (speakers ↔ hoofdtelefoon) en bij opstart.
/// - `Some(..)`: direct toepassen op de geladen divisies; elke volgende
///   `apply_saved_output_channels` (orgel-load/audio-wissel) past hem opnieuw
///   als laatste toe, bovenop de per-orgel opgeslagen routing.
/// - `None`: override weg; de per-orgel opgeslagen routing komt terug (divisies
///   zonder opgeslagen entry vallen terug op het standaard voorste paar).
#[tauri::command]
pub fn set_profile_channel_override(state: State<AppState>, channels: Option<Vec<(String, Vec<u16>)>>) -> Result<(), String> {
    info!("Profiel-kanaaloverride: {}",
        channels.as_ref().map(|c| format!("{} divisies", c.len())).unwrap_or_else(|| "gewist".to_string()));
    *state.profile_channel_override.write() = channels.clone();

    let organ = state.loaded_organ_info.read();
    let Some(ref o) = *organ else { return Ok(()); };

    match channels {
        Some(ov) => {
            for (division, chans) in &ov {
                if let Some(idx) = o.divisions.iter().position(|d| &d.name == division) {
                    {
                        let mut all = state.division_output_channels.write();
                        if idx < all.len() { all[idx] = chans.clone(); }
                    }
                    state.send_audio_command(AudioCommand::SetDivisionOutputChannels {
                        division_index: idx as u8,
                        channels: chans.clone(),
                    });
                }
            }
        }
        None => {
            // Terug naar de per-orgel opgeslagen routing voor ALLE divisies
            // (ook divisies die alleen door de override waren gezet).
            let saved_routing = {
                let organ_id = state.current_organ_id.read().clone();
                let lib = state.organ_library.read();
                organ_id.and_then(|id| lib.settings.get(&id).map(|s| s.division_output_pairs.clone()))
                    .unwrap_or_default()
            };
            for (idx, d) in o.divisions.iter().enumerate() {
                let target = saved_routing.iter().find(|e| e.division == d.name)
                    .map(|e| e.channels.clone().unwrap_or_else(|| migrate_pair_to_channels(e.pair)))
                    .unwrap_or_default();
                {
                    let mut all = state.division_output_channels.write();
                    if idx < all.len() { all[idx] = target.clone(); }
                }
                state.send_audio_command(AudioCommand::SetDivisionOutputChannels {
                    division_index: idx as u8,
                    channels: target,
                });
            }
        }
    }
    Ok(())
}

/// C/Cis-lade spreiding: globale parameters (sterkte 0..1, afval-met-toonhoogte 0..1, swap).
#[tauri::command]
pub fn set_ccis_spread(state: State<AppState>, strength: f32, falloff: f32, swap: bool) -> Result<(), String> {
    *state.ccis_spread.write() = (strength, falloff, swap);
    state.send_audio_command(AudioCommand::SetCcisSpread { strength, falloff, swap });
    Ok(())
}

/// Lees de globale C/Cis-spreiding parameters: (sterkte, afval, swap).
#[tauri::command]
pub fn get_ccis_spread(state: State<AppState>) -> (f32, f32, bool) {
    *state.ccis_spread.read()
}

/// Zet de C/Cis-lade spreiding aan/uit voor één divisie.
#[tauri::command]
pub fn set_division_ccis(state: State<AppState>, division: String, enabled: bool) -> Result<(), String> {
    let organ = state.loaded_organ_info.read();
    if let Some(ref o) = *organ {
        if let Some(idx) = o.divisions.iter().position(|d| d.name == division) {
            {
                let mut en = state.division_ccis_enabled.write();
                if idx < en.len() { en[idx] = enabled; }
            }
            state.send_audio_command(AudioCommand::SetDivisionCcis { division_index: idx as u8, enabled });
            return Ok(());
        }
    }
    Err(format!("Division not found: {}", division))
}

/// Lees de C/Cis-aan/uit per divisie (in volgorde van de divisies).
#[tauri::command]
pub fn get_division_ccis(state: State<AppState>) -> Vec<bool> {
    state.division_ccis_enabled.read().clone()
}

/// Kanaalaantal van de GEOPENDE audio-stream (= StatusDto.channels).
///
/// Vóór 0.7.37 enumereerde dit een verse cpal-host en las het
/// `default_output_config()` van het apparaat. Twee problemen: (1) onder ASIO
/// laadt een verse host de driver opnieuw (ASIOInit) en bij het opruimen van
/// dat tijdelijke Device volgt ASIOExit — de SPELENDE driver wordt geëxit
/// (asio-sys houdt `loaded_driver` per Asio-instantie) — dat gebeurde bij elk
/// extra scherm en bij de knop "Ondersteunde sample rates"; (2) faalde de
/// enumeratie, dan bleef de UI op 2 kanalen staan, terwijl de stream (ASIO:
/// alle driver-uitgangen) al lang meer had. Nu: de waarde van de lopende
/// stream, zonder enumeratie. Zonder player: 0 — de UI neemt binnen 100 ms de
/// waarde uit get_status over zodra er wél een stream is.
#[tauri::command]
pub fn query_audio_channel_count(state: State<AppState>) -> Result<u16, String> {
    // Eén read-guard, direct laten vallen (deadlock-les audit 44/59).
    let live = {
        let g = state.audio_player.read();
        g.as_ref().map(|p| *p.current_channels.read())
    };
    Ok(live.unwrap_or(0))
}

/// Set per-division stereo pan
#[tauri::command]
pub fn set_division_pan(state: State<AppState>, division: String, pan: f32) -> Result<(), String> {
    let organ = state.loaded_organ_info.read();
    if let Some(ref o) = *organ {
        if let Some(idx) = o.divisions.iter().position(|d| d.name == division) {
            let clamped = pan.clamp(-1.0, 1.0);
            state.send_audio_command(AudioCommand::SetDivisionPan {
                division_index: idx as u8,
                pan: clamped,
            });
            // Per-orgel opslag-spiegel (verzameld door do_save_organ_settings).
            state.division_pans.write().insert(division.clone(), clamped);
            return Ok(());
        }
    }
    Err(format!("Division not found: {}", division))
}

/// Wire-DTO voor één EQ-band (frontend ↔ backend).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EqBandDto {
    pub enabled: bool,
    /// "peak" | "lowpass" | "highpass" | "bandpass" | "lowshelf" | "highshelf"
    pub band_type: String,
    pub freq: f32,
    pub gain_db: f32,
    /// Bandbreedte in octaven
    pub bandwidth: f32,
    /// None = alle kanalen; anders 0-based fysiek uitgangskanaal
    pub channel: Option<u8>,
}

fn eq_bands_to_specs(bands: &[EqBandDto]) -> Vec<vpo_audio::EqBandSpec> {
    bands.iter().map(|b| vpo_audio::EqBandSpec {
        enabled: b.enabled,
        band_type: vpo_audio::EqBandType::from_str(&b.band_type),
        freq: b.freq,
        gain_db: b.gain_db,
        bandwidth_oct: b.bandwidth,
        channel: b.channel,
    }).collect()
}

/// Vrije multi-band EQ (GrandOrgue-stijl): per band type, frequentie, gain,
/// bandbreedte en doelkanaal (null = alle kanalen).
#[tauri::command]
pub fn set_eq_bands(state: State<AppState>, enabled: bool, bands: Vec<EqBandDto>) -> Result<(), String> {
    state.send_audio_command(AudioCommand::SetEqBands {
        enabled,
        bands: eq_bands_to_specs(&bands),
    });
    // Onthoud voor per-orgel opslag (save_current_organ_settings leest dit veld).
    *state.eq_settings.write() = Some(library::EqSettingsSaved {
        enabled,
        bands: bands.into_iter().map(|b| library::EqBandSaved {
            enabled: b.enabled,
            band_type: b.band_type,
            freq: b.freq,
            gain_db: b.gain_db,
            bandwidth: b.bandwidth,
            channel: b.channel,
        }).collect(),
        ..Default::default()
    });
    Ok(())
}

/// Legacy 3-band EQ (oude frontend/test-API): omgezet naar drie vrije banden.
#[tauri::command]
pub fn set_parametric_eq(state: State<AppState>, enabled: bool, low_freq: f32, low_gain: f32, mid_freq: f32, mid_gain: f32, mid_q: f32, high_freq: f32, high_gain: f32) -> Result<(), String> {
    // Q → bandbreedte in octaven: BW ≈ (2/ln2) * asinh(1/(2Q)).
    let q = mid_q.max(0.05);
    let bw = (2.0 / std::f32::consts::LN_2) * (1.0 / (2.0 * q)).asinh();
    let bands = vec![
        EqBandDto { enabled: true, band_type: "lowshelf".into(), freq: low_freq, gain_db: low_gain, bandwidth: 1.0, channel: None },
        EqBandDto { enabled: true, band_type: "peak".into(), freq: mid_freq, gain_db: mid_gain, bandwidth: bw, channel: None },
        EqBandDto { enabled: true, band_type: "highshelf".into(), freq: high_freq, gain_db: high_gain, bandwidth: 1.0, channel: None },
    ];
    set_eq_bands(state, enabled, bands)
}

/// Bewaar de volledige reverb-configuratie van het huidige orgel voor per-orgel opslag.
/// Apart van de losse set_reverb_*-audio-commando's: de frontend roept dit aan met de
/// complete configuratie zodat save_current_organ_settings die kan opslaan. Stuurt zelf
/// GEEN audio (dat doen de bestaande set_reverb_*-commando's al).
#[tauri::command]
pub fn persist_reverb_config(state: State<AppState>, reverb_type: String, preset: Option<u8>, rt60: f32, pre_delay_ms: f32, damping: f32, room_size: f32, mix: f32, ir_path: Option<String>) -> Result<(), String> {
    *state.reverb_settings.write() = Some(library::ReverbSettingsSaved {
        reverb_type, mix, algorithmic_preset: preset, rt60, pre_delay_ms, damping, room_size, ir_path,
    });
    Ok(())
}

/// Switch between convolution and algorithmic reverb
#[tauri::command]
pub fn set_reverb_type(state: State<AppState>, algorithmic: bool) -> Result<(), String> {
    state.send_audio_command(AudioCommand::SetReverbType { algorithmic });
    Ok(())
}

/// Set temperament: 12 cent offsets per note class [C..B] + global fine-tuning.
/// `name`/`a4_hz` zijn optioneel en dienen alleen voor per-orgel opslag (UI-herstel van de
/// gekozen stemming); de audio gebruikt note_offsets + fine_tune + retune.
/// `retune` (default false = Origineel): hertemperen per pijp vanaf de gemeten
/// toonhoogte (GrandOrgue-semantiek voor elke niet-"Original" stemming); false =
/// "Origineel (zoals opgenomen)" — alleen PitchTuning + offsets.
#[tauri::command]
pub fn set_temperament(state: State<AppState>, note_offsets: Vec<f32>, fine_tune: f32, name: Option<String>, a4_hz: Option<f32>, retune: Option<bool>) -> Result<(), String> {
    let mut offsets = [0.0f32; 12];
    for (i, &v) in note_offsets.iter().take(12).enumerate() {
        offsets[i] = v;
    }
    // Niet opgegeven = Origineel (geen hertemperen); de UI geeft retune expliciet mee.
    let retune_on = retune.unwrap_or(false);
    state.send_audio_command(AudioCommand::SetTemperament { note_offsets: offsets, fine_tune, retune: retune_on });
    // Onthoud voor per-orgel opslag (save_current_organ_settings leest dit veld).
    *state.temperament_settings.write() = Some(library::TemperamentSettingsSaved {
        name: name.unwrap_or_default(),
        custom_cents: Some(offsets),
        fine_tune_cents: fine_tune,
        a4_hz: a4_hz.unwrap_or(440.0),
        retune: Some(retune_on),
    });
    Ok(())
}

/// Export current scanned directory as .organ definition file
#[tauri::command]
pub fn export_organ_file(state: State<AppState>, directory: String, output_path: String) -> Result<(), String> {
    let path = std::path::Path::new(&directory);
    if !path.exists() || !path.is_dir() {
        return Err(format!("Map niet gevonden: {}", directory));
    }
    let organ = scan_samples_directory(path, &path.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "Orgel".to_string()))
        .map_err(|e| format!("Scan mislukt: {}", e))?;

    let out = std::path::Path::new(&output_path);
    organ.export_organ_file(out)
        .map_err(|e| format!("Export mislukt: {}", e))?;

    info!("Exported organ definition to: {}", output_path);
    Ok(())
}

/// Load an impulse response WAV file for convolution reverb
#[tauri::command]
pub fn load_impulse_response(state: State<AppState>, path: String) -> Result<(), String> {
    let ir_path = std::path::PathBuf::from(&path);
    if !ir_path.exists() {
        return Err(format!("IR file not found: {}", path));
    }
    // Decode + FFT-partitionering hier (command-thread), niet in de audio-
    // callback; de callback plugt het kant-en-klare object alleen in.
    let target_rate = state.audio_player.read().as_ref()
        .map(|p| *p.sample_rate.read())
        .filter(|&r| r > 0)
        .unwrap_or(44100);
    let rev = vpo_audio::ConvolutionReverb::from_wav(&ir_path, vpo_audio::ConvolutionReverb::aanbevolen_partitie(), target_rate)
        .map_err(|e| format!("IR laden mislukt: {}", e))?;
    state.send_audio_command(AudioCommand::LoadImpulseResponse(Box::new(rev)));
    info!("Loaded impulse response: {} ({} Hz)", path, target_rate);
    Ok(())
}

/// Enable or disable tremulant for a division
/// Finds all stops in the division that have tremulant samples and toggles them
#[tauri::command]
pub fn set_tremulant(state: State<AppState>, division_name: String, active: bool) -> Result<(), String> {
    do_set_tremulant(&state, &division_name, active).map(|_| ())
}

/// Kern van [`set_tremulant`], ook gebruikt door de test-API (POST /tremulant).
/// Geeft het aantal registers terug waarvan de tremulant-OPNAMEN geschakeld
/// zijn (0 = divisie bestaat, maar heeft geen trem-samples — dan doet alleen
/// de synth-LFO het werk).
pub fn do_set_tremulant(state: &AppState, division_name: &str, active: bool) -> Result<usize, String> {
    let organ = state.loaded_organ_info.read();

    if let Some(ref o) = *organ {
        if let Some(div) = o.divisions.iter().find(|d| d.name == division_name) {
            // Spiegel voor de afstandsbediening (GET /state) — ook zonder
            // trem-samples: de Console-stand is dan tóch 'aan' via de LFO.
            state.tremulant_live.write().insert(division_name.to_string(), active);
            let stop_ids: Vec<u32> = div.stops.iter()
                .filter(|s| s.has_tremulant)
                .map(|s| s.internal_stop_id)
                .collect();

            if stop_ids.is_empty() {
                info!("No tremulant samples available for division '{}'", division_name);
                return Ok(0);
            }

            let n = stop_ids.len();
            info!("Tremulant {} for division '{}': {} stops",
                  if active { "ON" } else { "OFF" }, division_name, n);

            state.send_audio_command(AudioCommand::SetTremulant { stop_ids, active });
            return Ok(n);
        }
    }

    Err(format!("Division not found: {}", division_name))
}

#[tauri::command]
pub fn get_status(state: State<AppState>) -> Result<StatusDto, String> {
    let organ = state.loaded_organ_info.read();
    let usb_midi_connected = *state.midi_connected.read();
    let ble_midi_connected = state.ble_midi.read().as_ref()
        .map(|m| !m.connected_devices().is_empty())
        .unwrap_or(false);
    let peaks = state.peak_meters();
    // Alles uit ÉÉN read-guard halen en die direct laten vallen. De oude code
    // nam audio_player.read() twee keer binnen dezelfde struct-expressie
    // (audio_running + voice_count): met parking_lot's eerlijke RwLock blokkeert
    // de tweede read zodra er een writer wacht — en dat is precies de
    // audio-wissel → recursieve read = permanente deadlock op de UI-thread.
    let (audio_running, voice_count, sample_rate, audio_host, audio_device, buffer_frames, channels) = {
        let guard = state.audio_player.read();
        match guard.as_ref() {
            Some(p) => (
                true,
                p.voice_count(),
                *p.sample_rate.read(),
                p.current_host.read().clone(),
                p.current_device.read().clone(),
                *p.current_buffer_frames.read(),
                *p.current_channels.read(),
            ),
            None => (false, 0, 0, String::new(), String::new(), 0, 0),
        }
    };

    Ok(StatusDto {
        audio_running,
        midi_connected: usb_midi_connected || ble_midi_connected,
        organ_loaded: organ.is_some(),
        voice_count,
        peak_left: peaks.0,
        peak_right: peaks.1,
        sample_rate,
        audio_host,
        audio_device,
        buffer_frames,
        channels,
        polyphony: crate::audio::polyphony_target(),
        render_load: crate::audio::render_load().0,
        render_peak: crate::audio::render_load().1,
        render_overloads: crate::audio::render_overload_count(),
        rt_drops: crate::state::rt_drop_count(),
        stereo_samples: vpo_sampler::stereo_loading(),
        midi_archiving: state.midi_archive.archiving.load(std::sync::atomic::Ordering::Relaxed),
        audio_ready: state.audio_ready.load(std::sync::atomic::Ordering::Relaxed),
        asio_restart_advice: state.asio_restart_advice.read().as_ref().map(|a| a.device.clone()),
        backend_reloads: state.backend_reloads.load(std::sync::atomic::Ordering::Relaxed),
        render_frames: crate::audio::render_frames_now(),
        mix_voice_load: crate::audio::render_pass_load().1,
    })
}

// ============ MIDI File Playback ============

#[derive(Debug, Serialize)]
pub struct MidiPlayerStatus {
    pub state: String,         // "stopped" | "playing" | "paused"
    pub position_ms: u64,
    pub duration_ms: u64,
    pub loaded: bool,
}

#[tauri::command]
pub fn midi_play_file(state: State<AppState>, path: String) -> Result<(), String> {
    let player = state.midi_player.read();
    let p = player.as_ref().ok_or_else(|| "MIDI player niet beschikbaar".to_string())?;
    p.load_and_play(std::path::Path::new(&path))?;
    Ok(())
}

#[tauri::command]
pub fn midi_stop_playback(state: State<AppState>) -> Result<(), String> {
    if let Some(p) = state.midi_player.read().as_ref() {
        p.stop();
        // Stuur all-notes-off naar audio
        state.send_audio_command(crate::audio::AudioCommand::AllNotesOff);
        // Afgespeelde noten zitten óók in held_notes (speler loopt door de
        // normale MIDI-verwerking): opruimen tegen spooktoetsen/hangers.
        state.held_notes.write().clear();
    }
    Ok(())
}

#[tauri::command]
pub fn midi_pause_playback(state: State<AppState>) -> Result<(), String> {
    if let Some(p) = state.midi_player.read().as_ref() {
        p.pause();
    }
    Ok(())
}

#[tauri::command]
pub fn midi_resume_playback(state: State<AppState>) -> Result<(), String> {
    if let Some(p) = state.midi_player.read().as_ref() {
        p.resume();
    }
    Ok(())
}

#[tauri::command]
pub fn midi_seek(state: State<AppState>, position_ms: u64) -> Result<(), String> {
    if let Some(p) = state.midi_player.read().as_ref() {
        p.seek_to(position_ms * 1000);
        // Veiligheid: stop hangende noten op huidige positie
        state.send_audio_command(crate::audio::AudioCommand::AllNotesOff);
        // Zie midi_stop_playback: bijbehorende held_notes ook wissen.
        state.held_notes.write().clear();
    }
    Ok(())
}

#[tauri::command]
pub fn midi_set_speed(state: State<AppState>, speed: f32) -> Result<(), String> {
    if let Some(p) = state.midi_player.read().as_ref() {
        p.set_speed(speed);
    }
    Ok(())
}

#[tauri::command]
pub fn midi_player_status(state: State<AppState>) -> Result<MidiPlayerStatus, String> {
    let player = state.midi_player.read();
    let p = match player.as_ref() {
        Some(p) => p,
        None => return Ok(MidiPlayerStatus {
            state: "stopped".to_string(),
            position_ms: 0,
            duration_ms: 0,
            loaded: false,
        }),
    };
    let s = match p.state() {
        vpo_midi::PlayerState::Stopped => "stopped",
        vpo_midi::PlayerState::Playing => "playing",
        vpo_midi::PlayerState::Paused => "paused",
    };
    Ok(MidiPlayerStatus {
        state: s.to_string(),
        position_ms: p.position_micros() / 1000,
        duration_ms: p.duration_micros() / 1000,
        loaded: p.is_loaded(),
    })
}

/// Lijst van sample rates die het ACTUEEL gebruikte audio-device ondersteunt.
/// Onder ASIO wordt bewust NIET geënumereerd (een verse cpal-host exit de
/// spelende driver — zie query_audio_channel_count); daar geldt de rate uit het
/// ASIO-configuratiepaneel, dus geven we de lopende rate terug.
#[tauri::command]
pub fn query_supported_sample_rates(state: State<AppState>) -> Result<Vec<u32>, String> {
    use cpal::traits::{DeviceTrait, HostTrait};
    let (host_name, device_name, live_rate) = {
        let ap = state.audio_player.read();
        match ap.as_ref() {
            Some(p) => (p.current_host.read().clone(), p.current_device.read().clone(), *p.sample_rate.read()),
            None => (String::new(), String::new(), 0u32),
        }
    };
    if host_name.eq_ignore_ascii_case("asio") {
        return Ok(if live_rate > 0 { vec![live_rate] } else { Vec::new() });
    }
    let host = crate::audio::resolve_host(if host_name.is_empty() { None } else { Some(&host_name) });
    let device = (!device_name.is_empty())
        .then(|| {
            host.output_devices().ok().and_then(|mut devs| {
                devs.find(|d| d.name().map(|n| n == device_name).unwrap_or(false))
            })
        })
        .flatten()
        .or_else(|| host.default_output_device())
        .ok_or_else(|| "Geen output audio-device gevonden".to_string())?;

    let configs = device.supported_output_configs()
        .map_err(|e| format!("Kan ondersteunde configs niet ophalen: {}", e))?;

    let mut rates: std::collections::BTreeSet<u32> = std::collections::BTreeSet::new();
    for cfg in configs {
        let min = cfg.min_sample_rate().0;
        let max = cfg.max_sample_rate().0;
        // Voeg gangbare rates toe die binnen min..=max vallen
        for &candidate in &[44100u32, 48000, 88200, 96000, 176400, 192000] {
            if candidate >= min && candidate <= max {
                rates.insert(candidate);
            }
        }
        // Plus de min/max zelf voor edge-cases
        rates.insert(min);
        rates.insert(max);
    }

    Ok(rates.into_iter().collect())
}

/// Koppel omschakelen (kern van het Tauri-commando toggle_coupler; ook voor de
/// afstandsbediening). Controleert eerst of de koppel bestaat, zodat een
/// willekeurige string nooit in active_couplers belandt.
pub fn toggle_coupler_inner(state: &AppState, coupler_id: &str) -> Result<bool, String> {
    let exists = state.loaded_organ_info.read().as_ref()
        .and_then(|o| o.couplers.as_ref())
        .map(|list| list.iter().any(|c| c.id == coupler_id))
        .unwrap_or(false);
    if !exists {
        return Err(format!("Onbekende koppel: {}", coupler_id));
    }
    info!("Toggle coupler: {}", coupler_id);
    let couplers_before: Vec<String> = state.active_couplers.read().clone();
    let active = state.toggle_coupler(coupler_id);
    state.note_manual_change(coupler_id);

    // Update coupler active state in organ info
    {
        let mut organ = state.loaded_organ_info.write();
        if let Some(ref mut o) = *organ {
            if let Some(ref mut couplers) = o.couplers {
                if let Some(c) = couplers.iter_mut().find(|c| c.id == coupler_id) {
                    c.active = active;
                }
            }
        }
    }

    // Stemmen synchroniseren met de nieuwe koppelstand: weggevallen routes
    // loslaten (anders hangers bij ingedrukte toetsen), nieuwe routes meeklinken.
    let couplers_after: Vec<String> = state.active_couplers.read().clone();
    state.sync_coupler_voices(&couplers_before, &couplers_after);

    Ok(active)
}

/// Toggle a coupler on/off
#[tauri::command]
pub fn toggle_coupler(state: State<AppState>, coupler_id: String) -> Result<bool, String> {
    toggle_coupler_inner(&state, &coupler_id)
}

/// Set all active couplers at once (for preset recall)
#[tauri::command]
pub fn set_active_couplers(state: State<AppState>, coupler_ids: Vec<String>) -> Result<(), String> {
    set_active_couplers_inner(&state, coupler_ids)
}

/// Kern van set_active_couplers (setzer/General Cancel), ook voor de test-API
/// (POST /couplers/set).
pub fn set_active_couplers_inner(state: &AppState, coupler_ids: Vec<String>) -> Result<(), String> {
    info!("set_active_couplers: {:?}", coupler_ids);
    let couplers_before: Vec<String> = state.active_couplers.read().clone();
    state.set_active_couplers(coupler_ids.clone());
    // Alle betrokken koppels zijn vanaf nu handmatig (trede-claim vervalt) —
    // ook koppels die de preset bevat én de trede al had geclaimd.
    for id in couplers_before.iter().chain(coupler_ids.iter()) {
        state.note_manual_change(id);
    }
    // Setzer/GC: de UI zet eerst de registers en dan de koppels; pas hier kan
    // de laatste (koppel-)claim verdwenen zijn (0.7.44, review P1).
    state.crescendo_herstart_indien_claims_leeg();

    // Update coupler active states in organ info
    {
        let mut organ = state.loaded_organ_info.write();
        if let Some(ref mut o) = *organ {
            if let Some(ref mut couplers) = o.couplers {
                for c in couplers.iter_mut() {
                    c.active = coupler_ids.contains(&c.id);
                }
            }
        }
    }

    // Stemmen synchroniseren met de nieuwe koppelstand (preset/crescendo vanuit
    // de UI): weggevallen routes loslaten — anders blijven gekoppelde stemmen
    // hangen wanneer een preset of trap de koppels wisselt met ingedrukte toetsen.
    state.sync_coupler_voices(&couplers_before, &coupler_ids);

    Ok(())
}

// ============ Swell Box (Zwelkast) Commands ============

/// Set division volume from UI (LED slider click)
#[tauri::command]
pub fn set_division_volume(state: State<AppState>, division: String, volume: f32) -> Result<(), String> {
    let organ = state.loaded_organ_info.read();
    if let Some(ref o) = *organ {
        if let Some(idx) = o.divisions.iter().position(|d| d.name == division) {
            state.set_division_gain(idx as u8, volume.clamp(0.0, 1.0));
            return Ok(());
        }
    }
    Err(format!("Division not found: {}", division))
}

/// Get all division volumes (for LED display polling)
#[tauri::command]
pub fn get_division_volumes(state: State<AppState>) -> Vec<DivisionVolumeDto> {
    let organ = state.loaded_organ_info.read();
    let gains = state.get_division_gains();
    match *organ {
        Some(ref o) => o.divisions.iter().enumerate().map(|(i, d)| {
            DivisionVolumeDto {
                division: d.name.clone(),
                volume: gains.get(i).copied().unwrap_or(1.0),
            }
        }).collect(),
        None => Vec::new(),
    }
}

/// Configure tremulant LFO for a division (fallback when no trem samples)
#[tauri::command]
pub fn set_tremulant_lfo(state: State<AppState>, division: String, active: bool, rate: f32, amp_depth: f32, pitch_depth: f32) -> Result<(), String> {
    let organ = state.loaded_organ_info.read();
    if let Some(ref o) = *organ {
        if let Some(idx) = o.divisions.iter().position(|d| d.name == division) {
            // Spiegel voor de afstandsbediening (GET /state).
            state.tremulant_live.write().insert(division.clone(), active);
            state.send_audio_command(AudioCommand::SetTremulantLFO {
                division_index: idx as u8,
                active,
                rate: rate.clamp(3.0, 10.0),
                amp_depth: amp_depth.clamp(0.0, 0.3),
                pitch_depth: pitch_depth.clamp(0.0, 50.0),
            });
            return Ok(());
        }
    }
    Err(format!("Division not found: {}", division))
}

/// Bewaar de tremulant-LFO-config van een divisie voor per-orgel opslag (beschikbaarheid +
/// parameters; NIET de live aan/uit-stand). Stuurt zelf geen audio — set_tremulant_lfo doet dat.
#[tauri::command]
pub fn persist_tremulant_config(state: State<AppState>, division: String, enabled: bool, rate: f32, amp_depth: f32, pitch_depth: f32) -> Result<(), String> {
    state.division_tremulants.write().insert(division, (enabled, rate, amp_depth, pitch_depth));
    Ok(())
}

/// Publiceer de indeling van de afstandsbediening (welke divisies/registers/
/// koppels en welke onderdelen het externe scherm toont). Alleen het
/// HOOFDVENSTER schrijft dit (single writer, zoals de andere gedeelde
/// UI-voorkeuren); de waarde wordt met de orgel-instellingen bewaard.
/// Mirror-only: er gaat geen audio-commando uit.
#[tauri::command]
pub fn set_remote_layout(state: State<AppState>, layout: library::RemoteLayoutSaved) -> Result<(), String> {
    set_remote_layout_inner(&state, layout);
    Ok(())
}

/// Kern van set_remote_layout — ook gebruikt door de test-API.
pub fn set_remote_layout_inner(state: &AppState, layout: library::RemoteLayoutSaved) {
    state.set_remote_layout_mirror(Some(layout));
}

/// De diepten die bij een karakter horen: (stoot, kanaal, doffer, verschil,
/// tongwerk apart). Neutraal = magazijnbalg met ventilator: alles half zo
/// diep en een kort, wijd kanaal. Hollands = spaanbalgen en lange kanalen.
pub fn wind_karakter_waarden(karakter: u8) -> (f32, f32, f32, f32, bool) {
    match karakter {
        0 => (0.5, 0.2, 0.3, 0.5, true),
        1 => (1.2, 0.8, 0.7, 1.0, true),
        _ => (1.2, 0.8, 0.7, 1.0, true),
    }
}

/// Van de opgeslagen groepsconfig (schuiven in procenten) naar wat de
/// audiothread wil (fracties). Bij karakter 'eigen' tellen de eigen diepten,
/// anders die van het karakter.
pub fn wind_instelling_van(cfg: &library::WindGroupConfigSaved) -> vpo_audio::WindInstelling {
    let (stoot, kanaal, doffer, verschil, tongwerk) = if cfg.karakter == 2 {
        (cfg.stoot, cfg.kanaal, cfg.doffer, cfg.verschil, cfg.tongwerk_apart)
    } else {
        wind_karakter_waarden(cfg.karakter)
    };
    vpo_audio::WindInstelling {
        enabled: cfg.enabled,
        reservoir_size: cfg.reservoir_size,
        damping: cfg.damping,
        max_sag: cfg.max_sag / 100.0,
        stoot: stoot.clamp(0.0, 2.0),
        kanaal: kanaal.clamp(0.0, 1.0),
        doffer: doffer.clamp(0.0, 1.0),
        verschil: verschil.clamp(0.0, 1.0),
        tongwerk_apart: tongwerk,
    }
}

/// Bewaar de wind-model-config van een wind-groep voor per-orgel opslag. Stuurt zelf geen audio
/// — set_wind_model doet dat. `config.group` = wind-groep-index (0..31).
///
/// Let op: `max_sag` staat hier in PROCENT (zoals de schuif in het scherm);
/// `wind_instelling_van` deelt door 100 voor de audiothread.
#[tauri::command]
pub fn persist_wind_group_config(state: State<AppState>, config: library::WindGroupConfigSaved) -> Result<(), String> {
    state.wind_group_configs.write().insert(config.group, config);
    Ok(())
}

/// Windmodel van de groep waarin deze divisie zit instellen (audio).
#[tauri::command]
pub fn set_wind_model(state: State<AppState>, division: String, config: library::WindGroupConfigSaved) -> Result<(), String> {
    let organ = state.loaded_organ_info.read();
    if let Some(ref o) = *organ {
        if let Some(idx) = o.divisions.iter().position(|d| d.name == division) {
            // Vertaal divisie-index naar groep-index via de huidige toewijzing.
            let group = state.division_wind_groups.read()
                .get(idx).copied().unwrap_or(idx as u8);
            state.send_audio_command(AudioCommand::SetWindModel {
                division_index: group, // audio thread interpreteert dit als groep-index
                instelling: wind_instelling_van(&config),
            });
            return Ok(());
        }
    }
    Err(format!("Division not found: {}", division))
}

/// Wijs een divisie aan een wind-groep toe (0..31).
/// Default identity: divisie i hoort bij groep i.
/// Twee divisies met dezelfde groep delen één wind-reservoir.
#[tauri::command]
pub fn set_division_wind_group(state: State<AppState>, division: String, group: u8) -> Result<(), String> {
    let organ = state.loaded_organ_info.read();
    if let Some(ref o) = *organ {
        if let Some(idx) = o.divisions.iter().position(|d| d.name == division) {
            let clamped = group.min(31);
            {
                let mut wg = state.division_wind_groups.write();
                if idx < wg.len() {
                    wg[idx] = clamped;
                }
            }
            state.send_audio_command(AudioCommand::SetDivisionWindGroup {
                division_index: idx as u8,
                group: clamped,
            });
            return Ok(());
        }
    }
    Err(format!("Division not found: {}", division))
}

/// Lees per-divisie wind-groep toewijzing (in volgorde van organ.divisions).
#[tauri::command]
pub fn get_division_wind_groups(state: State<AppState>) -> Vec<u8> {
    state.division_wind_groups.read().clone()
}

/// Lees het aantal actieve wind-groepen (1..8).
#[tauri::command]
pub fn get_num_wind_groups(state: State<AppState>) -> u8 {
    *state.num_wind_groups.read()
}

/// Stel het aantal actieve wind-groepen in (1..8).
/// Divisies die boven het max liggen worden teruggeklemd naar de hoogste geldige groep.
#[tauri::command]
pub fn set_num_wind_groups(state: State<AppState>, count: u8) -> Result<(), String> {
    let clamped = count.clamp(1, 8);
    *state.num_wind_groups.write() = clamped;
    // Clamp bestaande toewijzingen naar het nieuwe max
    let mut wg = state.division_wind_groups.write();
    let max_idx = clamped - 1;
    for slot in wg.iter_mut() {
        if *slot > max_idx {
            *slot = max_idx;
        }
    }
    // Stuur de aangepaste toewijzingen naar de audio thread
    let snapshot: Vec<u8> = wg.clone();
    drop(wg);
    for (i, &grp) in snapshot.iter().enumerate() {
        state.send_audio_command(AudioCommand::SetDivisionWindGroup {
            division_index: i as u8,
            group: grp,
        });
    }
    Ok(())
}

/// Configure swell box parameters for a division
#[tauri::command]
pub fn set_swell_config(state: State<AppState>, division: String, min_db: f32, filter_cutoff: f32) -> Result<(), String> {
    let organ = state.loaded_organ_info.read();
    if let Some(ref o) = *organ {
        if let Some(idx) = o.divisions.iter().position(|d| d.name == division) {
            let mdb = min_db.clamp(-30.0, -5.0);
            let cutoff = filter_cutoff.clamp(200.0, 5000.0);
            state.send_audio_command(AudioCommand::SetSwellConfig {
                division_index: idx as u8,
                min_db: mdb,
                filter_cutoff_closed: cutoff,
            });
            // Per-orgel opslag-spiegel (verzameld door do_save_organ_settings).
            state.division_swell_configs.write().insert(division.clone(), (mdb, cutoff));
            return Ok(());
        }
    }
    Err(format!("Division not found: {}", division))
}

/// Learn swell pedal MIDI binding for a division
#[tauri::command]
pub fn learn_swell_pedal(state: State<AppState>, division: String) -> Result<Option<SwellBindingDto>, String> {
    let organ = state.loaded_organ_info.read();
    let division_index = match *organ {
        Some(ref o) => o.divisions.iter().position(|d| d.name == division),
        None => None,
    };
    drop(organ);

    let idx = division_index.ok_or_else(|| format!("Division not found: {}", division))?;
    info!("Learning swell pedal for {} (index={})", division, idx);

    let result = state.learn_swell_pedal(&division, idx as u8)?;
    Ok(result.map(|b| SwellBindingDto {
        division: b.division_name,
        channel: b.channel,
        cc_num: b.cc_num,
        min_val: b.min_val,
        max_val: b.max_val,
        invert: b.invert,
    }))
}

/// Clear swell pedal MIDI binding for a division
#[tauri::command]
pub fn clear_swell_binding(state: State<AppState>, division: String) {
    info!("Clearing swell binding for {}", division);
    state.clear_swell_binding(&division);
}

/// Get all swell pedal bindings
#[tauri::command]
pub fn get_swell_bindings(state: State<AppState>) -> Vec<SwellBindingDto> {
    state.get_swell_bindings().into_iter().map(|b| SwellBindingDto {
        division: b.division_name,
        channel: b.channel,
        cc_num: b.cc_num,
        min_val: b.min_val,
        max_val: b.max_val,
        invert: b.invert,
    }).collect()
}

/// Zet/wissel het spiegelbeeld (invert) voor de zwelpedaal van één divisie.
#[tauri::command]
pub fn set_swell_invert(state: State<AppState>, division: String, invert: bool) -> Result<(), String> {
    let mut bindings = state.get_swell_bindings();
    let mut found = false;
    for b in bindings.iter_mut() {
        if b.division_name == division { b.invert = invert; found = true; }
    }
    if !found { return Err(format!("Geen zwelbinding voor {}", division)); }
    state.set_swell_bindings_replace(bindings);
    state.zwel_hertoepassen(&division); // meteen hoorbaar, niet pas bij de volgende beweging
    Ok(())
}

/// Zet/wissel het spiegelbeeld (invert) voor de generaal-crescendo pedaal.
#[tauri::command]
pub fn set_crescendo_invert(state: State<AppState>, invert: bool) -> Result<(), String> {
    let mut b = state.crescendo_binding.write();
    match *b {
        Some((ch, cc, mn, mx, _)) => { *b = Some((ch, cc, mn, mx, invert)); Ok(()) }
        None => Err("Nog geen crescendo-binding".to_string()),
    }
}

/// Stel het CC-bereik (min/max pedaalstand) voor de generaal-crescendo handmatig in.
#[tauri::command]
pub fn set_crescendo_range(state: State<AppState>, min_val: u8, max_val: u8) -> Result<(), String> {
    let mut b = state.crescendo_binding.write();
    match *b {
        Some((ch, cc, _, _, invert)) => { *b = Some((ch, cc, min_val.min(127), max_val.min(127), invert)); Ok(()) }
        None => Err("Nog geen crescendo-binding".to_string()),
    }
}

/// Stel het CC-bereik (min/max pedaalstand) voor de zwelpedaal van één divisie handmatig in.
/// De MIDI-thread leest min/max live per CC-bericht, dus de wijziging werkt direct.
#[tauri::command]
pub fn set_swell_range(state: State<AppState>, division: String, min_val: u8, max_val: u8) -> Result<(), String> {
    // Geordend: bij min > max werd de zwel een stapfunctie.
    let (lo, hi) = (min_val.min(max_val).min(127), max_val.max(min_val).min(127));
    let mut bindings = state.get_swell_bindings();
    let mut found = false;
    for b in bindings.iter_mut() {
        if b.division_name == division { b.min_val = lo; b.max_val = hi; found = true; }
    }
    if !found { return Err(format!("Geen zwelbinding voor {}", division)); }
    state.set_swell_bindings_replace(bindings);
    state.zwel_hertoepassen(&division);
    Ok(())
}

/// Lees de crescendo-binding (channel, cc, min, max, invert).
#[tauri::command]
pub fn get_crescendo_binding(state: State<AppState>) -> Option<(u8, u8, u8, u8, bool)> {
    *state.crescendo_binding.read()
}

// ============ Helper functions ============

/// Build a mapping from internal stop_id → division index
/// Used by the audio thread for per-division gain (swell box)
fn build_stop_division_map(divisions: &[DivisionDto]) -> std::collections::HashMap<u32, u8> {
    let mut map = std::collections::HashMap::new();
    for (div_idx, division) in divisions.iter().enumerate() {
        for stop in &division.stops {
            map.insert(stop.internal_stop_id, div_idx as u8);
        }
    }
    map
}

/// Windprofiel per stop (familie + voetmaat, 0.7.51) en per divisie het
/// verbruik van haar "pleno": alle registers op een vierklank een octaaf
/// boven de laagste toets. Dat pleno is de referentie voor "vol werk", zodat
/// een kistorgel en een domorgel bij hún pleno even ver inzakken.
/// Aantal koren van een mixtuur uit de naam: "Mixtuur IV" → 4, "Scherp
/// III-IV" → 4, "Mixtur major 4-5f." → 5, "Sesquialter II" → 2. Niets
/// gevonden → 4 (de gewone mixtuur).
fn koren_uit_naam(naam: &str) -> u8 {
    let mut beste = 0u8;
    for token in naam.split(|c: char| c.is_whitespace() || c == '(' || c == ')' || c == ',' || c == '.') {
        let t = token.trim_matches(|c: char| !c.is_alphanumeric() && c != '-');
        if t.is_empty() { continue; }
        // Romeins: alleen I/V/X, hooguit vier tekens, evt. "III-IV".
        for deel in t.split('-') {
            let u = deel.to_uppercase();
            if !u.is_empty() && u.len() <= 4 && u.chars().all(|c| c == 'I' || c == 'V' || c == 'X') {
                let v = match u.as_str() {
                    "I" => 1, "II" => 2, "III" => 3, "IV" => 4, "V" => 5, "VI" => 6, "VII" => 7, "VIII" => 8, _ => 0,
                };
                beste = beste.max(v);
            }
        }
        // Arabisch met "f" (Duits/Nederlands): "4f", "4-5f", "5F".
        let u = t.to_uppercase();
        if u.ends_with('F') {
            for deel in u.trim_end_matches('F').split('-') {
                if let Ok(v) = deel.parse::<u8>() {
                    if v <= 12 { beste = beste.max(v); }
                }
            }
        }
    }
    if beste == 0 { 4 } else { beste }
}

fn build_stop_wind_profiles(divisions: &[DivisionDto])
    -> (std::collections::HashMap<u32, vpo_audio::StopWindProfiel>, [f32; 32])
{
    let mut map = std::collections::HashMap::new();
    let mut pleno = [0.0f32; 32];
    for (div_idx, division) in divisions.iter().enumerate() {
        // Een pedaal (korte omvang) speelt geen vierklank in het tenor: "vol
        // werk" is daar twee noten in het groot octaaf (C, F, G, c, elk half);
        // een manuaal een vierklank een octaaf boven de laagste toets.
        let omvang = division.stops.iter()
            .map(|s| s.last_midi_note.saturating_sub(s.first_midi_note))
            .max().unwrap_or(0);
        let pedaal = omvang < 40;
        let (intervallen, gewicht): ([i32; 4], f32) = if pedaal { ([0, 5, 7, 12], 0.5) } else { ([12, 16, 19, 24], 1.0) };
        for stop in &division.stops {
            let familie = vpo_audio::familie_van_naam(&stop.name, stop.is_reed);
            let mut voet = vpo_audio::voet_uit_pitch(&stop.pitch);
            let mut koren = 1u8;
            if familie == vpo_audio::PijpFamilie::Mixtuur {
                // Sets zonder voetmaat (GrandOrgue zonder HarmonicNumber, dus
                // ook Friesach) geven "8'" terug; een mixtuur klinkt op de
                // hoogste koorpijp, niet op 8'.
                if voet > 4.0 { voet = if pedaal { 4.0 } else { 2.0 }; }
                koren = koren_uit_naam(&stop.name);
            }
            let profiel = vpo_audio::StopWindProfiel { familie, voet, koren };
            map.insert(stop.internal_stop_id, profiel);
            if div_idx < 32 {
                let laag = stop.first_midi_note.max(24) as i32;
                let hoog = stop.last_midi_note.max(stop.first_midi_note) as i32;
                for interval in intervallen {
                    let noot = (laag + interval).min(hoog).clamp(0, 127) as u8;
                    pleno[div_idx] += gewicht * vpo_audio::verbruik_van_pijp(&profiel, noot);
                }
            }
        }
    }
    (map, pleno)
}

#[cfg(test)]
mod windprofiel_tests {
    use super::koren_uit_naam;

    #[test]
    fn koren_uit_de_naam() {
        assert_eq!(koren_uit_naam("Mixtuur IV"), 4);
        assert_eq!(koren_uit_naam("Scherp III-IV"), 4);
        assert_eq!(koren_uit_naam("HW Mixtur major 4-5f. 2 2/3'"), 5);
        assert_eq!(koren_uit_naam("SW Plein Jeu 4-5f. 2'"), 5);
        assert_eq!(koren_uit_naam("Sesquialter II"), 2);
        assert_eq!(koren_uit_naam("Cornet V"), 5);
        assert_eq!(koren_uit_naam("Mixtuur"), 4);
    }
}

/// Detect reed (tongwerk) stops by name
fn is_reed_stop(name: &str) -> bool {
    let lower = name.to_lowercase();
    // Dutch, German, French, English reed stop names
    lower.contains("trompet") || lower.contains("trumpet") ||
    lower.contains("bazuin") || lower.contains("posaune") ||
    lower.contains("hobo") || lower.contains("oboe") || lower.contains("hautbois") ||
    lower.contains("schalmei") || lower.contains("chalumeau") ||
    lower.contains("cromorne") || lower.contains("cromhorne") || lower.contains("krummhorn") ||
    lower.contains("fagot") || lower.contains("basson") || lower.contains("bassoon") ||
    lower.contains("dulcian") || lower.contains("dulciaan") ||
    lower.contains("ranket") || lower.contains("regal") ||
    lower.contains("bombard") || lower.contains("tuba") ||
    lower.contains("clairon") || lower.contains("clarion") ||
    lower.contains("trompette") || lower.contains("clarinette") || lower.contains("clarinet") ||
    lower.contains("vox humana") || lower.contains("voix humaine") ||
    lower.contains("serpent") || lower.contains("musette")
}

fn get_stop_color(name: &str) -> &'static str {
    let lower = name.to_lowercase();

    if lower.contains("trompet") || lower.contains("bazuin") || lower.contains("hobo") ||
       lower.contains("schalmei") || lower.contains("cromorne") {
        "#d47070"
    } else if lower.contains("fluit") || lower.contains("gedekt") || lower.contains("gedackt") ||
              lower.contains("bourdon") || lower.contains("holpijp") || lower.contains("rörflöjt") {
        "#f5deb3"
    } else if lower.contains("viola") || lower.contains("gamba") || lower.contains("celeste") ||
              lower.contains("salicional") || lower.contains("voix") || lower.contains("aeoline") {
        "#90c090"
    } else if lower.contains("mixtuur") || lower.contains("cymbel") || lower.contains("sesquialter") ||
              lower.contains("cornet") || lower.contains("scherp") {
        "#b8b8d0"
    } else if lower.contains("koppel") || lower.contains("coupler") {
        "#c8a0c8"
    } else if lower.contains("tremulant") {
        "#88b8c8"
    } else if lower.contains("subbas") || lower.contains("bourdon 16") {
        "#a89070"
    } else {
        "#e8e8e0"
    }
}

fn roman_numeral(n: usize) -> &'static str {
    match n {
        1 => "I",
        2 => "II",
        3 => "III",
        4 => "IV",
        5 => "V",
        6 => "VI",
        _ => "",
    }
}

/// Build the organ's REAL couplers from the parsed definition: each manual's
/// referenced couplers (GrandOrgue `CouplerDef`; the Hauptwerk importer fills the
/// same structures from its KeyAction routing). IDs are prefixed `real_coupler_`
/// so the UI shows them by default. Returns empty when the source defines no
/// couplers — the caller then falls back to the generic `generate_couplers`.
fn build_couplers_from_definition(definition: &OrganDefinition) -> Vec<CouplerDto> {
    let mut couplers = Vec::new();
    let mut action_code: u8 = 100;
    for manual in &definition.manuals {
        let src_name = manual.name.clone();
        for cid in &manual.coupler_ids {
            let cdef = match definition.couplers.iter().find(|c| c.number == *cid) {
                Some(c) => c,
                None => continue,
            };
            let dest_name = definition.manuals.iter()
                .find(|m| m.number == cdef.destination_manual)
                .map(|m| m.name.clone())
                .unwrap_or_else(|| src_name.clone());
            let offset = cdef.destination_keyshift;
            let coupler_type = if offset >= 12 { "super" }
                else if offset <= -12 { "sub" }
                else { "unison" };
            // Many ODFs name couplers generically ("Coupler 1"); replace those with
            // a descriptive "source + destination" label (matching the app's own
            // coupler naming), keeping any meaningful ODF name as-is.
            let raw = cdef.name.trim();
            let rl = raw.to_lowercase();
            let is_generic = raw.is_empty()
                || rl.strip_prefix("coupler").map_or(false, |r| r.trim().chars().all(|c| c.is_ascii_digit()))
                || rl.strip_prefix("koppel").map_or(false, |r| r.trim().chars().all(|c| c.is_ascii_digit()));
            let suffix = if offset >= 12 { " 4'" } else if offset <= -12 { " 16'" } else { "" };
            let name = if is_generic {
                format!("{}{}\n+\n{}", src_name, suffix, dest_name)
            } else {
                raw.to_string()
            };
            couplers.push(CouplerDto {
                id: format!("real_coupler_{}", cdef.number),
                name,
                source_division: src_name.clone(),
                destination_division: dest_name,
                active: false,
                display_in_division: src_name.clone(),
                midi_action_code: action_code,
                coupler_type: coupler_type.into(),
                pitch_offset: offset,
            });
            action_code = action_code.saturating_add(1);
        }
    }
    couplers
}

/// Generate couplers based on division structure
fn generate_couplers(divisions: &[DivisionDto]) -> Vec<CouplerDto> {
    let mut couplers = Vec::new();
    let mut action_code: u8 = 100;

    // Detect pedal division
    let pedal_idx = divisions.iter().position(|d| {
        let lower = d.name.to_lowercase();
        lower.contains("pedal") || lower.contains("pedaal")
    });

    // Manual (non-pedal) divisions
    let manual_indices: Vec<usize> = (0..divisions.len())
        .filter(|&i| Some(i) != pedal_idx)
        .collect();

    // Pedal couplers: pedal keyboard → each manual's stops
    if let Some(ped_idx) = pedal_idx {
        let ped_name = &divisions[ped_idx].name;
        for &man_idx in &manual_indices {
            let man_name = &divisions[man_idx].name;
            let id = format!("coupler_{}_{}",
                man_name.to_lowercase().replace(' ', "_"),
                ped_name.to_lowercase().replace(' ', "_"));
            couplers.push(CouplerDto {
                id,
                name: format!("{}\n+\n{}", ped_name, man_name),
                source_division: ped_name.clone(),
                destination_division: man_name.clone(),
                active: false,
                display_in_division: ped_name.clone(),
                midi_action_code: action_code,
                coupler_type: "unison".into(),
                pitch_offset: 0,
            });
            action_code = action_code.saturating_add(1);
        }
    }

    // Manual couplers: each manual → each other manual (unison)
    for &src_idx in &manual_indices {
        for &dest_idx in &manual_indices {
            if src_idx == dest_idx { continue; }
            let src_name = &divisions[src_idx].name;
            let dest_name = &divisions[dest_idx].name;
            let id = format!("coupler_{}_{}",
                dest_name.to_lowercase().replace(' ', "_"),
                src_name.to_lowercase().replace(' ', "_"));
            couplers.push(CouplerDto {
                id,
                name: format!("{}\n+\n{}", src_name, dest_name),
                source_division: src_name.clone(),
                destination_division: dest_name.clone(),
                active: false,
                display_in_division: src_name.clone(),
                midi_action_code: action_code,
                coupler_type: "unison".into(),
                pitch_offset: 0,
            });
            action_code = action_code.saturating_add(1);
        }
    }

    // Super-octave couplers (+12 semitones / 4') for each manual pair
    for &src_idx in &manual_indices {
        for &dest_idx in &manual_indices {
            if src_idx == dest_idx { continue; }
            let src_name = &divisions[src_idx].name;
            let dest_name = &divisions[dest_idx].name;
            let id = format!("coupler_super_{}_{}",
                dest_name.to_lowercase().replace(' ', "_"),
                src_name.to_lowercase().replace(' ', "_"));
            couplers.push(CouplerDto {
                id,
                name: format!("{} 4'\n+\n{}", src_name, dest_name),
                source_division: src_name.clone(),
                destination_division: dest_name.clone(),
                active: false,
                display_in_division: src_name.clone(),
                midi_action_code: action_code,
                coupler_type: "super".into(),
                pitch_offset: 12,
            });
            action_code = action_code.saturating_add(1);
        }
    }

    // Sub-octave couplers (-12 semitones / 16') for each manual pair
    for &src_idx in &manual_indices {
        for &dest_idx in &manual_indices {
            if src_idx == dest_idx { continue; }
            let src_name = &divisions[src_idx].name;
            let dest_name = &divisions[dest_idx].name;
            let id = format!("coupler_sub_{}_{}",
                dest_name.to_lowercase().replace(' ', "_"),
                src_name.to_lowercase().replace(' ', "_"));
            couplers.push(CouplerDto {
                id,
                name: format!("{} 16'\n+\n{}", src_name, dest_name),
                source_division: src_name.clone(),
                destination_division: dest_name.clone(),
                active: false,
                display_in_division: src_name.clone(),
                midi_action_code: action_code,
                coupler_type: "sub".into(),
                pitch_offset: -12,
            });
            action_code = action_code.saturating_add(1);
        }
    }

    // Intra-manual super/sub octave couplers (self-coupling at different pitch)
    for &man_idx in &manual_indices {
        let name = &divisions[man_idx].name;
        let lower = name.to_lowercase().replace(' ', "_");
        // Super 4'
        couplers.push(CouplerDto {
            id: format!("coupler_super_{0}_{0}", lower),
            name: format!("{} 4'", name),
            source_division: name.clone(),
            destination_division: name.clone(),
            active: false,
            display_in_division: name.clone(),
            midi_action_code: action_code,
            coupler_type: "super".into(),
            pitch_offset: 12,
        });
        action_code = action_code.saturating_add(1);
        // Sub 16'
        couplers.push(CouplerDto {
            id: format!("coupler_sub_{0}_{0}", lower),
            name: format!("{} 16'", name),
            source_division: name.clone(),
            destination_division: name.clone(),
            active: false,
            display_in_division: name.clone(),
            midi_action_code: action_code,
            coupler_type: "sub".into(),
            pitch_offset: -12,
        });
        action_code = action_code.saturating_add(1);
    }

    // Tongwerken Af (T.A.) per division — disables reed stops
    for div in divisions {
        let lower = div.name.to_lowercase().replace(' ', "_");
        couplers.push(CouplerDto {
            id: format!("ta_{}", lower),
            name: format!("T.A. {}", div.name),
            source_division: div.name.clone(),
            destination_division: div.name.clone(),
            active: false,
            display_in_division: div.name.clone(),
            midi_action_code: action_code,
            coupler_type: "ta".into(),
            pitch_offset: 0,
        });
        action_code = action_code.saturating_add(1);
    }

    // Melody couplers: only highest held note coupled (per manual)
    for &man_idx in &manual_indices {
        let name = &divisions[man_idx].name;
        let lower = name.to_lowercase().replace(' ', "_");
        couplers.push(CouplerDto {
            id: format!("melody_{}", lower),
            name: format!("Melodie\n{}", name),
            source_division: name.clone(),
            destination_division: name.clone(),
            active: false,
            display_in_division: name.clone(),
            midi_action_code: action_code,
            coupler_type: "melody".into(),
            pitch_offset: 0,
        });
        action_code = action_code.saturating_add(1);
    }

    // Bass couplers: only lowest held note coupled (per manual → pedal)
    if let Some(ped_idx) = pedal_idx {
        let ped_name = &divisions[ped_idx].name;
        for &man_idx in &manual_indices {
            let man_name = &divisions[man_idx].name;
            let lower = man_name.to_lowercase().replace(' ', "_");
            couplers.push(CouplerDto {
                id: format!("bass_{}_{}", lower, ped_name.to_lowercase().replace(' ', "_")),
                name: format!("Bas\n{}", man_name),
                source_division: man_name.clone(),
                destination_division: ped_name.clone(),
                active: false,
                display_in_division: man_name.clone(),
                midi_action_code: action_code,
                coupler_type: "bass".into(),
                pitch_offset: 0,
            });
            action_code = action_code.saturating_add(1);
        }
    }

    couplers
}

// ============ Custom Organ Commands ============

use vpo_sampler::custom_organ::{CustomOrgan, CustomDivision, CustomStop, StopFamily, scan_samples_directory};

/// DTO for custom organ info
#[derive(Debug, Clone, Serialize)]
pub struct CustomOrganDto {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub builder: String,
    pub stop_count: usize,
    pub pipe_count: usize,
}

/// DTO for custom division
#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct CustomDivisionDto {
    pub id: String,
    pub name: String,
    pub midi_channel: Option<u8>,
    pub first_midi_note: u8,
    pub last_midi_note: u8,
    pub stops: Vec<CustomStopDto>,
}

/// DTO for custom stop
#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct CustomStopDto {
    pub id: String,
    pub name: String,
    pub pitch_feet: f32,
    pub family: String,
    pub pipe_count: usize,
}

/// Scan a directory for samples and create a custom organ
#[tauri::command]
pub fn scan_samples(directory: String) -> Result<CustomOrganDto, String> {
    let path = Path::new(&directory);
    if !path.exists() {
        return Err("Directory does not exist".to_string());
    }

    let organ_name = path.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "Custom Organ".to_string());

    let organ = scan_samples_directory(path, &organ_name)
        .map_err(|e| format!("Failed to scan directory: {}", e))?;

    Ok(CustomOrganDto {
        id: organ.id.clone(),
        name: organ.name.clone(),
        description: organ.description.clone(),
        builder: organ.builder.clone(),
        stop_count: organ.stop_count(),
        pipe_count: organ.pipe_count(),
    })
}

/// Create a new empty custom organ
#[tauri::command]
pub fn create_custom_organ(name: String) -> CustomOrganDto {
    let organ = CustomOrgan::new(&name);
    CustomOrganDto {
        id: organ.id.clone(),
        name: organ.name.clone(),
        description: organ.description.clone(),
        builder: organ.builder.clone(),
        stop_count: organ.stop_count(),
        pipe_count: organ.pipe_count(),
    }
}

/// Save a custom organ to a file
#[tauri::command]
pub fn save_custom_organ(organ_json: String, path: String) -> Result<(), String> {
    let organ: CustomOrgan = serde_json::from_str(&organ_json)
        .map_err(|e| format!("Invalid organ JSON: {}", e))?;

    organ.save(Path::new(&path))
        .map_err(|e| format!("Failed to save organ: {}", e))
}

/// Load a custom organ from a file
#[tauri::command]
pub fn load_custom_organ(path: String) -> Result<String, String> {
    let organ = CustomOrgan::load(Path::new(&path))
        .map_err(|e| format!("Failed to load organ: {}", e))?;

    serde_json::to_string(&organ)
        .map_err(|e| format!("Failed to serialize organ: {}", e))
}

/// Scan a directory and load preload buffers (attack portions) for instant playback
/// Full samples are loaded on-demand in the background when notes are played
/// Async + spawn_blocking — zie load_organ (zelfde UI-freeze-probleem).
#[tauri::command]
pub async fn load_samples_from_directory(state: State<'_, AppState>, directory: String) -> Result<OrganInfoDto, String> {
    let st = state.inner().clone();
    tokio::task::spawn_blocking(move || do_load_samples_from_directory(&st, &directory))
        .await
        .map_err(|e| format!("sample-laadtaak mislukt: {}", e))?
}

/// Herbruikbare directory-scan + load voor zowel Tauri commands als test API.
pub fn do_load_samples_from_directory(state: &AppState, directory: &str) -> Result<OrganInfoDto, String> {
    // Serialiseer t.o.v. audio-wissels en andere loads (zie load_switch_gate).
    let _gate = state.load_switch_gate.lock();
    do_load_samples_from_directory_locked(state, directory)
}

/// Variant ZONDER gate — zie do_load_organ_locked (re-entrante lock = deadlock).
pub fn do_load_samples_from_directory_locked(state: &AppState, directory: &str) -> Result<OrganInfoDto, String> {
    // Normaliseer de pad-notatie (zie do_load_organ) — voorkomt dubbele
    // bibliotheek-entries en gesplitste per-orgel-settings.
    let directory = &directory.replace('/', "\\");
    use std::collections::HashMap;
    use std::sync::Arc;
    use rayon::prelude::*;
    use vpo_sampler::{load_audio_preload, PreloadBuffer};
    use crate::audio::PRELOAD_SAMPLES;

    let path = Path::new(&directory);
    if !path.exists() {
        return Err("Directory does not exist".to_string());
    }

    // JM-Rec (vanaf 3.x) exporteert naast de sample-mappen een .organ-
    // definitie met manualen, registernamen/voettallen, zwelkasten,
    // tremulanten én koppels. De mapscan hieronder leidt alles alleen uit
    // mapnamen af en kent geen koppels — bij zo'n projectmap dus de definitie
    // laden, niet de mappen scannen (melding gebruiker: "koppels en registers
    // komen niet goed over" bij een set uit de nieuwste JM-Rec). Alleen bij
    // precies één .organ in de map zelf; anders blijft het gedrag ongewijzigd
    // (Puttershoek-stijl mappen zonder definitie).
    if let Ok(entries) = std::fs::read_dir(path) {
        let organ_files: Vec<std::path::PathBuf> = entries
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.is_file()
                && p.extension().map(|x| x.eq_ignore_ascii_case("organ")).unwrap_or(false))
            .collect();
        if !organ_files.is_empty() {
            // Bij meerdere definities (GrandOrgue-sets leveren vaak varianten
            // zoals "Naam FullImageResolution.organ"): eerst de definitie
            // waarvan de bestandsnaam gelijk is aan de mapnaam, anders de
            // kortste bestandsnaam — varianten voegen een achtervoegsel toe.
            let folder_stem = path.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
            let chosen = organ_files.iter()
                .find(|p| p.file_stem().map(|s| s.to_string_lossy().to_lowercase() == folder_stem).unwrap_or(false))
                .or_else(|| organ_files.iter().min_by_key(|p| p.file_name().map(|n| n.len()).unwrap_or(usize::MAX)))
                .cloned()
                .unwrap_or_else(|| organ_files[0].clone());
            let organ_path = chosen.to_string_lossy().to_string();
            if organ_files.len() > 1 {
                warn!("Map bevat {} .organ-bestanden; gekozen: {} (kies anders het gewenste bestand via de .organ-knop)", organ_files.len(), organ_path);
            } else {
                info!("Map bevat een .organ-definitie (JM-Rec-export): laden via {}", organ_path);
            }
            return do_load_organ_locked(state, &organ_path);
        }
        // Wel een JM-Rec-project, maar de .organ is (nog) niet geëxporteerd:
        // de mapscan kent geen koppels/voettallen — de gebruiker moet in JM-Rec
        // "Exporteer .organ (JM-Orgue)" doen.
        let has_jmrec_json = std::fs::read_dir(path).ok().into_iter().flatten()
            .filter_map(|e| e.ok())
            .any(|e| e.file_name().to_string_lossy().to_lowercase().ends_with(".jm-rec.json"));
        if has_jmrec_json {
            warn!("JM-Rec-projectmap zonder .organ-definitie: mapscan gebruikt (geen koppels/voettallen). Exporteer in JM-Rec via Instellingen → Exporteer .organ (JM-Orgue).");
        }
    }

    let organ_name = path.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "Custom Organ".to_string());
    // JM-Rec-projectmap zonder .organ: naam/bouwer/plaats uit het manifest,
    // anders zou de mapCODE de bibliotheeknaam worden.
    let (organ_name, organ_builder, organ_location) = match jm_rec_identity_for_dir(path) {
        Some((naam, bouwer, plaats)) => {
            info!("JM-Rec-manifest gevonden: '{}'", naam);
            (naam, bouwer, plaats)
        }
        None => (organ_name, "Custom Samples".to_string(), directory.to_string()),
    };

    info!("Scanning directory for samples: {}", directory);

    let custom_organ = scan_samples_directory(path, &organ_name)
        .map_err(|e| format!("Failed to scan directory: {}", e))?;

    info!("Found {} stops with {} pipes", custom_organ.stop_count(), custom_organ.pipe_count());

    // Microfoonperspectieven (submappen per stop, 0.7.38): laadplan uit de
    // opgeslagen/runtime-voorkeur; standaard alleen de eerste positie.
    let saved_persp = perspective_prefs_for(state, directory);
    let mut persp = plan_perspectives_custom(&custom_organ, &saved_persp);
    let enabled_labels: std::collections::HashSet<String> = persp.iter()
        .filter(|p| p.enabled)
        .map(|p| p.name.clone())
        .collect();
    let mut rank_layout: HashMap<u32, Vec<(u8, u8)>> = HashMap::new();
    let mut rank_summary: Vec<RankSummary> = Vec::new();

    // Collect all sample paths with their keys
    let mut load_tasks: Vec<((u32, u32), std::path::PathBuf)> = Vec::new();
    // Registers met een `_trem`-map (echte tremulant-opnamen).
    let mut stops_with_trem: std::collections::HashSet<u32> = std::collections::HashSet::new();
    let mut stop_id_counter = 1u32;
    let mut stop_action_code: u8 = 150; // Stops use action codes 150-255

    // Build organ info for UI
    let mut divisions: Vec<DivisionDto> = Vec::new();

    for (div_idx, division) in custom_organ.divisions.iter().enumerate() {
        let mut stops: Vec<StopDto> = Vec::new();

        for stop in &division.stops {
            let internal_stop_id = stop_id_counter;
            stop_id_counter += 1;

            // Find the first and last MIDI note for THIS stop (not the division).
            // De eerste noot komt ALTIJD uit laag 0 (ook als die positie is
            // uitgeschakeld): de noot→pipe_num-uitlijning van alle lagen hangt
            // eraan. De laatste noot over alle posities.
            let stop_first_note = stop.pipes.first().map(|p| p.midi_note).unwrap_or(36);
            let stop_last_note = stop.pipes.last().map(|p| p.midi_note)
                .into_iter()
                .chain(stop.perspectives.iter().filter_map(|p| p.pipes.last().map(|x| x.midi_note)))
                .max()
                .unwrap_or(96);

            // Lagen: 0 = primair (pipes/tremulant_pipes), i = perspectives[i-1].
            let mut layers: Vec<(u8, Option<&str>, &[vpo_sampler::CustomPipe], &[vpo_sampler::CustomPipe])> = vec![
                (0u8, stop.primary_perspective.as_deref(), stop.pipes.as_slice(),
                 if stop.has_tremulant { stop.tremulant_pipes.as_slice() } else { &[] }),
            ];
            for (i, p) in stop.perspectives.iter().enumerate() {
                layers.push(((i + 1) as u8, Some(p.name.as_str()), p.pipes.as_slice(), p.tremulant_pipes.as_slice()));
            }
            let mut any_trem = false;
            for (layer_idx, label, lpipes, ltrem) in &layers {
                if let Some(l) = label {
                    if !enabled_labels.contains(*l) {
                        continue;
                    }
                }
                let slot = persp.iter()
                    .position(|x| Some(x.name.as_str()) == *label)
                    .map(|i| i as u8 + 1)
                    .unwrap_or(0);
                let mut layer_any = false;
                // Collect sample paths for preloading (dry samples)
                for pipe in lpipes.iter() {
                    if pipe.midi_note < stop_first_note {
                        warn!("Stop '{}' positie {:?}: noot {} ligt onder de eerste noot {} van de primaire positie — overgeslagen",
                              stop.name, label, pipe.midi_note, stop_first_note);
                        continue;
                    }
                    let full_path = path.join(&pipe.sample_path);
                    let base = (pipe.midi_note as u32) - stop_first_note as u32 + 1;
                    load_tasks.push(((internal_stop_id, crate::audio::layer_key(base, *layer_idx)), full_path));
                    layer_any = true;
                }
                // Collect tremulant sample paths (same key mapping as dry)
                for pipe in ltrem.iter() {
                    if pipe.midi_note < stop_first_note {
                        continue;
                    }
                    let full_path = path.join(&pipe.sample_path);
                    let base = (pipe.midi_note as u32) - stop_first_note as u32 + 1;
                    // Zelfde sleutel als droog + TREM_FLAG: NoteOn kiest hem op
                    // de tremulantstand van het register (0.7.39).
                    let key = crate::audio::layer_key(base, *layer_idx) | crate::audio::TREM_FLAG;
                    load_tasks.push(((internal_stop_id, key), full_path));
                    any_trem = true;
                }
                if layer_any {
                    rank_layout.entry(internal_stop_id).or_default().push((*layer_idx, slot));
                }
            }
            if any_trem {
                stops_with_trem.insert(internal_stop_id);
            }
            rank_summary.push(RankSummary {
                stop_id: internal_stop_id,
                dto_id: format!("{}_{}", div_idx, internal_stop_id),
                name: stop.name.clone(),
                layers: layers.iter().map(|(idx, label, lpipes, _)| RankLayerSummary {
                    index: *idx,
                    name: label.map(|s| s.to_string()).unwrap_or_else(|| stop.name.clone()),
                    perspective: label.map(|s| s.to_string()),
                    pipes_nonempty: lpipes.len(),
                }).collect(),
            });

            let color = get_stop_color(&stop.name);

            info!("Stop '{}': internal_id={}, MIDI range {}-{}, trem={}, posities={}",
                  stop.name, internal_stop_id, stop_first_note, stop_last_note, any_trem, layers.len());

            let reed = matches!(stop.family, vpo_sampler::custom_organ::StopFamily::Reed) || is_reed_stop(&stop.name);
            stops.push(StopDto {
                id: format!("{}_{}", div_idx, internal_stop_id),
                name: stop.name.clone(),
                pitch: stop.pitch_label.clone()
                    .unwrap_or_else(|| format!("{}'", stop.pitch_feet as u32)),
                drawn: false,
                color: Some(color.to_string()),
                has_tremulant: any_trem,
                midi_action_code: stop_action_code,
                internal_stop_id,
                first_midi_note: stop_first_note,
                last_midi_note: stop_last_note,
                is_reed: reed,
            });
            stop_action_code = stop_action_code.saturating_add(1);
        }

        let display_name = if division.name.to_lowercase().contains("pedaal") ||
                             division.name.to_lowercase().contains("pedal") {
            "Pedaal".to_string()
        } else {
            format!("{} (I)", division.name)
        };

        let div_has_trem = stops.iter().any(|s| s.has_tremulant);
        divisions.push(DivisionDto {
            name: division.name.clone(),
            display_name,
            stops,
            has_tremulant: div_has_trem,
            // Echte opnamen (`_trem`-map), geen ODF-tremulant.
            tremulant_kind: if div_has_trem { Some("samples".to_string()) } else { None },
            has_swell: false, // custom sample folders carry no enclosure info
        });
    }

    // Load preload buffers in parallel (only attack portion ~200ms per sample)
    info!("Loading {} preload buffers (attack portions)...", load_tasks.len());
    let start_time = std::time::Instant::now();

    // Voortgang naar de frontend (zie do_load_organ): anders staat de
    // laad-overlay op 0% en oogt een koude load als vastgelopen.
    let progress_total = load_tasks.len();
    let progress_count = std::sync::atomic::AtomicUsize::new(0);
    emit_load_progress(state, 0, progress_total, "Samples laden");
    let preload_results: Vec<_> = load_tasks.par_iter()
        .filter_map(|(key, path)| {
            let n = progress_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
            if n % 100 == 0 || n == progress_total {
                emit_load_progress(state, n, progress_total, "Samples laden");
            }
            match load_audio_preload(path, PRELOAD_SAMPLES) {
                Ok(preload) => Some((*key, Arc::new(preload))),
                Err(e) => {
                    warn!("Failed to preload {:?}: {}", path, e);
                    None
                }
            }
        })
        .collect();

    let elapsed = start_time.elapsed();
    info!("Loaded {} preload buffers in {:.2}s", preload_results.len(), elapsed.as_secs_f32());

    // Convert to HashMap
    let preload_buffers: HashMap<(u32, u32), Arc<PreloadBuffer>> = preload_results.into_iter().collect();

    // Register preload buffers with audio player
    info!("Registering preload buffers for instant playback");
    {
        let player_lock = state.audio_player.read();
        if let Some(ref player) = *player_lock {
            player.send_command(crate::audio::AudioCommand::RegisterPreloadBuffers(Arc::new(preload_buffers)))
                .map_err(|e| format!("Failed to register preload buffers: {}", e))?;
            // Custom sample-mappen kennen geen percussieve stops of ODF-intonatie/loops:
            // wis de sets van een eventueel vorig (GrandOrgue/Hauptwerk-)orgel.
            let _ = player.send_command(crate::audio::AudioCommand::RegisterPercussiveStops(Default::default()));
            let _ = player.send_command(crate::audio::AudioCommand::RegisterOdfVoicings(Arc::new(Default::default())));
            // Ook de hertemper-tabel wissen: (stop_id, pipe_num)-keys van een
            // vorig GO-orgel zouden anders op deze set doorwerken.
            let _ = player.send_command(crate::audio::AudioCommand::RegisterOdfRetune(Arc::new(Default::default())));
            let _ = player.send_command(crate::audio::AudioCommand::RegisterOdfLoops(Arc::new(Default::default())));
            // Ook de tremulantregisters opnieuw zetten: had het vórige orgel
            // echte trem-opnamen en dit orgel niet, dan bleef die set staan en
            // zweeg de synth-LFO voor de verkeerde registers (auditbevinding 38).
            if !stops_with_trem.is_empty() {
                info!("Tremulant-samplelaag: {} registers met een _trem-map", stops_with_trem.len());
            }
            let _ = player.send_command(crate::audio::AudioCommand::RegisterTremStops(stops_with_trem.clone()));
            // Custom sample-mappen kennen geen toetsduur-releases of
            // ReleaseCrossfadeLength: wis de tabellen van een vorig ODF-orgel.
            let _ = player.send_command(crate::audio::AudioCommand::RegisterReleaseMeta(Default::default()));
            let _ = player.send_command(crate::audio::AudioCommand::RegisterReleaseCrossfade(Arc::new(Default::default())));
        }
    }

    // Laadplan per stop (altijd sturen, ook leeg — vervangt de map van het
    // vorige orgel) + live gains per perspectief-slot.
    {
        let multi = rank_layout.values().filter(|v| v.len() > 1).count();
        if multi > 0 {
            info!("Perspectieven: {} stops met meerdere posities geladen; {:?}", multi,
                  persp.iter().map(|p| format!("{}={}", p.name, if p.enabled { "aan" } else { "uit" })).collect::<Vec<_>>());
        }
        state.send_audio_command(crate::audio::AudioCommand::RegisterRankLayout(Arc::new(rank_layout)));
        for p in persp.iter_mut() {
            state.send_audio_command(crate::audio::AudioCommand::SetPerspectiveGain { slot: p.slot, gain_db: p.gain_db });
            p.loaded = p.enabled;
        }
    }

    let stop_count: usize = divisions.iter().map(|d| d.stops.len()).sum();
    let couplers = generate_couplers(&divisions);

    let organ_info = OrganInfoDto {
        id: directory.to_string(),
        name: organ_name,
        builder: organ_builder,
        location: organ_location,
        year: None,
        stop_count,
        divisions,
        couplers: Some(couplers),
        retune_pipes: 0,
        retune_total: 0,
        perspectives: perspective_dtos(&persp),
        layered_stops: 0,
        // Eigen samplemappen kennen geen aparte release-opnamen.
        release_pipes: 0,
    };

    // Koppels zoals de sampleset ze levert onthouden: vertrekpunt voor het
    // bij- en afschakelen van extra koppels (0.7.47).
    *state.base_couplers.write() = organ_info.couplers.clone().unwrap_or_default();
    state.extra_coupler_ids.write().clear();

    // Reset bij orgelwissel: stop alle klinkende noten en wis de getrokken registratie +
    // crescendo-stops + koppels, zodat geen geluid of registratie van het vórige orgel
    // achterblijft (anders het symptoom "geluid is actief maar de knop niet"). De
    // "laatste stand" van dít orgel wordt hierna via restore_organ_settings hersteld.
    state.send_audio_command(AudioCommand::AllNotesOff);
    state.drawn_stops.write().clear();
    // Spooktoetsen van het vorige orgel wissen: die zouden anders bij elke
    // registerwissel/crescendotrap van het nieuwe orgel stemmen starten die
    // nooit meer een NoteOff krijgen (hangers).
    state.held_notes.write().clear();
    *state.crescendo_active_stops.write() = Vec::new();
    *state.crescendo_stage.write() = 0;
    crate::state::CRESCENDO_HERSTART_VANAF_NUL.store(false, std::sync::atomic::Ordering::Relaxed);
    crate::state::zwel_nazending_leeg();
    crate::state::trede_filter_leeg();
    state.set_active_couplers(Vec::new());
    reset_organ_scoped_state(state);
    // Perspectieven + lagen-samenvatting van dít orgel (ná de reset).
    *state.perspectives.write() = persp;
    *state.rank_summary.write() = rank_summary;

    // Build stop→division map and register with audio thread
    let stop_div_map = build_stop_division_map(&organ_info.divisions);
    state.send_audio_command(AudioCommand::RegisterStopDivisionMap(stop_div_map));
    let (wind_profielen, wind_pleno) = build_stop_wind_profiles(&organ_info.divisions);
    state.send_audio_command(AudioCommand::RegisterStopWindProfiles(std::sync::Arc::new(wind_profielen), wind_pleno));
    state.reset_division_gains(organ_info.divisions.len());
    state.reset_division_settings(organ_info.divisions.len());

    // current_organ_id EERST (zie do_load_organ / auditbevinding 41).
    {
        let mut id = state.current_organ_id.write();
        *id = Some(directory.to_string());
    }
    {
        let mut current = state.loaded_organ_info.write();
        *current = Some(organ_info.clone());
    }

    info!("Loaded custom organ: {} stops (preload buffers ready)", stop_count);

    // Add to library if not already present
    add_or_refresh_library_entry(state, &organ_info, directory, "sample_directory");

    // current_organ_id is al vóór de DTO-publicatie gezet (auditbevinding 41).
    restore_organ_settings(state, directory);
    apply_pending_registration(state, &organ_info);
    // Zwelstand terugzetten (RegisterStopDivisionMap zette de audio op open).
    apply_swell_positions(state);

    Ok(fill_live_registration_flags(state, organ_info))
}

// ============ Library Helper Functions ============

/// Zet dit orgel in de bibliotheek, of VERVERS de bestaande entry.
///
/// Verversen is nodig sinds de naamregel voor JM-Rec-sets: anders zou een set
/// die al in de bibliotheek staat zijn oude naam (de mapcode) houden. `id` en
/// `source_path` blijven ongemoeid — die zijn de sleutel van de opgeslagen
/// instellingen (`lib.settings`, `.jm-settings.json`, `current_organ_id`).
///
/// De afbeelding wordt alleen (her)gezocht als er nog geen is, het bestand
/// verdwenen is, of de zoekversie is opgehoogd — en NOOIT als de gebruiker er
/// zelf een koos (`image_manual`).
///
/// LOCKVOLGORDE (zoals get_organ_image): korte READ-lock om te beslissen, lock
/// LOS tijdens de diepe afbeeldingszoektocht (die loopt over de hele
/// sample-map en duurde seconden), dan een korte WRITE-lock om te schrijven.
/// Anders bevriest elk venster dat de bibliotheek leest tijdens die scan — en
/// bij de eerste start na een upgrade heeft élke entry nog image_searched=None.
fn add_or_refresh_library_entry(state: &AppState, organ_info: &OrganInfoDto, source_path: &str, source_type: &str) {
    let norm = |s: &str| s.replace('/', "\\").to_lowercase();
    let key = norm(source_path);

    // 1) Korte read-lock: bestaat de entry al, en moet er gezocht worden?
    let (bestaat, moet_zoeken) = {
        let lib = state.organ_library.read();
        match lib.organs.iter().find(|o| norm(&o.source_path) == key) {
            Some(e) => {
                let bestand_weg = e.image_path.as_ref().map(|p| !Path::new(p).is_file()).unwrap_or(true);
                let al_gezocht = e.image_searched == Some(library::IMAGE_SEARCH_VERSION);
                (true, !e.image_manual && bestand_weg && !al_gezocht)
            }
            // Nieuw: afbeelding meteen zoeken (en het resultaat — ook "niets" —
            // vastleggen zodat de diepe scan niet elke keer herhaald wordt).
            None => (false, true),
        }
    };

    // 2) Zoeken ZONDER lock.
    let (gezocht, bron_leesbaar) = if moet_zoeken {
        let found = library::find_organ_image(source_path, source_type);
        let leesbaar = found.is_some() || afbeeldingsbron_leesbaar(source_path, source_type);
        info!(
            "Bibliotheek: afbeelding gezocht voor {} → {:?} (bronmap leesbaar: {})",
            source_path, found, leesbaar
        );
        (Some(found), leesbaar)
    } else {
        (None, false)
    };

    // 3) Korte write-lock om te schrijven. De entry kan intussen veranderd zijn
    //    (ander venster koos een afbeelding, of voegde hem net toe): opnieuw
    //    opzoeken en een handmatige keuze respecteren.
    let mut changed = false;
    {
        let mut lib = state.organ_library.write();
        match lib.organs.iter().position(|o| norm(&o.source_path) == key) {
            Some(idx) => {
                let e = &mut lib.organs[idx];
                for (veld, nieuw) in [
                    (&mut e.name, &organ_info.name),
                    (&mut e.builder, &organ_info.builder),
                    (&mut e.location, &organ_info.location),
                ] {
                    if *veld != *nieuw {
                        *veld = nieuw.clone();
                        changed = true;
                    }
                }
                if e.year != organ_info.year {
                    e.year = organ_info.year.clone();
                    changed = true;
                }
                if e.stop_count != organ_info.stop_count {
                    e.stop_count = organ_info.stop_count;
                    changed = true;
                }
                if let Some(found) = gezocht {
                    // Alleen vastleggen als er ook écht gezocht KON worden: op
                    // een losgekoppelde USB- of netwerkschijf levert de
                    // zoektocht óók None op, en dat mag geen blijvend "geen
                    // foto" worden (en het oude pad niet wissen).
                    if !e.image_manual && bron_leesbaar {
                        e.image_path = found;
                        e.image_searched = Some(library::IMAGE_SEARCH_VERSION);
                        changed = true;
                    }
                }
                if changed && bestaat {
                    info!("Bibliotheek-entry ververst: {}", source_path);
                }
            }
            None => {
                lib.organs.push(OrganLibraryEntry {
                    id: source_path.to_string(),
                    name: organ_info.name.clone(),
                    builder: organ_info.builder.clone(),
                    location: organ_info.location.clone(),
                    year: organ_info.year.clone(),
                    stop_count: organ_info.stop_count,
                    source_type: source_type.to_string(),
                    source_path: source_path.to_string(),
                    image_path: gezocht.flatten(),
                    image_manual: false,
                    // Onbereikbare bronmap → niet als "gezocht" wegschrijven,
                    // anders blijft dit orgel ook na het aankoppelen van de
                    // schijf voorgoed zonder foto.
                    image_searched: if bron_leesbaar { Some(library::IMAGE_SEARCH_VERSION) } else { None },
                });
                changed = true;
            }
        }
    }
    // parking_lot is niet re-entrant: save_library() neemt zelf een read-lock,
    // dus pas opslaan als de write-guard gevallen is.
    if changed {
        state.save_library();
    }
}

/// Pedaalkoppelingen uit opgeslagen instellingen, met dezelfde wederzijdse
/// uitsluiting als `AppState::claim_pedal_cc` (0.7.39). Die helper draait
/// alleen bij inleren/handmatig instellen; een .jm-settings.json uit 0.7.38 kan
/// nog een zwelbinding én de crescendobinding op dezelfde (kanaal, CC)
/// bevatten. Werden die allebei hersteld, dan won in de MIDI-lus
/// `cc_claimed_by_crescendo` — ongeacht of het crescendo aanstaat — en was de
/// zwelkast stil dood, zonder logregel ("als het één werkt, werkt het ander
/// niet"). Hier wint, consistent met de runtime-claim, de crescendobinding;
/// botsende zwelbindingen worden weggelaten met een warn!-regel (kanaal
/// 1-based, zoals de UI toont). De opgeschoonde set gaat bij de eerstvolgende
/// autosave mee naar schijf. Geeft (zwelbindingen, crescendobinding) terug
/// zoals ze hersteld worden.
fn pedaalbindingen_voor_herstel(settings: &OrganSettings) -> (Vec<SwellBindingSaved>, Option<CrescendoBindingSaved>) {
    let crescendo = settings.crescendo_binding.clone();
    let swell: Vec<SwellBindingSaved> = settings.swell_bindings.iter().filter(|b| {
        let botst = crescendo.as_ref()
            .map_or(false, |c| c.channel == b.channel && c.cc_num == b.cc_num);
        if botst {
            warn!(
                "Zwelbinding voor '{}' (kanaal {}, CC {}) weggelaten: dezelfde trede is als generaal crescendo gekoppeld",
                b.division_name, b.channel + 1, b.cc_num
            );
        }
        !botst
    }).cloned().collect();
    (swell, crescendo)
}

/// Heeft deze divisie echte tremulant-OPNAMEN? `tremulant_kind` "wave"
/// (GrandOrgue-golfvormtremulant) of "samples" (Hauptwerk-"tremmed"-laag,
/// JM-Rec `_trem`-map), óf een register met een trem-laag (stops_with_trem).
/// Daar hoort geen nagebootste LFO overheen: opname én nabootsing klinken dan
/// over elkaar.
fn divisie_heeft_tremulant_opnamen(div: &DivisionDto) -> bool {
    matches!(div.tremulant_kind.as_deref(), Some("wave") | Some("samples"))
        || div.stops.iter().any(|s| s.has_tremulant)
}

/// Opgeslagen LFO-voorkeuren afgestemd op het geladen orgel. Sinds 0.7.39 zet
/// de UI de nagebootste tremulant niet meer standaard aan voor divisies met
/// echte opnamen, maar een in 0.7.38 opgeslagen `enabled=true` kwam bij het
/// laden onverkort terug en zette via set_tremulant_lfo de LFO óver de
/// opnamen ("Tremulant LFO div X: active=true" naast de opname). Voor zulke
/// divisies wordt enabled=true hier op false gezet; de parameters (rate/
/// diepte) blijven staan, en voor divisies zonder opnamen blijft de voorkeur
/// ongemoeid. Geeft de aangepaste lijst terug plus de namen van de divisies
/// waarvoor de LFO is uitgezet (de aanroeper logt dat één keer).
fn nagebootste_tremulant_voor_herstel(
    saved: &[DivisionTremulantSaved],
    divisions: &[DivisionDto],
) -> (Vec<DivisionTremulantSaved>, Vec<String>) {
    let mut uitgezet: Vec<String> = Vec::new();
    let lijst = saved.iter().map(|t| {
        let mut t = t.clone();
        let echte_opnamen = divisions.iter()
            .any(|d| d.name == t.division && divisie_heeft_tremulant_opnamen(d));
        if t.enabled && echte_opnamen {
            t.enabled = false;
            uitgezet.push(t.division.clone());
        }
        t
    }).collect();
    (lijst, uitgezet)
}

fn restore_organ_settings(state: &AppState, organ_id: &str) {
    use crate::state::{MidiPresetBinding, MidiPresetTrigger, MidiChannelMapping, SwellBinding};

    // Bibliotheek (exact/case-ongevoelig, audit 49), anders .jm-settings.json
    // naast het orgel (wordt geïmporteerd, auditbevinding 17) — zie
    // saved_settings_for; dezelfde bron gebruikt de perspectieven-planning
    // vóór de load. Perspectief-gains zijn daar al toegepast (SetPerspectiveGain
    // bij de load), dus hier niets extra's voor perspectieven.
    let settings = match saved_settings_for(state, organ_id) {
        Some(s) => s,
        None => return,
    };

    info!("Restoring settings for organ: {}", organ_id);

    // Alle opnameposities in het geheugen houden (per orgel).
    *state.load_all_perspectives.write() = settings.load_all_perspectives;

    // Doorlopend klavier (per orgel).
    crate::state::DOORLOPEND_KLAVIER.store(
        settings.continuous_keyboard, std::sync::atomic::Ordering::Relaxed);

    // Extra koppels (per orgel): meteen in de koppellijst zetten.
    if !settings.extra_couplers.is_empty() {
        *state.extra_coupler_ids.write() = settings.extra_couplers.clone();
        koppellijst_herbouwen(state);
        info!("{} extra koppels hersteld", settings.extra_couplers.len());
    }

    // Restore MIDI channel mappings
    if !settings.midi_mappings.is_empty() {
        let mappings: Vec<MidiChannelMapping> = settings.midi_mappings.iter().map(|m| {
            MidiChannelMapping {
                division: m.division.clone(),
                channel: m.channel,
                transpose: m.transpose,
                first_midi_note: m.first_midi_note,
                last_midi_note: m.last_midi_note,
                short_octave: m.short_octave,
            }
        }).collect();
        state.set_midi_mappings(mappings);
        info!("Restored {} MIDI mappings", settings.midi_mappings.len());
    }

    // Restore preset bindings
    if !settings.preset_bindings.is_empty() {
        let bindings: Vec<MidiPresetBinding> = settings.preset_bindings.iter().filter_map(|b| {
            // Eén omzetting voor alle triggersoorten (ook sysex en ccbit).
            saved_to_preset_binding(b).ok()
        }).collect();
        let count = bindings.len();
        state.set_preset_bindings_replace(bindings);
        info!("Restored {} preset bindings", count);
    }

    // Restore swell bindings — de divisie-index wordt herleid uit de
    // divisienaam (de opgeslagen index kan verschoven zijn na her-import).
    // Bij een herlaad/wissel van hetzelfde orgel wint de laatst ontvangen
    // pedaalstand (stash) van de opgeslagen waarde.
    // Exclusiviteit zwelkast <-> generaal crescendo ook hier: een
    // 0.7.38-bestand kan beide op dezelfde trede hebben (zie
    // pedaalbindingen_voor_herstel — de crescendobinding wint).
    let (swell_saved, crescendo_saved) = pedaalbindingen_voor_herstel(&settings);
    if !swell_saved.is_empty() {
        let mut bindings = swell_bindings_from_saved(state, &swell_saved);
        if let Some((prev_id, stash)) = state.swell_last_stash.write().take() {
            if prev_id.eq_ignore_ascii_case(organ_id) {
                for b in bindings.iter_mut() {
                    if let Some((_, v)) = stash.iter().find(|(d, _)| *d == b.division_name) {
                        b.last_value = Some(*v);
                    }
                }
            }
        }
        let count = bindings.len();
        state.set_swell_bindings_replace(bindings);
        info!("Restored {} swell bindings", count);
    }
    // Stash is alleen voor déze herlaad; niet laten liggen voor een latere.
    *state.swell_last_stash.write() = None;

    // Generaal-crescendo matrix + aan/uit + kolomaantal (per orgel; de
    // load-reset heeft ze net gewist). De trede zelf staat na een load op 0;
    // de registers van de trede komen dus alleen via het pedaal terug.
    if !settings.crescendo_stages.is_empty() || settings.crescendo_enabled {
        *state.crescendo_stages.write() = settings.crescendo_stages.clone();
        *state.crescendo_enabled.write() = settings.crescendo_enabled;
        info!("Restored crescendo config: {} stages, enabled={}", settings.crescendo_stages.len(), settings.crescendo_enabled);
    }
    if settings.crescendo_num_stages > 0 {
        *state.crescendo_num_stages.write() = settings.crescendo_num_stages;
    }

    // Restore de "laatste stand": getrokken registratie + actieve koppels. Alleen de
    // registratie-set wordt gezet (geen audio getriggerd — er klinkt nog niets vlak na
    // het laden); zodra de gebruiker speelt klinken meteen de juiste registers.
    if !settings.drawn_stops.is_empty() && *state.restore_registration.read() {
        // Filter op registers die in dit orgel bestaan, voor het geval het bestand wijzigde.
        let valid: std::collections::HashSet<String> = {
            let organ = state.loaded_organ_info.read();
            match organ.as_ref() {
                Some(o) => o.divisions.iter().flat_map(|d| d.stops.iter().map(|s| s.id.clone())).collect(),
                None => std::collections::HashSet::new(),
            }
        };
        let restored: Vec<String> = settings.drawn_stops.iter()
            .filter(|id| valid.contains(*id))
            .cloned()
            .collect();
        let n = restored.len();
        let saved_count = settings.drawn_stops.len();
        *state.drawn_stops.write() = restored;
        info!("Restored {} drawn stops (laatste stand)", n);
        if n < saved_count {
            warn!("Registratie deels niet hersteld: {} van {} registers niet gevonden in dit orgel (gewijzigde sampleset?)", saved_count - n, saved_count);
        }
    }
    // Koppels vallen onder dezelfde "laatste stand"-toggle als de registers:
    // standaard (toggle uit) start een orgel volledig schoon — geen registers
    // én geen koppels aan. Voorheen kwamen koppels onvoorwaardelijk terug.
    if !settings.active_couplers.is_empty() && *state.restore_registration.read() {
        state.set_active_couplers(settings.active_couplers.clone());
        info!("Restored {} active couplers", settings.active_couplers.len());
    }

    // Crescendo-pedaalbinding herstellen (channel/cc/min/max/invert). Wint bij
    // een botsing met een zwelbinding (pedaalbindingen_voor_herstel).
    if let Some(b) = crescendo_saved {
        *state.crescendo_binding.write() = Some((b.channel, b.cc_num, b.min_val, b.max_val, b.invert));
        info!("Crescendo-koppeling hersteld: kanaal {} CC{} invert={}", b.channel + 1, b.cc_num, b.invert);
    }

    // Audio-DSP (master volume, temperament, reverb, EQ): zet ALLEEN de runtime-spiegels op
    // de waarde van DIT orgel (ook als None — zo lekt de waarde van het vorige orgel niet door
    // bij een opslag voordat de frontend pusht). We sturen hier bewust GEEN audio-commando's:
    //  - de frontend past deze vier toe bij load (loadAudioSettingsForOrgan), en
    //  - extra sends tijdens het laden geven onnodige contentie op het audio-commandokanaal.
    // Voor een orgel zonder opgeslagen waarde stuurt de frontend zijn default.
    *state.master_volume_db.write() = settings.master_volume_db;
    *state.temperament_settings.write() = settings.temperament.clone();
    *state.reverb_settings.write() = settings.reverb.clone();
    *state.eq_settings.write() = settings.eq.clone();
    if settings.master_volume_db.is_some() || settings.temperament.is_some()
        || settings.reverb.is_some() || settings.eq.is_some() {
        info!("Restored audio-DSP settings mirror (master/temperament/reverb/eq)");
    }

    // Per-divisie DSP-spiegels herstellen (mirror-only; de frontend past audio toe bij load).
    // De maps zijn net door reset_division_settings gewist, dus we vullen ze vers met DIT orgel.
    {
        let mut pans = state.division_pans.write();
        for p in &settings.division_pans { pans.insert(p.division.clone(), p.pan); }
    }
    {
        let mut sw = state.division_swell_configs.write();
        for s in &settings.division_swell_configs { sw.insert(s.division.clone(), (s.min_db, s.filter_cutoff)); }
    }
    {
        // Nagebootste tremulant (LFO) nooit over echte tremulant-opnamen heen:
        // een 0.7.38-bestand kan enabled=true hebben voor een wave/samples-
        // divisie (zie nagebootste_tremulant_voor_herstel). Eerst de lijst
        // bepalen onder alleen de orgel-read-lock, dan pas de spiegel schrijven.
        let (trems, uitgezet) = {
            let organ = state.loaded_organ_info.read();
            let divisions: &[DivisionDto] = organ.as_ref().map(|o| o.divisions.as_slice()).unwrap_or(&[]);
            nagebootste_tremulant_voor_herstel(&settings.division_tremulants, divisions)
        };
        for naam in &uitgezet {
            warn!("Nagebootste tremulant voor '{}' uitgezet: de set heeft echte tremulant-opnamen", naam);
        }
        let mut tr = state.division_tremulants.write();
        for t in &trems {
            tr.insert(t.division.clone(), (t.enabled, t.rate, t.amp_depth, t.pitch_depth));
        }
    }
    {
        let mut wg = state.wind_group_configs.write();
        for w in &settings.wind_group_configs {
            wg.insert(w.group, w.clone());
        }
    }
    // Indeling van de afstandsbediening van DIT orgel (reset_division_settings
    // zette hem net op None). De rev-bump laat de remote-pagina /organ opnieuw
    // ophalen; het hoofdvenster publiceert daarna dezelfde indeling opnieuw.
    if settings.remote_layout.is_some() {
        state.set_remote_layout_mirror(settings.remote_layout.clone());
        info!("Restored remote layout");
    }
}

// ============ Library Commands ============

/// Is de bibliotheek in dit proces al één keer opgefrist? (punt 8: bestaande
/// kaarten tonen anders hun oude naam — bv. de mapcode "PuttBätz" — totdat je
/// het orgel een keer opent.)
static BIB_NAMEN_VERVERST: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Event naar de frontend zodra de achtergrondopfris kaartnamen heeft
/// gewijzigd; Console.svelte haalt de bibliotheek dan opnieuw op
/// (`loadLibrary`). Payload: `{ "updated": <aantal gewijzigde kaarten> }`.
pub const BIB_BIJGEWERKT_EVENT: &str = "jm-orgue:library-updated";

/// Lees ALLEEN de `[Organ]`-sectie van een GrandOrgue-`.organ`, zonder de rest
/// te parsen: de sectie staat vooraan, dus de eerste 64 kB volstaat. Zo blijft
/// een ODF van tientallen MB's buiten het geheugen en worden er geen samples
/// aangeraakt. `None` = bestand onleesbaar (schijf weg) of geen `[Organ]`.
fn lees_organ_sectie_licht(path: &Path) -> Option<vpo_sampler::OrganInfo> {
    use std::io::Read;
    let f = std::fs::File::open(path).ok()?;
    let mut buf = Vec::new();
    f.take(64 * 1024).read_to_end(&mut buf).ok()?;

    // Zelfde decodering als de volledige parser: UTF-8, anders Latin-1. Een
    // afgekapt laatste teken telt NIET als "dus Latin-1" (dat zou mojibake
    // geven), vandaar de marge van 4 bytes.
    let txt = match std::str::from_utf8(&buf) {
        Ok(s) => s.to_string(),
        Err(e) if e.valid_up_to() + 4 >= buf.len() => {
            String::from_utf8_lossy(&buf[..e.valid_up_to()]).into_owned()
        }
        Err(_) => buf.iter().map(|&b| b as char).collect(),
    };

    let mut info = vpo_sampler::OrganInfo::default();
    let mut in_organ = false;
    let mut gevonden = false;
    for regel in txt.lines() {
        let regel = regel.trim_start_matches('\u{feff}').trim();
        if regel.is_empty() || regel.starts_with(';') {
            continue;
        }
        if regel.starts_with('[') && regel.ends_with(']') {
            if in_organ {
                break; // volgende sectie: klaar
            }
            in_organ = regel[1..regel.len() - 1].eq_ignore_ascii_case("Organ");
            gevonden |= in_organ;
            continue;
        }
        if !in_organ {
            continue;
        }
        let Some(eq) = regel.find('=') else { continue };
        let (sleutel, waarde) = (regel[..eq].trim(), regel[eq + 1..].trim());
        match sleutel {
            "ChurchName" => info.church_name = waarde.to_string(),
            "ChurchAddress" => info.church_address = waarde.to_string(),
            "OrganBuilder" => info.organ_builder = waarde.to_string(),
            "OrganComments" => info.organ_comments = waarde.to_string(),
            "RecordingDetails" => info.recording_details = waarde.to_string(),
            _ => {}
        }
    }
    if gevonden { Some(info) } else { None }
}

/// Naam/bouwer/plaats van één bibliotheek-entry, LICHTGEWICHT: alleen het
/// JM-Rec-manifest (`<stem>.jm-rec.json`) en/of de `[Organ]`-sectie — geen
/// volledige ODF-parse en geen samples. `None` = niets te zeggen (pad weg,
/// Hauptwerk-XML, of een mapscan zonder manifest) → entry ongemoeid laten.
fn bibliotheek_identiteit_licht(source_path: &str, source_type: &str) -> Option<(String, String, String)> {
    let pad = Path::new(source_path);
    if is_organ_file_id(source_path) {
        // Hauptwerk-XML valt hier bewust buiten: dat bestand is niet in één
        // sectie te lezen en de naamregel van 0.7.39 raakt het niet.
        if source_path.to_lowercase().ends_with(".organ_hauptwerk_xml") {
            return None;
        }
        let info = lees_organ_sectie_licht(pad)?;
        return Some(display_identity(pad, &info));
    }
    if source_type == "sample_directory" {
        // Mapscan: alleen een JM-Rec-manifest kan hier iets zinnigs zeggen.
        return jm_rec_identity_for_dir(pad);
    }
    None
}

/// Werk de naam/bouwer/plaats van alle bibliotheek-entries bij, LICHTGEWICHT
/// (zie [`bibliotheek_identiteit_licht`]). Draait op een achtergrondthread.
/// De snapshot en de bijwerking gebeuren elk onder een KORTE lock (alleen
/// geheugenwerk); het lezen van schijf en het opslaan gebeuren zonder lock —
/// een UI-thread die intussen de bibliotheek opent of wijzigt wacht dus
/// hooguit microseconden. Ontbrekende bestanden leveren `None` op en laten
/// de entry staan; er wordt nooit iets gewist. Geeft het aantal gewijzigde
/// entries terug (0 = niets veranderd, niets opgeslagen).
fn ververs_bibliotheek_namen(bibliotheek: &parking_lot::RwLock<library::OrganLibrary>, app_data_dir: &Path) -> usize {
    let norm = |s: &str| s.replace('/', "\\").to_lowercase();
    // Snapshot onder een korte read-lock; het lezen van schijf gebeurt zonder lock.
    let entries: Vec<(String, String, String, String, String)> = {
        let lib = bibliotheek.read();
        lib.organs
            .iter()
            .map(|o| {
                (
                    o.source_path.clone(),
                    o.source_type.clone(),
                    o.name.clone(),
                    o.builder.clone(),
                    o.location.clone(),
                )
            })
            .collect()
    };
    let mut wijzigingen: Vec<(String, String, String, String)> = Vec::new();
    for (source_path, source_type, naam, bouwer, plaats) in entries {
        if let Some((n, b, p)) = bibliotheek_identiteit_licht(&source_path, &source_type) {
            if n != naam || b != bouwer || p != plaats {
                wijzigingen.push((source_path, n, b, p));
            }
        }
    }
    if wijzigingen.is_empty() {
        return 0;
    }

    // Bijwerken onder een korte write-lock; loggen pas daarna (buiten de lock).
    let mut gewijzigd: Vec<(String, String)> = Vec::new();
    {
        let mut lib = bibliotheek.write();
        for (source_path, n, b, p) in wijzigingen {
            let sleutel = norm(&source_path);
            // Opnieuw opzoeken: de entry kan intussen weg zijn of al door een
            // echte load zijn bijgewerkt.
            if let Some(e) = lib.organs.iter_mut().find(|o| norm(&o.source_path) == sleutel) {
                if e.name != n || e.builder != b || e.location != p {
                    gewijzigd.push((std::mem::replace(&mut e.name, n.clone()), n));
                    e.builder = b;
                    e.location = p;
                }
            }
        }
    }
    for (oud, nieuw) in &gewijzigd {
        info!("Bibliotheek opgefrist: '{}' → '{}'", oud, nieuw);
    }
    if !gewijzigd.is_empty() {
        // Opslaan van een KOPIE: `AppState::save_library()` houdt de read-lock
        // vast tijdens het schrijven naar schijf, en dat zou een write vanaf
        // de UI-thread (verwijderen, afbeelding kiezen) zolang laten wachten.
        let kopie: library::OrganLibrary = (*bibliotheek.read()).clone();
        library::save_library(app_data_dir, &kopie);
    }
    gewijzigd.len()
}

/// Start `werk` één keer per proces op een eigen thread en geef METEEN terug;
/// de aanroeper wacht er nooit op. `vlag` bewaakt de eenmaligheid (tweede
/// aanroep: `None`, er gebeurt niets). De JoinHandle is er alleen voor de
/// tests; wie hem laat vallen laat de thread gewoon doorlopen.
fn start_opfris_eenmalig<F>(vlag: &std::sync::atomic::AtomicBool, werk: F) -> Option<std::thread::JoinHandle<()>>
where
    F: FnOnce() + Send + 'static,
{
    if vlag.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return None;
    }
    std::thread::Builder::new()
        .name("bibliotheek-opfris".into())
        .spawn(werk)
        .ok()
}

/// Meld de frontend dat de achtergrondopfris kaartnamen heeft gewijzigd.
/// Headless (test-API vóór Tauri-setup) is er geen AppHandle — dan overslaan.
fn emit_bibliotheek_bijgewerkt(state: &AppState, aantal: usize) {
    if let Some(app) = state.app_handle.read().as_ref() {
        use tauri::Emitter;
        let _ = app.emit(BIB_BIJGEWERKT_EVENT, serde_json::json!({ "updated": aantal }));
    }
}

/// Fris de bibliotheekkaarten ÉÉN keer per proces op (punt 8), volledig op
/// een achtergrondthread. De aanroeper (de UI-thread, via `get_organ_library`)
/// wacht er NIET op — ook de snapshot van de entries wordt op de
/// achtergrondthread genomen: een weggehaalde USB-schijf of een trage
/// netwerkmap mag het openen van de bibliotheek nooit ophouden. Zijn er
/// namen veranderd, dan gaat [`BIB_BIJGEWERKT_EVENT`] naar de frontend, die
/// de lijst opnieuw ophaalt.
fn ververs_bibliotheek_namen_eenmalig(state: &AppState) {
    if BIB_NAMEN_VERVERST.load(std::sync::atomic::Ordering::SeqCst) {
        return; // snelle uitweg: geen AppState-kloon voor niets
    }
    let st = state.clone();
    start_opfris_eenmalig(&BIB_NAMEN_VERVERST, move || {
        let aantal = ververs_bibliotheek_namen(&st.organ_library, &st.app_data_dir);
        if aantal > 0 {
            emit_bibliotheek_bijgewerkt(&st, aantal);
        }
    });
}

/// Synchroon Tauri-commando (UI-thread): geeft de bibliotheek METEEN terug.
/// De enige lock hier is de read-lock voor de kloon van de entries —
/// microseconden; de opfris loopt op de achtergrond (zie hierboven).
#[tauri::command]
pub fn get_organ_library(state: State<AppState>) -> Vec<OrganLibraryEntry> {
    ververs_bibliotheek_namen_eenmalig(state.inner());
    state.organ_library.read().organs.clone()
}

#[tauri::command]
pub fn remove_from_library(state: State<AppState>, id: String) {
    info!("Removing organ from library: {}", id);
    let mut lib = state.organ_library.write();
    // Een handmatig gekozen afbeelding staat als KOPIE in de app-datamap; die
    // hoort met de entry mee te verdwijnen.
    let kopie: Vec<String> = lib.organs.iter()
        .filter(|o| o.id == id)
        .filter_map(|o| o.image_path.clone())
        .collect();
    lib.organs.retain(|o| o.id != id);
    lib.settings.remove(&id);
    drop(lib);
    let map = organ_images_dir(&state);
    for k in kopie {
        if Path::new(&k).starts_with(&map) {
            let _ = std::fs::remove_file(&k);
        }
    }
    state.save_library();
}

#[derive(Debug, Deserialize)]
pub struct SaveSettingsDto {
    pub presets: HashMap<String, PresetData>,
}

use std::collections::HashMap;

/// Async + spawn_blocking: de autosave (elke registratiewijziging, 800 ms
/// debounce) wacht tot 5 s op de load_switch_gate — dat was voorheen 5 s
/// UI-freeze per autosave zodra er een load/wissel bezig was.
#[tauri::command]
pub async fn save_current_organ_settings(state: State<'_, AppState>, presets: HashMap<String, PresetData>) -> Result<(), String> {
    let st = state.inner().clone();
    tokio::task::spawn_blocking(move || do_save_organ_settings(&st, presets))
        .await
        .map_err(|e| format!("instellingen-opslaan-taak mislukt: {}", e))
}

/// Handregistratie = getrokken registers/koppels MINUS de claims van de
/// crescendotrede (lidmaatschap van de claimlijst, geen naamprefix). Gebruikt
/// door de autosave 'laatste stand' en de setzer-SET.
pub fn registration_without_crescendo(
    drawn: &[String],
    couplers: &[String],
    claims: &[String],
) -> (Vec<String>, Vec<String>) {
    let stops = drawn.iter().filter(|s| !claims.contains(s)).cloned().collect();
    let cpl = couplers.iter().filter(|c| !claims.contains(c)).cloned().collect();
    (stops, cpl)
}

/// Kern van het opslaan van per-orgel instellingen — herbruikbaar buiten de Tauri-command
/// (o.a. de test-API). Verzamelt de huidige state in OrganSettings en schrijft naar library
/// + .jm-settings.json.
pub fn do_save_organ_settings(state: &AppState, presets: HashMap<String, PresetData>) {
    use crate::state::MidiPresetTrigger;

    // Serialiseer met orgel-loads/audio-wissels: een save die midden in een
    // load valt las halfgewiste state onder de key van het oude orgel
    // (auditbevinding 64). Best effort: bij een timeout (load die lang duurt)
    // gaan we door — niet erger dan het oude gedrag.
    let _gate = state.load_switch_gate.try_lock_for(std::time::Duration::from_secs(5));
    if _gate.is_none() {
        warn!("Instellingen opslaan zonder load-gate (load/wissel loopt al >5s)");
    }

    let organ_id = state.current_organ_id.read().clone();
    let organ_id = match organ_id {
        Some(id) => id,
        None => {
            warn!("No organ loaded, cannot save settings");
            return;
        }
    };

    info!("Saving settings for organ: {}", organ_id);

    // Gather current MIDI mappings
    let midi_mappings: Vec<MidiMappingSaved> = state.get_midi_mappings().into_iter().map(|m| {
        MidiMappingSaved {
            division: m.division,
            channel: m.channel,
            transpose: m.transpose,
            first_midi_note: m.first_midi_note,
            last_midi_note: m.last_midi_note,
            short_octave: m.short_octave,
        }
    }).collect();

    // Gather current preset bindings
    let preset_bindings: Vec<PresetBindingSaved> = state.get_preset_bindings().into_iter().map(|b| {
        match b.trigger {
            MidiPresetTrigger::Note { channel, note } => PresetBindingSaved {
                preset_num: b.preset_num,
                trigger_type: "note".to_string(),
                note: Some(note),
                channel,
                controller: None,
                value: None,
                program: None,
                sysex_hex: None,
                bit: None,
            },
            MidiPresetTrigger::ControlChange { channel, controller, value } => PresetBindingSaved {
                preset_num: b.preset_num,
                trigger_type: "cc".to_string(),
                note: None,
                channel,
                controller: Some(controller),
                value: Some(value),
                program: None,
                sysex_hex: None,
                bit: None,
            },
            other => preset_binding_to_saved(&crate::state::MidiPresetBinding {
                preset_num: b.preset_num,
                trigger: other,
            }),
        }
    }).collect();

    // Gather current swell bindings
    let swell_bindings: Vec<SwellBindingSaved> = state.get_swell_bindings().into_iter().map(|b| {
        SwellBindingSaved {
            division_name: b.division_name,
            division_index: b.division_index,
            channel: b.channel,
            cc_num: b.cc_num,
            min_val: b.min_val,
            max_val: b.max_val,
            invert: b.invert,
            last_value: b.last_value,
        }
    }).collect();

    // Gather current pipe voicings
    let pipe_voicings: Vec<library::PipeVoicingSaved> = state.pipe_voicings.read()
        .iter()
        .map(|(&(stop_id, pipe_num), &(volume_db, pitch_cents))| library::PipeVoicingSaved {
            stop_id, pipe_num, volume_db, pitch_cents,
        })
        .collect();

    // Gather current division-output-channel routing.
    // Terwijl een profiel-kanaaloverride actief is bevat de live state de
    // (tijdelijke) profielrouting — die mag de per-orgel opslag niet
    // overschrijven; bewaar dan de bestaande opgeslagen routing.
    let division_output_pairs: Vec<library::DivisionOutputPairSaved> = if state.profile_channel_override.read().is_some() {
        let lib = state.organ_library.read();
        lib.settings.get(&organ_id)
            .map(|s| s.division_output_pairs.clone())
            .unwrap_or_default()
    } else {
        // Lock-volgorde: ALTIJD eerst loaded_organ_info, dan de divisie-lock
        // (zelfde volgorde als apply_saved_*), anders 3-thread-deadlock (audit 44).
        let organ = state.loaded_organ_info.read();
        let chans = state.division_output_channels.read();
        if let Some(ref o) = *organ {
            o.divisions.iter().enumerate()
                .filter_map(|(i, d)| {
                    chans.get(i).map(|c| library::DivisionOutputPairSaved {
                        division: d.name.clone(),
                        pair: 0,
                        channels: Some(c.clone()),
                    })
                })
                .collect()
        } else {
            Vec::new()
        }
    };

    // Gather current C/Cis-lade spreiding (per-divisie aan/uit + globale parameters)
    let division_ccis: Vec<library::DivisionCcisSaved> = {
        // Lock-volgorde: ALTIJD eerst loaded_organ_info, dan de divisie-lock
        // (zelfde volgorde als apply_saved_*), anders 3-thread-deadlock (audit 44).
        let organ = state.loaded_organ_info.read();
        let en = state.division_ccis_enabled.read();
        if let Some(ref o) = *organ {
            o.divisions.iter().enumerate()
                .filter_map(|(i, d)| {
                    en.get(i).map(|&e| library::DivisionCcisSaved {
                        division: d.name.clone(),
                        enabled: e,
                    })
                })
                .collect()
        } else {
            Vec::new()
        }
    };
    let ccis_spread = {
        let (strength, falloff, swap) = *state.ccis_spread.read();
        Some(library::CcisSpreadSaved { strength, falloff, swap })
    };

    // Laatste stand: getrokken registratie + actieve koppels (hersteld bij laden),
    // MINUS wat de crescendotrede bijtrok: die registers zijn geen
    // handregistratie en zouden na een herstart vast getrokken staan (de trede
    // begint dan op 0 en trekt ze nooit meer weg).
    let (drawn_stops, active_couplers) = registration_without_crescendo(
        &state.drawn_stops.read(), &state.get_active_couplers(), &state.crescendo_active_stops.read());

    // Generaal-crescendo pedaalbinding (channel/cc/min/max/invert) — persistent
    let crescendo_binding = state.crescendo_binding.read().map(|(ch, cc, mn, mx, inv)| library::CrescendoBindingSaved {
        channel: ch, cc_num: cc, min_val: mn, max_val: mx, invert: inv,
    });
    // Crescendo-matrix + aan/uit + kolomaantal (per orgel).
    let crescendo_stages = state.crescendo_stages.read().clone();
    let crescendo_enabled = *state.crescendo_enabled.read();
    let crescendo_num_stages = *state.crescendo_num_stages.read();

    // Gather current division-wind-group toewijzing
    let division_wind_groups: Vec<library::DivisionWindGroupSaved> = {
        // Lock-volgorde: ALTIJD eerst loaded_organ_info, dan de divisie-lock
        // (zelfde volgorde als apply_saved_*), anders 3-thread-deadlock (audit 44).
        let organ = state.loaded_organ_info.read();
        let groups = state.division_wind_groups.read();
        if let Some(ref o) = *organ {
            o.divisions.iter().enumerate()
                .filter_map(|(i, d)| {
                    groups.get(i).map(|&g| library::DivisionWindGroupSaved {
                        division: d.name.clone(),
                        group: g,
                    })
                })
                .collect()
        } else {
            Vec::new()
        }
    };

    let num_wind_groups = *state.num_wind_groups.read();

    // Alle audio-DSP-instellingen worden uit de runtime-mirrors verzameld (gezet door de
    // set_*/persist_*-commands); de reeds opgeslagen waarde dient als fallback wanneer
    // deze sessie nog niets gezet heeft (.or(existing_*)).
    let (existing_master, existing_reverb, existing_eq, existing_temperament, existing_remote_layout) = {
        let lib = state.organ_library.read();
        match lib.settings.get(&organ_id) {
            Some(s) => (s.master_volume_db, s.reverb.clone(), s.eq.clone(), s.temperament.clone(), s.remote_layout.clone()),
            None => (None, None, None, None, None),
        }
    };
    let master_volume_db = (*state.master_volume_db.read()).or(existing_master);
    let temperament = state.temperament_settings.read().clone().or(existing_temperament);
    let reverb = state.reverb_settings.read().clone().or(existing_reverb);
    let eq = state.eq_settings.read().clone().or(existing_eq);

    // Per-divisie DSP-spiegels → opslag (alleen divisies/groepen die de gebruiker zette;
    // de mirrors zijn per orgel gewist bij load, dus geen naam-collisie tussen orgels).
    let division_pans: Vec<library::DivisionPanSaved> = state.division_pans.read().iter()
        .map(|(division, &pan)| library::DivisionPanSaved { division: division.clone(), pan })
        .collect();
    let division_swell_configs: Vec<library::DivisionSwellConfigSaved> = state.division_swell_configs.read().iter()
        .map(|(division, &(min_db, filter_cutoff))| library::DivisionSwellConfigSaved {
            division: division.clone(), min_db, filter_cutoff,
        })
        .collect();
    let division_tremulants: Vec<library::DivisionTremulantSaved> = state.division_tremulants.read().iter()
        .map(|(division, &(enabled, rate, amp_depth, pitch_depth))| library::DivisionTremulantSaved {
            division: division.clone(), enabled, rate, amp_depth, pitch_depth,
        })
        .collect();
    let wind_group_configs: Vec<library::WindGroupConfigSaved> = state.wind_group_configs.read()
        .values().cloned().collect();

    // Microfoonperspectieven: aan/uit (laden) + volume per label.
    let perspectives: Vec<PerspectiveSaved> = state.perspectives.read().iter()
        .map(|p| PerspectiveSaved { name: p.name.clone(), enabled: p.enabled, gain_db: p.gain_db })
        .collect();

    // Indeling van de afstandsbediening: de spiegel van het hoofdvenster, met
    // de reeds opgeslagen indeling als terugval (een opslag vóórdat de Console
    // gepubliceerd heeft mag de bewaarde indeling niet wissen).
    let remote_layout = state.remote_layout.read().clone().or(existing_remote_layout);

    let settings = OrganSettings {
        presets,
        preset_bindings,
        swell_bindings,
        midi_mappings,
        pipe_voicings,
        division_output_pairs,
        division_wind_groups,
        num_wind_groups,
        master_volume_db,
        reverb,
        eq,
        temperament,
        division_ccis,
        ccis_spread,
        drawn_stops,
        active_couplers,
        crescendo_binding,
        crescendo_stages,
        crescendo_enabled,
        crescendo_num_stages,
        division_pans,
        division_swell_configs,
        division_tremulants,
        wind_group_configs,
        perspectives,
        load_all_perspectives: *state.load_all_perspectives.read(),
        extra_couplers: state.extra_coupler_ids.read().clone(),
        continuous_keyboard: crate::state::DOORLOPEND_KLAVIER.load(std::sync::atomic::Ordering::Relaxed),
        remote_layout,
    };

    // Save to organ directory as well (.jm-settings.json next to the organ)
    if let Some(dir) = organ_settings_dir(&organ_id) {
        let settings_path = dir.join(".jm-settings.json");
        match serde_json::to_string_pretty(&settings) {
            Ok(json) => {
                if let Err(e) = std::fs::write(&settings_path, json) {
                    warn!("Failed to write organ settings to {:?}: {}", settings_path, e);
                } else {
                    info!("Saved organ settings to {:?}", settings_path);
                }
            }
            Err(e) => warn!("Failed to serialize organ settings: {}", e),
        }
    }

    {
        let mut lib = state.organ_library.write();
        lib.settings.insert(organ_id, settings);
    }
    state.save_library();
}

/// Get saved settings for the current organ (from library or organ directory),
/// afgestemd op het geladen orgel (instellingen_voor_frontend).
#[tauri::command]
pub fn get_organ_settings(state: State<AppState>) -> Option<OrganSettings> {
    let organ_id = state.current_organ_id.read().clone()?;
    let settings = lees_opgeslagen_instellingen(&state, &organ_id)?;
    Some(instellingen_voor_frontend(&state, settings))
}

/// Ruwe opgeslagen instellingen: bibliotheek (AppData), anders de
/// .jm-settings.json naast het orgel. Geen afstemming op het geladen orgel —
/// dat doet instellingen_voor_frontend.
fn lees_opgeslagen_instellingen(state: &AppState, organ_id: &str) -> Option<OrganSettings> {
    // First try library (AppData)
    {
        let lib = state.organ_library.read();
        if let Some(settings) = lib.settings.get(organ_id) {
            return Some(settings.clone());
        }
    }

    // Fallback: try .jm-settings.json in organ directory
    if let Some(dir) = organ_settings_dir(organ_id) {
        let settings_path = dir.join(".jm-settings.json");
        if let Ok(json) = std::fs::read_to_string(&settings_path) {
            if let Ok(settings) = serde_json::from_str::<OrganSettings>(&json) {
                info!("Loaded organ settings from {:?}", settings_path);
                return Some(settings);
            }
        }
    }

    None
}

/// Opgeslagen instellingen zoals de frontend ze mag TOEPASSEN: de nagebootste
/// tremulant staat uit voor divisies met echte opnamen. Zonder deze stap zette
/// Console.svelte (loadAudioSettingsForOrgan → tremLfoEnabled →
/// set_tremulant_lfo) een 0.7.38-`enabled=true` alsnog als LFO over de opnamen
/// heen en pushte hij die stand via persist_tremulant_config terug in de
/// spiegel. Zonder geladen orgel ongewijzigd. Logt niets: restore_organ_settings
/// meldde het al één keer bij de load.
fn instellingen_voor_frontend(state: &AppState, mut settings: OrganSettings) -> OrganSettings {
    let organ = state.loaded_organ_info.read();
    if let Some(o) = organ.as_ref() {
        let (trems, _) = nagebootste_tremulant_voor_herstel(&settings.division_tremulants, &o.divisions);
        settings.division_tremulants = trems;
    }
    settings
}

/// Export all settings to a JSON file
#[tauri::command]
pub fn export_settings(state: State<AppState>, path: String) -> Result<(), String> {
    let organ_id = state.current_organ_id.read().clone()
        .ok_or("Geen orgel geladen")?;
    let lib = state.organ_library.read();
    let settings = lib.settings.get(&organ_id)
        .ok_or("Geen instellingen gevonden")?;
    let json = serde_json::to_string_pretty(settings)
        .map_err(|e| format!("Serialisatie fout: {}", e))?;
    std::fs::write(&path, json)
        .map_err(|e| format!("Kan niet schrijven naar {}: {}", path, e))?;
    info!("Exported settings to {}", path);
    Ok(())
}

/// Import settings from a JSON file
#[tauri::command]
pub fn import_settings(state: State<AppState>, path: String) -> Result<OrganSettings, String> {
    let json = std::fs::read_to_string(&path)
        .map_err(|e| format!("Kan niet lezen: {}", e))?;
    let settings: OrganSettings = serde_json::from_str(&json)
        .map_err(|e| format!("Ongeldig formaat: {}", e))?;

    // Save to library
    let organ_id = state.current_organ_id.read().clone()
        .ok_or("Geen orgel geladen")?;
    {
        let mut lib = state.organ_library.write();
        lib.settings.insert(organ_id, settings.clone());
    }
    state.save_library();
    info!("Imported settings from {}", path);
    Ok(settings)
}

/// Map waarin handmatig gekozen orgelafbeeldingen als kopie terechtkomen.
/// Kopiëren (i.p.v. het pad onthouden) is bewust: de bron kan een USB-stick of
/// een tijdelijke map zijn.
fn organ_images_dir(state: &AppState) -> std::path::PathBuf {
    state.app_data_dir.join("organ_images")
}

/// Korte, stabiele bestandsnaam voor de kopie (FNV-1a 64 over het orgel-id).
fn image_copy_stem(id: &str) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in id.to_lowercase().as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{:016x}", h)
}

/// Was de bronmap waarin `find_organ_image` zoekt op dít moment leesbaar?
///
/// Nodig om de negatieve cache (`image_searched`) eerlijk te houden: de
/// zoektocht geeft None zowel bij "gezocht en niets gevonden" als bij "kon
/// niet zoeken" (USB eruit, netwerkschijf weg, geen rechten). Zonder dit
/// onderscheid zou zo'n orgel voorgoed als "geen foto" vastliggen, ook nadat
/// de schijf er weer is.
///
/// `read_dir` en niet alleen `is_dir()`: een netwerkpad kan bestaan en tóch
/// onleesbaar zijn. De mappaden volgen `find_organ_image`: bij een
/// orgelbestand de map eromheen, bij een sample-map het pad zelf.
///
/// Hauptwerk: de zoektocht loopt óók door `<pakketwortel>/OrganInstallationPackages/<pakket>/`
/// (daar staat de consolefoto, niet naast de definitie). Staan de pakketten op
/// een losgekoppelde schijf terwijl de definitiemap wél leesbaar is, dan is er
/// dus NIET doorzocht — anders lag "geen foto" voorgoed vast. Alleen als de
/// pakketmap leesbaar was, telt de zoektocht als gedaan; geen pakketwortel
/// (`find_package_root` = None, de pakketten zijn onvindbaar) telt evenmin.
fn afbeeldingsbron_leesbaar(source_path: &str, source_type: &str) -> bool {
    let src = Path::new(source_path);
    let is_organ_file = source_type == "organ_file";
    let dir = if is_organ_file {
        match src.parent() {
            Some(p) => p.to_path_buf(),
            None => return false,
        }
    } else {
        src.to_path_buf()
    };
    if std::fs::read_dir(&dir).is_err() {
        return false;
    }
    if is_organ_file && vpo_sampler::is_hauptwerk_path(src) {
        return match vpo_sampler::find_package_root(src) {
            Some(root) => std::fs::read_dir(root.join("OrganInstallationPackages")).is_ok(),
            None => false,
        };
    }
    true
}

/// Afbeelding voor een bibliotheekkaart, op ID (= genormaliseerd bronpad).
///
/// Gebruikt de afbeelding die in de bibliotheek is opgeslagen; alleen als die
/// er niet is én er met deze zoekversie nog niet gezocht is, wordt de
/// zoektocht gedaan — en het resultaat (ook "niets gevonden") in de entry
/// vastgelegd. Zonder die negatieve cache scande elke opening van de
/// bibliotheek, en élk extra registervenster, de hele sampleset opnieuw.
#[tauri::command]
pub fn get_organ_image(state: State<AppState>, id: String) -> Option<String> {
    let norm = |s: &str| s.replace('/', "\\").to_lowercase();
    let key = norm(&id);

    let (pad, source_path, source_type, manual, al_gezocht, in_lib) = {
        let lib = state.organ_library.read();
        match lib.organs.iter().find(|o| norm(&o.id) == key || norm(&o.source_path) == key) {
            Some(e) => (
                e.image_path.clone(),
                e.source_path.clone(),
                e.source_type.clone(),
                e.image_manual,
                e.image_searched == Some(library::IMAGE_SEARCH_VERSION),
                true,
            ),
            None => (
                None,
                id.clone(),
                if is_organ_file_id(&id) { "organ_file".to_string() } else { "sample_directory".to_string() },
                false,
                false,
                false,
            ),
        }
    };

    if let Some(p) = pad.as_ref() {
        if Path::new(p).is_file() {
            return library::image_to_base64(p);
        }
    }
    // Handmatige keuze waarvan het bestand weg is: niet stilletjes een andere
    // afbeelding gaan zoeken — de gebruiker heeft bewust gekozen.
    if manual || al_gezocht {
        return None;
    }

    let found = library::find_organ_image(&source_path, &source_type);
    // "Niets gevonden" en "kon niet zoeken" zien er allebei uit als None. Alleen
    // het eerste mag in de negatieve cache: anders krijgt een orgel op een
    // losgekoppelde USB- of netwerkschijf voorgoed het stempel "geen foto".
    let bron_leesbaar = found.is_some() || afbeeldingsbron_leesbaar(&source_path, &source_type);
    if in_lib && bron_leesbaar {
        let mut lib = state.organ_library.write();
        if let Some(e) = lib.organs.iter_mut().find(|o| norm(&o.id) == key || norm(&o.source_path) == key) {
            e.image_path = found.clone();
            e.image_searched = Some(library::IMAGE_SEARCH_VERSION);
        }
        drop(lib); // save_library() neemt zelf een read-lock (parking_lot)
        state.save_library();
    }
    library::image_to_base64(&found?)
}

/// Kies handmatig een afbeelding voor een bibliotheekkaart, of zet hem terug
/// op automatisch zoeken (`path = None`). Het gekozen bestand wordt naar
/// `<app-data>/organ_images/` gekopieerd zodat de kaart blijft werken als de
/// bron (USB, Downloads) verdwijnt.
#[tauri::command]
pub fn set_organ_image(state: State<AppState>, id: String, path: Option<String>) -> Result<OrganLibraryEntry, String> {
    let norm = |s: &str| s.replace('/', "\\").to_lowercase();
    let key = norm(&id);
    let map = organ_images_dir(&state);

    // Doelbestand eerst klaarzetten (buiten de lock — kopiëren kan traag zijn).
    let nieuw: Option<String> = match path.as_ref() {
        Some(src) => {
            let src_path = Path::new(src);
            if !library::has_image_extension(src_path) {
                return Err("Geen ondersteund afbeeldingsformaat (jpg, jpeg, png, bmp, webp)".to_string());
            }
            if !src_path.is_file() {
                return Err(format!("Bestand niet gevonden: {}", src));
            }
            let ext = src_path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_else(|| "jpg".into());
            std::fs::create_dir_all(&map).map_err(|e| format!("Kan map niet maken: {}", e))?;
            let doel = map.join(format!("{}.{}", image_copy_stem(&key), ext));
            std::fs::copy(src_path, &doel).map_err(|e| format!("Kopiëren mislukt: {}", e))?;
            Some(doel.to_string_lossy().to_string())
        }
        None => None,
    };

    let (entry, oud) = {
        let mut lib = state.organ_library.write();
        let e = lib.organs.iter_mut()
            .find(|o| norm(&o.id) == key || norm(&o.source_path) == key)
            .ok_or_else(|| format!("Orgel niet in de bibliotheek: {}", id))?;
        let oud = e.image_path.clone();
        match &nieuw {
            Some(p) => {
                e.image_path = Some(p.clone());
                e.image_manual = true;
                e.image_searched = Some(library::IMAGE_SEARCH_VERSION);
            }
            None => {
                // Terug naar automatisch: de negatieve cache wissen zodat er
                // bij de volgende opening opnieuw gezocht wordt.
                e.image_path = None;
                e.image_manual = false;
                e.image_searched = None;
            }
        }
        (e.clone(), oud)
    };
    // Oude kopie opruimen (alleen in onze eigen map, nooit het origineel van
    // de gebruiker).
    if let Some(o) = oud {
        if Some(&o) != nieuw.as_ref() && Path::new(&o).starts_with(&map) {
            let _ = std::fs::remove_file(&o);
        }
    }
    state.save_library();
    info!("Orgelafbeelding ingesteld voor {}: {:?}", id, entry.image_path);
    Ok(entry)
}

#[tauri::command]
pub fn get_saved_presets(state: State<AppState>) -> HashMap<String, PresetData> {
    let organ_id = state.current_organ_id.read().clone();
    let organ_id = match organ_id {
        Some(id) => id,
        None => return HashMap::new(),
    };
    let lib = state.organ_library.read();
    lib.settings.get(&organ_id).map(|s| s.presets.clone()).unwrap_or_default()
}

#[tauri::command]
pub fn restore_preset_bindings(state: State<AppState>, bindings: Vec<PresetBindingSaved>) {
    use crate::state::{MidiPresetBinding, MidiPresetTrigger};

    let midi_bindings: Vec<MidiPresetBinding> = bindings.into_iter().filter_map(|b| {
        saved_to_preset_binding(&b).ok()
    }).collect();
    state.set_preset_bindings_replace(midi_bindings);
}

#[tauri::command]
pub fn restore_swell_bindings(state: State<AppState>, bindings: Vec<SwellBindingSaved>) {
    let swell = swell_bindings_from_saved(&state, &bindings);
    state.set_swell_bindings_replace(swell);
    apply_swell_positions(&state);
}

/// Opgeslagen zwelbindingen → runtime-bindingen. De divisie-index wordt uit de
/// divisienaam van het geladen orgel herleid; een binding op een naam die dit
/// orgel niet (meer) kent wordt overgeslagen (met melding) — anders stuurde de
/// zweltrede na een her-import met verschoven divisievolgorde het verkeerde
/// klavier aan. Zonder geladen orgel blijft de opgeslagen index gelden.
fn swell_bindings_from_saved(state: &AppState, saved: &[SwellBindingSaved]) -> Vec<crate::state::SwellBinding> {
    use crate::state::SwellBinding;
    let division_names: Option<Vec<String>> = state.loaded_organ_info.read().as_ref()
        .map(|o| o.divisions.iter().map(|d| d.name.clone()).collect());
    saved.iter().filter_map(|b| {
        let division_index = match &division_names {
            Some(names) => match names.iter().position(|n| *n == b.division_name) {
                Some(i) => i as u8,
                None => {
                    warn!("Zwelbinding voor '{}' overgeslagen: divisie bestaat niet in dit orgel", b.division_name);
                    return None;
                }
            },
            None => b.division_index,
        };
        Some(SwellBinding {
            division_name: b.division_name.clone(),
            division_index,
            channel: b.channel,
            cc_num: b.cc_num,
            min_val: b.min_val,
            max_val: b.max_val,
            invert: b.invert,
            last_value: b.last_value,
        })
    }).collect()
}

/// Zet de laatst bekende zwelstand per binding terug op de audio-thread én in
/// de UI-spiegel (division_gains). Nodig na elke orgel-(her)laad en audio-
/// wissel: RegisterStopDivisionMap/ClearSamples zetten de audio-gains hard op
/// 1.0 terwijl de fysieke trede dicht kan staan. Zonder bekende stand blijft
/// 1.0 (open) het veilige default.
pub fn apply_swell_positions(state: &AppState) {
    use crate::state::swell_gain_from_cc;
    let bindings = state.get_swell_bindings();
    let mut n = 0;
    for b in bindings.iter() {
        let Some(v) = b.last_value else { continue; };
        let gain = swell_gain_from_cc(v, b.min_val, b.max_val, b.invert);
        state.set_division_gain(b.division_index, gain);
        n += 1;
    }
    if n > 0 {
        info!("Zwelstand hersteld voor {} divisie(s)", n);
    }
}

// ============ MP3-recorder ============

/// Standaardmap voor opnames: ~/Documents/JM-Orgue-opnames (Windows: Mijn Documenten).
fn recordings_default_dir() -> std::path::PathBuf {
    if let Some(docs) = dirs::document_dir() {
        return docs.join("JM-Orgue-opnames");
    }
    std::path::PathBuf::from("JM-Orgue-opnames")
}

/// Genereer een tijdgestempeld bestandspad voor een nieuwe opname.
/// Formaat: jm-orgue-YYYYMMDD-HHMMSS.mp3 (UTC — zie epoch_to_ymdhms).
fn make_recording_path(custom: Option<String>) -> std::path::PathBuf {
    if let Some(p) = custom { return std::path::PathBuf::from(p); }
    make_timestamped_recording_path("mp3")
}

/// Bouw een tijdgestempeld opnamepad met de gegeven extensie in de standaard opnamemap.
/// Formaat: jm-orgue-YYYYMMDD-HHMMSS.<ext> (UTC — epoch_to_ymdhms doet geen tijdzone).
fn make_timestamped_recording_path(ext: &str) -> std::path::PathBuf {
    // Geen chrono-afhankelijkheid: gebruik epoch-seconden + simpele kalender-conversie.
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    // Converteer naar UTC datetime-componenten (eenvoudige Gregoriaanse conversie).
    let (y, mo, d, h, mi, s) = epoch_to_ymdhms(secs);
    let base = format!("jm-orgue-{:04}{:02}{:02}-{:02}{:02}{:02}", y, mo, d, h, mi, s);
    let dir = recordings_default_dir();
    // Voorkom overschrijven als twee opnames binnen dezelfde seconde starten: voeg een
    // teller toe (-1, -2, ...) als het bestand al bestaat.
    let mut path = dir.join(format!("{}.{}", base, ext));
    let mut counter = 1;
    while path.exists() && counter < 1000 {
        path = dir.join(format!("{}-{}.{}", base, counter, ext));
        counter += 1;
    }
    path
}

/// Zet epoch-seconden om naar (jaar, maand, dag, uur, min, sec) — UTC.
/// pub(crate): midi_archive.rs gebruikt dit als niet-Windows-fallback.
#[allow(dead_code)]
pub(crate) fn epoch_to_ymdhms(epoch: u64) -> (u32, u32, u32, u32, u32, u32) {
    let s = (epoch % 60) as u32;
    let m = ((epoch / 60) % 60) as u32;
    let h = ((epoch / 3600) % 24) as u32;
    let mut days = (epoch / 86400) as i64;
    // 1970-01-01 = donderdag, dag 0.
    let mut year = 1970i32;
    loop {
        let leap = (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0);
        let dy = if leap { 366 } else { 365 };
        if days < dy { break; }
        days -= dy;
        year += 1;
    }
    let leap = (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0);
    let mdays = [31, if leap {29} else {28}, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let mut month = 0usize;
    while month < 12 && days >= mdays[month] {
        days -= mdays[month];
        month += 1;
    }
    (year as u32, (month + 1) as u32, (days + 1) as u32, h, m, s)
}

/// Start een MP3-opname van de live audio-output. Default-pad indien `path` leeg.
#[tauri::command]
pub fn start_recording(state: State<AppState>, path: Option<String>) -> Result<String, String> {
    let out_path = make_recording_path(path);

    // Sample rate uit de audio-player lezen (anders 48 kHz fallback).
    let sr: u32 = {
        let ap = state.audio_player.read();
        ap.as_ref().map(|p| *p.sample_rate.read()).unwrap_or(48000)
    };

    // Koppel de recorder aan de audio-thread (globaal static, één opname tegelijk).
    *crate::audio::RECORDER.write() = Some(state.recorder.clone());

    state.recorder.start(out_path.clone(), sr)?;
    Ok(out_path.to_string_lossy().to_string())
}

/// Stop de actieve opname, flush + sluit het bestand.
#[tauri::command]
pub fn stop_recording(state: State<AppState>) -> Result<crate::recorder::RecorderStatus, String> {
    state.recorder.stop()?;
    let s = state.recorder.status();
    // Laat de globale RECORDER hangen — een vervolgopname herstart de sender opnieuw.
    Ok(s)
}

/// Status van de huidige (of laatste) opname — voor de UI-poll.
#[tauri::command]
pub fn get_recording_status(state: State<AppState>) -> crate::recorder::RecorderStatus {
    state.recorder.status()
}

/// Zet of de getrokken registratie ("laatste stand") bij orgel-load hersteld wordt.
/// Default = false (schone start). De frontend zet dit bij opstart op basis van een
/// gebruikersinstelling.
#[tauri::command]
pub fn set_restore_registration(state: State<AppState>, enabled: bool) {
    *state.restore_registration.write() = enabled;
}

#[tauri::command]
pub fn get_restore_registration(state: State<AppState>) -> bool {
    *state.restore_registration.read()
}

/// Lees de standaard-opnamemap zodat de UI hem kan tonen / openen.
#[tauri::command]
pub fn get_recordings_dir() -> String {
    recordings_default_dir().to_string_lossy().to_string()
}

// ============ Autostart bij Windows + Computer afsluiten ============

/// Naam waaronder we autostart registreren in HKCU\Software\Microsoft\Windows\CurrentVersion\Run.
const AUTOSTART_VALUE_NAME: &str = "JM-Orgue";

/// Bepaal het pad naar de geïnstalleerde vpo-app.exe (huidige executable).
/// GEEN letterlijke aanhalingstekens toevoegen: std::process::Command quote't de
/// argument zelf correct op de Windows-commandline, zodat reg.exe het BARE pad
/// (inclusief spaties) als registry-waarde opslaat. Zelf quotes toevoegen zou
/// letterlijke aanhalingstekens in de waarde zetten en autostart laten falen.
fn current_exe_path() -> Result<String, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    Ok(exe.to_string_lossy().to_string())
}

/// Staart van het logbestand (en van de vorige sessie, zie de log-rotatie in
/// main.rs) voor de feedback-popup. max_kb begrenst de huidige log; de vorige
/// sessie krijgt de helft, zodat het totaal klein genoeg blijft voor de
/// mail-relay.
#[tauri::command]
pub fn get_log_tail(max_kb: Option<u32>) -> Result<String, String> {
    fn tail_of(path: &std::path::Path, max_bytes: usize) -> Option<String> {
        let data = std::fs::read(path).ok()?;
        if data.is_empty() { return None; }
        let start = data.len().saturating_sub(max_bytes);
        // Niet midden in een regel beginnen: doorschuiven tot na de eerstvolgende newline.
        let mut s = start;
        if s > 0 {
            while s < data.len() && data[s] != b'\n' { s += 1; }
            s = (s + 1).min(data.len());
        }
        Some(String::from_utf8_lossy(&data[s..]).into_owned())
    }
    let dir = dirs::data_dir()
        .map(|d| d.join("nl.jm-orgue.app"))
        .ok_or_else(|| "Geen data-map gevonden".to_string())?;
    let per_file = (max_kb.unwrap_or(24).clamp(1, 128) as usize) * 1024;
    let mut out = String::new();
    if let Some(t) = tail_of(&dir.join("jm-orgue.log"), per_file) {
        out.push_str("==== huidige sessie (jm-orgue.log) ====\n");
        out.push_str(&t);
    }
    if let Some(t) = tail_of(&dir.join("jm-orgue.log.1"), per_file / 2) {
        out.push_str("\n==== vorige sessie (jm-orgue.log.1) ====\n");
        out.push_str(&t);
    }
    if out.is_empty() { out.push_str("(geen logbestand gevonden)"); }
    Ok(out)
}

/// Download een sampleset-zip (GitHub-release-asset uit ons eigen
/// samplesets.json-manifest) en pak hem uit in `doel_map`. De map wordt
/// aangemaakt en moet nieuw of leeg zijn. Voortgang via het event
/// "jm-orgue:sampleset-download" ({id, fase: "download"|"uitpakken", pct}).
/// Bij een fout wordt de (door ons aangemaakte) doelmap weer opgeruimd.
#[tauri::command]
pub async fn download_sampleset(app: tauri::AppHandle, id: String, url: String, doel_map: String) -> Result<String, String> {
    use tauri::Emitter;
    // Alleen onze eigen distributieroute: GitHub (release-assets redirecten
    // naar objects.githubusercontent.com — dat volgt ureq zelf).
    if !url.starts_with("https://github.com/") {
        return Err("Alleen GitHub-URL's uit het sampleset-manifest zijn toegestaan".to_string());
    }
    let doel = std::path::PathBuf::from(&doel_map);
    if doel.exists() && std::fs::read_dir(&doel).map(|mut d| d.next().is_some()).unwrap_or(true) {
        return Err(format!("Doelmap bestaat al en is niet leeg: {}", doel_map));
    }
    let emit = {
        let app = app.clone();
        let id = id.clone();
        move |fase: &str, pct: u32| {
            let _ = app.emit("jm-orgue:sampleset-download", serde_json::json!({
                "id": id, "fase": fase, "pct": pct.min(100),
            }));
        }
    };
    let result = tokio::task::spawn_blocking(move || -> Result<String, String> {
        std::fs::create_dir_all(&doel).map_err(|e| format!("Doelmap aanmaken mislukt: {}", e))?;
        let zip_pad = doel.join(".download.zip");
        // ---- Downloaden (streaming, met voortgang op content-length) ----
        let resp = ureq::get(&url).call().map_err(|e| format!("Download mislukt: {}", e))?;
        let totaal: u64 = resp.header("Content-Length")
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        {
            let mut reader = resp.into_reader();
            let mut file = std::fs::File::create(&zip_pad)
                .map_err(|e| format!("Zip wegschrijven mislukt: {}", e))?;
            let mut buf = [0u8; 65536];
            let mut gelezen: u64 = 0;
            let mut laatst_pct = 0u32;
            loop {
                let n = std::io::Read::read(&mut reader, &mut buf)
                    .map_err(|e| format!("Download afgebroken: {}", e))?;
                if n == 0 { break; }
                std::io::Write::write_all(&mut file, &buf[..n])
                    .map_err(|e| format!("Zip wegschrijven mislukt: {}", e))?;
                gelezen += n as u64;
                if totaal > 0 {
                    let pct = ((gelezen * 100) / totaal) as u32;
                    if pct > laatst_pct { laatst_pct = pct; emit("download", pct); }
                }
            }
        }
        // ---- Uitpakken (zip-slip-veilig via enclosed_name) ----
        emit("uitpakken", 0);
        let file = std::fs::File::open(&zip_pad).map_err(|e| e.to_string())?;
        let mut archief = zip::ZipArchive::new(file).map_err(|e| format!("Zip onleesbaar: {}", e))?;
        let n_entries = archief.len().max(1);
        for i in 0..archief.len() {
            let mut entry = archief.by_index(i).map_err(|e| e.to_string())?;
            let Some(rel) = entry.enclosed_name() else { continue; };
            let uit = doel.join(rel);
            if entry.is_dir() {
                std::fs::create_dir_all(&uit).map_err(|e| e.to_string())?;
            } else {
                if let Some(parent) = uit.parent() {
                    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                }
                let mut f = std::fs::File::create(&uit).map_err(|e| e.to_string())?;
                std::io::copy(&mut entry, &mut f).map_err(|e| format!("Uitpakken mislukt: {}", e))?;
            }
            emit("uitpakken", ((i + 1) * 100 / n_entries) as u32);
        }
        let _ = std::fs::remove_file(&zip_pad);
        info!("Sampleset gedownload en uitgepakt naar {:?}", doel);
        Ok(doel.to_string_lossy().to_string())
    }).await.map_err(|e| format!("Downloadtaak crashte: {}", e))?;
    if result.is_err() {
        // Halve download niet laten slingeren; de map was van ons.
        let _ = std::fs::remove_dir_all(std::path::Path::new(&doel_map));
    }
    result
}

/// Lees of JM-Orgue ingesteld staat om automatisch te starten bij Windows-aanmelding.
#[tauri::command]
pub fn get_autostart_enabled() -> Result<bool, String> {
    #[cfg(target_os = "windows")]
    {
        let output = std::process::Command::new("reg")
            .args(["query", "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Run", "/v", AUTOSTART_VALUE_NAME])
            .output()
            .map_err(|e| e.to_string())?;
        Ok(output.status.success())
    }
    #[cfg(not(target_os = "windows"))]
    { Ok(false) }
}

/// Zet/wis de autostart-registratie. Op niet-Windows platforms is dit een no-op.
#[tauri::command]
pub fn set_autostart_enabled(enabled: bool) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        if enabled {
            let exe = current_exe_path()?;
            let status = std::process::Command::new("reg")
                .args(["add", "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Run",
                       "/v", AUTOSTART_VALUE_NAME, "/t", "REG_SZ", "/d", &exe, "/f"])
                .status()
                .map_err(|e| e.to_string())?;
            if !status.success() { return Err("reg add failed".to_string()); }
            info!("Autostart enabled: {}", exe);
        } else {
            // Negeer fout als de waarde toch al weg was.
            let _ = std::process::Command::new("reg")
                .args(["delete", "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Run",
                       "/v", AUTOSTART_VALUE_NAME, "/f"])
                .status();
            info!("Autostart disabled");
        }
        Ok(())
    }
    #[cfg(not(target_os = "windows"))]
    { let _ = enabled; Err("Autostart is alleen op Windows ondersteund".to_string()) }
}

/// Sluit de software en de computer netjes af. Op Windows: `shutdown /s /t 0`.
/// Houdt rekening met opslaan van de laatste stand: de frontend roept eerst
/// flushAutoSave + persistOrganSettings aan voordat deze command wordt aangeroepen.
#[tauri::command]
pub fn shutdown_computer() -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        // /s = shutdown, /t 0 = direct, /f = force (sluit apps zonder prompts).
        let status = std::process::Command::new("shutdown")
            .args(["/s", "/t", "0", "/f"])
            .status()
            .map_err(|e| e.to_string())?;
        if !status.success() { return Err("shutdown command faalde".to_string()); }
        Ok(())
    }
    #[cfg(not(target_os = "windows"))]
    { Err("Computer afsluiten is alleen op Windows ondersteund".to_string()) }
}

#[cfg(test)]
mod perspective_plan_tests {
    use super::{plan_perspectives_custom, plan_perspectives_from, PerspectiveSaved};
    use vpo_sampler::custom_organ::{CustomOrgan, CustomPerspective, StopFamily};

    fn saved(name: &str, enabled: bool, gain_db: f32) -> PerspectiveSaved {
        PerspectiveSaved { name: name.into(), enabled, gain_db }
    }

    #[test]
    fn zonder_opslag_alleen_primaire_labels_aan() {
        // SJDL-vorm: front primair, rear/dry extra → alleen front aan.
        let obs = vec![
            ("front".to_string(), true, 56), ("rear".to_string(), false, 56), ("dry".to_string(), false, 56),
            ("front".to_string(), true, 30), ("rear".to_string(), false, 30), ("dry".to_string(), false, 30),
        ];
        let plan = plan_perspectives_from(obs, &[]);
        let names: Vec<&str> = plan.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, vec!["front", "rear", "dry"]);
        assert_eq!(plan.iter().map(|p| p.enabled).collect::<Vec<_>>(), vec![true, false, false]);
        assert_eq!(plan.iter().map(|p| p.slot).collect::<Vec<_>>(), vec![1, 2, 3]);
        assert_eq!(plan[0].pipe_count, 86);
        assert!(plan.iter().all(|p| !p.loaded && p.gain_db == 0.0));
    }

    #[test]
    fn opgeslagen_rear_aan_wordt_gehonoreerd_en_onbekend_genegeerd() {
        let obs = vec![("front".to_string(), true, 10), ("rear".to_string(), false, 10)];
        let plan = plan_perspectives_from(obs, &[saved("rear", true, -6.0), saved("front", false, 99.0), saved("spook", true, 0.0)]);
        assert_eq!(plan.len(), 2, "onbekend label 'spook' komt niet in de lijst");
        assert!(!plan[0].enabled && plan[0].gain_db == 12.0, "front uit, gain geclampt op +12");
        assert!(plan[1].enabled && (plan[1].gain_db + 6.0).abs() < 1e-6);
    }

    #[test]
    fn extra_laag_label_zonder_primair_staat_standaard_uit() {
        // GO-set met ongelabelde laag 0 en een "(rear)"-rank als extra laag:
        // rear komt nergens als primair voor → standaard uit (anders dubbel).
        let obs = vec![("rear".to_string(), false, 20)];
        let plan = plan_perspectives_from(obs, &[]);
        assert_eq!(plan.len(), 1);
        assert!(!plan[0].enabled);
        // Echte ranks zonder label leveren geen waarneming en dus geen entry.
        let empty = plan_perspectives_from(Vec::<(String, bool, usize)>::new(), &[]);
        assert!(empty.is_empty());
    }

    #[test]
    fn hoogstens_vijftien_labels() {
        let obs: Vec<(String, bool, usize)> = (0..20).map(|i| (format!("mic{}", i), i == 0, 1)).collect();
        let plan = plan_perspectives_from(obs, &[]);
        assert_eq!(plan.len(), 15);
        assert_eq!(plan.last().unwrap().slot, 15);
    }

    #[test]
    fn custom_plan_uit_submappen() {
        let mut organ = CustomOrgan::new("MicTest");
        let div = organ.add_division("HW", 36, 96);
        let s = div.add_stop("Prestant", 8.0, StopFamily::Principal);
        s.add_pipe(36, "HW/Prestant_8/Front/036-c.wav".into());
        s.primary_perspective = Some("Front".into());
        s.perspectives.push(CustomPerspective {
            name: "Rear".into(),
            pipes: vec![],
            tremulant_pipes: vec![],
        });
        let plan = plan_perspectives_custom(&organ, &[]);
        assert_eq!(plan.iter().map(|p| (p.name.as_str(), p.enabled)).collect::<Vec<_>>(), vec![("Front", true), ("Rear", false)]);
        // Stop zonder posities → geen labels.
        let plain = CustomOrgan::new("Plain");
        assert!(plan_perspectives_custom(&plain, &[]).is_empty());
    }
}

#[cfg(test)]
mod smf_tests {
    use super::{encode_smf, encode_smf_with_name};

    /// Lees een big-endian u32 op offset.
    fn be_u32(d: &[u8], off: usize) -> u32 {
        u32::from_be_bytes([d[off], d[off + 1], d[off + 2], d[off + 3]])
    }

    #[test]
    fn encode_smf_with_name_emits_ff03() {
        let events = vec![(0u64, 0xC0u8, 5u8, 0u8, 0u8)];
        let data = encode_smf_with_name(&events, Some("Bätz"));
        let track_len = be_u32(&data, 18) as usize;
        let track = &data[22..22 + track_len];
        // Track: [tempo-meta 7 bytes] [00 FF 03 len naam...] [PC] [eot]
        assert_eq!(&track[0..7], &[0x00, 0xFF, 0x51, 0x03, 0x07, 0xA1, 0x20]);
        let name = "Bätz".as_bytes();
        assert_eq!(&track[7..10], &[0x00, 0xFF, 0x03]);
        assert_eq!(track[10] as usize, name.len());
        assert_eq!(&track[11..11 + name.len()], name);
        // PC-event volgt direct op de naam.
        assert_eq!(&track[11 + name.len()..11 + name.len() + 3], &[0x00, 0xC0, 0x05]);
        // Zonder naam: byte-identiek aan de wrapper (geen FF 03).
        assert_eq!(encode_smf_with_name(&events, None), encode_smf(&events));
        assert!(!encode_smf(&events).windows(2).any(|w| w == [0xFF, 0x03]));
        // Lege naam → geen meta.
        assert_eq!(encode_smf_with_name(&events, Some("")), encode_smf(&events));
        // Lange naam wordt op tekens afgekapt (120) en blijft geldige UTF-8.
        let long: String = std::iter::repeat('ä').take(200).collect();
        let d2 = encode_smf_with_name(&events, Some(&long));
        let t2 = &d2[22..];
        // 120 tekens × 2 bytes = 240 → var-len in twee bytes (0x81 0x70).
        assert_eq!(&t2[7..12], &[0x00, 0xFF, 0x03, 0x81, 0x70]);
        let smf = midly::Smf::parse(&d2).expect("geldige SMF");
        let has_name = smf.tracks[0].iter().any(|ev| matches!(ev.kind,
            midly::TrackEventKind::Meta(midly::MetaMessage::TrackName(n)) if n.len() == 240));
        assert!(has_name);
    }

    #[test]
    fn encodes_valid_smf_header_and_trailer() {
        // Twee noten (on/off) op kanaal 0.
        let events = vec![
            (0u64, 0x90u8, 60u8, 100u8, 0u8),
            (500_000u64, 0x80u8, 60u8, 0u8, 0u8),
        ];
        let data = encode_smf(&events);

        // Header chunk
        assert_eq!(&data[0..4], b"MThd");
        assert_eq!(be_u32(&data, 4), 6); // header length
        assert_eq!(&data[8..10], &[0x00, 0x00]); // format 0
        assert_eq!(&data[10..12], &[0x00, 0x01]); // 1 track
        assert_eq!(&data[12..14], &[0x01, 0xE0]); // 480 PPQ

        // Track chunk
        assert_eq!(&data[14..18], b"MTrk");
        let track_len = be_u32(&data, 18) as usize;
        let track = &data[22..];
        assert_eq!(track.len(), track_len, "MTrk-lengteveld moet kloppen met de werkelijke trackbytes");
        // Eindigt op end-of-track meta-event.
        assert_eq!(&track[track.len() - 3..], &[0xFF, 0x2F, 0x00]);
        // Bevat het NoteOn-statusbyte ergens in de track.
        assert!(track.contains(&0x90));
    }

    #[test]
    fn program_change_omits_second_data_byte() {
        // PC heeft maar één databyte; de writer mag data2 niet meeschrijven.
        let events = vec![(0u64, 0xC0u8, 5u8, 0u8, 0u8)];
        let data = encode_smf(&events);
        let track_len = be_u32(&data, 18) as usize;
        let track = &data[22..22 + track_len];
        // Track: [tempo-meta (7 bytes)] [delta=0 (1)] [0xC0] [05] [eot (4)]
        // Zoek het 0xC0-event en controleer dat het direct gevolgd wordt door 0x05 en daarna de eot-delta.
        let pc_pos = track.iter().position(|&b| b == 0xC0).expect("PC-event aanwezig");
        assert_eq!(track[pc_pos + 1], 0x05);
        // Na de PC (1 databyte) volgt de end-of-track delta (0x00) + 0xFF 0x2F 0x00.
        assert_eq!(&track[pc_pos + 2..pc_pos + 6], &[0x00, 0xFF, 0x2F, 0x00]);
    }
}

#[cfg(test)]
mod crescendo_save_tests {
    use super::registration_without_crescendo;

    fn ids(v: &[&str]) -> Vec<String> { v.iter().map(|s| s.to_string()).collect() }

    #[test]
    fn opslaan_minus_trede_claims() {
        let drawn = ids(&["Prestant_8", "Trompet_8", "Octaaf_4"]);
        let couplers = ids(&["real_coupler_1", "coupler_II_I"]);
        let claims = ids(&["Octaaf_4", "real_coupler_1"]);
        let (stops, cpl) = registration_without_crescendo(&drawn, &couplers, &claims);
        assert_eq!(stops, ids(&["Prestant_8", "Trompet_8"]));
        assert_eq!(cpl, ids(&["coupler_II_I"]));
        // Zonder claims blijft alles staan; lege registratie blijft leeg.
        let (s2, c2) = registration_without_crescendo(&drawn, &couplers, &[]);
        assert_eq!(s2, drawn);
        assert_eq!(c2, couplers);
        let (s3, c3) = registration_without_crescendo(&[], &[], &claims);
        assert!(s3.is_empty() && c3.is_empty());
    }
}

#[cfg(test)]
mod jmrec_naam_tests {
    use super::display_identity;
    use std::path::Path;
    use vpo_sampler::grandorgue::OrganInfo;

    /// Pad naar <map>/<map>.organ, opgebouwd met de scheidingstekens van het
    /// draaiende platform. Een hardgecodeerd `C:\X\...` is op macOS en Linux
    /// geen mappad maar één bestandsnaam, waardoor file_stem() de hele string
    /// teruggaf en de mapcode-herkenning niet aansloeg (gevonden door de
    /// platformcontrole in de bouwstraat).
    fn odf_pad(map: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(map).join(format!("{}.organ", map))
    }

    fn info(kerk: &str, bouwer: &str, adres: &str, comments: &str) -> OrganInfo {
        OrganInfo {
            church_name: kerk.into(),
            church_address: adres.into(),
            organ_builder: bouwer.into(),
            organ_comments: comments.into(),
            recording_details: "44100 Hz, 16-bit, mp3".into(),
            ..Default::default()
        }
    }

    /// Schrijf een JM-Rec-projectmap in de tempmap: <stem>.organ bestaat niet
    /// echt (display_identity leest hem niet), het manifest wel.
    fn manifest_dir(naam: &str, json: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(naam);
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("PuttBatz.jm-rec.json"), json).unwrap();
        d
    }

    #[test]
    fn jmrec_marker_geeft_kerk_bouwer_plaats() {
        // Geen manifest: herkenning via OrganComments, delen uit de ODF.
        let o = info("Hervormde Kerk", "Bätz-Witte", "Puttershoek", "Opgenomen met JM-Rec v3.10");
        let (naam, bouwer, plaats) = display_identity(&odf_pad("PuttBatz"), &o);
        assert_eq!(naam, "Hervormde Kerk - Bätz-Witte - Puttershoek");
        assert_eq!(bouwer, "Bätz-Witte");
        assert_eq!(plaats, "Puttershoek");
    }

    #[test]
    fn jmrec_mapcode_als_kerknaam_valt_weg() {
        // JM-Rec schrijft de mapcode als ChurchName zodra de kerknaam leeg is.
        let o = info("PuttBatz", "Bätz-Witte", "Puttershoek", "Opgenomen met JM-Rec v3.10");
        let (naam, _, _) = display_identity(&odf_pad("PuttBatz"), &o);
        assert_eq!(naam, "Bätz-Witte - Puttershoek");
    }

    #[test]
    fn manifest_wint_van_de_odf_velden() {
        let d = manifest_dir(
            "jm_manifest_1",
            r#"{"jm_rec_version":"3.10","organ":"PuttBatz","kerk":"Hervormde Kerk","plaats":"Puttershoek","bouwer":"Bätz-Witte"}"#,
        );
        // ODF heeft de mapcode als kerknaam; het manifest de echte naam.
        let o = info("PuttBatz", "Onbekend", "", "");
        let (naam, bouwer, plaats) = display_identity(&d.join("PuttBatz.organ"), &o);
        assert_eq!(naam, "Hervormde Kerk - Bätz-Witte - Puttershoek");
        assert_eq!(bouwer, "Bätz-Witte");
        assert_eq!(plaats, "Puttershoek");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn manifest_met_lege_kerk_laat_dat_deel_weg() {
        let d = manifest_dir(
            "jm_manifest_2",
            r#"{"organ":"PuttBatz","kerk":"","plaats":"Puttershoek","bouwer":"Bätz-Witte"}"#,
        );
        let o = info("PuttBatz", "Bätz-Witte", "Puttershoek", "");
        let (naam, _, _) = display_identity(&d.join("PuttBatz.organ"), &o);
        assert_eq!(naam, "Bätz-Witte - Puttershoek");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn go_set_houdt_eigen_naam_en_krijgt_plaats() {
        // Geen JM-Rec-marker → naam ongewijzigd; location = ChurchAddress.
        let o = info("GreenPositiv", "Stanisław Pielczyk", "Katowice, Poland",
                     "Sample set was made by Piotr Grabowski.");
        let (naam, bouwer, plaats) = display_identity(Path::new(r"C:\X\GreenPositiv.organ"), &o);
        assert_eq!(naam, "GreenPositiv");
        assert_eq!(bouwer, "Stanisław Pielczyk");
        assert_eq!(plaats, "Katowice, Poland", "plaats uit ChurchAddress, niet RecordingDetails");
    }

    #[test]
    fn lege_plaats_valt_terug_op_opnamedetails() {
        let o = info("Oud orgel", "Bouwer", "", "");
        let (_, _, plaats) = display_identity(Path::new(r"C:\X\Oud.organ"), &o);
        assert_eq!(plaats, "44100 Hz, 16-bit, mp3");
    }
}

#[cfg(test)]
mod tremulant_kind_tests {
    use super::{
        divisie_heeft_tremulant_opnamen, nagebootste_tremulant_voor_herstel,
        tremulant_kind_voor_divisie, DivisionDto, StopDto,
    };
    use crate::library::DivisionTremulantSaved;

    fn stop(id: &str, trem: bool) -> StopDto {
        StopDto {
            id: id.to_string(), name: id.to_string(), pitch: "8".to_string(), drawn: false,
            color: None, has_tremulant: trem, midi_action_code: 0, internal_stop_id: 1,
            first_midi_note: 36, last_midi_note: 96, is_reed: false,
        }
    }

    fn divisie(naam: &str, kind: Option<&str>, stop_trem: bool) -> DivisionDto {
        DivisionDto {
            name: naam.to_string(), display_name: naam.to_string(),
            stops: vec![stop("Prestant_8", stop_trem)],
            has_tremulant: kind.is_some() || stop_trem,
            tremulant_kind: kind.map(str::to_string), has_swell: false,
        }
    }

    fn opgeslagen(div: &str, enabled: bool) -> DivisionTremulantSaved {
        DivisionTremulantSaved {
            division: div.to_string(), enabled, rate: 5.5, amp_depth: 12.0, pitch_depth: 20.0,
        }
    }

    #[test]
    fn echte_opnamen_herkend_aan_soort_of_registers() {
        assert!(divisie_heeft_tremulant_opnamen(&divisie("A", Some("wave"), false)));
        assert!(divisie_heeft_tremulant_opnamen(&divisie("B", Some("samples"), false)));
        assert!(divisie_heeft_tremulant_opnamen(&divisie("C", None, true)));
        assert!(!divisie_heeft_tremulant_opnamen(&divisie("D", Some("synth"), false)));
        assert!(!divisie_heeft_tremulant_opnamen(&divisie("E", None, false)));
    }

    #[test]
    fn opgeslagen_lfo_gaat_uit_over_echte_opnamen() {
        // 0.7.38-bestand: enabled=true voor divisies die sinds 0.7.39 als
        // wave/samples gelden — opname én LFO klonken over elkaar.
        let divs = vec![
            divisie("Hoofdwerk", Some("wave"), true),
            divisie("Bovenwerk", Some("samples"), true),
            divisie("Rugwerk", None, true), // alleen registers met een trem-laag
        ];
        let saved = vec![
            opgeslagen("Hoofdwerk", true), opgeslagen("Bovenwerk", true), opgeslagen("Rugwerk", true),
        ];
        let (lijst, uitgezet) = nagebootste_tremulant_voor_herstel(&saved, &divs);
        assert_eq!(uitgezet, vec!["Hoofdwerk".to_string(), "Bovenwerk".to_string(), "Rugwerk".to_string()]);
        assert_eq!(lijst.len(), 3);
        assert!(lijst.iter().all(|t| !t.enabled));
        // Parameters blijven staan; alleen de aan/uit-stand verandert.
        assert_eq!(lijst[0].rate, 5.5);
        assert_eq!(lijst[0].amp_depth, 12.0);
        assert_eq!(lijst[0].pitch_depth, 20.0);
    }

    #[test]
    fn opgeslagen_lfo_blijft_zonder_opnamen() {
        // Synth-ODF-tremulant, geen tremulant, of een divisie die dit orgel
        // niet kent: de bewaarde voorkeur blijft ongemoeid.
        let divs = vec![divisie("Hoofdwerk", Some("synth"), false), divisie("Pedaal", None, false)];
        let saved = vec![
            opgeslagen("Hoofdwerk", true), opgeslagen("Pedaal", true), opgeslagen("Onbekend", true),
        ];
        let (lijst, uitgezet) = nagebootste_tremulant_voor_herstel(&saved, &divs);
        assert!(uitgezet.is_empty());
        assert_eq!(lijst.len(), 3);
        assert!(lijst.iter().all(|t| t.enabled));
    }

    #[test]
    fn uitstaande_lfo_wordt_niet_gemeld() {
        let divs = vec![divisie("Hoofdwerk", Some("wave"), true)];
        let (lijst, uitgezet) = nagebootste_tremulant_voor_herstel(&[opgeslagen("Hoofdwerk", false)], &divs);
        assert!(uitgezet.is_empty());
        assert!(!lijst[0].enabled);
    }

    #[test]
    fn wave_vlag_zonder_opnamen_wordt_synth() {
        // Regressie: TremulantType=Wave zette de UI-LFO uit terwijl er geen
        // enkele trem-opname geladen was → helemaal geen tremulant meer.
        assert_eq!(tremulant_kind_voor_divisie(false, true, true), Some("synth"));
    }

    #[test]
    fn opnamen_bepalen_wave_of_samples() {
        assert_eq!(tremulant_kind_voor_divisie(true, true, true), Some("wave"));
        // Hauptwerk-"tremmed"-lagen: opnamen zonder ODF-wave-vlag.
        assert_eq!(tremulant_kind_voor_divisie(true, false, false), Some("samples"));
    }

    #[test]
    fn odf_tremulant_zonder_opnamen_is_synth_en_niets_is_niets() {
        assert_eq!(tremulant_kind_voor_divisie(false, false, true), Some("synth"));
        assert_eq!(tremulant_kind_voor_divisie(false, false, false), None);
    }
}

#[cfg(test)]
mod pedaalbinding_herstel_tests {
    use super::pedaalbindingen_voor_herstel;
    use crate::library::{CrescendoBindingSaved, OrganSettings, SwellBindingSaved};

    fn zwel(div: &str, ch: u8, cc: u8) -> SwellBindingSaved {
        SwellBindingSaved {
            division_name: div.to_string(), division_index: 0, channel: ch, cc_num: cc,
            min_val: 0, max_val: 127, invert: false, last_value: None,
        }
    }

    fn cresc(ch: u8, cc: u8) -> CrescendoBindingSaved {
        CrescendoBindingSaved { channel: ch, cc_num: cc, min_val: 0, max_val: 127, invert: false }
    }

    fn settings(zwellen: Vec<SwellBindingSaved>, crescendo: Option<CrescendoBindingSaved>) -> OrganSettings {
        OrganSettings { swell_bindings: zwellen, crescendo_binding: crescendo, ..Default::default() }
    }

    #[test]
    fn zwel_en_crescendo_op_dezelfde_trede_alleen_crescendo() {
        // 0.7.38-bestand: beide op (kanaal 0, CC7). Voorheen werden ze allebei
        // hersteld en was de zwelkast stil dood (cc_claimed_by_crescendo wint).
        let s = settings(vec![zwel("Nevenwerk", 0, 7)], Some(cresc(0, 7)));
        let (zwellen, crescendo) = pedaalbindingen_voor_herstel(&s);
        assert!(zwellen.is_empty());
        let c = crescendo.expect("crescendobinding blijft");
        assert_eq!((c.channel, c.cc_num), (0, 7));
    }

    #[test]
    fn zwel_op_ander_kanaal_blijft_naast_crescendo() {
        let s = settings(vec![zwel("Nevenwerk", 1, 7)], Some(cresc(0, 7)));
        let (zwellen, crescendo) = pedaalbindingen_voor_herstel(&s);
        assert_eq!(zwellen.len(), 1);
        assert_eq!((zwellen[0].channel, zwellen[0].cc_num), (1, 7));
        assert!(crescendo.is_some());
    }

    #[test]
    fn alleen_de_botsende_zwelbindingen_vervallen() {
        let s = settings(
            vec![zwel("Nevenwerk", 0, 7), zwel("Bovenwerk", 0, 7), zwel("Rugwerk", 0, 11)],
            Some(cresc(0, 7)),
        );
        let (zwellen, _) = pedaalbindingen_voor_herstel(&s);
        let namen: Vec<&str> = zwellen.iter().map(|b| b.division_name.as_str()).collect();
        assert_eq!(namen, vec!["Rugwerk"]);
        // Zonder crescendobinding blijft alles staan.
        let s2 = settings(vec![zwel("Nevenwerk", 0, 7)], None);
        let (z2, c2) = pedaalbindingen_voor_herstel(&s2);
        assert_eq!(z2.len(), 1);
        assert!(c2.is_none());
    }
}

#[cfg(test)]
mod bibliotheek_opfris_tests {
    use super::{
        bibliotheek_identiteit_licht, lees_organ_sectie_licht, start_opfris_eenmalig,
        ververs_bibliotheek_namen,
    };
    use crate::library::{OrganLibrary, OrganLibraryEntry};
    use parking_lot::RwLock;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    fn tempmap(naam: &str) -> PathBuf {
        let d = std::env::temp_dir().join(naam);
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// Een .organ met een [Organ]-sectie gevolgd door véél andere secties: de
    /// lichte lezer mag alleen de eerste sectie gebruiken.
    fn schrijf_organ(dir: &Path, stem: &str, organ_sectie: &str) -> PathBuf {
        let p = dir.join(format!("{}.organ", stem));
        let mut txt = format!("[Organ]\n{}\n", organ_sectie);
        for i in 1..=50 {
            txt.push_str(&format!("[Stop{:03}]\nChurchName=FOUT\nName=Prestant\n", i));
        }
        std::fs::write(&p, txt).unwrap();
        p
    }

    #[test]
    fn leest_alleen_de_organ_sectie() {
        let d = tempmap("jm_lib_licht_1");
        let p = schrijf_organ(
            &d,
            "GreenPositiv",
            "ChurchName=GreenPositiv\nChurchAddress=Katowice, Poland\nOrganBuilder=Pielczyk\nRecordingDetails=Piotr Grabowski\nNumberOfManuals=1",
        );
        let info = lees_organ_sectie_licht(&p).expect("[Organ] moet gelezen worden");
        assert_eq!(info.church_name, "GreenPositiv", "latere secties mogen niet meetellen");
        assert_eq!(info.church_address, "Katowice, Poland");
        assert_eq!(info.organ_builder, "Pielczyk");

        // Geen JM-Rec-markering → naam ongewijzigd, plaats uit ChurchAddress.
        let (naam, bouwer, plaats) =
            bibliotheek_identiteit_licht(&p.to_string_lossy(), "organ_file").expect("identiteit");
        assert_eq!(naam, "GreenPositiv");
        assert_eq!(bouwer, "Pielczyk");
        assert_eq!(plaats, "Katowice, Poland");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn jmrec_manifest_geeft_de_nieuwe_kaartnaam() {
        // Punt 8: de kaart toonde nog de mapcode "PuttBatz".
        let d = tempmap("jm_lib_licht_2");
        let p = schrijf_organ(&d, "PuttBatz", "ChurchName=PuttBatz\nNumberOfManuals=2");
        std::fs::write(
            d.join("PuttBatz.jm-rec.json"),
            r#"{"jm_rec_version":"3.10","organ":"PuttBatz","kerk":"Hervormde Kerk","plaats":"Puttershoek","bouwer":"Bätz-Witte"}"#,
        )
        .unwrap();
        let (naam, bouwer, plaats) =
            bibliotheek_identiteit_licht(&p.to_string_lossy(), "organ_file").expect("identiteit");
        assert_eq!(naam, "Hervormde Kerk - Bätz-Witte - Puttershoek");
        assert_eq!(bouwer, "Bätz-Witte");
        assert_eq!(plaats, "Puttershoek");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn mapscan_zonder_manifest_laat_de_entry_staan() {
        let d = tempmap("jm_lib_licht_3");
        // Sample-map zonder manifest: niets te zeggen → None (niet wissen).
        assert!(bibliotheek_identiteit_licht(&d.to_string_lossy(), "sample_directory").is_none());
        std::fs::write(
            d.join("PuttBatz.jm-rec.json"),
            r#"{"organ":"PuttBatz","kerk":"","plaats":"Puttershoek","bouwer":"Bätz-Witte"}"#,
        )
        .unwrap();
        let (naam, _, plaats) =
            bibliotheek_identiteit_licht(&d.to_string_lossy(), "sample_directory").expect("manifest");
        assert_eq!(naam, "Bätz-Witte - Puttershoek");
        assert_eq!(plaats, "Puttershoek");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn weggehaalde_schijf_en_hauptwerk_geven_none() {
        // USB eruit: het pad bestaat niet → niets bijwerken, niets wissen.
        assert!(lees_organ_sectie_licht(Path::new(r"Q:\bestaat-niet\x.organ")).is_none());
        assert!(bibliotheek_identiteit_licht(r"Q:\bestaat-niet\x.organ", "organ_file").is_none());
        // Hauptwerk-XML bewust overgeslagen (niet lichtgewicht te lezen).
        assert!(bibliotheek_identiteit_licht(r"Q:\x\Set.Organ_Hauptwerk_xml", "organ_file").is_none());
    }

    #[test]
    fn latin1_organ_sectie_blijft_leesbaar() {
        // Oude ODF's staan vaak in ISO-8859-1; "Bätz" mag niet verminken.
        let d = tempmap("jm_lib_licht_4");
        let p = d.join("Oud.organ");
        let mut bytes = b"[Organ]\nChurchName=B".to_vec();
        bytes.push(0xE4); // 'ä' in Latin-1
        bytes.extend_from_slice(b"tz-kerk\nOrganBuilder=Onbekend\n[Manual001]\n");
        std::fs::write(&p, bytes).unwrap();
        let info = lees_organ_sectie_licht(&p).expect("[Organ]");
        assert_eq!(info.church_name, "Bätz-kerk");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// Onderdeel B: `get_organ_library` (UI-thread) mag NOOIT op de opfrisser
    /// wachten. Het werk hier blokkeert tot de test het vrijgeeft — een
    /// "losgekoppelde USB-schijf". Zou de starter erop wachten (zoals de
    /// oude `recv_timeout` van 400 ms), dan kwam de aanroep pas na de
    /// vrijgave terug, of nooit.
    #[test]
    fn get_organ_library_wacht_niet_op_de_opfrisser() {
        let vlag = AtomicBool::new(false);
        let (vrijgave_tx, vrijgave_rx) = std::sync::mpsc::channel::<()>();
        let (gestart_tx, gestart_rx) = std::sync::mpsc::channel::<()>();
        let teller = Arc::new(AtomicUsize::new(0));
        let t = teller.clone();

        let t0 = Instant::now();
        let handle = start_opfris_eenmalig(&vlag, move || {
            t.fetch_add(1, Ordering::SeqCst);
            let _ = gestart_tx.send(());
            let _ = vrijgave_rx.recv(); // trage schijf: hangt tot de test hem loslaat
        })
        .expect("eerste aanroep start de opfris");
        let duur = t0.elapsed();
        assert!(duur < Duration::from_millis(100), "aanroeper wachtte {:?} op de opfrisser", duur);
        assert!(vlag.load(Ordering::SeqCst), "vlag staat meteen (één keer per proces)");

        // De thread draait (en hangt nog) terwijl de aanroeper al terug is.
        gestart_rx.recv_timeout(Duration::from_secs(5)).expect("opfris-thread is gestart");
        assert_eq!(teller.load(Ordering::SeqCst), 1);

        // Bibliotheek opnieuw geopend: tweede aanroep doet niets en is meteen terug.
        let t1 = Instant::now();
        assert!(start_opfris_eenmalig(&vlag, || panic!("mag niet nog eens draaien")).is_none());
        assert!(t1.elapsed() < Duration::from_millis(100));

        vrijgave_tx.send(()).unwrap();
        handle.join().unwrap();
        assert_eq!(teller.load(Ordering::SeqCst), 1, "het werk draait precies één keer");
    }

    fn entry(pad: &Path, naam: &str) -> OrganLibraryEntry {
        OrganLibraryEntry {
            id: pad.to_string_lossy().to_lowercase(),
            name: naam.into(),
            builder: "Custom Samples".into(),
            location: String::new(),
            year: None,
            stop_count: 0,
            source_type: "organ_file".into(),
            source_path: pad.to_string_lossy().to_string(),
            image_path: None,
            image_manual: false,
            image_searched: None,
        }
    }

    /// De achtergrondopfris werkt de kaart bij en slaat op; een entry zonder
    /// wijziging of zonder leesbaar bestand blijft met rust, en zonder
    /// wijziging wordt er niets naar schijf geschreven.
    #[test]
    fn opfris_werkt_kaartnamen_bij_en_slaat_alleen_bij_wijziging_op() {
        let d = tempmap("jm_lib_licht_5");
        let data = d.join("appdata");
        std::fs::create_dir_all(&data).unwrap();
        let p = schrijf_organ(&d, "PuttBatz", "ChurchName=PuttBatz\nNumberOfManuals=2");
        std::fs::write(
            d.join("PuttBatz.jm-rec.json"),
            r#"{"jm_rec_version":"3.10","organ":"PuttBatz","kerk":"Hervormde Kerk","plaats":"Puttershoek","bouwer":"Bätz-Witte"}"#,
        )
        .unwrap();
        let weg = d.join("weg.organ"); // bestaat niet: USB eruit

        let lib = RwLock::new(OrganLibrary {
            organs: vec![entry(&p, "PuttBatz"), entry(&weg, "Oude naam")],
            settings: Default::default(),
        });
        assert_eq!(ververs_bibliotheek_namen(&lib, &data), 1);
        {
            let l = lib.read();
            assert_eq!(l.organs[0].name, "Hervormde Kerk - Bätz-Witte - Puttershoek");
            assert_eq!(l.organs[0].builder, "Bätz-Witte");
            assert_eq!(l.organs[0].location, "Puttershoek");
            assert_eq!(l.organs[1].name, "Oude naam", "onleesbaar pad: entry blijft staan");
            assert_eq!(l.organs.len(), 2, "er wordt nooit iets gewist");
        }
        let json = data.join("organ_library.json");
        let txt = std::fs::read_to_string(&json).expect("bibliotheek is opgeslagen");
        assert!(txt.contains("Hervormde Kerk - Bätz-Witte - Puttershoek"));

        // Tweede ronde: niets veranderd → niets opgeslagen.
        std::fs::remove_file(&json).unwrap();
        assert_eq!(ververs_bibliotheek_namen(&lib, &data), 0);
        assert!(!json.exists(), "zonder wijziging wordt er niet opgeslagen");
        let _ = std::fs::remove_dir_all(&d);
    }
}

#[cfg(test)]
mod afbeeldingsbron_tests {
    use super::afbeeldingsbron_leesbaar;

    /// Punt 4: de negatieve afbeeldingscache mag "gezocht en niets gevonden"
    /// niet verwarren met "kon niet zoeken". Alleen bij een leesbare bronmap
    /// mag `image_searched` gezet worden.
    #[test]
    fn losgekoppelde_schijf_geldt_niet_als_gezocht() {
        // USB eruit / netwerkschijf weg: er viel niets te zoeken.
        assert!(!afbeeldingsbron_leesbaar(r"Q:\bestaat-niet\Orgel", "sample_directory"));
        assert!(!afbeeldingsbron_leesbaar(r"Q:\bestaat-niet\Orgel\set.organ", "organ_file"));
        assert!(!afbeeldingsbron_leesbaar(r"\\geen-server\orgels\set.organ", "organ_file"));
    }

    #[test]
    fn bereikbare_map_geldt_wel_als_gezocht() {
        let d = std::env::temp_dir().join("jm_afbeeldingsbron_1");
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let odf = d.join("set.organ");
        std::fs::write(&odf, b"[Organ]\n").unwrap();

        // Sample-map: het pad zelf. Orgelbestand: de map eromheen.
        assert!(afbeeldingsbron_leesbaar(&d.to_string_lossy(), "sample_directory"));
        assert!(afbeeldingsbron_leesbaar(&odf.to_string_lossy(), "organ_file"));
        // Een ODF dat zelf weg is, maar in een bestaande map stond: er kán
        // gezocht worden, dus "geen foto" mag hier wél vastgelegd worden.
        assert!(afbeeldingsbron_leesbaar(&d.join("weg.organ").to_string_lossy(), "organ_file"));
        // Een bestand als sample-map opgegeven is geen leesbare map.
        assert!(!afbeeldingsbron_leesbaar(&odf.to_string_lossy(), "sample_directory"));

        let _ = std::fs::remove_dir_all(&d);
    }

    /// Hauptwerk: de foto staat in OrganInstallationPackages/<pakket>/, niet
    /// naast de definitie. Een leesbare definitiemap zonder bereikbare
    /// pakketten (losgekoppelde schijf) mag dus NIET als "doorzocht" gelden.
    #[test]
    fn hauptwerk_zonder_bereikbare_pakketten_geldt_niet_als_gezocht() {
        let d = std::env::temp_dir().join("jm_afbeeldingsbron_hw");
        let _ = std::fs::remove_dir_all(&d);
        let defs = d.join("OrganDefinitions");
        let pakketten = d.join("OrganInstallationPackages");
        std::fs::create_dir_all(&defs).unwrap();
        std::fs::create_dir_all(pakketten.join("000690")).unwrap();
        let odf = defs.join("Set.Organ_Hauptwerk_xml");
        std::fs::write(&odf, b"<Hauptwerk/>").unwrap();
        let odf_s = odf.to_string_lossy().to_string();

        // Pakketten bereikbaar: definitiemap én pakketmap leesbaar -> doorzocht.
        assert!(afbeeldingsbron_leesbaar(&odf_s, "organ_file"));

        // Pakketten weg (schijf eruit): definitiemap nog leesbaar, maar de
        // plek waar de foto had moeten staan was onbereikbaar -> niet doorzocht.
        std::fs::remove_dir_all(&pakketten).unwrap();
        assert!(afbeeldingsbron_leesbaar(&defs.to_string_lossy(), "sample_directory"));
        assert!(!afbeeldingsbron_leesbaar(&odf_s, "organ_file"));

        // GrandOrgue (.organ) in dezelfde map kent geen pakketten: onveranderd.
        let go = defs.join("set.organ");
        std::fs::write(&go, b"[Organ]").unwrap();
        assert!(afbeeldingsbron_leesbaar(&go.to_string_lossy(), "organ_file"));

        let _ = std::fs::remove_dir_all(&d);
    }
}
