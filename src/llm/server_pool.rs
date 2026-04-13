// ─── llm/server_pool.rs ──────────────────────────────────────────────────────
// LlamaServerBackend (single llama-server process) and LlamaServerPool
// (manages N backends, one per model, ref-counted).

use anyhow::Result;

use super::json_repair::{repair_json, split_thinking};
use super::mock::mock_response;
use super::{LlmBackend, LlmOutput, SamplingParams, extract_llm_actions};

// ── Constants ────────────────────────────────────────────────────────────────

/// Candidate paths for the llama-server binary.
/// Checked in order — PrismML fork first (required for Bonsai 1-bit),
/// then official llama.cpp build (required for Gemma 4 / newer architectures),
/// then $PATH fallback.
const SERVER_BINARY_CANDIDATES: &[&str] = &[
    ".llama-build/bin/llama-server", // PrismML fork — build-bonsai-server.sh
    ".llama-official-build/bin/llama-server", // official llama.cpp — build-llama-server.sh
    "llama-server",                  // $PATH
];

/// Default base port for the server pool.
pub const LLAMA_BASE_PORT: u16 = 8766;
/// Maximum number of concurrent llama-server instances.
const MAX_SERVERS: usize = 8;

/// Timeout for a single inference call.  8B models on CPU can take 60–90 s.
const INFER_TIMEOUT_SECS: u64 = 180;

// ── Helpers ──────────────────────────────────────────────────────────────────

fn which_in_path(name: &str) -> bool {
    std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).any(|dir| dir.join(name).exists()))
        .unwrap_or(false)
}

/// Pick the right llama-server binary (Bonsai → PrismML fork, else official → fork → $PATH).
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

// ── LlamaServerBackend ──────────────────────────────────────────────────────

pub struct LlamaServerBackend {
    child: Option<std::process::Child>,
    base_url: String,
    live: bool,
}

impl LlamaServerBackend {
    /// Spawn `llama-server` with the given model file on the specified port.
    /// Returns `live: false` (hard-fail path) if binary or model are missing —
    /// the caller decides whether to exit or continue in mock.
    pub fn new(model_path: &str, ctx_size: usize, port: u16) -> Self {
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

    /// Kill the running server immediately (both owned child and any leaked process).
    /// Used before a model switch or context reset so `new()` always spawns fresh
    /// rather than hitting the "already healthy" reuse path.
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

// ── Backend inference impl ───────────────────────────────────────────────────

impl LlmBackend for LlamaServerBackend {
    fn infer(&mut self, system: &str, user: &str, sampling: &SamplingParams) -> Result<LlmOutput> {
        if !self.live {
            return mock_response(user, sampling.heat);
        }

        let url = format!("{}/v1/chat/completions", self.base_url);
        let heat_f = (sampling.heat as f64).clamp(0.0, 1.0);
        // Heat drives *sampling chaos* — at 1.0 the model should be nearly
        // unhinged, not just 3% wider nucleus. Scale every knob we can:
        //   • temperature ×(1 + heat·0.8)  → 0.9 → 1.62 at heat=1
        //   • top_p toward 1.0 fully       → 0.95 → 1.00 at heat=1
        //   • min_p floor toward 0         → 0.05 → 0.005 at heat=1
        //   • frequency_penalty up to +0.4 → discourages token repetition
        let temperature = ((sampling.temperature as f64) * (1.0 + heat_f * 0.8)).clamp(0.0, 2.0);
        let top_p =
            (sampling.top_p as f64 + heat_f * (1.0 - sampling.top_p as f64)).clamp(0.0, 1.0);
        let min_p = ((sampling.min_p as f64) * (1.0 - heat_f * 0.9)).clamp(0.0, 1.0);
        let frequency_penalty = (sampling.frequency_penalty as f64 + heat_f * 0.4).clamp(0.0, 2.0);

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
            "min_p": min_p,
            "repeat_penalty": sampling.repeat_penalty as f64,
            "frequency_penalty": frequency_penalty,
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
        let actions = if let Some(obj) = param_update.as_object_mut() {
            extract_llm_actions(obj)
        } else {
            Vec::new()
        };

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
            agent_id: None, // set by run_llm_loop
        })
    }
}

// ── Server pool ──────────────────────────────────────────────────────────────

struct ServerInstance {
    backend: LlamaServerBackend,
    model_path: String,
    port: u16,
    ref_count: usize,
}

/// Manages N llama-server processes, each on a unique port, each loading one
/// GGUF model.  Agents using the same model share a server (ref-counted).
pub struct LlamaServerPool {
    servers: Vec<ServerInstance>,
    base_port: u16,
    ctx_size: usize,
}

impl LlamaServerPool {
    pub fn new(base_port: u16, ctx_size: usize) -> Self {
        Self {
            servers: Vec::new(),
            base_port,
            ctx_size,
        }
    }

    /// Return the port of a live server for `model_path`.  If one already
    /// exists, bumps its ref_count.  Otherwise spawns a new server on the next
    /// free port (if under MAX_SERVERS).
    pub fn acquire(&mut self, model_path: &str) -> Result<u16> {
        self.acquire_with_vram(model_path, 0)
    }

