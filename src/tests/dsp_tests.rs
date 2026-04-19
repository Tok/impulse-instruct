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

    #[test]
    fn apply_pattern_style_on_advance_sets_global_and_agents() {
        use crate::state::apply_pattern_style_on_advance;
        let s = AppState::default();
        let (s, _) = spawn_agent(s, "A", &[], AgentRole::Producer, None);
        let (s, _) = spawn_agent(s, "B", &[], AgentRole::Producer, None);
        let s = apply_pattern_style_on_advance(s, Some("drum_and_bass"));
        assert_eq!(s.llm.active_style.as_deref(), Some("drum_and_bass"));
        for a in &s.llm_agents {
            assert_eq!(a.active_style.as_deref(), Some("drum_and_bass"));
        }
    }

    #[test]
    fn apply_pattern_style_on_advance_with_none_is_noop() {
        use crate::state::apply_pattern_style_on_advance;
        let mut s = AppState::default();
        s.llm.active_style = Some("ambient".into());
        let s = apply_pattern_style_on_advance(s, None);
        assert_eq!(s.llm.active_style.as_deref(), Some("ambient"));
    }

    #[test]
    fn apply_pattern_style_respects_agent_lock() {
        use crate::state::apply_pattern_style_on_advance;
        let s = AppState::default();
        let (mut s, _) = spawn_agent(s, "Locked", &[], AgentRole::Producer, None);
        s.llm_agents[0].style_locked = true;
        s.llm_agents[0].active_style = Some("techno".into());
        let s = apply_pattern_style_on_advance(s, Some("acid"));
        assert_eq!(s.llm.active_style.as_deref(), Some("acid"));
        assert_eq!(
            s.llm_agents[0].active_style.as_deref(),
            Some("techno"),
            "locked agent should retain its style"
        );
    }
}

// ── fx_math: extracted pure DSP helpers ─────────────────────────────────────

#[cfg(test)]
mod waveshaper_step_tests {
    use crate::audio::dsp::fx_math::waveshaper_step;

    #[test]
    fn zero_mix_returns_dry_signal_unchanged() {
        let dry = 0.42;
        assert_eq!(waveshaper_step(dry, 0.7, 0.0), dry);
    }

    #[test]
    fn full_wet_compresses_loud_input() {
        // The fast tanh approximation in dsp_util isn't bounded the way
        // libm's tanh is — it can exceed 1 — but it still pulls a hot
        // input toward saturation.  Verify the output is smaller in
        // magnitude than the dry input.
        let dry = 2.0;
        let out = waveshaper_step(dry, 1.0, 1.0);
        assert!(
            out.abs() < dry.abs(),
            "shaper should compress 2.0, got {out}"
        );
    }

    #[test]
    fn full_wet_passes_quiet_input_in_the_same_direction() {
        // Quiet inputs sit in the near-linear region.  The divisor by
        // tanh(drive) keeps them from being suppressed; just check the
        // sign and that the output is in a sensible amplitude range.
        let out = waveshaper_step(0.05, 0.5, 1.0);
        assert!(out > 0.0 && out < 1.0, "got {out}");
    }

    #[test]
    fn output_is_an_odd_function() {
        // Tanh is odd, so flipping the input flips the output.
        let pos = waveshaper_step(0.7, 0.5, 1.0);
        let neg = waveshaper_step(-0.7, 0.5, 1.0);
        assert!((pos + neg).abs() < 1e-6);
    }
}

#[cfg(test)]
mod drive_step_tests {
    use crate::audio::dsp::fx_math::drive_step;

    #[test]
    fn near_zero_drive_returns_dry() {
        assert_eq!(drive_step(0.5, 0.0, 1.0), 0.5);
    }

    #[test]
    fn loud_signal_compresses_relative_to_dry() {
        // Same caveat as the waveshaper: the fast tanh approximation
        // isn't bounded at ±1, but it still squeezes a hot input.
        let dry = 2.0;
        let out = drive_step(dry, 1.0, 1.0);
        assert!(out.abs() < dry.abs(), "got {out}");
    }

