// ─── config.rs ────────────────────────────────────────────────────────────────
// User-editable config loaded from `config.json` at the project root.
// Embedded defaults compile in so the binary works without the file.

use serde::Deserialize;
use std::sync::OnceLock;

static CONFIG: OnceLock<Config> = OnceLock::new();

const DEFAULT_JSON: &str = include_str!("../config.json");

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    /// Prompts sent automatically on startup once the LLM is ready.
    /// A random one is picked each launch.
    #[serde(default)]
    pub startup_prompts: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self { startup_prompts: Vec::new() }
    }
}

pub fn get() -> &'static Config {
    CONFIG.get_or_init(|| {
        let json = std::fs::read_to_string("config.json")
            .unwrap_or_else(|_| DEFAULT_JSON.to_string());
        match serde_json::from_str::<Config>(&json) {
            Ok(c) => c,
            Err(e) => {
                log::warn!("config.json parse error: {e} — using embedded defaults");
                serde_json::from_str(DEFAULT_JSON).expect("embedded config.json is valid")
            }
        }
    })
}

/// Pick a random startup prompt, or return `None` if the list is empty.
pub fn random_startup_prompt() -> Option<&'static str> {
    let prompts = &get().startup_prompts;
    if prompts.is_empty() {
        return None;
    }
    // Simple LCG seeded from current time — avoids pulling in `rand`.
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as usize)
        .unwrap_or(0);
    Some(prompts[seed % prompts.len()].as_str())
}
