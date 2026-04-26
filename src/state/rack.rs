// ─── state/rack.rs ────────────────────────────────────────────────────────────
// Modular rack state: modules + cables.  Each `RackModule` has a stable id,
// a kind, an enabled flag, and a layout position; cables connect output
// ports to input ports.  Cycle-checked at connect time (FX→FX cycles are
// allowed and lifted into feedback edges by `compile_fx_plan`).

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
    /// Sidechain audio input (ducker / vocoder modulator / sidechain
    /// compressor).  Carries audio but is treated as a tap — recorded
    /// in `sidechain_routes` and read with a one-sample delay, so
    /// cycles are safe by construction.
    SidechainIn,
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
    /// Per-cable gain for audio cables (0.0..=1.5).  Default 1.0 (unity).
    /// For forward audio cables on a voice→FX route, the gain scales the
    /// voice signal before it enters the chain (per-cable send amount).
    /// For back-edges (feedback cables), the gain is further clamped to
    /// `FEEDBACK_GAIN_MAX` (0.95) when compiled into the FX plan so the
    /// loop can't diverge regardless of what the user sets.
    /// Non-audio cables ignore this field.
    #[serde(default = "default_audio_gain")]
    pub audio_gain: f32,
}

fn default_audio_gain() -> f32 {
    1.0
}

/// Maximum feedback-edge gain — keeps FX→FX loops stable regardless of
/// what the user puts on `Cable.audio_gain`.  Picked just shy of 1.0 so
/// long reverb / delay tails can still build but can't run away to +∞.
pub const FEEDBACK_GAIN_MAX: f32 = 0.95;

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
    /// XY-pad expansion: when true, modules whose `kind.supports_xy_pad()`
    /// is true reserve an extra grid row and render an XY pad below their
    /// knobs.  Kinds that don't support an XY pad ignore this flag.
    /// Defaults to false so the rack starts compact and users opt into
    /// pads per-module via the title-bar chevron.
    #[serde(default)]
    pub pad_expanded: bool,
    /// For multi-pair XY pads (3-knob FX), which pair the pad currently
    /// drives: 0 = A/B, 1 = A/C, 2 = B/C.  Persisted so the selection
    /// survives save/restore and is API / LLM addressable.  Ignored by
    /// single-pair pads (and by kinds without a pad).
    #[serde(default)]
    pub pad_pair: u8,
}

impl RackModule {
    pub fn new(id: u32, kind: ModuleKind) -> Self {
        // FX modules start DISABLED so a freshly-added effect can't
        // click the signal at its default wet mix.  The user (or an
        // agent) toggles them on when they want the effect active;
        // until then the module sits in the rack inert.  Voices /
        // analysers / utility modules stay enabled — their "on by
        // default" is the usual expectation.
        let enabled = crate::state::fx_plan::kind_to_fx_step(kind).is_none();
        Self {
            id,
            kind,
            enabled,
            zone: kind.default_zone(),
            slot: 0,
            grid_col: 0,
            grid_row: 0,
            mod_selectors: Vec::new(),
            mod_input_depths: Vec::new(),
            mod_input_invert: Vec::new(),
            pad_expanded: false,
            pad_pair: 0,
        }
    }

