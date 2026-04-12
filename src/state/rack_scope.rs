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
        ModuleKind::An1xVoice => Some("an1x".to_string()),
        ModuleKind::AmenSampler => Some("amen".to_string()),
        ModuleKind::NoiseVoice => Some("noise".to_string()),
        ModuleKind::GranularTexture => Some("granular".to_string()),
        ModuleKind::StepSequencer => Some("sequencer".to_string()),
        ModuleKind::FxReverb
        | ModuleKind::FxDelay
        | ModuleKind::FxChorus
        | ModuleKind::FxPhaser
        | ModuleKind::FxRingMod
        | ModuleKind::FxWaveshaper
        | ModuleKind::FxBitcrush
        | ModuleKind::FxEq
        | ModuleKind::FxCompressor
        | ModuleKind::FxTapeSat
        | ModuleKind::FxDrive
        | ModuleKind::FxAutotune => Some("fx".to_string()),
        ModuleKind::LfoModule => Some("lfo".to_string()),
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
        ModuleKind::LfoModule => matches!(n.as_str(), "lfo"),
        ModuleKind::AcidBass => matches!(n.as_str(), "bass" | "acid" | "303"),
        ModuleKind::DrumKit808 => matches!(n.as_str(), "808" | "kit_a" | "drum_a" | "drums_a"),
        ModuleKind::DrumKit909 => matches!(n.as_str(), "909" | "kit_b" | "drum_b" | "drums_b"),
        ModuleKind::HooverLead => matches!(n.as_str(), "hoover" | "lead"),
        ModuleKind::An1xVoice => matches!(n.as_str(), "an1x" | "an-1x" | "pad" | "synth"),
        ModuleKind::AmenSampler => matches!(n.as_str(), "amen" | "sampler" | "break"),
        ModuleKind::NoiseVoice => matches!(n.as_str(), "noise"),
        ModuleKind::GranularTexture => matches!(n.as_str(), "granular" | "grain" | "texture"),
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
    }
}
