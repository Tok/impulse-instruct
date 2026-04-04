// ─── sequencer/mod.rs ────────────────────────────────────────────────────────
#![allow(dead_code)] // velocity/gate_samples reserved for future per-step routing
// Sample-accurate step sequencer clock.
// All functions are pure: they take state in, return (new_state, events).

use crate::state::{DrumVoice, SequencerState};

// ─── Events emitted by the sequencer ─────────────────────────────────────────

#[derive(Clone, Debug)]
pub enum TriggerEvent {
    DrumTrigger {
        voice: DrumVoice,
        velocity: f32, // reserved for per-step velocity routing
    },
    BassTrigger {
        note: u8,
        accent: bool,
        slide: bool,
        gate_samples: u32, // reserved for gate-off timing
    },
    BassGateOff,
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

#[derive(Clone, Debug)]
pub struct ClockState {
    pub sample_accumulator: f64, // fractional samples since last step
    pub current_step: usize,
    pub loop_count: u32, // increments each full pattern loop (used for probability RNG)
    pub gate_counter: u32, // samples remaining in bass gate
    pub gate_counter_hoover: u32, // samples remaining in hoover gate
    pub gate_counter_an1x: u32, // samples remaining in AN1X gate
}

impl Default for ClockState {
    fn default() -> Self {
        Self {
            sample_accumulator: 0.0,
            current_step: 0,
            loop_count: 0,
            gate_counter: 0,
            gate_counter_hoover: 0,
            gate_counter_an1x: 0,
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
    let mut gate_counter = clock.gate_counter;
    let mut gate_counter_hoover = clock.gate_counter_hoover;
    let mut gate_counter_an1x = clock.gate_counter_an1x;

    // Handle gate-off for bass
    if gate_counter > 0 {
        if block_size as u32 >= gate_counter {
            events.push(TriggerEvent::BassGateOff);
            gate_counter = 0;
        } else {
            gate_counter -= block_size as u32;
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
        for voice in DrumVoice::ALL {
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
                if s.active && prob_hit(s.probability, vstep, loop_count, *voice as u32) {
                    events.push(TriggerEvent::DrumTrigger {
                        voice: *voice,
                        velocity: s.velocity,
                    });
                }
            }
        }

        // Bass trigger — independent step length
        let bstep = step % seq.bass_steps.max(1);
        let bs = seq.bass_pattern.get(bstep).copied().unwrap_or_default();
        if bs.active {
            let gate_samples = (sps * bs.gate as f64) as u32;
            gate_counter = gate_samples;
            events.push(TriggerEvent::BassTrigger {
                note: bs.note,
                accent: bs.accent,
                slide: bs.slide,
                gate_samples,
            });
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
        gate_counter,
        gate_counter_hoover,
        gate_counter_an1x,
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
