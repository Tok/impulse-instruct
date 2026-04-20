// ─── Granular texture voice ──────────────────────────────────────────────────
// Standalone granular synthesis voice — overlapping short grains with
// per-grain rate, position, and pan, summed with a Hann window.  Extracted
// from samplers.rs so that file stays under the 1000-line cap now that the
// amen voice's pitch-preserving stretcher has grown.

use std::sync::Arc;

use super::voices::NoiseGen;

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

pub(crate) struct GranularVoice {
    samples: Option<Arc<Vec<f32>>>,
    grains: [Grain; MAX_GRAINS],
    spawn_counter: f32, // counts down to next grain spawn
    rng: NoiseGen,
}

impl GranularVoice {
    pub(crate) fn new(seed: u32) -> Self {
        Self {
            samples: None,
            grains: [Grain::default(); MAX_GRAINS],
            spawn_counter: 0.0,
            rng: NoiseGen::new(seed),
        }
    }

    pub(crate) fn load(&mut self, data: Arc<Vec<f32>>) {
        self.samples = Some(data);
        for g in &mut self.grains {
            g.active = false;
        }
    }

    /// Render one stereo sample pair from all active grains.
    /// Per-grain pan values are applied using equal-power panning.
    pub(crate) fn process(
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
