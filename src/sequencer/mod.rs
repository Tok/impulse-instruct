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
    pub gate_counter: u32,        // samples remaining in bass gate
    pub gate_counter_hoover: u32, // samples remaining in hoover gate
    pub gate_counter_an1x: u32,   // samples remaining in AN1X gate
}

impl Default for ClockState {
    fn default() -> Self {
        Self {
            sample_accumulator: 0.0,
            current_step: 0,
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
        step = (step + 1) % seq.steps.max(1);

        // Drum triggers
        for voice in DrumVoice::ALL {
            if let Some(pattern) = seq.drum_patterns.get(voice) {
                let s = pattern.get(step).copied().unwrap_or_default();
                if s.active {
                    events.push(TriggerEvent::DrumTrigger {
                        voice: *voice,
                        velocity: s.velocity,
                    });
                }
            }
        }

        // Bass trigger
        let bs = seq.bass_pattern.get(step).copied().unwrap_or_default();
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

        // Hoover trigger
        let hs = seq.hoover_pattern.get(step).copied().unwrap_or_default();
        if hs.active {
            let gate_samples = (sps * 0.75) as u32;
            gate_counter_hoover = gate_samples;
            events.push(TriggerEvent::HooverTrigger { note: hs.note });
        }

        // AN1X trigger
        let ax = seq.an1x_pattern.get(step).copied().unwrap_or_default();
        if ax.active {
            let gate_samples = (sps * ax.gate as f64) as u32;
            gate_counter_an1x = gate_samples;
            events.push(TriggerEvent::An1xTrigger { note: ax.note });
        }
    }

    let new_clock = ClockState {
        sample_accumulator: acc,
        current_step: step,
        gate_counter,
        gate_counter_hoover,
        gate_counter_an1x,
    };
    (new_clock, events)
}
