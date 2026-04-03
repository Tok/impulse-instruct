// ─── llm/mod.rs ──────────────────────────────────────────────────────────────
// LLM inference thread.
// Loads a GGUF model and runs continuous inference to control synth params.
// Communicates with UI via crossbeam channels.

pub mod instructions;
pub mod prompt;
pub mod styles;
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
    fn infer(&mut self, system: &str, user: &str, heat: f32) -> Result<LlmOutput>;
    fn context_size(&self) -> usize;
}

// ─── llama.cpp backend ────────────────────────────────────────────────────────

/// Heap-allocated to avoid moving the large model struct around.
#[cfg(feature = "llm")]
struct LlamaInner {
    backend: llama_cpp_2::llama_backend::LlamaBackend,
    model:   llama_cpp_2::model::LlamaModel,
}

#[cfg(feature = "llm")]
impl LlamaInner {
    fn load(model_path: &str) -> Result<Box<Self>> {
        use llama_cpp_2::{
            llama_backend::LlamaBackend,
            model::{LlamaModel, params::LlamaModelParams},
        };
        let backend = LlamaBackend::init()?;
        let params  = LlamaModelParams::default();
        let model   = LlamaModel::load_from_file(&backend, std::path::Path::new(model_path), &params)?;
        Ok(Box::new(Self { backend, model }))
    }
}

pub struct LlamaCppBackend {
    model_path: String,
    loaded: bool,
    #[cfg(feature = "llm")]
    inner: Option<Box<LlamaInner>>,
}

impl LlamaCppBackend {
    pub fn new(model_path: &str) -> Self {
        #[cfg(feature = "llm")]
        {
            if std::path::Path::new(model_path).exists() {
                match LlamaInner::load(model_path) {
                    Ok(inner) => {
                        log::info!("Model loaded: {}", model_path);
                        return Self { model_path: model_path.to_string(), loaded: true, inner: Some(inner) };
                    }
                    Err(e) => log::error!("Failed to load model '{}': {}", model_path, e),
                }
            } else {
                log::warn!(
                    "Model not found at '{}' — falling back to mock.\n  \
                     Run ./download-models.sh",
                    model_path
                );
            }
            return Self { model_path: model_path.to_string(), loaded: false, inner: None };
        }
        #[cfg(not(feature = "llm"))]
        {
            let loaded = std::path::Path::new(model_path).exists();
            if !loaded {
                log::warn!(
                    "Model not found at '{}' — running in mock mode.\n  \
                     Run ./download-models.sh",
                    model_path
                );
            } else {
                log::warn!(
                    "Model found at '{}' but `--features llm` not enabled — mock mode.",
                    model_path
                );
            }
            Self { model_path: model_path.to_string(), loaded }
        }
    }

    pub fn is_loaded(&self) -> bool { self.loaded }
}

impl LlmBackend for LlamaCppBackend {
    fn infer(&mut self, system: &str, user: &str, heat: f32) -> Result<LlmOutput> {
        #[cfg(feature = "llm")]
        if let Some(ref inner) = self.inner {
            return infer_real(inner, system, user, heat);
        }
        let _ = system;
        mock_response(user, heat)
    }

    fn context_size(&self) -> usize { 4096 }
}

// ─── Real llama.cpp inference ─────────────────────────────────────────────────

