// ─── state/mod.rs ── single source of truth for all synth parameters ─────────
// Pure data only — no methods that mutate in-place. Transitions at the bottom.
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};

pub mod drums;
pub use drums::*;
pub mod lfo;
pub use lfo::*;
pub mod music;
#[allow(unused_imports)]
pub use music::note_in_scale;
pub use music::{ROOT_NAMES, Scale, scale_degree, scale_notes, snap_to_scale};

pub mod synth_types;
pub use synth_types::{FilterMode, Waveform};

pub mod hoover;
pub use hoover::HooverState;

pub mod pluck;
pub use pluck::PluckState;

pub mod wavetable;
pub use wavetable::WavetableState;

pub mod noise;
pub use noise::NoiseVoiceState;

pub mod granular;
pub use granular::GranularState;

pub mod an1x;
pub use an1x::{An1xLfoTarget, An1xState, An1xWave};

pub mod sequencer_state;
pub use sequencer_state::{SequencerState, Step, TB303Step};

pub mod fx;
pub use fx::{FxState, ParamEqBand, ParamEqBandKind, default_param_eq_bands};

pub mod song;
pub use song::ChainSlotOverride;

pub mod style_overrides;
pub use style_overrides::{StyleOverride, effective_mc_lines, effective_themes};

pub const MAX_STEPS: usize = 64;
pub const MAX_BASS_VOICES: usize = 4;
/// Hard cap on `pattern_bank` / `chain` length.  MIDI imports of longer
/// pieces (Bach Italian Concerto III at a 32nd-note grid needs ~48
/// banks) write banks beyond the 8 rendered by the UI bank-selector
/// strip — the chain traverses them invisibly until the bank-selector
/// gets pagination (see PLAN.md `Sequencer → Paginated bank selector`).
pub const MAX_BANKS: usize = 64;
/// Number of bank slots pre-allocated by `default_pattern_bank` — matches
/// the visible A–H card strip.  Anything higher grows on demand when
/// `bank_write` / `bank_swap` / chain ops touch a higher slot.  Kept
/// separate from `MAX_BANKS` so fresh state only carries the 8 default
/// patterns in memory, not 64.
pub const DEFAULT_BANKS: usize = 8;

/// Valid sequencer BPM bounds — used by sliders, drag-values, clamp()s, and the
/// amen source-BPM field.  Narrower than General MIDI's 0–500 on purpose:
/// sub-40 crawls and 300+ is already in drum'n'bass / gabber territory.
pub const BPM_MIN: f32 = 40.0;
pub const BPM_MAX: f32 = 300.0;

// ─── Param control mode (tristate) ───────────────────────────────────────────

/// Whether a parameter is under user control, free for the LLM, or actively
/// targeted by the LLM.  Stored as two separate HashSets so the common "free"
/// case has zero allocation cost.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParamMode {
    /// Default: both user and LLM can change the value.
    Free,
    /// User has taken ownership — LLM will skip this param entirely.
    UserOwned,
    /// User wants the LLM to actively drive this param — hinted in every prompt.
    LlmFocus,
}

/// Derive the mode for a single dot-path from the two state sets.
pub fn param_mode(path: &str, locked: &HashSet<String>, focused: &HashSet<String>) -> ParamMode {
    if locked.contains(path) {
        return ParamMode::UserOwned;
    }
    if focused.contains(path) {
        return ParamMode::LlmFocus;
    }
    ParamMode::Free
}

/// Cycle a param through Free → UserOwned → LlmFocus → Free (pure).
pub fn cycle_param_mode(state: AppState, path: &str) -> AppState {
    let mut s = state;
    let owned = s.llm.locked_params.contains(path);
    let focused = s.llm.focused_params.contains(path);
    match (owned, focused) {
        (false, false) => {
            s.llm.locked_params.insert(path.to_string());
        }
        (true, false) => {
            s.llm.locked_params.remove(path);
            s.llm.focused_params.insert(path.to_string());
        }
        (_, true) => {
            s.llm.focused_params.remove(path);
        }
    }
    s
}

/// Set a param to a specific mode (Free clears both sets; pure function).
pub fn set_param_mode(state: AppState, path: &str, mode: ParamMode) -> AppState {
    let mut s = state;
    match mode {
        ParamMode::Free => {
            s.llm.locked_params.remove(path);
            s.llm.focused_params.remove(path);
        }
        ParamMode::UserOwned => {
            s.llm.focused_params.remove(path);
            s.llm.locked_params.insert(path.to_string());
        }
        ParamMode::LlmFocus => {
            s.llm.locked_params.remove(path);
            s.llm.focused_params.insert(path.to_string());
        }
    }
    s
}

