// ─── llm/styles.rs ────────────────────────────────────────────────────────────
// Style catalog for genre-aware prompting.
// Loaded once from `styles.json` at the project root (with embedded fallback).
// Each entry provides a descriptive creative brief that is injected into the
// system prompt when a style is active — the LLM reads it and decides what to do.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::OnceLock;

static STYLE_CATALOG: OnceLock<StyleCatalog> = OnceLock::new();

const DEFAULT_JSON: &str = include_str!("../../styles.json");

// ─── Types ────────────────────────────────────────────────────────────────────

/// Drum + bass seed pattern for a style — used as a seed suggestion in the prompt.
/// Arrays can be any length (typically 16 or 32 steps). The LLM adapts to the active step count.
/// All fields are optional; absent voices are left at the model's discretion.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct SeedPatterns {
    /// 0/1 array for 808-style kick.
    #[serde(default)]
    pub kick: Vec<u8>,
    /// 0/1 array for 808-style snare.
    #[serde(default)]
    pub snare: Vec<u8>,
    /// 0/1 array for closed hi-hat.
    #[serde(default)]
    pub hihat: Vec<u8>,
    /// 0/1 array for bass sequencer gate (which steps trigger bass).
    #[serde(default)]
    pub bass_steps: Vec<u8>,
    /// MIDI note array for bass sequencer (0 = not used on that step).
    #[serde(default)]
    pub bass_notes: Vec<u8>,
}

const NOTE_NAMES: [&str; 12] = [
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];

fn midi_name(n: u8) -> String {
    let oct = (n as i32 / 12) - 1;
    format!("{}{}", NOTE_NAMES[(n as usize) % 12], oct)
}

impl SeedPatterns {
    pub fn is_empty(&self) -> bool {
        self.kick.is_empty()
            && self.snare.is_empty()
            && self.hihat.is_empty()
            && self.bass_steps.is_empty()
    }

