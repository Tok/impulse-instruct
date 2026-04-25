// ─── audio/dsp/fx_extras.rs ──────────────────────────────────────────────────
// Tier-1 FX structs split out of `fx.rs` to keep that file under the
// 1000-line cap.  Same module-private exposure (`pub(crate)`) as the
// originals; `pub use fx_extras::*` in `audio/dsp/mod.rs` keeps the
// existing `use fx::*;` import sites unchanged.
//
// Structs in this file: Flanger, Limiter, Svf, CombRes, Tilt, Transient,
// Exciter.  All allocation-free in `process()`; ring buffers that are
// too large for the stack are `Box<[f32; N]>`-allocated once at
// construction.

use std::sync::Arc;

use rustfft::{Fft, FftPlanner, num_complex::Complex};

use super::fx::{Biquad, MAX_FLANGER_SIZE};

// ─── Flanger (short modulated delay with feedback) ───────────────────────────
//
// Sibling to the phaser but voiced by a comb filter (a short delay line with
// feedback).  The delay is swept by a low-rate LFO between ~0.5 ms and ~10 ms,
// producing the characteristic moving comb-notch series.  Negative feedback
// inverts the comb (notches become peaks) for the metallic / through-zero
// flavour.
//
// Stack-sized [f32; MAX_FLANGER_SIZE] keeps process() allocation-free.

pub(crate) struct Flanger {
    pub(crate) buf: [f32; MAX_FLANGER_SIZE],
    pub(crate) write: usize,
    pub(crate) lfo_phase: f32,
}

impl Flanger {
    pub(crate) fn new() -> Self {
        Self {
            buf: [0.0; MAX_FLANGER_SIZE],
            write: 0,
            lfo_phase: 0.0,
        }
    }

    /// `rate`: 0–1 → 0.05–4 Hz LFO rate.
    /// `depth`: 0–1 → sweep range up to ~9 ms around a 1 ms base.
    /// `feedback`: 0–1 → −0.95..+0.95 (centred at 0.5 = no feedback).
    /// `mix`: 0–1 wet/dry.
    pub(crate) fn process(
        &mut self,
        input: f32,
        rate: f32,
        depth: f32,
        feedback: f32,
        mix: f32,
        sr: f32,
    ) -> f32 {
        if mix < 0.001 && (feedback - 0.5).abs() < 0.001 {
            // Pure bypass when no wet and no feedback — still has to advance
            // the write head so the ring tracks `input` for when mix rises.
            self.buf[self.write] = input;
            self.write = (self.write + 1) & (MAX_FLANGER_SIZE - 1);
            return input;
        }

        let rate_hz = 0.05 + rate * 3.95;
        self.lfo_phase += rate_hz / sr;
        if self.lfo_phase >= 1.0 {
            self.lfo_phase -= 1.0;
        }

        // Triangle would also work, sine is smoother and matches the phaser.
        let lfo = 0.5 - 0.5 * (self.lfo_phase * std::f32::consts::TAU).cos();

        // Base 1 ms, sweep up to ~9 ms more.  Cap at MAX_FLANGER_SIZE−2 so
        // the linear interpolation never reads past the end of the ring.
        let base = (0.001 * sr).max(1.0);
        let sweep_max = (depth.clamp(0.0, 1.0) * 0.009 * sr).max(0.0);
        let delay = (base + lfo * sweep_max).clamp(1.0, (MAX_FLANGER_SIZE - 2) as f32);

        let read_pos = self.write as f32 + MAX_FLANGER_SIZE as f32 - delay;
        let idx = read_pos as usize & (MAX_FLANGER_SIZE - 1);
        let frac = read_pos - read_pos.floor();
        let next = (idx + 1) & (MAX_FLANGER_SIZE - 1);
        let delayed = self.buf[idx] + (self.buf[next] - self.buf[idx]) * frac;

        // 0.5-centred knob → ±0.95 signed feedback.  Clamp the absolute
        // amount just under unity to keep the comb stable.
        let fb = ((feedback.clamp(0.0, 1.0) - 0.5) * 1.9).clamp(-0.95, 0.95);
        let to_buf = input + delayed * fb;
        self.buf[self.write] = to_buf;
        self.write = (self.write + 1) & (MAX_FLANGER_SIZE - 1);

        input * (1.0 - mix) + delayed * mix
    }
}

// ─── Brick-wall lookahead limiter ─────────────────────────────────────────────
//
// Peak limiter with a fixed-size lookahead ring.  Reads the future N samples,
// computes the running peak, and ramps the gain *down* before the peak hits
// the output — so the actual ceiling is never overshot.  Release ramps the
// gain back up between peaks.  As a final safety net, the output is hard-
// clipped at the user ceiling, which is inaudible in normal operation but
// catches numerical edge cases.
//
// `threshold`: 0–1 → −24..0 dB (where limiting kicks in).
// `ceiling`:   0–1 → −12..0 dB (absolute output ceiling).
// `release`:   0–1 → 5–500 ms recovery time.
// `lookahead`: 0–1 → 0.5–10 ms peek window.

