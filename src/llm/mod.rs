// ─── llm/mod.rs ──────────────────────────────────────────────────────────────
// LLM inference thread.
// Loads a GGUF model and runs continuous inference to control synth params.
// Communicates with UI via crossbeam channels.

pub mod instructions;
mod json_repair;
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
use json_repair::{repair_json, split_thinking};

// ─── Messages ─────────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub enum LlmInput {
    /// Run inference with a user prompt.
    Infer { prompt: String, one_shot: bool },
    /// Kill the current server and restart with a new model file.
    SwitchModel(String),
    /// Kill and restart the server with the same model (clears KV cache / context window).
    ResetContext,
}

#[derive(Clone, Debug)]
pub enum LlmAction {
    SaveProject,
    SetHeat(f32),
    SetPersona(String),
    SetConversationMode(String),
    SetJamBars(f32),
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
    /// Snapshot of `AppState` taken immediately before the param update was applied.
    /// The UI uses this to push a correct undo entry.
    pub before_state: Option<Box<AppState>>,
    /// Actions extracted from the JSON response (save_project, heat, settings changes).
    pub actions: Vec<LlmAction>,
}

// ─── Sampling parameters (passed through to llama-server) ────────────────────

#[derive(Clone, Debug)]
pub struct SamplingParams {
    pub heat: f32, // 0–1: jam mutation intensity; drives temperature when temp_override is None
    pub temp_override: Option<f32>, // explicit temperature (0.0–2.0); bypasses heat→temp formula when Some
    pub top_k: i32,                 // 0 = disabled; Gemma default 64
    pub top_p: f32,                 // 0.0–1.0 nucleus; Gemma default 0.95
    pub min_p: f32,                 // 0.0–1.0 min prob floor; llama.cpp default 0.05
    pub repeat_penalty: f32,        // 1.0 = off; >1.0 penalises repeats
    pub frequency_penalty: f32,     // 0.0 = off (OpenAI-compat)
    pub seed: i64,                  // -1 = random
}

impl Default for SamplingParams {
    fn default() -> Self {
        Self {
            heat: 0.4,
            temp_override: None,
            top_k: 64,
            top_p: 0.95,
            min_p: 0.05,
            repeat_penalty: 1.0,
            frequency_penalty: 0.0,
            seed: -1,
        }
    }
}

// ─── LLM backend trait (swappable) ────────────────────────────────────────────

pub trait LlmBackend: Send {
    fn infer(&mut self, system: &str, user: &str, sampling: &SamplingParams) -> Result<LlmOutput>;
}

// ─── LLM server backend ───────────────────────────────────────────────────────
// Spawns llama-server as a child process and talks to it over HTTP.
// Falls back to mock if the server binary or model file is not found.
//
// Default model (Gemma 4 E4B): ./scripts/build-llama-server.sh
//                               ./scripts/download-models.sh
// Bonsai fallback (1-bit):      ./scripts/build-bonsai-server.sh
//                               ./scripts/download-models.sh bonsai

/// Candidate paths for the llama-server binary.
/// Checked in order — PrismML fork first (required for Bonsai 1-bit),
/// then official llama.cpp build (required for Gemma 4 / newer architectures),
/// then $PATH fallback.
const SERVER_BINARY_CANDIDATES: &[&str] = &[
    ".llama-build/bin/llama-server", // PrismML fork — build-bonsai-server.sh
    ".llama-official-build/bin/llama-server", // official llama.cpp — build-llama-server.sh
    "llama-server",                  // $PATH
];

/// Fixed port for llama-server.  Using a fixed port prevents the process-leak
/// problem that occurs with random ports: each restart now lands on the same
/// address so the OS rejects a second bind, and we can detect + reuse an
/// already-running healthy server.
const LLAMA_PORT: u16 = 8766;

