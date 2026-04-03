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
    /// Chain-of-thought from <think>…</think> blocks, if the model emitted one.
    pub thinking: Option<String>,
}

// ─── LLM backend trait (swappable) ────────────────────────────────────────────

pub trait LlmBackend: Send {
    fn infer(&mut self, system: &str, user: &str, heat: f32) -> Result<LlmOutput>;
}

// ─── Bonsai server backend ────────────────────────────────────────────────────
// Spawns PrismML's llama-server as a child process and talks to it over HTTP.
// Falls back to mock if the server binary or model file is not found.
//
// Build the server:  ./build-bonsai-server.sh
// Model:             ./download-models.sh

/// Candidate paths for the llama-server binary (PrismML fork).
const SERVER_BINARY_CANDIDATES: &[&str] = &[
    ".llama-build/bin/llama-server",  // built by build-bonsai-server.sh
    "llama-server",                    // $PATH
];

pub struct LlamaServerBackend {
    child:    Option<std::process::Child>,
    base_url: String,
    live:     bool,
}

impl LlamaServerBackend {
    /// Spawn `llama-server` with the given model file.
    /// Returns a backend in mock mode if binary or model are missing.
    pub fn new(model_path: &str) -> Self {
        let bin = SERVER_BINARY_CANDIDATES.iter()
            .find(|&&p| std::path::Path::new(p).exists()
                || which_in_path(p))
            .copied();

        let Some(bin) = bin else {
            log::warn!(
                "llama-server not found — run ./build-bonsai-server.sh to build it. \
                 Falling back to mock."
            );
            return Self { child: None, base_url: String::new(), live: false };
        };

        if !std::path::Path::new(model_path).exists() {
            log::warn!(
                "Model not found at '{}' — run ./download-models.sh. \
                 Falling back to mock.",
                model_path
            );
            return Self { child: None, base_url: String::new(), live: false };
        }

        // Pick a free port
        let port = free_port().unwrap_or(9999);
        let base_url = format!("http://127.0.0.1:{}", port);

        log::info!("Spawning llama-server on port {} with model {}", port, model_path);

        let child = std::process::Command::new(bin)
            .args([
                "--model", model_path,
                "--host", "127.0.0.1",
                "--port", &port.to_string(),
                "--ctx-size", "4096",
                "--n-gpu-layers", "99",
                "--log-disable",   // reduce noise; we log our own status
            ])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();

        match child {
            Err(e) => {
                log::error!("Failed to spawn llama-server: {} — falling back to mock.", e);
                Self { child: None, base_url: String::new(), live: false }
            }
            Ok(child) => {
                let mut backend = Self { child: Some(child), base_url, live: false };
                backend.wait_for_ready();
                backend
            }
        }
    }

    /// Connect to an already-running llama-server without spawning a new process.
    #[allow(dead_code)] // used by llm-tests feature (llm_suite.rs)
    /// Used by the LLM test suite when `LLAMA_SERVER_URL` is set.
    pub fn connect(base_url: &str) -> Self {
        let url = format!("{}/health", base_url);
        let live = ureq::get(&url).call()
            .map(|r| r.status() == 200)
            .unwrap_or(false);
        if !live {
            log::warn!("LlamaServerBackend::connect: server at {} not responding", base_url);
        }
        Self { child: None, base_url: base_url.to_string(), live }
    }

    pub fn is_live(&self) -> bool { self.live }

    /// Poll /health until the server is ready (up to ~30 s).
    fn wait_for_ready(&mut self) {
        let url = format!("{}/health", self.base_url);
        for attempt in 0..60 {
            std::thread::sleep(std::time::Duration::from_millis(500));
            match ureq::get(&url).call() {
                Ok(resp) if resp.status() == 200 => {
                    log::info!("Bonsai server ready after {}ms", (attempt + 1) * 500);
                    self.live = true;
                    return;
                }
                _ => {}
            }
        }
        log::error!("Bonsai server did not become ready within 30s — falling back to mock.");
    }
}

