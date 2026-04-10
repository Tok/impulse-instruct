// ─── audio/analysis.rs ────────────────────────────────────────────────────────
// Lightweight spectral + dynamics analysis — pure functions.
// Called from the UI thread when the user clicks "Listen"; never inside the
// audio callback. Allocation is fine here.

use std::f32::consts::PI;

// ─── Result struct ────────────────────────────────────────────────────────────

/// Per-band RMS, peak, crest factor, and transient density for a captured
/// audio window. All dB values are dBFS (0 dBFS = full scale).
#[derive(Debug, Clone)]
pub struct AudioAnalysis {
    /// <80 Hz sub-bass RMS in dBFS
    pub sub_rms_db: f32,
    /// 80–250 Hz low-mid RMS in dBFS
    pub low_rms_db: f32,
    /// 250 Hz–4 kHz mid RMS in dBFS
    pub mid_rms_db: f32,
    /// >4 kHz high RMS in dBFS
    pub high_rms_db: f32,
    /// Overall peak in dBFS
    pub peak_db: f32,
    /// Crest factor = peak − overall RMS (dynamic range indicator)
    pub crest_db: f32,
    /// Transient onsets per bar (estimated at detected BPM / per 4 beats)
    pub transients_per_bar: f32,
    /// Duration of the analysed window in seconds
    pub duration_secs: f32,
}

impl Default for AudioAnalysis {
    fn default() -> Self {
        Self {
            sub_rms_db: -96.0,
            low_rms_db: -96.0,
            mid_rms_db: -96.0,
            high_rms_db: -96.0,
            peak_db: -96.0,
            crest_db: 0.0,
            transients_per_bar: 0.0,
            duration_secs: 0.0,
        }
    }
}

impl AudioAnalysis {
    /// Fixed-width summary for the header bar (no jitter).
    pub fn one_line_summary(&self) -> String {
        format!(
            "sub:{:>4.0} low:{:>4.0} mid:{:>4.0} hi:{:>4.0} pk:{:>4.0}dB",
            self.sub_rms_db, self.low_rms_db, self.mid_rms_db, self.high_rms_db, self.peak_db
        )
    }

    /// Alert strings for extreme/unusual conditions. Empty vec if normal.
    pub fn alerts(&self) -> Vec<&'static str> {
        let mut out = Vec::new();
        if self.peak_db > -1.0 {
            out.push("CLIPPING");
        }
        if self.peak_db > -3.0 && self.peak_db <= -1.0 {
            out.push("near clip");
        }
        if self.high_rms_db > -8.0 && self.transients_per_bar > 6.0 {
            out.push("snare rush");
        }
        if self.sub_rms_db > -6.0 {
            out.push("sub overload");
        }
        if self.low_rms_db - self.mid_rms_db > 20.0 {
            out.push("muddy low end");
        }
        if self.high_rms_db > -6.0 {
            out.push("harsh highs");
        }
        if self.crest_db < 3.0 && self.peak_db > -20.0 {
            out.push("over-compressed");
        }
        if self.mid_rms_db > -4.0 {
            out.push("mid overload");
        }
        if self.peak_db < -40.0 {
            out.push("near silence");
        }
        out
    }
}

// ─── Core analysis ────────────────────────────────────────────────────────────

