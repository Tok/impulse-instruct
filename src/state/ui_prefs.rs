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

/// Step-button and XY-pad size.  Steps are chosen so consecutive sizes are
/// close to a 1.3× factor — each one is noticeably larger without wasting space.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PadSize {
    /// 18 px — compact; 16 steps take ~300 px.
    Small,
    /// 26 px — default; matches legacy 26 px step button.
    #[default]
    Normal,
    /// 34 px — generous; 16 steps take ~560 px.
    Large,
    /// 44 px — XL; best on wide/high-DPI displays.
    XL,
}

impl PadSize {
    pub fn px(self) -> f32 {
        match self {
            Self::Small => 18.0,
            Self::Normal => 26.0,
            Self::Large => 34.0,
            Self::XL => 44.0,
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
}

impl Default for UiPrefs {
    fn default() -> Self {
        Self {
            knob_style: KnobStyle::Chrome,
            knob_size: KnobSize::Small,
            pad_size: PadSize::Normal,
            use_sliders: false,
            huth_style: HuthStyle::PianoOnly,
            bloom_enabled: false,
            bloom_intensity: 0.5,
            log_level_idx: 2, // Info
        }
    }
}
