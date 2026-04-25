// ─── Sequencer state ─────────────────────────────────────────────────────────
// Step / TB303Step / SequencerState — the per-pattern data the sequencer
// thread reads to advance the clock.  Lives here rather than alongside the
// clock in `src/sequencer/` because it's part of AppState and needs to be
// serialisable / LLM-writable.
use serde::{Deserialize, Serialize};

use super::drums::DrumVoice;
use super::music::Scale;
use super::{MAX_BASS_VOICES, MAX_STEPS};

// ─── Bass pattern defaults (used by SequencerState::default + serde) ─────────

fn default_bass_patterns() -> Vec<Vec<TB303Step>> {
    // Empty patterns for every voice.  `TB303Step::default()` is
    // inactive, so nothing fires until the agent / scenario writes
    // real notes.  Keeping a hardcoded starter pattern here (it used
    // to be an A-minor riff on voice 0) meant fresh sessions loaded
    // with leftover sound — confusing when you want a blank canvas,
    // and flatly wrong for MIDI imports where the real notes land
    // via `bass_patterns` on the pattern_bank slots.  Genre-specific
    // example patterns still live in `seed_patterns` as prompt
    // reference only; we never feed them into state directly.
    vec![vec![TB303Step::default(); MAX_STEPS]; MAX_BASS_VOICES]
}

fn default_bass_voice_steps() -> Vec<usize> {
    vec![16usize; MAX_BASS_VOICES]
}

fn default_bass_voice_enabled() -> [bool; MAX_BASS_VOICES] {
    let mut arr = [false; MAX_BASS_VOICES];
    arr[0] = true;
    arr
}

fn default_step_division() -> u8 {
    4
}

fn default_pluck_pattern() -> Vec<TB303Step> {
    vec![TB303Step::default(); MAX_STEPS]
}

fn default_pluck_steps() -> usize {
    32
}

fn default_wavetable_pattern() -> Vec<TB303Step> {
    vec![TB303Step::default(); MAX_STEPS]
}

fn default_wavetable_steps() -> usize {
    32
}

fn default_sample_pattern() -> Vec<TB303Step> {
    vec![TB303Step::default(); MAX_STEPS]
}

fn default_sample_steps() -> usize {
    32
}

// ─── Step types ──────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct Step {
    pub active: bool,
    pub velocity: f32,    // 0–1
    pub probability: f32, // 0–1: chance the step fires (1.0 = always, 0.5 = 50%)
    #[serde(default = "default_ratchet")]
    pub ratchet: u8, // 1 = single hit, 2/3/4 = N sub-hits per step
    /// Slice index for sample-playback voices (AmenSampler).  0 = auto
    /// (the voice picks the next slice each trigger); 1..=16 = explicit.
    /// Ignored by purely synthesised drum voices.
    #[serde(default)]
    pub slice: u8,
    /// Conditional trigger — fire only every Nth voice cycle.  Stored
    /// as a 2-bit field: 0 = always fire (the default that keeps every
    /// existing pattern unchanged), 1 = every 2nd cycle, 2 = every
    /// 3rd, 3 = every 4th.  Inspired by Monome-style evolving
    /// patterns where a conditional fill develops over multiple
    /// passes through the same loop.
    #[serde(default)]
    pub cond: u8,
}

fn default_ratchet() -> u8 {
    1
}

