// ─── tests/cv_seq_tests.rs ───────────────────────────────────────────────────
// State + module tests for the CV sequencer module.  DSP-side
// behaviour (per-block step lookup + apply_mod_target dispatch)
// is covered by the existing process_block path; this file
// exercises the state shape, defaults, ModuleKind metadata,
// and the canonical per-step value contract.

#[cfg(test)]
mod cv_seq_state_tests {
    use crate::state::{AppState, CV_SEQ_SLOTS, CV_SEQ_STEPS, CvSeqSlot, LfoTarget};

    #[test]
    fn defaults_keep_every_slot_disabled_and_centered() {
        let s = AppState::default();
        // Four slots, all disabled with neutral 0.5 step values
        // so engaging the FX does nothing audible until the user
        // dials a step away from centre.
        assert_eq!(s.cv_seq.len(), CV_SEQ_SLOTS);
        for slot in &s.cv_seq {
            assert!(!slot.enabled);
            assert_eq!(slot.step_values.len(), CV_SEQ_STEPS);
            assert_eq!(slot.step_values, [0.5; CV_SEQ_STEPS]);
            assert_eq!(slot.target, LfoTarget::None);
            assert!((slot.depth - 0.3).abs() < 1e-5);
        }
    }

    #[test]
    fn slot_round_trips_through_default() {
        let s = CvSeqSlot::default();
        assert!(!s.enabled);
        assert_eq!(s.step_values, [0.5; CV_SEQ_STEPS]);
    }

    #[test]
    fn step_count_matches_canonical_bar() {
        // 16 steps mirrors the audio sequencer's canonical bar
        // grid so the CV walks in lock-step with the pattern.
        assert_eq!(CV_SEQ_STEPS, 16);
    }
}

#[cfg(test)]
mod cv_seq_module_tests {
    use crate::state::{ModuleKind, Zone, rack_scope::parse_module_kind};

    #[test]
    fn label_is_cv_seq() {
        assert_eq!(ModuleKind::CvSequencer.label(), "CV SEQ");
    }

    #[test]
    fn lives_in_fxmod_zone_with_no_audio_output() {
        let k = ModuleKind::CvSequencer;
        // CV-only module — no audio bus output.
        assert!(!k.has_audio_output());
        assert_eq!(k.default_zone(), Zone::FxMod);
    }

    #[test]
    fn parses_from_aliases() {
        for alias in ["cvsequencer", "cvseq", "cv_seq", "stepcv", "cv"] {
            assert_eq!(
                parse_module_kind(alias),
                Some(ModuleKind::CvSequencer),
                "alias `{alias}` should parse"
            );
        }
    }
}
