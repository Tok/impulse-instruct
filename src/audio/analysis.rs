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
        // Narrow window — only fire when actually within ~1 dB of clipping.
        // Previously -3.0 lit up on most default-volume material.
        if self.peak_db > -1.5 && self.peak_db <= -1.0 {
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

/// Musical pattern observations from sequencer/mix state.
/// Returns short warnings that inform the user and nudge agents.
pub fn pattern_alerts(state: &crate::state::AppState) -> Vec<String> {
    let mut out = Vec::new();
    let seq = &state.sequencer;
    let steps = seq.steps;

    // Bass density
    let bass_active = seq
        .bass_pattern
        .iter()
        .take(steps)
        .filter(|s| s.active)
        .count();
    let density = if steps > 0 {
        bass_active as f32 / steps as f32
    } else {
        0.0
    };
    if bass_active > 0 && density > 0.8 {
        out.push("bass very dense".into());
    } else if seq.running && bass_active == 0 {
        out.push("no bass notes".into());
    } else if bass_active > 0 && bass_active <= 2 && steps >= 16 {
        out.push("bass sparse".into());
    }

    // Monotone bass
    if bass_active >= 4 {
        let notes: Vec<u8> = seq
            .bass_pattern
            .iter()
            .take(steps)
            .filter(|s| s.active)
            .map(|s| s.note)
            .collect();
        if notes.iter().all(|n| *n == notes[0]) {
            out.push("bass monotone".into());
        }
    }

    // Kick check
    if let Some(kick_pat) = seq.drum_patterns.get(&crate::state::DrumVoice::Kick808) {
        let kick_active = kick_pat.iter().take(steps).filter(|s| s.active).count();
        if seq.running && kick_active == 0 {
            out.push("no kick".into());
        }
    }

    // FX extremes
    if state.fx.reverb_mix > 0.5 {
        out.push(format!("reverb high ({:.0}%)", state.fx.reverb_mix * 100.0));
    }
    if state.fx.delay_feedback > 0.6 {
        out.push(format!(
            "delay fb high ({:.0}%)",
            state.fx.delay_feedback * 100.0
        ));
    }
    if state.fx.distortion_mix > 0.5 && state.fx.distortion_drive > 0.5 {
        out.push("heavy distortion".into());
    }

    out
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

// ─── Pitch detection (autocorrelation, continuous Hz) ───────────────────────
//
// Returns `(freq_hz, confidence)` from a mono buffer.  Confidence is 0..1 —
// higher means more periodic / less noise.  Used by the Tuner viz module
// (which wants cents-off precision the integer-MIDI `detect_note` can't
// provide).
//
// The algorithm: cumulative-mean-normalised autocorrelation difference,
// the core idea behind YIN.  We compute d(τ) = Σ (x[n] − x[n+τ])² over
// a search range, normalise, and pick the lag with the lowest dip below
// a threshold.  Cheap and good enough for a UI tuner display.
//
// Search range: 60 Hz–2000 Hz (covers bass through middle treble).

const PITCH_MIN_HZ: f32 = 60.0;
const PITCH_MAX_HZ: f32 = 2000.0;

pub fn detect_pitch_hz(buf: &[f32], sample_rate: f32) -> Option<(f32, f32)> {
    if buf.len() < 512 {
        return None;
    }
    // RMS gate — skip silent input.
    let rms = rms_of(buf);
    if rms < 0.005 {
        return None;
    }

    let max_lag = (sample_rate / PITCH_MIN_HZ) as usize;
    let min_lag = (sample_rate / PITCH_MAX_HZ).max(2.0) as usize;
    if buf.len() <= max_lag * 2 {
        return None;
    }

    // Cumulative mean normalised difference.
    let n = buf.len() - max_lag;
    let mut diff = vec![0.0f32; max_lag + 1];
    for tau in 1..=max_lag {
        let mut sum = 0.0f32;
        for i in 0..n {
            let d = buf[i] - buf[i + tau];
            sum += d * d;
        }
        diff[tau] = sum;
    }
    let mut cmnd = vec![1.0f32; max_lag + 1];
    let mut running = 0.0f32;
    for tau in 1..=max_lag {
        running += diff[tau];
        cmnd[tau] = if running > 0.0 {
            diff[tau] * tau as f32 / running
        } else {
            1.0
        };
    }

    // Find the first dip below threshold within search range.
    let threshold = 0.15;
    let mut chosen: Option<usize> = None;
    for tau in min_lag..max_lag {
        if cmnd[tau] < threshold && cmnd[tau] < cmnd[tau + 1] {
            chosen = Some(tau);
            break;
        }
    }
    // Fallback: pick the lowest-dip lag in the search range.
    let tau = chosen.unwrap_or_else(|| {
        let mut best = min_lag;
        let mut best_v = cmnd[min_lag];
        for (t, &v) in cmnd.iter().enumerate().take(max_lag).skip(min_lag) {
            if v < best_v {
                best_v = v;
                best = t;
            }
        }
        best
    });

    // Parabolic interpolation around `tau` for sub-sample precision.
    let refined_tau = if tau > 1 && tau < max_lag {
        let a = cmnd[tau - 1];
        let b = cmnd[tau];
        let c = cmnd[tau + 1];
        let denom = 2.0 * (a - 2.0 * b + c);
        if denom.abs() > 1e-9 {
            tau as f32 + (a - c) / denom
        } else {
            tau as f32
        }
    } else {
        tau as f32
    };
    if refined_tau < 2.0 {
        return None;
    }
    let freq = sample_rate / refined_tau;
    let confidence = (1.0 - cmnd[tau]).clamp(0.0, 1.0);
    Some((freq, confidence))
}

// ─── Chroma vector + chord detection ─────────────────────────────────────────
//
// `chroma_from_spectrum` folds an FFT magnitude spectrum into 12 pitch-class
// bins (C, C#, D, ..., B).  Each bin sums all spectral peaks that map to
// that pitch class regardless of octave.
//
// `detect_chord` matches a chroma vector against the 24 major+minor triad
// templates and returns the best-fitting `(root_pc, ChordKind)`.

/// Bin a magnitude spectrum into 12 pitch classes.  `bin_hz` is the FFT bin
/// spacing (same value used by `compute_spectrum`).  Magnitudes are
/// expected in **dBFS** (the convention used by `compute_spectrum`); they
/// are converted back to linear amplitude before summing so quiet bins
/// don't drag the chroma toward zero.  Anything below −60 dBFS is
/// dropped as effectively silent.
pub fn chroma_from_spectrum(mags: &[f32], bin_hz: f32) -> [f32; 12] {
    let mut chroma = [0.0f32; 12];
    if bin_hz <= 0.0 {
        return chroma;
    }
    let lowest_bin = ((27.5 / bin_hz).ceil() as usize).max(1);
    for (i, &m_db) in mags.iter().enumerate().skip(lowest_bin) {
        if m_db < -60.0 {
            continue;
        }
        let f = i as f32 * bin_hz;
        if !(27.5..=4000.0).contains(&f) {
            continue;
        }
        let lin = 10.0f32.powf(m_db / 20.0);
        // Split contribution across the two nearest pitch classes by
        // fractional MIDI so a 440 Hz peak landing in an off-centre
        // FFT bin (1024-FFT @ 48 kHz has only ~46 Hz resolution)
        // doesn't round into a neighbouring pitch class on its own.
        let midi = crate::audio::dsp::hz_to_midi(f);
        let lo = midi.floor();
        let frac = midi - lo;
        let pc_lo = (lo as i32).rem_euclid(12) as usize;
        let pc_hi = ((lo as i32) + 1).rem_euclid(12) as usize;
        chroma[pc_lo] += lin * (1.0 - frac);
        chroma[pc_hi] += lin * frac;
    }
    // Normalise so the strongest pitch class is 1.0 (or all-zero on silence).
    let peak = chroma.iter().copied().fold(0.0f32, f32::max);
    if peak > 1e-9 {
        for v in &mut chroma {
            *v /= peak;
        }
    }
    chroma
}

/// Triad quality used by `detect_chord`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChordKind {
    Major,
    Minor,
}

/// Match a chroma vector against the 24 major/minor triad templates.
/// Returns `(root_pitch_class, kind, confidence)` where confidence is the
/// dot-product score normalised by template+chroma magnitudes (~0..1).
pub fn detect_chord(chroma: &[f32; 12]) -> Option<(u8, ChordKind, f32)> {
    let total: f32 = chroma.iter().sum();
    if total < 1e-3 {
        return None;
    }
    // Major triad mask: root, M3 (+4), P5 (+7).
    // Minor triad mask: root, m3 (+3), P5 (+7).
    let major: [f32; 12] = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0];
    let minor: [f32; 12] = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0];

    let mut best: Option<(u8, ChordKind, f32)> = None;
    for root in 0..12u8 {
        for (kind, mask) in [(ChordKind::Major, &major), (ChordKind::Minor, &minor)] {
            let mut score = 0.0f32;
            for i in 0..12 {
                score += chroma[i] * mask[(i + 12 - root as usize) % 12];
            }
            // 3 active bins per template; normalise to ~0..1.
            let confidence = (score / 3.0).clamp(0.0, 1.0);
            if best.is_none_or(|(_, _, b)| confidence > b) {
                best = Some((root, kind, confidence));
            }
        }
    }
    best
}

