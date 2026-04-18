#[cfg(test)]
#[allow(clippy::module_inception)]
mod lane_scheduler_tests {
    use crate::llm::lane_scheduler::{
        Xorshift32, compute_weight, effective_dynamism, heat_jitter, lane_dynamism, pick_jam_lane,
        recency_decay,
    };
    use crate::llm::lanes::LaneKind;
    use crate::llm::planner_jam::jam_plan;
    use crate::llm::styles::Style;
    use crate::state::{
        AppState, LaneScore, ModuleKind, PortDir, PortKind, PortRef, RackModule, RackState,
    };
    use std::collections::HashMap;

    // Build a bare-bones style with only `lane_dynamism` populated — the
    // other fields aren't consulted by the scheduler.
    fn style_with_dynamism(entries: &[(&str, f32)]) -> Style {
        let mut map = HashMap::new();
        for (k, v) in entries {
            map.insert((*k).to_string(), *v);
        }
        Style {
            id: "test".into(),
            name: "Test".into(),
            keywords: vec![],
            bpm_range: String::new(),
            brief: String::new(),
            description: String::new(),
            seed_patterns: Default::default(),
            suggested_root: None,
            suggested_scale: None,
            baseline_params: None,
            mc_lines: vec![],
            themes: vec![],
            rack_modules: vec![],
            lane_dynamism: map,
        }
    }

    // ── Fixtures ──────────────────────────────────────────────────────────────

    fn bass_only_state() -> AppState {
        let mut s = AppState::default();
        let mut rack = RackState::default();
        rack.modules.clear();
        rack.modules.push(RackModule::new(1, ModuleKind::AcidBass));
        rack.modules
            .push(RackModule::new(2, ModuleKind::MasterOutput));
        let _ = rack.connect(
            PortRef {
                module_id: 1,
                dir: PortDir::Out,
                kind: PortKind::Audio,
                index: 0,
            },
            PortRef {
                module_id: 2,
                dir: PortDir::In,
                kind: PortKind::Audio,
                index: 0,
            },
        );
        s.rack = rack;
        s.bass_voices[0].enabled = true;
        s
    }

    // ── Dynamism ──────────────────────────────────────────────────────────────

    #[test]
    fn rack_dynamism_is_zero() {
        // Scheduler must never pick the rack lane — rack composition is
        // user-owned (see pipeline writeback guard).
        assert_eq!(lane_dynamism(LaneKind::Rack), 0.0);
    }

    #[test]
    fn bass_more_dynamic_than_settings() {
        // Bass lines change constantly; BPM/key rarely do.
        assert!(lane_dynamism(LaneKind::Bass(0)) > lane_dynamism(LaneKind::Settings));
    }

    // ── Recency decay ─────────────────────────────────────────────────────────

    #[test]
    fn recency_decay_none_returns_full_weight() {
        // A lane that's never been picked should carry full weight.
        assert_eq!(recency_decay(None, 0), 1.0);
        assert_eq!(recency_decay(None, 99), 1.0);
    }

    #[test]
    fn recency_decay_just_changed_returns_zero() {
        let s = LaneScore {
            score: 0.5,
            last_changed_cycle: 10,
            change_count: 1,
        };
        assert_eq!(recency_decay(Some(&s), 10), 0.0);
    }

    #[test]
    fn recency_decay_known_points() {
        let mk = |cycle: u32| LaneScore {
            score: 0.5,
            last_changed_cycle: cycle,
            change_count: 1,
        };
        // gap=1 → 0.5, gap=3 → 0.75 (per comment in lane_scheduler.rs).
        let s = mk(0);
        assert!((recency_decay(Some(&s), 1) - 0.5).abs() < 1e-4);
        assert!((recency_decay(Some(&s), 3) - 0.75).abs() < 1e-4);
    }

    #[test]
    fn recency_decay_asymptotes_to_one() {
        let s = LaneScore {
            score: 0.5,
            last_changed_cycle: 0,
            change_count: 1,
        };
        // Very large gap → approaches 1.0 but doesn't overshoot.
        let decay = recency_decay(Some(&s), 10_000);
        assert!(decay < 1.0);
        assert!(decay > 0.999);
    }

