// ─── ui/mod.rs ────────────────────────────────────────────────────────────────
// Main egui application.

pub mod theme;
pub mod widgets;

/// Convert a dot-path + float value into a nested JSON object.
/// "bass.cutoff", 0.4  →  {"bass": {"cutoff": 0.4}}
fn dot_path_to_json(path: &str, value: f32) -> serde_json::Value {
    let parts: Vec<&str> = path.split('.').collect();
    let leaf = serde_json::json!(value);
    parts.iter().rev().fold(leaf, |acc, &key| serde_json::json!({ key: acc }))
}

/// Short note name for a MIDI note number (e.g. 60 → "C4").
fn note_name(midi: u8) -> &'static str {
    const NAMES: &[&str] = &[
        "C1","C#1","D1","D#1","E1","F1","F#1","G1","G#1","A1","A#1","B1",
        "C2","C#2","D2","D#2","E2","F2","F#2","G2","G#2","A2","A#2","B2",
        "C3","C#3","D3","D#3","E3","F3","F#3","G3","G#3","A3","A#3","B3",
        "C4","C#4","D4","D#4","E4","F4","F#4","G4","G#4","A4","A#4","B4",
        "C5","C#5","D5","D#5","E5","F5","F#5","G5","G#5","A5","A#5","B5",
        "C6",
    ];
    // MIDI 24 = C1
    let idx = midi.saturating_sub(24) as usize;
    NAMES.get(idx).copied().unwrap_or("?")
}

/// Open a URL in the system browser (cross-platform, no extra dep).
fn webbrowser_open(url: &str) -> std::io::Result<()> {
    #[cfg(target_os = "linux")]
    std::process::Command::new("xdg-open").arg(url).spawn()?;
    #[cfg(target_os = "macos")]
    std::process::Command::new("open").arg(url).spawn()?;
    #[cfg(target_os = "windows")]
    std::process::Command::new("cmd").args(["/c", "start", url]).spawn()?;
    Ok(())
}

use crossbeam_channel::{Receiver, Sender};
use egui::{CentralPanel, Frame, TopBottomPanel};
use parking_lot::RwLock;
use std::sync::Arc;

use crate::audio::{AudioCommand, AudioParams};
use crate::export::{export_wav, export_mp3};
use crate::llm::{LlmInput, LlmOutput};
use crate::midi::MidiEvent;
use crate::sequencer::TriggerEvent;
use crate::llm::styles::StyleCatalog;
use crate::state::{AppState, ConversationMode, DrumVoice, Waveform, toggle_drum_step, save_project};

// ─── Instrument slot system ───────────────────────────────────────────────────

/// The synthesis character of an instrument module.
/// Determines which draw function is called for `Panel::Instrument(i)`.
#[derive(Clone, Copy, PartialEq)]
enum InstrumentKind {
    AcidBass,   // draw_bass()
    DrumKit808, // draw_kit_a()
    DrumKit909, // draw_kit_b()
}

struct InstrumentSlot {
    label: &'static str,
    kind: InstrumentKind,
}

/// Active panel — Sequencer and Fx are fixed; Instrument(i) indexes into
/// `ImpulseApp.instruments` so the tab bar is fully data-driven.
#[derive(PartialEq, Clone, Copy)]
enum Panel {
    Sequencer,
    Instrument(usize),
    Fx,
}

// ─── ImpulseApp ───────────────────────────────────────────────────────────────

pub struct ImpulseApp {
    state: Arc<RwLock<AppState>>,
    audio_tx: rtrb::Producer<AudioCommand>,
    llm_tx: Sender<LlmInput>,
    llm_rx: Receiver<LlmOutput>,
    midi_rx: Receiver<MidiEvent>,
    midi_port: Option<String>,
    pressed_notes: std::collections::HashSet<u8>,
    prompt_input: String,
    log_text: String,
    active_panel: Panel,
    instruments: Vec<InstrumentSlot>,
    api_port: Option<u16>,
    show_about: bool,
    show_prefs: bool,
    export_bars: u32,
    ui_volume: f32,         // monitor-only gain; never written to state or export
    // Piano preferences
    piano_show_labels: bool,
    piano_show_colors: bool,
    // Last chain-of-thought from Bonsai (shown collapsible below the log)
    last_thinking: Option<String>,
    show_thinking: bool,
    // Control layout preference
    use_sliders: bool,
}

