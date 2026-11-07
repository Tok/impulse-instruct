// ─── state/math_module.rs ────────────────────────────────────────────────────
// Math CV utility — combine two CV inputs with a chosen
// operation (add / multiply / blend / max / min).  Two Mod-In
// ports; both must be cabled for the result to be meaningful
// (an unwired input reads as 0, which collapses the output for
// some ops — see the per-op behaviour notes below).

use serde::{Deserialize, Serialize};

pub const MATH_SLOTS: usize = 4;

/// Operation selector for the Math utility.  Stored as u8 so the
/// audio thread's Copy params can carry it without an enum
/// allocation.  Order is fixed; new ops append.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MathOp {
    /// `a + b` — sum, clamped to [-1, 1] downstream.
    Add,
    /// `a * b` — products are tame for bipolar [-1, 1] inputs.
    Multiply,
    /// `lerp(a, b, blend)` where `blend` is the per-slot knob.
    Blend,
    /// `max(a, b)` — always picks the larger value.
    Max,
    /// `min(a, b)` — always picks the smaller value.
    Min,
}

impl MathOp {
    pub fn name(self) -> &'static str {
        match self {
            MathOp::Add => "Add",
            MathOp::Multiply => "Multiply",
            MathOp::Blend => "Blend",
            MathOp::Max => "Max",
            MathOp::Min => "Min",
        }
    }

    pub fn next(self) -> MathOp {
        match self {
            MathOp::Add => MathOp::Multiply,
            MathOp::Multiply => MathOp::Blend,
            MathOp::Blend => MathOp::Max,
            MathOp::Max => MathOp::Min,
            MathOp::Min => MathOp::Add,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MathSlot {
    pub enabled: bool,
    pub op: MathOp,
    /// Blend amount 0..1 — only used when `op == Blend`.
    /// 0 = pure A, 1 = pure B.
    pub blend: f32,
}

impl Default for MathSlot {
    fn default() -> Self {
        Self {
            enabled: false,
            op: MathOp::Add,
            blend: 0.5,
        }
    }
}
