// ─── music_api/mod.rs ─────────────────────────────────────────────────────────
// Pure music-theory helpers callable by the LLM via a "music_api" JSON block.
//
// All functions are pure (same input → same output, no side effects).
// Seeded RNG helpers take an explicit seed so tests are deterministic.
//
// LLM usage (inside a JSON response):
//   {
//     "music_api": {
//       "chord": { "root": "E", "quality": "minor" },
//       "amen_pattern": { "heat": 0.6, "seed": 42 },
//       "scale_run": { "root": "A", "scale": "NaturalMinor", "direction": "up" }
//     }
//   }
// The results are applied to AppState by `apply_music_api` in state/transitions.rs.

use crate::state::{DrumVoice, Scale, Step, TB303Step};

// ─── Chord construction ───────────────────────────────────────────────────────

/// Chord quality — interval structure from the root.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChordQuality {
    Major, // 0, 4, 7
    Minor, // 0, 3, 7
    Dim,   // 0, 3, 6
    Aug,   // 0, 4, 8
    Sus2,  // 0, 2, 7
    Sus4,  // 0, 5, 7
    Dom7,  // 0, 4, 7, 10
    Maj7,  // 0, 4, 7, 11
    Min7,  // 0, 3, 7, 10
    Dim7,  // 0, 3, 6, 9
}

impl ChordQuality {
    pub fn intervals(self) -> &'static [i8] {
        match self {
            Self::Major => &[0, 4, 7],
            Self::Minor => &[0, 3, 7],
            Self::Dim => &[0, 3, 6],
            Self::Aug => &[0, 4, 8],
            Self::Sus2 => &[0, 2, 7],
            Self::Sus4 => &[0, 5, 7],
            Self::Dom7 => &[0, 4, 7, 10],
            Self::Maj7 => &[0, 4, 7, 11],
            Self::Min7 => &[0, 3, 7, 10],
            Self::Dim7 => &[0, 3, 6, 9],
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().trim_matches('"') {
            "major" | "maj" | "M" => Some(Self::Major),
            "minor" | "min" | "m" => Some(Self::Minor),
            "dim" | "diminished" => Some(Self::Dim),
            "aug" | "augmented" => Some(Self::Aug),
            "sus2" => Some(Self::Sus2),
            "sus4" | "sus" => Some(Self::Sus4),
            "dom7" | "7" => Some(Self::Dom7),
            "maj7" => Some(Self::Maj7),
            "min7" | "m7" => Some(Self::Min7),
            "dim7" => Some(Self::Dim7),
            _ => None,
        }
    }
}

/// Parse a root note name ("C", "C#", "Db", "A", …) to a MIDI note number
/// in octave 3 (C3 = 48).  Returns `None` on unknown names.
pub fn parse_root(s: &str) -> Option<u8> {
    let pc: u8 = match s.to_uppercase().trim_matches('"') {
        "C" => 0,
        "C#" | "DB" => 1,
        "D" => 2,
        "D#" | "EB" => 3,
        "E" => 4,
        "F" => 5,
        "F#" | "GB" => 6,
        "G" => 7,
        "G#" | "AB" => 8,
        "A" => 9,
        "A#" | "BB" => 10,
        "B" => 11,
        _ => return None,
    };
    Some(48 + pc) // octave 3
}

/// Build a chord as a Vec of MIDI note numbers.
/// `root` is a MIDI note (e.g. 52 = E3), `quality` determines intervals.
pub fn chord(root: u8, quality: ChordQuality) -> Vec<u8> {
    quality
        .intervals()
        .iter()
        .map(|&i| root.saturating_add_signed(i))
        .collect()
}

/// Return the chord notes as u8 semitone pitch-class values (0–11), suitable
/// for loading into a bass pattern via step notes.
pub fn chord_pcs(root: u8, quality: ChordQuality) -> Vec<u8> {
    chord(root, quality).iter().map(|&n| n % 12).collect()
}

// ─── Diatonic chord within a key ─────────────────────────────────────────────

