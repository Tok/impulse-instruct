// ─── sequencer/mod.rs ────────────────────────────────────────────────────────
#![allow(dead_code)] // velocity/gate_samples reserved for future per-step routing
// Sample-accurate step sequencer clock.
// All functions are pure: they take state in, return (new_state, events).

use crate::state::{DrumVoice, SequencerState};

// ─── Events emitted by the sequencer ─────────────────────────────────────────

pub mod preecho;
pub use preecho::{
    NoteApproach, NoteOverride, NoteShift, PreechoApply, PreechoConfig, RampCurve, preecho_apply,
    resolve_note_shift,
};

#[derive(Clone, Debug)]
pub enum TriggerEvent {
    DrumTrigger {
        voice: DrumVoice,
        velocity: f32, // reserved for per-step velocity routing
        /// Slice index from the step (0 = auto-advance in the voice).
        /// Only meaningful for sample-based voices (AmenSampler); pure
        /// synth drums ignore it.
        slice: u8,
    },
    BassTrigger {
        voice_idx: usize,
        note: u8,
        /// Accent intensity 0..=1 (0 = no accent).  Previously a bool;
        /// now proportional so the DSP can scale amp lift by value.
        accent: f32,
        /// Slide intensity 0..=1 (0 = no glide).  Controls the glide
        /// time coefficient on the receiving voice.
        slide: f32,
        gate_samples: u32, // reserved for gate-off timing
        /// Per-step pan, -1.0..1.0; 0 = use the voice's static pan.
        pan: f32,
    },
    BassGateOff {
        voice_idx: usize,
    },
    HooverTrigger {
        note: u8,
        /// Accent intensity 0..=1 (0 = no accent).  Scales the amp peak
        /// in `HooverVoice::process`.  Populated by preecho's
        /// `accent_ramp` in the lead-in window; anchors and out-of-window
        /// steps pass through the step's stored accent.
        accent: f32,
        /// Slide intensity 0..=1 (0 = no glide).  Controls the glide
        /// coefficient applied to the frequency when the voice already
        /// has a previous note.
        slide: f32,
    },
    HooverGateOff,
    An1xTrigger {
        note: u8,
        /// Accent intensity 0..=1 (0 = no accent).  Boosts amp-ADSR peak.
        accent: f32,
        /// Slide intensity 0..=1 (0 = no glide).  Shortens the effective
        /// portamento time (faster glide when 0, longer when 1).
        slide: f32,
    },
    An1xGateOff,
    /// Karplus-Strong pluck trigger.  Accent boosts the amp envelope's
    /// initial gain; slide is reserved (the plucked-string voice
    /// doesn't glide pitch today — each trigger re-primes the delay
    /// line fresh — but the field is carried through so a future
    /// legato mode can respect it).
    PluckTrigger {
        note: u8,
        accent: f32,
        slide: f32,
    },
    PluckGateOff,
    /// Wavetable voice trigger.  Same shape as PluckTrigger; `slide`
    /// is reserved for future legato handling — the wavetable voice
    /// snaps pitch on every trigger today.
    WavetableTrigger {
        note: u8,
        accent: f32,
        slide: f32,
    },
    WavetableGateOff,
    /// Sample-Instrument trigger — same shape as Wavetable.
    SampleTrigger {
        note: u8,
        accent: f32,
        slide: f32,
    },
    SampleGateOff,
}

impl TriggerEvent {
    /// True for events that release a sustaining note rather than start
    /// a new one.  Used by the chain advance "stop at end" path —
    /// `advance_clock` may have emitted step-zero note-ons for a
    /// pattern restart that's about to be cancelled, so the audio
    /// thread filters those out while keeping gate-offs (any note
    /// still sounding into the boundary needs a clean release).
    /// Pure helper so the filter logic is unit-testable.
    pub fn is_gate_off(&self) -> bool {
        matches!(
            self,
            TriggerEvent::BassGateOff { .. }
                | TriggerEvent::HooverGateOff
                | TriggerEvent::An1xGateOff
                | TriggerEvent::PluckGateOff
                | TriggerEvent::WavetableGateOff
        )
    }
}

