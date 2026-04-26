// ─── tests/quantizer_tests.rs ────────────────────────────────────────────────
// State + module + cable-compile tests for the Quantizer utility.

#[cfg(test)]
mod quantizer_state_tests {
    use crate::state::{AppState, QUANTIZER_SLOTS, QuantizerSlot, Scale};

    #[test]
    fn defaults_are_disabled_in_c_major() {
        let s = AppState::default();
        assert_eq!(s.quantizer.len(), QUANTIZER_SLOTS);
        for slot in &s.quantizer {
            assert!(!slot.enabled);
            assert_eq!(slot.root, 0);
            assert_eq!(slot.scale, Scale::Major);
        }
    }

    #[test]
    fn slot_round_trips_through_default() {
        let s = QuantizerSlot::default();
        assert!(!s.enabled);
        assert_eq!(s.root, 0);
        assert_eq!(s.scale, Scale::Major);
    }
}

#[cfg(test)]
mod quantizer_module_tests {
    use crate::state::{ModuleKind, Zone, rack_scope::parse_module_kind};

    #[test]
    fn label_is_quantizer() {
        assert_eq!(ModuleKind::Quantizer.label(), "QUANTIZER");
    }

    #[test]
    fn lives_in_fxmod_zone_with_no_audio_output() {
        let k = ModuleKind::Quantizer;
        assert!(!k.has_audio_output());
        assert_eq!(k.default_zone(), Zone::FxMod);
    }

    #[test]
    fn parses_from_aliases() {
        for alias in ["quantizer", "quant", "scalesnap", "quantize"] {
            assert_eq!(
                parse_module_kind(alias),
                Some(ModuleKind::Quantizer),
                "alias `{alias}` should parse"
            );
        }
    }
}

#[cfg(test)]
mod quantizer_cable_compile_tests {
    use crate::audio::dsp::{
        MOD_BUF_LFO_BASE, MOD_BUF_QUANTIZER_BASE, compile_mod_routes, compile_quantizer_params,
    };
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
    fn unwired_quantizer_has_sentinel_cv_in_buf_idx() {
        let mut s = AppState::default();
        s.rack.add_module(ModuleKind::Quantizer);
        let arr = compile_quantizer_params(&s);
        assert_eq!(arr[0].cv_in_buf_idx, u8::MAX);
    }

    #[test]
    fn lfo_to_quantizer_resolves_to_lfo_buf_idx() {
        let mut s = AppState::default();
        let lfo_id = s
            .rack
            .modules
            .iter()
            .find(|m| m.kind == ModuleKind::LfoModule)
            .map(|m| m.id)
            .unwrap();
        let q_id = s.rack.add_module(ModuleKind::Quantizer);
        patch_cv_to_mod(&mut s, lfo_id, q_id, 0);
        let arr = compile_quantizer_params(&s);
        assert_eq!(arr[0].cv_in_buf_idx as usize, MOD_BUF_LFO_BASE);
    }

    #[test]
    fn quantizer_to_synth_emits_route_with_quantizer_buf_idx() {
        let mut s = AppState::default();
        let q_id = s.rack.add_module(ModuleKind::Quantizer);
        let bass_id = s
            .rack
            .modules
            .iter()
            .find(|m| m.kind == ModuleKind::AcidBass)
            .map(|m| m.id)
            .unwrap();
        patch_cv_to_mod(&mut s, q_id, bass_id, 0);
        let (routes, count) = compile_mod_routes(&s);
        assert!(count >= 1, "Quantizer → synth cable must emit a route");
        let route = routes
            .iter()
            .take(count as usize)
            .find(|r| r.source_buf_idx as usize == MOD_BUF_QUANTIZER_BASE)
            .expect("at least one route should source from MOD_BUF_QUANTIZER_BASE");
        let _ = route;
    }
}
