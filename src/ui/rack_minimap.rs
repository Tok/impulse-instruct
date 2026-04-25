// ─── ui/rack_minimap.rs ──────────────────────────────────────────────────────
// Bird's-eye mini-map overlay for the rack canvas.  Anchored to the
// bottom-right of the visible canvas rect and shows:
//
//   • A rounded backdrop the same chip colour family as the rest of
//     the chrome.
//   • Each module's bounding rectangle, scaled into the mini-map
//     extent.  No labels — these are thumbnails for spatial nav, not
//     module identification.
//   • A viewport indicator (filled translucent rect) marking the
//     section currently visible in the scroll area.
//   • Click-to-pan: clicking inside the mini-map sets the scroll
//     offset so the clicked y becomes the centre of the viewport.
//
// Toggle is `UiPrefs.show_rack_minimap` (off by default).  When tall
// racks make the scroll bar uninformative, this is the recovery UI.

use egui::{Color32, Pos2, Rect, Stroke, Vec2, scroll_area::ScrollAreaOutput};

use crate::state::ModuleKind;
use crate::ui::theme;

/// Mini-map dimensions in screen points.
const MINIMAP_W: f32 = 140.0;
const MINIMAP_H: f32 = 110.0;
/// Padding from the canvas edge.
const MINIMAP_MARGIN: f32 = 8.0;

/// Map a card's screen-space rect to its content-space rect (i.e.
/// independent of the current scroll offset).  Pure helper so the
/// math can be unit-tested.
pub fn card_to_content_space(card: Rect, scroll_inner_min: Pos2, scroll_offset_y: f32) -> Rect {
    let dy = scroll_offset_y - scroll_inner_min.y;
    Rect::from_min_max(
        Pos2::new(card.min.x, card.min.y + dy),
        Pos2::new(card.max.x, card.max.y + dy),
    )
}

/// Map a content-space y position to the mini-map's y axis.  Returns
/// a value clamped to `[map_top, map_bottom]`.  Pure helper so a
/// regression on scroll math doesn't need a UI test to catch.
pub fn content_y_to_map_y(content_y: f32, content_h: f32, map_top: f32, map_h: f32) -> f32 {
    if content_h <= 1.0 {
        return map_top;
    }
    let t = (content_y / content_h).clamp(0.0, 1.0);
    map_top + t * map_h
}

/// Inverse of `content_y_to_map_y` — used by click-to-pan.
pub fn map_y_to_content_y(map_y: f32, content_h: f32, map_top: f32, map_h: f32) -> f32 {
    if map_h <= 1.0 {
        return 0.0;
    }
    let t = ((map_y - map_top) / map_h).clamp(0.0, 1.0);
    t * content_h
}

/// Draw the mini-map and handle click-to-pan.  Returns `true` when
/// the user clicked inside it (so the caller can request a repaint).
pub fn draw(
    ctx: &egui::Context,
    canvas_rect: Rect,
    cards: &[(ModuleKind, Rect)],
    scroll_out: &ScrollAreaOutput<()>,
) -> bool {
    let map_rect = Rect::from_min_size(
        Pos2::new(
            canvas_rect.max.x - MINIMAP_W - MINIMAP_MARGIN,
            canvas_rect.max.y - MINIMAP_H - MINIMAP_MARGIN,
        ),
        Vec2::new(MINIMAP_W, MINIMAP_H),
    );
    let layer = egui::LayerId::new(egui::Order::Foreground, egui::Id::new("rack_minimap"));
    let mut painter = ctx.layer_painter(layer);
    painter.set_clip_rect(canvas_rect);

    let r = egui::Rounding::same(4.0);
    painter.rect_filled(map_rect, r, Color32::from_rgba_premultiplied(8, 8, 8, 220));
    painter.rect_stroke(map_rect, r, Stroke::new(0.8, theme::IRON));

    let content_h = scroll_out.content_size.y.max(1.0);
    let scroll_inner_min = scroll_out.inner_rect.min;
    let scroll_offset_y = scroll_out.state.offset.y;

    // Map width: just clip cards to the canvas width range.  Most
    // racks fit a single column at the chosen grid_cols, so the X
    // axis is mostly informative — the Y axis is what users
    // actually scroll.
    let canvas_w = canvas_rect.width().max(1.0);

    for (_kind, card) in cards {
        let cs = card_to_content_space(*card, scroll_inner_min, scroll_offset_y);
        let y0 = content_y_to_map_y(cs.min.y, content_h, map_rect.min.y, MINIMAP_H);
        let y1 = content_y_to_map_y(cs.max.y, content_h, map_rect.min.y, MINIMAP_H);
        // X mapping: map [canvas_rect.min.x, canvas_rect.max.x] to mini-map x.
        let nx0 = ((cs.min.x - canvas_rect.min.x) / canvas_w).clamp(0.0, 1.0);
        let nx1 = ((cs.max.x - canvas_rect.min.x) / canvas_w).clamp(0.0, 1.0);
        let x0 = map_rect.min.x + nx0 * MINIMAP_W;
        let x1 = map_rect.min.x + nx1 * MINIMAP_W;
        let thumb = Rect::from_min_max(
            Pos2::new(x0 + 1.0, y0 + 1.0),
            Pos2::new((x1 - 1.0).max(x0 + 2.0), (y1 - 1.0).max(y0 + 2.0)),
        );
        painter.rect_filled(thumb, egui::Rounding::same(1.0), Color32::from_gray(70));
    }

    // Viewport indicator: the slice of content currently visible.
    let visible_h = scroll_out.inner_rect.height();
    let vy0 = content_y_to_map_y(scroll_offset_y, content_h, map_rect.min.y, MINIMAP_H);
    let vy1 = content_y_to_map_y(
        scroll_offset_y + visible_h,
        content_h,
        map_rect.min.y,
        MINIMAP_H,
    );
    let view_rect = Rect::from_min_max(
        Pos2::new(map_rect.min.x + 1.0, vy0),
        Pos2::new(map_rect.max.x - 1.0, vy1),
    );
    painter.rect_filled(
        view_rect,
        egui::Rounding::same(1.5),
        Color32::from_rgba_premultiplied(220, 220, 220, 22),
    );
    painter.rect_stroke(
        view_rect,
        egui::Rounding::same(1.5),
        Stroke::new(1.0, theme::CHALK),
    );

    // Click-to-pan handler — read pointer state directly since the
    // overlay layer doesn't go through the regular ui.interact path.
    // Drag works the same as click because the pointer is being
    // queried every frame while down.
    let pointer = ctx.pointer_latest_pos();
    let pointer_down = ctx.input(|i| i.pointer.primary_down());
    let pointer_pressed = ctx.input(|i| i.pointer.primary_pressed());
    let mut clicked = false;
    if let Some(pos) = pointer
        && map_rect.contains(pos)
        && (pointer_pressed || pointer_down)
    {
        let max_y = (content_h - visible_h).max(0.0);
        let target_centre = map_y_to_content_y(pos.y, content_h, map_rect.min.y, MINIMAP_H);
        let target_top = (target_centre - visible_h * 0.5).clamp(0.0, max_y);
        let mut state = scroll_out.state;
        state.offset.y = target_top;
        state.store(ctx, scroll_out.id);
        clicked = true;
    }
    clicked
}
