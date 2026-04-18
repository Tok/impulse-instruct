// ─── ui/rack_content_pad.rs ──────────────────────────────────────────────────
// XY-pad rendering helpers shared between the FX cards (in rack_content.rs)
// and the drum kit panels (ui/panels/drums.rs).  Split from rack_content.rs
// to keep that file under the 1000-line cap.
//
// Two pad shapes:
//   • 2-knob — direct pad, one pair (X × Y).
//   • 3-knob — switchable pad, three pairs (A/B, A/C, B/C) with a chip-style
//     cycle button.  Pair is stored in either `RackModule.pad_pair` (FX) or
//     egui temp memory (drums).
//
// `_bare` variants skip the glass frame so callers that already sit inside
// a glass group (drum voices) don't nest frames.

/// Vertical gap between the FX knob rows and the XY pad section.
/// Provides visual separation between the two control groups.
pub(crate) const PAD_SECTION_TOP_GAP: f32 = 4.0;
/// Width reserved next to a 3-pair pad for the side-mounted cycle button.
pub(crate) const PAD_CYCLE_SIDE_W: f32 = 44.0;

/// Given a 3-pair XY-pad pair index (0/1/2) and the three knob labels in
/// canonical A / B / C order, return the (x, y) label pair the pad drives
/// for that index.  Pair mapping is A/B → A/C → B/C, matching the tooltip
/// order the widget shows in its corner indicator.
pub(crate) fn three_pair_labels(
    pair: usize,
    a: &'static str,
    b: &'static str,
    c: &'static str,
) -> (&'static str, &'static str) {
    match pair {
        0 => (a, b),
        1 => (a, c),
        _ => (b, c),
    }
}

/// Render a small chip-style cycle button next to a multi-pair XY pad.
/// Shows the current "X × Y" pair with a ↻ affordance; left-click advances
/// the pair index (stored in egui temp memory keyed by `pad_id`).  The
/// pad's own right-click still cycles too.
pub(crate) fn draw_pad_cycle_button(
    ui: &mut egui::Ui,
    pad_id: &str,
    num_pairs: usize,
    label_x: &str,
    label_y: &str,
) {
    use egui::{Color32, RichText};
    let text = format!("{label_x} × {label_y}  ↻");
    let btn = egui::Button::new(
        RichText::new(text)
            .monospace()
            .size(8.5)
            .color(Color32::from_gray(180)),
    )
    .fill(Color32::from_gray(22))
    .stroke(egui::Stroke::new(0.5, Color32::from_gray(55)))
    .rounding(egui::Rounding::same(3.0));
    let resp = ui.add(btn).on_hover_text("Cycle pad pair");
    if resp.clicked() {
        let pair = crate::ui::widgets::xy_pad_pair(ui.ctx(), pad_id);
        let next = (pair + 1) % num_pairs.max(1);
        ui.ctx()
            .data_mut(|d| d.insert_temp(egui::Id::new(pad_id), next));
    }
}

/// 2-knob XY pad — single pair, fills the card's extra row.  Returns
/// `true` if either axis was dragged this frame.
pub(crate) fn render_two_pad(
    ui: &mut egui::Ui,
    pad_id: &str,
    label_x: &'static str,
    label_y: &'static str,
    x: &mut f32,
    y: &mut f32,
    x_locked: bool,
    y_locked: bool,
) -> bool {
    let pad_size = (ui.available_width() - 16.0).clamp(60.0, 120.0);
    let pad_locked = x_locked || y_locked;
    let mut changed = false;
    crate::ui::widgets::centered_row(ui, |ui| {
        let (pad_changed, _) =
            crate::ui::widgets::xy_pad(ui, pad_id, label_x, label_y, x, y, pad_size, pad_locked, 1);
        if pad_changed {
            changed = true;
        }
    });
    changed
}

