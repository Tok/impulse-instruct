// ─── audio/dsp/process_block_util.rs ─────────────────────────────────────────
// CV utility evaluation methods on `DspState` — extracted from
// `process_block.rs` once that file crossed the 1000-line cap during
// the FunctionGen ship.  Hosts the three newer utility evaluators
// (TriggerDiv / LogicGate / FunctionGen) that sit between the older
// in-line evaluators (Comparator, Sample-and-Hold, Math) which still
// live in `process_block.rs` for now.
//
// Same `impl DspState` block — Rust allows splitting impl across
// sibling files freely.  Real-time invariants apply: no allocations,
// no locks.  All scratch state lives in `DspState`.

use super::DspState;
use super::params::{
    AudioParams, MOD_BUF_CROSSFADER_BASE, MOD_BUF_FUNCTION_GEN_BASE, MOD_BUF_LOGIC_GATE_BASE,
    MOD_BUF_TRIGGER_DIV_BASE,
};

impl DspState {
    /// Evaluate the TriggerDiv slots.  Schmitt-style rising-edge
    /// detection: count climbs each time the input crosses 0.5
    /// upward.  Output fires (1.0) when the running count is
    /// divisible by the slot's ratio, else 0.0.  Disabled =
    /// passthrough so the user can unhook the gate without
    /// rewiring.
    pub(super) fn eval_trigger_div(&mut self, p_base: &AudioParams, p: &mut AudioParams) {
        for (i, td) in p_base.trigger_div.iter().enumerate() {
            let out_idx = MOD_BUF_TRIGGER_DIV_BASE + i;
            let raw = if td.cv_in_buf_idx == u8::MAX {
                0.0
            } else {
                p.cv_buf[td.cv_in_buf_idx as usize]
            };
            if !td.enabled {
                p.cv_buf[out_idx] = raw;
                self.trigger_div_prev[i] = raw;
                continue;
            }
            // Rising-edge detector with a single-threshold gate at 0.5.
            // Hysteresis would need both an upper + lower threshold band;
            // for V1 the simple threshold catches typical LFO gates /
            // comparator outputs without chatter.
            let was_high = self.trigger_div_prev[i] >= 0.5;
            let is_high = raw >= 0.5;
            if is_high && !was_high {
                self.trigger_div_count[i] = self.trigger_div_count[i].wrapping_add(1);
            }
            self.trigger_div_prev[i] = raw;
            let ratio = td.ratio.max(1) as u32;
            // Fire output during the entire input gate period when count
            // is an exact multiple of ratio.  Produces gate-out shape
            // matching the input gate width on kept fires, fully off on
            // skipped.
            let on = is_high && self.trigger_div_count[i].is_multiple_of(ratio);
            p.cv_buf[out_idx] = if on { 1.0 } else { 0.0 };
        }
    }

    /// Evaluate the LogicGate slots.  Boolean op (AND / OR / XOR) on
    /// two gate-domain inputs; output is 1.0 / 0.0.  Disabled passes
    /// A through unchanged so unhooking the gate doesn't kill the
    /// signal.
    pub(super) fn eval_logic_gate(&mut self, p_base: &AudioParams, p: &mut AudioParams) {
        for (i, lg) in p_base.logic_gate.iter().enumerate() {
            let out_idx = MOD_BUF_LOGIC_GATE_BASE + i;
            let a_raw = if lg.cv_in_a_buf_idx == u8::MAX {
                0.0
            } else {
                p.cv_buf[lg.cv_in_a_buf_idx as usize]
            };
            let b_raw = if lg.cv_in_b_buf_idx == u8::MAX {
                0.0
            } else {
                p.cv_buf[lg.cv_in_b_buf_idx as usize]
            };
            if !lg.enabled {
                p.cv_buf[out_idx] = a_raw;
                continue;
            }
            let a_high = a_raw >= 0.5;
            let b_high = b_raw >= 0.5;
            let on = match lg.op {
                crate::state::LogicOp::And => a_high && b_high,
                crate::state::LogicOp::Or => a_high || b_high,
                crate::state::LogicOp::Xor => a_high ^ b_high,
            };
            p.cv_buf[out_idx] = if on { 1.0 } else { 0.0 };
        }
    }

