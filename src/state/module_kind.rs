// ─── state/module_kind.rs ─────────────────────────────────────────────────────
// `ModuleKind` + `Zone` — the enumeration of every instantiable rack module
// type and the vertical layout zones.  Extracted from rack.rs to keep that
// file under the 1000-line cap.  Layout/label/grid metadata is exhaustive per
// variant here; adding a new kind forces every match to be updated.

use serde::{Deserialize, Serialize};

// ─── Module kinds ─────────────────────────────────────────────────────────────

/// Every instantiable module type in the rack.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ModuleKind {
    // ── Voice modules ─────────────────────────────────────────────────────────
    AcidBass,
    DrumKit808,
    DrumKit909,
    HooverLead,
    PluckString,
    WavetableVoice,
    An1xVoice,
    AmenSampler,
    NoiseVoice,
    GranularTexture,
    /// Dedicated hardcore-style kick voice — distinct from the 808/909 kicks.
    GabberKick,
    // ── TTS voice module (NeuTTS Air) ───────────────────────────────────────
    #[serde(alias = "EspeakNgTts", alias = "CoquiTts")]
    NeuTts,
    // ── Sequencer ─────────────────────────────────────────────────────────────
    /// Main step sequencer — drives all voice modules.
    StepSequencer,
    // ── FX modules ────────────────────────────────────────────────────────────
    FxReverb,
    FxDelay,
    FxChorus,
    FxPhaser,
    FxFlanger,
    FxLimiter,
    FxFilter,
    FxComb,
    FxTilt,
    FxTransient,
    FxExciter,
    FxMultitap,
    FxRevDelay,
    FxTapeStop,
    FxStutter,
    FxRingMod,
    FxWaveshaper,
    FxBitcrush,
    FxEq,
    FxCompressor,
    FxTapeSat,
    FxDrive,
    FxAutotune,
    FxPan,
    FxConvReverb,
    FxParamEq,
    FxPitchShift,
    // ── Analysis ──────────────────────────────────────────────────────────────
    SpectrumAnalyzer,
    StereoMeter,
    ActivityTimeline,
    /// Phosphor-style colored bar oscilloscope — mirrors the header
    /// `draw_scope_colored` widget but lives as a rack module so users
    /// can park it anywhere in the FX/Mod zone.  Reads the global
    /// scope buffer + history; no audio input cable needed.
    BarOscilloscope,
    /// Stereo vectorscope / goniometer — XY plot of L vs R from the
    /// engine's interleaved stereo buffer.  Reads `app.stereo_buf`
    /// (already maintained for the header phase-correlation strip).
    /// Pure UI; no audio thread interaction.
    StereoVectorscope,
    /// LFO output trace — waveform of the LFO module connected via
    /// the back-panel CV cable, rendered with the same phosphor look
    /// as the bar oscilloscope.  No audio path.
    LfoScope,
    /// Pitch tracker / tuner — autocorrelation pitch on the master
    /// scope buffer, displays note name + cents-off needle.  Cheap
    /// reuse of `detect_pitch_hz` in src/audio/analysis.rs.
    PitchTracker,
    /// Chord / key display — chroma-vector folding of the existing
    /// spectrum, matched against 24 major+minor triad templates.
    /// Reuses `compute_spectrum` so adding the module is essentially
    /// free DSP-wise.
    ChordDisplay,
    /// Spectrogram waterfall — rolling FFT history rendered as a
    /// scrolling 2D ColorImage (time on X, frequency on Y).  Heavier
    /// than the static spectrum bars; the UI paints a fixed-height
    /// pixel column per frame and recycles a `ColorImage` between
    /// repaints to keep the per-frame allocation bounded.
    Spectrogram,
    /// Loudness / LUFS meter — K-weighted (BS.1770) momentary + short-term
    /// loudness display.  Distinct from the per-frame peak / RMS in
    /// `StereoMeter` because LUFS is the integrated, perceptually
    /// weighted measure mastering engineers actually target.
    LoudnessMeter,
    /// Transport phase wheel — circular bar / beat indicator with
    /// sub-divisions, rendered from the current sequencer step + step
    /// fraction.  UI-only; no DSP.  Companion to `EventStream` —
    /// glanceable indicator of where in the bar we are.
    PhaseWheel,
    /// Real-time note / drum event stream — mirrors the header event
    /// stream widget.  Rendered as a rack module so multiple instances
    /// can show different scroll speeds / focus voices in future
    /// iterations.  V1: one canonical instance reads the global
    /// AppState log buffers.
    EventStream,
    // ── Modulation ────────────────────────────────────────────────────────────
    LfoModule,
    LlmAgent,
    // ── LLM console (singleton, Global zone) ──────────────────────────────
    LlmConsole,
    //── Utility ───────────────────────────────────────────────────────────────
    MasterOutput,
}