/// Pick a random diatonic triad from `scale` on `root`, seeded by `seed`.
/// Returns (chord_root_midi, ChordQuality).
pub fn random_diatonic_chord(root: u8, scale: Scale, seed: u64) -> (u8, ChordQuality) {
    let intervals = scale.intervals();
    if intervals.is_empty() {
        return (root, ChordQuality::Major);
    }
    let degree = lcg_usize(seed, intervals.len());
    let chord_root = root.saturating_add(intervals[degree]);

    // Determine quality from scale degree (major scale rules approximated for all modes)
    // Triad quality is determined by stacking thirds within the scale.
    let third = intervals[(degree + 2) % intervals.len()];
    let fifth = intervals[(degree + 4) % intervals.len()];
    let third_interval = (third + 12 - intervals[degree]) % 12;
    let fifth_interval = (fifth + 12 - intervals[degree]) % 12;

    let quality = match (third_interval, fifth_interval) {
        (4, 7) => ChordQuality::Major,
        (3, 7) => ChordQuality::Minor,
        (3, 6) => ChordQuality::Dim,
        (4, 8) => ChordQuality::Aug,
        _ => ChordQuality::Minor,
    };
    (chord_root, quality)
}

// ─── Amen pattern generation ──────────────────────────────────────────────────

/// Canonical Amen break kick/snare pattern (16 steps, 0/1).
/// Used as the deterministic base before heat mutation.
const AMEN_KICK_BASE: [bool; 16] = [
    true, false, false, false, false, false, true, false, false, false, false, false, false, false,
    false, false,
];
const AMEN_SNARE_BASE: [bool; 16] = [
    false, false, false, false, true, false, false, false, false, false, false, false, true, false,
    false, false,
];
const AMEN_HIHAT_BASE: [bool; 16] = [
    true, false, true, false, true, false, true, false, true, false, true, false, true, false,
    true, false,
];

/// Generate a randomised Amen break step pattern at a given `heat` (0.0–1.0).
/// `heat = 0` returns the canonical pattern; `heat = 1` adds maximum variation.
/// `seed` makes results deterministic for testing.
///
/// Returns `(kick_steps, snare_steps, hihat_steps)` — each is 16 `Step` values.
pub fn amen_pattern(heat: f32, seed: u64) -> ([Step; 16], [Step; 16], [Step; 16]) {
    let heat = heat.clamp(0.0, 1.0);
    let mut kick = AMEN_KICK_BASE;
    let mut snare = AMEN_SNARE_BASE;
    let mut hihat = AMEN_HIHAT_BASE;

    // Extra ghost notes probability scales with heat
    let ghost_prob = heat * 0.5; // up to 50% chance of adding a ghost at each step
    // Flip probability for base notes (remove some to keep it loose)
    let remove_prob = heat * 0.25;

    for i in 0..16usize {
        let r_ghost = lcg_f32(seed.wrapping_add(i as u64 * 3), 0);
        let r_remove = lcg_f32(seed.wrapping_add(i as u64 * 3 + 1), 0);
        let r_vel = lcg_f32(seed.wrapping_add(i as u64 * 3 + 2), 0);

        let velocity = (0.55 + r_vel * 0.45).min(1.0); // 0.55–1.0

        if kick[i] && r_remove < remove_prob {
            kick[i] = false;
        } else if !kick[i] && r_ghost < ghost_prob * 0.4 {
            kick[i] = true;
        }
        if snare[i] && r_remove < remove_prob * 0.5 {
            snare[i] = false;
        } else if !snare[i] && r_ghost < ghost_prob * 0.3 {
            snare[i] = true;
        }
        if !hihat[i] && r_ghost < ghost_prob * 0.6 {
            hihat[i] = true;
        }

        let _ = velocity; // used below when building Step values
    }

    let make_steps = |pattern: [bool; 16]| -> [Step; 16] {
        let mut steps = [Step::default(); 16];
        for (i, &active) in pattern.iter().enumerate() {
            steps[i].active = active;
            if active {
                let v = lcg_f32(seed.wrapping_add(i as u64 * 7 + 99), 0);
                steps[i].velocity = (0.6 + v * 0.4).min(1.0);
            }
        }
        steps
    };

    (make_steps(kick), make_steps(snare), make_steps(hihat))
}