    /// Evaluate the FunctionGen slots.  Re-triggerable AR envelope.
    /// State 0 = idle (output 0), 1 = attack (phase 0→1, output rises
    /// with curve shape), 2 = release (phase 0→1, output falls 1→0).
    /// Rising edge on the gate input restarts the attack segment.
    /// Curve 0..1 (knob centre 0.5 = linear; <0.5 = log/concave,
    /// >0.5 = exp/convex) shapes both segments via x^k mapping.
    pub(super) fn eval_function_gen(&mut self, p_base: &AudioParams, p: &mut AudioParams) {
        let block_dt = 1.0 / p_base.sample_rate.max(1.0);
        for (i, fg) in p_base.function_gen.iter().enumerate() {
            let out_idx = MOD_BUF_FUNCTION_GEN_BASE + i;
            let raw = if fg.cv_in_buf_idx == u8::MAX {
                0.0
            } else {
                p.cv_buf[fg.cv_in_buf_idx as usize]
            };
            if !fg.enabled {
                p.cv_buf[out_idx] = 0.0;
                self.function_gen_prev[i] = raw;
                self.function_gen_state[i] = 0;
                self.function_gen_phase[i] = 0.0;
                continue;
            }
            let was_high = self.function_gen_prev[i] >= 0.5;
            let is_high = raw >= 0.5;
            if is_high && !was_high {
                self.function_gen_state[i] = 1;
                self.function_gen_phase[i] = 0.0;
            }
            self.function_gen_prev[i] = raw;
            let curve_k = if (fg.curve - 0.5).abs() < 1e-4 {
                1.0_f32
            } else if fg.curve < 0.5 {
                // Log / concave: 0.5..0 → exponent 1..3.
                1.0 + (0.5 - fg.curve) * 4.0
            } else {
                // Exp / convex: 0.5..1 → exponent 1..0.33.
                1.0 / (1.0 + (fg.curve - 0.5) * 4.0)
            };
            let segment_dur = match self.function_gen_state[i] {
                1 => fg.attack * 1.0 + 1e-3,
                2 => fg.release * 3.0 + 1e-3,
                _ => 1.0,
            };
            if self.function_gen_state[i] != 0 {
                self.function_gen_phase[i] += block_dt / segment_dur;
                if self.function_gen_phase[i] >= 1.0 {
                    if self.function_gen_state[i] == 1 {
                        self.function_gen_state[i] = 2;
                        self.function_gen_phase[i] = 0.0;
                    } else {
                        self.function_gen_state[i] = 0;
                        self.function_gen_phase[i] = 0.0;
                    }
                }
            }
            let env = match self.function_gen_state[i] {
                1 => self.function_gen_phase[i].clamp(0.0, 1.0).powf(curve_k),
                2 => 1.0 - self.function_gen_phase[i].clamp(0.0, 1.0).powf(curve_k),
                _ => 0.0,
            };
            p.cv_buf[out_idx] = env;
        }
    }

    /// Evaluate the Crossfader slots.  `out = lerp(A, B, mix)` with
    /// the per-slot mix knob (0..1).  Disabled slots pass A through
    /// unchanged so the user can disable without rewiring.
    pub(super) fn eval_crossfader(&mut self, p_base: &AudioParams, p: &mut AudioParams) {
        for (i, xf) in p_base.crossfader.iter().enumerate() {
            let out_idx = MOD_BUF_CROSSFADER_BASE + i;
            let a = if xf.cv_in_a_buf_idx == u8::MAX {
                0.0
            } else {
                p.cv_buf[xf.cv_in_a_buf_idx as usize]
            };
            let b = if xf.cv_in_b_buf_idx == u8::MAX {
                0.0
            } else {
                p.cv_buf[xf.cv_in_b_buf_idx as usize]
            };
            if !xf.enabled {
                p.cv_buf[out_idx] = a;
                continue;
            }
            let mix = xf.mix.clamp(0.0, 1.0);
            p.cv_buf[out_idx] = a * (1.0 - mix) + b * mix;
        }
    }
}