const LIMITER_BUF: usize = 1024; // ~21 ms @ 48 kHz — heap-allocated at construction

pub(crate) struct Limiter {
    /// Pre-allocated lookahead buffer; size never changes after `new()`.
    buf: Box<[f32; LIMITER_BUF]>,
    write: usize,
    /// Smoothed gain reduction in linear units (1.0 = no reduction).
    gain: f32,
    /// Running max of |x| across the lookahead window — recomputed lazily so
    /// we don't scan every sample (the running window is cheap to maintain
    /// since the buffer is small).
    win_peak: f32,
}

impl Limiter {
    pub(crate) fn new() -> Self {
        Self {
            buf: Box::new([0.0; LIMITER_BUF]),
            write: 0,
            gain: 1.0,
            win_peak: 0.0,
        }
    }

    pub(crate) fn process(
        &mut self,
        input: f32,
        threshold: f32,
        ceiling: f32,
        release: f32,
        lookahead: f32,
        sr: f32,
    ) -> f32 {
        let look_s = ((0.0005 + lookahead.clamp(0.0, 1.0) * 0.0095) * sr) as usize;
        let look_s = look_s.clamp(2, LIMITER_BUF - 2);
        // Threshold 0..1 → −24..0 dB.  Ceiling 0..1 → −12..0 dB.
        let thr_lin = 10.0f32.powf((-24.0 * (1.0 - threshold)) / 20.0);
        let ceil_lin = 10.0f32.powf((-12.0 * (1.0 - ceiling)) / 20.0);
        let rel_s = (0.005 + release.clamp(0.0, 1.0) * 0.495) * sr;
        let rel_coef = (-1.0 / rel_s).exp();

        // Write current sample into ring at write head.
        self.buf[self.write] = input;
        // Read the (write - lookahead) sample as the delayed output.
        let read = (self.write + LIMITER_BUF - look_s) & (LIMITER_BUF - 1);
        let delayed = self.buf[read];

        // Update running peak across the next `look_s` samples.  Cheap full
        // scan since the window is small (≤480 samples) and we avoid the
        // book-keeping of a true monotonic deque.
        let mut peak = 0.0f32;
        for i in 0..look_s {
            let idx = (read + i) & (LIMITER_BUF - 1);
            let m = self.buf[idx].abs();
            if m > peak {
                peak = m;
            }
        }
        self.win_peak = peak;

        // Target gain so peak·gain ≤ ceiling.  Limit only when over threshold.
        let target = if peak > thr_lin {
            (ceil_lin / peak).min(1.0)
        } else {
            1.0
        };
        // Attack is instantaneous (lookahead lets us pre-empt); release is
        // exponential.  Going *down* (gain reduction) snaps to target,
        // going *up* eases via `rel_coef`.
        if target < self.gain {
            self.gain = target;
        } else {
            self.gain = target + (self.gain - target) * rel_coef;
        }

        self.write = (self.write + 1) & (LIMITER_BUF - 1);
        let y = delayed * self.gain;
        // Final hard clip at the ceiling — safety net for the rare case the
        // attack hasn't quite caught up (e.g. peak ramped between scans).
        y.clamp(-ceil_lin, ceil_lin)
    }
}

// ─── State-variable filter (LP / BP / HP / Notch) ─────────────────────────────
//
// Chamberlin SVF (oversampled 2× internally for stability at high cutoff).
// Provides all four outputs simultaneously; the `mode` selector picks one.
// Drive saturates the input for analog-flavour filter overdrive.

pub(crate) struct Svf {
    /// Bandpass integrator state.
    band: f32,
    /// Lowpass integrator state.
    low: f32,
}

impl Svf {
    pub(crate) fn new() -> Self {
        Self {
            band: 0.0,
            low: 0.0,
        }
    }

    /// `cutoff`: 0–1 → 20 Hz–18 kHz logarithmic.
    /// `resonance`: 0–1 → Q ≈ 0.5..20.
    /// `drive`: 0–1 → 0..6 pre-saturation.
    /// `mode`: 0=LP, 1=BP, 2=HP, 3=Notch.
    /// `mix`: 0–1 wet/dry.
    pub(crate) fn process(
        &mut self,
        input: f32,
        cutoff: f32,
        resonance: f32,
        drive: f32,
        mode: u8,
        mix: f32,
        sr: f32,
    ) -> f32 {
        if mix < 0.001 {
            return input;
        }
        // Log-mapped cutoff so the knob feels musical across the audio band.
        let fc = 20.0 * 900.0f32.powf(cutoff.clamp(0.0, 1.0)).clamp(20.0, sr * 0.45);
        let q = 0.5 + resonance.clamp(0.0, 1.0) * 19.5;
        let damp = (1.0 / q).min(2.0 - 1e-3);

        // Drive: gentle tanh-ish curve so the resonance peak doesn't blow up
        // when the user pushes drive AND resonance high simultaneously.
        let drive_amt = 1.0 + drive.clamp(0.0, 1.0) * 5.0;
        let xin = if drive > 0.001 {
            super::tanh(input * drive_amt) / drive_amt
        } else {
            input
        };

        // 2× oversample for stability at high cutoff.
        let f = 2.0 * (std::f32::consts::PI * fc / (sr * 2.0)).sin();
        let mut wet = 0.0f32;
        for _ in 0..2 {
            self.low += f * self.band;
            let high = xin - self.low - damp * self.band;
            self.band += f * high;
            wet = match mode {
                0 => self.low,
                1 => self.band,
                2 => high,
                _ => self.low + high, // notch = LP + HP
            };
        }
        input * (1.0 - mix) + wet * mix
    }
}

