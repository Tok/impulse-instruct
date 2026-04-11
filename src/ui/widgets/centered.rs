// ─── ui/widgets/centered.rs ── centered horizontal row ───────────────────────
// Uses egui temp memory to cache content width from the previous frame,
// then adds a centering spacer on the current frame.

use egui::Ui;

pub fn centered_row<R>(
    ui: &mut Ui,
    add_contents: impl FnOnce(&mut Ui) -> R,
) -> egui::InnerResponse<R> {
    // Stable unique ID per call site — ui.next_auto_id() increments each call
    // within the same parent, so sibling rows get distinct IDs without relying
    // on cursor position (which shifts when spacers change, causing oscillation).
    let row_id = ui.next_auto_id().with("centered_row_w");
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
