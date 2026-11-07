// ─── tests/slew_tests.rs ─────────────────────────────────────────────────────
// State + module + cable-compile tests for the Slew utility.
// DSP-side tests (per-block smoothing toward target) are
// exercised through process_block; here we lock down the wiring
// and the cable resolution.

#[cfg(test)]
mod slew_state_tests {
    use crate::state::{AppState, SLEW_SLOTS, SlewSlot};

    #[test]
    fn defaults_keep_every_slot_disabled_with_moderate_times() {
        let s = AppState::default();
        assert_eq!(s.slew.len(), SLEW_SLOTS);
        for slot in &s.slew {
            assert!(!slot.enabled);
            assert!((slot.attack - 0.2).abs() < 1e-5);
            assert!((slot.release - 0.2).abs() < 1e-5);
        }
    }

    #[test]
    fn slot_round_trips_through_default() {
        let s = SlewSlot::default();
        assert!(!s.enabled);
        assert!((s.attack - 0.2).abs() < 1e-5);
        assert!((s.release - 0.2).abs() < 1e-5);
    }
}

#[cfg(test)]
mod slew_module_tests {
    use crate::state::{ModuleKind, Zone, rack_scope::parse_module_kind};

    #[test]
    fn label_is_slew() {
        assert_eq!(ModuleKind::Slew.label(), "SLEW");
    }

    #[test]
    fn lives_in_fxmod_zone_with_no_audio_output() {
        let k = ModuleKind::Slew;
        // CV-only utility — no audio bus output.
        assert!(!k.has_audio_output());
        assert_eq!(k.default_zone(), Zone::FxMod);
    }

    #[test]
    fn parses_from_aliases() {
        for alias in ["slew", "glide", "lag", "portacv"] {
            assert_eq!(
                parse_module_kind(alias),
                Some(ModuleKind::Slew),
                "alias `{alias}` should parse"
            );
        }
    }
}

#[cfg(test)]
mod slew_cable_compile_tests {
    use crate::audio::dsp::{MOD_BUF_LFO_BASE, MOD_BUF_SLEW_BASE, compile_slew_params};
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
    fn unwired_slew_slot_has_sentinel_cv_in_buf_idx() {
        // No cable lands on the Slew → cv_in_buf_idx must be the
        // u8::MAX sentinel so the audio thread treats input as 0.
        let mut s = AppState::default();
        s.rack.add_module(ModuleKind::Slew);
        let arr = compile_slew_params(&s);
        assert_eq!(arr[0].cv_in_buf_idx, u8::MAX);
    }

    #[test]
    fn lfo_to_slew_cable_resolves_to_lfo_buf_idx() {
        // Cable from the first LFO module → first Slew module's
        // Mod-In must compile cv_in_buf_idx = MOD_BUF_LFO_BASE + 0.
        let mut s = AppState::default();
        let lfo_id = s
            .rack
            .modules
            .iter()
            .find(|m| m.kind == ModuleKind::LfoModule)
            .map(|m| m.id)
            .unwrap();
        let slew_id = s.rack.add_module(ModuleKind::Slew);
        patch_cv_to_mod(&mut s, lfo_id, slew_id, 0);
        let arr = compile_slew_params(&s);
        assert_eq!(
            arr[0].cv_in_buf_idx as usize, MOD_BUF_LFO_BASE,
            "first LFO → buf idx 0 = LFO_BASE",
        );
    }

    #[test]
    fn slew_to_synth_cable_emits_route_with_slew_buf_idx() {
        // Cable from Slew → AcidBass.Mod[0] (Fixed(BassPan)) must
        // compile through compile_mod_routes with source_buf_idx
        // pointing at MOD_BUF_SLEW_BASE + 0.
        use crate::audio::dsp::compile_mod_routes;
        let mut s = AppState::default();
        let slew_id = s.rack.add_module(ModuleKind::Slew);
        let bass_id = s
            .rack
            .modules
            .iter()
            .find(|m| m.kind == ModuleKind::AcidBass)
            .map(|m| m.id)
            .unwrap();
        patch_cv_to_mod(&mut s, slew_id, bass_id, 0);
        let (routes, count) = compile_mod_routes(&s);
        assert!(count >= 1, "Slew → bass cable must emit a route");
        let route = routes
            .iter()
            .take(count as usize)
            .find(|r| r.source_buf_idx as usize == MOD_BUF_SLEW_BASE)
            .expect("at least one route should source from MOD_BUF_SLEW_BASE");
        // Route resolved.  No further assertion needed beyond
        // existence + correct source_buf_idx.
        let _ = route;
    }
}
