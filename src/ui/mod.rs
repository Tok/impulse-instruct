// ─── ui/mod.rs ────────────────────────────────────────────────────────────────
// Main egui application.

mod header;
mod llm_strip;
pub mod panels;
pub mod theme;
pub mod widgets;
mod windows;

/// Convert a dot-path + float value into a nested JSON object.
/// "bass.cutoff", 0.4  →  {"bass": {"cutoff": 0.4}}
fn dot_path_to_json(path: &str, value: f32) -> serde_json::Value {
    let parts: Vec<&str> = path.split('.').collect();
    let leaf = serde_json::json!(value);
    parts
        .iter()
        .rev()
        .fold(leaf, |acc, &key| serde_json::json!({ key: acc }))
}

/// Short note name for a MIDI note number (e.g. 60 → "C4").
pub(crate) fn note_name(midi: u8) -> &'static str {
    const NAMES: &[&str] = &[
        "C1", "C#1", "D1", "D#1", "E1", "F1", "F#1", "G1", "G#1", "A1", "A#1", "B1", "C2", "C#2",
        "D2", "D#2", "E2", "F2", "F#2", "G2", "G#2", "A2", "A#2", "B2", "C3", "C#3", "D3", "D#3",
        "E3", "F3", "F#3", "G3", "G#3", "A3", "A#3", "B3", "C4", "C#4", "D4", "D#4", "E4", "F4",
        "F#4", "G4", "G#4", "A4", "A#4", "B4", "C5", "C#5", "D5", "D#5", "E5", "F5", "F#5", "G5",
        "G#5", "A5", "A#5", "B5", "C6",
    ];
    // MIDI 24 = C1
    let idx = midi.saturating_sub(24) as usize;
    NAMES.get(idx).copied().unwrap_or("?")
}

/// Scan the models/ directory and return paths of all .gguf files found.
pub(super) fn scan_models() -> Vec<String> {
    std::fs::read_dir("models")
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().map(|x| x == "gguf").unwrap_or(false))
        .map(|p| p.to_string_lossy().into_owned())
        .collect()
}

/// Open a URL in the system browser (cross-platform, no extra dep).
pub(super) fn webbrowser_open(url: &str) -> std::io::Result<()> {
    #[cfg(target_os = "linux")]
    std::process::Command::new("xdg-open").arg(url).spawn()?;
    #[cfg(target_os = "macos")]
    std::process::Command::new("open").arg(url).spawn()?;
    #[cfg(target_os = "windows")]
    std::process::Command::new("cmd")
        .args(["/c", "start", url])
        .spawn()?;
    Ok(())
}

use crossbeam_channel::{Receiver, Sender};
use egui::{CentralPanel, Frame, TopBottomPanel};
use parking_lot::RwLock;
use std::sync::Arc;

use crate::audio::{AudioCommand, AudioParams};
use crate::llm::{LlmInput, LlmOutput};
use crate::midi::MidiEvent;
use crate::sequencer::TriggerEvent;
use crate::state::{AppState, ConversationMode};

pub(super) const LOG_LEVELS: &[(&str, log::LevelFilter)] = &[
    ("ERROR", log::LevelFilter::Error),
    ("WARN", log::LevelFilter::Warn),
    ("INFO", log::LevelFilter::Info),
    ("DEBUG", log::LevelFilter::Debug),
];

// Startup prompts live in config.json and are loaded via crate::config.

// ─── Sequencer row layout ─────────────────────────────────────────────────────
pub(crate) const SEQ_LABEL_W: f32 = 72.0;
pub(crate) const SEQ_LABEL_H: f32 = 22.0;
pub(crate) const SEQ_VOL_W: f32 = 52.0;
pub(crate) const SEQ_VOL_H: f32 = 14.0;

// ─── Instrument slot system ───────────────────────────────────────────────────

/// The synthesis character of an instrument module.
/// Determines which draw function is called for `Panel::Instrument(i)`.
#[derive(Clone, Copy, PartialEq)]
enum InstrumentKind {
    AcidBass,   // draw_bass()
    DrumKit808, // draw_kit_a()
    DrumKit909, // draw_kit_b()
    HooverLead, // draw_hoover()
    An1xVoice,  // draw_an1x()
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
    Lfo,
}

