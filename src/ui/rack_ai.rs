// ─── ui/rack_ai.rs ── AI zone rendering (LLM console + agents) ──────────────
// Extracted from rack_canvas.rs to stay under the 1000-line limit.

use crate::state::{ModuleKind, Zone, rack::GRID_COLS};
use crate::ui::ImpulseApp;
use crate::ui::module_card::{self, PortPos};
use crate::ui::rack_canvas::{
    card_x, draw_zone_grid_dots, grid_step, module_grid_h, module_grid_w,
};
use crate::ui::rack_content::{draw_llm_agent_content, handle_title_drag, reorder_module_by_drop};

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

/// Fixed 6-line viewport for an agent's last JSON response. Always reserves
/// the same vertical space (even when empty) so long outputs can't push the
/// following `t/s` / cycles line off the card.
pub(crate) fn draw_last_response_preview(ui: &mut egui::Ui, last_resp: &str) {
    let font = egui::FontId::monospace(7.5);
    let row_h = ui.fonts(|f| f.row_height(&font));
    let viewport_h = row_h * 6.0 + 4.0;
    let viewport_w = ui.available_width();
    let mut view: String = last_resp.chars().take(720).collect();
    if last_resp.len() > 720 {
        view.push('…');
    }
    ui.allocate_ui(egui::vec2(viewport_w, viewport_h), |ui| {
        egui::ScrollArea::vertical()
            .min_scrolled_height(viewport_h)
            .max_height(viewport_h)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                egui::TextEdit::multiline(&mut view)
                    .desired_rows(6)
                    .desired_width(viewport_w)
                    .font(font)
                    .text_color(crate::ui::theme::SMOKE)
                    .interactive(false)
                    .show(ui);
            });
    });
}
