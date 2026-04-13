// ─── Sampler-based voices ────────────────────────────────────────────────────
// Amen/WAV playback + granular texture — separated from voices.rs to stay
// under the 1000-line limit.

use std::sync::Arc;

use super::voices::NoiseGen;

// ─── Amen / WAV sampler voice ─────────────────────────────────────────────────

/// Slice-aware sample playback.  Holds a pre-loaded mono f32 WAV (Arc) and
/// a trigger model where each fire plays one slice of the sample, optionally
/// reversed, with a gate (fraction of slice duration) and stutter (extra
/// retriggers of the same slice).  Allocation-free during playback.
pub(super) struct AmenVoice {
    samples: Option<Arc<Vec<f32>>>,
    /// Current read position (fractional).  Advances by `rate` per sample.
    pos: f32,
    /// End position of the current slice (in samples).  Playback stops when
    /// pos (in forward mode) crosses this, or dips below slice_start in
    /// reverse mode.
    slice_end: f32,
    /// Start position of the current slice (used for reverse + looping).
    slice_start: f32,
    /// Position at which the gate cuts (always in the forward direction of
    /// slice playback).  Equal to slice_end when gate == 1.0.
    gate_end: f32,
    /// Direction: 1.0 = forward, -1.0 = reverse.  Set at trigger time from
    /// the `reverse` param.
    direction: f32,
    /// Stutter retriggers remaining (0 = no more).
    stutter_left: u8,
    /// Auto-advance counter for slice-index 0 ("pick next slice").
    auto_slice: u8,
    playing: bool,
}

impl AmenVoice {
    pub(super) fn new() -> Self {
        Self {
            samples: None,
            pos: 0.0,
            slice_end: 0.0,
            slice_start: 0.0,
            gate_end: 0.0,
            direction: 1.0,
            stutter_left: 0,
            auto_slice: 0,
            playing: false,
        }
    }

    /// Replace the sample data (called from the audio command handler, not process_block).
    pub(super) fn load(&mut self, data: Arc<Vec<f32>>) {
        self.samples = Some(data);
        self.playing = false;
        self.pos = 0.0;
        self.auto_slice = 0;
    }

    /// Trigger playback of a single slice with the given parameters.
    /// - `slice_idx` — 0 means auto-advance (voice picks next slice), 1..=slice_count
    ///   selects that 1-based slice explicitly.  Values > slice_count are wrapped.
    /// - `slice_count` — how many equal slices to divide the usable region into.
    /// - `start_offset`, `end_offset` — usable region of the sample (0..1 of total).
    /// - `reverse` — play the slice from end to start.
    /// - `gate` — 0..1, fraction of the slice that actually plays.
    /// - `stutter` — extra retriggers of this same slice (0 = play once).
    pub(super) fn trigger(
        &mut self,
        slice_idx: u8,
        slice_count: u8,
        start_offset: f32,
        end_offset: f32,
        reverse: bool,
        gate: f32,
        stutter: u8,
    ) {
        let Some(samples) = self.samples.as_ref() else {
            return;
        };
        let n = samples.len() as f32;
        if n < 2.0 {
            return;
        }
        let slices = slice_count.max(1);
        let region_start = (start_offset.clamp(0.0, 1.0) * n).floor();
        let region_end = (end_offset.clamp(0.0, 1.0) * n)
            .floor()
            .max(region_start + 1.0);
        let region_len = region_end - region_start;
        let slice_len = region_len / slices as f32;

        // Resolve slice index.  slice_idx 0 means auto-advance.
        let idx0 = if slice_idx == 0 {
            let i = self.auto_slice % slices;
            self.auto_slice = (self.auto_slice + 1) % slices;
            i
        } else {
            (slice_idx - 1) % slices
        };

        let sstart = region_start + idx0 as f32 * slice_len;
        let send = sstart + slice_len;
        let gate_frac = gate.clamp(0.05, 1.0);

        self.slice_start = sstart;
        self.slice_end = send;
        self.direction = if reverse { -1.0 } else { 1.0 };
        if reverse {
            self.pos = send - 1.0;
            self.gate_end = send - slice_len * gate_frac;
        } else {
            self.pos = sstart;
            self.gate_end = sstart + slice_len * gate_frac;
        }
        self.stutter_left = stutter;
        self.playing = true;
    }

    /// Convenience for call sites that want the legacy "play whole sample"
    /// behavior (used by process() when slice_count == 1 with no args,
    /// and for tests).  Preserved for backward compatibility.
    #[allow(dead_code)]
    pub(super) fn trigger_whole(&mut self) {
        self.trigger(1, 1, 0.0, 1.0, false, 1.0, 0);
    }