/// Compute the per-block delta to add to `AppState.global_step_count`.
/// Returns 0 when the clock didn't advance (or the saved cursor is
/// somehow ahead of the current step, e.g. session restore from a
/// future timeline — defensively saturating so the counter never
/// runs backwards).
///
/// Polymeter-aware: the global tick no longer wraps at MAX_STEPS, so
/// the delta is the straight saturating difference.  Older builds
/// had a wrap-fallback branch which silently dropped one slot per
/// MAX_STEPS for any voice whose length didn't divide MAX_STEPS.
#[inline]
pub fn step_count_delta(prev_step: usize, curr_step: usize) -> u64 {
    curr_step.saturating_sub(prev_step) as u64
}

// ─── Clock state (audio-thread local, not in shared AppState) ─────────────────

/// Number of drum voices — must match DrumVoice::ALL.len()
const NUM_DRUM_VOICES: usize = 15;

#[derive(Clone, Debug)]
pub struct ClockState {
    pub sample_accumulator: f64, // fractional samples since last step
    pub current_step: usize,
    pub loop_count: u32, // increments each full pattern loop (used for probability RNG)
    pub gate_counters: [u32; crate::state::MAX_BASS_VOICES], // samples remaining per bass voice
    pub gate_counter_hoover: u32, // samples remaining in hoover gate
    pub gate_counter_an1x: u32, // samples remaining in AN1X gate
    pub gate_counter_pluck: u32, // samples remaining in pluck gate
    pub gate_counter_wavetable: u32, // samples remaining in wavetable gate
    pub gate_counter_sample: u32, // samples remaining in sample-instrument gate
    // Ratchet sub-hit tracking — fixed arrays, no allocation
    pub ratchet_remaining: [u8; NUM_DRUM_VOICES], // sub-hits left per voice
    pub ratchet_acc: [f64; NUM_DRUM_VOICES],      // sample accumulator since step fire
    pub ratchet_interval: [f64; NUM_DRUM_VOICES], // sps / N
    pub ratchet_vel: [f32; NUM_DRUM_VOICES],      // velocity of sub-hits
    pub ratchet_slice: [u8; NUM_DRUM_VOICES],     // slice index of sub-hits
}

impl Default for ClockState {
    fn default() -> Self {
        Self {
            sample_accumulator: 0.0,
            current_step: 0,
            loop_count: 0,
            gate_counters: [0; crate::state::MAX_BASS_VOICES],
            gate_counter_hoover: 0,
            gate_counter_an1x: 0,
            gate_counter_pluck: 0,
            gate_counter_wavetable: 0,
            gate_counter_sample: 0,
            ratchet_remaining: [0; NUM_DRUM_VOICES],
            ratchet_acc: [0.0; NUM_DRUM_VOICES],
            ratchet_interval: [0.0; NUM_DRUM_VOICES],
            ratchet_vel: [0.0; NUM_DRUM_VOICES],
            ratchet_slice: [0; NUM_DRUM_VOICES],
        }
    }
}

/// Compute samples per step at given BPM and sample rate.
/// `step_division` is the number of steps per beat (4 = 16th-note grid,
/// 8 = 32nd-note grid, 2 = 8th-note grid).  Pass `seq.step_division` so the
/// clock advances at whatever subdivision the pattern was authored at.
pub fn samples_per_step(bpm: f32, sample_rate: f32, step_division: u8) -> f64 {
    // beats_per_sec = bpm / 60
    // samples_per_beat = sr / (bpm / 60) = sr * 60 / bpm
    // samples_per_step = samples_per_beat / step_division
    let div = step_division.max(1) as f64;
    (sample_rate as f64 * 60.0) / (bpm as f64 * div)
}

