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
use parking_lot::RwLock;
use std::sync::Arc;
use std::time::Instant;

use crate::state::{AppState, ConversationMode, apply_llm_update};

// ─── Messages ─────────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub enum LlmInput {
    /// Run inference with a user prompt.
    Infer { prompt: String, one_shot: bool },
    /// Drop the current backend and reload with a new model file.
    SwitchModel(String),
}

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
}

// ─── LLM backend trait (swappable) ────────────────────────────────────────────

pub trait LlmBackend: Send {
    fn infer(&mut self, system: &str, user: &str, heat: f32) -> Result<LlmOutput>;
}

// ─── LLM server backend ───────────────────────────────────────────────────────
// Spawns PrismML's llama-server as a child process and talks to it over HTTP.
// Falls back to mock if the server binary or model file is not found.
//
// Build the server:  ./build-bonsai-server.sh
// Model:             ./download-models.sh

/// Candidate paths for the llama-server binary (PrismML fork).
const SERVER_BINARY_CANDIDATES: &[&str] = &[
    ".llama-build/bin/llama-server", // built by build-bonsai-server.sh
    "llama-server",                  // $PATH
];

/// Fixed port for llama-server.  Using a fixed port prevents the process-leak
/// problem that occurs with random ports: each restart now lands on the same
/// address so the OS rejects a second bind, and we can detect + reuse an
/// already-running healthy server.
const LLAMA_PORT: u16 = 8766;

pub struct LlamaServerBackend {
    child: Option<std::process::Child>,
    base_url: String,
    live: bool,
}

/// Kill any leftover llama-server processes from a previous run.
/// Called before spawning a new instance so stale processes don't compete for
/// GPU memory or hold the fixed port.
fn kill_leaked_servers(bin_path: &str) {
    #[cfg(unix)]
    {
        // Match on the full binary path to avoid killing unrelated processes.
        let _ = std::process::Command::new("pkill")
            .args(["-KILL", "-f", &format!("{} --model", bin_path)])
            .status();
        // Brief pause so the OS reclaims the port before we try to bind it.
        std::thread::sleep(std::time::Duration::from_millis(400));
    }
}

