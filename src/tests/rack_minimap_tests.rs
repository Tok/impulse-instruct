// ─── tests/rack_minimap_tests.rs ──────────────────────────────────────────────
// Pure helpers behind the rack mini-map: content/screen coordinate
// conversion + click-to-content-y inverse mapping.  The painter +
// pointer-driven click-to-pan path needs an egui context so it stays
// out of the unit tests; these lock the math.

#[cfg(test)]
mod coord_math {
    use crate::ui::rack_minimap::{card_to_content_space, content_y_to_map_y, map_y_to_content_y};
    use egui::{Pos2, Rect};

    #[test]
    fn card_in_visible_viewport_maps_to_offset_relative_y() {
        // A card visible at screen y=200, scroll viewport top=100,
        // scroll offset=50 → card's content-space top is at 200-100+50=150.
        let card = Rect::from_min_max(Pos2::new(20.0, 200.0), Pos2::new(60.0, 240.0));
        let cs = card_to_content_space(card, Pos2::new(0.0, 100.0), 50.0);
        assert!((cs.min.y - 150.0).abs() < 1e-3, "got {}", cs.min.y);
        assert!((cs.max.y - 190.0).abs() < 1e-3, "got {}", cs.max.y);
    }

    #[test]
    fn card_x_unaffected_by_scroll() {
        let card = Rect::from_min_max(Pos2::new(20.0, 100.0), Pos2::new(60.0, 140.0));
        let cs = card_to_content_space(card, Pos2::new(0.0, 100.0), 500.0);
        assert!((cs.min.x - 20.0).abs() < 1e-3);
        assert!((cs.max.x - 60.0).abs() < 1e-3);
    }

    #[test]
    fn content_y_maps_proportionally() {
        // map y=0..100, content_h=1000.  y=500 → halfway.
        let y = content_y_to_map_y(500.0, 1000.0, 0.0, 100.0);
        assert!((y - 50.0).abs() < 1e-3);
    }

    #[test]
    fn content_y_clamps_to_map_extent() {
        // out-of-range content y must clamp into the map rect.
        let above = content_y_to_map_y(-200.0, 1000.0, 10.0, 100.0);
        let below = content_y_to_map_y(99999.0, 1000.0, 10.0, 100.0);
        assert!((above - 10.0).abs() < 1e-3, "above: {above}");
        assert!((below - 110.0).abs() < 1e-3, "below: {below}");
    }

    #[test]
    fn empty_content_collapses_to_map_top() {
        // Tiny / zero content height shouldn't divide by zero.  Both
        // helpers must short-circuit to a sane value.
        let y = content_y_to_map_y(123.0, 0.5, 10.0, 100.0);
        assert!((y - 10.0).abs() < 1e-3);
    }

    #[test]
    fn map_y_to_content_y_is_inverse_for_round_trip() {
        // round-trip: content → map → content.
        let cy = 500.0;
        let map_top = 30.0;
        let map_h = 100.0;
        let content_h = 1000.0;
        let my = content_y_to_map_y(cy, content_h, map_top, map_h);
        let cy2 = map_y_to_content_y(my, content_h, map_top, map_h);
        assert!((cy - cy2).abs() < 1e-3, "round-trip drift: {cy} → {cy2}");
    }

    #[test]
    fn map_y_to_content_y_clamps_to_unit_interval() {
        // pointer y outside the map clamps to [0, content_h].
        let above = map_y_to_content_y(-10.0, 1000.0, 100.0, 80.0);
        let below = map_y_to_content_y(99999.0, 1000.0, 100.0, 80.0);
        assert!((above - 0.0).abs() < 1e-3);
        assert!((below - 1000.0).abs() < 1e-3);
    }
}
