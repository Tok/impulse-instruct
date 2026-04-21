// ─── ui/panels/sequencer_chain.rs ────────────────────────────────────────────
// Pattern bank selector and chain editor for the sequencer panel.
// Also home to small shared sequencer-row helpers (e.g. per-step pan).

use crate::llm::styles::StyleCatalog;
use crate::state::{
    bank_swap, bank_write, chain_pop, chain_push, clear_chain_slot_override, set_chain_enabled,
    set_chain_slot_bpm, set_chain_slot_repeats, set_chain_slot_style, swap_chain_slots,
};
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

/// Number of user-visible bank slots in the compact BANK strip.
/// Matches `DEFAULT_BANKS` — the first 8 slots are the ones the
/// A/B/C card UI renders.  Kept as a local const for loop bounds;
/// chain timelines render however many entries the chain actually
/// holds (up to `MAX_BANKS`).
const VISIBLE_BANK_SLOTS: usize = 8;

/// Compact label for a bank slot index.  1-based throughout — bank
/// index 0 shows as "1", index 7 as "8", index 52 as "53".
/// Previously the first 8 banks used letters A..H and machine-
/// generated banks past H rendered as decimals; mixing letters with
/// numbers (A..H then 9, 10, 11) was confusing, so everything is now
/// a plain 1-based count.
fn slot_label(idx: usize) -> String {
    (idx + 1).to_string()
}

