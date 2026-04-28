// ─── tests/mod_compile_extra_tests.rs ────────────────────────────────────────
// Coverage for the four `compile_*_params` passes that didn't yet have
// dedicated tests: TriggerDiv, LogicGate, FunctionGen, Crossfader.
// Each pass walks `AppState.rack.cables`, finds CV→Mod cables landing
// on its module-kind, and resolves the source side to a `cv_buf` slot
// index for the audio thread.  The pattern below mirrors the existing
// `slew_tests.rs` / `quantizer_tests.rs` tests — same patch helper,
// same shape of "unwired = sentinel" + "wired = correct buf idx" pair.

#[cfg(test)]
mod compile_trigger_div_tests {
    use crate::audio::dsp::{MOD_BUF_LFO_BASE, compile_trigger_div_params};
    use crate::state::{AppState, ModuleKind, PortDir, PortKind, PortRef};

    fn patch_cv_to_mod(s: &mut AppState, from: u32, to: u32, slot: u8) {
        s.rack.connect(
            PortRef {
                module_id: from,
                dir: PortDir::Out,
                kind: PortKind::Cv,
                index: 0,
            },
            PortRef {
                module_id: to,
                dir: PortDir::In,
                kind: PortKind::Mod,
                index: slot,
            },
        );
    }

    #[test]
    fn unwired_trigger_div_slot_has_sentinel_cv_in_buf_idx() {
        let mut s = AppState::default();
        s.rack.add_module(ModuleKind::TriggerDiv);
        let arr = compile_trigger_div_params(&s);
        assert_eq!(arr[0].cv_in_buf_idx, u8::MAX);
    }

    #[test]
    fn lfo_to_trigger_div_resolves_to_lfo_buf_idx() {
        let mut s = AppState::default();
        let lfo_id = s
            .rack
            .modules
            .iter()
            .find(|m| m.kind == ModuleKind::LfoModule)
            .map(|m| m.id)
            .unwrap();
        let td_id = s.rack.add_module(ModuleKind::TriggerDiv);
        patch_cv_to_mod(&mut s, lfo_id, td_id, 0);
        let arr = compile_trigger_div_params(&s);
        assert_eq!(arr[0].cv_in_buf_idx as usize, MOD_BUF_LFO_BASE);
    }

    /// `ratio` is snapped to one of the spec-defined values at compile
    /// time so the audio thread can match on a fixed set.  Pin that
    /// snap by setting an off-grid value and confirming the snap.
    #[test]
    fn ratio_is_snapped_to_nearest_valid_value() {
        let mut s = AppState::default();
        s.rack.add_module(ModuleKind::TriggerDiv);
        s.trigger_div[0].ratio = 6; // off-grid (valid set is [2, 3, 4, 5, 7])
        let arr = compile_trigger_div_params(&s);
        let valid = crate::state::trigger_div::TRIGGER_DIV_RATIOS;
        assert!(
            valid.contains(&arr[0].ratio),
            "ratio {} should snap to one of {valid:?}",
            arr[0].ratio
        );
    }
}

#[cfg(test)]
mod compile_logic_gate_tests {
    use crate::audio::dsp::{MOD_BUF_LFO_BASE, compile_logic_gate_params};
    use crate::state::{AppState, ModuleKind, PortDir, PortKind, PortRef};

    fn patch_cv_to_mod_index(s: &mut AppState, from: u32, to: u32, index: u8) {
        s.rack.connect(
            PortRef {
                module_id: from,
                dir: PortDir::Out,
                kind: PortKind::Cv,
                index: 0,
            },
            PortRef {
                module_id: to,
                dir: PortDir::In,
                kind: PortKind::Mod,
                index,
            },
        );
    }

    #[test]
    fn unwired_logic_gate_slot_has_both_inputs_sentinel() {
        let mut s = AppState::default();
        s.rack.add_module(ModuleKind::LogicGate);
        let arr = compile_logic_gate_params(&s);
        assert_eq!(arr[0].cv_in_a_buf_idx, u8::MAX);
        assert_eq!(arr[0].cv_in_b_buf_idx, u8::MAX);
    }

    /// Two CV inputs per slot — A is `cable.to.index = 0`, B is
    /// `index = 1`.  Pin the index → field mapping; a future swap
    /// would silently re-wire AND/OR truth tables.
    #[test]
    fn cable_index_dispatches_to_a_or_b_correctly() {
        let mut s = AppState::default();
        let lfo_id = s
            .rack
            .modules
            .iter()
            .find(|m| m.kind == ModuleKind::LfoModule)
            .map(|m| m.id)
            .unwrap();
        let lg_id = s.rack.add_module(ModuleKind::LogicGate);

        // Patch only A (index = 0); B should remain sentinel.
        patch_cv_to_mod_index(&mut s, lfo_id, lg_id, 0);
        let arr = compile_logic_gate_params(&s);
        assert_eq!(arr[0].cv_in_a_buf_idx as usize, MOD_BUF_LFO_BASE);
        assert_eq!(arr[0].cv_in_b_buf_idx, u8::MAX);
    }

