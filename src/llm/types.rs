// ─── llm/types.rs ────────────────────────────────────────────────────────────
// Shared message + param / output structs for the LLM thread, plus the
// `LlmBackend` trait that inference implementations (llama-server pool,
// mock) implement.  Extracted from mod.rs so the module entry point
// stays comfortably under the 1000-line cap — the core is `run_llm_loop`
// there; the types here are read and emitted across every branch.

use anyhow::Result;

use crate::state::AppState;

/// Messages sent **to** the LLM thread from the UI / HTTP layer.
#[derive(Clone, Debug)]
pub enum LlmInput {
    Infer {
        prompt: String,
        one_shot: bool,
        #[allow(dead_code)]
        agent_id: Option<u32>,
    },
    /// Change the global (console) model.  Resets every agent override to
    /// `None` so all agents inherit the new global, and unloads every other
    /// model server.  Agents wanting their own model can re-pick via the
    /// agent dropdown afterwards.
    SwitchModel(String),
    /// Change a single agent's model override.  `new_path = None` re-inherits
    /// the global model; `Some(path)` adds a per-agent override (separate
    /// llama-server load).  The pool is updated synchronously so the old
    /// model's server unloads if no other agent or the global still need it.
    ///
    /// `old_path` is the agent's previous override snapshotted by the UI at
    /// the moment of the click — included in the message because the UI
    /// optimistically writes the new value to state before the LLM thread
    /// receives this, so reading `state.llm_agents[…].model_path` here
    /// would always see the new value.
    SwitchAgentModel {
        agent_id: u32,
        old_path: Option<String>,
        new_path: Option<String>,
    },
    ResetContext,
}

/// Actions the LLM can request the UI perform beyond a simple param update.
#[derive(Clone, Debug)]
pub enum LlmAction {
    SaveProject,
    SetHeat(f32),
    SetStyle(String),
    SetPersona(String),
    SetConversationMode(String),
    SetJamBars(f32),
    SpawnAgent {
        persona: String,
        scope: Vec<String>,
        model: Option<String>,
        /// Conversation mode override ("off" | "producer" | "dj" | "mc").
        /// When Some("mc"), the UI dispatch also auto-wires TTS even if
        /// `tts` is false — MC without voice isn't meaningful.
        mode: Option<String>,
        /// When true, spawn a NeuTts module alongside the agent and wire a
        /// control cable from agent → TTS (mirrors the /api/rack/agent
        /// `tts: true` path).
        tts: bool,
    },
    DismissAgent,
    /// Send a structured hint to another agent by persona name.
    SendHint {
        to: String,
        hint: String,
    },
}

/// Output emitted by the LLM thread back to the UI / HTTP layer.
#[derive(Clone, Debug, Default)]
pub struct LlmOutput {
    pub text: String,
    pub param_update: Option<serde_json::Value>,
    pub tokens_per_sec: f32,
    pub prompt_tokens: usize,
    pub completion_tokens: usize,
    pub context_used: usize,
    pub is_jam: bool,
    /// Reasoning extracted from the "_thinking" JSON field.
    pub thinking: Option<String>,
    /// MC crowd line — spoken via TTS in MC/DJ mode; displayed in log with a marker.
    pub mc_line: Option<String>,
    /// Snapshot of `AppState` taken immediately before the param update was applied.
    /// The UI uses this to push a correct undo entry.
    pub before_state: Option<Box<AppState>>,
    /// Actions extracted from the JSON response (save_project, heat, settings changes).
    pub actions: Vec<LlmAction>,
    /// Which agent produced this output (None = singleton/legacy).
    pub agent_id: Option<u32>,
}

/// Sampling parameters threaded through every inference call — the llama-server
/// backend maps these onto its HTTP body; the mock backend uses `heat` for
/// response variation.
#[derive(Clone, Debug)]
pub struct SamplingParams {
    pub heat: f32, // 0–1: jam mutation intensity (used for top_p widening and mock responses)
    pub temperature: f32, // 0–2: inference sampling temperature sent directly to llama-server
    pub top_k: i32, // 0 = disabled; Gemma default 64
    pub top_p: f32, // 0.0–1.0 nucleus; Gemma default 0.95
    pub min_p: f32, // 0.0–1.0 min prob floor; llama.cpp default 0.05
    pub repeat_penalty: f32, // 1.0 = off; >1.0 penalises repeats
    pub frequency_penalty: f32, // 0.0 = off (OpenAI-compat)
    pub seed: i64, // -1 = random
}

impl Default for SamplingParams {
    fn default() -> Self {
        Self {
            heat: 0.4,
            temperature: 0.9,
            top_k: 64,
            top_p: 0.95,
            min_p: 0.05,
            repeat_penalty: 1.0,
            frequency_penalty: 0.0,
            seed: -1,
        }
    }
}

/// Trait implemented by each inference backend (llama-server pool, mock).
/// Keeps the backend choice pluggable without leaking implementation
/// details into the LLM thread.
pub trait LlmBackend: Send {
    fn infer(&mut self, system: &str, user: &str, sampling: &SamplingParams) -> Result<LlmOutput>;
}
