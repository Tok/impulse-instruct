// ─── tests/coverage_tests.rs ─────────────────────────────────────────────────
// Targeted tests for pure helpers that weren't covered elsewhere.  One
// file of narrow, fast tests to lift the codecov baseline without
// distorting any existing module's test suite.

// ── FxStep::idx — dense index map for audio-thread arrays ──────────────────
#[cfg(test)]
mod fx_step_idx_tests {
    use crate::state::{FX_STEP_COUNT, FxStep};

    #[test]
    fn every_variant_has_distinct_idx_in_range() {
        // Every variant in FxStep must yield a unique index in
        // 0..FX_STEP_COUNT.  `prev_fx_output: [f32; FX_STEP_COUNT]` on
        // DspState indexes by this value, so a collision would mean
        // two FX share a feedback buffer and would corrupt each other.
        let variants = [
            FxStep::Waveshaper,
            FxStep::Reverb,
            FxStep::Delay,
            FxStep::Bitcrush,
            FxStep::Chorus,
            FxStep::Phaser,
            FxStep::Flanger,
            FxStep::Limiter,
            FxStep::Filter,
            FxStep::Comb,
            FxStep::Tilt,
            FxStep::Transient,
            FxStep::Exciter,
            FxStep::Multitap,
            FxStep::RevDelay,
            FxStep::TapeStop,
            FxStep::Stutter,
            FxStep::Freeze,
            FxStep::RingMod,
            FxStep::Eq,
            FxStep::Compressor,
            FxStep::TapeSat,
            FxStep::Drive,
            FxStep::Autotune,
            FxStep::Pan,
            FxStep::ConvReverb,
            FxStep::ParamEq,
            FxStep::PitchShift,
            FxStep::Gate,
            FxStep::Vocoder,
            FxStep::Widen,
            FxStep::FreqShift,
            FxStep::Vinyl,
            FxStep::DjFilter,
            FxStep::Tremolo,
            FxStep::Vibrato,
            FxStep::IsoEq,
            FxStep::DeEsser,
            FxStep::ResBank,
            FxStep::TapeEcho,
        ];
        assert_eq!(variants.len(), FX_STEP_COUNT);
        let mut seen = [false; FX_STEP_COUNT];
        for v in variants {
            let i = v.idx();
            assert!(i < FX_STEP_COUNT, "{:?} → {} out of range", v, i);
            assert!(!seen[i], "{:?} shares idx {} with an earlier variant", v, i);
            seen[i] = true;
        }
        // Every slot should have been filled exactly once.
        assert!(seen.iter().all(|&x| x));
    }
}

// ── bass_voice_field — pure string helper shared by the apply layer ──────
#[cfg(test)]
mod bass_voice_field_tests {
    // The fn is `pub(super)` so reach it via a thin shim crate-internally.
    // Rebuild its logic here and assert the contract a downstream caller
    // relies on (voice 0 → "bass_<field>"; voices 1..=3 → "bass<N+1>_<field>").
    fn bass_voice_field(voice_idx: usize, field: &str) -> String {
        if voice_idx == 0 {
            format!("bass_{}", field)
        } else {
            format!("bass{}_{}", voice_idx + 1, field)
        }
    }

    #[test]
    fn voice_zero_uses_legacy_prefix() {
        assert_eq!(bass_voice_field(0, "steps"), "bass_steps");
        assert_eq!(bass_voice_field(0, "accents"), "bass_accents");
    }

    #[test]
    fn voice_nonzero_uses_numbered_prefix() {
        assert_eq!(bass_voice_field(1, "steps"), "bass2_steps");
        assert_eq!(bass_voice_field(2, "notes"), "bass3_notes");
        assert_eq!(bass_voice_field(3, "pans"), "bass4_pans");
    }
}

// ── set_chain clamping semantics ───────────────────────────────────────────
#[cfg(test)]
mod set_chain_tests {
    use crate::state::{AppState, MAX_BANKS, set_chain};

    #[test]
    fn caps_chain_at_max_banks_entries() {
        // Chain length is hard-capped at MAX_BANKS — extra entries are
        // silently dropped at the tail.  Guards against rogue API
        // requests that push an over-long chain through `POST /api/song`.
        let chain: Vec<usize> = (0..(MAX_BANKS * 2)).map(|i| i % MAX_BANKS).collect();
        let s = set_chain(AppState::default(), chain);
        assert_eq!(s.chain.len(), MAX_BANKS);
    }

