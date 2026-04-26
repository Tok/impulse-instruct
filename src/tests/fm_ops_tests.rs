// ─── tests/fm_ops_tests.rs ───────────────────────────────────────────────────
// State-level tests for the FM operator synth — defaults, the
// apply_llm_update path, lock honouring, ModuleKind ↔ trigger
// wiring, sequencer-lane plumbing.
//
// DSP-level correctness (algorithm output, ADSR, boundedness) lives
// next to the voice in `audio/dsp/fm_ops.rs`.

#[cfg(test)]
mod fm_ops_state_tests {
    use crate::state::{AppState, FmOpsState, apply_llm_update};

    #[test]
    fn defaults_are_two_op_stack_silent_until_enabled() {
        let s = FmOpsState::default();
        assert!(!s.enabled);
        assert_eq!(s.algorithm, 0); // stack
        assert_eq!(s.feedback, 0.0);
        // Op 1 carrier full output, op 2 modulator at moderate
        // index, ops 3-4 silent — a clean simple FM tone the user
        // can immediately dial.
        assert_eq!(s.op1.level, 1.0);
        assert!((s.op2.level - 0.5).abs() < 1e-5);
        assert_eq!(s.op3.level, 0.0);
        assert_eq!(s.op4.level, 0.0);
        // Ratios all unison-detent (0.5 → 1.0×).
        assert!((s.op1.ratio - 0.5).abs() < 1e-5);
        assert!((s.op2.ratio - 0.5).abs() < 1e-5);
    }

    #[test]
    fn llm_apply_writes_global_knobs() {
        let s0 = AppState::default();
        let json = serde_json::json!({
            "fm_ops": {
                "enabled": true,
                "volume": 1.0,
                "pan": -0.5,
                "algorithm": 2,
                "feedback": 0.3,
            }
        });
        let s1 = apply_llm_update(s0, &json, &[]);
        assert!(s1.fm_ops.enabled);
        assert!((s1.fm_ops.volume - 1.0).abs() < 1e-5);
        assert!((s1.fm_ops.pan + 0.5).abs() < 1e-5);
        assert_eq!(s1.fm_ops.algorithm, 2);
        assert!((s1.fm_ops.feedback - 0.3).abs() < 1e-5);
    }

    #[test]
    fn llm_apply_writes_per_op_fields() {
        let s0 = AppState::default();
        let json = serde_json::json!({
            "fm_ops": {
                "op1": {
                    "ratio": 0.7,
                    "level": 0.3,
                    "attack": 0.1,
                    "decay": 0.5,
                    "sustain": 0.4,
                    "release": 0.6,
                },
                "op3": {
                    "level": 0.8,
                }
            }
        });
        let s1 = apply_llm_update(s0, &json, &[]);
        assert!((s1.fm_ops.op1.ratio - 0.7).abs() < 1e-5);
        assert!((s1.fm_ops.op1.level - 0.3).abs() < 1e-5);
        assert!((s1.fm_ops.op1.attack - 0.1).abs() < 1e-5);
        assert!((s1.fm_ops.op1.decay - 0.5).abs() < 1e-5);
        assert!((s1.fm_ops.op1.sustain - 0.4).abs() < 1e-5);
        assert!((s1.fm_ops.op1.release - 0.6).abs() < 1e-5);
        assert!((s1.fm_ops.op3.level - 0.8).abs() < 1e-5);
        // Op 2 and op 4 untouched.
        assert!((s1.fm_ops.op2.level - 0.5).abs() < 1e-5);
        assert_eq!(s1.fm_ops.op4.level, 0.0);
    }

    #[test]
    fn locked_algorithm_is_skipped() {
        let s0 = AppState::default();
        let json = serde_json::json!({ "fm_ops": { "algorithm": 3 } });
        let locked = ["fm_ops.algorithm".to_string()];
        let s1 = apply_llm_update(s0, &json, &locked);
        assert_eq!(s1.fm_ops.algorithm, 0); // unchanged from default
    }

    #[test]
    fn algorithm_clamps_to_valid_range() {
        // 99 is past the 4-algorithm V1 ship; apply must clamp so
        // the audio thread always sees a valid topology.
        let s0 = AppState::default();
        let json = serde_json::json!({ "fm_ops": { "algorithm": 99 } });
        let s1 = apply_llm_update(s0, &json, &[]);
        assert!(s1.fm_ops.algorithm < crate::state::FM_ALGORITHM_COUNT);
    }

    #[test]
    fn sequencer_lane_writes_steps_and_notes() {
        let s0 = AppState::default();
        let json = serde_json::json!({
            "fm_ops": {
                "fm_ops_steps": [true, false, true, false],
                "fm_ops_notes": [60, 64, 67, 72],
            }
        });
        let s1 = apply_llm_update(s0, &json, &[]);
        assert!(s1.sequencer.fm_ops_pattern[0].active);
        assert!(!s1.sequencer.fm_ops_pattern[1].active);
        assert!(s1.sequencer.fm_ops_pattern[2].active);
        assert_eq!(s1.sequencer.fm_ops_pattern[0].note, 60);
        assert_eq!(s1.sequencer.fm_ops_pattern[2].note, 67);
    }
}

#[cfg(test)]
mod fm_ops_module_tests {
    use crate::state::{ModuleKind, rack_scope::parse_module_kind};

    #[test]
    fn label_is_fm_ops() {
        // Card title — guards against an accidental rename that
        // would break the rack-scope alias matcher below.
        assert_eq!(ModuleKind::FmOpsVoice.label(), "FM OPS");
    }

    #[test]
    fn parses_from_fm_ops_aliases() {
        for alias in ["fmops", "fm_ops", "fm", "dx7", "operator"] {
            assert_eq!(
                parse_module_kind(alias),
                Some(ModuleKind::FmOpsVoice),
                "alias `{alias}` should parse"
            );
        }
    }

    #[test]
    fn produces_audio_and_lives_in_voice_zone() {
        use crate::state::Zone;
        let k = ModuleKind::FmOpsVoice;
        assert!(k.has_audio_output());
        assert_eq!(k.default_zone(), Zone::Voice);
    }
}
