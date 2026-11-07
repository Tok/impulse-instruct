// ─── tests/rack_scope_tests.rs ───────────────────────────────────────────────
// Covers `parse_module_kind` / `rack_kind_name_matches` beyond the
// gabber-only smoke test already in rack_tests.rs.  The parser is the
// entry point for every LLM-emitted / HTTP-supplied module name, and its
// flex rules (snake_case + dashed + spaced + short aliases) are the most
// likely regression vector when we add new module kinds.
//
// Tests are grouped by: voice-kind aliases, FX-kind aliases, case / punct
// normalisation, unknown names (must return None), and the reverse
// matcher's "fx" catch-all plus kind-specific aliases.

use crate::state::ModuleKind;
use crate::state::rack_scope::{parse_module_kind, rack_kind_name_matches};

// ─── parse_module_kind — voice aliases ──────────────────────────────────────

#[test]
fn parse_module_kind_accepts_acid_bass_aliases() {
    assert_eq!(parse_module_kind("bass"), Some(ModuleKind::AcidBass));
    assert_eq!(parse_module_kind("AcidBass"), Some(ModuleKind::AcidBass));
    assert_eq!(parse_module_kind("acid_bass"), Some(ModuleKind::AcidBass));
    assert_eq!(parse_module_kind("303"), Some(ModuleKind::AcidBass));
}

#[test]
fn parse_module_kind_distinguishes_kit_a_and_kit_b() {
    // Canonical names + numeric aliases must NOT cross-map — feeding
    // "808" must never return DrumKit909.
    assert_eq!(parse_module_kind("808"), Some(ModuleKind::DrumKit808));
    assert_eq!(parse_module_kind("kit_a"), Some(ModuleKind::DrumKit808));
    assert_eq!(parse_module_kind("drum_a"), Some(ModuleKind::DrumKit808));

    assert_eq!(parse_module_kind("909"), Some(ModuleKind::DrumKit909));
    assert_eq!(parse_module_kind("kit_b"), Some(ModuleKind::DrumKit909));
    assert_eq!(parse_module_kind("drum_b"), Some(ModuleKind::DrumKit909));
}

#[test]
fn parse_module_kind_handles_hoover_an1x_amen_noise_granular() {
    assert_eq!(parse_module_kind("hoover"), Some(ModuleKind::HooverLead));
    assert_eq!(parse_module_kind("lead"), Some(ModuleKind::HooverLead));

    assert_eq!(parse_module_kind("an1x"), Some(ModuleKind::An1xVoice));
    assert_eq!(parse_module_kind("an-1x"), Some(ModuleKind::An1xVoice));
    assert_eq!(parse_module_kind("pad"), Some(ModuleKind::An1xVoice));

    assert_eq!(parse_module_kind("amen"), Some(ModuleKind::AmenSampler));
    assert_eq!(parse_module_kind("break"), Some(ModuleKind::AmenSampler));

    assert_eq!(parse_module_kind("noise"), Some(ModuleKind::NoiseVoice));
    assert_eq!(
        parse_module_kind("granular"),
        Some(ModuleKind::GranularTexture)
    );
    assert_eq!(
        parse_module_kind("texture"),
        Some(ModuleKind::GranularTexture)
    );
}

// ─── parse_module_kind — FX aliases ─────────────────────────────────────────

#[test]
fn parse_module_kind_handles_fx_aliases() {
    assert_eq!(parse_module_kind("reverb"), Some(ModuleKind::FxReverb));
    assert_eq!(parse_module_kind("verb"), Some(ModuleKind::FxReverb));

    assert_eq!(parse_module_kind("delay"), Some(ModuleKind::FxDelay));
    assert_eq!(parse_module_kind("echo"), Some(ModuleKind::FxDelay));

    assert_eq!(parse_module_kind("chorus"), Some(ModuleKind::FxChorus));
    assert_eq!(parse_module_kind("phaser"), Some(ModuleKind::FxPhaser));
    assert_eq!(parse_module_kind("flanger"), Some(ModuleKind::FxFlanger));
    assert_eq!(parse_module_kind("flange"), Some(ModuleKind::FxFlanger));
    assert_eq!(parse_module_kind("limiter"), Some(ModuleKind::FxLimiter));
    assert_eq!(parse_module_kind("filter"), Some(ModuleKind::FxFilter));
    assert_eq!(parse_module_kind("svf"), Some(ModuleKind::FxFilter));
    assert_eq!(parse_module_kind("comb"), Some(ModuleKind::FxComb));
    assert_eq!(parse_module_kind("tilt"), Some(ModuleKind::FxTilt));
    assert_eq!(
        parse_module_kind("transient"),
        Some(ModuleKind::FxTransient),
    );
    assert_eq!(parse_module_kind("exciter"), Some(ModuleKind::FxExciter));
    assert_eq!(parse_module_kind("multitap"), Some(ModuleKind::FxMultitap));
    assert_eq!(parse_module_kind("revdelay"), Some(ModuleKind::FxRevDelay));
    assert_eq!(parse_module_kind("tapestop"), Some(ModuleKind::FxTapeStop));
    assert_eq!(parse_module_kind("stutter"), Some(ModuleKind::FxStutter));
    assert_eq!(parse_module_kind("freeze"), Some(ModuleKind::FxFreeze));
    assert_eq!(parse_module_kind("eq"), Some(ModuleKind::FxEq));
    assert_eq!(
        parse_module_kind("compressor"),
        Some(ModuleKind::FxCompressor),
    );
    assert_eq!(parse_module_kind("drive"), Some(ModuleKind::FxDrive));
    assert_eq!(parse_module_kind("distortion"), Some(ModuleKind::FxDrive),);
    assert_eq!(parse_module_kind("bitcrush"), Some(ModuleKind::FxBitcrush));
    assert_eq!(parse_module_kind("lofi"), Some(ModuleKind::FxBitcrush));
    assert_eq!(parse_module_kind("ringmod"), Some(ModuleKind::FxRingMod));
    assert_eq!(parse_module_kind("autotune"), Some(ModuleKind::FxAutotune));
}