    #[test]
    fn dry_blend_at_zero_mix_with_active_drive() {
        // Drive is on but mix is dry — output should equal input.
        assert_eq!(drive_step(0.4, 0.5, 0.0), 0.4);
    }
}

#[cfg(test)]
mod bitcrush_step_tests {
    use crate::audio::dsp::fx_math::{BitcrushState, bitcrush_step};

    #[test]
    fn zero_mix_passes_signal_and_preserves_state() {
        let s0 = BitcrushState {
            held: 0.42,
            counter: 5,
        };
        let (s1, out) = bitcrush_step(s0, 0.1, 0.5, 0.5, 0.0);
        assert_eq!(out, 0.1);
        assert_eq!(s1.held, s0.held);
        assert_eq!(s1.counter, s0.counter);
    }

    #[test]
    fn first_call_quantises_input_and_arms_counter() {
        let (s1, _) = bitcrush_step(BitcrushState::default(), 0.5, 1.0, 0.0, 1.0);
        // rate=0 → counter loaded with 1 (the floor of 1 + 0*15).
        assert_eq!(s1.counter, 1);
    }

    #[test]
    fn within_hold_window_held_value_persists() {
        // First call latches; second + third should return the same held
        // value while the counter ticks down.
        let s0 = BitcrushState::default();
        let (s1, out1) = bitcrush_step(s0, 0.123, 1.0, 0.5, 1.0);
        // Use a very different input on the next call — should not be
        // re-quantised because the counter is non-zero.
        let (s2, out2) = bitcrush_step(s1, -0.987, 1.0, 0.5, 1.0);
        // mix=1 → output equals held value exactly.
        assert_eq!(out1, s1.held);
        assert_eq!(out2, s1.held);
        assert_eq!(s2.held, s1.held);
        assert!(s2.counter < s1.counter);
    }

    #[test]
    fn counter_reaches_zero_then_re_quantises_on_next_call() {
        // rate_norm=0 → the counter is loaded with `1` after a re-quant,
        // so two consecutive calls always trigger a fresh quantisation.
        let s0 = BitcrushState::default();
        let (s1, _) = bitcrush_step(s0, 0.5, 1.0, 0.0, 1.0);
        // Counter was set to 1; the next call decrements it to 0.
        let (s2, _) = bitcrush_step(s1, 0.7, 1.0, 0.0, 1.0);
        // Now counter is 0 → next call re-latches the new input.
        let (s3, out3) = bitcrush_step(s2, -0.4, 1.0, 0.0, 1.0);
        assert_ne!(s3.held, s1.held, "re-quant should pick up the new input");
        assert!(out3 < 0.0, "held value reflects the latest negative input");
    }
}

#[cfg(test)]
mod sidechain_tests {
    use crate::audio::dsp::fx_math::{sidechain_duck, sidechain_envelope_step};

    #[test]
    fn envelope_rises_toward_target_when_input_exceeds_prev() {
        let env = sidechain_envelope_step(0.0, 1.0, 1.0, 100.0, 44100.0);
        assert!(env > 0.0 && env < 1.0, "should coast partway, got {env}");
    }

    #[test]
    fn envelope_decays_when_target_drops_below_prev() {
        let env = sidechain_envelope_step(1.0, 0.0, 1.0, 100.0, 44100.0);
        assert!(env < 1.0 && env > 0.0);
    }

    #[test]
    fn envelope_decays_geometrically_with_zero_target() {
        let e1 = sidechain_envelope_step(1.0, 0.0, 1.0, 100.0, 44100.0);
        let e2 = sidechain_envelope_step(e1, 0.0, 1.0, 100.0, 44100.0);
        // Each release sample multiplies by the same release coefficient.
        let ratio = e2 / e1;
        let expected = e1; // also = release coefficient * 1.0
        assert!((ratio - expected).abs() < 1e-4);
    }