/// 3-knob switchable XY pad **without** the glass-pane wrapper — for
/// callers (drum kits) that already sit inside their own glass group and
/// shouldn't nest a second frame.  Pair state lives in egui temp memory
/// only (transient, not persisted).  `pad_size` is caller-controlled so
/// drum cards can match the prefs-tuned drum pad size instead of the
/// FX helper's narrow clamp.  Returns `true` if the pad was dragged.
#[allow(clippy::too_many_arguments)]
pub(crate) fn render_three_pad_bare(
    ui: &mut egui::Ui,
    pad_id: &str,
    labels: [&'static str; 3],
    fields: (&mut f32, &mut f32, &mut f32),
    locks: [bool; 3],
    pad_size: f32,
) -> bool {
    use crate::ui::widgets;
    let cur_pair = widgets::xy_pad_pair(ui.ctx(), pad_id) % 3;
    let (lx, ly) = three_pair_labels(cur_pair, labels[0], labels[1], labels[2]);
    let pad_locked = match cur_pair {
        0 => locks[0] || locks[1],
        1 => locks[0] || locks[2],
        _ => locks[1] || locks[2],
    };
    let (a, b, c) = fields;
    let mut val_changed = false;
    ui.horizontal(|ui| {
        let (pad_changed, _) = match cur_pair {
            0 => widgets::xy_pad(ui, pad_id, lx, ly, a, b, pad_size, pad_locked, 3),
            1 => widgets::xy_pad(ui, pad_id, lx, ly, a, c, pad_size, pad_locked, 3),
            _ => widgets::xy_pad(ui, pad_id, lx, ly, b, c, pad_size, pad_locked, 3),
        };
        if pad_changed {
            val_changed = true;
        }
        ui.add_space(4.0);
        ui.vertical(|ui| {
            draw_pad_cycle_button(ui, pad_id, 3, lx, ly);
        });
    });
    val_changed
}

/// 2-knob XY pad without the centred-row / glass wrapping.  Caller picks
/// the pad size.  Same contract as `render_two_pad` otherwise.
#[allow(clippy::too_many_arguments)]
pub(crate) fn render_two_pad_bare(
    ui: &mut egui::Ui,
    pad_id: &str,
    label_x: &'static str,
    label_y: &'static str,
    x: &mut f32,
    y: &mut f32,
    x_locked: bool,
    y_locked: bool,
    pad_size: f32,
) -> bool {
    let pad_locked = x_locked || y_locked;
    let (pad_changed, _) =
        crate::ui::widgets::xy_pad(ui, pad_id, label_x, label_y, x, y, pad_size, pad_locked, 1);
    pad_changed
}

/// 3-knob switchable XY pad — pad + side-mounted cycle chip inside a
/// glass pane, centred within the card width.  `pair_state` is the
/// authoritative pair index (0/1/2) persisted in `RackModule.pad_pair`;
/// the helper syncs it with the widget's egui temp memory so right-click
/// cycling on the pad and left-click on the chip both update it.
///
/// Returns `(value_changed, pair_changed)` — caller writes back values
/// if `value_changed`, persists `*pair_state` back to state if
/// `pair_changed`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn render_three_pad(
    ui: &mut egui::Ui,
    pad_id: &str,
    labels: [&'static str; 3],
    pair_state: &mut u8,
    fields: (&mut f32, &mut f32, &mut f32),
    locks: [bool; 3],
) -> (bool, bool) {
    use crate::ui::widgets;
    // Push authoritative pair into egui temp so the widget renders it.
    let cur_pair = (*pair_state as usize) % 3;
    ui.ctx()
        .data_mut(|d| d.insert_temp(egui::Id::new(pad_id), cur_pair));

    let (lx, ly) = three_pair_labels(cur_pair, labels[0], labels[1], labels[2]);
    let pad_locked = match cur_pair {
        0 => locks[0] || locks[1],
        1 => locks[0] || locks[2],
        _ => locks[1] || locks[2],
    };
    let (a, b, c) = fields;
    let mut val_changed = false;

    let group_w = ui.available_width();
    widgets::glass_group(ui, group_w, |ui| {
        let pad_size = (ui.available_width() - PAD_CYCLE_SIDE_W - 8.0).clamp(50.0, 100.0);
        widgets::centered_row(ui, |ui| {
            let (pad_changed, _) = match cur_pair {
                0 => widgets::xy_pad(ui, pad_id, lx, ly, a, b, pad_size, pad_locked, 3),
                1 => widgets::xy_pad(ui, pad_id, lx, ly, a, c, pad_size, pad_locked, 3),
                _ => widgets::xy_pad(ui, pad_id, lx, ly, b, c, pad_size, pad_locked, 3),
            };
            if pad_changed {
                val_changed = true;
            }
            ui.add_space(4.0);
            ui.vertical(|ui| {
                draw_pad_cycle_button(ui, pad_id, 3, lx, ly);
            });
        });
    });

    // Read back: right-click on pad and cycle-button clicks both went to temp.
    let new_pair = ui
        .ctx()
        .data(|d| d.get_temp::<usize>(egui::Id::new(pad_id)))
        .unwrap_or(cur_pair)
        % 3;
    let pair_changed = new_pair != cur_pair;
    if pair_changed {
        *pair_state = new_pair as u8;
    }
    (val_changed, pair_changed)
}