/// Pick the right llama-server binary for the given model path.
///
/// - Bonsai 8B (Q1_0_g128): requires the PrismML fork
///   (.llama-build/bin/llama-server)
/// - All other standard GGUF models: prefer the official build
///   (.llama-official-build/bin/llama-server) then fall back to PrismML fork
///   (which handles Qwen3, Gemma 4, etc.) then $PATH
fn pick_server_binary(model_path: &str) -> Option<&'static str> {
    let is_bonsai = model_path.to_lowercase().contains("bonsai");

    if is_bonsai {
        // Bonsai requires the PrismML fork — try it first
        SERVER_BINARY_CANDIDATES
            .iter()
            .copied()
            .find(|&p| std::path::Path::new(p).exists() || which_in_path(p))
    } else {
        // For standard GGUF models prefer the official build, then PrismML fork, then $PATH.
        // Official build handles newer architectures (Gemma 4, etc.) that the fork may not.
        let preference: &[&str] = &[
            ".llama-official-build/bin/llama-server",
            ".llama-build/bin/llama-server",
            "llama-server",
        ];
        preference
            .iter()
            .copied()
            .find(|&p| std::path::Path::new(p).exists() || which_in_path(p))
    }
}

pub struct LlamaServerBackend {
    child: Option<std::process::Child>,
    base_url: String,
    live: bool,
}

/// Kill any leftover llama-server processes from a previous run.
/// Called before spawning a new instance so stale processes don't compete for
/// GPU memory or hold the fixed port.
fn kill_leaked_servers(_bin_path: &str) {
    #[cfg(unix)]
    {
        // Match on the full binary path to avoid killing unrelated processes.
        let _ = std::process::Command::new("pkill")
            .args(["-KILL", "-f", &format!("{} --model", _bin_path)])
            .status();
        // Brief pause so the OS reclaims the port before we try to bind it.
        std::thread::sleep(std::time::Duration::from_millis(400));
    }
}

