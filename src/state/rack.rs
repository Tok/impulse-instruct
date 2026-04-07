// ─── state/rack.rs ────────────────────────────────────────────────────────────
// Modular rack state: which modules are active and how they are cabled.
//
// The rack replaces the fixed 5-tab layout.  Every voice, FX unit, and
// modulation source is a `RackModule` with a stable `id`, a `kind`, optional
// enabled flag, and a layout position used by the UI.
//
// Cables connect output ports to input ports.  In this first iteration the DSP
// still uses the existing fixed chain; cable state drives the visual routing
// overlay and will be wired into `compile_fx_plan` in a follow-up.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

// ─── Port types ───────────────────────────────────────────────────────────────

/// Signal kind carried by a port.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PortKind {
    /// Stereo audio signal.
    Audio,
    /// Mono CV signal (0–1).  Used for LFO targets, gate, pitch CV, etc.
    Cv,
    /// LLM control link — connects an LLM agent to modules it may control.
    Control,
}

/// Direction of a port relative to its module.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PortDir {
    In,
    Out,
}

/// A reference to a specific port on a specific module.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortRef {
    /// Stable module id this port belongs to.
    pub module_id: u32,
    /// Direction from the module's perspective.
    pub dir: PortDir,
    /// Signal kind (Audio / CV).
    pub kind: PortKind,
    /// Index within the module's ports of this direction+kind.
    /// e.g. a module with two CV ins uses indices 0 and 1.
    pub index: u8,
}

// ─── Cable ────────────────────────────────────────────────────────────────────

/// A colour assigned to a patch cable for visual differentiation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CableColor {
    Teal,
    Amber,
    Rose,
    Violet,
    Lime,
    Cyan,
    Orange,
    Pink,
}

impl CableColor {
    /// All variants in a cycle-friendly order.
    pub const ALL: &'static [Self] = &[
        Self::Teal,
        Self::Amber,
        Self::Rose,
        Self::Violet,
        Self::Lime,
        Self::Cyan,
        Self::Orange,
        Self::Pink,
    ];

    /// egui RGBA colour for rendering.
    pub fn egui_color(self) -> egui::Color32 {
        match self {
            Self::Teal => egui::Color32::from_rgb(0, 200, 180),
            Self::Amber => egui::Color32::from_rgb(255, 190, 40),
            Self::Rose => egui::Color32::from_rgb(240, 80, 110),
            Self::Violet => egui::Color32::from_rgb(180, 80, 240),
            Self::Lime => egui::Color32::from_rgb(140, 220, 60),
            Self::Cyan => egui::Color32::from_rgb(60, 200, 255),
            Self::Orange => egui::Color32::from_rgb(255, 140, 30),
            Self::Pink => egui::Color32::from_rgb(255, 130, 200),
        }
    }
}

/// A virtual patch cable connecting two ports.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Cable {
    pub from: PortRef,
    pub to: PortRef,
    pub color: CableColor,
}

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
    // ── TTS / MC voice modules ────────────────────────────────────────────────
    EspeakNgTts,
    CoquiTts,
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
    // ── Analysis ──────────────────────────────────────────────────────────────
    SpectrumAnalyzer,
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
            Self::DrumKit808 => "DRUM KIT A",
            Self::DrumKit909 => "DRUM KIT B",
            Self::HooverLead => "HOOVER",
            Self::An1xVoice => "AN1X",
            Self::AmenSampler => "AMEN",
            Self::NoiseVoice => "NOISE",
            Self::EspeakNgTts => "TTS ESPEAK",
            Self::CoquiTts => "TTS COQUI",
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
            Self::SpectrumAnalyzer => "SPECTRUM",
            Self::LfoModule => "LFO",
            Self::LlmAgent => "LLM AGENT",
            Self::LlmConsole => "LLM CONSOLE",
            Self::MasterOutput => "MASTER",
        }
    }

    /// Which zone this module belongs to by default.
    pub fn default_zone(self) -> Zone {
        match self {
            Self::StepSequencer | Self::MasterOutput | Self::LlmAgent | Self::LlmConsole => {
                Zone::Global
            }
            Self::AcidBass
            | Self::DrumKit808
            | Self::DrumKit909
            | Self::HooverLead
            | Self::An1xVoice
            | Self::AmenSampler
            | Self::NoiseVoice
            | Self::EspeakNgTts
            | Self::CoquiTts => Zone::Voice,
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
            | Self::SpectrumAnalyzer
            | Self::LfoModule => Zone::FxMod,
        }
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
                | Self::LfoModule
                | Self::LlmAgent
        )
    }
}