// ─── ImpulseApp ───────────────────────────────────────────────────────────────

pub struct ImpulseApp {
    state: Arc<RwLock<AppState>>,
    audio_tx: rtrb::Producer<AudioCommand>,
    scope_rx: rtrb::Consumer<f32>,
    scope_buf: Vec<f32>,
    llm_tx: Sender<LlmInput>,
    llm_rx: Receiver<LlmOutput>,
    midi_rx: Receiver<MidiEvent>,
    midi_port: Option<String>,
    pressed_notes: std::collections::HashSet<u8>,
    /// Note currently held down by the mouse (separate from MIDI-held notes).
    piano_mouse_note: Option<u8>,
    prompt_input: String,
    log_text: String,
    active_panel: Panel,
    instruments: Vec<InstrumentSlot>,
    api_port: Option<u16>,
    show_about: bool,
    show_prefs: bool,
    export_bars: u32,
    ui_volume: f32, // monitor-only gain; never written to state or export
    // Piano preferences
    piano_show_labels: bool,
    // Last chain-of-thought from Bonsai (shown collapsible below the log)
    last_thinking: Option<String>,
    show_thinking: bool,
    // Control layout preference — derived from AppState.ui_prefs each frame
    // Sequencer page (for >16 step patterns)
    seq_page: usize,
    // Voices manually expanded in the sequencer even if they have no active steps
    expanded_seq_voices: std::collections::HashSet<crate::state::DrumVoice>,
    // Copy/paste clipboard for drum patterns (voice → step slice)
    drum_clipboard: Option<(crate::state::DrumVoice, Vec<crate::state::Step>)>,
    // Model selector
    available_models: Vec<String>,
    // System info (GPU/VRAM/RAM) — polled in background thread
    sys_info: std::sync::Arc<std::sync::Mutex<crate::sysinfo::SysInfo>>,
    show_sysinfo: bool,
    // Preferences
    prefs_tab: usize,
    // log_level_idx now persisted in AppState.ui_prefs.log_level_idx
    // Startup hook: fire a prompt once the LLM transitions from initializing to ready
    startup_done: bool,
}

