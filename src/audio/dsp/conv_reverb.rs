// ─── audio/dsp/conv_reverb.rs ────────────────────────────────────────────────
// Convolution reverb voice for the `FxStep::ConvReverb` dispatch.
//
// Phase 1 ships the plumbing only: predelay line + tonal shaping (damp /
// lowcut) + dry/wet mix.  The `ir` slot and stereo `side` field are
// pre-allocated so Phase 2 can drop in partitioned overlap-save
// convolution against a user-loaded impulse response without touching
// the call sites.  No allocations inside `process()`.

use std::sync::Arc;

/// Predelay ring buffer length — covers the full 0..200 ms knob range at
/// the 48 kHz engine and any reasonable 44.1 kHz setting with headroom.
pub const CONV_REVERB_PREDELAY_LEN: usize = 16384;

/// Maximum predelay mapped from the `conv_reverb_predelay` knob (0..1).
pub const CONV_REVERB_MAX_PREDELAY_SEC: f32 = 0.2;

pub struct ConvReverb {
    predelay_buf: Vec<f32>,
    predelay_head: usize,
    lp_state: f32,
    hp_lp_state: f32,
    /// Loaded impulse-response samples.  Phase 2 populates this via
    /// `load_ir`; Phase 1 leaves it `None` and the `process` path falls
    /// back to filtered predelayed dry so the module is still audibly
    /// patchable end-to-end.
    #[allow(dead_code)]
    ir: Option<Arc<Vec<f32>>>,
    /// IR channel layout — 1 = mono, 2 = interleaved stereo.  Drives
    /// Phase 2's stereo-width expansion path.
    #[allow(dead_code)]
    ir_channels: u8,
    /// Whether the IR was stored reversed at load time.  Recomputing the
    /// IR when this toggles avoids a per-sample branch in Phase 2.
    #[allow(dead_code)]
    ir_reversed: bool,
    /// Per-sample side contribution latched for the master mixer — it
    /// gets added to the stereo side bus like `fx_pan_side`.  Phase 1
    /// parks it at zero; Phase 2 writes a true (wetL − wetR) * width / 2
    /// signal from the stereo IR convolution.
    pub side: f32,
}

impl ConvReverb {
    pub fn new() -> Self {
        Self {
            predelay_buf: vec![0.0; CONV_REVERB_PREDELAY_LEN],
            predelay_head: 0,
            lp_state: 0.0,
            hp_lp_state: 0.0,
            ir: None,
            ir_channels: 0,
            ir_reversed: false,
            side: 0.0,
        }
    }

    /// Swap in a new impulse response.  Called outside `process` (audio-
    /// command handler), so allocation is fine here.  Phase 2 will also
    /// recompute the FFT partition cache from `data` here.
    #[allow(dead_code)]
    pub fn load_ir(&mut self, data: Arc<Vec<f32>>, channels: u8, reversed: bool) {
        self.ir = Some(data);
        self.ir_channels = channels.clamp(1, 2);
        self.ir_reversed = reversed;
    }

    /// Clear the loaded IR (no-op in the wet path since Phase 1 never
    /// reads it).  Phase 2 uses this to drop the FFT partition cache.
    #[allow(dead_code)]
    pub fn clear_ir(&mut self) {
        self.ir = None;
        self.ir_channels = 0;
        self.ir_reversed = false;
    }

    /// Process one sample.  Mono in / mono out — the stereo side
    /// contribution is latched into `self.side` for the master mixer.
    ///
    /// Phase 1 stub: wet = predelayed dry, band-limited by damp / lowcut.
    /// Phase 2 replaces the wet path with partitioned overlap-save
    /// convolution against the loaded IR.
    #[allow(clippy::too_many_arguments)]
    pub fn process(
        &mut self,
        sig: f32,
        mix: f32,
        predelay: f32,
        damp: f32,
        lowcut: f32,
        _size: f32,
        _width: f32,
        sr: f32,
    ) -> f32 {
        let len = self.predelay_buf.len();
        if mix < 0.001 {
            self.side = 0.0;
            // Keep the delay line advancing so the first non-zero mix
            // doesn't read stale audio from before the knob was opened.
            self.predelay_buf[self.predelay_head] = sig;
            self.predelay_head = (self.predelay_head + 1) % len;
            return sig;
        }

        let pd_samples = (predelay.clamp(0.0, 1.0) * CONV_REVERB_MAX_PREDELAY_SEC * sr) as usize;
        let pd_samples = pd_samples.min(len - 1);
        let read_idx = (self.predelay_head + len - pd_samples) % len;
        let delayed = self.predelay_buf[read_idx];
        self.predelay_buf[self.predelay_head] = sig;
        self.predelay_head = (self.predelay_head + 1) % len;

        let mut wet = delayed;

        let damp = damp.clamp(0.0, 1.0);
        if damp > 0.001 {
            let fc = 20_000.0 * (400.0_f32 / 20_000.0).powf(damp);
            let a = 1.0 - (-std::f32::consts::TAU * fc / sr).exp();
            self.lp_state += a * (wet - self.lp_state);
            wet = self.lp_state;
        } else {
            self.lp_state = wet;
        }

        let lowcut = lowcut.clamp(0.0, 1.0);
        if lowcut > 0.001 {
            let fc = 20.0 * (800.0_f32 / 20.0).powf(lowcut);
            let a = 1.0 - (-std::f32::consts::TAU * fc / sr).exp();
            self.hp_lp_state += a * (wet - self.hp_lp_state);
            wet -= self.hp_lp_state;
        } else {
            self.hp_lp_state = wet;
        }

        self.side = 0.0;

        let mix = mix.clamp(0.0, 1.0);
        sig * (1.0 - mix) + wet * mix
    }
}

impl Default for ConvReverb {
    fn default() -> Self {
        Self::new()
    }
}
