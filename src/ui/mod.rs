// ─── ui/mod.rs ────────────────────────────────────────────────────────────────
// Main egui application.

pub mod theme;
pub mod widgets;

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
use crate::state::{AppState, DrumVoice, Waveform, toggle_drum_step, save_project};

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
    prompt_input: String,
    log_text: String,
    active_panel: Panel,
    instruments: Vec<InstrumentSlot>,
    api_port: Option<u16>, // Some(port) if --api was passed
    show_about: bool,
    export_bars: u32,
}

impl ImpulseApp {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        state: Arc<RwLock<AppState>>,
        audio_tx: rtrb::Producer<AudioCommand>,
        llm_tx: Sender<LlmInput>,
        llm_rx: Receiver<LlmOutput>,
        api_port: Option<u16>,
    ) -> Self {
        theme::apply(&cc.egui_ctx);
        let mut log_text = "[ Impulse Instruct ready ]\n".to_string();
        if let Some(port) = api_port {
            log_text.push_str(&format!("[ HTTP API active → http://localhost:{} ]\n", port));
        }
        Self {
            state,
            audio_tx,
            llm_tx,
            llm_rx,
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
            export_bars: 8,
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
            if !out.is_jam && (out.param_update.is_some() || (!out.text.is_empty() && !out.text.starts_with('['))) {
                let display = if let Some(ref update) = out.param_update {
                    // Prefer the natural-language comment; fall back to a terse param summary
                    if let Some(comment) = update.get("_comment").and_then(|v| v.as_str()) {
                        comment.to_string()
                    } else {
                        // Build a short summary of what changed
                        let keys: Vec<&str> = update.as_object()
                            .map(|o| o.keys().map(|k| k.as_str()).collect())
                            .unwrap_or_default();
                        format!("updated {}", keys.join(", "))
                    }
                } else {
                    out.text.clone()
                };
                self.log_text.push_str(&format!("Bonsai → {}\n", display));
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
}

impl eframe::App for ImpulseApp {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.drain_llm_outputs();
        ctx.request_repaint_after(std::time::Duration::from_millis(16));

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
                        let tps = s.llm.tokens_per_sec;
                        let ctx_pct = if s.llm.context_max > 0 {
                            s.llm.context_used as f32 / s.llm.context_max as f32 * 100.0
                        } else { 0.0 };

                        let inf_color = if inferring { theme::CHALK } else { theme::IRON };
                        ui.label(egui::RichText::new("●").color(inf_color).size(10.0));
                        ui.label(egui::RichText::new(
                            format!("{:.0}t/s  ctx:{:.0}%", tps, ctx_pct)
                        ).color(theme::SMOKE).size(9.5).monospace());

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

                    // API link (right-aligned)
                    if let Some(port) = self.api_port {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
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
                        });
                    }
                });
            });

        // ── LLM strip (log + prompt) ──────────────────────────────────────────
        TopBottomPanel::top("llm_strip")
            .frame(Frame::none().fill(theme::PIT).inner_margin(egui::Margin::symmetric(10.0, 6.0)))
            .min_height(80.0)
            .max_height(140.0)
            .show(ctx, |ui| {
                // Full-width prompt input
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

                    if (submit || enter_pressed) && !self.prompt_input.trim().is_empty() {
                        let prompt = self.prompt_input.trim().to_string();
                        self.log_text.push_str(&format!("YOU → {}\n", prompt));
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

        ui.horizontal_wrapped(|ui| {
            if widgets::knob(ui, "CUTOFF",    &mut cutoff,    locked.contains("bass.cutoff"))       { new_locks.push("bass.cutoff");       changed = true; }
            if widgets::knob(ui, "RESONANCE", &mut resonance, locked.contains("bass.resonance"))    { new_locks.push("bass.resonance");    changed = true; }
            if widgets::knob(ui, "ENV MOD",   &mut env_mod,   locked.contains("bass.env_mod"))      { new_locks.push("bass.env_mod");      changed = true; }
            if widgets::knob(ui, "DECAY",     &mut decay,     locked.contains("bass.decay"))        { new_locks.push("bass.decay");        changed = true; }
            if widgets::knob(ui, "ACCENT",    &mut accent,    locked.contains("bass.accent_level")) { new_locks.push("bass.accent_level"); changed = true; }
            if widgets::knob(ui, "DRIVE",     &mut dist,      locked.contains("bass.distortion"))   { new_locks.push("bass.distortion");   changed = true; }
            if widgets::knob(ui, "VOLUME",    &mut vol,       locked.contains("bass.volume"))       { new_locks.push("bass.volume");       changed = true; }
        });

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

        ui.horizontal_wrapped(|ui| {
            ui.group(|ui| {
                ui.label(egui::RichText::new("KICK").color(theme::FOG).monospace().size(9.5));
                if widgets::knob(ui, "PITCH", &mut kp,  false) { changed = true; }
                if widgets::knob(ui, "DECAY", &mut kd,  false) { changed = true; }
                if widgets::knob(ui, "PUNCH", &mut kpu, false) { changed = true; }
                if widgets::knob(ui, "LEVEL", &mut kv,  false) { changed = true; }
            });
            ui.add_space(4.0);
            ui.group(|ui| {
                ui.label(egui::RichText::new("SNARE").color(theme::FOG).monospace().size(9.5));
                if widgets::knob(ui, "TONE",   &mut st,  false) { changed = true; }
                if widgets::knob(ui, "SNAPPY", &mut ssn, false) { changed = true; }
                if widgets::knob(ui, "DECAY",  &mut sd,  false) { changed = true; }
                if widgets::knob(ui, "LEVEL",  &mut sv,  false) { changed = true; }
            });
            ui.add_space(4.0);
            ui.group(|ui| {
                ui.label(egui::RichText::new("HIHAT").color(theme::FOG).monospace().size(9.5));
                if widgets::knob(ui, "CLOSED", &mut hcd, false) { changed = true; }
                if widgets::knob(ui, "OPEN",   &mut hod, false) { changed = true; }
                if widgets::knob(ui, "LEVEL",  &mut hv,  false) { changed = true; }
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

        ui.horizontal_wrapped(|ui| {
            ui.group(|ui| {
                ui.label(egui::RichText::new("KICK").color(theme::FOG).monospace().size(9.5));
                if widgets::knob(ui, "PITCH", &mut kp,  false) { changed = true; }
                if widgets::knob(ui, "DECAY", &mut kd,  false) { changed = true; }
                if widgets::knob(ui, "PUNCH", &mut kpu, false) { changed = true; }
                if widgets::knob(ui, "LEVEL", &mut kv,  false) { changed = true; }
            });
            ui.add_space(4.0);
            ui.group(|ui| {
                ui.label(egui::RichText::new("SNARE").color(theme::FOG).monospace().size(9.5));
                if widgets::knob(ui, "TONE",   &mut st,  false) { changed = true; }
                if widgets::knob(ui, "SNAPPY", &mut ssn, false) { changed = true; }
                if widgets::knob(ui, "DECAY",  &mut sd,  false) { changed = true; }
                if widgets::knob(ui, "LEVEL",  &mut sv,  false) { changed = true; }
            });
            ui.add_space(4.0);
            ui.group(|ui| {
                ui.label(egui::RichText::new("CLAP / RIM").color(theme::FOG).monospace().size(9.5));
                if widgets::knob(ui, "CLAP DEC", &mut cd, false) { changed = true; }
                if widgets::knob(ui, "CLAP LVL", &mut cv, false) { changed = true; }
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

        ui.horizontal_wrapped(|ui| {
            ui.group(|ui| {
                ui.label(egui::RichText::new("REVERB").color(theme::FOG).monospace().size(9.5));
                let l_rs = locked.contains("fx.reverb_size");
                let l_rm = locked.contains("fx.reverb_mix");
                if widgets::knob(ui, "SIZE", &mut rs, l_rs) { if !l_rs { new_locks.push("fx.reverb_size"); } changed = true; }
                if widgets::knob(ui, "DAMP", &mut rd, false) { changed = true; }
                if widgets::knob(ui, "MIX",  &mut rm, l_rm) { if !l_rm { new_locks.push("fx.reverb_mix");  } changed = true; }
            });

            ui.add_space(4.0);

            ui.group(|ui| {
                ui.label(egui::RichText::new("DELAY").color(theme::FOG).monospace().size(9.5));
                if widgets::knob(ui, "TIME", &mut dt, false) { changed = true; }
                if widgets::knob(ui, "FDBK", &mut df, false) { changed = true; }
                if widgets::knob(ui, "MIX",  &mut dm, false) { changed = true; }
            });

            ui.add_space(4.0);

            ui.group(|ui| {
                ui.label(egui::RichText::new("DRIVE / MASTER").color(theme::FOG).monospace().size(9.5));
                if widgets::knob(ui, "DRIVE",  &mut dd, false) { changed = true; }
                if widgets::knob(ui, "MIX",    &mut dx, false) { changed = true; }
                if widgets::knob(ui, "MASTER", &mut mv, false) { changed = true; }
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
