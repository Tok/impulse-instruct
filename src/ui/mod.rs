// ─── ui/mod.rs — Main egui application ───────────────────────────────────────
pub mod agent_card;
pub mod agent_pills;
mod api_log_handler;
mod app_update;
mod flip;
pub mod fx_dir;
pub(crate) mod header;
pub(crate) mod header_menu;
mod link_handler;
mod llm_drain;
mod llm_log_color;
mod llm_strip;
mod midi_handler;
pub mod module_card;
pub mod module_card_back;
pub mod module_card_mod;
mod note;
pub mod panels;
pub mod patch_morph_handler;
mod rack_ai;
pub mod rack_cables;
pub mod rack_canvas;
mod rack_canvas_menus;
pub(crate) mod rack_content;
pub(crate) mod rack_content_conv_reverb;
pub(crate) mod rack_content_drag;
pub(crate) mod rack_content_fx_extras;
pub(crate) mod rack_content_pad;
pub(crate) mod rack_content_param_eq;
pub(crate) mod rack_content_pitch_shift;
mod rack_grid;
pub(crate) mod rack_minimap;
mod rack_scroll;
mod rack_toolbar;
pub(crate) mod scope_footer;
mod spectrum_header;
pub mod style_rack;
pub mod theme;
mod ui_helpers;
pub(crate) mod util;
pub mod widgets;
pub(crate) use note::{ansi_colorize_notes, note_freq_label, note_name};
pub(crate) use util::{scan_models, webbrowser_open};
mod prefs_controls;
mod windows;
mod windows_about;
mod windows_lane_diff;
mod windows_prefs;
mod windows_sysinfo;
mod windows_undo_timeline;
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
            // Outliers (doubled / dropped pulse) get discarded by the
            // pure helper — see midi::is_valid_clock_interval.
            if crate::midi::is_valid_clock_interval(secs) {
                self.intervals[self.head] = secs;
                self.head = (self.head + 1) % 8;
                if self.count < 8 {
                    self.count += 1;
                }
                let avg = self.intervals[..self.count].iter().sum::<f64>() / self.count as f64;
                let bpm = crate::midi::clock_interval_to_bpm(avg);
                self.last = Some(now);
                return Some(bpm);
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

/// Maximum number of past melodic notes the event-stream history retains.
/// 256 ≈ 8 bars at 32-step patterns, comfortably more than the widget shows.
pub(crate) const MELODIC_LOG_CAP: usize = 256;
pub(crate) const DRUM_LOG_CAP: usize = 512;

/// Which melodic voice fired a given log entry.  Bass tracks the per-voice
/// index so the heatmap can split bass1 / bass2 / etc., while the other
/// voices are singletons.  Variants kept narrow so the per-entry footprint
/// stays at one byte.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MelodicVoice {
    Bass(u8),
    An1x,
    Hoover,
}

/// One entry in the melodic-note history.  Recorded on each sequencer
/// step transition by snapshotting active notes from every melodic
/// pattern, so the event stream can render past notes from a frozen log
/// instead of the (mutable) live pattern data.
#[derive(Clone, Copy, Debug)]
pub struct MelodicLogEntry {
    /// `AppState.global_step_count` at the moment this note fired.
    pub fired_at: u64,
    /// MIDI pitch.
    pub note: u8,
    /// Step gate length 0..1 — used to scale the dot size.
    pub gate: f32,
    /// Accent intensity 0..=1 — renders proportionally larger / brighter.
    pub accent: f32,
    /// Slide intensity 0..=1 — renders proportionally longer trail.
    pub slide: f32,
    /// Source voice — used by the heatmap overlay to bin per-voice
    /// activity.  Existing dot rendering ignores this (every melodic
    /// entry renders identically).
    pub voice: MelodicVoice,
}

/// One entry in the drum-hit history — analogous to `MelodicLogEntry`
/// for kick/snare/hat/etc.  Lets the event stream freeze drum past-side
/// rendering the same way it freezes melodic past-side.
#[derive(Clone, Copy, Debug)]
pub struct DrumLogEntry {
    pub fired_at: u64,
    pub voice: crate::state::DrumVoice,
}

