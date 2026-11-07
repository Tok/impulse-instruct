// ─── tests/shortcut_overlay_tests.rs ──────────────────────────────────────────
// Sanity checks on the canonical SHORTCUT_GROUPS map — descriptions
// non-empty, no accidental duplicates, every group named, etc.  Cheap
// guard against entries silently rotting.

#[cfg(test)]
mod groups {
    use crate::ui::scope_footer::SHORTCUT_GROUPS;

    #[test]
    fn at_least_one_group_per_overlay() {
        assert!(!SHORTCUT_GROUPS.is_empty());
    }

    #[test]
    fn every_group_has_non_empty_label_and_rows() {
        for (label, rows) in SHORTCUT_GROUPS {
            assert!(!label.is_empty(), "group with empty label");
            assert!(!rows.is_empty(), "group {label:?} has no rows");
        }
    }

    #[test]
    fn every_row_has_non_empty_key_and_desc() {
        for (group, rows) in SHORTCUT_GROUPS {
            for (k, d) in *rows {
                assert!(!k.is_empty(), "empty key in group {group:?}");
                assert!(!d.is_empty(), "empty desc for key {k:?} in {group:?}");
            }
        }
    }

    #[test]
    fn no_duplicate_keys_across_groups() {
        // Bug guard: a stray copy/paste could register the same combo
        // under two groups.  Trip immediately on duplicates so the
        // overlay matches the dispatch (keys map to one action).
        let mut seen = std::collections::HashSet::new();
        for (_group, rows) in SHORTCUT_GROUPS {
            for (k, _) in *rows {
                assert!(seen.insert(*k), "duplicate shortcut entry: {k:?}");
            }
        }
    }

    #[test]
    fn snapshot_slots_are_listed_in_transport() {
        // Pattern snapshot keys (Shift+1..=4) must surface in the
        // overlay so live performers know about them.
        let transport = SHORTCUT_GROUPS
            .iter()
            .find(|(g, _)| *g == "Transport")
            .expect("Transport group missing");
        let keys: Vec<&str> = transport.1.iter().map(|(k, _)| *k).collect();
        for expected in ["Shift+1", "Shift+2", "Shift+3", "Shift+4"] {
            assert!(
                keys.contains(&expected),
                "expected {expected} in Transport group, got {keys:?}"
            );
        }
    }

    #[test]
    fn performance_mode_listed_under_view() {
        let view = SHORTCUT_GROUPS
            .iter()
            .find(|(g, _)| *g == "View")
            .expect("View group missing");
        assert!(
            view.1.iter().any(|(k, _)| *k == "F2"),
            "F2 (performance mode) missing from View group"
        );
    }
}