    #[test]
    fn duck_zero_env_means_no_attenuation() {
        assert_eq!(sidechain_duck(0.0, 0.5), 1.0);
    }

    #[test]
    fn duck_zero_amount_means_no_attenuation_either() {
        assert_eq!(sidechain_duck(0.5, 0.0), 1.0);
    }

    #[test]
    fn duck_clamps_at_full_attenuation() {
        // env=1, amount=1 would multiply to 4.0, then min(1.0)→1.0,
        // then 1.0-1.0 = 0.0.  Verify the clamp prevents negative gain.
        assert_eq!(sidechain_duck(1.0, 1.0), 0.0);
        assert_eq!(sidechain_duck(2.0, 1.0), 0.0);
    }
}

#[cfg(test)]
mod gated_reverb_envelope_tests {
    use crate::audio::dsp::fx_math::gated_reverb_envelope_step;

    #[test]
    fn zero_gate_time_keeps_envelope_open() {
        assert_eq!(gated_reverb_envelope_step(0.42, 0.0, 0.0, 44100.0), 1.0);
        assert_eq!(gated_reverb_envelope_step(0.42, 0.5, 0.0, 44100.0), 1.0);
    }

    #[test]
    fn loud_input_re_opens_the_gate() {
        let env = gated_reverb_envelope_step(0.1, 0.5, 0.5, 44100.0);
        assert_eq!(env, 1.0);
    }

    #[test]
    fn quiet_input_decays_envelope_geometrically() {
        let e0 = 1.0;
        let e1 = gated_reverb_envelope_step(e0, 0.0, 0.5, 44100.0);
        let e2 = gated_reverb_envelope_step(e1, 0.0, 0.5, 44100.0);
        assert!(e1 < e0);
        assert!(e2 < e1);
    }
}

#[cfg(test)]
mod lfo_value_tests {
    use crate::audio::dsp::fx_math::lfo_value_at;

    #[test]
    fn sine_matches_textbook_quarter_cycles() {
        assert!((lfo_value_at(0.0, 0, 0.0) - 0.0).abs() < 1e-5);
        assert!((lfo_value_at(0.25, 0, 0.0) - 1.0).abs() < 1e-5);
        assert!((lfo_value_at(0.5, 0, 0.0) - 0.0).abs() < 1e-5);
        assert!((lfo_value_at(0.75, 0, 0.0) + 1.0).abs() < 1e-5);
    }

    #[test]
    fn triangle_peaks_at_half_phase() {
        // 1 - 4*|phase - 0.5|
        assert!((lfo_value_at(0.0, 1, 0.0) + 1.0).abs() < 1e-5);
        assert!((lfo_value_at(0.5, 1, 0.0) - 1.0).abs() < 1e-5);
        assert!((lfo_value_at(1.0, 1, 0.0) + 1.0).abs() < 1e-5);
    }

    #[test]
    fn saw_up_ramps_minus1_to_plus1() {
        assert!((lfo_value_at(0.0, 2, 0.0) + 1.0).abs() < 1e-5);
        assert!((lfo_value_at(0.5, 2, 0.0) - 0.0).abs() < 1e-5);
        assert!((lfo_value_at(1.0, 2, 0.0) - 1.0).abs() < 1e-5);
    }

    #[test]
    fn saw_down_ramps_plus1_to_minus1() {
        assert!((lfo_value_at(0.0, 3, 0.0) - 1.0).abs() < 1e-5);
        assert!((lfo_value_at(0.5, 3, 0.0) - 0.0).abs() < 1e-5);
        assert!((lfo_value_at(1.0, 3, 0.0) + 1.0).abs() < 1e-5);
    }

    #[test]
    fn square_flips_at_half_phase() {
        assert_eq!(lfo_value_at(0.0, 4, 0.0), 1.0);
        assert_eq!(lfo_value_at(0.49, 4, 0.0), 1.0);
        assert_eq!(lfo_value_at(0.5, 4, 0.0), -1.0);
        assert_eq!(lfo_value_at(0.99, 4, 0.0), -1.0);
    }

