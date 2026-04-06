// ─── ui/mod.rs ────────────────────────────────────────────────────────────────
// Main egui application.

mod header;
mod llm_strip;
pub mod module_card;
mod note;
pub mod panels;
pub mod rack_cables;
pub mod rack_canvas;
pub(crate) mod rack_content;
pub mod theme;
pub mod widgets;
mod windows;

pub(crate) use note::{ansi_colorize_notes, note_name};

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

/// Scan for .gguf model files. Checks:
///   1. models/ relative to CWD (dev: cargo run from repo root)
///   2. models/ next to the binary (dist: user unpacked the release)
///   3. The repo root itself (for convenience when a .gguf is dropped there)
pub(super) fn scan_models() -> Vec<String> {
    let mut dirs: Vec<std::path::PathBuf> = vec![std::path::PathBuf::from("models")];
    if let Ok(exe) = std::env::current_exe()
        && let Some(parent) = exe.parent()
    {
        let sibling = parent.join("models");
        if sibling != std::path::Path::new("models") {
            dirs.push(sibling);
        }
    }
    let mut found: Vec<String> = dirs
        .into_iter()
        .flat_map(|dir| {
            std::fs::read_dir(&dir)
                .into_iter()
                .flatten()
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| p.extension().map(|x| x == "gguf").unwrap_or(false))
                .map(|p| p.to_string_lossy().into_owned())
                .collect::<Vec<_>>()
        })
        .collect();
    found.sort();
    found.dedup();
    found
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
use crate::state::{AppState, ConversationMode, compile_fx_plan};

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

// ─── MIDI clock BPM tracker ──────────────────────────────────────────────────

/// Derives BPM from incoming MIDI clock pulses (24 per quarter note).
/// Averages the last 8 inter-pulse intervals for stability.
struct MidiClockTracker {
    last: Option<std::time::Instant>,
    intervals: [f64; 8],
    head: usize,
    count: usize,
}

impl MidiClockTracker {
    fn new() -> Self {
        Self {
            last: None,
            intervals: [0.0; 8],
            head: 0,
            count: 0,
        }
    }

    /// Call on each 0xF8 pulse. Returns computed BPM if stable, else None.
    fn on_clock(&mut self) -> Option<f32> {
        let now = std::time::Instant::now();
        if let Some(last) = self.last {
            let secs = now.duration_since(last).as_secs_f64();
            // 10ms = 300 BPM, 300ms ≈ 8 BPM — ignore outliers outside this range.
            if secs > 0.01 && secs < 0.30 {
                self.intervals[self.head] = secs;
                self.head = (self.head + 1) % 8;
                if self.count < 8 {
                    self.count += 1;
                }
                let avg = self.intervals[..self.count].iter().sum::<f64>() / self.count as f64;
                let bpm = 60.0 / (avg * 24.0);
                self.last = Some(now);
                return Some(bpm as f32);
            }
        }
        self.last = Some(now);
        None
    }

    fn reset(&mut self) {
        self.last = None;
        self.count = 0;
        self.head = 0;
    }
}

// ─── Undo / redo history ─────────────────────────────────────────────────────

const HISTORY_DEPTH: usize = 50;

/// Ring-buffer undo/redo stack for `AppState` snapshots.
struct StateHistory {
    past: std::collections::VecDeque<crate::state::AppState>,
    future: Vec<crate::state::AppState>,
}

impl StateHistory {
    fn new() -> Self {
        Self {
            past: std::collections::VecDeque::with_capacity(HISTORY_DEPTH),
            future: Vec::new(),
        }
    }

    /// Record a snapshot before a mutation. Clears redo stack.
    fn push(&mut self, snapshot: crate::state::AppState) {
        if self.past.len() >= HISTORY_DEPTH {
            self.past.pop_front();
        }
        self.past.push_back(snapshot);
        self.future.clear();
    }

    /// Undo: restore previous state, push current to redo stack.
    /// Returns the state to restore, or None if nothing to undo.
    fn undo(&mut self, current: crate::state::AppState) -> Option<crate::state::AppState> {
        let prev = self.past.pop_back()?;
        self.future.push(current);
        Some(prev)
    }