impl ImpulseApp {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        state: Arc<RwLock<AppState>>,
        audio_tx: rtrb::Producer<AudioCommand>,
        llm_tx: Sender<LlmInput>,
        llm_rx: Receiver<LlmOutput>,
        midi_rx: Receiver<MidiEvent>,
        midi_port: Option<String>,
        api_port: Option<u16>,
    ) -> Self {
        theme::apply(&cc.egui_ctx);
        let mut log_text = "[ Impulse Instruct ready ]\n".to_string();
        if let Some(ref port) = midi_port {
            log_text.push_str(&format!("[ MIDI: {} ]\n", port));
        } else {
            log_text.push_str("[ MIDI: no device found ]\n");
        }
        if let Some(port) = api_port {
            log_text.push_str(&format!("[ HTTP API active → http://localhost:{} ]\n", port));
        }
        Self {
            state,
            audio_tx,
            llm_tx,
            llm_rx,
            midi_rx,
            midi_port,
            pressed_notes: std::collections::HashSet::new(),
            prompt_input: "let's make some acid".to_string(),
            log_text,
            active_panel: Panel::Sequencer,
            instruments: vec![
                InstrumentSlot { label: "BASS SYNTH", kind: InstrumentKind::AcidBass   },
                InstrumentSlot { label: "DRUM KIT A", kind: InstrumentKind::DrumKit808 },
                InstrumentSlot { label: "DRUM KIT B", kind: InstrumentKind::DrumKit909 },
            ],
            api_port,
            show_about: false,
            show_prefs: false,
            export_bars: 8,
            ui_volume: 1.0,
            piano_show_labels: true,
            piano_show_colors: true,
            last_thinking: None,
            show_thinking: false,
            use_sliders: false,
        }
    }

    /// Push any pending audio param snapshot to the audio thread.

    fn push_audio_params(&mut self) {
        let params = {
            let s = self.state.read();
            let mut p = AudioParams::from_app_state(&s);
            p.sample_rate = 44100.0; // will be overwritten by engine
            p
        };
        let _ = self.audio_tx.push(AudioCommand::UpdateParams(params));
    }

    /// Drain LLM output messages.
    fn drain_llm_outputs(&mut self) {
        while let Ok(out) = self.llm_rx.try_recv() {
            // Store thinking tokens for display
            if let Some(ref thinking) = out.thinking {
                if !thinking.is_empty() {
                    self.last_thinking = Some(thinking.clone());
                }
            }

            if !out.is_jam && (out.param_update.is_some() || (!out.text.is_empty() && !out.text.starts_with('['))) {
                let conv_mode = self.state.read().llm.conversation_mode.clone();
                let display = if let Some(ref update) = out.param_update {
                    if conv_mode == ConversationMode::Off {
                        // Off: show only what keys changed, no commentary
                        let keys: Vec<&str> = update.as_object()
                            .map(|o| o.keys().filter(|k| *k != "_comment").map(|k| k.as_str()).collect())
                            .unwrap_or_default();
                        format!("updated {}", keys.join(", "))
                    } else if let Some(comment) = update.get("_comment").and_then(|v| v.as_str()) {
                        comment.to_string()
                    } else {
                        let keys: Vec<&str> = update.as_object()
                            .map(|o| o.keys().filter(|k| *k != "_comment").map(|k| k.as_str()).collect())
                            .unwrap_or_default();
                        format!("updated {}", keys.join(", "))
                    }
                } else {
                    out.text.clone()
                };
                // Append thinking indicator when present
                let line = if out.thinking.as_ref().map_or(false, |t| !t.is_empty()) {
                    format!("Bonsai → {} [🧠]\n", display)
                } else {
                    format!("Bonsai → {}\n", display)
                };
                self.log_text.push_str(&line);
            }
            // If jam cycle done and auto_jam is on, re-trigger
            if out.text == "[jam_cycle_done]" {
                let auto_jam = self.state.read().llm.auto_jam;
                if auto_jam {
                    let _ = self.llm_tx.try_send(LlmInput {
                        prompt: "continue jamming, evolve the pattern".to_string(),
                        one_shot: false,
                    });
                }
            }
            // Push updated params after LLM changed state
            if out.param_update.is_some() {
                self.push_audio_params();
            }
        }
    }

    /// Drain incoming MIDI events, update pressed_notes, trigger DSP.
    fn drain_midi_events(&mut self) {
        use crate::midi::cc_to_param_path;
        use crate::state::{apply_llm_update, toggle_sequencer_running};

        while let Ok(event) = self.midi_rx.try_recv() {
            match event {
                MidiEvent::NoteOn { note, velocity, .. } => {
                    self.pressed_notes.insert(note);
                    let vel = velocity as f32 / 127.0;

                    let _ = self.audio_tx.push(AudioCommand::Trigger(
                        TriggerEvent::BassTrigger {
                            note,
                            accent: vel > 0.8,
                            slide: false,
                            gate_samples: 22050, // ~0.5 s at 44100 Hz
                        }
                    ));

                    // Write note into current step so you can step-program live.
                    let step = self.state.read().sequencer.current_step;
                    let s = self.state.read().clone();
                    let was_active = s.sequencer.bass_pattern[step].active;
                    *self.state.write() = crate::state::set_bass_step(s, step, note, was_active);
                }

                MidiEvent::NoteOff { note, .. } => {
                    self.pressed_notes.remove(&note);
                    let _ = self.audio_tx.push(AudioCommand::Trigger(TriggerEvent::BassGateOff));
                }

                // CC → synth params via the standard mapping table.
                // Builds a partial JSON update matching the dot-path and feeds it
                // through apply_llm_update so locked params are respected.
                MidiEvent::ControlChange { cc, value, .. } => {
                    if let Some((path, scale)) = cc_to_param_path(cc) {
                        let scaled = scale(value);
                        let update = dot_path_to_json(path, scaled);
                        let next = apply_llm_update(self.state.read().clone(), &update);
                        *self.state.write() = next;
                        self.push_audio_params();
                    }
                }

                // MIDI transport — Start/Stop control the sequencer.
                MidiEvent::Start => {
                    let s = self.state.read().clone();
                    if !s.sequencer.running {
                        *self.state.write() = toggle_sequencer_running(s);
                    }
                }
                MidiEvent::Stop => {
                    let s = self.state.read().clone();
                    if s.sequencer.running {
                        *self.state.write() = toggle_sequencer_running(s);
                    }
                }

                _ => {}
            }
        }
    }
}

impl eframe::App for ImpulseApp {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.drain_llm_outputs();
        self.drain_midi_events();
        ctx.request_repaint_after(std::time::Duration::from_millis(16));