    // ── Weight ────────────────────────────────────────────────────────────────

    #[test]
    fn compute_weight_rack_is_zero() {
        let mut rng = Xorshift32::new(1);
        // Rack's dynamism short-circuits the formula regardless of score.
        assert_eq!(
            compute_weight(LaneKind::Rack, None, 0, 0.5, None, &mut rng),
            0.0
        );
    }

    #[test]
    fn compute_weight_never_picked_has_weight() {
        let mut rng = Xorshift32::new(1);
        let w = compute_weight(LaneKind::Bass(0), None, 0, 0.0, None, &mut rng);
        // Bass + never-picked (score default 0.5) + recency 1.0 + no heat
        // jitter → weight = dynamism × 0.5.
        let expected = lane_dynamism(LaneKind::Bass(0)) * 0.5;
        assert!(
            (w - expected).abs() < 1e-4,
            "compute_weight={}, expected≈{}",
            w,
            expected
        );
    }

    #[test]
    fn compute_weight_perfect_score_zero() {
        // score=1.0 → (1 - score) = 0 → weight = 0.
        let mut rng = Xorshift32::new(1);
        let s = LaneScore {
            score: 1.0,
            last_changed_cycle: 0,
            change_count: 1,
        };
        // Big gap so recency doesn't also zero it out.
        let w = compute_weight(LaneKind::Bass(0), Some(&s), 100, 0.0, None, &mut rng);
        assert!(w.abs() < 1e-6);
    }

    // ── Heat jitter ───────────────────────────────────────────────────────────

    #[test]
    fn heat_zero_jitter_is_identity() {
        let mut rng = Xorshift32::new(1);
        // At heat=0 the multiplier is exactly 1 (no noise).
        assert_eq!(heat_jitter(0.0, &mut rng), 1.0);
    }

    #[test]
    fn heat_one_jitter_bounded() {
        let mut rng = Xorshift32::new(12345);
        // At heat=1 the multiplier stays in [1.0, 1.6).
        for _ in 0..200 {
            let j = heat_jitter(1.0, &mut rng);
            assert!((1.0..=1.6).contains(&j));
        }
    }

    // ── Xorshift ──────────────────────────────────────────────────────────────

    #[test]
    fn xorshift_deterministic_under_seed() {
        let mut a = Xorshift32::new(42);
        let mut b = Xorshift32::new(42);
        for _ in 0..10 {
            assert_eq!(a.next_u32(), b.next_u32());
        }
    }

    #[test]
    fn xorshift_unit_in_range() {
        let mut rng = Xorshift32::new(7);
        for _ in 0..1000 {
            let u = rng.unit_f32();
            assert!((0.0..1.0).contains(&u));
        }
    }

    // ── pick_jam_lane ─────────────────────────────────────────────────────────

    #[test]
    fn pick_empty_returns_none() {
        let state = AppState::default();
        let mut rng = Xorshift32::new(1);
        assert!(pick_jam_lane(&state, &[], &mut rng).is_none());
    }

    #[test]
    fn pick_rack_only_returns_none() {
        // Single-candidate with dynamism 0 → total weight 0 → None.
        let state = AppState::default();
        let mut rng = Xorshift32::new(1);
        assert!(pick_jam_lane(&state, &[LaneKind::Rack], &mut rng).is_none());
    }

    #[test]
    fn pick_single_candidate_returns_it() {
        let state = AppState::default();
        let mut rng = Xorshift32::new(1);
        let pick = pick_jam_lane(&state, &[LaneKind::Bass(0)], &mut rng);
        assert_eq!(pick, Some(LaneKind::Bass(0)));
    }

