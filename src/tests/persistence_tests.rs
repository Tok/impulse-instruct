// ─── tests/persistence_tests.rs ──────────────────────────────────────────────
// Tests for state/persistence.rs: SessionData round-trip, apply_session,
// AppState serialize/deserialize, save_project/load_project via temp dir.

#[cfg(test)]
mod session_data_tests {
    use crate::state::AutosaveInterval;
    use crate::state::persistence::SessionData;

    #[test]
    fn session_data_serialize_deserialize_round_trip() {
        let data = SessionData {
            active_style: Some("acid".to_string()),
            custom_style_text: Some("custom text".to_string()),
            user_instructions: Some("be loud".to_string()),
            persona_name: Some("DJ Bot".to_string()),
            model_path: Some("models/test.gguf".to_string()),
            show_cables: Some(true),
            rack_flipped: Some(false),
            ui_scale: Some(1.5),
            log_level_idx: Some(2),
            autosave_interval: Some(AutosaveInterval::FiveSec),
            wasd_as_arrows: Some(true),
            enable_thinking: Some(true),
            show_thinking_in_log: Some(false),
            temperature: Some(0.7),
            wizard_done: Some(true),
            ..Default::default()
        };
        let json = serde_json::to_string(&data).expect("serialize");
        let loaded: SessionData = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(loaded.active_style, data.active_style);
        assert_eq!(loaded.custom_style_text, data.custom_style_text);
        assert_eq!(loaded.user_instructions, data.user_instructions);
        assert_eq!(loaded.persona_name, data.persona_name);
        assert_eq!(loaded.model_path, data.model_path);
        assert_eq!(loaded.show_cables, Some(true));
        assert_eq!(loaded.rack_flipped, Some(false));
        assert_eq!(loaded.ui_scale, Some(1.5));
        assert_eq!(loaded.log_level_idx, Some(2));
        assert_eq!(loaded.autosave_interval, Some(AutosaveInterval::FiveSec));
        assert_eq!(loaded.wasd_as_arrows, Some(true));
        assert_eq!(loaded.enable_thinking, Some(true));
        assert_eq!(loaded.show_thinking_in_log, Some(false));
        assert_eq!(loaded.temperature, Some(0.7));
        assert_eq!(loaded.wizard_done, Some(true));
    }

    #[test]
    fn session_data_all_none_round_trip() {
        let data = SessionData::default();
        let json = serde_json::to_string(&data).expect("serialize");
        let loaded: SessionData = serde_json::from_str(&json).expect("deserialize");
        assert!(loaded.active_style.is_none());
        assert!(loaded.rack.is_none());
        assert!(loaded.model_path.is_none());
        assert!(loaded.wizard_done.is_none());
    }

    #[test]
    fn session_data_missing_fields_deserialize_with_defaults() {
        // Simulates loading a session.json from an older version with fewer fields
        let json = r#"{"active_style":"techno"}"#;
        let loaded: SessionData = serde_json::from_str(json).expect("deserialize");
        assert_eq!(loaded.active_style, Some("techno".to_string()));
        assert!(loaded.rack_flipped.is_none());
        assert!(loaded.wasd_as_arrows.is_none());
        assert!(loaded.wizard_done.is_none());
        assert!(loaded.temperature.is_none());
    }
}

#[cfg(test)]
mod apply_session_tests {
    use crate::state::persistence::{SessionData, apply_session};
    use crate::state::{AppState, AutosaveInterval, ModuleKind};

    #[test]
    fn apply_session_sets_style_and_instructions() {
        let mut state = AppState::default();
        let data = SessionData {
            active_style: Some("dnb".to_string()),
            custom_style_text: Some("fast breaks".to_string()),
            user_instructions: Some("keep it rolling".to_string()),
            ..Default::default()
        };
        apply_session(&mut state, data);
        assert_eq!(state.llm.active_style, Some("dnb".to_string()));
        assert_eq!(state.llm.custom_style_text, "fast breaks");
        assert_eq!(state.llm.user_instructions, "keep it rolling");
    }

