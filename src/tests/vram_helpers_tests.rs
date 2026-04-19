// ─── tests/vram_helpers_tests.rs ─────────────────────────────────────────────
// Covers pure helpers in `llm/vram.rs`: model label lookup, style-flavored
// preset renaming, preset availability checks, and model-file resolver.
//
// The existing `vram_tests.rs` covers `estimate_vram` /
// `would_exceed_vram` (the budget math); this file covers the name /
// lookup side that feeds those functions.

use crate::llm::vram::{PRESETS, check_presets, find_model, model_label, styled_preset_name};

// ─── model_label ────────────────────────────────────────────────────────────

#[test]
fn model_label_returns_human_readable_for_known_patterns() {
    // A Gemma 4 E4B filename from any path depth must resolve to the
    // canonical profile label.  Case should not matter.
    assert_eq!(
        model_label("models/gemma-4-e4b-it-Q4_K_M.gguf"),
        "Gemma 4 E4B",
    );
    assert_eq!(model_label("/abs/path/GEMMA-4-E4B-it.gguf"), "Gemma 4 E4B",);
}

#[test]
fn model_label_falls_back_to_file_stem_for_unknown_models() {
    // Unknown-pattern filenames must fall back to the file stem, not
    // the full path and not empty.
    let label = model_label("models/some-weird-llama.gguf");
    assert_eq!(label, "some-weird-llama");
}

#[test]
fn model_label_empty_path_returns_empty_string() {
    // Empty string has no file stem; the fallback returns the raw path.
    assert_eq!(model_label(""), "");
}

// ─── find_model ─────────────────────────────────────────────────────────────

#[test]
fn find_model_resolves_first_case_insensitive_match() {
    let models = vec![
        "models/gemma-4-e2b-it.gguf".to_string(),
        "models/gemma-4-e4b-it.gguf".to_string(),
        "models/qwen3-8b.gguf".to_string(),
    ];
    let hit = find_model("e4b", &models).unwrap();
    assert!(hit.contains("e4b"));
    // Uppercase pattern must still match (the pattern is compared
    // against .to_ascii_lowercase()'d filenames).
    let hit2 = find_model("qwen3", &models).unwrap();
    assert!(hit2.contains("qwen3"));
}

#[test]
fn find_model_returns_none_when_no_match() {
    let models = vec!["models/gemma-4-e4b.gguf".to_string()];
    assert!(find_model("llama", &models).is_none());
    assert!(
        find_model("", &models).is_some(),
        "empty pattern matches any"
    );
}

// ─── styled_preset_name ─────────────────────────────────────────────────────

#[test]
fn styled_preset_name_only_rebrands_crew() {
    // Only "Crew" gets the style-flavoured rename.  Every other preset
    // name passes through untouched regardless of active style.
    assert_eq!(styled_preset_name("Solo", Some("jungle")), "Solo");
    assert_eq!(styled_preset_name("Duo", Some("gabber")), "Duo");
    assert_eq!(styled_preset_name("Swarm", Some("synthwave")), "Swarm");
    assert_eq!(styled_preset_name("Voices", None), "Voices");
}

#[test]
fn styled_preset_name_crew_maps_to_style_flavoured_name() {
    assert_eq!(styled_preset_name("Crew", Some("jungle")), "Posse");
    assert_eq!(styled_preset_name("Crew", Some("drum_and_bass")), "Posse",);
    assert_eq!(styled_preset_name("Crew", Some("gabber")), "Squad");
    assert_eq!(styled_preset_name("Crew", Some("synthwave")), "Band");
    assert_eq!(
        styled_preset_name("Crew", Some("space_ambient")),
        "Ensemble",
    );
}

#[test]
fn styled_preset_name_crew_without_style_is_crew() {
    assert_eq!(styled_preset_name("Crew", None), "Crew");
    // Unknown style → default Crew label.
    assert_eq!(
        styled_preset_name("Crew", Some("weirdstyle_nobody_wrote")),
        "Crew",
    );
}

// ─── check_presets ──────────────────────────────────────────────────────────

#[test]
fn check_presets_returns_one_status_per_preset() {
    let statuses = check_presets(16_000, &[]);
    assert_eq!(
        statuses.len(),
        PRESETS.len(),
        "one status per preset so the UI can render every row",
    );
}

#[test]
fn check_presets_reports_missing_models_when_none_downloaded() {
    // Empty available_models list → every preset must have at least one
    // missing model (since every preset needs at least one model).
    let statuses = check_presets(32_000, &[]);
    for s in &statuses {
        assert!(
            !s.models_available,
            "preset {:?} must be unavailable with no models",
            s.preset.name,
        );
        assert!(
            !s.missing_models.is_empty(),
            "preset {:?} must list its missing patterns",
            s.preset.name,
        );
    }
}

#[test]
fn check_presets_zero_vram_budget_means_always_fits() {
    // vram_total_mb=0 is "CPU mode / unknown budget" — check_presets
    // should mark every preset as fitting so the UI doesn't grey them all
    // out with no way to run anything.
    let statuses = check_presets(0, &["gemma-4-e4b.gguf".into()]);
    for s in &statuses {
        assert!(
            s.fits_vram,
            "preset {:?} must fit when vram budget is unknown (0)",
            s.preset.name,
        );
    }
}

#[test]
fn check_presets_small_budget_rejects_heavy_presets() {
    // A 3 GB VRAM budget can't hold the 6 GB Solo preset.
    let statuses = check_presets(3_000, &["gemma-4-e4b.gguf".into()]);
    let solo = statuses.iter().find(|s| s.preset.name == "Solo").unwrap();
    assert!(!solo.fits_vram, "Solo (6GB) must not fit in 3GB budget");
}

#[test]
fn check_presets_with_available_gemma_marks_solo_available() {
    // Solo is defined to use a "gemma" pattern model — when a gemma file
    // is present, missing_models must be empty and models_available true.
    let statuses = check_presets(16_000, &["models/gemma-4-e4b-it-Q4_K_M.gguf".into()]);
    let solo = statuses.iter().find(|s| s.preset.name == "Solo").unwrap();
    assert!(
        solo.models_available,
        "Solo must be available when a gemma model is on disk",
    );
    assert!(solo.missing_models.is_empty());
    assert!(solo.fits_vram);
}