    #[test]
    fn pick_avoids_perfect_score_lane() {
        // Bass(0) is "perfect" (score=1.0 → weight=0).  With Bass(0) and
        // Bass(1) as candidates, the picker must always pick Bass(1).
        let mut state = AppState::default();
        state.llm.lane_scores.insert(
            LaneKind::Bass(0).label().to_string(),
            LaneScore {
                score: 1.0,
                last_changed_cycle: 0,
                change_count: 1,
            },
        );
        state.llm.jam_cycle_count = 100; // big gap so recency doesn't veto
        let mut rng = Xorshift32::new(1);
        for _ in 0..50 {
            let pick = pick_jam_lane(&state, &[LaneKind::Bass(0), LaneKind::Bass(1)], &mut rng);
            assert_eq!(pick, Some(LaneKind::Bass(1)));
        }
    }

    #[test]
    fn pick_avoids_just_changed_lane() {
        // Bass(0) just fired (gap=0) — the picker must pick Bass(1).
        let mut state = AppState::default();
        state.llm.jam_cycle_count = 5;
        state.llm.lane_scores.insert(
            LaneKind::Bass(0).label().to_string(),
            LaneScore {
                score: 0.5,
                last_changed_cycle: 5,
                change_count: 1,
            },
        );
        let mut rng = Xorshift32::new(1);
        for _ in 0..50 {
            let pick = pick_jam_lane(&state, &[LaneKind::Bass(0), LaneKind::Bass(1)], &mut rng);
            assert_eq!(pick, Some(LaneKind::Bass(1)));
        }
    }

    #[test]
    fn pick_low_score_gets_more_selections_than_high_score() {
        // Both lanes have non-recent history, but one scored 0.2 (weak)
        // and the other 0.9 (strong).  Over many picks, the weak one
        // should come up far more often.
        let mut state = AppState::default();
        state.llm.jam_cycle_count = 50;
        state.llm.lane_scores.insert(
            LaneKind::Bass(0).label().to_string(),
            LaneScore {
                score: 0.2,
                last_changed_cycle: 1,
                change_count: 1,
            },
        );
        state.llm.lane_scores.insert(
            LaneKind::Bass(1).label().to_string(),
            LaneScore {
                score: 0.9,
                last_changed_cycle: 1,
                change_count: 1,
            },
        );
        let mut rng = Xorshift32::new(1234);
        let mut weak = 0;
        let mut strong = 0;
        for _ in 0..500 {
            match pick_jam_lane(&state, &[LaneKind::Bass(0), LaneKind::Bass(1)], &mut rng) {
                Some(LaneKind::Bass(0)) => weak += 1,
                Some(LaneKind::Bass(1)) => strong += 1,
                _ => panic!("unexpected pick"),
            }
        }
        // Expected split ~0.8:0.1 ≈ 8:1.  Well above 3× is a safe lower bound.
        assert!(
            weak > 3 * strong,
            "weak={} strong={} (want weak >> strong)",
            weak,
            strong
        );
    }

    // ── jam_plan ──────────────────────────────────────────────────────────────

    #[test]
    fn jam_plan_returns_single_lane_on_live_rack() {
        // Bass-only rack has Settings + Bass(0) + Fx + Modulation as live.
        let state = bass_only_state();
        let mut rng = Xorshift32::new(42);
        let plan = jam_plan(&state, &mut rng);
        assert_eq!(plan.lanes.len(), 1);
        let expected = [
            LaneKind::Settings,
            LaneKind::Bass(0),
            LaneKind::Fx,
            LaneKind::Modulation,
        ];
        assert!(
            expected.contains(&plan.lanes[0]),
            "jam_plan returned {:?}, expected one of {:?}",
            plan.lanes,
            expected
        );
        assert!(plan.rationale.contains("phase-2"));
    }

    #[test]
    fn jam_plan_empty_on_bare_state() {
        // Default AppState has no rack modules reachable to master, so every
        // voice/kit lane is filtered out; only Settings/Fx/Modulation are
        // always live.  jam_plan still picks one of them — it isn't empty.
        let state = AppState::default();
        let mut rng = Xorshift32::new(1);
        let plan = jam_plan(&state, &mut rng);
        assert_eq!(plan.lanes.len(), 1);
    }

