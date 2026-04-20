// ─── tests/vram_tests.rs ──────────────────────────────────────────────────────
// Tests for VRAM budget estimation and guard logic.

#[cfg(test)]
mod vram_budget_tests {
    use crate::llm::vram::{estimate_total_vram, estimate_vram, would_exceed_vram};
    use crate::state::LlmAgentState;

    fn agent(model: Option<&str>) -> LlmAgentState {
        let mut a = LlmAgentState::new_default(0);
        a.model_path = model.map(|s| s.to_string());
        a
    }

    #[test]
    fn estimate_vram_known_model() {
        assert_eq!(estimate_vram("models/gemma-4-e4b-Q4.gguf"), 6000);
        assert_eq!(estimate_vram("models/qwen3-8b-Q4.gguf"), 7000);
    }

    #[test]
    fn estimate_total_single_agent_uses_global() {
        let agents = vec![agent(None)];
        let total = estimate_total_vram(&agents, "models/gemma-4-e4b.gguf", None);
        assert_eq!(total, 6000);
    }

    #[test]
    fn estimate_total_shared_model_counted_once() {
        let agents = vec![agent(None), agent(None), agent(None)];
        let total = estimate_total_vram(&agents, "models/gemma-4-e4b.gguf", None);
        // All three share the same model → only counted once
        assert_eq!(total, 6000);
    }

    #[test]
    fn estimate_total_mixed_models() {
        let agents = vec![
            agent(None),                         // global = gemma (6000)
            agent(Some("models/qwen3-8b.gguf")), // qwen3 8B (7000)
        ];
        let total = estimate_total_vram(&agents, "models/gemma-4-e4b.gguf", None);
        assert_eq!(total, 6000 + 7000);
    }

    #[test]
    fn estimate_total_with_candidate() {
        let agents = vec![agent(None)]; // gemma
        let total = estimate_total_vram(
            &agents,
            "models/gemma-4-e4b.gguf",
            Some("models/qwen3-8b.gguf"),
        );
        assert_eq!(total, 6000 + 7000);
    }

    #[test]
    fn candidate_already_loaded_not_double_counted() {
        let agents = vec![agent(Some("models/qwen3-8b.gguf"))];
        let total = estimate_total_vram(
            &agents,
            "models/gemma-4-e4b.gguf",
            Some("models/qwen3-8b.gguf"),
        );
        // Candidate is same as existing agent's model → only qwen3 counted
        // (no agent uses global gemma, so it's not included)
        assert_eq!(total, 7000);
    }

    #[test]
    fn would_exceed_cpu_mode_always_false() {
        let agents = vec![agent(None)];
        assert!(!would_exceed_vram(
            &agents,
            "models/gemma-4-e4b.gguf",
            None,
            0
        ));
    }

    #[test]
    fn would_exceed_within_budget() {
        let agents = vec![agent(None)];
        assert!(!would_exceed_vram(
            &agents,
            "models/gemma-4-e4b.gguf",
            None,
            16000
        ));
    }

    #[test]
    fn would_exceed_over_budget() {
        let agents = vec![agent(None)]; // gemma = 6000
        // Adding qwen3 8B = 7000, total = 13000 > 12000
        assert!(would_exceed_vram(
            &agents,
            "models/gemma-4-e4b.gguf",
            Some("models/qwen3-8b.gguf"),
            12000
        ));
    }
}

#[cfg(test)]
mod pick_fallback_model_tests {
    use crate::llm::vram::pick_fallback_model;
    use crate::state::LlmAgentState;

    fn agent(model: Option<&str>) -> LlmAgentState {
        let mut a = LlmAgentState::new_default(0);
        a.model_path = model.map(|s| s.to_string());
        a
    }

    #[test]
    fn picks_heaviest_fitting_alternative() {
        // Global gemma (6000) already costs the budget.  Adding qwen3-14b
        // (11000) blows it; qwen3-8b (7000) still doesn't fit;
        // gemma-4-e2b (4000) does.
        let agents = vec![agent(None)];
        let available = vec![
            "models/qwen3-14b.gguf".to_string(),
            "models/qwen3-8b.gguf".to_string(),
            "models/gemma-4-e2b.gguf".to_string(),
        ];
        let fb = pick_fallback_model(
            &agents,
            "models/gemma-4-e4b.gguf",
            "models/qwen3-14b.gguf",
            &available,
            10_000,
        );
        assert_eq!(fb, Some("models/gemma-4-e2b.gguf".to_string()));
    }

    #[test]
    fn returns_none_when_nothing_fits() {
        // Roster already saturates the 5 GB budget; no available model
        // is lighter than the candidate's 6 GB estimate that also fits.
        let agents = vec![
            agent(Some("models/gemma-4-e4b.gguf")),
            agent(Some("models/qwen3-8b.gguf")),
        ];
        let available = vec!["models/qwen3-14b.gguf".to_string()];
        let fb = pick_fallback_model(
            &agents,
            "models/gemma-4-e4b.gguf",
            "models/qwen3-14b.gguf",
            &available,
            5_000,
        );
        assert_eq!(fb, None);
    }

    #[test]
    fn cpu_mode_returns_none() {
        // `vram_total_mb == 0` means CPU mode — there's no budget to
        // downgrade for, so the function bails early with `None`.
        let agents = vec![agent(None)];
        let available = vec!["models/gemma-4-e2b.gguf".to_string()];
        let fb = pick_fallback_model(
            &agents,
            "models/gemma-4-e4b.gguf",
            "models/qwen3-14b.gguf",
            &available,
            0,
        );
        assert_eq!(fb, None);
    }

    #[test]
    fn never_picks_same_or_heavier_model() {
        // Candidate = gemma-4-e4b (6000).  Even if the budget had room
        // for qwen3-8b (7000), the function must not suggest it as a
        // "fallback" since that's an upgrade, not a downgrade.
        let agents: Vec<LlmAgentState> = vec![];
        let available = vec![
            "models/gemma-4-e4b.gguf".to_string(),
            "models/qwen3-8b.gguf".to_string(),
        ];
        let fb = pick_fallback_model(
            &agents,
            "models/gemma-4-e4b.gguf",
            "models/gemma-4-e4b.gguf",
            &available,
            20_000,
        );
        assert_eq!(
            fb, None,
            "same-sized / upgrade models must not be suggested"
        );
    }
}
