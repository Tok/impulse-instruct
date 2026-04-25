// ─── state/persona_preset.rs ──────────────────────────────────────────────────
// Persona library — save / load named agent configurations.
//
// A `PersonaPreset` captures the user-curated subset of `LlmAgentState`
// that defines a reusable persona: persona name, role, conversation
// mode, instructions, system-prompt override, and a few sampling knobs.
// Patterns / scope / model-path are deliberately omitted — those are
// session-context things that don't transfer between projects.
//
// On-disk layout: `~/.impulse_instruct/personas/<slug>.json`.  Slug is
// the persona name, lower-cased, non-ASCII / whitespace replaced with
// underscores, so a name like `Bass MC` becomes `bass_mc.json`.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use super::{AgentRole, ConversationMode, LlmAgentState};

/// Subset of `LlmAgentState` that a persona preset persists.  A new
/// agent created from a preset gets a fresh id / scope / model_path /
/// pattern state — the preset only fills in the personality knobs.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct PersonaPreset {
    pub name: String,
    #[serde(default)]
    pub role: AgentRole,
    #[serde(default)]
    pub conversation_mode: ConversationMode,
    #[serde(default)]
    pub user_instructions: String,
    #[serde(default)]
    pub system_prompt_override: String,
    #[serde(default = "default_temperature")]
    pub temperature: f32,
    #[serde(default)]
    pub enable_thinking: bool,
}

fn default_temperature() -> f32 {
    0.9
}

impl PersonaPreset {
    /// Capture an agent's current personality knobs as a preset.
    /// Pattern / scope / runtime fields are deliberately dropped.
    pub fn from_agent(agent: &LlmAgentState) -> Self {
        Self {
            name: agent.persona_name.clone(),
            role: agent.role,
            conversation_mode: agent.conversation_mode.clone(),
            user_instructions: agent.user_instructions.clone(),
            system_prompt_override: agent.system_prompt_override.clone(),
            temperature: agent.temperature,
            enable_thinking: agent.enable_thinking,
        }
    }

    /// Apply a preset onto an existing agent, mutating only the
    /// personality knobs.  Scope, model_path, patterns, and runtime
    /// counters are left untouched so loading a preset is non-
    /// destructive against in-flight session state.
    pub fn apply_to(&self, agent: &mut LlmAgentState) {
        agent.persona_name = self.name.clone();
        agent.role = self.role;
        agent.conversation_mode = self.conversation_mode.clone();
        agent.user_instructions = self.user_instructions.clone();
        agent.system_prompt_override = self.system_prompt_override.clone();
        agent.temperature = self.temperature.clamp(0.0, 2.0);
        agent.enable_thinking = self.enable_thinking;
    }
}

/// Map a persona name to a filesystem-safe slug.  Pure helper so the
/// path resolution can be unit-tested without touching disk.
pub fn slugify(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut last_under = false;
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
            last_under = false;
        } else if !last_under && !out.is_empty() {
            out.push('_');
            last_under = true;
        }
    }
    while out.ends_with('_') {
        out.pop();
    }
    if out.is_empty() {
        out.push_str("persona");
    }
    out
}

/// Default on-disk personas directory: `~/.impulse_instruct/personas/`.
/// Returns `None` when the home dir can't be resolved (CI containers,
/// chrooted jails) — callers fall back to a no-op persona library.
pub fn personas_dir() -> Option<PathBuf> {
    dirs_home_dir().map(|h| h.join(".impulse_instruct").join("personas"))
}

/// Tiny `dirs::home_dir` shim — avoids pulling in the `dirs` crate
/// just for this one call.  Reads `$HOME` (Unix) / `%USERPROFILE%`
/// (Windows) and returns `None` if neither is set.
fn dirs_home_dir() -> Option<PathBuf> {
    if let Ok(h) = std::env::var("HOME")
        && !h.is_empty()
    {
        return Some(PathBuf::from(h));
    }
    if let Ok(h) = std::env::var("USERPROFILE")
        && !h.is_empty()
    {
        return Some(PathBuf::from(h));
    }
    None
}

/// Save a persona preset to `<personas_dir>/<slug>.json`.  Creates
/// the personas directory if missing.  Returns the path written, or
/// an error string for the UI to surface.
pub fn save_preset(preset: &PersonaPreset) -> Result<PathBuf, String> {
    let dir = personas_dir().ok_or_else(|| "no home dir resolvable".to_string())?;
    save_preset_to_dir(&dir, preset)
}

/// Save a persona preset under an explicit directory.  Used by the
/// regular `save_preset` path with `personas_dir()`, and exposed
/// directly so unit tests can drive a temp dir without mutating
/// process-wide environment variables.
pub fn save_preset_to_dir(
    dir: &std::path::Path,
    preset: &PersonaPreset,
) -> Result<PathBuf, String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("mkdir {}: {e}", dir.display()))?;
    let path = dir.join(format!("{}.json", slugify(&preset.name)));
    let json = serde_json::to_string_pretty(preset).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| format!("write {}: {e}", path.display()))?;
    Ok(path)
}

/// Load a persona preset from a `.json` file.  Errors include path,
/// IO, and serde failures so the UI can surface them verbatim.
pub fn load_preset_from_path(path: &std::path::Path) -> Result<PersonaPreset, String> {
    let raw = std::fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    serde_json::from_str::<PersonaPreset>(&raw)
        .map_err(|e| format!("parse {}: {e}", path.display()))
}

/// List every `.json` file in the personas directory, sorted by
/// filename.  Empty when the directory is missing or contains no
/// `.json` files.
pub fn list_presets() -> Vec<PathBuf> {
    let Some(dir) = personas_dir() else {
        return Vec::new();
    };
    list_presets_in(&dir)
}

/// Variant for unit tests that want to enumerate a specific
/// directory without touching `personas_dir()` / `HOME`.
pub fn list_presets_in(dir: &std::path::Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.is_file()
                && p.extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(|e| e.eq_ignore_ascii_case("json"))
        })
        .collect();
    out.sort();
    out
}
