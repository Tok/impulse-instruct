// ─── tests/few_shot_tests.rs ──────────────────────────────────────────────────
// Per-lane few-shot example bank — slug map, file loader, render block.
// The lane-prompt integration is exercised indirectly via the existing
// lane prompt tests (the new EXAMPLES block only appears when an
// example file exists, which the unit tests don't put on disk).

#[cfg(test)]
mod slug {
    use crate::llm::few_shot::lane_slug;
    use crate::llm::lanes::LaneKind;

    #[test]
    fn settings_maps_to_settings() {
        assert_eq!(lane_slug(LaneKind::Settings), "settings");
    }

    #[test]
    fn bass_voices_get_one_indexed_slugs() {
        assert_eq!(lane_slug(LaneKind::Bass(0)), "bass1");
        assert_eq!(lane_slug(LaneKind::Bass(1)), "bass2");
        assert_eq!(lane_slug(LaneKind::Bass(2)), "bass3");
        assert_eq!(lane_slug(LaneKind::Bass(3)), "bass4");
    }

    #[test]
    fn kit_lanes_get_underscored_slugs() {
        assert_eq!(lane_slug(LaneKind::KitA), "kit_a");
        assert_eq!(lane_slug(LaneKind::KitB), "kit_b");
    }

    #[test]
    fn fx_modulation_rack_have_distinct_slugs() {
        assert_eq!(lane_slug(LaneKind::Fx), "fx");
        assert_eq!(lane_slug(LaneKind::Modulation), "modulation");
        assert_eq!(lane_slug(LaneKind::Rack), "rack");
    }
}

#[cfg(test)]
mod loader {
    use crate::llm::few_shot::{FewShotExample, load_examples_from_path, render_examples_section};

    fn temp_dir() -> std::path::PathBuf {
        let pid = std::process::id();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!("impulse_few_shot_{pid}_{nanos}"))
    }

    #[test]
    fn missing_file_yields_empty_list() {
        let dir = temp_dir();
        let path = dir.join("nonexistent.json");
        let out = load_examples_from_path(&path);
        assert!(out.is_empty());
    }

    #[test]
    fn well_formed_array_round_trips() {
        let dir = temp_dir();
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("bass1.json");
        let payload = r#"[
            {"prompt": "warmer", "output": "{\"bass\":{\"cutoff\":0.55}}"},
            {"prompt": "fold reso", "output": "{\"bass\":{\"resonance\":0.7}}"}
        ]"#;
        std::fs::write(&path, payload).unwrap();
        let out = load_examples_from_path(&path);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].prompt, "warmer");
        assert!(out[0].output.contains("0.55"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn malformed_file_yields_empty_list() {
        // Non-JSON bytes should NOT panic — the loader logs and
        // returns an empty Vec so few-shots are best-effort.
        let dir = temp_dir();
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("garbage.json");
        std::fs::write(&path, "this is not JSON").unwrap();
        let out = load_examples_from_path(&path);
        assert!(out.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn empty_array_renders_to_empty_section() {
        // Zero-example case must produce an empty string so the
        // lane-prompt format!() call doesn't insert a stray
        // "EXAMPLES:" header with nothing under it.
        let s = render_examples_section(&[]);
        assert!(s.is_empty(), "empty examples should render empty");
    }

    #[test]
    fn render_includes_each_example_prompt_and_output() {
        let examples = vec![
            FewShotExample {
                prompt: "warmer".into(),
                output: r#"{"bass":{"cutoff":0.55}}"#.into(),
            },
            FewShotExample {
                prompt: "fold reso".into(),
                output: r#"{"bass":{"resonance":0.7}}"#.into(),
            },
        ];
        let block = render_examples_section(&examples);
        assert!(block.contains("Example 1"));
        assert!(block.contains("Example 2"));
        assert!(block.contains("warmer"));
        assert!(block.contains("\"cutoff\":0.55"));
        assert!(block.contains("fold reso"));
    }

    #[test]
    fn render_truncates_at_five_examples() {
        // Past 5 entries the prompt would balloon and dilute the
        // signal; the renderer caps at 5.
        let mut examples = Vec::new();
        for i in 0..10 {
            examples.push(FewShotExample {
                prompt: format!("p{i}"),
                output: format!("o{i}"),
            });
        }
        let block = render_examples_section(&examples);
        assert!(block.contains("Example 5"));
        assert!(!block.contains("Example 6"), "renderer should cap at 5");
    }
}
