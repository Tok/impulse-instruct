// ─── state/mod.rs ────────────────────────────────────────────────────────────
#![allow(dead_code)] // many transition fns are called via API/MIDI, not yet wired in UI
// Single source of truth for all synth parameters.
// Pure data only — no methods that mutate in-place.
// All state transitions happen via the named functions at the bottom.

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
pub use ui_prefs::{AutosaveInterval, HuthStyle, KnobSize, KnobStyle, PadSize, UiPrefs};

pub mod rack;
pub use rack::{
    Cable, CableColor, FxPlan, FxStep, ModuleKind, PortDir, PortKind, PortRef, RackModule,
    RackState, Zone, compile_fx_plan,
};

// ─── Amen / WAV sampler voice state ──────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AmenState {
    /// Path to the WAV file to load (empty = no sample loaded).
    pub path: String,
    /// Playback pitch offset in semitones (-24 to +24). 0 = original pitch.
    pub pitch: f32,
    /// Output volume (0.0–1.0).
    pub volume: f32,
    /// When true, loops the sample; otherwise plays once per trigger.
    pub loop_mode: bool,
}

impl Default for AmenState {
    fn default() -> Self {
        Self {
            path: String::new(),
            pitch: 0.0,
            volume: 0.75,
            loop_mode: false,
        }
    }
}

// ─── Top-level ───────────────────────────────────────────────────────────────

fn default_pattern_bank() -> Vec<SequencerState> {
    vec![SequencerState::default(); 8]
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
    /// Modular rack — which modules are visible and how they are cabled.
    #[serde(default)]
    pub rack: RackState,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
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
            rack: Default::default(),
        }
    }
}

// ─── Bass synth ───────────────────────────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BassState {
    pub cutoff: f32,       // 0–1 → 200–8000 Hz
    pub resonance: f32,    // 0–1
    pub env_mod: f32,      // 0–1 filter env depth
    pub decay: f32,        // 0–1 → 50–2000 ms
    pub accent_level: f32, // 0–1
    pub waveform: Waveform,
    pub filter_mode: FilterMode, // LP / HP / BP
    pub distortion: f32,         // 0–1
    pub volume: f32,             // 0–1
    pub supersaw_detune: f32,    // 0–1 → 0–1 semitone spread between voices
    pub supersaw_voices: u8,     // 2–7
    pub sub_osc_level: f32,      // 0–1 sine one octave below, mixed before filter
    pub portamento_time: f32,    // 0–1 → 10ms–500ms slide/glide time
    pub noise_mix: f32,          // 0–1 white noise mixed into oscillator before filter
    pub osc_detune: f32,         // semitone offset -1..+1, shifts entire oscillator pitch
    pub fm_ratio: f32,           // 0–1 → modulator/carrier ratio 0.5–8.0
    pub fm_depth: f32,           // 0–1 FM modulation depth; 0 = off (pure additive)
}

impl Default for BassState {
    fn default() -> Self {
        Self {
            cutoff: 0.4,
            resonance: 0.6,
            env_mod: 0.5,
            decay: 0.4,
            accent_level: 0.7,
            waveform: Waveform::Saw,
            filter_mode: FilterMode::Lowpass,
            distortion: 0.2,
            volume: 0.8,
            supersaw_detune: 0.5,
            supersaw_voices: 5,
            sub_osc_level: 0.0,
            portamento_time: 0.1, // ~60ms
            noise_mix: 0.0,
            osc_detune: 0.0,
            fm_ratio: 0.0,
            fm_depth: 0.0,
        }
    }
}

// ─── Multi-voice bass ─────────────────────────────────────────────────────────

/// One bass voice slot: a `BassState` synth engine + per-voice musical context.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BassVoiceState {
    /// Synth parameters for this voice (identical layout to the legacy `BassState`).
    pub synth: BassState,
    /// Tonic note override (0=C … 11=B). Only used when `lock_key` is false.
    pub root_note: u8,
    /// Scale override. Only used when `lock_key` is false.
    pub scale: Scale,
    /// When true, use the global `sequencer.root_note` / `sequencer.scale` instead.
    pub lock_key: bool,
    /// Whether this voice is active. Voice 0 is always on; voices 1-3 can be toggled.
    pub enabled: bool,
}

impl Default for BassVoiceState {
    fn default() -> Self {
        Self {
            synth: BassState::default(),
            root_note: 9, // A — matches the default starter pattern
            scale: Scale::NaturalMinor,
            lock_key: true, // follow the global key by default
            enabled: false, // voices 1-3 start disabled; voice 0 is always enabled
        }
    }
}

fn default_bass_voices() -> Vec<BassVoiceState> {
    let mut voices: Vec<BassVoiceState> = (0..MAX_BASS_VOICES)
        .map(|_| BassVoiceState::default())
        .collect();
    // Voice 0 is always enabled
    voices[0].enabled = true;
    voices
}

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
}

