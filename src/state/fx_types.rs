// ─── state/fx_types.rs ───────────────────────────────────────────────────────
// Types for the compiled FX routing plan.  Lifted out of `rack.rs` (which
// was at the 1000-line cap) so the rack module can stay focused on
// module + cable management.  The runtime plan compiler lives next door
// in `fx_plan.rs`; these types are shared between both.

use std::collections::HashMap;

use super::module_kind::ModuleKind;

/// One processing step in the compiled FX routing plan.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FxStep {
    Waveshaper,
    Reverb,
    Delay,
    Bitcrush,
    Chorus,
    Phaser,
    Flanger,
    Limiter,
    Filter,
    Comb,
    Tilt,
    Transient,
    Exciter,
    Multitap,
    RevDelay,
    TapeStop,
    Stutter,
    Freeze,
    RingMod,
    Eq,
    Compressor,
    TapeSat,
    Drive,
    Autotune,
    Pan,
    ConvReverb,
    ParamEq,
    PitchShift,
    Gate,
    Vocoder,
    Widen,
    FreqShift,
    Vinyl,
    DjFilter,
    Tremolo,
}

/// Total FxStep variants — sized to pack dense per-FX audio-thread
/// caches (e.g. previous-sample outputs for feedback routes) without
/// allocating a HashMap every block.  Bump this when adding a new
/// `FxStep` variant so its `idx()` value has a slot in the cache.
pub const FX_STEP_COUNT: usize = 35;

impl FxStep {
    /// Dense 0..`FX_STEP_COUNT` index — stable; keep in lock-step with
    /// the variant declaration order above.  Used to index `[f32; N]`
    /// arrays inside `DspState` without any hashing overhead.
    pub fn idx(self) -> usize {
        match self {
            FxStep::Waveshaper => 0,
            FxStep::Reverb => 1,
            FxStep::Delay => 2,
            FxStep::Bitcrush => 3,
            FxStep::Chorus => 4,
            FxStep::Phaser => 5,
            FxStep::Flanger => 6,
            FxStep::Limiter => 7,
            FxStep::Filter => 8,
            FxStep::Comb => 9,
            FxStep::Tilt => 10,
            FxStep::Transient => 11,
            FxStep::Exciter => 12,
            FxStep::Multitap => 13,
            FxStep::RevDelay => 14,
            FxStep::TapeStop => 15,
            FxStep::Stutter => 16,
            FxStep::Freeze => 17,
            FxStep::RingMod => 18,
            FxStep::Eq => 19,
            FxStep::Compressor => 20,
            FxStep::TapeSat => 21,
            FxStep::Drive => 22,
            FxStep::Autotune => 23,
            FxStep::Pan => 24,
            FxStep::ConvReverb => 25,
            FxStep::ParamEq => 26,
            FxStep::PitchShift => 27,
            FxStep::Gate => 28,
            FxStep::Vocoder => 29,
            FxStep::Widen => 30,
            FxStep::FreqShift => 31,
            FxStep::Vinyl => 32,
            FxStep::DjFilter => 33,
            FxStep::Tremolo => 34,
        }
    }
}

/// Where an FX step's sidechain detector reads its signal from.  Resolved
/// from a `to.kind == PortKind::SidechainIn` cable in `compile_fx_plan`.
/// The audio thread looks up the value by FxStep target and reads the
/// corresponding cached sample (one-sample delay) before applying the FX.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SidechainSource {
    /// Voice bus output, keyed by `ModuleKind` (e.g. DrumKit808 → 808 mix).
    Voice(ModuleKind),
    /// Another FX's previous-sample output (one-sample delay, same
    /// machinery as feedback routes).
    Fx(FxStep),
}

/// One compiled feedback route — an FX→FX back-edge in the cable graph.
/// Declares "before processing `target`, read the previous sample output
/// of `source` and mix it into the input with `gain` (clamped 0..=0.95)".
/// Added by `compile_fx_plan` when a cable closes a cycle in the FX graph.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FeedbackRoute {
    /// FX whose *previous*-sample output is fed back.  The audio thread
    /// maintains a one-sample delay line keyed by FxStep, so the edge is
    /// algebraically well-defined (no instantaneous self-reference).
    pub source: FxStep,
    /// FX that receives the fed-back signal mixed into its input.
    pub target: FxStep,
    /// Feedback gain — already clamped to `FEEDBACK_GAIN_MAX` at compile
    /// time so the audio thread can use it directly.
    pub gain: f32,
}

/// One parallel send from a voice bus into an FX sub-chain.  A voice
/// with N entries in `voice_routes` drives N independent sends that
/// mix together at the output stage — each with its own chain and
/// wet-level gain.  Makes classic "voice → reverb at 30% + voice →
/// delay at 50%" patches expressible without a monolithic chain.
#[derive(Clone, Debug, PartialEq)]
pub struct VoiceSend {
    /// Linear FX chain walked forward from the cable's target FX.
    pub chain: Vec<FxStep>,
    /// Cable's `audio_gain` (clamped 0..=1.5) applied to the voice
    /// signal before it enters this chain.  Effectively a per-send wet
    /// knob — cost to the voice's volume is zero since the gain only
    /// scales the branch, not the dry pass.
    pub gain: f32,
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
    /// Per-voice-bus explicit FX sends (from Voice→FX cable topology).
    ///
    /// Key = voice module kind (AcidBass, DrumKit808, DrumKit909, AmenSampler, …).
    /// Value = **list** of parallel sends.  Each entry is an independent
    /// chain + gain pair; the audio thread runs every send and mixes
    /// their outputs.  A voice with no entry (or an empty vec) falls
    /// through to the global chain.
    pub voice_routes: HashMap<ModuleKind, Vec<VoiceSend>>,
    /// FX→FX feedback edges — cables that would close a cycle in the
    /// forward DAG are lifted out here with a one-sample delay.  The audio
    /// thread reads each source's previous-sample output and mixes it into
    /// the target's input before the target processes the current sample.
    pub feedback_routes: Vec<FeedbackRoute>,
    /// Sidechain edges (`to.kind == PortKind::SidechainIn`).  Keyed by
    /// the target FxStep so the audio thread can look up the sidechain
    /// source for each step in O(1).  When a step has no entry, the
    /// FX's sidechain input is treated as silent — gate / vocoder
    /// detectors fall back to the main signal so unconnected sidechain
    /// FX still behave gracefully (gate becomes a noise gate, vocoder
    /// becomes self-modulating).
    pub sidechain_routes: HashMap<FxStep, SidechainSource>,
}
