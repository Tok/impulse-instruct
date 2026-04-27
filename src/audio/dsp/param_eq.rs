// ─── audio/dsp/param_eq.rs ───────────────────────────────────────────────────
// 8-band parametric EQ driven by `ParamEqBand` state.
//
// Each band is a single biquad filter in transposed Direct Form II.
// Coefficients come from the RBJ Audio EQ Cookbook and are only
// recomputed when the source band parameters change — per-sample
// processing is 8 × (4 mul + 4 add + 2 state writes) plus the dirty-
// check comparison, so the cascade runs comfortably inside the audio
// callback with no allocations.
//
// The cascade is applied after the dirty check so disabled or
// 0 dB bands with `kind == Peak` are cheaply bypassed (their coeffs
// collapse to b0=1, b1=b2=a1=a2=0, which is still a valid pass-through
// but we short-circuit the multiply for cache friendliness).

use super::dsp_util::{AUDIBLE_HZ_MIN, nyquist_guard};
use crate::state::{ParamEqBand, ParamEqBandKind};

/// Number of bands in the ParamEq cascade.
pub const PARAM_EQ_BANDS: usize = 8;

/// One biquad filter + its last-known source parameters.  When the
/// current `ParamEqBand` diverges from `cached`, `refresh_coeffs`
/// recomputes b0..a2.  Filter state (`z1`, `z2`) carries across coef
/// updates so a parameter tweak glitches minimally instead of zipper-
/// resetting the memory on every frame.
#[derive(Clone, Copy, Debug)]
struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    z1: f32,
    z2: f32,
    // Cached source parameters — used to decide when to recompute.
    cached_kind: u8,
    cached_freq: f32,
    cached_gain: f32,
    cached_q: f32,
    cached_enabled: bool,
    cached_sr: f32,
    /// True when b0..a2 can be bypassed (band disabled or exactly unity).
    bypass: bool,
}

impl Biquad {
    fn identity() -> Self {
        Self {
            b0: 1.0,
            b1: 0.0,
            b2: 0.0,
            a1: 0.0,
            a2: 0.0,
            z1: 0.0,
            z2: 0.0,
            cached_kind: 255, // force recompute on first update
            cached_freq: 0.0,
            cached_gain: 0.0,
            cached_q: 0.0,
            cached_enabled: false,
            cached_sr: 0.0,
            bypass: true,
        }
    }

    #[inline]
    fn step(&mut self, x: f32) -> f32 {
        if self.bypass {
            return x;
        }
        // Transposed Direct Form II: minimal per-sample state.
        let y = self.b0 * x + self.z1;
        self.z1 = self.b1 * x + self.z2 - self.a1 * y;
        self.z2 = self.b2 * x - self.a2 * y;
        y
    }

    /// Recompute coefficients from the source band if anything changed.
    /// Called per-block (not per-sample) via the outer ParamEq's
    /// process loop — 8 scalar comparisons per band is cheap.
    fn refresh(&mut self, band: &ParamEqBand, sr: f32) {
        let kind_u8 = band.kind.to_u8();
        if self.cached_kind == kind_u8
            && self.cached_freq == band.freq_hz
            && self.cached_gain == band.gain_db
            && self.cached_q == band.q
            && self.cached_enabled == band.enabled
            && self.cached_sr == sr
        {
            return;
        }
        self.cached_kind = kind_u8;
        self.cached_freq = band.freq_hz;
        self.cached_gain = band.gain_db;
        self.cached_q = band.q;
        self.cached_enabled = band.enabled;
        self.cached_sr = sr;

        // Cheap bypass: disabled band or (peak at ~0 dB) → identity.
        // Shelf at 0 dB is also an identity — same short-circuit.
        if !band.enabled || band.gain_db.abs() < 1e-3 {
            self.b0 = 1.0;
            self.b1 = 0.0;
            self.b2 = 0.0;
            self.a1 = 0.0;
            self.a2 = 0.0;
            self.bypass = true;
            return;
        }

        let (b0, b1, b2, a0, a1, a2) =
            biquad_coeffs(band.kind, band.freq_hz, band.gain_db, band.q, sr);
        // Normalise by a0 so the filter is (b0/a0 + b1/a0 z^-1 + ...) / (1 + a1/a0 z^-1 + ...).
        let inv_a0 = 1.0 / a0;
        self.b0 = b0 * inv_a0;
        self.b1 = b1 * inv_a0;
        self.b2 = b2 * inv_a0;
        self.a1 = a1 * inv_a0;
        self.a2 = a2 * inv_a0;
        self.bypass = false;
    }
}

