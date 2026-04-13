// ─── ui/panels/tts.rs ─────────────────────────────────────────────────────────
// Rack module panel for NeuTts voice cards.
// Settings are per-module (stored in AppState.tts_modules).
//
// The panel can trigger speech two ways:
//   1. SAY — user types a line, we synthesise it immediately through
//      NeuTTS using this module's voice_ref / temp / top_k / top_p.
//   2. THEME / RHYME / SING — send a prompt to whichever agent
//      controls this TTS module (via control cable) and let it emit
//      an mc_line; the existing LLM → TTS pipeline handles playback.

use crate::llm::LlmInput;
use crate::state::TtsModuleState;
use crate::ui::{ImpulseApp, theme, widgets};

/// Mutate the TtsModuleState for `mid` inside `app`.
fn with_tts(app: &mut ImpulseApp, mid: u32, f: impl FnOnce(&mut TtsModuleState)) {
    if let Some(t) = app
        .state
        .write()
        .tts_modules
        .iter_mut()
        .find(|t| t.id == mid)
    {
        f(t);
    }
}

/// Find the agent that controls this TTS module (the source of the
/// Control-port cable terminating at the module).  Returns the agent's
/// persona name if exactly one controller exists — the routing target
/// used by `/api/prompt` / `LlmInput::Infer` dispatches.
fn controlling_agent(app: &ImpulseApp, tts_module_id: u32) -> Option<(u32, String)> {
    let s = app.state.read();
    let source_ids: Vec<u32> = s
        .rack
        .cables
        .iter()
        .filter(|c| {
            c.to.module_id == tts_module_id && c.from.kind == crate::state::PortKind::Control
        })
        .map(|c| c.from.module_id)
        .collect();
    for sid in source_ids {
        if let Some(a) = s.llm_agents.iter().find(|a| a.id == sid) {
            return Some((a.id, a.persona_name.clone()));
        }
    }
    None
}

/// Send a one-shot inference prompt to the agent controlling this TTS
/// module.  The agent's MC/DJ conversation mode causes it to emit an
/// mc_line which the LLM thread routes back through NeuTTS.
fn ask_controller(app: &ImpulseApp, tts_module_id: u32, prompt: &str) {
    let Some((agent_id, _)) = controlling_agent(app, tts_module_id) else {
        return;
    };
    let _ = app.llm_tx.try_send(LlmInput::Infer {
        prompt: prompt.to_string(),
        one_shot: true,
        agent_id: Some(agent_id),
    });
}

