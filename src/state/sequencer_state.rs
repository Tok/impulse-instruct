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
    // Voice 0 gets the same starter pattern as `bass_pattern`; voices 1-3 are silent.
    let mut patterns = Vec::with_capacity(MAX_BASS_VOICES);

    // Voice 0: A minor starter pattern (mirrors SequencerState default)
    let mut p0 = vec![TB303Step::default(); MAX_STEPS];
    let bass_notes: &[(usize, u8)] = &[(0, 45), (6, 48), (12, 52)];
    for &(step, note) in bass_notes {
        p0[step].active = true;
        p0[step].note = note;
    }
    patterns.push(p0);

    // Voices 1-3: silent
    for _ in 1..MAX_BASS_VOICES {
        patterns.push(vec![TB303Step::default(); MAX_STEPS]);
    }
    patterns
}

fn default_bass_voice_steps() -> Vec<usize> {
    vec![16usize; MAX_BASS_VOICES]
}

fn default_bass_voice_enabled() -> [bool; MAX_BASS_VOICES] {
    let mut arr = [false; MAX_BASS_VOICES];
    arr[0] = true;
    arr
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
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct TB303Step {
    pub active: bool,
    pub note: u8, // MIDI note number
    pub accent: bool,
    pub slide: bool,
    pub gate: f32, // 0–1 gate length ratio
    /// Per-step stereo pan, -1.0 = hard left, 0.0 = centre, 1.0 = hard right.
    /// Applied at trigger time on top of the voice's static pan setting.
    /// Defaults to 0 so old patterns deserialize unchanged.
    #[serde(default)]
    pub pan: f32,
}

impl Default for TB303Step {
    fn default() -> Self {
        Self {
            active: false,
            note: 36,
            accent: false,
            slide: false,
            gate: 0.5,
            pan: 0.0,
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
    pub drum_patterns: std::collections::HashMap<DrumVoice, Vec<Step>>,
    pub bass_pattern: Vec<TB303Step>,
    pub hoover_pattern: Vec<TB303Step>,
    pub an1x_pattern: Vec<TB303Step>,
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
            drum_patterns,
            bass_pattern: bass_pattern.clone(),
            hoover_pattern: vec![TB303Step::default(); MAX_STEPS],
            an1x_pattern: vec![TB303Step::default(); MAX_STEPS],
            root_note: 9, // A
            scale: Scale::NaturalMinor,
            scale_snap: false,
            drum_steps,
            bass_steps: 32,
            hoover_steps: 32,
            an1x_steps: 32,
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
        }
    }
}
