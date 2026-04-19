// ─── llm/vram.rs ─────────────────────────────────────────────────────────────
// VRAM budget: model profiles, agent presets, and budget estimation.
// All logic is pure functions over const data — no I/O.
// GPU detection is handled by `sysinfo.rs`; this module only does math.

use crate::state::AgentRole;

// ─── Model profiles ──────────────────────────────────────────────────────────

/// Static model metadata — maps filename patterns to VRAM estimates.
pub struct ModelProfile {
    /// Lowercase substring matched against the model filename.
    pub pattern: &'static str,
    /// Human-readable label.
    pub label: &'static str,
    /// Estimated VRAM in MB when fully GPU-offloaded (`--n-gpu-layers 99`).
    pub vram_mb: u64,
}

/// Known model profiles.  Order matters: first match wins in `estimate_vram`.
pub const MODEL_PROFILES: &[ModelProfile] = &[
    ModelProfile {
        pattern: "gemma-4-e4b",
        label: "Gemma 4 E4B",
        vram_mb: 6000,
    },
    ModelProfile {
        pattern: "gemma-4-e2b",
        label: "Gemma 4 E2B",
        vram_mb: 4000,
    },
    // Gemma 4 26B-A4B is MoE (4B active, 26B total).  VRAM listed below
    // is the disk-size-derived ceiling for the recommended quants on
    // unsloth/gemma-4-26B-A4B-it-GGUF.  Pattern order matters — list the
    // most-specific quant patterns first.
    ModelProfile {
        pattern: "gemma-4-26b-a4b-it-ud-iq2",
        label: "Gemma 4 26B-A4B IQ2",
        vram_mb: 11000,
    },
    ModelProfile {
        pattern: "gemma-4-26b-a4b-it-ud-q2",
        label: "Gemma 4 26B-A4B Q2",
        vram_mb: 11500,
    },
    ModelProfile {
        pattern: "gemma-4-26b-a4b-it-ud-iq3",
        label: "Gemma 4 26B-A4B IQ3",
        vram_mb: 12500,
    },
    ModelProfile {
        pattern: "gemma-4-26b-a4b-it-ud-q3",
        label: "Gemma 4 26B-A4B Q3",
        vram_mb: 13500,
    },
    ModelProfile {
        pattern: "gemma-4-26b-a4b-it-ud-iq4",
        label: "Gemma 4 26B-A4B IQ4",
        vram_mb: 14500,
    },
    ModelProfile {
        pattern: "gemma-4-26b-a4b",
        label: "Gemma 4 26B-A4B",
        vram_mb: 17500,
    },
    ModelProfile {
        pattern: "gemma-4-31b",
        label: "Gemma 4 31B",
        vram_mb: 20000,
    },
    ModelProfile {
        pattern: "gemma",
        label: "Gemma (other)",
        vram_mb: 6000,
    },
    ModelProfile {
        pattern: "deepseek-r1-distill-qwen-14b",
        label: "DeepSeek-R1 14B",
        vram_mb: 11000,
    },
    ModelProfile {
        pattern: "deepseek-r1-distill-qwen-7b",
        label: "DeepSeek-R1 7B",
        vram_mb: 7000,
    },
    ModelProfile {
        pattern: "deepseek-r1",
        label: "DeepSeek-R1",
        vram_mb: 7000,
    },
    ModelProfile {
        pattern: "qwen3-14b",
        label: "Qwen3 14B",
        vram_mb: 11000,
    },
    ModelProfile {
        pattern: "qwen3-8b",
        label: "Qwen3 8B",
        vram_mb: 7000,
    },
    ModelProfile {
        pattern: "qwen3",
        label: "Qwen3",
        vram_mb: 7000,
    },
];

/// Estimate VRAM usage (MB) for a model file path.
/// Matches against `MODEL_PROFILES`; falls back to a file-size heuristic
/// (VRAM ≈ 1.3 × GGUF file size).
pub fn estimate_vram(model_path: &str) -> u64 {
    let lower = model_path.to_ascii_lowercase();
    for p in MODEL_PROFILES {
        if lower.contains(p.pattern) {
            return p.vram_mb;
        }
    }
    // Fallback: file size × 1.3 (accounts for KV cache overhead).
    if let Ok(meta) = std::fs::metadata(model_path) {
        let file_mb = meta.len() / (1024 * 1024);
        return (file_mb as f64 * 1.3) as u64;
    }
    4000 // conservative default
}

