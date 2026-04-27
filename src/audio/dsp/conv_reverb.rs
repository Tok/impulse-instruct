// ─── audio/dsp/conv_reverb.rs ────────────────────────────────────────────────
// Convolution reverb voice for the `FxStep::ConvReverb` dispatch.
//
// Partitioned overlap-save FFT convolution: the IR is sliced into
// `CONV_PART`-sample blocks, each zero-padded to `CONV_FFT_SIZE` and
// forward-FFT'd at load time.  Per audio block we forward-FFT the
// (prev || current) input, multiply-accumulate against every partition
// from a frequency-domain delay line, inverse-FFT once (or twice for
// stereo IRs), and emit the back half of the IFFT as the valid wet
// output.  A stereo IR drives a true `mid ± side` return via
// `self.side` so the master mixer can widen the reverb across L/R;
// mono IRs leave `side` at zero and the wet returns centred.
//
// Startup latency is one partition (`CONV_PART` samples = 21.3 ms at
// 48 kHz): the first `CONV_PART` samples after `load_ir` return
// silence on the wet bus while the first block accumulates.  After
// that the output streams one sample per `process` call.
//
// Fallback: when no IR is loaded the `process` path uses the Phase 1
// stub (predelay + LP/HP filtering), so un-configured ConvReverb
// modules still colour the wet audibly and the user knows the chain
// is live.

use std::sync::Arc;

use rustfft::num_complex::Complex;
use rustfft::{Fft, FftPlanner};

use super::dsp_util::{MIX_BYPASS_THRESHOLD, one_pole_lp_alpha};
use super::pitch_shift::PitchShift;

/// Predelay ring buffer length — covers the full 0..200 ms knob range at
/// the 48 kHz engine and any reasonable 44.1 kHz setting with headroom.
pub const CONV_REVERB_PREDELAY_LEN: usize = 16384;

/// Maximum predelay mapped from the `conv_reverb_predelay` knob (0..1).
pub const CONV_REVERB_MAX_PREDELAY_SEC: f32 = 0.2;

/// Block size for partitioned overlap-save convolution.
pub const CONV_PART: usize = 1024;

/// FFT size = 2 × partition size, so overlap-save's "second half is
/// valid" rule holds.
pub const CONV_FFT_SIZE: usize = 2 * CONV_PART;

/// Inverse-FFT rescale factor — rustfft's inverse doesn't normalise.
const IFFT_SCALE: f32 = 1.0 / CONV_FFT_SIZE as f32;

pub struct ConvReverb {
    // ── Phase 1 carry-over ──────────────────────────────────────────────
    predelay_buf: Vec<f32>,
    predelay_head: usize,
    lp_state: f32,
    hp_lp_state: f32,
    pub side: f32,

    // ── FFT engine (pre-planned once; Arc-shared, not cloned per block) ──
    fft_fwd: Arc<dyn Fft<f32>>,
    fft_inv: Arc<dyn Fft<f32>>,
    /// Scratch buffer sized for the larger of fwd/inv plans.
    fft_scratch: Vec<Complex<f32>>,

    // ── IR (frequency-domain partition cache) ───────────────────────────
    /// IR left / mono partitions, each a length-`CONV_FFT_SIZE` spectrum.
    /// Empty when no IR is loaded.
    ir_l: Vec<Vec<Complex<f32>>>,
    /// IR right partitions.  Populated only for stereo IRs; empty for
    /// mono so the stereo-accumulate path can branch on `!is_empty()`.
    ir_r: Vec<Vec<Complex<f32>>>,

    // ── Input buffering + frequency-domain delay line ───────────────────
    /// Sample accumulator — fills to `CONV_PART`, then the block is
    /// processed.  `in_pos` tracks the next write slot.
    in_block: Vec<f32>,
    in_pos: usize,
    /// Previous complete input block — concatenated with `in_block` at
    /// FFT time so the result covers the full 2-block window.
    prev_block: Vec<f32>,
    /// Frequency-domain delay line.  `fdl_head` points at the slot that
    /// will hold the next input spectrum; older spectra walk forward
    /// modulo `fdl.len()`.  Each slot is `CONV_FFT_SIZE` complex bins.
    fdl: Vec<Vec<Complex<f32>>>,
    fdl_head: usize,

