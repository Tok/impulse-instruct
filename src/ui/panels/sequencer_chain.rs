// ─── ui/panels/sequencer_chain.rs ────────────────────────────────────────────
// Pattern bank selector and chain editor for the sequencer panel.
// Also home to small shared sequencer-row helpers (e.g. per-step pan).

use crate::llm::styles::StyleCatalog;
use crate::state::{bank_swap, bank_write, chain_pop, chain_push, set_chain_enabled};
use crate::ui::{ImpulseApp, theme};

/// Per-step bass pan cell — paints a centre-line + tick at the current
/// pan value; drag-to-set, right-click resets to centre.  Voice 0
/// mirrors `bass_pattern`; other voices write into `bass_patterns[vi]`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn pan_cell_voice(
    ui: &mut egui::Ui,
    app: &mut ImpulseApp,
    voice_idx: usize,
    abs: usize,
    pan: f32,
    enabled: bool,
    pad_px: f32,
    row_h: f32,
) {
    let (rect, resp) =
        ui.allocate_exact_size(egui::vec2(pad_px, row_h), egui::Sense::click_and_drag());
    let p = ui.painter_at(rect);
    let cy = rect.center().y;
    p.line_segment(
        [
            egui::pos2(rect.min.x + 2.0, cy),
            egui::pos2(rect.max.x - 2.0, cy),
        ],
        egui::Stroke::new(0.5, theme::PIT),
    );
    if enabled {
        let cx = rect.min.x + (pan.clamp(-1.0, 1.0) + 1.0) * 0.5 * rect.width();
        let col = if pan.abs() < 0.01 {
            theme::IRON
        } else {
            theme::CHALK
        };
        p.line_segment(
            [
                egui::pos2(cx, rect.min.y + 1.0),
                egui::pos2(cx, rect.max.y - 1.0),
            ],
            egui::Stroke::new(1.5, col),
        );
    }
    let write_pan = |app: &mut ImpulseApp, np: f32| {
        let vi = voice_idx.min(crate::state::MAX_BASS_VOICES - 1);
        let mut s = app.state.write();
        if let Some(pat) = s.sequencer.bass_patterns.get_mut(vi)
            && let Some(step) = pat.get_mut(abs)
        {
            step.pan = np;
        }
        if vi == 0
            && let Some(step) = s.sequencer.bass_pattern.get_mut(abs)
        {
            step.pan = np;
        }
    };
    if enabled
        && (resp.dragged() || resp.clicked())
        && let Some(pos) = resp.interact_pointer_pos()
    {
        let np = ((pos.x - rect.min.x) / rect.width().max(1.0) * 2.0 - 1.0).clamp(-1.0, 1.0);
        write_pan(app, np);
    }
    if enabled && resp.secondary_clicked() {
        write_pan(app, 0.0);
    }
}

const SLOT_NAMES: [&str; 8] = ["A", "B", "C", "D", "E", "F", "G", "H"];

