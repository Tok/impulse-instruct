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
    // ── Analysis ──────────────────────────────────────────────────────────────
    SpectrumAnalyzer,
    StereoMeter,
    ActivityTimeline,
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
            Self::SpectrumAnalyzer => "SPECTRUM",
            Self::StereoMeter => "STEREO METER",
            Self::ActivityTimeline => "TIMELINE",
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
            Self::MasterOutput => (grid_cols, 1),
            Self::AcidBass => (4, 7),
            // Drum kits are 4 rows tall so the 3 per-voice glass groups
            // (kick / snare / hihat or clap) each with an XY pad beside
            // their knobs don't get clipped / scrollbar'd.
            Self::DrumKit808 => (4, 5),
            Self::DrumKit909 => (4, 4),
            Self::HooverLead => (4, 2),
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
            Self::LfoModule => (2, 2),
            // FX modules — exhaustive so new variants cause a compile error
            Self::FxDelay => (2, 2), // 5-button row can't fit in 1 row
            // Convolution Reverb — 6 knobs + reverse toggle + IR picker row
            // need three grid rows; the picker label eats horizontal space
            // so 3 cols wide matches the content.
            Self::FxConvReverb => (3, 3),
            Self::FxReverb
            | Self::FxChorus
            | Self::FxPhaser
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
            | Self::SpectrumAnalyzer
            | Self::StereoMeter
            | Self::ActivityTimeline
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
