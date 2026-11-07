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

/// API calls that set up or start a session must close the startup wizard
/// (Rack Setup) instead of leaving it over a rack that's already playing.
#[cfg(test)]
mod wizard_dismiss_tests {
    use crate::api::ApiState;
    use crate::state::AppState;
    use axum::Json;
    use axum::extract::State;
    use parking_lot::RwLock;
    use std::sync::Arc;

    fn api() -> ApiState {
        let (llm_tx, _llm_rx) = crossbeam_channel::unbounded();
        let (api_log_tx, _log_rx) = crossbeam_channel::unbounded();
        ApiState {
            app_state: Arc::new(RwLock::new(AppState::default())),
            llm_tx,
            api_log_tx,
            params_dirty: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    #[tokio::test]
    async fn play_hides_wizard_without_persisting() {
        let api = api();
        crate::api::post_play(State(api.clone())).await;
        let s = api.app_state.read();
        assert!(s.sequencer.running);
        assert_eq!(s.wizard_dismiss_requested, Some(false));
    }

    #[tokio::test]
    async fn preset_dismisses_and_persists_and_play_keeps_it() {
        let api = api();
        let req: crate::api::PresetRequest =
            serde_json::from_value(serde_json::json!({ "name": "Solo" })).unwrap();
        crate::api::post_preset(State(api.clone()), Json(req))
            .await
            .expect("Solo preset exists");
        assert_eq!(api.app_state.read().wizard_dismiss_requested, Some(true));
        // A following play must not downgrade the pending "done" request.
        crate::api::post_play(State(api.clone())).await;
        assert_eq!(api.app_state.read().wizard_dismiss_requested, Some(true));
    }
}
