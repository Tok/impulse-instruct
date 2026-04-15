// ─── state/mod.rs ── single source of truth for all synth parameters ─────────
// Pure data only — no methods that mutate in-place. Transitions at the bottom.
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub mod drums;
pub use drums::*;
pub mod lfo;
pub use lfo::*;
pub mod music;
#[allow(unused_imports)]
pub use music::note_in_scale;
pub use music::{ROOT_NAMES, Scale, scale_degree, scale_notes, snap_to_scale};

pub mod synth_types;
pub use synth_types::{FilterMode, Waveform};

pub mod hoover;
pub use hoover::HooverState;

pub mod noise;
pub use noise::NoiseVoiceState;

pub mod granular;
pub use granular::GranularState;

pub mod an1x;
pub use an1x::{An1xLfoTarget, An1xState, An1xWave};

pub const MAX_STEPS: usize = 64;
pub const MAX_BASS_VOICES: usize = 4;

// ─── Param control mode (tristate) ───────────────────────────────────────────

/// Whether a parameter is under user control, free for the LLM, or actively
/// targeted by the LLM.  Stored as two separate HashSets so the common "free"
/// case has zero allocation cost.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParamMode {
    /// Default: both user and LLM can change the value.
    Free,
    /// User has taken ownership — LLM will skip this param entirely.
    UserOwned,
    /// User wants the LLM to actively drive this param — hinted in every prompt.
    LlmFocus,
}

/// Derive the mode for a single dot-path from the two state sets.
pub fn param_mode(path: &str, locked: &HashSet<String>, focused: &HashSet<String>) -> ParamMode {
    if locked.contains(path) {
        return ParamMode::UserOwned;
    }
    if focused.contains(path) {
        return ParamMode::LlmFocus;
    }
    ParamMode::Free
}

/// Cycle a param through Free → UserOwned → LlmFocus → Free (pure).
pub fn cycle_param_mode(state: AppState, path: &str) -> AppState {
    let mut s = state;
    let owned = s.llm.locked_params.contains(path);
    let focused = s.llm.focused_params.contains(path);
    match (owned, focused) {
        (false, false) => {
            s.llm.locked_params.insert(path.to_string());
        }
        (true, false) => {
            s.llm.locked_params.remove(path);
            s.llm.focused_params.insert(path.to_string());
        }
        (_, true) => {
            s.llm.focused_params.remove(path);
        }
    }
    s
}

/// Set a param to a specific mode (Free clears both sets; pure function).
pub fn set_param_mode(state: AppState, path: &str, mode: ParamMode) -> AppState {
    let mut s = state;
    match mode {
        ParamMode::Free => {
            s.llm.locked_params.remove(path);
            s.llm.focused_params.remove(path);
        }
        ParamMode::UserOwned => {
            s.llm.focused_params.remove(path);
            s.llm.locked_params.insert(path.to_string());
        }
        ParamMode::LlmFocus => {
            s.llm.locked_params.remove(path);
            s.llm.focused_params.insert(path.to_string());
        }
    }
    s
}

pub mod ui_prefs;
pub use ui_prefs::{AutosaveInterval, HuthStyle, UiPrefs};

pub(crate) mod fx_plan;
pub mod modulation;
pub mod rack;
mod rack_presets;
pub mod rack_scope;
pub use fx_plan::compile_fx_plan;
pub use modulation::{
    ModInput, lfo_target_short_label, mod_input_label, mod_inputs, parse_lfo_target,
};
pub use rack::{
    Cable, CableColor, FxPlan, FxStep, GRID_COLS, ModuleKind, PortDir, PortKind, PortRef,
    RackModule, RackState, Zone,
};
pub use rack_presets::RACK_PRESETS;
pub use rack_scope::{parse_module_kind, rack_kind_name_matches, scope_from_control_cables};

// Amen sampler + bass voice state live in their own modules to keep LOC
// under the 1000-line cap on state/mod.rs.
pub use amen::{AmenMeta, AmenState};
use bass::default_bass_voices;
pub use bass::{BassLfoTarget, BassState, BassVoiceState};
mod amen;
mod bass;

// ─── Top-level ───────────────────────────────────────────────────────────────
fn default_pattern_bank() -> Vec<SequencerState> {
    vec![SequencerState::default(); 8]
}