    #[test]
    fn sample_and_hold_returns_held_value_regardless_of_phase() {
        // Any waveform code outside 0..=4 is treated as S&H.
        assert_eq!(lfo_value_at(0.0, 5, 0.7), 0.7);
        assert_eq!(lfo_value_at(0.5, 99, -0.3), -0.3);
    }
}

#[cfg(test)]
mod free_eg_tests {
    use crate::audio::dsp::fx_math::free_eg_value_at;

    #[test]
    fn depth_at_half_zeros_the_output() {
        // bipolar_depth = (0.5 - 0.5) * 2 = 0
        let values = [1.0; 8];
        assert_eq!(free_eg_value_at(0.5, &values, 0.5), 0.0);
    }

    #[test]
    fn full_positive_depth_returns_the_step_value() {
        let values = [0.3; 8];
        assert!((free_eg_value_at(0.0, &values, 1.0) - 0.3).abs() < 1e-5);
    }

    #[test]
    fn full_negative_depth_inverts_the_step_value() {
        let values = [0.4; 8];
        assert!((free_eg_value_at(0.0, &values, 0.0) - -0.4).abs() < 1e-5);
    }

    #[test]
    fn linear_interpolation_between_steps() {
        // 8 step values, phase in (0..1) maps to position 0..7.
        // phase = 1/14 → pos ≈ 0.5 → halfway between values[0] and values[1].
        let mut values = [0.0; 8];
        values[0] = 0.0;
        values[1] = 1.0;
        let mid = free_eg_value_at(0.5 / 7.0, &values, 1.0);
        assert!((mid - 0.5).abs() < 1e-4, "expected midpoint 0.5 got {mid}");
    }

    #[test]
    fn phase_above_one_clamps_to_last_step() {
        let mut values = [0.0; 8];
        values[7] = 0.9;
        assert!((free_eg_value_at(2.0, &values, 1.0) - 0.9).abs() < 1e-5);
    }
}

// ── voices: low-level voice DSP primitives ──────────────────────────────────

#[cfg(test)]
mod ladder_filter_tests {
    use crate::audio::dsp::voices::LadderFilter;

    #[test]
    fn zero_input_zero_output_in_steady_state() {
        let mut f = LadderFilter::default();
        // Run a few samples of silence — the filter shouldn't ring without input.
        for _ in 0..10 {
            let out = f.process(0.0, 0.5, 0.0);
            assert!(out.abs() < 1e-6);
        }
    }

    #[test]
    fn output_attenuates_high_frequencies_relative_to_dc() {
        let mut f = LadderFilter::default();
        // Drive DC for 2000 samples — should approach the input.
        let mut dc_steady = 0.0;
        for _ in 0..2000 {
            dc_steady = f.process(0.5, 0.05, 0.0);
        }
        // Now feed a high-frequency square wave for 2000 samples.
        let mut f2 = LadderFilter::default();
        let mut hf_peak = 0.0_f32;
        for i in 0..2000 {
            let sq = if i % 2 == 0 { 0.5 } else { -0.5 };
            let out = f2.process(sq, 0.05, 0.0);
            hf_peak = hf_peak.max(out.abs());
        }
        assert!(
            hf_peak < dc_steady.abs(),
            "low cutoff should attenuate HF more than DC: hf_peak={hf_peak}, dc={dc_steady}"
        );
    }

    #[test]
    fn output_stays_finite_at_high_resonance() {
        let mut f = LadderFilter::default();
        // Self-oscillation territory; tanh saturation should keep things bounded.
        for _ in 0..1000 {
            let out = f.process(0.1, 0.5, 1.0);
            assert!(out.is_finite(), "filter blew up: {out}");
            assert!(out.abs() < 5.0, "filter ran away: {out}");
        }
    }
}