    // ── Per-style dynamism override (Phase 4) ────────────────────────────────

    #[test]
    fn effective_dynamism_no_style_matches_baseline() {
        // With `None` style, fall through to baseline.
        for lane in [LaneKind::Bass(0), LaneKind::KitA, LaneKind::Fx] {
            assert_eq!(effective_dynamism(lane, None), lane_dynamism(lane));
        }
    }

    #[test]
    fn effective_dynamism_exact_label_wins() {
        // A per-voice entry ("bass2") should beat the group entry ("bass").
        let style = style_with_dynamism(&[("bass", 0.5), ("bass2", 0.1)]);
        assert!((effective_dynamism(LaneKind::Bass(1), Some(&style)) - 0.1).abs() < 1e-4);
        // Bass(0) falls through the exact lookup and hits the group.
        assert!((effective_dynamism(LaneKind::Bass(0), Some(&style)) - 0.5).abs() < 1e-4);
    }

    #[test]
    fn effective_dynamism_group_covers_all_bass_voices() {
        // A single "bass" entry covers every bass voice.
        let style = style_with_dynamism(&[("bass", 0.3)]);
        for idx in 0..4 {
            assert!((effective_dynamism(LaneKind::Bass(idx), Some(&style)) - 0.3).abs() < 1e-4);
        }
    }

    #[test]
    fn effective_dynamism_rack_stays_zero_even_under_style() {
        // Rack is user-owned — no style override can unlock it for the
        // scheduler.
        let style = style_with_dynamism(&[("rack", 1.0)]);
        assert_eq!(effective_dynamism(LaneKind::Rack, Some(&style)), 0.0);
    }

    #[test]
    fn effective_dynamism_clamps_out_of_range() {
        let style = style_with_dynamism(&[("fx", 5.0), ("bass", -1.0)]);
        assert_eq!(effective_dynamism(LaneKind::Fx, Some(&style)), 1.0);
        assert_eq!(effective_dynamism(LaneKind::Bass(0), Some(&style)), 0.0);
    }

    #[test]
    fn compute_weight_uses_style_override() {
        // With style override raising fx dynamism above bass, fx should
        // end up with the higher weight when other factors are equal.
        let mut rng_a = Xorshift32::new(1);
        let mut rng_b = Xorshift32::new(1); // same seed → same heat jitter
        let style = style_with_dynamism(&[("fx", 0.95), ("bass", 0.10)]);
        let w_bass = compute_weight(LaneKind::Bass(0), None, 0, 0.0, Some(&style), &mut rng_a);
        let w_fx = compute_weight(LaneKind::Fx, None, 0, 0.0, Some(&style), &mut rng_b);
        assert!(
            w_fx > w_bass,
            "style override should put fx above bass: w_bass={}, w_fx={}",
            w_bass,
            w_fx
        );
    }

    // ── Phase 3: retry queue ─────────────────────────────────────────────────

    #[test]
    fn low_score_queues_retry() {
        use crate::llm::lane_eval::{RETRY_THRESHOLD, record_lane_score};
        let mut llm = crate::state::LlmState::default();
        record_lane_score(&mut llm, LaneKind::Bass(0), RETRY_THRESHOLD - 0.05);
        assert_eq!(llm.retry_queue.len(), 1);
        assert_eq!(llm.retry_queue.front().map(|s| s.as_str()), Some("bass1"));
    }

    #[test]
    fn exactly_at_threshold_queues_retry() {
        // Phase 3 says ≤ threshold — exact match must still queue.
        use crate::llm::lane_eval::{RETRY_THRESHOLD, record_lane_score};
        let mut llm = crate::state::LlmState::default();
        record_lane_score(&mut llm, LaneKind::KitA, RETRY_THRESHOLD);
        assert_eq!(llm.retry_queue.len(), 1);
    }

    #[test]
    fn high_score_does_not_queue() {
        use crate::llm::lane_eval::record_lane_score;
        let mut llm = crate::state::LlmState::default();
        record_lane_score(&mut llm, LaneKind::Bass(0), 0.85);
        assert!(llm.retry_queue.is_empty());
    }