/// Advance the sequencer clock by `block_size` samples.
/// Returns (new_clock, Vec<TriggerEvent>) — pure function.
pub fn advance_clock(
    clock: ClockState,
    seq: &SequencerState,
    block_size: usize,
    sample_rate: f32,
) -> (ClockState, Vec<TriggerEvent>) {
    if !seq.running {
        return (clock, vec![]);
    }

    let sps = samples_per_step(seq.bpm, sample_rate, seq.step_division);
    let mut events: Vec<TriggerEvent> = Vec::new();
    let mut acc = clock.sample_accumulator + block_size as f64;
    let mut step = clock.current_step;
    let mut loop_count = clock.loop_count;
    let mut gate_counters = clock.gate_counters;
    let mut gate_counter_hoover = clock.gate_counter_hoover;
    let mut gate_counter_an1x = clock.gate_counter_an1x;
    let mut gate_counter_pluck = clock.gate_counter_pluck;
    let mut gate_counter_wavetable = clock.gate_counter_wavetable;
    let mut gate_counter_sample = clock.gate_counter_sample;
    let mut ratchet_remaining = clock.ratchet_remaining;
    let mut ratchet_acc = clock.ratchet_acc;
    let mut ratchet_interval = clock.ratchet_interval;
    let mut ratchet_vel = clock.ratchet_vel;
    let mut ratchet_slice = clock.ratchet_slice;

    // Advance ratchet sub-hit accumulators and emit any pending sub-hits.
    // Each pending voice fires when its acc crosses the ratchet interval.
    for (i, voice) in DrumVoice::ALL.iter().enumerate() {
        if ratchet_remaining[i] == 0 {
            continue;
        }
        ratchet_acc[i] += block_size as f64;
        while ratchet_remaining[i] > 0 && ratchet_acc[i] >= ratchet_interval[i] {
            ratchet_acc[i] -= ratchet_interval[i];
            ratchet_remaining[i] -= 1;
            events.push(TriggerEvent::DrumTrigger {
                voice: *voice,
                velocity: ratchet_vel[i],
                slice: ratchet_slice[i],
            });
        }
    }

    // Handle gate-off for bass voices
    for (vi, gc) in gate_counters.iter_mut().enumerate() {
        if *gc > 0 {
            if block_size as u32 >= *gc {
                events.push(TriggerEvent::BassGateOff { voice_idx: vi });
                *gc = 0;
            } else {
                *gc -= block_size as u32;
            }
        }
    }

    // Handle gate-off for hoover
    if gate_counter_hoover > 0 {
        if block_size as u32 >= gate_counter_hoover {
            events.push(TriggerEvent::HooverGateOff);
            gate_counter_hoover = 0;
        } else {
            gate_counter_hoover -= block_size as u32;
        }
    }

    // Handle gate-off for AN1X
    if gate_counter_an1x > 0 {
        if block_size as u32 >= gate_counter_an1x {
            events.push(TriggerEvent::An1xGateOff);
            gate_counter_an1x = 0;
        } else {
            gate_counter_an1x -= block_size as u32;
        }
    }

    // Handle gate-off for Pluck
    if gate_counter_pluck > 0 {
        if block_size as u32 >= gate_counter_pluck {
            events.push(TriggerEvent::PluckGateOff);
            gate_counter_pluck = 0;
        } else {
            gate_counter_pluck -= block_size as u32;
        }
    }

    // Handle gate-off for Wavetable
    if gate_counter_wavetable > 0 {
        if block_size as u32 >= gate_counter_wavetable {
            events.push(TriggerEvent::WavetableGateOff);
            gate_counter_wavetable = 0;
        } else {
            gate_counter_wavetable -= block_size as u32;
        }
    }

    // Handle gate-off for Sample Instrument
    if gate_counter_sample > 0 {
        if block_size as u32 >= gate_counter_sample {
            events.push(TriggerEvent::SampleGateOff);
            gate_counter_sample = 0;
        } else {
            gate_counter_sample -= block_size as u32;
        }
    }

    // Swing: even steps (downbeats) are long, odd steps (upbeats) are short.
    // swing=0 → equal; swing=0.5 → 75%/25% triplet shuffle.
    // The duration before the NEXT step fires depends on the CURRENT step's parity.
    // After firing step N, the gap to step N+1 is:
    //   even N → sps*(1 + swing*0.5)  (upbeat comes late)
    //   odd  N → sps*(1 - swing*0.5)  (quick jump back to downbeat)
    let swing_offset = seq.swing as f64 * 0.5;

    loop {
        let step_sps = if step.is_multiple_of(2) {
            sps * (1.0 + swing_offset)
        } else {
            sps * (1.0 - swing_offset)
        };
        if acc < step_sps {
            break;
        }
        acc -= step_sps;
        // Advance the global tick counter without wrapping.  Voice
        // indexing uses `step % voice_steps` so any voice length is
        // honoured cleanly — wrapping at MAX_STEPS used to skip a
        // step when `MAX_STEPS % voice_steps != 0` (e.g. a 5-step
        // bass against the 64-cap dropped one slot every 64 ticks).
        // `usize` on 64-bit holds ~18 quintillion ticks; even at a
        // hot 16 steps/sec this overflows after centuries of
        // continuous play, so a deliberate wrap is unnecessary.
        step += 1;
        // Loop count derives from the global cycle length; chain
        // advancement + probability-RNG seeds use this to detect
        // pattern boundaries.  Computing it here (rather than
        // tracking a hand-incremented counter) makes the value
        // robust against polymetric voice lengths that don't divide
        // `seq.steps`.
        let new_loop_count = (step / seq.steps.max(1)) as u32;
        if new_loop_count != loop_count {
            loop_count = new_loop_count;
        }

        // Drum triggers — each voice uses its own step length for polyrhythm.
        let has_solo = !seq.soloed_drums.is_empty();
        for (i, voice) in DrumVoice::ALL.iter().enumerate() {
            if seq.muted_drums.contains(voice) {
                continue;
            }
            if has_solo && !seq.soloed_drums.contains(voice) {
                continue;
            }
            let vstep = step
                % seq
                    .drum_steps
                    .get(voice)
                    .copied()
                    .unwrap_or(seq.steps)
                    .max(1);
            if let Some(pattern) = seq.drum_patterns.get(voice) {
                let s = pattern.get(vstep).copied().unwrap_or_default();
                // Look up the voice's pre-echo config (keyed by voice
                // group name so a single "kit_a" entry drives every
                // 808 sub-voice).  Inactive configs return identity
                // scaling so this is a no-op for the default case.
                let voice_key = match voice {
                    DrumVoice::Kick808
                    | DrumVoice::Snare808
                    | DrumVoice::HihatClosed808
                    | DrumVoice::HihatOpen808
                    | DrumVoice::TomHi808
                    | DrumVoice::TomMid808
                    | DrumVoice::TomLo808 => "kit_a",
                    DrumVoice::Kick909
                    | DrumVoice::Snare909
                    | DrumVoice::HihatClosed909
                    | DrumVoice::HihatOpen909
                    | DrumVoice::Clap909
                    | DrumVoice::Rim909 => "kit_b",
                    DrumVoice::Amen => "amen",
                    DrumVoice::GabberKick => "gabber_kick",
                };
                let voice_steps = seq
                    .drum_steps
                    .get(voice)
                    .copied()
                    .unwrap_or(seq.steps)
                    .max(1);
                let pre = seq
                    .preecho
                    .get(voice_key)
                    .map(|cfg| preecho::preecho_apply(vstep, voice_steps, cfg))
                    .unwrap_or(preecho::PreechoApply::IDENTITY);

                let voice_cycle = step / voice_steps;
                if s.active
                    && cond_gate(voice_cycle, s.cond)
                    && prob_hit(
                        pre.probability_override.unwrap_or(s.probability),
                        vstep,
                        loop_count,
                        *voice as u32,
                    )
                {
                    let effective_vel = (s.velocity * pre.velocity_mul).clamp(0.0, 1.0);
                    let effective_ratchet = s.ratchet.saturating_add(pre.ratchet_add).min(8);
                    // Amen-specific auto: when step.slice is 0 (unset), map
                    // each step's INDEX to the slice index (1-based, so the
                    // DSP's `(slice_idx-1) % slices` resolves to vstep %
                    // slices).  This makes the obvious break-chopping use
                    // case work straight from the standard step lane:
                    // step N plays slice N.  Other drum voices ignore the
                    // slice field entirely.
                    let effective_slice = if matches!(*voice, DrumVoice::Amen) && s.slice == 0 {
                        // Auto-advance: step N plays slice N by default, OR
                        // slice_order[N % len] when the user defined a
                        // custom permutation on SequencerState.amen_slice_order.
                        let order = &seq.amen_slice_order;
                        let raw = if order.is_empty() {
                            vstep as u8
                        } else {
                            order[vstep % order.len()]
                        };
                        raw.saturating_add(1)
                    } else {
                        s.slice
                    };
                    events.push(TriggerEvent::DrumTrigger {
                        voice: *voice,
                        velocity: effective_vel,
                        slice: effective_slice,
                    });
                    // Schedule ratchet sub-hits (ratchet=1 means no sub-hits).
                    if effective_ratchet > 1 {
                        ratchet_remaining[i] = effective_ratchet - 1;
                        ratchet_interval[i] = sps / effective_ratchet as f64;
                        ratchet_acc[i] = 0.0;
                        ratchet_vel[i] = effective_vel;
                        ratchet_slice[i] = effective_slice;
                    }
                }
            }
        }

        // Bass triggers — one per enabled voice, each with its own pattern and step length
        #[allow(clippy::needless_range_loop)]
        for vi in 0..crate::state::MAX_BASS_VOICES {
            if !seq.bass_voice_enabled[vi] {
                continue;
            }
            let vsteps = seq
                .bass_voice_steps
                .get(vi)
                .copied()
                .unwrap_or(seq.bass_steps)
                .max(1);
            let bstep = step % vsteps;
            let pattern = if vi == 0 {
                seq.bass_pattern.as_slice()
            } else {
                seq.bass_patterns
                    .get(vi)
                    .map(|p| p.as_slice())
                    .unwrap_or(&[])
            };
            let bs = pattern.get(bstep).copied().unwrap_or_default();
            if bs.active && cond_gate(step / vsteps, bs.cond) {
                let gate_samples = (sps * bs.gate as f64) as u32;
                gate_counters[vi] = gate_samples;
                // Melodic preecho: when the "bass" voice has accent_ramp
                // or slide_cascade enabled, override this step's accent /
                // slide inside the lead-in window.  Anchors + non-lead-in
                // steps pass through unchanged (None override).
                let pre = seq
                    .preecho
                    .get("bass")
                    .map(|cfg| preecho::preecho_apply(bstep, vsteps, cfg))
                    .unwrap_or(preecho::PreechoApply::IDENTITY);
                let accent = pre.accent_override.unwrap_or(bs.accent);
                let slide = pre.slide_override.unwrap_or(bs.slide);
                // Note approach: when preecho returns a note_override, the
                // lead-in step plays a shifted version of the anchor's
                // stored note instead of its own.  Anchors and out-of-window
                // steps pass through unchanged.
                let note = match pre.note_override {
                    Some(ov) => {
                        let anchor_note = pattern
                            .get(ov.anchor_step as usize)
                            .map(|s| s.note)
                            .unwrap_or(bs.note);
                        preecho::resolve_note_shift(anchor_note, ov.shift, seq.root_note, seq.scale)
                    }
                    None => bs.note,
                };
                events.push(TriggerEvent::BassTrigger {
                    voice_idx: vi,
                    note,
                    accent,
                    slide,
                    gate_samples,
                    pan: bs.pan.clamp(-1.0, 1.0),
                });
            }
        }

        // Hoover trigger — independent step length.  Melodic preecho
        // overrides the step's accent / slide inside the lead-in window
        // just like the bass path above.
        let hstep = step % seq.hoover_steps.max(1);
        let hs = seq.hoover_pattern.get(hstep).copied().unwrap_or_default();
        if hs.active && cond_gate(step / seq.hoover_steps.max(1), hs.cond) {
            let gate_samples = (sps * 0.75) as u32;
            gate_counter_hoover = gate_samples;
            let hsteps = seq.hoover_steps.max(1);
            let pre = seq
                .preecho
                .get("hoover")
                .map(|cfg| preecho::preecho_apply(hstep, hsteps, cfg))
                .unwrap_or(preecho::PreechoApply::IDENTITY);
            let accent = pre.accent_override.unwrap_or(hs.accent);
            let slide = pre.slide_override.unwrap_or(hs.slide);
            let note = match pre.note_override {
                Some(ov) => {
                    let anchor_note = seq
                        .hoover_pattern
                        .get(ov.anchor_step as usize)
                        .map(|s| s.note)
                        .unwrap_or(hs.note);
                    preecho::resolve_note_shift(anchor_note, ov.shift, seq.root_note, seq.scale)
                }
                None => hs.note,
            };
            events.push(TriggerEvent::HooverTrigger {
                note,
                accent,
                slide,
            });
        }

        // AN1X trigger — independent step length, same preecho shape.
        let astep = step % seq.an1x_steps.max(1);
        let ax = seq.an1x_pattern.get(astep).copied().unwrap_or_default();
        if ax.active && cond_gate(step / seq.an1x_steps.max(1), ax.cond) {
            let gate_samples = (sps * ax.gate as f64) as u32;
            gate_counter_an1x = gate_samples;
            let asteps = seq.an1x_steps.max(1);
            let pre = seq
                .preecho
                .get("an1x")
                .map(|cfg| preecho::preecho_apply(astep, asteps, cfg))
                .unwrap_or(preecho::PreechoApply::IDENTITY);
            let accent = pre.accent_override.unwrap_or(ax.accent);
            let slide = pre.slide_override.unwrap_or(ax.slide);
            let note = match pre.note_override {
                Some(ov) => {
                    let anchor_note = seq
                        .an1x_pattern
                        .get(ov.anchor_step as usize)
                        .map(|s| s.note)
                        .unwrap_or(ax.note);
                    preecho::resolve_note_shift(anchor_note, ov.shift, seq.root_note, seq.scale)
                }
                None => ax.note,
            };
            events.push(TriggerEvent::An1xTrigger {
                note,
                accent,
                slide,
            });
        }

        // Pluck trigger — independent step length, same preecho shape.
        // The Karplus-Strong voice re-primes its delay line on every
        // trigger so slide is informational today, but we carry it
        // through for future legato / freq-interp work.
        let pstep = step % seq.pluck_steps.max(1);
        let ps = seq.pluck_pattern.get(pstep).copied().unwrap_or_default();
        if ps.active && cond_gate(step / seq.pluck_steps.max(1), ps.cond) {
            let gate_samples = (sps * ps.gate as f64) as u32;
            gate_counter_pluck = gate_samples;
            let psteps = seq.pluck_steps.max(1);
            let pre = seq
                .preecho
                .get("pluck")
                .map(|cfg| preecho::preecho_apply(pstep, psteps, cfg))
                .unwrap_or(preecho::PreechoApply::IDENTITY);
            let accent = pre.accent_override.unwrap_or(ps.accent);
            let slide = pre.slide_override.unwrap_or(ps.slide);
            let note = match pre.note_override {
                Some(ov) => {
                    let anchor_note = seq
                        .pluck_pattern
                        .get(ov.anchor_step as usize)
                        .map(|s| s.note)
                        .unwrap_or(ps.note);
                    preecho::resolve_note_shift(anchor_note, ov.shift, seq.root_note, seq.scale)
                }
                None => ps.note,
            };
            events.push(TriggerEvent::PluckTrigger {
                note,
                accent,
                slide,
            });
        }

        // Wavetable trigger — own step length + preecho hookup, same
        // shape as the other melodic lanes.
        let wstep = step % seq.wavetable_steps.max(1);
        let ws = seq
            .wavetable_pattern
            .get(wstep)
            .copied()
            .unwrap_or_default();
        if ws.active && cond_gate(step / seq.wavetable_steps.max(1), ws.cond) {
            let gate_samples = (sps * ws.gate as f64) as u32;
            gate_counter_wavetable = gate_samples;
            let wsteps = seq.wavetable_steps.max(1);
            let pre = seq
                .preecho
                .get("wavetable")
                .map(|cfg| preecho::preecho_apply(wstep, wsteps, cfg))
                .unwrap_or(preecho::PreechoApply::IDENTITY);
            let accent = pre.accent_override.unwrap_or(ws.accent);
            let slide = pre.slide_override.unwrap_or(ws.slide);
            let note = match pre.note_override {
                Some(ov) => {
                    let anchor_note = seq
                        .wavetable_pattern
                        .get(ov.anchor_step as usize)
                        .map(|s| s.note)
                        .unwrap_or(ws.note);
                    preecho::resolve_note_shift(anchor_note, ov.shift, seq.root_note, seq.scale)
                }
                None => ws.note,
            };
            events.push(TriggerEvent::WavetableTrigger {
                note,
                accent,
                slide,
            });
        }

        // Sample-Instrument trigger — V1 reuses the WavetableVoice
        // pattern shape but reads from `sample_pattern` / `sample_steps`.
        // No preecho hookup yet (deferred with the rest of V1.1).
        let sstep = step % seq.sample_steps.max(1);
        let ss = seq.sample_pattern.get(sstep).copied().unwrap_or_default();
        if ss.active && cond_gate(step / seq.sample_steps.max(1), ss.cond) {
            let gate_samples = (sps * ss.gate as f64) as u32;
            gate_counter_sample = gate_samples;
            events.push(TriggerEvent::SampleTrigger {
                note: ss.note,
                accent: ss.accent,
                slide: ss.slide,
            });
        }
    }

    let new_clock = ClockState {
        sample_accumulator: acc,
        current_step: step,
        loop_count,
        gate_counters,
        gate_counter_hoover,
        gate_counter_an1x,
        gate_counter_pluck,
        gate_counter_wavetable,
        gate_counter_sample,
        ratchet_remaining,
        ratchet_acc,
        ratchet_interval,
        ratchet_vel,
        ratchet_slice,
    };
    (new_clock, events)
}

