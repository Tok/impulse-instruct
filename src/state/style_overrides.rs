// ─── state/style_overrides.rs ────────────────────────────────────────────────
// Per-style mc_lines / themes overrides.
//
// Styles ship with defaults in `styles.json`, but users / agents may want
// to tweak the MC lines or themes for a specific style without re-
// compiling the binary.  We keep the overrides on `AppState` (persisted
// with session / project data) and expose two tiny helpers that the
// prompt builder and TTS panel consult instead of reading the catalog
// directly.  When no override is set, we fall back to the baseline
// values from the `StyleCatalog`.

use serde::{Deserialize, Serialize};

/// Per-style mc_lines / themes overrides.  Each field is optional so
/// users can override just the mc_lines (keep baseline themes) or vice
/// versa — and an empty override cell round-trips to "clear" rather
/// than "fall back to baseline".
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct StyleOverride {
    /// Overridden MC lines.  `Some(vec![])` = explicitly empty list
    /// (silences MC/DJ prompts for the style).  `None` = use baseline.
    #[serde(default)]
    pub mc_lines: Option<Vec<String>>,
    /// Overridden vocal themes.  Same Some/None semantics as `mc_lines`.
    #[serde(default)]
    pub themes: Option<Vec<String>>,
}

impl StyleOverride {
    /// True when the override carries no information — purely decorative.
    /// Lets save paths strip empty entries so session.json stays tidy.
    pub fn is_empty(&self) -> bool {
        self.mc_lines.is_none() && self.themes.is_none()
    }
}

/// Resolve the effective mc_lines for `style_id` — user override first,
/// baseline from the style catalog as fallback.  Returns an empty vec
/// when the style id is unknown.
pub fn effective_mc_lines(state: &super::AppState, style_id: &str) -> Vec<String> {
    if let Some(ov) = state.style_overrides.get(style_id)
        && let Some(lines) = &ov.mc_lines
    {
        return lines.clone();
    }
    crate::llm::styles::StyleCatalog::get()
        .find_by_id(style_id)
        .map(|s| s.mc_lines.clone())
        .unwrap_or_default()
}

/// Mirror of `effective_mc_lines` for themes.
pub fn effective_themes(state: &super::AppState, style_id: &str) -> Vec<String> {
    if let Some(ov) = state.style_overrides.get(style_id)
        && let Some(themes) = &ov.themes
    {
        return themes.clone();
    }
    crate::llm::styles::StyleCatalog::get()
        .find_by_id(style_id)
        .map(|s| s.themes.clone())
        .unwrap_or_default()
}