// ─── Comb resonator (tuned feedback comb) ─────────────────────────────────────
//
// Karplus-style feedback comb tuned to a pitch in Hz.  Distinct from the
// reverb (which is a stack of detuned combs) and the delay (which is a long
// modulated tape) — this is a single short comb whose feedback creates a
// resonant tone at `pitch` Hz with damping rolling off the highs.
//
// `pitch`:    0–1 → 40 Hz (low growl) .. 2000 Hz (metallic).
// `feedback`: 0–1 → 0..0.99.
// `damp`:     0–1 → 0..1 lowpass-coefficient on the feedback path.
// `mix`:      0–1 wet/dry.

const COMB_BUF: usize = 2048; // 40 Hz @ 48 kHz needs ~1200 samples; 2048 = power of two for masking

pub(crate) struct CombRes {
    buf: Box<[f32; COMB_BUF]>,
    write: usize,
    lp_state: f32,
}

impl CombRes {
    pub(crate) fn new() -> Self {
        Self {
            buf: Box::new([0.0; COMB_BUF]),
            write: 0,
            lp_state: 0.0,
        }
    }

    pub(crate) fn process(
        &mut self,
        input: f32,
        pitch: f32,
        feedback: f32,
        damp: f32,
        mix: f32,
        sr: f32,
    ) -> f32 {
        if mix < 0.001 && feedback < 0.001 {
            self.buf[self.write] = input;
            self.write = (self.write + 1) & (COMB_BUF - 1);
            return input;
        }
        // Pitch maps log-style across 40..2000 Hz so the knob is musical.
        let hz = 40.0 * 50.0f32.powf(pitch.clamp(0.0, 1.0));
        let delay_s = (sr / hz).clamp(2.0, (COMB_BUF - 2) as f32);
        let read_pos = self.write as f32 + COMB_BUF as f32 - delay_s;
        let idx = read_pos as usize & (COMB_BUF - 1);
        let frac = read_pos - read_pos.floor();
        let next = (idx + 1) & (COMB_BUF - 1);
        let delayed = self.buf[idx] + (self.buf[next] - self.buf[idx]) * frac;

        // One-pole LP on the feedback for tone-darkening / damping.
        let d = damp.clamp(0.0, 0.95);
        self.lp_state = self.lp_state * d + delayed * (1.0 - d);

        let fb = feedback.clamp(0.0, 0.99);
        let to_buf = input + self.lp_state * fb;
        self.buf[self.write] = to_buf;
        self.write = (self.write + 1) & (COMB_BUF - 1);

        input * (1.0 - mix) + delayed * mix
    }
}

// ─── Tilt EQ (single-knob spectral tilt) ──────────────────────────────────────
//
// Pivot frequency splits the spectrum into "low" and "high" halves; the tilt
// knob simultaneously boosts one half and cuts the other, total ±12 dB.
// Cheap reuse of the `Biquad` shelves below — one low-shelf + one high-shelf
// with mirrored gain.

pub(crate) struct Tilt {
    low: Biquad,
    hi: Biquad,
    sr: f32,
    last_tilt: f32,
    last_pivot: f32,
}

impl Tilt {
    pub(crate) fn new(sr: f32) -> Self {
        Self {
            low: Biquad::low_shelf(700.0, 0.0, sr),
            hi: Biquad::high_shelf(700.0, 0.0, sr),
            sr,
            last_tilt: f32::NAN,
            last_pivot: f32::NAN,
        }
    }

    /// `tilt`: 0–1 → −12..+12 dB (0.5 = flat). −1 = bass-heavy, +1 = treble-heavy.
    /// `pivot`: 0–1 → 200 Hz–5 kHz logarithmic.
    /// `mix`: 0–1 wet/dry.
    pub(crate) fn process(&mut self, input: f32, tilt: f32, pivot: f32, mix: f32) -> f32 {
        if mix < 0.001 {
            return input;
        }
        // Recompute coefficients only when knobs move appreciably.
        if (tilt - self.last_tilt).abs() > 0.001 || (pivot - self.last_pivot).abs() > 0.001 {
            let fc = 200.0 * 25.0f32.powf(pivot.clamp(0.0, 1.0));
            let signed = (tilt.clamp(0.0, 1.0) - 0.5) * 24.0;
            self.low = Biquad::low_shelf(fc, -signed, self.sr);
            self.hi = Biquad::high_shelf(fc, signed, self.sr);
            self.last_tilt = tilt;
            self.last_pivot = pivot;
        }
        let wet = self.hi.process(self.low.process(input));
        input * (1.0 - mix) + wet * mix
    }
}

