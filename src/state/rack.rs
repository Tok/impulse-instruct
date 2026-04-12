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

use std::collections::{HashMap, HashSet, VecDeque};

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
    Gray,
    Slate,
    Silver,
    Ash,
    Stone,
    Iron,
    Pewter,
    Smoke,
}

impl CableColor {
    /// All variants in a cycle-friendly order.
    pub const ALL: &'static [Self] = &[
        Self::Gray,
        Self::Slate,
        Self::Silver,
        Self::Ash,
        Self::Stone,
        Self::Iron,
        Self::Pewter,
        Self::Smoke,
    ];

    /// egui RGBA colour for rendering.
    pub fn egui_color(self) -> egui::Color32 {
        match self {
            Self::Gray => egui::Color32::from_rgb(90, 90, 90),
            Self::Slate => egui::Color32::from_rgb(120, 120, 120),
            Self::Silver => egui::Color32::from_rgb(160, 160, 160),
            Self::Ash => egui::Color32::from_rgb(140, 140, 140),
            Self::Stone => egui::Color32::from_rgb(100, 100, 100),
            Self::Iron => egui::Color32::from_rgb(75, 75, 75),
            Self::Pewter => egui::Color32::from_rgb(110, 110, 110),
            Self::Smoke => egui::Color32::from_rgb(130, 130, 130),
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
    GranularTexture,
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
            Self::LlmConsole => (grid_cols, 1),
            Self::MasterOutput => (grid_cols, 1),
            Self::AcidBass => (4, 7),
            Self::DrumKit808 => (3, 3),
            Self::DrumKit909 => (4, 3),
            Self::HooverLead => (4, 2),
            Self::An1xVoice => (6, 6),
            Self::AmenSampler => (3, 1),
            Self::NoiseVoice => (2, 1),
            Self::GranularTexture => (3, 1),
            Self::LlmAgent => (3, 2),
            Self::EspeakNgTts | Self::CoquiTts => (2, 3),
            Self::SpectrumAnalyzer => (4, 2),
            Self::ActivityTimeline => (4, 2),
            Self::StereoMeter => (2, 1),
            Self::LfoModule => (2, 2),
            // FX modules — exhaustive so new variants cause a compile error
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
            | Self::FxAutotune => (2, 1),
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
            | Self::GranularTexture
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
            | Self::StereoMeter
            | Self::ActivityTimeline
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

/// Fixed grid column count for the rack layout.
pub const GRID_COLS: u8 = 12;

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
    /// Grid column (0-based) in the zone's 12-column grid.
    #[serde(default)]
    pub grid_col: u8,
    /// Grid row (0-based) in the zone's grid.
    #[serde(default)]
    pub grid_row: u8,
}

impl RackModule {
    pub fn new(id: u32, kind: ModuleKind) -> Self {
        Self {
            id,
            kind,
            enabled: true,
            zone: kind.default_zone(),
            slot: 0,
            grid_col: 0,
            grid_row: 0,
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

    /// Sort modules into canonical order and pack onto the grid.
    pub fn arrange_canonical(&mut self) {
        // Canonical sort + grid bin-packing in one call.
        self.arrange_grid();
    }

    /// Bin-pack modules onto the 12-column grid within each zone.
    /// Scans top-to-bottom, left-to-right for the first position where
    /// the module's (w, h) span fits without overlapping existing modules.
    pub fn arrange_grid(&mut self) {
        fn order(kind: ModuleKind) -> u8 {
            // Same canonical order as arrange_canonical
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
                ModuleKind::GranularTexture => 17,
                ModuleKind::EspeakNgTts | ModuleKind::CoquiTts => 18,
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
                ModuleKind::StereoMeter => 33,
                ModuleKind::ActivityTimeline => 34,
                ModuleKind::LfoModule => 35,
            }
        }
        let cols = GRID_COLS as usize;
        let max_rows = 64usize; // generous upper bound

        for zone in [Zone::Global, Zone::Voice, Zone::FxMod] {
            // Collect and sort by canonical order
            let mut ids: Vec<(u32, ModuleKind)> = self
                .modules
                .iter()
                .filter(|m| m.zone == zone)
                .map(|m| (m.id, m.kind))
                .collect();
            ids.sort_by_key(|&(_, k)| order(k));

            // 2D occupancy grid
            let mut occ = vec![vec![false; cols]; max_rows];

            for (slot_idx, &(id, kind)) in ids.iter().enumerate() {
                let (w, h) = kind.grid_size(GRID_COLS);
                let w = w as usize;
                let h = h as usize;

                // Find first free position (top-to-bottom, left-to-right)
                let mut placed = false;
                'scan: for r in 0..max_rows - h {
                    for c in 0..=cols - w {
                        // Check if w×h block is free
                        let fits = (0..h).all(|dr| (0..w).all(|dc| !occ[r + dr][c + dc]));
                        if fits {
                            // Mark occupied
                            for dr in 0..h {
                                for dc in 0..w {
                                    occ[r + dr][c + dc] = true;
                                }
                            }
                            if let Some(m) = self.modules.iter_mut().find(|m| m.id == id) {
                                m.grid_col = c as u8;
                                m.grid_row = r as u8;
                                m.slot = slot_idx as u8;
                            }
                            placed = true;
                            break 'scan;
                        }
                    }
                }
                if !placed {
                    // Shouldn't happen with 64 rows, but fallback to (0, 0)
                    if let Some(m) = self.modules.iter_mut().find(|m| m.id == id) {
                        m.grid_col = 0;
                        m.grid_row = 0;
                        m.slot = slot_idx as u8;
                    }
                }
            }

            // ── Center-bias pass: shift row bands toward the center ──────
            // Find the rightmost occupied column per row, then group rows
            // connected by multi-row modules and apply a uniform shift.
            let zone_mods: Vec<(u32, u8, u8, u8, u8)> = self
                .modules
                .iter()
                .filter(|m| m.zone == zone)
                .map(|m| {
                    let (w, h) = m.kind.grid_size(GRID_COLS);
                    (m.id, m.grid_col, m.grid_row, w, h)
                })
                .collect();
            if zone_mods.is_empty() {
                continue;
            }
            // Union-find to group rows linked by tall modules
            let used_rows = zone_mods
                .iter()
                .map(|&(_, _, r, _, h)| (r + h) as usize)
                .max()
                .unwrap_or(0);
            let mut parent: Vec<usize> = (0..used_rows).collect();
            fn find(p: &mut [usize], x: usize) -> usize {
                if p[x] != x {
                    p[x] = find(p, p[x]);
                }
                p[x]
            }
            fn union(p: &mut [usize], a: usize, b: usize) {
                let ra = find(p, a);
                let rb = find(p, b);
                if ra != rb {
                    p[rb] = ra;
                }
            }
            for &(_, _, r, _, h) in &zone_mods {
                for dr in 1..h {
                    union(&mut parent, r as usize, (r + dr) as usize);
                }
            }
            // Compute max right edge per row-band
            let mut band_right: std::collections::HashMap<usize, u8> =
                std::collections::HashMap::new();
            for &(_, c, r, w, h) in &zone_mods {
                let right = c + w;
                for dr in 0..h {
                    let band = find(&mut parent, (r + dr) as usize);
                    let entry = band_right.entry(band).or_insert(0);
                    *entry = (*entry).max(right);
                }
            }
            // Apply centering shift per module
            for &(id, _, r, _, _) in &zone_mods {
                let band = find(&mut parent, r as usize);
                let right = band_right.get(&band).copied().unwrap_or(cols as u8);
                if right < cols as u8 {
                    let shift = (cols as u8 - right) / 2;
                    if shift > 0
                        && let Some(m) = self.modules.iter_mut().find(|m| m.id == id)
                    {
                        m.grid_col += shift;
                    }
                }
            }
        }
    }

    /// Remove audio cables that participate in cycles.  Non-audio cables are
    /// kept unconditionally.  Returns the number of cables removed.
    pub fn strip_audio_cycles(&mut self) -> usize {
        let before = self.cables.len();
        // Rebuild cables, keeping only those that don't create a cycle when
        // added incrementally (same logic as connect()).
        let old_cables = std::mem::take(&mut self.cables);
        for cable in old_cables {
            if cable.from.kind == PortKind::Audio
                && self.would_create_audio_cycle(cable.from.module_id, cable.to.module_id)
            {
                log::warn!(
                    "Stripped cyclic audio cable {} → {} during load",
                    cable.from.module_id,
                    cable.to.module_id
                );
                continue;
            }
            self.cables.push(cable);
        }
        before - self.cables.len()
    }

    /// Returns true if adding an audio cable from → to would create a cycle.
    pub fn would_create_audio_cycle(&self, from_id: u32, to_id: u32) -> bool {
        if from_id == to_id {
            return true;
        }
        // Build adjacency from existing audio cables, then check if to_id can
        // already reach from_id (i.e. adding from→to would close a cycle).
        let mut adj: HashMap<u32, Vec<u32>> = HashMap::new();
        for c in &self.cables {
            if c.from.kind == PortKind::Audio {
                adj.entry(c.from.module_id)
                    .or_default()
                    .push(c.to.module_id);
            }
        }
        // BFS from to_id — can we reach from_id?
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        queue.push_back(to_id);
        while let Some(node) = queue.pop_front() {
            if node == from_id {
                return true;
            }
            if visited.insert(node)
                && let Some(neighbors) = adj.get(&node)
            {
                queue.extend(neighbors);
            }
        }
        false
    }

    /// Add a cable between two ports. Rejects duplicates and audio cables that
    /// would create a cycle in the signal graph. Returns `true` if the cable was added.
    pub fn connect(&mut self, from: PortRef, to: PortRef) -> bool {
        // Reject duplicate cables (same from/to port pair)
        if self.cables.iter().any(|c| c.from == from && c.to == to) {
            return false;
        }
        if from.kind == PortKind::Audio
            && self.would_create_audio_cycle(from.module_id, to.module_id)
        {
            log::warn!(
                "Rejected audio cable {} → {}: would create a cycle",
                from.module_id,
                to.module_id
            );
            return false;
        }
        let color = self.next_cable_color();
        self.cables.push(Cable { from, to, color });
        true
    }

    /// Connect a control cable from `from_id` (Out) to `to_id` (In).
    /// Shorthand for the 8-line PortRef boilerplate used when wiring LLM agents.
    pub fn connect_control(&mut self, from_id: u32, to_id: u32) {
        self.connect(
            PortRef {
                module_id: from_id,
                dir: PortDir::Out,
                kind: PortKind::Control,
                index: 0,
            },
            PortRef {
                module_id: to_id,
                dir: PortDir::In,
                kind: PortKind::Control,
                index: 0,
            },
        );
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
        rack.add_module(ModuleKind::NoiseVoice);
        rack.add_module(ModuleKind::GranularTexture);
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
            ModuleKind::NoiseVoice,
            ModuleKind::GranularTexture,
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
                rack.connect_control(agent_id, *target_id);
            }
        }

        rack.arrange_grid();
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
        LfoTarget::PhaserRate | LfoTarget::PhaserDepth => Some(ModuleKind::FxPhaser),
        LfoTarget::DistortionDrive => Some(ModuleKind::FxWaveshaper),
        LfoTarget::MasterVolume => Some(ModuleKind::MasterOutput),
        LfoTarget::An1xCutoff | LfoTarget::An1xPitch => Some(ModuleKind::An1xVoice),
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
