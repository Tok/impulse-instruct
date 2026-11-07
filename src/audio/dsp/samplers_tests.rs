use super::*;
use crate::audio::dsp::params::MAX_AMEN_SLICES;

/// Build a ramp sample [0, 1, 2, …, n-1] as f32 so the read position
/// is recoverable from each output value.
fn ramp_sample(n: usize) -> Arc<Vec<f32>> {
    Arc::new((0..n).map(|i| i as f32).collect())
}

fn nan16() -> [f32; MAX_AMEN_SLICES] {
    [f32::NAN; MAX_AMEN_SLICES]
}

/// Per-slice reverse-override sentinel: every slot at `-1` means
/// "inherit the global `reverse` flag", which preserves the legacy
/// behaviour the existing trigger tests assert.
fn none16() -> [i8; MAX_AMEN_SLICES] {
    [-1_i8; MAX_AMEN_SLICES]
}

fn render(voice: &mut AmenVoice, n: usize) -> Vec<f32> {
    (0..n).map(|_| voice.process(0.0, 1.0, false)).collect()
}

#[test]
fn trigger_whole_plays_full_sample_forward() {
    let mut v = AmenVoice::new();
    v.load(ramp_sample(8));
    v.trigger_whole();
    let out = render(&mut v, 8);
    // Reads 0..7 with linear interp; last sample stops because
    // idx+1 hits sample length.  First five should be 0..4 exactly.
    assert_eq!(&out[..5], &[0.0, 1.0, 2.0, 3.0, 4.0]);
}

/// Simulates the sequencer's multi-step trigger pattern: with the
/// default step.slice = 0, the sequencer maps step N to effective_slice
/// = vstep + 1, so a 4-step pass with slice_count=4 should land on
/// slices 0, 1, 2, 3 (not stuck on the last one).  Reproduces the
/// shape of the user-reported "only loops the last fragment" bug.
#[test]
fn sequencer_pass_visits_every_slice() {
    let mut v = AmenVoice::new();
    v.load(ramp_sample(16)); // 4 slices × 4 samples
    let slice_count = 4u8;
    let mut first_samples = Vec::new();
    for vstep in 0u8..slice_count {
        // Mirror sequencer/mod.rs: effective_slice = vstep + 1 when
        // step.slice == 0 (the default for a freshly-active step).
        let effective_slice = vstep + 1;
        v.trigger(
            effective_slice,
            slice_count,
            0.0,
            1.0,
            false,
            1.0,
            0,
            &nan16(),
            &nan16(),
            &nan16(),
            &none16(),
            false,
            false,
            136.0,
            136.0,
        );
        // Render one sample to capture the slice's first read; the
        // ramp buffer makes that first read equal to the slice start.
        first_samples.push(v.process(0.0, 1.0, false));
    }
    // Each slice starts at vstep × 4 in the ramp, so first_samples
    // should be [0, 4, 8, 12] — proving every slice gets its turn.
    assert_eq!(
        first_samples,
        vec![0.0, 4.0, 8.0, 12.0],
        "sequencer pass should visit every slice in order"
    );
}

#[test]
fn slice_index_selects_correct_region() {
    // 16 samples / 4 slices = slice_len 4.  Slice 2 → positions 4..7.
    let mut v = AmenVoice::new();
    v.load(ramp_sample(16));
    v.trigger(
        2,
        4,
        0.0,
        1.0,
        false,
        1.0,
        0,
        &nan16(),
        &nan16(),
        &nan16(),
        &none16(),
        false,
        false,
        136.0,
        136.0,
    );
    let out = render(&mut v, 4);
    assert_eq!(out[0], 4.0);
    assert!(out[3] < 8.0 && out[3] >= 7.0);
}

#[test]
fn reverse_plays_backward() {
    let mut v = AmenVoice::new();
    v.load(ramp_sample(8));
    v.trigger(
        1,
        1,
        0.0,
        1.0,
        true,
        1.0,
        0,
        &nan16(),
        &nan16(),
        &nan16(),
        &none16(),
        false,
        false,
        136.0,
        136.0,
    );
    let out = render(&mut v, 4);
    // pos starts at send-1 = 7, decrements by 1.0 each call.
    assert!(out[0] > out[1] && out[1] > out[2]);
    assert!((out[0] - 7.0).abs() < 0.01);
}

#[test]
fn stutter_fits_inside_slice_budget() {
    // Slice length 8; stutter=1 → sub_len=4; gate=1 → window=4.
    // After 4 samples voice should retrigger (pos resets to slice start).
    let mut v = AmenVoice::new();
    v.load(ramp_sample(8));
    v.trigger(
        1,
        1,
        0.0,
        1.0,
        false,
        1.0,
        1,
        &nan16(),
        &nan16(),
        &nan16(),
        &none16(),
        false,
        false,
        136.0,
        136.0,
    );
    let out = render(&mut v, 8);
    // First sub-slice: 0,1,2,3.  Stutter retrigger resets to 0.
    assert_eq!(out[0], 0.0);
    assert!(out[3] >= 3.0);
    // Sample 4 is the first read of the retriggered slice → back near 0.
    assert!(
        out[4] < 1.0,
        "expected stutter retrigger near 0, got {}",
        out[4]
    );
}

