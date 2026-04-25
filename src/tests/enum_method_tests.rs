// ─── tests/enum_method_tests.rs ──────────────────────────────────────────────
// Grab-bag of tests for small pure methods on various state enums that
// didn't have dedicated coverage:
//   • `Scale::intervals` — semitone-offset tables.  Off-by-one here
//     would make every melodic voice play the wrong scale.
//   • `Scale::name` — display strings.
//   • `BassLfoTarget::label` / `An1xLfoTarget::label` — UI display.
//   • `An1xLfoTarget::next` — UI click-cycle through the three targets.
//   • `ModuleKind::label` / `default_zone` / `allows_multiple` — rack
//     placement + duplication rules.

use crate::state::{An1xLfoTarget, BassLfoTarget, ModuleKind, Scale, Zone};

// ─── Scale::intervals ───────────────────────────────────────────────────────

#[test]
fn scale_intervals_are_in_range_and_non_decreasing() {
    // Every interval must be in 0..12 (semitones from root) and the
    // list must be sorted ascending — the snap_to_scale / scale_degree
    // helpers walk the list assuming ordered distinct values.
    for scale in Scale::all() {
        let ivals = scale.intervals();
        assert!(!ivals.is_empty(), "{scale:?} intervals must not be empty");
        assert_eq!(
            ivals[0], 0,
            "{scale:?} must start at root (0), got {}",
            ivals[0]
        );
        for pair in ivals.windows(2) {
            assert!(
                pair[1] > pair[0],
                "{scale:?} intervals must be strictly ascending, got {pair:?}",
            );
        }
        for &i in ivals {
            assert!(i < 12, "{scale:?} has interval {i} ≥ 12 (out of octave)");
        }
    }
}

#[test]
fn scale_intervals_match_classical_shapes() {
    // Spot-check three canonical modes whose interval shapes are fixed
    // by centuries of music theory.  Any drift here is a bug.
    assert_eq!(Scale::Major.intervals(), &[0, 2, 4, 5, 7, 9, 11]);
    assert_eq!(Scale::NaturalMinor.intervals(), &[0, 2, 3, 5, 7, 8, 10]);
    assert_eq!(
        Scale::Chromatic.intervals(),
        &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11]
    );
}

#[test]
fn pentatonic_has_five_notes_and_blues_has_six() {
    // The non-diatonic scales have different lengths than the standard
    // 7-note modes — lock the length invariant.
    assert_eq!(Scale::Pentatonic.intervals().len(), 5);
    assert_eq!(Scale::Blues.intervals().len(), 6);
}

// ─── Scale::name — short display strings ────────────────────────────────────

#[test]
fn scale_names_are_short_and_non_empty() {
    // Name is shown in a small dropdown; keep short and non-empty.
    for scale in Scale::all() {
        let n = scale.name();
        assert!(!n.is_empty(), "{scale:?} display name must not be empty");
        assert!(
            n.len() <= 12,
            "{scale:?} name {n:?} is too long for the UI dropdown",
        );
    }
}

// ─── BassLfoTarget::label ───────────────────────────────────────────────────

#[test]
fn bass_lfo_target_labels_are_short_and_distinct() {
    let labels: std::collections::HashSet<&'static str> = [
        BassLfoTarget::Off,
        BassLfoTarget::Pitch,
        BassLfoTarget::PulseWidth,
        BassLfoTarget::FilterCutoff,
        BassLfoTarget::Amplitude,
    ]
    .iter()
    .map(|t| t.label())
    .collect();
    assert_eq!(labels.len(), 5, "BassLfoTarget labels must all be distinct");
    assert!(
        labels.iter().all(|l| !l.is_empty() && l.len() <= 8),
        "labels must be short (≤8 chars) for the per-voice LFO strip",
    );
}

// ─── An1xLfoTarget::next ────────────────────────────────────────────────────

