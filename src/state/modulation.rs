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
    use LfoTarget::*;
    use ModInput::{Fixed, Selector};
    use ModuleKind::*;
    match kind {
        // ── Voices with a single pan field — Fixed(Pan) + 2 selectors ─────
        AcidBass => &[Fixed(BassPan), Selector, Selector],
        HooverLead => &[Fixed(HooverPan), Selector],
        An1xVoice => &[Fixed(An1xPan), Selector, Selector],
        NoiseVoice => &[Fixed(NoisePan), Selector],
        // ── Voices without a single pan (multi-voice kits / pan-less) ─────
        DrumKit808 | DrumKit909 | AmenSampler | GranularTexture | NeuTts => {
            &[Selector, Selector, Selector]
        }
        // ── Sequencer — BPM, swing, and a spare selector ──────────────────
        StepSequencer => &[Selector, Selector, Selector],
        // ── FX ≤3 knobs → dedicated jack per knob ──────────────────────────
        FxChorus => &[Fixed(ChorusRate), Fixed(ChorusDepth), Fixed(ChorusMix)],
        FxPhaser => &[Fixed(PhaserRate), Fixed(PhaserDepth), Fixed(PhaserMix)],
        FxWaveshaper => &[Fixed(WaveshaperDrive), Fixed(WaveshaperMix)],
        FxDrive => &[Fixed(DistortionDrive), Fixed(DistortionMix)],
        FxBitcrush => &[Fixed(BitcrushBits), Fixed(BitcrushRate), Fixed(BitcrushMix)],
        FxRingMod => &[Fixed(RingModFreq), Fixed(RingModMix)],
        FxEq => &[Fixed(EqLow), Fixed(EqMid), Fixed(EqHigh)],
        FxTapeSat => &[Fixed(TapeDrive), Fixed(TapeMix), Fixed(TapeFlutter)],
        FxAutotune => &[Fixed(AutotuneAmount), Fixed(AutotuneMix)],
        // ── FX >3 knobs → 3 selectors ──────────────────────────────────────
        FxReverb | FxDelay | FxCompressor => &[Selector, Selector, Selector],
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
        BassPan => "B.PAN",
        HooverPan => "H.PAN",
        NoisePan => "N.PAN",
        Kick808Pitch => "K8.PIT",
        Kick808Pan => "K8.PAN",
        Snare808Pan => "S8.PAN",
        Hihat808Pan => "H8.PAN",
        Kick909Pan => "K9.PAN",
        Snare909Pan => "S9.PAN",
        Hihat909Pan => "H9.PAN",
        Clap909Pan => "C9.PAN",
        An1xCutoff => "A.CUT",
        An1xPitch => "A.PIT",
        An1xPan => "A.PAN",
        ReverbMix => "RVB.MX",
        ReverbSize => "RVB.SZ",
        ReverbDamp => "RVB.DP",
        DelayTime => "DLY.TM",
        DelayFeedback => "DLY.FB",
        DelayMix => "DLY.MX",
        ChorusRate => "CH.RT",
        ChorusDepth => "CH.DP",
        ChorusMix => "CH.MX",
        PhaserRate => "PH.RT",
        PhaserDepth => "PH.DP",
        PhaserMix => "PH.MX",
        WaveshaperDrive => "WS.DR",
        WaveshaperMix => "WS.MX",
        DistortionDrive => "DR.DR",
        DistortionMix => "DR.MX",
        BitcrushBits => "BC.BT",
        BitcrushRate => "BC.RT",
        BitcrushMix => "BC.MX",
        RingModFreq => "RM.FQ",
        RingModMix => "RM.MX",
        EqLow => "EQ.LO",
        EqMid => "EQ.MD",
        EqHigh => "EQ.HI",
        CompThresh => "CP.TH",
        CompRatio => "CP.RT",
        CompMix => "CP.MX",
        TapeDrive => "TP.DR",
        TapeMix => "TP.MX",
        TapeFlutter => "TP.FL",
        AutotuneAmount => "AT.AM",
        AutotuneMix => "AT.MX",
        MasterVolume => "M.VOL",
    }
}

/// Maps an `LfoTarget` to the rack `ModuleKind` it modulates.
/// Used to synthesise visual cables for active LFO slots.
pub(crate) fn lfo_target_module_kind(target: LfoTarget) -> Option<ModuleKind> {
    use LfoTarget::*;
    match target {
        None => Option::None,
        BassCutoff | BassResonance | BassPitch | BassVolume | BassPan => Some(ModuleKind::AcidBass),
        HooverPan => Some(ModuleKind::HooverLead),
        NoisePan => Some(ModuleKind::NoiseVoice),
        Kick808Pitch | Kick808Pan | Snare808Pan | Hihat808Pan => Some(ModuleKind::DrumKit808),
        Kick909Pan | Snare909Pan | Hihat909Pan | Clap909Pan => Some(ModuleKind::DrumKit909),
        An1xCutoff | An1xPitch | An1xPan => Some(ModuleKind::An1xVoice),
        ReverbMix | ReverbSize | ReverbDamp => Some(ModuleKind::FxReverb),
        DelayTime | DelayFeedback | DelayMix => Some(ModuleKind::FxDelay),
        ChorusRate | ChorusDepth | ChorusMix => Some(ModuleKind::FxChorus),
        PhaserRate | PhaserDepth | PhaserMix => Some(ModuleKind::FxPhaser),
        WaveshaperDrive | WaveshaperMix => Some(ModuleKind::FxWaveshaper),
        DistortionDrive | DistortionMix => Some(ModuleKind::FxDrive),
        BitcrushBits | BitcrushRate | BitcrushMix => Some(ModuleKind::FxBitcrush),
        RingModFreq | RingModMix => Some(ModuleKind::FxRingMod),
        EqLow | EqMid | EqHigh => Some(ModuleKind::FxEq),
        CompThresh | CompRatio | CompMix => Some(ModuleKind::FxCompressor),
        TapeDrive | TapeMix | TapeFlutter => Some(ModuleKind::FxTapeSat),
        AutotuneAmount | AutotuneMix => Some(ModuleKind::FxAutotune),
        MasterVolume => Some(ModuleKind::MasterOutput),
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