pub mod ui_prefs;
pub use ui_prefs::{AutosaveInterval, UiPrefs};

pub(crate) mod fx_plan;
pub mod fx_types;
pub mod modulation;
pub mod module_kind;
pub mod rack;
mod rack_presets;
pub mod rack_scope;
pub use fx_plan::compile_fx_plan;
pub use fx_types::{FX_STEP_COUNT, FeedbackRoute, FxPlan, FxStep, VoiceSend};
pub use modulation::{
    ModInput, lfo_target_short_label, mod_input_label, mod_inputs, parse_lfo_target,
};
pub use module_kind::{GRID_COLS, ModuleKind, Zone};
pub use rack::{
    Cable, CableColor, FEEDBACK_GAIN_MAX, PortDir, PortKind, PortRef, RackModule, RackState,
};
pub use rack_presets::RACK_PRESETS;
pub use rack_scope::{parse_module_kind, rack_kind_name_matches, scope_from_control_cables};

// Amen sampler + bass voice state live in their own modules to keep LOC
// under the 1000-line cap on state/mod.rs.
pub use amen::{AmenMeta, AmenState};
use bass::default_bass_voices;
pub use bass::{BassLfoTarget, BassState, BassVoiceState};
pub use gabber::GabberKickParams;
mod amen;
mod bass;
mod gabber;

// ─── Top-level ───────────────────────────────────────────────────────────────
fn default_pattern_bank() -> Vec<SequencerState> {
    vec![SequencerState::default(); DEFAULT_BANKS]
}

fn default_chain_loop() -> bool {
    true
}

