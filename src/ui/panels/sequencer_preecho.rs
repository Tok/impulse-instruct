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
use crate::ui::{ImpulseApp, SEQ_LABEL_W, theme, widgets};

/// Default cell height for the anchor strip; cell width tracks the
/// sequencer's `effective_pad_px()` at draw time so anchors align with
/// the step columns above.  When the sequencer hasn't laid out yet
/// (first frame) we fall back to this width.
const ANCHOR_STEP_W_FALLBACK: f32 = 21.0;
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

    let mut clear_clicked = false;

    // ── Line 1 — PRE-ECHO label + voice tabs + right-justified strip ──────
    ui.horizontal(|ui| {
        // Mirror the sequencer rows' prefix (10 + 10 px M/S spacers,
        // then a SEQ_LABEL_W-wide label slot containing PRE-ECHO).  The
        // voice tabs that follow then naturally start at the same x
        // anchor as the bass / drum sliders above.
        ui.add_space(10.0);
        ui.add_space(10.0);
        let label_w = SEQ_LABEL_W - 20.0;
        let (label_rect, _) =
            ui.allocate_exact_size(egui::vec2(label_w, 14.0), egui::Sense::hover());
        ui.painter().text(
            egui::pos2(label_rect.min.x, label_rect.center().y),
            egui::Align2::LEFT_CENTER,
            "PRE-ECHO",
            egui::FontId::monospace(8.0),
            theme::SMOKE,
        );

        // (PRE-ECHO label is painted directly into the label slot above.)
        // Voice tabs — sized like the BANK / CHAIN slots in the chain
        // row above (uniform width, height 14, monospace 8.0) so the
        // sequencer's two header strips visually align.  Width is 38 to
        // accommodate the longest voice label ("hoover").
        //
        // Fully-armed voices (enabled + anchors + length) get FOG;
        // configured-but-off voices (anchors present but enabled=false)
        // get a dimmer ASH so you can see at a glance which voice groups
        // are set up without being confused about which are actually
        // modulating the playback.
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
                .add_sized(
                    [38.0, 14.0],
                    egui::Button::new(egui::RichText::new(*vk).monospace().size(8.0).color(col))
                        .fill(fill),
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

        // Anchor strip geometry must mirror the sequencer's step row
        // exactly: every step is followed by `item_spacing.x`, plus a
        // 4 px spacer at each bar boundary and a 2 px spacer at every
        // 4-step boundary that isn't a bar (see `beat_div` in
        // sequencer.rs).  Without this the strip ends up several
        // pixels narrower than the grid above and anchor cells drift
        // left of their step buttons.
        let (seq_steps, time_sig_num) = {
            let s = app.state.read();
            (
                s.sequencer.steps.clamp(1, 64),
                s.sequencer.time_sig_num as usize,
            )
        };
        let cell_w = {
            let p = app.state.read().ui_prefs.effective_pad_px();
            if p > 1.0 { p } else { ANCHOR_STEP_W_FALLBACK }
        };
        let item_spacing_x = ui.spacing().item_spacing.x;
        let extra_before = |i: usize| -> f32 {
            if i == 0 {
                return 0.0;
            }
            let beat_pos = i % time_sig_num.max(1);
            if beat_pos == 0 {
                4.0
            } else if i.is_multiple_of(4) {
                2.0
            } else {
                0.0
            }
        };
        let mut strip_w = 0.0f32;
        for i in 0..seq_steps {
            if i > 0 {
                strip_w += item_spacing_x + extra_before(i);
            }
            strip_w += cell_w;
        }

        // Right-justify the anchor strip so its right edge lines up with
        // the sequencer's step grid above.  Sequencer rows end 8 px shy
        // of the panel's right edge (see step_grid_width / row_spacer).
        let strip_right = ui.max_rect().max.x - 8.0;
        let cur_x = ui.cursor().min.x;
        let pad_to_strip = (strip_right - strip_w - cur_x).max(ANCHOR_STRIP_PAD);
        ui.add_space(pad_to_strip);
        let (rect, resp) =
            ui.allocate_exact_size(egui::vec2(strip_w, ANCHOR_STEP_H), egui::Sense::click());
        let painter = ui.painter_at(rect);

        // Cumulative left-edge of each cell, mirroring the inter-cell
        // stride used by the sequencer's step row.  Used for both
        // drawing and click hit-testing so they stay in lock-step.
        let mut step_x: Vec<f32> = Vec::with_capacity(seq_steps);
        let mut x = 0.0f32;
        for i in 0..seq_steps {
            if i > 0 {
                x += cell_w + item_spacing_x + extra_before(i);
            }
            step_x.push(x);
        }

        let mut clicked_step: Option<usize> = None;
        if resp.clicked()
            && let Some(p) = resp.interact_pointer_pos()
        {
            let rel_x = p.x - rect.min.x;
            if rel_x >= 0.0 {
                let mut hit = seq_steps - 1;
                for (i, &x_left) in step_x.iter().enumerate() {
                    if rel_x < x_left + cell_w {
                        hit = i;
                        break;
                    }
                }
                clicked_step = Some(hit);
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
        for (i, &x_left) in step_x.iter().enumerate() {
            let x0 = rect.min.x + x_left;
            let r = egui::Rect::from_min_size(
                egui::pos2(x0, rect.min.y),
                egui::vec2(cell_w, ANCHOR_STEP_H),
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
                    egui::Stroke::new(0.6, theme::ASH),
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
    });

    // ── Line 2 — ON / LEN / VEL / RAT / CLEAR controls ────────────────────
    // Lifted out of the line-1 horizontal so the strip on line 1 has the
    // full panel width to right-justify into.  Without the split the
    // trailing controls pushed the strip off the right edge whenever the
    // panel was narrow (or many voice tabs were rendered).  Same leading
    // prefix as line 1 so the controls left-align with the sliders above
    // (bass / drum volume sliders start at SEQ_LABEL_W).
    ui.horizontal(|ui| {
        ui.add_space(10.0);
        ui.add_space(10.0);
        ui.add_space(SEQ_LABEL_W - 20.0);
        let mut enabled = cfg.enabled;
        if widgets::toggle_button(ui, if enabled { "ON" } else { "OFF" }, &mut enabled) {
            cfg.enabled = enabled;
            changed = true;
        }
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
            .on_hover_text(format!("Disable pre-echo for {}", selected))
            .clicked()
        {
            clear_clicked = true;
        }
    });

    if clear_clicked {
        app.state.write().sequencer.preecho.remove(&selected);
        cfg = PreechoConfig::default();
        changed = false;
    }

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
