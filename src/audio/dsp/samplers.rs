// ─── Sampler-based voices ────────────────────────────────────────────────────
// Amen/WAV slice playback with per-slice overrides and a granular pitch-
// preserving BPM stretch.  Granular texture voice lives next door in
// `granular_voice.rs` — kept separate so this file stays under the
// 1000-line cap after the stretcher's crossfade logic landed.

use std::sync::Arc;

use super::params::MAX_AMEN_SLICES;

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
        slice_positions: &[f32; MAX_AMEN_SLICES],
        slice_pitches: &[f32; MAX_AMEN_SLICES],
        slice_volumes: &[f32; MAX_AMEN_SLICES],
        slice_reverses: &[i8; MAX_AMEN_SLICES],
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
            let b_idx = (idx0 as usize + 1).min(MAX_AMEN_SLICES - 1);
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
        let nan16 = [f32::NAN; MAX_AMEN_SLICES];
        let none16 = [-1_i8; MAX_AMEN_SLICES];
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
#[path = "samplers_tests.rs"]
mod tests;
