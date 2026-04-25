// ─── state/sample_instrument.rs ──────────────────────────────────────────────
// Sample-Instrument voice — load a single pitched .wav and play it back
// across the keyboard via ratio resampling.  Distinct from `WavetableVoice`
// (which scans single-cycle frames) and `AmenSampler` (which plays slices
// at the original pitch): this module re-pitches the entire recording
// based on the played note vs. the root note.
//
// V1 is intentionally minimal — single sample, monophonic, simple AR
// envelope (no full ADSR yet), no loop points (always-loops the whole
// buffer), no on-load pitch detection.  The plan in PLAN.md tracks the
// V2 enhancements.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SampleInstrumentState {
    /// Gate — enable the voice in the mix.
    pub enabled: bool,
    /// Source-recording root note (MIDI).  Played notes are pitch-shifted
    /// relative to this: rate = 2^((played_note − root_note) / 12).
    /// Defaults to C4 (60); the user re-tunes via the UI.
    pub root_note: u8,
    /// Output volume (0..1.5; >1 boosts).
    pub volume: f32,
    /// Stereo pan (-1..+1).
    #[serde(default)]
    pub pan: f32,
    /// Fine pitch trim in cents (-100..+100), applied on top of the
    /// per-note pitch.
    #[serde(default)]
    pub pitch_offset_cents: f32,
    /// Filesystem path of the currently loaded sample.  Empty = none
    /// loaded; voice plays silence.  UI polls this so API-driven loads
    /// surface to the panel.
    #[serde(default)]
    pub sample_path: String,
    /// Attack time 0..1 → 0.5..1500 ms.
    #[serde(default = "default_attack")]
    pub attack: f32,
    /// Decay time 0..1 → 5..2000 ms.
    #[serde(default = "default_decay")]
    pub decay: f32,
    /// Sustain level 0..1 (0 = full decay, 1 = no decay).
    #[serde(default = "default_sustain")]
    pub sustain: f32,
    /// Release time 0..1 → 5..2000 ms.
    #[serde(default = "default_release")]
    pub release: f32,
    /// Loop start position (0..1 of buffer).
    #[serde(default)]
    pub loop_start: f32,
    /// Loop end position (0..1 of buffer; if ≤ loop_start, loop is disabled).
    #[serde(default = "default_loop_end")]
    pub loop_end: f32,
    /// True = play loops between loop_start and loop_end while gate held.
    /// False = one-shot; voice plays from loop_start once and falls silent.
    #[serde(default = "default_loop_enabled")]
    pub loop_enabled: bool,
    /// Per-voice filter cutoff 0..1 (log-mapped 20 Hz..18 kHz inside
    /// the SVF DSP).  Drives every active polyphony slot through the
    /// same shared cutoff + resonance + mode setting.  V2 Stage 6.
    #[serde(default = "default_filter_cutoff")]
    pub filter_cutoff: f32,
    /// Per-voice filter resonance 0..1 → Q ≈ 0.5..20.
    #[serde(default)]
    pub filter_resonance: f32,
    /// Filter mode: 0 = LP, 1 = BP, 2 = HP.  Stored as u8 to match the
    /// existing FxFilter (`svf_mode`) so the LLM can write a number.
    #[serde(default)]
    pub filter_mode: u8,
    /// Filter wet/dry 0..1.  At 0 the filter is bypassed entirely
    /// (no per-slot SVF state runs).  Default 0 so adding the module
    /// to a rack doesn't suddenly lop off the high end of every
    /// loaded sample.
    #[serde(default)]
    pub filter_mix: f32,
}

fn default_attack() -> f32 {
    0.0 // 0.5 ms — matches the V1 AR envelope
}
fn default_decay() -> f32 {
    0.0 // 5 ms — no audible decay by default
}
fn default_sustain() -> f32 {
    1.0 // sustain at full
}
fn default_release() -> f32 {
    0.1 // 100 ms — matches the V1 AR release tail
}
fn default_loop_end() -> f32 {
    1.0 // play to end of buffer
}
fn default_loop_enabled() -> bool {
    true // V1 always-loops; V1.1 default keeps that behaviour
}
fn default_filter_cutoff() -> f32 {
    1.0 // fully open — no audible filter when the user just enables the mix
}

impl Default for SampleInstrumentState {
    fn default() -> Self {
        Self {
            enabled: false,
            root_note: 60, // C4
            volume: 0.7,
            pan: 0.0,
            pitch_offset_cents: 0.0,
            sample_path: String::new(),
            attack: default_attack(),
            decay: default_decay(),
            sustain: default_sustain(),
            release: default_release(),
            loop_start: 0.0,
            loop_end: default_loop_end(),
            loop_enabled: default_loop_enabled(),
            filter_cutoff: default_filter_cutoff(),
            filter_resonance: 0.0,
            filter_mode: 0,
            filter_mix: 0.0,
        }
    }
}
