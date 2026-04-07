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
pub(super) fn colorize_log(text: &str, default_color: egui::Color32) -> egui::text::LayoutJob {
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
                    let has_octave = j < len && bytes[j].is_ascii_digit();
                    if has_octave {
                        j += 1;
                    }
                    // For bare notes (no accidental, no octave), require safe punctuation
                    // on both sides — prevents "D" in "D&B", "E" in "E-flat", etc.
                    let bare = acc.is_none() && !has_octave;
                    if bare {
                        let prev_char = if pos > 0 { bytes[pos - 1] } else { b' ' };
                        let safe_before = matches!(
                            prev_char,
                            b' ' | b'\t'
                                | b'\n'
                                | b'\r'
                                | b'('
                                | b'['
                                | b'{'
                                | b'"'
                                | b'\''
                                | b'`'
                                | b'/'
                        );
                        let next_char = if j < len { bytes[j] } else { b' ' };
                        let safe_after = matches!(
                            next_char,
                            b' ' | b'\t'
                                | b'\n'
                                | b'\r'
                                | b')'
                                | b']'
                                | b'}'
                                | b','
                                | b'.'
                                | b':'
                                | b';'
                                | b'"'
                                | b'\''
                                | b'`'
                                | b'/'
                        );
                        if !safe_before || !safe_after {
                            pos += 1;
                            continue;
                        }
                        // Extend span to cover "A minor", "G major", etc.
                        if next_char == b' ' && j < len {
                            const QUALITIES: &[&[u8]] = &[
                                b"major",
                                b"minor",
                                b"maj",
                                b"min",
                                b"diminished",
                                b"dim",
                                b"augmented",
                                b"aug",
                                b"sus",
                                b"suspended",
                                b"dorian",
                                b"phrygian",
                                b"lydian",
                                b"mixolydian",
                                b"locrian",
                                b"aeolian",
                                b"melodic",
                                b"harmonic",
                                b"pentatonic",
                                b"chromatic",
                            ];
                            let rest = &bytes[j + 1..];
                            for q in QUALITIES {
                                let qlen = q.len();
                                if rest.len() >= qlen {
                                    let matches_ci = rest[..qlen]
                                        .iter()
                                        .zip(*q)
                                        .all(|(a, b)| a.to_ascii_lowercase() == *b);
                                    let word_end =
                                        rest.len() == qlen || !rest[qlen].is_ascii_alphabetic();
                                    if matches_ci && word_end {
                                        j += 1 + qlen;
                                        break;
                                    }
                                }
                            }
                        }
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
    /// Drain the audio capture buffer, run analysis, and fire a one-shot LLM
    /// prompt with the results. No-op if no audio has been captured yet.
    pub(crate) fn trigger_listen(&mut self) {
        use crate::audio::analysis::{analyse_audio, format_snapshot};
        let mut captured: Vec<f32> = Vec::with_capacity(441_000);
        while let Ok(s) = self.capture_rx.pop() {
            captured.push(s);
        }
        if !captured.is_empty() {
            let analysis = analyse_audio(&captured, 44100.0);
            let snapshot = format_snapshot(&analysis);
            let prompt = format!(
                "{}\nYou are listening to the audio you just produced. React — correct any mix or arrangement issues. Respond in JSON.",
                snapshot
            );
            self.log_text.push_str("LISTEN → analysing…\n");
            let _ = self.llm_tx.try_send(LlmInput::Infer {
                prompt,
                one_shot: true,
                agent_id: None,
            });
            self.audio_analysis = Some(analysis);
            self.listen_pending = true;
        } else {
            self.log_text.push_str("LISTEN → no audio captured yet\n");
        }
    }

    /// LLM console content — rendered inside a rackable module card.
    /// Contains: style selector, instructions, log, JAM timing, LISTEN, prompt input.
    fn apply_style_selection(&mut self, maybe_id: Option<String>) {
        match maybe_id {
            None => {
                self.state.write().llm.active_style = None;
                self.log_text.push_str("Style cleared\n");
            }
            Some(ref id) if id == "__free__" => {
                self.state.write().llm.active_style = Some(id.clone());
                self.log_text.push_str("Style → Free\n");
                let _ = self.llm_tx.try_send(LlmInput::Infer {
                    prompt: "we're going free — be creative and unpredictable, surprise me".into(),
                    one_shot: true,
                    agent_id: None,
                });
            }
            Some(ref id) if id == "__custom__" => {
                self.state.write().llm.active_style = Some(id.clone());
                self.log_text.push_str("Style → Custom\n");
            }
            Some(id) => {
                let catalog = StyleCatalog::get();
                let (name, baseline) = catalog
                    .find_by_id(&id)
                    .map(|s| (s.name.clone(), s.baseline_params.clone()))
                    .unwrap_or_default();
                if let Some(ref bp) = baseline {
                    let current = self.state.read().clone();
                    let (ramped, remainder) =
                        crate::state::jam_tools::schedule_baseline_ramps(current, bp, 8.0);
                    let next = if remainder.as_object().is_some_and(|o| !o.is_empty()) {
                        apply_llm_update(ramped, &remainder, &[])
                    } else {
                        ramped
                    };
                    *self.state.write() = next;
                }
                self.state.write().llm.active_style = Some(id);
                let _ = self.llm_tx.try_send(LlmInput::ResetContext);
                self.log_text
                    .push_str(&format!("Style → {} (reset)\n", name));
                let _ = self.llm_tx.try_send(LlmInput::Infer {
                    prompt: format!(
                        "FULL RESET to {} — generate all parameters from scratch.",
                        name
                    ),
                    one_shot: true,
                    agent_id: None,
                });
            }
        }
    }

    pub(super) fn draw_llm_console_content(&mut self, ui: &mut egui::Ui) {
        // Override the centered layout from module_card — console needs full-width
        // text fields, log area, and prompt input that fill the card.
        ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
            self.draw_llm_console_inner(ui);
        });
    }

    fn draw_llm_console_inner(&mut self, ui: &mut egui::Ui) {
        // ── Model selector + context bar ─────────────────────────────
        ui.horizontal(|ui| {
            // Model dropdown (global default)
            if self.available_models.is_empty() {
                self.available_models = super::scan_models();
            }
            let (cur_model, ctx_used, ctx_max, is_mock) = {
                let s = self.state.read();
                (
                    s.llm.model_path.clone(),
                    s.llm.context_used,
                    s.llm.context_max,
                    s.llm.is_mock,
                )
            };
            let cur_short = std::path::Path::new(&cur_model)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown")
                .to_string();
            ui.label(
                egui::RichText::new("MODEL")
                    .monospace()
                    .size(8.0)
                    .color(theme::ASH),
            );
            egui::ComboBox::from_id_source("console_model")
                .selected_text(
                    egui::RichText::new(&cur_short)
                        .monospace()
                        .size(8.5)
                        .color(theme::SMOKE),
                )
                .width(180.0)
                .show_ui(ui, |ui| {
                    for path in &self.available_models.clone() {
                        let short = std::path::Path::new(path)
                            .file_stem()
                            .and_then(|s| s.to_str())
                            .unwrap_or(path)
                            .to_string();
                        let selected = *path == cur_model;
                        if ui
                            .selectable_label(
                                selected,
                                egui::RichText::new(&short)
                                    .monospace()
                                    .size(9.0)
                                    .color(if selected { theme::CHALK } else { theme::FOG }),
                            )
                            .clicked()
                            && !selected
                        {
                            let _ = self.llm_tx.try_send(LlmInput::SwitchModel(path.clone()));
                            self.state.write().llm.llm_initializing = true;
                        }
                    }
                });
            ui.separator();
            // Context bar
            let pct = if ctx_max > 0 {
                ctx_used as f32 / ctx_max as f32
            } else {
                0.0
            };
            ui.label(
                egui::RichText::new(format!("CTX {:.0}%", pct * 100.0))
                    .monospace()
                    .size(8.0)
                    .color(if pct > 0.85 { theme::FOG } else { theme::ASH }),
            );
            let (bar_rect, _) = ui.allocate_exact_size(egui::vec2(80.0, 6.0), egui::Sense::hover());
            let p = ui.painter();
            p.rect_filled(bar_rect, 1.0, egui::Color32::from_gray(38));
            let fill_w = (bar_rect.width() * pct.clamp(0.0, 1.0)).max(0.0);
            if fill_w > 0.0 {
                p.rect_filled(
                    egui::Rect::from_min_size(bar_rect.min, egui::vec2(fill_w, bar_rect.height())),
                    1.0,
                    egui::Color32::from_gray(if pct > 0.85 { 160 } else { 90 }),
                );
            }
            // Reset button
            if ui
                .add(
                    egui::Button::new(
                        egui::RichText::new("RST")
                            .monospace()
                            .size(7.5)
                            .color(theme::IRON),
                    )
                    .fill(egui::Color32::TRANSPARENT),
                )
                .on_hover_text("Reset context (restart server)")
                .clicked()
            {
                let _ = self.llm_tx.try_send(LlmInput::ResetContext);
            }
            if is_mock {
                ui.label(
                    egui::RichText::new("MOCK")
                        .monospace()
                        .size(8.0)
                        .color(theme::IRON),
                );
            }
        });

        // ── Style selector + instructions ────────────────────────────
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
                None => "None",
                Some("__free__") => "Free",
                Some("__custom__") => "Custom",
                Some(id) => catalog
                    .find_by_id(id)
                    .map(|s| s.name.as_str())
                    .unwrap_or("None"),
            };
            let mut new_sel: Option<Option<String>> = None;
            egui::ComboBox::from_id_source(ui.id().with("console_style"))
                .selected_text(egui::RichText::new(cur_name).monospace().size(9.5))
                .width(140.0)
                .show_ui(ui, |ui| {
                    if ui
                        .selectable_label(
                            cur_style.is_none(),
                            egui::RichText::new("None").monospace().size(9.5),
                        )
                        .clicked()
                    {
                        new_sel = Some(None);
                    }
                    if ui
                        .selectable_label(
                            cur_style.as_deref() == Some("__free__"),
                            egui::RichText::new("Free").monospace().size(9.5),
                        )
                        .clicked()
                    {
                        new_sel = Some(Some("__free__".into()));
                    }
                    if ui
                        .selectable_label(
                            cur_style.as_deref() == Some("__custom__"),
                            egui::RichText::new("Custom...").monospace().size(9.5),
                        )
                        .clicked()
                    {
                        new_sel = Some(Some("__custom__".into()));
                    }
                    ui.separator();
                    for s in catalog.styles() {
                        if ui
                            .selectable_label(
                                cur_style.as_deref() == Some(s.id.as_str()),
                                egui::RichText::new(&s.name).monospace().size(9.5),
                            )
                            .clicked()
                        {
                            new_sel = Some(Some(s.id.clone()));
                        }
                    }
                });
            if let Some(maybe_id) = new_sel {
                self.apply_style_selection(maybe_id);
            }
            ui.separator();
            // Instructions
            let mut instr = self.state.read().llm.user_instructions.clone();
            if ui
                .add(
                    egui::TextEdit::singleline(&mut instr)
                        .hint_text("persistent instructions…")
                        .desired_width(ui.available_width())
                        .font(egui::FontId::monospace(9.5)),
                )
                .changed()
            {
                self.state.write().llm.user_instructions = instr;
            }
        });

        // ── JAM timing row ───────────────────────────────────────────
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("JAM")
                    .monospace()
                    .size(8.5)
                    .color(theme::ASH),
            );
            let (jam_bars, cycle_count, _bpm, is_inferring, active_ramps, tps) = {
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
            for (label, bars) in &[
                ("CONT", 0.0f32),
                ("1", 1.0),
                ("2", 2.0),
                ("4", 4.0),
                ("8", 8.0),
            ] {
                let active = (jam_bars - bars).abs() < 0.01;
                let col = if active { theme::FOG } else { theme::SMOKE };
                if ui
                    .add(
                        egui::Button::new(
                            egui::RichText::new(*label).monospace().size(8.0).color(col),
                        )
                        .fill(egui::Color32::TRANSPARENT)
                        .min_size(egui::vec2(0.0, 12.0)),
                    )
                    .clicked()
                {
                    self.state.write().llm.jam_bars = *bars;
                }
            }
            ui.add_space(6.0);
            ui.label(
                egui::RichText::new(format!("#{}", cycle_count))
                    .monospace()
                    .size(8.0)
                    .color(theme::IRON),
            );
            if is_inferring {
                ui.label(egui::RichText::new("▶").size(8.0).color(theme::FOG))
                    .on_hover_text(format!("{:.1} tok/s", tps));
            } else if tps > 0.0 {
                ui.label(
                    egui::RichText::new(format!("{:.1}t/s", tps))
                        .monospace()
                        .size(7.5)
                        .color(theme::IRON),
                );
            }
            if active_ramps > 0 {
                ui.label(
                    egui::RichText::new(format!("~{}", active_ramps))
                        .monospace()
                        .size(7.5)
                        .color(theme::IRON),
                );
            }
            if let Some((fire_at, _)) = self.jam_next_fire {
                let remaining = fire_at.duration_since(std::time::Instant::now());
                ui.label(
                    egui::RichText::new(format!("in {:.1}s", remaining.as_secs_f32()))
                        .monospace()
                        .size(7.5)
                        .color(theme::ASH),
                );
            }
        });

        // ── Prompt input + ASK ───────────────────────────────────────
        ui.horizontal(|ui| {
            let avail = ui.available_width();
            let prompt_w = avail - 40.0;
            let response = ui.add(
                egui::TextEdit::singleline(&mut self.prompt_input)
                    .hint_text("prompt the model…")
                    .desired_width(prompt_w)
                    .font(egui::FontId::monospace(9.5)),
            );
            let submit = ui
                .button(egui::RichText::new("↵").monospace().size(10.0))
                .clicked();

            // Enter (without Shift) submits; trim the trailing newline first.
            let enter_pressed = response.has_focus()
                && ui.input(|i| i.key_pressed(egui::Key::Enter) && !i.modifiers.shift);
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
                        None => {
                            "do something interesting — evolve the pattern and sound".to_string()
                        }
                    };
                    (p, "YOU → [evolve]\n".to_string())
                } else {
                    let lower = typed.to_lowercase();
                    let catalog = StyleCatalog::get();
                    if let Some(matched) = catalog.styles().iter().find(|s| {
                        s.keywords
                            .iter()
                            .any(|kw| lower.contains(&kw.to_lowercase()))
                    }) {
                        self.state.write().llm.active_style = Some(matched.id.clone());
                        self.log_text
                            .push_str(&format!("Style → {}\n", matched.name));
                    }
                    (typed.clone(), format!("YOU → {}\n", typed))
                };
                self.log_text.push_str(&log_line);
                // Broadcast to all enabled agents so the full team responds.
                let enabled_agents: Vec<u32> = {
                    let s = self.state.read();
                    s.llm_agents
                        .iter()
                        .filter(|a| s.rack.modules.iter().any(|m| m.id == a.id && m.enabled))
                        .map(|a| a.id)
                        .collect()
                };
                if enabled_agents.is_empty() {
                    // No agents — fall back to singleton
                    let _ = self.llm_tx.try_send(LlmInput::Infer {
                        prompt,
                        one_shot: true,
                        agent_id: None,
                    });
                } else {
                    for aid in enabled_agents {
                        let _ = self.llm_tx.try_send(LlmInput::Infer {
                            prompt: prompt.clone(),
                            one_shot: true,
                            agent_id: Some(aid),
                        });
                    }
                }
                self.prompt_input.clear();
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::colorize_log;
    use crate::ui::theme;

    /// Collect all distinct colors used in the LayoutJob (excluding the default FOG).
    fn colored_spans(text: &str) -> Vec<(String, egui::Color32)> {
        let job = colorize_log(text, theme::FOG);
        job.sections
            .iter()
            .filter(|s| s.format.color != theme::FOG && s.format.color != theme::SMOKE)
            .map(|s| {
                let range = s.byte_range.clone();
                (text[range].to_string(), s.format.color)
            })
            .collect()
    }

    fn has_note_color(text: &str) -> bool {
        let job = colorize_log(text, theme::FOG);
        job.sections
            .iter()
            .any(|s| theme::NOTE_COLORS.iter().any(|&nc| nc == s.format.color))
    }

    #[test]
    fn dnb_not_colored() {
        assert!(
            !has_note_color("D&B is a genre"),
            "D in D&B must not be colored"
        );
        assert!(
            !has_note_color("listen to D&B"),
            "D in D&B must not be colored"
        );
    }

    #[test]
    fn e_flat_not_colored() {
        assert!(!has_note_color("E-flat"), "E in E-flat must not be colored");
    }

    #[test]
    fn note_with_accidental_is_colored() {
        assert!(has_note_color("play D#3"), "D#3 should be colored");
        assert!(has_note_color("root is Gb"), "Gb should be colored");
    }

    #[test]
    fn note_with_octave_is_colored() {
        assert!(has_note_color("note C4"), "C4 should be colored");
    }

    #[test]
    fn bare_note_at_word_boundary_is_colored() {
        assert!(
            has_note_color("root note is G"),
            "bare G at end should be colored"
        );
        assert!(
            has_note_color("key of D major"),
            "D in D major should be colored"
        );
    }

    #[test]
    fn quality_expression_colored_as_one_span() {
        let spans = colored_spans("key of A minor");
        assert_eq!(spans.len(), 1, "A minor should be one colored span");
        assert_eq!(spans[0].0, "A minor");
    }

    #[test]
    fn bare_note_before_punctuation_is_colored() {
        assert!(
            has_note_color("chord: G,"),
            "G before comma should be colored"
        );
        assert!(has_note_color("(G)"), "G in parens should be colored");
    }

    #[test]
    fn hz_is_colored() {
        assert!(has_note_color("440 Hz"), "440 Hz should be colored");
        assert!(has_note_color("440Hz"), "440Hz should be colored");
    }

    #[test]
    fn midi_context_is_colored() {
        assert!(has_note_color("note 60"), "note 60 should be colored");
        assert!(has_note_color("midi 69"), "midi 69 should be colored");
    }
}