// ─── Transient designer ───────────────────────────────────────────────────────
//
// Two envelope followers: one fast (transient-tracking), one slow (sustain-
// tracking).  Their *difference* is the transient envelope; ratio between
// them is the sustain envelope.  User knobs scale each in dB before the
// signal is gain-modulated.

pub(crate) struct Transient {
    fast_env: f32,
    slow_env: f32,
}

impl Transient {
    pub(crate) fn new() -> Self {
        Self {
            fast_env: 0.0,
            slow_env: 0.0,
        }
    }

    /// `attack`: 0–1 → −12..+12 dB transient gain (0.5 = flat).
    /// `sustain`: 0–1 → −12..+12 dB sustain gain.
    /// `mix`: 0–1 wet/dry.
    pub(crate) fn process(
        &mut self,
        input: f32,
        attack: f32,
        sustain: f32,
        mix: f32,
        sr: f32,
    ) -> f32 {
        if mix < 0.001 {
            return input;
        }
        let level = input.abs();
        // Fast follower: 1 ms attack, 30 ms release.
        let fast_a = (-1.0 / (sr * 0.001)).exp();
        let fast_r = (-1.0 / (sr * 0.030)).exp();
        // Slow follower: 25 ms attack, 200 ms release.
        let slow_a = (-1.0 / (sr * 0.025)).exp();
        let slow_r = (-1.0 / (sr * 0.200)).exp();
        self.fast_env = if level > self.fast_env {
            self.fast_env * fast_a + level * (1.0 - fast_a)
        } else {
            self.fast_env * fast_r + level * (1.0 - fast_r)
        };
        self.slow_env = if level > self.slow_env {
            self.slow_env * slow_a + level * (1.0 - slow_a)
        } else {
            self.slow_env * slow_r + level * (1.0 - slow_r)
        };

        // Transient = fast minus slow (positive when a sudden burst arrives).
        let trans = (self.fast_env - self.slow_env).max(0.0);
        // Sustain proxy = slow envelope itself.
        let sus = self.slow_env;
        // Map both knobs to ±12 dB linear-ish gains.
        let att_g = 10.0f32.powf(((attack.clamp(0.0, 1.0) - 0.5) * 24.0) / 20.0);
        let sus_g = 10.0f32.powf(((sustain.clamp(0.0, 1.0) - 0.5) * 24.0) / 20.0);

        // Apply: signal scaled by sustain gain, then add the (att_g − 1) ×
        // transient times its sign so we don't punch DC.  Transient is
        // already non-negative, so blend it scaled by `sign(input)` to
        // preserve waveshape.
        let sign = if input >= 0.0 { 1.0 } else { -1.0 };
        let wet = input * (sus_g - 1.0) * (sus / (level.max(1e-6)))
            + sign * trans * (att_g - 1.0)
            + input;
        input * (1.0 - mix) + wet * mix
    }
}

// ─── Exciter (high-shelf saturation) ──────────────────────────────────────────
//
// Highpass-isolate the upper band, soft-clip it, mix back into the dry signal.
// Adds shimmer / air without the broad hash of `FxDrive`.

pub(crate) struct Exciter {
    hp_state: f32,
}

impl Exciter {
    pub(crate) fn new() -> Self {
        Self { hp_state: 0.0 }
    }

    /// `amount`: 0–1 → 0..6 saturation drive on the isolated highs.
    /// `freq`:   0–1 → 1 kHz–10 kHz HP corner.
    /// `mix`:    0–1 wet/dry on the added harmonics.
    pub(crate) fn process(&mut self, input: f32, amount: f32, freq: f32, mix: f32, sr: f32) -> f32 {
        if mix < 0.001 {
            return input;
        }
        let fc = 1000.0 * 10.0f32.powf(freq.clamp(0.0, 1.0));
        // One-pole HP: y = x − lp(x); lp coefficient via RC time constant.
        let rc = 1.0 / (std::f32::consts::TAU * fc);
        let alpha = (1.0 / sr) / (rc + 1.0 / sr);
        self.hp_state += alpha * (input - self.hp_state);
        let hp = input - self.hp_state;

        let drive = 1.0 + amount.clamp(0.0, 1.0) * 5.0;
        let sat = super::tanh(hp * drive) / drive;
        // Harmonics blend in *additive* — mix=0 returns dry, mix=1 returns
        // dry+sat (we don't want to lose body when exciting).
        input + sat * mix
    }
}