/// UI-side approximation of the LLM input channel queue, broken down per
/// agent + a "global" bucket for `agent_id = None` sends.  Bumped at every
/// `try_send` on the UI side; decremented when the corresponding agent's
/// `is_inferring` flips false → true (i.e. the LLM thread popped it).
/// API and OSC sends bypass this shadow — acceptable for v1.
#[derive(Default, Clone, Debug)]
pub struct LlmQueueShadow {
    pub per_agent: std::collections::HashMap<u32, usize>,
    pub global: usize,
}

impl LlmQueueShadow {
    pub fn note_send(&mut self, agent_id: Option<u32>) {
        match agent_id {
            Some(id) => *self.per_agent.entry(id).or_insert(0) += 1,
            None => self.global += 1,
        }
    }
    pub fn note_start(&mut self, agent_id: Option<u32>) {
        match agent_id {
            Some(id) => {
                if let Some(v) = self.per_agent.get_mut(&id) {
                    *v = v.saturating_sub(1);
                }
            }
            None => self.global = self.global.saturating_sub(1),
        }
    }
    pub fn count_for(&self, agent_id: Option<u32>) -> usize {
        match agent_id {
            Some(id) => self.per_agent.get(&id).copied().unwrap_or(0),
            None => self.global,
        }
    }
}