#[test]
fn stutter_zero_plays_full_slice_then_stops() {
    let mut v = AmenVoice::new();
    v.load(ramp_sample(8));
    v.trigger(
        1,
        1,
        0.0,
        1.0,
        false,
        1.0,
        0,
        &nan16(),
        &nan16(),
        &nan16(),
        &none16(),
        false,
        false,
        136.0,
        136.0,
    );
    // 8 samples to play through, then silence (no stutter, no loop).
    let out = render(&mut v, 12);
    assert!(out[0] < out[5]);
    assert_eq!(out[10], 0.0);
    assert_eq!(out[11], 0.0);
}

#[test]
fn custom_positions_override_equal_division() {
    // 16 samples; positions [0.0, 0.5] for 2 slices → slice 1 = 0..8.
    let mut v = AmenVoice::new();
    v.load(ramp_sample(16));
    let mut pos = nan16();
    pos[0] = 0.0;
    pos[1] = 0.5;
    v.trigger(
        1,
        2,
        0.0,
        1.0,
        false,
        1.0,
        0,
        &pos,
        &nan16(),
        &nan16(),
        &none16(),
        false,
        false,
        136.0,
        136.0,
    );
    let out = render(&mut v, 8);
    assert_eq!(out[0], 0.0);
    assert!((out[7] - 7.0).abs() < 0.01);
}

#[test]
fn auto_advance_increments_each_trigger() {
    let mut v = AmenVoice::new();
    v.load(ramp_sample(16));
    // slice_count=4; slice 0 means auto.  First fire = slice 0 (region 0..4).
    v.trigger(
        0,
        4,
        0.0,
        1.0,
        false,
        1.0,
        0,
        &nan16(),
        &nan16(),
        &nan16(),
        &none16(),
        false,
        false,
        136.0,
        136.0,
    );
    assert_eq!(v.process(0.0, 1.0, false), 0.0);
    // Re-trigger advances: slice 1 (region 4..8).
    v.trigger(
        0,
        4,
        0.0,
        1.0,
        false,
        1.0,
        0,
        &nan16(),
        &nan16(),
        &nan16(),
        &none16(),
        false,
        false,
        136.0,
        136.0,
    );
    assert_eq!(v.process(0.0, 1.0, false), 4.0);
}

#[test]
fn per_slice_reverse_overrides_global_forward() {
    // Global `reverse=false` but slot 0 of `slice_reverses` = 1 (force
    // reverse).  Playing slice 1 (idx0=0) should read backwards.
    let mut v = AmenVoice::new();
    v.load(ramp_sample(8));
    let mut rev = none16();
    rev[0] = 1;
    v.trigger(
        1,
        1,
        0.0,
        1.0,
        false, // global: forward
        1.0,
        0,
        &nan16(),
        &nan16(),
        &nan16(),
        &rev,
        false,
        false,
        136.0,
        136.0,
    );
    let out = render(&mut v, 4);
    // Per-slice override flips direction: starts at 7.0 and walks down.
    assert!(out[0] > out[1] && out[1] > out[2]);
    assert!((out[0] - 7.0).abs() < 0.01);
}

#[test]
fn per_slice_override_forces_forward_on_global_reverse() {
    // Global `reverse=true` but slot 0 = 0 (force forward).  Slice plays
    // normally despite the global flag.
    let mut v = AmenVoice::new();
    v.load(ramp_sample(8));
    let mut rev = none16();
    rev[0] = 0;
    v.trigger(
        1,
        1,
        0.0,
        1.0,
        true, // global: reverse
        1.0,
        0,
        &nan16(),
        &nan16(),
        &nan16(),
        &rev,
        false,
        false,
        136.0,
        136.0,
    );
    let out = render(&mut v, 4);
    assert!(out[0] < out[1] && out[1] < out[2]);
    assert_eq!(out[0], 0.0);
}

#[test]
fn per_slice_sentinel_inherits_global() {
    // Slot 0 = -1 means "inherit global".  With global=true, playback
    // must be reverse even though the override array is "populated".
    let mut v = AmenVoice::new();
    v.load(ramp_sample(8));
    let rev = none16(); // all -1
    v.trigger(
        1,
        1,
        0.0,
        1.0,
        true, // global: reverse
        1.0,
        0,
        &nan16(),
        &nan16(),
        &nan16(),
        &rev,
        false,
        false,
        136.0,
        136.0,
    );
    let out = render(&mut v, 4);
    assert!(out[0] > out[1]);
    assert!((out[0] - 7.0).abs() < 0.01);
}