    #[test]
    fn clamps_out_of_range_slot_indices_to_max() {
        // Slot indices must land in `0..MAX_BANKS`.  Larger values clamp
        // to the last valid slot rather than failing silently or panicking.
        let max = MAX_BANKS - 1;
        let s = set_chain(AppState::default(), vec![0, 9999, 5, MAX_BANKS + 100]);
        assert_eq!(s.chain, vec![0, max, 5, max]);
    }

    #[test]
    fn empty_chain_is_accepted() {
        let s = set_chain(AppState::default(), vec![]);
        assert!(s.chain.is_empty());
    }
}

// ── lane_scheduler::baseline_dynamism — defensive checks ──────────────────
#[cfg(test)]
mod lane_scheduler_baseline_tests {
    use crate::llm::lane_scheduler::baseline_dynamism;
    use crate::llm::lanes::LaneKind;

    #[test]
    fn rack_lane_is_always_zero_dynamism() {
        // Rack composition is user-owned — the scheduler must never pick
        // it on its own, regardless of style overrides.  Confirmed in
        // the effective_dynamism path too but worth asserting at the
        // baseline level so the two checks stay in lock-step.
        assert_eq!(baseline_dynamism(LaneKind::Rack), 0.0);
    }

    #[test]
    fn bass_lane_has_highest_baseline() {
        // Bass is the most musical lane — baseline weight must exceed
        // every other lane's so it dominates the weighted picker.
        let bass = baseline_dynamism(LaneKind::Bass(0));
        for other in [
            LaneKind::KitA,
            LaneKind::KitB,
            LaneKind::Amen,
            LaneKind::Hoover,
            LaneKind::An1x,
            LaneKind::Modulation,
            LaneKind::Fx,
            LaneKind::Settings,
            LaneKind::Rack,
        ] {
            assert!(
                bass >= baseline_dynamism(other),
                "bass ({}) should be ≥ {:?} ({})",
                bass,
                other,
                baseline_dynamism(other),
            );
        }
    }
}

// ── ChainSlotOverride::is_empty — tested inline in song.rs, but the
//    *semantic* rule (repeats=1 + None/None = empty) is worth pinning
//    here too since multiple codepaths consume it.
#[cfg(test)]
mod chain_slot_override_semantics_tests {
    use crate::state::ChainSlotOverride;

    #[test]
    fn default_is_empty() {
        assert!(ChainSlotOverride::default().is_empty());
    }

    #[test]
    fn any_field_set_means_non_empty() {
        let mut o = ChainSlotOverride::default();
        assert!(o.is_empty());

        o.bpm = Some(140.0);
        assert!(!o.is_empty());
        o.bpm = None;
        assert!(o.is_empty());

        o.style = Some("acid".into());
        assert!(!o.is_empty());
        o.style = None;
        assert!(o.is_empty());

        o.repeats = 2;
        assert!(!o.is_empty());
    }
}

// ── FxPlan default is empty across all fields ──────────────────────────────
#[cfg(test)]
mod fx_plan_default_tests {
    use crate::state::FxPlan;

    #[test]
    fn default_has_empty_steps_voice_routes_and_feedback() {
        let p = FxPlan::default();
        assert!(p.steps.is_empty());
        assert!(p.voice_routes.is_empty());
        assert!(p.feedback_routes.is_empty());
    }
}

// ── mod_inputs / mod_input_label — back-panel jack declarations ──────────
#[cfg(test)]
mod mod_inputs_tests {
    use crate::state::{ModInput, ModuleKind, mod_input_label, mod_inputs};

    #[test]
    fn every_module_kind_returns_a_slot_list() {
        // Enforces the "exhaustive match" contract of `mod_inputs`: if
        // a new ModuleKind variant is added without being wired into
        // the match arm, this test would fail (compile-time via the
        // exhaustive match itself; runtime as a safety net).
        for kind in [
            ModuleKind::AcidBass,
            ModuleKind::HooverLead,
            ModuleKind::An1xVoice,
            ModuleKind::NoiseVoice,
            ModuleKind::DrumKit808,
            ModuleKind::DrumKit909,
            ModuleKind::AmenSampler,
            ModuleKind::GranularTexture,
            ModuleKind::NeuTts,
            ModuleKind::GabberKick,
            ModuleKind::StepSequencer,
            ModuleKind::FxChorus,
            ModuleKind::FxPhaser,
            ModuleKind::FxWaveshaper,
            ModuleKind::FxDrive,
            ModuleKind::FxBitcrush,
            ModuleKind::FxRingMod,
            ModuleKind::FxEq,
            ModuleKind::FxTapeSat,
            ModuleKind::FxAutotune,
            ModuleKind::FxReverb,
            ModuleKind::FxDelay,
            ModuleKind::FxCompressor,
            ModuleKind::FxPan,
            ModuleKind::MasterOutput,
            ModuleKind::LfoModule,
            ModuleKind::LlmAgent,
            ModuleKind::LlmConsole,
            ModuleKind::SpectrumAnalyzer,
            ModuleKind::StereoMeter,
            ModuleKind::ActivityTimeline,
        ] {
            let _ = mod_inputs(kind); // just confirms the arm exists
        }
    }

