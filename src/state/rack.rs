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

use serde::{Deserialize, Serialize};

// ─── Port types ───────────────────────────────────────────────────────────────

/// Signal kind carried by a port.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PortKind {
    /// Stereo audio signal.
    Audio,
    /// Mono CV signal (0–1).  Used for LFO targets, gate, pitch CV, etc.
    Cv,
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
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModuleKind {
    // ── Voice modules ─────────────────────────────────────────────────────────
    AcidBass,
    DrumKit808,
    DrumKit909,
    HooverLead,
    An1xVoice,
    AmenSampler,
    NoiseVoice,
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
    // ── Modulation ────────────────────────────────────────────────────────────
    LfoModule,
    // ── Utility ───────────────────────────────────────────────────────────────
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
            Self::LfoModule => "LFO",
            Self::MasterOutput => "MASTER",
        }
    }

    /// Which zone this module belongs to by default.
    pub fn default_zone(self) -> Zone {
        match self {
            Self::StepSequencer | Self::MasterOutput => Zone::Global,
            Self::AcidBass
            | Self::DrumKit808
            | Self::DrumKit909
            | Self::HooverLead
            | Self::An1xVoice
            | Self::AmenSampler
            | Self::NoiseVoice => Zone::Voice,
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
                | Self::LfoModule
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

        // ── Voice zone ───────────────────────────────────────────────────────
        rack.add_module(ModuleKind::AcidBass);
        rack.add_module(ModuleKind::DrumKit808);
        rack.add_module(ModuleKind::DrumKit909);
        rack.add_module(ModuleKind::HooverLead);
        rack.add_module(ModuleKind::An1xVoice);
        rack.add_module(ModuleKind::AmenSampler);

        // ── FX + Mod zone ────────────────────────────────────────────────────
        rack.add_module(ModuleKind::FxReverb);
        rack.add_module(ModuleKind::FxDelay);
        rack.add_module(ModuleKind::FxChorus);
        rack.add_module(ModuleKind::FxPhaser);
        rack.add_module(ModuleKind::FxEq);
        rack.add_module(ModuleKind::FxCompressor);
        rack.add_module(ModuleKind::FxTapeSat);
        rack.add_module(ModuleKind::FxDrive);
        rack.add_module(ModuleKind::FxWaveshaper);
        rack.add_module(ModuleKind::FxBitcrush);
        rack.add_module(ModuleKind::FxRingMod);
        // Default 4 LFO slots.
        rack.add_module(ModuleKind::LfoModule);
        rack.add_module(ModuleKind::LfoModule);
        rack.add_module(ModuleKind::LfoModule);
        rack.add_module(ModuleKind::LfoModule);

        // ── Default cables — every voice wired to master out ─────────────────
        // Collect IDs first (no borrow conflict with connect()).
        let find = |kind: ModuleKind| -> Option<u32> {
            rack.modules.iter().find(|m| m.kind == kind).map(|m| m.id)
        };
        let master_id = find(ModuleKind::MasterOutput);
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
        let _ = find; // end closure borrow before mutable connect() calls

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

        rack
    }
}