    // ── Block scratch ───────────────────────────────────────────────────
    /// Complex scratch for the per-block input FFT + partition
    /// accumulators.  Owned by the struct so `process` makes no
    /// allocations inside the audio callback.
    fft_buf: Vec<Complex<f32>>,
    acc_l: Vec<Complex<f32>>,
    acc_r: Vec<Complex<f32>>,

    // ── Output queues ───────────────────────────────────────────────────
    /// The last-produced `CONV_PART` wet samples for each channel.
    /// `out_pos == CONV_PART` means "consumed, waiting for next block";
    /// `out_pos == 0` means "just produced, emit on next call".
    out_l: Vec<f32>,
    out_r: Vec<f32>,
    out_pos: usize,

    // ── Shimmer ─────────────────────────────────────────────────────────
    /// Pitch-shift instance dedicated to the shimmer feedback path —
    /// fixed at +12 semitones (one octave up).  The wet output is fed
    /// through this and folded back into the convolution input on the
    /// next sample, producing the classic ambient-shimmer ladder.
    shimmer_shift: PitchShift,
    /// Last produced wet (pre-tone-shaping) — feeds the shimmer
    /// pitch-shift on the next call.  One-sample delay so the
    /// feedback doesn't form an immediate algebraic loop.
    last_wet_for_shimmer: f32,
}

impl ConvReverb {
    pub fn new() -> Self {
        let mut planner = FftPlanner::new();
        let fft_fwd = planner.plan_fft_forward(CONV_FFT_SIZE);
        let fft_inv = planner.plan_fft_inverse(CONV_FFT_SIZE);
        let scratch_len = fft_fwd
            .get_inplace_scratch_len()
            .max(fft_inv.get_inplace_scratch_len());
        Self {
            predelay_buf: vec![0.0; CONV_REVERB_PREDELAY_LEN],
            predelay_head: 0,
            lp_state: 0.0,
            hp_lp_state: 0.0,
            side: 0.0,
            fft_fwd,
            fft_inv,
            fft_scratch: vec![Complex::new(0.0, 0.0); scratch_len],
            ir_l: Vec::new(),
            ir_r: Vec::new(),
            in_block: vec![0.0; CONV_PART],
            in_pos: 0,
            prev_block: vec![0.0; CONV_PART],
            fdl: Vec::new(),
            fdl_head: 0,
            fft_buf: vec![Complex::new(0.0, 0.0); CONV_FFT_SIZE],
            acc_l: vec![Complex::new(0.0, 0.0); CONV_FFT_SIZE],
            acc_r: vec![Complex::new(0.0, 0.0); CONV_FFT_SIZE],
            out_l: vec![0.0; CONV_PART],
            out_r: vec![0.0; CONV_PART],
            out_pos: CONV_PART,
            shimmer_shift: PitchShift::new(),
            last_wet_for_shimmer: 0.0,
        }
    }

