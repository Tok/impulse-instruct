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

pub mod sample_instrument;
pub use sample_instrument::SampleInstrumentState;
pub mod sfz;
pub use sfz::{SfzFilType, SfzLoopMode, SfzRegion, parse_sfz};

pub mod noise;
pub use noise::NoiseVoiceState;

pub mod theremin;
pub use theremin::ThereminState;

pub mod pendulum;
pub use pendulum::PendulumState;

pub mod fm_ops;
pub use fm_ops::{FM_ALGORITHM_COUNT, FmOp, FmOpsState};

pub mod additive;
pub use additive::{ADDITIVE_HARMONICS, AdditiveState};

pub mod modal;
pub use modal::{MODAL_MODES, MODAL_RATIO_PRESETS, ModalState};

pub mod chiptune;
pub use chiptune::{
    CHIPTUNE_FILTER_MODES, CHIPTUNE_OSCS, CHIPTUNE_WAVEFORMS, ChiptuneState, SidOsc,
};

pub mod vocal;
pub use vocal::{VOCAL_VOWEL_PRESETS, VocalState};

pub mod granular;
pub use granular::GranularState;

pub mod an1x;
pub use an1x::{An1xLfoTarget, An1xState, An1xWave};

pub mod sequencer_state;
pub use sequencer_state::{SequencerState, Step, TB303Step};

pub mod cv_seq;
pub mod fx;
mod fx_defaults;
pub mod slew;
pub use cv_seq::{CV_SEQ_SLOTS, CV_SEQ_STEPS, CvSeqSlot};
pub use fx::{FxState, ParamEqBand, ParamEqBandKind, default_param_eq_bands};
pub use slew::{SLEW_SLOTS, SlewSlot};

pub mod chain_advance;
pub use chain_advance::{LoopBoundaryAction, build_advance_target, classify_loop_boundary};

pub mod morph;
pub use morph::{ChainMorph, bit_reverse_rank, morph_tick, step_swapped};

pub mod patch_morph;
pub use patch_morph::PatchMorphState;

pub mod persona_preset;
pub use persona_preset::{
    PersonaPreset, list_presets, list_presets_in, load_preset_from_path, personas_dir, save_preset,
    save_preset_to_dir, slugify,
};

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
pub mod rack_random;
pub mod rack_scope;
mod rack_wiring;
pub use fx_plan::compile_fx_plan;
pub use fx_types::{FX_STEP_COUNT, FeedbackRoute, FxPlan, FxStep, SidechainSource, VoiceSend};
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

pub mod automation_overlay;
pub use automation_overlay::{
    bass_lfo_curve_for_view, free_phase_per_step, rate_knob_to_hz, synced_phase_per_step,
};
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
    pub cv_seq: [CvSeqSlot; CV_SEQ_SLOTS],
    #[serde(default)]
    pub slew: [SlewSlot; SLEW_SLOTS],
    #[serde(default)]
    pub free_eg: FreeEg,
    #[serde(default)]
    pub noise_voice: NoiseVoiceState,
    #[serde(default)]
    pub theremin: ThereminState,
    #[serde(default)]
    pub pendulum: PendulumState,
    #[serde(default)]
    pub fm_ops: FmOpsState,
    #[serde(default)]
    pub additive: AdditiveState,
    #[serde(default)]
    pub modal: ModalState,
    #[serde(default)]
    pub chiptune: ChiptuneState,
    #[serde(default)]
    pub vocal: VocalState,
    #[serde(default)]
    pub granular: GranularState,
    #[serde(default)]
    pub hoover: HooverState,
    #[serde(default)]
    pub pluck: PluckState,
    #[serde(default)]
    pub wavetable: WavetableState,
    #[serde(default)]
    pub sample_instrument: SampleInstrumentState,
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
    /// Pattern morph in flight — set when the audio thread advances
    /// into a chain slot whose `ChainSlotOverride.morph_bars > 0`.
    /// While this is `Some`, each loop boundary applies one morph
    /// tick (replacing a growing fraction of step indices with the
    /// target pattern's same-index step) instead of jumping directly
    /// to the new pattern.  Cleared automatically when the morph
    /// completes.  Skipped from serialization — purely transient.
    #[serde(skip)]
    pub chain_morph: Option<ChainMorph>,
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
    /// AI patch-morph orchestration state — set by `/api/morph` or
    /// the UI menu, polled by `tick_patch_morph` each UI frame to
    /// fire the next LLM nudge when the bar interval elapses.
    /// `#[serde(skip)]` because morph progress is ephemeral —
    /// reloading a session shouldn't resurrect a half-finished
    /// arc.
    #[serde(skip)]
    pub patch_morph: PatchMorphState,
    /// API-requested zone collapse: (ai, global, voice, fxmod). None = no change.
    #[serde(skip)]
    pub collapse_requested: Option<(bool, bool, bool, bool)>,
    /// Compact audio analysis text, auto-updated by the UI every ~2s.
    /// Injected into every LLM system prompt as global context.
    #[serde(skip)]
    pub audio_snapshot: String,
    /// MPE per-note expression — captures the latest pitch bend,
    /// channel pressure, and timbre (CC74) from any non-zero MIDI
    /// channel.  Surfaced via `/api/state` and `/api/ws/state` so
    /// downstream patches / OSC bridges can react to MPE
    /// controllers; DSP integration (per-note pitch / cutoff
    /// modulation) is a follow-up.  Transient — not serialised.
    #[serde(skip)]
    pub mpe: MpeExpression,
}

