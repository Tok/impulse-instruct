// ─── state/ui_prefs.rs ───────────────────────────────────────────────────────
// Persistent UI preference types.

use serde::{Deserialize, Serialize};

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
    // ── Huth *Farbige Noten* per-component toggles ─────────────────────────
    // Sequencer step dots and the event-stream history are always Huth-
    // coloured — the toggles below only gate the places where a Huth
    // treatment competes with a legible grayscale alternative.
    /// Huth colors on the piano keyboard (key fills + note labels).
    #[serde(default = "default_true_pref")]
    pub huth_piano: bool,
    /// Tint the bar oscilloscope waveform by the detected fundamental pitch.
    #[serde(default)]
    pub huth_bar_osc: bool,
    /// Tint the ring oscilloscope waveform by the detected fundamental pitch.
    #[serde(default)]
    pub huth_ring_osc: bool,
    /// Color each spectrum bar by its centre-frequency pitch class, fading
    /// to grayscale at low amplitudes so silence reads neutral.
    #[serde(default)]
    pub huth_spectrum: bool,
    // ── Event stream display layers ─────────────────────────────────────────
    /// Show bass note events (Huth-colored circles).
    #[serde(default = "default_true_pref")]
    pub stream_bass_notes: bool,
    /// Show drum hits (small dots — kick white, hihat gray, clap bright).
    #[serde(default)]
    pub stream_drums: bool,
    /// Show Hz frequency scale on the Y axis.
    #[serde(default = "default_true_pref")]
    pub stream_hz_scale: bool,
    /// Show active ramp indicators.
    #[serde(default = "default_true_pref")]
    pub stream_ramps: bool,
    /// When true, LLM responses auto-scroll to the affected module.
    /// Off by default — mainly useful for demo recordings.
    #[serde(default)]
    pub llm_auto_scroll: bool,
    // ── Header visualization toggles ────────────────────────────────────────
    /// Show the linear (bar) oscilloscope in the header.  Off by default
    /// now that the spectrum analyser occupies the same slot; kept
    /// available via Preferences and set to on if both are enabled
    /// (oscilloscope draws on top).  The existing renderer lives in
    /// `scope_footer::draw_scope_colored` and will graduate to a
    /// rackable viz module — see PLAN.md.
    #[serde(default)]
    pub show_bar_oscilloscope: bool,
    /// Show the spectrum analyser (log-band FFT bars) in the header.
    /// Replaces the bar oscilloscope as the default center-panel viz.
    #[serde(default = "default_true_pref")]
    pub show_spectrum_bars: bool,
    /// Show the ring (circular) oscilloscope in the header.
    #[serde(default = "default_true_pref")]
    pub show_ring_oscilloscope: bool,
    /// Show the event stream (note/drum history) in the header.
    #[serde(default = "default_true_pref")]
    pub show_event_stream: bool,
    /// Show stereo/pan position indicator in the event stream.
    #[serde(default)]
    pub stream_stereo: bool,
    /// Rack grid columns (3–6). Determines cell size: rack_width / N.
    #[serde(default = "default_grid_cols")]
    pub rack_grid_cols: u8,
    /// When true, app startup reshapes the rack to match the active
    /// style's `rack_modules` (via `style_rack::apply`) instead of
    /// preserving whatever was saved in `session.json`.  Off by default
    /// so existing users keep their customised rack; opt-in for users
    /// who prefer a clean slate each time they relaunch a style.
    #[serde(default)]
    pub autosync_rack_on_start: bool,
}

fn default_grid_cols() -> u8 {
    5
}

fn default_true_pref() -> bool {
    true
}
fn default_phosphor_frames() -> usize {
    10
}
fn default_phosphor_intensity() -> f32 {
    0.6
}

/// Fixed knob body size (M = 55px).
pub const KNOB_PX: f32 = 55.0;
/// Fixed sequencer step button size (M = 34px).
pub const PAD_PX: f32 = 34.0;
/// Fixed XY control pad size (derived from PAD_PX).
pub const XY_PX: f32 = PAD_PX * (132.0 / 26.0);
/// Fixed envelope/ADSR display height (derived from XY_PX).
pub const ENV_H: f32 = XY_PX * 0.45;

impl UiPrefs {
    /// Knob body size in pixels (fixed at M = 55px).
    pub fn effective_knob_px(&self) -> f32 {
        KNOB_PX
    }
    /// Sequencer step button size in pixels (fixed at M = 34px).
    pub fn effective_pad_px(&self) -> f32 {
        PAD_PX
    }
    /// XY control pad size in pixels.
    pub fn effective_xy_px(&self) -> f32 {
        XY_PX
    }
    /// Envelope/ADSR display height in pixels.
    pub fn effective_env_h(&self) -> f32 {
        ENV_H
    }
}

impl Default for UiPrefs {
    fn default() -> Self {
        Self {
            bloom_enabled: false,
            bloom_intensity: 0.5,
            log_level_idx: 2, // Info
            ui_scale: 1.0,
            autosave_interval: AutosaveInterval::Immediate,
            wasd_as_arrows: false,
            crt_effect: false,
            phosphor_frames: default_phosphor_frames(),
            phosphor_intensity: default_phosphor_intensity(),
            huth_piano: true,
            huth_bar_osc: false,
            huth_ring_osc: false,
            huth_spectrum: false,
            stream_bass_notes: true,
            stream_drums: false,
            stream_hz_scale: true,
            stream_ramps: true,
            llm_auto_scroll: false,
            show_bar_oscilloscope: false,
            show_spectrum_bars: true,
            show_ring_oscilloscope: true,
            show_event_stream: true,
            stream_stereo: false,
            rack_grid_cols: 5,
            autosync_rack_on_start: false,
        }
    }
}
