// ─── tests/spectrum_onset_tests.rs ───────────────────────────────────────────
// Covers three more pure audio helpers:
//   • `compute_spectrum` — FFT magnitude spectrum (1024-bin Hann
//     window).  Used by the UI spectrum display + Huth temperature.
//   • `detect_onsets` — energy-based transient detector for break
//     slicing.  Used by the Amen sampler's AUTO slice button.
//   • `format_snapshot` — AudioAnalysis → human-readable string used
//     in LLM prompts.
//
// These are pure math with closed-form expected behaviours.  Catching
// silent drift here matters because the UI + sampler rely on stable
// output shape.

use crate::audio::analysis::{AudioAnalysis, format_snapshot};
use crate::audio::onset::detect_onsets;
use crate::audio::spectrum::{FFT_SIZE, compute_spectrum};

// ─── compute_spectrum ───────────────────────────────────────────────────────

#[test]
fn compute_spectrum_returns_half_fft_size_bins() {
    // Magnitude spectrum is positive frequencies only → N/2 bins.
    let sr = 44_100.0;
    let samples = vec![0.0_f32; FFT_SIZE];
    let result = compute_spectrum(&samples, sr);
    assert_eq!(result.magnitudes.len(), FFT_SIZE / 2);
}

#[test]
fn compute_spectrum_bin_hz_matches_sr_over_n() {
    // Bin resolution = sample_rate / FFT_SIZE.  Off-by-one here would
    // misalign every band lookup downstream.
    let sr = 48_000.0_f32;
    let samples = vec![0.0_f32; FFT_SIZE];
    let result = compute_spectrum(&samples, sr);
    assert!((result.bin_hz - (sr / FFT_SIZE as f32)).abs() < 1e-3);
}

#[test]
fn compute_spectrum_silence_is_at_floor() {
    // All-zero input → every bin at the -96 dB floor.  A silent
    // buffer must not produce above-floor magnitudes (would confuse
    // the UI's "signal present" detection).
    let samples = vec![0.0_f32; FFT_SIZE];
    let result = compute_spectrum(&samples, 44_100.0);
    for &m in &result.magnitudes {
        assert!(m <= -95.9, "silence should be at the -96 dB floor, got {m}",);
    }
}

#[test]
fn compute_spectrum_short_input_is_zero_padded_without_panic() {
    // Inputs shorter than FFT_SIZE must be zero-padded, not rejected.
    // Tested with a 256-sample buffer (quarter of the window).
    let samples = vec![0.1_f32; 256];
    let result = compute_spectrum(&samples, 44_100.0);
    assert_eq!(result.magnitudes.len(), FFT_SIZE / 2);
}

#[test]
fn compute_spectrum_sine_puts_energy_in_expected_bin() {
    // Generate a 1000 Hz sine wave at 44.1 kHz.  Expected bin:
    // 1000 / (44100/1024) ≈ bin 23.  That bin should stand out from
    // neighbours by a wide dB margin.
    let sr = 44_100.0_f32;
    let freq = 1000.0_f32;
    let samples: Vec<f32> = (0..FFT_SIZE)
        .map(|i| (2.0 * std::f32::consts::PI * freq * i as f32 / sr).sin())
        .collect();
    let result = compute_spectrum(&samples, sr);
    let expected_bin = (freq / result.bin_hz).round() as usize;
    let peak_bin = (0..result.magnitudes.len())
        .max_by(|&a, &b| {
            result.magnitudes[a]
                .partial_cmp(&result.magnitudes[b])
                .unwrap()
        })
        .unwrap();
    // The true peak should land within 1 bin of the expected one (Hann
    // window spreads energy slightly).
    assert!(
        peak_bin.abs_diff(expected_bin) <= 1,
        "1 kHz sine peak should be at bin {expected_bin}, got {peak_bin}",
    );
}

// ─── detect_onsets ──────────────────────────────────────────────────────────

#[test]
fn detect_onsets_always_anchors_slice_zero() {
    // Slice 0 must always be 0.0 — the sampler assumes it can index
    // there without a bounds check.  Even silence / tiny buffers
    // should return at least [0.0].
    let samples = vec![0.0_f32; 100];
    let out = detect_onsets(&samples, 44_100.0, 4);
    assert_eq!(out, vec![0.0]);
}

