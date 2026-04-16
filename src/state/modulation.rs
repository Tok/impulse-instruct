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
        GabberKick => &[Selector, Selector, Selector],
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
        FxReverb | FxDelay | FxCompressor | FxPan => &[Selector, Selector, Selector],
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
        Kick808Decay => "K8.DEC",
        Kick808Pan => "K8.PAN",
        Snare808Tone => "S8.TON",
        Snare808Decay => "S8.DEC",
        Snare808Pan => "S8.PAN",
        Hihat808Pan => "H8.PAN",
        Kick909Pitch => "K9.PIT",
        Kick909Decay => "K9.DEC",
        Kick909Pan => "K9.PAN",
        Snare909Tone => "S9.TON",
        Snare909Decay => "S9.DEC",
        Snare909Pan => "S9.PAN",
        Hihat909Pan => "H9.PAN",
        Clap909Decay => "C9.DEC",
        Clap909Pan => "C9.PAN",
        An1xCutoff => "A.CUT",
        An1xPitch => "A.PIT",
        An1xPan => "A.PAN",
        AmenVolume => "AM.VOL",
        AmenStart => "AM.STR",
        AmenGate => "AM.GTE",
        GranularVolume => "G.VOL",
        GranularDensity => "G.DEN",
        GranularGrain => "G.GRN",
        GranularPos => "G.POS",
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
        GabberKickPitch => "GK.PIT",
        GabberKickDecay => "GK.DEC",
        GabberKickClip => "GK.CLP",
        GabberKickPan => "GK.PAN",
        MasterVolume => "M.VOL",
        StereoWidth => "M.WID",
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
        Kick808Pitch | Kick808Decay | Kick808Pan | Snare808Tone | Snare808Decay | Snare808Pan
        | Hihat808Pan => Some(ModuleKind::DrumKit808),
        Kick909Pitch | Kick909Decay | Kick909Pan | Snare909Tone | Snare909Decay | Snare909Pan
        | Hihat909Pan | Clap909Decay | Clap909Pan => Some(ModuleKind::DrumKit909),
        An1xCutoff | An1xPitch | An1xPan => Some(ModuleKind::An1xVoice),
        AmenVolume | AmenStart | AmenGate => Some(ModuleKind::AmenSampler),
        GranularVolume | GranularDensity | GranularGrain | GranularPos => {
            Some(ModuleKind::GranularTexture)
        }
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
        GabberKickPitch | GabberKickDecay | GabberKickClip | GabberKickPan => {
            Some(ModuleKind::GabberKick)
        }
        MasterVolume | StereoWidth => Some(ModuleKind::MasterOutput),
    }
}