impl Default for Step {
    fn default() -> Self {
        Self {
            active: false,
            velocity: 1.0,
            probability: 1.0,
            ratchet: 1,
            slice: 0,
            cond: 0,
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct TB303Step {
    pub active: bool,
    pub note: u8, // MIDI note number
    /// Accent intensity 0..=1.  Drives amp-peak lift on the 303 voice
    /// proportionally (stronger accent → louder hit, bigger dot in UI).
    /// Values ≤ 0 are "no accent"; any positive value lifts the voice.
    /// Deserialises from legacy `bool` for project-file backwards compat.
    #[serde(default, deserialize_with = "de_bool_or_f32")]
    pub accent: f32,
    /// Slide amount 0..=1 — controls glide length into this step.  When
    /// the next active step fires while this flag is > 0, the pitch glide
    /// time is proportional to the value (0 = no glide, 1 = full glide).
    /// Deserialises from legacy `bool` for project-file backwards compat.
    #[serde(default, deserialize_with = "de_bool_or_f32")]
    pub slide: f32,
    pub gate: f32, // 0–1 gate length ratio
    /// Per-step stereo pan, -1.0 = hard left, 0.0 = centre, 1.0 = hard right.
    /// Applied at trigger time on top of the voice's static pan setting.
    /// Defaults to 0 so old patterns deserialize unchanged.
    #[serde(default)]
    pub pan: f32,
    /// Conditional trigger — see `Step::cond` for the field meaning.
    /// Lets melodic voices participate in the same Monome-style
    /// every-Nth-cycle gating as drum hits.
    #[serde(default)]
    pub cond: u8,
}

/// Accept either a legacy `bool` (true→1.0, false→0.0) or a numeric
/// value in [0, 1].  Keeps old project JSON round-tripping through the
/// new f32 fields without a migration step.
fn de_bool_or_f32<'de, D>(deserializer: D) -> Result<f32, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::Deserialize;
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum BoolOrF32 {
        B(bool),
        F(f32),
    }
    match BoolOrF32::deserialize(deserializer)? {
        BoolOrF32::B(b) => Ok(if b { 1.0 } else { 0.0 }),
        BoolOrF32::F(f) => Ok(f.clamp(0.0, 1.0)),
    }
}

impl Default for TB303Step {
    fn default() -> Self {
        Self {
            active: false,
            note: 36,
            accent: 0.0,
            slide: 0.0,
            gate: 0.5,
            pan: 0.0,
            cond: 0,
        }
    }
}

// ─── Pattern container ───────────────────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SequencerState {
    pub bpm: f32,
    pub steps: usize, // 1–64, active step count
    pub current_step: usize,
    pub running: bool,
    pub swing: f32,       // 0–1: 0=straight, 0.5=strong shuffle (75/25 triplet feel)
    pub time_sig_num: u8, // beats per bar (2–9, default 4); denominator always /4
    /// Step subdivisions per beat.  4 = 16th-note grid (the pre-existing
    /// behaviour, default); 8 = 32nd-note grid; 2 = 8th-note grid.  The
    /// sequencer clock and MIDI export both read this to derive samples
    /// per step and SMF ticks per step respectively.  Bumping it lets a
    /// pattern address faster subdivisions without changing BPM — e.g.
    /// importing a Bach Presto at a 32nd grid preserves 32nd-note runs
    /// that would otherwise collide on a 16th grid.
    #[serde(default = "default_step_division")]
    pub step_division: u8,
    pub drum_patterns: std::collections::HashMap<DrumVoice, Vec<Step>>,
    pub bass_pattern: Vec<TB303Step>,
    pub hoover_pattern: Vec<TB303Step>,
    pub an1x_pattern: Vec<TB303Step>,
    #[serde(default = "default_pluck_pattern")]
    pub pluck_pattern: Vec<TB303Step>,
    #[serde(default = "default_wavetable_pattern")]
    pub wavetable_pattern: Vec<TB303Step>,
    #[serde(default = "default_sample_pattern")]
    pub sample_pattern: Vec<TB303Step>,
    /// Tonic note (0=C … 11=B). Used for scale highlighting and LLM music theory.
    pub root_note: u8,
    /// Active scale / mode for this pattern.
    pub scale: Scale,
    /// When true, LLM-provided bass_notes are snapped to the active scale.
    pub scale_snap: bool,
    /// Per-voice step counts for polyrhythm. Each voice loops independently at
    /// its own length; voices not present here default to `steps`.
    pub drum_steps: std::collections::HashMap<DrumVoice, usize>,
    /// Independent step counts for bass, hoover, and AN1X lanes.
    pub bass_steps: usize,
    pub hoover_steps: usize,
    pub an1x_steps: usize,
    #[serde(default = "default_pluck_steps")]
    pub pluck_steps: usize,
    #[serde(default = "default_wavetable_steps")]
    pub wavetable_steps: usize,
    #[serde(default = "default_sample_steps")]
    pub sample_steps: usize,
    /// Per-voice bass patterns for multi-voice support. Voice 0 mirrors `bass_pattern`.
    #[serde(default = "default_bass_patterns")]
    pub bass_patterns: Vec<Vec<TB303Step>>,
    /// Per-voice step counts for bass voices. Voice 0 mirrors `bass_steps`.
    #[serde(default = "default_bass_voice_steps")]
    pub bass_voice_steps: Vec<usize>,
    /// Drum voices that are muted (never trigger regardless of pattern).
    pub muted_drums: std::collections::HashSet<DrumVoice>,
    /// Drum voices in solo mode. When non-empty, only these voices trigger.
    pub soloed_drums: std::collections::HashSet<DrumVoice>,
    /// When true, BPM is slaved to incoming MIDI clock pulses (0xF8).
    #[serde(default)]
    pub midi_clock_sync: bool,
    /// Which bass voices are enabled for sequencing. Synced from AppState.bass_voices[i].enabled.
    #[serde(default = "default_bass_voice_enabled")]
    pub bass_voice_enabled: [bool; MAX_BASS_VOICES],
    /// Pre-echo (lead-in) configs per voice.  When active, anchor steps
    /// get reinforced by a ramped build-up on the steps leading into
    /// them (velocity / ratchet).  See `crate::sequencer::preecho`.
    /// Lookup key = voice name ("bass", "kit_a", "kit_b", "amen",
    /// "hoover", "an1x").  Empty = no preecho on that voice.
    #[serde(default)]
    pub preecho: std::collections::HashMap<String, crate::sequencer::PreechoConfig>,
    /// Amen slice play order — maps step index to slice index.  Empty =
    /// identity (step N → slice N).  When populated, the sequencer's
    /// auto-advance path looks up `amen_slice_order[step % len]` so the
    /// user can rearrange the break into a custom permutation (e.g.
    /// `[3, 0, 1, 2]` shifts the whole pattern by 3 slices).
    #[serde(default)]
    pub amen_slice_order: Vec<u8>,
    /// Song-mode style tag.  When the pattern bank is playing through a
    /// chain and this slot becomes active, `pattern_style` (if `Some`)
    /// is applied as the global `LlmState.active_style` and propagated
    /// to all unlocked agents — the core of "per-chain style
    /// transitions".  `None` = leave the active style unchanged when
    /// the chain advances to this slot.
    #[serde(default)]
    pub pattern_style: Option<String>,
    /// Song-mode tempo-transition opt-in.  When `true`, the chain
    /// advance adopts *this* slot's `bpm` + `swing` instead of
    /// preserving the prior transport; `false` (the default, and the
    /// pre-Song-mode behaviour) keeps the existing bpm/swing so
    /// existing chain projects don't surprise-jump tempos on upgrade.
    /// `running` is always preserved regardless of this flag.
    #[serde(default)]
    pub pattern_bpm_apply: bool,
}

impl Default for SequencerState {
    fn default() -> Self {
        let mut drum_patterns = std::collections::HashMap::new();
        // Pre-allocate MAX_STEPS silent steps; only the first `steps` are active in the clock.
        for v in DrumVoice::ALL {
            drum_patterns.insert(*v, vec![Step::default(); MAX_STEPS]);
        }

        // All patterns start blank — the AI builds everything from scratch.
        let bass_pattern = vec![TB303Step::default(); MAX_STEPS];

        // Default: all drum voices use the global `steps` length.
        let mut drum_steps = std::collections::HashMap::new();
        for v in DrumVoice::ALL {
            drum_steps.insert(*v, 32usize);
        }

        Self {
            bpm: 120.0,
            steps: 32,
            current_step: 0,
            running: false,
            swing: 0.0,
            time_sig_num: 4,
            step_division: 4,
            drum_patterns,
            bass_pattern: bass_pattern.clone(),
            hoover_pattern: vec![TB303Step::default(); MAX_STEPS],
            an1x_pattern: vec![TB303Step::default(); MAX_STEPS],
            pluck_pattern: vec![TB303Step::default(); MAX_STEPS],
            wavetable_pattern: vec![TB303Step::default(); MAX_STEPS],
            sample_pattern: vec![TB303Step::default(); MAX_STEPS],
            root_note: 9, // A
            scale: Scale::NaturalMinor,
            scale_snap: false,
            drum_steps,
            bass_steps: 32,
            hoover_steps: 32,
            an1x_steps: 32,
            pluck_steps: 32,
            wavetable_steps: 32,
            sample_steps: 32,
            muted_drums: std::collections::HashSet::new(),
            soloed_drums: std::collections::HashSet::new(),
            midi_clock_sync: false,
            bass_patterns: {
                let mut pats = vec![bass_pattern];
                for _ in 1..MAX_BASS_VOICES {
                    pats.push(vec![TB303Step::default(); MAX_STEPS]);
                }
                pats
            },
            bass_voice_steps: vec![32usize; MAX_BASS_VOICES],
            bass_voice_enabled: default_bass_voice_enabled(),
            preecho: std::collections::HashMap::new(),
            amen_slice_order: Vec::new(),
            pattern_style: None,
            pattern_bpm_apply: false,
        }
    }
}

// ─── Huth temperature for a melodic pattern slice ────────────────────────────
// Pure helper.  Returns running totals (weighted_sum, total_weight) so the
// caller can sum across multiple voices / lanes before averaging.  The
// `semi_temps` argument is the 12-entry pitch-class table from
// `crate::ui::theme::NOTE_TEMP` (passed in to keep state ↛ ui dep clean).

/// Accent boost applied on top of a step's gate when weighting note temperature.
const TEMP_ACCENT_BOOST: f32 = 1.5;

/// Accumulate (weighted_sum, total_weight) of `theme::note_temperature(note)`
/// across the first `len` active steps, weighted by `gate × accent_boost`.
pub fn pattern_temperature_acc(
    steps: &[TB303Step],
    len: usize,
    semi_temps: &[f32; 12],
) -> (f32, f32) {
    let mut sum = 0.0_f32;
    let mut wsum = 0.0_f32;
    for s in steps.iter().take(len) {
        if !s.active {
            continue;
        }
        // Accent now carries an intensity 0..=1 — scale the boost
        // proportionally so a half-strength accent counts as half the
        // extra weight a full accent would add.
        let accent_mult = 1.0 + s.accent.clamp(0.0, 1.0) * (TEMP_ACCENT_BOOST - 1.0);
        let w = s.gate.max(0.0) * accent_mult;
        if w <= 0.0 {
            continue;
        }
        let pc = (s.note % 12) as usize;
        sum += w * semi_temps[pc];
        wsum += w;
    }
    (sum, wsum)
}