/// Analyse a mono f32 PCM buffer (any length, any sample rate).
/// Returns [`AudioAnalysis`] with per-band RMS, peak, crest, and transient
/// density. All heavy allocations live here — never call from the audio thread.
pub fn analyse_audio(samples: &[f32], sample_rate: f32) -> AudioAnalysis {
    if samples.is_empty() {
        return AudioAnalysis::default();
    }

    let n = samples.len();
    let duration_secs = n as f32 / sample_rate;

    // ── Band-split via one-pole IIR LP filters ────────────────────────────────
    // lp(x, fc) → low-pass at fc Hz
    // Bands:   sub  = lp(80)
    //          low  = lp(250) − lp(80)
    //          mid  = lp(4000) − lp(250)
    //          high = signal  − lp(4000)
    let lp80 = apply_lp(samples, 80.0, sample_rate);
    let lp250 = apply_lp(samples, 250.0, sample_rate);
    let lp4k = apply_lp(samples, 4000.0, sample_rate);

    let sub_rms = band_rms(&lp80, samples, 0); // 0 = use lp80 directly
    let low_rms = subtract_rms(&lp250, &lp80);
    let mid_rms = subtract_rms(&lp4k, &lp250);
    let high_rms = residual_rms(samples, &lp4k);

    // ── Overall peak + crest factor ───────────────────────────────────────────
    let peak = samples.iter().map(|&s| s.abs()).fold(0.0_f32, f32::max);
    let overall_rms = rms_of(samples);
    let crest_db = to_db(peak) - to_db(overall_rms.max(1e-9));

    // ── Transient density ─────────────────────────────────────────────────────
    // Use ~50ms energy frames; count frames where energy jumps >6 dB vs previous.
    let frame_size = (sample_rate * 0.05) as usize;
    let mut prev_rms = 0.0_f32;
    let mut onset_count = 0u32;
    for chunk in samples.chunks(frame_size) {
        let frame_rms = rms_of(chunk);
        if frame_rms > prev_rms * 2.0 && frame_rms > 0.01 {
            onset_count += 1;
        }
        prev_rms = frame_rms;
    }
    // Normalise to onsets/bar — assume 4/4 and estimate duration in bars
    // using the overall energy rhythm as a rough tempo proxy.
    // If we have BPM we'd use it; without it, we express as onsets/second.
    let transients_per_bar = if duration_secs > 0.0 {
        onset_count as f32 / duration_secs * 2.0 // *2 ≈ 2 sec/bar at 120 BPM
    } else {
        0.0
    };

    AudioAnalysis {
        sub_rms_db: to_db(sub_rms),
        low_rms_db: to_db(low_rms),
        mid_rms_db: to_db(mid_rms),
        high_rms_db: to_db(high_rms),
        peak_db: to_db(peak),
        crest_db,
        transients_per_bar,
        duration_secs,
    }
}

