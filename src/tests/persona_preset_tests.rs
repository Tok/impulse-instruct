// ─── tests/persona_preset_tests.rs ────────────────────────────────────────────
// PersonaPreset library — slug normalisation, agent ↔ preset round-
// trip, apply_to non-destructiveness against scope / patterns / model.
// File-system save/load is covered by a temp-dir round-trip.

#[cfg(test)]
mod slug {
    use crate::state::slugify;

    #[test]
    fn basic_words_lowercase() {
        assert_eq!(slugify("BASS"), "bass");
        assert_eq!(slugify("Bass"), "bass");
    }

    #[test]
    fn whitespace_becomes_single_underscore() {
        assert_eq!(slugify("Bass MC"), "bass_mc");
        assert_eq!(slugify("Bass    MC"), "bass_mc");
    }

    #[test]
    fn punctuation_collapses_to_underscore() {
        assert_eq!(slugify("Acid!Bass-303"), "acid_bass_303");
    }

    #[test]
    fn empty_or_pure_punctuation_falls_back_to_persona() {
        assert_eq!(slugify(""), "persona");
        assert_eq!(slugify("!!!"), "persona");
    }

    #[test]
    fn trailing_punctuation_is_stripped() {
        assert_eq!(slugify("Bass!!"), "bass");
        assert_eq!(slugify("MC...."), "mc");
    }

    #[test]
    fn unicode_is_dropped_as_non_alphanumeric() {
        // Non-ASCII alphanumerics map to underscores under the current
        // helper.  Locks the conservative behaviour so a future "smart
        // unicode slugger" change is a deliberate one.
        assert_eq!(slugify("Café"), "caf");
        assert_eq!(slugify("Bjørn"), "bj_rn");
    }
}

#[cfg(test)]
mod round_trip {
    use crate::state::{AgentRole, AppState, ConversationMode, LlmAgentState, PersonaPreset};

    fn agent_with_personality() -> LlmAgentState {
        let mut a = LlmAgentState::new_default(7);
        a.persona_name = "BASS".to_string();
        a.role = AgentRole::Producer;
        a.conversation_mode = ConversationMode::Mc;
        a.user_instructions = "lean on the resonance".to_string();
        a.system_prompt_override = "you are a 303 surgeon".to_string();
        a.temperature = 1.4;
        a.enable_thinking = true;
        a.scope = vec!["bass".to_string()];
        a.model_path = Some("models/some.gguf".to_string());
        a
    }

    #[test]
    fn from_agent_then_apply_to_preserves_personality_fields() {
        let a = agent_with_personality();
        let preset = PersonaPreset::from_agent(&a);
        let mut b = LlmAgentState::new_default(99);
        preset.apply_to(&mut b);
        assert_eq!(b.persona_name, "BASS");
        assert_eq!(b.role, AgentRole::Producer);
        assert_eq!(b.conversation_mode, ConversationMode::Mc);
        assert_eq!(b.user_instructions, "lean on the resonance");
        assert_eq!(b.system_prompt_override, "you are a 303 surgeon");
        assert!((b.temperature - 1.4).abs() < 1e-6);
        assert!(b.enable_thinking);
    }

    #[test]
    fn apply_to_preserves_scope_and_model_path() {
        // Loading a preset must NOT clobber session-context fields —
        // scope (which voice the agent writes), model_path (which
        // server it routes to), or the agent's id.
        let preset = PersonaPreset::from_agent(&agent_with_personality());
        let mut target = LlmAgentState::new_default(123);
        target.scope = vec!["fx".to_string()];
        target.model_path = Some("models/other.gguf".to_string());
        preset.apply_to(&mut target);
        assert_eq!(target.id, 123);
        assert_eq!(target.scope, vec!["fx".to_string()]);
        assert_eq!(target.model_path.as_deref(), Some("models/other.gguf"));
    }

    #[test]
    fn temperature_clamps_to_0_2_range_on_apply() {
        let mut preset = PersonaPreset::default();
        preset.temperature = 5.0;
        let mut a = LlmAgentState::new_default(1);
        preset.apply_to(&mut a);
        assert!(a.temperature <= 2.0);
        let mut preset_neg = PersonaPreset::default();
        preset_neg.temperature = -1.0;
        let mut a = LlmAgentState::new_default(1);
        preset_neg.apply_to(&mut a);
        assert!(a.temperature >= 0.0);
    }

