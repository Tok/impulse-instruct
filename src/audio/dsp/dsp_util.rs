// ─── DSP utilities ───────────────────────────────────────────────────────────
// Small pure functions shared across DSP modules.

/// Accent-step gain lift.  An accent value of 1.0 multiplies the voice's
/// output by `1.0 + ACCENT_LIFT`; 0.0 leaves it at baseline.  Shared by
/// bass / drum / an1x voices so per-voice accent semantics stay uniform
/// — changing one voice's lift used to require hunting down N local
/// constants.
pub(super) const ACCENT_LIFT: f32 = 0.3;

/// Fast tanh approximation (used by LadderFilter, Bass303, delay saturation).
pub(crate) fn tanh(x: f32) -> f32 {
    let x2 = x * x;
    x * (27.0 + x2) / (27.0 + 9.0 * x2)
}

/// Convert MIDI note number to frequency in Hz (12-TET, A4=440).
pub fn midi_to_hz(note: u8) -> f32 {
    440.0 * 2.0f32.powf((note as f32 - 69.0) / 12.0)
}

/// Convert frequency in Hz to fractional MIDI note number (12-TET, A4=440).
/// Inverse of `midi_to_hz`; callers round/clamp as needed.
pub fn hz_to_midi(hz: f32) -> f32 {
    69.0 + 12.0 * (hz / 440.0).log2()
}

/// Tuning system used by `midi_to_hz_tuned`.  The integer discriminants
/// match the persisted `u8` in `FxState::tuning` so old sessions load
/// without migration — `TuningSystem::from_u8` recovers the enum, and
/// unknown values fall back to `TwelveTet`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum TuningSystem {
    #[default]
    TwelveTet = 0,
    JustIntonation = 1,
    Slendro = 2,
    Pelog = 3,
}

impl TuningSystem {
    pub fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::JustIntonation,
            2 => Self::Slendro,
            3 => Self::Pelog,
            _ => Self::TwelveTet,
        }
    }
}

/// Convert MIDI note to Hz using the specified tuning system.
pub fn midi_to_hz_tuned(note: u8, tuning: TuningSystem) -> f32 {
    match tuning {
        TuningSystem::TwelveTet => midi_to_hz(note),
        TuningSystem::JustIntonation => midi_to_hz_just(note),
        TuningSystem::Slendro => midi_to_hz_slendro(note),
        TuningSystem::Pelog => midi_to_hz_pelog(note),
    }
}

/// Just intonation ratios (C-based, one octave).
fn midi_to_hz_just(note: u8) -> f32 {
    const RATIOS: [f32; 12] = [
        1.0,         // C  unison
        16.0 / 15.0, // C# minor second
        9.0 / 8.0,   // D  major second
        6.0 / 5.0,   // Eb minor third
        5.0 / 4.0,   // E  major third
        4.0 / 3.0,   // F  perfect fourth
        45.0 / 32.0, // F# tritone
        3.0 / 2.0,   // G  perfect fifth
        8.0 / 5.0,   // Ab minor sixth
        5.0 / 3.0,   // A  major sixth
        9.0 / 5.0,   // Bb minor seventh
        15.0 / 8.0,  // B  major seventh
    ];
    let octave = (note as i32 - 60) / 12;
    let degree = ((note as i32 - 60) % 12 + 12) as usize % 12;
    let base_c4 = 261.626; // C4
    base_c4 * RATIOS[degree] * 2.0f32.powi(octave)
}

/// Javanese slendro — 5-tone equal temperament.  Each MIDI step is one
/// slendro step of `2^(1/5)`, so 5 MIDI steps equal one octave (rather
/// than 12, the 12-TET default).
fn midi_to_hz_slendro(note: u8) -> f32 {
    let base = 261.626; // C4 = MIDI 60
    base * 2.0f32.powf((note as f32 - 60.0) / 5.0)
}

/// Javanese pelog (7-tone non-equal scale, approximated).
fn midi_to_hz_pelog(note: u8) -> f32 {
    // Pelog intervals in cents (approximate, varies by gamelan)
    const CENTS: [f32; 7] = [0.0, 120.0, 270.0, 400.0, 540.0, 675.0, 810.0];
    let octave = (note as i32 - 60) / 7;
    let degree = ((note as i32 - 60) % 7 + 7) as usize % 7;
    let base = 261.626;
    base * 2.0f32.powf((CENTS[degree] + octave as f32 * 1200.0) / 1200.0)
}
