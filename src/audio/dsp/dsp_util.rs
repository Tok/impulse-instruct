// ─── DSP utilities ───────────────────────────────────────────────────────────
// Small pure functions shared across DSP modules.

/// Fast tanh approximation (used by LadderFilter, Bass303, delay saturation).
pub(crate) fn tanh(x: f32) -> f32 {
    let x2 = x * x;
    x * (27.0 + x2) / (27.0 + 9.0 * x2)
}

/// Convert MIDI note number to frequency in Hz.
pub fn midi_to_hz(note: u8) -> f32 {
    440.0 * 2.0f32.powf((note as f32 - 69.0) / 12.0)
}
