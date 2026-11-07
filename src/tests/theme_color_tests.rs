//! `theme::rgba_unmultiplied` / `theme::white_alpha` must reproduce egui
//! ≤ 0.28's linear-light premultiply — every translucent colour in the UI
//! (LED halos, glass, overlays) was tuned against that math, and egui 0.37
//! switched `Color32::from_rgba_unmultiplied` to gamma-space premultiply.

#[cfg(test)]
mod linear_premultiply {
    use crate::ui::theme::{rgba_unmultiplied, white_alpha};
    use egui::Color32;

    #[test]
    fn opaque_and_transparent_short_circuit() {
        assert_eq!(
            rgba_unmultiplied(10, 20, 30, 255),
            Color32::from_rgb(10, 20, 30)
        );
        assert_eq!(rgba_unmultiplied(10, 20, 30, 0), Color32::TRANSPARENT);
    }

    #[test]
    fn low_alpha_white_matches_egui_028() {
        // egui 0.28: gamma(linear(255) * 25/255) = gamma(0.098) = 88.
        // egui 0.37's gamma-space premultiply would give 25 instead.
        let c = white_alpha(25);
        assert_eq!(c.to_array(), [88, 88, 88, 25]);
        assert_eq!(rgba_unmultiplied(255, 255, 255, 25), c);
    }

    #[test]
    fn half_alpha_white_is_brighter_than_gamma_premultiply() {
        // gamma(0.502) = 188 (vs 128 under gamma-space premultiply).
        assert_eq!(white_alpha(128).to_array(), [188, 188, 188, 128]);
    }

    #[test]
    fn black_is_unaffected_by_colour_space() {
        assert_eq!(rgba_unmultiplied(0, 0, 0, 100).to_array(), [0, 0, 0, 100]);
    }

    #[test]
    fn premultiplied_channels_never_exceed_alpha_scaled_input() {
        // Premultiplied rgb must stay ≤ the opaque channel value.
        for a in [1u8, 7, 64, 200, 254] {
            let [r, g, b, alpha] = rgba_unmultiplied(200, 120, 40, a).to_array();
            assert_eq!(alpha, a);
            assert!(r <= 200 && g <= 120 && b <= 40, "a={a}: {r},{g},{b}");
        }
    }
}
