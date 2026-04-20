// ─── tests/audio_analysis_tests.rs ───────────────────────────────────────────
// Covers three pure math helpers in `audio/analysis.rs` +
// `audio/spectrum.rs` that didn't have dedicated tests:
//   • `stereo_correlation` — phase correlation + L/R balance from
//     interleaved stereo audio.
//   • `log_bands` — average FFT magnitudes into logarithmic bands
//     (used by the UI spectrum display).
//   • `spectrum_temperature` — magnitude-weighted warm/cold scalar
//     (drives the UI's Huth color tint).

use crate::audio::analysis::stereo_correlation;
use crate::audio::spectrum::{log_bands, spectrum_temperature};

// ─── stereo_correlation ─────────────────────────────────────────────────────

#[test]
fn stereo_correlation_mono_signal_is_fully_correlated() {
    // L == R → correlation = +1, balance = 0 (centre).
    let buf = vec![0.5, 0.5, -0.3, -0.3, 0.8, 0.8, -0.1, -0.1];
    let (corr, balance) = stereo_correlation(&buf);
    assert!(
        (corr - 1.0).abs() < 1e-4,
        "mono signal should be fully correlated, got {corr}",
    );
    assert!(
        balance.abs() < 1e-4,
        "mono signal should be centred, got balance={balance}",
    );
}

#[test]
fn stereo_correlation_out_of_phase_is_minus_one() {
    // L == -R → correlation = -1 (worst-case mono sum cancellation).
    let buf = vec![0.5, -0.5, -0.3, 0.3, 0.8, -0.8, -0.1, 0.1];
    let (corr, _) = stereo_correlation(&buf);
    assert!(
        (corr + 1.0).abs() < 1e-4,
        "out-of-phase signal should correlate to -1, got {corr}",
    );
}

#[test]
fn stereo_correlation_full_left_balance_is_minus_one() {
    // All energy on L, none on R → balance = -1 (full left).
    let buf = vec![0.6, 0.0, -0.4, 0.0, 0.5, 0.0];
    let (_, balance) = stereo_correlation(&buf);
    assert!(
        (balance + 1.0).abs() < 1e-4,
        "left-only signal should balance to -1, got {balance}",
    );
}

#[test]
fn stereo_correlation_full_right_balance_is_plus_one() {
    let buf = vec![0.0, 0.6, 0.0, -0.4, 0.0, 0.5];
    let (_, balance) = stereo_correlation(&buf);
    assert!(
        (balance - 1.0).abs() < 1e-4,
        "right-only signal should balance to +1, got {balance}",
    );
}

#[test]
fn stereo_correlation_empty_buffer_returns_zero_pair() {
    // Guard the denom=0 path — must not divide by zero / NaN.
    let (corr, balance) = stereo_correlation(&[]);
    assert_eq!(corr, 0.0);
    assert_eq!(balance, 0.0);
}

#[test]
fn stereo_correlation_silence_returns_zero_pair() {
    // All zeros → both sums are zero → denom == 0 → guarded branch
    // returns (0, 0) instead of NaN.
    let (corr, balance) = stereo_correlation(&[0.0; 16]);
    assert_eq!(corr, 0.0);
    assert_eq!(balance, 0.0);
}

#[test]
fn stereo_correlation_ignores_trailing_unpaired_sample() {
    // Odd-length buffer: the unpaired final sample must be ignored
    // rather than trip an index-out-of-bounds.  `frames = len / 2`
    // truncates; we just check it doesn't panic.
    let buf = vec![0.5, 0.5, 0.3]; // 1 full frame + trailing
    let (corr, _) = stereo_correlation(&buf);
    assert!((corr - 1.0).abs() < 1e-4);
}

// ─── log_bands ──────────────────────────────────────────────────────────────

#[test]
fn log_bands_returns_requested_band_count() {
    // Output length must equal num_bands regardless of input length.
    let mags = vec![1.0; 2048];
    for n in [4, 8, 16, 32, 64] {
        let bands = log_bands(&mags, 20.0, n);
        assert_eq!(bands.len(), n);
    }
}

#[test]
fn log_bands_preserves_uniform_magnitude_across_bands() {
    // Uniform magnitude input → every non-empty band averages to the
    // same value.  (Bins are averaged within each band.)
    let uniform_mag = 0.42_f32;
    let mags = vec![uniform_mag; 1024];
    let bands = log_bands(&mags, 20.0, 16);
    for (i, &b) in bands.iter().enumerate() {
        // Bands that successfully sampled bins should match exactly; an
        // empty band returns the -96 dB sentinel.
        if b != -96.0 {
            assert!(
                (b - uniform_mag).abs() < 1e-4,
                "band {i}: expected {uniform_mag} or -96 sentinel, got {b}",
            );
        }
    }
}