    #[test]
    fn apply_session_sets_persona_and_model() {
        let mut state = AppState::default();
        let data = SessionData {
            persona_name: Some("Acid Bot".to_string()),
            model_path: Some("models/test.gguf".to_string()),
            ..Default::default()
        };
        apply_session(&mut state, data);
        assert_eq!(state.llm.persona_name, "Acid Bot");
        assert_eq!(state.llm.model_path, "models/test.gguf");
    }

    #[test]
    fn apply_session_ui_prefs_blob_overrides_legacy_fields() {
        // When `ui_prefs` is present it carries the full UiPrefs and
        // wins over the individual legacy Option fields.  The
        // canonical path for per-component Huth toggles / header viz
        // toggles to survive a restart.
        use crate::state::UiPrefs;
        let mut state = AppState::default();
        let mut prefs = UiPrefs::default();
        prefs.huth_piano = false;
        prefs.huth_bar_osc = true;
        prefs.huth_ring_osc = true;
        prefs.huth_spectrum = true;
        prefs.show_bar_oscilloscope = true;
        prefs.show_spectrum_bars = false;
        prefs.ui_scale = 1.25;
        let data = SessionData {
            ui_prefs: Some(prefs),
            // Legacy fields deliberately contradict the blob — the
            // blob must win.
            ui_scale: Some(2.5),
            wasd_as_arrows: Some(true),
            ..Default::default()
        };
        apply_session(&mut state, data);
        assert!(!state.ui_prefs.huth_piano);
        assert!(state.ui_prefs.huth_bar_osc);
        assert!(state.ui_prefs.huth_ring_osc);
        assert!(state.ui_prefs.huth_spectrum);
        assert!(state.ui_prefs.show_bar_oscilloscope);
        assert!(!state.ui_prefs.show_spectrum_bars);
        assert!((state.ui_prefs.ui_scale - 1.25).abs() < 1e-4);
        // Legacy wasd_as_arrows is ignored when the blob wins — the
        // blob's own default (false) stays in effect.
        assert!(!state.ui_prefs.wasd_as_arrows);
    }

    #[test]
    fn apply_session_falls_back_to_legacy_fields_without_blob() {
        // Pre-blob sessions only set the individual Option fields.
        // Those must still work so nobody's ui_scale / log level
        // silently resets on upgrade.
        let mut state = AppState::default();
        let data = SessionData {
            ui_prefs: None,
            ui_scale: Some(0.75),
            log_level_idx: Some(3),
            wasd_as_arrows: Some(true),
            autosync_rack_on_start: Some(true),
            ..Default::default()
        };
        apply_session(&mut state, data);
        assert!((state.ui_prefs.ui_scale - 0.75).abs() < 1e-4);
        assert_eq!(state.ui_prefs.log_level_idx, 3);
        assert!(state.ui_prefs.wasd_as_arrows);
        assert!(state.ui_prefs.autosync_rack_on_start);
    }

    #[test]
    fn apply_session_empty_model_path_not_applied() {
        let mut state = AppState::default();
        let orig_path = state.llm.model_path.clone();
        let data = SessionData {
            model_path: Some(String::new()),
            ..Default::default()
        };
        apply_session(&mut state, data);
        assert_eq!(
            state.llm.model_path, orig_path,
            "empty model path should be skipped"
        );
    }

    #[test]
    fn apply_session_autosync_rack_flag_round_trips() {
        use crate::state::persistence::{SessionData, apply_session};
        let mut state = AppState::default();
        assert!(
            !state.ui_prefs.autosync_rack_on_start,
            "default is off — existing users keep their customised rack"
        );
        let data = SessionData {
            autosync_rack_on_start: Some(true),
            ..Default::default()
        };
        apply_session(&mut state, data);
        assert!(state.ui_prefs.autosync_rack_on_start);
    }

