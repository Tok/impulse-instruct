// ─── ui/mod.rs — Main egui application ───────────────────────────────────────
pub mod agent_card;
pub mod agent_pills;
mod api_log_handler;
mod flip;
pub mod fx_dir;
mod header;
mod llm_strip;
mod midi_handler;
pub mod module_card;
pub mod module_card_mod;
mod note;
pub mod panels;
mod rack_ai;
pub mod rack_cables;
pub mod rack_canvas;
pub(crate) mod rack_content;
mod rack_scroll;
mod rack_toolbar;
mod scope_footer;
pub mod style_rack;
pub mod theme;
mod ui_helpers;
mod util;
pub mod widgets;
pub(crate) use note::{ansi_colorize_notes, note_freq_label, note_name};
pub(crate) use util::{scan_models, webbrowser_open};
mod prefs_controls;
mod windows;
mod wizard;

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

use crate::audio::AudioCommand;
use crate::llm::{LlmInput, LlmOutput};
use crate::midi::MidiEvent;
use crate::state::AppState;
use crossbeam_channel::{Receiver, Sender};
use egui::{CentralPanel, Frame, TopBottomPanel};
use parking_lot::RwLock;
use std::sync::Arc;
pub(super) const LOG_LEVELS: &[(&str, log::LevelFilter)] = &[
    ("error", log::LevelFilter::Error),
    ("warn", log::LevelFilter::Warn),
    ("info", log::LevelFilter::Info),
    ("debug", log::LevelFilter::Debug),
    ("trace", log::LevelFilter::Trace),
];

pub(crate) const SEQ_LABEL_W: f32 = 100.0;
pub(crate) const SEQ_LABEL_H: f32 = 22.0;
pub(crate) const SEQ_VOL_W: f32 = 330.0;
pub(crate) const SEQ_VOL_H: f32 = 14.0;

/// BPM tracker — averages last 8 inter-pulse intervals from MIDI clock (24 PPQN).
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

