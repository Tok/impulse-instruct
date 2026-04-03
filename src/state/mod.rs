// ─── state/mod.rs ────────────────────────────────────────────────────────────
#![allow(dead_code)] // many transition fns are called via API/MIDI, not yet wired in UI
// Single source of truth for all synth parameters.
// Pure data only — no methods that mutate in-place.
// All state transitions happen via the named functions at the bottom.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub const MAX_STEPS: usize = 64;

// ─── Top-level ───────────────────────────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AppState {
    pub bass: BassState,
    pub kit_a: DrumKit808,
    pub kit_b: DrumKit909,
    pub sequencer: SequencerState,
    pub fx: FxState,
    pub llm: LlmState,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            bass: BassState::default(),
            kit_a: DrumKit808::default(),
            kit_b: DrumKit909::default(),
            sequencer: SequencerState::default(),
            fx: FxState::default(),
            llm: LlmState::default(),
        }
    }
}

// ─── Bass synth ───────────────────────────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum Waveform {
    Saw,
    Square,
    Supersaw, // detuned unison saws
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BassState {
    pub cutoff: f32,       // 0–1 → 200–8000 Hz
    pub resonance: f32,    // 0–1
    pub env_mod: f32,      // 0–1 filter env depth
    pub decay: f32,        // 0–1 → 50–2000 ms
    pub accent_level: f32, // 0–1
    pub waveform: Waveform,
    pub distortion: f32,          // 0–1
    pub volume: f32,               // 0–1
    pub supersaw_detune: f32,      // 0–1 → 0–1 semitone spread between voices
    pub supersaw_voices: u8,       // 2–7
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
            distortion: 0.2,
            volume: 0.8,
            supersaw_detune: 0.5,
            supersaw_voices: 5,
        }
    }
}

// ─── Drum Kit A (808-style) ───────────────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct KickParams {
    pub pitch: f32,   // 0–1 → 40–80 Hz base
    pub decay: f32,   // 0–1 → 0.2–2.0 s
    pub punch: f32,   // 0–1 attack transient
    pub tone: f32,    // 0–1 sine/noise blend
    pub volume: f32,  // 0–1
}

