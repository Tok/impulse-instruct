// ─── tests/planner_tests.rs ──────────────────────────────────────────────────
// Pure-function coverage for src/llm/planner.rs: lane label dispatch,
// the `lane_is_live_pub` state-gate check, and the `heuristic_plan`
// short-prompt fast path that lets the Phase-3 pipeline skip the LLM
// planner for narrow single-lane commands.

#[cfg(test)]
mod lane_from_label_tests {
    use crate::llm::lanes::LaneKind;
    use crate::llm::planner::lane_from_label;

    #[test]
    fn all_known_labels_round_trip() {
        assert_eq!(lane_from_label("settings"), Some(LaneKind::Settings));
        assert_eq!(lane_from_label("bass1"), Some(LaneKind::Bass(0)));
        assert_eq!(lane_from_label("bass2"), Some(LaneKind::Bass(1)));
        assert_eq!(lane_from_label("bass3"), Some(LaneKind::Bass(2)));
        assert_eq!(lane_from_label("bass4"), Some(LaneKind::Bass(3)));
        assert_eq!(lane_from_label("kit_a"), Some(LaneKind::KitA));
        assert_eq!(lane_from_label("kit_b"), Some(LaneKind::KitB));
        assert_eq!(lane_from_label("amen"), Some(LaneKind::Amen));
        assert_eq!(lane_from_label("hoover"), Some(LaneKind::Hoover));
        assert_eq!(lane_from_label("an1x"), Some(LaneKind::An1x));
        assert_eq!(lane_from_label("fx"), Some(LaneKind::Fx));
        assert_eq!(lane_from_label("mod"), Some(LaneKind::Modulation));
        assert_eq!(lane_from_label("rack"), Some(LaneKind::Rack));
    }

    #[test]
    fn unknown_labels_return_none() {
        // Typos, capitalisation, and out-of-range bass indices must not
        // resolve — the pipeline's defensive `lane_is_live_pub` filter
        // relies on `None` here to drop stale planner output.
        assert_eq!(lane_from_label("bass"), None); // ambiguous, no index
        assert_eq!(lane_from_label("bass0"), None); // 1-indexed externally
        assert_eq!(lane_from_label("bass5"), None); // only 4 voices
        assert_eq!(lane_from_label("KIT_A"), None); // case-sensitive
        assert_eq!(lane_from_label(""), None);
        assert_eq!(lane_from_label("drums"), None);
    }
}

#[cfg(test)]
mod lane_is_live_tests {
    use crate::llm::lanes::LaneKind;
    use crate::llm::planner::lane_is_live_pub;
    use crate::state::{AppState, ModuleKind};

    /// The four "always-live" lanes return true even on an empty rack —
    /// they're state-driven, not module-driven.
    #[test]
    fn settings_fx_mod_rack_are_always_live() {
        let s = AppState::default();
        assert!(lane_is_live_pub(&s, LaneKind::Settings));
        assert!(lane_is_live_pub(&s, LaneKind::Fx));
        assert!(lane_is_live_pub(&s, LaneKind::Modulation));
        assert!(lane_is_live_pub(&s, LaneKind::Rack));
    }

    #[test]
    fn bass_lane_live_when_voice_enabled_and_rack_has_module() {
        let s = AppState::default();
        // Default state ships with AcidBass module + at least voice 0
        // enabled, so bass #1 is live.
        assert!(lane_is_live_pub(&s, LaneKind::Bass(0)));
    }

    #[test]
    fn bass_lane_dead_when_voice_disabled() {
        let mut s = AppState::default();
        s.bass_voices[0].enabled = false;
        assert!(!lane_is_live_pub(&s, LaneKind::Bass(0)));
    }

    #[test]
    fn bass_lane_dead_when_module_disconnected() {
        let mut s = AppState::default();
        // Yank every audio cable so AcidBass doesn't reach master.  The
        // live-check routes through `reaches_master`, so this kills the
        // lane even with the voice flag still on.
        s.rack.cables.clear();
        assert!(!lane_is_live_pub(&s, LaneKind::Bass(0)));
    }

