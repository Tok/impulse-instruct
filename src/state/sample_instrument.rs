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
    /// Formant-preserving pitch shift opt-in.  V2 Stage 8 lands the
    /// API surface (state field + LLM apply + UI toggle); the actual
    /// PSOLA / phase-vocoder DSP is deferred to a follow-up — until
    /// then this flag is a no-op (the cheap linear-resample path
    /// runs regardless).  Persisting it now means existing sessions
    /// pick up the real implementation transparently when it ships.
    #[serde(default)]
    pub formant_preserve: bool,
    /// Time-stretch ratio decoupled from pitch.  1.0 = play the
    /// source at its native tempo; 0.5 = half speed (twice as long);
    /// 2.0 = double speed (half as long).  Pitch stays at the
    /// played note regardless — implemented by reading the source
    /// at `time_stretch` and compensating with a phase-vocoder
    /// pitch shift of `pitch_ratio / time_stretch` so the output
    /// pitch lands on `pitch_ratio` (the played note).  Requires the
    /// formant-preserve path's spectral processor; the cheap
    /// linear-resample path ignores this knob (its read rate is
    /// the pitch ratio).  Default 1.0 = no stretch.
    #[serde(default = "default_time_stretch")]
    pub time_stretch: f32,
    /// Mellotron mode opt-in (absurd-queue voice #3).  When on, the
    /// slot's playback gains the character of a real Mellotron's
    /// tape-loop bank: a slow per-slot pitch wobble (1–3 Hz LFO + a
    /// touch of noise) for the tape-flutter sound, a brief
    /// spin-up transient on attack so each new note starts
    /// slightly flat and rises to pitch (the motor coming up to
    /// speed), and a mild tanh saturation to approximate tape
    /// compression.  Cheap path stays the V1.1 linear resample —
    /// the flutter / spin-up modulate the read rate directly
    /// without going through the spectral processor.  Default
    /// off so loading a session never silently changes tone.
    #[serde(default)]
    pub mellotron_mode: bool,
    /// Mellotron flutter depth 0..1.  Scales the LFO + noise pitch
    /// modulation when `mellotron_mode` is on.  At 0 the wobble is
    /// inaudible (still on but ±0 cents); at 1 it's a deep
    /// vintage-tape warble.  No effect when `mellotron_mode` is
    /// off.  0..1 → ±0..40 cents at the LFO peak.
    #[serde(default = "default_mellotron_flutter")]
    pub mellotron_flutter: f32,
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
fn default_time_stretch() -> f32 {
    1.0 // play at source's native tempo — no stretch
}
fn default_mellotron_flutter() -> f32 {
    0.4 // pleasant default warble — audible without being a parody
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
            formant_preserve: false,
            time_stretch: default_time_stretch(),
            mellotron_mode: false,
            mellotron_flutter: default_mellotron_flutter(),
        }
    }
}