/// Spectrum analyser display parameters.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SpectrumAnalyzerState {
    /// Exponential smoothing factor (0.0 = instant, 0.95 = very smooth).
    pub smoothing: f32,
    /// Show peak-hold markers.
    pub peak_hold: bool,
}
impl Default for SpectrumAnalyzerState {
    fn default() -> Self {
        Self {
            smoothing: 0.7,
            peak_hold: false,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AppState {
    /// All bass synth voices (1 active minimum; up to MAX_BASS_VOICES).
    #[serde(default = "default_bass_voices")]
    pub bass_voices: Vec<BassVoiceState>,
    /// Which voice index is currently selected in the UI / being edited.
    #[serde(default)]
    pub active_voice: usize,
    pub kit_a: DrumKit808,
    pub kit_b: DrumKit909,
    pub sequencer: SequencerState,
    pub fx: FxState,
    pub llm: LlmState,
    #[serde(default)]
    pub lfo: [LfoSlot; 4],
    #[serde(default)]
    pub free_eg: FreeEg,
    #[serde(default)]
    pub noise_voice: NoiseVoiceState,
    #[serde(default)]
    pub granular: GranularState,
    #[serde(default)]
    pub hoover: HooverState,
    #[serde(default)]
    pub pluck: PluckState,
    #[serde(default)]
    pub wavetable: WavetableState,
    #[serde(default)]
    pub an1x: An1xState,
    #[serde(default)]
    pub amen: AmenState,
    #[serde(default)]
    pub gabber_kick: GabberKickParams,
    #[serde(default)]
    pub ui_prefs: UiPrefs,
    /// 8 named pattern slots (A–H) for storage and chain playback.
    #[serde(default = "default_pattern_bank")]
    pub pattern_bank: Vec<SequencerState>,
    /// Which bank slot the UI is currently editing (0–7).
    #[serde(default)]
    pub pattern_edit: usize,
    /// Ordered playback chain — indices into pattern_bank (max 8 entries).
    #[serde(default)]
    pub chain: Vec<usize>,
    /// Optional per-chain-slot overrides, parallel to `chain`.  Missing
    /// entries (vec shorter than `chain`) or default entries fall back to
    /// the loaded pattern's own `pattern_style` / `pattern_bpm_apply`.
    /// Lets the same bank slot play under different styles / BPM /
    /// repeat counts at different positions in the song.
    #[serde(default)]
    pub chain_overrides: Vec<ChainSlotOverride>,
    /// When true the audio thread advances through `chain` on each pattern loop.
    #[serde(default)]
    pub chain_enabled: bool,
    /// Current position in the chain — written by audio thread, read by UI.
    #[serde(default)]
    pub chain_pos: usize,
    /// How many times the current chain slot has looped since last advance.
    /// Counts 0, 1, 2, … up to `override.repeats`; when it reaches the
    /// threshold the audio thread advances the slot and resets this to 0.
    /// Audio-thread-owned; persisted so restart inside a long slot doesn't
    /// teleport forward.
    #[serde(default)]
    pub chain_repeat_count: u8,
    /// When true (the legacy default), the audio thread wraps `chain_pos`
    /// back to 0 after the last slot's last repeat — the song loops
    /// forever.  When false, playback stops instead (sets
    /// `sequencer.running = false`), leaving `chain_pos` on the final
    /// slot.  One-shot imports (MIDI scores with a definite end) set
    /// this to false so the piece plays exactly once.
    #[serde(default = "default_chain_loop")]
    pub chain_loop: bool,
    /// When true, piano/MIDI note-ons while running write into the bass pattern.
    #[serde(default)]
    pub live_record: bool,
    /// Spectrum analyser parameters.
    #[serde(default)]
    pub spectrum: SpectrumAnalyzerState,
    /// Modular rack — which modules are visible and how they are cabled.
    #[serde(default)]
    pub rack: RackState,
    /// Per-agent LLM state for rackable LLM modules.
    #[serde(default)]
    pub llm_agents: Vec<LlmAgentState>,
    /// Per-module TTS state, keyed by TTS rack module id.
    #[serde(default)]
    pub tts_modules: Vec<TtsModuleState>,
    /// Per-style mc_lines / themes overrides, keyed by style id.
    /// Lets users edit MC vocab and vocal themes per genre without
    /// touching `styles.json`.  Missing / empty entries fall back to
    /// the catalog baseline — see `state::style_overrides`.
    #[serde(default)]
    pub style_overrides: HashMap<String, StyleOverride>,
    // api_log moved to a lock-free crossbeam channel (ApiState.api_log_tx → UI receiver)
    /// Scroll target — the UI scrolls to bring this zone/module into view, then clears.
    /// Format: zone name ("global", "voice", "fxmod") or module kind ("AcidBass", "DrumKit808", etc.)
    #[serde(skip)]
    pub scroll_target: Option<String>,
    /// When Some, the UI toggles or sets rack flip state, then clears.
    /// true = show back (cables), false = show front (knobs), None = no change.
    #[serde(skip)]
    pub rack_flip_requested: Option<bool>,
    /// Monotonic step counter — incremented by the audio thread each time
    /// `current_step` advances. Used for bar-based ramp timing.
    #[serde(skip)]
    pub global_step_count: u64,
    /// API-requested zone collapse: (ai, global, voice, fxmod). None = no change.
    #[serde(skip)]
    pub collapse_requested: Option<(bool, bool, bool, bool)>,
    /// Compact audio analysis text, auto-updated by the UI every ~2s.
    /// Injected into every LLM system prompt as global context.
    #[serde(skip)]
    pub audio_snapshot: String,
}

impl Default for AppState {
    fn default() -> Self {
        let mut s = Self {
            bass_voices: default_bass_voices(),
            active_voice: 0,
            kit_a: Default::default(),
            kit_b: Default::default(),
            sequencer: Default::default(),
            fx: Default::default(),
            llm: Default::default(),
            lfo: Default::default(),
            free_eg: Default::default(),
            noise_voice: Default::default(),
            granular: Default::default(),
            hoover: Default::default(),
            pluck: Default::default(),
            wavetable: Default::default(),
            an1x: Default::default(),
            amen: Default::default(),
            gabber_kick: Default::default(),
            ui_prefs: Default::default(),
            pattern_bank: default_pattern_bank(),
            pattern_edit: 0,
            chain: Vec::new(),
            chain_overrides: Vec::new(),
            chain_enabled: false,
            chain_pos: 0,
            chain_repeat_count: 0,
            chain_loop: true,
            live_record: false,
            spectrum: Default::default(),
            rack: Default::default(),
            llm_agents: Vec::new(),
            tts_modules: Vec::new(),
            style_overrides: HashMap::new(),
            scroll_target: None,
            rack_flip_requested: None,
            global_step_count: 0,
            collapse_requested: None,
            audio_snapshot: String::new(),
        };
        // Create a default agent for the LlmAgent rack module.
        if let Some(agent_id) = s
            .rack
            .modules
            .iter()
            .find(|m| m.kind == ModuleKind::LlmAgent)
            .map(|m| m.id)
        {
            s.llm_agents.push(LlmAgentState::new_default(agent_id));
        }
        // Create TTS module state for any TTS modules in the default rack.
        for m in &s.rack.modules {
            if m.kind == ModuleKind::NeuTts {
                s.tts_modules.push(TtsModuleState::new(m.id));
            }
        }
        s
    }
}

// ─── LLM ─────────────────────────────────────────────────────────────────────

// ConversationMode, TtsModuleState → state/tts_types.rs
pub mod tts_types;
pub use tts_types::{ConversationMode, TtsModuleState};

/// Whether to use the short `brief` or full `description` from styles.json.
/// Brief (~50 tokens) suits smaller/faster models; Full (~150 tokens) for capable models.
#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
pub enum StyleVerbosity {
    Brief, // short keyword-dense creative brief
    #[default]
    Full, // long prose description — more context for capable models
}

/// TTS backend selection.
/// A smooth parameter transition scheduled by the LLM.
///
/// Two timing modes:
/// - **Cycle-based** (legacy): `step_per_cycle != 0`, `total_global_steps == 0`.
///   `current` converges toward `target` by `step_per_cycle` each jam cycle.
/// - **Bar-based**: `total_global_steps > 0`.  Ticked per UI frame using
///   `global_step_count`.  Progress = `(now - start_global_step) / total_global_steps`.
///   Value = `lerp(from, target, progress)`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ParamRamp {
    pub param: String, // dot-path, e.g. "fx.reverb_mix"
    pub current: f32,  // cycle-based: running value.  bar-based: unused (use `from`).
    pub target: f32,
    pub step_per_cycle: f32, // cycle-based: added to current each cycle.  bar-based: 0.
    /// Bar-based ramp: original value at ramp creation.
    #[serde(default)]
    pub from: f32,
    /// Bar-based ramp: `global_step_count` when the ramp was created (0 = cycle-based).
    #[serde(default)]
    pub start_global_step: u64,
    /// Bar-based ramp: duration in sequencer steps (0 = cycle-based).
    #[serde(default)]
    pub total_global_steps: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LlmState {
    pub model_path: String,
    pub last_prompt: String,
    pub last_response: String,
    pub is_inferring: bool,
    pub tokens_per_sec: f32,
    pub prompt_tokens: usize,
    pub completion_tokens: usize,
    pub thinking_tokens: usize, // chars in last _thinking field ÷ 4 (approx)
    pub context_used: usize,
    pub context_max: usize,
    pub locked_params: HashSet<String>, // UserOwned: LLM skips these entirely
    pub focused_params: HashSet<String>, // LlmFocus: LLM should prioritize these
    pub auto_jam: bool,                 // LLM continuously generates pattern variations
    pub heat: f32,                      // 0–1: jam mutation intensity (low=subtle, high=wild)
    #[serde(default = "LlmState::default_temperature")]
    pub temperature: f32, // 0–2: inference sampling temperature sent to llama-server (decoupled from heat)
    pub conversation_mode: ConversationMode,
    pub active_style: Option<String>, // style id from styles.json, "__free__", "__custom__", or None
    pub custom_style_text: String,    // used when active_style == Some("__custom__")
    /// When true (default), agents cannot override the user-selected style via SetStyle action.
    #[serde(default = "default_true")]
    pub style_lock: bool,
    pub user_instructions: String, // persistent user instructions injected into every system prompt
    pub persona_name: String,      // AI persona name shown in UI and used in system prompt
    pub system_prompt_override: String, // if non-empty, replaces the generated system prompt entirely
    pub enable_thinking: bool,          // append /think or /no_think to prompt (Qwen3)
    pub show_thinking_in_log: bool,     // display reasoning blocks inline in the log
    // ── Sampling params (passed to llama-server on every inference call) ──────
    pub top_k: i32,             // 0 = disabled; Gemma default 64
    pub top_p: f32,             // nucleus sampling; Gemma default 0.95
    pub min_p: f32,             // min-prob floor; default 0.05
    pub repeat_penalty: f32,    // 1.0 = off; >1.0 penalises repeated tokens
    pub frequency_penalty: f32, // OpenAI-compat; 0.0 = off
    pub seed: i64,              // -1 = random each call
    // TTS settings moved to per-module TtsModuleState (AppState.tts_modules).
    pub style_verbosity: StyleVerbosity, // Brief = ~50 token brief, Full = ~150 token description
    pub auto_lock_on_touch: bool,        // if true, touching a knob locks it to user-only control
    pub auto_compact: bool,              // restart server automatically when context > 85% full
    pub is_mock: bool,                   // true when running without a real model (no llama-server)
    pub llm_initializing: bool, // true while wait_for_ready is running (suppress false mock warning)
    #[serde(default)]
    pub model_missing: bool, // no model file found on startup — prompt user to download
    #[serde(default)]
    pub active_ramps: Vec<ParamRamp>, // ongoing smooth parameter transitions
    #[serde(default)]
    pub jam_bars: f32, // 0.0 = continuous (fire immediately); N = wait N bars before re-triggering
    #[serde(default)]
    pub jam_cycle_count: u32, // total jam cycles completed (display only)
    /// Global toggle: allow agents to autonomously spawn new agents during jam.
    #[serde(default = "default_true")]
    pub agent_autonomy: bool,
    /// When true (default), route user prompts through the sequential
    /// lane pipeline — one planner call + one focused call per lane.
    /// Flip off to run the legacy monolithic path for debugging.
    #[serde(default = "default_true")]
    pub use_pipeline: bool,
    /// Live progress of the current pipeline run.  `Some` while a turn is
    /// in flight, `None` between turns.  Transient — not serialised.
    #[serde(skip)]
    pub pipeline_progress: Option<PipelineProgress>,
    /// Per-lane lifecycle scoring keyed by lane label (`"bass1"`,
    /// `"kit_a"`, …).  Populated by `lane_eval::evaluate_lane` after
    /// each successful pipeline lane apply.  The Phase 2 weighted
    /// scheduler reads this to bias the next jam-cycle's lane pick.
    /// Transient — not serialised.
    #[serde(skip)]
    pub lane_scores: HashMap<String, LaneScore>,
    /// Phase 3 retry queue — lane labels whose last `evaluate_lane`
    /// score fell below `RETRY_THRESHOLD`.  `planner_jam::jam_plan` drains
    /// this before running the weighted picker so a one-off bad output
    /// gets a deterministic do-over.  Deduped on insert, oldest dropped
    /// on overflow (`RETRY_QUEUE_MAX`).  Transient — not serialised.
    #[serde(skip)]
    pub retry_queue: VecDeque<String>,
    /// Short-lived feedback lines surfaced from the apply layer when the
    /// model produced something weak (e.g. a ramp whose target matches
    /// the current value, a ramp with an imperceptible delta, …).
    /// Threaded into the next system prompt as `RECENT FEEDBACK:` so
    /// the LLM can correct on the next turn.  Capped at `FEEDBACK_MAX`
    /// entries (FIFO).  Transient — not serialised.
    #[serde(skip)]
    pub recent_feedback: VecDeque<String>,
}

/// Cap on `LlmState.recent_feedback` — small enough that the next
/// prompt's FEEDBACK section stays under ~400 chars even if every slot
/// is filled.
pub const FEEDBACK_MAX: usize = 5;

/// One row of `LlmState.lane_scores`.  Tracks how well a lane's last
/// generated output matched the rules we encode in the system prompt
/// (subset rule, density, in-scale ratio, …) plus light bookkeeping
/// for recency-aware scheduling later.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LaneScore {
    /// 0..1 — additive partial credit.  1.0 = output matches every
    /// expectation; 0.0 = empty / schema-rejected / nonsense.
    pub score: f32,
    /// `LlmState.jam_cycle_count` at the moment this lane was last
    /// updated.  Used by Phase 2's recency decay.
    pub last_changed_cycle: u32,
    /// Total successful applies of this lane this session.
    pub change_count: u32,
}

/// Streaming progress for the lane pipeline — populated by the LLM thread's
/// pipeline callback so the UI can render a "lane 3 of 8: bass" bar.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PipelineProgress {
    /// Total lanes in the plan (set on PlanReady).
    pub total_lanes: usize,
    /// Lanes that finished — succeeded or failed.
    pub lanes_done: usize,
    /// Lanes that failed (subset of `lanes_done`).
    pub failed_count: usize,
    /// Label of the lane currently inferring, or `None` between lanes.
    pub current_lane: Option<String>,
}

fn default_true() -> bool {
    true
}

impl LlmState {
    fn default_temperature() -> f32 {
        0.9
    }
}

impl Default for LlmState {
    fn default() -> Self {
        Self {
            model_path: String::from("models/gemma-4-E4B-it-Q4_K_M.gguf"),
            last_prompt: String::new(),
            last_response: String::new(),
            is_inferring: false,
            tokens_per_sec: 0.0,
            prompt_tokens: 0,
            completion_tokens: 0,
            thinking_tokens: 0,
            context_used: 0,
            context_max: 65536,
            locked_params: HashSet::new(),
            focused_params: HashSet::new(),
            auto_jam: true,
            heat: 0.5,
            temperature: 0.9,
            conversation_mode: ConversationMode::Producer,
            active_style: None,
            custom_style_text: String::new(),
            style_lock: true,
            user_instructions: String::new(),
            persona_name: String::from("PULSE"),
            system_prompt_override: String::new(),
            enable_thinking: true,
            show_thinking_in_log: true,
            top_k: 64,
            top_p: 0.95,
            min_p: 0.05,
            repeat_penalty: 1.0,
            frequency_penalty: 0.0,
            seed: -1,
            style_verbosity: StyleVerbosity::Full,
            auto_lock_on_touch: false,
            auto_compact: true,
            is_mock: false,
            model_missing: false,
            llm_initializing: true, // cleared by LLM thread once live/mock status is known
            active_ramps: Vec::new(),
            jam_bars: 0.0,
            jam_cycle_count: 0,
            agent_autonomy: true,
            use_pipeline: true,
            pipeline_progress: None,
            lane_scores: HashMap::new(),
            retry_queue: VecDeque::new(),
            recent_feedback: VecDeque::new(),
        }
    }
}

/// Push a short feedback line onto `state.llm.recent_feedback`, capping
/// at `FEEDBACK_MAX` (oldest-out).  Used by the apply layer when model
/// output is weak but not invalid — the line gets threaded into the
/// next system prompt so the LLM can self-correct.
pub fn push_llm_feedback(state: &mut LlmState, msg: impl Into<String>) {
    let s = msg.into();
    while state.recent_feedback.len() >= FEEDBACK_MAX {
        state.recent_feedback.pop_front();
    }
    state.recent_feedback.push_back(s);
}

// ─── Per-agent LLM state (rackable LLM modules) ─────────────────────────────

/// Agent role — affects personality flavor in system prompt.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentRole {
    #[default]
    Producer,
    Mc,
    Dj,
    Specialist,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LlmAgentState {
    pub id: u32, // matches RackModule.id
    pub persona_name: String,
    pub heat: f32,
    pub temperature: f32,
    pub scope: Vec<String>, // top-level keys this agent may write (empty = all)
    /// Agent role — affects system prompt personality flavor.
    #[serde(default)]
    pub role: AgentRole,
    /// Whether this agent may spawn new agents during jam cycles.
    #[serde(default)]
    pub can_spawn: bool,
    /// Whether this agent may dismiss itself during jam cycles.
    #[serde(default)]
    pub can_dismiss: bool,
    pub jam_bars: f32,
    pub conversation_mode: ConversationMode,
    pub active_style: Option<String>,
    pub custom_style_text: String,
    pub user_instructions: String,
    pub enable_thinking: bool,
    /// Per-agent system prompt override. Empty = use auto-generated prompt.
    #[serde(default)]
    pub system_prompt_override: String,
    /// Per-agent model override. `None` = inherit from global `LlmState.model_path`.
    #[serde(default)]
    pub model_path: Option<String>,
    // Display-only (updated by inference thread)
    #[serde(default)]
    pub is_inferring: bool,
    #[serde(default)]
    pub last_response: String,
    #[serde(default)]
    pub tokens_per_sec: f32,
    #[serde(default)]
    pub jam_cycle_count: u32,
    /// Persistent memory: conversation snippets preserved across sessions.
    /// Injected into the system prompt so the agent remembers prior context.
    #[serde(default)]
    pub memory: Vec<String>,
    /// Style learning: user preference observations (e.g. "user likes high reverb").
    /// Accumulated from UI edits; injected into system prompt alongside memory.
    #[serde(default)]
    pub style_observations: Vec<String>,
    /// Pending hints from other agents, consumed on next inference.
    #[serde(default)]
    pub pending_hints: Vec<String>,
    /// Short-term conversation trail for this agent — the last few
    /// condensed outputs it produced, injected into the next cycle's
    /// prompt so the agent can evolve coherently instead of treating
    /// every cycle as a blank slate.  Capped at `AGENT_RECENT_OUTPUTS_MAX`.
    /// `#[serde(default)]` so older saves without the field load cleanly.
    #[serde(default)]
    pub recent_outputs: VecDeque<String>,
    /// When true, this agent's style is independent and won't change when the
    /// global style is changed. When false (default), style syncs with global.
    #[serde(default)]
    pub style_locked: bool,
    /// Per-agent random seed.  -1 = random each call.  Inherited from the
    /// global LlmState.seed via `propagate_seed` when not locked.
    #[serde(default = "default_agent_seed")]
    pub seed: i64,
    /// When true, this agent's seed is independent and won't change when the
    /// global seed is changed. When false (default), seed syncs with global.
    #[serde(default)]
    pub seed_locked: bool,
    /// Live pipeline progress for this agent's current inference, or `None`
    /// when idle.  Transient — not serialised.
    #[serde(skip)]
    pub pipeline_progress: Option<PipelineProgress>,
}

fn default_agent_seed() -> i64 {
    -1
}

/// Maximum number of memory entries per agent.
pub const AGENT_MEMORY_MAX: usize = 20;
/// Maximum number of style observations.
pub const STYLE_OBS_MAX: usize = 10;
/// Maximum number of short-term recent-output entries per agent.  Kept
/// small on purpose: three cycles of context is enough for coherent
/// evolution without bloating every prompt or encouraging the model
/// to repeat stale moves.
pub const AGENT_RECENT_OUTPUTS_MAX: usize = 3;

impl LlmAgentState {
    pub fn new_default(id: u32) -> Self {
        Self {
            id,
            persona_name: "PULSE".to_string(),
            heat: 0.5,
            temperature: 0.9,
            scope: Vec::new(),
            role: AgentRole::Producer,
            can_spawn: true,
            can_dismiss: true,
            jam_bars: 0.0,
            conversation_mode: ConversationMode::Producer,
            active_style: None,
            custom_style_text: String::new(),
            user_instructions: String::new(),
            enable_thinking: true,
            system_prompt_override: String::new(),
            model_path: None,
            is_inferring: false,
            last_response: String::new(),
            tokens_per_sec: 0.0,
            jam_cycle_count: 0,
            memory: Vec::new(),
            style_observations: Vec::new(),
            pending_hints: Vec::new(),
            recent_outputs: VecDeque::new(),
            style_locked: false,
            seed: -1,
            seed_locked: false,
            pipeline_progress: None,
        }
    }

