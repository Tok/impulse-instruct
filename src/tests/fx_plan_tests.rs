// ─── tests/fx_plan_tests.rs ──────────────────────────────────────────────────
// Tests for compile_fx_plan() — extracted from state_tests.rs.

#[cfg(test)]
mod fx_plan_tests {
    use crate::state::{
        FxStep, ModuleKind, PortDir, PortKind, PortRef, RackState, compile_fx_plan,
    };

    #[test]
    fn default_rack_compiles_full_chain_in_order() {
        let rack = RackState::default();
        let plan = compile_fx_plan(&rack);
        assert_eq!(plan.steps.len(), 12);
        assert_eq!(plan.steps[0], FxStep::Waveshaper);
        assert_eq!(plan.steps[1], FxStep::Reverb);
        assert_eq!(plan.steps[2], FxStep::Delay);
        assert_eq!(plan.steps[3], FxStep::Bitcrush);
        assert_eq!(plan.steps[4], FxStep::Chorus);
        assert_eq!(plan.steps[5], FxStep::Phaser);
        assert_eq!(plan.steps[6], FxStep::RingMod);
        assert_eq!(plan.steps[7], FxStep::Eq);
        assert_eq!(plan.steps[8], FxStep::Compressor);
        assert_eq!(plan.steps[9], FxStep::TapeSat);
        assert_eq!(plan.steps[10], FxStep::Drive);
        assert_eq!(plan.steps[11], FxStep::Autotune);
    }

    #[test]
    fn empty_rack_returns_empty_plan() {
        let rack = RackState {
            modules: Vec::new(),
            cables: Vec::new(),
            next_id: 0,
        };
        let plan = compile_fx_plan(&rack);
        assert!(plan.steps.is_empty());
    }

    #[test]
    fn disabled_fx_module_excluded_from_plan() {
        let mut rack = RackState::default();
        if let Some(m) = rack
            .modules
            .iter_mut()
            .find(|m| m.kind == ModuleKind::FxReverb)
        {
            m.enabled = false;
        }
        let plan = compile_fx_plan(&rack);
        assert!(
            !plan.steps.contains(&FxStep::Reverb),
            "disabled module must not appear in plan"
        );
        assert_eq!(plan.steps.len(), 11);
    }

    #[test]
    fn two_fx_custom_chain_compiles_in_correct_order() {
        let mut rack = RackState {
            modules: Vec::new(),
            cables: Vec::new(),
            next_id: 1,
        };
        let rev_id = rack.add_module(ModuleKind::FxReverb);
        let del_id = rack.add_module(ModuleKind::FxDelay);
        rack.connect(
            PortRef {
                module_id: rev_id,
                dir: PortDir::Out,
                kind: PortKind::Audio,
                index: 0,
            },
            PortRef {
                module_id: del_id,
                dir: PortDir::In,
                kind: PortKind::Audio,
                index: 0,
            },
        );
        let plan = compile_fx_plan(&rack);
        assert_eq!(plan.steps, vec![FxStep::Reverb, FxStep::Delay]);
    }

    #[test]
    fn default_rack_tts_has_reverb_route() {
        let rack = RackState::default();
        let plan = compile_fx_plan(&rack);
        let tts_route = plan.voice_routes.get(&ModuleKind::EspeakNgTts);
        assert!(
            tts_route.is_some(),
            "default rack: EspeakNgTts should have a voice route"
        );
        assert!(
            !plan.voice_routes.contains_key(&ModuleKind::AcidBass),
            "default rack: AcidBass should not have an explicit voice route"
        );
    }

    #[test]
    fn voice_to_fx_cable_creates_voice_route() {
        let mut rack = RackState {
            modules: Vec::new(),
            cables: Vec::new(),
            next_id: 1,
        };
        let bass_id = rack.add_module(ModuleKind::AcidBass);
        let rev_id = rack.add_module(ModuleKind::FxReverb);
        let del_id = rack.add_module(ModuleKind::FxDelay);
        rack.connect(
            PortRef {
                module_id: rev_id,
                dir: PortDir::Out,
                kind: PortKind::Audio,
                index: 0,
            },
            PortRef {
                module_id: del_id,
                dir: PortDir::In,
                kind: PortKind::Audio,
                index: 0,
            },
        );
        rack.connect(
            PortRef {
                module_id: bass_id,
                dir: PortDir::Out,
                kind: PortKind::Audio,
                index: 0,
            },
            PortRef {
                module_id: rev_id,
                dir: PortDir::In,
                kind: PortKind::Audio,
                index: 0,
            },
        );
        let plan = compile_fx_plan(&rack);
        assert_eq!(plan.steps, vec![FxStep::Reverb, FxStep::Delay]);
        let bass_route = plan
            .voice_routes
            .get(&ModuleKind::AcidBass)
            .expect("AcidBass should have an explicit route");
        assert_eq!(bass_route, &[FxStep::Reverb, FxStep::Delay]);
        assert!(!plan.voice_routes.contains_key(&ModuleKind::DrumKit808));
    }

    #[test]
    fn voice_route_single_fx_no_downstream() {
        let mut rack = RackState {
            modules: Vec::new(),
            cables: Vec::new(),
            next_id: 1,
        };
        let bass_id = rack.add_module(ModuleKind::AcidBass);
        let rev_id = rack.add_module(ModuleKind::FxReverb);
        rack.connect(
            PortRef {
                module_id: bass_id,
                dir: PortDir::Out,
                kind: PortKind::Audio,
                index: 0,
            },
            PortRef {
                module_id: rev_id,
                dir: PortDir::In,
                kind: PortKind::Audio,
                index: 0,
            },
        );
        let plan = compile_fx_plan(&rack);
        let bass_route = plan
            .voice_routes
            .get(&ModuleKind::AcidBass)
            .expect("should have a route");
        assert_eq!(bass_route, &[FxStep::Reverb]);
    }
}