    #[test]
    fn retry_queue_dedupes() {
        // Same lane failing twice should only occupy one queue slot.
        use crate::llm::lane_eval::record_lane_score;
        let mut llm = crate::state::LlmState::default();
        record_lane_score(&mut llm, LaneKind::Bass(0), 0.1);
        record_lane_score(&mut llm, LaneKind::Bass(0), 0.2);
        assert_eq!(llm.retry_queue.len(), 1);
    }

    #[test]
    fn retry_queue_respects_cap() {
        // Queue capped at RETRY_QUEUE_MAX — overflow drops the oldest
        // entry so a stuck-in-retry lane can't block fresh ones.
        use crate::llm::lane_eval::{RETRY_QUEUE_MAX, record_lane_score};
        let mut llm = crate::state::LlmState::default();
        let lanes = [
            LaneKind::Bass(0),
            LaneKind::Bass(1),
            LaneKind::Bass(2),
            LaneKind::Bass(3),
            LaneKind::KitA,
            LaneKind::KitB,
            LaneKind::Fx,
        ];
        for lane in lanes {
            record_lane_score(&mut llm, lane, 0.1);
        }
        assert!(llm.retry_queue.len() <= RETRY_QUEUE_MAX);
        // Oldest (bass1) should have been dropped to make room.
        assert!(!llm.retry_queue.iter().any(|l| l == "bass1"));
        // Newest (fx) should be at the back.
        assert_eq!(llm.retry_queue.back().map(|s| s.as_str()), Some("fx"));
    }

    #[test]
    fn jam_plan_pops_retry_head_before_weighted_pick() {
        // Queue a retry on Bass(0); jam_plan must return it regardless of
        // the weighted picker's preference.
        let mut state = bass_only_state();
        state
            .llm
            .retry_queue
            .push_back(LaneKind::Bass(0).label().to_string());
        let mut rng = Xorshift32::new(1);
        let plan = jam_plan(&state, &mut rng);
        assert_eq!(plan.lanes, vec![LaneKind::Bass(0)]);
        assert!(plan.from_retry);
        assert!(plan.rationale.contains("phase-3 retry"));
    }

    #[test]
    fn jam_plan_skips_dead_retry_entry() {
        // Queue a retry for a lane that isn't live (Hoover on a bass-only
        // rack).  jam_plan must fall through to the weighted picker.
        let mut state = bass_only_state();
        state
            .llm
            .retry_queue
            .push_back(LaneKind::Hoover.label().to_string());
        let mut rng = Xorshift32::new(1);
        let plan = jam_plan(&state, &mut rng);
        assert!(!plan.from_retry);
        assert!(plan.rationale.contains("phase-2"));
    }

    #[test]
    fn consume_retry_prefix_drops_head() {
        use crate::llm::planner_jam::consume_retry_prefix_mut;
        let mut llm = crate::state::LlmState::default();
        llm.retry_queue.push_back("bass1".into());
        llm.retry_queue.push_back("kit_a".into());
        consume_retry_prefix_mut(&mut llm, LaneKind::Bass(0));
        assert_eq!(llm.retry_queue.len(), 1);
        assert_eq!(llm.retry_queue.front().map(|s| s.as_str()), Some("kit_a"));
    }

    #[test]
    fn consume_retry_prefix_drops_dead_heads() {
        // Dead entry at the head gets dropped while consuming the target.
        use crate::llm::planner_jam::consume_retry_prefix_mut;
        let mut llm = crate::state::LlmState::default();
        llm.retry_queue.push_back("garbage_label".into()); // unknown lane
        llm.retry_queue.push_back("bass1".into());
        llm.retry_queue.push_back("fx".into());
        consume_retry_prefix_mut(&mut llm, LaneKind::Bass(0));
        // Both the unknown head and the consumed bass1 are gone.
        assert_eq!(llm.retry_queue.len(), 1);
        assert_eq!(llm.retry_queue.front().map(|s| s.as_str()), Some("fx"));
    }
}
