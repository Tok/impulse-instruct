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
}