/// Pitch-class names in the canonical order used by `chroma_from_spectrum`
/// and `detect_chord` (root index 0 = C).
pub const PITCH_CLASS_NAMES: [&str; 12] = [
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];

// ─── Spectrogram history sizing ──────────────────────────────────────────────

/// Number of past FFT frames to retain for the scrolling Spectrogram
/// module.  At typical UI rates (~60 Hz pushing every 2nd frame) this is
/// roughly 7 s of history; on faster machines closer to 4 s.
pub const SPECTROGRAM_HISTORY_LEN: usize = 200;

// ─── LUFS meter (K-weighted, momentary + short-term) ─────────────────────────
//
// K-weighting per ITU-R BS.1770-4: two biquads in series — a pre-filter
// (high-shelf at ≈ 1.5 kHz, +4 dB) followed by an RLB high-pass at ≈ 38 Hz.
// After K-weighting we exponentially-average the squared signal at two time
// constants: 400 ms for the momentary readout, 3 s for short-term.  The
// integrated LUFS measurement (with relative-level gating) is *not*
// implemented yet — the display uses momentary + short-term, which matches
// what most loudness meters surface to users.
//
// The biquad coefficients are hard-coded for 48 kHz — the engine ships at a
// fixed 48 kHz sample rate.  If we later support multi-SR, regenerate the
// coefficients per BS.1770 Annex 1.