// ─── Scale run ────────────────────────────────────────────────────────────────

/// Direction of a scale run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunDirection {
    Up,
    Down,
    UpDown, // ascend then descend (without repeating the peak)
    Random,
}

impl RunDirection {
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().trim_matches('"') {
            "down" | "descend" | "descending" => Self::Down,
            "updown" | "up_down" | "bounce" => Self::UpDown,
            "random" | "rand" => Self::Random,
            _ => Self::Up,
        }
    }
}

/// Generate a 16-step bass pattern running through `scale` starting from
/// `root_midi`, in the given `direction`.  Steps are spaced evenly; remaining
/// steps (when scale has < 8 notes) repeat or rest.
pub fn scale_run(
    root_midi: u8,
    scale: Scale,
    direction: RunDirection,
    seed: u64,
) -> [TB303Step; 16] {
    let intervals = scale.intervals();
    if intervals.is_empty() {
        return [TB303Step::default(); 16];
    }

    // Build one octave of scale notes from root_midi
    let notes: Vec<u8> = intervals
        .iter()
        .map(|&i| root_midi.saturating_add(i))
        .collect();

    let sequence: Vec<u8> = match direction {
        RunDirection::Up => {
            let mut seq = notes.clone();
            seq.push(root_midi.saturating_add(12)); // end on octave
            seq
        }
        RunDirection::Down => {
            let mut seq: Vec<u8> = notes.iter().rev().copied().collect();
            seq.insert(0, root_midi.saturating_add(12));
            seq
        }
        RunDirection::UpDown => {
            let mut seq = notes.clone();
            let mut down: Vec<u8> = notes.iter().rev().skip(1).copied().collect();
            seq.append(&mut down);
            seq
        }
        RunDirection::Random => {
            // Shuffle notes with LCG
            let mut shuffled = notes.clone();
            for i in (1..shuffled.len()).rev() {
                let j = lcg_usize(seed.wrapping_add(i as u64), i + 1);
                shuffled.swap(i, j);
            }
            shuffled
        }
    };

    let mut steps = [TB303Step::default(); 16];
    let seq_len = sequence.len();
    for i in 0..16usize {
        let note = sequence[i % seq_len];
        steps[i].active = true;
        steps[i].note = note % 12; // pitch-class for the bass synth
    }
    steps
}

// ─── LCG helpers (deterministic, allocation-free) ────────────────────────────

/// Single step of a 64-bit LCG.  Returns next state.
fn lcg_step(state: u64) -> u64 {
    state
        .wrapping_mul(6_364_136_223_846_793_005)
        .wrapping_add(1_442_695_040_888_963_407)
}

/// LCG-derived f32 in [0, 1).
fn lcg_f32(seed: u64, extra: u64) -> f32 {
    let s = lcg_step(seed.wrapping_add(extra));
    (s >> 33) as f32 / (1u64 << 31) as f32
}

/// LCG-derived usize in [0, n).
fn lcg_usize(seed: u64, n: usize) -> usize {
    let s = lcg_step(seed);
    ((s >> 33) as usize) % n
}

// ─── JSON dispatch ────────────────────────────────────────────────────────────

