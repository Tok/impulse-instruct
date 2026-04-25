// ─── tests/midi_learn_tests.rs ───────────────────────────────────────────────
// MIDI learn — UiPrefs.midi_cc_bindings persistence + apply path tests.
//
// The drain_midi_events handler itself wraps `ImpulseApp` (lots of egui /
// audio / channel setup), so these tests exercise the pure pieces:
//   • UiPrefs serde round-trips bindings
//   • A user binding's dot-path resolves through `dot_path_to_json` +
//     `apply_llm_update` to update the right field
//   • Removing a binding leaves UiPrefs in a sane state

#[cfg(test)]
mod ui_prefs_persistence {
    use crate::state::UiPrefs;

    #[test]
    fn default_bindings_are_empty() {
        let p = UiPrefs::default();
        assert!(p.midi_cc_bindings.is_empty());
    }

    #[test]
    fn bindings_round_trip_through_json() {
        let mut p = UiPrefs::default();
        p.midi_cc_bindings.insert(20, "bass.cutoff".to_string());
        p.midi_cc_bindings.insert(45, "fx.reverb_mix".to_string());
        let s = serde_json::to_string(&p).unwrap();
        let p2: UiPrefs = serde_json::from_str(&s).unwrap();
        assert_eq!(p2.midi_cc_bindings.len(), 2);
        assert_eq!(
            p2.midi_cc_bindings.get(&20).cloned(),
            Some("bass.cutoff".to_string())
        );
        assert_eq!(
            p2.midi_cc_bindings.get(&45).cloned(),
            Some("fx.reverb_mix".to_string())
        );
    }

    #[test]
    fn missing_field_in_old_session_deserializes_to_empty_map() {
        // Old session.json predates the field — serde default kicks in.
        let mut v: serde_json::Value = serde_json::to_value(UiPrefs::default()).unwrap();
        let obj = v.as_object_mut().unwrap();
        obj.remove("midi_cc_bindings");
        let s = serde_json::to_string(&v).unwrap();
        let p: UiPrefs = serde_json::from_str(&s).unwrap();
        assert!(p.midi_cc_bindings.is_empty());
    }

    #[test]
    fn removing_a_binding_leaves_others_intact() {
        let mut p = UiPrefs::default();
        p.midi_cc_bindings.insert(10, "bass.cutoff".to_string());
        p.midi_cc_bindings.insert(11, "bass.resonance".to_string());
        p.midi_cc_bindings.remove(&10);
        assert_eq!(p.midi_cc_bindings.len(), 1);
        assert!(p.midi_cc_bindings.contains_key(&11));
    }
}

#[cfg(test)]
mod apply_path {
    use crate::state::{AppState, apply_llm_update};

    /// Local mirror of `crate::ui::dot_path_to_json` (private to ui).
    /// Building the JSON manually keeps these tests independent of the
    /// UI module's visibility decisions.
    fn dot_path_to_json(path: &str, value: f32) -> serde_json::Value {
        let leaf = serde_json::json!(value);
        path.split('.')
            .rev()
            .fold(leaf, |acc, key| serde_json::json!({ key: acc }))
    }

    #[test]
    fn cc_value_normalizes_to_unit_interval_via_dot_path() {
        // CC value 64 (mid) → 64/127 ≈ 0.504, applied to bass.cutoff
        // routes via the legacy "bass" key into bass_voices[0].synth.cutoff.
        let scaled = 64.0_f32 / 127.0;
        let update = dot_path_to_json("bass.cutoff", scaled);
        let next = apply_llm_update(AppState::default(), &update, &[]);
        assert!(
            (next.bass_voices[0].synth.cutoff - scaled).abs() < 1e-3,
            "cutoff expected {}, got {}",
            scaled,
            next.bass_voices[0].synth.cutoff
        );
    }

    #[test]
    fn user_binding_path_can_target_any_top_level_param() {
        let scaled = 100.0_f32 / 127.0;
        let update = dot_path_to_json("fx.reverb_mix", scaled);
        let next = apply_llm_update(AppState::default(), &update, &[]);
        assert!(
            (next.fx.reverb_mix - scaled).abs() < 1e-3,
            "reverb_mix expected {}, got {}",
            scaled,
            next.fx.reverb_mix
        );
    }

    #[test]
    fn bogus_dot_path_does_not_panic_or_clobber_state() {
        let s0 = AppState::default();
        let update = dot_path_to_json("bass.no_such_field", 0.5);
        let s1 = apply_llm_update(s0.clone(), &update, &[]);
        assert!((s1.bass_voices[0].synth.cutoff - s0.bass_voices[0].synth.cutoff).abs() < 1e-6);
        assert!((s1.sequencer.bpm - s0.sequencer.bpm).abs() < 1e-6);
    }
}

#[cfg(test)]
mod precedence {
    use crate::state::UiPrefs;

    #[test]
    fn user_binding_overrides_static_table_for_same_cc() {
        // The static table maps CC74 → bass.cutoff.  A user binding
        // for CC74 → fx.reverb_mix should win — the handler short-
        // circuits before the static lookup.  We assert the storage
        // and the documented contract: user binding lookup returns
        // the user path even when the static path is set for the same
        // CC.  (The actual "wins over static" wiring lives in
        // ui::midi_handler; testing that path needs an ImpulseApp.)
        let mut p = UiPrefs::default();
        p.midi_cc_bindings.insert(74, "fx.reverb_mix".to_string());
        let user_path = p.midi_cc_bindings.get(&74).cloned();
        let static_path = crate::midi::cc_to_param_path(74).map(|(p, _)| p.to_string());
        assert_eq!(user_path.as_deref(), Some("fx.reverb_mix"));
        assert_eq!(static_path.as_deref(), Some("bass.cutoff"));
        // The handler precedence is documented in midi_handler.rs:
        // when user binding exists, the static path is not consulted.
    }
}
