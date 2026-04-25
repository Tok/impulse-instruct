// ─── state/llm_agent_state.rs ────────────────────────────────────────────────
// Per-agent LLM state — extracted from `state/mod.rs` to keep that
// file under the 1000-line cap.  Holds the `AgentRole` enum and the
// `LlmAgentState` struct with its two constructors (default new agent,
// and the "inherit from global LlmState" path used when spawning).
//
// All transient (skipped-from-serde) fields stay transient — token
// budget counters, pipeline progress, lane-score history etc. are
// rebuilt over a session and never written to session.json.

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

use super::{ConversationMode, LlmState, PipelineProgress};

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
    /// Cumulative prompt + completion tokens consumed by this agent
    /// across the session.  Surfaced on the agent card so the user can
    /// see which agents dominate VRAM / throughput.  Transient — not
    /// serialised; resets each launch.
    #[serde(skip)]
    pub total_prompt_tokens: u64,
    #[serde(skip)]
    pub total_completion_tokens: u64,
    /// Number of inference cycles this agent has *completed* (regardless
    /// of success).  Lets the UI show a per-cycle average without
    /// needing to keep a separate sliding window.  `jam_cycle_count`
    /// only ticks on jam cycles, so a dedicated counter is needed for
    /// the budget readout.  Transient.
    #[serde(skip)]
    pub completed_cycles: u32,
    /// Sleep mode: when true the agent is parked and won't be picked
    /// for inference (the round-robin / jam scheduler skips sleeping
    /// agents).  Persisted so a session reload preserves which
    /// specialists are dormant.  Toggled from the agent card.
    #[serde(default)]
    pub sleeping: bool,
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
            total_prompt_tokens: 0,
            total_completion_tokens: 0,
            completed_cycles: 0,
            sleeping: false,
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
            total_prompt_tokens: 0,
            total_completion_tokens: 0,
            completed_cycles: 0,
            sleeping: false,
        }
    }
}
