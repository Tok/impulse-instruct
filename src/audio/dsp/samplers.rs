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
    /// Additional pitch offset in semitones applied on top of the caller's
    /// pitch_semitones — set at trigger time from per-slice overrides
    /// and/or BPM stretch.
    extra_pitch: f32,
    /// Volume multiplier for the current slice — set at trigger from per-
    /// slice overrides (default 1.0).
    slice_volume: f32,
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
            extra_pitch: 0.0,
            slice_volume: 1.0,
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
    #[allow(clippy::too_many_arguments)]
    pub(super) fn trigger(
        &mut self,
        slice_idx: u8,
        slice_count: u8,
        start_offset: f32,
        end_offset: f32,
        reverse: bool,
        gate: f32,
        stutter: u8,
        slice_positions: &[f32; 16],
        slice_pitches: &[f32; 16],
        slice_volumes: &[f32; 16],
        bpm_stretch: bool,
        source_bpm: f32,
        sequencer_bpm: f32,
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
        let slice_len_equal = region_len / slices as f32;

        // Resolve slice index.  slice_idx 0 means auto-advance.
        let idx0 = if slice_idx == 0 {
            let i = self.auto_slice % slices;
            self.auto_slice = (self.auto_slice + 1) % slices;
            i
        } else {
            (slice_idx - 1) % slices
        };

        // Use custom positions when they're populated (entry 0 is NaN
        // sentinel for "unused").  Positions are normalized 0..1 of the
        // full sample and must be in ascending order.
        let use_custom = !slice_positions[0].is_nan();
        let (sstart, send) = if use_custom {
            let a = slice_positions[idx0 as usize];
            let b_idx = (idx0 as usize + 1).min(15);
            let b = if (idx0 as usize + 1) < slices as usize && !slice_positions[b_idx].is_nan() {
                slice_positions[b_idx]
            } else {
                end_offset.clamp(0.0, 1.0)
            };
            (a * n, b * n)
        } else {
            let s0 = region_start + idx0 as f32 * slice_len_equal;
            (s0, s0 + slice_len_equal)
        };
        let gate_frac = gate.clamp(0.05, 1.0);
        let slice_len = (send - sstart).max(1.0);
        // Stutter divides the slice budget so N retriggers fit inside one
        // slice duration instead of extending past it.  stutter=0 → full
        // slice; stutter=4 → five hits crammed into one slice's worth.
        let sub_len = (slice_len / (stutter as f32 + 1.0)).max(1.0);
        let gate_window = sub_len * gate_frac;

        self.slice_start = sstart;
        self.slice_end = send;
        self.direction = if reverse { -1.0 } else { 1.0 };
        if reverse {
            self.pos = send - 1.0;
            self.gate_end = send - gate_window;
        } else {
            self.pos = sstart;
            self.gate_end = sstart + gate_window;
        }
        self.stutter_left = stutter;

        // Compute extra pitch from BPM stretch + per-slice override.
        let mut extra = 0.0_f32;
        if bpm_stretch && source_bpm > 1.0 && sequencer_bpm > 1.0 {
            // rate = host/source → pitch shift in semitones = 12 * log2(rate)
            extra += 12.0 * (sequencer_bpm / source_bpm).log2();
        }
        if !slice_pitches[0].is_nan()
            && let Some(&sp) = slice_pitches.get(idx0 as usize)
            && !sp.is_nan()
        {
            extra += sp;
        }
        self.extra_pitch = extra;
        self.slice_volume = if !slice_volumes[0].is_nan() {
            slice_volumes
                .get(idx0 as usize)
                .copied()
                .filter(|v| !v.is_nan())
                .unwrap_or(1.0)
        } else {
            1.0
        };
        self.playing = true;
    }

    /// Convenience for call sites that want the legacy "play whole sample"
    /// behavior (used by process() when slice_count == 1 with no args,
    /// and for tests).  Preserved for backward compatibility.
    #[allow(dead_code)]
    pub(super) fn trigger_whole(&mut self) {
        let nan16 = [f32::NAN; 16];
        self.trigger(
            1, 1, 0.0, 1.0, false, 1.0, 0, &nan16, &nan16, &nan16, false, 136.0, 170.0,
        );
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
        let rate = 2.0_f32.powf((pitch_semitones + self.extra_pitch) / 12.0) * self.direction;

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

        // Clamp the index so reverse playback can safely start at
        // pos == send - 1 (which would otherwise sit on the last index
        // and trip an out-of-bounds neighbour read for interpolation).
        // The forward gate_end / reverse pos<1 checks above are the
        // real termination conditions.
        let len = samples.len();
        let idx = (self.pos as usize).min(len.saturating_sub(1));
        let frac = (self.pos - idx as f32).clamp(0.0, 1.0);
        let next = samples.get(idx + 1).copied().unwrap_or(samples[idx]);
        let out = samples[idx] + (next - samples[idx]) * frac;
        self.pos += rate;
        out * volume * self.slice_volume
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a ramp sample [0, 1, 2, …, n-1] as f32 so the read position
    /// is recoverable from each output value.
    fn ramp_sample(n: usize) -> Arc<Vec<f32>> {
        Arc::new((0..n).map(|i| i as f32).collect())
    }

    fn nan16() -> [f32; 16] {
        [f32::NAN; 16]
    }

    fn render(voice: &mut AmenVoice, n: usize) -> Vec<f32> {
        (0..n).map(|_| voice.process(0.0, 1.0, false)).collect()
    }

    #[test]
    fn trigger_whole_plays_full_sample_forward() {
        let mut v = AmenVoice::new();
        v.load(ramp_sample(8));
        v.trigger_whole();
        let out = render(&mut v, 8);
        // Reads 0..7 with linear interp; last sample stops because
        // idx+1 hits sample length.  First five should be 0..4 exactly.
        assert_eq!(&out[..5], &[0.0, 1.0, 2.0, 3.0, 4.0]);
    }

    #[test]
    fn slice_index_selects_correct_region() {
        // 16 samples / 4 slices = slice_len 4.  Slice 2 → positions 4..7.
        let mut v = AmenVoice::new();
        v.load(ramp_sample(16));
        v.trigger(
            2,
            4,
            0.0,
            1.0,
            false,
            1.0,
            0,
            &nan16(),
            &nan16(),
            &nan16(),
            false,
            136.0,
            136.0,
        );
        let out = render(&mut v, 4);
        assert_eq!(out[0], 4.0);
        assert!(out[3] < 8.0 && out[3] >= 7.0);
    }

    #[test]
    fn reverse_plays_backward() {
        let mut v = AmenVoice::new();
        v.load(ramp_sample(8));
        v.trigger(
            1,
            1,
            0.0,
            1.0,
            true,
            1.0,
            0,
            &nan16(),
            &nan16(),
            &nan16(),
            false,
            136.0,
            136.0,
        );
        let out = render(&mut v, 4);
        // pos starts at send-1 = 7, decrements by 1.0 each call.
        assert!(out[0] > out[1] && out[1] > out[2]);
        assert!((out[0] - 7.0).abs() < 0.01);
    }

    #[test]
    fn stutter_fits_inside_slice_budget() {
        // Slice length 8; stutter=1 → sub_len=4; gate=1 → window=4.
        // After 4 samples voice should retrigger (pos resets to slice start).
        let mut v = AmenVoice::new();
        v.load(ramp_sample(8));
        v.trigger(
            1,
            1,
            0.0,
            1.0,
            false,
            1.0,
            1,
            &nan16(),
            &nan16(),
            &nan16(),
            false,
            136.0,
            136.0,
        );
        let out = render(&mut v, 8);
        // First sub-slice: 0,1,2,3.  Stutter retrigger resets to 0.
        assert_eq!(out[0], 0.0);
        assert!(out[3] >= 3.0);
        // Sample 4 is the first read of the retriggered slice → back near 0.
        assert!(
            out[4] < 1.0,
            "expected stutter retrigger near 0, got {}",
            out[4]
        );
    }

    #[test]
    fn stutter_zero_plays_full_slice_then_stops() {
        let mut v = AmenVoice::new();
        v.load(ramp_sample(8));
        v.trigger(
            1,
            1,
            0.0,
            1.0,
            false,
            1.0,
            0,
            &nan16(),
            &nan16(),
            &nan16(),
            false,
            136.0,
            136.0,
        );
        // 8 samples to play through, then silence (no stutter, no loop).
        let out = render(&mut v, 12);
        assert!(out[0] < out[5]);
        assert_eq!(out[10], 0.0);
        assert_eq!(out[11], 0.0);
    }

    #[test]
    fn custom_positions_override_equal_division() {
        // 16 samples; positions [0.0, 0.5] for 2 slices → slice 1 = 0..8.
        let mut v = AmenVoice::new();
        v.load(ramp_sample(16));
        let mut pos = nan16();
        pos[0] = 0.0;
        pos[1] = 0.5;
        v.trigger(
            1,
            2,
            0.0,
            1.0,
            false,
            1.0,
            0,
            &pos,
            &nan16(),
            &nan16(),
            false,
            136.0,
            136.0,
        );
        let out = render(&mut v, 8);
        assert_eq!(out[0], 0.0);
        assert!((out[7] - 7.0).abs() < 0.01);
    }

    #[test]
    fn auto_advance_increments_each_trigger() {
        let mut v = AmenVoice::new();
        v.load(ramp_sample(16));
        // slice_count=4; slice 0 means auto.  First fire = slice 0 (region 0..4).
        v.trigger(
            0,
            4,
            0.0,
            1.0,
            false,
            1.0,
            0,
            &nan16(),
            &nan16(),
            &nan16(),
            false,
            136.0,
            136.0,
        );
        assert_eq!(v.process(0.0, 1.0, false), 0.0);
        // Re-trigger advances: slice 1 (region 4..8).
        v.trigger(
            0,
            4,
            0.0,
            1.0,
            false,
            1.0,
            0,
            &nan16(),
            &nan16(),
            &nan16(),
            false,
            136.0,
            136.0,
        );
        assert_eq!(v.process(0.0, 1.0, false), 4.0);
    }
}
