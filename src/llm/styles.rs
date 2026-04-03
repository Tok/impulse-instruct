// ─── llm/styles.rs ────────────────────────────────────────────────────────────
// Style catalog for genre-aware prompting.
// Loaded once from `styles.json` at the project root (with embedded fallback).
// Each entry provides a descriptive creative brief that is injected into the
// system prompt when a style is active — Bonsai reads it and decides what to do.

use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

static STYLE_CATALOG: OnceLock<StyleCatalog> = OnceLock::new();

const DEFAULT_JSON: &str = include_str!("../../styles.json");

// ─── Types ────────────────────────────────────────────────────────────────────

/// Typical 16-step drum grid for a style — used as a seed suggestion in the prompt.
/// All fields are optional; absent voices are left at the model's discretion.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct SeedPatterns {
    /// 16-element 0/1 array for 808-style kick.
    #[serde(default)]
    pub kick: Vec<u8>,
    /// 16-element 0/1 array for 808-style snare.
    #[serde(default)]
    pub snare: Vec<u8>,
    /// 16-element 0/1 array for closed hi-hat.
    #[serde(default)]
    pub hihat: Vec<u8>,
}

impl SeedPatterns {
    pub fn is_empty(&self) -> bool {
        self.kick.is_empty() && self.snare.is_empty() && self.hihat.is_empty()
    }

    /// Render as compact lines for prompt injection.
    pub fn to_prompt_lines(&self) -> String {
        let fmt = |v: &[u8]| -> String {
            let s: Vec<&str> = v.iter().map(|x| if *x == 1 { "1" } else { "0" }).collect();
            format!("[{}]", s.join(","))
        };
        let mut lines = Vec::new();
        if !self.kick.is_empty() {
            lines.push(format!("kick:  {}", fmt(&self.kick)));
        }
        if !self.snare.is_empty() {
            lines.push(format!("snare: {}", fmt(&self.snare)));
        }
        if !self.hihat.is_empty() {
            lines.push(format!("hihat: {}", fmt(&self.hihat)));
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
}