/// Return the human-readable label for a model, or its filename stem.
pub fn model_label(model_path: &str) -> String {
    let lower = model_path.to_ascii_lowercase();
    for p in MODEL_PROFILES {
        if lower.contains(p.pattern) {
            return p.label.to_string();
        }
    }
    std::path::Path::new(model_path)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| model_path.to_string())
}

// ─── Budget estimation ──────────────────────────────────────────────────────

/// Estimate total VRAM (MB) for all running agents plus an optional candidate.
/// Agents sharing the same model share one server, so each unique model is
/// counted only once.
pub fn estimate_total_vram(
    agents: &[crate::state::LlmAgentState],
    global_model: &str,
    candidate: Option<&str>,
) -> u64 {
    let mut unique = std::collections::HashSet::new();
    for a in agents {
        let model = a.model_path.as_deref().unwrap_or(global_model);
        if !model.is_empty() {
            unique.insert(model.to_string());
        }
    }
    if let Some(c) = candidate {
        if !c.is_empty() {
            unique.insert(c.to_string());
        }
    } else if !global_model.is_empty() {
        // Candidate uses global model
        unique.insert(global_model.to_string());
    }
    unique.iter().map(|m| estimate_vram(m)).sum()
}

/// Returns true if spawning one more agent (with the given model) would exceed
/// `vram_total_mb`.  Returns false (allow) when `vram_total_mb` is 0 (CPU mode).
pub fn would_exceed_vram(
    agents: &[crate::state::LlmAgentState],
    global_model: &str,
    candidate_model: Option<&str>,
    vram_total_mb: u64,
) -> bool {
    if vram_total_mb == 0 {
        return false; // CPU mode — no VRAM limit
    }
    let needed = estimate_total_vram(agents, global_model, candidate_model);
    needed > vram_total_mb
}

// ─── Agent presets ───────────────────────────────────────────────────────────

/// One agent in a preset configuration.
pub struct PresetAgent {
    /// Substring to match in `available_models` (lowercase).
    pub model_pattern: &'static str,
    /// Display name / persona.
    pub persona: &'static str,
    /// Scope strings (empty = controls everything).
    pub scope: &'static [&'static str],
    pub role: AgentRole,
}

/// A suggested multi-agent configuration.
pub struct AgentPreset {
    pub name: &'static str,
    pub description: &'static str,
    pub agents: &'static [PresetAgent],
    /// Sum of VRAM for all agents (unique models only — shared models count once).
    pub total_vram_mb: u64,
}

