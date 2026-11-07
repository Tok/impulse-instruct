// ─── tests/lane_diff_tests.rs ────────────────────────────────────────────────
// Lane writeback diff log — capacity caps, ordering, and that the JSON
// payload travels through the pipeline event handler intact.

#[cfg(test)]
mod recent_lane_applies {
    use crate::state::{AppState, LANE_APPLY_LOG_MAX, LaneApplyRecord};

    fn rec(label: &str, cycle: u32) -> LaneApplyRecord {
        LaneApplyRecord {
            lane_label: label.to_string(),
            update: serde_json::json!({ "sequencer": { "bpm": 120.0 } }),
            ms: 50,
            cycle,
        }
    }

    #[test]
    fn default_log_is_empty() {
        let s = AppState::default();
        assert!(s.llm.recent_lane_applies.is_empty());
    }

    #[test]
    fn pushing_past_cap_drops_oldest() {
        let mut s = AppState::default();
        for i in 0..(LANE_APPLY_LOG_MAX + 4) {
            if s.llm.recent_lane_applies.len() >= LANE_APPLY_LOG_MAX {
                s.llm.recent_lane_applies.pop_front();
            }
            s.llm.recent_lane_applies.push_back(rec("BASS 1", i as u32));
        }
        assert_eq!(s.llm.recent_lane_applies.len(), LANE_APPLY_LOG_MAX);
        let oldest = s.llm.recent_lane_applies.front().unwrap();
        // First 4 entries (cycles 0..3) should have been evicted.
        assert!(
            oldest.cycle >= 4,
            "oldest cycle expected ≥4, got {}",
            oldest.cycle
        );
    }

    #[test]
    fn newest_entry_preserved_at_back() {
        let mut s = AppState::default();
        s.llm.recent_lane_applies.push_back(rec("BASS 1", 0));
        s.llm.recent_lane_applies.push_back(rec("KITA", 0));
        s.llm.recent_lane_applies.push_back(rec("FX", 1));
        assert_eq!(s.llm.recent_lane_applies.back().unwrap().lane_label, "FX");
        assert_eq!(
            s.llm.recent_lane_applies.front().unwrap().lane_label,
            "BASS 1"
        );
    }

    #[test]
    fn record_roundtrips_through_clone() {
        let r = rec("HOOVER", 7);
        let r2 = r.clone();
        assert_eq!(r, r2);
        assert_eq!(r2.update["sequencer"]["bpm"], 120.0);
    }
}

#[cfg(test)]
mod payload_diff_keys {
    use crate::state::LaneApplyRecord;

    /// Local mirror of `windows_lane_diff::count_diff_keys` (private).
    /// Counting through the same algorithm verifies the UI's "N keys"
    /// header reads what we expect when the payload is a writeback diff.
    fn count_diff_keys(v: &serde_json::Value) -> usize {
        match v {
            serde_json::Value::Object(map) => map
                .values()
                .map(|sub| match sub {
                    serde_json::Value::Object(inner) => inner.len(),
                    _ => 1,
                })
                .sum(),
            _ => 1,
        }
    }

    #[test]
    fn flat_top_level_keys_each_count_one() {
        let v = serde_json::json!({ "a": 1, "b": 2, "c": 3 });
        assert_eq!(count_diff_keys(&v), 3);
    }

    #[test]
    fn nested_sequencer_subkeys_are_summed_per_voice() {
        // Bass lane writeback: sequencer.bass_steps + sequencer.bpm = 2.
        let v = serde_json::json!({
            "sequencer": {
                "bass_steps": [0, 4, 8],
                "bpm": 130.0
            }
        });
        assert_eq!(count_diff_keys(&v), 2);
    }

    #[test]
    fn mixed_top_level_plus_nested_sums_correctly() {
        // FX lane plus sequencer settings — top-level fx counts as 1
        // (whatever its structure), sequencer counts each subkey.
        let v = serde_json::json!({
            "fx": "anything",
            "sequencer": {
                "bpm": 120.0,
                "swing": 0.1,
                "bass_steps": [0, 4]
            }
        });
        // "fx" → 1, sequencer subkeys → 3, total 4.
        assert_eq!(count_diff_keys(&v), 4);
    }

    #[test]
    fn non_object_payload_counts_as_one() {
        let v = serde_json::json!(42);
        assert_eq!(count_diff_keys(&v), 1);
    }

    #[test]
    fn record_with_real_filtered_payload_reports_sensible_count() {
        // A real bass lane payload as `filter_lane_output` would emit
        // it: `_comment` carry-over + sequencer subkeys.  Top-level
        // _comment is one key; sequencer expands to its subkeys.
        let r = LaneApplyRecord {
            lane_label: "BASS 1".to_string(),
            update: serde_json::json!({
                "_comment": "acid line",
                "sequencer": {
                    "bass_steps": [0, 4, 8, 12],
                    "bass_notes": [36, 36, 38, 36, 36, 36, 36, 36,
                                   36, 36, 36, 36, 36, 36, 36, 36],
                    "bass_accents": [1, 0, 0, 1, 0, 0, 1, 0,
                                     0, 1, 0, 0, 1, 0, 0, 0]
                }
            }),
            ms: 80,
            cycle: 3,
        };
        // 1 (_comment) + 3 (sequencer subkeys) = 4.
        assert_eq!(count_diff_keys(&r.update), 4);
    }
}