    /// Load `data` as the new impulse response.  `channels` must be 1
    /// (mono) or 2 (interleaved stereo); other values are clamped.
    /// `reversed` stores the IR back-to-front for the reverse-reverb
    /// effect.  Called outside the audio callback (via
    /// `AudioCommand::LoadImpulseResponse`), so allocation + FFT
    /// pre-compute cost is fine here.
    pub fn load_ir(&mut self, data: Arc<Vec<f32>>, channels: u8, reversed: bool) {
        if data.is_empty() {
            self.clear_ir();
            return;
        }
        let channels = channels.clamp(1, 2) as usize;

        // Deinterleave into per-channel time-domain buffers.
        let (mut left, right): (Vec<f32>, Option<Vec<f32>>) = if channels == 1 {
            (data.as_ref().clone(), None)
        } else {
            let n_frames = data.len() / 2;
            let mut l = Vec::with_capacity(n_frames);
            let mut r = Vec::with_capacity(n_frames);
            for i in 0..n_frames {
                l.push(data[2 * i]);
                r.push(data[2 * i + 1]);
            }
            (l, Some(r))
        };
        let mut right = right;

        if reversed {
            left.reverse();
            if let Some(r) = right.as_mut() {
                r.reverse();
            }
        }

        let n_parts = left.len().div_ceil(CONV_PART).max(1);
        let mut parts_l: Vec<Vec<Complex<f32>>> = Vec::with_capacity(n_parts);
        let mut parts_r: Vec<Vec<Complex<f32>>> = if right.is_some() {
            Vec::with_capacity(n_parts)
        } else {
            Vec::new()
        };

        for p in 0..n_parts {
            let start = p * CONV_PART;
            let end = (start + CONV_PART).min(left.len());

            let mut buf = vec![Complex::new(0.0, 0.0); CONV_FFT_SIZE];
            for (i, &s) in left[start..end].iter().enumerate() {
                buf[i].re = s;
            }
            self.fft_fwd
                .process_with_scratch(&mut buf, &mut self.fft_scratch);
            parts_l.push(buf);

            if let Some(r) = &right {
                let mut buf = vec![Complex::new(0.0, 0.0); CONV_FFT_SIZE];
                for (i, &s) in r[start..end].iter().enumerate() {
                    buf[i].re = s;
                }
                self.fft_fwd
                    .process_with_scratch(&mut buf, &mut self.fft_scratch);
                parts_r.push(buf);
            }
        }

        // (Re)allocate the FDL to the new partition count.  Resetting
        // to zeros is important — a smaller IR would otherwise convolve
        // against stale spectra from a longer previous IR and throw
        // spurious wet content.
        self.fdl = (0..n_parts)
            .map(|_| vec![Complex::new(0.0, 0.0); CONV_FFT_SIZE])
            .collect();
        self.fdl_head = 0;

        self.ir_l = parts_l;
        self.ir_r = parts_r;

        // Reset stream state so the new IR starts from silence.
        self.in_block.fill(0.0);
        self.prev_block.fill(0.0);
        self.out_l.fill(0.0);
        self.out_r.fill(0.0);
        self.in_pos = 0;
        self.out_pos = CONV_PART;
    }

    /// Drop any loaded IR — the `process` path falls back to the
    /// Phase 1 filter-only wet.
    pub fn clear_ir(&mut self) {
        self.ir_l.clear();
        self.ir_r.clear();
        self.fdl.clear();
    }

    /// Number of IR partitions currently cached — exposed for tests
    /// that want to verify load_ir partitioned the data correctly.
    #[cfg(test)]
    pub fn partition_count(&self) -> usize {
        self.ir_l.len()
    }

