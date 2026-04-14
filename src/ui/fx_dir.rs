// ─── ui/fx_dir.rs ──────────────────────────────────────────────────────────
// Time-direction toggle for FX with a tail/echo (Reverb, Delay).
// Three states: 0 = FWD (normal), 1 = REV (preverb / anti-echo via a 1 s
// reversed input buffer), 2 = MIRROR (sum of forward + reverse).

use crate::ui::theme;

/// Render a 3-state cycle button.  Cycles 0 → 1 → 2 → 0 on click.
/// Returns true if the user clicked (so the caller can flush state).
pub fn draw_fx_dir_button(ui: &mut egui::Ui, dir: &mut u8, hover_prefix: &str) -> bool {
    let label = match *dir {
        1 => "REV",
        2 => "MIR",
        _ => "FWD",
    };
    let active = *dir != 0;
    let col = if active { theme::CHALK } else { theme::SMOKE };
    let fill = egui::Color32::from_gray(if active { 50 } else { 22 });
    let resp = ui
        .add(
            egui::Button::new(egui::RichText::new(label).monospace().size(8.0).color(col))
                .fill(fill)
                .min_size(egui::vec2(34.0, 18.0)),
        )
        .on_hover_text(format!(
            "{} — FWD / REV (preverb) / MIRROR.  REV+MIRROR feed a 1 s reversed input buffer.",
            hover_prefix
        ));
    if resp.clicked() {
        *dir = (*dir + 1) % 3;
        return true;
    }
    false
}
