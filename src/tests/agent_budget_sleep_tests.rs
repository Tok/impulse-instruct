// ─── tests/agent_budget_sleep_tests.rs ────────────────────────────────────────
// Per-agent token-budget bookkeeping + sleep-mode round-robin skip.
// The token writeback path lives inside the LLM worker thread (touches
// Arc<RwLock<AppState>> + channels), so the unit-testable surface is:
//   • `LlmAgentState` defaults the new fields cleanly
//   • Saturating arithmetic on the cumulative counters
//   • The "skip-sleeping-agents" filter that the heartbeat uses
//     (replicated as a pure helper here so a regression where sleep
//     gets ignored is caught without standing up the UI loop)

#[cfg(test)]
mod state_defaults {
    use crate::state::{AppState, LlmAgentState, LlmState};

    #[test]
    fn fresh_agent_has_zero_token_counters() {
        let a = LlmAgentState::new_default(1);
        assert_eq!(a.total_prompt_tokens, 0);
        assert_eq!(a.total_completion_tokens, 0);
        assert_eq!(a.completed_cycles, 0);
        assert!(!a.sleeping);
    }

    #[test]
    fn from_singleton_inherits_zero_token_counters() {
        let s = AppState::default();
        let a = LlmAgentState::from_singleton(7, &s.llm);
        assert_eq!(a.total_prompt_tokens, 0);
        assert_eq!(a.total_completion_tokens, 0);
        assert_eq!(a.completed_cycles, 0);
        assert!(!a.sleeping);
        // Sanity: from_singleton still copies non-budget fields from
        // LlmState, so the new fields didn't shadow anything.
        let llm: &LlmState = &s.llm;
        assert_eq!(a.persona_name, llm.persona_name);
    }

    #[test]
    fn sleeping_round_trips_through_serde() {
        // Token totals are #[serde(skip)] (transient) but `sleeping`
        // is persisted so a session reload remembers which
        // specialists were dormant.
        let mut a = LlmAgentState::new_default(1);
        a.sleeping = true;
        let json = serde_json::to_string(&a).unwrap();
        let a2: LlmAgentState = serde_json::from_str(&json).unwrap();
        assert!(a2.sleeping);
    }
}

#[cfg(test)]
mod budget_arithmetic {
    use crate::state::LlmAgentState;

    fn record_cycle(a: &mut LlmAgentState, prompt: u64, completion: u64) {
        a.total_prompt_tokens = a.total_prompt_tokens.saturating_add(prompt);
        a.total_completion_tokens = a.total_completion_tokens.saturating_add(completion);
        a.completed_cycles = a.completed_cycles.saturating_add(1);
    }

    #[test]
    fn cumulative_counts_sum_across_cycles() {
        let mut a = LlmAgentState::new_default(1);
        record_cycle(&mut a, 200, 50);
        record_cycle(&mut a, 180, 40);
        record_cycle(&mut a, 220, 60);
        assert_eq!(a.total_prompt_tokens, 600);
        assert_eq!(a.total_completion_tokens, 150);
        assert_eq!(a.completed_cycles, 3);
    }

    #[test]
    fn average_per_cycle_works_for_uneven_runs() {
        let mut a = LlmAgentState::new_default(1);
        record_cycle(&mut a, 100, 50);
        record_cycle(&mut a, 200, 100);
        let total = a.total_prompt_tokens + a.total_completion_tokens;
        let avg = total / a.completed_cycles as u64;
        assert_eq!(total, 450);
        assert_eq!(avg, 225);
    }

    #[test]
    fn saturating_add_clamps_at_u64_max() {
        // Defensive: a runaway model could burn through huge contexts;
        // saturating_add prevents overflow from corrupting counters.
        let mut a = LlmAgentState::new_default(1);
        a.total_prompt_tokens = u64::MAX - 10;
        record_cycle(&mut a, 100, 0);
        assert_eq!(a.total_prompt_tokens, u64::MAX);
    }
}

#[cfg(test)]
mod sleep_skip {
    use crate::state::{LlmAgentState, ModuleKind, RackModule, RackState};

    /// Pure replica of the heartbeat's "pickable agent" filter — agent
    /// is enabled in the rack AND not sleeping.  Lives here so a
    /// regression on the filter logic surfaces without UI plumbing.
    fn pickable<'a>(agents: &'a [LlmAgentState], rack: &'a RackState) -> Vec<u32> {
        agents
            .iter()
            .filter(|a| !a.sleeping && rack.modules.iter().any(|m| m.id == a.id && m.enabled))
            .map(|a| a.id)
            .collect()
    }

    fn agent_with(id: u32, sleeping: bool) -> LlmAgentState {
        let mut a = LlmAgentState::new_default(id);
        a.sleeping = sleeping;
        a
    }

    fn rack_with_modules(ids: &[u32], enabled: bool) -> RackState {
        let mut r = RackState::default();
        r.modules.clear();
        for &id in ids {
            let mut m = RackModule::new(id, ModuleKind::LlmAgent);
            m.enabled = enabled;
            r.modules.push(m);
        }
        r
    }

    #[test]
    fn awake_agents_are_picked() {
        let agents = vec![agent_with(1, false), agent_with(2, false)];
        let rack = rack_with_modules(&[1, 2], true);
        let picks = pickable(&agents, &rack);
        assert_eq!(picks, vec![1, 2]);
    }

    #[test]
    fn sleeping_agent_is_skipped() {
        let agents = vec![agent_with(1, true), agent_with(2, false)];
        let rack = rack_with_modules(&[1, 2], true);
        let picks = pickable(&agents, &rack);
        assert_eq!(picks, vec![2]);
    }

    #[test]
    fn all_sleeping_yields_empty_pick_list() {
        let agents = vec![agent_with(1, true), agent_with(2, true)];
        let rack = rack_with_modules(&[1, 2], true);
        let picks = pickable(&agents, &rack);
        assert!(picks.is_empty());
    }

    #[test]
    fn disabled_module_skipped_even_when_awake() {
        // Sleep is additive over the existing "rack-enabled" filter —
        // both gates must pass for the agent to be picked.
        let agents = vec![agent_with(1, false)];
        let rack = rack_with_modules(&[1], false);
        let picks = pickable(&agents, &rack);
        assert!(picks.is_empty());
    }
}