#[cfg(test)]
mod one_pole_tests {
    use crate::audio::dsp::voices::OnePole;

    #[test]
    fn coeff_one_freezes_state_to_initial_value() {
        let mut f = OnePole::default();
        // First call seeds state from input (since state starts at 0):
        // state = 0*1 + input*0 = 0; output = 0.
        let _ = f.process(1.0, 1.0);
        // Subsequent calls keep returning 0 because input is zero-weighted.
        assert_eq!(f.process(0.5, 1.0), 0.0);
    }

    #[test]
    fn coeff_zero_passes_input_through() {
        let mut f = OnePole::default();
        assert_eq!(f.process(0.7, 0.0), 0.7);
        assert_eq!(f.process(-0.3, 0.0), -0.3);
    }

    #[test]
    fn smoothing_converges_to_step_input() {
        let mut f = OnePole::default();
        let mut last = 0.0;
        for _ in 0..200 {
            last = f.process(1.0, 0.95);
        }
        assert!(
            (last - 1.0).abs() < 0.01,
            "should converge close to 1.0, got {last}"
        );
    }
}

#[cfg(test)]
mod noise_gen_tests {
    use crate::audio::dsp::voices::NoiseGen;

    #[test]
    fn output_stays_in_signed_unit_range() {
        let mut g = NoiseGen::new(42);
        for _ in 0..1000 {
            let v = g.next();
            assert!((-1.0..=1.0).contains(&v), "out of range: {v}");
        }
    }

    #[test]
    fn fixed_seed_is_deterministic() {
        let mut a = NoiseGen::new(123);
        let mut b = NoiseGen::new(123);
        for _ in 0..50 {
            assert_eq!(a.next(), b.next());
        }
    }

    #[test]
    fn distinct_seeds_diverge_within_a_few_samples() {
        let mut a = NoiseGen::new(1);
        let mut b = NoiseGen::new(2);
        let mut diverged = false;
        for _ in 0..16 {
            if a.next() != b.next() {
                diverged = true;
                break;
            }
        }
        assert!(diverged, "different seeds should produce different streams");
    }

    #[test]
    fn zero_seed_is_replaced_to_avoid_xorshift_lock() {
        let mut g = NoiseGen::new(0);
        // The constructor's `seed.max(1)` keeps xorshift32 out of its
        // degenerate fixed-point at zero.
        let v = g.next();
        assert!(v.is_finite());
        assert!(v != 0.0 || g.next() != 0.0, "stream got stuck at zero");
    }
}

#[cfg(test)]
mod envelope_tests {
    use crate::audio::dsp::voices::Envelope;

    #[test]
    fn fresh_envelope_is_inactive_and_returns_zero() {
        let mut e = Envelope::default();
        assert!(!e.active);
        assert_eq!(e.tick(0.99), 0.0);
    }

    #[test]
    fn trigger_arms_envelope_at_unity() {
        let mut e = Envelope::default();
        e.trigger();
        assert!(e.active);
        assert_eq!(e.value, 1.0);
    }

    #[test]
    fn tick_decays_geometrically_while_active() {
        let mut e = Envelope::default();
        e.trigger();
        let v1 = e.tick(0.5);
        let v2 = e.tick(0.5);
        assert_eq!(v1, 0.5);
        assert_eq!(v2, 0.25);
    }

    #[test]
    fn tiny_value_deactivates_envelope() {
        let mut e = Envelope::default();
        e.trigger();
        // Very low decay coefficient drops the envelope under 1e-6 quickly.
        for _ in 0..40 {
            let _ = e.tick(0.5);
        }
        assert!(!e.active, "envelope should have deactivated");
        assert_eq!(e.tick(0.99), 0.0);
    }
}

#[cfg(test)]
mod adsr_tests {
    use crate::audio::dsp::voices::{AdsrPhase, adsr_samples, adsr_tick};

