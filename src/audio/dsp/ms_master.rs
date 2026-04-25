// ─── audio/dsp/ms_master.rs ──────────────────────────────────────────────────
// Mid/Side master-bus processor.
//
// Sits after the raw mid/side computation in the master stage.  For
// each side:
//   • gain      — ±12 dB trim (0.5 knob = unity)
//   • tilt EQ   — pair of opposing shelves at 200 Hz / 5 kHz, gain tied
//                 to the tilt knob so -1 (0.0 knob) tilts bass-heavy
//                 and +1 (1.0 knob) tilts treble-heavy.  A classic
//                 mastering tilt in ~12 lines of biquad state.
//   • saturator — arctan soft-clip, drive tied to `sat` 0..1.
//
// Coefficient recompute is dirty-checked per band so the hot path
// stays `biquad.step + atan + mul` per sample per side (six biquads
// total — two per side — plus two `atan` calls).  At 48 kHz that's
// well under a percent of DSP budget on modern CPUs.

use super::param_eq::biquad_coeffs;
use crate::state::ParamEqBandKind;

const MS_LOW_FREQ: f32 = 200.0;
const MS_HIGH_FREQ: f32 = 5_000.0;
const MS_SHELF_Q: f32 = 0.7;
const MS_TILT_MAX_DB: f32 = 6.0;
const MS_GAIN_MAX_DB: f32 = 12.0;

#[derive(Clone, Copy, Debug)]
struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    z1: f32,
    z2: f32,
    cached_kind: u8,
    cached_freq: f32,
    cached_gain: f32,
    cached_q: f32,
    cached_sr: f32,
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
            cached_kind: 255,
            cached_freq: 0.0,
            cached_gain: 0.0,
            cached_q: 0.0,
            cached_sr: 0.0,
            bypass: true,
        }
    }

    #[inline]
    fn step(&mut self, x: f32) -> f32 {
        if self.bypass {
            return x;
        }
        let y = self.b0 * x + self.z1;
        self.z1 = self.b1 * x + self.z2 - self.a1 * y;
        self.z2 = self.b2 * x - self.a2 * y;
        y
    }

    fn refresh(&mut self, kind: ParamEqBandKind, freq: f32, gain_db: f32, q: f32, sr: f32) {
        let kind_u8 = kind.to_u8();
        if self.cached_kind == kind_u8
            && self.cached_freq == freq
            && self.cached_gain == gain_db
            && self.cached_q == q
            && self.cached_sr == sr
        {
            return;
        }
        self.cached_kind = kind_u8;
        self.cached_freq = freq;
        self.cached_gain = gain_db;
        self.cached_q = q;
        self.cached_sr = sr;
        if gain_db.abs() < 1e-3 {
            self.b0 = 1.0;
            self.b1 = 0.0;
            self.b2 = 0.0;
            self.a1 = 0.0;
            self.a2 = 0.0;
            self.bypass = true;
            return;
        }
        let (b0, b1, b2, a0, a1, a2) = biquad_coeffs(kind, freq, gain_db, q, sr);
        let inv_a0 = 1.0 / a0;
        self.b0 = b0 * inv_a0;
        self.b1 = b1 * inv_a0;
        self.b2 = b2 * inv_a0;
        self.a1 = a1 * inv_a0;
        self.a2 = a2 * inv_a0;
        self.bypass = false;
    }
}

/// Snapshot of all mid/side knobs passed per-sample.  Kept as a tiny
/// value-type so the audio callback copies instead of borrowing.
#[derive(Clone, Copy, Debug)]
pub struct MsMasterParams {
    pub mid_gain: f32,
    pub mid_tilt: f32,
    pub mid_sat: f32,
    pub side_gain: f32,
    pub side_tilt: f32,
    pub side_sat: f32,
}

impl MsMasterParams {
    /// Flat defaults — 0.5 on gain/tilt maps to unity / no-tilt;
    /// 0 on saturation = off.
    pub fn flat() -> Self {
        Self {
            mid_gain: 0.5,
            mid_tilt: 0.5,
            mid_sat: 0.0,
            side_gain: 0.5,
            side_tilt: 0.5,
            side_sat: 0.0,
        }
    }
}

