// ─── llm/mod.rs ──────────────────────────────────────────────────────────────
// LLM inference thread.
// Loads a GGUF model and runs continuous inference to control synth params.
// Communicates with UI via crossbeam channels.

pub mod instructions;
pub mod json_repair;
pub mod mock;
pub mod prompt;
pub mod styles;
pub use mock::mock_response;
use mock::run_mock_loop;
pub use prompt::build_system_prompt;
pub use prompt::param_json_schema;

use anyhow::Result;
use crossbeam_channel::{Receiver, Sender};
use parking_lot::RwLock;
use std::sync::Arc;
use std::time::Instant;

use crate::state::{AppState, ConversationMode, apply_llm_update};

#[derive(Clone, Debug)]
pub enum LlmInput {
    Infer {
        prompt: String,
        one_shot: bool,
        #[allow(dead_code)]
        agent_id: Option<u32>,
    },
    SwitchModel(String),
    ResetContext,
}

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
    },
    DismissAgent,
}

pub use json_repair::extract_llm_actions;

#[derive(Clone, Debug)]
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

pub trait LlmBackend: Send {
    fn infer(&mut self, system: &str, user: &str, sampling: &SamplingParams) -> Result<LlmOutput>;
}

pub mod server_pool;
pub use server_pool::{LLAMA_BASE_PORT, LlamaServerBackend, LlamaServerPool};

