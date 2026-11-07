use super::dsp_util::nyquist_guard;
use super::dsp_util::{AUDIBLE_HZ_MIN, MIX_BYPASS_THRESHOLD};
// ─── audio/dsp/fx_djfilter.rs ────────────────────────────────────────────────
// DJ filter — single-knob morph LP↔HP through BP with the classic
// resonance peak at the crossover.  Live-friendly one-knob FX from
// the wishlist.
//
// One state-variable filter integrator — every sample we compute the
// LP / BP / HP components in lock-step and crossfade between them
// using triangular weights that peak at the morph midpoint:
//   morph 0.0 → pure LP (low cutoff)
//   morph 0.5 → pure BP (mid cutoff, resonance peak)
//   morph 1.0 → pure HP (high cutoff)
//
// The cutoff sweeps log-symmetrically with morph so that morph=0
// reads as "filter heavy on the low side" and morph=1 reads as
// "filter heavy on the high side", the way real DJ-mixer EQ kills
// behave.  Resonance Q gets a φ-bigger boost at the morph midpoint
// where the BP component dominates, so the user hears the peak
// emerge as they sweep through the centre.

pub(crate) struct DjFilter {
    /// Bandpass integrator state (shared with the LP computation).
    band: f32,
    /// Lowpass integrator state.
    low: f32,
}

impl DjFilter {
    pub(crate) fn new() -> Self {
        Self {
            band: 0.0,
            low: 0.0,
        }
    }

