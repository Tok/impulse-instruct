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
