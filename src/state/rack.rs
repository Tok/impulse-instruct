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
    /// Per-knob modulation input (LFO → target knob).  Distinct from Cv so
    /// the rack UI and fx_plan can treat mod cables separately.
    Mod,
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

// `ModuleKind`, `Zone`, and `GRID_COLS` live in state/module_kind.rs.
// Re-exported here so `super::rack::{ModuleKind, Zone, GRID_COLS}` keeps
// working for all existing callers after the extraction.
pub use super::module_kind::{GRID_COLS, ModuleKind, Zone};

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
    /// Per-Selector slot: list of currently-active LfoTargets (multi-select).
    /// Indexed by selector position; an empty inner list = no targets active
    /// for that slot.
    #[serde(default)]
    pub mod_selectors: Vec<Vec<crate::state::LfoTarget>>,
    /// Per-mod-input depth (0..1).  Indexed by `mod_inputs(kind)` slot
    /// position — applies to both Fixed and Selector slots.  Empty / short
    /// vec defaults each missing entry to 1.0.
    #[serde(default)]
    pub mod_input_depths: Vec<f32>,
    /// Per-mod-input polarity flag (true = invert).  When true the depth
    /// is negated before being multiplied with the LFO value, so the
    /// modulation pulls the target down where it would normally push up.
    #[serde(default)]
    pub mod_input_invert: Vec<bool>,
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
            mod_selectors: Vec::new(),
            mod_input_depths: Vec::new(),
            mod_input_invert: Vec::new(),
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
    /// Dynamic height override for the sequencer (adapts to visible lanes).
    /// Updated each frame by rack_canvas via `set_sequencer_rows()`.
    /// Not persisted — recomputed on every render.
    #[serde(default, skip)]
    pub dyn_sequencer_rows: Option<u8>,
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
    /// Automatically finds the first free grid position in the module's zone
    /// so modules never stack on top of each other.
    pub fn add_module(&mut self, kind: ModuleKind) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        let zone = kind.default_zone();
        let slot = self.modules.iter().filter(|m| m.zone == zone).count() as u8;
        let (gc, gr) = self.find_free_position(kind);
        let mut m = RackModule::new(id, kind);
        m.slot = slot;
        m.grid_col = gc;
        m.grid_row = gr;
        self.modules.push(m);
        // Re-run the full layout so newly added modules get the center-bias
        // pass (keeps the zone visually balanced instead of piling everything
        // against the left edge).
        self.arrange_grid();
        id
    }

    /// Find the first free (col, row) position in the given kind's zone where
    /// a module of its size fits without overlapping existing modules.
    /// Returns (0, 0) as a fallback if the zone is completely full.
    fn find_free_position(&self, kind: ModuleKind) -> (u8, u8) {
        let (w, h) = kind.grid_size(GRID_COLS);
        let cols = GRID_COLS as usize;
        let max_rows = 64usize;
        let zone = kind.default_zone();

        // Build occupancy grid from existing modules in the same zone
        let mut occ = vec![vec![false; cols]; max_rows];
        for m in &self.modules {
            if m.zone != zone {
                continue;
            }
            let (mw, static_h) = m.kind.grid_size(GRID_COLS);
            let mh = self.dyn_height_override(m.kind).unwrap_or(static_h);
            for dr in 0..mh as usize {
                for dc in 0..mw as usize {
                    let r = m.grid_row as usize + dr;
                    let c = m.grid_col as usize + dc;
                    if r < max_rows && c < cols {
                        occ[r][c] = true;
                    }
                }
            }
        }

        // Scan top-to-bottom, left-to-right for first free block
        let w = w as usize;
        let h = self.dyn_height_override(kind).unwrap_or(h) as usize;
        if w == 0 || w > cols || h == 0 {
            return (0, 0);
        }
        for r in 0..(max_rows - h) {
            for c in 0..=(cols - w) {
                let fits = (0..h).all(|dr| (0..w).all(|dc| !occ[r + dr][c + dc]));
                if fits {
                    return (c as u8, r as u8);
                }
            }
        }
        (0, 0)
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
    /// Dynamic grid height override for modules whose size depends on their
    /// content. Currently only used for StepSequencer (grows with visible lanes).
    /// The caller (rack_canvas) updates this via `set_sequencer_dyn_rows()`.
    fn dyn_height_override(&self, kind: ModuleKind) -> Option<u8> {
        if kind == ModuleKind::StepSequencer {
            self.dyn_sequencer_rows
        } else {
            None
        }
    }

    pub fn arrange_grid(&mut self) {
        fn order(kind: ModuleKind) -> u8 {
            // Within each zone: full-width strips first, then smaller modules pack
            // into the free cells below. AI zone: console above, agents pack under.
            // MAIN AUDIO zone: sequencer, then master.
            match kind {
                ModuleKind::LlmConsole => 0,
                ModuleKind::LlmAgent => 1,
                ModuleKind::StepSequencer => 2,
                ModuleKind::MasterOutput => 3,
                // Canonical voice order in the voice zone: put the bass between
                // the two drum kits so the melodic voice sits centered between
                // low (808) and high (909) drums — also matches its pitch
                // register, which lives above 808 kicks and below 909 hats.
                ModuleKind::DrumKit808 => 10,
                ModuleKind::AcidBass => 11,
                ModuleKind::DrumKit909 => 12,
                // Gabber kick sits next to the drum kits — it's a drum voice.
                ModuleKind::GabberKick => 13,
                ModuleKind::HooverLead => 14,
                ModuleKind::An1xVoice => 15,
                ModuleKind::AmenSampler => 16,
                ModuleKind::NoiseVoice => 17,
                ModuleKind::GranularTexture => 18,
                ModuleKind::NeuTts => 19,
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
                ModuleKind::FxPan => 36,
                ModuleKind::SpectrumAnalyzer => 32,
                ModuleKind::StereoMeter => 33,
                ModuleKind::ActivityTimeline => 34,
                ModuleKind::LfoModule => 35,
            }
        }
        let cols = GRID_COLS as usize;
        let max_rows = 64usize; // generous upper bound

        for zone in [Zone::Ai, Zone::Global, Zone::Voice, Zone::FxMod] {
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
                let h = self.dyn_height_override(kind).unwrap_or(h);
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

    /// Wire standard default cables for whichever modules are present:
    /// seq→voices (CV), voices→master (audio), FX serial chain, TTS→reverb,
    /// and agent→all controllable (control). Safe to call on any rack — only
    /// wires modules that actually exist.
    pub fn wire_default_cables(&mut self) {
        let find = |modules: &[RackModule], kind: ModuleKind| -> Option<u32> {
            modules.iter().find(|m| m.kind == kind).map(|m| m.id)
        };
        let master_id = find(&self.modules, ModuleKind::MasterOutput);
        let seq_id = find(&self.modules, ModuleKind::StepSequencer);
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
        .filter_map(|&k| find(&self.modules, k))
        .collect();
        let tts_id = find(&self.modules, ModuleKind::NeuTts);
        let reverb_id = find(&self.modules, ModuleKind::FxReverb);
        let fx_ids: Vec<u32> = [
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
            ModuleKind::FxPan,
        ]
        .iter()
        .filter_map(|&k| find(&self.modules, k))
        .collect();

        // Seq → voices (CV)
        if let Some(sid) = seq_id {
            for vid in &voice_ids {
                self.connect(
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
        // TTS → reverb
        if let (Some(tid), Some(rid)) = (tts_id, reverb_id) {
            self.connect(
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
        // Voices → master
        if let Some(mid) = master_id {
            for vid in &voice_ids {
                self.connect(
                    PortRef {
                        module_id: *vid,
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
        // FX serial chain
        for pair in fx_ids.windows(2) {
            self.connect(
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
        // Agent → all controllable
        let agent_id = find(&self.modules, ModuleKind::LlmAgent);
        if let Some(aid) = agent_id {
            let targets: Vec<u32> = self
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
            for tid in &targets {
                self.connect_control(aid, *tid);
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
            dyn_sequencer_rows: None,
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
        rack.add_module(ModuleKind::NeuTts);

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

        rack.wire_default_cables();
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
    Pan,
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