    /// Create an agent from the current singleton LlmState values.
    pub fn from_singleton(id: u32, llm: &LlmState) -> Self {
        Self {
            id,
            persona_name: llm.persona_name.clone(),
            heat: llm.heat,
            temperature: llm.temperature,
            scope: Vec::new(),
            role: AgentRole::Producer,
            can_spawn: true,
            can_dismiss: true,
            jam_bars: llm.jam_bars,
            conversation_mode: llm.conversation_mode.clone(),
            active_style: llm.active_style.clone(),
            custom_style_text: llm.custom_style_text.clone(),
            user_instructions: llm.user_instructions.clone(),
            enable_thinking: llm.enable_thinking,
            system_prompt_override: String::new(),
            model_path: None,
            is_inferring: false,
            last_response: String::new(),
            tokens_per_sec: 0.0,
            jam_cycle_count: 0,
            memory: Vec::new(),
            style_observations: Vec::new(),
            pending_hints: Vec::new(),
            recent_outputs: VecDeque::new(),
            style_locked: false,
            seed: llm.seed,
            seed_locked: false,
            pipeline_progress: None,
        }
    }
}

/// Sync the default (first) LlmAgentState with the global LlmState.
pub mod jam_tools;
pub mod llm_apply;
pub mod llm_apply_seq;
pub(crate) mod llm_helpers;
pub(crate) mod llm_rack;
pub mod transitions;
pub mod transitions_presets;
pub mod transport;

pub use transitions::*;
pub use transitions_presets::*;
pub use transport::preserve_sequencer_transport;

pub mod persistence;
pub use persistence::{
    SESSION_PATH, SETTINGS_PATH, apply_session, load_model_setting, load_project, load_session,
    save_model_setting, save_project, save_session, save_session_ext,
};
