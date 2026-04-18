// ─── Sampler-based voices ────────────────────────────────────────────────────
// Amen/WAV slice playback with per-slice overrides and a granular pitch-
// preserving BPM stretch.  Granular texture voice lives next door in
// `granular_voice.rs` — kept separate so this file stays under the
// 1000-line cap after the stretcher's crossfade logic landed.

use std::sync::Arc;

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
    /// Pitch-preserving BPM stretch flag captured at trigger time.  When
    /// true, `process()` switches to a granular path: the source reads
    /// at native rate (no pitch shift) but at grain boundaries the read
    /// position jumps to achieve an average advance of `stretch_ratio`
    /// per output sample, which stretches timing without moving pitch.
    preserve_pitch: bool,
    /// Host_bpm / source_bpm, captured at trigger.  `< 1.0` slows the
    /// source (grain rewinds / repeats); `> 1.0` speeds it up (grain
    /// skips forward).  `1.0` or `preserve_pitch == false` disables
    /// the granular path.
    stretch_ratio: f32,
    /// Position within the current grain, 0..`GRAIN_LEN`.  Advances by
    /// 1.0 per output sample; at overflow we jump `self.pos` by
    /// `(stretch_ratio - 1.0) * GRAIN_LEN * direction` and wrap back to 0.
    grain_phase: f32,
    playing: bool,
}

/// Grain length for the pitch-preserving stretcher, in samples.  ~46 ms
/// at 44.1 kHz — long enough to hide splice clicks at moderate stretch
/// ratios, short enough that transient smearing stays in the "granular
/// flavour" range rather than "audible slur".
const AMEN_GRAIN_LEN: f32 = 2048.0;

/// Length of the grain-boundary crossfade, in samples.  ~5.8 ms at
/// 44.1 kHz; short enough that the lookahead window doesn't smear the
/// transient ahead of the splice, long enough to smooth the amplitude /
/// phase mismatch between the tail of the outgoing grain and the head
/// of the incoming one.
const AMEN_GRAIN_FADE: f32 = 256.0;