// ── Pitch-preserving granular stretch ───────────────────────────────

/// Build a sample of known length filled with a ramp 0..n-1 so a
/// test can reason about the read position by inspecting the
/// returned value — the preserve-pitch logic lives in the read-
/// position maths, not the audio itself.
fn run_preserve(sample_len: usize, stretch_on: bool, preserve_on: bool) -> f32 {
    let mut v = AmenVoice::new();
    v.load(ramp_sample(sample_len));
    // Play the whole sample as one slice (count=1).  Gate=1 + loop_mode=true
    // in process(...) keeps the voice live past the natural end.
    v.trigger(
        1,
        1,
        0.0,
        1.0,
        false,
        1.0,
        0,
        &nan16(),
        &nan16(),
        &nan16(),
        &none16(),
        stretch_on,
        preserve_on,
        170.0, // source_bpm — faster than host, so preserve slows by ratio 120/170.
        120.0, // sequencer_bpm
    );
    // One grain's worth of samples + a few more to land after the jump.
    let n = AMEN_GRAIN_LEN as usize + 10;
    let mut last = 0.0;
    for _ in 0..n {
        last = v.process(0.0, 1.0, true);
    }
    last
}

#[test]
fn preserve_pitch_flag_captured_at_trigger() {
    // Preserve mode only engages when both the bpm_stretch flag and
    // the preserve flag are true; the trigger has four combinations
    // and only (true, true) should flip preserve_pitch on.
    let mut v = AmenVoice::new();
    v.load(ramp_sample(32));
    let fire = |v: &mut AmenVoice, stretch: bool, preserve: bool| {
        v.trigger(
            1,
            1,
            0.0,
            1.0,
            false,
            1.0,
            0,
            &nan16(),
            &nan16(),
            &nan16(),
            &none16(),
            stretch,
            preserve,
            170.0,
            120.0,
        );
    };
    fire(&mut v, false, false);
    assert!(!v.preserve_pitch);
    fire(&mut v, false, true);
    assert!(!v.preserve_pitch, "preserve=true alone must not engage");
    fire(&mut v, true, false);
    assert!(!v.preserve_pitch, "stretch without preserve stays classic");
    fire(&mut v, true, true);
    assert!(v.preserve_pitch);
    assert!(
        (v.stretch_ratio - 120.0 / 170.0).abs() < 1e-4,
        "stretch_ratio should capture host/source, got {}",
        v.stretch_ratio,
    );
}

#[test]
fn preserve_mode_suppresses_bpm_pitch_shift() {
    // Classic stretch bakes a pitch shift into extra_pitch; preserve
    // mode must NOT.  A per-slice pitch override still composes, but
    // here we leave it unset so preserve mode's extra_pitch is 0.
    let mut v = AmenVoice::new();
    v.load(ramp_sample(32));
    v.trigger(
        1,
        1,
        0.0,
        1.0,
        false,
        1.0,
        0,
        &nan16(),
        &nan16(),
        &nan16(),
        &none16(),
        true, // bpm_stretch
        true, // preserve
        170.0,
        120.0,
    );
    assert!(
        v.extra_pitch.abs() < 1e-4,
        "preserve mode should leave extra_pitch at 0, got {}",
        v.extra_pitch
    );
}

#[test]
fn classic_stretch_still_applies_pitch_shift() {
    // Sanity check: disabling preserve keeps the original semantics.
    // host/source = 120/170 ≈ 0.706 → 12*log2(0.706) ≈ -6.03 semitones.
    let mut v = AmenVoice::new();
    v.load(ramp_sample(32));
    v.trigger(
        1,
        1,
        0.0,
        1.0,
        false,
        1.0,
        0,
        &nan16(),
        &nan16(),
        &nan16(),
        &none16(),
        true,  // bpm_stretch
        false, // preserve
        170.0,
        120.0,
    );
    let expected = 12.0 * (120.0_f32 / 170.0).log2();
    assert!(
        (v.extra_pitch - expected).abs() < 1e-3,
        "classic stretch should pitch-shift: got {}, want {}",
        v.extra_pitch,
        expected,
    );
}

#[test]
fn preserve_mode_crosses_grain_boundary_and_reads_stretched() {
    // Preserve slows the break by stretch_ratio ≈ 0.706.  At the
    // grain boundary, pos rewinds by (0.706 - 1) * GRAIN_LEN ≈
    // -603 samples so the next grain re-reads the tail of the
    // previous.  Easiest invariant: the reading head is still
    // inside the sample rather than running off the end after
    // GRAIN_LEN + 10 samples of playback at native rate.
    let sample_len = (AMEN_GRAIN_LEN as usize) + 256;
    // Sanity: classic mode at native rate would read near sample
    // index GRAIN_LEN, comfortably under sample_len.
    let classic = run_preserve(sample_len, false, false);
    let preserve = run_preserve(sample_len, true, true);
    assert!(classic.is_finite());
    assert!(preserve.is_finite());
    // Preserve mode should have rewound at the boundary, so its
    // read position is *behind* classic's — ramp value therefore
    // lower.
    assert!(
        preserve < classic,
        "preserve read should trail classic (rewound at grain boundary): classic={}, preserve={}",
        classic,
        preserve,
    );
}

