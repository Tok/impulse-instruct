// ─── ui/rack_grid.rs ── Fixed 12-column rack grid helpers ────────────────────
// Fixed 12-column grid (like CSS/Bootstrap).  Each module spans a number of
// columns and rows defined in `ModuleKind::grid_size()`.
// Extracted from rack_canvas.rs.

use egui::Color32;

use crate::state::{ModuleKind, rack::GRID_COLS};

pub(super) const RACK_GAP: f32 = 4.0;

/// Size of one grid column (floored to whole pixels for crisp alignment).
pub(super) fn grid_col_w(available_w: f32) -> f32 {
    let n = GRID_COLS as f32;
    ((available_w - (n - 1.0) * RACK_GAP) / n).floor()
}

/// Pixel width for a module's grid column span.
pub(super) fn module_grid_w(kind: ModuleKind, col_w: f32) -> f32 {
    let (c, _) = kind.grid_size(GRID_COLS);
    c as f32 * col_w + (c as f32 - 1.0).max(0.0) * RACK_GAP
}

/// Pixel height for a module's grid row span.
pub(super) fn module_grid_h(kind: ModuleKind, col_w: f32) -> f32 {
    let (_, r) = kind.grid_size(GRID_COLS);
    module_grid_h_rows(r, col_w)
}

/// Pixel height for an explicit grid-row count (used when the row count
/// differs from the kind's static value, e.g. an XY-pad-expanded FX card).
pub(super) fn module_grid_h_rows(rows: u8, col_w: f32) -> f32 {
    let r = rows as f32;
    r * col_w + (r - 1.0).max(0.0) * RACK_GAP
}

/// Dynamic row count for the sequencer grid cell — adapts to visible lanes.
pub(super) fn sequencer_grid_rows(state: &crate::state::AppState, col_w: f32) -> u8 {
    use crate::state::ModuleKind;
    let has = |k: ModuleKind| state.rack.modules.iter().any(|m| m.kind == k && m.enabled);
    let has_bass = has(ModuleKind::AcidBass);
    let has_hoover = has(ModuleKind::HooverLead);
    let has_an1x = has(ModuleKind::An1xVoice);
    let active_drum_voices = state
        .sequencer
        .drum_patterns
        .iter()
        .filter(|(_, p)| p.iter().any(|st| st.active))
        .count();
    // Pixel estimate — mirrors draw_sequencer. Tuned so the grid cell fits
    // the content exactly (no trailing empty row).
    let pad = state.ui_prefs.effective_pad_px();
    let step_row = pad + 22.0; // main step button row with padding
    let marker_row = 16.0; // accent / slide marker row (compact)
    let drum_sub = 14.0; // vel/prob/ratchet sub-lane under drum step row
    let header_h = 55.0;
    let sub_rows = crate::ui::panels::sequencer::sub_rows_for(state.sequencer.steps) as f32;
    let bass_h = if has_bass {
        sub_rows * (step_row + marker_row + marker_row)
    } else {
        0.0
    };
    let hoover_h = if has_hoover { sub_rows * step_row } else { 0.0 };
    let an1x_h = if has_an1x { sub_rows * step_row } else { 0.0 };
    let drums_h = active_drum_voices as f32 * sub_rows * (step_row + drum_sub);
    // Pre-echo row that lives at the bottom of the sequencer panel
    // (always rendered): padding + horizontal row with the anchor
    // strip (21 px square cells + leading/trailing 6 px pads)
    // + labels + toggles + trailing pad.  Budget 42 px covers it
    // with a few px of margin so the cell isn't shaved close.
    let preecho_h = 42.0;
    let total_h = header_h + bass_h + hoover_h + an1x_h + drums_h + preecho_h;
    // Convert pixel height to grid rows: each grid row occupies col_w + RACK_GAP
    // (minus one trailing gap on the last row).
    let step = col_w + RACK_GAP;
    let grid_rows = ((total_h + RACK_GAP) / step).ceil() as u8;
    grid_rows.max(2)
}

/// Pixel height for the sequencer, dynamically sized by rack contents.
pub(super) fn sequencer_grid_h(state: &crate::state::AppState, col_w: f32) -> f32 {
    let r = sequencer_grid_rows(state, col_w) as f32;
    r * col_w + (r - 1.0).max(0.0) * RACK_GAP
}

/// Grid step: col_w + gap. Cards at adjacent grid positions are separated by this.
pub(super) fn grid_step(col_w: f32) -> f32 {
    col_w + RACK_GAP
}

/// Width spanning `n` grid columns (including internal gaps).
#[allow(dead_code)]
pub(crate) fn span_w(n: u8, col_w: f32) -> f32 {
    n as f32 * col_w + (n as f32 - 1.0).max(0.0) * RACK_GAP
}

/// Published grid column width — readable from any panel via egui temp data.
/// Stored by `draw_rack_inner`, read via `grid_unit()`.
pub(super) const GRID_COL_W_ID: &str = "rack_grid_col_w";

/// Read the current grid column width from egui temp data.
/// Panels can use this to size sub-elements to grid multiples.
/// Returns 0.0 if called outside the rack draw cycle.
#[allow(dead_code)]
pub(crate) fn grid_unit(ctx: &egui::Context) -> f32 {
    ctx.data(|d| d.get_temp::<f32>(egui::Id::new(GRID_COL_W_ID)))
        .unwrap_or(0.0)
}

/// Compute card X position — mirrored horizontally when the rack is flipped.
pub(super) fn card_x(zone_left: f32, gc: u8, col_span: u8, step: f32, flipped: bool) -> f32 {
    if flipped {
        // Mirror: a card at column gc with width col_span maps to
        // column (GRID_COLS - gc - col_span) from the left.
        let mirror_col = GRID_COLS.saturating_sub(gc + col_span);
        zone_left + mirror_col as f32 * step
    } else {
        zone_left + gc as f32 * step
    }
}

/// Paint subtle dots at grid intersections within a zone's content area.
/// Called per-zone so each collapsible section gets its own aligned grid.
/// `zone_left` is the X where modules actually start (cursor X at zone start).
pub(super) fn draw_zone_grid_dots(
    ui: &egui::Ui,
    zone_left: f32,
    zone_top: f32,
    zone_bottom: f32,
    col_w: f32,
) {
    if col_w < 5.0 || zone_bottom <= zone_top {
        return;
    }
    let painter = ui.painter();
    let dot_color = Color32::from_gray(35);
    let step = col_w + RACK_GAP;
    // Dots sit at gap centerlines — offset by half_gap so a card spanning
    // columns c..c+n has dots at its left edge, between it and its neighbour,
    // and at its right edge.
    let half = RACK_GAP * 0.5;
    let ox = zone_left - half;
    let oy = zone_top - half;
    let right = ox + GRID_COLS as f32 * step + RACK_GAP;
    let mut y = oy;
    while y <= zone_bottom + step {
        for c in 0..=GRID_COLS {
            let x = ox + c as f32 * step;
            if x <= right + 1.0 {
                painter.circle_filled(egui::Pos2::new(x, y), 1.0, dot_color);
            }
        }
        y += step;
    }
}
