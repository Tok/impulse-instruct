// ─── tests/cv_utility_tests.rs ───────────────────────────────────────────────
// State + module + cable-compile tests for the Comparator,
// Sample-and-hold, and Math CV utilities.  Each module follows
// the same template established by the Slew + Quantizer tests;
// this file consolidates the three smaller modules into one
// test file rather than splitting them across three.

#[cfg(test)]
mod comparator_tests {
    use crate::audio::dsp::{
        MOD_BUF_COMPARATOR_BASE, compile_comparator_params, compile_mod_routes,
    };
    use crate::state::{
        AppState, COMPARATOR_SLOTS, ComparatorSlot, ModuleKind, PortDir, PortKind, PortRef, Zone,
        rack_scope::parse_module_kind,
    };

    #[test]
    fn defaults_disabled_with_zero_threshold() {
        let s = AppState::default();
        assert_eq!(s.comparator.len(), COMPARATOR_SLOTS);
        for slot in &s.comparator {
            assert!(!slot.enabled);
            assert_eq!(slot.threshold, 0.0);
        }
    }

    #[test]
    fn slot_default_round_trip() {
        let s = ComparatorSlot::default();
        assert!(!s.enabled);
        assert_eq!(s.threshold, 0.0);
    }

    #[test]
    fn label_zone_and_aliases() {
        let k = ModuleKind::Comparator;
        assert_eq!(k.label(), "COMPARATOR");
        assert!(!k.has_audio_output());
        assert_eq!(k.default_zone(), Zone::FxMod);
        for alias in ["comparator", "compare", "threshold"] {
            assert_eq!(parse_module_kind(alias), Some(ModuleKind::Comparator));
        }
    }

    #[test]
    fn comparator_to_synth_route_uses_comparator_buf_idx() {
        let mut s = AppState::default();
        let comp_id = s.rack.add_module(ModuleKind::Comparator);
        let bass_id = s
            .rack
            .modules
            .iter()
            .find(|m| m.kind == ModuleKind::AcidBass)
            .map(|m| m.id)
            .unwrap();
        s.rack.connect(
            PortRef {
                module_id: comp_id,
                dir: PortDir::Out,
                kind: PortKind::Cv,
                index: 0,
            },
            PortRef {
                module_id: bass_id,
                dir: PortDir::In,
                kind: PortKind::Mod,
                index: 0,
            },
        );
        let (routes, count) = compile_mod_routes(&s);
        assert!(count >= 1);
        assert!(
            routes
                .iter()
                .take(count as usize)
                .any(|r| r.source_buf_idx as usize == MOD_BUF_COMPARATOR_BASE),
            "Comparator → synth route must source from MOD_BUF_COMPARATOR_BASE",
        );
    }

    #[test]
    fn unwired_slot_has_sentinel_input() {
        let mut s = AppState::default();
        s.rack.add_module(ModuleKind::Comparator);
        let arr = compile_comparator_params(&s);
        assert_eq!(arr[0].cv_in_buf_idx, u8::MAX);
    }
}

#[cfg(test)]
mod sample_hold_tests {
    use crate::audio::dsp::{
        MOD_BUF_LFO_BASE, MOD_BUF_SAMPLE_HOLD_BASE, compile_mod_routes, compile_sample_hold_params,
    };
    use crate::state::{
        AppState, ModuleKind, PortDir, PortKind, PortRef, SAMPLE_HOLD_SLOTS, SampleHoldSlot, Zone,
        rack_scope::parse_module_kind,
    };

    #[test]
    fn defaults_disabled() {
        let s = AppState::default();
        assert_eq!(s.sample_hold.len(), SAMPLE_HOLD_SLOTS);
        for slot in &s.sample_hold {
            assert!(!slot.enabled);
        }
    }

    #[test]
    fn slot_default_round_trip() {
        let s = SampleHoldSlot::default();
        assert!(!s.enabled);
    }

    #[test]
    fn label_zone_and_aliases() {
        let k = ModuleKind::SampleHold;
        assert_eq!(k.label(), "S&H");
        assert!(!k.has_audio_output());
        assert_eq!(k.default_zone(), Zone::FxMod);
        for alias in ["samplehold", "sample_hold", "snh"] {
            assert_eq!(parse_module_kind(alias), Some(ModuleKind::SampleHold));
        }
    }

    #[test]
    fn lfo_to_sh_resolves_input() {
        let mut s = AppState::default();
        let lfo_id = s
            .rack
            .modules
            .iter()
            .find(|m| m.kind == ModuleKind::LfoModule)
            .map(|m| m.id)
            .unwrap();
        let sh_id = s.rack.add_module(ModuleKind::SampleHold);
        s.rack.connect(
            PortRef {
                module_id: lfo_id,
                dir: PortDir::Out,
                kind: PortKind::Cv,
                index: 0,
            },
            PortRef {
                module_id: sh_id,
                dir: PortDir::In,
                kind: PortKind::Mod,
                index: 0,
            },
        );
        let arr = compile_sample_hold_params(&s);
        assert_eq!(arr[0].cv_in_buf_idx as usize, MOD_BUF_LFO_BASE);
    }

