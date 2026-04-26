// ─── state/rack_scope.rs ─────────────────────────────────────────────────────
// Scope resolution and name matching for rack modules.
// Split from rack.rs to stay under the 1000-line limit.

use super::rack::{ModuleKind, PortDir, PortKind, RackState};

/// Derive an agent's scope from its outgoing Control cables.
/// Returns the scope strings (e.g. ["bass", "kit_a", "fx"]) for the target modules.
/// Empty result means no control cables → agent controls everything.
pub fn scope_from_control_cables(rack: &RackState, agent_id: u32) -> Vec<String> {
    let targets: Vec<u32> = rack
        .cables
        .iter()
        .filter(|c| {
            c.from.module_id == agent_id
                && c.from.kind == PortKind::Control
                && c.from.dir == PortDir::Out
        })
        .map(|c| c.to.module_id)
        .collect();
    if targets.is_empty() {
        return Vec::new(); // no cables = controls everything
    }
    let mut scope = Vec::new();
    for tid in &targets {
        if let Some(m) = rack.modules.iter().find(|m| m.id == *tid)
            && let Some(name) = kind_to_scope_name(m.kind)
            && !scope.contains(&name)
        {
            scope.push(name);
        }
    }
    scope
}

/// Map a ModuleKind to the scope string used by apply_llm_update.
fn kind_to_scope_name(kind: ModuleKind) -> Option<String> {
    match kind {
        ModuleKind::AcidBass => Some("bass".to_string()),
        ModuleKind::DrumKit808 => Some("kit_a".to_string()),
        ModuleKind::DrumKit909 => Some("kit_b".to_string()),
        ModuleKind::HooverLead => Some("hoover".to_string()),
        ModuleKind::PluckString => Some("pluck".to_string()),
        ModuleKind::WavetableVoice => Some("wavetable".to_string()),
        ModuleKind::SampleInstrument => Some("sample".to_string()),
        ModuleKind::An1xVoice => Some("an1x".to_string()),
        ModuleKind::AmenSampler => Some("amen".to_string()),
        ModuleKind::NoiseVoice => Some("noise".to_string()),
        ModuleKind::Theremin => Some("theremin".to_string()),
        ModuleKind::Pendulum => Some("pendulum".to_string()),
        ModuleKind::FmOpsVoice => Some("fm_ops".to_string()),
        ModuleKind::AdditiveVoice => Some("additive".to_string()),
        ModuleKind::ModalVoice => Some("modal".to_string()),
        ModuleKind::ChiptuneVoice => Some("chiptune".to_string()),
        ModuleKind::VocalVoice => Some("vocal".to_string()),
        ModuleKind::GranularTexture => Some("granular".to_string()),
        ModuleKind::GabberKick => Some("gabber_kick".to_string()),
        ModuleKind::StepSequencer => Some("sequencer".to_string()),
        ModuleKind::FxReverb
        | ModuleKind::FxDelay
        | ModuleKind::FxChorus
        | ModuleKind::FxPhaser
        | ModuleKind::FxFlanger
        | ModuleKind::FxLimiter
        | ModuleKind::FxFilter
        | ModuleKind::FxComb
        | ModuleKind::FxTilt
        | ModuleKind::FxTransient
        | ModuleKind::FxExciter
        | ModuleKind::FxMultitap
        | ModuleKind::FxRevDelay
        | ModuleKind::FxTapeStop
        | ModuleKind::FxStutter
        | ModuleKind::FxFreeze
        | ModuleKind::FxRingMod
        | ModuleKind::FxWaveshaper
        | ModuleKind::FxBitcrush
        | ModuleKind::FxEq
        | ModuleKind::FxCompressor
        | ModuleKind::FxGate
        | ModuleKind::FxVocoder
        | ModuleKind::FxTapeSat
        | ModuleKind::FxDrive
        | ModuleKind::FxAutotune
        | ModuleKind::FxWiden
        | ModuleKind::FxFreqShift
        | ModuleKind::FxVinyl
        | ModuleKind::FxDjFilter
        | ModuleKind::FxTremolo
        | ModuleKind::FxVibrato => Some("fx".to_string()),
        ModuleKind::LfoModule => Some("lfo".to_string()),
        _ => None,
    }
}

