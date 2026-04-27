// ─── audio/spectrum.rs ────────────────────────────────────────────────────────
// Real-time FFT spectrum analysis.  Pure function — runs in the UI thread on
// samples already captured by the scope ring buffer.  No allocations in the
// audio callback.

use rustfft::{FftPlanner, num_complex::Complex};

/// FFT window size.  1024 gives ~43 Hz resolution at 44.1 kHz (good enough
/// for a visual analyser; higher sizes add latency without visible benefit).
pub const FFT_SIZE: usize = 1024;

/// Result of a single FFT frame.
pub struct SpectrumResult {
    /// Magnitude per bin in dBFS (0..N/2), floor at -96 dB.
    pub magnitudes: Vec<f32>,
    /// Hertz per bin (sample_rate / FFT_SIZE).
    pub bin_hz: f32,
}

/// Compute the magnitude spectrum of `samples`.
///
/// - Truncates or zero-pads to `FFT_SIZE`.
/// - Applies a Hann window.
/// - Returns positive-frequency bins only (N/2).
pub fn compute_spectrum(samples: &[f32], sample_rate: f32) -> SpectrumResult {
    let mut buf: Vec<Complex<f32>> = Vec::with_capacity(FFT_SIZE);
    for i in 0..FFT_SIZE {
        let sample = if i < samples.len() { samples[i] } else { 0.0 };
        let window = hann(i, FFT_SIZE);
        buf.push(Complex::new(sample * window, 0.0));
    }

    let mut planner = FftPlanner::new();
    let fft = planner.plan_fft_forward(FFT_SIZE);
    fft.process(&mut buf);

    let half = FFT_SIZE / 2;
    let norm = FFT_SIZE as f32;
    let magnitudes: Vec<f32> = buf[..half]
        .iter()
        .map(|c| {
            let mag = c.norm() / norm;
            to_db(mag)
        })
        .collect();

    SpectrumResult {
        magnitudes,
        bin_hz: sample_rate / FFT_SIZE as f32,
    }
}

/// Map N/2 linear FFT bins into `num_bands` logarithmic bands (20 Hz – 20 kHz).
/// Returns averaged dB per band.
pub fn log_bands(magnitudes: &[f32], bin_hz: f32, num_bands: usize) -> Vec<f32> {
    let lo = 20.0_f32;
    let hi = 20_000.0_f32;
    let log_lo = lo.ln();
    let log_hi = hi.ln();
    let mut bands = vec![-96.0_f32; num_bands];

    for (i, band) in bands.iter_mut().enumerate() {
        let f_start = (log_lo + (log_hi - log_lo) * i as f32 / num_bands as f32).exp();
        let f_end = (log_lo + (log_hi - log_lo) * (i + 1) as f32 / num_bands as f32).exp();
        let bin_start = (f_start / bin_hz).floor() as usize;
        let bin_end = ((f_end / bin_hz).ceil() as usize).min(magnitudes.len());
        if bin_start >= magnitudes.len() || bin_start >= bin_end {
            continue;
        }
        let sum: f32 = magnitudes[bin_start..bin_end].iter().sum();
        let count = (bin_end - bin_start) as f32;
        *band = sum / count;
    }
    bands
}

// ─── Huth temperature (warm/cold) from spectrum ──────────────────────────────
// Magnitude-weighted average of the per-semitone warm/cold scalar.  Pure
// function — caller supplies the 12-entry semitone table (the UI layer's
// `theme::NOTE_TEMP`) so this stays in the audio crate without a UI dep.

/// Bins below this many dB above floor contribute zero weight.
const TEMP_DB_THRESHOLD: f32 = -60.0;
/// Lower frequency cutoff — below this is subsonic / DC bin.
const TEMP_HZ_MIN: f32 = 30.0;
/// Upper frequency cutoff — above this it's mostly cymbal noise without
/// a meaningful pitch class.
const TEMP_HZ_MAX: f32 = 5_000.0;

/// Magnitude-weighted warm/cold value for the spectrum.  Returns a number
/// in `[-1.0, 1.0]` (-1 cold / +1 warm) or `NaN` if there is no signal
/// above the threshold.  `magnitudes` are dBFS as produced by
/// [`compute_spectrum`].
pub fn spectrum_temperature(magnitudes: &[f32], bin_hz: f32, semi_temps: &[f32; 12]) -> f32 {
    if magnitudes.is_empty() || bin_hz <= 0.0 {
        return f32::NAN;
    }
    let bin_lo = ((TEMP_HZ_MIN / bin_hz).floor() as usize).max(1);
    let bin_hi = ((TEMP_HZ_MAX / bin_hz).ceil() as usize).min(magnitudes.len());
    if bin_lo >= bin_hi {
        return f32::NAN;
    }
    let mut weighted_sum = 0.0_f32;
    let mut total_weight = 0.0_f32;
    for (i, &mag) in magnitudes.iter().enumerate().take(bin_hi).skip(bin_lo) {
        let w = (mag - TEMP_DB_THRESHOLD).max(0.0);
        if w <= 0.0 {
            continue;
        }
        let hz = i as f32 * bin_hz;
        let semi = crate::audio::dsp::hz_to_midi(hz).round() as i32;
        let pc = semi.rem_euclid(12) as usize;
        weighted_sum += w * semi_temps[pc];
        total_weight += w;
    }
    if total_weight <= 0.0 {
        f32::NAN
    } else {
        (weighted_sum / total_weight).clamp(-1.0, 1.0)
    }
}