    /// Like `acquire`, but rejects new model loads that would exceed `vram_total_mb`.
    /// Pass 0 to skip the VRAM check (CPU mode or unknown).
    pub fn acquire_with_vram(&mut self, model_path: &str, vram_total_mb: u64) -> Result<u16> {
        // Reuse existing server for the same model.
        if let Some(inst) = self.servers.iter_mut().find(|s| s.model_path == model_path) {
            inst.ref_count += 1;
            log::info!(
                "Pool: reusing server on port {} for {} (ref_count={})",
                inst.port,
                model_path,
                inst.ref_count,
            );
            return Ok(inst.port);
        }
        if self.servers.len() >= MAX_SERVERS {
            anyhow::bail!(
                "Server pool full ({} / {}) — cannot load another model",
                self.servers.len(),
                MAX_SERVERS,
            );
        }
        // VRAM budget check before spawning a new server.
        if vram_total_mb > 0 {
            let loaded: u64 = self
                .servers
                .iter()
                .map(|s| crate::llm::vram::estimate_vram(&s.model_path))
                .sum();
            let candidate = crate::llm::vram::estimate_vram(model_path);
            if loaded + candidate > vram_total_mb {
                anyhow::bail!(
                    "VRAM budget exceeded: loaded {}MB + {}MB for {} > {}MB total",
                    loaded,
                    candidate,
                    model_path,
                    vram_total_mb,
                );
            }
        }
        let port = self.next_free_port();
        log::info!(
            "Pool: spawning new server on port {} for {}",
            port,
            model_path
        );
        let backend = LlamaServerBackend::new(model_path, self.ctx_size, port);
        self.servers.push(ServerInstance {
            backend,
            model_path: model_path.to_string(),
            port,
            ref_count: 1,
        });
        Ok(port)
    }

    /// Decrement the ref_count for `model_path`.  Shuts down the server when
    /// ref_count reaches zero.
    pub fn release(&mut self, model_path: &str) {
        if let Some(idx) = self.servers.iter().position(|s| s.model_path == model_path) {
            let inst = &mut self.servers[idx];
            inst.ref_count = inst.ref_count.saturating_sub(1);
            log::info!(
                "Pool: released {} on port {} (ref_count={})",
                model_path,
                inst.port,
                inst.ref_count,
            );
            if inst.ref_count == 0 {
                self.servers[idx].backend.shutdown();
                self.servers.remove(idx);
            }
        }
    }

    /// Dispatch inference to the server on `port`.
    pub fn infer(
        &mut self,
        port: u16,
        system: &str,
        user: &str,
        sampling: &SamplingParams,
    ) -> Result<LlmOutput> {
        let inst = self
            .servers
            .iter_mut()
            .find(|s| s.port == port)
            .ok_or_else(|| anyhow::anyhow!("No server on port {}", port))?;
        inst.backend.infer(system, user, sampling)
    }

    /// True if at least one server is live.
    pub fn is_any_live(&self) -> bool {
        self.servers.iter().any(|s| s.backend.is_live())
    }

    /// Shut down a specific server by model path (e.g. for context reset).
    pub fn shutdown_model(&mut self, model_path: &str) {
        if let Some(idx) = self.servers.iter().position(|s| s.model_path == model_path) {
            self.servers[idx].backend.shutdown();
            self.servers.remove(idx);
        }
    }

    /// Shut down all servers (app exit).
    pub fn shutdown_all(&mut self) {
        for inst in &mut self.servers {
            inst.backend.shutdown();
        }
        self.servers.clear();
    }

    fn next_free_port(&self) -> u16 {
        // Find the lowest port in range that isn't already in use.
        for offset in 0..MAX_SERVERS as u16 {
            let candidate = self.base_port + offset;
            if !self.servers.iter().any(|s| s.port == candidate) {
                return candidate;
            }
        }
        self.base_port + self.servers.len() as u16
    }
}

impl Drop for LlamaServerPool {
    fn drop(&mut self) {
        self.shutdown_all();
    }
}

#[cfg(test)]
impl LlamaServerPool {
    /// Insert a fake (non-live) server entry for testing pool logic
    /// without spawning real llama-server processes.
    pub fn insert_test_server(&mut self, model_path: &str, port: u16) {
        self.servers.push(ServerInstance {
            backend: LlamaServerBackend {
                child: None,
                base_url: format!("http://127.0.0.1:{}", port),
                live: false,
            },
            model_path: model_path.to_string(),
            port,
            ref_count: 1,
        });
    }

    pub fn server_count(&self) -> usize {
        self.servers.len()
    }

    pub fn ref_count_for(&self, model_path: &str) -> Option<usize> {
        self.servers
            .iter()
            .find(|s| s.model_path == model_path)
            .map(|s| s.ref_count)
    }

    pub fn port_for(&self, model_path: &str) -> Option<u16> {
        self.servers
            .iter()
            .find(|s| s.model_path == model_path)
            .map(|s| s.port)
    }
}