// ── wrap_into_slice ────────────────────────────────────────────────

#[test]
fn wrap_forward_past_end_lands_inside_slice() {
    // Slice = [100, 200).  A position 250 (50 past end) should
    // land at 150 — start + (pos - end) modulo slice_len.
    let p = super::wrap_into_slice(250.0, 100.0, 200.0, true);
    assert!((p - 150.0).abs() < 1e-4);
}

#[test]
fn wrap_forward_below_start_reflects_to_end() {
    // Slow-down rewind — position 70 (30 before start) should
    // wrap to 170 (end - 30).
    let p = super::wrap_into_slice(70.0, 100.0, 200.0, true);
    assert!((p - 170.0).abs() < 1e-4);
}

#[test]
fn wrap_reverse_below_start_wraps_to_end_minus_one() {
    // Reverse mode uses end - 1 as the mirror landing so the
    // interpolator has a valid neighbour sample.
    let p = super::wrap_into_slice(70.0, 100.0, 200.0, false);
    assert!((p - 169.0).abs() < 1e-4);
}

#[test]
fn wrap_identity_inside_slice() {
    let p = super::wrap_into_slice(150.0, 100.0, 200.0, true);
    assert!((p - 150.0).abs() < 1e-4);
    let p = super::wrap_into_slice(150.0, 100.0, 200.0, false);
    assert!((p - 150.0).abs() < 1e-4);
}

// ── Grain-boundary crossfade (v2) ──────────────────────────────────

/// Render `count` samples from a fresh preserve-mode amen with
/// a simple sine-like ramp source, returning the whole vector.
fn render_preserve(count: usize, sample_len: usize) -> Vec<f32> {
    let mut v = AmenVoice::new();
    v.load(ramp_sample(sample_len));
    v.trigger(
        1,
        1,
        0.0,
        1.0,
        false,
        1.0,
        0,
        &nan16(),
        &nan16(),
        &nan16(),
        &none16(),
        true,  // bpm_stretch
        true,  // preserve
        170.0, // source
        120.0, // host — slows by ~0.706
    );
    (0..count).map(|_| v.process(0.0, 1.0, true)).collect()
}

#[test]
fn crossfade_blends_current_and_lookahead_in_fade_window() {
    // One full grain + enough past to cover the fade + a few
    // samples of the next grain.  Within the fade window, the
    // output should smoothly transition from the "current read"
    // toward the "lookahead read"; outside it, output equals the
    // raw read (ramp sample value at the read position).
    let sample_len = (AMEN_GRAIN_LEN as usize) * 3;
    let total = (AMEN_GRAIN_LEN as usize) + 16;
    let out = render_preserve(total, sample_len);
    let fade_start = (AMEN_GRAIN_LEN - AMEN_GRAIN_FADE) as usize;
    let fade_end = AMEN_GRAIN_LEN as usize;
    // The sample just before the fade should track the read
    // position nearly identically (no crossfade contribution).
    let pre = out[fade_start.saturating_sub(4)];
    // Mid-fade: must differ from the pure-ramp extrapolation
    // because the lookahead is pulling toward a different
    // source position.
    let mid = out[(fade_start + fade_end) / 2];
    let mid_extrapolated = pre + ((fade_start + fade_end) as f32 / 2.0 - fade_start as f32);
    assert!(
        (mid - mid_extrapolated).abs() > 1.0,
        "mid-fade sample should differ from pure-ramp prediction: mid={}, pure={}",
        mid,
        mid_extrapolated,
    );
}

#[test]
fn crossfade_eliminates_splice_discontinuity() {
    // Measure the sample-to-sample delta across the grain boundary.
    // With the crossfade, it should stay in the neighbourhood of
    // the per-sample delta elsewhere in the waveform (ramp slope
    // ≈ 1.0 per sample) rather than jumping by the full rewind
    // distance ~600 samples that v1 would produce.
    let sample_len = (AMEN_GRAIN_LEN as usize) * 3;
    let total = (AMEN_GRAIN_LEN as usize) + 16;
    let out = render_preserve(total, sample_len);
    let splice = AMEN_GRAIN_LEN as usize;
    let delta = (out[splice] - out[splice - 1]).abs();
    assert!(
        delta < 10.0,
        "splice delta should be small after crossfade, got {}",
        delta
    );
}
