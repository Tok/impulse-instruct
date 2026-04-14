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

/// Size of each step cell in the anchor strip — kept square so they
/// read as actual toggle boxes, not slivers.  21 px matches the
/// bumped size from the earlier "1.5×" pass.
const ANCHOR_STEP_W: f32 = 21.0;
const ANCHOR_STEP_H: f32 = 21.0;
/// Space to reserve on either side of the strip (and above/below)
/// so the cells don't butt up against neighboring widgets.
const ANCHOR_STRIP_PAD: f32 = 6.0;

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

        // Voice tabs.  Fully-armed voices (enabled + anchors + length)
        // get FOG; configured-but-off voices (anchors present but
        // enabled=false) get a dimmer IRON so you can see at a glance
        // which voice groups are set up without being confused about
        // which are actually modulating the playback.
        let preecho_map = app.state.read().sequencer.preecho.clone();
        for vk in VOICE_KEYS {
            let is_sel = selected == *vk;
            let entry = preecho_map.get(*vk);
            let is_active = entry.map(|c| c.is_active()).unwrap_or(false);
            let is_armed_but_off = entry
                .map(|c| !c.is_active() && !c.anchors.is_empty() && c.length > 0)
                .unwrap_or(false);
            let col = if is_sel {
                theme::CHALK
            } else if is_active {
                theme::FOG
            } else if is_armed_but_off {
                theme::ASH
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

        // Clickable anchor strip — one cell per sequencer step.  Click
        // to toggle anchor at that step.  Lit cells = anchors; dimly
        // lit = inside a lead-in window of some anchor.  Padded on all
        // four sides so the square cells have visible breathing room.
        ui.add_space(ANCHOR_STRIP_PAD);
        let seq_steps = app.state.read().sequencer.steps.clamp(1, 64);
        let strip_w = ANCHOR_STEP_W * seq_steps as f32 + (seq_steps as f32 - 1.0);
        // Horizontal padding sandwiches the strip between two spacers so
        // it doesn't touch the separator or the LEN label.
        ui.add_space(ANCHOR_STRIP_PAD);
        let (rect, resp) =
            ui.allocate_exact_size(egui::vec2(strip_w, ANCHOR_STEP_H), egui::Sense::click());
        let painter = ui.painter_at(rect);
        let mut clicked_step: Option<usize> = None;
        if resp.clicked()
            && let Some(p) = resp.interact_pointer_pos()
        {
            let rel = (p.x - rect.min.x) / (ANCHOR_STEP_W + 1.0);
            if rel >= 0.0 {
                clicked_step = Some((rel as usize).min(seq_steps - 1));
            }
        }
        // Compute which steps are inside a lead-in window for preview.
        let in_leadin = |step: usize| -> bool {
            if cfg.length == 0 {
                return false;
            }
            for &a in &cfg.anchors {
                let a = a as usize;
                if a >= seq_steps {
                    continue;
                }
                let d = (a + seq_steps - step) % seq_steps;
                if d > 0 && d <= cfg.length as usize {
                    return true;
                }
            }
            false
        };
        for i in 0..seq_steps {
            let x0 = rect.min.x + i as f32 * (ANCHOR_STEP_W + 1.0);
            let r = egui::Rect::from_min_size(
                egui::pos2(x0, rect.min.y),
                egui::vec2(ANCHOR_STEP_W, ANCHOR_STEP_H),
            );
            let is_anchor = cfg.anchors.iter().any(|&a| a as usize == i);
            let fill = if is_anchor {
                theme::CHALK
            } else if in_leadin(i) {
                egui::Color32::from_gray(70)
            } else {
                egui::Color32::from_gray(22)
            };
            painter.rect_filled(r, egui::Rounding::same(1.5), fill);
            // Beat boundaries get a subtle brighter outline so 4/4 subdivisions read cleanly.
            if i % 4 == 0 {
                painter.rect_stroke(
                    r,
                    egui::Rounding::same(1.5),
                    egui::Stroke::new(0.6, egui::Color32::from_gray(90)),
                );
            }
        }
        if let Some(step) = clicked_step {
            let step_u = step as u8;
            if cfg.anchors.contains(&step_u) {
                cfg.anchors.retain(|a| *a != step_u);
            } else {
                cfg.anchors.push(step_u);
                cfg.anchors.sort();
                cfg.anchors.dedup();
            }
            // If the user is adding their first anchor and LEN is still at
            // 0 (the factory default), seed a musically-useful default so
            // the modulation actually fires.  One beat = seq_steps / 4
            // (typical 32-step bar → 8-step lead-in, which is the width
            // of a bar-quarter — decent "build-up" length).  Capped at 16.
            if cfg.length == 0 && !cfg.anchors.is_empty() {
                cfg.length = ((seq_steps / 4).max(1) as u8).min(16);
                // Velocity ramp is the most common / least surprising
                // default.  Ratchet ramp stays off (it's more intrusive).
                cfg.velocity_ramp = true;
            }
            changed = true;
        }
        // Trailing horizontal pad — mirror of the leading ANCHOR_STRIP_PAD.
        ui.add_space(ANCHOR_STRIP_PAD);

        // Master ON/OFF — a quick bypass that preserves anchors + length
        // so the user can audition with/without the modulation.
        let mut enabled = cfg.enabled;
        if widgets::toggle_button(ui, if enabled { "ON" } else { "OFF" }, &mut enabled) {
            cfg.enabled = enabled;
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

        // CLEAR — fully remove preecho for the currently-selected voice.
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
            app.state.write().sequencer.preecho.remove(&selected);
            // Skip the persist-below path by zeroing cfg and signalling
            // not changed; the remove already did the state write.
            cfg = PreechoConfig::default();
            changed = false;
        }
    });

    if new_selected != selected {
        ui.ctx()
            .data_mut(|d| d.insert_temp(sel_id, new_selected.clone()));
    }

    if changed {
        // Always persist the edit even when inactive.  Previously we
        // removed the config when !is_active(), which wiped a user's
        // fresh anchor the moment they clicked it (length default was 0
        // so is_active was false until the length was also bumped — a
        // race the user hit every time).  Now the config sticks; only
        // the CLEAR button removes it (via a separate branch below).
        app.state
            .write()
            .sequencer
            .preecho
            .insert(selected.clone(), cfg);
    }

    // Trailing vertical pad below the whole row so the preecho section
    // has visible separation from the lane scroll area above it and the
    // module boundary below it.
    ui.add_space(ANCHOR_STRIP_PAD);
}
