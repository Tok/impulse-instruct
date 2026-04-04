// ─── llm/mod.rs ──────────────────────────────────────────────────────────────
// LLM inference thread.
// Loads a GGUF model and runs continuous inference to control synth params.
// Communicates with UI via crossbeam channels.

pub mod instructions;
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
///   (which handles Qwen3, Llama 3.1, etc.) then $PATH
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
        let bin = pick_server_binary(model_path);

        let Some(bin) = bin else {
            log::error!(
                "llama-server binary not found — run ./build-bonsai-server.sh (Bonsai) \
                 or ./build-llama-server.sh (all other models)."
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
                "8192",
                "--n-gpu-layers",
                "99",
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
                        log::error!(
                            "llama-server exited early ({}). \
                             The model may be unsupported by this server build, \
                             or it ran out of VRAM. Check VRAM usage and try a \
                             smaller model.",
                            status
                        );
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
    fn infer(&mut self, system: &str, user: &str, heat: f32) -> Result<LlmOutput> {
        if !self.live {
            return mock_response(user, heat);
        }

        let url = format!("{}/v1/chat/completions", self.base_url);
        // heat 0.0 → temp 0.1 (near-deterministic), heat 1.0 → temp 1.2 (creative)
        let temperature = 0.1_f64 + (heat as f64).clamp(0.0, 1.0) * 1.1;

        // json_object mode keeps the server honest about emitting valid JSON.
        // max_tokens: compact index-format patterns are small (~10 tok/voice), but the
        // model may still emit FX + LFO + AN1X simultaneously. 1200 gives real headroom.
        let body = serde_json::json!({
            "model": "bonsai",
            "messages": [
                { "role": "system",  "content": system },
                { "role": "user",    "content": user   }
            ],
            "temperature": temperature,
            "max_tokens": 1200,
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
/// back to the top level, strip `"fx"` keys nested inside `"fx"`, and convert
/// LFO dot-notation objects (`"lfo": {"lfo[0].enabled": true, …}`) to the expected
/// array format (`"lfo": [{"enabled": true, …}, …]`).
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

    // Fix LFO dot-notation: model sometimes emits
    //   "lfo": {"lfo[0].enabled": true, "lfo[0].rate": 0.1, "lfo[1].target": "BassCutoff"}
    // instead of the expected array format. Convert to:
    //   "lfo": [{"enabled": true, "rate": 0.1}, {"target": "BassCutoff"}]
    if let Some(lfo_val) = obj.get("lfo")
        && let Some(lfo_obj) = lfo_val.as_object()
    {
        // Check if any key matches "lfo[N].field" pattern
        let has_dot_notation = lfo_obj
            .keys()
            .any(|k| k.starts_with("lfo[") && k.contains("]."));
        if has_dot_notation {
            let mut slots: [serde_json::Map<String, serde_json::Value>; 4] = Default::default();
            for (key, val) in lfo_obj {
                // Parse "lfo[N].field" → slot index N, field name
                if let Some(rest) = key.strip_prefix("lfo[")
                    && let Some(bracket) = rest.find("].")
                    && let Ok(idx) = rest[..bracket].parse::<usize>()
                    && idx < 4
                {
                    let field = &rest[bracket + 2..];
                    slots[idx].insert(field.to_string(), val.clone());
                }
            }
            let lfo_array: serde_json::Value = serde_json::Value::Array(
                slots.into_iter().map(serde_json::Value::Object).collect(),
            );
            obj.insert("lfo".to_string(), lfo_array);
        }
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
        });
        run_mock_loop(state, input_rx, output_tx);
        return;
    }

    let model_path = state.read().llm.model_path.clone();
    let mut backend = LlamaServerBackend::new(&model_path);

    if !backend.is_live() {
        {
            let mut s = state.write();
            s.llm.is_mock = false;
            s.llm.llm_initializing = false;
        }
        log::error!(
            "LLM server unavailable and --mock not passed. \
             Build the server: ./build-bonsai-server.sh \
             Download a model: ./download-models.sh \
             Then restart. Pass --mock to run without a model."
        );
        std::thread::sleep(std::time::Duration::from_secs(1));
        std::process::exit(1);
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
                backend = LlamaServerBackend::new(&new_path);
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
                });
                continue;
            }
            LlmInput::ResetContext => {
                let model_path = state.read().llm.model_path.clone();
                log::info!("LLM: resetting context (restart with same model)");
                backend.shutdown();
                state.write().llm.context_used = 0;
                backend = LlamaServerBackend::new(&model_path);
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

        let heat = state.read().llm.heat;
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
        let result = backend.infer(&system, &think_prompt, heat);
        let elapsed = t0.elapsed().as_secs_f32();

        match result {
            Ok(output) => {
                let tps = output.tokens_per_sec;
                let ptok = output.prompt_tokens;
                let ctok = output.completion_tokens;
                let ctx = output.context_used;
                let tthink = output.thinking.as_ref().map(|t| t.len() / 4).unwrap_or(0);

                if let Some(ref update) = output.param_update {
                    let current = state.read().clone();
                    let next = apply_llm_update(current, update);
                    *state.write() = next;
                }

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
                        backend.shutdown();
                        state.write().llm.context_used = 0;
                        backend = LlamaServerBackend::new(&model_path);
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
                        });
                    }
                }

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

                    let (
                        tts_on,
                        mode,
                        tts_pitch,
                        tts_speed,
                        tts_amplitude,
                        voice_char,
                        randomise,
                        rev_mix,
                        bitcrush,
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
                            s.llm.tts_reverb_mix,
                            s.llm.tts_bitcrush,
                        )
                    };
                    let tts_mode = matches!(mode, ConversationMode::Mc | ConversationMode::Dj);
                    if tts_on && tts_mode {
                        let tts_text = output.mc_line.as_deref().unwrap_or(comment);
                        log::info!("[TTS] {}", tts_text);
                        speak_fx(
                            tts_text,
                            &mode,
                            tts_pitch,
                            tts_speed,
                            tts_amplitude,
                            &voice_char,
                            randomise,
                            rev_mix,
                            bitcrush,
                            &tts_tx,
                        );
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
            });
        }
    }

    log::info!("LLM thread exiting");
}

pub mod tts;
pub use tts::speak_fx;