impl LlamaServerBackend {
    /// Spawn `llama-server` with the given model file.
    /// Returns `live: false` (hard-fail path) if binary or model are missing —
    /// the caller (`run_llm_loop`) decides whether to exit or continue in mock.
    pub fn new(model_path: &str) -> Self {
        let bin = SERVER_BINARY_CANDIDATES
            .iter()
            .find(|&&p| std::path::Path::new(p).exists() || which_in_path(p))
            .copied();

        let Some(bin) = bin else {
            log::error!(
                "llama-server binary not found — run ./build-bonsai-server.sh to build it."
            );
            return Self {
                child: None,
                base_url: String::new(),
                live: false,
            };
        };

        if !std::path::Path::new(model_path).exists() {
            log::error!(
                "Model not found at '{}' — run ./download-models.sh.",
                model_path
            );
            return Self {
                child: None,
                base_url: String::new(),
                live: false,
            };
        }

        let port = LLAMA_PORT;
        let base_url = format!("http://127.0.0.1:{}", port);

        // Reuse an already-healthy server (e.g. user restarted the UI without
        // killing the server) — avoids a 30–90 s reload of the model.
        let health_url = format!("{}/health", base_url);
        if ureq::get(&health_url)
            .call()
            .map(|r| r.status() == 200)
            .unwrap_or(false)
        {
            log::info!(
                "Reusing existing llama-server on port {} (already healthy)",
                port
            );
            return Self {
                child: None,
                base_url,
                live: true,
            };
        }

        // Kill any leaked process from a previous run that holds the port.
        kill_leaked_servers(bin);

        log::info!(
            "Spawning llama-server on port {} with model {}",
            port,
            model_path
        );

        let child = std::process::Command::new(bin)
            .args([
                "--model",
                model_path,
                "--host",
                "127.0.0.1",
                "--port",
                &port.to_string(),
                "--ctx-size",
                "8192",
                "--n-gpu-layers",
                "99",
                "--log-disable", // reduce noise; we log our own status
            ])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();

        match child {
            Err(e) => {
                log::error!(
                    "Failed to spawn llama-server: {} — falling back to mock.",
                    e
                );
                Self {
                    child: None,
                    base_url: String::new(),
                    live: false,
                }
            }
            Ok(child) => {
                let mut backend = Self {
                    child: Some(child),
                    base_url,
                    live: false,
                };
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
        let live = ureq::get(&url)
            .call()
            .map(|r| r.status() == 200)
            .unwrap_or(false);
        if !live {
            log::warn!(
                "LlamaServerBackend::connect: server at {} not responding",
                base_url
            );
        }
        Self {
            child: None,
            base_url: base_url.to_string(),
            live,
        }
    }

    pub fn is_live(&self) -> bool {
        self.live
    }

    /// Poll /health until the server is ready (up to ~90 s).
    /// llama-server can take a while to load a large model into VRAM.
    fn wait_for_ready(&mut self) {
        let url = format!("{}/health", self.base_url);
        for attempt in 0..180 {
            std::thread::sleep(std::time::Duration::from_millis(500));
            match ureq::get(&url).call() {
                Ok(resp) if resp.status() == 200 => {
                    log::info!("LLM server ready after {}ms", (attempt + 1) * 500);
                    self.live = true;
                    return;
                }
                _ => {}
            }
        }
        log::error!("LLM server did not become ready within 90s — falling back to mock.");
    }
}

impl Drop for LlamaServerBackend {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            // llama-server can ignore SIGTERM while in CPU/GPU kernel work — use SIGKILL.
            #[cfg(unix)]
            {
                let pid = child.id() as i32;
                unsafe {
                    libc::kill(pid, libc::SIGKILL);
                }
            }
            #[cfg(not(unix))]
            let _ = child.kill();
            let _ = child.wait();
            log::info!("LLM server stopped.");
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
        // max_tokens: two 16-step arrays + bass params ~400 tokens; _thinking adds ~100.
        // 768 gives comfortable headroom.
        let body = serde_json::json!({
            "model": "bonsai",
            "messages": [
                { "role": "system",  "content": system },
                { "role": "user",    "content": user   }
            ],
            "temperature": temperature,
            "max_tokens": 768,
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
            .map_err(|e| match e {
                ureq::Error::Status(code, response) => {
                    let body = response.into_string().unwrap_or_default();
                    anyhow::anyhow!("llama-server request failed: status code {code}\nbody: {body}")
                }
                other => anyhow::anyhow!("llama-server request failed: {other}"),
            })?;

        let elapsed = t0.elapsed().as_secs_f32();

        let resp_text = resp
            .into_string()
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

        // Strip <think>…</think> if the model emits it (Qwen3-style), otherwise pass through.
        let (tag_thinking, json_text) = split_thinking(&raw_content);

        let usage = &resp_json["usage"];
        let prompt_tok = usage["prompt_tokens"].as_u64().unwrap_or(0) as usize;
        let compl_tok = usage["completion_tokens"].as_u64().unwrap_or(0) as usize;
        let tps = if elapsed > 0.0 {
            compl_tok as f32 / elapsed
        } else {
            0.0
        };
        let ctx_used = usage["total_tokens"].as_u64().unwrap_or(0) as usize;

        let mut param_update = repair_json(json_text.trim()).ok_or_else(|| {
            anyhow::anyhow!("JSON parse and repair both failed\nraw: {json_text}")
        })?;

        // Extract _thinking from the JSON itself (our prompted reasoning field).
        // Prefer tag-based thinking if the model produced it; fall back to _thinking field.
        let thinking = tag_thinking.or_else(|| {
            param_update
                .as_object_mut()
                .and_then(|o| o.remove("_thinking"))
                .and_then(|v| v.as_str().map(|s| s.to_string()))
                .filter(|s| !s.is_empty())
        });

        if let Some(ref t) = thinking {
            log::debug!(
                "LLM thinking ({} chars): {}",
                t.len(),
                &t[..t.len().min(120)]
            );
        }

        // Extract mc_line — crowd-facing shout for MC/DJ mode TTS.
        let mc_line = param_update
            .as_object_mut()
            .and_then(|o| o.remove("mc_line"))
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .filter(|s| !s.is_empty());

        Ok(LlmOutput {
            text: json_text,
            param_update: Some(param_update),
            tokens_per_sec: tps,
            prompt_tokens: prompt_tok,
            completion_tokens: compl_tok,
            context_used: ctx_used,
            is_jam: false,
            thinking,
            mc_line,
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
        if esc {
            esc = false;
            continue;
        }
        if c == '\\' && in_str {
            esc = true;
            continue;
        }
        if c == '"' {
            in_str = !in_str;
            continue;
        }
        if !in_str {
            match c {
                '{' => obj_depth += 1,
                '}' => obj_depth -= 1,
                '[' => arr_depth += 1,
                ']' => arr_depth -= 1,
                _ => {}
            }
        }
    }
    // Close in reverse order: arrays first, then objects
    for _ in 0..arr_depth.max(0) {
        attempt.push(']');
    }
    for _ in 0..obj_depth.max(0) {
        attempt.push('}');
    }

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
    let (bass_lift, fx_lift) =
        if let Some(seq) = obj.get_mut("sequencer").and_then(|s| s.as_object_mut()) {
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
    if let Some(rest) = s.strip_prefix("<think>")
        && let Some(end) = rest.find("</think>")
    {
        let thinking = rest[..end].trim().to_string();
        let after = rest[end + "</think>".len()..].trim().to_string();
        return (Some(thinking).filter(|t| !t.is_empty()), after);
    }
    (None, s.to_string())
}

fn which_in_path(name: &str) -> bool {
    std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).any(|dir| dir.join(name).exists()))
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
            prompt_tokens: 0,
            completion_tokens: 0,
            context_used: 256,
            is_jam: false,
            thinking: None,
            mc_line: None,
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
    } else if prompt_lower.contains("fast")
        || prompt_lower.contains("hard")
        || prompt_lower.contains("harder")
    {
        serde_json::json!({
            "_comment": "harder — decay tighter, distortion drive up",
            "bass": { "decay": 0.2, "distortion": 0.12, "env_mod": 0.85 },
            "fx": { "distortion_drive": 0.15, "distortion_mix": 0.3 }
        })
    } else if prompt_lower.contains("mellow")
        || prompt_lower.contains("chill")
        || prompt_lower.contains("softer")
        || prompt_lower.contains("quieter")
    {
        serde_json::json!({
            "_comment": "softer — filter open, resonance back, volume down",
            "bass": { "cutoff": 0.6, "resonance": 0.3, "env_mod": 0.25, "volume": 0.75 },
            "fx": { "reverb_mix": 0.3, "delay_mix": 0.2 }
        })
    } else if prompt_lower.contains("melody")
        || prompt_lower.contains("notes")
        || prompt_lower.contains("bass line")
        || prompt_lower.contains("bassline")
        || prompt_lower.contains("clap")
        || prompt_lower.contains("snare")
        || prompt_lower.contains("hihat")
        || prompt_lower.contains("hi-hat")
        || prompt_lower.contains("hat")
        || prompt_lower.contains("kick")
        || prompt_lower.contains("pattern")
    {
        // Pattern requests require the real model — mock mode can only nudge knobs.
        serde_json::json!({
            "_comment": "pattern/rhythm request — needs real model; nudging filter in mock mode",
            "bass": { "env_mod": (0.4 + heat * 0.3).clamp(0.2, 0.95) }
        })
    } else if prompt_lower.contains("reverb")
        || prompt_lower.contains("space")
        || prompt_lower.contains("room")
        || prompt_lower.contains("atmosphere")
    {
        serde_json::json!({
            "_comment": "reverb + delay up for depth and space",
            "fx": { "reverb_mix": 0.3, "reverb_size": 0.6, "delay_mix": 0.18, "delay_feedback": 0.4 }
        })
    } else if prompt_lower.contains("delay") || prompt_lower.contains("echo") {
        serde_json::json!({
            "_comment": "dotted-eighth delay",
            "fx": { "delay_time": 0.375, "delay_feedback": 0.45, "delay_mix": 0.25 }
        })
    } else if prompt_lower.contains("distort")
        || prompt_lower.contains("drive")
        || prompt_lower.contains("grit")
    {
        serde_json::json!({
            "_comment": "master bus saturation up",
            "fx": { "distortion_drive": 0.25, "distortion_mix": 0.4 }
        })
    } else if prompt_lower.contains("no fx")
        || prompt_lower.contains("dry")
        || prompt_lower.contains("clean")
    {
        serde_json::json!({
            "_comment": "all FX cleared",
            "fx": { "reverb_mix": 0.0, "delay_mix": 0.0, "distortion_mix": 0.0, "distortion_drive": 0.0 }
        })
    } else if prompt_lower.contains("simpler")
        || prompt_lower.contains("strip")
        || prompt_lower.contains("minimal")
    {
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
                ("full acid — resonance + chaos up", 0.2, 0.88, 0.85, 148.0),
                ("dark + slow — filter down, BPM down", 0.7, 0.40, 0.30, 95.0),
                ("hard + fast — filter sweep, BPM up", 0.5, 0.65, 0.70, 160.0),
                ("hypnotic — mid filter, lower BPM", 0.6, 0.55, 0.50, 118.0),
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
        prompt_tokens: 0,
        completion_tokens: 0,
        context_used: 256,
        is_jam: false,
        thinking: None,
        mc_line: None,
    })
}

// ─── LLM thread loop ──────────────────────────────────────────────────────────

pub fn run_llm_loop(
    state: Arc<RwLock<AppState>>,
    input_rx: Receiver<LlmInput>,
    output_tx: Sender<LlmOutput>,
    mock: bool,
) {
    if mock {
        // Explicit --mock flag: skip server entirely, run mock forever.
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
        });
        run_mock_loop(state, input_rx, output_tx);
        return;
    }

