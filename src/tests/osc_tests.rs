// ─── tests/osc_tests.rs ──────────────────────────────────────────────────────
// OSC parser — address routing + arg coercion.  Pure tests that don't
// open a UDP socket; they call `parse_osc_addr` directly with synthetic
// argument vectors.  The dispatch path is harder to test (writes to
// an Arc<RwLock<AppState>> + sends on an LlmInput channel) and is
// covered by integration smoke tests when needed.

#[cfg(test)]
mod parse {
    use crate::osc::{OscAction, parse_osc_addr};
    use rosc::OscType;

    fn s(v: &str) -> OscType {
        OscType::String(v.to_string())
    }

    #[test]
    fn rejects_addresses_without_impulse_prefix() {
        assert!(parse_osc_addr("/foo/bar", &[]).is_none());
        assert!(parse_osc_addr("/sequencer/play", &[]).is_none());
        assert!(parse_osc_addr("/", &[]).is_none());
    }

    #[test]
    fn play_and_stop_take_no_args() {
        assert_eq!(
            parse_osc_addr("/impulse/sequencer/play", &[]),
            Some(OscAction::Play)
        );
        assert_eq!(
            parse_osc_addr("/impulse/sequencer/stop", &[]),
            Some(OscAction::Stop)
        );
    }

    #[test]
    fn prompt_takes_a_string_arg() {
        let out = parse_osc_addr("/impulse/prompt", &[s("make it darker")]);
        assert_eq!(out, Some(OscAction::Prompt("make it darker".into())));
    }

    #[test]
    fn prompt_without_args_rejects() {
        assert!(parse_osc_addr("/impulse/prompt", &[]).is_none());
    }

    #[test]
    fn lock_unlock_take_path_strings() {
        assert_eq!(
            parse_osc_addr("/impulse/lock", &[s("bass.cutoff")]),
            Some(OscAction::Lock("bass.cutoff".into()))
        );
        assert_eq!(
            parse_osc_addr("/impulse/unlock", &[s("fx.reverb_mix")]),
            Some(OscAction::Unlock("fx.reverb_mix".into()))
        );
    }

    #[test]
    fn scroll_takes_target_string() {
        assert_eq!(
            parse_osc_addr("/impulse/scroll", &[s("fxmod")]),
            Some(OscAction::Scroll("fxmod".into()))
        );
    }

    #[test]
    fn preset_takes_name_string() {
        assert_eq!(
            parse_osc_addr("/impulse/preset", &[s("Crew")]),
            Some(OscAction::Preset("Crew".into()))
        );
    }

    #[test]
    fn style_with_id_sets_some() {
        assert_eq!(
            parse_osc_addr("/impulse/style", &[s("drum_and_bass")]),
            Some(OscAction::Style(Some("drum_and_bass".into())))
        );
    }

    #[test]
    fn style_with_empty_string_clears() {
        // Empty string → None.  Lets a TouchOSC text widget clear the
        // style without a separate command.
        assert_eq!(
            parse_osc_addr("/impulse/style", &[s("")]),
            Some(OscAction::Style(None))
        );
    }

    #[test]
    fn param_update_routes_section_param_pair() {
        let out = parse_osc_addr("/impulse/bass/cutoff", &[OscType::Float(0.7)]);
        match out {
            Some(OscAction::ParamUpdate(v)) => {
                let f = v["bass"]["cutoff"].as_f64().unwrap();
                assert!((f - 0.7).abs() < 1e-3, "expected ~0.7, got {f}");
            }
            other => panic!("expected ParamUpdate, got {other:?}"),
        }
    }

    #[test]
    fn param_update_accepts_int_for_bpm() {
        // Hardware controllers often send integers — the dispatch
        // path's apply_llm_update accepts JSON numbers, so int args
        // must coerce to JSON numbers (not get dropped).
        let out = parse_osc_addr("/impulse/sequencer/bpm", &[OscType::Int(130)]);
        match out {
            Some(OscAction::ParamUpdate(v)) => {
                assert_eq!(v["sequencer"]["bpm"].as_i64().unwrap(), 130);
            }
            other => panic!("expected ParamUpdate, got {other:?}"),
        }
    }

    #[test]
    fn unsupported_section_param_arg_type_rejects() {
        // Blob / time / nil should not produce a JSON value.
        let out = parse_osc_addr("/impulse/bass/cutoff", &[OscType::Nil]);
        assert!(out.is_none());
    }

    #[test]
    fn lock_with_non_string_arg_rejects() {
        let out = parse_osc_addr("/impulse/lock", &[OscType::Float(0.5)]);
        assert!(out.is_none());
    }

    #[test]
    fn unknown_two_segment_address_rejects() {
        // /impulse/foo with no parameter sub-path falls through.
        let out = parse_osc_addr("/impulse/foo", &[OscType::Float(0.5)]);
        assert!(out.is_none());
    }
}