    /// `morph`: 0..1 — 0=LP-heavy, 0.5=BP-with-peak, 1=HP-heavy.
    /// `resonance`: 0..1 → base Q (peaks higher at morph midpoint).
    /// `mix`: 0..1 wet/dry.
    pub(crate) fn process(
        &mut self,
        input: f32,
        morph: f32,
        resonance: f32,
        mix: f32,
        sr: f32,
    ) -> f32 {
        if mix < MIX_BYPASS_THRESHOLD {
            return input;
        }
        let m = morph.clamp(0.0, 1.0);
        // Cutoff sweep: 80 Hz → 1 kHz → 8 kHz log-symmetric.  Two
        // equal log-octaves so morph=0.5 lands exactly at 1 kHz
        // regardless of resonance.  Heavy on the low end at morph=0
        // (everything but the bass cut), heavy on the high end at
        // morph=1 (everything but the air cut).
        let fc = 80.0 * 100.0_f32.powf(m).clamp(AUDIBLE_HZ_MIN, nyquist_guard(sr));
        // Resonance peak emerges at the morph midpoint where the BP
        // weight dominates.  `bp_emphasis` is triangular 0..1 with
        // its peak at m=0.5; multiplied into the user's resonance so
        // the LP/HP edges still respond to the knob, just less
        // dramatically.
        let bp_emphasis = 1.0 - (2.0 * m - 1.0).abs();
        let q = 0.5 + resonance.clamp(0.0, 1.0) * (4.0 + bp_emphasis * 12.0);
        let damp = (1.0 / q).min(2.0 - 1e-3);

        // 2× oversample for stability at high cutoff (same trick as
        // the existing Svf — the integrator can ring at sr/4 without
        // it).  We compute LP / BP / HP every sub-sample but only
        // crossfade them once at the end.
        let f = 2.0 * (std::f32::consts::PI * fc / (sr * 2.0)).sin();
        let mut low_out = 0.0_f32;
        let mut band_out = 0.0_f32;
        let mut high_out = 0.0_f32;
        for _ in 0..2 {
            self.low += f * self.band;
            let high = input - self.low - damp * self.band;
            self.band += f * high;
            low_out = self.low;
            band_out = self.band;
            high_out = high;
        }

        // Triangular crossfade weights:
        //   m=0   → (1, 0, 0) — pure LP
        //   m=0.5 → (0, 1, 0) — pure BP
        //   m=1   → (0, 0, 1) — pure HP
        let w_low = (1.0 - 2.0 * m).max(0.0);
        let w_high = (2.0 * m - 1.0).max(0.0);
        let w_band = 1.0 - (2.0 * m - 1.0).abs();
        let wet = low_out * w_low + band_out * w_band + high_out * w_high;
        input * (1.0 - mix) + wet * mix
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Drive a sine at `freq_hz` through the filter for `n` samples and
    /// return the peak absolute output (after warm-up).
    fn peak_response(filter: &mut DjFilter, freq_hz: f32, morph: f32, sr: f32) -> f32 {
        // Warm-up: let the integrator settle.
        let two_pi = std::f32::consts::TAU;
        for i in 0..1024 {
            let t = i as f32 / sr;
            let x = (two_pi * freq_hz * t).sin() * 0.5;
            let _ = filter.process(x, morph, 0.3, 1.0, sr);
        }
        let mut peak = 0.0_f32;
        for i in 0..2048 {
            let t = (i + 1024) as f32 / sr;
            let x = (two_pi * freq_hz * t).sin() * 0.5;
            let out = filter.process(x, morph, 0.3, 1.0, sr);
            peak = peak.max(out.abs());
        }
        peak
    }

    #[test]
    fn dry_when_mix_zero() {
        let mut f = DjFilter::new();
        let out = f.process(0.5, 0.5, 1.0, 0.0, 48_000.0);
        assert_eq!(out, 0.5, "mix=0 must bypass exactly");
    }

    #[test]
    fn morph_zero_is_lowpass() {
        // At morph=0 the cutoff is ~80 Hz so a 50 Hz sine passes
        // (mostly) and a 5 kHz sine is heavily attenuated.  We
        // can't hit zero exactly with a 1-pole-ish SVF tail, but
        // the LP region must show meaningful attenuation up high.
        let mut f = DjFilter::new();
        let lo = peak_response(&mut f, 50.0, 0.0, 48_000.0);
        let mut g = DjFilter::new();
        let hi = peak_response(&mut g, 5_000.0, 0.0, 48_000.0);
        assert!(
            lo > hi * 5.0,
            "LP at morph=0: low band should dwarf high band — lo={lo}, hi={hi}"
        );
    }

    #[test]
    fn morph_one_is_highpass() {
        // Mirror image: morph=1, cutoff ~8 kHz — high frequencies
        // pass, low frequencies attenuated.
        let mut f = DjFilter::new();
        let lo = peak_response(&mut f, 50.0, 1.0, 48_000.0);
        let mut g = DjFilter::new();
        let hi = peak_response(&mut g, 12_000.0, 1.0, 48_000.0);
        assert!(
            hi > lo * 5.0,
            "HP at morph=1: high band should dwarf low band — lo={lo}, hi={hi}"
        );
    }

    #[test]
    fn morph_half_peaks_at_centre_frequency() {
        // At morph=0.5 the cutoff lands at ~1 kHz with the BP
        // resonance peak.  A 1 kHz sine should come through louder
        // than a 50 Hz sine (LP edge) or a 12 kHz sine (HP edge).
        let mut f = DjFilter::new();
        let mid = peak_response(&mut f, 1_000.0, 0.5, 48_000.0);
        let mut g = DjFilter::new();
        let lo = peak_response(&mut g, 50.0, 0.5, 48_000.0);
        let mut h = DjFilter::new();
        let hi = peak_response(&mut h, 12_000.0, 0.5, 48_000.0);
        assert!(
            mid > lo && mid > hi,
            "BP at morph=0.5: 1 kHz peak — mid={mid}, lo={lo}, hi={hi}"
        );
    }

    #[test]
    fn high_resonance_at_midpoint_narrows_the_passband() {
        // The "resonance peak at the crossover" claim: with max
        // resonance + morph=0.5, the BP centred at ~1 kHz must
        // pass tones near the centre at close to unity gain while
        // attenuating tones far away — i.e. the BP is narrow.
        // Steady-state measurement (long warm-up) so transient
        // ringing of the integrator doesn't pollute the peak.
        let two_pi = std::f32::consts::TAU;
        let sr = 48_000.0;
        let warmup = 8_000;
        let measure = 2_000;

        let mut centre = DjFilter::new();
        let mut peak_centre = 0.0_f32;
        for i in 0..(warmup + measure) {
            let t = i as f32 / sr;
            let x = (two_pi * 1_000.0 * t).sin() * 0.3;
            let out = centre.process(x, 0.5, 1.0, 1.0, sr);
            if i >= warmup {
                peak_centre = peak_centre.max(out.abs());
            }
        }

        let mut off = DjFilter::new();
        let mut peak_off = 0.0_f32;
        for i in 0..(warmup + measure) {
            let t = i as f32 / sr;
            // 100 Hz — well below the BP centre at 1 kHz, even at
            // Q=16.5 it's outside the narrow passband.
            let x = (two_pi * 100.0 * t).sin() * 0.3;
            let out = off.process(x, 0.5, 1.0, 1.0, sr);
            if i >= warmup {
                peak_off = peak_off.max(out.abs());
            }
        }

        assert!(
            peak_centre > peak_off * 3.0,
            "BP narrows at midpoint: centre={peak_centre}, off={peak_off}"
        );
    }

    #[test]
    fn output_stays_bounded_under_full_signal() {
        let mut f = DjFilter::new();
        let two_pi = std::f32::consts::TAU;
        let sr = 48_000.0;
        let mut peak = 0.0_f32;
        // Sweep morph through the full range with full resonance
        // and full mix on a strong sine — output must stay finite
        // and well-bounded.
        for i in 0..16_000 {
            let t = i as f32 / sr;
            let x = (two_pi * 440.0 * t).sin() * 0.8;
            let m = (i as f32 / 16_000.0).clamp(0.0, 1.0);
            let out = f.process(x, m, 1.0, 1.0, sr);
            assert!(out.is_finite(), "non-finite at sample {i}: {out}");
            peak = peak.max(out.abs());
        }
        assert!(peak < 8.0, "DJ filter peak runaway: {peak}");
    }
}