        // ── Preferences window ────────────────────────────────────────────────
        if self.show_prefs {
            egui::Window::new("Preferences")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.set_min_width(260.0);

                    // ── Bonsai personality ────────────────────────────────────
                    widgets::section_header(ui, "BONSAI PERSONALITY");
                    ui.label(egui::RichText::new("How Bonsai narrates its moves").monospace().size(8.5).color(theme::IRON));
                    ui.add_space(4.0);
                    let cur_mode = self.state.read().llm.conversation_mode.clone();
                    let modes: &[(&str, ConversationMode, &str)] = &[
                        ("Off",      ConversationMode::Off,      "no commentary"),
                        ("Producer", ConversationMode::Producer, "what & why (default)"),
                        ("DJ",       ConversationMode::Dj,       "hype party energy"),
                        ("MC",       ConversationMode::Mc,       "jungle/rave MC"),
                    ];
                    for (label, mode, hint) in modes {
                        ui.horizontal(|ui| {
                            let selected = cur_mode == *mode;
                            let text = egui::RichText::new(*label).monospace().size(10.0)
                                .color(if selected { theme::CHALK } else { theme::FOG });
                            if ui.selectable_label(selected, text).clicked() && !selected {
                                self.state.write().llm.conversation_mode = mode.clone();
                            }
                            ui.label(egui::RichText::new(*hint).monospace().size(8.5).color(theme::IRON));
                        });
                    }

                    ui.add_space(8.0);
                    ui.separator();
                    ui.add_space(4.0);

                    // ── Piano display ─────────────────────────────────────────
                    widgets::section_header(ui, "PIANO DISPLAY");
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("Note labels").monospace().size(9.5).color(theme::FOG));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            widgets::toggle_button(ui, if self.piano_show_labels { "ON" } else { "OFF" }, &mut self.piano_show_labels);
                        });
                    });
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("Farbige Noten colors").monospace().size(9.5).color(theme::FOG));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            widgets::toggle_button(ui, if self.piano_show_colors { "ON" } else { "OFF" }, &mut self.piano_show_colors);
                        });
                    });
                    ui.add_space(8.0);
                    ui.separator();
                    ui.add_space(4.0);
                    ui.vertical_centered(|ui| {
                        if ui.button("Close").clicked() {
                            self.show_prefs = false;
                        }
                    });
                });
        }

        // ── About window ──────────────────────────────────────────────────────
        if self.show_about {
            egui::Window::new("About Impulse Instruct")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.label(egui::RichText::new("◆ IMPULSE INSTRUCT").monospace().size(14.0).color(theme::CHALK));
                        ui.label(egui::RichText::new("v0.1 — LLM-controlled synthesizer").monospace().size(9.5).color(theme::SMOKE));
                        ui.add_space(8.0);
                        ui.label(egui::RichText::new("Bass Synth  ·  Drum Kit A  ·  Drum Kit B").monospace().size(9.0).color(theme::ASH));
                        ui.label(egui::RichText::new("Bonsai 8B 1-bit · llama.cpp").monospace().size(9.0).color(theme::ASH));
                        ui.add_space(10.0);
                        ui.label(egui::RichText::new("Type a prompt and press ASK.").monospace().size(9.0).color(theme::FOG));
                        ui.label(egui::RichText::new("Toggle JAM for continuous mutation.").monospace().size(9.0).color(theme::FOG));
                        ui.label(egui::RichText::new("HEAT controls how wild it gets.").monospace().size(9.0).color(theme::FOG));
                        ui.add_space(10.0);
                        if ui.button("Close").clicked() {
                            self.show_about = false;
                        }
                    });
                });
        }

        // ── Menu bar ──────────────────────────────────────────────────────────
        TopBottomPanel::top("menu_bar")
            .frame(Frame::none().fill(theme::VOID).inner_margin(egui::Margin::symmetric(4.0, 2.0)))
            .show(ctx, |ui| {
                egui::menu::bar(ui, |ui| {
                    ui.menu_button(egui::RichText::new("File").monospace().size(10.0), |ui| {
                        if ui.button(egui::RichText::new("Save project").monospace().size(10.0)).clicked() {
                            let snapshot = self.state.read().clone();
                            match save_project(&snapshot) {
                                Ok(path) => {
                                    let msg = format!("[ saved → {} ]", path.display());
                                    log::info!("{}", msg);
                                    self.log_text.push_str(&format!("{}\n", msg));
                                }
                                Err(e) => {
                                    let msg = format!("[ save failed: {} ]", e);
                                    log::error!("{}", msg);
                                    self.log_text.push_str(&format!("{}\n", msg));
                                }
                            }
                            ui.close_menu();
                        }

                        ui.separator();

                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new("Bars:").monospace().size(9.5).color(theme::SMOKE));
                            ui.add(egui::DragValue::new(&mut self.export_bars).range(1..=64).speed(1.0));
                        });

                        if ui.button(egui::RichText::new("Export WAV").monospace().size(10.0)).clicked() {
                            let snapshot = self.state.read().clone();
                            let bars = self.export_bars;
                            match export_wav(&snapshot, bars) {
                                Ok(path) => {
                                    let msg = format!("[ exported → {} ]", path.display());
                                    log::info!("{}", msg);
                                    self.log_text.push_str(&format!("{}\n", msg));
                                }
                                Err(e) => {
                                    let msg = format!("[ export failed: {} ]", e);
                                    log::error!("{}", msg);
                                    self.log_text.push_str(&format!("{}\n", msg));
                                }
                            }
                            ui.close_menu();
                        }

                        if ui.button(egui::RichText::new("Export MP3").monospace().size(10.0)).clicked() {
                            let snapshot = self.state.read().clone();
                            let bars = self.export_bars;
                            match export_mp3(&snapshot, bars) {
                                Ok(path) => {
                                    let msg = format!("[ exported → {} ]", path.display());
                                    log::info!("{}", msg);
                                    self.log_text.push_str(&format!("{}\n", msg));
                                }
                                Err(e) => {
                                    let msg = format!("[ export: {} ]", e);
                                    log::warn!("{}", msg);
                                    self.log_text.push_str(&format!("{}\n", msg));
                                }
                            }
                            ui.close_menu();
                        }

                        ui.separator();

                        if ui.button(egui::RichText::new("Quit").monospace().size(10.0)).clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    });

                    ui.menu_button(egui::RichText::new("Help").monospace().size(10.0), |ui| {
                        if ui.button(egui::RichText::new("Preferences…").monospace().size(10.0)).clicked() {
                            self.show_prefs = true;
                            ui.close_menu();
                        }
                        ui.separator();
                        if ui.button(egui::RichText::new("About").monospace().size(10.0)).clicked() {
                            self.show_about = true;
                            ui.close_menu();
                        }
                    });
                });
            });

        let _ = frame; // suppress unused warning (frame.close() replaced by viewport cmd)

        // ── Header ────────────────────────────────────────────────────────────
        TopBottomPanel::top("header")
            .frame(Frame::none().fill(theme::VOID).inner_margin(egui::Margin::symmetric(10.0, 6.0)))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    // Logo
                    ui.label(
                        egui::RichText::new("◆ IMPULSE INSTRUCT")
                            .color(theme::CHALK)
                            .size(13.0)
                            .monospace()
                            .strong()
                    );

                    ui.add_space(16.0);

                    // Model stats
                    {
                        let s = self.state.read();
                        let inferring = s.llm.is_inferring;
                        let tps  = s.llm.tokens_per_sec;
                        let ptok = s.llm.prompt_tokens;
                        let ctok = s.llm.completion_tokens;
                        let ctx_pct = if s.llm.context_max > 0 {
                            s.llm.context_used as f32 / s.llm.context_max as f32 * 100.0
                        } else { 0.0 };

                        let inf_color = if inferring { theme::CHALK } else { theme::IRON };
                        ui.label(egui::RichText::new("●").color(inf_color).size(10.0));
                        let stats = if ptok > 0 || ctok > 0 {
                            format!("{:.0}t/s  p:{} c:{}  ctx:{:.0}%", tps, ptok, ctok, ctx_pct)
                        } else {
                            format!("{:.0}t/s  ctx:{:.0}%", tps, ctx_pct)
                        };
                        ui.label(egui::RichText::new(stats).color(theme::SMOKE).size(9.5).monospace());

                        ui.add_space(8.0);

                        // BPM display
                        let bpm = s.sequencer.bpm;
                        let running = s.sequencer.running;
                        let run_color = if running { theme::CHALK } else { theme::ASH };
                        ui.label(egui::RichText::new(format!("{:.0} BPM", bpm))
                            .color(run_color).size(11.0).monospace());
                    }

                    ui.add_space(8.0);

                    // Play / Stop
                    {
                        let running = self.state.read().sequencer.running;
                        let play_label = if running { "■ STOP" } else { "▶ PLAY" };
                        if ui.button(egui::RichText::new(play_label).monospace().size(10.0)).clicked() {
                            let next = crate::state::toggle_sequencer_running(self.state.read().clone());
                            *self.state.write() = next;
                        }
                    }

                    ui.add_space(4.0);

                    // Heat slider
                    {
                        let mut heat = self.state.read().llm.heat;
                        let heat_color = if heat < 0.3 { theme::IRON }
                            else if heat < 0.6 { theme::ASH }
                            else if heat < 0.85 { theme::SMOKE }
                            else { theme::CHALK };
                        ui.label(egui::RichText::new("HEAT").color(heat_color).monospace().size(9.0));
                        if ui.add_sized(
                            [80.0, 16.0],
                            egui::Slider::new(&mut heat, 0.0..=1.0).show_value(false)
                        ).changed() {
                            self.state.write().llm.heat = heat;
                        }
                    }

                    ui.add_space(4.0);

                    // JAM toggle
                    let jam = self.state.read().llm.auto_jam;
                    let jam_color = if jam { theme::CHALK } else { theme::ASH };
                    if ui.button(egui::RichText::new("JAM").color(jam_color).monospace().size(10.0)).clicked() {
                        let mut next = self.state.read().clone();
                        next.llm.auto_jam = !next.llm.auto_jam;
                        let now_jamming = next.llm.auto_jam;
                        *self.state.write() = next;
                        if now_jamming {
                            let _ = self.llm_tx.try_send(LlmInput {
                                prompt: "start jamming".to_string(),
                                one_shot: false,
                            });
                        }
                    }

                    ui.add_space(4.0);

                    // Knob / Slider toggle
                    {
                        let label = if self.use_sliders { "SLIDERS" } else { "KNOBS" };
                        let color  = if self.use_sliders { theme::CHALK } else { theme::ASH };
                        if ui.button(egui::RichText::new(label).color(color).monospace().size(9.0)).clicked() {
                            self.use_sliders = !self.use_sliders;
                        }
                    }

                    // Right-aligned: VOL slider + optional API link
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        // API link
                        if let Some(port) = self.api_port {
                            let api_label = format!("API :{}", port);
                            let btn = ui.add(
                                egui::Button::new(
                                    egui::RichText::new(&api_label).color(theme::SMOKE).monospace().size(8.5)
                                )
                                .fill(theme::VOID)
                                .stroke(egui::Stroke::new(1.0, theme::SLATE))
                            );
                            if btn.clicked() {
                                let url = format!("http://localhost:{}/api/schema", port);
                                let _ = webbrowser_open(&url);
                            }
                            if btn.hovered() {
                                btn.on_hover_text("Open API schema in browser");
                            }
                            ui.add_space(8.0);
                        }

                        // Monitor volume slider (does not affect export)
                        let vol_color = if self.ui_volume < 0.4 { theme::IRON }
                            else if self.ui_volume < 0.75 { theme::ASH }
                            else { theme::SMOKE };
                        ui.label(egui::RichText::new(format!("{:.0}%", self.ui_volume * 100.0))
                            .color(vol_color).monospace().size(9.0));
                        if ui.add_sized(
                            [72.0, 16.0],
                            egui::Slider::new(&mut self.ui_volume, 0.0..=1.0).show_value(false)
                        ).changed() {
                            let _ = self.audio_tx.push(AudioCommand::SetMonitorVolume(self.ui_volume));
                        }
                        ui.label(egui::RichText::new("VOL").color(vol_color).monospace().size(9.0));
                    });
                });
            });

        // ── LLM strip (log + prompt) ──────────────────────────────────────────
        TopBottomPanel::top("llm_strip")
            .frame(Frame::none().fill(theme::PIT).inner_margin(egui::Margin::symmetric(10.0, 6.0)))
            .min_height(80.0)
            .max_height(140.0)
            .show(ctx, |ui| {
                // ── Style selector ────────────────────────────────────────────
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("STYLE").monospace().size(9.0).color(theme::IRON));
                    ui.add_space(4.0);

                    let cur_style = self.state.read().llm.active_style.clone();
                    let catalog = StyleCatalog::get();
                    let cur_name = match cur_style.as_deref() {
                        None            => "None",
                        Some("__free__")   => "Free",
                        Some("__custom__") => "Custom",
                        Some(id) => catalog.find_by_id(id).map(|s| s.name.as_str()).unwrap_or("None"),
                    };

                    let mut new_style_selection: Option<Option<String>> = None;

                    egui::ComboBox::from_id_source("style_selector")
                        .selected_text(egui::RichText::new(cur_name).monospace().size(9.5))
                        .width(160.0)
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
                                let active = cur_style.as_deref() == Some(style.id.as_str());
                                if ui.selectable_label(
                                    active,
                                    egui::RichText::new(&style.name).monospace().size(9.5),
                                ).clicked() {
                                    new_style_selection = Some(Some(style.id.clone()));
                                }
                            }
                        });

                    if let Some(maybe_id) = new_style_selection {
                        match maybe_id {
                            None => {
                                self.state.write().llm.active_style = None;
                                self.log_text.push_str("Style cleared\n");
                            }
                            Some(ref id) if id == "__free__" => {
                                self.state.write().llm.active_style = Some(id.clone());
                                self.log_text.push_str("Style → Free (no constraints)\n");
                                let _ = self.llm_tx.try_send(LlmInput {
                                    prompt: "we're going free — be creative and unpredictable, surprise me".to_string(),
                                    one_shot: true,
                                });
                            }
                            Some(ref id) if id == "__custom__" => {
                                self.state.write().llm.active_style = Some(id.clone());
                                self.log_text.push_str("Style → Custom (edit brief below)\n");
                            }
                            Some(id) => {
                                let name = catalog.find_by_id(&id)
                                    .map(|s| s.name.clone()).unwrap_or_default();
                                self.state.write().llm.active_style = Some(id);
                                let prompt = format!(
                                    "we're going {} now — set up the sound and rhythm for this style",
                                    name
                                );
                                self.log_text.push_str(&format!("Style → {}\n", name));
                                let _ = self.llm_tx.try_send(LlmInput { prompt, one_shot: true });
                            }
                        }
                    }
                });

                // ── Custom style brief input (shown only when Custom is active) ──
                if self.state.read().llm.active_style.as_deref() == Some("__custom__") {
                    ui.horizontal(|ui| {
                        ui.add_space(40.0); // indent to align with dropdown content
                        let mut custom_text = self.state.read().llm.custom_style_text.clone();
                        let r = ui.add(
                            egui::TextEdit::singleline(&mut custom_text)
                                .hint_text("describe your style brief for Bonsai…")
                                .desired_width(ui.available_width() - 60.0)
                                .font(egui::FontId::monospace(10.0))
                        );
                        if r.changed() {
                            self.state.write().llm.custom_style_text = custom_text.clone();
                        }
                        if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))
                            && !custom_text.trim().is_empty()
                        {
                            self.log_text.push_str("Custom style brief updated\n");
                            let _ = self.llm_tx.try_send(LlmInput {
                                prompt: "apply the active style brief — update sound and rhythm accordingly".to_string(),
                                one_shot: true,
                            });
                        }
                    });
                }

                ui.add_space(2.0);

                // ── Persistent user instructions ──────────────────────────────
                {
                    let mut instr = self.state.read().llm.user_instructions.clone();
                    let r = ui.add(
                        egui::TextEdit::singleline(&mut instr)
                            .hint_text("persistent instructions for Bonsai (injected into every prompt)…")
                            .desired_width(ui.available_width())
                            .font(egui::FontId::monospace(10.0))
                    );
                    if r.changed() {
                        self.state.write().llm.user_instructions = instr;
                    }
                }

                ui.add_space(2.0);

                // ── Prompt input ──────────────────────────────────────────────
                ui.horizontal(|ui| {
                    let text_width = ui.available_width() - 52.0; // reserve space for ASK button
                    let response = ui.add(
                        egui::TextEdit::singleline(&mut self.prompt_input)
                            .hint_text("prompt the model…")
                            .desired_width(text_width)
                            .font(egui::FontId::monospace(10.5))
                    );
                    let submit = ui.button(
                        egui::RichText::new("ASK").monospace().size(10.0)
                    ).clicked();

                    let enter_pressed = response.lost_focus()
                        && ctx.input(|i| i.key_pressed(egui::Key::Enter));

                    if submit || enter_pressed {
                        let typed = self.prompt_input.trim().to_string();
                        let (prompt, log_line) = if typed.is_empty() {
                            // Empty submit — nudge Bonsai to do something fresh
                            let active_style = self.state.read().llm.active_style.clone();
                            let p = match active_style.as_deref() {
                                Some(id) => {
                                    let name = StyleCatalog::get()
                                        .find_by_id(id)
                                        .map(|s| s.name.as_str())
                                        .unwrap_or(id);
                                    format!("do something fresh in the {} style — surprise me", name)
                                }
                                None => "do something interesting — evolve the pattern and sound however you feel".to_string(),
                            };
                            (p, "YOU → ✦\n".to_string())
                        } else {
                            (typed.clone(), format!("YOU → {}\n", typed))
                        };
                        self.log_text.push_str(&log_line);
                        let _ = self.llm_tx.try_send(LlmInput { prompt, one_shot: true });
                        self.prompt_input.clear();
                    }
                });

                ui.add_space(4.0);

                // Selectable, copy-pastable log backed by an append-only string
                egui::ScrollArea::vertical()
                    .id_source("log_scroll")
                    .stick_to_bottom(true)
                    .max_height(90.0)
                    .auto_shrink([false, false])
                    .show(ui, |ui: &mut egui::Ui| {
                        ui.add(
                            egui::TextEdit::multiline(&mut self.log_text)
                                .desired_width(f32::INFINITY)
                                .font(egui::FontId::monospace(9.0))
                                .text_color(theme::SMOKE)
                                .frame(false)
                                .interactive(true)
                        );
                    });

                // Thinking tokens (collapsible, only shown when present)
                if let Some(ref thinking) = self.last_thinking.clone() {
                    ui.horizontal(|ui| {
                        let label = if self.show_thinking { "▾ thinking" } else { "▸ thinking" };
                        if ui.small_button(egui::RichText::new(label).color(theme::IRON).size(9.0)).clicked() {
                            self.show_thinking = !self.show_thinking;
                        }
                    });
                    if self.show_thinking {
                        egui::ScrollArea::vertical()
                            .id_source("thinking_scroll")
                            .max_height(60.0)
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                let mut t = thinking.clone();
                                ui.add(
                                    egui::TextEdit::multiline(&mut t)
                                        .desired_width(f32::INFINITY)
                                        .font(egui::FontId::monospace(9.0))
                                        .text_color(theme::IRON)
                                        .frame(false)
                                        .interactive(false)
                                );
                            });
                    }
                }
            });

        // ── Tab bar (data-driven — add InstrumentSlot to self.instruments to extend) ──
        TopBottomPanel::top("tabs")
            .frame(Frame::none().fill(theme::DEEP).inner_margin(egui::Margin::symmetric(8.0, 2.0)))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    // Fixed left tab
                    let tab = |ui: &mut egui::Ui, panel: Panel, label: &str, active_panel: Panel| -> bool {
                        let active = active_panel == panel;
                        let color = if active { theme::CHALK } else { theme::IRON };
                        let fill  = if active { theme::SLATE  } else { theme::DEEP  };
                        ui.add(
                            egui::Button::new(egui::RichText::new(label).color(color).monospace().size(9.5))
                                .fill(fill)
                                .stroke(egui::Stroke::new(1.0, if active { theme::ASH } else { theme::VOID }))
                        ).clicked()
                    };

                    if tab(ui, Panel::Sequencer, "SEQUENCER", self.active_panel) {
                        self.active_panel = Panel::Sequencer;
                    }
                    for i in 0..self.instruments.len() {
                        let label = self.instruments[i].label;
                        if tab(ui, Panel::Instrument(i), label, self.active_panel) {
                            self.active_panel = Panel::Instrument(i);
                        }
                    }
                    if tab(ui, Panel::Fx, "FX CHAIN", self.active_panel) {
                        self.active_panel = Panel::Fx;
                    }
                });
            });

        // ── Piano display (bottom, always visible) ────────────────────────────
        TopBottomPanel::bottom("piano")
            .frame(Frame::none().fill(theme::VOID).inner_margin(egui::Margin::symmetric(0.0, 0.0)))
            .exact_height(80.0)
            .show(ctx, |ui| {
                self.draw_piano(ui, ctx);
            });

        // ── Main content ──────────────────────────────────────────────────────
        CentralPanel::default()
            .frame(Frame::none().fill(theme::DEEP).inner_margin(egui::Margin::same(8.0)))
            .show(ctx, |ui| {
                match self.active_panel {
                    Panel::Sequencer => self.draw_sequencer(ui),
                    Panel::Instrument(i) => {
                        let kind = self.instruments[i].kind;
                        match kind {
                            InstrumentKind::AcidBass   => self.draw_bass(ui),
                            InstrumentKind::DrumKit808 => self.draw_kit_a(ui),
                            InstrumentKind::DrumKit909 => self.draw_kit_b(ui),
                        }
                    }
                    Panel::Fx => self.draw_fx(ui),
                }
            });
    }
}