// ─── Multitap delay ──────────────────────────────────────────────────────────
//
// 4 fixed taps with linearly-spread spacing.  `time` sets the spacing of
// the *furthest* tap (1..1000 ms), `spread` controls how the inner taps
// distribute (0 = all bunched at the furthest, 1 = evenly spaced from
// 0..time).  `feedback` recirculates the summed-tap output back into the
// input.  Distinct from `FxDelay` (single-tap tape) and `FxConvReverb`
// (impulse-response convolution): a deliberate rhythmic / dub flavour
// from N evenly-spaced echoes.

const MULTITAP_BUF: usize = 96_000; // 2 s @ 48 kHz
const MULTITAP_TAPS: usize = 4;

pub(crate) struct Multitap {
    buf: Box<[f32; MULTITAP_BUF]>,
    write: usize,
}

impl Multitap {
    pub(crate) fn new() -> Self {
        Self {
            buf: Box::new([0.0; MULTITAP_BUF]),
            write: 0,
        }
    }

    /// `time`: 0–1 → 1 ms..1000 ms furthest tap.
    /// `spread`: 0–1 → 0=collapsed onto furthest tap, 1=evenly distributed.
    /// `feedback`: 0–1 → 0..0.85.
    /// `mix`: 0–1 wet/dry.
    pub(crate) fn process(
        &mut self,
        input: f32,
        time: f32,
        spread: f32,
        feedback: f32,
        mix: f32,
        sr: f32,
    ) -> f32 {
        if mix < 0.001 && feedback < 0.001 {
            self.buf[self.write] = input;
            self.write = (self.write + 1) % MULTITAP_BUF;
            return input;
        }
        let max_delay_s = (0.001 + time.clamp(0.0, 1.0) * 0.999) * sr;
        let max_delay = max_delay_s.clamp(2.0, (MULTITAP_BUF - 2) as f32) as usize;
        let s = spread.clamp(0.0, 1.0);

        let mut wet = 0.0f32;
        for tap in 0..MULTITAP_TAPS {
            // Tap n (1-indexed): position = max_delay * lerp(1.0, n/N, spread).
            let collapsed = 1.0;
            let spread_pos = (tap + 1) as f32 / MULTITAP_TAPS as f32;
            let pos_frac = collapsed * (1.0 - s) + spread_pos * s;
            let off = ((max_delay as f32 * pos_frac) as usize).clamp(1, MULTITAP_BUF - 1);
            let read = (self.write + MULTITAP_BUF - off) % MULTITAP_BUF;
            wet += self.buf[read];
        }
        wet /= MULTITAP_TAPS as f32;

        let fb = feedback.clamp(0.0, 0.85);
        self.buf[self.write] = input + wet * fb;
        self.write = (self.write + 1) % MULTITAP_BUF;
        input * (1.0 - mix) + wet * mix
    }
}

// ─── Reverse delay ───────────────────────────────────────────────────────────
//
// Fills a buffer for `time` seconds, then plays it back reversed for the
// next `time` seconds while the next segment fills.  Two ping-pong segments
// keep the output continuous at every segment boundary.

const REVDELAY_BUF: usize = 96_000; // 2 s @ 48 kHz per segment

pub(crate) struct RevDelay {
    /// Two ping-pong segments — one writes while the other plays back.
    seg: [Box<[f32; REVDELAY_BUF]>; 2],
    /// Current write segment (0 or 1).
    write_seg: usize,
    /// Position within the active write segment (0..len).
    write_pos: usize,
    /// Position within the active read segment (counts down for reverse).
    read_pos: usize,
    /// Length of the current segment in samples.
    seg_len: usize,
}

impl RevDelay {
    pub(crate) fn new() -> Self {
        Self {
            seg: [Box::new([0.0; REVDELAY_BUF]), Box::new([0.0; REVDELAY_BUF])],
            write_seg: 0,
            write_pos: 0,
            read_pos: 0,
            seg_len: 1,
        }
    }

    /// `time`: 0–1 → 50 ms..2 s segment length.
    /// `feedback`: 0–1 → 0..0.85 (the reversed wet feeds back into the
    /// new segment).
    /// `mix`: 0–1 wet/dry.
    pub(crate) fn process(
        &mut self,
        input: f32,
        time: f32,
        feedback: f32,
        mix: f32,
        sr: f32,
    ) -> f32 {
        // Segment length — clamped to the buffer.
        let len_s = (0.05 + time.clamp(0.0, 1.0) * 1.95) * sr;
        let len = (len_s as usize).clamp(64, REVDELAY_BUF);
        // Refresh the segment length on each pass; if it shrinks mid-flight
        // we'll wrap a touch sooner, which is acceptable.
        if self.seg_len != len {
            self.seg_len = len;
            // Reset read so we don't index outside the current segment.
            self.read_pos = self.read_pos.min(len.saturating_sub(1));
        }

        let read_seg = 1 - self.write_seg;
        let wet = if self.read_pos < self.seg_len {
            self.seg[read_seg][self.seg_len - 1 - self.read_pos]
        } else {
            0.0
        };

        let fb = feedback.clamp(0.0, 0.85);
        self.seg[self.write_seg][self.write_pos] = input + wet * fb;
        self.write_pos += 1;
        self.read_pos += 1;

        if self.write_pos >= self.seg_len {
            // Swap segments at the boundary; the just-written segment
            // becomes the next read source.
            self.write_pos = 0;
            self.read_pos = 0;
            self.write_seg = 1 - self.write_seg;
        }

        input * (1.0 - mix) + wet * mix
    }
}