#[test]
fn detect_onsets_returns_sorted_unique_positions_in_unit_range() {
    // Build a buffer with 3 energy spikes separated by silence.  The
    // detector should find them, normalise to 0..1 of buffer length,
    // and return ascending unique values.
    let sr = 44_100.0_f32;
    let n = (sr as usize) * 2; // 2 s buffer
    let mut samples = vec![0.0_f32; n];
    for &hit_idx in &[n / 4, n / 2, 3 * n / 4] {
        // Short burst of high energy
        for k in 0..200 {
            if hit_idx + k < n {
                samples[hit_idx + k] = 0.8;
            }
        }
    }
    let out = detect_onsets(&samples, sr, 16);
    assert!(out.first() == Some(&0.0), "slice 0 must anchor at 0.0");
    for pair in out.windows(2) {
        assert!(
            pair[1] > pair[0],
            "onsets must be strictly ascending: {pair:?}",
        );
    }
    for &p in &out {
        assert!((0.0..=1.0).contains(&p), "onset {p} out of unit range");
    }
}

#[test]
fn detect_onsets_caps_at_max_slices() {
    // Output length is bounded by max_slices.  Request 2 → get ≤2.
    let sr = 44_100.0_f32;
    let n = (sr as usize) * 2;
    let mut samples = vec![0.0_f32; n];
    for &hit in &[n / 8, n / 4, n / 2, 3 * n / 4, 7 * n / 8] {
        for k in 0..200 {
            if hit + k < n {
                samples[hit + k] = 0.8;
            }
        }
    }
    let out = detect_onsets(&samples, sr, 2);
    assert!(
        out.len() <= 2,
        "output must respect max_slices cap, got {}",
        out.len(),
    );
}

#[test]
fn detect_onsets_max_slices_zero_returns_anchor_only() {
    // max_slices=0 is "don't slice" — return just the anchor so callers
    // don't have to special-case empty output.
    let samples = vec![0.1_f32; 48_000];
    let out = detect_onsets(&samples, 44_100.0, 0);
    assert_eq!(out, vec![0.0]);
}

#[test]
fn detect_onsets_tiny_buffer_returns_anchor_only() {
    // Buffer shorter than the minimum (<512 samples) can't be
    // analysed meaningfully; return just the anchor.
    let out = detect_onsets(&vec![0.5_f32; 100], 44_100.0, 8);
    assert_eq!(out, vec![0.0]);
}

// ─── format_snapshot ────────────────────────────────────────────────────────

#[test]
fn format_snapshot_includes_every_analysis_field() {
    // Snapshot feeds the LLM prompt; every numeric field must appear
    // (otherwise the model can't reason about the missing dimension).
    let a = AudioAnalysis {
        sub_rms_db: -12.3,
        low_rms_db: -20.1,
        mid_rms_db: -18.4,
        high_rms_db: -25.0,
        peak_db: -3.0,
        crest_db: 9.5,
        transients_per_bar: 4.2,
        duration_secs: 2.0,
    };
    let out = format_snapshot(&a);
    // Headers.
    assert!(out.contains("AUDIO SNAPSHOT"));
    assert!(out.contains("Band RMS"));
    assert!(out.contains("Peak:"));
    assert!(out.contains("Crest:"));
    assert!(out.contains("Transients:"));
    // Numeric values (rounded to one decimal where applicable).
    assert!(out.contains("2.0s"), "duration must appear: {out}");
    assert!(out.contains("-3.0"), "peak must appear: {out}");
    assert!(out.contains("9.5"), "crest must appear: {out}");
    assert!(out.contains("4.2"), "transients/bar must appear: {out}");
}

#[test]
fn format_snapshot_default_state_produces_floor_values() {
    // Default AudioAnalysis = "silence / no activity".  The rendered
    // snapshot should reflect that (-96 dB floor values) without
    // panicking on the extreme numbers.
    let a = AudioAnalysis::default();
    let out = format_snapshot(&a);
    // dBFS values render as "-96" (no decimal, {:.0} format).
    let floor_count = out.matches("-96").count();
    assert!(
        floor_count >= 4,
        "expected four -96 dBFS band values, got snapshot:\n{out}",
    );
}
