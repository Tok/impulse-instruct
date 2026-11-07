// ─── audio/dsp/pitch_shift.rs ────────────────────────────────────────────────
// Bidirectional grain-based pitch shifter for the `FxStep::PitchShift`
// dispatch.
//
// Two-grain overlap-add PSOLA with explicit grain respawn.  Each
// grain has its own envelope phase (offset by half a period so their
// fadeouts alternate) and its own read position.  When a grain's
// envelope wraps 1 → 0 the grain is silent, so we respawn its read
// position one grain-length BEHIND the current write head.  Over the
// grain's full cycle the read walks from `(write - G)` to
// `(write - G) + G·ratio`, which stays in freshly-written audio for
// both upshift and downshift.
//
// Optional feedback: the last wet sample is mixed into the input at
// user-specified depth (clamped to avoid runaway) so stacked
// harmonies ladder up naturally when the user sets high mix + fbk.

/// Ring-buffer length — 16 384 samples = ~340 ms @ 48 kHz.
use super::dsp_util::MIX_BYPASS_THRESHOLD;
const PITCH_BUF: usize = 16_384;

/// Grain period — samples between envelope wraps.  2048 at 48 kHz is
/// ~43 ms, long enough to avoid audible flutter at moderate ratios
/// without so much latency that drag becomes noticeable.
const PITCH_GRAIN: f32 = 2048.0;

/// Max feedback coefficient — stops the wet ladder from blowing up
/// when mix is also high.
const FEEDBACK_MAX: f32 = 0.95;

pub struct PitchShift {
    buf: Vec<f32>,
    write: usize,
    /// Grain-1 read position (fractional sample index into buf).
    r1: f32,
    /// Grain-2 read position.
    r2: f32,
    /// Grain-1 envelope phase, 0..1 — triangular envelope respawns at 0.
    env1: f32,
    /// Grain-2 envelope phase, offset by 0.5 from `env1` so the
    /// triangular windows alternate their fade-in/fade-out peaks.
    env2: f32,
    /// Previous wet sample — fed back into the next call at `fbk`.
    last_wet: f32,
}

impl PitchShift {
    pub fn new() -> Self {
        Self {
            buf: vec![0.0; PITCH_BUF],
            write: 0,
            r1: 0.0,
            r2: 0.0,
            env1: 0.0,
            env2: 0.5,
            last_wet: 0.0,
        }
    }

    /// Process one sample.  `semi` + `cents` combine into the pitch
    /// ratio (both feed the same internal total).  `mix = 0` or zero
    /// total pitch offset bypasses the wet path; the ring buffer
    /// still advances so the first non-zero mix doesn't pick up
    /// stale audio.
    pub fn process(&mut self, input: f32, semi: f32, cents: f32, mix: f32, fbk: f32) -> f32 {
        let buf_len = self.buf.len();
        let buf_f = buf_len as f32;

        // Write every call so the ring stays current even in bypass.
        let feedback_in = input + self.last_wet * fbk.clamp(0.0, FEEDBACK_MAX);
        self.buf[self.write % buf_len] = feedback_in;
        self.write = self.write.wrapping_add(1);

        let total_semi = semi + cents * 0.01;
        if mix < MIX_BYPASS_THRESHOLD || total_semi.abs() < 0.005 {
            self.last_wet = 0.0;
            return input;
        }

        let clamped_semi = total_semi.clamp(-24.0, 24.0);
        let ratio = 2.0_f32.powf(clamped_semi / 12.0);

        // Advance grain phases.  Each grain re-spawns its read head
        // the moment its envelope wraps 1 → 0 (grain is silent at
        // that instant, so the discontinuity is masked).
        self.env1 += 1.0 / PITCH_GRAIN;
        self.env2 += 1.0 / PITCH_GRAIN;
        let write_f = self.write as f32;
        if self.env1 >= 1.0 {
            self.env1 -= 1.0;
            self.r1 = write_f - PITCH_GRAIN;
        }
        if self.env2 >= 1.0 {
            self.env2 -= 1.0;
            self.r2 = write_f - PITCH_GRAIN;
        }

        self.r1 += ratio;
        self.r2 += ratio;

        let read_sample = |buf: &[f32], pos: f32| -> f32 {
            // rem_euclid handles both negative and overflow positions
            // without branches; the ring buffer is toroidal so any
            // integer offset wraps cleanly.
            let pf = pos.rem_euclid(buf_f);
            let i0 = pf as usize;
            let i1 = (i0 + 1) % buf_len;
            let frac = pf - pf.floor();
            buf[i0] * (1.0 - frac) + buf[i1] * frac
        };
        let s1 = read_sample(&self.buf, self.r1);
        let s2 = read_sample(&self.buf, self.r2);

        // Triangular crossfade — each grain's weight is
        // 1 - |2 env - 1|, which sums to 1.0 across the half-period
        // offset of env1 and env2.
        let w1 = 1.0 - (2.0 * self.env1 - 1.0).abs();
        let w2 = 1.0 - (2.0 * self.env2 - 1.0).abs();
        let wet = s1 * w1 + s2 * w2;
        self.last_wet = wet;

        input * (1.0 - mix) + wet * mix
    }
}

impl Default for PitchShift {
    fn default() -> Self {
        Self::new()
    }
}