impl Default for KickParams {
    fn default() -> Self {
        Self { pitch: 0.5, decay: 0.6, punch: 0.45, tone: 0.8, volume: 0.65 }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SnareParams {
    pub tone: f32,    // 0–1 tone freq
    pub snappy: f32,  // 0–1 noise amount
    pub decay: f32,   // 0–1
    pub volume: f32,
}

impl Default for SnareParams {
    fn default() -> Self {
        Self { tone: 0.5, snappy: 0.6, decay: 0.4, volume: 0.85 }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HihatParams {
    pub decay: f32,   // 0–1 (open hat = higher)
    pub tone: f32,    // 0–1 filter cutoff
    pub volume: f32,
}

impl Default for HihatParams {
    fn default() -> Self {
        Self { decay: 0.2, tone: 0.7, volume: 0.75 }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TomParams {
    pub pitch: f32,
    pub decay: f32,
    pub volume: f32,
}

impl Default for TomParams {
    fn default() -> Self {
        Self { pitch: 0.5, decay: 0.5, volume: 0.7 }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DrumKit808 {
    pub kick: KickParams,
    pub snare: SnareParams,
    pub hihat_closed: HihatParams,
    pub hihat_open: HihatParams,
    pub tom_hi: TomParams,
    pub tom_mid: TomParams,
    pub tom_lo: TomParams,
}

impl Default for DrumKit808 {
    fn default() -> Self {
        Self {
            kick: KickParams::default(),
            snare: SnareParams::default(),
            hihat_closed: HihatParams { decay: 0.08, tone: 0.8, volume: 0.7 },
            hihat_open: HihatParams { decay: 0.4, tone: 0.75, volume: 0.7 },
            tom_hi: TomParams { pitch: 0.7, decay: 0.4, volume: 0.7 },
            tom_mid: TomParams { pitch: 0.5, decay: 0.45, volume: 0.7 },
            tom_lo: TomParams { pitch: 0.3, decay: 0.5, volume: 0.7 },
        }
    }
}

// ─── Drum Kit B (909-style) ───────────────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ClapParams {
    pub decay: f32,
    pub volume: f32,
}

impl Default for ClapParams {
    fn default() -> Self {
        Self { decay: 0.3, volume: 0.8 }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DrumKit909 {
    pub kick: KickParams,
    pub snare: SnareParams,
    pub hihat_closed: HihatParams,
    pub hihat_open: HihatParams,
    pub clap: ClapParams,
    pub rim: SnareParams, // rim shot reuses snare params
}

impl Default for DrumKit909 {
    fn default() -> Self {
        Self {
            kick: KickParams { pitch: 0.55, decay: 0.5, punch: 0.5, tone: 0.9, volume: 0.65 },
            snare: SnareParams { tone: 0.55, snappy: 0.7, decay: 0.35, volume: 0.85 },
            hihat_closed: HihatParams { decay: 0.06, tone: 0.85, volume: 0.7 },
            hihat_open: HihatParams { decay: 0.45, tone: 0.8, volume: 0.7 },
            clap: ClapParams::default(),
            rim: SnareParams { tone: 0.7, snappy: 0.3, decay: 0.15, volume: 0.75 },
        }
    }
}

// ─── Sequencer ───────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, Serialize, Deserialize, Default, PartialEq)]
pub struct Step {
    pub active: bool,
    pub velocity: f32, // 0–1
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct TB303Step {
    pub active: bool,
    pub note: u8,   // MIDI note number
    pub accent: bool,
    pub slide: bool,
    pub gate: f32, // 0–1 gate length ratio
}

impl Default for TB303Step {
    fn default() -> Self {
        Self { active: false, note: 36, accent: false, slide: false, gate: 0.5 }
    }
}

// Which drum machine each drum pattern lane belongs to
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum DrumVoice {
    Kick808,
    Snare808,
    HihatClosed808,
    HihatOpen808,
    TomHi808,
    TomMid808,
    TomLo808,
    Kick909,
    Snare909,
    HihatClosed909,
    HihatOpen909,
    Clap909,
    Rim909,
}

impl DrumVoice {
    pub const ALL: &'static [DrumVoice] = &[
        DrumVoice::Kick808,
        DrumVoice::Snare808,
        DrumVoice::HihatClosed808,
        DrumVoice::HihatOpen808,
        DrumVoice::TomHi808,
        DrumVoice::TomMid808,
        DrumVoice::TomLo808,
        DrumVoice::Kick909,
        DrumVoice::Snare909,
        DrumVoice::HihatClosed909,
        DrumVoice::HihatOpen909,
        DrumVoice::Clap909,
        DrumVoice::Rim909,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            DrumVoice::Kick808 => "KA Kick",
            DrumVoice::Snare808 => "KA Snare",
            DrumVoice::HihatClosed808 => "KA CHH",
            DrumVoice::HihatOpen808 => "KA OHH",
            DrumVoice::TomHi808 => "KA Hi Tom",
            DrumVoice::TomMid808 => "KA Mid Tom",
            DrumVoice::TomLo808 => "KA Lo Tom",
            DrumVoice::Kick909 => "KB Kick",
            DrumVoice::Snare909 => "KB Snare",
            DrumVoice::HihatClosed909 => "KB CHH",
            DrumVoice::HihatOpen909 => "KB OHH",
            DrumVoice::Clap909 => "KB Clap",
            DrumVoice::Rim909 => "KB Rim",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SequencerState {
    pub bpm: f32,
    pub steps: usize, // 1–64, active step count
    pub current_step: usize,
    pub running: bool,
    pub drum_patterns: std::collections::HashMap<DrumVoice, Vec<Step>>,
    pub bass_pattern: Vec<TB303Step>,
}

impl Default for SequencerState {
    fn default() -> Self {
        let mut drum_patterns = std::collections::HashMap::new();
        // Pre-allocate MAX_STEPS silent steps; only the first `steps` are active in the clock.
        for v in DrumVoice::ALL {
            drum_patterns.insert(*v, vec![Step::default(); MAX_STEPS]);
        }

        // Minimal starter beat: 4-on-the-floor kick + offbeat hi-hats.
        // Just enough to hear the clock — Bonsai writes all creative patterns.
        let kick_steps = [1,0,0,0, 1,0,0,0, 1,0,0,0, 1,0,0,0usize];
        let hat_steps  = [0,0,1,0, 0,0,1,0, 0,0,1,0, 0,0,1,0usize];
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

        Self {
            bpm: 120.0,
            steps: 16,
            current_step: 0,
            running: false,
            drum_patterns,
            bass_pattern: vec![TB303Step::default(); MAX_STEPS],
        }
    }
}

// ─── FX Chain ────────────────────────────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FxState {
    pub reverb_size: f32,       // 0–1 room size
    pub reverb_damp: f32,       // 0–1 damping
    pub reverb_mix: f32,        // 0–1 wet/dry
    pub delay_time: f32,        // 0–1 → 0–1000 ms
    pub delay_feedback: f32,    // 0–1
    pub delay_mix: f32,         // 0–1 wet/dry
    pub distortion_drive: f32,  // 0–1
    pub distortion_mix: f32,    // 0–1 wet/dry
    pub compressor_threshold: f32, // 0–1 → -40–0 dB
    pub compressor_ratio: f32,     // 0–1 → 1–20:1
    pub master_volume: f32,     // 0–1
    pub bitcrush_bits: f32,  // 0–1: 1.0 = full quality (bypass), 0.0 = 1-bit
    pub bitcrush_rate: f32,  // 0–1: 0.0 = no decimation, 1.0 = extreme downsampling
    pub bitcrush_mix: f32,   // 0–1: wet/dry
}

impl Default for FxState {
    fn default() -> Self {
        Self {
            reverb_size: 0.4,
            reverb_damp: 0.5,
            reverb_mix: 0.0,
            delay_time: 0.375,
            delay_feedback: 0.4,
            delay_mix: 0.0,
            distortion_drive: 0.0,
            distortion_mix: 0.0,
            compressor_threshold: 0.7,
            compressor_ratio: 0.3,
            master_volume: 0.85,
            bitcrush_bits: 1.0,
            bitcrush_rate: 0.0,
            bitcrush_mix: 0.0,
        }
    }
}

// ─── LLM ─────────────────────────────────────────────────────────────────────

/// How Bonsai presents itself in the `_comment` field.
#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
pub enum ConversationMode {
    Off,            // no commentary shown; brief technical label only
    #[default]
    Producer,       // candid — what changed and why it serves the music
    Dj,             // hype DJ persona, cheesy party energy
    Mc,             // jungle/rave MC — "selector!", "junglist massive!", "rewind!"
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
    pub locked_params: HashSet<String>, // dot-path keys the user has taken over
    pub auto_jam: bool, // LLM continuously generates pattern variations
    pub heat: f32,      // 0–1: jam mutation intensity (low=subtle, high=wild)
    pub conversation_mode: ConversationMode,
    pub active_style: Option<String>, // style id from styles.json, "__free__", "__custom__", or None
    pub custom_style_text: String,    // used when active_style == Some("__custom__")
    pub user_instructions: String,    // persistent user instructions injected into every system prompt
    pub persona_name: String,         // AI persona name shown in UI and used in system prompt
    pub system_prompt_override: String, // if non-empty, replaces the generated system prompt entirely
    pub tts_enabled: bool,            // speak _comment via espeak-ng when true
}

impl Default for LlmState {
    fn default() -> Self {
        Self {
            model_path: String::from("models/Bonsai-8B.gguf"),
            last_prompt: String::new(),
            last_response: String::new(),
            is_inferring: false,
            tokens_per_sec: 0.0,
            prompt_tokens: 0,
            completion_tokens: 0,
            thinking_tokens: 0,
            context_used: 0,
            context_max: 4096,
            locked_params: {
                let mut s = HashSet::new();
                s.insert("sequencer.bpm".to_string()); // user controls tempo by default
                s
            },
            auto_jam: false,
            heat: 0.4,
            conversation_mode: ConversationMode::Producer,
            active_style: None,
            custom_style_text: String::new(),
            user_instructions: String::new(),
            persona_name: String::from("PULSE"),
            system_prompt_override: String::new(),
            tts_enabled: false,
        }
    }
}

// ─── Pure state transition functions ─────────────────────────────────────────

/// Apply an LLM-generated partial update, respecting locked params.
/// Returns the new state (caller replaces old state with this).
pub fn apply_llm_update(state: AppState, update: &serde_json::Value) -> AppState {
    let mut s = state;
    let locked = &s.llm.locked_params.clone();

    if let Some(b) = update.get("bass").and_then(|v| v.as_object()) {
        s.bass.cutoff       = unlocked_f32(s.bass.cutoff,       b, "cutoff",       "bass.cutoff",       locked);
        s.bass.resonance    = unlocked_f32(s.bass.resonance,    b, "resonance",    "bass.resonance",    locked);
        s.bass.env_mod      = unlocked_f32(s.bass.env_mod,      b, "env_mod",      "bass.env_mod",      locked);
        s.bass.decay        = unlocked_f32(s.bass.decay,        b, "decay",        "bass.decay",        locked);
        s.bass.accent_level = unlocked_f32(s.bass.accent_level, b, "accent_level", "bass.accent_level", locked);
        s.bass.distortion   = unlocked_f32(s.bass.distortion,   b, "distortion",   "bass.distortion",   locked);
        s.bass.volume       = unlocked_f32(s.bass.volume,       b, "volume",       "bass.volume",       locked);
        s.bass.supersaw_detune = unlocked_f32(s.bass.supersaw_detune, b, "supersaw_detune", "bass.supersaw_detune", locked);
        if let Some(v) = b.get("supersaw_voices").and_then(|v| v.as_u64()) {
            if !locked.contains("bass.supersaw_voices") {
                s.bass.supersaw_voices = (v as u8).clamp(2, 7);
            }
        }
        if !locked.contains("bass.waveform") {
            if let Some(w) = b.get("waveform").and_then(|v| v.as_str()) {
                s.bass.waveform = match w {
                    "Square"   => Waveform::Square,
                    "Supersaw" => Waveform::Supersaw,
                    _          => Waveform::Saw,
                };
            }
        }
    }

    if let Some(seq) = update.get("sequencer").and_then(|v| v.as_object()) {
        if !locked.contains("sequencer.bpm") {
            if let Some(bpm) = seq.get("bpm").and_then(|v| v.as_f64()) {
                s.sequencer.bpm = (bpm as f32).clamp(40.0, 250.0);
            }
        }
        if !locked.contains("sequencer.steps") {
            if let Some(steps) = seq.get("steps").and_then(|v| v.as_u64()) {
                s = expand_sequencer_steps(s, steps as usize);
            }
        }
        if !locked.contains("sequencer.bass_steps") {
            if let Some(arr) = seq.get("bass_steps").and_then(|v| v.as_array()) {
                for (i, val) in arr.iter().enumerate().take(MAX_STEPS) {
                    if let Some(active) = val.as_bool() {
                        s.sequencer.bass_pattern[i].active = active;
                    }
                }
            }
        }
        if !locked.contains("sequencer.bass_notes") {
            if let Some(arr) = seq.get("bass_notes").and_then(|v| v.as_array()) {
                for (i, val) in arr.iter().enumerate().take(MAX_STEPS) {
                    if let Some(note) = val.as_u64() {
                        s.sequencer.bass_pattern[i].note = note.clamp(0, 127) as u8;
                    }
                }
            }
        }
        if !locked.contains("sequencer.kick_a_steps") {
            if let Some(arr) = seq.get("kick_a_steps").and_then(|v| v.as_array()) {
                if let Some(pattern) = s.sequencer.drum_patterns.get_mut(&DrumVoice::Kick808) {
                    for (i, val) in arr.iter().enumerate().take(MAX_STEPS) {
                        if let Some(active) = val.as_bool() {
                            pattern[i].active = active;
                            if active && pattern[i].velocity == 0.0 {
                                pattern[i].velocity = 1.0;
                            }
                        }
                    }
                }
            }
        }
        // Generic drum pattern helper — avoids repeating the same block per voice
        let drum_pattern_fields: &[(&str, DrumVoice, f32)] = &[
            ("hihat_a_steps",  DrumVoice::HihatClosed808, 0.7),
            ("snare_a_steps",  DrumVoice::Snare808,       1.0),
            ("kick_b_steps",   DrumVoice::Kick909,        1.0),
            ("snare_b_steps",  DrumVoice::Snare909,       1.0),
            ("clap_b_steps",   DrumVoice::Clap909,        1.0),
            ("hihat_b_steps",  DrumVoice::HihatClosed909, 0.7),
        ];
        for &(field, voice, default_vel) in drum_pattern_fields {
            let lock_key = format!("sequencer.{}", field);
            if !locked.contains(&lock_key) {
                if let Some(arr) = seq.get(field).and_then(|v| v.as_array()) {
                    if let Some(pattern) = s.sequencer.drum_patterns.get_mut(&voice) {
                        for (i, val) in arr.iter().enumerate().take(MAX_STEPS) {
                            if let Some(active) = val.as_bool() {
                                pattern[i].active = active;
                                if active && pattern[i].velocity == 0.0 {
                                    pattern[i].velocity = default_vel;
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    if let Some(fx) = update.get("fx").and_then(|v| v.as_object()) {
        s.fx.reverb_size      = unlocked_f32(s.fx.reverb_size,      fx, "reverb_size",      "fx.reverb_size",      locked);
        s.fx.reverb_mix       = unlocked_f32(s.fx.reverb_mix,       fx, "reverb_mix",       "fx.reverb_mix",       locked);
        s.fx.delay_time       = unlocked_f32(s.fx.delay_time,       fx, "delay_time",       "fx.delay_time",       locked);
        s.fx.delay_feedback   = unlocked_f32(s.fx.delay_feedback,   fx, "delay_feedback",   "fx.delay_feedback",   locked);
        s.fx.delay_mix        = unlocked_f32(s.fx.delay_mix,        fx, "delay_mix",        "fx.delay_mix",        locked);
        s.fx.distortion_drive = unlocked_f32(s.fx.distortion_drive, fx, "distortion_drive", "fx.distortion_drive", locked);
        s.fx.distortion_mix   = unlocked_f32(s.fx.distortion_mix,   fx, "distortion_mix",   "fx.distortion_mix",   locked);
        s.fx.bitcrush_bits    = unlocked_f32(s.fx.bitcrush_bits,    fx, "bitcrush_bits",    "fx.bitcrush_bits",    locked);
        s.fx.bitcrush_rate    = unlocked_f32(s.fx.bitcrush_rate,    fx, "bitcrush_rate",    "fx.bitcrush_rate",    locked);
        s.fx.bitcrush_mix     = unlocked_f32(s.fx.bitcrush_mix,     fx, "bitcrush_mix",     "fx.bitcrush_mix",     locked);
    }

    s
}

/// Returns the updated value if not locked, otherwise returns the original.
/// Pure — no side effects.
fn unlocked_f32(
    current: f32,
    obj: &serde_json::Map<String, serde_json::Value>,
    key: &str,
    path: &str,
    locked: &HashSet<String>,
) -> f32 {
    if locked.contains(path) { return current; }
    obj.get(key)
        .and_then(|v| v.as_f64())
        .map(|v| (v as f32).clamp(0.0, 1.0))
        .unwrap_or(current)
}

/// Set the active step count, tiling existing patterns into the new slots when expanding.
///
/// When going from 16 → 32 steps, steps 16–31 are filled by repeating the pattern from 0–15.
/// When going from 16 → 64, the 16-step pattern is repeated into all four banks.
/// Shrinking never erases data — the slots above the new count remain in memory (hidden).
/// Any LLM-provided pattern arrays applied *after* this call will overwrite the tiled values.
pub fn expand_sequencer_steps(state: AppState, new_steps: usize) -> AppState {
    let mut s = state;
    let old_steps = s.sequencer.steps;
    let new_steps = new_steps.clamp(1, MAX_STEPS);
    s.sequencer.steps = new_steps;

    if new_steps > old_steps && old_steps > 0 {
        // Tile bass pattern
        for i in old_steps..new_steps {
            s.sequencer.bass_pattern[i] = s.sequencer.bass_pattern[i % old_steps].clone();
        }
        // Tile every drum voice
        let voices: Vec<DrumVoice> = s.sequencer.drum_patterns.keys().cloned().collect();
        for voice in voices {
            if let Some(pattern) = s.sequencer.drum_patterns.get_mut(&voice) {
                for i in old_steps..new_steps {
                    pattern[i] = pattern[i % old_steps].clone();
                }
            }
        }
    }

    s
}

/// Toggle sequencer running state.
pub fn toggle_sequencer_running(state: AppState) -> AppState {
    let mut s = state;
    s.sequencer.running = !s.sequencer.running;
    s
}

/// Lock a single parameter so the LLM cannot change it.
pub fn lock_param(state: AppState, path: &str) -> AppState {
    let mut s = state;
    s.llm.locked_params.insert(path.to_string());
    s
}

/// Lock multiple parameters at once.
pub fn lock_params(state: AppState, paths: &[&str]) -> AppState {
    let mut s = state;
    for path in paths {
        s.llm.locked_params.insert(path.to_string());
    }
    s
}

/// Unlock a single parameter.
pub fn unlock_param(state: AppState, path: &str) -> AppState {
    let mut s = state;
    s.llm.locked_params.remove(path);
    s
}

/// Unlock multiple parameters at once.
pub fn unlock_params(state: AppState, paths: &[&str]) -> AppState {
    let mut s = state;
    for path in paths {
        s.llm.locked_params.remove(*path);
    }
    s
}

/// Toggle a drum step (pure function).
pub fn toggle_drum_step(state: AppState, voice: DrumVoice, step: usize) -> AppState {
    let mut s = state;
    if let Some(pattern) = s.sequencer.drum_patterns.get_mut(&voice) {
        if step < pattern.len() {
            pattern[step].active = !pattern[step].active;
            if pattern[step].active && pattern[step].velocity == 0.0 {
                pattern[step].velocity = 1.0;
            }
        }
    }
    s
}

/// Set a 303 step note.
pub fn set_bass_step(state: AppState, step: usize, note: u8, active: bool) -> AppState {
    let mut s = state;
    if step < s.sequencer.bass_pattern.len() {
        s.sequencer.bass_pattern[step].active = active;
        s.sequencer.bass_pattern[step].note = note;
    }
    s
}

// ─── Project save / load ──────────────────────────────────────────────────────

/// Serialise `state` to `project-<unix_seconds>.json` in the current directory.
/// Returns the path written on success.
pub fn save_project(state: &AppState) -> Result<std::path::PathBuf, String> {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let path = std::path::PathBuf::from(format!("project-{}.json", ts));
    let json = serde_json::to_string_pretty(state)
        .map_err(|e| format!("serialise error: {e}"))?;
    std::fs::write(&path, json)
        .map_err(|e| format!("write error: {e}"))?;
    Ok(path)
}

/// Load an `AppState` from a JSON project file.
pub fn load_project(path: &std::path::Path) -> Result<AppState, String> {
    let json = std::fs::read_to_string(path)
        .map_err(|e| format!("read error: {e}"))?;
    serde_json::from_str(&json)
        .map_err(|e| format!("parse error: {e}"))
}
