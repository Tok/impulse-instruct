// ─── state/music.rs ──────────────────────────────────────────────────────────
// Musical key and scale types + pure utility functions.

use serde::{Deserialize, Serialize};

/// Musical scale / mode.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Scale {
    /// Natural minor / Aeolian — the default; acid, techno, jungle all live here.
    #[default]
    NaturalMinor,
    Major,
    Dorian,     // minor with raised 6th — BoC, jazz-funk bass lines
    Phrygian,   // minor with b2 — flamenco, dark techno
    Lydian,     // major with raised 4th — dreamy, cinematic
    Mixolydian, // major with b7 — blues, funk, classic rock
    Locrian,    // diminished feel — rare, very dark
    Pentatonic, // minor pentatonic — universal, hard to go wrong
    Blues,      // pentatonic + tritone blue note
    Chromatic,  // all 12 notes — no snapping
}

impl Scale {
    /// Semitone offsets from the root for each degree of this scale.
    pub fn intervals(self) -> &'static [u8] {
        match self {
            Scale::Major => &[0, 2, 4, 5, 7, 9, 11],
            Scale::NaturalMinor => &[0, 2, 3, 5, 7, 8, 10],
            Scale::Dorian => &[0, 2, 3, 5, 7, 9, 10],
            Scale::Phrygian => &[0, 1, 3, 5, 7, 8, 10],
            Scale::Lydian => &[0, 2, 4, 6, 7, 9, 11],
            Scale::Mixolydian => &[0, 2, 4, 5, 7, 9, 10],
            Scale::Locrian => &[0, 1, 3, 5, 6, 8, 10],
            Scale::Pentatonic => &[0, 3, 5, 7, 10],
            Scale::Blues => &[0, 3, 5, 6, 7, 10],
            Scale::Chromatic => &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11],
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Scale::Major => "Major",
            Scale::NaturalMinor => "Minor",
            Scale::Dorian => "Dorian",
            Scale::Phrygian => "Phrygian",
            Scale::Lydian => "Lydian",
            Scale::Mixolydian => "Mixolyd",
            Scale::Locrian => "Locrian",
            Scale::Pentatonic => "Penta",
            Scale::Blues => "Blues",
            Scale::Chromatic => "Chrom",
        }
    }

    pub fn all() -> &'static [Scale] {
        &[
            Scale::NaturalMinor,
            Scale::Major,
            Scale::Dorian,
            Scale::Phrygian,
            Scale::Lydian,
            Scale::Mixolydian,
            Scale::Locrian,
            Scale::Pentatonic,
            Scale::Blues,
            Scale::Chromatic,
        ]
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "Major" => Some(Scale::Major),
            "Minor" | "NaturalMinor" | "Natural Minor" | "Aeolian" => Some(Scale::NaturalMinor),
            "Dorian" => Some(Scale::Dorian),
            "Phrygian" => Some(Scale::Phrygian),
            "Lydian" => Some(Scale::Lydian),
            "Mixolydian" | "Mixolyd" => Some(Scale::Mixolydian),
            "Locrian" => Some(Scale::Locrian),
            "Pentatonic" | "Penta" | "PentatonicMinor" => Some(Scale::Pentatonic),
            "Blues" => Some(Scale::Blues),
            "Chromatic" | "Chrom" => Some(Scale::Chromatic),
            _ => None,
        }
    }
}

/// Short name for a root note (0=C … 11=B).
pub const ROOT_NAMES: [&str; 12] = [
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];

/// Whether `note` (MIDI 0–127) belongs to the scale built on `root` (0–11).
pub fn note_in_scale(note: u8, root: u8, scale: Scale) -> bool {
    let offset = (note % 12 + 12 - root % 12) % 12;
    scale.intervals().contains(&offset)
}

/// Scale degree index (0 = tonic) if `note` is in the scale, else `None`.
pub fn scale_degree(note: u8, root: u8, scale: Scale) -> Option<usize> {
    let offset = (note % 12 + 12 - root % 12) % 12;
    scale.intervals().iter().position(|&i| i == offset)
}

/// All MIDI note numbers (0–127) belonging to the scale.
pub fn scale_notes(root: u8, scale: Scale) -> Vec<u8> {
    (0u8..=127)
        .filter(|&n| note_in_scale(n, root, scale))
        .collect()
}

/// Snap `note` to the nearest note in the given scale (smallest semitone distance).
/// Preserves the note's general register; ties broken upward.
pub fn snap_to_scale(note: u8, root: u8, scale: Scale) -> u8 {
    if scale == Scale::Chromatic {
        return note;
    }
    let candidates = scale_notes(root, scale);
    candidates
        .into_iter()
        .min_by_key(|&n| (n as i16 - note as i16).unsigned_abs())
        .unwrap_or(note)
}
