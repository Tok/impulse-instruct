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
        let tts_route = plan.voice_routes.get(&ModuleKind::NeuTts);
        assert!(
            tts_route.is_some(),
            "default rack: NeuTts should have a voice route"
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

    #[test]
    fn cycle_rejected_by_connect() {
        let mut rack = RackState {
            modules: Vec::new(),
            cables: Vec::new(),
            next_id: 1,
        };
        let a = rack.add_module(ModuleKind::FxDelay);
        let b = rack.add_module(ModuleKind::FxReverb);
        // A → B: fine
        assert!(rack.connect(
            PortRef {
                module_id: a,
                dir: PortDir::Out,
                kind: PortKind::Audio,
                index: 0
            },
            PortRef {
                module_id: b,
                dir: PortDir::In,
                kind: PortKind::Audio,
                index: 0
            },
        ));
        // B → A: would create a cycle, must be rejected
        assert!(!rack.connect(
            PortRef {
                module_id: b,
                dir: PortDir::Out,
                kind: PortKind::Audio,
                index: 0
            },
            PortRef {
                module_id: a,
                dir: PortDir::In,
                kind: PortKind::Audio,
                index: 0
            },
        ));
        assert_eq!(rack.cables.len(), 1);
    }

    #[test]
    fn compile_fx_plan_handles_cyclic_cables_gracefully() {
        // Manually inject a cycle (bypassing connect validation) and verify
        // compile_fx_plan terminates without hanging.
        let mut rack = RackState {
            modules: Vec::new(),
            cables: Vec::new(),
            next_id: 1,
        };
        let a = rack.add_module(ModuleKind::FxDelay);
        let b = rack.add_module(ModuleKind::FxReverb);
        // Force a cycle by pushing cables directly
        rack.cables.push(crate::state::Cable {
            from: PortRef {
                module_id: a,
                dir: PortDir::Out,
                kind: PortKind::Audio,
                index: 0,
            },
            to: PortRef {
                module_id: b,
                dir: PortDir::In,
                kind: PortKind::Audio,
                index: 0,
            },
            color: crate::state::CableColor::Gray,
        });
        rack.cables.push(crate::state::Cable {
            from: PortRef {
                module_id: b,
                dir: PortDir::Out,
                kind: PortKind::Audio,
                index: 0,
            },
            to: PortRef {
                module_id: a,
                dir: PortDir::In,
                kind: PortKind::Audio,
                index: 0,
            },
            color: crate::state::CableColor::Gray,
        });
        // Must terminate (Kahn's sort handles cycles, voice walk has visited set)
        let plan = compile_fx_plan(&rack);
        // Kahn's topo-sort produces empty output for nodes in cycles
        assert!(plan.steps.is_empty());
    }

    #[test]
    fn strip_audio_cycles_removes_cycle() {
        let mut rack = RackState {
            modules: Vec::new(),
            cables: Vec::new(),
            next_id: 1,
        };
        let a = rack.add_module(ModuleKind::FxDelay);
        let b = rack.add_module(ModuleKind::FxReverb);
        let c = rack.add_module(ModuleKind::FxChorus);
        // Linear chain A → B → C (no cycle)
        rack.cables.push(crate::state::Cable {
            from: PortRef {
                module_id: a,
                dir: PortDir::Out,
                kind: PortKind::Audio,
                index: 0,
            },
            to: PortRef {
                module_id: b,
                dir: PortDir::In,
                kind: PortKind::Audio,
                index: 0,
            },
            color: crate::state::CableColor::Gray,
        });
        rack.cables.push(crate::state::Cable {
            from: PortRef {
                module_id: b,
                dir: PortDir::Out,
                kind: PortKind::Audio,
                index: 0,
            },
            to: PortRef {
                module_id: c,
                dir: PortDir::In,
                kind: PortKind::Audio,
                index: 0,
            },
            color: crate::state::CableColor::Gray,
        });
        // Add cycle: C → A
        rack.cables.push(crate::state::Cable {
            from: PortRef {
                module_id: c,
                dir: PortDir::Out,
                kind: PortKind::Audio,
                index: 0,
            },
            to: PortRef {
                module_id: a,
                dir: PortDir::In,
                kind: PortKind::Audio,
                index: 0,
            },
            color: crate::state::CableColor::Gray,
        });
        assert_eq!(rack.cables.len(), 3);
        let removed = rack.strip_audio_cycles();
        assert_eq!(removed, 1); // C → A removed
        assert_eq!(rack.cables.len(), 2); // A → B and B → C kept
    }
}