/// Load the first line of voices/<name>.txt as a transcript preview.
/// Empty string if missing or unreadable.
fn read_voice_transcript(voice_ref: &str) -> String {
    if voice_ref.is_empty() {
        return String::new();
    }
    let path = format!("voices/{}.txt", voice_ref);
    std::fs::read_to_string(&path)
        .ok()
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

/// Read the current style's themes and mc_lines (if any) from
/// StyleCatalog so the ask-controller prompts can name them
/// specifically.  Empty vec if no active style or no themes defined.
fn active_style_themes(app: &ImpulseApp) -> Vec<String> {
    let s = app.state.read();
    let Some(ref id) = s.llm.active_style else {
        return vec![];
    };
    if id == "__custom__" || id == "__free__" {
        return vec![];
    }
    crate::llm::styles::StyleCatalog::get()
        .find_by_id(id)
        .map(|st| st.themes.clone())
        .unwrap_or_default()
}

pub fn draw_tts(app: &mut ImpulseApp, ui: &mut egui::Ui, module_id: u32) {
    // ── Ensure module state exists ──────────────────────────────────────────
    {
        let s = app.state.read();
        if !s.tts_modules.iter().any(|t| t.id == module_id) {
            drop(s);
            let tts = TtsModuleState::new(module_id);
            app.state.write().tts_modules.push(tts);
        }
    }

    // ── Snapshot ─────────────────────────────────────────────────────────────
    let (voice_ref, temperature, top_k, top_p, pitch_snap, enabled, tts_state) = {
        let s = app.state.read();
        let t = s.tts_modules.iter().find(|t| t.id == module_id).unwrap();
        let mod_enabled = s
            .rack
            .modules
            .iter()
            .find(|m| m.id == module_id)
            .map(|m| m.enabled)
            .unwrap_or(false);
        (
            t.voice_ref.clone(),
            t.temperature,
            t.top_k,
            t.top_p,
            t.pitch_snap,
            mod_enabled,
            t.clone(),
        )
    };

    if !enabled {
        ui.add_space(2.0);
        ui.label(
            egui::RichText::new("Module disabled")
                .monospace()
                .size(7.5)
                .color(theme::PIT),
        );
        return;
    }

    ui.add_space(3.0);
    widgets::section_header(ui, "NeuTTS Air");

    let small_label = |text: &str| {
        egui::RichText::new(text)
            .monospace()
            .size(8.0)
            .color(theme::SMOKE)
    };

    // ── Voice selector ──────────────────────────────────────────────────────
    let voices: Vec<String> = std::fs::read_dir("voices")
        .ok()
        .map(|entries| {
            let mut v: Vec<String> = entries
                .filter_map(|e| e.ok())
                .filter_map(|e| {
                    let n = e.file_name().to_string_lossy().to_string();
                    n.strip_suffix(".wav").map(|s| s.to_string())
                })
                .collect();
            v.sort();
            v
        })
        .unwrap_or_default();
    ui.horizontal(|ui| {
        ui.label(small_label("VOICE"));
        for name in &voices {
            let active = *name == voice_ref;
            let col = if active { theme::CHALK } else { theme::IRON };
            let fill = if active {
                egui::Color32::from_gray(50)
            } else {
                egui::Color32::TRANSPARENT
            };
            if ui
                .add(
                    egui::Button::new(
                        egui::RichText::new(name.to_uppercase())
                            .monospace()
                            .size(7.5)
                            .color(col),
                    )
                    .fill(fill)
                    .min_size(egui::vec2(0.0, 14.0)),
                )
                .clicked()
            {
                let val = name.clone();
                with_tts(app, module_id, |t| t.voice_ref = val);
            }
        }
        if voices.is_empty() {
            ui.label(
                egui::RichText::new("No voices in voices/")
                    .monospace()
                    .size(7.0)
                    .color(theme::PIT),
            );
            if ui
                .small_button(egui::RichText::new("GET").monospace().size(7.0))
                .on_hover_text(
                    "Open archive.org/details/librivoxaudio\n\
                     Download a short clean WAV (3–15s of one speaker, no music)\n\
                     and drop it into voices/ alongside a .txt transcript.",
                )
                .clicked()
            {
                let _ =
                    crate::ui::util::webbrowser_open("https://archive.org/details/librivoxaudio");
            }
        }
    });

    // ── Conditioning preview — transcript of the selected voice ─────────────
    let transcript = read_voice_transcript(&voice_ref);
    if !transcript.is_empty() {
        let preview: String = transcript.chars().take(110).collect();
        let trailing = if transcript.len() > 110 { "…" } else { "" };
        ui.label(
            egui::RichText::new(format!("“{}{}”", preview, trailing))
                .italics()
                .monospace()
                .size(7.0)
                .color(theme::ASH),
        );
    }

    ui.add_space(2.0);
    widgets::section_header(ui, "Speak");

    // ── Direct user input + SAY ──────────────────────────────────────────────
    // Per-module text buffer stored in egui memory — doesn't need to persist
    // across sessions and we don't want to pollute TtsModuleState with UI
    // state.  Key by module_id so each TTS module has its own line.
    let mem_id = egui::Id::new(("tts_say_input", module_id));
    let mut line: String = ui
        .ctx()
        .data(|d| d.get_temp::<String>(mem_id))
        .unwrap_or_default();
    let mut input_changed = false;
    ui.horizontal(|ui| {
        let resp = ui.add(
            egui::TextEdit::singleline(&mut line)
                .hint_text("type something to speak…")
                .desired_width(ui.available_width() - 36.0)
                .font(egui::FontId::monospace(8.0)),
        );
        input_changed = resp.changed() || resp.lost_focus();
        let send =
            resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && !line.is_empty();
        let clicked = ui
            .small_button(egui::RichText::new("SAY").monospace().size(7.5))
            .on_hover_text("Synthesise this line immediately through NeuTTS")
            .clicked()
            && !line.is_empty();
        if send || clicked {
            crate::llm::speak_neutts(&line, &tts_state, &app.tts_tx);
        }
    });
    if input_changed {
        ui.ctx().data_mut(|d| d.insert_temp(mem_id, line.clone()));
    }

    // ── Ask-controller buttons ──────────────────────────────────────────────
    // Pull the controlling agent + active-style themes so the prompts can
    // nudge the agent toward on-theme lines.
    let controller = controlling_agent(app, module_id);
    let themes = active_style_themes(app);
    let theme_hint = if themes.is_empty() {
        String::new()
    } else {
        format!(" Themes: {}.", themes.join(", "))
    };

    ui.horizontal(|ui| {
        ui.label(small_label("ASK"));
        let has_controller = controller.is_some();
        ui.add_enabled_ui(has_controller, |ui| {
            if ui
                .add(
                    egui::Button::new(egui::RichText::new("THEME").monospace().size(7.5))
                        .min_size(egui::vec2(0.0, 14.0)),
                )
                .on_hover_text(
                    "Ask the controlling agent for a short on-theme shout.\n\
                     The agent's mc_line is auto-played through this TTS.",
                )
                .clicked()
            {
                ask_controller(
                    app,
                    module_id,
                    &format!(
                        "Drop a single short on-theme shout-out, one line, peak energy.{}",
                        theme_hint
                    ),
                );
            }
            if ui
                .add(
                    egui::Button::new(egui::RichText::new("RHYME").monospace().size(7.5))
                        .min_size(egui::vec2(0.0, 14.0)),
                )
                .on_hover_text("Ask the controlling agent for a rhyming couplet on theme")
                .clicked()
            {
                ask_controller(
                    app,
                    module_id,
                    &format!(
                        "Drop a single short rhyming couplet, peak-time energy, on theme.{}",
                        theme_hint
                    ),
                );
            }
            if ui
                .add(
                    egui::Button::new(egui::RichText::new("SING").monospace().size(7.5))
                        .min_size(egui::vec2(0.0, 14.0)),
                )
                .on_hover_text("Ask the controlling agent for a short singable hook")
                .clicked()
            {
                ask_controller(
                    app,
                    module_id,
                    &format!(
                        "Drop a single short singable hook line, melodic, on theme.{}",
                        theme_hint
                    ),
                );
            }
        });
    });
    if let Some((_, ref persona)) = controller {
        ui.label(
            egui::RichText::new(format!("controller: {}", persona))
                .monospace()
                .size(7.0)
                .color(theme::ASH),
        );
    } else {
        ui.label(
            egui::RichText::new("No controlling agent — wire a control cable from an MC agent")
                .monospace()
                .size(7.0)
                .color(theme::PIT),
        );
    }

    ui.add_space(2.0);
    widgets::section_header(ui, "Synth");

    // ── Temperature ─────────────────────────────────────────────────────────
    ui.horizontal(|ui| {
        ui.label(small_label("TEMP"));
        let mut v = temperature;
        if ui
            .add(egui::DragValue::new(&mut v).range(0.1..=2.0).speed(0.01))
            .changed()
        {
            with_tts(app, module_id, |t| t.temperature = v);
        }
    });

    // ── Top-K ───────────────────────────────────────────────────────────────
    ui.horizontal(|ui| {
        ui.label(small_label("TOP-K"));
        let mut v = top_k as i32;
        if ui
            .add(egui::DragValue::new(&mut v).range(1..=200).speed(1))
            .changed()
        {
            let val = v as u16;
            with_tts(app, module_id, |t| t.top_k = val);
        }
    });

    // ── Top-P ───────────────────────────────────────────────────────────────
    ui.horizontal(|ui| {
        ui.label(small_label("TOP-P"));
        let mut v = top_p;
        if ui
            .add(egui::DragValue::new(&mut v).range(0.1..=1.0).speed(0.01))
            .changed()
        {
            with_tts(app, module_id, |t| t.top_p = v);
        }
    });

    ui.add_space(3.0);
    widgets::section_header(ui, "FX");

    // ── Pitch snap ──────────────────────────────────────────────────────────
    ui.horizontal(|ui| {
        ui.label(small_label("PITCH SNAP"));
        let mut ps = pitch_snap;
        if widgets::toggle_button(ui, if ps { "ON" } else { "OFF" }, &mut ps) {
            with_tts(app, module_id, |t| t.pitch_snap = ps);
        }
    });
    ui.label(
        egui::RichText::new("snap voice to nearest in-key note")
            .monospace()
            .size(7.0)
            .color(theme::PIT),
    );
}
