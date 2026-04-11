// ─── ui/widgets/centered.rs ── centered horizontal row ───────────────────────
// Uses egui temp memory to cache content width from the previous frame,
// then adds a centering spacer on the current frame.

use egui::Ui;

pub fn centered_row<R>(
    ui: &mut Ui,
    add_contents: impl FnOnce(&mut Ui) -> R,
) -> egui::InnerResponse<R> {
    let row_id = ui.id().with("centered_row_w");
    let prev_w: f32 = ui.ctx().data(|d| d.get_temp(row_id).unwrap_or(0.0));
    let avail = ui.available_width();
    let spacer = ((avail - prev_w) / 2.0).max(0.0);
    let resp = ui.horizontal(|ui| {
        if spacer > 1.0 {
            ui.add_space(spacer);
        }
        add_contents(ui)
    });
    let content_w = resp.response.rect.width() - spacer;
    ui.ctx().data_mut(|d| d.insert_temp(row_id, content_w));
    resp
}