// ─── Panel drawing ────────────────────────────────────────────────────────────

impl ImpulseApp {
    #[allow(dead_code)] // used when panels get individual frames
    fn panel_frame() -> Frame {
        Frame::none()
            .fill(theme::PIT)
            .stroke(egui::Stroke::new(1.0, theme::SLATE))
            .rounding(egui::Rounding::same(3.0))
            .inner_margin(egui::Margin::same(8.0))
    }

    // ── Sequencer ────────────────────────────────────────────────────────────

    fn draw_sequencer(&mut self, ui: &mut egui::Ui) {
        let (current_step, running) = {
            let s = self.state.read();
            (s.sequencer.current_step, s.sequencer.running)
        };
        // Only highlight the cursor when the sequencer is actually playing;
        // usize::MAX guarantees no step matches when stopped.
        let cursor = if running { current_step } else { usize::MAX };

        widgets::section_header(ui, "STEP SEQUENCER — 16 STEPS");

        // Steps counter control
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("STEPS").color(theme::SMOKE).monospace().size(9.0));
            let mut steps = self.state.read().sequencer.steps;
            if ui.small_button("−").clicked() && steps > 1 { steps -= 1; self.state.write().sequencer.steps = steps; }
            ui.label(egui::RichText::new(format!("{:02}", steps)).color(theme::FOG).monospace());
            if ui.small_button("+").clicked() && steps < 16 { steps += 1; self.state.write().sequencer.steps = steps; }