    #[test]
    fn apply_session_ui_prefs() {
        let mut state = AppState::default();
        let data = SessionData {
            ui_scale: Some(2.0),
            log_level_idx: Some(3),
            autosave_interval: Some(AutosaveInterval::ThirtySec),
            wasd_as_arrows: Some(true),
            ..Default::default()
        };
        apply_session(&mut state, data);
        assert!((state.ui_prefs.ui_scale - 2.0).abs() < 1e-4);
        assert_eq!(state.ui_prefs.log_level_idx, 3);
        assert_eq!(
            state.ui_prefs.autosave_interval,
            AutosaveInterval::ThirtySec
        );
        assert!(state.ui_prefs.wasd_as_arrows);
    }

    #[test]
    fn apply_session_ui_scale_clamped() {
        let mut state = AppState::default();
        let data = SessionData {
            ui_scale: Some(10.0), // way too high
            ..Default::default()
        };
        apply_session(&mut state, data);
        assert!(
            state.ui_prefs.ui_scale <= 3.0,
            "ui_scale should clamp to 3.0"
        );

        let data2 = SessionData {
            ui_scale: Some(0.1), // too low
            ..Default::default()
        };
        apply_session(&mut state, data2);
        assert!(
            state.ui_prefs.ui_scale >= 0.5,
            "ui_scale should clamp to 0.5"
        );
    }

    #[test]
    fn apply_session_llm_thinking_and_temperature() {
        let mut state = AppState::default();
        let data = SessionData {
            enable_thinking: Some(true),
            show_thinking_in_log: Some(true),
            temperature: Some(0.9),
            ..Default::default()
        };
        apply_session(&mut state, data);
        assert!(state.llm.enable_thinking);
        assert!(state.llm.show_thinking_in_log);
        assert!((state.llm.temperature - 0.9).abs() < 1e-4);
    }

    #[test]
    fn apply_session_none_fields_leave_state_unchanged() {
        let mut state = AppState::default();
        let orig_style = state.llm.active_style.clone();
        let orig_scale = state.ui_prefs.ui_scale;
        let data = SessionData::default(); // all None
        apply_session(&mut state, data);
        assert_eq!(state.llm.active_style, orig_style);
        assert!((state.ui_prefs.ui_scale - orig_scale).abs() < 1e-4);
    }

    #[test]
    fn apply_session_migration_adds_llm_console_if_missing() {
        let mut state = AppState::default();
        // Remove LlmConsole from rack
        state
            .rack
            .modules
            .retain(|m| m.kind != ModuleKind::LlmConsole);
        assert!(
            !state
                .rack
                .modules
                .iter()
                .any(|m| m.kind == ModuleKind::LlmConsole)
        );

        let data = SessionData::default();
        apply_session(&mut state, data);
        assert!(
            state
                .rack
                .modules
                .iter()
                .any(|m| m.kind == ModuleKind::LlmConsole),
            "migration should add LlmConsole"
        );
    }

    #[test]
    fn apply_session_migration_adds_llm_agent_if_missing() {
        let mut state = AppState::default();
        // Remove all LlmAgent modules
        state
            .rack
            .modules
            .retain(|m| m.kind != ModuleKind::LlmAgent);
        state.llm_agents.clear();

        let data = SessionData::default();
        apply_session(&mut state, data);
        assert!(
            state
                .rack
                .modules
                .iter()
                .any(|m| m.kind == ModuleKind::LlmAgent),
            "migration should add LlmAgent"
        );
        assert!(
            !state.llm_agents.is_empty(),
            "migration should add agent state"
        );
    }

