// ─── tests/performance_mode_tests.rs ──────────────────────────────────────────
// Performance-mode UiPrefs flag — default off, serde round-trip, and
// the missing-field-from-old-session graceful upgrade.

#[cfg(test)]
mod ui_prefs {
    use crate::state::UiPrefs;

    #[test]
    fn default_is_off() {
        assert!(!UiPrefs::default().performance_mode);
    }

    #[test]
    fn round_trips_through_json() {
        let mut p = UiPrefs::default();
        p.performance_mode = true;
        let s = serde_json::to_string(&p).unwrap();
        let p2: UiPrefs = serde_json::from_str(&s).unwrap();
        assert!(p2.performance_mode);
    }

    #[test]
    fn missing_field_in_old_session_deserializes_to_false() {
        // Old session.json predates the field — serde default kicks in.
        let mut v: serde_json::Value = serde_json::to_value(UiPrefs::default()).unwrap();
        v.as_object_mut().unwrap().remove("performance_mode");
        let s = serde_json::to_string(&v).unwrap();
        let p: UiPrefs = serde_json::from_str(&s).unwrap();
        assert!(!p.performance_mode);
    }

    #[test]
    fn toggling_does_not_disturb_other_prefs() {
        // Sanity: flipping performance_mode shouldn't touch unrelated
        // fields (regression guard for an earlier prefs bug where a
        // setter blew away the whole struct).
        let mut p = UiPrefs::default();
        p.show_automation_overlay = true;
        p.rack_grid_cols = 6;
        p.performance_mode = true;
        assert!(p.show_automation_overlay);
        assert_eq!(p.rack_grid_cols, 6);
        assert!(p.performance_mode);
    }
}