// ─── Zone ────────────────────────────────────────────────────────────────────

/// The vertical zone a module lives in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Zone {
    /// Clock, transport, master output — full-width strip at the top.
    Global,
    /// Voice / instrument modules.
    Voice,
    /// FX processors and modulation sources.
    FxMod,
}

// ─── Module ───────────────────────────────────────────────────────────────────

/// A single instantiated module in the rack.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RackModule {
    /// Stable identifier used by `Cable` references.
    pub id: u32,
    pub kind: ModuleKind,
    /// Whether this module contributes to the signal path.
    pub enabled: bool,
    /// Zone the module belongs to (determines which section of the UI it appears in).
    pub zone: Zone,
    /// Position within the zone (left-to-right ordering).  Lower = further left.
    pub slot: u8,
}

impl RackModule {
    pub fn new(id: u32, kind: ModuleKind) -> Self {
        Self {
            id,
            kind,
            enabled: true,
            zone: kind.default_zone(),
            slot: 0,
        }
    }
}

// ─── RackState ───────────────────────────────────────────────────────────────

/// Top-level rack: the ordered list of active modules and their cable connections.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RackState {
    pub modules: Vec<RackModule>,
    pub cables: Vec<Cable>,
    /// Counter for assigning stable module IDs.
    pub next_id: u32,
}

impl RackState {
    /// Return the next free colour for a new cable (cycles through `CableColor::ALL`).
    pub fn next_cable_color(&self) -> CableColor {
        let idx = self.cables.len() % CableColor::ALL.len();
        CableColor::ALL[idx]
    }

    /// Find a module by id.
    pub fn module(&self, id: u32) -> Option<&RackModule> {
        self.modules.iter().find(|m| m.id == id)
    }

    /// Modules in a given zone, sorted by slot.
    pub fn zone_modules(&self, zone: Zone) -> Vec<&RackModule> {
        let mut v: Vec<&RackModule> = self.modules.iter().filter(|m| m.zone == zone).collect();
        v.sort_by_key(|m| m.slot);
        v
    }

