// ─── audio/dsp/fx_math.rs ────────────────────────────────────────────────────
// Pure DSP math extracted from `mod.rs::apply_fx_step` and the
// `process_block` mixing / envelope-follower stages.  Keeping these as
// free functions buys two things:
//
// 1. They become unit-testable against closed-form expected values
//    without spinning up a full DspState (which owns hardware-state
//    things like rtrb consumers and reverb buffers).
// 2. The audio thread keeps its zero-allocation guarantee — the
//    helpers take and return only stack values; any held state
//    (bitcrush counter, sidechain envelope) lives in the caller and is
//    threaded through as `prev → next`.
//
// Anything stateful enough to need a buffer (reverb / delay / chorus /
// phaser / EQ / compressor / tape sat / autotune) stays inside its
// owning DSP processor — those aren't the right shape for free
// functions.

use super::dsp_util::tanh;

/// Crossfaded `tanh` waveshaper used by `FxStep::Waveshaper`.
///
/// `drive_norm` is 0..1 from the UI knob; the curve maps it to a 1..9×
/// pre-tanh gain.  The inner `tanh(drive)` divisor keeps the signal
/// hot but bounded as drive rises (without it, low drive sounds
/// quiet).  `mix` is the dry/wet blend.
pub fn waveshaper_step(sig: f32, drive_norm: f32, mix: f32) -> f32 {
    if mix <= 0.001 {
        return sig;
    }
    let drive = drive_norm * 8.0 + 1.0;
    let shaped = tanh(sig * drive) / tanh(drive);
    sig * (1.0 - mix) + shaped * mix
}

/// Crossfaded soft-clip distortion used by `FxStep::Drive`.  Same
/// shape as the waveshaper but with a gentler 1..7× drive curve so
/// the two FX feel different in the chain.
pub fn drive_step(sig: f32, drive_norm: f32, mix: f32) -> f32 {
    if drive_norm <= 0.01 {
        return sig;
    }
    let drive = drive_norm * 6.0 + 1.0;
    let dist = tanh(sig * drive);
    sig * (1.0 - mix) + dist * mix
}

/// State carried between successive `bitcrush_step` calls.  The held
/// value implements the sample-hold (rate reduction) and the counter
/// counts down the hold window.
#[derive(Clone, Copy, Debug, Default)]
pub struct BitcrushState {
    pub held: f32,
    pub counter: u32,
}

/// One sample of bit-depth + sample-rate reduction.  When the counter
/// hits zero the input is requantised to `bits` and re-held for
/// `1 + rate * 15` frames.  `mix` blends with the dry input.
///
/// Returns `(new_state, output_sample)` so the caller can keep the
/// state on the stack and avoid taking `&mut self` anywhere.
pub fn bitcrush_step(
    state: BitcrushState,
    sig: f32,
    bits_norm: f32,
    rate_norm: f32,
    mix: f32,
) -> (BitcrushState, f32) {
    if mix <= 0.01 {
        return (state, sig);
    }
    let mut state = state;
    if state.counter == 0 {
        let bits = (1.0 + bits_norm * 15.0).round().max(1.0);
        let scale = (1u32 << (bits as u32 - 1)) as f32;
        state.held = (sig * scale).round() / scale;
        state.counter = (1.0 + rate_norm * 15.0) as u32;
    } else {
        state.counter -= 1;
    }
    let out = sig * (1.0 - mix) + state.held * mix;
    (state, out)
}

/// One sample of attack/release envelope follower used by the
/// sidechain ducker.  `target` is the (positive) input level to
/// follow.  Attack is faster than release: when the input rises above
/// the previous envelope we coast toward it through `attack_ms`,
/// otherwise we decay through `release_ms`.
pub fn sidechain_envelope_step(
    prev_env: f32,
    target: f32,
    attack_ms: f32,
    release_ms: f32,
    sr: f32,
) -> f32 {
    let att_coeff = (-1.0 / (attack_ms * 0.001 * sr)).exp();
    let rel_coeff = (-1.0 / (release_ms * 0.001 * sr)).exp();
    if target > prev_env {
        target + (prev_env - target) * att_coeff
    } else {
        prev_env * rel_coeff
    }
}

/// Convert a sidechain envelope sample into the duck multiplier the
/// bus is multiplied by.  `amount` is the UI knob 0..1; the 4× factor
/// gives the knob useful headroom — at amount=0.25 the duck reaches
/// unity at env≈1.0.
pub fn sidechain_duck(env: f32, amount: f32) -> f32 {
    1.0 - (env * amount * 4.0).min(1.0)
}

/// One sample of the reverb gate envelope follower.  When the
/// detection-bus magnitude crosses ~0.08 we re-open the gate; the
/// envelope otherwise decays exponentially across `gate_time_s`.  A
/// `gate_time_s` of 0 disables the gate (always-open).
pub fn gated_reverb_envelope_step(
    prev_env: f32,
    detect_abs: f32,
    gate_time_s: f32,
    sr: f32,
) -> f32 {
    if gate_time_s <= 0.001 {
        return 1.0;
    }
    if detect_abs > 0.08 {
        1.0
    } else {
        prev_env * (-1.0 / (gate_time_s * sr)).exp()
    }
}

/// Pure LFO value lookup for a normalised phase 0..1.  For S&H, callers
/// pass the most recently latched value as `sh_held` — this function
/// never updates it; the caller decides when a phase wrap happened and
/// re-latches via its noise source.
pub fn lfo_value_at(phase: f32, waveform: crate::state::LfoWaveform, sh_held: f32) -> f32 {
    use crate::state::LfoWaveform;
    use std::f32::consts::TAU;
    match waveform {
        LfoWaveform::Sine => (phase * TAU).sin(),
        LfoWaveform::Triangle => 1.0 - 4.0 * (phase - 0.5).abs(),
        LfoWaveform::Saw => phase * 2.0 - 1.0,
        LfoWaveform::InvSaw => 1.0 - phase * 2.0,
        LfoWaveform::Square => {
            if phase < 0.5 {
                1.0
            } else {
                -1.0
            }
        }
        LfoWaveform::SampleAndHold => sh_held,
    }
}

/// Free-EG sample value: linearly interpolate the 8-step value array
/// at a normalised phase 0..1, then scale by a bipolar depth derived
/// from a 0..1 UI knob (0.5 → 0, 0 → -1, 1 → +1).
pub fn free_eg_value_at(phase: f32, values: &[f32; 8], depth_norm: f32) -> f32 {
    let pos = phase.clamp(0.0, 1.0) * 7.0; // 0..7
    let idx = pos.floor() as usize;
    let frac = pos.fract();
    let v0 = values[idx.min(7)];
    let v1 = values[(idx + 1).min(7)];
    let level = v0 + (v1 - v0) * frac;
    let bipolar_depth = (depth_norm - 0.5) * 2.0;
    level * bipolar_depth
}