mod undo;
pub(crate) use api_log_handler::{ActivityAction, ActivityEntry};
use undo::StateHistory;
pub struct ImpulseApp {
    state: Arc<RwLock<AppState>>,
    audio_tx: rtrb::Producer<AudioCommand>,
    pub(crate) tts_tx: crate::audio::TtsSink,
    scope_rx: rtrb::Consumer<f32>,
    scope_buf: Vec<f32>,
    scope_history: std::collections::VecDeque<Vec<f32>>,
    last_seq_step: usize, // smooth event stream
    session_start: std::time::Instant,
    last_step_time: f64,
    capture_rx: rtrb::Consumer<f32>,
    dsp_load_rx: rtrb::Consumer<f32>,
    dsp_load_buf: Vec<f32>,
    pub(crate) amen_wave_cache: (String, Vec<(f32, f32)>),
    pub(crate) neutts_online: bool,
    pub(crate) granular_capture_rx: rtrb::Consumer<f32>,
    pub(crate) granular_tap: Vec<f32>, // ring buffer, ~3s master output for CAPTURE
    pub(crate) granular_tap_head: usize,
    audio_analysis: Option<crate::audio::analysis::AudioAnalysis>, // ~2s auto-refresh
    last_analysis_time: f64,
    listen_pending: bool, // LISTEN button flag — labels next LLM resp "LISTEN →"
    llm_tx: Sender<LlmInput>,
    llm_rx: Receiver<LlmOutput>,
    midi_rx: Receiver<MidiEvent>,
    midi_port: Option<String>,
    pressed_notes: std::collections::HashSet<u8>,
    piano_mouse_note: Option<u8>, // mouse-held note, separate from MIDI
    prompt_input: String,
    log_text: String,
    api_port: Option<u16>,
    api_log_rx: crossbeam_channel::Receiver<String>, // from ApiState log sender
    last_log_line: String,                           // dedup
    log_repeat_count: u32,
    show_about: bool,
    pub(crate) activity_log: Vec<ActivityEntry>,
    pub(crate) show_prefs: bool,
    export_bars: u32,
    ui_volume: f32,
    piano_show_labels: bool,
    pub(crate) spectrum_magnitudes: Vec<f32>,
    pub(crate) spectrum_peaks: Vec<f32>,
    stereo_rx: rtrb::Consumer<f32>,
    stereo_buf: Vec<f32>,
    pub(crate) stereo_corr: f32,
    pub(crate) stereo_balance: f32,
    last_thinking: Option<String>,
    seq_page: usize,
    pub(crate) seq_prefix_width: f32,
    expanded_seq_voices: std::collections::HashSet<crate::state::DrumVoice>,
    drum_clipboard: Option<(crate::state::DrumVoice, Vec<crate::state::Step>)>,
    available_models: Vec<String>,
    sys_info: std::sync::Arc<std::sync::Mutex<crate::sysinfo::SysInfo>>,
    show_sysinfo: bool,
    pub(crate) show_wizard: bool,
    pub(crate) wizard_selected: usize,
    pub(crate) wizard_rack_preset: usize,
    prefs_tab: usize,
    llm_tab: usize,
    startup_done: bool,
    midi_clock_tracker: MidiClockTracker,
    history: StateHistory,
    pub(crate) cable_drag: Option<rack_canvas::CableDrag>,
    pub(crate) show_cables: bool,
    pub(crate) rack_flipped: bool,
    pub(crate) flip_to_back_count: u32, // flips-to-back count, cycles scroll target
    pub(crate) ctrl_locked: bool,
    pub(crate) show_shortcuts: bool,
    pub(crate) add_menu_zone: Option<crate::state::Zone>,
    // Module being dragged by its title bar (id + current pointer position).
    pub(crate) module_drag: Option<rack_canvas::ModuleDrag>,
    // Auto-save: set when rack or session-worthy state changes; saved next frame.
    pub(crate) session_dirty: bool,
    pub(crate) zone_y: [f32; 4], // [ai, global, voice, fxmod] rack scroll offsets
    pub(crate) focused_module: Option<crate::state::ModuleKind>, // rack highlight target
    pub(crate) focus_time: std::time::Instant, // shine-animation timestamp
    last_saved_rack_sig: (usize, usize),
    last_save_time: std::time::Instant,
    pub(crate) module_scales: std::collections::HashMap<crate::state::ModuleKind, f32>,
    auto_listen: bool,
    auto_listen_counter: u32,
    jam_next_fire: Option<(std::time::Instant, Option<u32>)>,
    jam_next_agent: usize,
    native_ppp: f32,
    api_params_dirty: Arc<std::sync::atomic::AtomicBool>,
    pub(crate) touch_mode: Option<crate::state::ParamMode>,
    pub(crate) zone_ai_collapsed: bool,
    pub(crate) zone_global_collapsed: bool,
    pub(crate) zone_voice_collapsed: bool,
    pub(crate) zone_fxmod_collapsed: bool,
    /// Module ID pending removal confirmation (None = no dialog shown).
    pub(crate) confirm_remove_module: Option<u32>,
}

/// Audio channels bundled to keep `ImpulseApp::new` under the arg limit.
pub struct AudioChannels {
    pub params_tx: rtrb::Producer<AudioCommand>,
    pub scope_rx: rtrb::Consumer<f32>,
    pub capture_rx: rtrb::Consumer<f32>,
    pub dsp_load_rx: rtrb::Consumer<f32>,
    pub stereo_rx: rtrb::Consumer<f32>,
    pub granular_capture_rx: rtrb::Consumer<f32>,
    pub tts_tx: crate::audio::TtsSink,
}

