// ─── state/modulation.rs ─────────────────────────────────────────────────────
// Per-knob modulation interface.
//
// Every `ModuleKind` declares a list of `ModInput` slots — jacks that appear on
// the back panel for patching LFO (or other CV) sources into specific knobs.
// The rule: every module either provides at least one mod input, or explicitly
// opts out with an empty slot list.  Adding a new `ModuleKind` variant forces
// the exhaustive match in `mod_inputs` to be updated, enforcing the interface.

use super::{LfoTarget, ModuleKind, PortKind};

/// A modulation-input jack on a module's back panel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModInput {
    /// Dedicated jack hard-wired to a specific target knob.
    Fixed(LfoTarget),
    /// Generic jack whose target is picked from a dropdown on the back panel.
    /// The selected target is stored in `RackModule.mod_selectors[index]`.
    Selector,
}

/// Return the declared modulation input slots for a module kind.  Exhaustive:
/// every kind must return (possibly empty) list.
pub fn mod_inputs(kind: ModuleKind) -> &'static [ModInput] {
    use ModInput::Selector;
    use ModuleKind::*;
    match kind {
        // ── Voices — many knobs, 3 generic selector jacks each ────────────
        AcidBass | DrumKit808 | DrumKit909 | HooverLead | An1xVoice | AmenSampler
        | GranularTexture | NeuTts => &[Selector, Selector, Selector],
        NoiseVoice => &[Selector, Selector],
        // ── Sequencer — BPM, swing, and one spare selector ────────────────
        StepSequencer => &[Selector, Selector, Selector],
        // ── FX — 2 selector jacks each (most have 2–3 knobs) ──────────────
        FxReverb | FxDelay | FxChorus | FxPhaser | FxRingMod | FxWaveshaper | FxBitcrush | FxEq
        | FxCompressor | FxTapeSat | FxDrive | FxAutotune => &[Selector, Selector],
        // ── Explicit opt-outs ─────────────────────────────────────────────
        MasterOutput | LfoModule | LlmAgent | LlmConsole | SpectrumAnalyzer | StereoMeter
        | ActivityTimeline => &[],
    }
}

/// Label for a mod-input jack at `index` on `kind`, shown next to the port
/// circle on the back panel.
pub fn mod_input_label(kind: ModuleKind, index: usize) -> String {
    match mod_inputs(kind).get(index) {
        Some(ModInput::Fixed(target)) => lfo_target_short_label(*target).to_string(),
        Some(ModInput::Selector) => format!("MOD{}", index + 1),
        None => String::new(),
    }
}

/// Short (≤6-char) label for a mod target, used on back-panel jacks.
pub fn lfo_target_short_label(target: LfoTarget) -> &'static str {
    use LfoTarget::*;
    match target {
        None => "—",
        BassCutoff => "B.CUT",
        BassResonance => "B.RES",
        BassPitch => "B.PIT",
        BassVolume => "B.VOL",
        ReverbMix => "RVMIX",
        DelayTime => "DLTIM",
        DelayFeedback => "DLFBK",
        ChorusMix => "CHMIX",
        ChorusRate => "CHRAT",
        Kick808Pitch => "K8PIT",
        PhaserRate => "PHRAT",
        PhaserDepth => "PHDEP",
        DistortionDrive => "DIST",
        MasterVolume => "MVOL",
        An1xCutoff => "A.CUT",
        An1xPitch => "A.PIT",
    }
}

/// Maps an `LfoTarget` to the rack `ModuleKind` it modulates.
/// Used to synthesise visual cables for active LFO slots.
pub(crate) fn lfo_target_module_kind(target: LfoTarget) -> Option<ModuleKind> {
    use LfoTarget::*;
    match target {
        None => Option::None,
        BassCutoff | BassResonance | BassPitch | BassVolume => Some(ModuleKind::AcidBass),
        Kick808Pitch => Some(ModuleKind::DrumKit808),
        ReverbMix => Some(ModuleKind::FxReverb),
        DelayTime | DelayFeedback => Some(ModuleKind::FxDelay),
        ChorusMix | ChorusRate => Some(ModuleKind::FxChorus),
        PhaserRate | PhaserDepth => Some(ModuleKind::FxPhaser),
        DistortionDrive => Some(ModuleKind::FxWaveshaper),
        MasterVolume => Some(ModuleKind::MasterOutput),
        An1xCutoff | An1xPitch => Some(ModuleKind::An1xVoice),
    }
}

/// `PortKind` emitted on a module's primary output.
pub(crate) fn rack_out_port_kind(kind: ModuleKind) -> PortKind {
    match kind {
        ModuleKind::LlmAgent => PortKind::Control,
        ModuleKind::LfoModule | ModuleKind::StepSequencer => PortKind::Cv,
        _ => PortKind::Audio,
    }
}
