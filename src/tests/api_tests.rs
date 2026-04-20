#[cfg(test)]
#[allow(clippy::module_inception)]
mod api_tests {
    use crate::api::PromptRequest;

    #[test]
    fn prompt_request_defaults_one_shot_true() {
        // Backwards-compat: clients that don't send `one_shot` must keep
        // getting single-shot behaviour, same as before the jam-via-API
        // plumbing landed.
        let r: PromptRequest = serde_json::from_str(r#"{"prompt":"hi"}"#).unwrap();
        assert!(r.one_shot, "missing one_shot must default to true");
        assert!(r.agent.is_none());
    }

    #[test]
    fn prompt_request_explicit_false_preserves_jam_mode() {
        let r: PromptRequest =
            serde_json::from_str(r#"{"prompt":"jam","one_shot":false}"#).unwrap();
        assert!(!r.one_shot);
    }

    #[test]
    fn prompt_request_explicit_true_still_works() {
        let r: PromptRequest = serde_json::from_str(r#"{"prompt":"one","one_shot":true}"#).unwrap();
        assert!(r.one_shot);
    }

    #[test]
    fn prompt_request_with_agent_field() {
        let r: PromptRequest = serde_json::from_str(r#"{"prompt":"hi","agent":"BASS"}"#).unwrap();
        assert_eq!(r.agent.as_deref(), Some("BASS"));
        assert!(r.one_shot);
    }
}