    /// Render one sample. `pitch_semitones` shifts playback speed (±24 st);
    /// positive = faster/higher, negative = slower/lower.  `loop_mode`
    /// restarts the current slice instead of stopping when it ends —
    /// useful for sustained pad-style playback, less common for breaks.
    pub(super) fn process(&mut self, pitch_semitones: f32, volume: f32, loop_mode: bool) -> f32 {
        let samples = match &self.samples {
            Some(s) => s,
            None => return 0.0,
        };
        if !self.playing {
            return 0.0;
        }
        let rate = 2.0_f32.powf(pitch_semitones / 12.0) * self.direction;

        // Gate / end-of-slice handling.
        let forward = self.direction > 0.0;
        let ended = if forward {
            self.pos >= self.gate_end || self.pos as usize + 1 >= samples.len()
        } else {
            self.pos <= self.gate_end || self.pos < 1.0
        };
        if ended {
            if self.stutter_left > 0 {
                self.stutter_left -= 1;
                if forward {
                    self.pos = self.slice_start;
                } else {
                    self.pos = self.slice_end - 1.0;
                }
            } else if loop_mode {
                if forward {
                    self.pos = self.slice_start;
                } else {
                    self.pos = self.slice_end - 1.0;
                }
            } else {
                self.playing = false;
                return 0.0;
            }
        }

        let idx = self.pos as usize;
        if idx + 1 >= samples.len() {
            self.playing = false;
            return 0.0;
        }
        let frac = self.pos - idx as f32;
        let out = samples[idx] + (samples[idx + 1] - samples[idx]) * frac;
        self.pos += rate;
        out * volume
    }
}

// ─── Granular texture voice ─────────────────────────────────────────────────

const MAX_GRAINS: usize = 32;

/// A single active grain with its playback state.
#[derive(Clone, Copy)]
struct Grain {
    pos: f32,     // current position in sample buffer (fractional)
    rate: f32,    // playback rate (1.0 = normal pitch)
    age: f32,     // 0→1 progress through the grain window
    inv_len: f32, // 1.0 / grain_length_samples
    pan: f32,     // -1..+1 stereo position
    active: bool,
}

impl Default for Grain {
    fn default() -> Self {
        Self {
            pos: 0.0,
            rate: 1.0,
            age: 0.0,
            inv_len: 1.0,
            pan: 0.0,
            active: false,
        }
    }
}

pub(super) struct GranularVoice {
    samples: Option<Arc<Vec<f32>>>,
    grains: [Grain; MAX_GRAINS],
    spawn_counter: f32, // counts down to next grain spawn
    rng: NoiseGen,
}

impl GranularVoice {
    pub(super) fn new(seed: u32) -> Self {
        Self {
            samples: None,
            grains: [Grain::default(); MAX_GRAINS],
            spawn_counter: 0.0,
            rng: NoiseGen::new(seed),
        }
    }

    pub(super) fn load(&mut self, data: Arc<Vec<f32>>) {
        self.samples = Some(data);
        for g in &mut self.grains {
            g.active = false;
        }
    }

    /// Render one stereo sample pair from all active grains.
    /// Per-grain pan values are applied using equal-power panning.
    pub(super) fn process(
        &mut self,
        volume: f32,
        density: f32,
        grain_size: f32,
        position: f32,
        jitter: f32,
        pitch_scatter: f32,
        sr: f32,
    ) -> (f32, f32) {
        let samples = match &self.samples {
            Some(s) if !s.is_empty() => s,
            _ => return (0.0, 0.0),
        };
        if volume < 0.001 {
            return (0.0, 0.0);
        }

        let buf_len = samples.len() as f32;

        // Spawn new grains based on density (1–40 grains/sec)
        let grains_per_sec = 1.0 + density * 39.0;
        self.spawn_counter -= 1.0;
        if self.spawn_counter <= 0.0 {
            self.spawn_counter = sr / grains_per_sec;
            // Find a free grain slot
            if let Some(g) = self.grains.iter_mut().find(|g| !g.active) {
                let size_ms = 10.0 + grain_size * 490.0; // 10–500 ms
                let grain_samples = (size_ms * 0.001 * sr).max(1.0);
                let rng_val = self.rng.next(); // -1..+1
                let jitter_offset = rng_val * jitter * 0.25 * buf_len;
                let start = (position * buf_len + jitter_offset).rem_euclid(buf_len);
                let rng_pitch = self.rng.next();
                let pitch_st = rng_pitch * pitch_scatter * 12.0; // ±12 st
                let rate = 2.0_f32.powf(pitch_st / 12.0);
                let rng_pan = self.rng.next();

                g.pos = start;
                g.rate = rate;
                g.age = 0.0;
                g.inv_len = 1.0 / grain_samples;
                g.pan = rng_pan; // used for stereo later; mono mix ignores it
                g.active = true;
            }
        }

        // Mix all active grains with per-grain panning
        let (mut out_l, mut out_r) = (0.0_f32, 0.0_f32);
        for g in &mut self.grains {
            if !g.active {
                continue;
            }
            // Hann window: sin²(π × age)
            let window = {
                let x = g.age * std::f32::consts::PI;
                let s = x.sin();
                s * s
            };
            // Linear interpolation from buffer
            let idx = g.pos as usize;
            if idx + 1 < samples.len() {
                let frac = g.pos - idx as f32;
                let sample = samples[idx] + (samples[idx + 1] - samples[idx]) * frac;
                let amp = sample * window;
                // Equal-power pan: pan ∈ -1..+1 → L/R gain
                let pan_r = (g.pan + 1.0) * 0.5; // 0..1
                let pan_l = 1.0 - pan_r;
                out_l += amp * pan_l;
                out_r += amp * pan_r;
            }
            g.pos = (g.pos + g.rate).rem_euclid(buf_len);
            g.age += g.inv_len;
            if g.age >= 1.0 {
                g.active = false;
            }
        }

        let gain = volume * 0.3; // scale down — many overlapping grains can be loud
        (out_l * gain, out_r * gain)
    }
}