/// Format an analysis snapshot as structured text for the LLM prompt.
pub fn format_snapshot(a: &AudioAnalysis) -> String {
    format!(
        "[AUDIO SNAPSHOT — {:.1}s captured]\n\
         Band RMS (dBFS):  sub {:.0}  low {:.0}  mid {:.0}  high {:.0}\n\
         Peak: {:.1} dBFS  |  Crest: {:.1} dB  |  Transients: ~{:.1}/bar",
        a.duration_secs,
        a.sub_rms_db,
        a.low_rms_db,
        a.mid_rms_db,
        a.high_rms_db,
        a.peak_db,
        a.crest_db,
        a.transients_per_bar,
    )
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// One-pole IIR low-pass filter applied to `samples`. Returns a new Vec.
fn apply_lp(samples: &[f32], cutoff_hz: f32, sample_rate: f32) -> Vec<f32> {
    let c = (-2.0 * PI * cutoff_hz / sample_rate).exp();
    let a = 1.0 - c;
    let mut out = Vec::with_capacity(samples.len());
    let mut z = 0.0_f32;
    for &x in samples {
        z = a * x + c * z;
        out.push(z);
    }
    out
}

/// RMS of a filtered band signal (used for sub band — lp80 directly).
fn band_rms(band: &[f32], _original: &[f32], _mode: u8) -> f32 {
    rms_of(band)
}

/// RMS of (a − b) elementwise — produces the band between two LP cutoffs.
fn subtract_rms(high_lp: &[f32], low_lp: &[f32]) -> f32 {
    let n = high_lp.len().min(low_lp.len());
    let sum_sq: f32 = (0..n)
        .map(|i| {
            let v = high_lp[i] - low_lp[i];
            v * v
        })
        .sum();
    (sum_sq / n as f32).sqrt()
}

/// RMS of (original − lp) — the high-frequency residual.
fn residual_rms(original: &[f32], lp: &[f32]) -> f32 {
    let n = original.len().min(lp.len());
    let sum_sq: f32 = (0..n)
        .map(|i| {
            let v = original[i] - lp[i];
            v * v
        })
        .sum();
    (sum_sq / n as f32).sqrt()
}

fn rms_of(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum_sq: f32 = samples.iter().map(|&s| s * s).sum();
    (sum_sq / samples.len() as f32).sqrt()
}

fn to_db(linear: f32) -> f32 {
    if linear <= 1e-9 {
        return -96.0;
    }
    (20.0 * linear.log10()).max(-96.0)
}

// ─── Stereo correlation ──────────────────────────────────────────────────────

/// Compute stereo phase correlation (-1..+1) and L/R balance (-1..+1) from
/// interleaved stereo samples [L0, R0, L1, R1, ...].
///
/// Correlation: +1 = mono (L==R), 0 = uncorrelated, -1 = out of phase (L==-R).
/// Balance: -1 = full left, 0 = center, +1 = full right.
pub fn stereo_correlation(interleaved: &[f32]) -> (f32, f32) {
    let frames = interleaved.len() / 2;
    if frames == 0 {
        return (0.0, 0.0);
    }
    let mut sum_lr = 0.0_f64;
    let mut sum_ll = 0.0_f64;
    let mut sum_rr = 0.0_f64;
    let mut sum_l = 0.0_f64;
    let mut sum_r = 0.0_f64;
    for i in 0..frames {
        let l = interleaved[i * 2] as f64;
        let r = interleaved[i * 2 + 1] as f64;
        sum_lr += l * r;
        sum_ll += l * l;
        sum_rr += r * r;
        sum_l += l.abs();
        sum_r += r.abs();
    }
    let denom = (sum_ll * sum_rr).sqrt();
    let corr = if denom > 1e-12 {
        (sum_lr / denom) as f32
    } else {
        0.0
    };
    let total = sum_l + sum_r;
    let balance = if total > 1e-12 {
        ((sum_r - sum_l) / total) as f32
    } else {
        0.0
    };
    (corr.clamp(-1.0, 1.0), balance.clamp(-1.0, 1.0))
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_input_returns_defaults() {
        let a = analyse_audio(&[], 44100.0);
        assert_eq!(a.peak_db, -96.0);
    }

    #[test]
    fn silence_is_minus_96() {
        let silence = vec![0.0f32; 44100];
        let a = analyse_audio(&silence, 44100.0);
        assert!(a.peak_db <= -90.0);
        assert!(a.sub_rms_db <= -90.0);
    }

    #[test]
    fn full_scale_sine_peak_near_zero_db() {
        let sr = 44100.0f32;
        let samples: Vec<f32> = (0..44100)
            .map(|i| (2.0 * std::f32::consts::PI * 440.0 * i as f32 / sr).sin())
            .collect();
        let a = analyse_audio(&samples, sr);
        // Peak of a 0 dBFS sine should be close to 0 dBFS
        assert!(a.peak_db > -1.0, "peak_db was {}", a.peak_db);
        // RMS of sine at 0 dBFS is −3 dBFS
        assert!(a.mid_rms_db > -6.0, "mid_rms_db was {}", a.mid_rms_db);
    }

    #[test]
    fn low_freq_sine_goes_mostly_to_sub_band() {
        let sr = 44100.0f32;
        let samples: Vec<f32> = (0..44100)
            .map(|i| (2.0 * std::f32::consts::PI * 40.0 * i as f32 / sr).sin())
            .collect();
        let a = analyse_audio(&samples, sr);
        // Sub should be much louder than high
        assert!(
            a.sub_rms_db > a.high_rms_db + 20.0,
            "sub={} high={}",
            a.sub_rms_db,
            a.high_rms_db
        );
    }

    #[test]
    fn format_snapshot_contains_key_labels() {
        let a = AudioAnalysis {
            sub_rms_db: -18.0,
            low_rms_db: -14.0,
            mid_rms_db: -22.0,
            high_rms_db: -28.0,
            peak_db: -2.0,
            crest_db: 12.0,
            transients_per_bar: 8.0,
            duration_secs: 4.0,
        };
        let s = format_snapshot(&a);
        assert!(s.contains("AUDIO SNAPSHOT"));
        assert!(s.contains("sub"));
        assert!(s.contains("Crest"));
        assert!(s.contains("Transients"));
    }

    #[test]
    fn to_db_clamps_at_minus_96() {
        assert_eq!(to_db(0.0), -96.0);
        assert_eq!(to_db(1e-12), -96.0);
    }

    #[test]
    fn stereo_correlation_mono_is_one() {
        // Identical L and R → correlation = 1.0
        let interleaved: Vec<f32> = (0..200)
            .flat_map(|i| {
                let v = (i as f32 * 0.1).sin();
                [v, v]
            })
            .collect();
        let (corr, bal) = super::stereo_correlation(&interleaved);
        assert!((corr - 1.0).abs() < 0.01, "corr={corr}");
        assert!(bal.abs() < 0.1, "bal={bal}");
    }

    #[test]
    fn stereo_correlation_inverted_is_minus_one() {
        let interleaved: Vec<f32> = (0..200)
            .flat_map(|i| {
                let v = (i as f32 * 0.1).sin();
                [v, -v]
            })
            .collect();
        let (corr, _bal) = super::stereo_correlation(&interleaved);
        assert!((corr - (-1.0)).abs() < 0.01, "corr={corr}");
    }

    #[test]
    fn stereo_balance_panned_right() {
        let interleaved: Vec<f32> = (0..200)
            .flat_map(|i| {
                let v = (i as f32 * 0.1).sin();
                [v * 0.2, v * 0.8] // mostly right
            })
            .collect();
        let (_corr, bal) = super::stereo_correlation(&interleaved);
        assert!(bal > 0.3, "bal={bal} should be positive (right-heavy)");
    }
}