    /// Process one sample.  Mono in / mono (mid) out; a stereo side
    /// component is latched into `self.side` for the master mixer.
    #[allow(clippy::too_many_arguments)]
    pub fn process(
        &mut self,
        sig: f32,
        mix: f32,
        predelay: f32,
        damp: f32,
        lowcut: f32,
        size: f32,
        width: f32,
        shimmer: f32,
        sr: f32,
    ) -> f32 {
        let pd_len = self.predelay_buf.len();

        if mix < MIX_BYPASS_THRESHOLD {
            self.side = 0.0;
            self.last_wet_for_shimmer = 0.0;
            // Keep the delay line advancing so the first non-zero mix
            // doesn't read stale audio from before the knob was opened.
            self.predelay_buf[self.predelay_head] = sig;
            self.predelay_head = (self.predelay_head + 1) % pd_len;
            return sig;
        }

        // Predelay: write the current sample to the head, then read
        // `pd_samples` behind that write so `predelay=0` is a true
        // zero-delay pass (read picks up the value we just stored).
        // Used as the convolution input so the predelay knob pushes
        // the wet onset back without affecting the dry path.
        let pd_samples = (predelay.clamp(0.0, 1.0) * CONV_REVERB_MAX_PREDELAY_SEC * sr) as usize;
        let pd_samples = pd_samples.min(pd_len - 1);
        self.predelay_buf[self.predelay_head] = sig;
        let read_idx = (self.predelay_head + pd_len - pd_samples) % pd_len;
        let delayed = self.predelay_buf[read_idx];
        self.predelay_head = (self.predelay_head + 1) % pd_len;

        // Shimmer: pitch-shift the previous wet sample +12 ST and
        // mix it into the convolution input.  Internal mix on the
        // pitch shifter stays at 1.0 (fully wet) so we get the pure
        // shifted signal; the depth knob applies after.  The +12
        // semitone offset is hard-wired — V1 of the shimmer flag is
        // a single up-octave ladder, no chord stacking.
        //
        // One-sample delay (via `last_wet_for_shimmer`) breaks the
        // algebraic loop — without it, the wet would depend on
        // itself within the same `process` call.
        let shimmer = shimmer.clamp(0.0, 1.0);
        let shimmer_in = if shimmer > 0.001 {
            let pitched =
                self.shimmer_shift
                    .process(self.last_wet_for_shimmer, 12.0, 0.0, 1.0, 0.0);
            delayed + pitched * shimmer
        } else {
            // Keep the pitch-shifter ring buffer current at zero
            // depth so the first non-zero shimmer doesn't pick up
            // stale audio from before the knob was opened.
            let _ = self
                .shimmer_shift
                .process(self.last_wet_for_shimmer, 12.0, 0.0, 0.0, 0.0);
            delayed
        };

        // Get the convolved (or filter-only fallback) wet pair.
        let (wet_l, wet_r) = if self.ir_l.is_empty() {
            // No IR loaded → Phase 1 stub.  Still honours damp/lowcut
            // on the wet path so the module always responds to the
            // full knob set regardless of IR state.
            let w = shimmer_in;
            (w, w)
        } else {
            self.feed_conv(shimmer_in, size);
            // First `CONV_PART` calls after load: no full block yet,
            // so the out queue is "empty" (out_pos == CONV_PART).
            // Emit silence on the wet bus; the dry pass carries on via
            // the final mix so the user still hears the signal.
            if self.out_pos < CONV_PART {
                let pair = (self.out_l[self.out_pos], self.out_r[self.out_pos]);
                self.out_pos += 1;
                pair
            } else {
                (0.0, 0.0)
            }
        };

        // Tonal shaping — applied to the mid channel, which is what
        // the downstream mix path uses.  Side inherits the same
        // bandlimit via the filter states so the stereo image stays
        // coherent.
        let mut wet_l = wet_l;
        let mut wet_r = wet_r;

        let damp = damp.clamp(0.0, 1.0);
        if damp > 0.001 {
            let fc = 20_000.0 * (400.0_f32 / 20_000.0).powf(damp);
            let a = one_pole_lp_alpha(fc, sr);
            let mid = (wet_l + wet_r) * 0.5;
            self.lp_state += a * (mid - self.lp_state);
            let delta = self.lp_state - mid;
            wet_l += delta;
            wet_r += delta;
        } else {
            self.lp_state = (wet_l + wet_r) * 0.5;
        }

        let lowcut = lowcut.clamp(0.0, 1.0);
        if lowcut > 0.001 {
            let fc = 20.0 * (800.0_f32 / 20.0).powf(lowcut);
            let a = one_pole_lp_alpha(fc, sr);
            let mid = (wet_l + wet_r) * 0.5;
            self.hp_lp_state += a * (mid - self.hp_lp_state);
            wet_l -= self.hp_lp_state;
            wet_r -= self.hp_lp_state;
        } else {
            self.hp_lp_state = (wet_l + wet_r) * 0.5;
        }

        // Mid / side split — width scales the side contribution so
        // `width=0` collapses the wet back to mono regardless of IR.
        let width = width.clamp(0.0, 1.0);
        let mid = (wet_l + wet_r) * 0.5;
        self.side = (wet_l - wet_r) * 0.5 * width;

        // Stash the wet for next call's shimmer feedback.  We grab
        // the post-tone mid so the shimmer ladder inherits the
        // damp / lowcut shaping — keeps the up-octave fold from
        // building harshness in bands the user has already cut.
        self.last_wet_for_shimmer = mid;

        let mix = mix.clamp(0.0, 1.0);
        sig * (1.0 - mix) + mid * mix
    }