#[cfg(feature = "llm")]
fn infer_real(inner: &LlamaInner, system: &str, user: &str, heat: f32) -> Result<LlmOutput> {
    use llama_cpp_2::{
        model::{AddBos, LlamaChatMessage},
        context::params::LlamaContextParams,
        llama_batch::LlamaBatch,
        sampling::LlamaSampler,
        json_schema_to_grammar,
    };
    use std::num::NonZeroU32;

    // ── Prompt ────────────────────────────────────────────────────────────────
    let messages = vec![
        LlamaChatMessage::new("system".to_string(), system.to_string())?,
        LlamaChatMessage::new("user".to_string(),   user.to_string())?,
    ];
    let template = inner.model.chat_template(None)?;
    let prompt   = inner.model.apply_chat_template(&template, &messages, true)?;

    // ── Tokenise ──────────────────────────────────────────────────────────────
    let input_tokens = inner.model.str_to_token(&prompt, AddBos::Always)?;
    let n_input      = input_tokens.len();

    // ── Context ───────────────────────────────────────────────────────────────
    // Round up to next power of two; at least 2048
    let ctx_size = ((n_input + 512).next_power_of_two() as u32).max(2048);
    let ctx_params = LlamaContextParams::default()
        .with_n_ctx(Some(NonZeroU32::new(ctx_size).unwrap()));
    let mut ctx = inner.model.new_context(&inner.backend, ctx_params)?;

    // ── Fill batch with input ─────────────────────────────────────────────────
    let mut batch = LlamaBatch::new(ctx_size as usize, 1);
    for (i, &tok) in input_tokens.iter().enumerate() {
        batch.add(tok, i as i32, &[0], i == n_input - 1)?;
    }
    ctx.decode(&mut batch)?;

    // ── Sampler: grammar + temperature ────────────────────────────────────────
    let schema        = serde_json::to_string(&crate::llm::param_json_schema())?;
    let grammar       = json_schema_to_grammar(&schema)?;
    let grammar_sampler = LlamaSampler::grammar(&inner.model, &grammar, "root")?;
    // heat 0.0 → temp 0.1 (near-deterministic), heat 1.0 → temp 1.2 (wild)
    let temp = 0.1_f32 + heat.clamp(0.0, 1.0) * 1.1;
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_micros();
    let mut sampler = LlamaSampler::chain_simple([
        grammar_sampler,
        LlamaSampler::temp(temp),
        LlamaSampler::dist(seed),
    ]);

    // ── Generate ──────────────────────────────────────────────────────────────
    let t0    = std::time::Instant::now();
    let eos   = inner.model.token_eos();
    let mut decoder = encoding_rs::UTF_8.new_decoder();
    let mut output  = String::new();
    let mut n_gen   = 0usize;

    for pos in n_input..(n_input + 512) {
        let next = sampler.sample(&ctx, batch.n_tokens() - 1);
        sampler.accept(next);
        if next == eos { break; }

        output.push_str(&inner.model.token_to_piece(next, &mut decoder, false, None)?);
        n_gen += 1;

        batch.clear();
        batch.add(next, pos as i32, &[0], true)?;
        ctx.decode(&mut batch)?;
    }

    let tps = n_gen as f32 / t0.elapsed().as_secs_f32().max(0.001);

    let json = serde_json::from_str::<serde_json::Value>(output.trim())
        .map_err(|e| anyhow::anyhow!("JSON parse failed: {e}\nraw: {output}"))?;

    Ok(LlmOutput {
        text:          output,
        param_update:  Some(json),
        tokens_per_sec: tps,
        context_used:  n_input + n_gen,
        is_jam:        false,
    })
}

