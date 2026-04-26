// ─── tests/fx_step_idx_tests.rs ──────────────────────────────────────────────
// Covers two small dispatchers the audio thread relies on for dense,
// hash-free indexing into `[f32; FX_STEP_COUNT]` arrays:
//   • `FxStep::idx` — stable 0..FX_STEP_COUNT mapping.  A collision or
//     a value ≥ FX_STEP_COUNT would cause the audio thread to write
//     feedback-buffer data to the wrong step, or panic on bounds
//     check.
//   • `fx_plan::kind_to_fx_step` — pure ModuleKind → FxStep dispatch.
//     Every `Fx*` kind must map to the matching step; non-FX kinds
//     must return None so the rack's cycle check can scope feedback
//     permissions to FX-only cycles.

use crate::state::{FX_STEP_COUNT, FxStep, ModuleKind, fx_plan};

// ─── FxStep::idx ────────────────────────────────────────────────────────────

const ALL_FX_STEPS: &[FxStep] = &[
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
];

#[test]
fn every_fx_step_idx_lives_inside_the_dense_array_bounds() {
    // The whole point of `idx` is to index a `[T; FX_STEP_COUNT]`.  An
    // out-of-range index would trip a bounds-check panic in the audio
    // callback — a real-time show-stopper.
    for step in ALL_FX_STEPS {
        assert!(
            step.idx() < FX_STEP_COUNT,
            "{step:?}.idx() = {} exceeds FX_STEP_COUNT ({FX_STEP_COUNT})",
            step.idx(),
        );
    }
}

#[test]
fn fx_step_idx_produces_distinct_values_for_each_variant() {
    // Two variants mapping to the same index would corrupt the audio
    // thread's feedback-line cache (writes for step A overwrite step
    // B's state).  Must be a bijection on 0..FX_STEP_COUNT.
    let mut seen = std::collections::HashSet::new();
    for step in ALL_FX_STEPS {
        let idx = step.idx();
        assert!(
            seen.insert(idx),
            "idx collision at {idx}: {step:?} clashes with an earlier variant",
        );
    }
    // And we must hit every slot, not a sparse subset.
    assert_eq!(
        seen.len(),
        FX_STEP_COUNT,
        "FxStep::idx should cover every 0..FX_STEP_COUNT slot exactly once",
    );
}

#[test]
fn fx_step_idx_well_known_mappings_are_stable() {
    // Spot-check the three earliest variants; reordering these would
    // invalidate every saved feedback-graph.
    assert_eq!(FxStep::Waveshaper.idx(), 0);
    assert_eq!(FxStep::Reverb.idx(), 1);
    assert_eq!(FxStep::Delay.idx(), 2);
}

// ─── kind_to_fx_step ────────────────────────────────────────────────────────

#[test]
fn kind_to_fx_step_maps_every_fx_kind_to_its_matching_step() {
    // Every Fx* ModuleKind must resolve to the corresponding FxStep.
    // A mis-mapping here would route an FxDelay module's audio through
    // (say) the Reverb processor at compile_fx_plan time.
    for (kind, step) in [
        (ModuleKind::FxWaveshaper, FxStep::Waveshaper),
        (ModuleKind::FxReverb, FxStep::Reverb),
        (ModuleKind::FxDelay, FxStep::Delay),
        (ModuleKind::FxBitcrush, FxStep::Bitcrush),
        (ModuleKind::FxChorus, FxStep::Chorus),
        (ModuleKind::FxPhaser, FxStep::Phaser),
        (ModuleKind::FxFlanger, FxStep::Flanger),
        (ModuleKind::FxLimiter, FxStep::Limiter),
        (ModuleKind::FxFilter, FxStep::Filter),
        (ModuleKind::FxComb, FxStep::Comb),
        (ModuleKind::FxTilt, FxStep::Tilt),
        (ModuleKind::FxTransient, FxStep::Transient),
        (ModuleKind::FxExciter, FxStep::Exciter),
        (ModuleKind::FxMultitap, FxStep::Multitap),
        (ModuleKind::FxRevDelay, FxStep::RevDelay),
        (ModuleKind::FxTapeStop, FxStep::TapeStop),
        (ModuleKind::FxStutter, FxStep::Stutter),
        (ModuleKind::FxFreeze, FxStep::Freeze),
        (ModuleKind::FxRingMod, FxStep::RingMod),
        (ModuleKind::FxEq, FxStep::Eq),
        (ModuleKind::FxCompressor, FxStep::Compressor),
        (ModuleKind::FxTapeSat, FxStep::TapeSat),
        (ModuleKind::FxDrive, FxStep::Drive),
        (ModuleKind::FxAutotune, FxStep::Autotune),
        (ModuleKind::FxPan, FxStep::Pan),
        (ModuleKind::FxConvReverb, FxStep::ConvReverb),
        (ModuleKind::FxParamEq, FxStep::ParamEq),
        (ModuleKind::FxPitchShift, FxStep::PitchShift),
    ] {
        assert_eq!(
            fx_plan::kind_to_fx_step(kind),
            Some(step),
            "{kind:?} should map to {step:?}",
        );
    }
}

#[test]
fn kind_to_fx_step_returns_none_for_non_fx_kinds() {
    for kind in [
        ModuleKind::AcidBass,
        ModuleKind::DrumKit808,
        ModuleKind::DrumKit909,
        ModuleKind::HooverLead,
        ModuleKind::An1xVoice,
        ModuleKind::AmenSampler,
        ModuleKind::NoiseVoice,
        ModuleKind::GabberKick,
        ModuleKind::MasterOutput,
        ModuleKind::StepSequencer,
        ModuleKind::LlmAgent,
        ModuleKind::LlmConsole,
        ModuleKind::LfoModule,
        ModuleKind::NeuTts,
    ] {
        assert!(
            fx_plan::kind_to_fx_step(kind).is_none(),
            "{kind:?} must NOT resolve to an FxStep",
        );
    }
}

#[test]
fn kind_to_fx_step_agrees_with_kind_is_fx() {
    // The two helpers should never disagree — `kind_is_fx` is defined
    // as `kind_to_fx_step(k).is_some()`, so a future refactor that
    // forgets to update one must trip this invariant.
    for kind in [
        ModuleKind::FxReverb,
        ModuleKind::FxDelay,
        ModuleKind::AcidBass,
        ModuleKind::MasterOutput,
        ModuleKind::LfoModule,
    ] {
        assert_eq!(
            fx_plan::kind_to_fx_step(kind).is_some(),
            fx_plan::kind_is_fx(kind),
            "kind_to_fx_step and kind_is_fx disagree on {kind:?}",
        );
    }
}
