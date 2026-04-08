// ─── tests/dsp_tests.rs ──────────────────────────────────────────────────────
// Tests for new DSP features: tuning, cross-mod params, sidechain, multiband,
// tape delay, reverb freeze, style propagation.

#[cfg(test)]
mod tuning_tests {
    use crate::audio::dsp::midi_to_hz;

    // We can't directly call midi_to_hz_tuned (it's pub(crate) in dsp_util),
    // but we can test it through the public midi_to_hz and verify 12-TET.
    #[test]
    fn midi_to_hz_a4_is_440() {
        let hz = midi_to_hz(69);
        assert!((hz - 440.0).abs() < 0.01, "A4 should be 440 Hz, got {}", hz);
    }

    #[test]
    fn midi_to_hz_c4_is_261() {
        let hz = midi_to_hz(60);
        assert!(
            (hz - 261.626).abs() < 0.1,
            "C4 should be ~261.6 Hz, got {}",
            hz
        );
    }

    #[test]
    fn midi_to_hz_octave_doubles() {
        let c3 = midi_to_hz(48);
        let c4 = midi_to_hz(60);
        assert!(
            (c4 / c3 - 2.0).abs() < 0.001,
            "octave should double frequency"
        );
    }
}

#[cfg(test)]
mod new_fx_param_tests {
    use crate::state::{AppState, apply_llm_update, lock_param};

    #[test]
    fn delay_wow_flutter_applied() {
        let s = AppState::default();
        let update = serde_json::json!({ "fx": { "delay_wow_flutter": 0.5 } });
        let s = apply_llm_update(s, &update, &[]);
        assert!((s.fx.delay_wow_flutter - 0.5).abs() < 1e-4);
    }

    #[test]
    fn delay_saturation_applied() {
        let s = AppState::default();
        let update = serde_json::json!({ "fx": { "delay_saturation": 0.7 } });
        let s = apply_llm_update(s, &update, &[]);
        assert!((s.fx.delay_saturation - 0.7).abs() < 1e-4);
    }

    #[test]
    fn sidechain_params_applied() {
        let s = AppState::default();
        let update = serde_json::json!({
            "fx": { "sidechain_amount": 0.6, "sidechain_attack": 0.3, "sidechain_release": 0.8 }
        });
        let s = apply_llm_update(s, &update, &[]);
        assert!((s.fx.sidechain_amount - 0.6).abs() < 1e-4);
        assert!((s.fx.sidechain_attack - 0.3).abs() < 1e-4);
        assert!((s.fx.sidechain_release - 0.8).abs() < 1e-4);
    }

    #[test]
    fn compressor_multiband_applied() {
        let s = AppState::default();
        let update = serde_json::json!({ "fx": { "compressor_multiband": 0.9 } });
        let s = apply_llm_update(s, &update, &[]);
        assert!((s.fx.compressor_multiband - 0.9).abs() < 1e-4);
    }

    #[test]
    fn stereo_width_applied() {
        let s = AppState::default();
        let update = serde_json::json!({ "fx": { "stereo_width": 0.8 } });
        let s = apply_llm_update(s, &update, &[]);
        assert!((s.fx.stereo_width - 0.8).abs() < 1e-4);
    }

    #[test]
    fn reverb_freeze_applied() {
        let s = AppState::default();
        let update = serde_json::json!({ "fx": { "reverb_freeze": true } });
        let s = apply_llm_update(s, &update, &[]);
        assert!(s.fx.reverb_freeze);
    }

    #[test]
    fn xmod_params_applied() {
        let s = AppState::default();
        let update = serde_json::json!({
            "fx": { "xmod_bass_to_an1x_pitch": 0.4, "xmod_noise_to_filter": 0.6 }
        });
        let s = apply_llm_update(s, &update, &[]);
        assert!((s.fx.xmod_bass_to_an1x_pitch - 0.4).abs() < 1e-4);
        assert!((s.fx.xmod_noise_to_filter - 0.6).abs() < 1e-4);
    }

    #[test]
    fn tuning_param_applied() {
        let s = AppState::default();
        let update = serde_json::json!({ "fx": { "tuning": 2 } });
        let s = apply_llm_update(s, &update, &[]);
        assert_eq!(s.fx.tuning, 2);
    }

    #[test]
    fn locked_sidechain_not_overwritten() {
        let s = lock_param(AppState::default(), "fx.sidechain_amount");
        let orig = s.fx.sidechain_amount;
        let update = serde_json::json!({ "fx": { "sidechain_amount": 0.99 } });
        let s = apply_llm_update(s, &update, &[]);
        assert_eq!(s.fx.sidechain_amount, orig);
    }
}

#[cfg(test)]
mod granular_llm_tests {
    use crate::state::{AppState, apply_llm_update};

    #[test]
    fn granular_params_applied() {
        let s = AppState::default();
        let update = serde_json::json!({
            "granular": {
                "enabled": true,
                "volume": 0.6,
                "density": 0.8,
                "grain_size": 0.5,
                "position": 0.3,
                "pitch_scatter": 0.4
            }
        });
        let s = apply_llm_update(s, &update, &[]);
        assert!(s.granular.enabled);
        assert!((s.granular.volume - 0.6).abs() < 1e-4);
        assert!((s.granular.density - 0.8).abs() < 1e-4);
        assert!((s.granular.position - 0.3).abs() < 1e-4);
    }
}

#[cfg(test)]
mod noise_llm_tests {
    use crate::state::{AppState, apply_llm_update};

    #[test]
    fn noise_envelope_params_applied() {
        let s = AppState::default();
        let update = serde_json::json!({
            "noise": {
                "attack": 0.5, "release": 0.7,
                "filter_lfo_rate": 0.3, "filter_lfo_depth": 0.4,
                "sh_rate": 0.6, "sh_depth": 0.5
            }
        });
        let s = apply_llm_update(s, &update, &[]);
        assert!((s.noise_voice.attack - 0.5).abs() < 1e-4);
        assert!((s.noise_voice.release - 0.7).abs() < 1e-4);
        assert!((s.noise_voice.filter_lfo_rate - 0.3).abs() < 1e-4);
        assert!((s.noise_voice.sh_rate - 0.6).abs() < 1e-4);
    }
}

#[cfg(test)]
mod style_propagation_tests {
    use crate::state::{AgentRole, AppState, propagate_style, spawn_agent};

    #[test]
    fn propagate_style_updates_unlocked_agents() {
        let s = AppState::default();
        let (s, _) = spawn_agent(s, "Agent1", &[], AgentRole::Producer, None);
        let (s, _) = spawn_agent(s, "Agent2", &[], AgentRole::Producer, None);
        let s = propagate_style(s, "acid");
        for agent in &s.llm_agents {
            assert_eq!(agent.active_style, Some("acid".to_string()));
        }
    }

    #[test]
    fn propagate_style_skips_locked_agents() {
        let s = AppState::default();
        let (mut s, _) = spawn_agent(s, "Agent1", &[], AgentRole::Producer, None);
        // Lock the last agent's style
        s.llm_agents.last_mut().unwrap().style_locked = true;
        s.llm_agents.last_mut().unwrap().active_style = Some("techno".to_string());
        let s = propagate_style(s, "acid");
        let locked_agent = s.llm_agents.last().unwrap();
        assert_eq!(
            locked_agent.active_style,
            Some("techno".to_string()),
            "locked agent should keep its style"
        );
    }
}