impl LlamaServerBackend {
    /// Spawn `llama-server` with the given model file.
    /// Returns `live: false` (hard-fail path) if binary or model are missing —
    /// the caller (`run_llm_loop`) decides whether to exit or continue in mock.
    pub fn new(model_path: &str, ctx_size: usize) -> Self {
        let bin = pick_server_binary(model_path);

        let Some(bin) = bin else {
            log::error!(
                "llama-server binary not found — run ./scripts/build-llama-server.sh \
                 (or ./scripts/build-bonsai-server.sh for the 1-bit Bonsai fork)."
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

        // Log free VRAM so OOM failures are immediately diagnosable.
        if let Ok(out) = std::process::Command::new("nvidia-smi")
            .args([
                "--query-gpu=memory.free,memory.total",
                "--format=csv,noheader,nounits",
            ])
            .output()
        {
            let s = String::from_utf8_lossy(&out.stdout);
            let parts: Vec<&str> = s.trim().splitn(2, ", ").collect();
            if let (Some(free), Some(total)) = (parts.first(), parts.get(1)) {
                log::info!("VRAM: {} MB free / {} MB total", free.trim(), total.trim());
            }
        }

        log::info!(
            "Spawning llama-server ({}) on port {} with model {}",
            bin,
            port,
            model_path
        );

        // Redirect server stderr to a log file so crashes are diagnosable.
        let stderr_log = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open("llama-server.log")
            .map(std::process::Stdio::from)
            .unwrap_or(std::process::Stdio::null());

        let child = std::process::Command::new(bin)
            .args([
                "--model",
                model_path,
                "--host",
                "127.0.0.1",
                "--port",
                &port.to_string(),
                "--ctx-size",
                &ctx_size.to_string(),
                "--n-gpu-layers",
                "99",
                "--flash-attn",
                "on",             // ~30% faster on CUDA; auto-detected if unsupported
                "--cache-type-k", // KV cache quantization: less VRAM, faster
                "q8_0",
                "--cache-type-v",
                "q8_0",
                "--log-disable", // reduce noise; we log our own status
            ])
            .stdout(std::process::Stdio::null())
            .stderr(stderr_log)
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
            .map(|r| {
                if r.status() != 200 {
                    return false;
                }
                // Some llama-server builds return HTTP 200 while still loading the model
                // (body: {"status":"loading model"}).  Only treat as live when body has "ok".
                r.into_string()
                    .map(|body| body.contains("\"ok\""))
                    .unwrap_or(false)
            })
            .unwrap_or(false);
        if !live {
            log::warn!(
                "LlamaServerBackend::connect: server at {} not ready (not ok)",
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

    /// Poll /health until the server is ready (up to ~120 s).
    /// llama-server can take a while to load a large model into VRAM.
    /// Bails out immediately if the child process exits (crashed).
    fn wait_for_ready(&mut self) {
        let url = format!("{}/health", self.base_url);
        for attempt in 0..240 {
            // Check if the child crashed before the health endpoint came up.
            if let Some(ref mut child) = self.child {
                match child.try_wait() {
                    Ok(Some(status)) => {
                        // Read the tail of llama-server.log for the actual error.
                        let detail = std::fs::read_to_string("llama-server.log")
                            .ok()
                            .map(|s| {
                                // Last non-empty line is usually the most useful.
                                s.lines()
                                    .rfind(|l| !l.trim().is_empty())
                                    .unwrap_or("")
                                    .trim()
                                    .to_string()
                            })
                            .filter(|s| !s.is_empty());
                        if let Some(msg) = detail {
                            log::error!("llama-server exited early ({}): {}", status, msg);
                        } else {
                            log::error!(
                                "llama-server exited early ({}) — see llama-server.log for details.",
                                status
                            );
                        }
                        self.child = None;
                        return;
                    }
                    Ok(None) => {} // still running — continue polling
                    Err(_) => {}
                }
            }
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
        log::error!("LLM server did not become ready within 120s — falling back to mock.");
    }
}

impl LlamaServerBackend {
    /// Kill the running server immediately (both owned child and any leaked process on the
    /// fixed port). Used before a model switch or context reset so `new()` always spawns
    /// fresh rather than hitting the "already healthy" reuse path.
    pub fn shutdown(&mut self) {
        if let Some(mut child) = self.child.take() {
            #[cfg(unix)]
            {
                let pid = child.id() as i32;
                unsafe { libc::kill(pid, libc::SIGKILL) };
            }
            #[cfg(not(unix))]
            let _ = child.kill();
            let _ = child.wait();
        }
        // Also kill any detached server that was reused without owning the child PID.
        // Try both builds — either could be holding the port.
        for &bin in SERVER_BINARY_CANDIDATES {
            if std::path::Path::new(bin).exists() || which_in_path(bin) {
                kill_leaked_servers(bin);
            }
        }
        self.live = false;
        log::info!("LLM server stopped.");
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
    fn infer(&mut self, system: &str, user: &str, sampling: &SamplingParams) -> Result<LlmOutput> {
        if !self.live {
            return mock_response(user, sampling.heat);
        }

        let url = format!("{}/v1/chat/completions", self.base_url);
        let heat_f = (sampling.heat as f64).clamp(0.0, 1.0);
        // Temperature: use explicit override when set, otherwise follow heat formula.
        let temperature = sampling
            .temp_override
            .map(|t| t as f64)
            .unwrap_or_else(|| 0.1 + heat_f * 1.6);
        // At high heat (and when not pinned), widen top_p for more creative choices.
        let top_p = if sampling.temp_override.is_some() {
            sampling.top_p as f64
        } else {
            (sampling.top_p as f64 + heat_f * (1.0 - sampling.top_p as f64) * 0.6).clamp(0.0, 1.0)
        };

        // json_object mode keeps the server honest about emitting valid JSON.
        // max_tokens: full-reset responses (all voices + FX + LFO) can exceed 1200 tokens
        // and truncate mid-JSON.  2400 gives headroom for the largest possible response.
        let body = serde_json::json!({
            "model": "local",
            "messages": [
                { "role": "system",  "content": system },
                { "role": "user",    "content": user   }
            ],
            "temperature": temperature,
            "top_k": sampling.top_k,
            "top_p": top_p,
            "min_p": sampling.min_p as f64,
            "repeat_penalty": sampling.repeat_penalty as f64,
            "frequency_penalty": sampling.frequency_penalty as f64,
            "seed": sampling.seed,
            "max_tokens": 2400,
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

        // Extract actions from "save_project" and "settings" keys.
        let mut actions = Vec::new();
        if let Some(obj) = param_update.as_object_mut() {
            if obj
                .remove("save_project")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
            {
                actions.push(LlmAction::SaveProject);
            }
            if let Some(s) = obj.get("settings").and_then(|v| v.as_object()).cloned() {
                if let Some(h) = s.get("heat").and_then(|v| v.as_f64()) {
                    actions.push(LlmAction::SetHeat((h as f32).clamp(0.0, 1.0)));
                }
                if let Some(p) = s.get("persona").and_then(|v| v.as_str()) {
                    actions.push(LlmAction::SetPersona(p.to_string()));
                }
                if let Some(m) = s.get("conversation_mode").and_then(|v| v.as_str()) {
                    actions.push(LlmAction::SetConversationMode(m.to_string()));
                }
                if let Some(j) = s.get("jam_bars").and_then(|v| v.as_f64()) {
                    actions.push(LlmAction::SetJamBars((j as f32).max(0.0)));
                }
            }
            // Don't pass "settings" to apply_llm_update - remove it
            obj.remove("settings");
        }

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
            before_state: None, // set by run_llm_loop after apply_llm_update
            actions,
        })
    }
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

/// Best-effort JSON repair for truncated or structurally confused LLM output.
/// 1. Try parsing as-is.
/// 2. Close unclosed brackets and retry.
/// 3. Sanitize the resulting structure (lift misplaced keys, remove nested fx loops).
fn which_in_path(name: &str) -> bool {
    std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).any(|dir| dir.join(name).exists()))
        .unwrap_or(false)
}

// ─── LLM thread loop ──────────────────────────────────────────────────────────

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
        });
        run_mock_loop(state, input_rx, output_tx);
        return;
    }

    let model_path = state.read().llm.model_path.clone();
    let ctx_size = state.read().llm.context_max;
    let mut backend = LlamaServerBackend::new(&model_path, ctx_size);

    if !backend.is_live() {
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
    });

    while let Ok(input) = input_rx.recv() {
        match &input {
            LlmInput::SwitchModel(new_path) => {
                let new_path = new_path.clone();
                log::info!("LLM: switching model -> {}", new_path);
                // Kill the running server BEFORE calling new() so the health-check
                // reuse path in new() doesn't silently keep the old model loaded.
                backend.shutdown();
                state.write().llm.model_path = new_path.clone();
                state.write().llm.context_used = 0;
                let ctx_size = state.read().llm.context_max;
                backend = LlamaServerBackend::new(&new_path, ctx_size);
                {
                    let mut s = state.write();
                    s.llm.is_mock = !backend.is_live();
                    s.llm.llm_initializing = false;
                }
                if backend.is_live() {
                    crate::state::save_model_setting(&new_path);
                }
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
                    before_state: None,
                    actions: vec![],
                });
                continue;
            }
            LlmInput::ResetContext => {
                let model_path = state.read().llm.model_path.clone();
                log::info!("LLM: resetting context (restart with same model)");
                backend.shutdown();
                state.write().llm.context_used = 0;
                let ctx_size = state.read().llm.context_max;
                backend = LlamaServerBackend::new(&model_path, ctx_size);
                {
                    let mut s = state.write();
                    s.llm.is_mock = !backend.is_live();
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
                });
                continue;
            }
            LlmInput::Infer { .. } => {}
        }

        let LlmInput::Infer {
            ref prompt,
            one_shot,
        } = input
        else {
            continue;
        };

        if one_shot {
            log::info!("YOU -> {}", prompt);
        } else {
            log::debug!("YOU (jam) -> {}", prompt);
        }

        let system = build_system_prompt(&state.read().clone());
        {
            let mut s = state.write();
            s.llm.is_inferring = true;
            s.llm.last_prompt = prompt.clone();
        }

        let sampling = {
            let s = state.read();
            SamplingParams {
                heat: s.llm.heat,
                temp_override: s.llm.llm_temp_override,
                top_k: s.llm.top_k,
                top_p: s.llm.top_p,
                min_p: s.llm.min_p,
                repeat_penalty: s.llm.repeat_penalty,
                frequency_penalty: s.llm.frequency_penalty,
                seed: s.llm.seed,
            }
        };
        let enable_thinking = state.read().llm.enable_thinking;
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
        let result = backend.infer(&system, &think_prompt, &sampling);
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
                    let next = apply_llm_update(current, update);
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
                            "LLM: context {:.0}% full — auto-compact: restarting server",
                            pct * 100.0
                        );
                        let model_path = state.read().llm.model_path.clone();
                        let ctx_size = state.read().llm.context_max;
                        backend.shutdown();
                        state.write().llm.context_used = 0;
                        backend = LlamaServerBackend::new(&model_path, ctx_size);
                        state.write().llm.is_mock = !backend.is_live();
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
                        });
                    }
                }

                if let Some(ref update) = output.param_update {
                    let comment = update
                        .get("_comment")
                        .and_then(|v| v.as_str())
                        .unwrap_or(&output.text);
                    let persona = state.read().llm.persona_name.clone();
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
            });
        }
    }

    log::info!("LLM thread exiting");
}

pub mod tts;
pub use tts::{TtsParams, speak_coqui, speak_fx};
