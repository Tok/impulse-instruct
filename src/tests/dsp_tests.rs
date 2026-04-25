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

#[cfg(test)]
mod chain_transport_tests {
    use crate::state::{SequencerState, chain_advance_transport};

    fn loaded_slot(bpm: f32, swing: f32, apply: bool) -> SequencerState {
        let mut s = SequencerState::default();
        s.bpm = bpm;
        s.swing = swing;
        s.pattern_bpm_apply = apply;
        s.running = true;
        s
    }

    #[test]
    fn preserves_prior_transport_when_apply_off() {
        let loaded = loaded_slot(140.0, 0.4, false);
        let out = chain_advance_transport(loaded, 120.0, 0.0, true);
        assert!((out.bpm - 120.0).abs() < 1e-4, "bpm should stay prior");
        assert!((out.swing - 0.0).abs() < 1e-4, "swing should stay prior");
        assert!(out.running, "running always preserved");
    }

    #[test]
    fn adopts_loaded_transport_when_apply_on() {
        let loaded = loaded_slot(140.0, 0.4, true);
        let out = chain_advance_transport(loaded, 120.0, 0.0, true);
        assert!((out.bpm - 140.0).abs() < 1e-4, "bpm should jump to loaded");
        assert!(
            (out.swing - 0.4).abs() < 1e-4,
            "swing should jump to loaded"
        );
    }

    #[test]
    fn running_always_reflects_prior() {
        // Loaded slot has running=true; prior transport was stopped → stays stopped.
        let loaded = loaded_slot(140.0, 0.4, true);
        let out = chain_advance_transport(loaded, 120.0, 0.0, false);
        assert!(!out.running, "running mirrors prior regardless of apply");
    }
}

#[cfg(test)]
mod chain_preserve_non_bass_tests {
    use crate::state::{
        DrumVoice, SequencerState, Step, TB303Step, chain_advance_preserve_non_bass,
    };

    fn bank_with_bass_only() -> SequencerState {
        // A "freshly imported" bank: carries the bass line, leaves
        // drums/hoover/an1x empty (MIDI importer writes bass patterns
        // only).
        let mut s = SequencerState::default();
        s.bass_pattern[0] = TB303Step {
            active: true,
            note: 60,
            accent: 0.0,
            slide: 0.0,
            gate: 0.5,
            pan: 0.0,
            cond: 0,
        };
        s.pattern_bpm_apply = true;
        s.bpm = 180.0;
        s
    }

    fn prior_with_drums() -> SequencerState {
        // An "outgoing" sequencer where a KIT agent wrote drum hits
        // during the last 4-second bank play.  Also carries non-default
        // hoover/an1x to prove they survive.
        let mut s = SequencerState::default();
        if let Some(pat) = s.drum_patterns.get_mut(&DrumVoice::Kick808) {
            pat[0] = Step {
                active: true,
                velocity: 1.0,
                probability: 1.0,
                ratchet: 1,
                slice: 0,
                cond: 0,
            };
            pat[4] = pat[0];
            pat[8] = pat[0];
            pat[12] = pat[0];
        }
        s.hoover_pattern[0].active = true;
        s.an1x_pattern[2].active = true;
        s.swing = 0.25;
        s.time_sig_num = 7; // unusual time sig — proves it carries through
        s
    }

    #[test]
    fn carries_drums_hoover_an1x_across_swap() {
        let loaded = bank_with_bass_only();
        let prior = prior_with_drums();
        let out = chain_advance_preserve_non_bass(loaded, &prior, true);
        // Drums: all four kicks survive the bank swap.
        let kick = out.drum_patterns.get(&DrumVoice::Kick808).unwrap();
        assert!(kick[0].active && kick[4].active && kick[8].active && kick[12].active);
        // Hoover + an1x hits survive.
        assert!(out.hoover_pattern[0].active);
        assert!(out.an1x_pattern[2].active);
        // Time sig + swing: prior wins (musical coherence across banks).
        assert_eq!(out.time_sig_num, 7);
        assert!((out.swing - 0.25).abs() < 1e-4);
    }

