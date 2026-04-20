// ─── state/transitions_presets.rs ─────────────────────────────────────────────
// Voice preset state transitions — all take ownership of `AppState` and
// return a new one.  Extracted from `transitions.rs` to keep both files
// under the 1000-line hook limit.

use super::{AppState, FilterMode, Waveform};

/// Apply the Reese bass preset.
/// Detuned dual saws + sub oscillator + highpass to cut sub mud + light chorus.
/// LLM trigger: "Reese bass", "detuned bass", "jungle bass".
pub fn apply_reese_preset(state: AppState) -> AppState {
    let mut s = state;
    s.bass_voices[0].synth.waveform = Waveform::Supersaw;
    s.bass_voices[0].synth.supersaw_voices = 2;
    s.bass_voices[0].synth.supersaw_detune = 0.3; // tight detuning — beating without flange
    s.bass_voices[0].synth.sub_osc_level = 0.5;
    s.bass_voices[0].synth.filter_mode = FilterMode::Highpass;
    s.bass_voices[0].synth.cutoff = 0.25; // HP removes low mud, keeps mid growl
    s.bass_voices[0].synth.resonance = 0.35;
    s.bass_voices[0].synth.env_mod = 0.0;
    s.bass_voices[0].synth.distortion = 0.15;
    s.bass_voices[0].synth.fm_depth = 0.0;
    s
}

/// Gabber kick preset: extreme pitch envelope + hard clip for distorted swooping kick.
pub fn apply_gabber_kick_preset(state: AppState) -> AppState {
    let mut s = state;
    s.kit_a.kick.pitch = 0.35; // lower base pitch
    s.kit_a.kick.decay = 0.7; // long tail
    s.kit_a.kick.punch = 0.9; // maximum transient
    s.kit_a.kick.pitch_env_depth = 0.9; // extreme pitch sweep
    s.kit_a.kick.pitch_env_time = 0.6; // long sweep time — the gabber "swooop"
    s.kit_a.kick.clip = 0.8; // heavy hard clipping
    s.kit_a.kick.volume = 0.85;
    s
}

/// Apply the Hoover lead preset — sets canonical Hoover parameters.
/// LLM trigger: "add a hoover", "rave lead", "dominator".
pub fn apply_hoover_preset(state: AppState) -> AppState {
    let mut s = state;
    s.hoover.enabled = true;
    s.hoover.filter_start = 0.82;
    s.hoover.sweep_time = 0.55;
    s.hoover.resonance = 0.76;
    s.hoover.detune = 0.42;
    s.hoover.voices = 5;
    s.hoover.pitch_lfo_rate = 1.3;
    s.hoover.pitch_lfo_depth = 0.18;
    s.hoover.volume = 0.72;
    s
}

/// Apply the BoC-style AN1X preset — warm detuned pad with slow attack and LFO drift.
/// LLM trigger: "add a pad", "warm lead", "BoC", "ambient".
pub fn apply_boc_preset(state: AppState) -> AppState {
    let mut s = state;
    s.an1x.enabled = true;
    s.an1x.osc1_wave = crate::state::An1xWave::Saw;
    s.an1x.osc2_wave = crate::state::An1xWave::Saw;
    s.an1x.osc1_level = 0.8;
    s.an1x.osc2_level = 0.65;
    s.an1x.osc2_detune = 0.53; // ~+2.9 semitones — classic detuned beating
    s.an1x.osc2_octave = 0;
    s.an1x.sub_level = 0.15;
    s.an1x.filter_cutoff = 0.42;
    s.an1x.filter_resonance = 0.28;
    s.an1x.filter_env_amount = 0.58; // gentle positive mod
    s.an1x.filter_attack = 0.15;
    s.an1x.filter_decay = 0.5;
    s.an1x.filter_sustain = 0.35;
    s.an1x.filter_release = 0.4;
    s.an1x.amp_attack = 0.32; // slow pad attack
    s.an1x.amp_decay = 0.55;
    s.an1x.amp_sustain = 0.65;
    s.an1x.amp_release = 0.5;
    s.an1x.lfo_rate = 0.09; // ~0.12 Hz — barely perceptible breathing
    s.an1x.lfo_depth = 0.12;
    s.an1x.lfo_target = crate::state::An1xLfoTarget::Pitch;
    s.an1x.lfo_delay = 0.4; // LFO fades in over ~1.6s after note is struck
    s.an1x.pitch_env_attack = 0.0;
    s.an1x.pitch_env_decay = 0.1;
    s.an1x.pitch_env_amount = 0.5; // neutral — no pitch transient on pads
    s.an1x.hard_sync = false;
    s.an1x.lfo_bpm_sync = false;
    s.an1x.lfo_sync_beats = 4.0;
    s.an1x.drift = 0.14;
    s.an1x.glide_time = 0.18;
    s.an1x.glide_legato = true;
    s.an1x.volume = 0.75;
    s
}

