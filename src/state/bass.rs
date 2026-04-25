// ─── state/bass.rs ────────────────────────────────────────────────────────────
// Bass (303-style) synth state.  Extracted from state/mod.rs to keep that
// file under the 1000-line limit.
//
// BassState is the full synth engine parameter set — 303 squelch territory
// by default, but with ADSR and PWM fields that also cover 101-style stab
// or pad shaping.  BassVoiceState wraps it with per-voice musical context
// (root note, scale, enabled flag).

use serde::{Deserialize, Serialize};

use super::MAX_BASS_VOICES;
use super::lfo::LfoWaveform;
use super::music::Scale;
use super::synth_types::{FilterMode, Waveform};

/// LFO modulation target for the per-voice bass LFO.
/// Mirrors the SH-101 LFO routing: pitch vibrato, PWM on the pulse,
/// filter cutoff (wah), or amplitude (tremolo).  Off disables the LFO
/// entirely regardless of depth.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum BassLfoTarget {
    #[default]
    Off,
    Pitch,
    PulseWidth,
    FilterCutoff,
    Amplitude,
    /// Per-voice stereo pan modulation.  Routes the LFO's depth+fade-
    /// shaped value into a per-voice side-bus contribution that the
    /// master mixer sums alongside the existing pan_303 path.  Combined
    /// with the per-voice `lfo_phase` offset this lets two bass voices
    /// run anti-phase pan sweeps (e.g. voice 0 phase 0, voice 1 phase
    /// 0.5) without any host-side automation traffic.
    Pan,
}

impl BassLfoTarget {
    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "OFF",
            Self::Pitch => "PITCH",
            Self::PulseWidth => "PWM",
            Self::FilterCutoff => "CUTOFF",
            Self::Amplitude => "AMP",
            Self::Pan => "PAN",
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::Off => Self::Pitch,
            Self::Pitch => Self::PulseWidth,
            Self::PulseWidth => Self::FilterCutoff,
            Self::FilterCutoff => Self::Amplitude,
            Self::Amplitude => Self::Pan,
            Self::Pan => Self::Off,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BassState {
    pub cutoff: f32,       // 0–1 → 200–8000 Hz
    pub resonance: f32,    // 0–1
    pub env_mod: f32,      // 0–1 filter env depth
    pub decay: f32,        // 0–1 → 50–2000 ms (legacy single-knob filter env)
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
    #[serde(default)]
    pub pan: f32, // -1.0 (left) to +1.0 (right), 0.0 = center

    // ── Amp envelope (ADSR) — 101-style shaping ─────────────────────────────
    // Defaults map to the legacy 303 behavior: instant attack, no decay to
    // sustain (sustain stays at 1.0 while gate held), brief release on
    // gate-off.  Non-default values give 101-style pad or stab envelopes.
    #[serde(default)]
    pub amp_attack: f32, // 0–1 → 0–1s
    #[serde(default = "default_sustain_full")]
    pub amp_sustain: f32, // 0–1 (1.0 = hold at full while gate on)
    #[serde(default = "default_release_short")]
    pub amp_release: f32, // 0–1 → 0–2s

    // ── Filter envelope (ADSR) ──────────────────────────────────────────────
    // `decay` (above) remains the legacy filter-env decay time.  Defaults
    // here map to 303 behavior: no attack, no sustain (filter env fully
    // decays to 0 using `decay`), brief release on gate-off.
    #[serde(default)]
    pub filter_attack: f32, // 0–1 → 0–0.5s
    #[serde(default)]
    pub filter_sustain: f32, // 0–1
    #[serde(default = "default_release_short")]
    pub filter_release: f32, // 0–1 → 0–2s

    // ── Pulse-width for square oscillator (PWM) ─────────────────────────────
    // Ignored for Saw / Supersaw.  0.05..0.95, centered at 0.5 = classic
    // square.  Going narrow gives the "reedy" 101 sound.
    #[serde(default = "default_pulse_width")]
    pub pulse_width: f32,

    // ── Per-voice LFO — SH-101-style routable modulator ─────────────────────
    // Target defaults to Off so existing sessions aren't affected.  Set a
    // target + non-zero depth to get pitch vibrato, PWM oscillation,
    // filter wah, or tremolo without going near the global LFO matrix.
    #[serde(default)]
    pub lfo_target: BassLfoTarget,
    #[serde(default)]
    pub lfo_rate: f32, // 0–1 → 0.01–20 Hz (free); ignored when bpm_sync
    #[serde(default)]
    pub lfo_depth: f32, // 0–1; per-target scaling applied in DSP
    #[serde(default)]
    pub lfo_waveform: LfoWaveform,
    #[serde(default)]
    pub lfo_delay: f32, // 0–1 → 0–4 s fade-in before LFO reaches full depth
    #[serde(default)]
    pub lfo_bpm_sync: bool,
    #[serde(default = "default_lfo_sync_beats")]
    pub lfo_sync_beats: f32, // division in beats (1=quarter, 0.5=8th, 0.25=16th, 4=bar)
    /// Per-voice LFO phase offset (0..1 → 0..2π).  Added to the
    /// running LFO phase before the waveform lookup, so two voices
    /// sharing the same rate can be set anti-phase (offset 0.5),
    /// 90° apart (0.25), etc. — particularly useful with the Pan
    /// target where two voices produce stereo motion that sums to
    /// mono-safe energy at the L/R extremes.
    #[serde(default)]
    pub lfo_phase: f32,
}

fn default_sustain_full() -> f32 {
    1.0
}
fn default_release_short() -> f32 {
    0.05
}
fn default_pulse_width() -> f32 {
    0.5
}
fn default_lfo_sync_beats() -> f32 {
    1.0 // quarter note
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
            pan: 0.0,
            amp_attack: 0.0,
            amp_sustain: default_sustain_full(),
            amp_release: default_release_short(),
            filter_attack: 0.0,
            filter_sustain: 0.0,
            filter_release: default_release_short(),
            pulse_width: default_pulse_width(),
            lfo_target: BassLfoTarget::Off,
            lfo_rate: 0.25, // ~0.5 Hz when off — harmless default
            lfo_depth: 0.0,
            lfo_waveform: LfoWaveform::Sine,
            lfo_delay: 0.0,
            lfo_bpm_sync: false,
            lfo_sync_beats: default_lfo_sync_beats(),
            lfo_phase: 0.0,
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

pub(super) fn default_bass_voices() -> Vec<BassVoiceState> {
    let mut voices: Vec<BassVoiceState> = (0..MAX_BASS_VOICES)
        .map(|_| BassVoiceState::default())
        .collect();
    // Voice 0 is always enabled
    voices[0].enabled = true;
    voices
}
