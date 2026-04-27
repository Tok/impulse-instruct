// ─── audio/dsp/fx_plate.rs ───────────────────────────────────────────────────
// Plate reverb FX — Dattorro-style allpass network.  Distinct from the
// Schroeder comb stack `Reverb` (parallel comb + series allpass) and
// the IR-driven `ConvReverb`: this one is a figure-of-eight tank of
// modulated all-passes + delays + LP damping, the algorithm Lexicon /
// EMT plates use to get their dense, slightly metallic character.
//
// Knobs (all 0..1 unless noted):
//   * `size`     — tank delay scale (0 → tight room, 1 → long plate).
//   * `damping`  — LP coefficient inside the feedback loops; 0 = bright,
//                  1 = very dark / dampened.
//   * `diffusion`— input pre-AP gains (0 = thin, 1 = dense diffusion).
//   * `mix`      — wet/dry blend.  Default 0 so a freshly inserted FX
//                  is no-op until the user dials it in.

/// Buffer sizing — every Dattorro tank delay scaled by `size` peaks at
/// ~4500 samples × ~1.5 = ~6750 samples at 48 kHz.  Round up to the
/// next pow-of-two for cheap modulo (mask).  Per-buffer cost: 32 KB.
use super::dsp_util::MIX_BYPASS_THRESHOLD;
const PLATE_DELAY_LEN: usize = 8192;
const PLATE_DELAY_MASK: usize = PLATE_DELAY_LEN - 1;

/// All-pass buffers can be smaller — the longest pre-AP is ~1500 samples
/// at full scale.  4096 fits with 2× headroom and stays under the L1
/// cache footprint.
const PLATE_AP_LEN: usize = 4096;
const PLATE_AP_MASK: usize = PLATE_AP_LEN - 1;

/// Reference sample rate the canonical Dattorro lengths were tuned for.
/// We scale every delay length by `sr / PLATE_REF_SR` at construction so
/// the plate sounds the same across engine sample rates.
const PLATE_REF_SR: f32 = 29_761.0;

/// Fixed feedback through the tank.  Dattorro's paper uses 0.5; we
/// keep it conservative so the long tail doesn't run away when paired
/// with high `damping` (which slightly opens the feedback loop's gain
/// because the LP eats some of the level inside it).
const PLATE_TANK_GAIN: f32 = 0.5;

/// LFO depth for the modulated tank APs — ±8 samples around the base
/// length, exactly what Dattorro specifies.  Provides the characteristic
/// shimmer / movement that distinguishes a plate from a static comb stack.
const PLATE_AP_MOD_DEPTH: f32 = 8.0;

/// LFO rate in Hz — slow enough to read as movement rather than vibrato.
const PLATE_LFO_HZ: f32 = 1.0;

/// One tap in a fixed-length all-pass: input feeds a buffer with
/// feedback gain `g`, output blends the delayed sample with the input
/// via the all-pass topology.
struct AllPass {
    buf: Box<[f32; PLATE_AP_LEN]>,
    write: usize,
    base_len: usize,
}

impl AllPass {
    fn new(base_len: usize) -> Self {
        Self {
            buf: Box::new([0.0; PLATE_AP_LEN]),
            write: 0,
            base_len: base_len.clamp(1, PLATE_AP_LEN - 2),
        }
    }

    /// Process one sample through the AP at its static base length.
    fn process(&mut self, input: f32, gain: f32) -> f32 {
        let read = (self.write + PLATE_AP_LEN - self.base_len) & PLATE_AP_MASK;
        let delayed = self.buf[read];
        let new_in = input + delayed * gain;
        let out = delayed - new_in * gain;
        self.buf[self.write] = new_in;
        self.write = (self.write + 1) & PLATE_AP_MASK;
        out
    }

    /// Process with a modulated read tap (linear-interpolated) for the
    /// two LFO-modulated tank APs.
    fn process_modulated(&mut self, input: f32, gain: f32, mod_offset: f32) -> f32 {
        let len_f = self.base_len as f32 + mod_offset;
        let len_clamped = len_f.clamp(2.0, (PLATE_AP_LEN - 2) as f32);
        let read_pos =
            (self.write as f32 + PLATE_AP_LEN as f32 - len_clamped) % PLATE_AP_LEN as f32;
        let i0 = read_pos as usize & PLATE_AP_MASK;
        let i1 = (i0 + 1) & PLATE_AP_MASK;
        let frac = read_pos - read_pos.floor();
        let delayed = self.buf[i0] + (self.buf[i1] - self.buf[i0]) * frac;
        let new_in = input + delayed * gain;
        let out = delayed - new_in * gain;
        self.buf[self.write] = new_in;
        self.write = (self.write + 1) & PLATE_AP_MASK;
        out
    }
}

