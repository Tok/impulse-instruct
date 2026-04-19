// ─── tests/fx_math_tests.rs ──────────────────────────────────────────────────
// Covers the pure DSP helpers in `audio/dsp/fx_math.rs`.  Each function is
// stateless (or threads its state through explicitly), so they can be
// tested against closed-form expectations without spinning up a full
// DspState.  Tests pin the dry-bypass fast paths, the shape of the
// saturation curves, the sidechain envelope timing, LFO waveform output,
// and the 8-step free-EG interpolation.

use crate::audio::dsp::fx_math::{
    BitcrushState, bitcrush_step, drive_step, free_eg_value_at, gated_reverb_envelope_step,
    lfo_value_at, sidechain_duck, sidechain_envelope_step, waveshaper_step,
};
use crate::state::LfoWaveform;

// ─── Waveshaper / drive ──────────────────────────────────────────────────────

#[test]
fn waveshaper_mix_zero_passes_dry_signal_through() {
    // mix ≤ 0.001 is the "bypass" fast path — output must equal input
    // byte-for-byte.  Any drift here means the bypass is accidentally
    // running the tanh pipeline.
    assert_eq!(waveshaper_step(0.37, 1.0, 0.0), 0.37);
    assert_eq!(waveshaper_step(-0.8, 1.0, 0.0005), -0.8);
}

#[test]
fn waveshaper_mix_one_fully_saturates() {
    // At mix=1, the output is the shaped signal alone.  The shape is
    // tanh(sig*drive)/tanh(drive) which is bounded in [-1, 1] even
    // before the dry blend — check both that it's bounded AND that
    // higher drive drives harder (lower absolute gain reduction).
    let gentle = waveshaper_step(0.5, 0.0, 1.0);
    let heavy = waveshaper_step(0.5, 1.0, 1.0);
    assert!(gentle.abs() <= 1.0);
    assert!(heavy.abs() <= 1.0);
    // Heavier drive pushes 0.5 closer toward its asymptote of ±1.
    assert!(heavy > gentle, "heavy drive should saturate harder");
}

#[test]
fn drive_step_zero_norm_bypasses() {
    // drive_norm ≤ 0.01 is the "knob off" bypass.
    assert_eq!(drive_step(0.4, 0.0, 1.0), 0.4);
    assert_eq!(drive_step(-0.9, 0.005, 1.0), -0.9);
}

#[test]
fn drive_step_saturates_harder_than_dry() {
    // At max drive+mix the output should be strongly saturated.  The
    // shared fast-tanh approximation can overshoot ±1 by ~1% near the
    // knee — asserting "pushes harder than dry" is the invariant that
    // matters; the hard ±1 ceiling belongs to a real tanh.
    let out = drive_step(0.5, 1.0, 1.0);
    assert!(out.abs() > 0.5, "drive at max should push 0.5 harder");
    assert!(out.abs() < 1.05, "fast-tanh approximation stays near unity");
}

// ─── Bitcrush ───────────────────────────────────────────────────────────────

#[test]
fn bitcrush_mix_zero_bypasses_and_preserves_state() {
    let state = BitcrushState {
        held: 0.99,
        counter: 7,
    };
    let (after, out) = bitcrush_step(state, -0.42, 1.0, 1.0, 0.0);
    assert_eq!(out, -0.42, "bypass must return dry signal untouched");
    assert_eq!(after.held, 0.99, "bypass must not touch held value");
    assert_eq!(after.counter, 7, "bypass must not tick the counter");
}

#[test]
fn bitcrush_latches_at_counter_zero_and_then_holds() {
    // counter=0 triggers a new quantize+hold.  Feed a fixed input at full
    // mix: the first call re-quantizes `sig` to `bits` and writes held;
    // subsequent calls (counter > 0) decrement but return the held value.
    let state = BitcrushState::default(); // counter=0 → will latch
    let (after_first, _) = bitcrush_step(state, 0.5, 1.0, 0.5, 1.0);
    assert!(after_first.counter > 0, "latch should arm the counter");
    let latched = after_first.held;
    // A later call with a DIFFERENT input while counter>0 should reuse
    // the latched value (the whole point of sample-rate reduction).
    let (after_hold, out_hold) = bitcrush_step(after_first, 0.99, 1.0, 0.5, 1.0);
    assert_eq!(
        after_hold.held, latched,
        "held value should persist through the hold window"
    );
    assert_eq!(
        out_hold, latched,
        "hold-phase output is the latched value at mix=1"
    );
    assert_eq!(after_hold.counter, after_first.counter - 1);
}