/// Parse a flexible module-kind string (from LLM JSON or HTTP API) into a
/// `ModuleKind`.  Accepts snake_case, dashed, spaced, and short aliases.
pub fn parse_module_kind(name: &str) -> Option<ModuleKind> {
    use ModuleKind::*;
    match name
        .to_ascii_lowercase()
        .replace(['-', '_', ' '], "")
        .as_str()
    {
        "acidbass" | "bass" | "303" => Some(AcidBass),
        "drumkit808" | "808" | "kita" | "druma" | "drumsa" => Some(DrumKit808),
        "drumkit909" | "909" | "kitb" | "drumb" | "drumsb" => Some(DrumKit909),
        "hooverlead" | "hoover" | "lead" => Some(HooverLead),
        "pluckstring" | "pluck" | "karplus" | "string" | "kstring" => Some(PluckString),
        "wavetable" | "wavetablevoice" | "wt" | "wave_table" => Some(WavetableVoice),
        "sample" | "sampleinstrument" | "instrument" | "keyboard" => Some(SampleInstrument),
        "an1xvoice" | "an1x" | "an-1x" | "pad" | "synth" => Some(An1xVoice),
        "amensampler" | "amen" | "sampler" | "break" => Some(AmenSampler),
        "noisevoice" | "noise" => Some(NoiseVoice),
        "theremin" => Some(Theremin),
        "pendulum" => Some(Pendulum),
        "fmopsvoice" | "fm_ops" | "fmops" | "fm" | "dx7" | "operator" => Some(FmOpsVoice),
        "additivevoice" | "additive" | "harmonic" | "partials" | "drawbar" | "organ" => {
            Some(AdditiveVoice)
        }
        "modalvoice" | "modal" | "physical" | "physicalmodel" | "bell" | "marimba" | "tubular"
        | "glass" | "struck" => Some(ModalVoice),
        "chiptunevoice" | "chiptune" | "sid" | "c64" | "8bit" | "8-bit" | "tracker" => {
            Some(ChiptuneVoice)
        }
        "vocalvoice" | "vocal" | "vowel" | "formant" | "choir" => Some(VocalVoice),
        "granulartexture" | "granular" | "grain" | "texture" => Some(GranularTexture),
        "gabberkick" | "gabber" | "hardcorekick" | "rotterdam" => Some(GabberKick),
        "fxreverb" | "reverb" | "verb" => Some(FxReverb),
        "fxdelay" | "delay" | "echo" => Some(FxDelay),
        "fxchorus" | "chorus" | "ensemble" => Some(FxChorus),
        "fxphaser" | "phaser" | "phase" => Some(FxPhaser),
        "fxflanger" | "flanger" | "flange" => Some(FxFlanger),
        "fxlimiter" | "limiter" | "brickwall" | "brick" => Some(FxLimiter),
        "fxfilter" | "filter" | "svf" | "vcf" => Some(FxFilter),
        "fxcomb" | "comb" | "resonator" => Some(FxComb),
        "fxtilt" | "tilt" | "tilteq" => Some(FxTilt),
        "fxtransient" | "transient" | "transients" | "envshape" => Some(FxTransient),
        "fxexciter" | "exciter" | "aural" | "shimmer" => Some(FxExciter),
        "fxmultitap" | "multitap" | "tappeddelay" | "pingpong" => Some(FxMultitap),
        "fxrevdelay" | "revdelay" | "reversedelay" | "reverseecho" => Some(FxRevDelay),
        "fxtapestop" | "tapestop" | "halt" | "stop" => Some(FxTapeStop),
        "fxstutter" | "stutter" | "repeater" | "glitch" => Some(FxStutter),
        "fxfreeze" | "freeze" | "spectralfreeze" | "drone" => Some(FxFreeze),
        "fxeq" | "eq" | "equalizer" | "equaliser" => Some(FxEq),
        "fxcompressor" | "compressor" | "comp" => Some(FxCompressor),
        "fxgate" | "gate" | "noisegate" | "ducker" | "ducking" => Some(FxGate),
        "fxvocoder" | "vocoder" | "talkbox" | "vocode" => Some(FxVocoder),
        "fxtapesat" | "tapesat" | "tape" | "saturation" => Some(FxTapeSat),
        "fxdrive" | "drive" | "overdrive" | "distortion" => Some(FxDrive),
        "fxautotune" | "autotune" | "pitchcorrect" | "tune" => Some(FxAutotune),
        "fxwaveshaper" | "waveshaper" | "shaper" => Some(FxWaveshaper),
        "fxbitcrush" | "bitcrush" | "lofi" => Some(FxBitcrush),
        "fxringmod" | "ringmod" | "ring" => Some(FxRingMod),
        "fxpan" | "pan" | "autopan" => Some(FxPan),
        "fxwiden" | "widen" | "widener" | "stereowidth" | "haas" => Some(FxWiden),
        "fxfreqshift" | "freqshift" | "frequencyshifter" | "ssbshift" | "bode" => Some(FxFreqShift),
        "fxvinyl" | "vinyl" | "cassette" | "tapevinyl" => Some(FxVinyl),
        "fxdjfilter" | "djfilter" | "dj" | "morphfilter" | "djkill" => Some(FxDjFilter),
        "fxtremolo" | "tremolo" | "trem" | "amplmod" | "ampmod" => Some(FxTremolo),
        "fxvibrato" | "vibrato" | "vib" | "pitchmod" | "pitchwobble" => Some(FxVibrato),
        "lfomodule" | "lfo" => Some(LfoModule),
        "spectrumanalyzer" | "spectrum" | "analyser" | "analyzer" => Some(SpectrumAnalyzer),
        "stereometer" | "stereo" | "correlation" | "meter" => Some(StereoMeter),
        "activitytimeline" | "timeline" | "activity" | "log" => Some(ActivityTimeline),
        "stereovectorscope" | "vectorscope" | "goniometer" | "lissajous" => Some(StereoVectorscope),
        "lfoscope" => Some(LfoScope),
        "pitchtracker" | "tuner" | "pitchdetect" => Some(PitchTracker),
        "chorddisplay" | "chord" | "key" => Some(ChordDisplay),
        "spectrogram" | "waterfall" => Some(Spectrogram),
        "loudnessmeter" | "lufs" | "loudness" => Some(LoudnessMeter),
        "phasewheel" | "transport" | "beatwheel" | "bar" => Some(PhaseWheel),
        "neutts" | "tts" | "voice" | "mc" => Some(NeuTts),
        _ => None,
    }
}