    /// Grid (col_span, row_span) taking `pad_expanded` into account.
    /// Returns `(w, h + 1)` when this module supports an XY pad and the
    /// pad is currently expanded; otherwise the base size from `kind.grid_size()`.
    /// Callers that need to respect dynamic overrides like `StepSequencer`
    /// should go through `RackState::effective_grid_size()` instead.
    pub fn grid_size(&self, grid_cols: u8) -> (u8, u8) {
        let (w, h) = self.kind.grid_size(grid_cols);
        if self.kind.supports_xy_pad() && self.pad_expanded {
            (w, h + 1)
        } else {
            (w, h)
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

    /// Whether `module_id` has any audible path through audio cables to a
    /// `MasterOutput` module.  Walks `Audio` cables forward (out→in), only
    /// stepping through enabled modules.  Modules without an audio output
    /// (LFO, LlmAgent, etc.) always return `true` — they contribute via
    /// other channels and shouldn't be flagged "disconnected from master".
    pub fn reaches_master(&self, module_id: u32) -> bool {
        let master_id = match self
            .modules
            .iter()
            .find(|m| m.kind == ModuleKind::MasterOutput)
        {
            Some(m) => m.id,
            None => return false,
        };
        if module_id == master_id {
            return true;
        }
        // Modules without an Audio Out are not part of the audio graph.
        let start = match self.module(module_id) {
            Some(m) => m,
            None => return false,
        };
        let has_audio_out = self.cables.iter().any(|c| {
            c.from.module_id == module_id
                && c.from.dir == PortDir::Out
                && c.from.kind == PortKind::Audio
        }) || matches!(
            start.kind,
            ModuleKind::AcidBass
                | ModuleKind::HooverLead
                | ModuleKind::DrumKit808
                | ModuleKind::DrumKit909
                | ModuleKind::AmenSampler
                | ModuleKind::GranularTexture
                | ModuleKind::GabberKick
                | ModuleKind::NoiseVoice
                | ModuleKind::Theremin
                | ModuleKind::Pendulum
                | ModuleKind::FmOpsVoice
                | ModuleKind::AdditiveVoice
                | ModuleKind::ModalVoice
                | ModuleKind::ChiptuneVoice
                | ModuleKind::VocalVoice
                | ModuleKind::An1xVoice
                | ModuleKind::NeuTts
                | ModuleKind::FxReverb
                | ModuleKind::FxDelay
                | ModuleKind::FxChorus
                | ModuleKind::FxPhaser
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
                | ModuleKind::FxPan
                | ModuleKind::FxWiden
                | ModuleKind::FxFreqShift
                | ModuleKind::FxTremolo
                | ModuleKind::FxVibrato
                | ModuleKind::FxIsoEq
                | ModuleKind::FxDeEsser
                | ModuleKind::FxResBank
                | ModuleKind::FxTapeEcho
                | ModuleKind::FxMultibandComp
                | ModuleKind::FxGrainDelay
                | ModuleKind::FxSpectralGate
        );
        if !has_audio_out {
            return true;
        }
        let mut visited: std::collections::HashSet<u32> = std::collections::HashSet::new();
        let mut stack = vec![module_id];
        while let Some(id) = stack.pop() {
            if !visited.insert(id) {
                continue;
            }
            if id == master_id {
                return true;
            }
            for cable in self.cables.iter().filter(|c| {
                c.from.module_id == id
                    && c.from.dir == PortDir::Out
                    && c.from.kind == PortKind::Audio
                    && c.to.kind == PortKind::Audio
            }) {
                if let Some(dst) = self.module(cable.to.module_id) {
                    // MasterOutput is reachable even if marked disabled —
                    // walking through other disabled modules drops the path.
                    if dst.kind == ModuleKind::MasterOutput || dst.enabled {
                        stack.push(dst.id);
                    }
                }
            }
        }
        false
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
        // New modules start with `pad_expanded: false`, so we only need the
        // base grid height — the extra row is allocated once the user
        // expands the pad (arrange_grid reflows at that point).
        let cols = GRID_COLS as usize;
        let max_rows = 64usize;
        let zone = kind.default_zone();

        // Build occupancy grid from existing modules in the same zone
        let mut occ = vec![vec![false; cols]; max_rows];
        for m in &self.modules {
            if m.zone != zone {
                continue;
            }
            let (mw, mh) = self.effective_grid_size(m);
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

    /// Effective grid size for a specific module instance, combining the
    /// kind's static size with per-module expansion (`pad_expanded`) and
    /// dynamic overrides like the sequencer's visible-lane count.
    pub fn effective_grid_size(&self, m: &RackModule) -> (u8, u8) {
        let (w, h) = m.kind.grid_size(GRID_COLS);
        let h = if m.kind.supports_xy_pad() && m.pad_expanded {
            h + 1
        } else {
            h
        };
        let h = self.dyn_height_override(m.kind).unwrap_or(h);
        (w, h)
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
                // Pluck sits next to Hoover in the voice strip — both
                // are monophonic melodic voices.
                ModuleKind::PluckString => 14,
                // Wavetable next to the other monophonic melodic voices.
                ModuleKind::WavetableVoice => 15,
                // Sample Instrument lives next to Wavetable — both are
                // user-loaded-WAV pitched voices.
                ModuleKind::SampleInstrument => 15,
                ModuleKind::An1xVoice => 15,
                ModuleKind::AmenSampler => 16,
                ModuleKind::NoiseVoice => 17,
                // Theremin sits next to the noise voice — both are
                // "weird sustained drones" tonally.
                ModuleKind::Theremin => 17,
                // Pendulum same family — drone voice with no
                // sequencer trigger, knob-driven beat character.
                ModuleKind::Pendulum => 17,
                // FM operator synth — sequencer-driven voice, sits
                // with the synthesised voices (between AcidBass and
                // HooverLead range).  Bell / FM-bass / E-piano
                // territory complements the AN1X subtractive bank.
                ModuleKind::FmOpsVoice => 13,
                // Additive — sequencer-driven voice; sits next to
                // the FM op synth in the synthesised-voice cluster
                // since both are spectrum-shaping voices that
                // complement the subtractive AN1X.
                ModuleKind::AdditiveVoice => 13,
                // Modal — same cluster as Additive / FM op since
                // it's another spectrum-shaping voice (struck
                // resonator bank instead of partial sum).
                ModuleKind::ModalVoice => 13,
                // Chiptune — synthesised voice cluster with the
                // rest of the spectrum-shaping bank.
                ModuleKind::ChiptuneVoice => 13,
                // Vocal — synthesised voice cluster (formant
                // bank, distinct from NeuTts which lives further
                // along with the sample-style voices).
                ModuleKind::VocalVoice => 13,
                ModuleKind::GranularTexture => 18,
                ModuleKind::NeuTts => 19,
                ModuleKind::FxWaveshaper => 20,
                ModuleKind::FxReverb => 21,
                ModuleKind::FxDelay => 22,
                ModuleKind::FxBitcrush => 23,
                ModuleKind::FxChorus => 24,
                ModuleKind::FxPhaser => 25,
                // Flanger sits adjacent to the phaser in the FX strip — both
                // are LFO-modulated comb-flavour effects, the user reaches
                // for them in the same context.
                ModuleKind::FxFlanger => 25,
                // Comb belongs in the modulation-flavour cluster too —
                // it's a feedback comb tuned to a pitch.
                ModuleKind::FxComb => 26,
                ModuleKind::FxRingMod => 27,
                // Filter / Tilt slot near the EQ family.
                ModuleKind::FxFilter => 28,
                ModuleKind::FxTilt => 29,
                // Transient / Exciter / Limiter — dynamics + mastering
                // tools, near the compressor / tape sat cluster.
                ModuleKind::FxTransient => 32,
                ModuleKind::FxExciter => 33,
                ModuleKind::FxLimiter => 34,
                // Multitap / RevDelay sit next to the regular Delay.
                ModuleKind::FxMultitap => 22,
                ModuleKind::FxRevDelay => 22,
                // Tape stop / Stutter are rhythmic-modulation FX —
                // park them near the bitcrush / drive cluster.
                ModuleKind::FxTapeStop => 23,
                ModuleKind::FxStutter => 23,
                // Freezer parks near the convolution / spectral cluster.
                ModuleKind::FxFreeze => 24,
                ModuleKind::FxEq => 27,
                ModuleKind::FxCompressor => 28,
                // Gate / Vocoder cluster with the dynamics tools — same
                // sidechain idiom as the compressor, users reach for them
                // in the same context.
                ModuleKind::FxGate => 28,
                ModuleKind::FxVocoder => 28,
                ModuleKind::FxTapeSat => 29,
                ModuleKind::FxDrive => 30,
                ModuleKind::FxAutotune => 31,
                ModuleKind::FxPan => 36,
                // Widen sits next to Pan — both stereo master-stage FX.
                ModuleKind::FxWiden => 36,
                // ConvReverb sorts right next to the stock reverb so the two
                // reverbs sit side-by-side in the FX strip.
                ModuleKind::FxConvReverb => 37,
                // ParamEq sorts right after the fixed 3-band EQ so they
                // appear next to each other in the FX strip.
                ModuleKind::FxParamEq => 38,
                // PitchShift next to Autotune (both are pitch-domain FX).
                ModuleKind::FxPitchShift => 39,
                // FreqShift sits next to PitchShift — both pitch-domain.
                ModuleKind::FxFreqShift => 39,
                // Vinyl groups with the saturation / colour cluster
                // (TapeSat / Drive) — same family of analog-character
                // colour effects.
                ModuleKind::FxVinyl => 29,
                // DJ filter sits next to the static Filter — both
                // are LP/HP/BP shaping FX, just with different
                // control surfaces (DJ filter is one-knob morph,
                // FxFilter has cutoff + mode + drive).
                ModuleKind::FxDjFilter => 19,
                // Tremolo lives in the modulation-FX cluster next
                // to Pan / Chorus / Phaser — all internal-LFO-
                // driven movement effects.
                ModuleKind::FxTremolo => 36,
                // Vibrato joins the same cluster — pitch-modulation
                // cousin of Tremolo's amplitude modulation.
                ModuleKind::FxVibrato => 36,
                // ISO EQ groups with the DJ filter — both are
                // performance-oriented filter / band-shaping FX.
                ModuleKind::FxIsoEq => 19,
                // De-esser groups with the dynamics tools — same
                // sidechain idiom as the gate / compressor.
                ModuleKind::FxDeEsser => 28,
                // Resonator bank groups with the comb resonator —
                // same family of pitched-resonance FX, just with
                // six tuned bands instead of one.
                ModuleKind::FxResBank => 9,
                // Tape echo lives next to the stock delay /
                // multitap / revdelay cluster — same delay-line
                // family, distinct character.
                ModuleKind::FxTapeEcho => 11,
                // Multiband compressor sits with the dynamics
                // tools (single-band comp, gate, vocoder).
                ModuleKind::FxMultibandComp => 28,
                // Grain delay groups with the delay-line cluster
                // (delay / multitap / revdelay / tape echo).
                ModuleKind::FxGrainDelay => 11,
                // Spectral gate groups with FxFreeze — both
                // spectral-domain effects, both V1 approximations
                // pending FFT machinery.
                ModuleKind::FxSpectralGate => 24,
                ModuleKind::SpectrumAnalyzer => 32,
                ModuleKind::StereoMeter => 33,
                ModuleKind::ActivityTimeline => 34,
                ModuleKind::LfoModule => 35,
                ModuleKind::CvSequencer => 35,
                ModuleKind::Slew => 35,
                ModuleKind::Quantizer => 35,
                // Bar oscilloscope sorts next to the spectrum module —
                // both are global-bus visualisers; users tend to want
                // them adjacent.
                ModuleKind::BarOscilloscope => 40,
                // Goniometer / vectorscope sits next to the bar scope —
                // both are global-bus stereo / waveform visualisers.
                ModuleKind::StereoVectorscope => 41,
                // LFO scope groups with the LFO modules.
                ModuleKind::LfoScope => 42,
                // Tuner + chord display group with the spectrum cluster.
                ModuleKind::PitchTracker => 43,
                ModuleKind::ChordDisplay => 44,
                // Spectrogram pairs with the bar spectrum — same data,
                // different time-axis treatment.
                ModuleKind::Spectrogram => 46,
                // LUFS sits with the rest of the analysis cluster.
                ModuleKind::LoudnessMeter => 47,
                // Phase wheel pairs with EventStream (transport readout).
                ModuleKind::PhaseWheel => 48,
                // Event stream is melodic / rhythmic activity, parks
                // next to ActivityTimeline.
                ModuleKind::EventStream => 49,
                // Pattern heatmap groups with the activity / event
                // viz cluster — all sequencer-state readouts.
                ModuleKind::PatternHeatmap => 50,
                // Onset grid groups with the heatmap — both
                // sequencer-relative analysis tools.
                ModuleKind::OnsetGrid => 51,
            }
        }
        let cols = GRID_COLS as usize;
        let max_rows = 64usize; // generous upper bound

        for zone in [Zone::Ai, Zone::Global, Zone::Voice, Zone::FxMod] {
            // Collect and sort by canonical order
            let mut ids: Vec<(u32, ModuleKind, bool)> = self
                .modules
                .iter()
                .filter(|m| m.zone == zone)
                .map(|m| (m.id, m.kind, m.pad_expanded))
                .collect();
            ids.sort_by_key(|&(_, k, _)| order(k));

            // 2D occupancy grid
            let mut occ = vec![vec![false; cols]; max_rows];

            for (slot_idx, &(id, kind, pad_expanded)) in ids.iter().enumerate() {
                let (w, h) = kind.grid_size(GRID_COLS);
                let h = if kind.supports_xy_pad() && pad_expanded {
                    h + 1
                } else {
                    h
                };
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
                    let (w, h) = self.effective_grid_size(m);
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

    // `wire_default_cables` lives in `rack_wiring.rs` — it's an `impl
    // RackState` block in a sibling file so this one stays under the
    // 1000-line cap.

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
                && !(self.is_fx_module(cable.from.module_id)
                    && self.is_fx_module(cable.to.module_id))
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

    /// Whether `module_id` is an FX module — used to scope cycle checks
    /// so FX→FX feedback loops are allowed while voice-path cycles stay
    /// rejected.  Returns `false` for unknown ids.
    pub fn is_fx_module(&self, module_id: u32) -> bool {
        self.module(module_id)
            .map(|m| super::fx_plan::kind_is_fx(m.kind))
            .unwrap_or(false)
    }

    /// Returns true if adding an audio cable from → to would create a cycle.
    pub fn would_create_audio_cycle(&self, from_id: u32, to_id: u32) -> bool {
        if from_id == to_id {
            return true;
        }
        // Build adjacency from existing audio cables, then check if to_id can
        // already reach from_id (i.e. adding from→to would close a cycle).
        // Sidechain edges (`to.kind == SidechainIn`) are excluded — they're
        // taps, read with a one-sample delay, and don't propagate signal
        // forward in the chain.
        let mut adj: HashMap<u32, Vec<u32>> = HashMap::new();
        for c in &self.cables {
            if c.from.kind == PortKind::Audio && c.to.kind != PortKind::SidechainIn {
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
        // Loosen the cycle check: FX→FX cycles are allowed (they become
        // feedback routes at compile time with a clamped gain so the loop
        // can't diverge).  Cycles involving a voice / master / LLM module
        // stay rejected — those would be genuine graph errors, not
        // musical feedback.  Sidechain destinations are taps — they read
        // the source with a one-sample delay so no cycle check applies.
        if from.kind == PortKind::Audio
            && to.kind != PortKind::SidechainIn
            && self.would_create_audio_cycle(from.module_id, to.module_id)
            && !(self.is_fx_module(from.module_id) && self.is_fx_module(to.module_id))
        {
            log::warn!(
                "Rejected audio cable {} → {}: would create a non-FX cycle",
                from.module_id,
                to.module_id
            );
            return false;
        }
        let color = self.next_cable_color();
        self.cables.push(Cable {
            from,
            to,
            color,
            audio_gain: default_audio_gain(),
        });
        true
    }

    /// Connect a sidechain cable: `from_id`'s audio out → `to_id`'s
    /// sidechain in.  Tap, not a forward send.
    pub fn connect_sidechain(&mut self, from_id: u32, to_id: u32) -> bool {
        let mk = |id, dir, kind| PortRef {
            module_id: id,
            dir,
            kind,
            index: 0,
        };
        self.connect(
            mk(from_id, PortDir::Out, PortKind::Audio),
            mk(to_id, PortDir::In, PortKind::SidechainIn),
        )
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

// FX routing plan types live in `state/fx_types.rs` now — re-exported
// from state/mod.rs for backwards compatibility with `state::FxStep`,
// `state::FeedbackRoute`, etc.  The runtime compiler is in `fx_plan.rs`.