/// Generate a plausible JSON response for testing without a real model.
/// Comments are intentionally terse — personality is generated by the real LLM
/// from the system prompt; there is no point faking it in mock mode.
pub fn mock_response(prompt: &str, heat: f32) -> Result<LlmOutput> {
    let prompt_lower = prompt.to_lowercase();

    // Instruction set takes priority — handles all specific add/remove commands
    // and named presets. Falls through to jam-variation when nothing matches.
    if let Some(inst) = instructions::InstructionSet::get().find_best_match(&prompt_lower) {
        let mut json = inst.params.clone();
        if json.get("_comment").is_none() {
            json["_comment"] = serde_json::json!(inst.comment);
        }
        return Ok(LlmOutput {
            text: serde_json::to_string_pretty(&json).unwrap_or_default(),
            param_update: Some(json),
            tokens_per_sec: 42.0,
            context_used: 256,
            is_jam: false,
        });
    }

    let json = if prompt_lower.contains("acid") {
        serde_json::json!({
            "_comment": "acid — bass resonance + env_mod up, short decay",
            "bass": { "cutoff": 0.5, "resonance": 0.70, "env_mod": 0.80, "decay": 0.38, "distortion": 0.05, "volume": 0.9 },
            "sequencer": { "bpm": 135.0 },
            "fx": { "distortion_drive": 0.0, "distortion_mix": 0.0, "delay_time": 0.3, "delay_feedback": 0.35, "delay_mix": 0.15 }
        })
    } else if prompt_lower.contains("dark") || prompt_lower.contains("deep") {
        serde_json::json!({
            "_comment": "dark — filter cutoff down, reverb + delay up",
            "bass": { "cutoff": 0.15, "resonance": 0.5, "env_mod": 0.3 },
            "fx": { "reverb_size": 0.8, "reverb_mix": 0.5, "delay_mix": 0.3 }
        })
    } else if prompt_lower.contains("fast") || prompt_lower.contains("hard") || prompt_lower.contains("harder") {
        serde_json::json!({
            "_comment": "harder — decay tighter, distortion drive up",
            "bass": { "decay": 0.2, "distortion": 0.12, "env_mod": 0.85 },
            "fx": { "distortion_drive": 0.15, "distortion_mix": 0.3 }
        })
    } else if prompt_lower.contains("mellow") || prompt_lower.contains("chill") || prompt_lower.contains("softer") || prompt_lower.contains("quieter") {
        serde_json::json!({
            "_comment": "softer — filter open, resonance back, volume down",
            "bass": { "cutoff": 0.6, "resonance": 0.3, "env_mod": 0.25, "volume": 0.75 },
            "fx": { "reverb_mix": 0.3, "delay_mix": 0.2 }
        })
    } else if prompt_lower.contains("melody") || prompt_lower.contains("notes") || prompt_lower.contains("bass line") || prompt_lower.contains("bassline")
           || prompt_lower.contains("clap") || prompt_lower.contains("snare")
           || prompt_lower.contains("hihat") || prompt_lower.contains("hi-hat") || prompt_lower.contains("hat")
           || prompt_lower.contains("kick") || prompt_lower.contains("pattern") {
        // Pattern requests require the real model — mock mode can only nudge knobs.
        serde_json::json!({
            "_comment": "pattern/rhythm request — needs real model; nudging filter in mock mode",
            "bass": { "env_mod": (0.4 + heat * 0.3).clamp(0.2, 0.95) }
        })
    } else if prompt_lower.contains("reverb") || prompt_lower.contains("space") || prompt_lower.contains("room") || prompt_lower.contains("atmosphere") {
        serde_json::json!({
            "_comment": "reverb + delay up for depth and space",
            "fx": { "reverb_mix": 0.3, "reverb_size": 0.6, "delay_mix": 0.18, "delay_feedback": 0.4 }
        })
    } else if prompt_lower.contains("delay") || prompt_lower.contains("echo") {
        serde_json::json!({
            "_comment": "dotted-eighth delay",
            "fx": { "delay_time": 0.375, "delay_feedback": 0.45, "delay_mix": 0.25 }
        })
    } else if prompt_lower.contains("distort") || prompt_lower.contains("drive") || prompt_lower.contains("grit") {
        serde_json::json!({
            "_comment": "master bus saturation up",
            "fx": { "distortion_drive": 0.25, "distortion_mix": 0.4 }
        })
    } else if prompt_lower.contains("no fx") || prompt_lower.contains("dry") || prompt_lower.contains("clean") {
        serde_json::json!({
            "_comment": "all FX cleared",
            "fx": { "reverb_mix": 0.0, "delay_mix": 0.0, "distortion_mix": 0.0, "distortion_drive": 0.0 }
        })
    } else if prompt_lower.contains("simpler") || prompt_lower.contains("strip") || prompt_lower.contains("minimal") {
        serde_json::json!({
            "_comment": "FX stripped back",
            "fx": { "reverb_mix": 0.0, "delay_mix": 0.0 }
        })
    } else {
        // Jam mode — heat controls how dramatic the mutation is
        let ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .subsec_millis() as f32;
        let p1 = ms / 1000.0;
        let p2 = ms * 1.7 / 1000.0;
        let p3 = ms * 0.4 / 1000.0;
        let sweep = heat * 0.5 + 0.05;
        let cut = (0.3 + (p1 * 2.7).sin().abs() * sweep * 1.6).clamp(0.15, 0.95);
        let res = (0.45 + (p2 * 1.9).cos() * sweep).clamp(0.2, 0.95);

        if heat < 0.3 {
            serde_json::json!({
                "_comment": "filter nudge",
                "bass": { "cutoff": cut, "resonance": res }
            })
        } else if heat < 0.6 {
            let bpm_shift = (p3.sin() * heat * 12.0) as i32;
            serde_json::json!({
                "_comment": "filter + bpm nudge",
                "bass": { "cutoff": cut, "resonance": res,
                           "env_mod": (0.5 + (p2 * 0.8).sin() * sweep).clamp(0.2, 0.95) },
                "sequencer": { "bpm": (130.0 + bpm_shift as f32).clamp(100.0, 160.0) }
            })
        } else if heat < 0.85 {
            let bpm = (125.0 + (p3 * 2.0).sin() * heat * 20.0).clamp(110.0, 155.0);
            serde_json::json!({
                "_comment": "filter + bpm push",
                "bass": { "cutoff": cut, "resonance": res,
                           "env_mod": (0.6 + (p1 * 1.1).cos() * sweep).clamp(0.3, 0.95),
                           "decay": (0.3 + (p2 * 0.5).sin().abs() * 0.4).clamp(0.15, 0.8) },
                "sequencer": { "bpm": bpm }
            })
        } else {
            let styles: &[(&str, f32, f32, f32, f32)] = &[
                ("full acid — resonance + chaos up",   0.2, 0.88, 0.85, 148.0),
                ("dark + slow — filter down, BPM down", 0.7, 0.40, 0.30,  95.0),
                ("hard + fast — filter sweep, BPM up",  0.5, 0.65, 0.70, 160.0),
                ("hypnotic — mid filter, lower BPM",    0.6, 0.55, 0.50, 118.0),
            ];
            let (cmt, cut2, res2, env2, bpm2) = styles[((ms * 0.003) as usize) % styles.len()];
            serde_json::json!({
                "_comment": cmt,
                "bass": { "cutoff": cut2, "resonance": res2, "env_mod": env2 },
                "sequencer": { "bpm": bpm2 }
            })
        }
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

        let heat = state.read().llm.heat;
        let t0 = Instant::now();
        let result = backend.infer(&system, &input.prompt, heat);
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