/// Compute RBJ Audio EQ Cookbook biquad coefficients for a peak or
/// shelf filter.  Returned as (b0, b1, b2, a0, a1, a2) — caller
/// normalises by a0.  Pure function; exposed at crate-level so the UI
/// curve renderer can reuse it.
pub fn biquad_coeffs(
    kind: ParamEqBandKind,
    freq_hz: f32,
    gain_db: f32,
    q: f32,
    sr: f32,
) -> (f32, f32, f32, f32, f32, f32) {
    let freq_hz = freq_hz.clamp(AUDIBLE_HZ_MIN, nyquist_guard(sr));
    let q = q.clamp(0.1, 10.0);
    let gain_db = gain_db.clamp(-18.0, 18.0);

    let a_amp = (10.0_f32).powf(gain_db / 40.0);
    let w0 = std::f32::consts::TAU * freq_hz / sr;
    let cos_w0 = w0.cos();
    let sin_w0 = w0.sin();
    let alpha = sin_w0 / (2.0 * q);

    match kind {
        ParamEqBandKind::Peak => {
            let a_alpha = alpha * a_amp;
            let a_alpha_inv = alpha / a_amp;
            let b0 = 1.0 + a_alpha;
            let b1 = -2.0 * cos_w0;
            let b2 = 1.0 - a_alpha;
            let a0 = 1.0 + a_alpha_inv;
            let a1 = -2.0 * cos_w0;
            let a2 = 1.0 - a_alpha_inv;
            (b0, b1, b2, a0, a1, a2)
        }
        ParamEqBandKind::LowShelf => {
            let sqrt_a = a_amp.sqrt();
            let two_sqrt_a_alpha = 2.0 * sqrt_a * alpha;
            let b0 = a_amp * ((a_amp + 1.0) - (a_amp - 1.0) * cos_w0 + two_sqrt_a_alpha);
            let b1 = 2.0 * a_amp * ((a_amp - 1.0) - (a_amp + 1.0) * cos_w0);
            let b2 = a_amp * ((a_amp + 1.0) - (a_amp - 1.0) * cos_w0 - two_sqrt_a_alpha);
            let a0 = (a_amp + 1.0) + (a_amp - 1.0) * cos_w0 + two_sqrt_a_alpha;
            let a1 = -2.0 * ((a_amp - 1.0) + (a_amp + 1.0) * cos_w0);
            let a2 = (a_amp + 1.0) + (a_amp - 1.0) * cos_w0 - two_sqrt_a_alpha;
            (b0, b1, b2, a0, a1, a2)
        }
        ParamEqBandKind::HighShelf => {
            let sqrt_a = a_amp.sqrt();
            let two_sqrt_a_alpha = 2.0 * sqrt_a * alpha;
            let b0 = a_amp * ((a_amp + 1.0) + (a_amp - 1.0) * cos_w0 + two_sqrt_a_alpha);
            let b1 = -2.0 * a_amp * ((a_amp - 1.0) + (a_amp + 1.0) * cos_w0);
            let b2 = a_amp * ((a_amp + 1.0) + (a_amp - 1.0) * cos_w0 - two_sqrt_a_alpha);
            let a0 = (a_amp + 1.0) - (a_amp - 1.0) * cos_w0 + two_sqrt_a_alpha;
            let a1 = 2.0 * ((a_amp - 1.0) - (a_amp + 1.0) * cos_w0);
            let a2 = (a_amp + 1.0) - (a_amp - 1.0) * cos_w0 - two_sqrt_a_alpha;
            (b0, b1, b2, a0, a1, a2)
        }
    }
}

/// Magnitude (linear) of a single biquad at `freq_hz`.  Pure;
/// independent of filter state.  Used by the UI curve renderer to
/// draw the composite freq response without reaching into DspState.
pub fn band_magnitude(band: &ParamEqBand, sr: f32, freq_hz: f32) -> f32 {
    if !band.enabled || band.gain_db.abs() < 1e-3 {
        return 1.0;
    }
    let (b0, b1, b2, a0, a1, a2) = biquad_coeffs(band.kind, band.freq_hz, band.gain_db, band.q, sr);
    let inv_a0 = 1.0 / a0;
    let (b0, b1, b2, a1, a2) = (
        b0 * inv_a0,
        b1 * inv_a0,
        b2 * inv_a0,
        a1 * inv_a0,
        a2 * inv_a0,
    );
    let w = std::f32::consts::TAU * freq_hz / sr;
    let cos_w = w.cos();
    let cos_2w = (2.0 * w).cos();
    let sin_w = w.sin();
    let sin_2w = (2.0 * w).sin();
    // H(e^jw) = (b0 + b1 e^-jw + b2 e^-2jw) / (1 + a1 e^-jw + a2 e^-2jw)
    let num_re = b0 + b1 * cos_w + b2 * cos_2w;
    let num_im = -b1 * sin_w - b2 * sin_2w;
    let den_re = 1.0 + a1 * cos_w + a2 * cos_2w;
    let den_im = -a1 * sin_w - a2 * sin_2w;
    let num_mag2 = num_re * num_re + num_im * num_im;
    let den_mag2 = den_re * den_re + den_im * den_im;
    (num_mag2 / den_mag2.max(1e-12)).sqrt()
}

/// Composite magnitude in dB at `freq_hz` for the full cascade —
/// sum of per-band dB contributions so the UI curve + downstream
/// visualisers share one canonical response function.
pub fn cascade_db(bands: &[ParamEqBand], sr: f32, freq_hz: f32) -> f32 {
    let mut total_db = 0.0_f32;
    for b in bands {
        let mag = band_magnitude(b, sr, freq_hz);
        if mag > 1e-9 {
            total_db += super::dsp_util::lin_to_db(mag);
        }
    }
    total_db
}

pub struct ParamEq {
    biquads: [Biquad; PARAM_EQ_BANDS],
}

impl ParamEq {
    pub fn new() -> Self {
        Self {
            biquads: [Biquad::identity(); PARAM_EQ_BANDS],
        }
    }

    /// Process one audio sample through the 8-band cascade.  Each
    /// band's coefficients are refreshed on parameter drift; identity
    /// bands short-circuit to a pass-through.  `sr` is the engine
    /// sample rate at process time — used by the RBJ coefficient
    /// formulas and cached per-band to detect rate changes.
    pub fn process(&mut self, sig: f32, bands: &[ParamEqBand; PARAM_EQ_BANDS], sr: f32) -> f32 {
        let mut y = sig;
        for (bq, band) in self.biquads.iter_mut().zip(bands.iter()) {
            bq.refresh(band, sr);
            y = bq.step(y);
        }
        y
    }
}

impl Default for ParamEq {
    fn default() -> Self {
        Self::new()
    }
}
