// ─── state/ui_prefs.rs ───────────────────────────────────────────────────────
// Persistent UI preference types: knob style/size, pad size, UiPrefs struct.

use serde::{Deserialize, Serialize};

/// Visual style for rotary knobs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum KnobStyle {
    /// Flat monochrome disc (original style).
    Flat,
    /// Neumorphic chrome — concentric rings, raised tick, value arc.
    #[default]
    Chrome,
}

/// Knob body size in pixels.  Steps follow the Fibonacci sequence so each size
/// is φ ≈ 1.618× the previous — proportions that feel natural together.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum KnobSize {
    /// 34 px body — compact, fits many knobs per row.
    Small,
    /// 55 px body — default; slightly larger than the legacy 44 px size.
    #[default]
    Normal,
    /// 89 px body — detailed view.
    Large,
    /// 144 px body — XL / presentation mode.
    XL,
}

impl KnobSize {
    /// Body rect width/height in pixels.
    pub fn body_px(self) -> f32 {
        match self {
            Self::Small => 34.0,
            Self::Normal => 55.0,
            Self::Large => 89.0,
            Self::XL => 144.0,
        }
    }

    /// Total allocation height including the label strip below (φ-proportioned).
    pub fn total_px(self) -> f32 {
        let b = self.body_px();
        b + (b * 0.28).max(14.0).round()
    }
}

/// Step-button and XY-pad size.  Steps mirror the KnobSize Fibonacci series
/// so pad and knob sizes stay proportionally consistent when both are changed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PadSize {
    /// 34 px — compact; 16 steps fit in ~560 px.
    Small,
    /// 55 px — default; matches KnobSize::Normal, good on most displays.
    #[default]
    Normal,
    /// 89 px — large; generous detail on wide/high-DPI displays.
    Large,
    /// 144 px — XL; presentation / accessibility mode.
    XL,
}

impl PadSize {
    pub fn px(self) -> f32 {
        match self {
            Self::Small => 34.0,
            Self::Normal => 55.0,
            Self::Large => 89.0,
            Self::XL => 144.0,
        }
    }
}

/// How much Huth *Farbige Noten* color theory is applied to the UI.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum HuthStyle {
    /// Huth colors only on the piano keyboard (existing behavior).
    #[default]
    PianoOnly,
    /// Piano + sequencer melodic note cells rendered as Huth U-shapes.
    Full,
    /// All UI chrome is monochrome; piano uses standard black/white.
    Off,
}

/// Persistent UI preferences stored in AppState so they survive across sessions.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UiPrefs {
    /// Knob visual style (Flat / Chrome).
    pub knob_style: KnobStyle,
    /// Knob body size following Fibonacci steps.
    pub knob_size: KnobSize,
    /// Step button and XY pad size.
    pub pad_size: PadSize,
    /// When true, panels render horizontal sliders instead of knobs.
    pub use_sliders: bool,
    /// How broadly Huth *Farbige Noten* colors are applied.
    pub huth_style: HuthStyle,
    /// Post-process bloom glow on note highlights (future: needs wgpu pass).
    pub bloom_enabled: bool,
    /// Bloom intensity 0–1.
    pub bloom_intensity: f32,
    /// Persisted log level index (0=Trace … 4=Error); matches LOG_LEVELS in header.rs.
    pub log_level_idx: usize,
    /// Global UI scale multiplier applied via pixels_per_point (0.5–3.0, default 1.0).
    pub ui_scale: f32,
}

impl Default for UiPrefs {
    fn default() -> Self {
        Self {
            knob_style: KnobStyle::Chrome,
            knob_size: KnobSize::Normal,
            pad_size: PadSize::Normal,
            use_sliders: false,
            huth_style: HuthStyle::PianoOnly,
            bloom_enabled: false,
            bloom_intensity: 0.5,
            log_level_idx: 2, // Info
            ui_scale: 1.0,
        }
    }
}