    /// Add a module of the given kind, returning its assigned id.
    pub fn add_module(&mut self, kind: ModuleKind) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        // Assign slot = count of existing modules in same zone.
        let slot = self
            .modules
            .iter()
            .filter(|m| m.zone == kind.default_zone())
            .count() as u8;
        let mut m = RackModule::new(id, kind);
        m.slot = slot;
        self.modules.push(m);
        id
    }

    /// Remove a module and any cables connected to it.
    pub fn remove_module(&mut self, id: u32) {
        self.modules.retain(|m| m.id != id);
        self.cables
            .retain(|c| c.from.module_id != id && c.to.module_id != id);
    }

    /// Sort modules within each zone into a canonical order and reassign slot indices.
    pub fn arrange_canonical(&mut self) {
        fn order(kind: ModuleKind) -> u8 {
            match kind {
                ModuleKind::LlmConsole => 0,
                ModuleKind::LlmAgent => 1,
                ModuleKind::StepSequencer => 2,
                ModuleKind::MasterOutput => 3,
                ModuleKind::AcidBass => 10,
                ModuleKind::DrumKit808 => 11,
                ModuleKind::DrumKit909 => 12,
                ModuleKind::HooverLead => 13,
                ModuleKind::An1xVoice => 14,
                ModuleKind::AmenSampler => 15,
                ModuleKind::NoiseVoice => 16,
                ModuleKind::EspeakNgTts | ModuleKind::CoquiTts => 17,
                ModuleKind::FxWaveshaper => 20,
                ModuleKind::FxReverb => 21,
                ModuleKind::FxDelay => 22,
                ModuleKind::FxBitcrush => 23,
                ModuleKind::FxChorus => 24,
                ModuleKind::FxPhaser => 25,
                ModuleKind::FxRingMod => 26,
                ModuleKind::FxEq => 27,
                ModuleKind::FxCompressor => 28,
                ModuleKind::FxTapeSat => 29,
                ModuleKind::FxDrive => 30,
                ModuleKind::FxAutotune => 31,
                ModuleKind::SpectrumAnalyzer => 32,
                ModuleKind::LfoModule => 33,
            }
        }
        for zone in [Zone::Global, Zone::Voice, Zone::FxMod] {
            let mut ids: Vec<(u32, u8)> = self
                .modules
                .iter()
                .filter(|m| m.zone == zone)
                .map(|m| (m.id, order(m.kind)))
                .collect();
            ids.sort_by_key(|&(_, o)| o);
            for (slot, &(id, _)) in ids.iter().enumerate() {
                if let Some(m) = self.modules.iter_mut().find(|m| m.id == id) {
                    m.slot = slot as u8;
                }
            }
        }
    }

    /// Add a cable between two ports (no duplicate check — caller ensures validity).
    pub fn connect(&mut self, from: PortRef, to: PortRef) {
        let color = self.next_cable_color();
        self.cables.push(Cable { from, to, color });
    }

    /// Remove the cable connecting the given two ports (if any).
    pub fn disconnect(&mut self, from: &PortRef, to: &PortRef) {
        self.cables.retain(|c| !(&c.from == from && &c.to == to));
    }
}

