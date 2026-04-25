// ─── audio/dsp/wavetable.rs ──────────────────────────────────────────────────
// Wavetable synthesis voice.
//
// The user-supplied WAV is split at load time into fixed-size frames
// of `WT_FRAME_SIZE` samples (Serum convention).  At play time the
// `position` knob picks a fractional frame index; the oscillator
// reads two adjacent frames at the same fractional phase and
// linearly interpolates between them, so sweeping the knob morphs
// smoothly through the wavetable instead of stepping.
//
// `phase_offset` shifts the read inside the frame (0..1 → 0..2π) so
// the user can detune the cycle's start without retriggering.  The
// pitch lock is `freq * WT_FRAME_SIZE / sr` samples-per-output —
// frame size is the period at the reference frequency.

use std::sync::Arc;

use super::AudioParams;
use super::dsp_util::{TuningSystem, midi_to_hz_tuned};

/// Single-cycle frame size.  2048 matches Serum's de-facto convention
/// and gives a fundamental of `sr / 2048` ≈ 23.4 Hz at 48 kHz when
/// the table is read at unity speed — comfortably below the lowest
/// MIDI note we expect to play.
pub const WT_FRAME_SIZE: usize = 2048;

pub struct WavetableVoice {
    /// Loaded frames.  Empty when no wavetable is loaded; the voice
    /// processes to silence in that case.
    frames: Arc<Vec<f32>>,
    frame_count: usize,
    /// Read position within the current frame (0..WT_FRAME_SIZE),
    /// fractional for linear interpolation.
    phase: f32,
    /// Currently playing frequency in Hz — set on trigger.
    freq: f32,
    /// Amp envelope state — fast attack, slow release like Pluck.
    amp_env: f32,
    gate: bool,
    /// Per-step accent (0..=1).
    accent: f32,
}

impl WavetableVoice {
    pub fn new() -> Self {
        Self {
            frames: Arc::new(Vec::new()),
            frame_count: 0,
            phase: 0.0,
            freq: 0.0,
            amp_env: 0.0,
            gate: false,
            accent: 0.0,
        }
    }

    /// Swap in new sample data.  Called outside `process` (audio-
    /// command handler) so the Arc allocation + frame split is fine
    /// here.  `data` is a mono WAV resampled to the engine rate;
    /// frames are derived by chunking into `WT_FRAME_SIZE` blocks.
    pub fn load(&mut self, data: Arc<Vec<f32>>) {
        let frame_count = data.len() / WT_FRAME_SIZE;
        self.frames = data;
        self.frame_count = frame_count;
    }

    /// Drop the loaded wavetable — the voice falls silent.
    #[allow(dead_code)]
    pub fn clear(&mut self) {
        self.frames = Arc::new(Vec::new());
        self.frame_count = 0;
    }

    pub fn trigger(
        &mut self,
        note: u8,
        tuning: TuningSystem,
        accent: f32,
        slide: f32,
        pitch_offset_semi: f32,
    ) {
        let _ = slide;
        let offset_ratio = 2.0_f32.powf(pitch_offset_semi / 12.0);
        self.freq = (midi_to_hz_tuned(note, tuning) * offset_ratio).max(20.0);
        self.phase = 0.0;
        self.amp_env = 0.0;
        self.gate = true;
        self.accent = accent.clamp(0.0, 1.0);
    }

    pub fn gate_off(&mut self) {
        self.gate = false;
    }

    pub fn process(&mut self, sr: f32, p: &AudioParams) -> f32 {
        if self.frame_count == 0 || (self.amp_env < 1e-6 && !self.gate) {
            return 0.0;
        }

        // Amp envelope — same shape as Pluck.
        if self.gate {
            let atk = (-1.0_f32 / (0.003 * sr)).exp();
            self.amp_env = 1.0 - (1.0 - self.amp_env) * atk;
        } else {
            let rel = (-1.0_f32 / (0.1 * sr)).exp();
            self.amp_env *= rel;
        }

        // Frame-rate phase advance.  Treats `WT_FRAME_SIZE` samples
        // as one full cycle: at freq Hz we want freq cycles per
        // second → freq * WT_FRAME_SIZE samples-of-table per second.
        let phase_inc = self.freq * WT_FRAME_SIZE as f32 / sr;
        self.phase += phase_inc;
        let frame_size_f = WT_FRAME_SIZE as f32;
        if self.phase >= frame_size_f {
            self.phase -= frame_size_f;
        }

        // Phase offset (0..1) wraps into the table cycle.
        let phase_off_samples = p.wavetable_phase_offset.clamp(0.0, 1.0) * frame_size_f;
        let read_phase = (self.phase + phase_off_samples) % frame_size_f;

        // Fractional frame index — `position` 0..1 maps to 0..N-1
        // so the user can sweep the full table edge-to-edge with a
        // single full knob rotation.
        let pos = p.wavetable_position.clamp(0.0, 1.0);
        let frame_f = pos * (self.frame_count.saturating_sub(1)) as f32;
        let frame_a = frame_f as usize;
        let frame_b = (frame_a + 1).min(self.frame_count.saturating_sub(1));
        let frame_frac = frame_f - frame_a as f32;

        // Linear-interp read inside each frame.
        let i0 = read_phase as usize;
        let i1 = (i0 + 1) % WT_FRAME_SIZE;
        let phase_frac = read_phase - i0 as f32;
        let sample_at = |frame_idx: usize| -> f32 {
            let base = frame_idx * WT_FRAME_SIZE;
            let a = self.frames.get(base + i0).copied().unwrap_or(0.0);
            let b = self.frames.get(base + i1).copied().unwrap_or(0.0);
            a + (b - a) * phase_frac
        };
        let s_a = sample_at(frame_a);
        let s_b = sample_at(frame_b);
        let sample = s_a + (s_b - s_a) * frame_frac;

        let accent_gain = 1.0 + self.accent * 0.4;
        sample * self.amp_env * accent_gain * p.wavetable_volume.clamp(0.0, 1.5)
    }
}

impl Default for WavetableVoice {
    fn default() -> Self {
        Self::new()
    }
}
