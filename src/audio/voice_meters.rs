// ─── audio/voice_meters.rs ───────────────────────────────────────────────────
// Per-voice peak-envelope levels published from the audio thread for the
// `VoiceMeterStrip` viz module.  Audio thread maintains a peak-decay
// envelope per voice bus inside `DspState` and publishes the latest
// value once per audio callback into the shared `VoiceLevels` array.
// UI thread reads when painting the meter strip.
//
// Pattern mirrors `sample_instrument_poly` (Arc<AtomicU8>) — atomic
// shared between audio + UI for "latest value only" data, with f32 bits
// stored in an AtomicU32 slot.  Cheaper than an rtrb stream the UI would
// have to drain to find the current value, and lock-free.
//
// Stable slot layout — never reorder, since the UI indexes by voice
// kind via `voice_meter_idx`.  Adding a new voice kind appends to the
// end of `VoiceMeterSlot` enum and bumps `VOICE_METER_SLOTS`.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use crate::state::ModuleKind;

/// Number of voice slots.  Sized to cover every voice ModuleKind
/// (singletons in the audio thread).  Bump when adding a new voice
/// kind to the audio path.
pub const VOICE_METER_SLOTS: usize = 20;

/// Map a voice `ModuleKind` to its fixed meter slot.  Returns `None`
/// for non-voice kinds (FX, sequencer, modulation utilities, etc.).
/// Stable — never reorder; the UI relies on these indices.
pub fn voice_meter_idx(kind: ModuleKind) -> Option<usize> {
    Some(match kind {
        ModuleKind::AcidBass => 0,
        ModuleKind::DrumKit808 => 1,
        ModuleKind::DrumKit909 => 2,
        ModuleKind::HooverLead => 3,
        ModuleKind::PluckString => 4,
        ModuleKind::WavetableVoice => 5,
        ModuleKind::SampleInstrument => 6,
        ModuleKind::An1xVoice => 7,
        ModuleKind::AmenSampler => 8,
        ModuleKind::NoiseVoice => 9,
        ModuleKind::Theremin => 10,
        ModuleKind::Pendulum => 11,
        ModuleKind::FmOpsVoice => 12,
        ModuleKind::AdditiveVoice => 13,
        ModuleKind::ModalVoice => 14,
        ModuleKind::ChiptuneVoice => 15,
        ModuleKind::VocalVoice => 16,
        ModuleKind::GranularTexture => 17,
        ModuleKind::GabberKick => 18,
        ModuleKind::NeuTts => 19,
        _ => return None,
    })
}

/// Short display label for a meter slot.  Mirrors the voice card
/// labels for visual consistency in the strip.
pub fn voice_meter_label(idx: usize) -> &'static str {
    match idx {
        0 => "BASS",
        1 => "808",
        2 => "909",
        3 => "HOOVER",
        4 => "PLUCK",
        5 => "WT",
        6 => "SAMP",
        7 => "AN1X",
        8 => "AMEN",
        9 => "NOIS",
        10 => "THER",
        11 => "PEND",
        12 => "FM",
        13 => "ADD",
        14 => "MODAL",
        15 => "CHIP",
        16 => "VOC",
        17 => "GRAN",
        18 => "GAB",
        19 => "TTS",
        _ => "",
    }
}

/// Shared per-voice peak-envelope levels.  Wrapped in `Arc` so audio
/// + UI threads can both hold a clone without coordination cost.
pub struct VoiceLevels {
    /// Each slot holds an f32 peak-decay envelope value, stored as
    /// `f32::to_bits()` in the AtomicU32.  `Relaxed` ordering is fine —
    /// stale reads are harmless for a meter, and torn reads can't
    /// happen because AtomicU32 is single-word atomic.
    pub levels: [AtomicU32; VOICE_METER_SLOTS],
}

impl VoiceLevels {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            levels: std::array::from_fn(|_| AtomicU32::new(0)),
        })
    }

    /// Read the current envelope value for slot `idx`.  Returns 0.0
    /// if `idx` is out of range.
    pub fn read(&self, idx: usize) -> f32 {
        if idx >= VOICE_METER_SLOTS {
            return 0.0;
        }
        f32::from_bits(self.levels[idx].load(Ordering::Relaxed))
    }

    /// Write a fresh envelope value for slot `idx`.  Out-of-range
    /// indices are silently dropped — the audio thread would have to
    /// be writing to a slot that doesn't exist, which is a programmer
    /// error rather than runtime data.
    pub fn write(&self, idx: usize, level: f32) {
        if idx >= VOICE_METER_SLOTS {
            return;
        }
        self.levels[idx].store(level.to_bits(), Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_preserves_f32_bits() {
        let m = VoiceLevels::new();
        m.write(0, 0.42);
        assert!((m.read(0) - 0.42).abs() < 1e-6);
    }

    #[test]
    fn out_of_range_read_is_zero() {
        let m = VoiceLevels::new();
        assert_eq!(m.read(VOICE_METER_SLOTS), 0.0);
        assert_eq!(m.read(VOICE_METER_SLOTS + 100), 0.0);
    }

    #[test]
    fn out_of_range_write_silently_dropped() {
        let m = VoiceLevels::new();
        m.write(VOICE_METER_SLOTS, 1.0); // off the end
        // First slot must remain untouched.
        assert_eq!(m.read(0), 0.0);
    }

    #[test]
    fn voice_meter_idx_round_trips_for_every_voice_kind() {
        // Every voice kind must produce a Some(idx) within range, and
        // `idx` must round-trip back through the label table.
        let voice_kinds = [
            ModuleKind::AcidBass,
            ModuleKind::DrumKit808,
            ModuleKind::DrumKit909,
            ModuleKind::HooverLead,
            ModuleKind::PluckString,
            ModuleKind::WavetableVoice,
            ModuleKind::SampleInstrument,
            ModuleKind::An1xVoice,
            ModuleKind::AmenSampler,
            ModuleKind::NoiseVoice,
            ModuleKind::Theremin,
            ModuleKind::Pendulum,
            ModuleKind::FmOpsVoice,
            ModuleKind::AdditiveVoice,
            ModuleKind::ModalVoice,
            ModuleKind::ChiptuneVoice,
            ModuleKind::VocalVoice,
            ModuleKind::GranularTexture,
            ModuleKind::GabberKick,
            ModuleKind::NeuTts,
        ];
        let mut seen = [false; VOICE_METER_SLOTS];
        for kind in voice_kinds {
            let idx = voice_meter_idx(kind).expect("voice kind must have a slot");
            assert!(idx < VOICE_METER_SLOTS, "{kind:?} idx {idx} out of range");
            assert!(!seen[idx], "two voice kinds share idx {idx}");
            assert!(
                !voice_meter_label(idx).is_empty(),
                "slot {idx} ({kind:?}) has empty label"
            );
            seen[idx] = true;
        }
        assert!(seen.iter().all(|&x| x), "every slot covered exactly once");
    }

    #[test]
    fn voice_meter_idx_returns_none_for_non_voice_kinds() {
        for kind in [
            ModuleKind::FxReverb,
            ModuleKind::MasterOutput,
            ModuleKind::StepSequencer,
            ModuleKind::LfoModule,
            ModuleKind::SpectrumAnalyzer,
            ModuleKind::LlmAgent,
        ] {
            assert!(
                voice_meter_idx(kind).is_none(),
                "{kind:?} should not have a meter slot"
            );
        }
    }
}