pub const PRESETS: &[AgentPreset] = &[
    AgentPreset {
        name: "Solo",
        description: "1x Gemma 4 E4B — single full-control agent",
        agents: &[PresetAgent {
            model_pattern: "gemma",
            persona: "PULSE",
            scope: &[],
            role: AgentRole::Producer,
        }],
        total_vram_mb: 6000,
    },
    AgentPreset {
        name: "Duo",
        description: "2x Gemma — bass specialist + drums/FX",
        agents: &[
            PresetAgent {
                model_pattern: "gemma",
                persona: "ACID",
                scope: &["bass", "sequencer"],
                role: AgentRole::Specialist,
            },
            PresetAgent {
                model_pattern: "gemma",
                persona: "RHYTHM",
                scope: &["kit_a", "kit_b", "fx"],
                role: AgentRole::Specialist,
            },
        ],
        // Same model → shared server, single VRAM cost.
        total_vram_mb: 6000,
    },
    AgentPreset {
        name: "Swarm",
        description: "4x Gemma — lead + 3 scoped helpers (bass / drums / fx)",
        agents: &[
            PresetAgent {
                model_pattern: "gemma",
                persona: "PULSE",
                scope: &[],
                role: AgentRole::Producer,
            },
            PresetAgent {
                model_pattern: "gemma",
                persona: "ACID",
                scope: &["bass"],
                role: AgentRole::Specialist,
            },
            PresetAgent {
                model_pattern: "gemma",
                persona: "DRUMS",
                scope: &["kit_a", "kit_b"],
                role: AgentRole::Specialist,
            },
            PresetAgent {
                model_pattern: "gemma",
                persona: "FX",
                scope: &["fx"],
                role: AgentRole::Specialist,
            },
        ],
        // Same model → shared server, single VRAM cost.
        total_vram_mb: 6000,
    },
    AgentPreset {
        name: "Crew",
        description: "5x Gemma — conductor + 4 scoped specialists",
        agents: &[
            PresetAgent {
                model_pattern: "gemma",
                persona: "CONDUCTOR",
                scope: &["sequencer"],
                role: AgentRole::Producer,
            },
            PresetAgent {
                model_pattern: "gemma",
                persona: "BASS",
                scope: &["bass"],
                role: AgentRole::Specialist,
            },
            PresetAgent {
                model_pattern: "gemma",
                persona: "DRUMS",
                scope: &["kit_a", "kit_b"],
                role: AgentRole::Specialist,
            },
            PresetAgent {
                model_pattern: "gemma",
                persona: "KEYS",
                scope: &["hoover", "an1x"],
                role: AgentRole::Specialist,
            },
            PresetAgent {
                model_pattern: "gemma",
                persona: "FX",
                scope: &["fx"],
                role: AgentRole::Specialist,
            },
        ],
        total_vram_mb: 6000,
    },
    AgentPreset {
        name: "Voices",
        description: "5x Gemma — one agent per voice",
        agents: &[
            PresetAgent {
                model_pattern: "gemma",
                persona: "PULSE",
                scope: &[],
                role: AgentRole::Producer,
            },
            PresetAgent {
                model_pattern: "gemma",
                persona: "ACID",
                scope: &["bass", "sequencer"],
                role: AgentRole::Specialist,
            },
            PresetAgent {
                model_pattern: "gemma",
                persona: "DRUMS",
                scope: &["kit_a", "kit_b", "amen"],
                role: AgentRole::Specialist,
            },
            PresetAgent {
                model_pattern: "gemma",
                persona: "SYNTH",
                scope: &["hoover", "an1x", "noise"],
                role: AgentRole::Specialist,
            },
            PresetAgent {
                model_pattern: "gemma",
                persona: "FX",
                scope: &["fx", "lfo"],
                role: AgentRole::Specialist,
            },
        ],
        total_vram_mb: 6000,
    },
];