// ─── parse_module_kind — normalisation ──────────────────────────────────────

#[test]
fn parse_module_kind_is_case_insensitive() {
    assert_eq!(parse_module_kind("REVERB"), Some(ModuleKind::FxReverb));
    assert_eq!(parse_module_kind("Reverb"), Some(ModuleKind::FxReverb));
    assert_eq!(
        parse_module_kind("drumkit808"),
        Some(ModuleKind::DrumKit808),
    );
}

#[test]
fn parse_module_kind_strips_dashes_underscores_and_spaces() {
    // Same "ring mod" input in four flavours should all resolve.
    assert_eq!(parse_module_kind("ring mod"), Some(ModuleKind::FxRingMod));
    assert_eq!(parse_module_kind("ring-mod"), Some(ModuleKind::FxRingMod));
    assert_eq!(parse_module_kind("ring_mod"), Some(ModuleKind::FxRingMod));
    assert_eq!(parse_module_kind("Ring Mod"), Some(ModuleKind::FxRingMod));
}

#[test]
fn parse_module_kind_returns_none_for_unknown() {
    assert_eq!(parse_module_kind(""), None);
    assert_eq!(parse_module_kind("whoknows"), None);
    assert_eq!(parse_module_kind("super_synth"), None);
}

// ─── rack_kind_name_matches ─────────────────────────────────────────────────

#[test]
fn rack_kind_name_matches_fx_catchall_covers_all_fx_kinds() {
    // "fx" must match EVERY FxXxx kind — that's the LLM's fallback when
    // it doesn't want to be specific.  Non-FX kinds must reject "fx".
    let fx_kinds = [
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
        ModuleKind::FxFreeze,
        ModuleKind::FxRingMod,
        ModuleKind::FxWaveshaper,
        ModuleKind::FxBitcrush,
        ModuleKind::FxEq,
        ModuleKind::FxCompressor,
        ModuleKind::FxTapeSat,
        ModuleKind::FxDrive,
        ModuleKind::FxAutotune,
        ModuleKind::FxPan,
    ];
    for kind in fx_kinds {
        assert!(
            rack_kind_name_matches(kind, "fx"),
            "{kind:?} must match \"fx\"",
        );
    }
    // Voice kinds should NOT match "fx".
    assert!(!rack_kind_name_matches(ModuleKind::AcidBass, "fx"));
    assert!(!rack_kind_name_matches(ModuleKind::DrumKit808, "fx"));
    assert!(!rack_kind_name_matches(ModuleKind::HooverLead, "fx"));
}

#[test]
fn rack_kind_name_matches_lfo_does_not_match_fx() {
    // LfoModule sits near FX visually but it isn't an audio-FX kind — it
    // must NOT match the "fx" catch-all.
    assert!(rack_kind_name_matches(ModuleKind::LfoModule, "lfo"));
    assert!(!rack_kind_name_matches(ModuleKind::LfoModule, "fx"));
}

#[test]
fn rack_kind_name_matches_drum_kits_accept_aliases() {
    assert!(rack_kind_name_matches(ModuleKind::DrumKit808, "808"));
    assert!(rack_kind_name_matches(ModuleKind::DrumKit808, "kit_a"));
    assert!(rack_kind_name_matches(ModuleKind::DrumKit808, "drums_a"));
    assert!(!rack_kind_name_matches(ModuleKind::DrumKit808, "909"));

    assert!(rack_kind_name_matches(ModuleKind::DrumKit909, "909"));
    assert!(rack_kind_name_matches(ModuleKind::DrumKit909, "kit_b"));
    assert!(!rack_kind_name_matches(ModuleKind::DrumKit909, "808"));
}

#[test]
fn rack_kind_name_matches_master_output_accepts_short_forms() {
    assert!(rack_kind_name_matches(ModuleKind::MasterOutput, "master"));
    assert!(rack_kind_name_matches(ModuleKind::MasterOutput, "out"));
    assert!(rack_kind_name_matches(ModuleKind::MasterOutput, "output"));
    assert!(!rack_kind_name_matches(ModuleKind::MasterOutput, "fx"));
}

#[test]
fn rack_kind_name_matches_ignores_case() {
    assert!(rack_kind_name_matches(ModuleKind::FxReverb, "REVERB"));
    assert!(rack_kind_name_matches(ModuleKind::FxReverb, "Reverb"));
    assert!(rack_kind_name_matches(ModuleKind::AcidBass, "BASS"));
}

#[test]
fn rack_kind_name_matches_rejects_unrelated_names() {
    // Bass doesn't match "reverb"; reverb doesn't match "bass" (even
    // though both are catch-all-able by "fx" / voice category).
    assert!(!rack_kind_name_matches(ModuleKind::AcidBass, "reverb"));
    assert!(!rack_kind_name_matches(ModuleKind::FxReverb, "bass"));
    assert!(!rack_kind_name_matches(ModuleKind::HooverLead, "bass"));
}
