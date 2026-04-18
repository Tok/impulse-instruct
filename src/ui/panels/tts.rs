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

use crate::state::TtsModuleState;
use crate::state::tts_types::NEUTTS_PORT;
use crate::ui::{ImpulseApp, theme, widgets};

/// Quick health check of the NeuTTS Python server.  Returns true if
/// GET http://127.0.0.1:NEUTTS_PORT/health answers with 2xx.  Uses a
/// 250ms timeout so a dead server doesn't stall the UI thread.
/// Called infrequently (~1 Hz) — cached on `tts_server_online_at` below.
fn check_neutts_alive() -> bool {
    let url = format!("http://127.0.0.1:{}/health", NEUTTS_PORT);
    ureq::AgentBuilder::new()
        .timeout_connect(std::time::Duration::from_millis(250))
        .timeout(std::time::Duration::from_millis(250))
        .build()
        .get(&url)
        .call()
        .map(|r| (200..300).contains(&r.status()))
        .unwrap_or(false)
}

/// Spawn the NeuTTS Python server in the background.  Uses the optional
/// .neutts-venv python if present (matches demo/lib.sh), else system
/// python3.  The spawned process is detached — we don't track its PID
/// in the UI; the cleanup trap in demo/record-demo.sh or a manual
/// `pkill -f neutts-server.py` handles shutdown.  No-op if spawning
/// fails.
fn spawn_neutts_server() {
    let venv_py = std::path::Path::new(".neutts-venv/bin/python");
    let python = if venv_py.exists() {
        venv_py.to_string_lossy().into_owned()
    } else {
        "python3".to_string()
    };
    let _ = std::process::Command::new(python)
        .arg("scripts/neutts-server.py")
        .arg("--port")
        .arg(NEUTTS_PORT.to_string())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
}

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
fn ask_controller(app: &mut ImpulseApp, tts_module_id: u32, prompt: &str) {
    let Some((agent_id, _)) = controlling_agent(app, tts_module_id) else {
        return;
    };
    app.send_llm_infer(prompt.to_string(), true, Some(agent_id));
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

    // ── Server status + START button ────────────────────────────────────────
    // Poll the NeuTTS /health endpoint at most once per second (keyed by
    // ctx.input().time — shared across all TTS panels drawn this frame).
    // When offline, SAY / ASK and runtime mc_line synthesis all fail
    // silently at the HTTP POST, which is a confusing failure mode
    // ("LLM emitted the line, nothing plays").  Surfacing status +
    // offering a one-click START fixes that.
    let now = ui.ctx().input(|i| i.time);
    let last_check: f64 = ui
        .ctx()
        .data(|d| d.get_temp::<f64>(egui::Id::new("tts_health_check_at")))
        .unwrap_or(-10.0);
    if now - last_check > 1.0 {
        app.neutts_online = check_neutts_alive();
        ui.ctx()
            .data_mut(|d| d.insert_temp(egui::Id::new("tts_health_check_at"), now));
    }
    ui.horizontal(|ui| {
        let (label, color) = if app.neutts_online {
            ("NeuTTS: ONLINE", theme::CHALK)
        } else {
            ("NeuTTS: OFFLINE", theme::PIT)
        };
        ui.label(
            egui::RichText::new(label)
                .monospace()
                .size(7.0)
                .color(color),
        );
        if !app.neutts_online
            && ui
                .small_button(egui::RichText::new("START").monospace().size(7.0))
                .on_hover_text(format!(
                    "Spawn scripts/neutts-server.py on port {NEUTTS_PORT}.\n\
                     Uses .neutts-venv/bin/python if present, else python3.\n\
                     First start takes a few seconds to load the model.",
                ))
                .clicked()
        {
            spawn_neutts_server();
            // Reset the cache so the next frame re-polls immediately
            // once the server comes up.
            ui.ctx()
                .data_mut(|d| d.insert_temp(egui::Id::new("tts_health_check_at"), -10.0_f64));
        }
    });

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
    // Resolve controller now so the empty-SAY fallback can prompt it.
    let controller_for_say = controlling_agent(app, module_id);
    ui.horizontal(|ui| {
        let resp = ui.add(
            egui::TextEdit::singleline(&mut line)
                .hint_text("speak this… (empty + SAY = improvise)")
                .desired_width(ui.available_width() - 36.0)
                .font(egui::FontId::monospace(8.0)),
        );
        input_changed = resp.changed() || resp.lost_focus();
        let enter_pressed = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        let clicked = ui
            .small_button(egui::RichText::new("SAY").monospace().size(7.5))
            .on_hover_text(
                "With text: synthesise it immediately through NeuTTS.\n\
                 Empty: ask the controlling agent to improvise something\n\
                 that fits its persona (rhyme, shout, sung hook, bleep…).",
            )
            .clicked();
        if (enter_pressed || clicked) && !line.is_empty() {
            // Normal path — type a line, synthesise it.
            let say_line = format!("► {}", line); // U+25BA speaking marker
            app.log_text.push_str(&say_line);
            app.log_text.push('\n');
            log::info!("{}", say_line);
            crate::llm::speak_neutts(&line, &tts_state, &app.tts_tx);
        } else if clicked && line.is_empty() {
            // Empty SAY → fall through to the controlling agent.  The
            // prompt leaves the form open ("rhyme / sung hook / shout /
            // bleep") so personas like ROBOT can pick bleep-bloop and a
            // jump-up MC can pick a rave shout.  Themes from the active
            // style land alongside so the line stays in character.
            if let Some((agent_id, _persona)) = controller_for_say.as_ref() {
                let themes = active_style_themes(app);
                let theme_hint = if themes.is_empty() {
                    String::new()
                } else {
                    format!(" Themes: {}.", themes.join(", "))
                };
                let prompt = format!(
                    "Improvise one short line that fits YOUR persona — a rhyme, \
                     a shout, a sung hook, a robotic bleep-bloop, whatever is \
                     most in character.  One line only, peak-time energy.{}",
                    theme_hint
                );
                app.send_llm_infer(prompt, true, Some(*agent_id));
            }
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

    // Fixed label column width so the value widgets below align into a
    // tidy vertical stack.  Eyeballed at 84px — wide enough for the
    // longest label ("TEMPERATURE") at 8pt monospace.
    let label_w: f32 = 84.0;
    let labelled_row = |ui: &mut egui::Ui, text: &str, body: &mut dyn FnMut(&mut egui::Ui)| {
        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(egui::vec2(label_w, 14.0), egui::Sense::hover());
            ui.painter().text(
                rect.left_center(),
                egui::Align2::LEFT_CENTER,
                text,
                egui::FontId::monospace(8.0),
                theme::SMOKE,
            );
            body(ui);
        });
    };

    // ── Temperature ─────────────────────────────────────────────────────────
    labelled_row(ui, "TEMPERATURE", &mut |ui| {
        let mut v = temperature;
        if ui
            .add(egui::DragValue::new(&mut v).range(0.1..=2.0).speed(0.01))
            .changed()
        {
            with_tts(app, module_id, |t| t.temperature = v);
        }
    });

    // ── Top-K ───────────────────────────────────────────────────────────────
    labelled_row(ui, "TOP-K", &mut |ui| {
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
    labelled_row(ui, "TOP-P", &mut |ui| {
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

    // ── Pitch snap — same label-aligned row as the Synth controls ──────────
    labelled_row(ui, "PITCH SNAP", &mut |ui| {
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