/// Conditional-trigger gate — Monome-style "fire only every Nth voice
/// cycle".  `cond` is the 2-bit value stored on the step (0 = always,
/// 1 = every 2nd cycle, 2 = every 3rd, 3 = every 4th).  `voice_cycle`
/// is `step / voice_steps`, so the gate is independent for each
/// voice's natural cycle length and stays correct under polymeter.
#[inline]
pub(crate) fn cond_gate(voice_cycle: usize, cond: u8) -> bool {
    let n = (cond as usize + 1).max(1);
    voice_cycle.is_multiple_of(n)
}

/// Cheap deterministic probability gate — no allocation, no global state.
/// Returns true with probability `prob` (0.0–1.0).
/// Uses a Knuth-style hash of step + loop + voice_id as the random source.
#[inline]
fn prob_hit(prob: f32, step: usize, loop_count: u32, voice_id: u32) -> bool {
    if prob >= 1.0 {
        return true;
    }
    if prob <= 0.0 {
        return false;
    }
    // Combine all entropy sources into one 32-bit hash.
    let h = (step as u32)
        .wrapping_mul(2654435769)
        .wrapping_add(loop_count.wrapping_mul(1234567891))
        .wrapping_add(voice_id.wrapping_mul(0x9e3779b9));
    let norm = (h >> 8) as f32 / (u32::MAX >> 8) as f32; // 0..1
    norm < prob
}