#[test]
fn bitcrush_quantize_reduces_precision() {
    // bits=0 → 1-bit quantizer: output is effectively ±1 / scale = ±1 / 1.
    // Any non-zero input should snap to ±1 (or 0) on the latch.
    let (after, _) = bitcrush_step(BitcrushState::default(), 0.3, 0.0, 0.0, 1.0);
    // With bits_norm=0, bits=1, scale=1 → held = round(0.3*1)/1 = 0.
    assert_eq!(after.held, 0.0);
    let (after2, _) = bitcrush_step(BitcrushState::default(), 0.8, 0.0, 0.0, 1.0);
    // round(0.8) = 1.
    assert_eq!(after2.held, 1.0);
}

// ─── Sidechain envelope + duck ──────────────────────────────────────────────

#[test]
fn sidechain_envelope_tracks_rising_input_through_attack() {
    // Input rises from 0 → 1 over a single sample with attack=1ms at
    // sr=48k.  The envelope should move toward 1 but not snap there
    // instantly (the attack coefficient is exp(-1/(0.001*48k))).
    let env = sidechain_envelope_step(0.0, 1.0, 1.0, 100.0, 48_000.0);
    assert!(env > 0.0, "env should rise toward the target");
    assert!(env < 1.0, "attack must not be instantaneous");
    // Same input with longer attack → slower rise (smaller env).
    let slower = sidechain_envelope_step(0.0, 1.0, 50.0, 100.0, 48_000.0);
    assert!(slower < env, "longer attack should lag more");
}

#[test]
fn sidechain_envelope_release_decays_toward_zero() {
    // Target below previous env → release path.  Output scales prev by
    // the release coefficient, which is in (0,1).  Zero attack time
    // doesn't matter — we're on the release branch.
    let env = sidechain_envelope_step(1.0, 0.0, 1.0, 100.0, 48_000.0);
    assert!(env > 0.0 && env < 1.0);
}

#[test]
fn sidechain_duck_at_zero_env_is_unity() {
    // No detection bus activity → no duck.
    assert_eq!(sidechain_duck(0.0, 1.0), 1.0);
    assert_eq!(sidechain_duck(0.0, 0.5), 1.0);
}

#[test]
fn sidechain_duck_saturates_at_full_suppression() {
    // env * amount * 4 ≥ 1 → duck clamps to 0 (signal fully suppressed).
    // env=0.5, amount=1.0 → 0.5*4 = 2.0 → clamp(1) → 1 - 1 = 0.
    assert_eq!(sidechain_duck(0.5, 1.0), 0.0);
    // Just above threshold.
    assert_eq!(sidechain_duck(0.3, 1.0), 1.0 - 1.2_f32.min(1.0));
}

// ─── Gated reverb envelope ──────────────────────────────────────────────────

#[test]
fn gated_reverb_disabled_when_time_is_zero() {
    // gate_time_s ≤ 0.001 → always-open (returns 1.0 unconditionally).
    assert_eq!(gated_reverb_envelope_step(0.0, 0.0, 0.0, 48_000.0), 1.0);
    assert_eq!(gated_reverb_envelope_step(0.5, 0.99, 0.0005, 48_000.0), 1.0);
}

#[test]
fn gated_reverb_opens_on_detection_and_decays_without_it() {
    // detect_abs > 0.08 → env snaps to 1.0.
    assert_eq!(gated_reverb_envelope_step(0.0, 0.5, 0.5, 48_000.0), 1.0);
    // Below threshold → decay from prev by exp(-1/(time*sr)).
    let decayed = gated_reverb_envelope_step(1.0, 0.01, 0.5, 48_000.0);
    assert!(decayed > 0.0 && decayed < 1.0);
}

// ─── LFO lookup ─────────────────────────────────────────────────────────────

#[test]
fn lfo_sine_hits_zero_and_peaks_at_quarter_phase() {
    // phase 0 → sin(0) = 0; phase 0.25 → sin(τ/4) = 1; phase 0.75 → -1.
    assert!(lfo_value_at(0.0, LfoWaveform::Sine, 0.0).abs() < 1e-4);
    assert!((lfo_value_at(0.25, LfoWaveform::Sine, 0.0) - 1.0).abs() < 1e-4);
    assert!((lfo_value_at(0.75, LfoWaveform::Sine, 0.0) + 1.0).abs() < 1e-4);
}

