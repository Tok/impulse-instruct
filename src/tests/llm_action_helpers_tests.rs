// ─── tests/llm_action_helpers_tests.rs ────────────────────────────────────────
// Pure helpers behind the LlmAction dispatch in `ui/llm_drain.rs`.
// The dispatch itself touches `Arc<RwLock<AppState>>` + log buffers
// + history stacks (impure shell), so the testable kernel is the
// classification + queue mutation logic.

#[cfg(test)]
mod conversation_mode_parse {
    use crate::state::ConversationMode;

    #[test]
    fn known_modes_parse_case_insensitively() {
        assert_eq!(
            ConversationMode::from_str_lossy("off"),
            ConversationMode::Off
        );
        assert_eq!(
            ConversationMode::from_str_lossy("OFF"),
            ConversationMode::Off
        );
        assert_eq!(
            ConversationMode::from_str_lossy("Producer"),
            ConversationMode::Producer
        );
        assert_eq!(ConversationMode::from_str_lossy("DJ"), ConversationMode::Dj);
        assert_eq!(ConversationMode::from_str_lossy("mc"), ConversationMode::Mc);
    }

    #[test]
    fn whitespace_is_trimmed() {
        assert_eq!(
            ConversationMode::from_str_lossy("  mc  "),
            ConversationMode::Mc
        );
    }

    #[test]
    fn unknown_strings_fall_back_to_producer() {
        // Defensive: the LLM may emit garbage / future modes / typos.
        // The fallback shouldn't be silent — Producer is the safe
        // default that emits readable comments without persona drift.
        assert_eq!(
            ConversationMode::from_str_lossy("hype"),
            ConversationMode::Producer
        );
        assert_eq!(
            ConversationMode::from_str_lossy(""),
            ConversationMode::Producer
        );
        assert_eq!(
            ConversationMode::from_str_lossy("PRODUCER"),
            ConversationMode::Producer
        );
    }
}

#[cfg(test)]
mod broadcast_scope_matcher {
    use crate::state::{LlmAgentState, agent_matches_broadcast_scope};

    fn agent_with_scope(persona: &str, scope: &[&str]) -> LlmAgentState {
        let mut a = LlmAgentState::new_default(1);
        a.persona_name = persona.to_string();
        a.scope = scope.iter().map(|s| s.to_string()).collect();
        a
    }

    #[test]
    fn scoped_agent_matches_its_label() {
        let a = agent_with_scope("BASS", &["bass"]);
        assert!(agent_matches_broadcast_scope(&a, "bass"));
    }

    #[test]
    fn match_is_case_insensitive() {
        let a = agent_with_scope("BASS", &["bass"]);
        assert!(agent_matches_broadcast_scope(&a, "BASS"));
        assert!(agent_matches_broadcast_scope(&a, "Bass"));
    }

    #[test]
    fn agent_with_multi_label_scope_matches_any_member() {
        let a = agent_with_scope("KIT", &["kit_a", "kit_b"]);
        assert!(agent_matches_broadcast_scope(&a, "kit_a"));
        assert!(agent_matches_broadcast_scope(&a, "kit_b"));
        assert!(!agent_matches_broadcast_scope(&a, "amen"));
    }

    #[test]
    fn unscoped_agent_matches_persona_name() {
        // An agent with empty scope (admin / global) is still
        // reachable by a persona-name broadcast.
        let a = agent_with_scope("PULSE", &[]);
        assert!(agent_matches_broadcast_scope(&a, "PULSE"));
        assert!(agent_matches_broadcast_scope(&a, "pulse"));
    }

    #[test]
    fn unscoped_agent_does_not_match_random_label() {
        let a = agent_with_scope("PULSE", &[]);
        assert!(!agent_matches_broadcast_scope(&a, "bass"));
        assert!(!agent_matches_broadcast_scope(&a, "kit_a"));
    }

    #[test]
    fn scoped_agent_does_not_fall_through_to_persona_match() {
        // A scoped agent whose persona happens to equal the broadcast
        // label but whose scope DOESN'T contain it must NOT match.
        // The persona-name fallback is reserved for unscoped agents.
        let a = agent_with_scope("BASS", &["fx"]);
        assert!(!agent_matches_broadcast_scope(&a, "BASS"));
    }
}

#[cfg(test)]
mod hint_queue {
    use crate::state::{HINT_QUEUE_MAX, LlmAgentState, push_pending_hint};

    #[test]
    fn push_appends_to_pending_hints() {
        let mut a = LlmAgentState::new_default(1);
        push_pending_hint(&mut a, "go half-time".into());
        assert_eq!(a.pending_hints, vec!["go half-time".to_string()]);
    }

    #[test]
    fn queue_caps_at_max_and_drops_oldest() {
        let mut a = LlmAgentState::new_default(1);
        for i in 0..(HINT_QUEUE_MAX + 3) {
            push_pending_hint(&mut a, format!("hint-{i}"));
        }
        assert_eq!(a.pending_hints.len(), HINT_QUEUE_MAX);
        // Oldest 3 dropped → first surviving entry is "hint-3".
        assert_eq!(a.pending_hints[0], "hint-3");
        // Last entry is the most recent push.
        let last = a.pending_hints.last().unwrap();
        assert_eq!(last, &format!("hint-{}", HINT_QUEUE_MAX + 2));
    }

    #[test]
    fn duplicate_hints_are_kept_until_cap() {
        // Some agents may legitimately receive the same hint from two
        // sources (e.g. global + scoped broadcast).  push_pending_hint
        // does NOT dedupe; it just caps the depth.
        let mut a = LlmAgentState::new_default(1);
        push_pending_hint(&mut a, "warm".into());
        push_pending_hint(&mut a, "warm".into());
        assert_eq!(a.pending_hints.len(), 2);
    }
}