/// Keep `pos` inside `[slice_start, slice_end)` by wrapping on whichever
/// boundary was crossed.  Used by the pitch-preserve stretcher both for
/// the jump at a grain boundary and for the lookahead read during the
/// crossfade window.  `forward` selects which end is the "start" of
/// playback so reverse-mode wraps land on the mirror side.
fn wrap_into_slice(pos: f32, slice_start: f32, slice_end: f32, forward: bool) -> f32 {
    let slice_len = (slice_end - slice_start).max(1.0);
    if forward {
        if pos >= slice_end {
            slice_start + (pos - slice_end) % slice_len
        } else if pos < slice_start {
            slice_end - (slice_start - pos) % slice_len
        } else {
            pos
        }
    } else if pos <= slice_start {
        slice_end - 1.0 - (slice_start - pos) % slice_len
    } else if pos >= slice_end {
        slice_start + (pos - slice_end) % slice_len
    } else {
        pos
    }
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
            preserve_pitch: false,
            stretch_ratio: 1.0,
            grain_phase: 0.0,
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
    /// - `slice_reverses` — per-slice direction override.  `-1` in a slot = use the
    ///   global `reverse`; `0` = force forward; `1` = force reverse.  Lets specific
    ///   slices glitch backwards while the rest of the break plays forward.
    /// - `bpm_stretch_preserve` — when both `bpm_stretch` and this are true, the
    ///   voice runs a granular stretch instead of the resample-based one: timing
    ///   follows the host tempo while pitch stays at the source's original.
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
        slice_reverses: &[i8; 16],
        bpm_stretch: bool,
        bpm_stretch_preserve: bool,
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
        // Per-slice direction override: -1 in the slot = inherit global,
        // 0 = force forward, 1 = force reverse.  Fallthrough when the
        // slice index is out of bounds (idx0 >= 16 can happen with
        // slice_count caps that grow later).
        let reversed = match slice_reverses.get(idx0 as usize).copied().unwrap_or(-1) {
            1 => true,
            0 => false,
            _ => reverse,
        };
        self.direction = if reversed { -1.0 } else { 1.0 };
        if reversed {
            self.pos = send - 1.0;
            self.gate_end = send - gate_window;
        } else {
            self.pos = sstart;
            self.gate_end = sstart + gate_window;
        }
        self.stutter_left = stutter;

        // BPM stretch has two modes.  Classic (pitch-shifting) bakes the
        // tempo ratio into `extra_pitch`, resampling the voice — fastest +
        // simplest, but pitch moves with tempo.  Preserve mode leaves
        // pitch alone and lets the granular path in `process()` adjust
        // timing at grain boundaries, so per-slice pitch overrides still
        // compose cleanly on top.
        let mut extra = 0.0_f32;
        let stretch_on = bpm_stretch && source_bpm > 1.0 && sequencer_bpm > 1.0;
        let preserve = stretch_on && bpm_stretch_preserve;
        if stretch_on && !preserve {
            // rate = host/source → pitch shift in semitones = 12 * log2(rate)
            extra += 12.0 * (sequencer_bpm / source_bpm).log2();
        }
        self.preserve_pitch = preserve;
        self.stretch_ratio = if preserve {
            (sequencer_bpm / source_bpm).clamp(0.25, 4.0)
        } else {
            1.0
        };
        self.grain_phase = 0.0;
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
        let none16 = [-1_i8; 16];
        self.trigger(
            1, 1, 0.0, 1.0, false, 1.0, 0, &nan16, &nan16, &nan16, &none16, false, false, 136.0,
            170.0,
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
                self.grain_phase = 0.0;
            } else if loop_mode {
                if forward {
                    self.pos = self.slice_start;
                } else {
                    self.pos = self.slice_end - 1.0;
                }
                self.grain_phase = 0.0;
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
        // Small closure for linear-interp reads; used both for the main
        // read and (in preserve mode) for the crossfade lookahead.
        let read = |p: f32| -> f32 {
            let i = (p as usize).min(len.saturating_sub(1));
            let f = (p - i as f32).clamp(0.0, 1.0);
            let nx = samples.get(i + 1).copied().unwrap_or(samples[i]);
            samples[i] + (nx - samples[i]) * f
        };
        let base_out = read(self.pos);

        // Pitch-preserving stretch: advance the grain counter and, at the
        // grain boundary, jump `pos` by `(stretch_ratio - 1) * GRAIN_LEN`
        // in the direction of playback.  Average advance of `pos` works
        // out to `stretch_ratio` per output sample, matching host tempo
        // while the per-sample read rate stayed at pitch-only (no BPM
        // pitch shift baked into `extra_pitch`).
        //
        // v2 adds a short crossfade during the final `AMEN_GRAIN_FADE`
        // samples of each grain: the output blends from the current read
        // at `self.pos` toward the lookahead read at `self.pos + jump`.
        // At the splice, `self.pos` jumps by the same amount, landing on
        // exactly the sample the crossfade was already heading toward —
        // the output curve is continuous through the splice, killing the
        // v1 click at strong ratios.
        let stretcher_active = self.preserve_pitch && (self.stretch_ratio - 1.0).abs() > 1e-4;
        let out = if stretcher_active && self.grain_phase >= AMEN_GRAIN_LEN - AMEN_GRAIN_FADE {
            let jump = (self.stretch_ratio - 1.0) * AMEN_GRAIN_LEN * self.direction;
            let look_pos =
                wrap_into_slice(self.pos + jump, self.slice_start, self.slice_end, forward);
            let look_out = read(look_pos);
            let t = ((self.grain_phase - (AMEN_GRAIN_LEN - AMEN_GRAIN_FADE)) / AMEN_GRAIN_FADE)
                .clamp(0.0, 1.0);
            base_out * (1.0 - t) + look_out * t
        } else {
            base_out
        };

        self.pos += rate;

        if stretcher_active {
            self.grain_phase += 1.0;
            if self.grain_phase >= AMEN_GRAIN_LEN {
                let jump = (self.stretch_ratio - 1.0) * AMEN_GRAIN_LEN * self.direction;
                self.pos =
                    wrap_into_slice(self.pos + jump, self.slice_start, self.slice_end, forward);
                self.grain_phase = 0.0;
            }
        }

        out * volume * self.slice_volume
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

    /// Per-slice reverse-override sentinel: every slot at `-1` means
    /// "inherit the global `reverse` flag", which preserves the legacy
    /// behaviour the existing trigger tests assert.
    fn none16() -> [i8; 16] {
        [-1_i8; 16]
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
            &none16(),
            false,
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
            &none16(),
            false,
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
            &none16(),
            false,
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
            &none16(),
            false,
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
            &none16(),
            false,
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
            &none16(),
            false,
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
            &none16(),
            false,
            false,
            136.0,
            136.0,
        );
        assert_eq!(v.process(0.0, 1.0, false), 4.0);
    }

    #[test]
    fn per_slice_reverse_overrides_global_forward() {
        // Global `reverse=false` but slot 0 of `slice_reverses` = 1 (force
        // reverse).  Playing slice 1 (idx0=0) should read backwards.
        let mut v = AmenVoice::new();
        v.load(ramp_sample(8));
        let mut rev = none16();
        rev[0] = 1;
        v.trigger(
            1,
            1,
            0.0,
            1.0,
            false, // global: forward
            1.0,
            0,
            &nan16(),
            &nan16(),
            &nan16(),
            &rev,
            false,
            false,
            136.0,
            136.0,
        );
        let out = render(&mut v, 4);
        // Per-slice override flips direction: starts at 7.0 and walks down.
        assert!(out[0] > out[1] && out[1] > out[2]);
        assert!((out[0] - 7.0).abs() < 0.01);
    }

    #[test]
    fn per_slice_override_forces_forward_on_global_reverse() {
        // Global `reverse=true` but slot 0 = 0 (force forward).  Slice plays
        // normally despite the global flag.
        let mut v = AmenVoice::new();
        v.load(ramp_sample(8));
        let mut rev = none16();
        rev[0] = 0;
        v.trigger(
            1,
            1,
            0.0,
            1.0,
            true, // global: reverse
            1.0,
            0,
            &nan16(),
            &nan16(),
            &nan16(),
            &rev,
            false,
            false,
            136.0,
            136.0,
        );
        let out = render(&mut v, 4);
        assert!(out[0] < out[1] && out[1] < out[2]);
        assert_eq!(out[0], 0.0);
    }

    #[test]
    fn per_slice_sentinel_inherits_global() {
        // Slot 0 = -1 means "inherit global".  With global=true, playback
        // must be reverse even though the override array is "populated".
        let mut v = AmenVoice::new();
        v.load(ramp_sample(8));
        let rev = none16(); // all -1
        v.trigger(
            1,
            1,
            0.0,
            1.0,
            true, // global: reverse
            1.0,
            0,
            &nan16(),
            &nan16(),
            &nan16(),
            &rev,
            false,
            false,
            136.0,
            136.0,
        );
        let out = render(&mut v, 4);
        assert!(out[0] > out[1]);
        assert!((out[0] - 7.0).abs() < 0.01);
    }

    // ── Pitch-preserving granular stretch ───────────────────────────────

    /// Build a sample of known length filled with a ramp 0..n-1 so a
    /// test can reason about the read position by inspecting the
    /// returned value — the preserve-pitch logic lives in the read-
    /// position maths, not the audio itself.
    fn run_preserve(sample_len: usize, stretch_on: bool, preserve_on: bool) -> f32 {
        let mut v = AmenVoice::new();
        v.load(ramp_sample(sample_len));
        // Play the whole sample as one slice (count=1).  Gate=1 + loop_mode=true
        // in process(...) keeps the voice live past the natural end.
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
            &none16(),
            stretch_on,
            preserve_on,
            170.0, // source_bpm — faster than host, so preserve slows by ratio 120/170.
            120.0, // sequencer_bpm
        );
        // One grain's worth of samples + a few more to land after the jump.
        let n = AMEN_GRAIN_LEN as usize + 10;
        let mut last = 0.0;
        for _ in 0..n {
            last = v.process(0.0, 1.0, true);
        }
        last
    }

    #[test]
    fn preserve_pitch_flag_captured_at_trigger() {
        // Preserve mode only engages when both the bpm_stretch flag and
        // the preserve flag are true; the trigger has four combinations
        // and only (true, true) should flip preserve_pitch on.
        let mut v = AmenVoice::new();
        v.load(ramp_sample(32));
        let fire = |v: &mut AmenVoice, stretch: bool, preserve: bool| {
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
                &none16(),
                stretch,
                preserve,
                170.0,
                120.0,
            );
        };
        fire(&mut v, false, false);
        assert!(!v.preserve_pitch);
        fire(&mut v, false, true);
        assert!(!v.preserve_pitch, "preserve=true alone must not engage");
        fire(&mut v, true, false);
        assert!(!v.preserve_pitch, "stretch without preserve stays classic");
        fire(&mut v, true, true);
        assert!(v.preserve_pitch);
        assert!(
            (v.stretch_ratio - 120.0 / 170.0).abs() < 1e-4,
            "stretch_ratio should capture host/source, got {}",
            v.stretch_ratio,
        );
    }

    #[test]
    fn preserve_mode_suppresses_bpm_pitch_shift() {
        // Classic stretch bakes a pitch shift into extra_pitch; preserve
        // mode must NOT.  A per-slice pitch override still composes, but
        // here we leave it unset so preserve mode's extra_pitch is 0.
        let mut v = AmenVoice::new();
        v.load(ramp_sample(32));
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
            &none16(),
            true, // bpm_stretch
            true, // preserve
            170.0,
            120.0,
        );
        assert!(
            v.extra_pitch.abs() < 1e-4,
            "preserve mode should leave extra_pitch at 0, got {}",
            v.extra_pitch
        );
    }

    #[test]
    fn classic_stretch_still_applies_pitch_shift() {
        // Sanity check: disabling preserve keeps the original semantics.
        // host/source = 120/170 ≈ 0.706 → 12*log2(0.706) ≈ -6.03 semitones.
        let mut v = AmenVoice::new();
        v.load(ramp_sample(32));
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
            &none16(),
            true,  // bpm_stretch
            false, // preserve
            170.0,
            120.0,
        );
        let expected = 12.0 * (120.0_f32 / 170.0).log2();
        assert!(
            (v.extra_pitch - expected).abs() < 1e-3,
            "classic stretch should pitch-shift: got {}, want {}",
            v.extra_pitch,
            expected,
        );
    }

    #[test]
    fn preserve_mode_crosses_grain_boundary_and_reads_stretched() {
        // Preserve slows the break by stretch_ratio ≈ 0.706.  At the
        // grain boundary, pos rewinds by (0.706 - 1) * GRAIN_LEN ≈
        // -603 samples so the next grain re-reads the tail of the
        // previous.  Easiest invariant: the reading head is still
        // inside the sample rather than running off the end after
        // GRAIN_LEN + 10 samples of playback at native rate.
        let sample_len = (AMEN_GRAIN_LEN as usize) + 256;
        // Sanity: classic mode at native rate would read near sample
        // index GRAIN_LEN, comfortably under sample_len.
        let classic = run_preserve(sample_len, false, false);
        let preserve = run_preserve(sample_len, true, true);
        assert!(classic.is_finite());
        assert!(preserve.is_finite());
        // Preserve mode should have rewound at the boundary, so its
        // read position is *behind* classic's — ramp value therefore
        // lower.
        assert!(
            preserve < classic,
            "preserve read should trail classic (rewound at grain boundary): classic={}, preserve={}",
            classic,
            preserve,
        );
    }

    // ── wrap_into_slice ────────────────────────────────────────────────

    #[test]
    fn wrap_forward_past_end_lands_inside_slice() {
        // Slice = [100, 200).  A position 250 (50 past end) should
        // land at 150 — start + (pos - end) modulo slice_len.
        let p = super::wrap_into_slice(250.0, 100.0, 200.0, true);
        assert!((p - 150.0).abs() < 1e-4);
    }

    #[test]
    fn wrap_forward_below_start_reflects_to_end() {
        // Slow-down rewind — position 70 (30 before start) should
        // wrap to 170 (end - 30).
        let p = super::wrap_into_slice(70.0, 100.0, 200.0, true);
        assert!((p - 170.0).abs() < 1e-4);
    }

    #[test]
    fn wrap_reverse_below_start_wraps_to_end_minus_one() {
        // Reverse mode uses end - 1 as the mirror landing so the
        // interpolator has a valid neighbour sample.
        let p = super::wrap_into_slice(70.0, 100.0, 200.0, false);
        assert!((p - 169.0).abs() < 1e-4);
    }

    #[test]
    fn wrap_identity_inside_slice() {
        let p = super::wrap_into_slice(150.0, 100.0, 200.0, true);
        assert!((p - 150.0).abs() < 1e-4);
        let p = super::wrap_into_slice(150.0, 100.0, 200.0, false);
        assert!((p - 150.0).abs() < 1e-4);
    }

    // ── Grain-boundary crossfade (v2) ──────────────────────────────────

    /// Render `count` samples from a fresh preserve-mode amen with
    /// a simple sine-like ramp source, returning the whole vector.
    fn render_preserve(count: usize, sample_len: usize) -> Vec<f32> {
        let mut v = AmenVoice::new();
        v.load(ramp_sample(sample_len));
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
            &none16(),
            true,  // bpm_stretch
            true,  // preserve
            170.0, // source
            120.0, // host — slows by ~0.706
        );
        (0..count).map(|_| v.process(0.0, 1.0, true)).collect()
    }

    #[test]
    fn crossfade_blends_current_and_lookahead_in_fade_window() {
        // One full grain + enough past to cover the fade + a few
        // samples of the next grain.  Within the fade window, the
        // output should smoothly transition from the "current read"
        // toward the "lookahead read"; outside it, output equals the
        // raw read (ramp sample value at the read position).
        let sample_len = (AMEN_GRAIN_LEN as usize) * 3;
        let total = (AMEN_GRAIN_LEN as usize) + 16;
        let out = render_preserve(total, sample_len);
        let fade_start = (AMEN_GRAIN_LEN - AMEN_GRAIN_FADE) as usize;
        let fade_end = AMEN_GRAIN_LEN as usize;
        // The sample just before the fade should track the read
        // position nearly identically (no crossfade contribution).
        let pre = out[fade_start.saturating_sub(4)];
        // Mid-fade: must differ from the pure-ramp extrapolation
        // because the lookahead is pulling toward a different
        // source position.
        let mid = out[(fade_start + fade_end) / 2];
        let mid_extrapolated = pre + ((fade_start + fade_end) as f32 / 2.0 - fade_start as f32);
        assert!(
            (mid - mid_extrapolated).abs() > 1.0,
            "mid-fade sample should differ from pure-ramp prediction: mid={}, pure={}",
            mid,
            mid_extrapolated,
        );
    }

    #[test]
    fn crossfade_eliminates_splice_discontinuity() {
        // Measure the sample-to-sample delta across the grain boundary.
        // With the crossfade, it should stay in the neighbourhood of
        // the per-sample delta elsewhere in the waveform (ramp slope
        // ≈ 1.0 per sample) rather than jumping by the full rewind
        // distance ~600 samples that v1 would produce.
        let sample_len = (AMEN_GRAIN_LEN as usize) * 3;
        let total = (AMEN_GRAIN_LEN as usize) + 16;
        let out = render_preserve(total, sample_len);
        let splice = AMEN_GRAIN_LEN as usize;
        let delta = (out[splice] - out[splice - 1]).abs();
        assert!(
            delta < 10.0,
            "splice delta should be small after crossfade, got {}",
            delta
        );
    }
}
