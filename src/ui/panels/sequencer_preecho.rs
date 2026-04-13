// ─── ui/panels/sequencer_preecho.rs ──────────────────────────────────────────
// Pre-echo (anchor lead-ins) UI row — one compact section per voice at the
// bottom of the sequencer panel.  Extracted from sequencer.rs to stay
// under the 1000-line LOC cap.  Semantics live in src/sequencer/preecho.rs.

use crate::ui::{ImpulseApp, theme, widgets};

pub(super) fn draw_preecho_row(app: &mut ImpulseApp, ui: &mut egui::Ui) {
    use crate::sequencer::PreechoConfig;
    ui.add_space(4.0);
    egui::CollapsingHeader::new(
        egui::RichText::new("PRE-ECHO — anchor lead-ins")
            .monospace()
            .size(8.0)
            .color(theme::SMOKE),
    )
    .id_source("preecho_section")
    .default_open(false)
    .show(ui, |ui| {
        for voice_key in &["kit_a", "kit_b", "amen", "bass", "hoover", "an1x"] {
            let mut cfg = app
                .state
                .read()
                .sequencer
                .preecho
                .get(*voice_key)
                .cloned()
                .unwrap_or_default();
            // Per-voice in-memory text buffer keyed by id so the user can
            // type an anchor list without it snapping back on every draw.
            let anchors_id = egui::Id::new(("preecho_anchors", *voice_key));
            let anchors_str_prev = anchor_list_to_string(&cfg.anchors);
            let mut anchors_str: String = ui
                .ctx()
                .data(|d| d.get_temp::<String>(anchors_id))
                .unwrap_or_else(|| anchors_str_prev.clone());
            let mut changed = false;
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(voice_key.to_uppercase())
                        .monospace()
                        .size(8.0)
                        .color(theme::SMOKE),
                );
                // Anchor list
                let resp = ui.add(
                    egui::TextEdit::singleline(&mut anchors_str)
                        .hint_text("anchors e.g. 0, 16")
                        .desired_width(110.0)
                        .font(egui::FontId::monospace(8.0)),
                );
                if resp.changed() {
                    ui.ctx()
                        .data_mut(|d| d.insert_temp(anchors_id, anchors_str.clone()));
                }
                if resp.lost_focus() {
                    cfg.anchors = parse_anchor_list(&anchors_str);
                    changed = true;
                }
                // Length
                ui.label(
                    egui::RichText::new("LEN")
                        .monospace()
                        .size(7.5)
                        .color(theme::SMOKE),
                );
                let mut len = cfg.length as i32;
                if ui
                    .add(egui::DragValue::new(&mut len).range(0..=16).speed(0.1))
                    .changed()
                {
                    cfg.length = len.clamp(0, 16) as u8;
                    changed = true;
                }
                // VEL / RAT ramps
                let mut vr = cfg.velocity_ramp;
                if widgets::toggle_button(ui, if vr { "VEL" } else { "vel" }, &mut vr) {
                    cfg.velocity_ramp = vr;
                    changed = true;
                }
                let mut rr = cfg.ratchet_ramp;
                if widgets::toggle_button(ui, if rr { "RAT" } else { "rat" }, &mut rr) {
                    cfg.ratchet_ramp = rr;
                    changed = true;
                }
                if ui
                    .small_button(
                        egui::RichText::new("CLEAR")
                            .monospace()
                            .size(7.0)
                            .color(theme::ASH),
                    )
                    .on_hover_text("Disable pre-echo for this voice")
                    .clicked()
                {
                    cfg = PreechoConfig::default();
                    ui.ctx()
                        .data_mut(|d| d.insert_temp(anchors_id, String::new()));
                    changed = true;
                }
            });
            if changed {
                if cfg.is_active() {
                    app.state
                        .write()
                        .sequencer
                        .preecho
                        .insert((*voice_key).to_string(), cfg);
                } else {
                    app.state.write().sequencer.preecho.remove(*voice_key);
                }
            }
        }
    });
}

pub(super) fn anchor_list_to_string(anchors: &[u8]) -> String {
    anchors
        .iter()
        .map(|n| n.to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

pub(super) fn parse_anchor_list(s: &str) -> Vec<u8> {
    s.split(|c: char| c == ',' || c.is_whitespace())
        .filter(|t| !t.is_empty())
        .filter_map(|t| t.parse::<u8>().ok())
        .filter(|n| (*n as usize) < 64)
        .collect()
}