    /// Render as compact lines for prompt injection.
    pub fn to_prompt_lines(&self) -> String {
        let fmt_bits = |v: &[u8]| -> String {
            let s: Vec<&str> = v.iter().map(|x| if *x == 1 { "1" } else { "0" }).collect();
            format!("[{}]", s.join(","))
        };
        let mut lines = Vec::new();
        if !self.kick.is_empty() {
            lines.push(format!("kick:       {}", fmt_bits(&self.kick)));
        }
        if !self.snare.is_empty() {
            lines.push(format!("snare:      {}", fmt_bits(&self.snare)));
        }
        if !self.hihat.is_empty() {
            lines.push(format!("hihat:      {}", fmt_bits(&self.hihat)));
        }
        if !self.bass_steps.is_empty() {
            lines.push(format!("bass_steps: {}", fmt_bits(&self.bass_steps)));
        }
        if !self.bass_notes.is_empty() {
            // Show both MIDI ints and note names side by side
            let nums: Vec<String> = self.bass_notes.iter().map(|n| n.to_string()).collect();
            let names: Vec<String> = self
                .bass_notes
                .iter()
                .zip(self.bass_steps.iter().chain(std::iter::repeat(&1u8)))
                .map(|(&n, &s)| {
                    if s == 1 {
                        midi_name(n)
                    } else {
                        "..".to_string()
                    }
                })
                .collect();
            lines.push(format!(
                "bass_notes: [{}]  ({})",
                nums.join(","),
                names.join(" ")
            ));
        }
        lines.join("\n")
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Style {
    pub id: String,
    pub name: String,
    /// Words/phrases that trigger this style when typed in a prompt.
    pub keywords: Vec<String>,
    /// Informational BPM range shown in the UI.
    pub bpm_range: String,
    /// Short creative brief (~50 tokens). Used with smaller/faster models or
    /// tight context windows. Falls back to `description` if empty.
    #[serde(default)]
    pub brief: String,
    /// Full creative brief (~150 tokens) — the original prose description.
    /// Used with larger models that benefit from richer context.
    pub description: String,
    /// Canonical 16-step starter patterns for this genre (optional).
    /// Injected into the style section so the model has a concrete reference.
    #[serde(default)]
    pub seed_patterns: SeedPatterns,
    /// Suggested tonic for this genre (MIDI 0–11, e.g. 0=C, 9=A). None = no suggestion.
    #[serde(default)]
    pub suggested_root: Option<u8>,
    /// Suggested scale name for this genre (e.g. "minor", "dorian", "chromatic").
    #[serde(default)]
    pub suggested_scale: Option<String>,
    /// Immediate parameter reset applied when style is selected (before LLM fires).
    /// Same format as LLM JSON output — applied via `apply_llm_update`.
    /// Establishes a style baseline so the LLM doesn't inherit a previous style's settings.
    #[serde(default)]
    pub baseline_params: Option<serde_json::Value>,
    /// Example MC/DJ lines for this style. Fed to MC-mode agents as examples
    /// of what to say. Not spoken directly — the model uses them as reference.
    #[serde(default)]
    pub mc_lines: Vec<String>,
    /// Topic words/themes for singer/rapper agents. Gives creative direction
    /// about what to sing or rap about in this style.
    #[serde(default)]
    pub themes: Vec<String>,
    /// Suggested rack module set for this style.  Names match
    /// `state::rack_scope::parse_module_kind` (e.g. "bass", "808", "909",
    /// "amen", "reverb", "delay", "drive").  When the user picks this
    /// style and the rack doesn't already contain a module of each listed
    /// kind, the missing ones are added (auto-wired to master via
    /// `wire_default_cables` afterwards).  Existing modules are kept so
    /// switching styles isn't destructive.
    #[serde(default)]
    pub rack_modules: Vec<String>,
    /// Optional per-lane dynamism overrides for the Phase 2 jam scheduler
    /// (`lane_scheduler::effective_dynamism`).  Keys match either the exact
    /// lane label (`"bass1"`, `"kit_a"`, `"fx"`) or a group label (`"bass"`
    /// covers every bass voice).  Values are `0.0..=1.0` — higher → picked
    /// more often during jam cycles.  Absent entries fall back to the
    /// baked-in defaults in `lane_scheduler::baseline_dynamism`.
    #[serde(default)]
    pub lane_dynamism: HashMap<String, f32>,
    /// Optional genre-flavoured jam prompt.  When set, overrides the
    /// generic `"continue jamming, evolve the pattern"` message fired
    /// every jam cycle so the agent gets style-specific direction (e.g.
    /// "wobble the reese", "push the hats into 16ths", "let the
    /// reverb tail breathe").  `None` = use the generic prompt.
    #[serde(default)]
    pub jam_prompt_template: Option<String>,
}

pub struct StyleCatalog(Vec<Style>);

impl StyleCatalog {
    pub fn get() -> &'static StyleCatalog {
        STYLE_CATALOG.get_or_init(|| {
            let json =
                std::fs::read_to_string("styles.json").unwrap_or_else(|_| DEFAULT_JSON.to_string());
            let styles: Vec<Style> = serde_json::from_str(&json).unwrap_or_else(|e| {
                log::warn!("styles.json parse error: {e} — using embedded defaults");
                serde_json::from_str(DEFAULT_JSON).unwrap_or_default()
            });
            log::debug!("Style catalog loaded: {} styles", styles.len());
            StyleCatalog(styles)
        })
    }

    pub fn styles(&self) -> &[Style] {
        &self.0
    }

    pub fn find_by_id(&self, id: &str) -> Option<&Style> {
        self.0.iter().find(|s| s.id == id)
    }

    /// Resolve a free-text style identifier (typically from an LLM
    /// `SetStyle` action) to the canonical id stored in
    /// `LlmState.active_style`.  Lookup order:
    /// 1. Exact id match (`"drum_and_bass"`).
    /// 2. Case-insensitive id match (`"Drum_And_Bass"`).
    /// 3. Case-insensitive display-name match (`"Drum and Bass"`).
    ///
    /// Returns `None` when the catalog has no style under any of those
    /// keys — caller leaves `active_style` untouched.  Pure helper so
    /// the resolver logic is unit-testable.
    pub fn resolve_style_id(&self, query: &str) -> Option<String> {
        // Fast path: exact id match.
        if let Some(s) = self.find_by_id(query) {
            return Some(s.id.clone());
        }
        // Fallback: case-insensitive against id OR display name.
        let lo = query.to_lowercase();
        self.0
            .iter()
            .find(|s| s.id.to_lowercase() == lo || s.name.to_lowercase() == lo)
            .map(|s| s.id.clone())
    }
}
