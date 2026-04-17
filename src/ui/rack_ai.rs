// ─── ui/rack_ai.rs ── AI zone rendering (LLM console + agents) ──────────────
// Extracted from rack_canvas.rs to stay under the 1000-line limit.

use crate::state::{ModuleKind, Zone, rack::GRID_COLS};
use crate::ui::ImpulseApp;
use crate::ui::module_card::{self, PortPos};
use crate::ui::rack_content::{draw_llm_agent_content, handle_title_drag, reorder_module_by_drop};
use crate::ui::rack_grid::{card_x, draw_zone_grid_dots, grid_step, module_grid_h, module_grid_w};

#[allow(clippy::too_many_arguments)]
pub(super) fn draw_ai_zone(
    app: &mut ImpulseApp,
    ui: &mut egui::Ui,
    ports: &mut Vec<PortPos>,
    card_rects: &mut Vec<(ModuleKind, egui::Rect)>,
    col_w: f32,
    available_w: f32,
    content_top: f32,
    all_collapsed: bool,
) {
    app.zone_y[0] = ui.cursor().top() - content_top;
    {
        let (add, toggle, toggle_all) =
            module_card::zone_rail(ui, "AI", true, 24, app.zone_ai_collapsed, all_collapsed);
        if toggle {
            app.zone_ai_collapsed = !app.zone_ai_collapsed;
        }
        if toggle_all {
            let target = !all_collapsed;
            app.zone_ai_collapsed = target;
            app.zone_global_collapsed = target;
            app.zone_voice_collapsed = target;
            app.zone_fxmod_collapsed = target;
        }
        if add {
            app.add_menu_zone = Some(Zone::Ai);
        }
    }

    if app.zone_ai_collapsed {
        return;
    }

    let ctx_ai = ui.ctx().clone();
    let zone_left = ui.cursor().left();
    let zone_top = ui.cursor().top();
    let step = grid_step(col_w);

    let ai_mods: Vec<(u32, ModuleKind, bool, u8, u8)> = {
        let s = app.state.read();
        s.rack
            .modules
            .iter()
            .filter(|m| m.zone == Zone::Ai)
            .map(|m| (m.id, m.kind, m.enabled, m.grid_col, m.grid_row))
            .collect()
    };
    let zone_rows = ai_mods
        .iter()
        .map(|&(_, kind, _, _, gr)| {
            let (_, h) = kind.grid_size(GRID_COLS);
            gr as usize + h as usize
        })
        .max()
        .unwrap_or(0);
    let zone_h = zone_rows as f32 * step;
    let zone_rect = ui
        .allocate_exact_size(egui::Vec2::new(available_w, zone_h), egui::Sense::hover())
        .0;

    for &(id, kind, enabled, gc, gr) in &ai_mods {
        let slot_w = module_grid_w(kind, col_w);
        let slot_h = module_grid_h(kind, col_w);
        let (col_span, _) = kind.grid_size(GRID_COLS);
        let x = card_x(zone_rect.min.x, gc, col_span, step, app.rack_flipped);
        let y = zone_rect.min.y + gr as f32 * step;
        let card_rect =
            egui::Rect::from_min_size(egui::Pos2::new(x, y), egui::Vec2::new(slot_w, slot_h));
        let mut child = ui.child_ui(card_rect, egui::Layout::top_down(egui::Align::LEFT), None);
        let resp = if app.rack_flipped {
            module_card::module_card_back(
                &mut child,
                id,
                kind,
                enabled,
                Some(slot_w),
                Some(slot_h),
                app.kind_scale(kind),
                ports,
            )
        } else {
            module_card::module_card_sized(
                &mut child,
                id,
                kind,
                enabled,
                Some(slot_w),
                Some(slot_h),
                app.kind_scale(kind),
                ports,
                |ui| match kind {
                    ModuleKind::LlmConsole => app.draw_llm_console_content(ui),
                    ModuleKind::LlmAgent => draw_llm_agent_content(app, ui, id),
                    _ => {}
                },
            )
            .0
        };
        card_rects.push((kind, resp.card_rect));
        if resp.toggle_clicked
            && let Some(m) = app
                .state
                .write()
                .rack
                .modules
                .iter_mut()
                .find(|m| m.id == id)
        {
            m.enabled = !enabled;
        }
        if resp.remove_clicked {
            app.confirm_remove_module = Some(id);
        }
        if kind == ModuleKind::LlmAgent {
            let drop_pos = ctx_ai.pointer_latest_pos().unwrap_or_default();
            let zo = zone_rect.min;
            if handle_title_drag(app, &ctx_ai, id, &resp, Zone::Ai, zo, step, col_w) {
                reorder_module_by_drop(app, id, drop_pos, Zone::Ai, zo, step, col_w);
            }
        }
    }

    draw_zone_grid_dots(ui, zone_left, zone_top, zone_top + zone_h, col_w);
}

/// Fixed 6-line preview of an agent's last JSON response, painted directly
/// via the clipped painter. Using a painter (instead of TextEdit/ScrollArea)
/// guarantees the preview can't overflow its box — text beyond the 6-line
/// viewport is simply not drawn, and lines beyond the rect bottom are
/// clipped by `painter.with_clip_rect(rect)`.
pub(crate) fn draw_last_response_preview(ui: &mut egui::Ui, last_resp: &str) {
    let font = egui::FontId::monospace(7.5);
    let row_h = ui.fonts(|f| f.row_height(&font));
    const LINES: usize = 6;
    let viewport_h = row_h * LINES as f32 + 4.0;
    let viewport_w = ui.available_width();
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(viewport_w, viewport_h), egui::Sense::hover());
    if last_resp.is_empty() {
        return;
    }

    let color = crate::ui::theme::SMOKE;
    let painter = ui.painter_at(rect);
    let available_text_w = (rect.width() - 4.0).max(1.0);

    // Wrap existing newlines first, then word-wrap each to the viewport width.
    let mut wrapped: Vec<String> = Vec::new();
    'outer: for raw_line in last_resp.split('\n') {
        if raw_line.is_empty() {
            wrapped.push(String::new());
            if wrapped.len() >= LINES {
                break 'outer;
            }
            continue;
        }
        let mut current = String::new();
        for ch in raw_line.chars() {
            let candidate: String = current.chars().chain(std::iter::once(ch)).collect();
            let w = ui.fonts(|f| {
                f.layout_no_wrap(candidate.clone(), font.clone(), color)
                    .size()
                    .x
            });
            if w > available_text_w && !current.is_empty() {
                wrapped.push(std::mem::take(&mut current));
                if wrapped.len() >= LINES {
                    break 'outer;
                }
            }
            current.push(ch);
        }
        if !current.is_empty() {
            wrapped.push(current);
            if wrapped.len() >= LINES {
                break 'outer;
            }
        }
    }
    // Truncate ellipsis on the last line if we cut the response off.
    if wrapped.len() >= LINES
        && let Some(last) = wrapped.last_mut()
        && !last.ends_with('…')
    {
        // Shorten enough to fit the ellipsis.
        while ui.fonts(|f| {
            let mut t = last.clone();
            t.push('…');
            f.layout_no_wrap(t, font.clone(), color).size().x
        }) > available_text_w
            && !last.is_empty()
        {
            last.pop();
        }
        last.push('…');
    }

    for (i, line) in wrapped.iter().take(LINES).enumerate() {
        let y = rect.min.y + 2.0 + row_h * i as f32;
        painter.text(
            egui::pos2(rect.min.x + 2.0, y),
            egui::Align2::LEFT_TOP,
            line,
            font.clone(),
            color,
        );
    }
}