#[test]
fn an1x_lfo_target_next_cycles_in_three_steps() {
    // The UI's click-cycle must land back on the starting variant
    // after exactly three steps; any shorter cycle skips a target.
    let start = An1xLfoTarget::Pitch;
    let step1 = start.next();
    let step2 = step1.next();
    let step3 = step2.next();
    assert_eq!(step1, An1xLfoTarget::FilterCutoff);
    assert_eq!(step2, An1xLfoTarget::Amplitude);
    assert_eq!(step3, An1xLfoTarget::Pitch, "cycle must return to start");
}

#[test]
fn an1x_lfo_target_labels_are_distinct() {
    let labels: std::collections::HashSet<&'static str> = [
        An1xLfoTarget::Pitch,
        An1xLfoTarget::FilterCutoff,
        An1xLfoTarget::Amplitude,
    ]
    .iter()
    .map(|t| t.label())
    .collect();
    assert_eq!(labels.len(), 3);
}

// ─── ModuleKind::label ──────────────────────────────────────────────────────

#[test]
fn module_kind_labels_are_non_empty_uppercase_and_distinct() {
    // Used as the module card title — must be non-empty and
    // distinctive enough to identify the module at a glance.
    let kinds = [
        ModuleKind::AcidBass,
        ModuleKind::DrumKit808,
        ModuleKind::DrumKit909,
        ModuleKind::FxReverb,
        ModuleKind::FxDelay,
        ModuleKind::LfoModule,
        ModuleKind::LlmAgent,
        ModuleKind::LlmConsole,
        ModuleKind::MasterOutput,
        ModuleKind::NeuTts,
    ];
    let mut labels = std::collections::HashSet::new();
    for k in kinds {
        let l = k.label();
        assert!(!l.is_empty(), "{k:?} label must not be empty");
        assert!(
            l.chars().any(|c| c.is_ascii_uppercase()),
            "{k:?} label should be uppercase: {l:?}",
        );
        assert!(labels.insert(l), "duplicate module label {l:?} for {k:?}");
    }
}

// ─── ModuleKind::default_zone ───────────────────────────────────────────────

#[test]
fn default_zones_place_modules_in_the_expected_zone() {
    // Zone placement drives the rack's vertical stripe layout.  Test
    // one representative per zone so regressions in the big match stand
    // out.
    assert_eq!(ModuleKind::LlmConsole.default_zone(), Zone::Ai);
    assert_eq!(ModuleKind::LlmAgent.default_zone(), Zone::Ai);
    assert_eq!(ModuleKind::StepSequencer.default_zone(), Zone::Global);
    assert_eq!(ModuleKind::MasterOutput.default_zone(), Zone::Global);
    assert_eq!(ModuleKind::AcidBass.default_zone(), Zone::Voice);
    assert_eq!(ModuleKind::DrumKit808.default_zone(), Zone::Voice);
    assert_eq!(ModuleKind::FxReverb.default_zone(), Zone::FxMod);
    assert_eq!(ModuleKind::LfoModule.default_zone(), Zone::FxMod);
}

// ─── ModuleKind::allows_multiple ────────────────────────────────────────────

#[test]
fn allows_multiple_is_true_for_fx_lfo_and_agents() {
    // FX modules + LFO slots + LLM agents are all rack elements users
    // routinely duplicate.  A false here would silently refuse the
    // second instance.
    for k in [
        ModuleKind::FxReverb,
        ModuleKind::FxDelay,
        ModuleKind::FxChorus,
        ModuleKind::FxPhaser,
        ModuleKind::FxFlanger,
        ModuleKind::FxLimiter,
        ModuleKind::FxFilter,
        ModuleKind::FxComb,
        ModuleKind::FxTilt,
        ModuleKind::FxTransient,
        ModuleKind::FxExciter,
        ModuleKind::FxMultitap,
        ModuleKind::FxRevDelay,
        ModuleKind::FxTapeStop,
        ModuleKind::FxStutter,
        ModuleKind::FxRingMod,
        ModuleKind::FxWaveshaper,
        ModuleKind::FxBitcrush,
        ModuleKind::FxEq,
        ModuleKind::FxCompressor,
        ModuleKind::FxTapeSat,
        ModuleKind::FxDrive,
        ModuleKind::FxAutotune,
        ModuleKind::FxPan,
        ModuleKind::LfoModule,
        ModuleKind::LlmAgent,
    ] {
        assert!(
            k.allows_multiple(),
            "{k:?} should allow multiple rack instances",
        );
    }
}