/// Latest MPE expression values from any non-zero MIDI channel.
/// Each channel that emits expression overwrites the same fields,
/// so V1 is "last-channel-wins" — fine for monophonic bass control,
/// extensible to per-channel later.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MpeExpression {
    /// Channel that last sent any expression event (1..=15).  Zero
    /// = uninitialised (no MPE traffic yet).
    pub channel: u8,
    /// Pitch bend, -1.0..=1.0 (raw bend, not semitones).
    pub pitch_bend: f32,
    /// Channel pressure / aftertouch, 0.0..=1.0.
    pub pressure: f32,
    /// Timbre (CC74), 0.0..=1.0.  Y-axis on the ROLI Seaboard etc.
    pub timbre: f32,
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
            cv_seq: std::array::from_fn(|_| CvSeqSlot::default()),
            slew: std::array::from_fn(|_| SlewSlot::default()),
            free_eg: Default::default(),
            noise_voice: Default::default(),
            theremin: Default::default(),
            pendulum: Default::default(),
            fm_ops: Default::default(),
            additive: Default::default(),
            modal: Default::default(),
            chiptune: Default::default(),
            vocal: Default::default(),
            granular: Default::default(),
            hoover: Default::default(),
            pluck: Default::default(),
            wavetable: Default::default(),
            sample_instrument: Default::default(),
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
            chain_morph: None,
            live_record: false,
            spectrum: Default::default(),
            rack: Default::default(),
            llm_agents: Vec::new(),
            tts_modules: Vec::new(),
            style_overrides: HashMap::new(),
            scroll_target: None,
            rack_flip_requested: None,
            global_step_count: 0,
            patch_morph: Default::default(),
            collapse_requested: None,
            audio_snapshot: String::new(),
            mpe: MpeExpression::default(),
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
    /// Writeback diff log — last `LANE_APPLY_LOG_MAX` successful lane
    /// applies, newest at the back.  Each row holds the JSON payload
    /// that was applied (keys = params that changed) so the UI can
    /// render a "what changed this turn" diff panel.  Transient —
    /// not serialised; rebuilt as the pipeline runs.
    #[serde(skip)]
    pub recent_lane_applies: VecDeque<LaneApplyRecord>,
    /// Lane-score auto-tuner: long-term running average per
    /// `(style, lane_label)` key.  Each successful pipeline lane
    /// apply pushes its evaluated score onto the matching running
    /// average; the jam scheduler's `effective_dynamism` reads it
    /// to bias the picker toward lanes that have historically
    /// scored well in the active style.  Transient (not serialised)
    /// — rebuilt over a session as cycles run.
    #[serde(skip)]
    pub lane_avg_per_style: HashMap<String, LaneAverage>,
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

/// Cap on `LlmAgentState.pending_hints` — five queued hints is plenty
/// for any cycle's prompt-injection budget.  Older entries get
/// drained when this cap is exceeded.
pub const HINT_QUEUE_MAX: usize = 5;

