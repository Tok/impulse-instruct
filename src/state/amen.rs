// ─── state/amen.rs ───────────────────────────────────────────────────────────
// AmenSampler voice state.  Extracted from state/mod.rs to keep that file
// under the 1000-line limit.
//
// AmenState covers the DSP-side playback parameters.  AmenMeta is a non-
// serialized cache of the currently-loaded WAV's header info, populated by
// the UI panel for display.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AmenState {
    /// Path to the WAV file to load (empty = no sample loaded).
    pub path: String,
    /// Playback pitch offset in semitones (-24 to +24). 0 = original pitch.
    pub pitch: f32,
    /// Output volume (0.0–1.0).
    pub volume: f32,
    /// When true, loops the slice; otherwise plays once per trigger.
    pub loop_mode: bool,
    /// How many equal-length slices the sample is divided into.
    /// 1 = no slicing (whole sample plays on trigger).
    /// 2/4/8/16 = break-chopping territory; each drum step can pick a slice.
    #[serde(default = "default_slice_count")]
    pub slice_count: u8,
    /// Usable region start, 0.0–1.0 of full sample length.
    #[serde(default)]
    pub start_offset: f32,
    /// Usable region end, 0.0–1.0.  Must be > start_offset.
    #[serde(default = "default_end_offset")]
    pub end_offset: f32,
    /// Play slices in reverse.
    #[serde(default)]
    pub reverse: bool,
    /// Fraction of slice duration that actually plays before cutting.
    /// 1.0 = full slice, 0.5 = first half, 0.1 = very short stutter.
    #[serde(default = "default_amen_gate")]
    pub gate: f32,
    /// Number of extra retriggers of the same slice per step trigger.
    /// 0 = play once.  1–4 = ratchet-for-samples.
    #[serde(default)]
    pub stutter: u8,
    /// Cached metadata about the currently-loaded WAV (display-only).
    /// Not serialized — populated by the UI panel when a file is loaded.
    #[serde(skip)]
    pub meta: Option<AmenMeta>,
}

fn default_slice_count() -> u8 {
    8
}
fn default_end_offset() -> f32 {
    1.0
}
fn default_amen_gate() -> f32 {
    1.0
}

/// Cached WAV header info for the currently-loaded sample.  Lives on
/// AmenState but never persisted — recomputed at load time.
#[derive(Clone, Debug, Default)]
pub struct AmenMeta {
    /// Total mono frames at 44.1 kHz after internal resample.
    pub samples: usize,
    /// Original sample rate from the file header.
    pub src_rate: u32,
    /// Original channel count from the file header.
    pub channels: u16,
    /// Original bit depth (we currently require 16).
    pub bits: u16,
    /// File size on disk in bytes.
    pub file_bytes: u64,
}

impl Default for AmenState {
    fn default() -> Self {
        Self {
            path: String::new(),
            pitch: 0.0,
            volume: 0.75,
            loop_mode: false,
            slice_count: default_slice_count(),
            start_offset: 0.0,
            end_offset: default_end_offset(),
            reverse: false,
            gate: default_amen_gate(),
            stutter: 0,
            meta: None,
        }
    }
}