// ─── Tape stop ───────────────────────────────────────────────────────────────
//
// Mix knob doubles as ramp progress — 0 = normal pass-through, 1 = fully
// stopped (silent).  Internally maintains a delay line; the read head's
// playback rate ramps from 1.0 down to 0.0 as `mix` rises, simulating the
// platter winding to a halt.  A tone-darkening lowpass that tracks the
// rate keeps the signal from sounding edgy as it slows.

const TAPESTOP_BUF: usize = 96_000; // 2 s @ 48 kHz

pub(crate) struct TapeStop {
    buf: Box<[f32; TAPESTOP_BUF]>,
    write: usize,
    /// Fractional read head — advances by `rate` per sample.  Re-anchors to
    /// `write` whenever mix drops back to 0 (preventing drift).
    read: f32,
    /// One-pole LP state for the dynamic darkening.
    lp_state: f32,
    /// Last-frame mix to detect rising-edge re-engage.
    last_mix: f32,
}

impl TapeStop {
    pub(crate) fn new() -> Self {
        Self {
            buf: Box::new([0.0; TAPESTOP_BUF]),
            write: 0,
            read: 0.0,
            lp_state: 0.0,
            last_mix: 0.0,
        }
    }

    /// `mix`: 0–1 — also acts as the ramp position (0 = pass-through, 1
    /// = silenced).  Curve is shaped so the perceived slow-down feels
    /// closer to logarithmic than linear.
    /// `time`: 0–1 → 0.05..2 s scratch-tail buffer length cap.
    pub(crate) fn process(&mut self, input: f32, mix: f32, _time: f32, _sr: f32) -> f32 {
        // Always write the dry signal so re-engagements can pull from
        // recent material without an attack-lag glitch.
        self.buf[self.write] = input;
        self.write = (self.write + 1) % TAPESTOP_BUF;

        // Re-anchor read head when mix returns from > 0 to 0.
        if self.last_mix > 0.001 && mix < 0.001 {
            self.read = self.write as f32;
            self.lp_state = 0.0;
        }
        self.last_mix = mix;

        if mix < 0.001 {
            return input;
        }

        // Ramp curve: rate = (1 - mix)^2 — slows perceptually.
        let rate = (1.0 - mix.clamp(0.0, 1.0)).powi(2);
        // Advance read by `rate` samples per output sample.
        let mut read_pos = self.read;
        // Linear-interp read.
        let idx = read_pos as usize % TAPESTOP_BUF;
        let frac = read_pos - read_pos.floor();
        let next = (idx + 1) % TAPESTOP_BUF;
        let raw = self.buf[idx] + (self.buf[next] - self.buf[idx]) * frac;

        read_pos += rate;
        if read_pos >= TAPESTOP_BUF as f32 {
            read_pos -= TAPESTOP_BUF as f32;
        }
        self.read = read_pos;

        // Lowpass darkens with the ramp.  alpha → 0 as mix → 1.
        let alpha = (1.0 - mix.clamp(0.0, 1.0)) * 0.6 + 0.05;
        self.lp_state += alpha * (raw - self.lp_state);
        // Output is the slowed+darkened wet, scaled by (1-mix) so it
        // smoothly trails to silence as the ramp completes.
        self.lp_state * (1.0 - mix.clamp(0.0, 1.0))
    }
}

// ─── Stutter / repeater ──────────────────────────────────────────────────────
//
// Captures a slice every `period` samples and loops it for the remainder of
// the period.  `period` is derived from BPM and the user's rate
// subdivision, so the stutter is automatically beat-synced.

const STUTTER_BUF: usize = 48_000; // 1 s @ 48 kHz captures plenty of slice

pub(crate) struct Stutter {
    /// Slice buffer — captured once per period, replayed across the period.
    slice: Box<[f32; STUTTER_BUF]>,
    slice_len: usize,
    /// Position within the slice for the current period playback.
    play_pos: usize,
    /// Counts samples since the slice was last captured.
    period_pos: usize,
}

impl Stutter {
    pub(crate) fn new() -> Self {
        Self {
            slice: Box::new([0.0; STUTTER_BUF]),
            slice_len: 0,
            play_pos: 0,
            period_pos: 0,
        }
    }