// ─── Euclidean rhythm generator ───────────────────────────────────────────────

/// Distribute `pulses` evenly across `steps` using the Bjorklund algorithm.
/// Returns a `Vec<bool>` of length `steps` with `pulses` `true` values placed
/// as evenly as possible (Euclidean rhythm / Bjorklund distribution).
///
/// Classic examples: `(4, 16)` = 4-on-the-floor, `(5, 16)` = clave, `(3, 8)` = basic.
/// LLM trigger: "5-in-16 euclidean kick", "3-in-8 euclidean hi-hat".
pub fn euclidean_rhythm(pulses: usize, steps: usize) -> Vec<bool> {
    if steps == 0 {
        return vec![];
    }
    let pulses = pulses.min(steps);
    if pulses == 0 {
        return vec![false; steps];
    }
    if pulses == steps {
        return vec![true; steps];
    }

    // Bjorklund: build two groups iteratively.
    // pattern[i] = (ones, zeros) per group element.
    let mut pattern: Vec<(usize, usize)> = vec![(1, 0); pulses];
    let mut remainder: Vec<(usize, usize)> = vec![(0, 1); steps - pulses];

    while remainder.len() > 1 {
        let take = remainder.len().min(pattern.len());
        let next_pattern: Vec<(usize, usize)> = pattern
            .iter()
            .zip(remainder.iter())
            .map(|(a, b)| (a.0 + b.0, a.1 + b.1))
            .collect();
        let leftover: Vec<(usize, usize)> = if pattern.len() > take {
            pattern[take..].to_vec()
        } else {
            remainder[take..].to_vec()
        };
        pattern = next_pattern;
        remainder = leftover;
    }

    // Flatten groups into a bool vector.
    let mut result = Vec::with_capacity(steps);
    let flatten = |result: &mut Vec<bool>, groups: &[(usize, usize)]| {
        for (o, z) in groups {
            result.extend(std::iter::repeat_n(true, *o));
            result.extend(std::iter::repeat_n(false, *z));
        }
    };
    flatten(&mut result, &pattern);
    flatten(&mut result, &remainder);
    result.truncate(steps);
    result
}

// keep unused variable warnings quiet during tests
#[allow(dead_code)]
const _EUCLID_SANITY: () = ();
