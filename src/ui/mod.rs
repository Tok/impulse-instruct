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
use crate::llm::{LlmInput, LlmOutput};
use crate::state::{AppState, DrumVoice, Waveform, toggle_drum_step};

// ─── ImpulseApp ───────────────────────────────────────────────────────────────

pub struct ImpulseApp {
    state: Arc<RwLock<AppState>>,
    audio_tx: rtrb::Producer<AudioCommand>,
    llm_tx: Sender<LlmInput>,
    llm_rx: Receiver<LlmOutput>,
    prompt_input: String,
    log_lines: Vec<String>,
    active_panel: Panel,
    api_port: Option<u16>, // Some(port) if --api was passed
}

#[derive(PartialEq, Clone, Copy)]
enum Panel { Sequencer, Drums808, Drums909, Bass303, Fx }

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
        let mut log_lines = vec!["[ Impulse Instruct ready ]".into()];
        if let Some(port) = api_port {
            log_lines.push(format!("[ HTTP API active → http://localhost:{} ]", port));
        }
        Self {
            state,
            audio_tx,
            llm_tx,
            llm_rx,
            prompt_input: String::new(),
            log_lines,
            active_panel: Panel::Sequencer,
            api_port,
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
            if !out.text.is_empty() && !out.text.starts_with('[') {
                let trimmed = if out.text.len() > 200 {
                    format!("{}…", &out.text[..200])
                } else {
                    out.text.clone()
                };
                self.log_lines.push(format!("LLM → {}", trimmed));
                if self.log_lines.len() > 50 {
                    self.log_lines.remove(0);
                }
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
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.drain_llm_outputs();
        ctx.request_repaint_after(std::time::Duration::from_millis(16));

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
                            self.state.write().sequencer.running = !running;
                        }
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        // API link (right-most, only shown when --api was passed)
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
                            ui.add_space(4.0);
                        }

                        // Prompt submit
                        let submit = ui.button(
                            egui::RichText::new("ASK").monospace().size(10.0)
                        ).clicked();

                        // Jam toggle
                        let jam = {
                            let s = self.state.read();
                            s.llm.auto_jam
                        };
                        let jam_color = if jam { theme::CHALK } else { theme::ASH };
                        if ui.button(egui::RichText::new("JAM").color(jam_color).monospace().size(10.0)).clicked() {
                            let mut s = self.state.write();
                            s.llm.auto_jam = !s.llm.auto_jam;
                            if s.llm.auto_jam {
                                let _ = self.llm_tx.try_send(LlmInput {
                                    prompt: "start jamming".to_string(),
                                    one_shot: false,
                                });
                            }
                        }

                        ui.add_space(4.0);

                        // Text input
                        let response = ui.add(
                            egui::TextEdit::singleline(&mut self.prompt_input)
                                .hint_text("prompt the model…")
                                .desired_width(320.0)
                                .font(egui::FontId::monospace(10.5))
                        );

                        let enter_pressed = response.lost_focus()
                            && ctx.input(|i| i.key_pressed(egui::Key::Enter));

                        if (submit || enter_pressed) && !self.prompt_input.trim().is_empty() {
                            let prompt = self.prompt_input.trim().to_string();
                            self.log_lines.push(format!("YOU → {}", prompt));
                            let _ = self.llm_tx.try_send(LlmInput { prompt, one_shot: true });
                            self.prompt_input.clear();
                        }
                    });
                });
            });

        // ── Tab bar ───────────────────────────────────────────────────────────
        TopBottomPanel::top("tabs")
            .frame(Frame::none().fill(theme::DEEP).inner_margin(egui::Margin::symmetric(8.0, 2.0)))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    for (panel, label) in [
                        (Panel::Sequencer, "SEQUENCER"),
                        (Panel::Bass303,   "303 BASS"),
                        (Panel::Drums808,  "808 DRUMS"),
                        (Panel::Drums909,  "909 DRUMS"),
                        (Panel::Fx,        "FX CHAIN"),
                    ] {
                        let active = self.active_panel == panel;
                        let color = if active { theme::CHALK } else { theme::IRON };
                        let fill = if active { theme::SLATE } else { theme::DEEP };
                        if ui.add(
                            egui::Button::new(egui::RichText::new(label).color(color).monospace().size(9.5))
                                .fill(fill)
                                .stroke(egui::Stroke::new(1.0, if active { theme::ASH } else { theme::VOID }))
                        ).clicked() {
                            self.active_panel = panel;
                        }
                    }
                });
            });

        // ── Log strip ─────────────────────────────────────────────────────────
        TopBottomPanel::bottom("log")
            .frame(Frame::none().fill(theme::VOID).inner_margin(egui::Margin::symmetric(10.0, 4.0)))
            .min_height(30.0)
            .max_height(60.0)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().stick_to_bottom(true).show(ui, |ui| {
                    for line in &self.log_lines {
                        ui.label(egui::RichText::new(line).color(theme::SMOKE).monospace().size(9.0));
                    }
                });
            });

        // ── Main content ──────────────────────────────────────────────────────
        CentralPanel::default()
            .frame(Frame::none().fill(theme::DEEP).inner_margin(egui::Margin::same(8.0)))
            .show(ctx, |ui| {
                match self.active_panel {
                    Panel::Sequencer => self.draw_sequencer(ui),
                    Panel::Bass303   => self.draw_303(ui),
                    Panel::Drums808  => self.draw_808(ui),
                    Panel::Drums909  => self.draw_909(ui),
                    Panel::Fx        => self.draw_fx(ui),
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
        let current_step = self.state.read().sequencer.current_step;

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
                        let is_current = i == current_step;
                        let vel = pattern[i].velocity;
                        let enabled = i < step_count;

                        ui.add_enabled_ui(enabled, |ui| {
                            if widgets::step_button(ui, is_active, is_current, vel) {
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
            ui.add_space(6.0);
            ui.separator();
            ui.add_space(4.0);
            ui.label(egui::RichText::new("303 BASS").color(theme::SMOKE).monospace().size(8.5));
            ui.horizontal(|ui| {
                ui.add_sized([80.0, 22.0], egui::Label::new(""));
                let (bass_pattern, step_count) = {
                    let s = self.state.read();
                    (s.sequencer.bass_pattern, s.sequencer.steps)
                };
                for i in 0..num_steps {
                    if i > 0 && i % 4 == 0 { ui.add_space(2.0); }
                    let is_active = bass_pattern[i].active;
                    let is_current = i == current_step;
                    ui.add_enabled_ui(i < step_count, |ui| {
                        if widgets::step_button(ui, is_active, is_current, 1.0) {
                            let mut s = self.state.write();
                            s.sequencer.bass_pattern[i].active = !s.sequencer.bass_pattern[i].active;
                        }
                    });
                }
            });
        });
    }

    // ── 303 Bass ─────────────────────────────────────────────────────────────

    fn draw_303(&mut self, ui: &mut egui::Ui) {
        widgets::section_header(ui, "TB-303 BASS SYNTHESIZER");

        let locked = self.state.read().llm.locked_params.clone();

        ui.horizontal_wrapped(|ui| {
            let mut changed = false;
            {
                let mut s = self.state.write();
                let cutoff = s.tb303.cutoff;
                let resonance = s.tb303.resonance;
                let env_mod = s.tb303.env_mod;
                let decay = s.tb303.decay;
                let accent = s.tb303.accent_level;
                let dist = s.tb303.distortion;
                let vol = s.tb303.volume;

                let mut c = cutoff;   if widgets::knob(ui, "CUTOFF",   &mut c, locked.contains("tb303.cutoff"))   { s.tb303.cutoff = c;      s.llm.locked_params.insert("tb303.cutoff".into());      changed = true; }
                let mut r = resonance;if widgets::knob(ui, "RESONANCE",&mut r, locked.contains("tb303.resonance")){ s.tb303.resonance = r;  s.llm.locked_params.insert("tb303.resonance".into());  changed = true; }
                let mut e = env_mod;  if widgets::knob(ui, "ENV MOD",  &mut e, locked.contains("tb303.env_mod"))  { s.tb303.env_mod = e;     s.llm.locked_params.insert("tb303.env_mod".into());     changed = true; }
                let mut d = decay;    if widgets::knob(ui, "DECAY",    &mut d, locked.contains("tb303.decay"))    { s.tb303.decay = d;       s.llm.locked_params.insert("tb303.decay".into());       changed = true; }
                let mut a = accent;   if widgets::knob(ui, "ACCENT",   &mut a, locked.contains("tb303.accent_level")) { s.tb303.accent_level = a; s.llm.locked_params.insert("tb303.accent_level".into()); changed = true; }
                let mut ds = dist;    if widgets::knob(ui, "DRIVE",    &mut ds, locked.contains("tb303.distortion")) { s.tb303.distortion = ds; s.llm.locked_params.insert("tb303.distortion".into()); changed = true; }
                let mut v = vol;      if widgets::knob(ui, "VOLUME",   &mut v, locked.contains("tb303.volume"))  { s.tb303.volume = v;      s.llm.locked_params.insert("tb303.volume".into());      changed = true; }
            }

            if changed { self.push_audio_params(); }
        });

        ui.add_space(8.0);

        // Waveform toggle
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("WAVE").color(theme::SMOKE).monospace().size(9.0));
            let waveform = self.state.read().tb303.waveform.clone();
            let saw_active = waveform == Waveform::Saw;
            let mut saw = saw_active;
            if widgets::toggle_button(ui, "SAW", &mut saw) {
                self.state.write().tb303.waveform = Waveform::Saw;
                self.push_audio_params();
            }
            let mut sq = !saw_active;
            if widgets::toggle_button(ui, "SQR", &mut sq) {
                self.state.write().tb303.waveform = Waveform::Square;
                self.push_audio_params();
            }
        });

        ui.add_space(12.0);

        // Locked params management
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("LOCKED:").color(theme::SMOKE).monospace().size(8.5));
            let locked = self.state.read().llm.locked_params.clone();
            if locked.is_empty() {
                ui.label(egui::RichText::new("none (LLM controls all)").color(theme::IRON).monospace().size(8.5));
            } else {
                let params: Vec<String> = locked.iter()
                    .filter(|p| p.starts_with("tb303"))
                    .cloned().collect();
                for p in &params {
                    let short = p.replace("tb303.", "");
                    if ui.small_button(egui::RichText::new(format!("× {}", short)).monospace().size(8.0)).clicked() {
                        self.state.write().llm.locked_params.remove(p);
                    }
                }
            }
            if ui.small_button(egui::RichText::new("UNLOCK ALL").monospace().size(8.0)).clicked() {
                let mut s = self.state.write();
                s.llm.locked_params.retain(|p| !p.starts_with("tb303"));
            }
        });
    }

    // ── 808 Drums ────────────────────────────────────────────────────────────

    fn draw_808(&mut self, ui: &mut egui::Ui) {
        widgets::section_header(ui, "TR-808 DRUM MACHINE");

        let mut changed = false;

        ui.horizontal_wrapped(|ui| {
            // Kick
            ui.group(|ui| {
                ui.label(egui::RichText::new("KICK").color(theme::FOG).monospace().size(9.5));
                let mut s = self.state.write();
                let mut p = s.tr808.kick.pitch;
                let mut d = s.tr808.kick.decay;
                let mut pu = s.tr808.kick.punch;
                let mut v = s.tr808.kick.volume;
                if widgets::knob(ui, "PITCH", &mut p, false) { s.tr808.kick.pitch = p; changed = true; }
                if widgets::knob(ui, "DECAY", &mut d, false) { s.tr808.kick.decay = d; changed = true; }
                if widgets::knob(ui, "PUNCH", &mut pu, false) { s.tr808.kick.punch = pu; changed = true; }
                if widgets::knob(ui, "LEVEL", &mut v, false) { s.tr808.kick.volume = v; changed = true; }
            });

            ui.add_space(4.0);

            // Snare
            ui.group(|ui| {
                ui.label(egui::RichText::new("SNARE").color(theme::FOG).monospace().size(9.5));
                let mut s = self.state.write();
                let mut t = s.tr808.snare.tone;
                let mut sn = s.tr808.snare.snappy;
                let mut d = s.tr808.snare.decay;
                let mut v = s.tr808.snare.volume;
                if widgets::knob(ui, "TONE",   &mut t, false) { s.tr808.snare.tone = t; changed = true; }
                if widgets::knob(ui, "SNAPPY", &mut sn, false) { s.tr808.snare.snappy = sn; changed = true; }
                if widgets::knob(ui, "DECAY",  &mut d, false) { s.tr808.snare.decay = d; changed = true; }
                if widgets::knob(ui, "LEVEL",  &mut v, false) { s.tr808.snare.volume = v; changed = true; }
            });

            ui.add_space(4.0);

            // Hihats
            ui.group(|ui| {
                ui.label(egui::RichText::new("HIHAT").color(theme::FOG).monospace().size(9.5));
                let mut s = self.state.write();
                let mut cd = s.tr808.hihat_closed.decay;
                let mut od = s.tr808.hihat_open.decay;
                let mut v = s.tr808.hihat_closed.volume;
                if widgets::knob(ui, "CLOSED", &mut cd, false) { s.tr808.hihat_closed.decay = cd; changed = true; }
                if widgets::knob(ui, "OPEN",   &mut od, false) { s.tr808.hihat_open.decay = od; changed = true; }
                if widgets::knob(ui, "LEVEL",  &mut v, false) { s.tr808.hihat_closed.volume = v; s.tr808.hihat_open.volume = v; changed = true; }
            });
        });

        if changed { self.push_audio_params(); }
    }

    // ── 909 Drums ────────────────────────────────────────────────────────────

    fn draw_909(&mut self, ui: &mut egui::Ui) {
        widgets::section_header(ui, "TR-909 DRUM MACHINE");

        let mut changed = false;

        ui.horizontal_wrapped(|ui| {
            ui.group(|ui| {
                ui.label(egui::RichText::new("KICK").color(theme::FOG).monospace().size(9.5));
                let mut s = self.state.write();
                let mut p  = s.tr909.kick.pitch;
                let mut d  = s.tr909.kick.decay;
                let mut pu = s.tr909.kick.punch;
                let mut v  = s.tr909.kick.volume;
                if widgets::knob(ui, "PITCH", &mut p,  false) { s.tr909.kick.pitch  = p;  changed = true; }
                if widgets::knob(ui, "DECAY", &mut d,  false) { s.tr909.kick.decay  = d;  changed = true; }
                if widgets::knob(ui, "PUNCH", &mut pu, false) { s.tr909.kick.punch  = pu; changed = true; }
                if widgets::knob(ui, "LEVEL", &mut v,  false) { s.tr909.kick.volume = v;  changed = true; }
            });
            ui.add_space(4.0);
            ui.group(|ui| {
                ui.label(egui::RichText::new("SNARE").color(theme::FOG).monospace().size(9.5));
                let mut s = self.state.write();
                let mut t  = s.tr909.snare.tone;
                let mut sn = s.tr909.snare.snappy;
                let mut d  = s.tr909.snare.decay;
                let mut v  = s.tr909.snare.volume;
                if widgets::knob(ui, "TONE",   &mut t,  false) { s.tr909.snare.tone   = t;  changed = true; }
                if widgets::knob(ui, "SNAPPY", &mut sn, false) { s.tr909.snare.snappy = sn; changed = true; }
                if widgets::knob(ui, "DECAY",  &mut d,  false) { s.tr909.snare.decay  = d;  changed = true; }
                if widgets::knob(ui, "LEVEL",  &mut v,  false) { s.tr909.snare.volume = v;  changed = true; }
            });
            ui.add_space(4.0);
            ui.group(|ui| {
                ui.label(egui::RichText::new("CLAP / RIM").color(theme::FOG).monospace().size(9.5));
                let mut s = self.state.write();
                let mut cd = s.tr909.clap.decay;
                let mut cv = s.tr909.clap.volume;
                if widgets::knob(ui, "CLAP DEC", &mut cd, false) { s.tr909.clap.decay  = cd; changed = true; }
                if widgets::knob(ui, "CLAP LVL", &mut cv, false) { s.tr909.clap.volume = cv; changed = true; }
            });
        });

        if changed { self.push_audio_params(); }
    }

    // ── FX Chain ─────────────────────────────────────────────────────────────

    fn draw_fx(&mut self, ui: &mut egui::Ui) {
        widgets::section_header(ui, "FX CHAIN");

        let locked = self.state.read().llm.locked_params.clone();
        let mut changed = false;

        ui.horizontal_wrapped(|ui| {
            ui.group(|ui| {
                ui.label(egui::RichText::new("REVERB").color(theme::FOG).monospace().size(9.5));
                let mut s = self.state.write();
                let mut rs = s.fx.reverb_size;
                let mut rd = s.fx.reverb_damp;
                let mut rm = s.fx.reverb_mix;
                let l_rs = locked.contains("fx.reverb_size");
                let l_rm = locked.contains("fx.reverb_mix");
                if widgets::knob(ui, "SIZE", &mut rs, l_rs) { s.fx.reverb_size = rs; if !l_rs { s.llm.locked_params.insert("fx.reverb_size".into()); } changed = true; }
                if widgets::knob(ui, "DAMP", &mut rd, false) { s.fx.reverb_damp = rd; changed = true; }
                if widgets::knob(ui, "MIX",  &mut rm, l_rm) { s.fx.reverb_mix  = rm; if !l_rm { s.llm.locked_params.insert("fx.reverb_mix".into()); } changed = true; }
            });

            ui.add_space(4.0);

            ui.group(|ui| {
                ui.label(egui::RichText::new("DELAY").color(theme::FOG).monospace().size(9.5));
                let mut s = self.state.write();
                let mut dt = s.fx.delay_time;
                let mut df = s.fx.delay_feedback;
                let mut dm = s.fx.delay_mix;
                if widgets::knob(ui, "TIME", &mut dt, false) { s.fx.delay_time = dt; changed = true; }
                if widgets::knob(ui, "FDBK", &mut df, false) { s.fx.delay_feedback = df; changed = true; }
                if widgets::knob(ui, "MIX",  &mut dm, false) { s.fx.delay_mix = dm; changed = true; }
            });

            ui.add_space(4.0);

            ui.group(|ui| {
                ui.label(egui::RichText::new("DRIVE / MASTER").color(theme::FOG).monospace().size(9.5));
                let mut s = self.state.write();
                let mut dd = s.fx.distortion_drive;
                let mut dm = s.fx.distortion_mix;
                let mut mv = s.fx.master_volume;
                if widgets::knob(ui, "DRIVE", &mut dd, false) { s.fx.distortion_drive = dd; changed = true; }
                if widgets::knob(ui, "MIX",   &mut dm, false) { s.fx.distortion_mix = dm;   changed = true; }
                if widgets::knob(ui, "MASTER",&mut mv, false) { s.fx.master_volume = mv;     changed = true; }
            });
        });

        if changed { self.push_audio_params(); }
    }
}