/// Case-insensitive parse of an `LfoTarget` variant name (e.g. "BassPan",
/// "reverbmix").  Used by the API + LLM action handlers.  Returns `None`
/// for unrecognised names; "none" / "" / "—" / "-" parse to `LfoTarget::None`.
pub fn parse_lfo_target(name: &str) -> Option<LfoTarget> {
    use LfoTarget::*;
    let n = name.trim().to_ascii_lowercase();
    Some(match n.as_str() {
        "none" | "" | "—" | "-" => None,
        "basscutoff" => BassCutoff,
        "bassresonance" => BassResonance,
        "basspitch" => BassPitch,
        "bassvolume" => BassVolume,
        "basspan" => BassPan,
        "hooverpan" => HooverPan,
        "noisepan" => NoisePan,
        "kick808pitch" => Kick808Pitch,
        "kick808decay" => Kick808Decay,
        "kick808pan" => Kick808Pan,
        "snare808tone" => Snare808Tone,
        "snare808decay" => Snare808Decay,
        "snare808pan" => Snare808Pan,
        "hihat808pan" => Hihat808Pan,
        "kick909pitch" => Kick909Pitch,
        "kick909decay" => Kick909Decay,
        "kick909pan" => Kick909Pan,
        "snare909tone" => Snare909Tone,
        "snare909decay" => Snare909Decay,
        "snare909pan" => Snare909Pan,
        "hihat909pan" => Hihat909Pan,
        "clap909decay" => Clap909Decay,
        "clap909pan" => Clap909Pan,
        "an1xcutoff" => An1xCutoff,
        "an1xpitch" => An1xPitch,
        "an1xpan" => An1xPan,
        "amenvolume" => AmenVolume,
        "amenstart" => AmenStart,
        "amengate" => AmenGate,
        "granularvolume" => GranularVolume,
        "granulardensity" => GranularDensity,
        "granulargrain" => GranularGrain,
        "granularpos" => GranularPos,
        "reverbmix" => ReverbMix,
        "reverbsize" => ReverbSize,
        "reverbdamp" => ReverbDamp,
        "delaytime" => DelayTime,
        "delayfeedback" => DelayFeedback,
        "delaymix" => DelayMix,
        "chorusrate" => ChorusRate,
        "chorusdepth" => ChorusDepth,
        "chorusmix" => ChorusMix,
        "phaserrate" => PhaserRate,
        "phaserdepth" => PhaserDepth,
        "phasermix" => PhaserMix,
        "waveshaperdrive" => WaveshaperDrive,
        "waveshapermix" => WaveshaperMix,
        "distortiondrive" => DistortionDrive,
        "distortionmix" => DistortionMix,
        "bitcrushbits" => BitcrushBits,
        "bitcrushrate" => BitcrushRate,
        "bitcrushmix" => BitcrushMix,
        "ringmodfreq" => RingModFreq,
        "ringmodmix" => RingModMix,
        "eqlow" => EqLow,
        "eqmid" => EqMid,
        "eqhigh" => EqHigh,
        "compthresh" => CompThresh,
        "compratio" => CompRatio,
        "compmix" => CompMix,
        "tapedrive" => TapeDrive,
        "tapemix" => TapeMix,
        "tapeflutter" => TapeFlutter,
        "autotuneamount" => AutotuneAmount,
        "autotunemix" => AutotuneMix,
        "gabberkickpitch" => GabberKickPitch,
        "gabberkickdecay" => GabberKickDecay,
        "gabberkickclip" => GabberKickClip,
        "gabberkickpan" => GabberKickPan,
        "mastervolume" => MasterVolume,
        "stereowidth" => StereoWidth,
        _ => return Option::None,
    })
}

/// Apply one `rack.mod_cable` JSON entry to the rack — patches the cable,
/// optionally sets depth, and optionally sets selector targets in one shot.
/// Each entry: { from_lfo: idx, to: <kind name>, slot: u8, depth?: 0..1,
/// targets?: [String] }.
pub fn apply_llm_mod_cable_entry(rack: &mut crate::state::RackState, v: &serde_json::Value) {
    use crate::state::{ModuleKind, PortDir, PortKind, PortRef};
    let lfo_idx = v.get("from_lfo").and_then(|x| x.as_u64()).unwrap_or(0) as usize;
    let to_name = v.get("to").and_then(|x| x.as_str());
    let slot = v.get("slot").and_then(|x| x.as_u64()).unwrap_or(0) as u8;
    let depth = v.get("depth").and_then(|x| x.as_f64()).map(|d| d as f32);
    let lfo_id = rack
        .modules
        .iter()
        .filter(|m| m.kind == ModuleKind::LfoModule)
        .nth(lfo_idx)
        .map(|m| m.id);
    let to_id = to_name.and_then(|n| {
        rack.modules
            .iter()
            .find(|m| crate::state::rack_kind_name_matches(m.kind, n))
            .map(|m| m.id)
    });
    let (Some(fid), Some(tid)) = (lfo_id, to_id) else {
        return;
    };
    rack.connect(
        PortRef {
            module_id: fid,
            dir: PortDir::Out,
            kind: PortKind::Cv,
            index: 0,
        },
        PortRef {
            module_id: tid,
            dir: PortDir::In,
            kind: PortKind::Mod,
            index: slot,
        },
    );
    let Some(m) = rack.modules.iter_mut().find(|m| m.id == tid) else {
        return;
    };
    if let Some(d) = depth {
        let idx = slot as usize;
        if m.mod_input_depths.len() <= idx {
            m.mod_input_depths.resize(idx + 1, 1.0);
        }
        m.mod_input_depths[idx] = d.clamp(0.0, 1.0);
    }
    if let Some(targets) = v.get("targets").and_then(|x| x.as_array()) {
        let parsed: Vec<LfoTarget> = targets
            .iter()
            .filter_map(|tv| tv.as_str())
            .filter_map(parse_lfo_target)
            .collect();
        let idx = slot as usize;
        if m.mod_selectors.len() <= idx {
            m.mod_selectors.resize(idx + 1, Vec::new());
        }
        m.mod_selectors[idx] = parsed;
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