/// Warm Pad preset — lush, slow-moving, classic analog pad sound.
pub fn apply_warm_pad_preset(state: AppState) -> AppState {
    let mut s = state;
    s.an1x.enabled = true;
    s.an1x.osc1_wave = crate::state::An1xWave::Saw;
    s.an1x.osc2_wave = crate::state::An1xWave::Saw;
    s.an1x.osc1_level = 0.75;
    s.an1x.osc2_level = 0.7;
    s.an1x.osc2_detune = 0.52;
    s.an1x.sub_level = 0.3;
    s.an1x.filter_cutoff = 0.35;
    s.an1x.filter_resonance = 0.15;
    s.an1x.filter_env_amount = 0.55;
    s.an1x.filter_attack = 0.4;
    s.an1x.filter_decay = 0.6;
    s.an1x.filter_sustain = 0.4;
    s.an1x.filter_release = 0.55;
    s.an1x.amp_attack = 0.45;
    s.an1x.amp_decay = 0.5;
    s.an1x.amp_sustain = 0.7;
    s.an1x.amp_release = 0.6;
    s.an1x.drift = 0.08;
    s.an1x.volume = 0.7;
    s
}

/// Evolving Texture preset — slowly morphing sound with LFO on filter.
pub fn apply_evolving_texture_preset(state: AppState) -> AppState {
    let mut s = state;
    s.an1x.enabled = true;
    s.an1x.osc1_wave = crate::state::An1xWave::Saw;
    s.an1x.osc2_wave = crate::state::An1xWave::Triangle;
    s.an1x.osc1_level = 0.6;
    s.an1x.osc2_level = 0.8;
    s.an1x.osc2_detune = 0.54;
    s.an1x.filter_cutoff = 0.5;
    s.an1x.filter_resonance = 0.35;
    s.an1x.filter_env_amount = 0.6;
    s.an1x.filter_attack = 0.3;
    s.an1x.filter_decay = 0.7;
    s.an1x.filter_sustain = 0.3;
    s.an1x.filter_release = 0.7;
    s.an1x.amp_attack = 0.5;
    s.an1x.amp_decay = 0.6;
    s.an1x.amp_sustain = 0.55;
    s.an1x.amp_release = 0.75;
    s.an1x.lfo_rate = 0.06;
    s.an1x.lfo_depth = 0.25;
    s.an1x.lfo_target = crate::state::An1xLfoTarget::FilterCutoff;
    s.an1x.lfo_delay = 0.5;
    s.an1x.drift = 0.2;
    s.an1x.volume = 0.65;
    s
}

/// Glass Pad preset — bright, crystalline, shimmering pad.
pub fn apply_glass_pad_preset(state: AppState) -> AppState {
    let mut s = state;
    s.an1x.enabled = true;
    s.an1x.osc1_wave = crate::state::An1xWave::Triangle;
    s.an1x.osc2_wave = crate::state::An1xWave::Sine;
    s.an1x.osc1_level = 0.9;
    s.an1x.osc2_level = 0.5;
    s.an1x.osc2_detune = 0.53;
    s.an1x.osc2_octave = 1;
    s.an1x.filter_cutoff = 0.7;
    s.an1x.filter_resonance = 0.4;
    s.an1x.filter_env_amount = 0.62;
    s.an1x.filter_attack = 0.2;
    s.an1x.filter_decay = 0.5;
    s.an1x.filter_sustain = 0.5;
    s.an1x.filter_release = 0.65;
    s.an1x.amp_attack = 0.35;
    s.an1x.amp_decay = 0.45;
    s.an1x.amp_sustain = 0.6;
    s.an1x.amp_release = 0.8;
    s.an1x.hard_sync = true;
    s.an1x.drift = 0.05;
    s.an1x.volume = 0.6;
    s
}

/// Sub Drone preset — deep, sustained, barely audible movement.
pub fn apply_sub_drone_preset(state: AppState) -> AppState {
    let mut s = state;
    s.an1x.enabled = true;
    s.an1x.osc1_wave = crate::state::An1xWave::Sine;
    s.an1x.osc2_wave = crate::state::An1xWave::Triangle;
    s.an1x.osc1_level = 0.9;
    s.an1x.osc2_level = 0.4;
    s.an1x.osc2_detune = 0.505;
    s.an1x.osc2_octave = -1;
    s.an1x.sub_level = 0.6;
    s.an1x.filter_cutoff = 0.2;
    s.an1x.filter_resonance = 0.1;
    s.an1x.filter_env_amount = 0.5;
    s.an1x.filter_attack = 0.6;
    s.an1x.filter_decay = 0.8;
    s.an1x.filter_sustain = 0.6;
    s.an1x.filter_release = 0.9;
    s.an1x.amp_attack = 0.7;
    s.an1x.amp_decay = 0.7;
    s.an1x.amp_sustain = 0.8;
    s.an1x.amp_release = 0.95;
    s.an1x.lfo_rate = 0.03;
    s.an1x.lfo_depth = 0.08;
    s.an1x.lfo_target = crate::state::An1xLfoTarget::Pitch;
    s.an1x.drift = 0.25;
    s.an1x.volume = 0.7;
    s
}