impl Default for RackState {
    fn default() -> Self {
        // Default rack mirrors the original fixed layout so projects that
        // pre-date the rack system still render correctly.
        let mut rack = Self {
            modules: Vec::new(),
            cables: Vec::new(),
            next_id: 100, // Reserve low ids for system nodes
        };

        // ── Global zone ──────────────────────────────────────────────────────
        rack.add_module(ModuleKind::StepSequencer);
        rack.add_module(ModuleKind::MasterOutput);
        rack.add_module(ModuleKind::LlmConsole);

        // ── Voice zone ───────────────────────────────────────────────────────
        rack.add_module(ModuleKind::AcidBass);
        rack.add_module(ModuleKind::DrumKit808);
        rack.add_module(ModuleKind::DrumKit909);
        rack.add_module(ModuleKind::HooverLead);
        rack.add_module(ModuleKind::An1xVoice);
        rack.add_module(ModuleKind::AmenSampler);
        rack.add_module(ModuleKind::EspeakNgTts);

        // ── FX + Mod zone — order matches the fixed chain in process_block ───
        rack.add_module(ModuleKind::FxWaveshaper);
        rack.add_module(ModuleKind::FxReverb);
        rack.add_module(ModuleKind::FxDelay);
        rack.add_module(ModuleKind::FxBitcrush);
        rack.add_module(ModuleKind::FxChorus);
        rack.add_module(ModuleKind::FxPhaser);
        rack.add_module(ModuleKind::FxRingMod);
        rack.add_module(ModuleKind::FxEq);
        rack.add_module(ModuleKind::FxCompressor);
        rack.add_module(ModuleKind::FxTapeSat);
        rack.add_module(ModuleKind::FxDrive);
        rack.add_module(ModuleKind::FxAutotune);
        // Default 4 LFO slots.
        rack.add_module(ModuleKind::LfoModule);
        rack.add_module(ModuleKind::LfoModule);
        rack.add_module(ModuleKind::LfoModule);
        rack.add_module(ModuleKind::LfoModule);
        // Default LLM agent
        rack.add_module(ModuleKind::LlmAgent);

        // ── Default cables ────────────────────────────────────────────────────
        // Collect IDs first (no borrow conflict with connect()).
        let find = |kind: ModuleKind| -> Option<u32> {
            rack.modules.iter().find(|m| m.kind == kind).map(|m| m.id)
        };
        let master_id = find(ModuleKind::MasterOutput);
        let seq_id = find(ModuleKind::StepSequencer);

        // Voice → MasterOutput (voice mix bus).
        let voice_ids: Vec<u32> = [
            ModuleKind::AcidBass,
            ModuleKind::DrumKit808,
            ModuleKind::DrumKit909,
            ModuleKind::HooverLead,
            ModuleKind::An1xVoice,
            ModuleKind::AmenSampler,
        ]
        .iter()
        .filter_map(|&k| find(k))
        .collect();

        // TTS default cable: EspeakNgTts → FxReverb (bypasses master bus).
        let tts_id = find(ModuleKind::EspeakNgTts);
        let reverb_id = find(ModuleKind::FxReverb);

        // Serial FX chain (mirrors the hardcoded process_block order).
        let fx_chain: &[ModuleKind] = &[
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
        ];
        let fx_ids: Vec<u32> = fx_chain.iter().filter_map(|&k| find(k)).collect();
        let _ = find; // end closure borrow before mutable connect() calls

        // StepSequencer → each voice (gate/CV)
        if let Some(sid) = seq_id {
            for vid in &voice_ids {
                rack.connect(
                    PortRef {
                        module_id: sid,
                        dir: PortDir::Out,
                        kind: PortKind::Cv,
                        index: 0,
                    },
                    PortRef {
                        module_id: *vid,
                        dir: PortDir::In,
                        kind: PortKind::Cv,
                        index: 0,
                    },
                );
            }
        }

        // TTS → FxReverb
        if let (Some(tid), Some(rid)) = (tts_id, reverb_id) {
            rack.connect(
                PortRef {
                    module_id: tid,
                    dir: PortDir::Out,
                    kind: PortKind::Audio,
                    index: 0,
                },
                PortRef {
                    module_id: rid,
                    dir: PortDir::In,
                    kind: PortKind::Audio,
                    index: 0,
                },
            );
        }

        // Voice → master bus
        if let Some(mid) = master_id {
            for vid in voice_ids {
                rack.connect(
                    PortRef {
                        module_id: vid,
                        dir: PortDir::Out,
                        kind: PortKind::Audio,
                        index: 0,
                    },
                    PortRef {
                        module_id: mid,
                        dir: PortDir::In,
                        kind: PortKind::Audio,
                        index: 0,
                    },
                );
            }
        }

        // FX serial chain: each FX out → next FX in
        for pair in fx_ids.windows(2) {
            rack.connect(
                PortRef {
                    module_id: pair[0],
                    dir: PortDir::Out,
                    kind: PortKind::Audio,
                    index: 0,
                },
                PortRef {
                    module_id: pair[1],
                    dir: PortDir::In,
                    kind: PortKind::Audio,
                    index: 0,
                },
            );
        }

        // LLM Agent → all controllable modules (Control cables)
        let agent_id = rack
            .modules
            .iter()
            .find(|m| m.kind == ModuleKind::LlmAgent)
            .map(|m| m.id);
        if let Some(agent_id) = agent_id {
            let controllable: Vec<u32> = rack
                .modules
                .iter()
                .filter(|m| {
                    !matches!(
                        m.kind,
                        ModuleKind::MasterOutput | ModuleKind::LlmAgent | ModuleKind::LlmConsole
                    )
                })
                .map(|m| m.id)
                .collect();
            for target_id in &controllable {
                rack.connect(
                    PortRef {
                        module_id: agent_id,
                        dir: PortDir::Out,
                        kind: PortKind::Control,
                        index: 0,
                    },
                    PortRef {
                        module_id: *target_id,
                        dir: PortDir::In,
                        kind: PortKind::Control,
                        index: 0,
                    },
                );
            }
        }

        rack
    }
}

// ─── FX routing plan ──────────────────────────────────────────────────────────

