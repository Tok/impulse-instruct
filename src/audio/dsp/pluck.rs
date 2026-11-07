// ─── audio/dsp/pluck.rs ──────────────────────────────────────────────────────
// Karplus-Strong plucked-string voice.
//
// Algorithm:
//   1. On trigger: compute delay-line length L = round(sr / freq).
//      Fill buf[0..L] with white noise (the "plectrum" excitation).
//   2. Each sample: y = buf[read]; filter = (y + prev) * 0.5 * damping;
//      buf[write] = filter; advance read/write pointers modulo L.
//   3. The per-pass lowpass gradually rounds off the spectrum, so the
//      pluck decays from bright → mellow → silent over the decay time.
//
// Output passes through a one-pole lowpass controlled by `brightness`
// so the user can tame the raw edge without shortening the tail.  An
// AMP envelope provides soft attack so retriggers don't click.

use super::AudioParams;
use super::dsp_util::{TuningSystem, midi_to_hz_tuned, one_pole_coef, one_pole_lp_alpha};

/// Max delay-line length — sized for the lowest musically useful
/// fundamental we expect.  At 48 kHz a 4096-sample delay line covers
/// frequencies down to ~12 Hz, well below MIDI 0 (~8 Hz).
const PLUCK_MAX_LEN: usize = 4096;

pub struct PluckVoice {
    buf: Vec<f32>,
    /// Active delay-line length for the current note (`read` wraps
    /// modulo this, not `buf.len()`).  Re-computed per trigger.
    len: usize,
    /// Read index into `buf` — the classic K-S algorithm reads and
    /// then overwrites the SAME slot, so one pointer covers both.
    read: usize,
    /// Previous output sample — feeds the 2-tap averaging filter.
    prev_y: f32,
    /// Amp envelope — smooths retrigger clicks.  Climbs to ~1.0 on
    /// trigger, released when `gate == false`.
    amp_env: f32,
    gate: bool,
    /// Output one-pole LP state for the brightness knob.
    bright_lp: f32,
    /// Per-step accent, applied multiplicatively to the amp env peak.
    accent: f32,
    /// Deterministic xorshift state for the excitation noise — per-
    /// voice so two PluckVoice instances don't phase-lock.
    rng: u32,
}

impl PluckVoice {
    pub fn new() -> Self {
        Self {
            buf: vec![0.0; PLUCK_MAX_LEN],
            len: 2,
            read: 0,
            prev_y: 0.0,
            amp_env: 0.0,
            gate: false,
            bright_lp: 0.0,
            accent: 0.0,
            rng: 0xCAFE_F00D,
        }
    }

    /// Re-prime the delay line for a fresh pluck.  Called from the
    /// audio-command handler (outside `process`) — the fill loop
    /// doesn't allocate (buffer is a pre-allocated Vec sized at new).
    pub fn trigger(
        &mut self,
        note: u8,
        tuning: TuningSystem,
        accent: f32,
        slide: f32,
        sr: f32,
        pitch_offset_semi: f32,
    ) {
        let _ = slide; // reserved for future legato mode
        let offset_ratio = 2.0_f32.powf(pitch_offset_semi / 12.0);
        let freq = (midi_to_hz_tuned(note, tuning) * offset_ratio).max(20.0);
        // Delay-line length sets the fundamental: L samples round-trip
        // at sr Hz gives a period of L/sr → frequency of sr/L.
        let len = ((sr / freq).round() as usize).clamp(16, self.buf.len());
        self.len = len;
        self.read = 0;
        self.prev_y = 0.0;
        // Fill with white noise — xorshift32 is plenty random for a
        // one-shot burst, no allocator calls inside the audio thread.
        for i in 0..len {
            self.buf[i] = self.next_noise();
        }
        // Zero any stale tail past the new active region so a shorter
        // trigger after a longer one doesn't read old content on wrap.
        for i in len..self.buf.len() {
            self.buf[i] = 0.0;
        }
        self.amp_env = 0.0;
        self.gate = true;
        self.accent = accent.clamp(0.0, 1.0);
    }

    pub fn gate_off(&mut self) {
        self.gate = false;
    }

    /// xorshift32-based white-noise source for the excitation burst.
    /// Keeps a per-voice RNG state so re-triggers yield fresh noise
    /// without calling the global rand or allocating.
    fn next_noise(&mut self) -> f32 {
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x;
        // Map u32 into ±1.  Scale down to ~0.6 so the peak isn't so
        // hot it immediately clips the master — the pluck still has
        // plenty of transient punch.
        ((x as f32 / u32::MAX as f32) * 2.0 - 1.0) * 0.6
    }

    pub fn process(&mut self, sr: f32, p: &AudioParams) -> f32 {
        if self.amp_env < 1e-6 && !self.gate {
            return 0.0;
        }

        // Amp envelope: fast attack (~3 ms), slow release (~100 ms)
        // — the body of the pluck's decay lives in the delay-line
        // feedback itself, this envelope only hides retrigger clicks.
        if self.gate {
            let atk = one_pole_coef(0.003, sr);
            self.amp_env = 1.0 - (1.0 - self.amp_env) * atk;
        } else {
            let rel = one_pole_coef(0.1, sr);
            self.amp_env *= rel;
        }

        // Karplus-Strong feedback step.  Classic form: read the
        // oldest slot, write the filtered value back into the SAME
        // slot (one pointer, one tap).  After `len` samples every
        // slot has been replaced by the filtered version, which
        // gradually rounds the noise-burst spectrum into a decaying
        // pitched tone.  `damping` maps to a feedback coefficient
        // close to 1.0 — high values preserve every pass (long
        // sustain), low values kill the tone quickly.
        let damping = 0.92 + p.pluck_damping.clamp(0.0, 1.0) * 0.075;
        let len = self.len.max(1);
        let y = self.buf[self.read];
        let filt = (y + self.prev_y) * 0.5 * damping;
        self.prev_y = y;
        self.buf[self.read] = filt;
        self.read = (self.read + 1) % len;

        // Brightness: one-pole LP on the OUTPUT tap, independent of
        // the feedback lowpass inside the loop so users can tame the
        // raw edge without shortening the decay.  Fast coefficient
        // math in the hot path — no `exp` per sample.
        let bright = p.pluck_brightness.clamp(0.0, 1.0);
        let fc = 400.0 + bright * 15_000.0;
        let coeff = one_pole_lp_alpha(fc, sr);
        self.bright_lp += coeff * (y - self.bright_lp);

        // Accent scales the final output, matching the convention the
        // other melodic voices use.  1 + accent * 0.4 leaves accent=0
        // at baseline and gives a 4 dB lift at accent=1.
        let accent_gain = 1.0 + self.accent * 0.4;
        self.bright_lp * self.amp_env * accent_gain * p.pluck_volume.clamp(0.0, 1.5)
    }
}

impl Default for PluckVoice {
    fn default() -> Self {
        Self::new()
    }
}