    #[test]
    fn apply_session_rack_override() {
        let mut state = AppState::default();
        let custom_rack = crate::state::RackState {
            modules: Vec::new(),
            cables: Vec::new(),
            next_id: 999,
            dyn_sequencer_rows: None,
        };
        let data = SessionData {
            rack: Some(custom_rack),
            ..Default::default()
        };
        // Rack will be replaced, then migration kicks in to add LlmConsole/LlmAgent
        apply_session(&mut state, data);
        assert!(state.rack.next_id >= 999);
    }
}

#[cfg(test)]
mod app_state_serde_tests {
    use crate::state::AppState;

    #[test]
    fn app_state_serialize_deserialize_round_trip() {
        let state = AppState::default();
        let json = serde_json::to_string(&state).expect("serialize AppState");
        let loaded: AppState = serde_json::from_str(&json).expect("deserialize AppState");

        // Spot-check key fields survived the round-trip
        assert_eq!(loaded.sequencer.bpm, state.sequencer.bpm);
        assert_eq!(loaded.sequencer.steps, state.sequencer.steps);
        assert_eq!(
            loaded.bass_voices[0].synth.cutoff,
            state.bass_voices[0].synth.cutoff
        );
        assert_eq!(loaded.fx.reverb_mix, state.fx.reverb_mix);
        assert_eq!(loaded.hoover.enabled, state.hoover.enabled);
        assert_eq!(loaded.an1x.enabled, state.an1x.enabled);
        assert_eq!(loaded.rack.modules.len(), state.rack.modules.len());
        assert_eq!(loaded.rack.cables.len(), state.rack.cables.len());
    }

    #[test]
    fn app_state_modified_then_round_trip() {
        let mut state = AppState::default();
        state.sequencer.bpm = 175.0;
        state.bass_voices[0].synth.cutoff = 0.42;
        state.fx.reverb_mix = 0.77;
        state.hoover.enabled = true;
        state.an1x.drift = 0.15;

        let json = serde_json::to_string(&state).expect("serialize");
        let loaded: AppState = serde_json::from_str(&json).expect("deserialize");

        assert!((loaded.sequencer.bpm - 175.0).abs() < 0.01);
        assert!((loaded.bass_voices[0].synth.cutoff - 0.42).abs() < 1e-4);
        assert!((loaded.fx.reverb_mix - 0.77).abs() < 1e-4);
        assert!(loaded.hoover.enabled);
        assert!((loaded.an1x.drift - 0.15).abs() < 1e-4);
    }
}

#[cfg(test)]
mod project_file_tests {
    use crate::state::AppState;
    use crate::state::persistence::load_project;