            ui.add_space(12.0);

            // BPM
            let mut bpm = self.state.read().sequencer.bpm;
            ui.label(egui::RichText::new("BPM").color(theme::SMOKE).monospace().size(9.0));
            let resp = ui.add(egui::DragValue::new(&mut bpm).range(40.0..=250.0).speed(0.5));
            if resp.changed() {
                self.state.write().sequencer.bpm = bpm;
                self.push_audio_params();
            }
        });

        ui.add_space(6.0);

        let voices = DrumVoice::ALL;
        let num_steps = 16;

        // Step grid — draw each row
        egui::ScrollArea::vertical().show(ui, |ui| {
            for voice in voices {
                ui.horizontal(|ui| {
                    // Voice label
                    let label = voice.label();
                    ui.add_sized(
                        [80.0, 22.0],
                        egui::Label::new(
                            egui::RichText::new(label).color(theme::SMOKE).monospace().size(8.5)
                        )
                    );

                    // Step buttons
                    let (pattern, step_count) = {
                        let s = self.state.read();
                        let pat = s.sequencer.drum_patterns.get(voice).copied()
                            .unwrap_or_default();
                        (pat, s.sequencer.steps)
                    };

                    let mut toggled = None;
                    for i in 0..num_steps {
                        // Group dividers every 4
                        if i > 0 && i % 4 == 0 {
                            ui.add_space(2.0);
                        }
                        let is_active = pattern[i].active;
                        let is_current = i == cursor;
                        let vel = pattern[i].velocity;
                        let enabled = i < step_count;

                        ui.add_enabled_ui(enabled, |ui| {
                            if widgets::step_button(ui, is_active, is_current, vel, None) {
                                toggled = Some(i);
                            }
                        });
                    }

                    if let Some(step) = toggled {
                        let s = self.state.read().clone();
                        *self.state.write() = toggle_drum_step(s, *voice, step);
                    }
                });
            }

            // Bass row
            ui.add_space(2.0);
            ui.separator();
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                ui.add_sized(
                    [80.0, 22.0],
                    egui::Label::new(
                        egui::RichText::new("BASS").color(theme::SMOKE).monospace().size(8.5)
                    )
                );
                let (bass_pattern, step_count) = {
                    let s = self.state.read();
                    (s.sequencer.bass_pattern, s.sequencer.steps)
                };
                for i in 0..num_steps {
                    if i > 0 && i % 4 == 0 { ui.add_space(2.0); }
                    let is_active = bass_pattern[i].active;
                    let is_current = i == cursor;
                    let note_col = Some(theme::note_color(bass_pattern[i].note));
                    ui.add_enabled_ui(i < step_count, |ui| {
                        if widgets::step_button(ui, is_active, is_current, 1.0, note_col) {
                            let s = self.state.read().clone();
                            let note = s.sequencer.bass_pattern[i].note;
                            let was = s.sequencer.bass_pattern[i].active;
                            *self.state.write() = crate::state::set_bass_step(s, i, note, !was);
                        }
                    });
                }
            });
        });
    }

    // ── Bass synth ────────────────────────────────────────────────────────────

    fn draw_bass(&mut self, ui: &mut egui::Ui) {
        widgets::section_header(ui, "BASS SYNTHESIZER");

        // Snapshot everything needed for rendering — lock released before any widget call
        let (mut cutoff, mut resonance, mut env_mod, mut decay, mut accent, mut dist, mut vol, waveform, locked) = {
            let s = self.state.read();
            (s.bass.cutoff, s.bass.resonance, s.bass.env_mod, s.bass.decay,
             s.bass.accent_level, s.bass.distortion, s.bass.volume,
             s.bass.waveform.clone(), s.llm.locked_params.clone())
        };

        let mut new_locks: Vec<&str> = Vec::new();
        let mut changed = false;

        let use_sliders = self.use_sliders;
        let draw_bass_controls = |ui: &mut egui::Ui| {
            if widgets::param_control(ui, "CUTOFF",    &mut cutoff,    locked.contains("bass.cutoff"),       use_sliders) { new_locks.push("bass.cutoff");       changed = true; }
            if widgets::param_control(ui, "RESONANCE", &mut resonance, locked.contains("bass.resonance"),    use_sliders) { new_locks.push("bass.resonance");    changed = true; }
            if widgets::param_control(ui, "ENV MOD",   &mut env_mod,   locked.contains("bass.env_mod"),      use_sliders) { new_locks.push("bass.env_mod");      changed = true; }
            if widgets::param_control(ui, "DECAY",     &mut decay,     locked.contains("bass.decay"),        use_sliders) { new_locks.push("bass.decay");        changed = true; }
            if widgets::param_control(ui, "ACCENT",    &mut accent,    locked.contains("bass.accent_level"), use_sliders) { new_locks.push("bass.accent_level"); changed = true; }
            if widgets::param_control(ui, "DRIVE",     &mut dist,      locked.contains("bass.distortion"),   use_sliders) { new_locks.push("bass.distortion");   changed = true; }
            if widgets::param_control(ui, "VOLUME",    &mut vol,       locked.contains("bass.volume"),       use_sliders) { new_locks.push("bass.volume");       changed = true; }
        };
        if use_sliders {
            ui.vertical(draw_bass_controls);
        } else {
            ui.horizontal_wrapped(draw_bass_controls);
        }

        // Apply all changes in a single brief write
        if changed {
            let mut s = self.state.write();
            s.bass.cutoff       = cutoff;
            s.bass.resonance    = resonance;
            s.bass.env_mod      = env_mod;
            s.bass.decay        = decay;
            s.bass.accent_level = accent;
            s.bass.distortion   = dist;
            s.bass.volume       = vol;
            for path in new_locks {
                s.llm.locked_params.insert(path.to_string());
            }
            drop(s);
            self.push_audio_params();
        }

        ui.add_space(8.0);

        // Waveform toggle
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("WAVE").color(theme::SMOKE).monospace().size(9.0));
            let saw_active = waveform == Waveform::Saw;
            let mut saw = saw_active;
            if widgets::toggle_button(ui, "SAW", &mut saw) {
                self.state.write().bass.waveform = Waveform::Saw;
                self.push_audio_params();
            }
            let mut sq = !saw_active;
            if widgets::toggle_button(ui, "SQR", &mut sq) {
                self.state.write().bass.waveform = Waveform::Square;
                self.push_audio_params();
            }
        });

        ui.add_space(12.0);

        // Locked params management
        let locked_bass: Vec<String> = locked.iter()
            .filter(|p| p.starts_with("bass"))
            .cloned().collect();

        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("LOCKED:").color(theme::SMOKE).monospace().size(8.5));
            if locked_bass.is_empty() {
                ui.label(egui::RichText::new("none (LLM controls all)").color(theme::IRON).monospace().size(8.5));
            } else {
                let mut to_remove: Option<String> = None;
                for p in &locked_bass {
                    let short = p.replace("bass.", "");
                    if ui.small_button(egui::RichText::new(format!("× {}", short)).monospace().size(8.0)).clicked() {
                        to_remove = Some(p.clone());
                    }
                }
                if let Some(p) = to_remove {
                    let next = crate::state::unlock_param(self.state.read().clone(), &p);
                    *self.state.write() = next;
                }
            }
            if ui.small_button(egui::RichText::new("UNLOCK ALL").monospace().size(8.0)).clicked() {
                let mut next = self.state.read().clone();
                next.llm.locked_params.retain(|p| !p.starts_with("bass"));
                *self.state.write() = next;
            }
        });
    }

    // ── Drum Kit A ────────────────────────────────────────────────────────────

    fn draw_kit_a(&mut self, ui: &mut egui::Ui) {
        widgets::section_header(ui, "DRUM KIT A");

        // Snapshot all values before any widget rendering
        let (mut kp, mut kd, mut kpu, mut kv,
             mut st, mut ssn, mut sd, mut sv,
             mut hcd, mut hod, mut hv) = {
            let s = self.state.read();
            (s.kit_a.kick.pitch, s.kit_a.kick.decay, s.kit_a.kick.punch, s.kit_a.kick.volume,
             s.kit_a.snare.tone, s.kit_a.snare.snappy, s.kit_a.snare.decay, s.kit_a.snare.volume,
             s.kit_a.hihat_closed.decay, s.kit_a.hihat_open.decay, s.kit_a.hihat_closed.volume)
        };
        let mut changed = false;

        let use_sliders = self.use_sliders;
        ui.horizontal_wrapped(|ui| {
            ui.group(|ui| {
                ui.label(egui::RichText::new("KICK").color(theme::FOG).monospace().size(9.5));
                if widgets::param_control(ui, "PITCH", &mut kp,  false, use_sliders) { changed = true; }
                if widgets::param_control(ui, "DECAY", &mut kd,  false, use_sliders) { changed = true; }
                if widgets::param_control(ui, "PUNCH", &mut kpu, false, use_sliders) { changed = true; }
                if widgets::param_control(ui, "LEVEL", &mut kv,  false, use_sliders) { changed = true; }
            });
            ui.add_space(4.0);
            ui.group(|ui| {
                ui.label(egui::RichText::new("SNARE").color(theme::FOG).monospace().size(9.5));
                if widgets::param_control(ui, "TONE",   &mut st,  false, use_sliders) { changed = true; }
                if widgets::param_control(ui, "SNAPPY", &mut ssn, false, use_sliders) { changed = true; }
                if widgets::param_control(ui, "DECAY",  &mut sd,  false, use_sliders) { changed = true; }
                if widgets::param_control(ui, "LEVEL",  &mut sv,  false, use_sliders) { changed = true; }
            });
            ui.add_space(4.0);
            ui.group(|ui| {
                ui.label(egui::RichText::new("HIHAT").color(theme::FOG).monospace().size(9.5));
                if widgets::param_control(ui, "CLOSED", &mut hcd, false, use_sliders) { changed = true; }
                if widgets::param_control(ui, "OPEN",   &mut hod, false, use_sliders) { changed = true; }
                if widgets::param_control(ui, "LEVEL",  &mut hv,  false, use_sliders) { changed = true; }
            });
        });

        // Single brief write with all changes
        if changed {
            let mut s = self.state.write();
            s.kit_a.kick.pitch          = kp;
            s.kit_a.kick.decay          = kd;
            s.kit_a.kick.punch          = kpu;
            s.kit_a.kick.volume         = kv;
            s.kit_a.snare.tone          = st;
            s.kit_a.snare.snappy        = ssn;
            s.kit_a.snare.decay         = sd;
            s.kit_a.snare.volume        = sv;
            s.kit_a.hihat_closed.decay  = hcd;
            s.kit_a.hihat_open.decay    = hod;
            s.kit_a.hihat_closed.volume = hv;
            s.kit_a.hihat_open.volume   = hv;
            drop(s);
            self.push_audio_params();
        }
    }

    // ── Drum Kit B ────────────────────────────────────────────────────────────

    fn draw_kit_b(&mut self, ui: &mut egui::Ui) {
        widgets::section_header(ui, "DRUM KIT B");

        let (mut kp, mut kd, mut kpu, mut kv,
             mut st, mut ssn, mut sd, mut sv,
             mut cd, mut cv) = {
            let s = self.state.read();
            (s.kit_b.kick.pitch, s.kit_b.kick.decay, s.kit_b.kick.punch, s.kit_b.kick.volume,
             s.kit_b.snare.tone, s.kit_b.snare.snappy, s.kit_b.snare.decay, s.kit_b.snare.volume,
             s.kit_b.clap.decay, s.kit_b.clap.volume)
        };
        let mut changed = false;

        let use_sliders = self.use_sliders;
        ui.horizontal_wrapped(|ui| {
            ui.group(|ui| {
                ui.label(egui::RichText::new("KICK").color(theme::FOG).monospace().size(9.5));
                if widgets::param_control(ui, "PITCH", &mut kp,  false, use_sliders) { changed = true; }
                if widgets::param_control(ui, "DECAY", &mut kd,  false, use_sliders) { changed = true; }
                if widgets::param_control(ui, "PUNCH", &mut kpu, false, use_sliders) { changed = true; }
                if widgets::param_control(ui, "LEVEL", &mut kv,  false, use_sliders) { changed = true; }
            });
            ui.add_space(4.0);
            ui.group(|ui| {
                ui.label(egui::RichText::new("SNARE").color(theme::FOG).monospace().size(9.5));
                if widgets::param_control(ui, "TONE",   &mut st,  false, use_sliders) { changed = true; }
                if widgets::param_control(ui, "SNAPPY", &mut ssn, false, use_sliders) { changed = true; }
                if widgets::param_control(ui, "DECAY",  &mut sd,  false, use_sliders) { changed = true; }
                if widgets::param_control(ui, "LEVEL",  &mut sv,  false, use_sliders) { changed = true; }
            });
            ui.add_space(4.0);
            ui.group(|ui| {
                ui.label(egui::RichText::new("CLAP / RIM").color(theme::FOG).monospace().size(9.5));
                if widgets::param_control(ui, "CLAP DEC", &mut cd, false, use_sliders) { changed = true; }
                if widgets::param_control(ui, "CLAP LVL", &mut cv, false, use_sliders) { changed = true; }
            });
        });

        if changed {
            let mut s = self.state.write();
            s.kit_b.kick.pitch   = kp;
            s.kit_b.kick.decay   = kd;
            s.kit_b.kick.punch   = kpu;
            s.kit_b.kick.volume  = kv;
            s.kit_b.snare.tone   = st;
            s.kit_b.snare.snappy = ssn;
            s.kit_b.snare.decay  = sd;
            s.kit_b.snare.volume = sv;
            s.kit_b.clap.decay   = cd;
            s.kit_b.clap.volume  = cv;
            drop(s);
            self.push_audio_params();
        }
    }

    // ── Piano keyboard display ────────────────────────────────────────────────

    fn draw_piano(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        use egui::{Color32, Pos2, Rect, Sense, Stroke, Vec2};

        // Range: C1 (MIDI 24) → C6 (MIDI 84) = 5 octaves + high C
        const START_NOTE: u8 = 24;
        const END_NOTE:   u8 = 84;   // inclusive
        const N_OCTAVES:  usize = 5;
        const N_WHITE:    usize = N_OCTAVES * 7 + 1; // 36 white keys (including C6)

        // Which semitones are white keys?
        const fn is_white(semitone: u8) -> bool {
            matches!(semitone, 0 | 2 | 4 | 5 | 7 | 9 | 11)
        }

        // Black key center positions in white-key units from octave start (C=0).
        // Each value sits at the boundary between two adjacent white keys so the
        // black key straddles the gap, matching a real piano layout.
        const BLACK_KEYS: &[(u8, f32)] = &[
            (1,  1.0), // C# — between C and D
            (3,  2.0), // D# — between D and E
            (6,  4.0), // F# — between F and G
            (8,  5.0), // G# — between G and A
            (10, 6.0), // A# — between A and B
        ];

        let available_w = ui.available_width();
        let wk_w = (available_w / N_WHITE as f32).max(8.0);
        let wk_h = 74.0_f32;
        let bk_w = wk_w * 0.55;
        let bk_h = wk_h * 0.60;
        let total_w = wk_w * N_WHITE as f32;

        let (rect, response) = ui.allocate_exact_size(
            Vec2::new(total_w, wk_h),
            Sense::click_and_drag(),
        );

        // Detect click/drag position for interactive playing
        let click_pos: Option<Pos2> = if response.is_pointer_button_down_on() || response.dragged() {
            ctx.input(|i| i.pointer.interact_pos())
        } else {
            None
        };
        let mut clicked_note: Option<u8> = None;

        // Sequencer cursor note (for highlighting)
        let (seq_note, seq_running) = {
            let s = self.state.read();
            let step = s.sequencer.current_step;
            let running = s.sequencer.running;
            if running && s.sequencer.bass_pattern[step].active {
                (Some(s.sequencer.bass_pattern[step].note), true)
            } else {
                (None, running)
            }
        };
        let _ = seq_running;

        if !ui.is_rect_visible(rect) { return; }

        let painter = ui.painter();
        painter.rect_filled(rect, 0.0, theme::VOID);

        let ox = rect.min.x; // origin x
        let oy = rect.min.y;

        let use_color  = self.piano_show_colors;
        let show_label = self.piano_show_labels;

        // Classic (non-Farbige) base colors
        let classic_white_inactive = Color32::from_rgb(58, 58, 58);
        let classic_black_inactive = Color32::from_rgb(18, 18, 18);
        let classic_active         = Color32::from_rgb(200, 200, 200);

        // ── White keys ────────────────────────────────────────────────────────
        let mut white_idx = 0usize;
        for note in START_NOTE..=END_NOTE {
            let semi = note % 12;
            if !is_white(semi) { continue; }

            let x = ox + white_idx as f32 * wk_w;
            let key_rect = Rect::from_min_size(
                Pos2::new(x + 0.5, oy),
                Vec2::new(wk_w - 1.0, wk_h),
            );

            let pressed    = self.pressed_notes.contains(&note);
            let seq_active = seq_note == Some(note);
            let active     = pressed || seq_active;

            let fill: Color32 = if use_color {
                let huth = theme::note_color(note);
                if pressed       { theme::lerp_color(huth, theme::CHALK, 0.25) }
                else if seq_active { theme::lerp_color(huth, theme::SMOKE, 0.35) }
                else               { theme::lerp_color(huth, Color32::from_rgb(62, 62, 62), 0.80) }
            } else {
                if active { classic_active } else { classic_white_inactive }
            };

            painter.rect_filled(key_rect, egui::Rounding::same(1.0), fill);
            painter.rect_stroke(key_rect, egui::Rounding::same(1.0),
                Stroke::new(0.5, theme::SLATE));

            // Label — all white keys when labels are on
            if show_label {
                let lbl = note_name(note);
                // For non-C notes, trim the octave number to save space
                let lbl_short = if semi == 0 { lbl } else { &lbl[..lbl.len()-1] };
                let label_color = if active {
                    if use_color { theme::VOID } else { theme::DEEP }
                } else {
                    theme::ASH
                };
                painter.text(
                    Pos2::new(x + wk_w * 0.5, oy + wk_h - 9.0),
                    egui::Align2::CENTER_CENTER,
                    lbl_short,
                    egui::FontId::monospace(7.0),
                    label_color,
                );
            }

            // Click detection — white keys
            if let Some(cp) = click_pos {
                if key_rect.contains(cp) { clicked_note = Some(note); }
            }

            white_idx += 1;
        }

        // ── Black keys (drawn on top) ─────────────────────────────────────────
        for oct in 0..N_OCTAVES {
            for &(semi, wk_off) in BLACK_KEYS {
                let note = START_NOTE + oct as u8 * 12 + semi;
                if note > END_NOTE { continue; }

                let white_oct_start = ox + oct as f32 * 7.0 * wk_w;
                let x = white_oct_start + wk_off * wk_w - bk_w * 0.5;
                let key_rect = Rect::from_min_size(
                    Pos2::new(x, oy),
                    Vec2::new(bk_w, bk_h),
                );

                let pressed    = self.pressed_notes.contains(&note);
                let seq_active = seq_note == Some(note);
                let active     = pressed || seq_active;

                let fill: Color32 = if use_color {
                    let huth = theme::note_color(note);
                    if pressed       { theme::lerp_color(huth, theme::CHALK, 0.15) }
                    else if seq_active { huth }
                    else               { theme::lerp_color(huth, theme::PIT, 0.82) }
                } else {
                    if active { classic_active } else { classic_black_inactive }
                };

                painter.rect_filled(key_rect, egui::Rounding::same(1.0), fill);
                painter.rect_stroke(key_rect, egui::Rounding::same(1.0),
                    Stroke::new(0.5, theme::SLATE));

                // Label — sharp name only (no octave number, key is too narrow)
                if show_label {
                    // e.g. "C#" from "C#3"
                    let full = note_name(note);
                    let sharp = &full[..full.len()-1]; // strip octave digit
                    let label_color = if active { theme::VOID } else { theme::IRON };
                    painter.text(
                        Pos2::new(x + bk_w * 0.5, oy + bk_h - 8.0),
                        egui::Align2::CENTER_CENTER,
                        sharp,
                        egui::FontId::monospace(6.0),
                        label_color,
                    );
                }

                // Click detection — black keys take priority
                if let Some(cp) = click_pos {
                    if key_rect.contains(cp) { clicked_note = Some(note); }
                }
            }
        }

        // ── MIDI device label (right side, subtle) ────────────────────────────
        if let Some(ref port) = self.midi_port {
            let short = port.trim().split(' ').next().unwrap_or(port);
            painter.text(
                Pos2::new(rect.max.x - 4.0, oy + wk_h - 4.0),
                egui::Align2::RIGHT_BOTTOM,
                short,
                egui::FontId::monospace(7.5),
                theme::IRON,
            );
        }

        // ── Click-to-play ─────────────────────────────────────────────────────
        if let Some(note) = clicked_note {
            if !self.pressed_notes.contains(&note) {
                self.pressed_notes.insert(note);
                let _ = self.audio_tx.push(AudioCommand::Trigger(
                    TriggerEvent::BassTrigger {
                        note,
                        accent: false,
                        slide: false,
                        gate_samples: 22050,
                    }
                ));
            }
        } else if response.drag_stopped() || (!response.is_pointer_button_down_on() && !self.pressed_notes.is_empty()) {
            // Release all click-triggered notes when pointer lifts
            // (MIDI notes are managed by their own NoteOff messages)
            // Only clear notes that aren't from MIDI (we track MIDI separately)
            // Simple heuristic: clear on pointer release
            let _ = self.audio_tx.push(AudioCommand::Trigger(TriggerEvent::BassGateOff));
        }
    }

    // ── FX Chain ─────────────────────────────────────────────────────────────

    fn draw_fx(&mut self, ui: &mut egui::Ui) {
        widgets::section_header(ui, "FX CHAIN");

        // Snapshot all FX values + locked set before any widget call
        let (mut rs, mut rd, mut rm,
             mut dt, mut df, mut dm,
             mut dd, mut dx, mut mv,
             locked) = {
            let s = self.state.read();
            (s.fx.reverb_size, s.fx.reverb_damp, s.fx.reverb_mix,
             s.fx.delay_time, s.fx.delay_feedback, s.fx.delay_mix,
             s.fx.distortion_drive, s.fx.distortion_mix, s.fx.master_volume,
             s.llm.locked_params.clone())
        };

        let mut new_locks: Vec<&str> = Vec::new();
        let mut changed = false;

        let use_sliders = self.use_sliders;
        ui.horizontal_wrapped(|ui| {
            ui.group(|ui| {
                ui.label(egui::RichText::new("REVERB").color(theme::FOG).monospace().size(9.5));
                let l_rs = locked.contains("fx.reverb_size");
                let l_rm = locked.contains("fx.reverb_mix");
                if widgets::param_control(ui, "SIZE", &mut rs, l_rs,  use_sliders) { if !l_rs { new_locks.push("fx.reverb_size"); } changed = true; }
                if widgets::param_control(ui, "DAMP", &mut rd, false, use_sliders) { changed = true; }
                if widgets::param_control(ui, "MIX",  &mut rm, l_rm,  use_sliders) { if !l_rm { new_locks.push("fx.reverb_mix");  } changed = true; }
            });

            ui.add_space(4.0);

            ui.group(|ui| {
                ui.label(egui::RichText::new("DELAY").color(theme::FOG).monospace().size(9.5));
                if widgets::param_control(ui, "TIME", &mut dt, false, use_sliders) { changed = true; }
                if widgets::param_control(ui, "FDBK", &mut df, false, use_sliders) { changed = true; }
                if widgets::param_control(ui, "MIX",  &mut dm, false, use_sliders) { changed = true; }
            });

            ui.add_space(4.0);

            ui.group(|ui| {
                ui.label(egui::RichText::new("DRIVE / MASTER").color(theme::FOG).monospace().size(9.5));
                if widgets::param_control(ui, "DRIVE",  &mut dd, false, use_sliders) { changed = true; }
                if widgets::param_control(ui, "MIX",    &mut dx, false, use_sliders) { changed = true; }
                if widgets::param_control(ui, "MASTER", &mut mv, false, use_sliders) { changed = true; }
            });
        });

        // Single brief write after all groups are rendered
        if changed {
            let mut s = self.state.write();
            s.fx.reverb_size       = rs;
            s.fx.reverb_damp       = rd;
            s.fx.reverb_mix        = rm;
            s.fx.delay_time        = dt;
            s.fx.delay_feedback    = df;
            s.fx.delay_mix         = dm;
            s.fx.distortion_drive  = dd;
            s.fx.distortion_mix    = dx;
            s.fx.master_volume     = mv;
            for path in new_locks {
                s.llm.locked_params.insert(path.to_string());
            }
            drop(s);
            self.push_audio_params();
        }
    }
}