impl ModuleKind {
    /// Short display label shown in the module card title bar.
    pub fn label(self) -> &'static str {
        match self {
            Self::AcidBass => "BASS SYNTH",
            Self::DrumKit808 => "808 KIT",
            Self::DrumKit909 => "909 KIT",
            Self::HooverLead => "HOOVER",
            Self::PluckString => "PLUCK",
            Self::WavetableVoice => "WAVETABLE",
            Self::An1xVoice => "AN1X",
            Self::AmenSampler => "AMEN",
            Self::NoiseVoice => "NOISE",
            Self::GranularTexture => "GRANULAR",
            Self::GabberKick => "GABBER KICK",
            Self::NeuTts => "TTS VOICE",
            Self::StepSequencer => "SEQUENCER",
            Self::FxReverb => "REVERB",
            Self::FxDelay => "DELAY",
            Self::FxChorus => "CHORUS",
            Self::FxPhaser => "PHASER",
            Self::FxFlanger => "FLANGER",
            Self::FxLimiter => "LIMITER",
            Self::FxFilter => "FILTER",
            Self::FxComb => "COMB",
            Self::FxTilt => "TILT EQ",
            Self::FxTransient => "TRANSIENT",
            Self::FxExciter => "EXCITER",
            Self::FxMultitap => "MULTITAP",
            Self::FxRevDelay => "REV DELAY",
            Self::FxTapeStop => "TAPE STOP",
            Self::FxStutter => "STUTTER",
            Self::FxRingMod => "RING MOD",
            Self::FxWaveshaper => "WAVESHAPER",
            Self::FxBitcrush => "BITCRUSH",
            Self::FxEq => "EQ",
            Self::FxCompressor => "COMPRESSOR",
            Self::FxTapeSat => "TAPE SAT",
            Self::FxDrive => "DRIVE",
            Self::FxAutotune => "AUTOTUNE",
            Self::FxPan => "PAN",
            Self::FxConvReverb => "CONV REV",
            Self::FxParamEq => "PARAM EQ",
            Self::FxPitchShift => "PITCH",
            Self::SpectrumAnalyzer => "SPECTRUM",
            Self::StereoMeter => "STEREO METER",
            Self::ActivityTimeline => "TIMELINE",
            Self::BarOscilloscope => "OSCILLOSCOPE",
            Self::StereoVectorscope => "GONIOMETER",
            Self::LfoScope => "LFO SCOPE",
            Self::PitchTracker => "TUNER",
            Self::ChordDisplay => "CHORD",
            Self::Spectrogram => "SPECTROGRAM",
            Self::LoudnessMeter => "LUFS",
            Self::PhaseWheel => "PHASE",
            Self::EventStream => "EVENT STREAM",
            Self::LfoModule => "LFO",
            Self::LlmAgent => "LLM AGENT",
            Self::LlmConsole => "LLM CONSOLE",
            Self::MasterOutput => "MASTER",
        }
    }

    /// Grid size in (columns, rows) for the 12-column rack grid.
    /// Full-width modules use `grid_cols` for width; all others are fixed.
    /// Height is enforced as a minimum — content taller than this grows naturally.
    pub fn grid_size(self, grid_cols: u8) -> (u8, u8) {
        match self {
            //                                     W     H
            Self::StepSequencer => (grid_cols, 2),
            // LlmConsole: 2 rows.  The content (square cycle-viz widget +
            // lane-score strip + prompt/log panel) needs more than a
            // single grid cell of height at typical col_w values —
            // rendered content otherwise spills past its allocated slot
            // into the zone below, where the next zone's backdrop / the
            // first card paints over the overflow and the console
            // partly disappears.
            Self::LlmConsole => (grid_cols, 2),
            // Master: row 1 = MASTER VOLUME + voice-presence strip,
            // row 2 = mid/side master knobs (6 rotaries).
            Self::MasterOutput => (grid_cols, 2),
            Self::AcidBass => (4, 7),
            // Drum kits are 4 rows tall so the 3 per-voice glass groups
            // (kick / snare / hihat or clap) each with an XY pad beside
            // their knobs don't get clipped / scrollbar'd.
            Self::DrumKit808 => (4, 5),
            Self::DrumKit909 => (4, 4),
            Self::HooverLead => (4, 2),
            // Pluck — few knobs (damp/bright/vol/pan/offset + on-off),
            // comfortably fits in 3x2.
            Self::PluckString => (3, 2),
            // Wavetable — pos/phase + vol/pan/pitch + LOAD button +
            // filename label.  Same 3x2 envelope as Pluck.
            Self::WavetableVoice => (3, 2),
            Self::An1xVoice => (6, 6),
            Self::AmenSampler => (3, 3),
            Self::NoiseVoice => (2, 1),
            Self::GranularTexture => (3, 2),
            Self::GabberKick => (3, 2),
            Self::LlmAgent => (3, 2),
            Self::NeuTts => (2, 3),
            Self::SpectrumAnalyzer => (4, 2),
            Self::ActivityTimeline => (4, 2),
            Self::StereoMeter => (2, 1),
            // Bar oscilloscope — wide enough that its phosphor trails
            // read clearly without dominating the rack.  Same envelope
            // as the spectrum module so they line up when stacked.
            Self::BarOscilloscope => (4, 2),
            // Stereo vectorscope — square card so the lissajous lobes
            // aren't squashed into an oval.
            Self::StereoVectorscope => (2, 2),
            // LFO scope — same 4×2 envelope as the bar scope so the
            // two viz modules line up visually when stacked.
            Self::LfoScope => (4, 2),
            // Tuner — compact, single big note + cents needle.
            Self::PitchTracker => (2, 2),
            // Chord display — slightly wider so "Cm7" / "F#maj"
            // readouts have room to breathe.
            Self::ChordDisplay => (3, 2),
            // Spectrogram — wide so the time axis has room to scroll;
            // matches the spectrum-analyzer footprint at 4×2 so the two
            // align visually when stacked.
            Self::Spectrogram => (4, 2),
            // LUFS — single-row meter with two big numbers.
            Self::LoudnessMeter => (2, 2),
            // Phase wheel — square so the circle isn't squashed.
            Self::PhaseWheel => (2, 2),
            // Event stream — wider than the analysis modules so notes
            // have horizontal room to scroll past.  Two rows tall so
            // the future / past split is legible.
            Self::EventStream => (6, 2),
            Self::LfoModule => (2, 2),
            // FX modules — exhaustive so new variants cause a compile error
            Self::FxDelay => (2, 2), // 5-button row can't fit in 1 row
            // Convolution Reverb — 6 knobs + reverse toggle + IR picker row
            // need three grid rows; the picker label eats horizontal space
            // so 3 cols wide matches the content.
            Self::FxConvReverb => (3, 3),
            // Param EQ — curve editor dominates the card; 4 cols gives
            // the 20 Hz–20 kHz axis room to breathe, 4 rows leaves room
            // for the curve + band-param readout strip below it.
            Self::FxParamEq => (4, 4),
            // PitchShift — SHIFT / MIX / FBK on row 1, FINE on row 2
            // so the semitone knob doesn't crowd the cents readout.
            Self::FxPitchShift => (2, 2),
            // Flanger — 4 knobs (rate / depth / feedback / mix) is one
            // more than the phaser bundle, so it gets a 2×2 footprint
            // like the delay rather than the 2×1 used by the smaller FX.
            Self::FxFlanger => (2, 2),
            // Limiter, Filter, Comb each have 4 knobs (some plus a mode
            // selector); they need 2×2 to fit two rows.
            Self::FxLimiter => (2, 2),
            Self::FxFilter => (2, 2),
            Self::FxComb => (2, 2),
            // 4 knobs each — 2 rows of 2.
            Self::FxMultitap => (2, 2),
            Self::FxRevDelay => (2, 2),
            Self::FxStutter => (2, 2),
            Self::FxReverb
            | Self::FxChorus
            | Self::FxPhaser
            | Self::FxTilt
            | Self::FxTransient
            | Self::FxExciter
            | Self::FxTapeStop
            | Self::FxRingMod
            | Self::FxWaveshaper
            | Self::FxBitcrush
            | Self::FxEq
            | Self::FxCompressor
            | Self::FxTapeSat
            | Self::FxDrive
            | Self::FxAutotune
            | Self::FxPan => (2, 1),
        }
    }

    /// Which zone this module belongs to by default.
    pub fn default_zone(self) -> Zone {
        match self {
            Self::LlmConsole | Self::LlmAgent => Zone::Ai,
            Self::StepSequencer | Self::MasterOutput => Zone::Global,
            Self::AcidBass
            | Self::DrumKit808
            | Self::DrumKit909
            | Self::HooverLead
            | Self::PluckString
            | Self::WavetableVoice
            | Self::An1xVoice
            | Self::AmenSampler
            | Self::NoiseVoice
            | Self::GranularTexture
            | Self::NeuTts
            | Self::GabberKick => Zone::Voice,
            Self::FxReverb
            | Self::FxDelay
            | Self::FxChorus
            | Self::FxPhaser
            | Self::FxFlanger
            | Self::FxLimiter
            | Self::FxFilter
            | Self::FxComb
            | Self::FxTilt
            | Self::FxTransient
            | Self::FxExciter
            | Self::FxMultitap
            | Self::FxRevDelay
            | Self::FxTapeStop
            | Self::FxStutter
            | Self::FxRingMod
            | Self::FxWaveshaper
            | Self::FxBitcrush
            | Self::FxEq
            | Self::FxCompressor
            | Self::FxTapeSat
            | Self::FxDrive
            | Self::FxAutotune
            | Self::FxPan
            | Self::FxConvReverb
            | Self::FxParamEq
            | Self::FxPitchShift
            | Self::SpectrumAnalyzer
            | Self::StereoMeter
            | Self::ActivityTimeline
            | Self::BarOscilloscope
            | Self::StereoVectorscope
            | Self::LfoScope
            | Self::PitchTracker
            | Self::ChordDisplay
            | Self::Spectrogram
            | Self::LoudnessMeter
            | Self::PhaseWheel
            | Self::EventStream
            | Self::LfoModule => Zone::FxMod,
        }
    }

    /// True if this module produces an audio bus signal (voice or FX).  Used by
    /// the back-panel "reaches MASTER" indicator — modules without an audio
    /// output (sequencer, LFO, agents, meters) are not part of the audio
    /// graph and their LED is hidden entirely.
    pub fn has_audio_output(self) -> bool {
        matches!(
            self,
            Self::AcidBass
                | Self::HooverLead
                | Self::PluckString
                | Self::WavetableVoice
                | Self::DrumKit808
                | Self::DrumKit909
                | Self::AmenSampler
                | Self::GranularTexture
                | Self::GabberKick
                | Self::NoiseVoice
                | Self::An1xVoice
                | Self::NeuTts
                | Self::FxReverb
                | Self::FxDelay
                | Self::FxChorus
                | Self::FxPhaser
                | Self::FxFlanger
                | Self::FxLimiter
                | Self::FxFilter
                | Self::FxComb
                | Self::FxTilt
                | Self::FxTransient
                | Self::FxExciter
                | Self::FxMultitap
                | Self::FxRevDelay
                | Self::FxTapeStop
                | Self::FxStutter
                | Self::FxRingMod
                | Self::FxWaveshaper
                | Self::FxBitcrush
                | Self::FxEq
                | Self::FxCompressor
                | Self::FxTapeSat
                | Self::FxDrive
                | Self::FxAutotune
                | Self::FxPan
                | Self::FxConvReverb
                | Self::FxParamEq
                | Self::FxPitchShift
        )
    }

    /// True for FX modules that offer an XY pad in the expanded view.
    /// Modules returning true get an extra grid row when `pad_expanded` is
    /// true on their `RackModule`, plus a chevron toggle in the title bar.
    pub fn supports_xy_pad(self) -> bool {
        matches!(
            self,
            Self::FxReverb
                | Self::FxDelay
                | Self::FxChorus
                | Self::FxPhaser
                | Self::FxFlanger
                | Self::FxLimiter
                | Self::FxFilter
                | Self::FxComb
                | Self::FxTilt
                | Self::FxTransient
                | Self::FxExciter
                | Self::FxMultitap
                | Self::FxRevDelay
                | Self::FxTapeStop
                | Self::FxStutter
                | Self::FxRingMod
                | Self::FxWaveshaper
                | Self::FxBitcrush
                | Self::FxEq
                | Self::FxCompressor
                | Self::FxTapeSat
                | Self::FxDrive
                | Self::FxAutotune
                | Self::FxPan
                | Self::FxConvReverb
                | Self::FxPitchShift
        )
    }

    /// Whether this module type may have more than one instance in the rack.
    pub fn allows_multiple(self) -> bool {
        matches!(
            self,
            Self::FxReverb
                | Self::FxDelay
                | Self::FxChorus
                | Self::FxPhaser
                | Self::FxFlanger
                | Self::FxLimiter
                | Self::FxFilter
                | Self::FxComb
                | Self::FxTilt
                | Self::FxTransient
                | Self::FxExciter
                | Self::FxMultitap
                | Self::FxRevDelay
                | Self::FxTapeStop
                | Self::FxStutter
                | Self::FxRingMod
                | Self::FxWaveshaper
                | Self::FxBitcrush
                | Self::FxEq
                | Self::FxCompressor
                | Self::FxTapeSat
                | Self::FxDrive
                | Self::FxAutotune
                | Self::FxPan
                | Self::FxConvReverb
                | Self::FxParamEq
                | Self::FxPitchShift
                | Self::LfoModule
                | Self::LlmAgent
        )
    }
}

// ─── Zone ────────────────────────────────────────────────────────────────────

/// Fixed grid column count for the rack layout.
pub const GRID_COLS: u8 = 12;

/// The vertical zone a module lives in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Zone {
    /// LLM console + agents.
    Ai,
    /// Clock/sequencer, master output — the main audio strip.
    /// Labelled "MAIN AUDIO" in the UI; the enum name is kept for serde
    /// backward compat with pre-split sessions.
    Global,
    /// Voice / instrument modules.
    Voice,
    /// FX processors and modulation sources.
    FxMod,
}

impl Zone {
    /// Lowercase scroll-target name (`"ai"`, `"global"`, `"voice"`, `"fxmod"`).
    pub fn scroll_name(self) -> &'static str {
        match self {
            Zone::Ai => "ai",
            Zone::Global => "global",
            Zone::Voice => "voice",
            Zone::FxMod => "fxmod",
        }
    }
}