const LUFS_PRE_B0: f32 = 1.535_124_9;
const LUFS_PRE_B1: f32 = -2.691_696_2;
const LUFS_PRE_B2: f32 = 1.198_392_8;
const LUFS_PRE_A1: f32 = -1.690_659_3;
const LUFS_PRE_A2: f32 = 0.732_480_8;

const LUFS_RLB_B0: f32 = 1.0;
const LUFS_RLB_B1: f32 = -2.0;
const LUFS_RLB_B2: f32 = 1.0;
const LUFS_RLB_A1: f32 = -1.990_047_5;
const LUFS_RLB_A2: f32 = 0.990_072_3;

/// LUFS calibration constant from BS.1770 (for K-weighted RMS → LUFS).
const LUFS_OFFSET_DB: f32 = -0.691;

/// Stateful momentary + short-term LUFS meter.  `process_sample` is cheap
/// enough to run on every UI tick.
#[derive(Debug, Clone)]
pub struct LufsMeter {
    /// Pre-filter biquad state: x[n-1], x[n-2], y[n-1], y[n-2].
    pre: [f32; 4],
    /// RLB high-pass biquad state.
    rlb: [f32; 4],
    /// EMA of K-weighted squared sample at the 400 ms time constant.
    momentary_ms: f32,
    /// EMA of K-weighted squared sample at the 3 s time constant.
    short_term_ms: f32,
    /// Pre-computed EMA coefficients (cached on construction).
    momentary_coef: f32,
    short_term_coef: f32,
}

impl LufsMeter {
    pub fn new(sample_rate: f32) -> Self {
        // EMA coefficient α such that the impulse decays to 1/e in τ seconds.
        let alpha = |tau: f32| -> f32 {
            let s = (sample_rate * tau).max(1.0);
            1.0 - (-1.0 / s).exp()
        };
        Self {
            pre: [0.0; 4],
            rlb: [0.0; 4],
            momentary_ms: 0.0,
            short_term_ms: 0.0,
            momentary_coef: alpha(0.400),
            short_term_coef: alpha(3.000),
        }
    }