    #[test]
    fn adsr_samples_minimum_clamps_to_one() {
        // v=0 → 1 ms; at 1000 Hz that's exactly 1 sample, but the floor
        // protects against absurdly short attacks at very low SRs too.
        let n = adsr_samples(0.0, 100.0, 10_000.0);
        assert_eq!(n, 1.0);
    }

    #[test]
    fn adsr_samples_quadratic_curve_grows_with_v() {
        let sr = 44100.0;
        let n_quarter = adsr_samples(0.25, sr, 10_000.0);
        let n_half = adsr_samples(0.5, sr, 10_000.0);
        let n_full = adsr_samples(1.0, sr, 10_000.0);
        assert!(n_quarter < n_half);
        assert!(n_half < n_full);
        // Quadratic: doubling v roughly quadruples the resulting time.
        assert!(n_half / n_quarter > 3.0);
    }

    #[test]
    fn adsr_attack_phase_advances_value_toward_unity() {
        let mut phase = AdsrPhase::Attack;
        let mut val = 0.0;
        for _ in 0..2000 {
            adsr_tick(&mut phase, &mut val, true, 0.05, 0.5, 0.5, 0.5, 44100.0);
        }
        assert!(val > 0.5, "attack should ramp the value up, got {val}");
    }

    #[test]
    fn adsr_release_phase_decays_to_zero() {
        let mut phase = AdsrPhase::Release;
        let mut val = 0.5;
        // Quadratic curve: at v=0.05 release is ~76 ms; we need a bit
        // over that to fall below the 1e-4 idle threshold.
        for _ in 0..50_000 {
            adsr_tick(&mut phase, &mut val, false, 0.5, 0.5, 0.5, 0.05, 44100.0);
        }
        assert_eq!(val, 0.0);
        assert_eq!(phase, AdsrPhase::Idle);
    }

    #[test]
    fn adsr_release_re_enters_attack_when_gate_returns() {
        let mut phase = AdsrPhase::Release;
        let mut val = 0.3;
        // Gate goes high while in Release — the engine should retrigger.
        adsr_tick(&mut phase, &mut val, true, 0.5, 0.5, 0.5, 0.5, 44100.0);
        assert_eq!(phase, AdsrPhase::Attack);
    }

    #[test]
    fn adsr_sustain_drops_to_release_when_gate_clears() {
        let mut phase = AdsrPhase::Sustain;
        let mut val = 0.7;
        adsr_tick(&mut phase, &mut val, false, 0.5, 0.5, 0.7, 0.5, 44100.0);
        assert_eq!(phase, AdsrPhase::Release);
    }

    #[test]
    fn adsr_idle_phase_does_nothing() {
        let mut phase = AdsrPhase::Idle;
        let mut val = 0.0;
        for _ in 0..100 {
            adsr_tick(&mut phase, &mut val, false, 0.5, 0.5, 0.5, 0.5, 44100.0);
        }
        assert_eq!(phase, AdsrPhase::Idle);
        assert_eq!(val, 0.0);
    }
}

#[cfg(test)]
mod osc_sample_tests {
    use crate::audio::dsp::voices::osc_sample;

    #[test]
    fn saw_ramps_minus1_to_plus1() {
        let mut ns = 1u32;
        assert!((osc_sample(0, 0.0, &mut ns) + 1.0).abs() < 1e-5);
        assert!((osc_sample(0, 0.5, &mut ns) - 0.0).abs() < 1e-5);
        assert!((osc_sample(0, 1.0, &mut ns) - 1.0).abs() < 1e-5);
    }

    #[test]
    fn square_flips_at_half_phase() {
        let mut ns = 1u32;
        assert_eq!(osc_sample(1, 0.0, &mut ns), 1.0);
        assert_eq!(osc_sample(1, 0.49, &mut ns), 1.0);
        assert_eq!(osc_sample(1, 0.5, &mut ns), -1.0);
        assert_eq!(osc_sample(1, 0.99, &mut ns), -1.0);
    }