#[test]
fn allows_multiple_is_false_for_singletons() {
    // Voice modules, sequencer, console, and master must be unique —
    // "add a second MasterOutput" makes no sense.
    for k in [
        ModuleKind::AcidBass,
        ModuleKind::DrumKit808,
        ModuleKind::DrumKit909,
        ModuleKind::HooverLead,
        ModuleKind::An1xVoice,
        ModuleKind::AmenSampler,
        ModuleKind::NoiseVoice,
        ModuleKind::GranularTexture,
        ModuleKind::GabberKick,
        ModuleKind::NeuTts,
        ModuleKind::StepSequencer,
        ModuleKind::MasterOutput,
        ModuleKind::LlmConsole,
    ] {
        assert!(
            !k.allows_multiple(),
            "{k:?} should be a singleton (only one instance per rack)",
        );
    }
}

// ─── Visualiser modules ─────────────────────────────────────────────────────

#[test]
fn viz_modules_default_to_fxmod_and_carry_labels() {
    // BarOscilloscope and EventStream were lifted from the always-on
    // header into rack-placeable visualisers.  Lock the zone +
    // distinct labels so future enum additions don't quietly steal
    // the same label or land in the wrong zone.
    for k in [
        ModuleKind::BarOscilloscope,
        ModuleKind::StereoVectorscope,
        ModuleKind::LfoScope,
        ModuleKind::PitchTracker,
        ModuleKind::ChordDisplay,
        ModuleKind::Spectrogram,
        ModuleKind::LoudnessMeter,
        ModuleKind::PhaseWheel,
        ModuleKind::EventStream,
    ] {
        assert_eq!(k.default_zone(), Zone::FxMod, "{k:?} expected in FxMod");
        let l = k.label();
        assert!(!l.is_empty(), "{k:?} label must not be empty");
    }
    // All viz labels must be distinct so the title bar never repeats
    // across two different module kinds.
    let labels = [
        ModuleKind::BarOscilloscope.label(),
        ModuleKind::StereoVectorscope.label(),
        ModuleKind::LfoScope.label(),
        ModuleKind::PitchTracker.label(),
        ModuleKind::ChordDisplay.label(),
        ModuleKind::Spectrogram.label(),
        ModuleKind::LoudnessMeter.label(),
        ModuleKind::PhaseWheel.label(),
        ModuleKind::EventStream.label(),
    ];
    for i in 0..labels.len() {
        for j in (i + 1)..labels.len() {
            assert_ne!(labels[i], labels[j], "viz module labels must be distinct");
        }
    }
}

#[test]
fn viz_modules_have_no_audio_or_mod_io() {
    // Visualisers don't produce audio (no MASTER reach indicator) and
    // don't expose mod-input jacks (nothing to modulate).
    for k in [
        ModuleKind::BarOscilloscope,
        ModuleKind::StereoVectorscope,
        ModuleKind::LfoScope,
        ModuleKind::PitchTracker,
        ModuleKind::ChordDisplay,
        ModuleKind::Spectrogram,
        ModuleKind::LoudnessMeter,
        ModuleKind::PhaseWheel,
        ModuleKind::EventStream,
    ] {
        assert!(!k.has_audio_output(), "{k:?} must not produce audio");
        assert!(
            crate::state::mod_inputs(k).is_empty(),
            "{k:?} must not declare mod inputs"
        );
    }
}