/// One processing step in the compiled FX routing plan.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FxStep {
    Waveshaper,
    Reverb,
    Delay,
    Bitcrush,
    Chorus,
    Phaser,
    RingMod,
    Eq,
    Compressor,
    TapeSat,
    Drive,
    Autotune,
}

/// Compiled FX processing order derived from the rack cable graph.
///
/// Computed outside the audio thread and sent via `AudioCommand::SetFxPlan`
/// whenever the rack topology changes.  The audio thread stores this in
/// `DspState` and iterates it in `process_block()`.
#[derive(Clone, Debug, Default)]
pub struct FxPlan {
    /// Global FX chain order (from FX→FX cable topology).
    /// Applied to all voice buses that have no explicit voice_routes entry.
    pub steps: Vec<FxStep>,
    /// Per-voice-bus explicit FX chains (from Voice→FX cable topology).
    ///
    /// Key = voice module kind (AcidBass, DrumKit808, DrumKit909, AmenSampler, …).
    /// Value = ordered FX steps reachable from that voice's explicit cables.
    ///
    /// When non-empty for a voice, the DSP routes that bus through its own
    /// chain instead of the global chain.
    pub voice_routes: HashMap<ModuleKind, Vec<FxStep>>,
}

// ─── LLM rack helpers ─────────────────────────────────────────────────────────

/// Maps an `LfoTarget` to the rack `ModuleKind` it modulates.
/// Used to synthesise visual cables for active LFO slots.
/// Returns `None` for `LfoTarget::None` or targets without a matching module kind.
pub(crate) fn lfo_target_module_kind(target: crate::state::LfoTarget) -> Option<ModuleKind> {
    use crate::state::LfoTarget;
    match target {
        LfoTarget::None => None,
        LfoTarget::BassCutoff
        | LfoTarget::BassResonance
        | LfoTarget::BassPitch
        | LfoTarget::BassVolume => Some(ModuleKind::AcidBass),
        LfoTarget::Kick808Pitch => Some(ModuleKind::DrumKit808),
        LfoTarget::ReverbMix => Some(ModuleKind::FxReverb),
        LfoTarget::DelayTime | LfoTarget::DelayFeedback => Some(ModuleKind::FxDelay),
        LfoTarget::ChorusMix | LfoTarget::ChorusRate => Some(ModuleKind::FxChorus),
    }
}