    #[test]
    fn triangle_peaks_at_half_phase() {
        let mut ns = 1u32;
        assert!((osc_sample(2, 0.0, &mut ns) + 1.0).abs() < 1e-5);
        assert!((osc_sample(2, 0.5, &mut ns) - 1.0).abs() < 1e-5);
        assert!((osc_sample(2, 1.0, &mut ns) + 1.0).abs() < 1e-5);
    }

    #[test]
    fn sine_matches_textbook_quarter_cycles() {
        let mut ns = 1u32;
        assert!((osc_sample(3, 0.0, &mut ns) - 0.0).abs() < 1e-5);
        assert!((osc_sample(3, 0.25, &mut ns) - 1.0).abs() < 1e-5);
        assert!((osc_sample(3, 0.75, &mut ns) + 1.0).abs() < 1e-5);
    }

    #[test]
    fn noise_mode_advances_the_state() {
        let mut ns = 1u32;
        let _ = osc_sample(99, 0.0, &mut ns);
        assert_ne!(ns, 1u32, "LCG should advance the state");
    }

    #[test]
    fn noise_output_stays_in_signed_unit_range() {
        let mut ns = 42u32;
        for _ in 0..500 {
            let v = osc_sample(99, 0.0, &mut ns);
            assert!(v.is_finite());
            assert!((-1.0..=1.0).contains(&v));
        }
    }
}

#[cfg(test)]
mod drum_voice_idx_tests {
    use crate::audio::dsp::voices::drum_voice_idx;
    use crate::state::DrumVoice;

    #[test]
    fn block_808_indices_match_expected_layout() {
        assert_eq!(drum_voice_idx(&DrumVoice::Kick808), 0);
        assert_eq!(drum_voice_idx(&DrumVoice::Snare808), 1);
        assert_eq!(drum_voice_idx(&DrumVoice::HihatClosed808), 2);
        assert_eq!(drum_voice_idx(&DrumVoice::HihatOpen808), 3);
        assert_eq!(drum_voice_idx(&DrumVoice::TomHi808), 4);
        assert_eq!(drum_voice_idx(&DrumVoice::TomMid808), 5);
        assert_eq!(drum_voice_idx(&DrumVoice::TomLo808), 6);
    }

    #[test]
    fn block_909_starts_after_808_block() {
        assert_eq!(drum_voice_idx(&DrumVoice::Kick909), 7);
        assert_eq!(drum_voice_idx(&DrumVoice::Snare909), 8);
        assert_eq!(drum_voice_idx(&DrumVoice::HihatClosed909), 9);
        assert_eq!(drum_voice_idx(&DrumVoice::HihatOpen909), 10);
        assert_eq!(drum_voice_idx(&DrumVoice::Clap909), 11);
        assert_eq!(drum_voice_idx(&DrumVoice::Rim909), 12);
    }

    #[test]
    fn amen_and_gabber_are_at_the_tail() {
        assert_eq!(drum_voice_idx(&DrumVoice::Amen), 13);
        assert_eq!(drum_voice_idx(&DrumVoice::GabberKick), 14);
    }

    #[test]
    fn every_drum_voice_maps_to_a_unique_slot() {
        let voices = [
            DrumVoice::Kick808,
            DrumVoice::Snare808,
            DrumVoice::HihatClosed808,
            DrumVoice::HihatOpen808,
            DrumVoice::TomHi808,
            DrumVoice::TomMid808,
            DrumVoice::TomLo808,
            DrumVoice::Kick909,
            DrumVoice::Snare909,
            DrumVoice::HihatClosed909,
            DrumVoice::HihatOpen909,
            DrumVoice::Clap909,
            DrumVoice::Rim909,
            DrumVoice::Amen,
            DrumVoice::GabberKick,
        ];
        let mut seen = std::collections::HashSet::new();
        for v in &voices {
            assert!(seen.insert(drum_voice_idx(v)), "duplicate index for {v:?}");
        }
        assert_eq!(seen.len(), voices.len());
    }
}
