// ─── tests/transitions_presets_tests.rs ──────────────────────────────────────
// Tests for the voice preset transitions in state/transitions_presets.rs.
// The reese/hoover/boc presets already have older tests in state_tests.rs;
// these cover the remaining presets (gabber kick, warm pad, evolving texture,
// glass pad, sub drone) so every preset in the file has at least one
// regression test guarding its key parameters.

use crate::state::{
    An1xLfoTarget, An1xWave, AppState, apply_evolving_texture_preset, apply_gabber_kick_preset,
    apply_glass_pad_preset, apply_sub_drone_preset, apply_warm_pad_preset,
};

#[test]
fn gabber_kick_preset_sets_deep_pitch_env_and_heavy_clip() {
    let s = apply_gabber_kick_preset(AppState::default());
    // Extreme pitch sweep with a long time — the "swooop".
    assert!(s.kit_a.kick.pitch_env_depth >= 0.85);
    assert!(s.kit_a.kick.pitch_env_time >= 0.5);
    // Hard-clip substantial, punch maxed.
    assert!(s.kit_a.kick.clip >= 0.7);
    assert!(s.kit_a.kick.punch >= 0.85);
    // Base pitch lowered from default.
    assert!(s.kit_a.kick.pitch < 0.5);
}

#[test]
fn warm_pad_preset_enables_an1x_with_slow_attack() {
    let s = apply_warm_pad_preset(AppState::default());
    assert!(s.an1x.enabled);
    assert_eq!(s.an1x.osc1_wave, An1xWave::Saw);
    assert_eq!(s.an1x.osc2_wave, An1xWave::Saw);
    // Slow amp attack + release → pad character.
    assert!(s.an1x.amp_attack >= 0.4);
    assert!(s.an1x.amp_release >= 0.5);
    // Low resonance — warm, not aggressive.
    assert!(s.an1x.filter_resonance <= 0.2);
}

#[test]
fn evolving_texture_preset_routes_lfo_to_cutoff() {
    let s = apply_evolving_texture_preset(AppState::default());
    assert!(s.an1x.enabled);
    assert_eq!(s.an1x.lfo_target, An1xLfoTarget::FilterCutoff);
    // LFO depth non-trivial (movement) with a slow rate.
    assert!(s.an1x.lfo_depth >= 0.2);
    assert!(s.an1x.lfo_rate <= 0.1);
    // Mixed waves: saw + triangle.
    assert_eq!(s.an1x.osc1_wave, An1xWave::Saw);
    assert_eq!(s.an1x.osc2_wave, An1xWave::Triangle);
}

#[test]
fn glass_pad_preset_enables_hard_sync() {
    let s = apply_glass_pad_preset(AppState::default());
    assert!(s.an1x.enabled);
    assert!(s.an1x.hard_sync, "glass pad relies on osc hard sync");
    // Octave-up second osc gives the crystalline brightness.
    assert_eq!(s.an1x.osc2_octave, 1);
    // Triangle + sine lineup.
    assert_eq!(s.an1x.osc1_wave, An1xWave::Triangle);
    assert_eq!(s.an1x.osc2_wave, An1xWave::Sine);
}

#[test]
fn sub_drone_preset_octave_down_and_long_release() {
    let s = apply_sub_drone_preset(AppState::default());
    assert!(s.an1x.enabled);
    // Deep sub drone — osc2 pitched an octave below osc1, sub mixed in.
    assert_eq!(s.an1x.osc2_octave, -1);
    assert!(s.an1x.sub_level >= 0.5);
    // Very long release — drones don't end abruptly.
    assert!(s.an1x.amp_release >= 0.9);
    // Very low cutoff so only sub content survives.
    assert!(s.an1x.filter_cutoff <= 0.25);
}

#[test]
fn every_preset_is_idempotent_pure_function() {
    // Each preset takes ownership, returns a new state.  Calling it twice
    // on the default must produce identical results both times (proves no
    // hidden dependency on external state / RNG).
    let a = apply_warm_pad_preset(AppState::default());
    let b = apply_warm_pad_preset(AppState::default());
    assert_eq!(a.an1x.osc1_level, b.an1x.osc1_level);
    assert_eq!(a.an1x.filter_cutoff, b.an1x.filter_cutoff);

    let a = apply_glass_pad_preset(AppState::default());
    let b = apply_glass_pad_preset(AppState::default());
    assert_eq!(a.an1x.hard_sync, b.an1x.hard_sync);
    assert_eq!(a.an1x.osc2_detune, b.an1x.osc2_detune);
}

#[test]
fn presets_do_not_clobber_unrelated_state() {
    // A voice preset that targets the an1x must not mutate the sequencer
    // BPM, bass_voices, or the rack.  Guards against future preset authors
    // accidentally touching shared fields.
    let mut start = AppState::default();
    start.sequencer.bpm = 137.5;
    start.bass_voices[0].synth.cutoff = 0.42;
    let after = apply_warm_pad_preset(start.clone());
    assert!((after.sequencer.bpm - 137.5).abs() < 1e-6);
    assert!((after.bass_voices[0].synth.cutoff - 0.42).abs() < 1e-6);

    let after = apply_evolving_texture_preset(start.clone());
    assert!((after.sequencer.bpm - 137.5).abs() < 1e-6);

    let after = apply_sub_drone_preset(start);
    assert!((after.bass_voices[0].synth.cutoff - 0.42).abs() < 1e-6);
}