    /// `rate`: 0–1 → quantised to 1/4, 1/8, 1/16, 1/32 note divisions.
    /// `slice_frac`: 0–1 → fraction of the period that's captured (rest
    /// of the period replays the captured slice).
    /// `mix`: 0–1 wet/dry.
    /// `bpm`: passed in so the period stays musically aligned.
    pub(crate) fn process(
        &mut self,
        input: f32,
        rate: f32,
        slice_frac: f32,
        mix: f32,
        bpm: f32,
        sr: f32,
    ) -> f32 {
        if mix < 0.001 {
            self.period_pos = 0;
            self.play_pos = 0;
            return input;
        }
        // Quantise rate to 1/4, 1/8, 1/16, 1/32.
        let div = match (rate.clamp(0.0, 0.999) * 4.0) as usize {
            0 => 4u32,  // quarter
            1 => 8u32,  // eighth
            2 => 16u32, // sixteenth
            _ => 32u32, // thirty-second
        };
        let beat_s = 60.0 / bpm.max(20.0);
        let period_s = beat_s * 4.0 / div as f32;
        let period = ((period_s * sr) as usize).clamp(64, STUTTER_BUF);
        let cap_len = ((period as f32 * slice_frac.clamp(0.05, 1.0)) as usize)
            .clamp(8, period.min(STUTTER_BUF));

        // Capture phase: write input into the slice buffer for the first
        // `cap_len` samples of the period.
        if self.period_pos < cap_len {
            self.slice[self.period_pos] = input;
            self.slice_len = cap_len;
            self.play_pos = 0;
        }

        let wet = if self.slice_len > 0 {
            let s = self.slice[self.play_pos % self.slice_len];
            self.play_pos = (self.play_pos + 1) % self.slice_len;
            s
        } else {
            input
        };

        self.period_pos += 1;
        if self.period_pos >= period {
            self.period_pos = 0;
        }

        input * (1.0 - mix) + wet * mix
    }
}

// ─── Spectral freezer (FFT magnitudes hold + random-phase resynth) ───────────
//
// Captures one FFT frame on the rising edge of `mix > 0`; thereafter
// regenerates output frames at the same magnitudes but with fresh random
// phases per frame, IFFT'd and overlap-add'd into the output buffer.
//
// FFT_SIZE = 1024 with a 256-sample hop (75 % overlap) — standard for
// spectral-domain effects.  Both forward + inverse FFTs are pre-planned at
// construction so process() never allocates.
//
// The user knob is just `mix`: > 0 engages capture-and-hold; back to 0
// resets and resumes pass-through.  Future enhancements: re-trigger knob
// (force a new capture mid-freeze), spread knob (smear magnitudes across
// neighbouring bins for chorus-like motion).

const FREEZE_FFT_SIZE: usize = 1024;
const FREEZE_HOP_SIZE: usize = 256;
const FREEZE_BINS: usize = FREEZE_FFT_SIZE / 2 + 1;

pub(crate) struct Freeze {
    /// Forward FFT plan (pre-allocated by FftPlanner).
    fft_fwd: Arc<dyn Fft<f32>>,
    /// Inverse FFT plan.
    fft_inv: Arc<dyn Fft<f32>>,
    /// Scratch buffer for in-place FFT (sized to the larger of fwd / inv).
    fft_scratch: Vec<Complex<f32>>,
    /// Working complex buffer for FFT input/output.
    work: Vec<Complex<f32>>,
    /// Pre-computed Hann window of length FREEZE_FFT_SIZE.
    hann: Vec<f32>,
    /// Input ring buffer — last FFT_SIZE samples, ordered with `in_pos`
    /// pointing at the next write position.
    in_buf: Vec<f32>,
    in_pos: usize,
    /// Output ring buffer for overlap-add.  Must be at least
    /// FFT_SIZE + HOP samples; we use 2 * FFT_SIZE for headroom.
    out_buf: Vec<f32>,
    out_read: usize,
    out_write_anchor: usize,
    /// Captured magnitude spectrum.  Only populated when mix > 0 and we've
    /// taken a snapshot.
    captured_mag: Vec<f32>,
    captured: bool,
    /// Sample counter since last hop.
    hop_counter: usize,
    /// xorshift RNG state for per-frame phase randomisation.
    rng_state: u32,
    /// Last-frame mix to detect rising edge.
    last_mix: f32,
}

impl Freeze {
    pub(crate) fn new() -> Self {
        let mut planner = FftPlanner::new();
        let fft_fwd = planner.plan_fft_forward(FREEZE_FFT_SIZE);
        let fft_inv = planner.plan_fft_inverse(FREEZE_FFT_SIZE);
        let scratch_len = fft_fwd
            .get_inplace_scratch_len()
            .max(fft_inv.get_inplace_scratch_len());
        let hann: Vec<f32> = (0..FREEZE_FFT_SIZE)
            .map(|n| 0.5 - 0.5 * (std::f32::consts::TAU * n as f32 / FREEZE_FFT_SIZE as f32).cos())
            .collect();
        Self {
            fft_fwd,
            fft_inv,
            fft_scratch: vec![Complex::new(0.0, 0.0); scratch_len],
            work: vec![Complex::new(0.0, 0.0); FREEZE_FFT_SIZE],
            hann,
            in_buf: vec![0.0; FREEZE_FFT_SIZE],
            in_pos: 0,
            out_buf: vec![0.0; FREEZE_FFT_SIZE * 2],
            out_read: 0,
            out_write_anchor: 0,
            captured_mag: vec![0.0; FREEZE_BINS],
            captured: false,
            hop_counter: 0,
            rng_state: 0x6D2B_79F5,
            last_mix: 0.0,
        }
    }

