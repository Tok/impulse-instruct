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
        PluckString => &[Selector, Selector, Selector],
        WavetableVoice => &[Selector, Selector, Selector],
        SampleInstrument => &[Selector, Selector, Selector],
        An1xVoice => &[Fixed(An1xPan), Selector, Selector],
        NoiseVoice => &[Fixed(NoisePan), Selector],
        // Theremin: only the selectors — every meaningful target
        // (X / Y / portamento / brightness) is a continuous knob the
        // user reaches via mod-routes rather than a fixed jack.
        Theremin => &[Selector, Selector, Selector],
        // Pendulum: same as Theremin — selectors only, the user
        // routes LFOs to base_pitch / detune / mix from the back
        // panel without dedicated fixed jacks.
        Pendulum => &[Selector, Selector, Selector],
        // FM operator synth: 24+ per-op knobs is far too many for
        // dedicated jacks — selectors let the user route LFOs to
        // any combination of op level / ratio / ADSR fields.
        FmOpsVoice => &[Selector, Selector, Selector],
        // Additive: 16 per-harmonic levels + voice ADSR + vol +
        // pan — too many fixed jacks would be needed; selectors
        // let the user pick which fields to drive.
        AdditiveVoice => &[Selector, Selector, Selector],
        // Modal: same problem — 8 per-mode levels + voice fields
        // → selectors let the user pick which fields to drive.
        ModalVoice => &[Selector, Selector, Selector],
        // Chiptune: 3 oscs × 6 fields each + filter + flags →
        // selectors throughout.
        ChiptuneVoice => &[Selector, Selector, Selector],
        // Vocal: vowel + morph + brightness + shift + ADSR —
        // selectors so the user picks which to drive.
        VocalVoice => &[Selector, Selector, Selector],
        // ── Voices without a single pan (multi-voice kits / pan-less) ─────
        DrumKit808 | DrumKit909 | AmenSampler | GranularTexture | NeuTts => {
            &[Selector, Selector, Selector]
        }
        GabberKick => &[Selector, Selector, Selector],
        // ── Sequencer — BPM, swing, and a spare selector ──────────────────
        StepSequencer => &[Selector, Selector, Selector],
        // Vinyl: 3 knobs, all selectors so the user routes LFOs
        // to noise / wear / mix from the back panel.
        FxVinyl => &[Selector, Selector, Selector],
        // DJ filter: 3 knobs (morph / resonance / mix), all
        // selectors — auto-morph patches need an LFO on the morph
        // jack, but users may also want resonance riding the LFO.
        FxDjFilter => &[Selector, Selector, Selector],
        // Tremolo: 4 knobs (rate / depth / shape / mix); all
        // selectors so the user can route LFOs to any of them
        // (e.g. one LFO sweeping the rate for "speeding-up" effects).
        FxTremolo => &[Selector, Selector, Selector, Selector],
        // Vibrato: same 4 knobs as tremolo, same routing freedom.
        FxVibrato => &[Selector, Selector, Selector, Selector],
        // ISO EQ: 4 knobs (low / mid / high / mix); all selectors so
        // the user can sequence kill patterns via LFO routing.
        FxIsoEq => &[Selector, Selector, Selector, Selector],
        // De-esser: 4 knobs (freq / threshold / amount / mix); all
        // selectors — LFO on threshold gives a gating-style breath
        // patch.
        FxDeEsser => &[Selector, Selector, Selector, Selector],
        // ── FX ≤3 knobs → dedicated jack per knob ──────────────────────────
        FxChorus => &[Fixed(ChorusRate), Fixed(ChorusDepth), Fixed(ChorusMix)],
        FxPhaser => &[Fixed(PhaserRate), Fixed(PhaserDepth), Fixed(PhaserMix)],
        FxFlanger => &[
            Fixed(FlangerRate),
            Fixed(FlangerDepth),
            Fixed(FlangerFeedback),
            Fixed(FlangerMix),
        ],
        FxLimiter => &[
            Fixed(LimiterThreshold),
            Fixed(LimiterCeiling),
            Fixed(LimiterRelease),
        ],
        FxFilter => &[
            Fixed(SvfCutoff),
            Fixed(SvfResonance),
            Fixed(SvfDrive),
            Fixed(SvfMix),
        ],
        FxComb => &[
            Fixed(CombPitch),
            Fixed(CombFeedback),
            Fixed(CombDamp),
            Fixed(CombMix),
        ],
        FxTilt => &[Fixed(TiltTilt), Fixed(TiltPivot), Fixed(TiltMix)],
        FxTransient => &[
            Fixed(TransientAttack),
            Fixed(TransientSustain),
            Fixed(TransientMix),
        ],
        FxExciter => &[Fixed(ExciterAmount), Fixed(ExciterFreq), Fixed(ExciterMix)],
        FxMultitap => &[
            Fixed(MultitapTime),
            Fixed(MultitapSpread),
            Fixed(MultitapFeedback),
            Fixed(MultitapMix),
        ],
        FxRevDelay => &[
            Fixed(RevDelayTime),
            Fixed(RevDelayFeedback),
            Fixed(RevDelayMix),
        ],
        FxTapeStop => &[Fixed(TapeStopMix)],
        FxStutter => &[Fixed(StutterRate), Fixed(StutterSlice), Fixed(StutterMix)],
        FxFreeze => &[Fixed(FreezeMix)],
        FxWaveshaper => &[Fixed(WaveshaperDrive), Fixed(WaveshaperMix)],
        FxDrive => &[Fixed(DistortionDrive), Fixed(DistortionMix)],
        FxBitcrush => &[Fixed(BitcrushBits), Fixed(BitcrushRate), Fixed(BitcrushMix)],
        FxRingMod => &[Fixed(RingModFreq), Fixed(RingModMix)],
        FxEq => &[Fixed(EqLow), Fixed(EqMid), Fixed(EqHigh)],
        FxTapeSat => &[Fixed(TapeDrive), Fixed(TapeMix), Fixed(TapeFlutter)],
        FxAutotune => &[Fixed(AutotuneAmount), Fixed(AutotuneMix)],
        FxWiden => &[Fixed(WidenHaas), Fixed(WidenSide), Fixed(WidenMix)],
        FxFreqShift => &[
            Fixed(FreqShiftAmount),
            Fixed(FreqShiftFeedback),
            Fixed(FreqShiftMix),
        ],
        // ── FX >3 knobs → 3 selectors ──────────────────────────────────────
        FxReverb | FxDelay | FxCompressor | FxPan | FxConvReverb | FxParamEq | FxPitchShift => {
            &[Selector, Selector, Selector]
        }
        // Sidechain FX (Gate / Vocoder) — knob counts in the >3 bucket;
        // 3 selectors keep the back-panel uniform.  The sidechain audio
        // input is its own jack on the front (rendered separately from
        // these modulation jacks).
        FxGate | FxVocoder => &[Selector, Selector, Selector],
        // ── Explicit opt-outs ─────────────────────────────────────────────
        MasterOutput | LfoModule | LlmAgent | LlmConsole | SpectrumAnalyzer | StereoMeter
        | ActivityTimeline | BarOscilloscope | StereoVectorscope | LfoScope | PitchTracker
        | ChordDisplay | Spectrogram | LoudnessMeter | PhaseWheel | EventStream => &[],
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
        NeuTtsVolume => "TTS.VOL",
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
        FlangerRate => "FL.RT",
        FlangerDepth => "FL.DP",
        FlangerFeedback => "FL.FB",
        FlangerMix => "FL.MX",
        LimiterThreshold => "LM.TH",
        LimiterCeiling => "LM.CE",
        LimiterRelease => "LM.RL",
        LimiterLookahead => "LM.LA",
        SvfCutoff => "FT.CT",
        SvfResonance => "FT.RS",
        SvfDrive => "FT.DR",
        SvfMix => "FT.MX",
        CombPitch => "CB.PT",
        CombFeedback => "CB.FB",
        CombDamp => "CB.DP",
        CombMix => "CB.MX",
        TiltTilt => "TL.TL",
        TiltPivot => "TL.PV",
        TiltMix => "TL.MX",
        TransientAttack => "TR.AT",
        TransientSustain => "TR.SU",
        TransientMix => "TR.MX",
        ExciterAmount => "EX.AM",
        ExciterFreq => "EX.FQ",
        ExciterMix => "EX.MX",
        MultitapTime => "MT.TM",
        MultitapSpread => "MT.SP",
        MultitapFeedback => "MT.FB",
        MultitapMix => "MT.MX",
        RevDelayTime => "RD.TM",
        RevDelayFeedback => "RD.FB",
        RevDelayMix => "RD.MX",
        TapeStopMix => "TS.MX",
        StutterRate => "ST.RT",
        StutterSlice => "ST.SL",
        StutterMix => "ST.MX",
        FreezeMix => "FZ.MX",
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
        GateThreshold => "GT.TH",
        GateAttack => "GT.AT",
        GateRelease => "GT.RL",
        GateDepth => "GT.DP",
        GateMix => "GT.MX",
        VocoderBands => "VC.BD",
        VocoderCarrierMix => "VC.CR",
        VocoderSense => "VC.SN",
        VocoderMix => "VC.MX",
        WidenHaas => "WD.HS",
        WidenSide => "WD.SD",
        WidenMix => "WD.MX",
        FreqShiftAmount => "FS.AM",
        FreqShiftFeedback => "FS.FB",
        FreqShiftMix => "FS.MX",
        SampleVolume => "SP.VOL",
        SamplePan => "SP.PAN",
        SamplePitch => "SP.PIT",
        SampleCutoff => "SP.CUT",
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
        NeuTtsVolume => Some(ModuleKind::NeuTts),
        ReverbMix | ReverbSize | ReverbDamp => Some(ModuleKind::FxReverb),
        DelayTime | DelayFeedback | DelayMix => Some(ModuleKind::FxDelay),
        ChorusRate | ChorusDepth | ChorusMix => Some(ModuleKind::FxChorus),
        PhaserRate | PhaserDepth | PhaserMix => Some(ModuleKind::FxPhaser),
        FlangerRate | FlangerDepth | FlangerFeedback | FlangerMix => Some(ModuleKind::FxFlanger),
        LimiterThreshold | LimiterCeiling | LimiterRelease | LimiterLookahead => {
            Some(ModuleKind::FxLimiter)
        }
        SvfCutoff | SvfResonance | SvfDrive | SvfMix => Some(ModuleKind::FxFilter),
        CombPitch | CombFeedback | CombDamp | CombMix => Some(ModuleKind::FxComb),
        TiltTilt | TiltPivot | TiltMix => Some(ModuleKind::FxTilt),
        TransientAttack | TransientSustain | TransientMix => Some(ModuleKind::FxTransient),
        ExciterAmount | ExciterFreq | ExciterMix => Some(ModuleKind::FxExciter),
        MultitapTime | MultitapSpread | MultitapFeedback | MultitapMix => {
            Some(ModuleKind::FxMultitap)
        }
        RevDelayTime | RevDelayFeedback | RevDelayMix => Some(ModuleKind::FxRevDelay),
        TapeStopMix => Some(ModuleKind::FxTapeStop),
        StutterRate | StutterSlice | StutterMix => Some(ModuleKind::FxStutter),
        FreezeMix => Some(ModuleKind::FxFreeze),
        WaveshaperDrive | WaveshaperMix => Some(ModuleKind::FxWaveshaper),
        DistortionDrive | DistortionMix => Some(ModuleKind::FxDrive),
        BitcrushBits | BitcrushRate | BitcrushMix => Some(ModuleKind::FxBitcrush),
        RingModFreq | RingModMix => Some(ModuleKind::FxRingMod),
        EqLow | EqMid | EqHigh => Some(ModuleKind::FxEq),
        CompThresh | CompRatio | CompMix => Some(ModuleKind::FxCompressor),
        GateThreshold | GateAttack | GateRelease | GateDepth | GateMix => Some(ModuleKind::FxGate),
        VocoderBands | VocoderCarrierMix | VocoderSense | VocoderMix => Some(ModuleKind::FxVocoder),
        WidenHaas | WidenSide | WidenMix => Some(ModuleKind::FxWiden),
        FreqShiftAmount | FreqShiftFeedback | FreqShiftMix => Some(ModuleKind::FxFreqShift),
        SampleVolume | SamplePan | SamplePitch | SampleCutoff => Some(ModuleKind::SampleInstrument),
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
        "neuttsvolume" | "ttsvolume" => NeuTtsVolume,
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
        "flangerrate" => FlangerRate,
        "flangerdepth" => FlangerDepth,
        "flangerfeedback" => FlangerFeedback,
        "flangermix" => FlangerMix,
        "limiterthreshold" => LimiterThreshold,
        "limiterceiling" => LimiterCeiling,
        "limiterrelease" => LimiterRelease,
        "limiterlookahead" => LimiterLookahead,
        "svfcutoff" | "filtercutoff" => SvfCutoff,
        "svfresonance" | "filterresonance" => SvfResonance,
        "svfdrive" | "filterdrive" => SvfDrive,
        "svfmix" | "filtermix" => SvfMix,
        "combpitch" => CombPitch,
        "combfeedback" => CombFeedback,
        "combdamp" => CombDamp,
        "combmix" => CombMix,
        "tilttilt" | "tilt" => TiltTilt,
        "tiltpivot" => TiltPivot,
        "tiltmix" => TiltMix,
        "transientattack" => TransientAttack,
        "transientsustain" => TransientSustain,
        "transientmix" => TransientMix,
        "exciteramount" => ExciterAmount,
        "exciterfreq" => ExciterFreq,
        "excitermix" => ExciterMix,
        "multitaptime" => MultitapTime,
        "multitapspread" => MultitapSpread,
        "multitapfeedback" => MultitapFeedback,
        "multitapmix" => MultitapMix,
        "revdelaytime" => RevDelayTime,
        "revdelayfeedback" => RevDelayFeedback,
        "revdelaymix" => RevDelayMix,
        "tapestopmix" => TapeStopMix,
        "stutterrate" => StutterRate,
        "stutterslice" => StutterSlice,
        "stuttermix" => StutterMix,
        "freezemix" => FreezeMix,
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
        "gatethreshold" => GateThreshold,
        "gateattack" => GateAttack,
        "gaterelease" => GateRelease,
        "gatedepth" => GateDepth,
        "gatemix" => GateMix,
        "vocoderbands" => VocoderBands,
        "vocodercarriermix" => VocoderCarrierMix,
        "vocodersense" => VocoderSense,
        "vocodermix" => VocoderMix,
        "widenhaas" => WidenHaas,
        "widenside" => WidenSide,
        "widenmix" => WidenMix,
        "freqshiftamount" | "freqshift" => FreqShiftAmount,
        "freqshiftfeedback" => FreqShiftFeedback,
        "freqshiftmix" => FreqShiftMix,
        "samplevolume" | "samplevol" => SampleVolume,
        "samplepan" => SamplePan,
        "samplepitch" => SamplePitch,
        "samplecutoff" => SampleCutoff,
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