impl ImpulseApp {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        state: Arc<RwLock<AppState>>,
        mut audio: AudioChannels,
        llm_tx: Sender<LlmInput>,
        llm_rx: Receiver<LlmOutput>,
        midi_rx: Receiver<MidiEvent>,
        midi_port: Option<String>,
        api_log_rx: crossbeam_channel::Receiver<String>,
        api_port: Option<u16>,
        skip_wizard: bool,
        api_params_dirty: Arc<std::sync::atomic::AtomicBool>,
    ) -> Self {
        theme::apply(&cc.egui_ctx);
        log::info!("ImpulseApp::new — creating UI…");

        // Load last session — only if session.json exists (deletion = clean start).
        if std::path::Path::new("session.json").exists()
            && let Some(storage) = cc.storage
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

        // Restore persisted log level
        {
            let idx = state.read().ui_prefs.log_level_idx;
            if let Some((_, filter)) = LOG_LEVELS.get(idx) {
                log::set_max_level(*filter);
            }
        }
        // Don't auto-start the sequencer — let the user or AI start it.
        state.write().sequencer.running = false;

        // Pre-load amen WAV if path is set in restored session.
        let amen_path = state.read().amen.path.clone();
        if !amen_path.is_empty()
            && let Some(data) = crate::audio::load_wav_to_44100(&amen_path)
        {
            let _ = audio.params_tx.push(AudioCommand::LoadSampler(data));
        }

        let midi_line = midi_port
            .as_ref()
            .map(|p| format!("[MIDI: {}]\n", p))
            .unwrap_or_else(|| "[MIDI: no device found]\n".into());
        let api_line = api_port
            .map(|p| format!("[HTTP API active → http://localhost:{}]\n", p))
            .unwrap_or_default();
        let log_text = format!("[Impulse Instruct ready]\n{}{}", midi_line, api_line);
        Self {
            state,
            audio_tx: audio.params_tx,
            tts_tx: audio.tts_tx,
            scope_rx: audio.scope_rx,
            scope_buf: Vec::new(),
            scope_history: std::collections::VecDeque::with_capacity(12),
            last_seq_step: usize::MAX,
            session_start: std::time::Instant::now(),
            last_step_time: 0.0,
            capture_rx: audio.capture_rx,
            dsp_load_rx: audio.dsp_load_rx,
            dsp_load_buf: Vec::with_capacity(64),
            amen_wave_cache: (String::new(), Vec::new()),
            neutts_online: false,
            granular_capture_rx: audio.granular_capture_rx,
            granular_tap: vec![0.0; 44_100 * 3], // 3s at 44.1k
            granular_tap_head: 0,
            audio_analysis: None,
            last_analysis_time: 0.0,
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
            api_log_rx,
            last_log_line: String::new(),
            log_repeat_count: 0,
            show_about: false,
            activity_log: Vec::new(),
            show_prefs: false,
            export_bars: 8,
            ui_volume: 1.0,
            piano_show_labels: true,
            spectrum_magnitudes: Vec::new(),
            spectrum_peaks: Vec::new(),
            stereo_rx: audio.stereo_rx,
            stereo_buf: Vec::with_capacity(4096),
            stereo_corr: 0.0,
            stereo_balance: 0.0,
            last_thinking: None,
            seq_page: 0,
            seq_prefix_width: 0.0,
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
            show_wizard: !skip_wizard,
            wizard_selected: if crate::state::load_session()
                .and_then(|s| s.llm_agents)
                .map(|a| a.is_empty())
                .unwrap_or(true)
            {
                0 // default to first preset (Solo) when no prior session
            } else {
                usize::MAX // WIZARD_RESUME
            },

            wizard_rack_preset: 1, // default to "Basic" (303 + 808 + 909)
            prefs_tab: 0,
            llm_tab: 0,
            startup_done: false,
            midi_clock_tracker: MidiClockTracker::new(),
            history: StateHistory::new(),
            cable_drag: None,
            show_cables: crate::state::load_session()
                .and_then(|s| s.show_cables)
                .unwrap_or(true),
            rack_flipped: false, // volatile — always start in front view
            flip_to_back_count: 0,
            ctrl_locked: false,
            show_shortcuts: false,
            add_menu_zone: None,
            module_drag: None,
            session_dirty: false,
            zone_y: [0.0; 4],
            focused_module: None,
            focus_time: std::time::Instant::now(),
            last_saved_rack_sig: (0, 0),
            last_save_time: std::time::Instant::now(),
            module_scales: crate::state::load_session()
                .and_then(|s| s.module_scales)
                .unwrap_or_default(),
            auto_listen: false,
            auto_listen_counter: 0,
            jam_next_fire: None,
            jam_next_agent: 0,
            native_ppp: 0.0, // captured on first frame after DPI is established
            api_params_dirty,
            touch_mode: None,
            zone_ai_collapsed: false,
            zone_global_collapsed: false,
            zone_voice_collapsed: false,
            zone_fxmod_collapsed: false,
            confirm_remove_module: None,
        }
    }
    /// Record the current state to the undo history before a mutation.
    pub(crate) fn push_history(&mut self) {
        let snapshot = self.state.read().clone();
        self.history.push(snapshot);
    }

    /// Effective scale for a module kind (1.0 if unset).
    pub(crate) fn kind_scale(&self, kind: crate::state::ModuleKind) -> f32 {
        self.module_scales.get(&kind).copied().unwrap_or(1.0)
    }

    fn drain_llm_outputs(&mut self) {
        while let Ok(out) = self.llm_rx.try_recv() {
            log::info!(
                "UI: drain_llm_output received (has_update={}, text_len={})",
                out.param_update.is_some(),
                out.text.len()
            );
            // Resolve persona: agent-specific name when available, else singleton.
            let persona_name = out
                .agent_id
                .and_then(|aid| {
                    let s = self.state.read();
                    s.llm_agents
                        .iter()
                        .find(|a| a.id == aid)
                        .map(|a| a.persona_name.clone())
                })
                .unwrap_or_else(|| self.state.read().llm.persona_name.clone());

            // Store thinking tokens for display; also echo to log if enabled.
            if let Some(ref thinking) = out.thinking
                && !thinking.is_empty()
            {
                self.last_thinking = Some(thinking.clone());
                let think_persona = persona_name.clone();
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
                let display = crate::state::format_llm_display(
                    out.param_update.as_ref(),
                    &out.text,
                    &conv_mode,
                );
                // Append thinking indicator when present; tag audio-feedback responses
                let persona = if self.listen_pending {
                    self.listen_pending = false;
                    "LISTEN".to_string()
                } else {
                    persona_name.clone()
                };
                let line = if out.thinking.as_ref().is_some_and(|t| !t.is_empty()) {
                    format!("{}: {} [think]\n", persona, display)
                } else {
                    format!("{}: {}\n", persona, display)
                };
                log::info!("{}", ansi_colorize_notes(line.trim_end()));
                self.log_text.push_str(&line);
                let action = if out.param_update.is_some() {
                    ActivityAction::ParamUpdate
                } else {
                    ActivityAction::Response
                };
                self.activity_log.push(ActivityEntry {
                    timestamp: std::time::Instant::now(),
                    persona: persona.clone(),
                    action,
                    detail: display,
                });
                if self.activity_log.len() > 500 {
                    self.activity_log.drain(..100);
                }
                if let Some(ref mc) = out.mc_line {
                    self.log_text.push_str(&format!("► {}\n", mc));
                    log::info!("► {}", mc);
                }
            }
            // Jam re-triggers unless heat is at zero (model is parked).
            if out.text == "[jam_cycle_done]" && self.state.read().llm.heat > 0.0 {
                {
                    // Advance ramps selectively (don't full-replace state — would overwrite API/rack edits).
                    let cur = self.state.read().clone();
                    let next = crate::state::jam_tools::advance_ramps(cur);
                    let mut s = self.state.write();
                    let step = s.sequencer.current_step;
                    s.bass_voices = next.bass_voices;
                    s.kit_a = next.kit_a;
                    s.kit_b = next.kit_b;
                    s.sequencer = next.sequencer;
                    s.sequencer.current_step = step;
                    s.fx = next.fx;
                    s.lfo = next.lfo;
                }
                self.state.write().llm.jam_cycle_count =
                    self.state.read().llm.jam_cycle_count.saturating_add(1);
                if self.auto_listen {
                    self.auto_listen_counter += 1;
                    if self.auto_listen_counter >= 4 {
                        self.auto_listen_counter = 0;
                        self.trigger_listen();
                    }
                }
                // Round-robin: pick next enabled agent
                let (next_id, jam_bars, bpm) = {
                    let s = self.state.read();
                    let agents = &s.llm_agents;
                    let enabled: Vec<_> = agents
                        .iter()
                        .enumerate()
                        .filter(|(_, a)| s.rack.modules.iter().any(|m| m.id == a.id && m.enabled))
                        .collect();
                    if enabled.is_empty() {
                        (None, s.llm.jam_bars, s.sequencer.bpm)
                    } else {
                        let idx = self.jam_next_agent % enabled.len();
                        self.jam_next_agent = idx + 1;
                        let agent = enabled[idx].1;
                        (Some(agent.id), agent.jam_bars, s.sequencer.bpm)
                    }
                };
                if jam_bars > 0.0 && bpm > 0.0 {
                    let delay_ms = (jam_bars * 240_000.0 / bpm) as u64;
                    self.jam_next_fire = Some((
                        std::time::Instant::now() + std::time::Duration::from_millis(delay_ms),
                        next_id,
                    ));
                } else if let Some(aid) = next_id {
                    let _ = self.llm_tx.try_send(LlmInput::Infer {
                        prompt: "continue jamming, evolve the pattern".to_string(),
                        one_shot: false,
                        agent_id: Some(aid),
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
                    LlmAction::SetStyle(sid) if !out.is_jam => {
                        // Respect style lock — agents can't override user-selected style
                        if !self.state.read().llm.style_lock {
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
                    }
                    LlmAction::SetHeat(_) => {} // heat is user-only; always ignore
                    LlmAction::SetStyle(_) => {} // jam: ignore
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
                    LlmAction::SpawnAgent {
                        persona,
                        scope,
                        model,
                        mode,
                        tts,
                    } => {
                        let s = self.state.read();
                        let ok = s.llm.agent_autonomy
                            && out
                                .agent_id
                                .and_then(|aid| s.llm_agents.iter().find(|a| a.id == aid))
                                .map(|a| a.can_spawn)
                                .unwrap_or(true);
                        // VRAM budget check
                        let vram_ok = !ok || {
                            let vram_total = self
                                .sys_info
                                .lock()
                                .ok()
                                .map(|si| si.vram_total_mb)
                                .unwrap_or(0);
                            !crate::llm::vram::would_exceed_vram(
                                &s.llm_agents,
                                &s.llm.model_path,
                                model.as_deref(),
                                vram_total,
                            )
                        };
                        if !vram_ok {
                            log::warn!(
                                "Rejected agent spawn '{}': would exceed VRAM budget",
                                persona
                            );
                        }
                        drop(s);
                        if ok && vram_ok {
                            self.push_history();
                            let snapshot = self.state.read().clone();
                            let (spawned, agent_id) = crate::state::spawn_agent(
                                snapshot,
                                persona,
                                scope,
                                crate::state::AgentRole::Producer,
                                model.clone(),
                            );
                            let new_state = crate::state::apply_agent_mode_and_tts(
                                spawned,
                                agent_id,
                                mode.as_deref(),
                                *tts,
                            );
                            *self.state.write() = new_state;
                            let tts_tag = if *tts { " + TTS" } else { "" };
                            self.log_text.push_str(&format!(
                                "Agent spawned: {persona} ({scope:?}){tts_tag}\n"
                            ));
                            self.session_dirty = true;
                        }
                    }
                    LlmAction::DismissAgent => {
                        if let Some(aid) = out.agent_id {
                            let s = self.state.read();
                            let ok = s.llm.agent_autonomy
                                && s.llm_agents
                                    .iter()
                                    .find(|a| a.id == aid)
                                    .map(|a| a.can_dismiss)
                                    .unwrap_or(false);
                            let count = s.llm_agents.len();
                            let name = s
                                .llm_agents
                                .iter()
                                .find(|a| a.id == aid)
                                .map(|a| a.persona_name.clone())
                                .unwrap_or_default();
                            drop(s);
                            if ok && count > 1 {
                                self.push_history();
                                self.state.write().rack.remove_module(aid);
                                self.state.write().llm_agents.retain(|a| a.id != aid);
                                self.push_fx_plan();
                                self.log_text.push_str(&format!("{} signed off\n", name));
                                if self.state.read().llm_agents.len() == 1 {
                                    self.state.write().llm_agents[0].scope.clear();
                                }
                                self.session_dirty = true;
                            }
                        }
                    }
                    LlmAction::SendHint { to, hint } => {
                        let mut s = self.state.write();
                        if let Some(target) = s
                            .llm_agents
                            .iter_mut()
                            .find(|a| a.persona_name.eq_ignore_ascii_case(to))
                        {
                            target.pending_hints.push(hint.clone());
                            // Cap at 5 pending hints
                            if target.pending_hints.len() > 5 {
                                target.pending_hints.drain(..target.pending_hints.len() - 5);
                            }
                        }
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
                log::debug!("UI: LLM output processed, audio params pushed");

                // Highlight (and optionally scroll to) the module affected by this update.
                if let Some(kind) = ui_helpers::llm_update_focus_kind(out.param_update.as_ref()) {
                    self.focused_module = Some(kind);
                    self.focus_time = std::time::Instant::now();
                    if self.state.read().ui_prefs.llm_auto_scroll {
                        self.state.write().scroll_target =
                            Some(kind.default_zone().scroll_name().to_string());
                    }
                }
            }
        }
        log::trace!("UI: drain_llm_outputs complete");
    }
}

impl eframe::App for ImpulseApp {
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
            // Stop sequencer while wizard is visible so no sound plays before user decides.
            if self.show_wizard && self.state.read().sequencer.running {
                self.state.write().sequencer.running = false;
                self.push_audio_params();
            }
        }
        // Context-sensitive Ctrl+MW zoom: per-module over cards, global elsewhere.
        let cg = self.state.read().ui_prefs.ui_scale;
        match util::detect_ctrl_zoom(ctx, &self.module_scales, cg) {
            Some(util::ZoomTarget::Kind(kind, s)) => {
                self.module_scales.insert(kind, s);
                self.session_dirty = true;
            }
            Some(util::ZoomTarget::Global(s)) => {
                self.state.write().ui_prefs.ui_scale = s;
                self.session_dirty = true;
            }
            None => {}
        }
        ctx.set_pixels_per_point(self.native_ppp * self.state.read().ui_prefs.ui_scale);

        // Publish touch_mode + WASD flag so widgets can read them without signature changes.
        ctx.data_mut(|d| {
            d.insert_temp(egui::Id::new("touch_mode"), self.touch_mode);
            d.insert_temp(
                egui::Id::new("wasd_as_arrows"),
                self.state.read().ui_prefs.wasd_as_arrows,
            );
            d.insert_temp(egui::Id::new("ctrl_locked"), self.ctrl_locked);
        });

        self.drain_llm_outputs();
        self.drain_api_log();
        self.drain_midi_events();
        // Poll API params_dirty flag — push audio params when API changed state
        if self
            .api_params_dirty
            .swap(false, std::sync::atomic::Ordering::Relaxed)
        {
            self.push_audio_params();
        }

        // ── Auto-save: write session.json if the state changed ──────────────
        // (moved here to diagnose hang — see if save blocks)
        if let Some((fire_at, pending_agent)) = self.jam_next_fire {
            if std::time::Instant::now() >= fire_at {
                self.jam_next_fire = None;
                if self.state.read().llm.heat > 0.0 {
                    let _ = self.llm_tx.try_send(LlmInput::Infer {
                        prompt: "continue jamming, evolve the pattern".to_string(),
                        one_shot: false,
                        agent_id: pending_agent,
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
                crate::state::save_session(&state, self.show_cables, self.rack_flipped);
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
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Tab)) {
            self.toggle_rack_flip();
        }
        let api_flip = self.state.write().rack_flip_requested.take();
        if let Some(show_back) = api_flip
            && show_back != self.rack_flipped
        {
            self.toggle_rack_flip();
        }
        if ctx.input_mut(|i| {
            i.consume_key(egui::Modifiers::SHIFT, egui::Key::Slash) // Shift+/ = ?
                || i.consume_key(egui::Modifiers::NONE, egui::Key::F1)
        }) {
            self.show_shortcuts = !self.show_shortcuts;
        }

        // ── Startup hook — auto-prompt after wizard closes ──────────────────
        // Once the wizard is dismissed and the LLM is ready, send a one-shot
        // prompt to get a basic pattern going so the track isn't silent.
        if !self.startup_done && !self.show_wizard && !self.state.read().llm.llm_initializing {
            self.startup_done = true;
            let has_agents = !self.state.read().llm_agents.is_empty();
            if has_agents {
                let _ = self.llm_tx.try_send(crate::llm::LlmInput::Infer {
                    prompt: "Pick a style and create a pattern. Bass line should use \
                             3-5 different notes but leave gaps — not every step \
                             needs a note. Use accent and slide on some steps. \
                             Add a kick pattern and hi-hats. Set the filter \
                             to something interesting. Set pan positions for \
                             stereo width and add subtle chorus."
                        .into(),
                    one_shot: true,
                    agent_id: None,
                });
                self.log_text
                    .push_str("AUTO → startup prompt sent, generating initial pattern…\n");
            }
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(16));
        // ── Drain scope + DSP load ring buffers ──────────────────────────────
        while let Ok(s) = self.scope_rx.pop() {
            self.scope_buf.push(s);
        }
        if self.scope_buf.len() > 512 {
            let drain = self.scope_buf.len() - 512;
            self.scope_buf.drain(..drain);
        }
        if self.scope_buf.len() >= 64 {
            // phosphor persistence — frame count from preferences
            self.scope_history.push_back(self.scope_buf.clone());
            let max_frames = self.state.read().ui_prefs.phosphor_frames.clamp(2, 20);
            if self.scope_history.len() > max_frames {
                self.scope_history.pop_front();
            }
        }
        self.update_spectrum();
        // Stereo correlation meter
        while let Ok(s) = self.stereo_rx.pop() {
            self.stereo_buf.push(s);
        }
        if self.stereo_buf.len() > 4096 {
            self.stereo_buf.drain(..self.stereo_buf.len() - 4096);
        }
        if self.stereo_buf.len() >= 200 {
            let (c, b) = crate::audio::analysis::stereo_correlation(&self.stereo_buf);
            self.stereo_corr = self.stereo_corr * 0.8 + c * 0.2;
            self.stereo_balance = self.stereo_balance * 0.8 + b * 0.2;
        }
        while let Ok(load) = self.dsp_load_rx.pop() {
            self.dsp_load_buf.push(load);
        }
        if self.dsp_load_buf.len() > 64 {
            let drain = self.dsp_load_buf.len() - 64;
            self.dsp_load_buf.drain(..drain);
        }
        // Track step changes for smooth event stream interpolation
        {
            let step = self.state.read().sequencer.current_step;
            if step != self.last_seq_step {
                self.last_seq_step = step;
                self.last_step_time = ctx.input(|i| i.time);
            }
        }
        self.update_audio_analysis(ctx);
        self.tick_ramps();
        self.draw_windows(ctx);
        self.draw_menu_and_header(ctx);
        TopBottomPanel::bottom("footer")
            .frame(
                Frame::none()
                    .fill(theme::VOID)
                    .inner_margin(egui::Margin::symmetric(6.0, 2.0)),
            )
            .exact_height(18.0)
            .show(ctx, |ui| {
                let (nm, na, nc) = {
                    let s = self.state.read();
                    (
                        s.rack.modules.len(),
                        s.llm_agents.len(),
                        s.rack.cables.len(),
                    )
                };
                let was_flipped = self.rack_flipped;
                scope_footer::draw_footer_status(
                    ui,
                    &self.midi_port,
                    &self.dsp_load_buf,
                    &mut self.rack_flipped,
                    &mut self.ctrl_locked,
                    scope_footer::FooterStats {
                        n_modules: nm,
                        n_agents: na,
                        n_cables: nc,
                        uptime_secs: self.session_start.elapsed().as_secs(),
                        api_port: self.api_port,
                    },
                );
                if was_flipped != self.rack_flipped {
                    self.rack_flipped = was_flipped;
                    self.toggle_rack_flip();
                }
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
        // When the startup wizard is visible, show an empty central panel
        // so the rack doesn't load and nothing plays in the background.
        CentralPanel::default()
            .frame(
                Frame::none()
                    .fill(egui::Color32::from_gray(8))
                    .inner_margin(egui::Margin::same(4.0)),
            )
            .show(ctx, |ui| {
                if !self.show_wizard {
                    rack_canvas::draw_rack(self, ctx, ui);
                }
            });
        if self.show_shortcuts && scope_footer::draw_shortcuts_overlay(ctx) {
            self.show_shortcuts = false;
        }
        if self.state.read().ui_prefs.crt_effect {
            scope_footer::draw_crt_overlay(ctx);
        }
    }
}