/// True when `agent` matches the broadcast scope label.  Scope match
/// is case-insensitive: an agent whose `scope` contains the label
/// directly, OR whose persona name matches the label when scope is
/// empty (i.e. agent is unscoped → reachable by any persona-name
/// broadcast).  Pure helper so the matcher is unit-testable.
pub fn agent_matches_broadcast_scope(agent: &LlmAgentState, scope: &str) -> bool {
    let scope_lower = scope.to_ascii_lowercase();
    let scope_match = agent
        .scope
        .iter()
        .any(|sc| sc.eq_ignore_ascii_case(&scope_lower));
    let persona_match =
        agent.scope.is_empty() && agent.persona_name.eq_ignore_ascii_case(&scope_lower);
    scope_match || persona_match
}

/// Push a hint into an agent's `pending_hints` queue, draining the
/// oldest entries to stay at most `HINT_QUEUE_MAX` deep.  Pure helper
/// (`&mut LlmAgentState` is a method-style receiver, fine per the
/// guide — the agent owns its own consistency).
pub fn push_pending_hint(agent: &mut LlmAgentState, hint: String) {
    agent.pending_hints.push(hint);
    if agent.pending_hints.len() > HINT_QUEUE_MAX {
        let drop_n = agent.pending_hints.len() - HINT_QUEUE_MAX;
        agent.pending_hints.drain(..drop_n);
    }
}

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

/// Long-term per-(style, lane) running average score — the lane-score
/// auto-tuner reads this to bias jam-cycle dynamism toward lanes that
/// have historically scored well in the active style.  Distinct from
/// `LaneScore` (which is the most recent per-lane score and drives
/// short-term recency).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LaneAverage {
    pub sum: f32,
    pub n: u32,
}

impl LaneAverage {
    pub fn mean(&self) -> Option<f32> {
        if self.n == 0 {
            None
        } else {
            Some(self.sum / self.n as f32)
        }
    }
    pub fn update(&mut self, score: f32) {
        self.sum += score.clamp(0.0, 1.0);
        self.n = self.n.saturating_add(1);
    }
}

/// Compose the `(style, lane)` map key the auto-tuner uses.  Pure
/// helper so callers can read the same key in tests / UI.
pub fn lane_avg_key(style: &str, lane_label: &str) -> String {
    format!("{}/{}", style, lane_label)
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

/// One row in the writeback diff log — captured each time a lane
/// successfully applies its JSON update to AppState.  The `update`
/// field already contains *only the keys that changed* (the schema +
/// filter strip everything outside the lane's scope), so it doubles as
/// a "what changed this turn" diff payload for the UI.
#[derive(Clone, Debug, PartialEq)]
pub struct LaneApplyRecord {
    /// Display label of the lane (e.g. "BASS 1", "KITA", "FX").
    pub lane_label: String,
    /// JSON payload that was applied — keys are the params that
    /// actually changed, values are the new values.
    pub update: serde_json::Value,
    /// Wall-clock time the lane's inference took, in milliseconds.
    pub ms: u128,
    /// `LlmState.jam_cycle_count` at apply time — lets the UI group
    /// rows by jam cycle / one-shot turn.
    pub cycle: u32,
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
            recent_lane_applies: VecDeque::new(),
            lane_avg_per_style: HashMap::new(),
            recent_feedback: VecDeque::new(),
        }
    }
}

/// Maximum number of `LaneApplyRecord` rows kept in
/// `LlmState.recent_lane_applies`.  Two jam cycles' worth of lanes is
/// plenty for a "recently changed" panel; older history goes through
/// the regular log.
pub const LANE_APPLY_LOG_MAX: usize = 16;

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

/// Per-agent LLM state — extracted into its own module to keep
/// `state/mod.rs` under the 1000-line cap.  Re-exported below so the
/// canonical paths `crate::state::AgentRole` /
/// `crate::state::LlmAgentState` keep working.
pub mod llm_agent_state;
pub use llm_agent_state::{
    AGENT_MEMORY_MAX, AGENT_RECENT_OUTPUTS_MAX, AgentRole, LlmAgentState, STYLE_OBS_MAX,
};

/// Sync the default (first) LlmAgentState with the global LlmState.
pub mod jam_tools;
pub mod llm_apply;
pub mod llm_apply_seq;
pub(crate) mod llm_helpers;
pub(crate) mod llm_helpers_fx;
pub(crate) mod llm_helpers_voices_v2;
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