/// Hann window coefficient for sample `i` of `n`.
fn hann(i: usize, n: usize) -> f32 {
    let phase = std::f32::consts::TAU * i as f32 / n as f32;
    0.5 * (1.0 - phase.cos())
}

/// Linear amplitude to dBFS, floored at -96.
fn to_db(linear: f32) -> f32 {
    if linear <= 0.0 {
        -96.0
    } else {
        (20.0 * linear.log10()).max(-96.0)
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hann_window_endpoints_are_zero() {
        assert!((hann(0, 1024)).abs() < 1e-6);
    }

    #[test]
    fn hann_window_midpoint_is_one() {
        assert!((hann(512, 1024) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn to_db_floor() {
        assert_eq!(to_db(0.0), -96.0);
        assert_eq!(to_db(-1.0), -96.0);
    }

    #[test]
    fn to_db_unity() {
        assert!((to_db(1.0)).abs() < 0.01); // 0 dB
    }

    #[test]
    fn spectrum_of_silence_is_floor() {
        let silence = vec![0.0_f32; 1024];
        let result = compute_spectrum(&silence, 44100.0);
        assert_eq!(result.magnitudes.len(), FFT_SIZE / 2);
        assert!(result.magnitudes.iter().all(|&m| m <= -90.0));
    }

    #[test]
    fn spectrum_of_sine_peaks_at_correct_bin() {
        // 440 Hz sine at 44100 Hz sample rate
        let sr = 44100.0_f32;
        let freq = 440.0_f32;
        let samples: Vec<f32> = (0..FFT_SIZE)
            .map(|i| (std::f32::consts::TAU * freq * i as f32 / sr).sin())
            .collect();
        let result = compute_spectrum(&samples, sr);
        // Expected bin: 440 / (44100/1024) ≈ 10.2 → bin 10
        let expected_bin = (freq / result.bin_hz).round() as usize;
        let peak_bin = result
            .magnitudes
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
            .unwrap()
            .0;
        assert!(
            (peak_bin as i32 - expected_bin as i32).unsigned_abs() <= 1,
            "peak at bin {} but expected ~{}",
            peak_bin,
            expected_bin,
        );
    }

    #[test]
    fn log_bands_returns_correct_count() {
        let mags = vec![-40.0; 512];
        let bands = log_bands(&mags, 44100.0 / 1024.0, 64);
        assert_eq!(bands.len(), 64);
    }

    #[test]
    fn short_input_is_zero_padded() {
        let short = vec![0.5; 100]; // much less than FFT_SIZE
        let result = compute_spectrum(&short, 44100.0);
        assert_eq!(result.magnitudes.len(), FFT_SIZE / 2);
    }

    // C  C# D  D# E  F  F# G  G# A  A# B
    const HUTH_TEMPS: [f32; 12] = [
        -1.00, -0.87, -0.50, 0.00, 0.50, 1.00, 0.87, 0.34, -0.17, -0.64, -0.91, -1.00,
    ];

    fn sine(freq: f32, sr: f32) -> Vec<f32> {
        (0..FFT_SIZE)
            .map(|i| (std::f32::consts::TAU * freq * i as f32 / sr).sin())
            .collect()
    }

    #[test]
    fn spectrum_temperature_silence_is_nan() {
        let mags = vec![-96.0_f32; 512];
        let t = spectrum_temperature(&mags, 44100.0 / 1024.0, &HUTH_TEMPS);
        assert!(t.is_nan(), "got {t}");
    }

    #[test]
    fn spectrum_temperature_empty_input_is_nan() {
        assert!(spectrum_temperature(&[], 44.0, &HUTH_TEMPS).is_nan());
    }

    #[test]
    fn spectrum_temperature_c_tone_reads_cold() {
        // C5 = 523.25 Hz — Huth Blue, coldest.
        let sr = 44100.0_f32;
        let result = compute_spectrum(&sine(523.25, sr), sr);
        let t = spectrum_temperature(&result.magnitudes, result.bin_hz, &HUTH_TEMPS);
        assert!(t < -0.5, "C tone should read cold, got {t}");
    }

    #[test]
    fn spectrum_temperature_f_tone_reads_warm() {
        // F5 = 698.46 Hz — Huth Orange, warmest.
        let sr = 44100.0_f32;
        let result = compute_spectrum(&sine(698.46, sr), sr);
        let t = spectrum_temperature(&result.magnitudes, result.bin_hz, &HUTH_TEMPS);
        assert!(t > 0.5, "F tone should read warm, got {t}");
    }

    #[test]
    fn spectrum_temperature_in_range() {
        // Mixed broadband signal — value must stay clamped.
        let sr = 44100.0_f32;
        let mut samples = sine(220.0, sr);
        for (i, s) in sine(440.0, sr).iter().enumerate() {
            samples[i] += s;
        }
        let result = compute_spectrum(&samples, sr);
        let t = spectrum_temperature(&result.magnitudes, result.bin_hz, &HUTH_TEMPS);
        assert!((-1.0..=1.0).contains(&t), "{t} out of range");
    }
}
