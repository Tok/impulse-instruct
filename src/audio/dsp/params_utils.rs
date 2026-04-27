// ─── audio/dsp/params_utils.rs ───────────────────────────────────────────────
// Per-slot ParamsCopy structs for the modulation utility modules.
// Lifted out of `params.rs` once that file crossed the 1000-line cap
// during the LogicGate ship.  Sibling file — these structs are still
// re-exported through `params` so consumers don't need to know which
// file they live in.
//
// Each utility kind has a `<Kind>ParamsCopy` struct with the fields
// the audio thread needs to evaluate the slot every block:
//   * `enabled` — the slot's on/off flag.
//   * `cv_in_buf_idx` (or `cv_in_a_buf_idx` / `cv_in_b_buf_idx`) —
//     resolved by the cable compile pass to the source's `cv_buf`
//     index.  `u8::MAX` = unwired (slot reads 0).
//   * Per-utility config (knob values / op selectors / ratios).

/// Per-slot LFO configuration passed to the audio thread (Copy-safe).
#[derive(Clone, Copy, Debug)]
pub struct LfoParamsCopy {
    pub enabled: bool,
    pub waveform: crate::state::LfoWaveform,
    pub rate: f32,         // 0–1
    pub depth: f32,        // 0–1
    pub phase_offset: f32, // 0–1
    pub target: u8,        // opcode from `lfo_target_to_u8`
}

/// Per-slot CV sequencer configuration passed to the audio thread
/// (Copy-safe).  Step values stored as a fixed `[f32; 16]` so the
/// audio thread can index by `current_step` without a heap walk.
#[derive(Clone, Copy, Debug)]
pub struct CvSeqParamsCopy {
    pub enabled: bool,
    pub step_values: [f32; crate::state::CV_SEQ_STEPS],
    pub depth: f32, // 0..1 — bipolar swing around 0.5 step value
    pub target: u8, // opcode from `lfo_target_to_u8`
}

impl Default for CvSeqParamsCopy {
    fn default() -> Self {
        Self {
            enabled: false,
            step_values: [0.5; crate::state::CV_SEQ_STEPS],
            depth: 0.0,
            target: 0,
        }
    }
}

/// Per-slot Slew / glide configuration passed to the audio thread.
/// `cv_in_buf_idx = u8::MAX` means "unwired" — the slot still runs
/// but reads 0 every block (decays to 0 with the release time).
#[derive(Clone, Copy, Debug)]
pub struct SlewParamsCopy {
    pub enabled: bool,
    pub attack: f32,
    pub release: f32,
    /// `cv_buf` index where this slew's input value is read from
    /// each block.  Resolved by the cable compile pass; defaults
    /// to `u8::MAX` (unwired) when no cable lands on this slot.
    pub cv_in_buf_idx: u8,
}

impl Default for SlewParamsCopy {
    fn default() -> Self {
        Self {
            enabled: false,
            attack: 0.2,
            release: 0.2,
            cv_in_buf_idx: u8::MAX,
        }
    }
}

/// Per-slot Quantizer configuration passed to the audio thread.
/// Stores the scale + root choice + the resolved input buf
/// index.  The scale's note set is rebuilt on the audio thread
/// each block (cheap — 12 candidate notes max).
#[derive(Clone, Copy, Debug)]
pub struct QuantizerParamsCopy {
    pub enabled: bool,
    pub root: u8,
    pub scale: crate::state::Scale,
    pub cv_in_buf_idx: u8,
}

impl Default for QuantizerParamsCopy {
    fn default() -> Self {
        Self {
            enabled: false,
            root: 0,
            scale: crate::state::Scale::Major,
            cv_in_buf_idx: u8::MAX,
        }
    }
}

/// Per-slot Comparator configuration.
#[derive(Clone, Copy, Debug)]
pub struct ComparatorParamsCopy {
    pub enabled: bool,
    pub threshold: f32,
    pub cv_in_buf_idx: u8,
}

impl Default for ComparatorParamsCopy {
    fn default() -> Self {
        Self {
            enabled: false,
            threshold: 0.0,
            cv_in_buf_idx: u8::MAX,
        }
    }
}

/// Per-slot LogicGate configuration.  Two CV inputs (A, B) and a
/// boolean op selector; output is a 0/1 gate.
#[derive(Clone, Copy, Debug)]
pub struct LogicGateParamsCopy {
    pub enabled: bool,
    pub op: crate::state::LogicOp,
    pub cv_in_a_buf_idx: u8,
    pub cv_in_b_buf_idx: u8,
}

impl Default for LogicGateParamsCopy {
    fn default() -> Self {
        Self {
            enabled: false,
            op: crate::state::LogicOp::And,
            cv_in_a_buf_idx: u8::MAX,
            cv_in_b_buf_idx: u8::MAX,
        }
    }
}

/// Per-slot TriggerDiv configuration.  Single CV input + a
/// division ratio; output is a 0/1 gate that fires every Nth rising
/// edge of the input.
#[derive(Clone, Copy, Debug)]
pub struct TriggerDivParamsCopy {
    pub enabled: bool,
    /// Division ratio — clamped to a member of
    /// `crate::state::TRIGGER_DIV_RATIOS` at compile time so the
    /// audio-thread mod arithmetic stays cheap.
    pub ratio: u8,
    pub cv_in_buf_idx: u8,
}

impl Default for TriggerDivParamsCopy {
    fn default() -> Self {
        Self {
            enabled: false,
            ratio: 2,
            cv_in_buf_idx: u8::MAX,
        }
    }
}

/// Per-slot Sample-and-hold configuration.  No knobs in V1 —
/// the slot just latches its input on each new sequencer step.
#[derive(Clone, Copy, Debug)]
pub struct SampleHoldParamsCopy {
    pub enabled: bool,
    pub cv_in_buf_idx: u8,
}

impl Default for SampleHoldParamsCopy {
    fn default() -> Self {
        Self {
            enabled: false,
            cv_in_buf_idx: u8::MAX,
        }
    }
}

/// Per-slot Math configuration.  Two CV-input ports (resolved by
/// the cable compile pass into `cv_in_a_buf_idx` and
/// `cv_in_b_buf_idx`) and an op selector + blend knob.
#[derive(Clone, Copy, Debug)]
pub struct MathParamsCopy {
    pub enabled: bool,
    pub op: crate::state::MathOp,
    pub blend: f32,
    pub cv_in_a_buf_idx: u8,
    pub cv_in_b_buf_idx: u8,
}

impl Default for MathParamsCopy {
    fn default() -> Self {
        Self {
            enabled: false,
            op: crate::state::MathOp::Add,
            blend: 0.5,
            cv_in_a_buf_idx: u8::MAX,
            cv_in_b_buf_idx: u8::MAX,
        }
    }
}
