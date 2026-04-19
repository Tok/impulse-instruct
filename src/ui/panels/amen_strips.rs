// ─── ui/panels/amen_strips.rs ────────────────────────────────────────────────
// Slice-direction and slice-order mini-strips used by the Amen panel.
// Split out of `amen.rs` so the panel's main `draw_amen` stays readable
// and each strip's logic can be tweaked without scrolling past the other.

use crate::ui::theme;

/// Render the per-slice direction strip — one cell per slice, each showing
/// the direction that slice plays: `→` forward, `←` reverse.  Click any
/// cell to flip it.  When the `reverses` vec is empty, every slice
/// inherits the global direction (`global_reverse`), so the strip shows
/// a single-glyph column of either all-forward or all-reverse cells with
/// a slightly dimmer tint — one click then flips that slice explicitly
/// and lights up the strip.  A `RESET` button clears the vec, restoring
/// "inherit global" for every slice.
///
/// `reverses` is mutated in-place; returns `true` if anything changed.
pub(super) fn draw_slice_reverse_strip(
    ui: &mut egui::Ui,
    reverses: &mut Vec<bool>,
    slice_count: u8,
    active_slice: Option<u8>,
    global_reverse: bool,
) -> bool {
    let n = slice_count.max(1) as usize;
    // "Inherit global" mode: the vec is empty.  We render the strip in
    // that mode by reading each cell as `global_reverse`.  First click
    // promotes the strip out of inherit mode by filling with the global
    // value, then flipping the clicked cell.
    let inheriting = reverses.is_empty();
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        ui.label(
            egui::RichText::new("DIR/SL")
                .color(theme::SMOKE)
                .monospace()
                .size(7.5),
        );
        #[allow(clippy::needless_range_loop)]
        for i in 0..n {
            let cell_reversed = if inheriting {
                global_reverse
            } else {
                reverses.get(i).copied().unwrap_or(global_reverse)
            };
            let label = if cell_reversed { "←" } else { "→" };
            // Highlight the cell whose slice is currently playing so the
            // user can track which slice the playhead's on — mirrors the
            // order strip's active-cell tint.
            let is_active = active_slice.map(|s| s as usize) == Some(i);
            let bg = if is_active {
                egui::Color32::from_gray(80)
            } else {
                theme::PIT
            };
            // Dim the glyph slightly when inheriting so the strip reads
            // as "not customised yet" without showing a separate colour
            // state.  Once the user clicks, the vec populates and full
            // intensity kicks in.
            let fg = match (is_active, inheriting) {
                (true, _) => theme::CHALK,
                (false, true) => theme::ASH,
                (false, false) => theme::FOG,
            };
            let hover = if inheriting {
                format!(
                    "Slice {} · inherits global {} (click to customise)",
                    i + 1,
                    if global_reverse { "REVERSE" } else { "FORWARD" },
                )
            } else {
                format!(
                    "Slice {} · {} (click to flip)",
                    i + 1,
                    if cell_reversed { "REVERSE" } else { "FORWARD" },
                )
            };
            if ui
                .add(
                    egui::Button::new(egui::RichText::new(label).monospace().size(8.0).color(fg))
                        .fill(bg)
                        .min_size(egui::vec2(18.0, 16.0)),
                )
                .on_hover_text(hover)
                .clicked()
            {
                if inheriting {
                    // Promote: fill with the global direction, then flip
                    // the clicked slot.  Now the vec is populated so
                    // subsequent clicks toggle in place.
                    reverses.clear();
                    reverses.extend(std::iter::repeat_n(global_reverse, n));
                    reverses[i] = !global_reverse;
                } else {
                    // Already customised — simple flip.  Auto-resize if
                    // slice_count grew since last customisation.
                    if reverses.len() < n {
                        let fill = global_reverse;
                        reverses.resize(n, fill);
                    }
                    reverses[i] = !reverses[i];
                }
                changed = true;
            }
        }
        if ui
            .small_button(egui::RichText::new("RESET").monospace().size(7.0))
            .on_hover_text("Clear per-slice directions — every slice inherits global REV/FWD")
            .clicked()
        {
            reverses.clear();
            changed = true;
        }
    });
    changed
}

/// Render the slice-order edit strip — one cell per step position, each
/// showing which slice plays at that step.  Click any cell to cycle through
/// 1..=slice_count (wrapping back to 1).  A small `RESET` button restores
/// the identity mapping (empty `amen_slice_order`).
///
/// `order` is mutated in-place; returns `true` if anything changed.
pub(super) fn draw_slice_order_strip(
    ui: &mut egui::Ui,
    order: &mut Vec<u8>,
    slice_count: u8,
    active_slice: Option<u8>,
) -> bool {
    let n = slice_count.max(1) as usize;
    if order.len() != n {
        // Auto-resize to match current slice count.  Initialise new entries
        // with their identity index so resizing slice count from 4 → 8 leaves
        // the existing cells alone and adds 4..7 at the end.
        let prev_len = order.len();
        order.resize(n, 0);
        for (i, slot) in order.iter_mut().enumerate().skip(prev_len) {
            *slot = i as u8;
        }
    }
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        ui.label(
            egui::RichText::new("ORDER")
                .color(theme::SMOKE)
                .monospace()
                .size(7.5),
        );
        #[allow(clippy::needless_range_loop)]
        for i in 0..n {
            let slot = order[i] as usize;
            let label = format!("{}", slot + 1);
            // Highlight the cell whose POSITION matches the active step
            // (where the playhead currently is in the order strip).
            let is_active = active_slice.map(|s| s as usize) == Some(slot);
            let bg = if is_active {
                egui::Color32::from_gray(80)
            } else {
                theme::PIT
            };
            let fg = if is_active { theme::CHALK } else { theme::FOG };
            if ui
                .add(
                    egui::Button::new(egui::RichText::new(&label).monospace().size(8.0).color(fg))
                        .fill(bg)
                        .min_size(egui::vec2(18.0, 16.0)),
                )
                .on_hover_text(format!(
                    "Step {} → slice {} (click to cycle)",
                    i + 1,
                    slot + 1
                ))
                .clicked()
            {
                order[i] = ((slot + 1) % n) as u8;
                changed = true;
            }
        }
        if ui
            .small_button(egui::RichText::new("RESET").monospace().size(7.0))
            .on_hover_text("Reset to identity (step N → slice N)")
            .clicked()
        {
            order.clear();
            changed = true;
        }
        if ui
            .small_button(egui::RichText::new("RAND").monospace().size(7.0))
            .on_hover_text("Shuffle the order into a random permutation of slices 1..N")
            .clicked()
        {
            // SystemTime-seeded Fisher-Yates over 0..n.  Permutes the whole
            // strip; pair with RESET if you want the identity back.
            let seed = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos() as u64)
                .unwrap_or(0xC0FFEE);
            let mut perm: Vec<u8> = (0..n as u8).collect();
            let mut s = seed | 1;
            for i in (1..perm.len()).rev() {
                s = s
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                let j = ((s >> 33) as usize) % (i + 1);
                perm.swap(i, j);
            }
            *order = perm;
            changed = true;
        }
    });
    changed
}
