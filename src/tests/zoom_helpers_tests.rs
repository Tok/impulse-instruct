// ─── tests/zoom_helpers_tests.rs ──────────────────────────────────────────────
// Pure helpers behind the Ctrl+scroll zoom dispatcher in ui/util.rs.
// The dispatcher itself reads `egui::Context` (impure shell), but the
// step/clamp math factors out cleanly and locks down the per-axis
// damping curve + zoom range bounds.

#[cfg(test)]
mod step_from_delta {
    use crate::ui::util::zoom_step_from_delta;

    #[test]
    fn zero_delta_zero_step() {
        assert!((zoom_step_from_delta(0.0)).abs() < 1e-9);
    }

    #[test]
    fn small_delta_scales_linearly_below_saturation() {
        // Below saturation (|delta| < 60) the response is linear:
        // delta/120 ∈ (-0.5, 0.5) is below the clamp, then × 0.1.
        // So delta=30 → 0.25 × 0.1 = 0.025, delta=12 → 0.01.
        assert!((zoom_step_from_delta(30.0) - 0.025).abs() < 1e-6);
        assert!((zoom_step_from_delta(12.0) - 0.01).abs() < 1e-6);
    }

    #[test]
    fn saturates_at_one_half_notch_in_either_direction() {
        // The clamp is at ±0.5 of (delta/120), so the helper
        // saturates the per-tick step magnitude at 0.05 — any raw
        // delta with |delta| ≥ 60 produces the same step.  Locks
        // the documented "can't lurch by half a unit in one frame"
        // contract; a future loosening would trip this test.
        for d in [60.0, 120.0, 1_000.0, 99_999.0] {
            let s = zoom_step_from_delta(d);
            assert!((s - 0.05).abs() < 1e-6, "delta={d} got {s}");
        }
        for d in [-60.0, -120.0, -1_000.0, -99_999.0] {
            let s = zoom_step_from_delta(d);
            assert!((s - -0.05).abs() < 1e-6, "delta={d} got {s}");
        }
    }
}

#[cfg(test)]
mod scale_clamps {
    use crate::ui::util::{next_global_scale, next_module_scale};

    #[test]
    fn module_scale_clamps_to_unit_band() {
        // Per-module zoom: 0.5..=2.0.
        assert!((next_module_scale(1.0, 0.1) - 1.1).abs() < 1e-6);
        assert!((next_module_scale(0.4, -0.5) - 0.5).abs() < 1e-6);
        assert!((next_module_scale(2.5, 0.5) - 2.0).abs() < 1e-6);
    }

    #[test]
    fn global_scale_allows_wider_band() {
        // Global zoom: 0.5..=3.0 (ROOM for chrome/text scaling).
        assert!((next_global_scale(1.0, 0.5) - 1.5).abs() < 1e-6);
        assert!((next_global_scale(2.5, 0.5) - 3.0).abs() < 1e-6);
        assert!((next_global_scale(0.4, -0.5) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn module_max_is_lower_than_global_max() {
        // Documented contract: per-module zoom is a finer band so it
        // can stack on top of the global scale at render time.
        // A future change that broadens module range past 2.0× would
        // trip this test.
        let m = next_module_scale(99.0, 99.0);
        let g = next_global_scale(99.0, 99.0);
        assert!(m < g, "module max {m} should be < global max {g}");
        assert!((m - 2.0).abs() < 1e-6);
        assert!((g - 3.0).abs() < 1e-6);
    }

    #[test]
    fn round_trip_at_unit_is_idempotent() {
        // Ctrl+0 (or no scroll) lands on 1.0 — repeated 0-step calls
        // must converge.
        let mut s = 1.0_f32;
        for _ in 0..10 {
            s = next_module_scale(s, 0.0);
        }
        assert!((s - 1.0).abs() < 1e-6);
    }
}
