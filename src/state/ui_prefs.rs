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

/// Step-button and XY-pad size.  Steps follow the Fibonacci series so each
/// level is ~φ× the previous.  Shifted one step below KnobSize so that
/// Normal pads (34 px) and Normal knobs (55 px) look balanced side-by-side.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PadSize {
    /// 21 px — very compact; 16 steps fit in ~340 px.
    Small,
    /// 34 px — default; 16 steps take ~545 px, comfortable on 1080p+.
    #[default]
    Normal,
    /// 55 px — large; generous detail on wide/high-DPI displays.
    Large,
    /// 89 px — XL; accessibility / presentation mode.
    XL,
}

impl PadSize {
    pub fn px(self) -> f32 {
        match self {
            Self::Small => 21.0,
            Self::Normal => 34.0,
            Self::Large => 55.0,
            Self::XL => 89.0,
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

/// How often the session is auto-saved to `session.json`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum AutosaveInterval {
    /// Save immediately on every state change (original behaviour).
    #[default]
    Immediate,
    /// Save at most once every 5 seconds.
    FiveSec,
    /// Save at most once every 30 seconds.
    ThirtySec,
    /// Never auto-save — user must trigger a manual project save.
    Manual,
}

impl AutosaveInterval {
    pub fn label(self) -> &'static str {
        match self {
            Self::Immediate => "Immediate",
            Self::FiveSec => "5 seconds",
            Self::ThirtySec => "30 seconds",
            Self::Manual => "Manual only",
        }
    }

    /// Returns `None` for Immediate/Manual (handled separately), or a `Duration`.
    pub fn duration(self) -> Option<std::time::Duration> {
        match self {
            Self::Immediate | Self::Manual => None,
            Self::FiveSec => Some(std::time::Duration::from_secs(5)),
            Self::ThirtySec => Some(std::time::Duration::from_secs(30)),
        }
    }
}

/// Persistent UI preferences stored in AppState so they survive across sessions.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UiPrefs {
    /// Knob visual style (Flat / Chrome).
    pub knob_style: KnobStyle,
    /// Knob body size following Fibonacci steps.
    pub knob_size: KnobSize,
    /// Custom knob body size in pixels, overrides `knob_size` when `Some`.
    #[serde(default)]
    pub custom_knob_px: Option<f32>,
    /// Step button and XY pad size (enum preset for both, individually overrideable).
    pub pad_size: PadSize,
    /// Custom sequencer step button size in pixels, overrides `pad_size` when `Some`.
    #[serde(default)]
    pub custom_pad_px: Option<f32>,
    /// Custom XY control pad size in pixels, overrides the derived size when `Some`.
    #[serde(default)]
    pub custom_xy_px: Option<f32>,
    /// Custom envelope/ADSR display height in pixels, overrides the derived height when `Some`.
    #[serde(default)]
    pub custom_env_h: Option<f32>,
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
    /// How often the session is auto-saved.
    #[serde(default)]
    pub autosave_interval: AutosaveInterval,
    /// When true, WASD keys act as arrow keys (scroll rack, turn knobs).
    #[serde(default)]
    pub wasd_as_arrows: bool,
    /// CRT scan-line overlay and vignette effect.
    #[serde(default)]
    pub crt_effect: bool,
    /// Oscilloscope phosphor persistence: number of history frames (2–20, default 10).
    #[serde(default = "default_phosphor_frames")]
    pub phosphor_frames: usize,
    /// Oscilloscope phosphor glow intensity (0.0–1.0, default 0.6).
    #[serde(default = "default_phosphor_intensity")]
    pub phosphor_intensity: f32,
}

fn default_phosphor_frames() -> usize {
    10
}
fn default_phosphor_intensity() -> f32 {
    0.6
}

impl UiPrefs {
    /// Effective knob body size in pixels — custom override when set, else enum step.
    pub fn effective_knob_px(&self) -> f32 {
        self.custom_knob_px
            .unwrap_or_else(|| self.knob_size.body_px())
            .clamp(12.0, 200.0)
    }

    /// Effective sequencer step button size in pixels.
    pub fn effective_pad_px(&self) -> f32 {
        self.custom_pad_px
            .unwrap_or_else(|| self.pad_size.px())
            .clamp(10.0, 150.0)
    }

    /// Effective XY control pad size in pixels.
    /// Defaults to `pad_size.px() × 3.38` (legacy formula) when not overridden.
    pub fn effective_xy_px(&self) -> f32 {
        self.custom_xy_px
            .unwrap_or_else(|| self.pad_size.px() * (176.0 / 26.0))
            .clamp(40.0, 400.0)
    }

    /// Effective envelope/ADSR display height in pixels.
    /// Defaults to 30% of the XY pad size, minimum 28 px.
    pub fn effective_env_h(&self) -> f32 {
        self.custom_env_h
            .unwrap_or_else(|| (self.effective_xy_px() * 0.45).max(60.0))
            .clamp(16.0, 200.0)
    }
}

impl Default for UiPrefs {
    fn default() -> Self {
        Self {
            knob_style: KnobStyle::Chrome,
            knob_size: KnobSize::Normal,
            custom_knob_px: None,
            pad_size: PadSize::Normal,
            custom_pad_px: None,
            custom_xy_px: None,
            custom_env_h: None,
            use_sliders: false,
            huth_style: HuthStyle::PianoOnly,
            bloom_enabled: false,
            bloom_intensity: 0.5,
            log_level_idx: 2, // Info
            ui_scale: 1.0,
            autosave_interval: AutosaveInterval::Immediate,
            wasd_as_arrows: false,
            crt_effect: false,
            phosphor_frames: default_phosphor_frames(),
            phosphor_intensity: default_phosphor_intensity(),
        }
    }
}
