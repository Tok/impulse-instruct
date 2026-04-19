// ─── tests/fx_plan_tests.rs ──────────────────────────────────────────────────
// Tests for compile_fx_plan() — extracted from state_tests.rs.

#[cfg(test)]
mod fx_plan_tests {
    use crate::state::{
        FxStep, ModuleKind, PortDir, PortKind, PortRef, RackState, compile_fx_plan,
    };

    #[test]
    fn default_rack_compiles_only_wired_fx() {
        // `wire_default_cables` no longer chains every FX serially —
        // only the "important" ones (Reverb + Delay) are wired straight
        // to MASTER, so the plan should contain just those two steps.
        // Other FX live in the rack but are intentionally orphaned.
        let rack = RackState::default();
        let plan = compile_fx_plan(&rack);
        assert!(plan.steps.contains(&FxStep::Reverb));
        assert!(plan.steps.contains(&FxStep::Delay));
        assert!(
            !plan.steps.contains(&FxStep::Waveshaper),
            "orphan FX must not appear in the global plan"
        );
        assert!(!plan.steps.contains(&FxStep::Bitcrush));
        assert!(!plan.steps.contains(&FxStep::Chorus));
    }

    #[test]
    fn empty_rack_returns_empty_plan() {
        let rack = RackState {
            modules: Vec::new(),
            cables: Vec::new(),
            next_id: 0,
            dyn_sequencer_rows: None,
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
        // Default wiring leaves Reverb + Delay as the only wired FX, so
        // disabling Reverb should leave just Delay.
        assert!(plan.steps.contains(&FxStep::Delay));
    }

    #[test]
    fn two_fx_custom_chain_compiles_in_correct_order() {
        let mut rack = RackState {
            modules: Vec::new(),
            cables: Vec::new(),
            next_id: 1,
            dyn_sequencer_rows: None,
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
            dyn_sequencer_rows: None,
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
            dyn_sequencer_rows: None,
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
    fn voice_send_gain_captured_from_first_voice_fx_cable() {
        // The first Voice→FX cable's `audio_gain` becomes the voice's
        // send gain in `FxPlan.voice_send_gain` — handy for per-voice
        // wet/dry balance without touching the voice's volume knob.
        let mut rack = RackState {
            modules: Vec::new(),
            cables: Vec::new(),
            next_id: 1,
            dyn_sequencer_rows: None,
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
        // Mutate the newly-created cable's audio_gain to simulate an
        // agent dialling the send softer.
        rack.cables.last_mut().unwrap().audio_gain = 0.4;
        let plan = compile_fx_plan(&rack);
        let g = plan.voice_send_gain.get(&ModuleKind::AcidBass).copied();
        assert_eq!(g, Some(0.4));
    }

    #[test]
    fn feedback_gain_clamped_to_feedback_max() {
        // User-set audio_gain above FEEDBACK_GAIN_MAX must be clamped at
        // compile time so the audio thread can never blow up.
        let mut rack = RackState {
            modules: Vec::new(),
            cables: Vec::new(),
            next_id: 1,
            dyn_sequencer_rows: None,
        };
        let a = rack.add_module(ModuleKind::FxDelay);
        let b = rack.add_module(ModuleKind::FxReverb);
        // Forward A → B.
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
            audio_gain: 1.0,
        });
        // Feedback B → A with an unreasonably high gain.
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
            audio_gain: 5.0,
        });
        let plan = compile_fx_plan(&rack);
        assert_eq!(plan.feedback_routes.len(), 1);
        assert!(plan.feedback_routes[0].gain <= crate::state::FEEDBACK_GAIN_MAX);
    }

    #[test]
    fn fx_to_fx_cycle_allowed_as_feedback_route() {
        // Loosened in the send-bus refactor: FX→FX cycles are now
        // accepted and turned into feedback routes at compile time
        // (the previous "cycle_rejected_by_connect" test — flipped).
        let mut rack = RackState {
            modules: Vec::new(),
            cables: Vec::new(),
            next_id: 1,
            dyn_sequencer_rows: None,
        };
        let a = rack.add_module(ModuleKind::FxDelay);
        let b = rack.add_module(ModuleKind::FxReverb);
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
        // B → A closes an FX-only cycle — now accepted.
        assert!(rack.connect(
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
        assert_eq!(rack.cables.len(), 2);
    }

    #[test]
    fn voice_to_voice_cycle_still_rejected() {
        // Cycles that don't live entirely inside the FX graph stay
        // rejected — feedback only makes musical sense between FX.
        let mut rack = RackState {
            modules: Vec::new(),
            cables: Vec::new(),
            next_id: 1,
            dyn_sequencer_rows: None,
        };
        let a = rack.add_module(ModuleKind::AcidBass);
        let b = rack.add_module(ModuleKind::FxReverb);
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
        // B (FX) → A (Voice) would close a non-FX cycle → rejected.
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
            dyn_sequencer_rows: None,
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
            audio_gain: 1.0,
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
            audio_gain: 1.0,
        });
        // Must terminate — and now, rather than producing an empty
        // plan, the cycle-breaker designates the second cable as a
        // feedback route while still topologically sorting the first.
        let plan = compile_fx_plan(&rack);
        assert_eq!(plan.steps.len(), 2, "both FX should still be ordered");
        assert_eq!(
            plan.feedback_routes.len(),
            1,
            "the closing cable becomes a feedback route"
        );
        let fr = plan.feedback_routes[0];
        assert_eq!(fr.source, crate::state::FxStep::Reverb);
        assert_eq!(fr.target, crate::state::FxStep::Delay);
        assert!(
            fr.gain <= crate::state::FEEDBACK_GAIN_MAX,
            "gain must be clamped to FEEDBACK_GAIN_MAX (got {})",
            fr.gain
        );
    }

    #[test]
    fn strip_audio_cycles_removes_cycle() {
        let mut rack = RackState {
            modules: Vec::new(),
            cables: Vec::new(),
            next_id: 1,
            dyn_sequencer_rows: None,
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
            audio_gain: 1.0,
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
            audio_gain: 1.0,
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
            audio_gain: 1.0,
        });
        assert_eq!(rack.cables.len(), 3);
        let removed = rack.strip_audio_cycles();
        // FX→FX cycles are now preserved as feedback routes, so the
        // strip pass keeps all three cables.  Non-FX cycles (e.g. a
        // voice looped back into itself) would still be stripped.
        assert_eq!(removed, 0, "FX-only cycles are kept as feedback");
        assert_eq!(rack.cables.len(), 3);
    }
}