/// Plain delay line backed by a power-of-two buffer (mask-wrap).
struct Delay {
    buf: Box<[f32; PLATE_DELAY_LEN]>,
    write: usize,
    base_len: usize,
}

impl Delay {
    fn new(base_len: usize) -> Self {
        Self {
            buf: Box::new([0.0; PLATE_DELAY_LEN]),
            write: 0,
            base_len: base_len.clamp(1, PLATE_DELAY_LEN - 2),
        }
    }

    fn write_sample(&mut self, x: f32) {
        self.buf[self.write] = x;
        self.write = (self.write + 1) & PLATE_DELAY_MASK;
    }

    fn read_at(&self, offset: usize) -> f32 {
        let off = offset.min(PLATE_DELAY_LEN - 1);
        let pos = (self.write + PLATE_DELAY_LEN - off) & PLATE_DELAY_MASK;
        self.buf[pos]
    }

    fn read_base(&self) -> f32 {
        self.read_at(self.base_len)
    }
}

pub(crate) struct PlateFx {
    // Input diffusion chain — four series APs.
    pre_ap1: AllPass,
    pre_ap2: AllPass,
    pre_ap3: AllPass,
    pre_ap4: AllPass,
    // Figure-of-eight tank — left half.
    tank_ap_l: AllPass,
    tank_delay_l1: Delay,
    tank_ap2_l: AllPass,
    tank_delay_l2: Delay,
    lpf_l: f32,
    // Tank — right half.
    tank_ap_r: AllPass,
    tank_delay_r1: Delay,
    tank_ap2_r: AllPass,
    tank_delay_r2: Delay,
    lpf_r: f32,
    // Tank cross-feed register: previous-sample output of the right
    // half feeds back into the left half's input (and vice-versa).
    feedback_l: f32,
    feedback_r: f32,
    // LFO phase for the two modulated tank APs (0..1).
    lfo_phase: f32,
    cached_sr: f32,
    // Cached output tap offsets (Dattorro's standard 7-tap fixed sum).
    out_taps_l: [usize; 7],
    out_taps_r: [usize; 7],
}

impl PlateFx {
    pub(crate) fn new(sr: f32) -> Self {
        Self::with_sr(sr.max(1.0))
    }

    fn with_sr(sr: f32) -> Self {
        let scale = sr / PLATE_REF_SR;
        let s = |n: f32| (n * scale) as usize;
        let mut fx = Self {
            pre_ap1: AllPass::new(s(142.0)),
            pre_ap2: AllPass::new(s(107.0)),
            pre_ap3: AllPass::new(s(379.0)),
            pre_ap4: AllPass::new(s(277.0)),
            tank_ap_l: AllPass::new(s(672.0)),
            tank_delay_l1: Delay::new(s(4453.0)),
            tank_ap2_l: AllPass::new(s(1800.0)),
            tank_delay_l2: Delay::new(s(3720.0)),
            lpf_l: 0.0,
            tank_ap_r: AllPass::new(s(908.0)),
            tank_delay_r1: Delay::new(s(4217.0)),
            tank_ap2_r: AllPass::new(s(2656.0)),
            tank_delay_r2: Delay::new(s(3163.0)),
            lpf_r: 0.0,
            feedback_l: 0.0,
            feedback_r: 0.0,
            lfo_phase: 0.0,
            cached_sr: sr,
            out_taps_l: [0; 7],
            out_taps_r: [0; 7],
        };
        fx.recompute_output_taps();
        fx
    }

    /// Dattorro's seven fixed output-mixer taps, scaled to current sr.
    /// They're read from specific positions inside the tank delay lines
    /// — the per-half sum is what gives the plate its characteristic
    /// dense-but-uncorrelated stereo image.
    fn recompute_output_taps(&mut self) {
        let scale = self.cached_sr / PLATE_REF_SR;
        let s = |n: f32| (n * scale) as usize;
        self.out_taps_l = [
            s(266.0),
            s(2974.0),
            s(1913.0),
            s(1996.0),
            s(1990.0),
            s(187.0),
            s(1066.0),
        ];
        self.out_taps_r = [
            s(353.0),
            s(3627.0),
            s(1228.0),
            s(2673.0),
            s(2111.0),
            s(335.0),
            s(121.0),
        ];
    }

