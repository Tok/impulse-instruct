// ─── state/automation_overlay.rs ─────────────────────────────────────────────
// Pure helpers that compute the per-step LFO sample curve for the
// sequencer's automation-lane overlay.  The overlay paints a sparkline
// under each voice's step row so the user can see where the modulator
// sits at each step relative to the beat grid.
//
// Kept as a pure function so the UI never has to inline phase / rate
// math — and so the values can be unit-tested without an egui context.

use super::bass::{BassLfoTarget, BassState};
use super::lfo::LfoWaveform;

/// Convert a normalised LFO `rate` knob (0..1) to a free-running rate
/// in Hz.  Matches the DSP path's mapping (quartic shaping so the low
/// end has fine control around 0.05–1 Hz).  Floor at 0.01 Hz so a
/// value of 0 doesn't make the LFO infinitely slow / stationary.
#[inline]
pub fn rate_knob_to_hz(rate: f32) -> f32 {
    let r = rate.clamp(0.0, 1.0);
    0.01 + (r * r * r * r) * 19.99
}

/// Phase advance (0..1) per sequencer step for an LFO synced to the
/// transport.  `step_division` is `sequencer.step_division` (4 = 16th-
/// note grid).  `sync_beats` is the LFO's `lfo_sync_beats` (1 = one
/// quarter note per cycle, 4 = one bar per cycle).
#[inline]
pub fn synced_phase_per_step(step_division: u8, sync_beats: f32) -> f32 {
    let div = (step_division as f32).max(1.0);
    let beats = sync_beats.max(0.0625);
    1.0 / (beats * div)
}

/// Phase advance (0..1) per sequencer step for a free-running LFO.
#[inline]
pub fn free_phase_per_step(rate_hz: f32, bpm: f32, step_division: u8) -> f32 {
    let bpm = bpm.max(20.0);
    let div = (step_division as f32).max(1.0);
    let step_seconds = 60.0 / (bpm * div);
    rate_hz * step_seconds
}

/// Compute the LFO output curve for `visible_steps` consecutive steps
/// starting at `page_start_step`.  Returns one bipolar (-1..1) value
/// per step, scaled by the LFO's depth.  When the LFO is off or its
/// depth is zero the result is all zeros (caller can elide the paint).
///
/// Sample-and-hold uses a deterministic stand-in (zero) for V1 — the
/// real DSP latches values from a noise source we don't replicate
/// here; better to draw flat than to invent values that don't match
/// what's actually played.
pub fn bass_lfo_curve_for_view(
    synth: &BassState,
    bpm: f32,
    step_division: u8,
    page_start_step: usize,
    visible_steps: usize,
) -> Vec<f32> {
    if synth.lfo_target == BassLfoTarget::Off || synth.lfo_depth <= 1e-6 {
        return vec![0.0; visible_steps];
    }
    let phase_per_step = if synth.lfo_bpm_sync {
        synced_phase_per_step(step_division, synth.lfo_sync_beats)
    } else {
        free_phase_per_step(rate_knob_to_hz(synth.lfo_rate), bpm, step_division)
    };
    let depth = synth.lfo_depth.clamp(0.0, 1.0);
    let phase_offset = synth.lfo_phase.rem_euclid(1.0);
    let sh_held = 0.0;
    (0..visible_steps)
        .map(|i| {
            let abs_step = page_start_step + i;
            let phase = (abs_step as f32 * phase_per_step + phase_offset).rem_euclid(1.0);
            let raw = match synth.lfo_waveform {
                LfoWaveform::Sine => (phase * std::f32::consts::TAU).sin(),
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
            };
            (raw * depth).clamp(-1.0, 1.0)
        })
        .collect()
}