    /// Redo: re-apply a previously undone change.
    fn redo(&mut self, current: crate::state::AppState) -> Option<crate::state::AppState> {
        let next = self.future.pop()?;
        if self.past.len() >= HISTORY_DEPTH {
            self.past.pop_front();
        }
        self.past.push_back(current);
        Some(next)
    }

    fn can_undo(&self) -> bool {
        !self.past.is_empty()
    }
    fn can_redo(&self) -> bool {
        !self.future.is_empty()
    }
}

// ─── ImpulseApp ───────────────────────────────────────────────────────────────

pub struct ImpulseApp {
    state: Arc<RwLock<AppState>>,
    audio_tx: rtrb::Producer<AudioCommand>,
    scope_rx: rtrb::Consumer<f32>,
    scope_buf: Vec<f32>,
    capture_rx: rtrb::Consumer<f32>,
    /// Most-recent audio analysis snapshot. None until the user clicks Listen.
    audio_analysis: Option<crate::audio::analysis::AudioAnalysis>,
    /// True when the most-recently-sent prompt came from the Listen button —
    /// used to label the LLM response as "LISTEN →" in the log.
    listen_pending: bool,
    llm_tx: Sender<LlmInput>,
    llm_rx: Receiver<LlmOutput>,
    midi_rx: Receiver<MidiEvent>,
    midi_port: Option<String>,
    pressed_notes: std::collections::HashSet<u8>,
    /// Note currently held down by the mouse (separate from MIDI-held notes).
    piano_mouse_note: Option<u8>,
    prompt_input: String,
    log_text: String,
    api_port: Option<u16>,
    show_about: bool,
    pub(crate) show_prefs: bool,
    export_bars: u32,
    ui_volume: f32, // monitor-only gain; never written to state or export
    // Piano preferences
    piano_show_labels: bool,
    // Last chain-of-thought from the LLM (shown collapsible below the log)
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
    llm_tab: usize,
    // log_level_idx now persisted in AppState.ui_prefs.log_level_idx
    // Startup hook: fire a prompt once the LLM transitions from initializing to ready
    startup_done: bool,
    // MIDI clock BPM tracker — averages recent pulse intervals to derive tempo.
    midi_clock_tracker: MidiClockTracker,
    // Undo/redo history — ring buffer of recent AppState snapshots.
    history: StateHistory,
    // In-progress cable drag in the rack patch view.
    pub(crate) cable_drag: Option<rack_canvas::CableDrag>,
    // When true, patch cables are drawn over the rack. Tab to toggle.
    pub(crate) show_cables: bool,
    // Zone whose [+ ADD] popup is currently open.
    pub(crate) add_menu_zone: Option<crate::state::Zone>,
    // Module being dragged by its title bar (id + current pointer position).
    pub(crate) module_drag: Option<rack_canvas::ModuleDrag>,
    // Auto-save: set when rack or session-worthy state changes; saved next frame.
    pub(crate) session_dirty: bool,
    // Track last-saved rack cable/module count to detect changes.
    last_saved_rack_sig: (usize, usize),
    // Timestamp of the most recent actual save (for interval throttling).
    last_save_time: std::time::Instant,
    // Auto-listen: when enabled, trigger LISTEN automatically every N jam cycles.
    auto_listen: bool,
    // Counts jam cycles since the last auto-listen trigger.
    auto_listen_counter: u32,
    // When jam_bars > 0, this holds the Instant when the next jam cycle should fire.
    jam_next_fire: Option<std::time::Instant>,
    // When true the LLM strip collapses to show only the prompt row.
    pub(crate) llm_strip_collapsed: bool,
    // Native pixels_per_point at startup — used as base for ui_scale.
    native_ppp: f32,
    /// Central lock-paint mode: None = normal drag, Some(mode) = click paints that mode.
    pub(crate) touch_mode: Option<crate::state::ParamMode>,
    // Per-zone collapse state.
    pub(crate) zone_global_collapsed: bool,
    pub(crate) zone_voice_collapsed: bool,
    pub(crate) zone_fxmod_collapsed: bool,
}

impl ImpulseApp {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        state: Arc<RwLock<AppState>>,
        mut audio_tx: rtrb::Producer<AudioCommand>,
        scope_rx: rtrb::Consumer<f32>,
        capture_rx: rtrb::Consumer<f32>,
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

