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
    /// Manual slice start positions (normalized 0..1 of full sample).
    /// Empty → slices are equal divisions of [start_offset, end_offset]
    /// (the default and legacy behavior).  When populated, entry N is
    /// the start of slice N; slice N ends at entry N+1 (or at end_offset
    /// for the last entry).  Populated by transient detection (AUTO
    /// button) or, later, manual marker editing.
    #[serde(default)]
    pub slice_positions: Vec<f32>,
    /// Per-slice pitch offset in semitones (−24..+24).  Empty = all slices
    /// share the global `pitch`.  When populated, entry N overrides the
    /// pitch for slice N.  Stacks on top of `pitch` and BPM stretch.
    #[serde(default)]
    pub slice_pitches: Vec<f32>,
    /// Per-slice volume multiplier (0..2).  Empty = all slices share the
    /// global `volume`.  When populated, entry N scales the output of
    /// slice N multiplicatively.
    #[serde(default)]
    pub slice_volumes: Vec<f32>,
    /// The BPM the loaded sample was originally recorded at.  Only used
    /// when `bpm_stretch` is true.  Default is 136 (classic Amen Brother
    /// tempo) — change per-sample in the UI or via the API.
    #[serde(default = "default_source_bpm")]
    pub source_bpm: f32,
    /// Stretch sample playback to match the current sequencer BPM.  Simple
    /// resample-based stretch, which also shifts pitch — that's the
    /// classic jump-up drumbreak treatment.  Pitch-preserving stretch
    /// (phase vocoder / granular) could be a follow-up; this covers the
    /// 90% use case.
    #[serde(default)]
    pub bpm_stretch: bool,
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
fn default_source_bpm() -> f32 {
    136.0
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
            slice_positions: Vec::new(),
            slice_pitches: Vec::new(),
            slice_volumes: Vec::new(),
            source_bpm: default_source_bpm(),
            bpm_stretch: false,
            meta: None,
        }
    }
}