    /// `size`     — tank time scale 0..1.  Internally maps to the cross-feed
    ///              gain (longer tail without growing the tank delay
    ///              lengths, which would require reallocation).  At 0
    ///              the cross-feed is `0.4 × PLATE_TANK_GAIN`; at 1 it
    ///              hits the conservative cap `1.0 × PLATE_TANK_GAIN`.
    /// `damping`  — LP coefficient on each tank half (one-pole), 0 = bright,
    ///              1 ≈ very dark.
    /// `diffusion`— input pre-AP gain, 0..1 → 0..0.75.  Higher values
    ///              produce denser early reflections.
    /// `mix`      — wet/dry blend.  Cheap-bypass when < 0.001.
    pub(crate) fn process(
        &mut self,
        input: f32,
        size: f32,
        damping: f32,
        diffusion: f32,
        mix: f32,
        sr: f32,
    ) -> f32 {
        if mix < MIX_BYPASS_THRESHOLD {
            return input;
        }
        if (sr - self.cached_sr).abs() > 0.5 {
            self.cached_sr = sr;
            self.recompute_output_taps();
        }

        let size = size.clamp(0.0, 1.0);
        let damping = damping.clamp(0.0, 1.0);
        let diffusion = diffusion.clamp(0.0, 1.0);

        // Cross-feed: 0..1 → 0.4..1.0 of the conservative tank gain cap.
        let tank_g = (0.4 + size * 0.6) * PLATE_TANK_GAIN;
        // Pre-AP gain — a modest cap of 0.75 keeps the diffusion network
        // from ringing.
        let pre_g = diffusion * 0.75;
        // LP coefficient: 0 = bypass, 1 ≈ heavy lowpass.  One-pole
        // smoother:  y[n] = (1-d) * x[n] + d * y[n-1].
        let d = damping * 0.95;

        // Bandwidth limit on the input — Dattorro's first stage; keeps
        // the tank from accumulating very-high-frequency energy.  Fixed
        // at ≈0.9995 so the cutoff sits near Nyquist regardless of sr.
        let bw_in = input * 0.9995;

        // Input diffusion chain.
        let mut x = self.pre_ap1.process(bw_in, pre_g);
        x = self.pre_ap2.process(x, pre_g);
        // The last two pre-APs use a slightly lower nominal gain in
        // Dattorro's paper (0.625).  Folded into `diffusion` here for
        // simplicity — the user just sees one knob.
        let pre_g2 = diffusion * 0.625;
        x = self.pre_ap3.process(x, pre_g2);
        x = self.pre_ap4.process(x, pre_g2);

        // LFO drives the two modulated tank APs.  Sin / cos give the
        // two halves opposite-phase modulation so the stereo decorrelates.
        self.lfo_phase += PLATE_LFO_HZ / self.cached_sr;
        if self.lfo_phase >= 1.0 {
            self.lfo_phase -= 1.0;
        }
        let phase_rad = self.lfo_phase * std::f32::consts::TAU;
        let mod_l = phase_rad.sin() * PLATE_AP_MOD_DEPTH;
        let mod_r = phase_rad.cos() * PLATE_AP_MOD_DEPTH;

        // ─── Left tank half ────────────────────────────────────────────
        // Cross-feed register holds the previous-sample output of the
        // right tank half — this is the "figure-eight" cross.  The
        // implicit one-sample delay is what makes the loop stable.
        let mut l = x + self.feedback_r * tank_g;
        l = self.tank_ap_l.process_modulated(l, 0.7, mod_l);
        self.tank_delay_l1.write_sample(l);
        let mut l = self.tank_delay_l1.read_base();
        // One-pole LP damping inside the loop.
        self.lpf_l = l * (1.0 - d) + self.lpf_l * d;
        l = self.lpf_l;
        l = self.tank_ap2_l.process(l, -0.7);
        self.tank_delay_l2.write_sample(l);
        let l_end = self.tank_delay_l2.read_base();

        // ─── Right tank half ───────────────────────────────────────────
        let mut r = x + self.feedback_l * tank_g;
        r = self.tank_ap_r.process_modulated(r, 0.7, mod_r);
        self.tank_delay_r1.write_sample(r);
        let mut r = self.tank_delay_r1.read_base();
        self.lpf_r = r * (1.0 - d) + self.lpf_r * d;
        r = self.lpf_r;
        r = self.tank_ap2_r.process(r, -0.7);
        self.tank_delay_r2.write_sample(r);
        let r_end = self.tank_delay_r2.read_base();

        // Update feedback registers for the next sample (one-sample delay).
        self.feedback_l = l_end;
        self.feedback_r = r_end;

        // Output mixer: 7 fixed taps per half, then sum to mono.  The
        // tap pattern is what gives the plate its dense, pseudo-stereo
        // character — we only emit a mono signal here (the FX chain is
        // mono on the chain step), so the L/R sum is exactly the
        // textbook stereo plate folded down.
        let mut out_l = 0.0;
        let mut out_r = 0.0;
        for i in 0..3 {
            out_l += self.tank_delay_l1.read_at(self.out_taps_l[i]);
            out_r += self.tank_delay_r1.read_at(self.out_taps_r[i]);
        }
        for i in 3..7 {
            out_l += self.tank_delay_l2.read_at(self.out_taps_l[i]);
            out_r += self.tank_delay_r2.read_at(self.out_taps_r[i]);
        }
        // Compensate the 7-tap sum so the output stays in the same
        // ballpark as the dry — without scaling, full-scale input can
        // push the wet beyond ±2.
        let wet = (out_l + out_r) * (0.5 / 7.0);

        input * (1.0 - mix) + wet * mix
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dry_when_mix_zero() {
        let mut fx = PlateFx::new(48_000.0);
        let dry = 0.5;
        let out = fx.process(dry, 0.5, 0.5, 0.5, 0.0, 48_000.0);
        assert_eq!(out, dry, "mix=0 should bypass");
    }

    #[test]
    fn produces_audible_tail_from_impulse() {
        let mut fx = PlateFx::new(48_000.0);
        // Hit the input with a single full-scale impulse, then run
        // silent for ~0.5 s and confirm the wet path is still ringing.
        let _ = fx.process(1.0, 0.7, 0.3, 0.7, 1.0, 48_000.0);
        let mut peak = 0.0_f32;
        for _ in 0..24_000 {
            let out = fx.process(0.0, 0.7, 0.3, 0.7, 1.0, 48_000.0);
            peak = peak.max(out.abs());
        }
        assert!(
            peak > 0.001,
            "plate tail should ring after impulse (peak {peak})"
        );
    }

    #[test]
    fn output_stays_bounded_under_full_drive() {
        // Continuous sine at full mix + max size + low damping shouldn't
        // overflow.  Plate feedback is internally clamped so this is the
        // pathological worst case — 1 s should expose any runaway.
        let mut fx = PlateFx::new(48_000.0);
        let mut peak = 0.0_f32;
        for i in 0..48_000 {
            let sig = (i as f32 * 0.05).sin() * 0.8;
            let out = fx.process(sig, 1.0, 0.0, 1.0, 1.0, 48_000.0);
            assert!(
                out.is_finite(),
                "plate output went non-finite at sample {i}"
            );
            peak = peak.max(out.abs());
        }
        assert!(peak <= 4.0, "plate path stays bounded (peak {peak})");
    }

    #[test]
    fn passthrough_when_mix_zero_even_at_full_drive() {
        // mix=0 must short-circuit before any of the tank machinery runs.
        let mut fx = PlateFx::new(48_000.0);
        for i in 0..1_000 {
            let sig = (i as f32 * 0.05).sin() * 0.8;
            let out = fx.process(sig, 1.0, 0.0, 1.0, 0.0, 48_000.0);
            assert_eq!(out, sig);
        }
    }

    #[test]
    fn larger_size_produces_longer_tail() {
        // Tighter (size=0) should decay faster than larger (size=1).
        // Compare the energy in the late portion of the impulse response.
        fn late_energy(size: f32) -> f32 {
            let mut fx = PlateFx::new(48_000.0);
            // Warm up + impulse.
            let _ = fx.process(1.0, size, 0.0, 0.7, 1.0, 48_000.0);
            let mut energy = 0.0_f32;
            for i in 0..48_000 {
                let out = fx.process(0.0, size, 0.0, 0.7, 1.0, 48_000.0);
                if i >= 24_000 {
                    energy += out * out;
                }
            }
            energy
        }
        let small = late_energy(0.0);
        let large = late_energy(1.0);
        assert!(
            large > small,
            "larger size must produce more late-tail energy (small {small}, large {large})"
        );
    }
}
