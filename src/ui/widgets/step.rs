// ─── ui/widgets/step.rs ───────────────────────────────────────────────────────
// Sequencer step widgets: standard step button + Huth Farbige Noten U-cup cell.

use egui::{Color32, Pos2, Sense, Stroke, Ui, Vec2};

use crate::ui::theme;

// ─── Step Button ─────────────────────────────────────────────────────────────

/// A sequencer step button with neumorphic raised/pressed chrome style.
///
/// Returns `Some(new_active)` when the user's interaction asks to set
/// this step's state, and `None` otherwise.  Two gestures produce a
/// request:
///
///   • **Click / tap**: returns `Some(!active)` — classic toggle.
///   • **Drag-paint**: holding the pointer down and dragging across
///     steps activates (or clears) each step that enters the path.
///     The paint direction is locked at the drag's start — pressing
///     on an inactive step paints ON as the user drags over more
///     inactive steps; pressing on an active step paints OFF.  A step
///     that already matches the paint direction is left alone (so the
///     gesture is idempotent and doesn't flip back and forth).
///
/// `vel` tints the fill when active (0 = dim, 1 = full bright).
/// `dot_color`: when Some, the button body stays neutral and only a small
///   coloured dot is drawn — used for bass steps so the palette stays subtle.
/// `note_label`: when Some, a tiny note name (e.g. "C4") is drawn in
///   `dot_color` inside the cell, above the dot. Ignored for drum rows.
/// `size_px` comes from `UiPrefs.pad_size.px()`.
/// `probability` — 0.0–1.0, shown as a dim ring when < 1.0 to indicate uncertainty.
pub fn step_button(
    ui: &mut Ui,
    active: bool,
    current: bool,
    vel: f32,
    probability: f32,
    dot_color: Option<Color32>,
    note_label: Option<&str>,
    size_px: f32,
) -> Option<bool> {
    let sz = Vec2::splat(size_px);
    let (rect, response) = ui.allocate_exact_size(sz, Sense::click_and_drag());

    // Drag-paint bookkeeping shared across every step-button widget in
    // this frame.  The key is a single global `Id` because only one
    // pointer can be mid-drag at a time — two grids can coexist because
    // the pointer can only be hovering over one cell per frame.
    let paint_key = egui::Id::new("step_paint_dir");
    let mut action: Option<bool> = None;

    // Plain click (including touch-tap) → toggle.
    if response.clicked() {
        action = Some(!active);
    } else if response.drag_started() {
        // Drag begins on this cell — the opposite state becomes the
        // paint direction for the rest of the gesture.
        let dir = !active;
        ui.ctx().data_mut(|d| d.insert_temp(paint_key, dir));
        action = Some(dir);
    } else if response.contains_pointer() && ui.input(|i| i.pointer.primary_down()) {
        // Pointer is down and has entered this cell — honour the paint
        // direction set at drag start.  `active != d` check keeps the
        // gesture idempotent: hovering over a cell that already matches
        // the direction doesn't re-toggle it.
        let dir: Option<bool> = ui.ctx().data(|d| d.get_temp(paint_key));
        if let Some(d) = dir
            && active != d
        {
            action = Some(d);
        }
    }
    // Clear the paint direction once the pointer is up, regardless of
    // where the release happened.  Idempotent across every widget, so
    // doing it here instead of a central loop keeps step_button self-
    // contained.
    if !ui.input(|i| i.pointer.primary_down()) {
        ui.ctx().data_mut(|d| d.remove::<bool>(paint_key));
    }

    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        // LED halo + current-step bloom use the SAME painter as the
        // step chrome.  Previously this was a foreground-layer
        // painter (Order::Foreground), then a `painter_at(rect.expand(…))`
        // with a wider clip — both variants caused the sequencer grid
        // to disappear in some recording geometries (expanded clip
        // replaced the parent's clip, which in a nested ScrollArea
        // sometimes culled the step chrome itself).  Using
        // `painter.clone()` keeps the glow on the parent layer with
        // the parent's clip, so rendering stays inside the panel no
        // matter what the scroll position is.  The halo will be
        // clipped at the step rect's boundary rather than extending
        // into the gap, which is fine — the bloom falls off fast and
        // the step's own chrome was covering the bleed anyway.
        let glow = painter.clone();
        // Generous rounding — no hard square corners visible
        let r = egui::Rounding::same((size_px * 0.22).max(4.0));
        let inner = rect.shrink(1.5);

        if active {
            // Debossed / pressed look:
            // Dark inset shadow frame, slightly brighter outer ring, deep inner well.
            painter.rect_filled(inner, r, theme::DEEP);
            // Bright bottom-right rim (light bounces off raised surround)
            painter.rect_stroke(inner.shrink(0.5), r, Stroke::new(1.0, theme::IRON));
            // Dark top-left rim (pressed inward — shadow side)
            painter.rect_stroke(inner, r, Stroke::new(1.0, theme::VOID));
            // Inset well
            let inset = inner.shrink(2.5);
            painter.rect_filled(inset, r, Color32::from_gray(22));

            if let Some(col) = dot_color {
                let dot_r = (size_px * 0.18).max(2.5);
                let dot_pos = Pos2::new(inset.center().x, inset.max.y - dot_r - 1.0);
                theme::led(&glow, dot_pos, dot_r, col, 1.0);
                // Note name label above the dot (only when cell is large enough)
                if let Some(label) = note_label
                    && size_px >= 26.0
                {
                    let font_sz = (size_px * 0.22).clamp(7.0, 10.0);
                    let text_pos = Pos2::new(inset.center().x, inset.min.y + font_sz * 0.5 + 1.0);
                    painter.text(
                        text_pos,
                        egui::Align2::CENTER_CENTER,
                        label,
                        egui::FontId::monospace(font_sz),
                        col,
                    );
                }
            } else {
                // Velocity heat fill — brighter = louder
                let dim = 0.35_f32 + vel * 0.65;
                let g = (200.0 * dim) as u8;
                painter.rect_filled(inset, r, Color32::from_rgba_unmultiplied(g, g, g, 70));
            }
            // Probability indicator: small corner dot when < 100%
            if probability < 0.99 {
                let prob_g = (40.0 + probability * 80.0) as u8;
                let dot_r = (size_px * 0.08).max(1.5);
                let pos = Pos2::new(inset.right() - dot_r - 1.0, inset.top() + dot_r + 1.0);
                painter.circle_filled(pos, dot_r, Color32::from_gray(prob_g));
            }
        } else {
            // Raised look:
            // Bright top-left rim, dark bottom-right — classic emboss.
            painter.rect_filled(inner, r, Color32::from_gray(32));
            // Top-left highlight
            painter.rect_stroke(inner, r, Stroke::new(1.0, Color32::from_gray(58)));
            // Bottom-right shadow (slightly inset to overlay only bottom/right)
            painter.rect_stroke(
                inner.shrink(0.5),
                r,
                Stroke::new(1.0, Color32::from_gray(10)),
            );

            if let Some(col) = dot_color {
                let dot_r = (size_px * 0.14).max(2.0);
                let dot_pos = Pos2::new(inner.center().x, inner.max.y - dot_r - 2.0);
                // Dim LED — half-intensity so inactive steps still feel "on standby"
                theme::led(&glow, dot_pos, dot_r, col, 0.45);
            }
        }

        // Current-step cursor: outer bloom glow + bright border + inner ring.
        // Bloom paints on `glow` (expanded-clip painter on the parent
        // panel layer — see `halo_reach` above) so the three bloom
        // rings extend past this step's rect.  Neighbouring steps
        // drawn later in the same pass will cover the portion of the
        // bloom that falls inside their rect; the remaining bleed is
        // the visible bloom signature.
        if current {
            // Outer bloom halos
            for i in 1..=3u8 {
                let expand = i as f32 * 1.5;
                let alpha = 40u8.saturating_sub(i * 12);
                glow.rect_filled(
                    rect.expand(expand),
                    r,
                    Color32::from_rgba_unmultiplied(220, 220, 220, alpha),
                );
            }
            // Bright outer border
            painter.rect_stroke(rect.shrink(0.5), r, Stroke::new(1.5, theme::CHALK));
            // Subtle inner ring — reinforces the "lit up" face
            painter.rect_stroke(
                inner.shrink(1.5),
                r,
                Stroke::new(1.0, Color32::from_rgba_unmultiplied(200, 200, 200, 45)),
            );
        }
    }

    action
}
