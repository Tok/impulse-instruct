// ─── ui/llm_strip.rs ──────────────────────────────────────────────────────────
// LLM interaction strip: style selector, instructions, prompt input, log.
//
// Layout (resizable panel, drag the bottom border):
//   TOP row  — LEFT: style + instructions  |  RIGHT: log (fills height)
//   BOTTOM row — full-width prompt input + ASK button (vertically centred)

use crate::llm::LlmInput;
use crate::llm::styles::StyleCatalog;
use crate::state::apply_llm_update;
use crate::ui::{ImpulseApp, theme};
use egui::{Frame, TopBottomPanel};

// ─── Huth note colorizer ──────────────────────────────────────────────────────

/// Parse `text` and return a LayoutJob where note references are colored with
/// Huth *Farbige Noten* colors.
///
/// Recognized patterns:
/// • Note+octave: `C4`, `A#3`, `Bb2` etc.  (`[A-G][#b]?\d`)
/// • Plain note name at a word boundary: `C`, `G#`, `Bb` etc.
///   (only when the note letter is NOT surrounded by other letters)
/// • Frequency: `440Hz`, `261.6 Hz` etc. — mapped to nearest chromatic semitone
/// • MIDI number context: `note 60`, `midi 72`, `pitch 48`
fn colorize_log(text: &str, default_color: egui::Color32) -> egui::text::LayoutJob {
    use egui::text::{LayoutJob, TextFormat};

    let mut job = LayoutJob::default();
    let font = egui::FontId::monospace(9.0);
    // Thinking lines render in SMOKE (darker than default FOG) to visually separate them.
    let think_color = theme::SMOKE;

    // Returns the base color for the line starting at byte offset `p`.
    let line_color_at = |p: usize, bytes: &[u8], text: &str| -> egui::Color32 {
        let end = bytes[p..]
            .iter()
            .position(|&b| b == b'\n')
            .map(|i| p + i)
            .unwrap_or(bytes.len());
        if text[p..end].contains("(think):") {
            think_color
        } else {
            default_color
        }
    };

    // Semitone index 0..12 for a note letter + optional accidental.
    let note_semitone = |c: char, acc: Option<char>| -> Option<u8> {
        let base: i8 = match c {
            'C' => 0,
            'D' => 2,
            'E' => 4,
            'F' => 5,
            'G' => 7,
            'A' => 9,
            'B' => 11,
            _ => return None,
        };
        let offset: i8 = match acc {
            Some('#') => 1,
            Some('b') => -1,
            _ => 0,
        };
        Some(((base + offset).rem_euclid(12)) as u8)
    };

    // Hz → chromatic semitone 0..12
    let freq_semitone = |hz: f64| -> u8 {
        let midi = 69.0 + 12.0 * (hz / 440.0_f64).log2();
        (midi.round() as i64).rem_euclid(12) as u8
    };

    // MIDI note number → chromatic semitone 0..12
    let midi_semitone = |n: u8| -> u8 { n % 12 };

    let bytes = text.as_bytes();
    let len = bytes.len();
    let mut pos = 0usize; // current byte offset
    let mut seg = 0usize; // start of pending plain segment
    // Base color for the current line (updated on each newline).
    let mut cur_line_color = line_color_at(0, bytes, text);

    // Flush a plain segment from `seg` to `end` using the current line color.
    macro_rules! flush {
        ($end:expr) => {
            if seg < $end {
                job.append(
                    &text[seg..$end],
                    0.0,
                    TextFormat {
                        font_id: font.clone(),
                        color: cur_line_color,
                        ..Default::default()
                    },
                );
                seg = $end;
            }
        };
    }

    // Append a colored span (Huth color), preceded by any pending plain segment.
    // Inlines the flush to avoid a seg write that would be immediately overwritten.
    macro_rules! colored {
        ($start:expr, $end:expr, $semitone:expr) => {
            if seg < $start {
                job.append(
                    &text[seg..$start],
                    0.0,
                    TextFormat {
                        font_id: font.clone(),
                        color: cur_line_color,
                        ..Default::default()
                    },
                );
            }
            job.append(
                &text[$start..$end],
                0.0,
                TextFormat {
                    font_id: font.clone(),
                    color: theme::NOTE_COLORS[$semitone as usize],
                    ..Default::default()
                },
            );
            seg = $end;
            pos = $end;
        };
    }

    while pos < len {
        let b = bytes[pos];

        // On newline, flush the current line and update color for the next line.
        if b == b'\n' {
            flush!(pos + 1);
            pos += 1;
            if pos < len {
                cur_line_color = line_color_at(pos, bytes, text);
            }
            continue;
        }

        // ── Note name (ASCII A–G) ─────────────────────────────────────────────
        if b.is_ascii_uppercase() && matches!(b, b'A'..=b'G') {
            // Require word-start: pos==0 or previous byte not alphabetic
            let prev_ok = pos == 0 || !bytes[pos - 1].is_ascii_alphabetic();
            if prev_ok {
                let note_char = b as char;
                let mut j = pos + 1;
                // Optional accidental (lowercase b for flat, # for sharp)
                let acc = if j < len && (bytes[j] == b'#' || bytes[j] == b'b') {
                    j += 1;
                    Some(bytes[j - 1] as char)
                } else {
                    None
                };
                // Word-end required: next byte must not be alphabetic (handles CTRL, BPM etc.)
                let next_ok = j >= len || !bytes[j].is_ascii_alphabetic();
                if next_ok && let Some(st) = note_semitone(note_char, acc) {
                    // Optionally consume a trailing octave digit (makes C4, A#3 etc.)
                    if j < len && bytes[j].is_ascii_digit() {
                        j += 1;
                    }
                    colored!(pos, j, st);
                    continue;
                }
            }
        }

        // ── Frequency: digits (optional dot+digits) optionally space then Hz ─
        if b.is_ascii_digit() {
            let mut j = pos;
            while j < len && (bytes[j].is_ascii_digit() || bytes[j] == b'.') {
                j += 1;
            }
            let num_str = &text[pos..j];
            let mut k = j;
            if k < len && bytes[k] == b' ' {
                k += 1;
            }
            if k + 2 <= len
                && bytes[k..k + 2].eq_ignore_ascii_case(b"Hz")
                && let Ok(hz) = num_str.parse::<f64>()
                && (20.0..=20_000.0).contains(&hz)
            {
                let st = freq_semitone(hz);
                colored!(pos, k + 2, st);
                continue;
            }
            // ── MIDI number context: "note 60", "midi 72", "pitch 48" ─────────
            let prefix_end = pos;
            let prefix_start = pos.saturating_sub(7);
            let prefix = &text[prefix_start..prefix_end];
            let is_midi_ctx = ["note ", "midi ", "pitch ", "step "]
                .iter()
                .any(|kw| prefix.ends_with(kw));
            if is_midi_ctx && let Ok(n) = num_str.parse::<u8>() {
                let st = midi_semitone(n);
                colored!(pos, j, st);
                continue;
            }
        }

        pos += 1;
    }
    flush!(len);
    let _ = seg; // last flush writes seg but nothing reads it after
    job
}