    /// xorshift32 — fast deterministic RNG for the random-phase resynth.
    /// Matches the style used elsewhere in the engine (NoiseGen).
    fn rand_u32(&mut self) -> u32 {
        let mut x = self.rng_state;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng_state = x;
        x
    }

    fn rand_phase(&mut self) -> f32 {
        // Uniform 0..2π.
        (self.rand_u32() as f32 / u32::MAX as f32) * std::f32::consts::TAU
    }

    /// Run one FFT capture on the most recent FFT_SIZE input samples,
    /// ordered linearly (oldest first).  Stores magnitude per bin in
    /// `captured_mag`.
    fn capture(&mut self) {
        // Copy in_buf into work in time order, applying the Hann window.
        for i in 0..FREEZE_FFT_SIZE {
            let idx = (self.in_pos + i) % FREEZE_FFT_SIZE;
            self.work[i] = Complex::new(self.in_buf[idx] * self.hann[i], 0.0);
        }
        self.fft_fwd
            .process_with_scratch(&mut self.work, &mut self.fft_scratch);
        for (i, m) in self.captured_mag.iter_mut().enumerate().take(FREEZE_BINS) {
            *m = self.work[i].norm();
        }
        self.captured = true;
    }

    /// Synthesise one output frame and overlap-add into `out_buf` starting
    /// at `out_write_anchor`.  Advances `out_write_anchor` by HOP_SIZE.
    fn synthesise_frame(&mut self) {
        // Build a complex spectrum with captured magnitudes + random phases,
        // mirrored for negative frequencies (Hermitian symmetry → real
        // output).
        for i in 0..FREEZE_BINS {
            let mag = self.captured_mag[i];
            let phase = if i == 0 || i == FREEZE_FFT_SIZE / 2 {
                0.0 // DC + Nyquist must be real
            } else {
                self.rand_phase()
            };
            let (s, c) = phase.sin_cos();
            self.work[i] = Complex::new(mag * c, mag * s);
        }
        // Mirror conjugate for the upper half.
        for i in 1..FREEZE_FFT_SIZE / 2 {
            let conj = self.work[i].conj();
            self.work[FREEZE_FFT_SIZE - i] = conj;
        }
        self.fft_inv
            .process_with_scratch(&mut self.work, &mut self.fft_scratch);
        // Overlap-add with Hann window into out_buf.  Normalise by FFT_SIZE
        // (rustfft's inverse is unscaled) and by the Hann² overlap-add gain
        // factor (≈ 1.5 at 75 % overlap with Hann window) to keep level
        // roughly equal to dry input.
        let norm = 1.0 / (FREEZE_FFT_SIZE as f32 * 1.5);
        for i in 0..FREEZE_FFT_SIZE {
            let idx = (self.out_write_anchor + i) % self.out_buf.len();
            self.out_buf[idx] += self.work[i].re * self.hann[i] * norm;
        }
        self.out_write_anchor = (self.out_write_anchor + FREEZE_HOP_SIZE) % self.out_buf.len();
    }

    pub(crate) fn process(&mut self, input: f32, mix: f32, _sr: f32) -> f32 {
        // Always write input into the input ring; we need it ready when
        // freeze first engages.
        self.in_buf[self.in_pos] = input;
        self.in_pos = (self.in_pos + 1) % FREEZE_FFT_SIZE;

        // Detect rising / falling edges of mix > 0.
        if self.last_mix < 0.001 && mix > 0.001 {
            // Engaging — schedule a capture on the next hop boundary.
            self.captured = false;
        } else if mix < 0.001 {
            // Disengaging — clear the captured snapshot and silence the
            // pending output so the next engagement starts fresh.
            self.captured = false;
            for s in &mut self.out_buf {
                *s = 0.0;
            }
            self.out_read = self.out_write_anchor;
        }
        self.last_mix = mix;

        // Read the next overlap-add output sample.
        let wet = self.out_buf[self.out_read];
        // Clear the slot we just consumed so subsequent overlap-adds don't
        // accumulate stale energy.
        self.out_buf[self.out_read] = 0.0;
        self.out_read = (self.out_read + 1) % self.out_buf.len();

        // Hop boundary: capture (if newly engaged) and synthesise one frame.
        self.hop_counter += 1;
        if self.hop_counter >= FREEZE_HOP_SIZE {
            self.hop_counter = 0;
            if mix > 0.001 {
                if !self.captured {
                    self.capture();
                }
                self.synthesise_frame();
            }
        }

        if mix < 0.001 {
            input
        } else {
            input * (1.0 - mix) + wet * mix
        }
    }
}