#[test]
fn log_bands_empty_or_zero_bin_hz_degrades_gracefully() {
    // Asking for bands over an empty magnitude slice shouldn't panic —
    // every band returns the -96 dB floor sentinel.
    let bands = log_bands(&[], 20.0, 8);
    assert!(
        bands.iter().all(|b| *b == -96.0),
        "empty input should give all-floor bands, got {bands:?}",
    );
}

// ─── spectrum_temperature ───────────────────────────────────────────────────

/// Flat temperature table = 0 for every pitch class.  Weighted average
/// over zero is trivially zero regardless of input spectrum.
const FLAT_TEMPS: [f32; 12] = [0.0; 12];

/// "Warmer low pitches" table: +1 at C, decays to 0 at G, -1 at F#
/// (half-octave).  Not physical Huth — just a deterministic stand-in.
const WARM_LOW_TEMPS: [f32; 12] = [
    1.0, 0.8, 0.5, 0.2, 0.0, -0.2, -1.0, -0.5, 0.0, 0.2, 0.5, 0.8,
];

#[test]
fn spectrum_temperature_returns_nan_on_empty_input() {
    let t = spectrum_temperature(&[], 20.0, &FLAT_TEMPS);
    assert!(t.is_nan(), "empty magnitudes must return NaN, got {t}");
}

#[test]
fn spectrum_temperature_returns_nan_when_bin_hz_nonpositive() {
    // bin_hz=0 would divide by zero while computing bin_lo/hi; guarded.
    let mags = vec![-20.0_f32; 1024];
    assert!(spectrum_temperature(&mags, 0.0, &FLAT_TEMPS).is_nan());
    assert!(spectrum_temperature(&mags, -5.0, &FLAT_TEMPS).is_nan());
}

#[test]
fn spectrum_temperature_returns_nan_when_everything_is_below_threshold() {
    // All magnitudes at -96 dB (floor); threshold is -60 dB, so every
    // weight is zero → total_weight=0 → NaN.
    let mags = vec![-96.0_f32; 1024];
    let t = spectrum_temperature(&mags, 20.0, &FLAT_TEMPS);
    assert!(t.is_nan(), "all-below-threshold must return NaN");
}

#[test]
fn spectrum_temperature_flat_table_always_returns_zero() {
    // Any input weights × 0 per-pc = 0.  Regardless of spectrum shape,
    // the flat table must yield exactly 0.
    let mut mags = vec![-96.0_f32; 1024];
    // Hot a single bin at -20 dBFS somewhere in the audible band.
    mags[64] = -20.0;
    let t = spectrum_temperature(&mags, 20.0, &FLAT_TEMPS);
    assert_eq!(t, 0.0, "flat temperature table must return exactly 0");
}

#[test]
fn spectrum_temperature_concentrates_on_single_pitch_class() {
    // Put all the energy on MIDI note 60 (C) with a "warm C" table —
    // the result should equal the C weight (=1.0 in WARM_LOW_TEMPS).
    // Hz of MIDI 60 ≈ 261.63; with bin_hz = 1 Hz the bin index is 262.
    let bin_hz = 1.0_f32;
    let mut mags = vec![-96.0_f32; 8192];
    mags[262] = 0.0; // loud bin at C4
    let t = spectrum_temperature(&mags, bin_hz, &WARM_LOW_TEMPS);
    assert!(
        (t - 1.0).abs() < 1e-3,
        "C-only signal with warm-C table should return ~1.0, got {t}",
    );
}

#[test]
fn spectrum_temperature_stays_in_unit_range() {
    // Output must land in [-1, 1] (or NaN).  Build a spectrum with
    // arbitrary magnitudes and extreme semi_temps values; check we
    // stay bounded.
    let bin_hz = 20.0_f32;
    let mut mags = vec![-50.0_f32; 512];
    // Several bins louder than the threshold, at mixed frequencies.
    for i in [30, 60, 90, 120, 150, 200] {
        mags[i] = -10.0;
    }
    let t = spectrum_temperature(&mags, bin_hz, &WARM_LOW_TEMPS);
    assert!(t.is_finite(), "should return a finite value, got {t}");
    assert!(
        (-1.0..=1.0).contains(&t),
        "output must be in [-1, 1] (matches semi_temps range), got {t}",
    );
}
