// ─── sequencer/mod.rs ────────────────────────────────────────────────────────
#![allow(dead_code)] // velocity/gate_samples reserved for future per-step routing
// Sample-accurate step sequencer clock.
// All functions are pure: they take state in, return (new_state, events).

use crate::state::{DrumVoice, SequencerState};

// ─── Events emitted by the sequencer ─────────────────────────────────────────

pub mod preecho;
pub use preecho::{PreechoConfig, preecho_scale};

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
        accent: bool,
        slide: bool,
        gate_samples: u32, // reserved for gate-off timing
        /// Per-step pan, -1.0..1.0; 0 = use the voice's static pan.
        pan: f32,
    },
    BassGateOff {
        voice_idx: usize,
    },
    HooverTrigger {
        note: u8,
    },
    HooverGateOff,
    An1xTrigger {
        note: u8,
    },
    An1xGateOff,
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
            ratchet_remaining: [0; NUM_DRUM_VOICES],
            ratchet_acc: [0.0; NUM_DRUM_VOICES],
            ratchet_interval: [0.0; NUM_DRUM_VOICES],
            ratchet_vel: [0.0; NUM_DRUM_VOICES],
            ratchet_slice: [0; NUM_DRUM_VOICES],
        }
    }
}

/// Compute samples per 16th note at given BPM and sample rate.
pub fn samples_per_step(bpm: f32, sample_rate: f32) -> f64 {
    // 1 beat = 4 16th notes; beats_per_sec = bpm/60
    // samples_per_beat = sr / (bpm/60) = sr * 60 / bpm
    // samples_per_16th = samples_per_beat / 4
    (sample_rate as f64 * 60.0) / (bpm as f64 * 4.0)
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

    let sps = samples_per_step(seq.bpm, sample_rate);
    let mut events: Vec<TriggerEvent> = Vec::new();
    let mut acc = clock.sample_accumulator + block_size as f64;
    let mut step = clock.current_step;
    let mut loop_count = clock.loop_count;
    let mut gate_counters = clock.gate_counters;
    let mut gate_counter_hoover = clock.gate_counter_hoover;
    let mut gate_counter_an1x = clock.gate_counter_an1x;
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
        let prev_step = step;
        // Advance the global tick counter, wrapping at MAX_STEPS.
        // Each voice uses `step % voice_steps` to index its own pattern.
        step = (step + 1) % crate::state::MAX_STEPS;
        if step < prev_step {
            loop_count = loop_count.wrapping_add(1);
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
                let (vel_mul, rat_add) = seq
                    .preecho
                    .get(voice_key)
                    .map(|cfg| preecho::preecho_scale(vstep, voice_steps, cfg))
                    .unwrap_or((1.0, 0));

                if s.active && prob_hit(s.probability, vstep, loop_count, *voice as u32) {
                    let effective_vel = (s.velocity * vel_mul).clamp(0.0, 1.0);
                    let effective_ratchet = s.ratchet.saturating_add(rat_add).min(8);
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
            if bs.active {
                let gate_samples = (sps * bs.gate as f64) as u32;
                gate_counters[vi] = gate_samples;
                events.push(TriggerEvent::BassTrigger {
                    voice_idx: vi,
                    note: bs.note,
                    accent: bs.accent,
                    slide: bs.slide,
                    gate_samples,
                    pan: bs.pan.clamp(-1.0, 1.0),
                });
            }
        }

        // Hoover trigger — independent step length
        let hstep = step % seq.hoover_steps.max(1);
        let hs = seq.hoover_pattern.get(hstep).copied().unwrap_or_default();
        if hs.active {
            let gate_samples = (sps * 0.75) as u32;
            gate_counter_hoover = gate_samples;
            events.push(TriggerEvent::HooverTrigger { note: hs.note });
        }

        // AN1X trigger — independent step length
        let astep = step % seq.an1x_steps.max(1);
        let ax = seq.an1x_pattern.get(astep).copied().unwrap_or_default();
        if ax.active {
            let gate_samples = (sps * ax.gate as f64) as u32;
            gate_counter_an1x = gate_samples;
            events.push(TriggerEvent::An1xTrigger { note: ax.note });
        }
    }

    let new_clock = ClockState {
        sample_accumulator: acc,
        current_step: step,
        loop_count,
        gate_counters,
        gate_counter_hoover,
        gate_counter_an1x,
        ratchet_remaining,
        ratchet_acc,
        ratchet_interval,
        ratchet_vel,
        ratchet_slice,
    };
    (new_clock, events)
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