impl ImpulseApp {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        state: Arc<RwLock<AppState>>,
        audio_tx: rtrb::Producer<AudioCommand>,
        scope_rx: rtrb::Consumer<f32>,
        llm_tx: Sender<LlmInput>,
        llm_rx: Receiver<LlmOutput>,
        midi_rx: Receiver<MidiEvent>,
        midi_port: Option<String>,
        api_port: Option<u16>,
    ) -> Self {
        theme::apply(&cc.egui_ctx);

        // ── Load last session from eframe's persistent storage ────────────────
        if let Some(storage) = cc.storage
            && let Some(json) = storage.get_string("session")
            && let Ok(mut loaded) = serde_json::from_str::<AppState>(&json)
        {
            // Reset runtime-only flags; the LLM thread sets them once it's ready.
            loaded.llm.is_mock = false;
            loaded.llm.llm_initializing = true;
            loaded.llm.is_inferring = false;
            loaded.llm.last_response = String::new();
            loaded.sequencer.running = false; // started below
            loaded.sequencer.current_step = 0;
            *state.write() = loaded;
            log::info!("Session restored from last run.");
        }

        // Restore persisted log level from ui_prefs
        {
            let idx = state.read().ui_prefs.log_level_idx;
            if let Some((_, filter)) = LOG_LEVELS.get(idx) {
                log::set_max_level(*filter);
            }
        }

        // Auto-start sequencer so there's always audio from the first frame.
        {
            let mut s = state.write();
            s.sequencer.running = true;
        }

        let mut log_text = "[ Impulse Instruct ready ]\n".to_string();
        if let Some(ref port) = midi_port {
            log_text.push_str(&format!("[ MIDI: {} ]\n", port));
        } else {
            log_text.push_str("[ MIDI: no device found ]\n");
        }
        if let Some(port) = api_port {
            log_text.push_str(&format!(
                "[ HTTP API active → http://localhost:{} ]\n",
                port
            ));
        }
        Self {
            state,
            audio_tx,
            scope_rx,
            scope_buf: Vec::new(),
            llm_tx,
            llm_rx,
            midi_rx,
            midi_port,
            pressed_notes: std::collections::HashSet::new(),
            piano_mouse_note: None,
            prompt_input: String::new(),
            log_text,
            active_panel: Panel::Sequencer,
            instruments: vec![
                InstrumentSlot {
                    label: "BASS SYNTH",
                    kind: InstrumentKind::AcidBass,
                },
                InstrumentSlot {
                    label: "DRUM KIT A",
                    kind: InstrumentKind::DrumKit808,
                },
                InstrumentSlot {
                    label: "DRUM KIT B",
                    kind: InstrumentKind::DrumKit909,
                },
                InstrumentSlot {
                    label: "HOOVER",
                    kind: InstrumentKind::HooverLead,
                },
                InstrumentSlot {
                    label: "AN1X",
                    kind: InstrumentKind::An1xVoice,
                },
            ],
            api_port,
            show_about: false,
            show_prefs: false,
            export_bars: 8,
            ui_volume: 1.0,
            piano_show_labels: true,
            last_thinking: None,
            show_thinking: false,
            seq_page: 0,
            expanded_seq_voices: std::collections::HashSet::new(),
            drum_clipboard: None,
            available_models: Vec::new(),
            sys_info: {
                let shared =
                    std::sync::Arc::new(std::sync::Mutex::new(crate::sysinfo::SysInfo::default()));
                crate::sysinfo::spawn_poller(std::sync::Arc::clone(&shared), 3);
                shared
            },
            show_sysinfo: false,
            prefs_tab: 0,
            startup_done: false,
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
        let _ = self
            .audio_tx
            .push(AudioCommand::UpdateParams(Box::new(params)));
    }

    /// Drain LLM output messages.
    fn drain_llm_outputs(&mut self) {
        while let Ok(out) = self.llm_rx.try_recv() {
            // Store thinking tokens for display
            if let Some(ref thinking) = out.thinking
                && !thinking.is_empty()
            {
                self.last_thinking = Some(thinking.clone());
            }

            if !out.is_jam
                && (out.param_update.is_some()
                    || (!out.text.is_empty() && !out.text.starts_with('[')))
            {
                let conv_mode = self.state.read().llm.conversation_mode.clone();
                let display = if let Some(ref update) = out.param_update {
                    if conv_mode == ConversationMode::Off {
                        // Off: show only what keys changed, no commentary
                        let keys: Vec<&str> = update
                            .as_object()
                            .map(|o| {
                                o.keys()
                                    .filter(|k| *k != "_comment")
                                    .map(|k| k.as_str())
                                    .collect()
                            })
                            .unwrap_or_default();
                        format!("updated {}", keys.join(", "))
                    } else if let Some(comment) = update.get("_comment").and_then(|v| v.as_str()) {
                        comment.to_string()
                    } else {
                        let keys: Vec<&str> = update
                            .as_object()
                            .map(|o| {
                                o.keys()
                                    .filter(|k| *k != "_comment")
                                    .map(|k| k.as_str())
                                    .collect()
                            })
                            .unwrap_or_default();
                        format!("updated {}", keys.join(", "))
                    }
                } else {
                    out.text.clone()
                };
                // Append thinking indicator when present
                let persona = self.state.read().llm.persona_name.clone();
                let line = if out.thinking.as_ref().is_some_and(|t| !t.is_empty()) {
                    format!("{} → {} [think]\n", persona, display)
                } else {
                    format!("{} → {}\n", persona, display)
                };
                self.log_text.push_str(&line);
                // MC line: shown separately with a marker so it's visually distinct
                if let Some(ref mc) = out.mc_line {
                    self.log_text.push_str(&format!("◆ {}\n", mc));
                }
            }
            // If jam cycle done and auto_jam is on, re-trigger
            if out.text == "[jam_cycle_done]" {
                let auto_jam = self.state.read().llm.auto_jam;
                if auto_jam {
                    let _ = self.llm_tx.try_send(LlmInput::Infer {
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
                MidiEvent::NoteOn {
                    note, velocity: 0, ..
                } => {
                    // NoteOn with vel=0 is standard MIDI running-status NoteOff
                    self.pressed_notes.remove(&note);
                    let _ = self
                        .audio_tx
                        .push(AudioCommand::Trigger(TriggerEvent::BassGateOff));
                }

                MidiEvent::NoteOn { note, velocity, .. } => {
                    self.pressed_notes.insert(note);
                    let vel = velocity as f32 / 127.0;

                    let _ = self
                        .audio_tx
                        .push(AudioCommand::Trigger(TriggerEvent::BassTrigger {
                            note,
                            accent: vel > 0.8,
                            slide: false,
                            gate_samples: 22050, // ~0.5 s at 44100 Hz
                        }));

                    // Write note into current step so you can step-program live.
                    let step = self.state.read().sequencer.current_step;
                    let s = self.state.read().clone();
                    let was_active = s
                        .sequencer
                        .bass_pattern
                        .get(step)
                        .map(|b| b.active)
                        .unwrap_or(false);
                    *self.state.write() = crate::state::set_bass_step(s, step, note, was_active);
                }

                MidiEvent::NoteOff { note, .. } => {
                    self.pressed_notes.remove(&note);
                    let _ = self
                        .audio_tx
                        .push(AudioCommand::Trigger(TriggerEvent::BassGateOff));
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
    /// Persist synth state so the next launch resumes from the same session.
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        let s = self.state.read().clone();
        if let Ok(json) = serde_json::to_string(&s) {
            storage.set_string("session", json);
        }
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.drain_llm_outputs();
        self.drain_midi_events();

        // ── Startup hook ──────────────────────────────────────────────────────
        // Fire once — right after the LLM transitions from initializing to ready.
        if !self.startup_done && !self.state.read().llm.llm_initializing {
            self.startup_done = true;
            if let Some(prompt) = crate::config::random_startup_prompt() {
                let _ = self.llm_tx.try_send(LlmInput::Infer {
                    prompt: prompt.to_string(),
                    one_shot: true,
                });
                log::info!("Startup prompt: {}", prompt);
            }
        }

        ctx.request_repaint_after(std::time::Duration::from_millis(16));

        // ── Drain scope ring buffer ────────────────────────────────────────────
        while let Ok(s) = self.scope_rx.pop() {
            self.scope_buf.push(s);
        }
        if self.scope_buf.len() > 512 {
            let drain = self.scope_buf.len() - 512;
            self.scope_buf.drain(..drain);
        }

        // ── Floating windows (prefs / about / sysinfo) ────────────────────────
        self.draw_windows(ctx);

        // ── Menu bar + Header ─────────────────────────────────────────────────
        self.draw_menu_and_header(ctx);

        // ── LLM interaction strip ────────────────────────────────────────────
        self.draw_llm_strip(ctx);

        // ── Oscilloscope strip ────────────────────────────────────────────────
        TopBottomPanel::top("scope")
            .frame(
                Frame::none()
                    .fill(theme::PIT)
                    .inner_margin(egui::Margin::symmetric(8.0, 4.0)),
            )
            .exact_height(48.0)
            .show(ctx, |ui| {
                self.draw_scope(ui);
            });

        // ── Tab bar (data-driven — add InstrumentSlot to self.instruments to extend) ──
        TopBottomPanel::top("tabs")
            .frame(
                Frame::none()
                    .fill(theme::DEEP)
                    .inner_margin(egui::Margin::symmetric(8.0, 2.0)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    // Fixed left tab
                    let tab = |ui: &mut egui::Ui,
                               panel: Panel,
                               label: &str,
                               active_panel: Panel|
                     -> bool {
                        let active = active_panel == panel;
                        let color = if active { theme::CHALK } else { theme::IRON };
                        let fill = if active { theme::SLATE } else { theme::DEEP };
                        ui.add(
                            egui::Button::new(
                                egui::RichText::new(label)
                                    .color(color)
                                    .monospace()
                                    .size(9.5),
                            )
                            .fill(fill)
                            .stroke(egui::Stroke::new(
                                1.0,
                                if active { theme::ASH } else { theme::VOID },
                            )),
                        )
                        .clicked()
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
                    if tab(ui, Panel::Lfo, "LFO", self.active_panel) {
                        self.active_panel = Panel::Lfo;
                    }
                });
            });

        // ── Footer ────────────────────────────────────────────────────────────
        TopBottomPanel::bottom("footer")
            .frame(
                Frame::none()
                    .fill(theme::VOID)
                    .inner_margin(egui::Margin::symmetric(6.0, 2.0)),
            )
            .exact_height(18.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let midi_text = match &self.midi_port {
                        Some(port) => format!("MIDI: {}", port.trim()),
                        None => "MIDI: no device".to_string(),
                    };
                    ui.label(
                        egui::RichText::new(midi_text)
                            .color(theme::IRON)
                            .monospace()
                            .size(9.0),
                    );
                });
            });

        TopBottomPanel::bottom("piano")
            .frame(
                Frame::none()
                    .fill(theme::VOID)
                    .inner_margin(egui::Margin::symmetric(0.0, 0.0)),
            )
            .exact_height(80.0)
            .show(ctx, |ui| {
                panels::draw_piano(self, ui, ctx);
            });

        // ── Main content ──────────────────────────────────────────────────────
        CentralPanel::default()
            .frame(
                Frame::none()
                    .fill(theme::DEEP)
                    .inner_margin(egui::Margin::same(8.0)),
            )
            .show(ctx, |ui| match self.active_panel {
                Panel::Sequencer => panels::draw_sequencer(self, ui),
                Panel::Instrument(i) => {
                    let kind = self.instruments[i].kind;
                    match kind {
                        InstrumentKind::AcidBass => panels::draw_bass(self, ui),
                        InstrumentKind::DrumKit808 => panels::draw_kit_a(self, ui),
                        InstrumentKind::DrumKit909 => panels::draw_kit_b(self, ui),
                        InstrumentKind::HooverLead => panels::draw_hoover(self, ui),
                        InstrumentKind::An1xVoice => panels::draw_an1x(self, ui),
                    }
                }
                Panel::Fx => panels::draw_fx(self, ui),
                Panel::Lfo => panels::draw_lfo(self, ui),
            });
    }
}

// ─── Panel drawing ────────────────────────────────────────────────────────────

impl ImpulseApp {
    fn draw_scope(&self, ui: &mut egui::Ui) {
        let (rect, _) =
            ui.allocate_exact_size(egui::vec2(ui.available_width(), 40.0), egui::Sense::hover());
        if !ui.is_rect_visible(rect) {
            return;
        }
        let painter = ui.painter();
        painter.rect_filled(rect, egui::Rounding::ZERO, theme::PIT);
        painter.rect_stroke(
            rect,
            egui::Rounding::ZERO,
            egui::Stroke::new(1.0, theme::SLATE),
        );

        let n = self.scope_buf.len();
        if n < 2 {
            return;
        }
        let w = rect.width();
        let h = rect.height();
        let mid = rect.center().y;
        let amp = h * 0.45;

        let points: Vec<egui::Pos2> = self
            .scope_buf
            .iter()
            .enumerate()
            .map(|(i, &s)| {
                let x = rect.min.x + (i as f32 / (n - 1) as f32) * w;
                let y = mid - s.clamp(-1.0, 1.0) * amp;
                egui::Pos2::new(x, y)
            })
            .collect();

        for i in 0..points.len().saturating_sub(1) {
            painter.line_segment(
                [points[i], points[i + 1]],
                egui::Stroke::new(1.0, theme::CHALK),
            );
        }
    }

    #[allow(dead_code)] // used when panels get individual frames
    fn panel_frame() -> Frame {
        Frame::none()
            .fill(theme::PIT)
            .stroke(egui::Stroke::new(1.0, theme::SLATE))
            .rounding(egui::Rounding::same(3.0))
            .inner_margin(egui::Margin::same(8.0))
    }
}
