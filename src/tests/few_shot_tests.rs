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

#[cfg(test)]
mod path_building {
    use crate::llm::few_shot::example_path_for;
    use crate::llm::lanes::LaneKind;

    /// `example_path_for` puts the slug under `examples/<slug>.json`
    /// relative to the working directory.  Pin the convention so a
    /// future move of the example bank shows up as a deliberate
    /// constant change.
    #[test]
    fn returns_examples_subdir_path_with_json_suffix() {
        let p = example_path_for(LaneKind::Settings);
        assert_eq!(p.to_string_lossy(), "examples/settings.json");
    }

    /// Per-bass-voice slug (bass1 / bass2 / …) flows through into the
    /// path so each voice has its own example bank.
    #[test]
    fn distinct_bass_voices_map_to_distinct_files() {
        assert_eq!(
            example_path_for(LaneKind::Bass(0)).to_string_lossy(),
            "examples/bass1.json"
        );
        assert_eq!(
            example_path_for(LaneKind::Bass(2)).to_string_lossy(),
            "examples/bass3.json"
        );
    }

    /// Every lane variant produces a unique file path — a regression
    /// where two lanes collapsed to the same slug would silently
    /// share their example banks.
    #[test]
    fn every_lane_variant_has_distinct_path() {
        use std::collections::HashSet;
        let lanes = [
            LaneKind::Settings,
            LaneKind::Bass(0),
            LaneKind::Bass(1),
            LaneKind::Bass(2),
            LaneKind::Bass(3),
            LaneKind::KitA,
            LaneKind::KitB,
            LaneKind::Amen,
            LaneKind::Hoover,
            LaneKind::An1x,
            LaneKind::Fx,
            LaneKind::Modulation,
            LaneKind::Rack,
        ];
        let paths: HashSet<_> = lanes
            .iter()
            .map(|l| example_path_for(*l).to_string_lossy().to_string())
            .collect();
        assert_eq!(paths.len(), lanes.len(), "two lanes share an example path");
    }
}

#[cfg(test)]
mod load_for_lane {
    use crate::llm::few_shot::load_examples_for_lane;
    use crate::llm::lanes::LaneKind;

    /// Calling against a lane whose example file does not exist on
    /// disk returns an empty Vec — best-effort enrichment, never a
    /// hard dep.  Unit tests run from the repo root where the
    /// `examples/` dir may or may not be present; either way the
    /// fallback path has to be silent and non-panicking.
    #[test]
    fn missing_or_unparseable_file_yields_empty_vec() {
        // The contract is "no panic, returns Vec" regardless of
        // whether `examples/<slug>.json` exists in the working
        // directory — unit tests run from the repo root and the
        // example bank may or may not be present.  Calling the
        // function on a few representative lanes verifies the
        // fallback path doesn't panic.
        let _ = load_examples_for_lane(LaneKind::Bass(99));
        let _ = load_examples_for_lane(LaneKind::Modulation);
        let _ = load_examples_for_lane(LaneKind::Rack);
    }
}