pub struct ImpulseApp {
    state: Arc<RwLock<AppState>>,
    audio_tx: rtrb::Producer<AudioCommand>,
    pub(crate) tts_tx: crate::audio::TtsSink,
    scope_rx: rtrb::Consumer<f32>,
    scope_buf: Vec<f32>,
    scope_history: std::collections::VecDeque<Vec<f32>>,
    last_seq_step: usize, // smooth event stream
    /// `state.global_step_count` snapshot taken at the moment the UI last
    /// detected a step transition.  Header.rs derives `smooth_global_step`
    /// from this rather than the live state, so the smoothed playhead
    /// can't jitter when the audio thread updates state between the
    /// step-change detection and the header's render-time state read.
    last_step_global: u64,
    /// Heartbeat anti-spam timer: when heat > 0 and the jam loop is dormant
    /// (no in-flight inference, no scheduled fire), the UI fires one Infer
    /// to kick it off — but the LLM thread takes a frame or two to flip
    /// `is_inferring`, so without a cooldown we'd fire several duplicates
    /// during that window.
    last_jam_kickoff: std::time::Instant,
    /// UI-side shadow of the LLM input queue.  Every `LlmInput::Infer` sent
    /// from this UI bumps a counter; transitions of `is_inferring` from
    /// false → true decrement.  Drives the LLM console's cycle viz.
    /// Doesn't see sends from the HTTP API or OSC — acceptable for v1.
    pub(crate) llm_queue: LlmQueueShadow,
    /// `is_inferring` snapshot per agent + global from the previous frame —
    /// used to detect false → true transitions for the queue shadow.
    last_inferring_per_agent: std::collections::HashMap<u32, bool>,
    last_inferring_global: bool,
    session_start: std::time::Instant,
    last_step_time: f64,
    /// Log of melodic notes that have actually fired, captured on each
    /// step transition.  The event-stream widget renders past notes from
    /// this log so mutations to the pattern don't erase visible history.
    pub(crate) melodic_log: std::collections::VecDeque<MelodicLogEntry>,
    /// Parallel log for drum hits — same role, drum voices instead of
    /// pitched notes.
    pub(crate) drum_log: std::collections::VecDeque<DrumLogEntry>,
    capture_rx: rtrb::Consumer<f32>,
    dsp_load_rx: rtrb::Consumer<f32>,
    dsp_load_buf: Vec<f32>,
    /// Live SampleInstrument polyphony readout (0..=POLY_VOICES).
    /// Updated once per audio callback by the engine; the panel reads
    /// it on paint to drive the dot meter.
    pub(crate) sample_instrument_poly: Arc<std::sync::atomic::AtomicU8>,
    pub(crate) amen_ui: panels::amen_viz::AmenUiState,
    /// Most recently loaded IR path for the convolution reverb.  The
    /// main update tick watches `fx.conv_reverb_ir_path` and reloads when
    /// it diverges from this — covers the API path that writes the
    /// string without sending a LoadImpulseResponse command directly.
    pub(crate) last_conv_reverb_ir_path: String,
    /// Same poll-cache pattern for the wavetable voice's `wave_path`
    /// — surfaces `/api/wavetable` writes into the audio thread on
    /// the next frame without coupling the API thread to the audio
    /// command queue directly.
    pub(crate) last_wavetable_path: String,
    pub(crate) last_sample_instrument_path: String,
    /// SampleInstrument runtime regions cached on the UI side for the
    /// zone-map visualizer.  Mirrors what the audio thread holds —
    /// populated when an SFZ loads and cleared when a single WAV is
    /// loaded (single-WAV mode shows the waveform thumbnail instead).
    /// Cheap copy because each entry's `samples` is an `Arc`.
    pub(crate) sample_sfz_regions: Vec<crate::audio::dsp::sample_instrument::SfzRegionRuntime>,
    /// Index into `sample_sfz_regions` for the currently selected
    /// SFZ region — drives the per-zone parameter inspector beneath
    /// the zone map.  UI-only state (not persisted to AppState).
    /// Cleared when a fresh SFZ loads or a single-WAV swap empties
    /// the region list.
    pub(crate) sample_selected_region: Option<usize>,
    /// Ableton Link bidirectional tempo sync.  Disabled by default;
    /// the user toggles via Preferences → Sync.  When enabled +
    /// built with the `link` feature, `update()` polls every frame
    /// to keep `state.sequencer.bpm` aligned with the network and
    /// pushes local edits back to peers.
    pub(crate) link_sync: crate::sync::LinkSync,
    /// Last BPM written to AppState by the Link pull — used to
    /// distinguish "user edited bpm" (push to network) from "network
    /// pulled the bpm" (don't push, would create a feedback loop).
    pub(crate) last_link_bpm: f32,
    /// Wall-clock instant of the last drift re-snap (or session
    /// start as a sentinel).  Rate-limits the continuous drift
    /// correction so a thrashing source can't cause a re-snap on
    /// every UI tick.  None until the first drift check fires.
    pub(crate) last_link_drift_resnap: Option<std::time::Instant>,
    /// Min/max waveform thumbnail for the SampleInstrument's loaded
    /// single-WAV buffer.  Cached on path so the per-frame paint just
    /// reads the vec.  Empty / stale-path → rebuilt on next paint.
    pub(crate) sample_wave_cache: (String, Vec<(f32, f32)>),
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
    /// Transient target for "learn next CC".  When `Some(path)`, the
    /// next incoming CC binds itself to that dot-path in
    /// `UiPrefs.midi_cc_bindings` and clears the field.  Cleared on
    /// app start; never persisted.  See Preferences → Controls → MIDI.
    pub(crate) midi_learn_target: Option<String>,
    /// Text-input buffer for the "Add MIDI binding" form in
    /// Preferences → Controls.  Holds the dot-path the user is
    /// typing before they click "Learn next CC".
    pub(crate) midi_learn_input: String,
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
    /// Last non-zero monitor volume — restored when the user un-mutes.
    pre_mute_volume: f32,
    /// User-adjusted center column width in the lower header band (event
    /// stream + bar oscilloscope).  0 means "use default = ~40 % of width".
    pub(crate) lower_center_w: f32,
    piano_show_labels: bool,
    pub(crate) spectrum_magnitudes: Vec<f32>,
    pub(crate) spectrum_peaks: Vec<f32>,
    stereo_rx: rtrb::Consumer<f32>,
    stereo_buf: Vec<f32>,
    pub(crate) stereo_corr: f32,
    pub(crate) stereo_balance: f32,
    /// Rolling FFT history for the `Spectrogram` viz module.  One entry
    /// per UI update tick; capped at `SPECTROGRAM_HISTORY_LEN` frames
    /// (≈5 s of scroll at typical UI rates).
    pub(crate) spectrogram_history: std::collections::VecDeque<Vec<f32>>,
    /// LUFS meter — driven from the master scope buffer per UI tick.
    pub(crate) lufs_meter: crate::audio::analysis::LufsMeter,
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
    /// Optional style id that seeds the wizard's rack instead of the
    /// generic `RACK_PRESETS` picks.  `Some(id)` causes `apply_wizard_preset`
    /// to route through `style_rack::apply` with the style's `rack_modules`
    /// and to stamp `baseline_params` on fresh load.  `None` = classic
    /// behaviour (generic preset rack, no style set).
    pub(crate) wizard_style_id: Option<String>,
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
    /// Toggle for the Lane Diff window — shows the per-lane writeback
    /// log captured by the pipeline.  Off by default; opened from the
    /// header's view menu.
    pub(crate) show_lane_diff: bool,
    /// Toggle for the Undo Timeline window — slider over the
    /// `StateHistory` past/future stacks for visual A/B comparison
    /// of recent state changes.  Off by default; opened from the
    /// header's view menu.
    pub(crate) show_undo_timeline: bool,
    pub(crate) add_menu_zone: Option<crate::state::Zone>,
    // Module being dragged by its title bar (id + current pointer position).
    pub(crate) module_drag: Option<rack_canvas::ModuleDrag>,
    // Auto-save: set when rack or session-worthy state changes; saved next frame.
    pub(crate) session_dirty: bool,
    pub(crate) zone_y: [f32; 4], // [ai, global, voice, fxmod] rack scroll offsets
    pub(crate) focused_module: Option<crate::state::ModuleKind>, // rack highlight target
    pub(crate) focus_time: std::time::Instant, // shine-animation timestamp
    last_saved_rack_sig: (usize, usize),
    /// Hash of (global model_path, every agent model_path).  Bumps the
    /// session_dirty flag when the user changes any model selection so the
    /// autosave persists the choice across restarts — the rack signature
    /// alone misses model picks because they don't add modules or cables.
    last_saved_model_sig: u64,
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
    pub sample_instrument_poly: Arc<std::sync::atomic::AtomicU8>,
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
        if std::path::Path::new(crate::state::SESSION_PATH).exists()
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
            last_step_global: 0,
            // Set in the past so the first frame's heartbeat check passes
            // immediately once heat > 0.
            last_jam_kickoff: std::time::Instant::now() - std::time::Duration::from_secs(10),
            llm_queue: LlmQueueShadow::default(),
            last_inferring_per_agent: std::collections::HashMap::new(),
            last_inferring_global: false,
            melodic_log: std::collections::VecDeque::with_capacity(MELODIC_LOG_CAP),
            drum_log: std::collections::VecDeque::with_capacity(DRUM_LOG_CAP),
            session_start: std::time::Instant::now(),
            last_step_time: 0.0,
            capture_rx: audio.capture_rx,
            dsp_load_rx: audio.dsp_load_rx,
            dsp_load_buf: Vec::with_capacity(64),
            sample_instrument_poly: audio.sample_instrument_poly,
            amen_ui: Default::default(),
            last_conv_reverb_ir_path: String::new(),
            last_wavetable_path: String::new(),
            last_sample_instrument_path: String::new(),
            sample_sfz_regions: Vec::new(),
            sample_selected_region: None,
            sample_wave_cache: (String::new(), Vec::new()),
            link_sync: crate::sync::LinkSync::new(120.0),
            last_link_bpm: 0.0,
            last_link_drift_resnap: None,
            neutts_online: false,
            granular_capture_rx: audio.granular_capture_rx,
            granular_tap: vec![0.0; crate::audio::SAMPLE_RATE_HZ as usize * 3], // 3s ring buffer
            granular_tap_head: 0,
            audio_analysis: None,
            last_analysis_time: 0.0,
            listen_pending: false,
            llm_tx,
            llm_rx,
            midi_rx,
            midi_port,
            pressed_notes: std::collections::HashSet::new(),
            midi_learn_target: None,
            midi_learn_input: String::new(),
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
            pre_mute_volume: 1.0,
            lower_center_w: 0.0,
            piano_show_labels: true,
            spectrum_magnitudes: Vec::new(),
            spectrum_peaks: Vec::new(),
            stereo_rx: audio.stereo_rx,
            stereo_buf: Vec::with_capacity(4096),
            stereo_corr: 0.0,
            stereo_balance: 0.0,
            spectrogram_history: std::collections::VecDeque::with_capacity(
                crate::audio::analysis::SPECTROGRAM_HISTORY_LEN,
            ),
            lufs_meter: crate::audio::analysis::LufsMeter::new(crate::audio::SAMPLE_RATE),
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

            wizard_rack_preset: 3, // default to "Full" (everything — 303 + both drum kits + hoover + an1x + amen + FX chain)
            wizard_style_id: None,
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
            show_lane_diff: false,
            show_undo_timeline: false,
            add_menu_zone: None,
            module_drag: None,
            session_dirty: false,
            zone_y: [0.0; 4],
            focused_module: None,
            focus_time: std::time::Instant::now(),
            last_saved_rack_sig: (0, 0),
            last_saved_model_sig: 0,
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
}
