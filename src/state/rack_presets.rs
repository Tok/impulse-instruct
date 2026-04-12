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
            ModuleKind::AcidBass,
            ModuleKind::DrumKit808,
            ModuleKind::DrumKit909,
            ModuleKind::HooverLead,
            ModuleKind::An1xVoice,
            ModuleKind::AmenSampler,
            ModuleKind::NoiseVoice,
            ModuleKind::GranularTexture,
            ModuleKind::NeuTts,
        ],
        fx: &[
            ModuleKind::FxWaveshaper,
            ModuleKind::FxReverb,
            ModuleKind::FxDelay,
            ModuleKind::FxBitcrush,
            ModuleKind::FxChorus,
            ModuleKind::FxPhaser,
            ModuleKind::FxRingMod,
            ModuleKind::FxEq,
            ModuleKind::FxCompressor,
            ModuleKind::FxTapeSat,
            ModuleKind::FxDrive,
            ModuleKind::FxAutotune,
        ],
        lfo_count: 4,
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
