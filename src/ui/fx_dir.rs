// ─── ui/fx_dir.rs ──────────────────────────────────────────────────────────
// Time-direction toggle for FX with a tail/echo (Reverb, Delay).
// Three states: 0 = FWD (normal), 1 = REV (preverb / anti-echo via a 1 s
// reversed input buffer), 2 = MIRROR (sum of forward + reverse).

use crate::ui::theme;

/// Render a 3-state cycle button.  Cycles 0 → 1 → 2 → 0 on click.
/// Returns true if the user clicked (so the caller can flush state).
pub fn draw_fx_dir_button(ui: &mut egui::Ui, dir: &mut u8, hover_prefix: &str) -> bool {
    let label = match *dir {
        1 => "REVERSE",
        2 => "MIRROR",
        _ => "FORWARD",
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
            "{} — FWD / REV (preverb) / MIRROR.  REV+MIRROR feed a reversed input buffer (loop length set by the quant button).",
            hover_prefix
        ));
    if resp.clicked() {
        *dir = (*dir + 1) % 3;
        return true;
    }
    false
}

/// Render a quant cycle button next to the FWD/REV/MIRROR control.
/// Cycles 0..=4: 1s (free) → 1/4 bar → 1/2 → 1 bar → 2 bars → 1s.
/// Returns true if the user clicked.
pub fn draw_fx_rev_quant_button(ui: &mut egui::Ui, quant: &mut u8, hover_prefix: &str) -> bool {
    let label = match *quant {
        1 => "1/4",
        2 => "1/2",
        3 => "1",
        4 => "2",
        _ => "1s",
    };
    let active = *quant != 0;
    let col = if active { theme::CHALK } else { theme::SMOKE };
    let fill = egui::Color32::from_gray(if active { 50 } else { 22 });
    let resp = ui
        .add(
            egui::Button::new(egui::RichText::new(label).monospace().size(8.0).color(col))
                .fill(fill)
                .min_size(egui::vec2(28.0, 18.0)),
        )
        .on_hover_text(format!(
            "{} rewind cycle — 1s (free) / 1/4 bar / 1/2 bar / 1 bar / 2 bars.  Snaps the REV+MIRROR loop length to the active BPM.",
            hover_prefix
        ));
    if resp.clicked() {
        *quant = (*quant + 1) % 5;
        return true;
    }
    false
}