impl Default for TB303Step {
    fn default() -> Self {
        Self {
            active: false,
            note: 36,
            accent: false,
            slide: false,
            gate: 0.5,
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
}

impl Default for SequencerState {
    fn default() -> Self {
        let mut drum_patterns = std::collections::HashMap::new();
        // Pre-allocate MAX_STEPS silent steps; only the first `steps` are active in the clock.
        for v in DrumVoice::ALL {
            drum_patterns.insert(*v, vec![Step::default(); MAX_STEPS]);
        }

        // Starter beat: 4-on-the-floor kick + offbeat closed hi-hats.
        let kick_steps = [1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0usize];
        let hat_steps = [0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 1, 0usize];
        if let Some(p) = drum_patterns.get_mut(&DrumVoice::Kick808) {
            for (i, &on) in kick_steps.iter().enumerate() {
                p[i].active = on == 1;
                p[i].velocity = 1.0;
            }
        }
        if let Some(p) = drum_patterns.get_mut(&DrumVoice::HihatClosed808) {
            for (i, &on) in hat_steps.iter().enumerate() {
                p[i].active = on == 1;
                p[i].velocity = 0.7;
            }
        }

        // Starter bass: A minor chord tones spread across the bar.
        // A2 (45) on beat 1 · C3 (48) between beats 2–3 · E3 (52) on beat 4.
        let mut bass_pattern = vec![TB303Step::default(); MAX_STEPS];
        let bass_notes: &[(usize, u8)] = &[(0, 45), (6, 48), (12, 52)];
        for &(step, note) in bass_notes {
            bass_pattern[step].active = true;
            bass_pattern[step].note = note;
        }

        // Default: all drum voices use the global `steps` length.
        let mut drum_steps = std::collections::HashMap::new();
        for v in DrumVoice::ALL {
            drum_steps.insert(*v, 16usize);
        }

        Self {
            bpm: 120.0,
            steps: 16,
            current_step: 0,
            running: false,
            swing: 0.0,
            time_sig_num: 4,
            drum_patterns,
            bass_pattern: bass_pattern.clone(),
            hoover_pattern: vec![TB303Step::default(); MAX_STEPS],
            an1x_pattern: vec![TB303Step::default(); MAX_STEPS],
            root_note: 9, // A — the starter pattern is A minor
            scale: Scale::NaturalMinor,
            scale_snap: false,
            drum_steps,
            bass_steps: 16,
            hoover_steps: 16,
            an1x_steps: 16,
            muted_drums: std::collections::HashSet::new(),
            soloed_drums: std::collections::HashSet::new(),
            midi_clock_sync: false,
            bass_patterns: {
                // Voice 0 = same starter pattern; voices 1-3 = silent
                let mut pats = vec![bass_pattern];
                for _ in 1..MAX_BASS_VOICES {
                    pats.push(vec![TB303Step::default(); MAX_STEPS]);
                }
                pats
            },
            bass_voice_steps: vec![16usize; MAX_BASS_VOICES],
            bass_voice_enabled: default_bass_voice_enabled(),
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
    pub delay_time: f32,  // 0–1 → 0–1000 ms
    pub delay_feedback: f32, // 0–1
    pub delay_mix: f32,   // 0–1 wet/dry
    pub distortion_drive: f32, // 0–1
    pub distortion_mix: f32, // 0–1 wet/dry
    pub compressor_threshold: f32, // 0–1 → -40–0 dB
    pub compressor_ratio: f32, // 0–1 → 1:1–20:1
    pub compressor_mix: f32, // 0–1 wet/dry (0 = bypassed)
    pub master_volume: f32, // 0–1
    pub tape_drive: f32,  // 0–1 saturation amount
    pub tape_mix: f32,    // 0–1 wet/dry
    pub tape_flutter: f32, // 0–1 wow/flutter depth
    #[serde(default)]
    pub master_pitch_st: f32, // -12..+12 semitones: global pitch offset for melodic voices
    pub bitcrush_bits: f32, // 0–1: 1.0 = full quality (bypass), 0.0 = 1-bit
    pub bitcrush_rate: f32, // 0–1: 0.0 = no decimation, 1.0 = extreme downsampling
    pub bitcrush_mix: f32, // 0–1: wet/dry
    pub chorus_rate: f32, // 0–1 → 0.1–8 Hz LFO rate
    pub chorus_depth: f32, // 0–1 modulation depth
    pub chorus_mix: f32,  // 0–1 wet/dry
    pub phaser_rate: f32, // 0–1 → 0.05–5 Hz LFO rate
    pub phaser_depth: f32, // 0–1 sweep depth
    pub phaser_mix: f32,  // 0–1 wet/dry
    pub waveshaper_drive: f32, // 0–1 → soft-clip drive amount (pre-FX)
    pub waveshaper_mix: f32, // 0–1 wet/dry
    pub ring_mod_freq: f32, // 0–1 → 50–500 Hz carrier frequency
    pub ring_mod_mix: f32, // 0–1 wet/dry
    pub eq_low_gain: f32, // -1..+1 → -12..+12 dB low shelf (~200 Hz)
    pub eq_mid_gain: f32, // -1..+1 → -12..+12 dB mid peak (~1 kHz)
    pub eq_hi_gain: f32,  // -1..+1 → -12..+12 dB high shelf (~5 kHz)
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
            delay_time: 0.375,
            delay_feedback: 0.4,
            delay_mix: 0.0,
            distortion_drive: 0.0,
            distortion_mix: 0.0,
            compressor_threshold: 0.7,
            compressor_ratio: 0.3,
            compressor_mix: 0.0,
            master_volume: 0.85,
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

/// How the AI persona presents itself in the `_comment` field.
#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
pub enum ConversationMode {
    Off, // no commentary shown; brief technical label only
    #[default]
    Producer, // candid — what changed and why it serves the music
    Dj,  // hype DJ persona, cheesy party energy
    Mc,  // jungle/rave MC — "selector!", "junglist massive!", "rewind!"
}

/// espeak-ng voice character preset for TTS output.
/// Auto = follow ConversationMode's default. Others override the voice selection.
#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
pub enum McVoiceChar {
    #[default]
    Auto, // follow ConversationMode defaults
    JungleMc,      // fast, high-pitched ragga MC (en+m3)
    RaveAnnouncer, // loud, rapid hype announcer (en+m2)
    Robot,         // robotic, flat, low (en+m7)
    SmoothDj,      // mid-low smooth DJ (en+m4)
}

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
/// `current` converges toward `target` by `step_per_cycle` each jam cycle.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ParamRamp {
    pub param: String, // dot-path, e.g. "fx.reverb_mix"
    pub current: f32,
    pub target: f32,
    pub step_per_cycle: f32, // added to current each cycle until target is reached
}

/// EspeakNg: always available, low quality, zero setup.
/// CoquiTts: higher quality neural voice; requires `tts` CLI in PATH. Falls
///           back to espeak-ng automatically if the binary is not found.
#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
pub enum TtsEngine {
    #[default]
    EspeakNg,
    CoquiTts,
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
    pub user_instructions: String, // persistent user instructions injected into every system prompt
    pub persona_name: String,      // AI persona name shown in UI and used in system prompt
    pub system_prompt_override: String, // if non-empty, replaces the generated system prompt entirely
    pub enable_thinking: bool,          // append /think or /no_think to prompt (Qwen3)
    pub show_thinking_in_log: bool,     // display reasoning blocks inline in the log
    // ── Sampling params (passed to llama-server on every inference call) ──────
    pub top_k: i32,                  // 0 = disabled; Gemma default 64
    pub top_p: f32,                  // nucleus sampling; Gemma default 0.95
    pub min_p: f32,                  // min-prob floor; default 0.05
    pub repeat_penalty: f32,         // 1.0 = off; >1.0 penalises repeated tokens
    pub frequency_penalty: f32,      // OpenAI-compat; 0.0 = off
    pub seed: i64,                   // -1 = random each call
    pub tts_enabled: bool,           // speak MC/DJ content via espeak-ng when true
    pub tts_pitch: u8,               // 0 = mode default; 1–99 override
    pub tts_speed: u16,              // 0 = mode default; words/min override
    pub tts_amplitude: u8,           // 0 = default (100); 1–200 override
    pub tts_voice_char: McVoiceChar, // voice character preset (Auto = follow mode)
    pub tts_randomise: bool,         // ±10% pitch/speed jitter per utterance
    pub tts_pitch_snap: bool,        // snap TTS voice to nearest in-key note (T-Pain effect)
    #[serde(default)]
    pub tts_engine: TtsEngine, // EspeakNg (default) or CoquiTts (falls back if not installed)
    pub style_verbosity: StyleVerbosity, // Brief = ~50 token brief, Full = ~150 token description
    pub auto_lock_on_touch: bool,    // if true, touching a knob locks it to user-only control
    pub auto_compact: bool,          // restart server automatically when context > 85% full
    pub is_mock: bool,               // true when running without a real model (no llama-server)
    pub llm_initializing: bool, // true while wait_for_ready is running (suppress false mock warning)
    #[serde(default)]
    pub model_missing: bool, // no model file found on startup — prompt user to download
    #[serde(default)]
    pub active_ramps: Vec<ParamRamp>, // ongoing smooth parameter transitions
    #[serde(default)]
    pub jam_bars: f32, // 0.0 = continuous (fire immediately); N = wait N bars before re-triggering
    #[serde(default)]
    pub jam_cycle_count: u32, // total jam cycles completed (display only)
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
            tts_enabled: false,
            tts_pitch: 0,
            tts_speed: 0,
            tts_amplitude: 0,
            tts_voice_char: McVoiceChar::Auto,
            tts_randomise: false,
            tts_pitch_snap: false,
            tts_engine: TtsEngine::EspeakNg,
            style_verbosity: StyleVerbosity::Full,
            auto_lock_on_touch: false,
            auto_compact: true,
            is_mock: false,
            model_missing: false,
            llm_initializing: true, // cleared by LLM thread once live/mock status is known
            active_ramps: Vec::new(),
            jam_bars: 0.0,
            jam_cycle_count: 0,
        }
    }
}

pub mod jam_tools;
pub mod llm_apply;
pub(crate) mod llm_helpers;
pub mod transitions;

pub use transitions::*;

pub mod persistence;
pub use persistence::{
    apply_session, load_model_setting, load_session, save_model_setting, save_project, save_session,
};