    let model_path = state.read().llm.model_path.clone();
    let mut backend = LlamaServerBackend::new(&model_path);

    if !backend.is_live() {
        // Server failed to start and --mock was not passed: hard fail.
        {
            let mut s = state.write();
            s.llm.is_mock = false; // not mock — this is a hard error
            s.llm.llm_initializing = false;
        }
        log::error!(
            "LLM server unavailable and --mock not passed. \
             Build the server: ./build-bonsai-server.sh \
             Download a model: ./download-models.sh \
             Then restart. Pass --mock to run without a model."
        );
        // Give the UI a moment to paint the error state before exiting.
        std::thread::sleep(std::time::Duration::from_secs(1));
        std::process::exit(1);
    }

    // Server is live.
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
    });

    while let Ok(input) = input_rx.recv() {
        // ── Model switch ──────────────────────────────────────────────────────
        if let LlmInput::SwitchModel(ref new_path) = input {
            log::info!("LLM: switching model -> {}", new_path);
            state.write().llm.model_path = new_path.clone();
            // Drop old backend (kills server subprocess if owned), reload new one
            backend = LlamaServerBackend::new(new_path);
            state.write().llm.is_mock = !backend.is_live();
            let status = if backend.is_live() {
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
            });
            continue;
        }

        let LlmInput::Infer {
            ref prompt,
            one_shot,
        } = input
        else {
            continue;
        };

        // Snapshot prompt before acquiring any lock (clone outside critical section)
        if one_shot {
            log::info!("YOU -> {}", prompt);
        } else {
            log::debug!("YOU (jam) -> {}", prompt);
        }

        // Build system prompt from a read snapshot — lock held for clone only
        let system = build_system_prompt(&state.read().clone());

        // Brief write to mark inferring + store last prompt
        {
            let mut s = state.write();
            s.llm.is_inferring = true;
            s.llm.last_prompt = prompt.clone();
        }

        let heat = state.read().llm.heat;
        let enable_thinking = state.read().llm.enable_thinking;
        // Qwen3-style thinking toggle: /think enables chain-of-thought (slower, deeper),
        // /no_think disables it (faster responses for simple commands).
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
        let result = backend.infer(&system, &think_prompt, heat);
        let elapsed = t0.elapsed().as_secs_f32();

        match result {
            Ok(output) => {
                let tps = output.tokens_per_sec;
                let ptok = output.prompt_tokens;
                let ctok = output.completion_tokens;
                let ctx = output.context_used;
                let tthink = output.thinking.as_ref().map(|t| t.len() / 4).unwrap_or(0);

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
                    s.llm.prompt_tokens = ptok;
                    s.llm.completion_tokens = ctok;
                    s.llm.context_used = ctx;
                    s.llm.thinking_tokens = tthink;
                    s.llm.last_response = output.text.clone();
                }

                // Log the natural-language comment if present, otherwise the raw response
                if let Some(ref update) = output.param_update {
                    let comment = update
                        .get("_comment")
                        .and_then(|v| v.as_str())
                        .unwrap_or(&output.text);
                    let persona = state.read().llm.persona_name.clone();
                    if one_shot {
                        log::info!("{} -> {}", persona, comment);
                    } else {
                        log::debug!("{} (jam) -> {}", persona, comment);
                    }

                    // TTS: speak mc_line (crowd shout) if present, else _comment.
                    // Only fires in MC/DJ modes — producer mode is never read aloud.
                    let (tts_on, mode, tts_pitch, tts_speed, tts_amplitude) = {
                        let s = state.read();
                        (
                            s.llm.tts_enabled,
                            s.llm.conversation_mode.clone(),
                            s.llm.tts_pitch,
                            s.llm.tts_speed,
                            s.llm.tts_amplitude,
                        )
                    };
                    let tts_mode = matches!(mode, ConversationMode::Mc | ConversationMode::Dj);
                    if tts_on && tts_mode {
                        let tts_text = output.mc_line.as_deref().unwrap_or(comment);
                        log::info!("[TTS] {}", tts_text);
                        speak(tts_text, &mode, tts_pitch, tts_speed, tts_amplitude);
                    }
                }
                let mut output = output;
                output.is_jam = !one_shot;
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
        if one_shot {
            // nothing — wait for next prompt
        } else {
            let _ = input_rx.try_recv(); // drain any queued prompts
            // Signal back so UI can re-trigger if in auto_jam mode
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
            });
        }
    }

    log::info!("LLM thread exiting");
}