pub fn run_llm_loop(
    state: Arc<RwLock<AppState>>,
    input_rx: Receiver<LlmInput>,
    output_tx: Sender<LlmOutput>,
    mock: bool,
    tts_tx: Arc<parking_lot::Mutex<rtrb::Producer<f32>>>,
) {
    if mock {
        {
            let mut s = state.write();
            s.llm.is_mock = true;
            s.llm.llm_initializing = false;
        }
        log::warn!("Mock mode enabled via --mock flag. No real inference.");
        let _ = output_tx.try_send(LlmOutput {
            text: "[ Mock mode — pass --mock to confirm, or add model + server ]".to_string(),
            param_update: None,
            tokens_per_sec: 0.0,
            prompt_tokens: 0,
            completion_tokens: 0,
            context_used: 0,
            is_jam: false,
            thinking: None,
            mc_line: None,
            before_state: None,
            actions: vec![],
            agent_id: None,
        });
        run_mock_loop(state, input_rx, output_tx);
        return;
    }

    let model_path = state.read().llm.model_path.clone();
    let ctx_size = state.read().llm.context_max;
    let mut pool = LlamaServerPool::new(LLAMA_BASE_PORT, ctx_size);
    let _ = pool.acquire(&model_path);

    if !pool.is_any_live() {
        {
            let mut s = state.write();
            s.llm.is_mock = true;
            s.llm.model_missing = true;
            s.llm.llm_initializing = false;
        }
        log::error!(
            "No model found at '{}'. \
             Download one with: ./scripts/download-models.sh \
             Then select it in Prefs and press Restart.",
            model_path
        );
        let _ = output_tx.try_send(LlmOutput {
            text: "[ No model found — download one with ./scripts/download-models.sh \
                 then select it in Prefs → Model and press Restart.\n\
                 Running without LLM — synth works, prompts ignored. ]"
                .to_string(),
            param_update: None,
            tokens_per_sec: 0.0,
            prompt_tokens: 0,
            completion_tokens: 0,
            context_used: 0,
            is_jam: false,
            thinking: None,
            mc_line: None,
            before_state: None,
            actions: vec![],
            agent_id: None,
        });
        run_mock_loop(state, input_rx, output_tx);
        return;
    }

    {
        let mut s = state.write();
        s.llm.is_mock = false;
        s.llm.llm_initializing = false;
    }
    log::info!("LLM thread started — server live: {}", model_path);
    let _ = output_tx.try_send(LlmOutput {
        text: format!("[ Model loaded: {} ]", model_path),
        param_update: None,
        tokens_per_sec: 0.0,
        prompt_tokens: 0,
        completion_tokens: 0,
        context_used: 0,
        is_jam: false,
        thinking: None,
        mc_line: None,
        before_state: None,
        actions: vec![],
        agent_id: None,
    });

    while let Ok(input) = input_rx.recv() {
        match &input {
            LlmInput::SwitchModel(new_path) => {
                let new_path = new_path.clone();
                let old_path = state.read().llm.model_path.clone();
                log::info!("LLM: switching global model {} -> {}", old_path, new_path);
                pool.release(&old_path);
                state.write().llm.model_path = new_path.clone();
                state.write().llm.context_used = 0;
                let live = pool.acquire(&new_path).is_ok() && pool.is_any_live();
                {
                    let mut s = state.write();
                    s.llm.is_mock = !live;
                    s.llm.llm_initializing = false;
                }
                if live {
                    crate::state::save_model_setting(&new_path);
                }
                let status = if live {
                    format!("[ Model loaded: {} ]", new_path)
                } else {
                    format!("[ Model not found: {} — check path and restart ]", new_path)
                };
                state.write().llm.last_response = status.clone();
                let _ = output_tx.try_send(LlmOutput {
                    text: status,
                    param_update: None,
                    tokens_per_sec: 0.0,
                    prompt_tokens: 0,
                    completion_tokens: 0,
                    context_used: 0,
                    is_jam: false,
                    thinking: None,
                    mc_line: None,
                    before_state: None,
                    actions: vec![],
                    agent_id: None,
                });
                continue;
            }
            LlmInput::ResetContext => {
                let model_path = state.read().llm.model_path.clone();
                log::info!("LLM: resetting context (restart with same model)");
                pool.shutdown_model(&model_path);
                state.write().llm.context_used = 0;
                let live = pool.acquire(&model_path).is_ok() && pool.is_any_live();
                {
                    let mut s = state.write();
                    s.llm.is_mock = !live;
                    s.llm.llm_initializing = false;
                }
                let _ = output_tx.try_send(LlmOutput {
                    text: "[ Context reset ]".to_string(),
                    param_update: None,
                    tokens_per_sec: 0.0,
                    prompt_tokens: 0,
                    completion_tokens: 0,
                    context_used: 0,
                    is_jam: false,
                    thinking: None,
                    mc_line: None,
                    before_state: None,
                    actions: vec![],
                    agent_id: None,
                });
                continue;
            }
            LlmInput::Infer { .. } => {}
        }

        let LlmInput::Infer {
            ref prompt,
            one_shot,
            agent_id,
        } = input
        else {
            continue;
        };

        if one_shot {
            log::info!("YOU -> {}", prompt);
        } else {
            log::debug!("YOU (jam) -> {}", prompt);
        }

        // Look up agent state; fall back to singleton for agent_id=None or not found.
        // Also capture per-agent overrides for the system prompt.
        #[allow(clippy::type_complexity)]
        let (
            agent_heat,
            agent_temp,
            agent_scope,
            agent_enable_thinking,
            agent_model,
            agent_conv_mode,
            agent_style,
            agent_custom_style,
            agent_instructions,
            agent_persona,
            agent_prompt_override,
        ) = {
            let s = state.read();
            if let Some(aid) = agent_id {
                if let Some(a) = s.llm_agents.iter().find(|a| a.id == aid) {
                    let model = a
                        .model_path
                        .clone()
                        .unwrap_or_else(|| s.llm.model_path.clone());
                    (
                        a.heat,
                        a.temperature,
                        a.scope.clone(),
                        a.enable_thinking,
                        model,
                        Some(a.conversation_mode.clone()),
                        Some(a.active_style.clone()),
                        Some(a.custom_style_text.clone()),
                        Some(a.user_instructions.clone()),
                        Some(a.persona_name.clone()),
                        Some(a.system_prompt_override.clone()),
                    )
                } else {
                    (
                        s.llm.heat,
                        s.llm.temperature,
                        vec![],
                        s.llm.enable_thinking,
                        s.llm.model_path.clone(),
                        None,
                        None,
                        None,
                        None,
                        None,
                        None,
                    )
                }
            } else {
                (
                    s.llm.heat,
                    s.llm.temperature,
                    vec![],
                    s.llm.enable_thinking,
                    s.llm.model_path.clone(),
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                )
            }
        };

        // Ensure the agent's model has a server in the pool.
        let infer_port = match pool.acquire(&agent_model) {
            Ok(port) => port,
            Err(e) => {
                log::error!("Pool: cannot acquire server for {}: {}", agent_model, e);
                // Clear inferring state and skip this request.
                let mut s = state.write();
                s.llm.is_inferring = false;
                if let Some(aid) = agent_id
                    && let Some(a) = s.llm_agents.iter_mut().find(|a| a.id == aid)
                {
                    a.is_inferring = false;
                }
                continue;
            }
        };

        // Build system prompt with per-agent overrides patched in.
        let system = {
            let mut snap = state.read().clone();
            if let Some(mode) = agent_conv_mode {
                snap.llm.conversation_mode = mode;
            }
            if let Some(style) = agent_style {
                snap.llm.active_style = style;
            }
            if let Some(custom) = agent_custom_style {
                snap.llm.custom_style_text = custom;
            }
            if let Some(instr) = agent_instructions {
                snap.llm.user_instructions = instr;
            }
            if let Some(persona) = agent_persona.clone() {
                snap.llm.persona_name = persona;
            }
            if let Some(override_text) = agent_prompt_override
                && !override_text.is_empty()
            {
                snap.llm.system_prompt_override = override_text;
            }
            snap.llm.heat = agent_heat;
            build_system_prompt(&snap, &agent_scope)
        };
        {
            let mut s = state.write();
            s.llm.is_inferring = true;
            s.llm.last_prompt = prompt.clone();
            if let Some(aid) = agent_id
                && let Some(a) = s.llm_agents.iter_mut().find(|a| a.id == aid)
            {
                a.is_inferring = true;
            }
        }

        let sampling = {
            let s = state.read();
            SamplingParams {
                heat: agent_heat,
                temperature: agent_temp,
                top_k: s.llm.top_k,
                top_p: s.llm.top_p,
                min_p: s.llm.min_p,
                repeat_penalty: s.llm.repeat_penalty,
                frequency_penalty: s.llm.frequency_penalty,
                seed: s.llm.seed,
            }
        };
        let enable_thinking = agent_enable_thinking;
        let think_prompt = format!(
            "{} {}",
            prompt,
            if enable_thinking {
                "/think"
            } else {
                "/no_think"
            }
        );
        let t0 = Instant::now();
        let result = pool.infer(infer_port, &system, &think_prompt, &sampling);
        let elapsed = t0.elapsed().as_secs_f32();

        match result {
            Ok(output) => {
                let tps = output.tokens_per_sec;
                let ptok = output.prompt_tokens;
                let ctok = output.completion_tokens;
                let ctx = output.context_used;
                let tthink = output.thinking.as_ref().map(|t| t.len() / 4).unwrap_or(0);

                let before_state = if let Some(ref update) = output.param_update {
                    let current = state.read().clone();
                    let before = Box::new(current.clone());
                    let next = apply_llm_update(current, update, &agent_scope);
                    *state.write() = next;
                    Some(before)
                } else {
                    None
                };

                {
                    let mut s = state.write();
                    s.llm.is_inferring = false;
                    s.llm.tokens_per_sec = tps;
                    s.llm.prompt_tokens = ptok;
                    s.llm.completion_tokens = ctok;
                    s.llm.context_used = ctx;
                    s.llm.thinking_tokens = tthink;
                    s.llm.last_response = output.text.clone();
                }

                // Auto-compact: if context is > 85% full and the setting is on,
                // restart the server now so the *next* inference starts fresh.
                {
                    let s = state.read();
                    let pct = if s.llm.context_max > 0 {
                        ctx as f32 / s.llm.context_max as f32
                    } else {
                        0.0
                    };
                    if s.llm.auto_compact && pct >= 0.85 {
                        drop(s);
                        log::info!(
                            "LLM: context {:.0}% full — auto-compact: restarting server for {}",
                            pct * 100.0,
                            agent_model,
                        );
                        pool.shutdown_model(&agent_model);
                        state.write().llm.context_used = 0;
                        let live = pool.acquire(&agent_model).is_ok() && pool.is_any_live();
                        state.write().llm.is_mock = !live;
                        let _ = output_tx.try_send(LlmOutput {
                            text: "[ Context auto-compacted ]".to_string(),
                            param_update: None,
                            tokens_per_sec: 0.0,
                            prompt_tokens: 0,
                            completion_tokens: 0,
                            context_used: 0,
                            is_jam: false,
                            thinking: None,
                            mc_line: None,
                            before_state: None,
                            actions: vec![],
                            agent_id: None,
                        });
                    }
                }

                if let Some(ref update) = output.param_update {
                    let comment = update
                        .get("_comment")
                        .and_then(|v| v.as_str())
                        .unwrap_or(&output.text);
                    let persona = agent_persona
                        .clone()
                        .unwrap_or_else(|| state.read().llm.persona_name.clone());
                    if !one_shot {
                        log::debug!("{} (jam) -> {}", persona, comment);
                    }

                    let (
                        tts_on,
                        mode,
                        tts_pitch,
                        tts_speed,
                        tts_amplitude,
                        voice_char,
                        randomise,
                        pitch_snap,
                        root_note,
                        tts_scale,
                        tts_engine,
                    ) = {
                        let s = state.read();
                        (
                            s.llm.tts_enabled,
                            s.llm.conversation_mode.clone(),
                            s.llm.tts_pitch,
                            s.llm.tts_speed,
                            s.llm.tts_amplitude,
                            s.llm.tts_voice_char.clone(),
                            s.llm.tts_randomise,
                            s.llm.tts_pitch_snap,
                            s.sequencer.root_note,
                            s.sequencer.scale,
                            s.llm.tts_engine.clone(),
                        )
                    };
                    let tts_mode = matches!(mode, ConversationMode::Mc | ConversationMode::Dj);
                    if tts_on && tts_mode {
                        use crate::state::TtsEngine;
                        let tts_text = output.mc_line.as_deref().unwrap_or(comment);
                        log::info!("[TTS] {}", tts_text);
                        let tp = tts::TtsParams {
                            pitch: tts_pitch,
                            speed: tts_speed,
                            amplitude: tts_amplitude,
                            voice_char: &voice_char,
                            randomise,
                            pitch_snap,
                            root_note,
                            scale: tts_scale,
                        };
                        match tts_engine {
                            TtsEngine::CoquiTts => speak_coqui(tts_text, &mode, &tp, &tts_tx),
                            TtsEngine::EspeakNg => speak_fx(tts_text, &mode, &tp, &tts_tx),
                        }
                    }
                }
                let mut output = output;
                output.is_jam = !one_shot;
                output.before_state = before_state;
                output.agent_id = agent_id;
                // Write stats back to agent
                if let Some(aid) = agent_id {
                    let mut s = state.write();
                    if let Some(a) = s.llm_agents.iter_mut().find(|a| a.id == aid) {
                        a.is_inferring = false;
                        a.tokens_per_sec = output.tokens_per_sec;
                        a.last_response = output.text.clone();
                        a.jam_cycle_count = a.jam_cycle_count.saturating_add(1);
                    }
                }
                let _ = output_tx.try_send(output);
                log::debug!("inference complete in {:.2}s", elapsed);
            }
            Err(e) => {
                log::error!("LLM inference error: {}", e);
                let mut s = state.write();
                s.llm.is_inferring = false;
                s.llm.last_response = format!("Error: {}", e);
                if let Some(aid) = agent_id
                    && let Some(a) = s.llm_agents.iter_mut().find(|a| a.id == aid)
                {
                    a.is_inferring = false;
                }
            }
        }

        if one_shot {
            // wait for next prompt
        } else {
            let _ = input_rx.try_recv();
            let _ = output_tx.send(LlmOutput {
                text: "[jam_cycle_done]".to_string(),
                param_update: None,
                tokens_per_sec: 0.0,
                prompt_tokens: 0,
                completion_tokens: 0,
                context_used: 0,
                is_jam: true,
                thinking: None,
                mc_line: None,
                before_state: None,
                actions: vec![],
                agent_id: None,
            });
        }
    }

    log::info!("LLM thread exiting");
}

pub mod tts;
pub use tts::{TtsParams, speak_coqui, speak_fx};