    #[test]
    fn json_round_trip_preserves_all_fields() {
        let p = PersonaPreset::from_agent(&agent_with_personality());
        let s = serde_json::to_string(&p).unwrap();
        let p2: PersonaPreset = serde_json::from_str(&s).unwrap();
        assert_eq!(p, p2);
    }

    #[test]
    fn missing_fields_in_old_preset_use_defaults() {
        // A bare-bones JSON file (only `name`) must deserialise via
        // serde defaults — guards the "ship a few curated presets in
        // an older format" path against future field additions.
        let p: PersonaPreset = serde_json::from_str(r#"{ "name": "MINIMAL" }"#).unwrap();
        assert_eq!(p.name, "MINIMAL");
        assert!((p.temperature - 0.9).abs() < 1e-6);
        assert_eq!(p.role, AgentRole::default());
        assert_eq!(p.user_instructions, "");
    }

    #[test]
    fn apply_round_trip_through_agent_state_is_idempotent() {
        let a = agent_with_personality();
        let preset = PersonaPreset::from_agent(&a);
        // Build a fresh agent, apply, re-snapshot, re-apply — should
        // converge.
        let mut b = LlmAgentState::new_default(1);
        preset.apply_to(&mut b);
        let preset2 = PersonaPreset::from_agent(&b);
        assert_eq!(preset, preset2);
    }
}

#[cfg(test)]
mod fs_io {
    use crate::state::{PersonaPreset, list_presets_in, load_preset_from_path, save_preset_to_dir};

    /// Build a unique temp directory per-test (process id + nanoseconds)
    /// so parallel tests don't collide.  The explicit `_to_dir` /
    /// `list_presets_in` variants of the helpers let us drive the FS
    /// path without mutating the process-wide HOME env var.
    fn temp_dir_for_test(tag: &str) -> std::path::PathBuf {
        let pid = std::process::id();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!("impulse_persona_{tag}_{pid}_{nanos}"))
    }

    #[test]
    fn save_to_dir_creates_the_directory_if_missing() {
        let dir = temp_dir_for_test("mkdir");
        assert!(!dir.exists(), "precondition: dir must not exist");
        let p = PersonaPreset {
            name: "BASS".into(),
            ..PersonaPreset::default()
        };
        let path = save_preset_to_dir(&dir, &p).expect("save");
        assert!(path.exists());
        assert!(dir.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_then_load_round_trip_preserves_fields() {
        let dir = temp_dir_for_test("rt");
        let mut p = PersonaPreset::default();
        p.name = "BASS MC".to_string();
        p.user_instructions = "lean on the bass".to_string();
        p.temperature = 1.2;
        let path = save_preset_to_dir(&dir, &p).expect("save");
        // file stem comes from slugify("BASS MC") = "bass_mc".
        assert_eq!(path.file_stem().unwrap().to_string_lossy(), "bass_mc");
        let loaded = load_preset_from_path(&path).expect("load");
        assert_eq!(loaded, p);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn list_presets_in_enumerates_only_json_files() {
        let dir = temp_dir_for_test("list");
        std::fs::create_dir_all(&dir).unwrap();
        // Two valid presets…
        let _ = save_preset_to_dir(
            &dir,
            &PersonaPreset {
                name: "ALPHA".into(),
                ..PersonaPreset::default()
            },
        )
        .unwrap();
        let _ = save_preset_to_dir(
            &dir,
            &PersonaPreset {
                name: "BRAVO".into(),
                ..PersonaPreset::default()
            },
        )
        .unwrap();
        // …plus a stray non-JSON file the lister must ignore.
        std::fs::write(dir.join("README.txt"), "not a preset").unwrap();
        let listing = list_presets_in(&dir);
        assert_eq!(listing.len(), 2, "non-JSON files must be ignored");
        let stems: Vec<String> = listing
            .iter()
            .map(|p| p.file_stem().unwrap().to_string_lossy().into_owned())
            .collect();
        // Sorted alphabetically by filename.
        assert_eq!(stems, vec!["alpha".to_string(), "bravo".to_string()]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn list_presets_in_missing_dir_returns_empty() {
        let dir = temp_dir_for_test("missing");
        // Directory never created — should return empty Vec, not panic.
        let listing = list_presets_in(&dir);
        assert!(listing.is_empty());
    }
}