/// Returns the `PortKind` emitted on a module's primary output.
/// CV sources (LFO, Sequencer) use `Cv`; everything else uses `Audio`.
/// The destination port kind always matches the source.
pub(crate) fn rack_out_port_kind(kind: ModuleKind) -> PortKind {
    match kind {
        ModuleKind::LlmAgent => PortKind::Control,
        ModuleKind::LfoModule | ModuleKind::StepSequencer => PortKind::Cv,
        _ => PortKind::Audio,
    }
}

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
            "bitcrush" | "bit_crush" | "bit crush" | "lofi" | "lo-fi"
        ),
        ModuleKind::FxReverb => matches!(n.as_str(), "reverb" | "verb"),
        ModuleKind::FxDelay => matches!(n.as_str(), "delay" | "echo"),
        ModuleKind::FxChorus => matches!(n.as_str(), "chorus" | "ensemble"),
        ModuleKind::FxPhaser => matches!(n.as_str(), "phaser" | "phase"),
        ModuleKind::FxRingMod => {
            matches!(n.as_str(), "ringmod" | "ring_mod" | "ring mod" | "ring")
        }
        ModuleKind::FxWaveshaper => {
            matches!(n.as_str(), "waveshaper" | "wave_shaper" | "shaper")
        }
        ModuleKind::FxEq => matches!(n.as_str(), "eq" | "equalizer" | "equaliser"),
        ModuleKind::FxCompressor => matches!(n.as_str(), "compressor" | "comp"),
        ModuleKind::FxTapeSat => matches!(
            n.as_str(),
            "tapesat" | "tape_sat" | "tape sat" | "tape" | "saturation"
        ),
        ModuleKind::FxDrive => matches!(n.as_str(), "drive" | "overdrive" | "distortion"),
        ModuleKind::FxAutotune => matches!(
            n.as_str(),
            "autotune" | "auto_tune" | "pitch_correct" | "tune"
        ),
        ModuleKind::LfoModule => matches!(n.as_str(), "lfo"),
        ModuleKind::AcidBass => matches!(n.as_str(), "bass" | "acid" | "303"),
        ModuleKind::DrumKit808 => matches!(n.as_str(), "808" | "kit_a" | "drum_a" | "drums_a"),
        ModuleKind::DrumKit909 => matches!(n.as_str(), "909" | "kit_b" | "drum_b" | "drums_b"),
        ModuleKind::HooverLead => matches!(n.as_str(), "hoover" | "lead"),
        ModuleKind::An1xVoice => matches!(n.as_str(), "an1x" | "an-1x" | "pad" | "synth"),
        ModuleKind::AmenSampler => matches!(n.as_str(), "amen" | "sampler" | "break"),
        ModuleKind::NoiseVoice => matches!(n.as_str(), "noise"),
        ModuleKind::EspeakNgTts | ModuleKind::CoquiTts => {
            matches!(n.as_str(), "espeak" | "coqui" | "tts" | "mc" | "voice")
        }
        ModuleKind::MasterOutput => {
            matches!(n.as_str(), "master" | "master_out" | "out" | "output")
        }
        ModuleKind::StepSequencer => matches!(n.as_str(), "sequencer" | "seq"),
        ModuleKind::LlmAgent => matches!(n.as_str(), "llm" | "agent" | "ai" | "llm_agent"),
        ModuleKind::LlmConsole => matches!(n.as_str(), "console" | "llm_console"),
        ModuleKind::SpectrumAnalyzer => matches!(n.as_str(), "spectrum" | "analyser" | "analyzer"),
    }
}

fn kind_to_fx_step(kind: ModuleKind) -> Option<FxStep> {
    match kind {
        ModuleKind::FxWaveshaper => Some(FxStep::Waveshaper),
        ModuleKind::FxReverb => Some(FxStep::Reverb),
        ModuleKind::FxDelay => Some(FxStep::Delay),
        ModuleKind::FxBitcrush => Some(FxStep::Bitcrush),
        ModuleKind::FxChorus => Some(FxStep::Chorus),
        ModuleKind::FxPhaser => Some(FxStep::Phaser),
        ModuleKind::FxRingMod => Some(FxStep::RingMod),
        ModuleKind::FxEq => Some(FxStep::Eq),
        ModuleKind::FxCompressor => Some(FxStep::Compressor),
        ModuleKind::FxTapeSat => Some(FxStep::TapeSat),
        ModuleKind::FxDrive => Some(FxStep::Drive),
        ModuleKind::FxAutotune => Some(FxStep::Autotune),
        _ => None,
    }
}