    #[test]
    fn keeps_loaded_bass_pattern() {
        // Bank's bass line is the whole point of the swap — must be
        // present after the preserve-merge.
        let loaded = bank_with_bass_only();
        let prior = prior_with_drums();
        let out = chain_advance_preserve_non_bass(loaded, &prior, true);
        assert!(out.bass_pattern[0].active);
        assert_eq!(out.bass_pattern[0].note, 60);
    }

    #[test]
    fn always_inherits_prior_bpm_even_when_loaded_sets_pattern_bpm_apply() {
        // Contract: in MIDI-playback mode we WANT scripted `set_bpm`
        // calls (e.g. the Bach demo halfstepping to 120 after importing
        // a 240-BPM file) to survive every bank swap.  So BPM always
        // comes from prior here, regardless of what the loaded bank
        // pinned — pattern_bpm_apply is only honoured on the
        // loop=true / user-composed-song path.
        let loaded = bank_with_bass_only(); // bpm=180, pattern_bpm_apply=true
        let prior = prior_with_drums(); // default bpm=120
        let out = chain_advance_preserve_non_bass(loaded, &prior, true);
        assert!(
            (out.bpm - 120.0).abs() < 1e-4,
            "preserve-mode should inherit prior bpm, got {}",
            out.bpm
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
    use crate::state::LfoWaveform;

    #[test]
    fn sine_matches_textbook_quarter_cycles() {
        assert!((lfo_value_at(0.0, LfoWaveform::Sine, 0.0) - 0.0).abs() < 1e-5);
        assert!((lfo_value_at(0.25, LfoWaveform::Sine, 0.0) - 1.0).abs() < 1e-5);
        assert!((lfo_value_at(0.5, LfoWaveform::Sine, 0.0) - 0.0).abs() < 1e-5);
        assert!((lfo_value_at(0.75, LfoWaveform::Sine, 0.0) + 1.0).abs() < 1e-5);
    }

    #[test]
    fn triangle_peaks_at_half_phase() {
        // 1 - 4*|phase - 0.5|
        assert!((lfo_value_at(0.0, LfoWaveform::Triangle, 0.0) + 1.0).abs() < 1e-5);
        assert!((lfo_value_at(0.5, LfoWaveform::Triangle, 0.0) - 1.0).abs() < 1e-5);
        assert!((lfo_value_at(1.0, LfoWaveform::Triangle, 0.0) + 1.0).abs() < 1e-5);
    }

    #[test]
    fn saw_up_ramps_minus1_to_plus1() {
        assert!((lfo_value_at(0.0, LfoWaveform::Saw, 0.0) + 1.0).abs() < 1e-5);
        assert!((lfo_value_at(0.5, LfoWaveform::Saw, 0.0) - 0.0).abs() < 1e-5);
        assert!((lfo_value_at(1.0, LfoWaveform::Saw, 0.0) - 1.0).abs() < 1e-5);
    }

    #[test]
    fn saw_down_ramps_plus1_to_minus1() {
        assert!((lfo_value_at(0.0, LfoWaveform::InvSaw, 0.0) - 1.0).abs() < 1e-5);
        assert!((lfo_value_at(0.5, LfoWaveform::InvSaw, 0.0) - 0.0).abs() < 1e-5);
        assert!((lfo_value_at(1.0, LfoWaveform::InvSaw, 0.0) + 1.0).abs() < 1e-5);
    }

    #[test]
    fn square_flips_at_half_phase() {
        assert_eq!(lfo_value_at(0.0, LfoWaveform::Square, 0.0), 1.0);
        assert_eq!(lfo_value_at(0.49, LfoWaveform::Square, 0.0), 1.0);
        assert_eq!(lfo_value_at(0.5, LfoWaveform::Square, 0.0), -1.0);
        assert_eq!(lfo_value_at(0.99, LfoWaveform::Square, 0.0), -1.0);
    }

    #[test]
    fn sample_and_hold_returns_held_value_regardless_of_phase() {
        assert_eq!(lfo_value_at(0.0, LfoWaveform::SampleAndHold, 0.7), 0.7);
        assert_eq!(lfo_value_at(0.5, LfoWaveform::SampleAndHold, -0.3), -0.3);
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
