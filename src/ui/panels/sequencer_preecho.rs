// ─── ui/panels/sequencer_preecho.rs ──────────────────────────────────────────
// Pre-echo (anchor lead-ins) UI row — compact single-row layout with voice
// tabs.  Semantics live in src/sequencer/preecho.rs.
//
// Layout (one horizontal line):
//   PRE-ECHO  [kit_a][kit_b][amen][bass][hoover][an1x]
//             anchors: 0, 16    LEN 4    VEL  RAT    CLEAR
//
// The six voice buttons act as tabs — clicking one switches the
// editor to that voice's config; all voices' configs are preserved.
// An optional second row shows a status summary of all voices with
// active preecho, so the user can see at a glance which voices are
// being modulated without clicking through each tab.

use crate::sequencer::PreechoConfig;
use crate::ui::{ImpulseApp, theme, widgets};

const VOICE_KEYS: &[&str] = &["kit_a", "kit_b", "amen", "bass", "hoover", "an1x"];

pub(super) fn draw_preecho_row(app: &mut ImpulseApp, ui: &mut egui::Ui) {
    ui.add_space(2.0);

    // Which voice is currently being edited.  Stored in egui memory so
    // the tab selection sticks across frames without polluting AppState.
    let sel_id = egui::Id::new("preecho_selected_voice");
    let mut selected: String = ui
        .ctx()
        .data(|d| d.get_temp::<String>(sel_id))
        .unwrap_or_else(|| "kit_a".to_string());
    if !VOICE_KEYS.iter().any(|v| *v == selected) {
        selected = "kit_a".to_string();
    }

    // Snapshot the currently-selected voice's config for editing.
    let mut cfg = app
        .state
        .read()
        .sequencer
        .preecho
        .get(&selected)
        .cloned()
        .unwrap_or_default();

    let anchors_id = egui::Id::new(("preecho_anchors", selected.clone()));
    let anchors_str_default = anchor_list_to_string(&cfg.anchors);
    let mut anchors_str: String = ui
        .ctx()
        .data(|d| d.get_temp::<String>(anchors_id))
        .unwrap_or(anchors_str_default.clone());
    // If the stored buffer is stale relative to the resolved config
    // (e.g. LLM wrote new anchors externally), prefer the resolved one.
    if anchor_list_matches(&anchors_str, &cfg.anchors).not_ok() {
        anchors_str = anchors_str_default;
    }

    let mut changed = false;
    let mut new_selected = selected.clone();

    ui.horizontal(|ui| {
        // Header label.
        ui.label(
            egui::RichText::new("PRE-ECHO")
                .monospace()
                .size(8.0)
                .color(theme::SMOKE),
        );
        ui.add_space(4.0);

        // Voice tabs.  Active voices (with any anchors + length > 0)
        // get a CHALK label so you can see at a glance which voice
        // groups have the modulator armed.
        let preecho_map = app.state.read().sequencer.preecho.clone();
        for vk in VOICE_KEYS {
            let is_sel = selected == *vk;
            let is_active = preecho_map.get(*vk).map(|c| c.is_active()).unwrap_or(false);
            let col = if is_sel {
                theme::CHALK
            } else if is_active {
                theme::FOG
            } else {
                theme::IRON
            };
            let fill = if is_sel {
                egui::Color32::from_gray(45)
            } else {
                egui::Color32::TRANSPARENT
            };
            if ui
                .add(
                    egui::Button::new(egui::RichText::new(*vk).monospace().size(7.5).color(col))
                        .fill(fill)
                        .min_size(egui::vec2(0.0, 14.0)),
                )
                .on_hover_text(if is_active {
                    format!("{} — preecho active", vk)
                } else {
                    format!("{} — preecho off", vk)
                })
                .clicked()
            {
                new_selected = (*vk).to_string();
            }
        }
        ui.separator();

        // Anchor list input.
        let resp = ui.add(
            egui::TextEdit::singleline(&mut anchors_str)
                .hint_text("anchors e.g. 0, 16")
                .desired_width(96.0)
                .font(egui::FontId::monospace(8.0)),
        );
        if resp.changed() {
            ui.ctx()
                .data_mut(|d| d.insert_temp(anchors_id, anchors_str.clone()));
        }
        if resp.lost_focus() || resp.ctx.input(|i| i.key_pressed(egui::Key::Enter)) {
            cfg.anchors = parse_anchor_list(&anchors_str);
            changed = true;
        }

        // LEN drag value.
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

        // VEL / RAT toggles.
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

        // CLEAR — disable preecho for the currently-selected voice.
        if ui
            .small_button(
                egui::RichText::new("CLEAR")
                    .monospace()
                    .size(7.0)
                    .color(theme::ASH),
            )
            .on_hover_text(format!("Disable pre-echo for {}", selected))
            .clicked()
        {
            cfg = PreechoConfig::default();
            ui.ctx()
                .data_mut(|d| d.insert_temp(anchors_id, String::new()));
            changed = true;
        }
    });

    if new_selected != selected {
        ui.ctx()
            .data_mut(|d| d.insert_temp(sel_id, new_selected.clone()));
    }

    if changed {
        if cfg.is_active() {
            app.state
                .write()
                .sequencer
                .preecho
                .insert(selected.clone(), cfg);
        } else {
            app.state.write().sequencer.preecho.remove(&selected);
        }
    }
}

fn anchor_list_to_string(anchors: &[u8]) -> String {
    anchors
        .iter()
        .map(|n| n.to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

fn parse_anchor_list(s: &str) -> Vec<u8> {
    s.split(|c: char| c == ',' || c.is_whitespace())
        .filter(|t| !t.is_empty())
        .filter_map(|t| t.parse::<u8>().ok())
        .filter(|n| (*n as usize) < 64)
        .collect()
}

/// Result of comparing the user's in-memory anchor string against the
/// resolved config — used to detect when an external write invalidates
/// the displayed text.
struct AnchorCheck(bool);

impl AnchorCheck {
    fn not_ok(self) -> bool {
        !self.0
    }
}

fn anchor_list_matches(text: &str, cfg_anchors: &[u8]) -> AnchorCheck {
    AnchorCheck(parse_anchor_list(text) == cfg_anchors)
}