/// Compact bank + chain on a single horizontal line.
pub fn draw_pattern_chain(app: &mut ImpulseApp, ui: &mut egui::Ui) {
    let (pattern_edit, chain, chain_enabled, chain_pos) = {
        let s = app.state.read();
        (
            s.pattern_edit,
            s.chain.clone(),
            s.chain_enabled,
            s.chain_pos,
        )
    };

    // Bank slots
    ui.label(
        egui::RichText::new("BANK")
            .color(theme::SMOKE)
            .monospace()
            .size(8.0),
    );
    for (slot, &name) in SLOT_NAMES.iter().enumerate() {
        let is_edit = slot == pattern_edit;
        let col = if is_edit { theme::CHALK } else { theme::PIT };
        let fill = if is_edit {
            egui::Color32::from_gray(45)
        } else {
            egui::Color32::TRANSPARENT
        };
        let resp = ui.add_sized(
            [16.0, 14.0],
            egui::Button::new(egui::RichText::new(name).monospace().size(8.0).color(col))
                .fill(fill),
        );
        if resp.clicked() {
            let s = app.state.read().clone();
            *app.state.write() = bank_swap(s, slot);
        }
        if resp.secondary_clicked() {
            let s = app.state.read().clone();
            *app.state.write() = bank_write(s, slot);
        }
    }

    ui.separator();

    // Chain slots (compact)
    ui.label(
        egui::RichText::new("CHAIN")
            .color(theme::SMOKE)
            .monospace()
            .size(8.0),
    );
    for pos in 0..8usize {
        let label = chain.get(pos).map(|&s| SLOT_NAMES[s % 8]).unwrap_or("·");
        let cursor_here = chain_enabled && !chain.is_empty() && chain_pos % chain.len() == pos;
        let fill = if cursor_here {
            egui::Color32::from_gray(70)
        } else {
            egui::Color32::TRANSPARENT
        };
        let col = if chain.get(pos).is_some() {
            theme::FOG
        } else {
            theme::IRON
        };
        ui.add_sized(
            [16.0, 14.0],
            egui::Button::new(egui::RichText::new(label).monospace().size(8.0).color(col))
                .fill(fill),
        );
    }
    let small_btn = |ui: &mut egui::Ui, label: &str| {
        ui.add_sized(
            [14.0, 14.0],
            egui::Button::new(
                egui::RichText::new(label)
                    .monospace()
                    .size(8.0)
                    .color(theme::FOG),
            ),
        )
        .clicked()
    };
    if small_btn(ui, "+") {
        let s = app.state.read().clone();
        *app.state.write() = chain_push(s, pattern_edit);
    }
    if small_btn(ui, "−") {
        let s = app.state.read().clone();
        *app.state.write() = chain_pop(s);
    }
    let on_col = if chain_enabled {
        theme::CHALK
    } else {
        theme::IRON
    };
    let on_fill = if chain_enabled {
        egui::Color32::from_gray(50)
    } else {
        egui::Color32::TRANSPARENT
    };
    if ui
        .add_sized(
            [22.0, 14.0],
            egui::Button::new(
                egui::RichText::new(if chain_enabled { "ON" } else { "OF" })
                    .monospace()
                    .size(8.0)
                    .color(on_col),
            )
            .fill(on_fill),
        )
        .clicked()
    {
        let s = app.state.read().clone();
        *app.state.write() = set_chain_enabled(s, !chain_enabled);
    }

    // Per-slot style tag — pick a style to apply on chain-advance into the
    // currently-edited slot.  None = "leave active style unchanged when
    // this slot plays".  Shown as "STYLE [name]" after ON/OF so the whole
    // row reads as an arrangement tool.
    ui.separator();
    let cur_style = app.state.read().sequencer.pattern_style.clone();
    let cur_label = match cur_style.as_deref() {
        Some(id) => StyleCatalog::get()
            .styles()
            .iter()
            .find(|s| s.id == id)
            .map(|s| s.name.clone())
            .unwrap_or_else(|| id.to_string()),
        None => "—".to_string(),
    };
    ui.label(
        egui::RichText::new("STYLE")
            .color(theme::SMOKE)
            .monospace()
            .size(8.0),
    );
    let mut new_style: Option<Option<String>> = None;
    egui::ComboBox::from_id_source("pattern_style_combo")
        .selected_text(
            egui::RichText::new(cur_label)
                .monospace()
                .size(8.0)
                .color(theme::FOG),
        )
        .width(110.0)
        .show_ui(ui, |ui| {
            if ui
                .selectable_label(
                    cur_style.is_none(),
                    egui::RichText::new("— (no style change)")
                        .monospace()
                        .size(9.0),
                )
                .clicked()
            {
                new_style = Some(None);
            }
            ui.separator();
            for s in StyleCatalog::get().styles() {
                if ui
                    .selectable_label(
                        cur_style.as_deref() == Some(s.id.as_str()),
                        egui::RichText::new(&s.name).monospace().size(9.0),
                    )
                    .clicked()
                {
                    new_style = Some(Some(s.id.clone()));
                }
            }
        });
    if let Some(sel) = new_style {
        app.state.write().sequencer.pattern_style = sel;
    }

    // Per-slot tempo opt-in — when lit, chain advance into this slot uses
    // the slot's own bpm / swing instead of preserving the prior transport.
    // Off by default so existing chain projects don't surprise-jump tempos.
    let bpm_apply = app.state.read().sequencer.pattern_bpm_apply;
    let (bpm_col, bpm_fill) = if bpm_apply {
        (theme::CHALK, egui::Color32::from_gray(50))
    } else {
        (theme::IRON, egui::Color32::TRANSPARENT)
    };
    let bpm_resp = ui
        .add_sized(
            [30.0, 14.0],
            egui::Button::new(
                egui::RichText::new("BPM⇥")
                    .monospace()
                    .size(8.0)
                    .color(bpm_col),
            )
            .fill(bpm_fill),
        )
        .on_hover_text(if bpm_apply {
            "This slot drives its bpm/swing on chain advance. Click to disable."
        } else {
            "This slot's bpm/swing persists across chain advances. Click to drive tempo on entry."
        });
    if bpm_resp.clicked() {
        app.state.write().sequencer.pattern_bpm_apply = !bpm_apply;
    }
}