    /// Push one sample into the block accumulator; when the block is
    /// full, run the partitioned convolution and refill the output
    /// queue.  Extracted so the `process` hot path stays linear.
    fn feed_conv(&mut self, sample: f32, size: f32) {
        self.in_block[self.in_pos] = sample;
        self.in_pos += 1;
        if self.in_pos >= CONV_PART {
            self.process_conv_block(size);
            self.in_pos = 0;
        }
    }

    /// Run the partitioned overlap-save FFT convolution for the block
    /// that just filled in `in_block`.  Writes `CONV_PART` output
    /// samples into `out_l` / `out_r` and resets `out_pos` to 0.
    fn process_conv_block(&mut self, size: f32) {
        let n_parts = self.ir_l.len();
        if n_parts == 0 || self.fdl.is_empty() {
            return;
        }

        // Advance FDL head: the slot that was oldest becomes the new
        // "fdl[0]" (newest).  Older spectra walk forward by one.
        self.fdl_head = (self.fdl_head + self.fdl.len() - 1) % self.fdl.len();

        // Pack [prev_block || in_block] as real samples in fft_buf.
        for i in 0..CONV_PART {
            self.fft_buf[i] = Complex::new(self.prev_block[i], 0.0);
            self.fft_buf[CONV_PART + i] = Complex::new(self.in_block[i], 0.0);
        }
        self.fft_fwd
            .process_with_scratch(&mut self.fft_buf, &mut self.fft_scratch);

        // Copy the input spectrum into the FDL at the new head slot.
        self.fdl[self.fdl_head].copy_from_slice(&self.fft_buf);

        // Active partition count — SIZE knob truncates the IR tail
        // at process time.  Always keep at least 1 partition so the
        // early reflections survive even at size=0.
        let size = size.clamp(0.0, 1.0);
        let active = ((size * n_parts as f32).round() as usize)
            .max(1)
            .min(n_parts);
        let stereo = !self.ir_r.is_empty();

        // Clear the accumulator(s).  `fill` works because Complex<f32>
        // implements `Copy`.
        self.acc_l.fill(Complex::new(0.0, 0.0));
        if stereo {
            self.acc_r.fill(Complex::new(0.0, 0.0));
        }

        // Frequency-domain accumulate: Y[k] = Σ FDL[p][k] · H_p[k].
        // FDL index (head + p) % len gives the p-blocks-ago spectrum.
        let fdl_len = self.fdl.len();
        for p in 0..active {
            let idx = (self.fdl_head + p) % fdl_len;
            let xp = &self.fdl[idx];
            let hp_l = &self.ir_l[p];
            for k in 0..CONV_FFT_SIZE {
                self.acc_l[k] += xp[k] * hp_l[k];
            }
            if stereo {
                let hp_r = &self.ir_r[p];
                for k in 0..CONV_FFT_SIZE {
                    self.acc_r[k] += xp[k] * hp_r[k];
                }
            }
        }

        // IFFT the left/mono accumulator.  rustfft's inverse needs the
        // 1/N scaling applied by the caller.  The back half of the
        // IFFT output is the valid overlap-save result.
        // Move acc_l → fft_buf for in-place IFFT (scratch space).
        std::mem::swap(&mut self.fft_buf, &mut self.acc_l);
        self.fft_inv
            .process_with_scratch(&mut self.fft_buf, &mut self.fft_scratch);
        for i in 0..CONV_PART {
            self.out_l[i] = self.fft_buf[CONV_PART + i].re * IFFT_SCALE;
        }
        std::mem::swap(&mut self.fft_buf, &mut self.acc_l);

        if stereo {
            std::mem::swap(&mut self.fft_buf, &mut self.acc_r);
            self.fft_inv
                .process_with_scratch(&mut self.fft_buf, &mut self.fft_scratch);
            for i in 0..CONV_PART {
                self.out_r[i] = self.fft_buf[CONV_PART + i].re * IFFT_SCALE;
            }
            std::mem::swap(&mut self.fft_buf, &mut self.acc_r);
        } else {
            self.out_r.copy_from_slice(&self.out_l);
        }

        // Promote in_block to prev_block for the next FFT window.
        std::mem::swap(&mut self.prev_block, &mut self.in_block);

        self.out_pos = 0;
    }
}

impl Default for ConvReverb {
    fn default() -> Self {
        Self::new()
    }
}