/// Match a module kind against a flexible name string from the LLM.
pub fn rack_kind_name_matches(kind: ModuleKind, name: &str) -> bool {
    let n = name.to_lowercase();
    match kind {
        ModuleKind::FxBitcrush => matches!(
            n.as_str(),
            "bitcrush" | "bit_crush" | "bit crush" | "lofi" | "lo-fi" | "fx"
        ),
        ModuleKind::FxReverb => matches!(n.as_str(), "reverb" | "verb" | "fx"),
        ModuleKind::FxDelay => matches!(n.as_str(), "delay" | "echo" | "fx"),
        ModuleKind::FxChorus => matches!(n.as_str(), "chorus" | "ensemble" | "fx"),
        ModuleKind::FxPhaser => matches!(n.as_str(), "phaser" | "phase" | "fx"),
        ModuleKind::FxFlanger => matches!(n.as_str(), "flanger" | "flange" | "fx"),
        ModuleKind::FxLimiter => matches!(n.as_str(), "limiter" | "brickwall" | "brick" | "fx"),
        ModuleKind::FxFilter => matches!(n.as_str(), "filter" | "svf" | "vcf" | "fx"),
        ModuleKind::FxComb => matches!(n.as_str(), "comb" | "resonator" | "fx"),
        ModuleKind::FxTilt => matches!(n.as_str(), "tilt" | "tilteq" | "fx"),
        ModuleKind::FxTransient => matches!(n.as_str(), "transient" | "transients" | "fx"),
        ModuleKind::FxExciter => matches!(n.as_str(), "exciter" | "aural" | "shimmer" | "fx"),
        ModuleKind::FxMultitap => matches!(n.as_str(), "multitap" | "pingpong" | "fx"),
        ModuleKind::FxRevDelay => matches!(n.as_str(), "revdelay" | "reverse" | "rev" | "fx"),
        ModuleKind::FxTapeStop => matches!(n.as_str(), "tapestop" | "halt" | "stop" | "fx"),
        ModuleKind::FxStutter => matches!(n.as_str(), "stutter" | "repeater" | "glitch" | "fx"),
        ModuleKind::FxFreeze => matches!(n.as_str(), "freeze" | "spectralfreeze" | "drone" | "fx"),
        ModuleKind::FxRingMod => {
            matches!(
                n.as_str(),
                "ringmod" | "ring_mod" | "ring mod" | "ring" | "fx"
            )
        }
        ModuleKind::FxWaveshaper => {
            matches!(n.as_str(), "waveshaper" | "wave_shaper" | "shaper" | "fx")
        }
        ModuleKind::FxEq => matches!(n.as_str(), "eq" | "equalizer" | "equaliser" | "fx"),
        ModuleKind::FxCompressor => matches!(n.as_str(), "compressor" | "comp" | "fx"),
        ModuleKind::FxGate => matches!(
            n.as_str(),
            "gate" | "noisegate" | "noise_gate" | "ducker" | "ducking" | "fx"
        ),
        ModuleKind::FxVocoder => matches!(
            n.as_str(),
            "vocoder" | "vocode" | "talkbox" | "talk_box" | "fx"
        ),
        ModuleKind::FxTapeSat => matches!(
            n.as_str(),
            "tapesat" | "tape_sat" | "tape sat" | "tape" | "saturation" | "fx"
        ),
        ModuleKind::FxDrive => {
            matches!(n.as_str(), "drive" | "overdrive" | "distortion" | "fx")
        }
        ModuleKind::FxAutotune => matches!(
            n.as_str(),
            "autotune" | "auto_tune" | "pitch_correct" | "tune" | "fx"
        ),
        ModuleKind::FxPan => matches!(n.as_str(), "pan" | "autopan" | "fx"),
        ModuleKind::FxWiden => matches!(
            n.as_str(),
            "widen" | "widener" | "haas" | "stereowidth" | "fx"
        ),
        ModuleKind::FxFreqShift => matches!(
            n.as_str(),
            "freqshift" | "freq_shift" | "frequencyshifter" | "ssb" | "bode" | "fx"
        ),
        ModuleKind::FxVinyl => matches!(n.as_str(), "vinyl" | "cassette" | "tape" | "fx"),
        ModuleKind::FxDjFilter => matches!(
            n.as_str(),
            "djfilter" | "dj_filter" | "dj filter" | "dj" | "morphfilter" | "fx"
        ),
        ModuleKind::FxTremolo => matches!(
            n.as_str(),
            "tremolo" | "trem" | "ampmod" | "ampl_mod" | "fx"
        ),
        ModuleKind::FxVibrato => matches!(
            n.as_str(),
            "vibrato" | "vib" | "pitchmod" | "pitch_mod" | "fx"
        ),
        ModuleKind::FxConvReverb => matches!(
            n.as_str(),
            "convreverb" | "conv_reverb" | "conv reverb" | "convolution" | "ir" | "fx"
        ),
        ModuleKind::FxParamEq => matches!(
            n.as_str(),
            "parameq"
                | "param_eq"
                | "param eq"
                | "parametric"
                | "parametric_eq"
                | "curve_eq"
                | "fx"
        ),
        ModuleKind::FxPitchShift => matches!(
            n.as_str(),
            "pitch" | "pitch_shift" | "pitchshift" | "pitch shift" | "shifter" | "harmony" | "fx"
        ),
        ModuleKind::LfoModule => matches!(n.as_str(), "lfo"),
        ModuleKind::AcidBass => matches!(n.as_str(), "bass" | "acid" | "303"),
        ModuleKind::DrumKit808 => matches!(n.as_str(), "808" | "kit_a" | "drum_a" | "drums_a"),
        ModuleKind::DrumKit909 => matches!(n.as_str(), "909" | "kit_b" | "drum_b" | "drums_b"),
        ModuleKind::HooverLead => matches!(n.as_str(), "hoover" | "lead"),
        ModuleKind::PluckString => matches!(
            n.as_str(),
            "pluck" | "pluckstring" | "pluck_string" | "karplus" | "string" | "kstring"
        ),
        ModuleKind::WavetableVoice => matches!(
            n.as_str(),
            "wavetable" | "wavetablevoice" | "wave_table" | "wt"
        ),
        ModuleKind::SampleInstrument => matches!(
            n.as_str(),
            "sample" | "sampleinstrument" | "instrument" | "keyboard"
        ),
        ModuleKind::An1xVoice => matches!(n.as_str(), "an1x" | "an-1x" | "pad" | "synth"),
        ModuleKind::AmenSampler => matches!(n.as_str(), "amen" | "sampler" | "break"),
        ModuleKind::NoiseVoice => matches!(n.as_str(), "noise"),
        ModuleKind::Theremin => matches!(n.as_str(), "theremin"),
        ModuleKind::Pendulum => matches!(n.as_str(), "pendulum"),
        ModuleKind::FmOpsVoice => {
            matches!(n.as_str(), "fm_ops" | "fmops" | "fm" | "dx7" | "operator")
        }
        ModuleKind::AdditiveVoice => matches!(
            n.as_str(),
            "additive" | "harmonic" | "partials" | "drawbar" | "organ"
        ),
        ModuleKind::ModalVoice => matches!(
            n.as_str(),
            "modal" | "physical" | "bell" | "marimba" | "tubular" | "glass" | "struck"
        ),
        ModuleKind::ChiptuneVoice => {
            matches!(n.as_str(), "chiptune" | "sid" | "c64" | "8bit" | "tracker")
        }
        ModuleKind::VocalVoice => matches!(n.as_str(), "vocal" | "vowel" | "formant" | "choir"),
        ModuleKind::GranularTexture => matches!(n.as_str(), "granular" | "grain" | "texture"),
        ModuleKind::GabberKick => {
            matches!(
                n.as_str(),
                "gabber" | "gabber_kick" | "hardcore" | "rotterdam"
            )
        }
        ModuleKind::NeuTts => {
            matches!(n.as_str(), "neutts" | "tts" | "mc" | "voice")
        }
        ModuleKind::MasterOutput => {
            matches!(n.as_str(), "master" | "master_out" | "out" | "output")
        }
        ModuleKind::StepSequencer => matches!(n.as_str(), "sequencer" | "seq"),
        ModuleKind::LlmAgent => matches!(n.as_str(), "llm" | "agent" | "ai" | "llm_agent"),
        ModuleKind::LlmConsole => matches!(n.as_str(), "console" | "llm_console"),
        ModuleKind::SpectrumAnalyzer => matches!(n.as_str(), "spectrum" | "analyser" | "analyzer"),
        ModuleKind::StereoMeter => matches!(n.as_str(), "stereo" | "correlation" | "meter"),
        ModuleKind::ActivityTimeline => matches!(n.as_str(), "timeline" | "activity" | "log"),
        ModuleKind::BarOscilloscope => {
            matches!(n.as_str(), "scope" | "oscilloscope" | "waveform")
        }
        ModuleKind::StereoVectorscope => matches!(
            n.as_str(),
            "vectorscope" | "goniometer" | "lissajous" | "stereoscope"
        ),
        ModuleKind::LfoScope => matches!(n.as_str(), "lfoscope" | "lfo_scope"),
        ModuleKind::PitchTracker => matches!(n.as_str(), "tuner" | "pitch" | "pitchtracker"),
        ModuleKind::ChordDisplay => matches!(n.as_str(), "chord" | "key" | "chords"),
        ModuleKind::Spectrogram => matches!(n.as_str(), "spectrogram" | "waterfall"),
        ModuleKind::LoudnessMeter => matches!(n.as_str(), "lufs" | "loudness" | "loudnessmeter"),
        ModuleKind::PhaseWheel => matches!(n.as_str(), "phasewheel" | "phase" | "transport"),
        ModuleKind::EventStream => matches!(n.as_str(), "events" | "stream" | "notes"),
    }
}