impl ImpulseApp {
    /// Style selector, prompt input, log, and thinking display.
    pub(super) fn draw_llm_strip(&mut self, ctx: &egui::Context) {
        // Compact default: style row ~22px + instructions ~22px + prompt ~34px + top margin 4px ≈ 82px.
        // User can drag the bottom border down to reveal more log lines.
        let collapsed = self.llm_strip_collapsed;
        TopBottomPanel::top("llm_strip")
            .frame(Frame::none().fill(theme::PIT).inner_margin(egui::Margin { left: 8.0, right: 8.0, top: 4.0, bottom: 0.0 }))
            .resizable(!collapsed)
            .min_height(if collapsed { 36.0 } else { 70.0 })
            .max_height(if collapsed { 36.0 } else { f32::INFINITY })
            .default_height(if collapsed { 36.0 } else { 95.0 })
            .show(ctx, |ui| {
                // ── Collapse toggle (top-right corner) ───────────────────────
                ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
                    let icon = if collapsed { "▼" } else { "▲" };
                    if ui
                        .add(
                            egui::Button::new(
                                egui::RichText::new(icon).monospace().size(9.0).color(theme::IRON),
                            )
                            .frame(false),
                        )
                        .on_hover_text(if collapsed { "Expand LLM strip" } else { "Collapse LLM strip" })
                        .clicked()
                    {
                        self.llm_strip_collapsed = !self.llm_strip_collapsed;
                    }
                });

                if collapsed {
                    // Collapsed: show only the prompt row
                } else {

                // ── TOP row: style + instructions | log ───────────────────────
                ui.horizontal(|ui| {
                    // ── LEFT column: style + instructions ─────────────────────
                    let left_w = (ui.available_width() * 0.36).clamp(270.0, 420.0);

                    ui.scope(|ui| {
                        ui.set_min_width(left_w);
                        ui.set_max_width(left_w);

                        // Style selector row
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new("STYLE")
                                    .monospace()
                                    .size(9.0)
                                    .color(theme::ASH),
                            );
                            ui.add_space(4.0);

                            let cur_style = self.state.read().llm.active_style.clone();
                            let catalog = StyleCatalog::get();
                            let cur_name = match cur_style.as_deref() {
                                None               => "None",
                                Some("__free__")   => "Free",
                                Some("__custom__") => "Custom",
                                Some(id) => catalog.find_by_id(id)
                                    .map(|s| s.name.as_str())
                                    .unwrap_or("None"),
                            };

                            let mut new_style_selection: Option<Option<String>> = None;

                            egui::ComboBox::from_id_source("style_selector")
                                .selected_text(
                                    egui::RichText::new(cur_name).monospace().size(9.5),
                                )
                                .width(140.0)
                                .show_ui(ui, |ui| {
                                    if ui.selectable_label(
                                        cur_style.is_none(),
                                        egui::RichText::new("None").monospace().size(9.5),
                                    ).clicked() {
                                        new_style_selection = Some(None);
                                    }
                                    if ui.selectable_label(
                                        cur_style.as_deref() == Some("__free__"),
                                        egui::RichText::new("Free").monospace().size(9.5),
                                    ).clicked() {
                                        new_style_selection = Some(Some("__free__".to_string()));
                                    }
                                    if ui.selectable_label(
                                        cur_style.as_deref() == Some("__custom__"),
                                        egui::RichText::new("Custom...").monospace().size(9.5),
                                    ).clicked() {
                                        new_style_selection = Some(Some("__custom__".to_string()));
                                    }
                                    ui.separator();
                                    for style in catalog.styles() {
                                        let active =
                                            cur_style.as_deref() == Some(style.id.as_str());
                                        if ui.selectable_label(
                                            active,
                                            egui::RichText::new(&style.name)
                                                .monospace()
                                                .size(9.5),
                                        ).clicked() {
                                            new_style_selection = Some(Some(style.id.clone()));
                                        }
                                    }
                                });

                            // Inline custom brief when __custom__ is active
                            if cur_style.as_deref() == Some("__custom__") {
                                let mut custom_text =
                                    self.state.read().llm.custom_style_text.clone();
                                let r = ui.add(
                                    egui::TextEdit::singleline(&mut custom_text)
                                        .hint_text("style brief…")
                                        .desired_width(ui.available_width() - 4.0)
                                        .font(egui::FontId::monospace(9.5)),
                                );
                                if r.changed() {
                                    self.state.write().llm.custom_style_text = custom_text.clone();
                                }
                                if r.lost_focus()
                                    && ui.input(|i| i.key_pressed(egui::Key::Enter))
                                    && !custom_text.trim().is_empty()
                                {
                                    self.log_text.push_str("Custom style brief updated\n");
                                    let _ = self.llm_tx.try_send(LlmInput::Infer {
                                        prompt: "apply the active style brief — update sound and rhythm accordingly".to_string(),
                                        one_shot: true,
                                    });
                                }
                            }

                            if let Some(maybe_id) = new_style_selection {
                                match maybe_id {
                                    None => {
                                        self.state.write().llm.active_style = None;
                                        self.log_text.push_str("Style cleared\n");
                                    }
                                    Some(ref id) if id == "__free__" => {
                                        self.state.write().llm.active_style = Some(id.clone());
                                        self.log_text.push_str("Style → Free\n");
                                        let _ = self.llm_tx.try_send(LlmInput::Infer {
                                            prompt: "we're going free — be creative and unpredictable, surprise me".to_string(),
                                            one_shot: true,
                                        });
                                    }
                                    Some(ref id) if id == "__custom__" => {
                                        self.state.write().llm.active_style = Some(id.clone());
                                        self.log_text.push_str("Style → Custom\n");
                                    }
                                    Some(id) => {
                                        let (name, baseline) = catalog.find_by_id(&id)
                                            .map(|s| (s.name.clone(), s.baseline_params.clone()))
                                            .unwrap_or_default();
                                        if let Some(ref bp) = baseline {
                                            let current = self.state.read().clone();
                                            *self.state.write() = apply_llm_update(current, bp);
                                        }
                                        self.state.write().llm.active_style = Some(id);
                                        let _ = self.llm_tx.try_send(LlmInput::ResetContext);
                                        let prompt = format!(
                                            "FULL RESET to {} — generate all parameters from scratch.",
                                            name
                                        );
                                        self.log_text
                                            .push_str(&format!("Style → {} (reset)\n", name));
                                        let _ = self.llm_tx.try_send(LlmInput::Infer {
                                            prompt,
                                            one_shot: true,
                                        });
                                    }
                                }
                            }
                        });

                        // Persistent instructions
                        {
                            let mut instr = self.state.read().llm.user_instructions.clone();
                            let r = ui.add(
                                egui::TextEdit::singleline(&mut instr)
                                    .hint_text("persistent instructions…")
                                    .desired_width(left_w - 4.0)
                                    .font(egui::FontId::monospace(9.5)),
                            );
                            if r.changed() {
                                self.state.write().llm.user_instructions = instr;
                            }
                        }
                    }); // end left scope

                    ui.add_space(4.0);
                    ui.separator();
                    ui.add_space(4.0);

                    // ── RIGHT column: log + thinking ──────────────────────────
                    ui.vertical(|ui| {
                        // Thinking toggle (compact, one line)
                        if let Some(ref thinking) = self.last_thinking.clone() {
                            let think_persona = self.state.read().llm.persona_name.clone();
                            let label = if self.show_thinking {
                                format!("▾ {} (think)", think_persona)
                            } else {
                                format!("▸ {} (think)", think_persona)
                            };
                            let label = label.as_str();
                            if ui
                                .add(
                                    egui::Button::new(
                                        egui::RichText::new(label)
                                            .color(theme::IRON)
                                            .monospace()
                                            .size(8.5),
                                    )
                                    .frame(false),
                                )
                                .clicked()
                            {
                                self.show_thinking = !self.show_thinking;
                            }
                            if self.show_thinking {
                                egui::ScrollArea::vertical()
                                    .id_source("thinking_scroll")
                                    .max_height(30.0)
                                    .auto_shrink([false; 2])
                                    .show(ui, |ui| {
                                        let mut t = thinking.clone();
                                        ui.add(
                                            egui::TextEdit::multiline(&mut t)
                                                .desired_width(f32::INFINITY)
                                                .font(egui::FontId::monospace(8.5))
                                                .text_color(theme::ASH)
                                                .frame(false)
                                                .interactive(false),
                                        );
                                    });
                            }
                        }

                        // Log fills remaining height — colored note refs via Huth palette
                        egui::ScrollArea::vertical()
                            .id_source("log_scroll")
                            .stick_to_bottom(true)
                            .auto_shrink([false; 2])
                            .show(ui, |ui: &mut egui::Ui| {
                                let job = colorize_log(&self.log_text, theme::FOG);
                                ui.add(
                                    egui::Label::new(job)
                                        .selectable(true),
                                );
                            });
                    });
                }); // end top row

                // ── JAM timing row ────────────────────────────────────────────
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("JAM")
                            .monospace()
                            .size(8.5)
                            .color(theme::ASH),
                    );
                    // Interval selector: CONT | 1 | 2 | 4 | 8 bars
                    let (jam_bars, cycle_count, bpm, is_inferring, active_ramps, tps) = {
                        let s = self.state.read();
                        (
                            s.llm.jam_bars,
                            s.llm.jam_cycle_count,
                            s.sequencer.bpm,
                            s.llm.is_inferring,
                            s.llm.active_ramps.len(),
                            s.llm.tokens_per_sec,
                        )
                    };
                    for (label, bars) in &[("CONT", 0.0f32), ("1", 1.0), ("2", 2.0), ("4", 4.0), ("8", 8.0)] {
                        let active = (jam_bars - bars).abs() < 0.01;
                        let col = if active { theme::FOG } else { theme::SMOKE };
                        if ui.add(
                            egui::Button::new(
                                egui::RichText::new(*label).monospace().size(8.0).color(col),
                            )
                            .fill(egui::Color32::TRANSPARENT)
                            .min_size(egui::vec2(0.0, 12.0)),
                        ).on_hover_text(if *bars == 0.0 {
                            "Fire next cycle immediately after inference".to_string()
                        } else {
                            format!("Wait {} bar{} (~{:.0}s) between cycles",
                                bars, if *bars > 1.0 { "s" } else { "" },
                                bars * 240.0 / bpm)
                        }).clicked() {
                            self.state.write().llm.jam_bars = *bars;
                        }
                    }
                    ui.add_space(6.0);
                    // Cycle counter
                    ui.label(
                        egui::RichText::new(format!("#{}", cycle_count))
                            .monospace()
                            .size(8.0)
                            .color(theme::IRON),
                    ).on_hover_text("Total jam cycles completed");
                    // Inference indicator + tokens/sec
                    if is_inferring {
                        ui.label(
                            egui::RichText::new("▶").size(8.0).color(theme::FOG),
                        ).on_hover_text(format!("{:.1} tok/s", tps));
                    } else if tps > 0.0 {
                        ui.label(
                            egui::RichText::new(format!("{:.1}t/s", tps))
                                .monospace()
                                .size(7.5)
                                .color(theme::IRON),
                        ).on_hover_text("Tokens per second (last inference)");
                    }
                    // Active ramps indicator
                    if active_ramps > 0 {
                        ui.label(
                            egui::RichText::new(format!("~{}", active_ramps))
                                .monospace()
                                .size(7.5)
                                .color(theme::IRON),
                        ).on_hover_text(format!("{} active ramp{}", active_ramps, if active_ramps > 1 { "s" } else { "" }));
                    }
                    // Countdown when waiting between cycles
                    if let Some(fire_at) = self.jam_next_fire {
                        let remaining = fire_at.duration_since(std::time::Instant::now());
                        ui.label(
                            egui::RichText::new(format!("in {:.1}s", remaining.as_secs_f32()))
                                .monospace()
                                .size(7.5)
                                .color(theme::ASH),
                        ).on_hover_text("Next jam cycle fires after this delay");
                    }
                });

                // ── LISTEN bar: capture + analysis display ────────────────────
                ui.horizontal(|ui| {
                    // Listen button — drains capture ring buffer, runs analysis
                    if ui.add(
                        egui::Button::new(
                            egui::RichText::new("LISTEN")
                                .monospace()
                                .size(8.5)
                                .color(theme::ASH),
                        )
                        .fill(egui::Color32::TRANSPARENT)
                        .min_size(egui::vec2(0.0, 14.0)),
                    ).clicked() {
                        self.trigger_listen();
                    }

                    // AUTO toggle — re-triggers LISTEN every 4 jam cycles
                    let auto_color = if self.auto_listen { theme::FOG } else { theme::SMOKE };
                    if ui.add(
                        egui::Button::new(
                            egui::RichText::new("AUTO")
                                .monospace()
                                .size(8.0)
                                .color(auto_color),
                        )
                        .fill(egui::Color32::TRANSPARENT)
                        .min_size(egui::vec2(0.0, 14.0)),
                    ).clicked() {
                        self.auto_listen = !self.auto_listen;
                        self.auto_listen_counter = 0;
                    }

                    // Show snapshot stats if available
                    // Per-voice level bars derived from volume params in state
                    {
                        let s = self.state.read();
                        let levels: &[(&str, f32)] = &[
                            ("BAS", s.bass.volume),
                            ("K-A", s.kit_a.kick.volume),
                            ("S-A", s.kit_a.snare.volume),
                            ("HH", s.kit_a.hihat_closed.volume),
                            ("K-B", s.kit_b.kick.volume),
                            ("S-B", s.kit_b.snare.volume),
                            ("CLP", s.kit_b.clap.volume),
                            ("AMN", s.amen.volume),
                        ];
                        ui.add_space(6.0);
                        for (label, vol) in levels {
                            let bar_h = 10.0_f32;
                            let bar_w = 3.0_f32;
                            let fill_h = (vol * bar_h).clamp(0.0, bar_h);
                            let (resp, painter) = ui.allocate_painter(
                                egui::vec2(bar_w + 1.0, bar_h),
                                egui::Sense::hover(),
                            );
                            let rect = resp.rect;
                            // Background
                            painter.rect_filled(rect, 0.0, theme::VOID);
                            // Fill from bottom
                            let fill_rect = egui::Rect::from_min_max(
                                egui::pos2(rect.min.x, rect.max.y - fill_h),
                                rect.max,
                            );
                            painter.rect_filled(fill_rect, 0.0, theme::IRON);
                            resp.on_hover_text(format!("{} {:.0}%", label, vol * 100.0));
                        }
                    }

                    if let Some(ref a) = self.audio_analysis {
                        ui.add_space(4.0);
                        ui.label(
                            egui::RichText::new(format!(
                                "sub {:+.0}  low {:+.0}  mid {:+.0}  hi {:+.0}  pk {:+.1}  crest {:.0}dB  ~{:.0}tr/bar",
                                a.sub_rms_db, a.low_rms_db, a.mid_rms_db, a.high_rms_db,
                                a.peak_db, a.crest_db, a.transients_per_bar
                            ))
                            .monospace()
                            .size(8.0)
                            .color(theme::IRON),
                        );
                    }
                });

                } // end !collapsed block

                // ── BOTTOM row: full-width prompt + Enter (vertically centred) ──
                ui.with_layout(
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                    let avail = ui.available_width();
                    let prompt_w = avail - 50.0;
                    let response = ui.add(
                        egui::TextEdit::multiline(&mut self.prompt_input)
                            .hint_text("prompt the model…")
                            .desired_width(prompt_w)
                            .desired_rows(2)
                            .font(egui::FontId::monospace(11.5)),
                    );
                    let submit = ui
                        .add_sized(
                            [44.0, response.rect.height()],
                            egui::Button::new(
                                egui::RichText::new("↵").monospace().size(14.0),
                            ),
                        )
                        .clicked();

                    // Enter (without Shift) submits; trim the trailing newline first.
                    let enter_pressed = response.has_focus()
                        && ctx.input(|i| {
                            i.key_pressed(egui::Key::Enter) && !i.modifiers.shift
                        });
                    if enter_pressed && self.prompt_input.ends_with('\n') {
                        self.prompt_input.pop();
                    }

                    if submit || enter_pressed {
                        let typed = self.prompt_input.trim().to_string();
                        let (prompt, log_line) = if typed.is_empty() {
                            let active_style = self.state.read().llm.active_style.clone();
                            let p = match active_style.as_deref() {
                                Some(id) => {
                                    let name = StyleCatalog::get()
                                        .find_by_id(id)
                                        .map(|s| s.name.as_str())
                                        .unwrap_or(id);
                                    format!("do something fresh in the {} style", name)
                                }
                                None => "do something interesting — evolve the pattern and sound".to_string(),
                            };
                            (p, "YOU → [evolve]\n".to_string())
                        } else {
                            let lower = typed.to_lowercase();
                            let catalog = StyleCatalog::get();
                            if let Some(matched) = catalog.styles().iter().find(|s| {
                                s.keywords.iter().any(|kw| lower.contains(&kw.to_lowercase()))
                            }) {
                                self.state.write().llm.active_style = Some(matched.id.clone());
                                self.log_text.push_str(&format!("Style → {}\n", matched.name));
                            }
                            (typed.clone(), format!("YOU → {}\n", typed))
                        };
                        self.log_text.push_str(&log_line);
                        let _ = self.llm_tx.try_send(LlmInput::Infer { prompt, one_shot: true });
                        self.prompt_input.clear();
                    }
                    });
            });
    }
}
