// ─── llm/mod.rs ──────────────────────────────────────────────────────────────
// LLM inference thread.
// Loads a GGUF model and runs continuous inference to control synth params.
// Communicates with UI via crossbeam channels.

pub mod prompt;
pub use prompt::build_system_prompt;
pub use prompt::param_json_schema;

use anyhow::Result;
use crossbeam_channel::{Receiver, Sender};
use std::sync::Arc;
use parking_lot::RwLock;
use std::time::Instant;

use crate::state::{AppState, apply_llm_update};

// ─── Messages ─────────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct LlmInput {
    pub prompt: String,
    pub one_shot: bool, // false = continuous jam mode
}

#[derive(Clone, Debug)]
pub struct LlmOutput {
    pub text: String,
    pub param_update: Option<serde_json::Value>,
    pub tokens_per_sec: f32,
    pub context_used: usize,
    pub is_jam: bool,
}

// ─── LLM backend trait (swappable) ────────────────────────────────────────────

pub trait LlmBackend: Send {
    fn infer(&mut self, system: &str, user: &str) -> Result<LlmOutput>;
    #[allow(dead_code)]
    fn context_size(&self) -> usize;
}

// ─── llama.cpp backend ────────────────────────────────────────────────────────

pub struct LlamaCppBackend {
    model_path: String,
    loaded: bool,
}

impl LlamaCppBackend {
    pub fn new(model_path: &str) -> Self {
        let path = std::path::Path::new(model_path);
        let loaded = path.exists();
        if !loaded {
            log::warn!(
                "GGUF model not found at '{}' — running in mock mode.\n  \
                 Download the model with:  ./download-models.sh",
                model_path
            );
        }
        Self { model_path: model_path.to_string(), loaded }
    }

    pub fn is_loaded(&self) -> bool { self.loaded }
}

impl LlmBackend for LlamaCppBackend {
    fn infer(&mut self, _system: &str, user: &str) -> Result<LlmOutput> {
        if !self.loaded {
            return mock_response(user);
        }

        // When model file exists, use llama-cpp-2:
        // This is the real implementation path — requires llama-cpp-2 crate
        // and the model file present at self.model_path.
        //
        // The actual llama-cpp-2 API:
        //   use llama_cpp_2::model::LlamaModel;
        //   use llama_cpp_2::context::LlamaContext;
        //   let model = LlamaModel::load_from_file(&params, path, &model_params)?;
        //   let ctx = model.new_context(&backend, ctx_params)?;
        //   // tokenize, decode, sample tokens until EOF
        //
        // For now, fall through to mock until model loading is wired up.
        log::debug!("Model found at {}, but llama-cpp-2 inference not yet fully wired.", self.model_path);
        mock_response(user)
    }

    fn context_size(&self) -> usize { 4096 }
}

/// Generate a plausible JSON response for testing without a real model.
fn mock_response(prompt: &str) -> Result<LlmOutput> {
    let prompt_lower = prompt.to_lowercase();

    let json = if prompt_lower.contains("acid") {
        serde_json::json!({
            "_comment": "classic acid setup — resonant filter sweeping over a syncopated 303 line, 4-on-the-floor kick",
            "tb303": { "cutoff": 0.5, "resonance": 0.70, "env_mod": 0.80, "decay": 0.38, "distortion": 0.05, "volume": 0.9 },
            "sequencer": {
                "bpm": 135.0,
                "bass_steps": [true, false, true, false, false, true, false, true, true, false, false, true, false, true, false, false],
                "bass_notes": [36, 36, 36, 36, 36, 41, 36, 43, 36, 36, 36, 36, 36, 43, 36, 36],
                "kick808_steps": [true, false, false, false, true, false, false, false, true, false, false, false, true, false, false, false],
                "hihat808_steps": [false, false, true, false, false, false, true, false, false, false, true, false, false, false, true, false]
            },
            "fx": { "distortion_drive": 0.0, "distortion_mix": 0.0, "delay_time": 0.3, "delay_feedback": 0.35, "delay_mix": 0.15 }
        })
    } else if prompt_lower.contains("dark") || prompt_lower.contains("deep") {
        serde_json::json!({
            "_comment": "going dark — pulling the filter way down and adding space with long reverb and delay",
            "tb303": { "cutoff": 0.15, "resonance": 0.5, "env_mod": 0.3 },
            "fx": { "reverb_size": 0.8, "reverb_mix": 0.5, "delay_mix": 0.3 }
        })
    } else if prompt_lower.contains("fast") || prompt_lower.contains("hard") {
        serde_json::json!({
            "_comment": "cranking the tempo and adding drive for harder, more aggressive energy",
            "sequencer": { "bpm": 160.0 },
            "tb303": { "decay": 0.2, "distortion": 0.4 }
        })
    } else if prompt_lower.contains("mellow") || prompt_lower.contains("chill") {
        serde_json::json!({
            "_comment": "dialing back to something smooth — slower tempo, softer filter, letting the reverb breathe",
            "sequencer": { "bpm": 95.0 },
            "tb303": { "cutoff": 0.55, "resonance": 0.3, "env_mod": 0.2 },
            "fx": { "reverb_mix": 0.45, "delay_mix": 0.35 }
        })
    } else {
        // Random variation for jam mode
        let phase = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .subsec_millis() as f32 / 1000.0;
        let cut = 0.3 + (phase * 2.7).sin().abs() * 0.5;
        let res = 0.4 + (phase * 1.3).cos().abs() * 0.4;
        serde_json::json!({
            "_comment": "nudging the filter for some movement",
            "tb303": { "cutoff": cut, "resonance": res }
        })
    };

    Ok(LlmOutput {
        text: serde_json::to_string_pretty(&json).unwrap_or_default(),
        param_update: Some(json),
        tokens_per_sec: 42.0, // mock speed
        context_used: 256,
        is_jam: false, // caller sets this after the fact if needed
    })
}