    #[test]
    fn sh_to_synth_route_uses_sh_buf_idx() {
        let mut s = AppState::default();
        let sh_id = s.rack.add_module(ModuleKind::SampleHold);
        let bass_id = s
            .rack
            .modules
            .iter()
            .find(|m| m.kind == ModuleKind::AcidBass)
            .map(|m| m.id)
            .unwrap();
        s.rack.connect(
            PortRef {
                module_id: sh_id,
                dir: PortDir::Out,
                kind: PortKind::Cv,
                index: 0,
            },
            PortRef {
                module_id: bass_id,
                dir: PortDir::In,
                kind: PortKind::Mod,
                index: 0,
            },
        );
        let (routes, count) = compile_mod_routes(&s);
        assert!(
            routes
                .iter()
                .take(count as usize)
                .any(|r| r.source_buf_idx as usize == MOD_BUF_SAMPLE_HOLD_BASE),
        );
    }
}

#[cfg(test)]
mod math_tests {
    use crate::audio::dsp::{
        MOD_BUF_LFO_BASE, MOD_BUF_MATH_BASE, compile_math_params, compile_mod_routes,
    };
    use crate::state::{
        AppState, MATH_SLOTS, MathOp, MathSlot, ModuleKind, PortDir, PortKind, PortRef, Zone,
        rack_scope::parse_module_kind,
    };

    #[test]
    fn defaults_disabled_add_50_50_blend() {
        let s = AppState::default();
        assert_eq!(s.math.len(), MATH_SLOTS);
        for slot in &s.math {
            assert!(!slot.enabled);
            assert_eq!(slot.op, MathOp::Add);
            assert!((slot.blend - 0.5).abs() < 1e-5);
        }
    }

    #[test]
    fn slot_default_round_trip() {
        let s = MathSlot::default();
        assert!(!s.enabled);
        assert_eq!(s.op, MathOp::Add);
    }

    #[test]
    fn label_zone_and_aliases() {
        let k = ModuleKind::Math;
        assert_eq!(k.label(), "MATH");
        assert!(!k.has_audio_output());
        assert_eq!(k.default_zone(), Zone::FxMod);
        for alias in ["math", "mathmodule", "cvmath"] {
            assert_eq!(parse_module_kind(alias), Some(ModuleKind::Math));
        }
    }

    #[test]
    fn op_cycles_through_all_variants() {
        let mut op = MathOp::Add;
        let mut seen = std::collections::HashSet::new();
        for _ in 0..5 {
            seen.insert(op);
            op = op.next();
        }
        assert_eq!(seen.len(), 5, "all 5 ops visited in one full cycle");
    }

    #[test]
    fn dual_input_resolves_a_and_b_independently() {
        // Two different LFO modules feed Math.A and Math.B.
        // Both inputs must compile to distinct buf indices.
        let mut s = AppState::default();
        let lfos: Vec<u32> = s
            .rack
            .modules
            .iter()
            .filter(|m| m.kind == ModuleKind::LfoModule)
            .map(|m| m.id)
            .collect();
        assert!(lfos.len() >= 2, "test premise: two LFOs in default rack");
        let math_id = s.rack.add_module(ModuleKind::Math);
        // LFO 0 → Math.A (Mod-In index 0)
        s.rack.connect(
            PortRef {
                module_id: lfos[0],
                dir: PortDir::Out,
                kind: PortKind::Cv,
                index: 0,
            },
            PortRef {
                module_id: math_id,
                dir: PortDir::In,
                kind: PortKind::Mod,
                index: 0,
            },
        );
        // LFO 1 → Math.B (Mod-In index 1)
        s.rack.connect(
            PortRef {
                module_id: lfos[1],
                dir: PortDir::Out,
                kind: PortKind::Cv,
                index: 0,
            },
            PortRef {
                module_id: math_id,
                dir: PortDir::In,
                kind: PortKind::Mod,
                index: 1,
            },
        );
        let arr = compile_math_params(&s);
        assert_eq!(arr[0].cv_in_a_buf_idx as usize, MOD_BUF_LFO_BASE);
        assert_eq!(arr[0].cv_in_b_buf_idx as usize, MOD_BUF_LFO_BASE + 1);
    }

    #[test]
    fn math_to_synth_route_uses_math_buf_idx() {
        let mut s = AppState::default();
        let math_id = s.rack.add_module(ModuleKind::Math);
        let bass_id = s
            .rack
            .modules
            .iter()
            .find(|m| m.kind == ModuleKind::AcidBass)
            .map(|m| m.id)
            .unwrap();
        s.rack.connect(
            PortRef {
                module_id: math_id,
                dir: PortDir::Out,
                kind: PortKind::Cv,
                index: 0,
            },
            PortRef {
                module_id: bass_id,
                dir: PortDir::In,
                kind: PortKind::Mod,
                index: 0,
            },
        );
        let (routes, count) = compile_mod_routes(&s);
        assert!(
            routes
                .iter()
                .take(count as usize)
                .any(|r| r.source_buf_idx as usize == MOD_BUF_MATH_BASE),
        );
    }
}