/// Build an `FxPlan` from the rack cable graph using a topological sort
/// (Kahn's algorithm) over FX-to-FX audio cable connections.
///
/// Only enabled FX modules that are connected to at least one other FX
/// module (or are solo in the graph) are included.  Modules with no
/// FX-to-FX cables are excluded so the plan is empty when nothing is patched.
pub fn compile_fx_plan(rack: &RackState) -> FxPlan {
    // Collect enabled FX module id → kind.
    let fx_map: HashMap<u32, ModuleKind> = rack
        .modules
        .iter()
        .filter(|m| m.enabled && kind_to_fx_step(m.kind).is_some())
        .map(|m| (m.id, m.kind))
        .collect();

    if fx_map.is_empty() {
        return FxPlan::default();
    }

    // Build adjacency and in-degree over FX→FX audio cables only.
    let mut adj: HashMap<u32, Vec<u32>> = HashMap::new();
    let mut in_degree: HashMap<u32, usize> = fx_map.keys().map(|&id| (id, 0)).collect();

    for cable in &rack.cables {
        if cable.from.kind != PortKind::Audio {
            continue;
        }
        let from_fx = fx_map.contains_key(&cable.from.module_id);
        let to_fx = fx_map.contains_key(&cable.to.module_id);
        if from_fx && to_fx {
            adj.entry(cable.from.module_id)
                .or_default()
                .push(cable.to.module_id);
            *in_degree.entry(cable.to.module_id).or_insert(0) += 1;
        }
    }

    // Kahn's topological sort — stable ordering via sorted initial queue.
    let mut queue: Vec<u32> = {
        let mut q: Vec<u32> = in_degree
            .iter()
            .filter(|(_, deg)| **deg == 0)
            .map(|(id, _)| *id)
            .collect();
        q.sort_unstable();
        q
    };

    let mut ordered: Vec<FxStep> = Vec::with_capacity(fx_map.len());
    while !queue.is_empty() {
        let id = queue.remove(0);
        if let Some(&kind) = fx_map.get(&id)
            && let Some(step) = kind_to_fx_step(kind)
        {
            ordered.push(step);
        }
        if let Some(neighbors) = adj.get(&id) {
            let mut next_ready: Vec<u32> = Vec::new();
            for &nid in neighbors {
                if let Some(deg) = in_degree.get_mut(&nid) {
                    *deg -= 1;
                    if *deg == 0 {
                        next_ready.push(nid);
                    }
                }
            }
            next_ready.sort_unstable();
            queue.extend(next_ready);
        }
    }

    // ── Per-voice FX routes (Voice→FX cables) ────────────────────────────────
    // Build a map: voice module kind → ordered list of FX steps reachable via
    // explicit Voice→FX audio cables.  A voice without such cables has no entry
    // here and falls through to the global chain in the DSP.

    // Voice module kinds that map to DSP buses.
    const VOICE_KINDS: &[ModuleKind] = &[
        ModuleKind::AcidBass,
        ModuleKind::DrumKit808,
        ModuleKind::DrumKit909,
        ModuleKind::HooverLead,
        ModuleKind::An1xVoice,
        ModuleKind::AmenSampler,
        ModuleKind::NoiseVoice,
        ModuleKind::EspeakNgTts,
        ModuleKind::CoquiTts,
    ];

    // Map voice module id → kind for quick lookup.
    let voice_id_map: HashMap<u32, ModuleKind> = rack
        .modules
        .iter()
        .filter(|m| m.enabled && VOICE_KINDS.contains(&m.kind))
        .map(|m| (m.id, m.kind))
        .collect();

    let mut voice_routes: HashMap<ModuleKind, Vec<FxStep>> = HashMap::new();

    for cable in &rack.cables {
        if cable.from.kind != PortKind::Audio {
            continue;
        }
        let voice_kind = match voice_id_map.get(&cable.from.module_id) {
            Some(&k) => k,
            None => continue,
        };
        let first_fx = match fx_map.get(&cable.to.module_id) {
            Some(&k) => k,
            None => continue,
        };
        // BFS/DFS from first_fx through FX→FX adjacency to collect the sub-chain
        // reachable from this voice.
        let first_step = match kind_to_fx_step(first_fx) {
            Some(s) => s,
            None => continue,
        };
        let mut chain: Vec<FxStep> = vec![first_step];
        // Walk adj from first FX module id.
        let mut cur_id = cable.to.module_id;
        // Follow the single-output chain (linear adjacency).
        loop {
            let next_ids = adj.get(&cur_id).map(|v| v.as_slice()).unwrap_or(&[]);
            match next_ids {
                [next_id] => {
                    if let Some(&kind) = fx_map.get(next_id)
                        && let Some(step) = kind_to_fx_step(kind)
                    {
                        chain.push(step);
                        cur_id = *next_id;
                    } else {
                        break;
                    }
                }
                _ => break, // fan-out or end of chain
            }
        }
        voice_routes.entry(voice_kind).or_insert(chain);
    }

    FxPlan {
        steps: ordered,
        voice_routes,
    }
}