/// Apply a `"music_api"` JSON block to `AppState`.  Called from
/// `apply_llm_update` when the LLM response includes this key.
///
/// Supported commands (any subset, evaluated in order):
///
/// ```json
/// {
///   "music_api": {
///     "chord": { "root": "E", "quality": "minor" },
///     "amen_pattern": { "heat": 0.6 },
///     "scale_run": { "root": "A", "scale": "NaturalMinor", "direction": "up" }
///   }
/// }
/// ```
///
/// `seed` is derived from the current system time (ms) for variety; a fixed
/// seed can be provided in JSON for determinism.
pub fn apply_music_api(
    state: crate::state::AppState,
    api: &serde_json::Map<String, serde_json::Value>,
) -> crate::state::AppState {
    let mut s = state;

    let seed: u64 = api.get("seed").and_then(|v| v.as_u64()).unwrap_or_else(|| {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .subsec_nanos() as u64
    });

    // ── chord ─────────────────────────────────────────────────────────────────
    if let Some(chord_obj) = api.get("chord").and_then(|v| v.as_object()) {
        let root_str = chord_obj
            .get("root")
            .and_then(|v| v.as_str())
            .unwrap_or("C");
        let quality_str = chord_obj
            .get("quality")
            .and_then(|v| v.as_str())
            .unwrap_or("minor");
        if let Some(root_midi) = parse_root(root_str) {
            let quality = ChordQuality::from_str(quality_str).unwrap_or(ChordQuality::Minor);
            let notes = chord(root_midi, quality);
            // Write the first 3–4 notes across bass steps 0, 4, 8, 12.
            let positions = [0usize, 4, 8, 12];
            for (&pos, &note) in positions.iter().zip(notes.iter()) {
                if pos < s.sequencer.bass_pattern.len() {
                    s.sequencer.bass_pattern[pos].active = true;
                    s.sequencer.bass_pattern[pos].note = note % 12;
                    s.sequencer.bass_pattern[pos].accent = true;
                }
            }
        }
    }

    // ── amen_pattern ──────────────────────────────────────────────────────────
    if let Some(amen_obj) = api.get("amen_pattern").and_then(|v| v.as_object()) {
        let heat = amen_obj.get("heat").and_then(|v| v.as_f64()).unwrap_or(0.5) as f32;
        let local_seed = amen_obj
            .get("seed")
            .and_then(|v| v.as_u64())
            .unwrap_or(seed);
        let (kick_steps, snare_steps, hihat_steps) = amen_pattern(heat, local_seed);

        // Write into Kit A (808-style) drum patterns.
        if let Some(p) = s.sequencer.drum_patterns.get_mut(&DrumVoice::Kick808) {
            let len = p.len().min(16);
            p[..len].copy_from_slice(&kick_steps[..len]);
        }
        if let Some(p) = s.sequencer.drum_patterns.get_mut(&DrumVoice::Snare808) {
            let len = p.len().min(16);
            p[..len].copy_from_slice(&snare_steps[..len]);
        }
        if let Some(p) = s
            .sequencer
            .drum_patterns
            .get_mut(&DrumVoice::HihatClosed808)
        {
            let len = p.len().min(16);
            p[..len].copy_from_slice(&hihat_steps[..len]);
        }
    }

    // ── scale_run ─────────────────────────────────────────────────────────────
    if let Some(run_obj) = api.get("scale_run").and_then(|v| v.as_object()) {
        let root_str = run_obj.get("root").and_then(|v| v.as_str()).unwrap_or("A");
        let scale_str = run_obj
            .get("scale")
            .and_then(|v| v.as_str())
            .unwrap_or("NaturalMinor");
        let dir_str = run_obj
            .get("direction")
            .and_then(|v| v.as_str())
            .unwrap_or("up");
        let local_seed = run_obj.get("seed").and_then(|v| v.as_u64()).unwrap_or(seed);

        if let Some(root_midi) = parse_root(root_str) {
            let scale = Scale::from_str(scale_str).unwrap_or(Scale::NaturalMinor);
            let direction = RunDirection::from_str(dir_str);
            let run_steps = scale_run(root_midi, scale, direction, local_seed);
            let len = s.sequencer.bass_pattern.len().min(16);
            s.sequencer.bass_pattern[..len].copy_from_slice(&run_steps[..len]);
        }
    }

    s
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn e_major_chord_correct_intervals() {
        let root = parse_root("E").unwrap(); // E3 = 52
        let notes = chord(root, ChordQuality::Major);
        // E major = E (52), G# (56), B (59)
        assert_eq!(notes, vec![52, 56, 59]);
    }

    #[test]
    fn a_minor_chord_correct_intervals() {
        let root = parse_root("A").unwrap(); // A3 = 57
        let notes = chord(root, ChordQuality::Minor);
        // A minor = A (57), C (60), E (64)
        assert_eq!(notes, vec![57, 60, 64]);
    }

    #[test]
    fn dim7_has_four_notes() {
        let root = parse_root("C").unwrap();
        let notes = chord(root, ChordQuality::Dim7);
        assert_eq!(notes.len(), 4);
        // Intervals: 0, 3, 6, 9
        assert_eq!(notes[1] - notes[0], 3);
        assert_eq!(notes[2] - notes[0], 6);
        assert_eq!(notes[3] - notes[0], 9);
    }

    #[test]
    fn parse_root_all_notes() {
        for (name, expected_pc) in &[
            ("C", 0u8),
            ("C#", 1),
            ("D", 2),
            ("D#", 3),
            ("E", 4),
            ("F", 5),
            ("F#", 6),
            ("G", 7),
            ("G#", 8),
            ("A", 9),
            ("A#", 10),
            ("B", 11),
        ] {
            let midi = parse_root(name).unwrap();
            assert_eq!(midi % 12, *expected_pc, "failed for {}", name);
        }
    }

    #[test]
    fn amen_pattern_no_heat_matches_base() {
        let (kick, snare, hihat) = amen_pattern(0.0, 0);
        // At heat=0 no mutations → matches canonical pattern
        for (i, &base) in AMEN_KICK_BASE.iter().enumerate() {
            assert_eq!(kick[i].active, base, "kick step {} mismatch at heat=0", i);
        }
        for (i, &base) in AMEN_SNARE_BASE.iter().enumerate() {
            assert_eq!(snare[i].active, base, "snare step {} mismatch at heat=0", i);
        }
        for (i, &base) in AMEN_HIHAT_BASE.iter().enumerate() {
            assert_eq!(hihat[i].active, base, "hihat step {} mismatch at heat=0", i);
        }
    }

    #[test]
    fn amen_pattern_high_heat_is_deterministic() {
        let (a1, b1, c1) = amen_pattern(0.9, 12345);
        let (a2, b2, c2) = amen_pattern(0.9, 12345);
        for i in 0..16 {
            assert_eq!(a1[i].active, a2[i].active);
            assert_eq!(b1[i].active, b2[i].active);
            assert_eq!(c1[i].active, c2[i].active);
        }
    }

    #[test]
    fn scale_run_up_has_all_active() {
        let steps = scale_run(48, Scale::Major, RunDirection::Up, 0);
        assert!(steps.iter().all(|s| s.active));
    }

    #[test]
    fn scale_run_returns_16_steps() {
        let steps = scale_run(48, Scale::NaturalMinor, RunDirection::UpDown, 99);
        assert_eq!(steps.len(), 16);
    }

    #[test]
    fn chord_quality_from_str_roundtrip() {
        assert_eq!(ChordQuality::from_str("major"), Some(ChordQuality::Major));
        assert_eq!(ChordQuality::from_str("min"), Some(ChordQuality::Minor));
        assert_eq!(ChordQuality::from_str("dim7"), Some(ChordQuality::Dim7));
        assert_eq!(ChordQuality::from_str("sus4"), Some(ChordQuality::Sus4));
        assert_eq!(ChordQuality::from_str("xyz"), None);
    }

    #[test]
    fn random_diatonic_chord_is_in_scale() {
        let root = parse_root("C").unwrap();
        let (chord_root, _quality) = random_diatonic_chord(root, Scale::Major, 777);
        // The chord root should be a note in C major (offsets: 0,2,4,5,7,9,11)
        let offset = (chord_root % 12 + 12 - root % 12) % 12;
        let major_offsets = [0u8, 2, 4, 5, 7, 9, 11];
        assert!(
            major_offsets.contains(&offset),
            "chord root offset {} not in C major",
            offset
        );
    }
}