// ─── LLM thread loop ──────────────────────────────────────────────────────────

pub fn run_llm_loop(
    state: Arc<RwLock<AppState>>,
    input_rx: Receiver<LlmInput>,
    output_tx: Sender<LlmOutput>,
) {
    let model_path = state.read().llm.model_path.clone();
    let mut backend = LlamaCppBackend::new(&model_path);

    if backend.is_loaded() {
        log::info!("LLM thread started — model: {}", model_path);
        let _ = output_tx.try_send(LlmOutput {
            text: format!("[ Model loaded: {} ]", model_path),
            param_update: None,
            tokens_per_sec: 0.0,
            context_used: 0,
            is_jam: false,
        });
    } else {
        log::info!("LLM thread started — mock mode (no model file)");
        let msg = format!(
            "[ Mock mode — no model at '{}'. Run ./download-models.sh to get Bonsai 8B ]",
            model_path
        );
        let _ = output_tx.try_send(LlmOutput {
            text: msg,
            param_update: None,
            tokens_per_sec: 0.0,
            context_used: 0,
            is_jam: false,
        });
    }

    loop {
        // Block until a prompt arrives
        let input = match input_rx.recv() {
            Ok(i) => i,
            Err(_) => break, // channel closed, shutdown
        };

        // Snapshot prompt before acquiring any lock (clone outside critical section)
        let prompt = input.prompt.clone();
        if input.one_shot {
            log::info!("YOU → {}", prompt);
        } else {
            log::debug!("YOU (jam) → {}", prompt);
        }

        // Build system prompt from a read snapshot — lock held for clone only
        let system = build_system_prompt(&state.read().clone());

        // Brief write to mark inferring + store last prompt
        {
            let mut s = state.write();
            s.llm.is_inferring = true;
            s.llm.last_prompt = prompt;
        }

        let t0 = Instant::now();
        let result = backend.infer(&system, &input.prompt);
        let elapsed = t0.elapsed().as_secs_f32();

        match result {
            Ok(output) => {
                let tps = output.tokens_per_sec;
                let ctx = output.context_used;

                // Apply param update to shared state
                if let Some(ref update) = output.param_update {
                    let current = state.read().clone();
                    let next = apply_llm_update(current, update);
                    *state.write() = next;
                }

                // Update LLM meta
                {
                    let mut s = state.write();
                    s.llm.is_inferring = false;
                    s.llm.tokens_per_sec = tps;
                    s.llm.context_used = ctx;
                    s.llm.last_response = output.text.clone();
                }

                // Log the natural-language comment if present, otherwise the raw response
                if let Some(ref update) = output.param_update {
                    let comment = update.get("_comment").and_then(|v| v.as_str())
                        .unwrap_or(&output.text);
                    if input.one_shot {
                        log::info!("Bonsai → {}", comment);
                    } else {
                        log::debug!("Bonsai (jam) → {}", comment);
                    }
                }
                let mut output = output;
                output.is_jam = !input.one_shot;
                let _ = output_tx.try_send(output);
                log::debug!("inference complete in {:.2}s", elapsed);
            }
            Err(e) => {
                log::error!("LLM inference error: {}", e);
                let mut s = state.write();
                s.llm.is_inferring = false;
                s.llm.last_response = format!("Error: {}", e);
            }
        }

        // In jam mode, re-queue immediately
        if input.one_shot {
            // nothing — wait for next prompt
        } else {
            let _ = input_rx.try_recv(); // drain any queued prompts
            // Signal back so UI can re-trigger if in auto_jam mode
            let _ = output_tx.send(LlmOutput {
                text: "[jam_cycle_done]".to_string(),
                param_update: None,
                tokens_per_sec: 0.0,
                context_used: 0,
                is_jam: true,
            });
        }
    }

    log::info!("LLM thread exiting");
}
