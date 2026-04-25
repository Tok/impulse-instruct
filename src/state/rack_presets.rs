// ─── state/rack_presets.rs ───────────────────────────────���─────────────────────
// Rack layout presets — extracted from rack.rs to stay under the line limit.

use super::rack::{ModuleKind, RackState};

/// A named rack layout preset: which voice and FX modules to include.
pub struct RackPreset {
    pub name: &'static str,
    pub description: &'static str,
    pub voices: &'static [ModuleKind],
    pub fx: &'static [ModuleKind],
    pub lfo_count: u8,
}

pub const RACK_PRESETS: &[RackPreset] = &[
    RackPreset {
        name: "Empty",
        description: "Sequencer + master only — build from scratch",
        voices: &[],
        fx: &[],
        lfo_count: 0,
    },
    RackPreset {
        name: "Basic",
        description: "303 bass, 808 and 909 kits — no FX",
        voices: &[
            ModuleKind::AcidBass,
            ModuleKind::DrumKit808,
            ModuleKind::DrumKit909,
        ],
        fx: &[],
        lfo_count: 0,
    },
    RackPreset {
        name: "Standard",
        description: "Bass, drums, hoover + reverb, delay, compressor",
        voices: &[
            ModuleKind::AcidBass,
            ModuleKind::DrumKit808,
            ModuleKind::DrumKit909,
            ModuleKind::HooverLead,
        ],
        fx: &[
            ModuleKind::FxReverb,
            ModuleKind::FxDelay,
            ModuleKind::FxCompressor,
        ],
        lfo_count: 2,
    },
    RackPreset {
        name: "Full",
        description: "All instruments and FX — full gallery",
        voices: &[
            // All voice modules — Full really means full.  V2 added
            // PluckString / WavetableVoice / SampleInstrument; older
            // Full preset predated those.
            ModuleKind::AcidBass,
            ModuleKind::DrumKit808,
            ModuleKind::DrumKit909,
            ModuleKind::GabberKick,
            ModuleKind::HooverLead,
            ModuleKind::An1xVoice,
            ModuleKind::PluckString,
            ModuleKind::WavetableVoice,
            ModuleKind::SampleInstrument,
            ModuleKind::AmenSampler,
            ModuleKind::NoiseVoice,
            ModuleKind::GranularTexture,
            ModuleKind::NeuTts,
        ],
        fx: &[
            // Distortion / saturation family
            ModuleKind::FxWaveshaper,
            ModuleKind::FxBitcrush,
            ModuleKind::FxTapeSat,
            ModuleKind::FxDrive,
            ModuleKind::FxExciter,
            // Filter / EQ family
            ModuleKind::FxFilter,
            ModuleKind::FxEq,
            ModuleKind::FxParamEq,
            ModuleKind::FxTilt,
            // Modulation family
            ModuleKind::FxChorus,
            ModuleKind::FxPhaser,
            ModuleKind::FxFlanger,
            ModuleKind::FxRingMod,
            ModuleKind::FxComb,
            // Time-domain / delay family
            ModuleKind::FxDelay,
            ModuleKind::FxMultitap,
            ModuleKind::FxRevDelay,
            ModuleKind::FxReverb,
            ModuleKind::FxConvReverb,
            // Pitch family
            ModuleKind::FxPitchShift,
            ModuleKind::FxFreqShift,
            ModuleKind::FxAutotune,
            // Glitch / performance family
            ModuleKind::FxTapeStop,
            ModuleKind::FxStutter,
            ModuleKind::FxFreeze,
            ModuleKind::FxTransient,
            // Dynamics / mastering family
            ModuleKind::FxCompressor,
            ModuleKind::FxGate,
            ModuleKind::FxLimiter,
            ModuleKind::FxVocoder,
            // Stereo placement
            ModuleKind::FxPan,
            ModuleKind::FxWiden,
        ],
        lfo_count: 6,
    },
    // Curated viz showcase — extends the "Full" gallery with one
    // representative module per analysis family.  Kept out of "Full"
    // because all 12 viz modules together overwhelm the rack and
    // most overlap functionally; this preset gives users a guided
    // tour without burying the audio path.
    RackPreset {
        name: "Full + Viz",
        description: "Full gallery + curated analysis modules",
        voices: &[
            ModuleKind::AcidBass,
            ModuleKind::DrumKit808,
            ModuleKind::DrumKit909,
            ModuleKind::GabberKick,
            ModuleKind::HooverLead,
            ModuleKind::An1xVoice,
            ModuleKind::PluckString,
            ModuleKind::WavetableVoice,
            ModuleKind::SampleInstrument,
            ModuleKind::AmenSampler,
            ModuleKind::NoiseVoice,
            ModuleKind::GranularTexture,
            ModuleKind::NeuTts,
        ],
        fx: &[
            // Same FX gallery as "Full"...
            ModuleKind::FxWaveshaper,
            ModuleKind::FxBitcrush,
            ModuleKind::FxTapeSat,
            ModuleKind::FxDrive,
            ModuleKind::FxExciter,
            ModuleKind::FxFilter,
            ModuleKind::FxEq,
            ModuleKind::FxParamEq,
            ModuleKind::FxTilt,
            ModuleKind::FxChorus,
            ModuleKind::FxPhaser,
            ModuleKind::FxFlanger,
            ModuleKind::FxRingMod,
            ModuleKind::FxComb,
            ModuleKind::FxDelay,
            ModuleKind::FxMultitap,
            ModuleKind::FxRevDelay,
            ModuleKind::FxReverb,
            ModuleKind::FxConvReverb,
            ModuleKind::FxPitchShift,
            ModuleKind::FxFreqShift,
            ModuleKind::FxAutotune,
            ModuleKind::FxTapeStop,
            ModuleKind::FxStutter,
            ModuleKind::FxFreeze,
            ModuleKind::FxTransient,
            ModuleKind::FxCompressor,
            ModuleKind::FxGate,
            ModuleKind::FxLimiter,
            ModuleKind::FxVocoder,
            ModuleKind::FxPan,
            ModuleKind::FxWiden,
            // ...plus the curated viz set: one per family.
            // Output level (peak / RMS).
            ModuleKind::StereoMeter,
            // FFT bars — canonical spectral view.
            ModuleKind::SpectrumAnalyzer,
            // Phosphor scope — canonical time-domain view.
            ModuleKind::BarOscilloscope,
            // Note history with Huth coloring — distinctive widget.
            ModuleKind::EventStream,
            // K-weighted loudness — mastering-grade level reference.
            ModuleKind::LoudnessMeter,
            // Bar / beat indicator — glanceable transport readout.
            ModuleKind::PhaseWheel,
        ],
        lfo_count: 6,
    },
];

impl RackState {
    /// Build a rack from a preset. Always includes Sequencer, MasterOutput,
    /// LlmConsole, and one LlmAgent. Voice and FX modules come from the preset.
    pub fn from_preset(preset: &RackPreset) -> Self {
        let mut rack = Self {
            modules: Vec::new(),
            cables: Vec::new(),
            next_id: 100,
            dyn_sequencer_rows: None,
        };
        rack.add_module(ModuleKind::StepSequencer);
        rack.add_module(ModuleKind::MasterOutput);
        rack.add_module(ModuleKind::LlmConsole);
        for &kind in preset.voices {
            rack.add_module(kind);
        }
        for &kind in preset.fx {
            rack.add_module(kind);
        }
        for _ in 0..preset.lfo_count {
            rack.add_module(ModuleKind::LfoModule);
        }
        rack.add_module(ModuleKind::LlmAgent);
        rack.wire_default_cables();
        rack.arrange_canonical();
        rack
    }
}