    #[test]
    fn save_and_load_project_round_trip() {
        let mut state = AppState::default();
        state.sequencer.bpm = 160.0;
        state.bass_voices[0].synth.cutoff = 0.55;

        // save_project writes to CWD; use a temp dir
        let dir = std::env::temp_dir().join("impulse_test_project");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("test_project.json");
        let json = serde_json::to_string_pretty(&state).expect("serialize");
        std::fs::write(&path, json).expect("write");

        let loaded = load_project(&path).expect("load");
        assert!((loaded.sequencer.bpm - 160.0).abs() < 0.01);
        assert!((loaded.bass_voices[0].synth.cutoff - 0.55).abs() < 1e-4);

        // Cleanup
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn load_project_nonexistent_file_errors() {
        let result = load_project(std::path::Path::new("/tmp/nonexistent_impulse_test.json"));
        assert!(result.is_err());
    }

    #[test]
    fn load_project_invalid_json_errors() {
        let dir = std::env::temp_dir();
        let path = dir.join("impulse_test_bad.json");
        std::fs::write(&path, "not valid json {{{").expect("write");
        let result = load_project(&path);
        assert!(result.is_err());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn list_recent_projects_orders_newest_first_and_filters_prefix() {
        use crate::ui::header::list_recent_projects_in;
        // Isolated tempdir so the test never reads real project files.
        let dir = std::env::temp_dir().join("impulse_test_recent_projects");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("mkdir");

        // Write three project files with distinct, increasing mtimes.
        let paths = ["project-a.json", "project-b.json", "project-c.json"];
        for (i, name) in paths.iter().enumerate() {
            let p = dir.join(name);
            std::fs::write(&p, "{}").expect("write");
            // Push mtime forward so ordering is stable — sleep is too slow,
            // so use filetime via std::fs::File::set_modified instead.
            let f = std::fs::OpenOptions::new()
                .write(true)
                .open(&p)
                .expect("open");
            let t = std::time::SystemTime::UNIX_EPOCH
                + std::time::Duration::from_secs(1_000_000 + i as u64);
            f.set_modified(t).expect("set mtime");
        }
        // A file that doesn't match the prefix — must be excluded.
        std::fs::write(dir.join("notes.txt"), "x").expect("write noise");
        std::fs::write(dir.join("other.json"), "{}").expect("write noise");

        let found = list_recent_projects_in(&dir);
        let names: Vec<String> = found
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        // Newest first (c, b, a) and only project-*.json entries.
        assert_eq!(
            names,
            vec!["project-c.json", "project-b.json", "project-a.json"]
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn list_recent_projects_empty_on_nonexistent_dir() {
        use crate::ui::header::list_recent_projects_in;
        let dir = std::env::temp_dir().join("impulse_test_recent_does_not_exist");
        let _ = std::fs::remove_dir_all(&dir); // ensure missing
        let found = list_recent_projects_in(&dir);
        assert!(found.is_empty());
    }
}

// ─── format_llm_display tests ───────────────────────────────────────────────
#[cfg(test)]
mod format_display_tests {
    use crate::state::{ConversationMode, format_llm_display};

    #[test]
    fn no_param_update_returns_text() {
        let display = format_llm_display(None, "hello world", &ConversationMode::Producer);
        assert_eq!(display, "hello world");
    }

    #[test]
    fn off_mode_shows_only_keys() {
        let update = serde_json::json!({ "bass": { "cutoff": 0.5 }, "fx": { "reverb_mix": 0.3 } });
        let display = format_llm_display(Some(&update), "", &ConversationMode::Off);
        assert!(display.starts_with("updated "));
        assert!(display.contains("bass"));
        assert!(display.contains("fx"));
        assert!(!display.contains("cutoff")); // only top-level keys
    }

    #[test]
    fn off_mode_excludes_comment() {
        let update = serde_json::json!({
            "bass": { "cutoff": 0.5 },
            "_comment": "I made it acid"
        });
        let display = format_llm_display(Some(&update), "", &ConversationMode::Off);
        assert!(
            !display.contains("acid"),
            "Off mode should not show _comment"
        );
        assert!(display.contains("bass"));
    }

    #[test]
    fn producer_mode_shows_comment_when_present() {
        let update = serde_json::json!({
            "bass": { "cutoff": 0.5 },
            "_comment": "Opened the filter for more brightness"
        });
        let display = format_llm_display(Some(&update), "", &ConversationMode::Producer);
        assert_eq!(display, "Opened the filter for more brightness");
    }

    #[test]
    fn producer_mode_fallback_to_keys_when_no_comment() {
        let update = serde_json::json!({ "fx": { "delay_mix": 0.4 } });
        let display = format_llm_display(Some(&update), "", &ConversationMode::Producer);
        assert!(display.starts_with("updated "));
        assert!(display.contains("fx"));
    }

    #[test]
    fn dj_mode_shows_comment_when_present() {
        let update = serde_json::json!({
            "sequencer": { "bpm": 140 },
            "_comment": "Let's go!"
        });
        let display = format_llm_display(Some(&update), "", &ConversationMode::Dj);
        assert_eq!(display, "Let's go!");
    }

    #[test]
    fn empty_update_object_shows_updated_nothing() {
        let update = serde_json::json!({});
        let display = format_llm_display(Some(&update), "", &ConversationMode::Producer);
        assert_eq!(display, "updated ");
    }
}