pub struct MsMaster {
    mid_lo: Biquad,
    mid_hi: Biquad,
    side_lo: Biquad,
    side_hi: Biquad,
}

impl MsMaster {
    pub fn new() -> Self {
        Self {
            mid_lo: Biquad::identity(),
            mid_hi: Biquad::identity(),
            side_lo: Biquad::identity(),
            side_hi: Biquad::identity(),
        }
    }

    /// Process one (mid, side) pair.  `sr` is the engine sample rate,
    /// passed per-call so the coefficient cache can detect rate
    /// changes.  Returns the tonally-shaped, saturated pair — the
    /// caller is responsible for the mid±side → L/R recombination.
    pub fn process(&mut self, mid: f32, side: f32, sr: f32, p: MsMasterParams) -> (f32, f32) {
        let mid_gain_lin = knob_to_gain_lin(p.mid_gain);
        let side_gain_lin = knob_to_gain_lin(p.side_gain);

        // Tilt pair: low-shelf gain is the tilt's inverse, high-shelf
        // gain is the tilt's direct value.  At tilt=0.5 both collapse
        // to 0 dB so the biquads enter their fast-bypass branch.
        let mid_tilt = (p.mid_tilt - 0.5) * 2.0 * MS_TILT_MAX_DB;
        let side_tilt = (p.side_tilt - 0.5) * 2.0 * MS_TILT_MAX_DB;

        self.mid_lo.refresh(
            ParamEqBandKind::LowShelf,
            MS_LOW_FREQ,
            -mid_tilt,
            MS_SHELF_Q,
            sr,
        );
        self.mid_hi.refresh(
            ParamEqBandKind::HighShelf,
            MS_HIGH_FREQ,
            mid_tilt,
            MS_SHELF_Q,
            sr,
        );
        self.side_lo.refresh(
            ParamEqBandKind::LowShelf,
            MS_LOW_FREQ,
            -side_tilt,
            MS_SHELF_Q,
            sr,
        );
        self.side_hi.refresh(
            ParamEqBandKind::HighShelf,
            MS_HIGH_FREQ,
            side_tilt,
            MS_SHELF_Q,
            sr,
        );

        // Gain first so subsequent shelf/saturation see the scaled
        // signal — matches the order a hardware mastering chain usually
        // wires (input trim → EQ → saturator).
        let mut m = mid * mid_gain_lin;
        let mut s = side * side_gain_lin;
        m = self.mid_lo.step(m);
        m = self.mid_hi.step(m);
        s = self.side_lo.step(s);
        s = self.side_hi.step(s);
        m = soft_clip(m, p.mid_sat);
        s = soft_clip(s, p.side_sat);
        (m, s)
    }
}

impl Default for MsMaster {
    fn default() -> Self {
        Self::new()
    }
}

/// Map the 0..1 gain knob into a linear multiplier covering
/// ±`MS_GAIN_MAX_DB` dB around unity at 0.5.
fn knob_to_gain_lin(knob: f32) -> f32 {
    let db = (knob.clamp(0.0, 1.0) - 0.5) * 2.0 * MS_GAIN_MAX_DB;
    10.0_f32.powf(db / 20.0)
}

/// Arctan soft-clip with strict ±1 output bound.  `sat ∈ [0, 1]`;
/// `0` is a bypass (no arctan call).  At low sat the curve is
/// essentially linear; at high sat mid-range values get boosted
/// toward unity while any over-driven input lands inside ±1 — so
/// the downstream master clamp never hits the saturator's output.
pub fn soft_clip(x: f32, sat: f32) -> f32 {
    let sat = sat.clamp(0.0, 1.0);
    if sat < 1e-4 {
        return x;
    }
    // drive 1..5 over sat 0..1 — keeps low-sat subtle (near-linear
    // for |x| ≤ 1) and high-sat limiter-like.  Output is scaled by
    // 2/π so the atan's asymptotic ±π/2 maps to strict ±1.
    let drive = 1.0 + sat * 4.0;
    (x * drive).atan() * (std::f32::consts::FRAC_2_PI)
}