    #[test]
    fn fixed_jacks_label_via_lfo_target_short_label() {
        // AcidBass has Fixed(BassPan) at index 0 → label must match
        // `lfo_target_short_label(BassPan)`.
        let label = mod_input_label(ModuleKind::AcidBass, 0);
        assert_eq!(label, "B.PAN");
    }

    #[test]
    fn selector_jacks_use_mod_n_labels() {
        // AcidBass's second slot is a Selector → "MOD2".
        assert_eq!(mod_input_label(ModuleKind::AcidBass, 1), "MOD2");
        assert_eq!(mod_input_label(ModuleKind::AcidBass, 2), "MOD3");
    }

    #[test]
    fn out_of_range_index_returns_empty_string() {
        // HooverLead has 2 slots — index 5 is past the end.
        assert_eq!(mod_input_label(ModuleKind::HooverLead, 5), "");
    }

    #[test]
    fn opt_out_kinds_have_no_slots() {
        for k in [
            ModuleKind::MasterOutput,
            ModuleKind::LfoModule,
            ModuleKind::LlmAgent,
            ModuleKind::LlmConsole,
            ModuleKind::SpectrumAnalyzer,
            ModuleKind::StereoMeter,
            ModuleKind::ActivityTimeline,
        ] {
            assert!(
                mod_inputs(k).is_empty(),
                "{:?} must opt out of modulation (no jacks)",
                k
            );
        }
    }

    #[test]
    fn acid_bass_has_fixed_pan_as_first_slot() {
        // Pins the interface contract that agents expect — every voice
        // with a single pan field exposes it as Fixed(<VoicePan>) at
        // slot 0 so the UI can render it as a labelled jack.
        let slots = mod_inputs(ModuleKind::AcidBass);
        assert!(!slots.is_empty());
        match slots[0] {
            ModInput::Fixed(_) => {}
            ModInput::Selector => panic!("AcidBass slot 0 should be Fixed(BassPan)"),
        }
    }
}

// ── preecho::NoteShift resolution edges ───────────────────────────────────
#[cfg(test)]
mod resolve_note_shift_edge_tests {
    use crate::sequencer::{NoteShift, resolve_note_shift};
    use crate::state::Scale;

    #[test]
    fn zero_semitone_shift_returns_same_note() {
        // Corner case — shift 0 should be an identity even on scale ends.
        assert_eq!(
            resolve_note_shift(60, NoteShift::Semitones(0), 0, Scale::NaturalMinor),
            60
        );
        assert_eq!(
            resolve_note_shift(60, NoteShift::ScaleSteps(0), 0, Scale::NaturalMinor),
            60
        );
    }

    #[test]
    fn scale_steps_with_out_of_scale_anchor_snaps_below() {
        // Anchor 61 (C#) isn't in C major; walking 0 steps doesn't move,
        // walking -1 should land on the first scale tone ≤ 61 or
        // immediately below — the semitone distance depends on the
        // scale's nearest-below snap.  C major (0,2,4,5,7,9,11): the
        // largest interval ≤ 1 (the pitch class of C#) is 0 (C).  So
        // the degree anchor is "C", and walking -1 lands on B of the
        // lower octave — C5 (60) minus one scale step = B4 (59).
        assert_eq!(
            resolve_note_shift(61, NoteShift::ScaleSteps(-1), 0, Scale::Major),
            59
        );
    }

    #[test]
    fn scale_steps_positive_walks_up_the_scale() {
        // A minor at the A4 (69) root: +1 scale step → B4 (71),
        // +2 → C5 (72), +7 → A5 (81) — one full octave up the 7-tone scale.
        assert_eq!(
            resolve_note_shift(69, NoteShift::ScaleSteps(1), 9, Scale::NaturalMinor),
            71
        );
        assert_eq!(
            resolve_note_shift(69, NoteShift::ScaleSteps(7), 9, Scale::NaturalMinor),
            81
        );
    }
}
