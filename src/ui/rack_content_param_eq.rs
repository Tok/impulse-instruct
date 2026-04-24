// ─── ui/rack_content_param_eq.rs ─────────────────────────────────────────────
// ParamEq module card — curve editor + per-band readout strip.
// Extracted from rack_content.rs to keep that file under the 1000-line
// cap; the other FX modules are simpler two-tuple / three-knob cards
// and can stay inline there.

use crate::ui::{ImpulseApp, theme, widgets};

/// Render the ParamEq card.  Returns `true` when the user edited any
/// band so the caller can push audio params + mark the session dirty.
pub(super) fn draw_param_eq(app: &mut ImpulseApp, ui: &mut egui::Ui, module_id: u32) -> bool {
    let mut local = app.state.read().fx.param_eq_bands;
    let avail = ui.available_size();
    let readout_h = 18.0;
    let curve_h = (avail.y - readout_h - 4.0).max(60.0);
    let curve_size = egui::Vec2::new(avail.x, curve_h);
    let any_changed = widgets::param_eq_curve(
        ui,
        &format!("param_eq_{module_id}"),
        &mut local,
        crate::audio::SAMPLE_RATE,
        curve_size,
    );
    ui.add_space(2.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 3.0;
        for (i, b) in local.iter().enumerate() {
            let kind_short = match b.kind {
                crate::state::ParamEqBandKind::LowShelf => "LS",
                crate::state::ParamEqBandKind::Peak => "PK",
                crate::state::ParamEqBandKind::HighShelf => "HS",
            };
            let freq_s = if b.freq_hz >= 1_000.0 {
                format!("{:.1}k", b.freq_hz / 1_000.0)
            } else {
                format!("{:.0}", b.freq_hz)
            };
            let label = format!("{}{} {} {:+.1}", i + 1, kind_short, freq_s, b.gain_db);
            let color = if b.enabled { theme::ASH } else { theme::PIT };
            ui.label(
                egui::RichText::new(label)
                    .monospace()
                    .size(8.0)
                    .color(color),
            );
        }
    });
    if any_changed {
        app.state.write().fx.param_eq_bands = local;
    }
    any_changed
}
