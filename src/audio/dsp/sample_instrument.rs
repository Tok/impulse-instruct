// ─── audio/dsp/sample_instrument.rs ──────────────────────────────────────────
// Pitched sample-playback voice — load a single recording and replay it
// at a pitch derived from the played sequencer note (relative to a
// configurable root note).
//
// V1 is intentionally minimal:
//   • Linear-interp resampling (cheap; 4-pt Hermite is a future enhancement).
//   • Simple AR amp envelope (matches WavetableVoice / Pluck).
//   • Always-loops the loaded sample (loop start/end markers planned for V1.1).
//   • Monophonic — a new trigger steals the running voice.
//
// The struct mirrors `WavetableVoice` so the surrounding plumbing
// (load/trigger/process) is familiar; the difference is that we resample
// the entire buffer rather than scanning fixed-size frames.

use std::sync::Arc;

use super::AudioParams;
use super::dsp_util::{TuningSystem, midi_to_hz_tuned};

pub struct SampleInstrumentVoice {
    /// Loaded mono sample data.  Empty when nothing is loaded.
    samples: Arc<Vec<f32>>,
    /// Read position in samples (fractional for linear interpolation).
    pos: f32,
    /// Source-recording reference Hz — set on `load()` from the user's
    /// `root_note` so the pitch ratio is always note-relative.  Defaults
    /// to C4 frequency until a sample is loaded.
    root_freq: f32,
    /// Current playback frequency (Hz) — set on trigger.
    freq: f32,
    /// Amp envelope state (same shape as WavetableVoice / Pluck).
    amp_env: f32,
    gate: bool,
    /// Per-step accent (0..=1).
    accent: f32,
}

impl SampleInstrumentVoice {
    pub fn new() -> Self {
        Self {
            samples: Arc::new(Vec::new()),
            pos: 0.0,
            root_freq: 261.625_56, // C4 — overwritten when the user sets root_note
            freq: 0.0,
            amp_env: 0.0,
            gate: false,
            accent: 0.0,
        }
    }

    /// Swap in a freshly-loaded mono sample buffer.  `data` must already
    /// be at the engine sample rate.
    pub fn load(&mut self, data: Arc<Vec<f32>>) {
        self.samples = data;
        self.pos = 0.0;
        self.amp_env = 0.0;
    }

    /// Update the source-recording root note (e.g. user re-tunes the
    /// instrument).  Recalculates `root_freq` so future triggers shift
    /// relative to the new reference.
    pub fn set_root_note(&mut self, midi: u8, tuning: TuningSystem) {
        self.root_freq = midi_to_hz_tuned(midi, tuning).max(20.0);
    }

    pub fn trigger(
        &mut self,
        note: u8,
        tuning: TuningSystem,
        accent: f32,
        slide: f32,
        pitch_offset_cents: f32,
    ) {
        let _ = slide;
        let cents_ratio = 2.0_f32.powf(pitch_offset_cents / 1200.0);
        self.freq = (midi_to_hz_tuned(note, tuning) * cents_ratio).max(20.0);
        self.pos = 0.0;
        self.amp_env = 0.0;
        self.gate = true;
        self.accent = accent.clamp(0.0, 1.0);
    }

    pub fn gate_off(&mut self) {
        self.gate = false;
    }

    pub fn process(&mut self, sr: f32, p: &AudioParams) -> f32 {
        let n = self.samples.len();
        if n < 2 || (self.amp_env < 1e-6 && !self.gate) {
            return 0.0;
        }

        // AR envelope — fast attack, slow release (matches WavetableVoice).
        if self.gate {
            let atk = (-1.0_f32 / (0.003 * sr)).exp();
            self.amp_env = 1.0 - (1.0 - self.amp_env) * atk;
        } else {
            let rel = (-1.0_f32 / (0.1 * sr)).exp();
            self.amp_env *= rel;
        }

        // Resample rate: how many source samples per output sample.
        // ratio = freq / root_freq — at unity (played note == root) we
        // step one sample per output, preserving original pitch.
        let rate = (self.freq / self.root_freq).clamp(0.05, 64.0);

        // Read current position with linear interpolation.
        let i0 = self.pos as usize;
        let frac = self.pos - i0 as f32;
        let s0 = self.samples[i0 % n];
        let s1 = self.samples[(i0 + 1) % n];
        let sample = s0 + (s1 - s0) * frac;

        // Advance.  Loop the entire buffer for V1.
        self.pos += rate;
        while self.pos >= n as f32 {
            self.pos -= n as f32;
        }

        let accent_gain = 1.0 + self.accent * 0.4;
        sample * self.amp_env * accent_gain * p.sample_volume.clamp(0.0, 1.5)
    }
}

impl Default for SampleInstrumentVoice {
    fn default() -> Self {
        Self::new()
    }
}
