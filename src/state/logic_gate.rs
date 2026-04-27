// ─── state/logic_gate.rs ─────────────────────────────────────────────────────
// Logic-gate CV utility — combine two gate inputs (A, B) with a
// boolean operation (AND / OR / XOR).  Inputs are gate-domain
// (treated as high if ≥ 0.5).  Distinct from Math (continuous CV
// arithmetic): the output is always 0.0 or 1.0, suitable for
// driving downstream gate-consuming utilities (TriggerDiv,
// Comparator, Sample-and-hold).
//
// Combinator for euclidean / TriggerDiv patches: pair two divider
// outputs through AND for "fires only when both happen on the
// same step", or XOR for "fires on the steps where exactly one
// fires".  Same Mod-In topology as Math (`cable.to.index` 0 = A,
// 1 = B); the per-utility compile pass branches on that index.

use serde::{Deserialize, Serialize};

pub const LOGIC_GATE_SLOTS: usize = 4;

/// Boolean operation selector.  Order is stable — new ops append.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LogicOp {
    /// Fires when *both* inputs are high.
    And,
    /// Fires when *either* input is high.
    Or,
    /// Fires when *exactly one* input is high.
    Xor,
}

impl LogicOp {
    pub fn name(self) -> &'static str {
        match self {
            LogicOp::And => "AND",
            LogicOp::Or => "OR",
            LogicOp::Xor => "XOR",
        }
    }

    pub fn next(self) -> LogicOp {
        match self {
            LogicOp::And => LogicOp::Or,
            LogicOp::Or => LogicOp::Xor,
            LogicOp::Xor => LogicOp::And,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LogicGateSlot {
    pub enabled: bool,
    pub op: LogicOp,
}

impl Default for LogicGateSlot {
    fn default() -> Self {
        Self {
            enabled: false,
            op: LogicOp::And,
        }
    }
}