        // Pre-load amen WAV if path is set in restored session.
        {
            let path = state.read().amen.path.clone();
            if !path.is_empty()
                && let Some(data) = crate::audio::load_wav_to_44100(&path)
            {
                let _ = audio_tx.push(AudioCommand::LoadSampler(data));
            }
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
            capture_rx,
            audio_analysis: None,
            listen_pending: false,
            llm_tx,
            llm_rx,
            midi_rx,
            midi_port,
            pressed_notes: std::collections::HashSet::new(),
            piano_mouse_note: None,
            prompt_input: String::new(),
            log_text,
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
            llm_tab: 0,
            startup_done: false,
            midi_clock_tracker: MidiClockTracker::new(),
            history: StateHistory::new(),
            cable_drag: None,
            show_cables: crate::state::load_session()
                .and_then(|s| s.show_cables)
                .unwrap_or(true),
            add_menu_zone: None,
            module_drag: None,
            session_dirty: false,
            last_saved_rack_sig: (0, 0),
            last_save_time: std::time::Instant::now(),
            auto_listen: false,
            auto_listen_counter: 0,
            jam_next_fire: None,
            llm_strip_collapsed: false,
            native_ppp: 0.0, // captured on first frame after DPI is established
            touch_mode: None,
            zone_global_collapsed: false,
            zone_voice_collapsed: false,
            zone_fxmod_collapsed: false,
        }
    }

    /// Push any pending audio param snapshot to the audio thread.
    /// Record the current state to the undo history before a mutation.
    pub(crate) fn push_history(&mut self) {
        let snapshot = self.state.read().clone();
        self.history.push(snapshot);
    }

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

    /// Recompile the FX routing plan from the current rack cable graph and
    /// send it to the audio thread.  Call whenever rack topology changes
    /// (cable connect/disconnect, module enable/disable, module remove).
    pub(crate) fn push_fx_plan(&mut self) {
        let plan = {
            let s = self.state.read();
            compile_fx_plan(&s.rack)
        };
        let _ = self.audio_tx.push(AudioCommand::SetFxPlan(plan));
    }

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
            });
            self.audio_analysis = Some(analysis);
            self.listen_pending = true;
        } else {
            self.log_text.push_str("LISTEN → no audio captured yet\n");
        }
    }

    /// Drain LLM output messages.
    fn drain_llm_outputs(&mut self) {
        while let Ok(out) = self.llm_rx.try_recv() {
            // Store thinking tokens for display; also echo to log if enabled.
            if let Some(ref thinking) = out.thinking
                && !thinking.is_empty()
            {
                self.last_thinking = Some(thinking.clone());
                let think_persona = self.state.read().llm.persona_name.clone();
                log::info!(
                    "{} (thinking): {}",
                    think_persona,
                    ansi_colorize_notes(thinking)
                );
                if self.state.read().llm.show_thinking_in_log {
                    self.log_text
                        .push_str(&format!("{} (thinking): {}\n", think_persona, thinking));
                }
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
                // Append thinking indicator when present; tag audio-feedback responses
                let persona = if self.listen_pending {
                    self.listen_pending = false;
                    "LISTEN".to_string()
                } else {
                    self.state.read().llm.persona_name.clone()
                };
                let line = if out.thinking.as_ref().is_some_and(|t| !t.is_empty()) {
                    format!("{} -> {} [think]\n", persona, display)
                } else {
                    format!("{} -> {}\n", persona, display)
                };
                log::info!("{}", ansi_colorize_notes(line.trim_end()));
                self.log_text.push_str(&line);
                // MC line: shown separately with a marker so it's visually distinct
                if let Some(ref mc) = out.mc_line {
                    self.log_text.push_str(&format!("◆ {}\n", mc));
                }
            }
            // Jam re-triggers unless heat is at zero (model is parked).
            if out.text == "[jam_cycle_done]" && self.state.read().llm.heat > 0.0 {
                // Advance any scheduled param ramps by one cycle.
                {
                    let cur = self.state.read().clone();
                    let next = crate::state::jam_tools::advance_ramps(cur);
                    *self.state.write() = next;
                }
                // Increment cycle count.
                self.state.write().llm.jam_cycle_count =
                    self.state.read().llm.jam_cycle_count.saturating_add(1);
                // Auto-listen: every 4 jam cycles, inject an audio snapshot.
                if self.auto_listen {
                    self.auto_listen_counter += 1;
                    if self.auto_listen_counter >= 4 {
                        self.auto_listen_counter = 0;
                        self.trigger_listen();
                    }
                }
                // Schedule next cycle: immediately if jam_bars == 0, else after N bars.
                let (jam_bars, bpm) = {
                    let s = self.state.read();
                    (s.llm.jam_bars, s.sequencer.bpm)
                };
                if jam_bars > 0.0 && bpm > 0.0 {
                    let delay_ms = (jam_bars * 240_000.0 / bpm) as u64;
                    self.jam_next_fire = Some(
                        std::time::Instant::now() + std::time::Duration::from_millis(delay_ms),
                    );
                } else {
                    let _ = self.llm_tx.try_send(LlmInput::Infer {
                        prompt: "continue jamming, evolve the pattern".to_string(),
                        one_shot: false,
                    });
                }
            }
            use crate::llm::LlmAction;
            for action in &out.actions {
                match action {
                    LlmAction::SaveProject => {
                        let msg = match crate::state::save_project(&self.state.read().clone()) {
                            Ok(p) => format!("Saved → {}\n", p.display()),
                            Err(e) => format!("Save failed: {e}\n"),
                        };
                        self.log_text.push_str(&msg);
                    }
                    LlmAction::SetHeat(h) if !out.is_jam => {
                        self.state.write().llm.heat = *h;
                        self.session_dirty = true;
                    }
                    LlmAction::SetStyle(sid) if !out.is_jam => {
                        use crate::llm::styles::StyleCatalog;
                        let cat = StyleCatalog::get();
                        let resolved = cat
                            .find_by_id(sid)
                            .or_else(|| {
                                let lo = sid.to_lowercase();
                                cat.styles().iter().find(|s| {
                                    s.id.to_lowercase() == lo || s.name.to_lowercase() == lo
                                })
                            })
                            .map(|s| s.id.clone());
                        if let Some(id) = resolved {
                            self.state.write().llm.active_style = Some(id);
                        }
                        self.session_dirty = true;
                    }
                    LlmAction::SetHeat(_) | LlmAction::SetStyle(_) => {} // jam: ignore
                    LlmAction::SetPersona(p) => {
                        self.state.write().llm.persona_name = p.clone();
                        self.session_dirty = true;
                    }
                    LlmAction::SetConversationMode(m) => {
                        use crate::state::ConversationMode;
                        let mode = match m.to_lowercase().as_str() {
                            "off" => ConversationMode::Off,
                            "dj" => ConversationMode::Dj,
                            "mc" => ConversationMode::Mc,
                            _ => ConversationMode::Producer,
                        };
                        self.state.write().llm.conversation_mode = mode;
                        self.session_dirty = true;
                    }
                    LlmAction::SetJamBars(b) => {
                        self.state.write().llm.jam_bars = *b;
                        self.session_dirty = true;
                    }
                }
            }
            // Push updated params after LLM changed state; record the pre-update
            // snapshot to the undo history so Ctrl+Z can revert an LLM response.
            if out.param_update.is_some() {
                if let Some(before) = out.before_state {
                    self.history.push(*before); // snapshot taken by LLM thread pre-update
                } else {
                    self.push_history(); // fallback: snapshot current state
                }
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
                        .push(AudioCommand::Trigger(TriggerEvent::BassGateOff {
                            voice_idx: 0,
                        }));
                }

                MidiEvent::NoteOn { note, velocity, .. } => {
                    self.pressed_notes.insert(note);
                    let vel = velocity as f32 / 127.0;

                    let _ = self
                        .audio_tx
                        .push(AudioCommand::Trigger(TriggerEvent::BassTrigger {
                            voice_idx: 0,
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
                        .push(AudioCommand::Trigger(TriggerEvent::BassGateOff {
                            voice_idx: 0,
                        }));
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
                    self.midi_clock_tracker.reset();
                    let s = self.state.read().clone();
                    if !s.sequencer.running {
                        *self.state.write() = toggle_sequencer_running(s);
                    }
                }
                MidiEvent::Stop => {
                    self.midi_clock_tracker.reset();
                    let s = self.state.read().clone();
                    if s.sequencer.running {
                        *self.state.write() = toggle_sequencer_running(s);
                    }
                }

                // MIDI clock — derive BPM from pulse timing when sync is on.
                MidiEvent::Clock => {
                    let sync_on = self.state.read().sequencer.midi_clock_sync;
                    if sync_on && let Some(bpm) = self.midi_clock_tracker.on_clock() {
                        self.state.write().sequencer.bpm = bpm.clamp(20.0, 300.0);
                        self.push_audio_params();
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
        // Capture the system's native pixels_per_point on the first real frame,
        // after the window is shown and DPI is known.
        if self.native_ppp <= 0.0 {
            self.native_ppp = ctx.pixels_per_point();
        }
        // Apply persisted UI scale — only override when the user has changed it.
        let ui_scale = self.state.read().ui_prefs.ui_scale;
        if (ui_scale - 1.0).abs() > 0.005 {
            ctx.set_pixels_per_point(self.native_ppp * ui_scale);
        }

        // Publish touch_mode so widgets can read it without signature changes.
        ctx.data_mut(|d| {
            d.insert_temp(egui::Id::new("touch_mode"), self.touch_mode);
        });

        self.drain_llm_outputs();
        self.drain_midi_events();

        // ── Jam timer: fire delayed jam cycle when the bar-count delay elapses ──
        if let Some(fire_at) = self.jam_next_fire {
            if std::time::Instant::now() >= fire_at {
                self.jam_next_fire = None;
                if self.state.read().llm.heat > 0.0 {
                    let _ = self.llm_tx.try_send(LlmInput::Infer {
                        prompt: "continue jamming, evolve the pattern".to_string(),
                        one_shot: false,
                    });
                }
            } else {
                // Still waiting — request a repaint so we check again next frame
                ctx.request_repaint_after(std::time::Duration::from_millis(200));
            }
        }

        // ── Auto-save session when rack or key settings change ────────────────
        {
            let rack = &self.state.read().rack;
            let sig = (
                rack.modules.len() + rack.cables.len() * 100,
                rack.modules
                    .iter()
                    .map(|m| m.slot as usize + m.enabled as usize * 1000)
                    .sum::<usize>(),
            );
            if sig != self.last_saved_rack_sig {
                self.last_saved_rack_sig = sig;
                self.session_dirty = true;
            }
        }
        if self.session_dirty {
            use crate::state::AutosaveInterval;
            let interval = self.state.read().ui_prefs.autosave_interval;
            let should_save = match interval {
                AutosaveInterval::Manual => false,
                AutosaveInterval::Immediate => true,
                _ => interval
                    .duration()
                    .map(|d| self.last_save_time.elapsed() >= d)
                    .unwrap_or(false),
            };
            if should_save {
                let state = self.state.read().clone();
                crate::state::save_session(&state, self.show_cables);
                self.session_dirty = false;
                self.last_save_time = std::time::Instant::now();
            }
        }

        // ── Undo / redo (Ctrl+Z / Ctrl+Y or Ctrl+Shift+Z) ────────────────────
        let undo = ctx.input_mut(|i| {
            i.consume_key(egui::Modifiers::CTRL, egui::Key::Z) && !i.modifiers.shift
        });
        let redo = ctx.input_mut(|i| {
            i.consume_key(egui::Modifiers::CTRL, egui::Key::Y)
                || (i.consume_key(egui::Modifiers::CTRL | egui::Modifiers::SHIFT, egui::Key::Z))
        });
        if undo {
            let current = self.state.read().clone();
            if let Some(prev) = self.history.undo(current) {
                *self.state.write() = prev;
                self.push_audio_params();
            }
        } else if redo {
            let current = self.state.read().clone();
            if let Some(next) = self.history.redo(current) {
                *self.state.write() = next;
                self.push_audio_params();
            }
        }

        // ── Tab: toggle cable visibility ──────────────────────────────────────
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Tab)) {
            self.show_cables = !self.show_cables;
            self.session_dirty = true;
        }

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
                            .color(theme::ASH)
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

        // ── Rack canvas (replaces tab panels) ────────────────────────────────
        CentralPanel::default()
            .frame(
                Frame::none()
                    .fill(egui::Color32::from_gray(8))
                    .inner_margin(egui::Margin::same(4.0)),
            )
            .show(ctx, |ui| {
                rack_canvas::draw_rack(self, ctx, ui);
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