impl Drop for LlamaServerBackend {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            // llama-server can ignore SIGTERM while in CPU/GPU kernel work — use SIGKILL.
            #[cfg(unix)]
            {
                let pid = child.id() as i32;
                unsafe { libc::kill(pid, libc::SIGKILL); }
            }
            #[cfg(not(unix))]
            let _ = child.kill();
            let _ = child.wait();
            log::info!("Bonsai server stopped.");
        }
    }
}

/// Timeout for a single inference call.  8B models on CPU can take 60–90 s.
const INFER_TIMEOUT_SECS: u64 = 180;

impl LlmBackend for LlamaServerBackend {
    fn infer(&mut self, system: &str, user: &str, heat: f32) -> Result<LlmOutput> {
        if !self.live {
            return mock_response(user, heat);
        }

        let url = format!("{}/v1/chat/completions", self.base_url);
        // heat 0.0 → temp 0.1 (near-deterministic), heat 1.0 → temp 1.2 (creative)
        let temperature = 0.1_f64 + (heat as f64).clamp(0.0, 1.0) * 1.1;

        // json_object mode keeps the server honest about emitting valid JSON.
        // max_tokens: a full response with two 16-step arrays + bass params needs ~350–400
        // tokens; 512 gives comfortable headroom without wasting inference time.
        let body = serde_json::json!({
            "model": "bonsai",
            "messages": [
                { "role": "system",  "content": system },
                { "role": "user",    "content": user   }
            ],
            "temperature": temperature,
            "max_tokens": 512,
            "response_format": { "type": "json_object" }
        });

        let agent = ureq::AgentBuilder::new()
            .timeout(std::time::Duration::from_secs(INFER_TIMEOUT_SECS))
            .build();

        let t0 = std::time::Instant::now();
        let resp = agent
            .post(&url)
            .set("Content-Type", "application/json")
            .send_json(&body)
            .map_err(|e| anyhow::anyhow!("llama-server request failed: {}", e))?;

        let elapsed = t0.elapsed().as_secs_f32();

        let resp_text = resp.into_string()
            .map_err(|e| anyhow::anyhow!("failed to read server response body: {}", e))?;

        let resp_json: serde_json::Value = serde_json::from_str(&resp_text)
            .map_err(|e| anyhow::anyhow!("failed to parse server JSON: {e}\nraw: {resp_text}"))?;

        let raw_content = resp_json["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or("")
            .to_string();

        if raw_content.is_empty() {
            log::warn!("llama-server returned empty content; full response: {resp_text}");
        }

        // Strip <think>…</think> chain-of-thought block that Qwen3 may emit.
        let (thinking, json_text) = split_thinking(&raw_content);
        if thinking.is_some() {
            log::debug!("Bonsai thinking: {} chars", thinking.as_ref().unwrap().len());
        }

        let usage = &resp_json["usage"];
        let completion_tokens = usage["completion_tokens"].as_f64().unwrap_or(0.0) as f32;
        let tps = if elapsed > 0.0 { completion_tokens / elapsed } else { 0.0 };
        let ctx_used = usage["total_tokens"].as_u64().unwrap_or(0) as usize;

        let param_update = repair_json(json_text.trim())
            .ok_or_else(|| anyhow::anyhow!("JSON parse and repair both failed\nraw: {json_text}"))?;

        Ok(LlmOutput {
            text: json_text,
            param_update: Some(param_update),
            tokens_per_sec: tps,
            context_used: ctx_used,
            is_jam: false,
            thinking,
        })
    }

}

// ─── Helpers ──────────────────────────────────────────────────────────────────

/// Best-effort JSON repair for truncated or structurally confused LLM output.
/// 1. Try parsing as-is.
/// 2. Close unclosed brackets and retry.
/// 3. Sanitize the resulting structure (lift misplaced keys, remove nested fx loops).
fn repair_json(s: &str) -> Option<serde_json::Value> {
    // Fast path — valid JSON
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(s) {
        let fixed = sanitize_json_structure(v);
        return Some(fixed);
    }

    log::warn!("JSON parse failed — attempting repair (truncated/malformed output)");

    // Build a repaired candidate by closing unclosed brackets.
    let mut attempt = s.trim_end_matches([',', ' ', '\n', '\r', ':']).to_string();

    // Count unclosed nesting levels (ignore chars inside strings).
    let (mut obj_depth, mut arr_depth) = (0i32, 0i32);
    let mut in_str = false;
    let mut esc = false;
    for c in attempt.chars() {
        if esc        { esc = false; continue; }
        if c == '\\'  && in_str { esc = true; continue; }
        if c == '"'   { in_str = !in_str; continue; }
        if !in_str    {
            match c {
                '{' => obj_depth += 1,
                '}' => obj_depth -= 1,
                '[' => arr_depth += 1,
                ']' => arr_depth -= 1,
                _   => {}
            }
        }
    }
    // Close in reverse order: arrays first, then objects
    for _ in 0..arr_depth.max(0) { attempt.push(']'); }
    for _ in 0..obj_depth.max(0) { attempt.push('}'); }

    serde_json::from_str::<serde_json::Value>(&attempt)
        .ok()
        .map(sanitize_json_structure)
}

/// Promote `bass` and `fx` keys that the model incorrectly nests inside `sequencer`
/// back to the top level, and strip `"fx"` keys nested inside `"fx"` (looping pattern).
fn sanitize_json_structure(v: serde_json::Value) -> serde_json::Value {
    let mut obj = match v {
        serde_json::Value::Object(m) => m,
        other => return other,
    };

    // Extract misplaced keys from inside "sequencer"
    let (bass_lift, fx_lift) = if let Some(seq) = obj.get_mut("sequencer").and_then(|s| s.as_object_mut()) {
        (seq.remove("bass"), seq.remove("fx"))
    } else {
        (None, None)
    };
    if let Some(b) = bass_lift {
        obj.entry("bass").or_insert(b);
    }
    if let Some(f) = fx_lift {
        obj.entry("fx").or_insert(f);
    }

    // Remove nested "fx" inside "fx" (the recursive loop the model falls into)
    if let Some(fx) = obj.get_mut("fx").and_then(|f| f.as_object_mut()) {
        fx.remove("fx");
    }

    serde_json::Value::Object(obj)
}

/// Split a `<think>…</think>` block from the front of a model response.
/// Returns `(thinking_text, remainder)`.  If no block is present, thinking is None
/// and the whole string is returned as remainder.
fn split_thinking(s: &str) -> (Option<String>, String) {
    let s = s.trim();
    if let Some(rest) = s.strip_prefix("<think>") {
        if let Some(end) = rest.find("</think>") {
            let thinking = rest[..end].trim().to_string();
            let after = rest[end + "</think>".len()..].trim().to_string();
            return (Some(thinking).filter(|t| !t.is_empty()), after);
        }
    }
    (None, s.to_string())
}

fn free_port() -> Option<u16> {
    std::net::TcpListener::bind("127.0.0.1:0").ok()
        .and_then(|l| l.local_addr().ok())
        .map(|a| a.port())
}

fn which_in_path(name: &str) -> bool {
    std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths)
            .any(|dir| dir.join(name).exists()))
        .unwrap_or(false)
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
            thinking: None,
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
        is_jam: false,
        thinking: None,
    })
}

// ─── LLM thread loop ──────────────────────────────────────────────────────────

pub fn run_llm_loop(
    state: Arc<RwLock<AppState>>,
    input_rx: Receiver<LlmInput>,
    output_tx: Sender<LlmOutput>,
) {
    let model_path = state.read().llm.model_path.clone();
    let mut backend = LlamaServerBackend::new(&model_path);

    if backend.is_live() {
        log::info!("LLM thread started — Bonsai server live: {}", model_path);
        let _ = output_tx.try_send(LlmOutput {
            text: format!("[ Bonsai server loaded: {} ]", model_path),
            param_update: None,
            tokens_per_sec: 0.0,
            context_used: 0,
            is_jam: false,
            thinking: None,
        });
    } else {
        log::warn!(
            "LLM thread started — mock mode. \
             Run ./build-bonsai-server.sh + ./download-models.sh to enable real inference."
        );
        let _ = output_tx.try_send(LlmOutput {
            text: "[ Mock mode — run ./build-bonsai-server.sh + ./download-models.sh ]".to_string(),
            param_update: None,
            tokens_per_sec: 0.0,
            context_used: 0,
            is_jam: false,
            thinking: None,
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
                thinking: None,
            });
        }
    }

    log::info!("LLM thread exiting");
}