/// How many chain slots the song timeline renders inline before it
/// windows around the playhead.  For a 53-bank MIDI import the full
/// timeline would be ~53 × 78 px of horizontal buttons, pushing the
/// sequencer panel off-screen; we show at most this many bars
/// centred on `chain_pos` instead, so the timeline stays the same
/// width regardless of chain length.
const TIMELINE_VISIBLE_MAX: usize = 12;

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

    // Tight button padding for this compact strip — default style
    // pads buttons 6 × 3 px which makes the 16 × 14 slot cells render
    // as `[   A   ]` instead of `[A]`, because egui grows a button's
    // intrinsic size past the requested `add_sized` when text +
    // padding exceeds it.  Shrinking to 2 × 1 keeps "A" snug inside
    // its 16-px cell.  Scoped via `ui.style_mut` so only this strip
    // is affected; surrounding panels keep their normal button feel.
    ui.style_mut().spacing.button_padding = egui::vec2(2.0, 1.0);
    ui.spacing_mut().item_spacing.x = 2.0;

    // Bank slots
    ui.label(
        egui::RichText::new("BANK")
            .color(theme::SMOKE)
            .monospace()
            .size(8.0),
    );
    for slot in 0..VISIBLE_BANK_SLOTS {
        let name = slot_label(slot);
        let is_edit = slot == pattern_edit;
        let col = if is_edit { theme::CHALK } else { theme::PIT };
        let fill = if is_edit {
            egui::Color32::from_gray(45)
        } else {
            egui::Color32::TRANSPARENT
        };
        let resp = ui.add_sized(
            [16.0, 14.0],
            egui::Button::new(egui::RichText::new(&name).monospace().size(8.0).color(col))
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
        let label_owned = chain.get(pos).map(|&s| slot_label(s));
        let label: &str = label_owned.as_deref().unwrap_or("·");
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

    // ── Export current pattern as .mid ───────────────────────────────────
    // Writes `pattern-<unix_secs>.mid` in cwd.  Sequencer-only export —
    // FX state stays in the project JSON.
    ui.separator();
    let export_resp = ui
        .add_sized(
            [54.0, 14.0],
            egui::Button::new(
                egui::RichText::new("MIDI ⇩")
                    .monospace()
                    .size(8.0)
                    .color(theme::FOG),
            ),
        )
        .on_hover_text(
            "Export the active pattern as a Standard MIDI File in the current directory.",
        );
    if export_resp.clicked() {
        let seq = app.state.read().sequencer.clone();
        match crate::midi::save_midi_export(&seq) {
            Ok(path) => log::info!("MIDI export → {}", path.display()),
            Err(e) => log::error!("MIDI export failed: {}", e),
        }
    }
}

// ─── Song-mode timeline ──────────────────────────────────────────────────────
// Gantt-style visualisation of the chain, sitting below the compact
// bank/chain row.  Each chain slot renders as a wider bar showing:
//   • pattern letter (A–H)          • ×N repeat badge (if > 1)
//   • style override tag (if set)   • BPM override tag (if set)
// The currently-playing slot gets a highlighted frame; a thin playhead
// line inside it shows which repeat is active.  Click a bar to open a
// popover for editing the slot's overrides.  Drag bars to reorder.

const TIMELINE_BAR_W: f32 = 20.0;
const TIMELINE_BAR_H: f32 = 22.0;
const TIMELINE_GAP: f32 = 2.0;

pub fn draw_song_timeline(app: &mut ImpulseApp, ui: &mut egui::Ui) {
    let (chain, overrides, chain_enabled, chain_pos, repeat_count) = {
        let s = app.state.read();
        (
            s.chain.clone(),
            s.chain_overrides.clone(),
            s.chain_enabled,
            s.chain_pos,
            s.chain_repeat_count,
        )
    };
    if chain.is_empty() {
        // Nothing to draw — keep the vertical rhythm by emitting a
        // subtle "add patterns above to build a song" hint so the
        // section doesn't silently disappear for new users.
        ui.horizontal(|ui| {
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new(
                    "SONG — chain is empty; push bank slots above to compose a song",
                )
                .color(theme::IRON)
                .monospace()
                .size(8.0),
            );
        });
        return;
    }

    // Drag-to-reorder bookkeeping.  `drag_src` tracks the slot the
    // pointer picked up this frame; on hover over a different slot it
    // swaps with it and follows.
    let drag_key = egui::Id::new("song_timeline_drag_src");
    let popover_key = egui::Id::new("song_timeline_popover");

    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("SONG")
                .color(theme::SMOKE)
                .monospace()
                .size(8.0),
        );
        ui.add_space(2.0);

        let mut swap_request: Option<(usize, usize)> = None;
        let mut open_popover: Option<usize> = None;

        // Windowed range — at most `TIMELINE_VISIBLE_MAX` bars, centred
        // on the current playhead when possible.  For short chains
        // (<= MAX) this is a no-op and we render the whole thing; for
        // the ~53-bank Bach MIDI import we render a 12-bar window
        // sliding with the playhead so the sequencer panel stays
        // on-screen.  When the window is clipped, a small "…/N" hint
        // before / after tells the user the chain extends further.
        let chain_len = chain.len();
        let (win_start, win_end) = if chain_len <= TIMELINE_VISIBLE_MAX {
            (0, chain_len)
        } else {
            let half = TIMELINE_VISIBLE_MAX / 2;
            let center = chain_pos.min(chain_len.saturating_sub(1));
            let start = center.saturating_sub(half);
            let end = (start + TIMELINE_VISIBLE_MAX).min(chain_len);
            // Pull start back if we hit the right edge so the window
            // stays full-sized even near the end of the chain.
            let start = end.saturating_sub(TIMELINE_VISIBLE_MAX);
            (start, end)
        };

        if win_start > 0 {
            ui.label(
                egui::RichText::new(format!("…{}", win_start))
                    .color(theme::IRON)
                    .monospace()
                    .size(8.0),
            );
            ui.add_space(2.0);
        }

        for (pos, &bank_idx) in chain
            .iter()
            .enumerate()
            .skip(win_start)
            .take(win_end - win_start)
        {
            let (rect, resp) = ui.allocate_exact_size(
                egui::vec2(TIMELINE_BAR_W, TIMELINE_BAR_H),
                egui::Sense::click_and_drag(),
            );
            ui.add_space(TIMELINE_GAP);

            // ── Paint ─────────────────────────────────────────────
            let cursor_here = chain_enabled && !chain.is_empty() && chain_pos % chain.len() == pos;
            let hovered = resp.hovered();
            let fill = if cursor_here {
                egui::Color32::from_gray(55)
            } else if hovered {
                egui::Color32::from_gray(38)
            } else {
                egui::Color32::from_gray(26)
            };
            let stroke = if cursor_here {
                egui::Stroke::new(1.5, theme::CHALK)
            } else {
                egui::Stroke::new(0.5, theme::IRON)
            };
            let painter = ui.painter_at(rect);
            let rounding = egui::Rounding::same(3.0);
            painter.rect_filled(rect, rounding, fill);
            painter.rect_stroke(rect, rounding, stroke);

            // Pattern number, centred in the narrow bar.  Font sized
            // so up to 3 digits ("999") still fits cleanly inside
            // TIMELINE_BAR_W (20 px); longer labels just truncate —
            // a chain longer than 999 banks is well past MAX_BANKS
            // so this is purely a render fallback.
            let letter = slot_label(bank_idx);
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                &letter,
                egui::FontId::monospace(10.0),
                theme::CHALK,
            );

            // Override summary doesn't fit in a 20 px bar — surface
            // it via the popover instead (click the bar to open).
            // Re-fetch the override below for the drag / click
            // handlers that still need it; paint nothing here.
            let ov = overrides.get(pos).cloned().unwrap_or_default();

            // Playhead — a thin vertical line inside the currently-
            // playing slot, positioned by how many repeats have passed.
            // Gives a rough "where am I in this slot" sense without
            // needing per-step granularity.
            if cursor_here {
                let total_reps = ov.repeats.max(1) as f32;
                let t = (repeat_count as f32 / total_reps).clamp(0.0, 1.0);
                let x = rect.min.x + 2.0 + (rect.width() - 4.0) * t;
                painter.line_segment(
                    [
                        egui::pos2(x, rect.min.y + 2.0),
                        egui::pos2(x, rect.max.y - 2.0),
                    ],
                    egui::Stroke::new(1.0, theme::CHALK),
                );
            }

            // ── Interactions ──────────────────────────────────────
            if resp.drag_started() {
                ui.ctx().data_mut(|d| d.insert_temp(drag_key, pos));
            }
            if resp.dragged() {
                // While dragging, swap with any slot we're hovering
                // over — the visual follows the pointer.
                if let Some(pointer) = ui.input(|i| i.pointer.interact_pos())
                    && let Some(src) = ui.ctx().data(|d| d.get_temp::<usize>(drag_key))
                    && src != pos
                {
                    // If the pointer has crossed into this bar, swap.
                    if rect.contains(pointer) {
                        swap_request = Some((src, pos));
                        ui.ctx().data_mut(|d| d.insert_temp(drag_key, pos));
                    }
                }
            }
            // Click (no drag) opens a per-slot override popover.
            if resp.clicked() && !resp.drag_started() {
                open_popover = Some(pos);
            }
        }

        if win_end < chain_len {
            ui.add_space(2.0);
            ui.label(
                egui::RichText::new(format!("+{}…", chain_len - win_end))
                    .color(theme::IRON)
                    .monospace()
                    .size(8.0),
            );
        }

        if let Some((a, b)) = swap_request {
            let s = app.state.read().clone();
            *app.state.write() = swap_chain_slots(s, a, b);
        }
        if let Some(pos) = open_popover {
            ui.ctx().data_mut(|d| d.insert_temp(popover_key, pos));
        }
        // Clear drag source on pointer release — otherwise a subsequent
        // click without a preceding drag_started keeps the stale src.
        if !ui.input(|i| i.pointer.primary_down()) {
            ui.ctx().data_mut(|d| d.remove::<usize>(drag_key));
        }
    });

    // Popover — one floating window that edits whichever slot the user
    // most recently clicked on.  Lets the user set bpm / style / repeats
    // without cluttering the bar itself.
    let popover_pos: Option<usize> = ui.ctx().data(|d| d.get_temp(popover_key));
    if let Some(pos) = popover_pos
        && pos < chain.len()
    {
        let mut open = true;
        egui::Window::new("")
            .id(egui::Id::new(("song_popover", pos)))
            .title_bar(false)
            .resizable(false)
            .collapsible(false)
            .open(&mut open)
            .fixed_pos(ui.cursor().min + egui::vec2(60.0, 28.0))
            .show(ui.ctx(), |ui| {
                let cur = overrides.get(pos).cloned().unwrap_or_default();
                ui.label(
                    egui::RichText::new(format!(
                        "Slot {} — pattern {}",
                        pos + 1,
                        slot_label(chain[pos]),
                    ))
                    .color(theme::CHALK)
                    .monospace()
                    .size(10.0),
                );
                ui.separator();

                // Repeats
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("×repeats")
                            .color(theme::FOG)
                            .monospace()
                            .size(9.0),
                    );
                    let mut r = cur.repeats as i32;
                    if ui
                        .add(egui::DragValue::new(&mut r).range(1..=64).speed(0.1))
                        .changed()
                    {
                        let s = app.state.read().clone();
                        *app.state.write() = set_chain_slot_repeats(s, pos, r.max(1) as u8);
                    }
                });

                // Style override
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("style")
                            .color(theme::FOG)
                            .monospace()
                            .size(9.0),
                    );
                    let cur_label = match &cur.style {
                        Some(id) => StyleCatalog::get()
                            .find_by_id(id)
                            .map(|s| s.name.clone())
                            .unwrap_or_else(|| id.clone()),
                        None => "—".to_string(),
                    };
                    let mut pick: Option<Option<String>> = None;
                    egui::ComboBox::from_id_source(("song_popover_style", pos))
                        .selected_text(egui::RichText::new(cur_label).monospace().size(9.0))
                        .width(120.0)
                        .show_ui(ui, |ui| {
                            if ui
                                .selectable_label(
                                    cur.style.is_none(),
                                    egui::RichText::new("—").monospace().size(9.0),
                                )
                                .clicked()
                            {
                                pick = Some(None);
                            }
                            for s in StyleCatalog::get().styles() {
                                if ui
                                    .selectable_label(
                                        cur.style.as_deref() == Some(s.id.as_str()),
                                        egui::RichText::new(&s.name).monospace().size(9.0),
                                    )
                                    .clicked()
                                {
                                    pick = Some(Some(s.id.clone()));
                                }
                            }
                        });
                    if let Some(p) = pick {
                        let st = app.state.read().clone();
                        *app.state.write() = set_chain_slot_style(st, pos, p);
                    }
                });

                // BPM override
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("bpm")
                            .color(theme::FOG)
                            .monospace()
                            .size(9.0),
                    );
                    let mut use_bpm = cur.bpm.is_some();
                    if ui.checkbox(&mut use_bpm, "").changed() {
                        let st = app.state.read().clone();
                        let new_bpm = if use_bpm {
                            Some(cur.bpm.unwrap_or(120.0))
                        } else {
                            None
                        };
                        *app.state.write() = set_chain_slot_bpm(st, pos, new_bpm);
                    }
                    if use_bpm {
                        let mut b = cur.bpm.unwrap_or(120.0);
                        if ui
                            .add(egui::DragValue::new(&mut b).range(40.0..=300.0).speed(0.5))
                            .changed()
                        {
                            let st = app.state.read().clone();
                            *app.state.write() = set_chain_slot_bpm(st, pos, Some(b));
                        }
                    }
                });

                ui.separator();
                if ui
                    .button(
                        egui::RichText::new("Clear all overrides")
                            .monospace()
                            .size(9.0),
                    )
                    .clicked()
                {
                    let st = app.state.read().clone();
                    *app.state.write() = clear_chain_slot_override(st, pos);
                }
            });
        if !open {
            ui.ctx().data_mut(|d| d.remove::<usize>(popover_key));
        }
    }
}