/// Return a style-flavored display name for an agent preset.  The underlying
/// `AgentPreset::name` (`"Crew"`, `"Solo"`, …) remains the canonical id used
/// by the API and tests; this helper just re-labels it for the UI.  Only the
/// `Crew` preset is renamed today — the plan calls out `Crew → Band / Posse
/// / Squad / Ensemble per style`.
pub fn styled_preset_name(preset_name: &'static str, style_id: Option<&str>) -> &'static str {
    if preset_name != "Crew" {
        return preset_name;
    }
    match style_id {
        // Posse — MC/rave lineage, sound-system heritage.
        Some("jungle" | "drum_and_bass" | "uk_garage" | "dubstep" | "breakcore") => "Posse",
        // Squad — hard, aggressive, tight-knit kit.
        Some("gabber" | "early_rave" | "darksynth" | "electro") => "Squad",
        // Band — retro, song-oriented.
        Some("synthwave" | "vaporwave" | "lo_fi_hip_hop") => "Band",
        // Ensemble — contemplative / orchestral.
        Some(
            "ambient_house" | "ambient_techno" | "dark_ambient" | "space_ambient" | "meditation"
            | "baroque_bach" | "idm",
        ) => "Ensemble",
        _ => "Crew",
    }
}

/// Result of checking a preset against available resources.
pub struct PresetStatus {
    pub preset: &'static AgentPreset,
    /// Whether all required models are present on disk.
    pub models_available: bool,
    /// Whether total VRAM fits in the budget.
    pub fits_vram: bool,
    /// Distinct model patterns that are missing from `available_models`.
    pub missing_models: Vec<&'static str>,
}

/// Check each preset against available VRAM and downloaded models.
/// Returns statuses for ALL presets (caller decides how to render unavailable ones).
pub fn check_presets(vram_total_mb: u64, available_models: &[String]) -> Vec<PresetStatus> {
    let lower_models: Vec<String> = available_models
        .iter()
        .map(|p| p.to_ascii_lowercase())
        .collect();

    PRESETS
        .iter()
        .map(|preset| {
            // Collect distinct model patterns needed by this preset.
            let mut patterns: Vec<&str> = preset.agents.iter().map(|a| a.model_pattern).collect();
            patterns.sort();
            patterns.dedup();

            let missing: Vec<&'static str> = patterns
                .iter()
                .filter(|pat| !lower_models.iter().any(|m| m.contains(*pat)))
                .copied()
                .collect();

            PresetStatus {
                preset,
                models_available: missing.is_empty(),
                fits_vram: vram_total_mb == 0 || preset.total_vram_mb <= vram_total_mb,
                missing_models: missing,
            }
        })
        .collect()
}

/// Find the model file in `available_models` matching `pattern` (case-insensitive).
pub fn find_model(pattern: &str, available_models: &[String]) -> Option<String> {
    available_models
        .iter()
        .find(|m| m.to_ascii_lowercase().contains(pattern))
        .cloned()
}

/// When adding `candidate_model` as a new agent would blow the VRAM
/// budget, pick the *heaviest* model from `available_models` that still
/// fits — i.e. the nearest-quality downgrade.  Returns `None` if no
/// available model fits the remaining budget (e.g. even the smallest
/// model is too big for the current agent roster).
///
/// The chosen model is strictly smaller than the candidate (so callers
/// never re-try the exact same bind) and honours the same
/// `would_exceed_vram` rule the spawn path uses, so callers can
/// confidently swap the candidate for the fallback without re-checking.
pub fn pick_fallback_model(
    agents: &[crate::state::LlmAgentState],
    global_model: &str,
    candidate_model: &str,
    available_models: &[String],
    vram_total_mb: u64,
) -> Option<String> {
    if vram_total_mb == 0 {
        return None; // CPU mode — nothing to downgrade for
    }
    let candidate_vram = estimate_vram(candidate_model);
    let mut ranked: Vec<(&String, u64)> = available_models
        .iter()
        .map(|m| (m, estimate_vram(m)))
        .filter(|(m, v)| *v < candidate_vram && m.as_str() != candidate_model)
        .collect();
    // Heaviest first — nearest-quality downgrade.
    ranked.sort_by(|a, b| b.1.cmp(&a.1));
    for (path, _) in ranked {
        if !would_exceed_vram(agents, global_model, Some(path), vram_total_mb) {
            return Some(path.clone());
        }
    }
    None
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn estimate_gemma() {
        assert_eq!(estimate_vram("models/gemma-4-e4b-it-Q4_K_M.gguf"), 6000);
    }

    #[test]
    fn estimate_gemma_26b_quants() {
        // Pattern order in MODEL_PROFILES means the more-specific quant
        // patterns must beat the generic "gemma-4-26b-a4b" fallback.
        assert_eq!(
            estimate_vram("models/gemma-4-26B-A4B-it-UD-IQ2_XXS.gguf"),
            11000
        );
        assert_eq!(
            estimate_vram("models/gemma-4-26B-A4B-it-UD-Q3_K_M.gguf"),
            13500
        );
        assert_eq!(
            estimate_vram("models/gemma-4-26B-A4B-it-UD-IQ4_XS.gguf"),
            14500
        );
        // Generic fallback for an unknown 26B-A4B quant.
        assert_eq!(
            estimate_vram("models/gemma-4-26B-A4B-it-UD-Q4_K_XL.gguf"),
            17500
        );
    }

    #[test]
    fn estimate_deepseek_14b() {
        assert_eq!(
            estimate_vram("models/DeepSeek-R1-Distill-Qwen-14B-Q4_K_M.gguf"),
            11000
        );
    }

    #[test]
    fn estimate_deepseek_7b() {
        assert_eq!(
            estimate_vram("models/deepseek-r1-distill-qwen-7b-Q4.gguf"),
            7000
        );
    }

    #[test]
    fn estimate_unknown_fallback() {
        // No file on disk → conservative 4000 MB default.
        assert_eq!(estimate_vram("/nonexistent/mystery.gguf"), 4000);
    }

    #[test]
    fn model_label_known() {
        assert_eq!(
            model_label("models/gemma-4-e4b-it-Q4_K_M.gguf"),
            "Gemma 4 E4B"
        );
    }

    #[test]
    fn model_label_unknown() {
        let label = model_label("models/mystery-7b-Q4.gguf");
        assert_eq!(label, "mystery-7b-Q4");
    }

    /// Find a preset status by name.
    fn find_status<'a>(statuses: &'a [PresetStatus], name: &str) -> &'a PresetStatus {
        statuses
            .iter()
            .find(|s| s.preset.name == name)
            .unwrap_or_else(|| panic!("preset '{}' not found", name))
    }

    #[test]
    fn presets_all_available_6gb() {
        let models = vec!["models/gemma-4-e4b-it-Q4_K_M.gguf".to_string()];
        let statuses = check_presets(6000, &models);
        for name in ["Solo", "Duo", "Swarm", "Crew", "Voices"] {
            let s = find_status(&statuses, name);
            assert!(s.fits_vram, "{name} should fit 6 GB");
            assert!(s.models_available, "{name} should find Gemma");
        }
    }

    #[test]
    fn presets_no_gemma() {
        let models: Vec<String> = vec![];
        let statuses = check_presets(12000, &models);
        for name in ["Solo", "Duo", "Swarm", "Crew", "Voices"] {
            let s = find_status(&statuses, name);
            assert!(!s.models_available, "{name} should require Gemma");
            assert_eq!(s.missing_models, vec!["gemma"]);
        }
    }

    #[test]
    fn presets_small_vram() {
        let models = vec!["models/gemma-4-e4b.gguf".to_string()];
        let statuses = check_presets(4000, &models);
        // All presets need 6 GB → none fit on a 4 GB budget.
        for name in ["Solo", "Duo", "Swarm", "Crew", "Voices"] {
            assert!(
                !find_status(&statuses, name).fits_vram,
                "{name} should not fit 4 GB"
            );
        }
    }

    #[test]
    fn presets_zero_vram_is_cpu_mode() {
        // vram_total_mb == 0 means no GPU detected → all presets "fit" (CPU mode).
        let models = vec!["models/gemma.gguf".to_string()];
        let statuses = check_presets(0, &models);
        assert!(statuses.iter().all(|s| s.fits_vram));
    }

    #[test]
    fn band_has_five_agents() {
        let band = PRESETS.iter().find(|p| p.name == "Crew").unwrap();
        assert_eq!(band.agents.len(), 5);
        assert_eq!(band.agents[0].persona, "CONDUCTOR");
    }

    #[test]
    fn voices_has_five_agents() {
        let voices = PRESETS.iter().find(|p| p.name == "Voices").unwrap();
        assert_eq!(voices.agents.len(), 5);
    }

    #[test]
    fn find_model_case_insensitive() {
        let models = vec!["models/Gemma-4-E4B-it-Q4_K_M.gguf".to_string()];
        assert!(find_model("gemma", &models).is_some());
    }

    #[test]
    fn find_model_missing() {
        let models = vec!["models/gemma.gguf".to_string()];
        assert!(find_model("qwen3", &models).is_none());
    }

    #[test]
    fn styled_preset_name_renames_crew() {
        assert_eq!(styled_preset_name("Crew", Some("jungle")), "Posse");
        assert_eq!(styled_preset_name("Crew", Some("drum_and_bass")), "Posse");
        assert_eq!(styled_preset_name("Crew", Some("gabber")), "Squad");
        assert_eq!(styled_preset_name("Crew", Some("synthwave")), "Band");
        assert_eq!(styled_preset_name("Crew", Some("dark_ambient")), "Ensemble");
    }

    #[test]
    fn styled_preset_name_crew_default_when_unmapped() {
        assert_eq!(styled_preset_name("Crew", None), "Crew");
        assert_eq!(styled_preset_name("Crew", Some("acid_techno")), "Crew");
        assert_eq!(styled_preset_name("Crew", Some("__unknown__")), "Crew");
    }

    #[test]
    fn styled_preset_name_leaves_other_presets_alone() {
        for preset in ["Solo", "Duo", "Swarm", "Voices"] {
            assert_eq!(styled_preset_name(preset, Some("jungle")), preset);
            assert_eq!(styled_preset_name(preset, None), preset);
        }
    }
}