    /// Push one sample through the K-weighting filters and update the EMAs.
    pub fn process_sample(&mut self, x: f32) {
        // Pre-filter biquad — direct form I.
        let pre_y = LUFS_PRE_B0 * x + LUFS_PRE_B1 * self.pre[0] + LUFS_PRE_B2 * self.pre[1]
            - LUFS_PRE_A1 * self.pre[2]
            - LUFS_PRE_A2 * self.pre[3];
        self.pre[1] = self.pre[0];
        self.pre[0] = x;
        self.pre[3] = self.pre[2];
        self.pre[2] = pre_y;

        // RLB high-pass biquad.
        let rlb_y = LUFS_RLB_B0 * pre_y + LUFS_RLB_B1 * self.rlb[0] + LUFS_RLB_B2 * self.rlb[1]
            - LUFS_RLB_A1 * self.rlb[2]
            - LUFS_RLB_A2 * self.rlb[3];
        self.rlb[1] = self.rlb[0];
        self.rlb[0] = pre_y;
        self.rlb[3] = self.rlb[2];
        self.rlb[2] = rlb_y;

        let sq = rlb_y * rlb_y;
        self.momentary_ms =
            self.momentary_ms * (1.0 - self.momentary_coef) + sq * self.momentary_coef;
        self.short_term_ms =
            self.short_term_ms * (1.0 - self.short_term_coef) + sq * self.short_term_coef;
    }

    /// Push a slice of samples through the filters.  Convenience wrapper
    /// around `process_sample`.
    pub fn process_block(&mut self, samples: &[f32]) {
        for &s in samples {
            self.process_sample(s);
        }
    }

    /// Convert a mean-square value to LUFS (K-weighted dB) per BS.1770.
    fn ms_to_lufs(ms: f32) -> f32 {
        if ms < 1e-12 {
            -120.0
        } else {
            (LUFS_OFFSET_DB + 10.0 * ms.log10()).max(-120.0)
        }
    }

    /// 400 ms windowed loudness in LUFS (more responsive).
    pub fn momentary_lufs(&self) -> f32 {
        Self::ms_to_lufs(self.momentary_ms)
    }

    /// 3 s windowed loudness in LUFS (smoother, more representative).
    pub fn short_term_lufs(&self) -> f32 {
        Self::ms_to_lufs(self.short_term_ms)
    }

    /// Reset internal filter state and EMAs.  Use when restarting playback
    /// or switching sources.
    pub fn reset(&mut self) {
        self.pre = [0.0; 4];
        self.rlb = [0.0; 4];
        self.momentary_ms = 0.0;
        self.short_term_ms = 0.0;
    }
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
    fn detect_pitch_silence_is_none() {
        let buf = vec![0.0f32; 2048];
        assert!(detect_pitch_hz(&buf, 48_000.0).is_none());
    }

    #[test]
    fn detect_pitch_440_hz_sine_lands_within_5_cents() {
        let sr = 48_000.0_f32;
        let samples: Vec<f32> = (0..4096)
            .map(|i| (2.0 * std::f32::consts::PI * 440.0 * i as f32 / sr).sin() * 0.5)
            .collect();
        let (hz, conf) = detect_pitch_hz(&samples, sr).expect("should detect 440 Hz");
        let cents_off = 1200.0 * (hz / 440.0).log2();
        assert!(
            cents_off.abs() < 5.0,
            "detected {hz} Hz, off by {cents_off} cents",
        );
        assert!(conf > 0.5, "confidence too low: {conf}");
    }

    #[test]
    fn detect_pitch_220_hz_sine_lands_within_5_cents() {
        let sr = 48_000.0_f32;
        let samples: Vec<f32> = (0..4096)
            .map(|i| (2.0 * std::f32::consts::PI * 220.0 * i as f32 / sr).sin() * 0.4)
            .collect();
        let (hz, _) = detect_pitch_hz(&samples, sr).expect("should detect 220 Hz");
        let cents_off = 1200.0 * (hz / 220.0).log2();
        assert!(cents_off.abs() < 5.0, "detected {hz} Hz");
    }