// ─── Mock loop ───────────────────────────────────────────────────────────────

/// Runs when --mock is explicitly passed.  Handles inference with `mock_response`
/// and model-switch no-ops.  Never exits unless the channel closes.
fn run_mock_loop(
    state: Arc<RwLock<AppState>>,
    input_rx: Receiver<LlmInput>,
    output_tx: Sender<LlmOutput>,
) {
    while let Ok(input) = input_rx.recv() {
        if let LlmInput::SwitchModel(ref new_path) = input {
            log::warn!(
                "Mock mode: model switch to '{}' ignored (no real backend)",
                new_path
            );
            continue;
        }

        let LlmInput::Infer {
            ref prompt,
            one_shot,
        } = input
        else {
            continue;
        };

        let heat = state.read().llm.heat;
        {
            let mut s = state.write();
            s.llm.is_inferring = true;
            s.llm.last_prompt = prompt.clone();
        }

        match mock_response(prompt, heat) {
            Ok(output) => {
                if let Some(ref update) = output.param_update {
                    let current = state.read().clone();
                    let next = apply_llm_update(current, update);
                    *state.write() = next;
                    let comment = update
                        .get("_comment")
                        .and_then(|v| v.as_str())
                        .unwrap_or("[mock]");
                    let persona = state.read().llm.persona_name.clone();
                    if one_shot {
                        log::info!("{} (mock) -> {}", persona, comment);
                    }
                }
                {
                    let mut s = state.write();
                    s.llm.is_inferring = false;
                    s.llm.tokens_per_sec = output.tokens_per_sec;
                    s.llm.last_response = output.text.clone();
                }
                let mut output = output;
                output.is_jam = !one_shot;
                let _ = output_tx.try_send(output);
                if !one_shot {
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
                    });
                }
            }
            Err(e) => {
                log::error!("Mock inference error: {}", e);
                state.write().llm.is_inferring = false;
            }
        }
    }
    log::info!("Mock LLM loop exiting");
}

pub mod tts;
pub use tts::speak;