#[test]
fn lfo_triangle_peaks_at_half_and_bottoms_at_endpoints() {
    // phase=0: 1 - 4*0.5 = -1; phase=0.5: 1 - 0 = 1; phase=1 mirrors 0.
    assert!((lfo_value_at(0.0, LfoWaveform::Triangle, 0.0) + 1.0).abs() < 1e-4);
    assert!((lfo_value_at(0.5, LfoWaveform::Triangle, 0.0) - 1.0).abs() < 1e-4);
}

#[test]
fn lfo_saws_are_monotonic() {
    // Saw ramps from -1 to +1; InvSaw ramps from +1 to -1.
    let up_low = lfo_value_at(0.0, LfoWaveform::Saw, 0.0);
    let up_high = lfo_value_at(1.0, LfoWaveform::Saw, 0.0);
    assert!(up_low < up_high);
    assert!((up_low + 1.0).abs() < 1e-4);
    assert!((up_high - 1.0).abs() < 1e-4);

    let down_low = lfo_value_at(0.0, LfoWaveform::InvSaw, 0.0);
    let down_high = lfo_value_at(1.0, LfoWaveform::InvSaw, 0.0);
    assert!(down_low > down_high);
}

#[test]
fn lfo_square_steps_at_half() {
    assert_eq!(lfo_value_at(0.0, LfoWaveform::Square, 0.0), 1.0);
    assert_eq!(lfo_value_at(0.49, LfoWaveform::Square, 0.0), 1.0);
    assert_eq!(lfo_value_at(0.51, LfoWaveform::Square, 0.0), -1.0);
}

#[test]
fn lfo_sample_and_hold_returns_held_value_unchanged() {
    // Caller manages the held value; this fn just returns whatever it was given.
    assert_eq!(lfo_value_at(0.3, LfoWaveform::SampleAndHold, 0.42), 0.42);
    assert_eq!(lfo_value_at(0.7, LfoWaveform::SampleAndHold, -0.9), -0.9);
}

// ─── Free-EG ────────────────────────────────────────────────────────────────

#[test]
fn free_eg_depth_center_is_silent() {
    // depth_norm = 0.5 → bipolar_depth = 0 → output is always 0 regardless
    // of phase or values.  Core invariant of the bipolar mapping.
    let values = [0.5, 1.0, -0.5, 0.0, 0.7, -0.8, 0.3, 0.1];
    for phase in [0.0, 0.3, 0.5, 0.8, 1.0] {
        assert_eq!(free_eg_value_at(phase, &values, 0.5), 0.0);
    }
}

#[test]
fn free_eg_interpolates_between_adjacent_steps() {
    // values[0] = 0.0, values[1] = 1.0; phase 0 → 0, phase 1/7 → values[1] = 1.
    // At phase=0.5/7 ≈ 0.0714, the interp should sit roughly halfway between
    // values[0] and values[1] → 0.5, scaled by full positive depth (=1).
    let values = [0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];
    let mid = free_eg_value_at(0.5 / 7.0, &values, 1.0);
    assert!(
        (mid - 0.5).abs() < 1e-3,
        "expected 0.5 via linear interp + full +depth, got {mid}",
    );
}

#[test]
fn free_eg_negative_depth_inverts_output() {
    // depth_norm = 0 → bipolar_depth = -1 → output flips sign of level.
    let values = [1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0];
    assert!((free_eg_value_at(0.3, &values, 0.0) + 1.0).abs() < 1e-3);
    assert!((free_eg_value_at(0.3, &values, 1.0) - 1.0).abs() < 1e-3);
}

#[test]
fn free_eg_phase_out_of_range_is_clamped() {
    // Phase >1 or <0 should NOT panic (array index must stay in-bounds).
    // values[7] = 0.9, depth full → output ≈ 0.9.
    let values = [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.9];
    let out_hi = free_eg_value_at(2.0, &values, 1.0);
    let out_lo = free_eg_value_at(-0.5, &values, 1.0);
    assert!(
        (out_hi - 0.9).abs() < 1e-3,
        "phase=2 should clamp to last step"
    );
    assert_eq!(out_lo, 0.0, "phase=-0.5 should clamp to first step");
}