/// Spectrum analyser display parameters.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SpectrumAnalyzerState {
    /// Exponential smoothing factor (0.0 = instant, 0.95 = very smooth).
    pub smoothing: f32,
    /// Show peak-hold markers.
    pub peak_hold: bool,
}
impl Default for SpectrumAnalyzerState {
    fn default() -> Self {
        Self {
            smoothing: 0.7,
            peak_hold: false,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AppState {
    /// All bass synth voices (1 active minimum; up to MAX_BASS_VOICES).
    #[serde(default = "default_bass_voices")]
    pub bass_voices: Vec<BassVoiceState>,
    /// Which voice index is currently selected in the UI / being edited.
    #[serde(default)]
    pub active_voice: usize,
    pub kit_a: DrumKit808,
    pub kit_b: DrumKit909,
    pub sequencer: SequencerState,
    pub fx: FxState,
    pub llm: LlmState,
    #[serde(default)]
    pub lfo: [LfoSlot; 4],
    #[serde(default)]
    pub free_eg: FreeEg,
    #[serde(default)]
    pub noise_voice: NoiseVoiceState,
    #[serde(default)]
    pub granular: GranularState,
    #[serde(default)]
    pub hoover: HooverState,
    #[serde(default)]
    pub an1x: An1xState,
    #[serde(default)]
    pub amen: AmenState,
    #[serde(default)]
    pub ui_prefs: UiPrefs,
    /// 8 named pattern slots (A–H) for storage and chain playback.
    #[serde(default = "default_pattern_bank")]
    pub pattern_bank: Vec<SequencerState>,
    /// Which bank slot the UI is currently editing (0–7).
    #[serde(default)]
    pub pattern_edit: usize,
    /// Ordered playback chain — indices into pattern_bank (max 8 entries).
    #[serde(default)]
    pub chain: Vec<usize>,
    /// When true the audio thread advances through `chain` on each pattern loop.
    #[serde(default)]
    pub chain_enabled: bool,
    /// Current position in the chain — written by audio thread, read by UI.
    #[serde(default)]
    pub chain_pos: usize,
    /// When true, piano/MIDI note-ons while running write into the bass pattern.
    #[serde(default)]
    pub live_record: bool,
    /// Spectrum analyser parameters.
    #[serde(default)]
    pub spectrum: SpectrumAnalyzerState,
    /// Modular rack — which modules are visible and how they are cabled.
    #[serde(default)]
    pub rack: RackState,
    /// Per-agent LLM state for rackable LLM modules.
    #[serde(default)]
    pub llm_agents: Vec<LlmAgentState>,
    /// Per-module TTS state, keyed by TTS rack module id.
    #[serde(default)]
    pub tts_modules: Vec<TtsModuleState>,
    // api_log moved to a lock-free crossbeam channel (ApiState.api_log_tx → UI receiver)
    /// Scroll target — the UI scrolls to bring this zone/module into view, then clears.
    /// Format: zone name ("global", "voice", "fxmod") or module kind ("AcidBass", "DrumKit808", etc.)
    #[serde(skip)]
    pub scroll_target: Option<String>,
    /// When Some, the UI toggles or sets rack flip state, then clears.
    /// true = show back (cables), false = show front (knobs), None = no change.
    #[serde(skip)]
    pub rack_flip_requested: Option<bool>,
    /// Monotonic step counter — incremented by the audio thread each time
    /// `current_step` advances. Used for bar-based ramp timing.
    #[serde(skip)]
    pub global_step_count: u64,
    /// API-requested zone collapse: (ai, global, voice, fxmod). None = no change.
    #[serde(skip)]
    pub collapse_requested: Option<(bool, bool, bool, bool)>,
    /// Compact audio analysis text, auto-updated by the UI every ~2s.
    /// Injected into every LLM system prompt as global context.
    #[serde(skip)]
    pub audio_snapshot: String,
}

impl Default for AppState {
    fn default() -> Self {
        let mut s = Self {
            bass_voices: default_bass_voices(),
            active_voice: 0,
            kit_a: Default::default(),
            kit_b: Default::default(),
            sequencer: Default::default(),
            fx: Default::default(),
            llm: Default::default(),
            lfo: Default::default(),
            free_eg: Default::default(),
            noise_voice: Default::default(),
            granular: Default::default(),
            hoover: Default::default(),
            an1x: Default::default(),
            amen: Default::default(),
            ui_prefs: Default::default(),
            pattern_bank: default_pattern_bank(),
            pattern_edit: 0,
            chain: Vec::new(),
            chain_enabled: false,
            chain_pos: 0,
            live_record: false,
            spectrum: Default::default(),
            rack: Default::default(),
            llm_agents: Vec::new(),
            tts_modules: Vec::new(),
            scroll_target: None,
            rack_flip_requested: None,
            global_step_count: 0,
            collapse_requested: None,
            audio_snapshot: String::new(),
        };
        // Create a default agent for the LlmAgent rack module.
        if let Some(agent_id) = s
            .rack
            .modules
            .iter()
            .find(|m| m.kind == ModuleKind::LlmAgent)
            .map(|m| m.id)
        {
            s.llm_agents.push(LlmAgentState::new_default(agent_id));
        }
        // Create TTS module state for any TTS modules in the default rack.
        for m in &s.rack.modules {
            if m.kind == ModuleKind::NeuTts {
                s.tts_modules.push(TtsModuleState::new(m.id));
            }
        }
        s
    }
}

// ─── Bass synth ───────────────────────────────────────────────────────────────

// Bass voice state + multi-voice wrapper extracted to bass.rs — see
// `pub use bass::...` above.  This region is intentionally empty.

fn default_bass_patterns() -> Vec<Vec<TB303Step>> {
    // Voice 0 gets the same starter pattern as `bass_pattern`; voices 1-3 are silent.
    let mut patterns = Vec::with_capacity(MAX_BASS_VOICES);

    // Voice 0: A minor starter pattern (mirrors SequencerState default)
    let mut p0 = vec![TB303Step::default(); MAX_STEPS];
    let bass_notes: &[(usize, u8)] = &[(0, 45), (6, 48), (12, 52)];
    for &(step, note) in bass_notes {
        p0[step].active = true;
        p0[step].note = note;
    }
    patterns.push(p0);

    // Voices 1-3: silent
    for _ in 1..MAX_BASS_VOICES {
        patterns.push(vec![TB303Step::default(); MAX_STEPS]);
    }
    patterns
}

fn default_bass_voice_steps() -> Vec<usize> {
    vec![16usize; MAX_BASS_VOICES]
}

fn default_bass_voice_enabled() -> [bool; MAX_BASS_VOICES] {
    let mut arr = [false; MAX_BASS_VOICES];
    arr[0] = true;
    arr
}

// ─── Sequencer ───────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct Step {
    pub active: bool,
    pub velocity: f32,    // 0–1
    pub probability: f32, // 0–1: chance the step fires (1.0 = always, 0.5 = 50%)
    #[serde(default = "default_ratchet")]
    pub ratchet: u8, // 1 = single hit, 2/3/4 = N sub-hits per step
    /// Slice index for sample-playback voices (AmenSampler).  0 = auto
    /// (the voice picks the next slice each trigger); 1..=16 = explicit.
    /// Ignored by purely synthesised drum voices.
    #[serde(default)]
    pub slice: u8,
}

fn default_ratchet() -> u8 {
    1
}

impl Default for Step {
    fn default() -> Self {
        Self {
            active: false,
            velocity: 1.0,
            probability: 1.0,
            ratchet: 1,
            slice: 0,
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct TB303Step {
    pub active: bool,
    pub note: u8, // MIDI note number
    pub accent: bool,
    pub slide: bool,
    pub gate: f32, // 0–1 gate length ratio
    /// Per-step stereo pan, -1.0 = hard left, 0.0 = centre, 1.0 = hard right.
    /// Applied at trigger time on top of the voice's static pan setting.
    /// Defaults to 0 so old patterns deserialize unchanged.
    #[serde(default)]
    pub pan: f32,
}

impl Default for TB303Step {
    fn default() -> Self {
        Self {
            active: false,
            note: 36,
            accent: false,
            slide: false,
            gate: 0.5,
            pan: 0.0,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SequencerState {
    pub bpm: f32,
    pub steps: usize, // 1–64, active step count
    pub current_step: usize,
    pub running: bool,
    pub swing: f32,       // 0–1: 0=straight, 0.5=strong shuffle (75/25 triplet feel)
    pub time_sig_num: u8, // beats per bar (2–9, default 4); denominator always /4
    pub drum_patterns: std::collections::HashMap<DrumVoice, Vec<Step>>,
    pub bass_pattern: Vec<TB303Step>,
    pub hoover_pattern: Vec<TB303Step>,
    pub an1x_pattern: Vec<TB303Step>,
    /// Tonic note (0=C … 11=B). Used for scale highlighting and LLM music theory.
    pub root_note: u8,
    /// Active scale / mode for this pattern.
    pub scale: Scale,
    /// When true, LLM-provided bass_notes are snapped to the active scale.
    pub scale_snap: bool,
    /// Per-voice step counts for polyrhythm. Each voice loops independently at
    /// its own length; voices not present here default to `steps`.
    pub drum_steps: std::collections::HashMap<DrumVoice, usize>,
    /// Independent step counts for bass, hoover, and AN1X lanes.
    pub bass_steps: usize,
    pub hoover_steps: usize,
    pub an1x_steps: usize,
    /// Per-voice bass patterns for multi-voice support. Voice 0 mirrors `bass_pattern`.
    #[serde(default = "default_bass_patterns")]
    pub bass_patterns: Vec<Vec<TB303Step>>,
    /// Per-voice step counts for bass voices. Voice 0 mirrors `bass_steps`.
    #[serde(default = "default_bass_voice_steps")]
    pub bass_voice_steps: Vec<usize>,
    /// Drum voices that are muted (never trigger regardless of pattern).
    pub muted_drums: std::collections::HashSet<DrumVoice>,
    /// Drum voices in solo mode. When non-empty, only these voices trigger.
    pub soloed_drums: std::collections::HashSet<DrumVoice>,
    /// When true, BPM is slaved to incoming MIDI clock pulses (0xF8).
    #[serde(default)]
    pub midi_clock_sync: bool,
    /// Which bass voices are enabled for sequencing. Synced from AppState.bass_voices[i].enabled.
    #[serde(default = "default_bass_voice_enabled")]
    pub bass_voice_enabled: [bool; MAX_BASS_VOICES],
    /// Pre-echo (lead-in) configs per voice.  When active, anchor steps
    /// get reinforced by a ramped build-up on the steps leading into
    /// them (velocity / ratchet).  See `crate::sequencer::preecho`.
    /// Lookup key = voice name ("bass", "kit_a", "kit_b", "amen",
    /// "hoover", "an1x").  Empty = no preecho on that voice.
    #[serde(default)]
    pub preecho: std::collections::HashMap<String, crate::sequencer::PreechoConfig>,
    /// Amen slice play order — maps step index to slice index.  Empty =
    /// identity (step N → slice N).  When populated, the sequencer's
    /// auto-advance path looks up `amen_slice_order[step % len]` so the
    /// user can rearrange the break into a custom permutation (e.g.
    /// `[3, 0, 1, 2]` shifts the whole pattern by 3 slices).
    #[serde(default)]
    pub amen_slice_order: Vec<u8>,
}

impl Default for SequencerState {
    fn default() -> Self {
        let mut drum_patterns = std::collections::HashMap::new();
        // Pre-allocate MAX_STEPS silent steps; only the first `steps` are active in the clock.
        for v in DrumVoice::ALL {
            drum_patterns.insert(*v, vec![Step::default(); MAX_STEPS]);
        }

        // All patterns start blank — the AI builds everything from scratch.
        let bass_pattern = vec![TB303Step::default(); MAX_STEPS];

        // Default: all drum voices use the global `steps` length.
        let mut drum_steps = std::collections::HashMap::new();
        for v in DrumVoice::ALL {
            drum_steps.insert(*v, 32usize);
        }

        Self {
            bpm: 120.0,
            steps: 32,
            current_step: 0,
            running: false,
            swing: 0.0,
            time_sig_num: 4,
            drum_patterns,
            bass_pattern: bass_pattern.clone(),
            hoover_pattern: vec![TB303Step::default(); MAX_STEPS],
            an1x_pattern: vec![TB303Step::default(); MAX_STEPS],
            root_note: 9, // A
            scale: Scale::NaturalMinor,
            scale_snap: false,
            drum_steps,
            bass_steps: 32,
            hoover_steps: 32,
            an1x_steps: 32,
            muted_drums: std::collections::HashSet::new(),
            soloed_drums: std::collections::HashSet::new(),
            midi_clock_sync: false,
            bass_patterns: {
                let mut pats = vec![bass_pattern];
                for _ in 1..MAX_BASS_VOICES {
                    pats.push(vec![TB303Step::default(); MAX_STEPS]);
                }
                pats
            },
            bass_voice_steps: vec![32usize; MAX_BASS_VOICES],
            bass_voice_enabled: default_bass_voice_enabled(),
            preecho: std::collections::HashMap::new(),
            amen_slice_order: Vec::new(),
        }
    }
}

// ─── FX Chain ────────────────────────────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FxState {
    pub reverb_size: f32, // 0–1 room size
    pub reverb_damp: f32, // 0–1 damping
    pub reverb_mix: f32,  // 0–1 wet/dry
    #[serde(default)]
    pub reverb_gate_time: f32, // 0 = no gate; 0.01–2.0 s gate close time (gated reverb)
    #[serde(default)]
    pub reverb_freeze: bool, // true = infinite hold, tail loops indefinitely
    /// Reverb time direction: 0=FWD (normal), 1=REV (preverb — reverb of a
    /// reversed input buffer; sounds like reverb that builds INTO the hit),
    /// 2=MIRROR (sum of forward + reverse).  Reverse and mirror require a
    /// 1 s circular buffer of past input.
    #[serde(default)]
    pub reverb_dir: u8,
    /// Beat division for the REV/MIRROR rewind cycle.  0=free 1 s,
    /// 1=1/4 bar (1 beat), 2=1/2 bar, 3=1 bar, 4=2 bars.  Snaps the
    /// reverse-tap loop length to the active BPM.
    #[serde(default)]
    pub reverb_rev_quant: u8,
    pub delay_time: f32,     // 0–1 → 0–2000 ms
    pub delay_feedback: f32, // 0–1
    pub delay_mix: f32,      // 0–1 wet/dry
    /// Delay time direction: 0=FWD (echoes after the dry hit), 1=REV
    /// (anti-echoes preceding the hit, via reversed input buffer),
    /// 2=MIRROR.
    #[serde(default)]
    pub delay_dir: u8,
    /// Same beat-division snap as `reverb_rev_quant` but for the delay's
    /// reverse tap.
    #[serde(default)]
    pub delay_rev_quant: u8,
    #[serde(default)]
    pub delay_wow_flutter: f32, // 0–1 tape wow/flutter depth
    #[serde(default)]
    pub delay_saturation: f32, // 0–1 tape saturation on feedback
    pub distortion_drive: f32,     // 0–1
    pub distortion_mix: f32,       // 0–1 wet/dry
    pub compressor_threshold: f32, // 0–1 → -40–0 dB
    pub compressor_ratio: f32,     // 0–1 → 1:1–20:1
    pub compressor_mix: f32,       // 0–1 wet/dry (0 = bypassed)
    #[serde(default)]
    pub compressor_multiband: f32, // 0 = single band, >0 = 3-band (low/mid/high)
    pub master_volume: f32,        // 0–1
    #[serde(default)]
    pub stereo_width: f32, // 0–1: 0=mono, 0.5=normal, 1=wide
    #[serde(default)]
    pub tuning: u8, // 0=12-TET, 1=just intonation, 2=slendro, 3=pelog
    #[serde(default)]
    pub xmod_bass_to_an1x_pitch: f32, // 0–1 bass osc → AN1X pitch FM depth
    #[serde(default)]
    pub xmod_noise_to_filter: f32, // 0–1 noise → bass filter cutoff mod depth
    #[serde(default)]
    pub sidechain_amount: f32, // 0–1 sidechain compression depth (kick ducks bass/pad)
    #[serde(default)]
    pub sidechain_attack: f32, // 0–1 → 0.1–50 ms attack
    #[serde(default)]
    pub sidechain_release: f32, // 0–1 → 10–500 ms release
    pub tape_drive: f32,           // 0–1 saturation amount
    pub tape_mix: f32,             // 0–1 wet/dry
    pub tape_flutter: f32,         // 0–1 wow/flutter depth
    #[serde(default)]
    pub master_pitch_st: f32, // -12..+12 semitones: global pitch offset for melodic voices
    pub bitcrush_bits: f32,        // 0–1: 1.0 = full quality (bypass), 0.0 = 1-bit
    pub bitcrush_rate: f32,        // 0–1: 0.0 = no decimation, 1.0 = extreme downsampling
    pub bitcrush_mix: f32,         // 0–1: wet/dry
    pub chorus_rate: f32,          // 0–1 → 0.1–8 Hz LFO rate
    pub chorus_depth: f32,         // 0–1 modulation depth
    pub chorus_mix: f32,           // 0–1 wet/dry
    pub phaser_rate: f32,          // 0–1 → 0.05–5 Hz LFO rate
    pub phaser_depth: f32,         // 0–1 sweep depth
    pub phaser_mix: f32,           // 0–1 wet/dry
    pub waveshaper_drive: f32,     // 0–1 → soft-clip drive amount (pre-FX)
    pub waveshaper_mix: f32,       // 0–1 wet/dry
    pub ring_mod_freq: f32,        // 0–1 → 50–500 Hz carrier frequency
    pub ring_mod_mix: f32,         // 0–1 wet/dry
    pub eq_low_gain: f32,          // -1..+1 → -12..+12 dB low shelf (~200 Hz)
    pub eq_mid_gain: f32,          // -1..+1 → -12..+12 dB mid peak (~1 kHz)
    pub eq_hi_gain: f32,           // -1..+1 → -12..+12 dB high shelf (~5 kHz)
    #[serde(default)]
    pub autotune_amount: f32, // 0–1 → 0..+12 semitones upward pitch shift
    #[serde(default)]
    pub autotune_mix: f32, // 0–1 wet/dry
}

impl Default for FxState {
    fn default() -> Self {
        Self {
            reverb_size: 0.4,
            reverb_damp: 0.5,
            reverb_mix: 0.0,
            reverb_gate_time: 0.0,
            reverb_freeze: false,
            reverb_dir: 0,
            reverb_rev_quant: 0,
            delay_time: 0.375,
            delay_feedback: 0.4,
            delay_mix: 0.0,
            delay_dir: 0,
            delay_rev_quant: 0,
            delay_wow_flutter: 0.0,
            delay_saturation: 0.0,
            distortion_drive: 0.0,
            distortion_mix: 0.0,
            compressor_threshold: 0.7,
            compressor_ratio: 0.3,
            compressor_mix: 0.0,
            compressor_multiband: 0.0,
            master_volume: 0.85,
            stereo_width: 0.5,
            tuning: 0,
            xmod_bass_to_an1x_pitch: 0.0,
            xmod_noise_to_filter: 0.0,
            sidechain_amount: 0.0,
            sidechain_attack: 0.1,
            sidechain_release: 0.3,
            tape_drive: 0.3,
            tape_mix: 0.0,
            tape_flutter: 0.2,
            master_pitch_st: 0.0,
            bitcrush_bits: 1.0,
            bitcrush_rate: 0.0,
            bitcrush_mix: 0.0,
            chorus_rate: 0.3,
            chorus_depth: 0.5,
            chorus_mix: 0.0,
            phaser_rate: 0.3,
            phaser_depth: 0.5,
            phaser_mix: 0.0,
            waveshaper_drive: 0.0,
            waveshaper_mix: 0.0,
            ring_mod_freq: 0.2,
            ring_mod_mix: 0.0,
            eq_low_gain: 0.0,
            eq_mid_gain: 0.0,
            eq_hi_gain: 0.0,
            autotune_amount: 0.0,
            autotune_mix: 0.0,
        }
    }
}

// ─── LLM ─────────────────────────────────────────────────────────────────────

// ConversationMode, TtsModuleState → state/tts_types.rs
pub mod tts_types;
pub use tts_types::{ConversationMode, TtsModuleState};

/// Whether to use the short `brief` or full `description` from styles.json.
/// Brief (~50 tokens) suits smaller/faster models; Full (~150 tokens) for capable models.
#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
pub enum StyleVerbosity {
    Brief, // short keyword-dense creative brief
    #[default]
    Full, // long prose description — more context for capable models
}

/// TTS backend selection.
/// A smooth parameter transition scheduled by the LLM.
///
/// Two timing modes:
/// - **Cycle-based** (legacy): `step_per_cycle != 0`, `total_global_steps == 0`.
///   `current` converges toward `target` by `step_per_cycle` each jam cycle.
/// - **Bar-based**: `total_global_steps > 0`.  Ticked per UI frame using
///   `global_step_count`.  Progress = `(now - start_global_step) / total_global_steps`.
///   Value = `lerp(from, target, progress)`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ParamRamp {
    pub param: String, // dot-path, e.g. "fx.reverb_mix"
    pub current: f32,  // cycle-based: running value.  bar-based: unused (use `from`).
    pub target: f32,
    pub step_per_cycle: f32, // cycle-based: added to current each cycle.  bar-based: 0.
    /// Bar-based ramp: original value at ramp creation.
    #[serde(default)]
    pub from: f32,
    /// Bar-based ramp: `global_step_count` when the ramp was created (0 = cycle-based).
    #[serde(default)]
    pub start_global_step: u64,
    /// Bar-based ramp: duration in sequencer steps (0 = cycle-based).
    #[serde(default)]
    pub total_global_steps: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LlmState {
    pub model_path: String,
    pub last_prompt: String,
    pub last_response: String,
    pub is_inferring: bool,
    pub tokens_per_sec: f32,
    pub prompt_tokens: usize,
    pub completion_tokens: usize,
    pub thinking_tokens: usize, // chars in last _thinking field ÷ 4 (approx)
    pub context_used: usize,
    pub context_max: usize,
    pub locked_params: HashSet<String>, // UserOwned: LLM skips these entirely
    pub focused_params: HashSet<String>, // LlmFocus: LLM should prioritize these
    pub auto_jam: bool,                 // LLM continuously generates pattern variations
    pub heat: f32,                      // 0–1: jam mutation intensity (low=subtle, high=wild)
    #[serde(default = "LlmState::default_temperature")]
    pub temperature: f32, // 0–2: inference sampling temperature sent to llama-server (decoupled from heat)
    pub conversation_mode: ConversationMode,
    pub active_style: Option<String>, // style id from styles.json, "__free__", "__custom__", or None
    pub custom_style_text: String,    // used when active_style == Some("__custom__")
    /// When true (default), agents cannot override the user-selected style via SetStyle action.
    #[serde(default = "default_true")]
    pub style_lock: bool,
    pub user_instructions: String, // persistent user instructions injected into every system prompt
    pub persona_name: String,      // AI persona name shown in UI and used in system prompt
    pub system_prompt_override: String, // if non-empty, replaces the generated system prompt entirely
    pub enable_thinking: bool,          // append /think or /no_think to prompt (Qwen3)
    pub show_thinking_in_log: bool,     // display reasoning blocks inline in the log
    // ── Sampling params (passed to llama-server on every inference call) ──────
    pub top_k: i32,             // 0 = disabled; Gemma default 64
    pub top_p: f32,             // nucleus sampling; Gemma default 0.95
    pub min_p: f32,             // min-prob floor; default 0.05
    pub repeat_penalty: f32,    // 1.0 = off; >1.0 penalises repeated tokens
    pub frequency_penalty: f32, // OpenAI-compat; 0.0 = off
    pub seed: i64,              // -1 = random each call
    // TTS settings moved to per-module TtsModuleState (AppState.tts_modules).
    pub style_verbosity: StyleVerbosity, // Brief = ~50 token brief, Full = ~150 token description
    pub auto_lock_on_touch: bool,        // if true, touching a knob locks it to user-only control
    pub auto_compact: bool,              // restart server automatically when context > 85% full
    pub is_mock: bool,                   // true when running without a real model (no llama-server)
    pub llm_initializing: bool, // true while wait_for_ready is running (suppress false mock warning)
    #[serde(default)]
    pub model_missing: bool, // no model file found on startup — prompt user to download
    #[serde(default)]
    pub active_ramps: Vec<ParamRamp>, // ongoing smooth parameter transitions
    #[serde(default)]
    pub jam_bars: f32, // 0.0 = continuous (fire immediately); N = wait N bars before re-triggering
    #[serde(default)]
    pub jam_cycle_count: u32, // total jam cycles completed (display only)
    /// Global toggle: allow agents to autonomously spawn new agents during jam.
    #[serde(default = "default_true")]
    pub agent_autonomy: bool,
}

fn default_true() -> bool {
    true
}

impl LlmState {
    fn default_temperature() -> f32 {
        0.9
    }
}

impl Default for LlmState {
    fn default() -> Self {
        Self {
            model_path: String::from("models/gemma-4-E4B-it-Q4_K_M.gguf"),
            last_prompt: String::new(),
            last_response: String::new(),
            is_inferring: false,
            tokens_per_sec: 0.0,
            prompt_tokens: 0,
            completion_tokens: 0,
            thinking_tokens: 0,
            context_used: 0,
            context_max: 32768,
            locked_params: HashSet::new(),
            focused_params: HashSet::new(),
            auto_jam: true,
            heat: 0.4,
            temperature: 0.9,
            conversation_mode: ConversationMode::Producer,
            active_style: None,
            custom_style_text: String::new(),
            style_lock: true,
            user_instructions: String::new(),
            persona_name: String::from("PULSE"),
            system_prompt_override: String::new(),
            enable_thinking: true,
            show_thinking_in_log: true,
            top_k: 64,
            top_p: 0.95,
            min_p: 0.05,
            repeat_penalty: 1.0,
            frequency_penalty: 0.0,
            seed: -1,
            style_verbosity: StyleVerbosity::Full,
            auto_lock_on_touch: false,
            auto_compact: true,
            is_mock: false,
            model_missing: false,
            llm_initializing: true, // cleared by LLM thread once live/mock status is known
            active_ramps: Vec::new(),
            jam_bars: 0.0,
            jam_cycle_count: 0,
            agent_autonomy: true,
        }
    }
}

// ─── Per-agent LLM state (rackable LLM modules) ─────────────────────────────

/// Agent role — affects personality flavor in system prompt.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentRole {
    #[default]
    Producer,
    Mc,
    Dj,
    Specialist,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LlmAgentState {
    pub id: u32, // matches RackModule.id
    pub persona_name: String,
    pub heat: f32,
    pub temperature: f32,
    pub scope: Vec<String>, // top-level keys this agent may write (empty = all)
    /// Agent role — affects system prompt personality flavor.
    #[serde(default)]
    pub role: AgentRole,
    /// Whether this agent may spawn new agents during jam cycles.
    #[serde(default)]
    pub can_spawn: bool,
    /// Whether this agent may dismiss itself during jam cycles.
    #[serde(default)]
    pub can_dismiss: bool,
    pub jam_bars: f32,
    pub conversation_mode: ConversationMode,
    pub active_style: Option<String>,
    pub custom_style_text: String,
    pub user_instructions: String,
    pub enable_thinking: bool,
    /// Per-agent system prompt override. Empty = use auto-generated prompt.
    #[serde(default)]
    pub system_prompt_override: String,
    /// Per-agent model override. `None` = inherit from global `LlmState.model_path`.
    #[serde(default)]
    pub model_path: Option<String>,
    // Display-only (updated by inference thread)
    #[serde(default)]
    pub is_inferring: bool,
    #[serde(default)]
    pub last_response: String,
    #[serde(default)]
    pub tokens_per_sec: f32,
    #[serde(default)]
    pub jam_cycle_count: u32,
    /// Persistent memory: conversation snippets preserved across sessions.
    /// Injected into the system prompt so the agent remembers prior context.
    #[serde(default)]
    pub memory: Vec<String>,
    /// Style learning: user preference observations (e.g. "user likes high reverb").
    /// Accumulated from UI edits; injected into system prompt alongside memory.
    #[serde(default)]
    pub style_observations: Vec<String>,
    /// Pending hints from other agents, consumed on next inference.
    #[serde(default)]
    pub pending_hints: Vec<String>,
    /// When true, this agent's style is independent and won't change when the
    /// global style is changed. When false (default), style syncs with global.
    #[serde(default)]
    pub style_locked: bool,
}

/// Maximum number of memory entries per agent.
pub const AGENT_MEMORY_MAX: usize = 20;
/// Maximum number of style observations.
pub const STYLE_OBS_MAX: usize = 10;

impl LlmAgentState {
    pub fn new_default(id: u32) -> Self {
        Self {
            id,
            persona_name: "PULSE".to_string(),
            heat: 0.4,
            temperature: 0.9,
            scope: Vec::new(),
            role: AgentRole::Producer,
            can_spawn: true,
            can_dismiss: true,
            jam_bars: 0.0,
            conversation_mode: ConversationMode::Producer,
            active_style: None,
            custom_style_text: String::new(),
            user_instructions: String::new(),
            enable_thinking: true,
            system_prompt_override: String::new(),
            model_path: None,
            is_inferring: false,
            last_response: String::new(),
            tokens_per_sec: 0.0,
            jam_cycle_count: 0,
            memory: Vec::new(),
            style_observations: Vec::new(),
            pending_hints: Vec::new(),
            style_locked: false,
        }
    }

    /// Create an agent from the current singleton LlmState values.
    pub fn from_singleton(id: u32, llm: &LlmState) -> Self {
        Self {
            id,
            persona_name: llm.persona_name.clone(),
            heat: llm.heat,
            temperature: llm.temperature,
            scope: Vec::new(),
            role: AgentRole::Producer,
            can_spawn: true,
            can_dismiss: true,
            jam_bars: llm.jam_bars,
            conversation_mode: llm.conversation_mode.clone(),
            active_style: llm.active_style.clone(),
            custom_style_text: llm.custom_style_text.clone(),
            user_instructions: llm.user_instructions.clone(),
            enable_thinking: llm.enable_thinking,
            system_prompt_override: String::new(),
            model_path: None,
            is_inferring: false,
            last_response: String::new(),
            tokens_per_sec: 0.0,
            jam_cycle_count: 0,
            memory: Vec::new(),
            style_observations: Vec::new(),
            pending_hints: Vec::new(),
            style_locked: false,
        }
    }
}

/// Sync the default (first) LlmAgentState with the global LlmState.
pub mod jam_tools;
pub mod llm_apply;
pub(crate) mod llm_helpers;
pub mod transitions;

pub use transitions::*;

pub mod persistence;
pub use persistence::{
    apply_session, load_model_setting, load_session, save_model_setting, save_project,
    save_session, save_session_ext,
};
