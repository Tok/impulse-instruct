// ─── tests/lane_auto_tuner_tests.rs ───────────────────────────────────────────
// Lane-score auto-tuner — running average + bias formula + integration
// with the per-(style, lane) lookup helper.

#[cfg(test)]
mod running_avg {
    use crate::state::LaneAverage;

    #[test]
    fn empty_average_has_no_mean() {
        let a = LaneAverage::default();
        assert!(a.mean().is_none());
        assert_eq!(a.n, 0);
    }

    #[test]
    fn single_score_round_trips() {
        let mut a = LaneAverage::default();
        a.update(0.7);
        assert!((a.mean().unwrap() - 0.7).abs() < 1e-6);
        assert_eq!(a.n, 1);
    }

    #[test]
    fn averages_arithmetic_mean() {
        let mut a = LaneAverage::default();
        a.update(0.2);
        a.update(0.4);
        a.update(0.9);
        assert!((a.mean().unwrap() - 0.5).abs() < 1e-6);
    }

    #[test]
    fn out_of_range_scores_clamp_to_unit_interval() {
        // A buggy evaluator that returns a negative score or >1
        // shouldn't pull the running average outside [0, 1].
        let mut a = LaneAverage::default();
        a.update(-0.5);
        a.update(2.0);
        let m = a.mean().unwrap();
        assert!((0.0..=1.0).contains(&m), "mean {m} out of [0, 1]");
    }
}

#[cfg(test)]
mod bias_formula {
    use crate::llm::lane_scheduler::auto_tuner_bias;

    #[test]
    fn unknown_average_is_neutral() {
        assert!((auto_tuner_bias(None, 0) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn neutral_average_is_neutral() {
        // avg=0.5 means no signal — bias should sit at 1.0 even when
        // the sample count is high.
        assert!((auto_tuner_bias(Some(0.5), 100) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn high_average_with_full_trust_boosts_to_about_1_3() {
        let b = auto_tuner_bias(Some(1.0), 100);
        assert!((b - 1.30).abs() < 1e-3, "expected ~1.30, got {b}");
    }

    #[test]
    fn low_average_with_full_trust_reduces_to_about_0_7() {
        let b = auto_tuner_bias(Some(0.0), 100);
        assert!((b - 0.70).abs() < 1e-3, "expected ~0.70, got {b}");
    }

    #[test]
    fn small_sample_count_means_dampened_bias() {
        // Trust ramps over ~5 observations.  At n=1 a perfect 1.0
        // score should produce far less of a boost than at n=100.
        let b1 = auto_tuner_bias(Some(1.0), 1);
        let b100 = auto_tuner_bias(Some(1.0), 100);
        assert!(
            b1 < b100,
            "low-n should boost less than high-n: {b1} vs {b100}"
        );
        assert!(b1 > 1.0, "still some positive bias even at n=1: {b1}");
    }

    #[test]
    fn output_is_bounded_in_0_7_to_1_3() {
        // Exhaustive ish: try a grid of (avg, n) and verify the
        // contract.  Important: a runaway average plus a high
        // sample count must NOT push past 1.3 / below 0.7.
        for n in [0u32, 1, 5, 20, 1_000_000] {
            for avg in [0.0_f32, 0.25, 0.5, 0.75, 1.0] {
                let b = auto_tuner_bias(Some(avg), n);
                assert!(
                    (0.7..=1.3).contains(&b),
                    "out of range: avg={avg} n={n} → bias={b}"
                );
            }
        }
    }
}

#[cfg(test)]
mod integration {
    use crate::llm::lane_scheduler::lane_auto_tuner_bias;
    use crate::llm::lanes::LaneKind;
    use crate::state::{AppState, LaneAverage, lane_avg_key};

    fn populate(s: &mut AppState, style: &str, lane_label: &str, scores: &[f32]) {
        let key = lane_avg_key(style, lane_label);
        let avg = s.llm.lane_avg_per_style.entry(key).or_default();
        for sc in scores {
            avg.update(*sc);
        }
    }

    #[test]
    fn bias_neutral_when_no_active_style() {
        let s = AppState::default();
        let (b, n) = lane_auto_tuner_bias(&s, LaneKind::Bass(0));
        assert!((b - 1.0).abs() < 1e-6);
        assert_eq!(n, 0);
    }

    #[test]
    fn bias_neutral_when_style_set_but_no_history() {
        let mut s = AppState::default();
        s.llm.active_style = Some("jungle".into());
        let (b, n) = lane_auto_tuner_bias(&s, LaneKind::Bass(0));
        assert!((b - 1.0).abs() < 1e-6);
        assert_eq!(n, 0);
    }

    #[test]
    fn high_history_in_active_style_boosts_bias() {
        let mut s = AppState::default();
        s.llm.active_style = Some("jungle".into());
        populate(
            &mut s,
            "jungle",
            "bass1",
            &[0.95, 0.92, 0.98, 0.9, 0.96, 0.94],
        );
        let (b, n) = lane_auto_tuner_bias(&s, LaneKind::Bass(0));
        assert!(b > 1.0, "expected boost, got {b}");
        assert!(n >= 5);
    }

    #[test]
    fn history_in_other_style_does_not_affect_bias() {
        // A bass lane that scored 1.0 in jungle should NOT bias the
        // same lane when the active style is now drum_and_bass.
        let mut s = AppState::default();
        s.llm.active_style = Some("drum_and_bass".into());
        populate(&mut s, "jungle", "bass1", &[1.0; 10]);
        let (b, _) = lane_auto_tuner_bias(&s, LaneKind::Bass(0));
        assert!((b - 1.0).abs() < 1e-6, "bias should be neutral: {b}");
    }

    #[test]
    fn lane_average_struct_serde_compatible() {
        // Defensive: even though it's #[serde(skip)] on LlmState,
        // the LaneAverage struct itself must still be cheap to clone
        // (it lives in a HashMap, copied during snapshots).
        let mut a = LaneAverage::default();
        a.update(0.5);
        let copy = a;
        assert_eq!(a, copy);
    }
}