    /// Index ≥ 2 is silently dropped — guard against a future port
    /// renumbering that accidentally lands on an invalid slot.
    #[test]
    fn out_of_range_cable_index_is_dropped() {
        let mut s = AppState::default();
        let lfo_id = s
            .rack
            .modules
            .iter()
            .find(|m| m.kind == ModuleKind::LfoModule)
            .map(|m| m.id)
            .unwrap();
        let lg_id = s.rack.add_module(ModuleKind::LogicGate);
        patch_cv_to_mod_index(&mut s, lfo_id, lg_id, 5);
        let arr = compile_logic_gate_params(&s);
        assert_eq!(arr[0].cv_in_a_buf_idx, u8::MAX);
        assert_eq!(arr[0].cv_in_b_buf_idx, u8::MAX);
    }
}

#[cfg(test)]
mod compile_function_gen_tests {
    use crate::audio::dsp::{MOD_BUF_LFO_BASE, compile_function_gen_params};
    use crate::state::{AppState, ModuleKind, PortDir, PortKind, PortRef};

    fn patch_cv_to_mod(s: &mut AppState, from: u32, to: u32, slot: u8) {
        s.rack.connect(
            PortRef {
                module_id: from,
                dir: PortDir::Out,
                kind: PortKind::Cv,
                index: 0,
            },
            PortRef {
                module_id: to,
                dir: PortDir::In,
                kind: PortKind::Mod,
                index: slot,
            },
        );
    }

    #[test]
    fn unwired_function_gen_slot_has_sentinel_cv_in_buf_idx() {
        let mut s = AppState::default();
        s.rack.add_module(ModuleKind::FunctionGen);
        let arr = compile_function_gen_params(&s);
        assert_eq!(arr[0].cv_in_buf_idx, u8::MAX);
    }

    #[test]
    fn lfo_to_function_gen_resolves_to_lfo_buf_idx() {
        let mut s = AppState::default();
        let lfo_id = s
            .rack
            .modules
            .iter()
            .find(|m| m.kind == ModuleKind::LfoModule)
            .map(|m| m.id)
            .unwrap();
        let fg_id = s.rack.add_module(ModuleKind::FunctionGen);
        patch_cv_to_mod(&mut s, lfo_id, fg_id, 0);
        let arr = compile_function_gen_params(&s);
        assert_eq!(arr[0].cv_in_buf_idx as usize, MOD_BUF_LFO_BASE);
    }

    /// Attack / release / curve knob values clamp to 0..=1 at compile
    /// time so the audio-thread state machine never sees out-of-range
    /// inputs.  Pin the clamp so a future widened range has to be a
    /// deliberate change.
    #[test]
    fn knob_values_clamp_to_unit_range() {
        let mut s = AppState::default();
        s.rack.add_module(ModuleKind::FunctionGen);
        s.function_gen[0].attack = 1.5;
        s.function_gen[0].release = -0.2;
        s.function_gen[0].curve = 99.0;
        let arr = compile_function_gen_params(&s);
        assert_eq!(arr[0].attack, 1.0);
        assert_eq!(arr[0].release, 0.0);
        assert_eq!(arr[0].curve, 1.0);
    }
}

#[cfg(test)]
mod compile_crossfader_tests {
    use crate::audio::dsp::{MOD_BUF_LFO_BASE, compile_crossfader_params};
    use crate::state::{AppState, ModuleKind, PortDir, PortKind, PortRef};

    fn patch_cv_to_mod_index(s: &mut AppState, from: u32, to: u32, index: u8) {
        s.rack.connect(
            PortRef {
                module_id: from,
                dir: PortDir::Out,
                kind: PortKind::Cv,
                index: 0,
            },
            PortRef {
                module_id: to,
                dir: PortDir::In,
                kind: PortKind::Mod,
                index,
            },
        );
    }

    #[test]
    fn unwired_crossfader_slot_has_both_inputs_sentinel() {
        let mut s = AppState::default();
        s.rack.add_module(ModuleKind::Crossfader);
        let arr = compile_crossfader_params(&s);
        assert_eq!(arr[0].cv_in_a_buf_idx, u8::MAX);
        assert_eq!(arr[0].cv_in_b_buf_idx, u8::MAX);
    }

    /// `mix` knob clamps to 0..=1 — same shape as the FunctionGen
    /// knob clamps.  Out-of-range mix would otherwise let a runaway
    /// state value bypass the unit-interval invariant the audio
    /// thread relies on for the lerp.
    #[test]
    fn mix_clamps_to_unit_range() {
        let mut s = AppState::default();
        s.rack.add_module(ModuleKind::Crossfader);
        s.crossfader[0].mix = 2.0;
        let arr = compile_crossfader_params(&s);
        assert_eq!(arr[0].mix, 1.0);
        s.crossfader[0].mix = -0.5;
        let arr = compile_crossfader_params(&s);
        assert_eq!(arr[0].mix, 0.0);
    }

    /// A → cv_in_a_buf_idx, B → cv_in_b_buf_idx.  Same dispatch as
    /// LogicGate; pin separately since the field tuple is different
    /// and a refactor that consolidates them might miss one.
    #[test]
    fn cable_index_dispatches_to_a_or_b_correctly() {
        let mut s = AppState::default();
        let lfo_id = s
            .rack
            .modules
            .iter()
            .find(|m| m.kind == ModuleKind::LfoModule)
            .map(|m| m.id)
            .unwrap();
        let xf_id = s.rack.add_module(ModuleKind::Crossfader);
        patch_cv_to_mod_index(&mut s, lfo_id, xf_id, 1); // B-only
        let arr = compile_crossfader_params(&s);
        assert_eq!(arr[0].cv_in_a_buf_idx, u8::MAX);
        assert_eq!(arr[0].cv_in_b_buf_idx as usize, MOD_BUF_LFO_BASE);
    }
}
