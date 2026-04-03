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

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Style {
    pub id: String,
    pub name: String,
    /// Words/phrases that trigger this style when typed in a prompt.
    pub keywords: Vec<String>,
    /// Informational BPM range shown in the UI.
    pub bpm_range: String,
    /// The text injected into the system prompt when this style is active.
    /// Written as a creative brief for Bonsai — no hardcoded param values.
    pub description: String,
}

pub struct StyleCatalog(Vec<Style>);

impl StyleCatalog {
    pub fn get() -> &'static StyleCatalog {
        STYLE_CATALOG.get_or_init(|| {
            let json = std::fs::read_to_string("styles.json")
                .unwrap_or_else(|_| DEFAULT_JSON.to_string());
            let styles: Vec<Style> = serde_json::from_str(&json)
                .unwrap_or_else(|e| {
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

    /// Find the best-matching style for a free-text prompt.
    /// Uses substring matching on keywords; returns the highest-scoring match.
    pub fn find_by_prompt(&self, prompt: &str) -> Option<&Style> {
        let lower = prompt.to_lowercase();
        self.0.iter()
            .filter_map(|s| {
                let score: usize = s.keywords.iter()
                    .filter(|kw| lower.contains(kw.as_str()))
                    .map(|kw| kw.split_whitespace().count())
                    .sum();
                if score > 0 { Some((score, s)) } else { None }
            })
            .max_by_key(|(score, _)| *score)
            .map(|(_, s)| s)
    }
}