    #[test]
    fn kit_lanes_track_rack_membership() {
        let mut s = AppState::default();
        // Default rack has both kits.
        assert!(lane_is_live_pub(&s, LaneKind::KitA));
        assert!(lane_is_live_pub(&s, LaneKind::KitB));
        // Disable kit A and it drops out; kit B stays live.
        for m in &mut s.rack.modules {
            if m.kind == ModuleKind::DrumKit808 {
                m.enabled = false;
            }
        }
        assert!(!lane_is_live_pub(&s, LaneKind::KitA));
        assert!(lane_is_live_pub(&s, LaneKind::KitB));
    }

    #[test]
    fn out_of_range_bass_voice_index_is_dead() {
        let s = AppState::default();
        // Only 4 bass voices (0..=3); 99 is out of range.
        assert!(!lane_is_live_pub(&s, LaneKind::Bass(99)));
    }
}

#[cfg(test)]
mod heuristic_plan_tests {
    use crate::llm::lanes::LaneKind;
    use crate::llm::planner_heuristic::heuristic_plan;
    use crate::state::AppState;

    #[test]
    fn long_prompt_returns_none() {
        // >120-char prompts punt to the real planner so the heuristic
        // doesn't over-fit on compound commands.
        let s = AppState::default();
        let long = "a".repeat(200);
        assert!(heuristic_plan(&s, &long).is_none());
    }

    #[test]
    fn bare_bass_index_picks_that_voice() {
        let mut s = AppState::default();
        // Default state only enables voice 0; wake voice 1 so the
        // heuristic doesn't bounce off the live-check.
        s.bass_voices[1].enabled = true;
        let p = heuristic_plan(&s, "make bass 2 louder").unwrap();
        assert_eq!(p.lanes, vec![LaneKind::Bass(1)]);
        assert!(!p.from_retry);
    }

    #[test]
    fn fx_keyword_routes_to_fx_lane() {
        let s = AppState::default();
        let p = heuristic_plan(&s, "add reverb").unwrap();
        assert_eq!(p.lanes, vec![LaneKind::Fx]);
    }

    #[test]
    fn kit_a_keyword_routes_to_kit_a() {
        let s = AppState::default();
        // "808" is one of the kit-A disambiguators.
        let p = heuristic_plan(&s, "less 808 kick").unwrap();
        assert_eq!(p.lanes, vec![LaneKind::KitA]);
    }

    #[test]
    fn kit_b_keyword_routes_to_kit_b() {
        let s = AppState::default();
        let p = heuristic_plan(&s, "spice up the 909").unwrap();
        assert_eq!(p.lanes, vec![LaneKind::KitB]);
    }

    #[test]
    fn mixed_kits_punts_to_planner() {
        let s = AppState::default();
        // Mentions both 808 and 909 — ambiguous, heuristic bails out.
        assert!(heuristic_plan(&s, "808 kick + 909 hats").is_none());
    }

    #[test]
    fn hoover_keyword_routes_to_hoover() {
        let s = AppState::default();
        // Default rack includes HooverLead; the heuristic reaches it.
        let p = heuristic_plan(&s, "hoover line").unwrap();
        assert_eq!(p.lanes, vec![LaneKind::Hoover]);
    }

    #[test]
    fn amen_keyword_routes_to_amen() {
        let s = AppState::default();
        let p = heuristic_plan(&s, "chop the amen").unwrap();
        assert_eq!(p.lanes, vec![LaneKind::Amen]);
    }

    #[test]
    fn an1x_keyword_routes_to_an1x() {
        let s = AppState::default();
        let p = heuristic_plan(&s, "an1x pad").unwrap();
        assert_eq!(p.lanes, vec![LaneKind::An1x]);
    }

    #[test]
    fn rationale_is_populated() {
        let s = AppState::default();
        // Bass #1 is always enabled by default; safer than 2/3/4.
        let p = heuristic_plan(&s, "bass 1").unwrap();
        assert!(
            p.rationale.contains("heuristic"),
            "rationale should mention the heuristic fast path: {:?}",
            p.rationale
        );
    }
}