    #[test]
    fn chroma_a880_peaks_at_a_pitch_class() {
        // 880 Hz = A5 → pitch class 9.  The 1024-FFT at 48 kHz has only
        // ~46 Hz resolution, which is too coarse around A4 (440 Hz)
        // to land on a single bin reliably; at A5 the bins-per-semitone
        // is sufficient for a clean result.
        let sr = 48_000.0_f32;
        let samples: Vec<f32> = (0..1024)
            .map(|i| (2.0 * std::f32::consts::PI * 880.0 * i as f32 / sr).sin() * 0.5)
            .collect();
        let spec = crate::audio::spectrum::compute_spectrum(&samples, sr);
        let chroma = chroma_from_spectrum(&spec.magnitudes, spec.bin_hz);
        let max_idx = (0..12)
            .max_by(|a, b| chroma[*a].partial_cmp(&chroma[*b]).unwrap())
            .unwrap();
        assert_eq!(max_idx, 9, "expected A (idx 9), got {max_idx}");
    }

    #[test]
    fn detect_chord_c_major_triad() {
        // Mock chroma directly — A sum of C (0), E (4), G (7) should
        // resolve as C major.
        let mut chroma = [0.0f32; 12];
        chroma[0] = 1.0;
        chroma[4] = 1.0;
        chroma[7] = 1.0;
        let (root, kind, conf) = detect_chord(&chroma).expect("should detect chord");
        assert_eq!(root, 0);
        assert_eq!(kind, ChordKind::Major);
        assert!(conf > 0.9, "confidence {conf} too low for clean triad");
    }

    #[test]
    fn detect_chord_a_minor_triad() {
        // A (9), C (0), E (4) → A minor.
        let mut chroma = [0.0f32; 12];
        chroma[9] = 1.0;
        chroma[0] = 1.0;
        chroma[4] = 1.0;
        let (root, kind, _) = detect_chord(&chroma).expect("should detect chord");
        assert_eq!(root, 9);
        assert_eq!(kind, ChordKind::Minor);
    }

    #[test]
    fn lufs_meter_silence_floors_below_minus_70() {
        let mut m = LufsMeter::new(48_000.0);
        let silence = vec![0.0f32; 48_000];
        m.process_block(&silence);
        assert!(
            m.momentary_lufs() < -70.0,
            "silence should floor; got {}",
            m.momentary_lufs()
        );
        assert!(m.short_term_lufs() < -70.0);
    }

    #[test]
    fn lufs_meter_full_scale_sine_is_in_negative_range() {
        // A 0 dBFS peak 1 kHz sine has RMS −3 dBFS; K-weighting at 1 kHz
        // adds ~+1 dB, so steady-state LUFS lands around −2..−5.  We
        // check momentary (400 ms tau, converges in ~1 s) rather than
        // short-term (3 s tau, would need many more samples to settle).
        let sr = 48_000.0_f32;
        let samples: Vec<f32> = (0..(sr as usize) * 2)
            .map(|i| (2.0 * std::f32::consts::PI * 1_000.0 * i as f32 / sr).sin())
            .collect();
        let mut m = LufsMeter::new(sr);
        m.process_block(&samples);
        let mom = m.momentary_lufs();
        assert!(
            (-6.0..=2.0).contains(&mom),
            "0 dBFS sine M LUFS should be near 0; got {mom}",
        );
    }

    #[test]
    fn lufs_meter_quieter_input_is_lower_lufs() {
        // Same test idea as above but using the momentary meter so the
        // EMA has time to settle within the 2 s test window.
        let sr = 48_000.0_f32;
        let n = (sr as usize) * 2;
        let mut loud = LufsMeter::new(sr);
        let mut quiet = LufsMeter::new(sr);
        for i in 0..n {
            let s = (2.0 * std::f32::consts::PI * 1_000.0 * i as f32 / sr).sin();
            loud.process_sample(s);
            quiet.process_sample(s * 0.1); // -20 dB
        }
        let delta = loud.momentary_lufs() - quiet.momentary_lufs();
        assert!(
            (delta - 20.0).abs() < 1.0,
            "expected ~20 dB delta, got {delta}",
        );
    }

    #[test]
    fn lufs_reset_clears_state() {
        let mut m = LufsMeter::new(48_000.0);
        m.process_block(&vec![0.5_f32; 48_000]);
        assert!(m.momentary_lufs() > -50.0);
        m.reset();
        assert!(m.momentary_lufs() < -100.0);
    }

    #[test]
    fn detect_chord_silence_is_none() {
        let chroma = [0.0f32; 12];
        assert!(detect_chord(&chroma).is_none());
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
