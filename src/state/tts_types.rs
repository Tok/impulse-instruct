// ─── state/tts_types.rs ──────────────────────────────────────────────────────
// TTS-related types: conversation mode, per-module NeuTTS settings.

use serde::{Deserialize, Serialize};

/// How the AI persona presents itself in the `_comment` field.
#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
pub enum ConversationMode {
    Off, // no commentary shown; brief technical label only
    #[default]
    Producer, // candid — what changed and why it serves the music
    Dj,  // hype DJ persona, cheesy party energy
    Mc,  // jungle/rave MC — "selector!", "junglist massive!", "rewind!"
}

/// NeuTTS server port.
pub const NEUTTS_PORT: u16 = 8770;

/// Per-module TTS settings for NeuTTS Air.  Keyed by the TTS rack module's
/// `id` in `AppState.tts_modules`.  An agent speaks through a TTS module when
/// a control cable connects the agent to the TTS module.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TtsModuleState {
    pub id: u32,
    /// Voice reference stem — matches a file pair in `voices/{ref}.wav` + `.txt`.
    #[serde(default = "default_voice_ref")]
    pub voice_ref: String,
    /// NeuTTS sampling temperature (0.5–1.5, default 0.8).
    #[serde(default = "default_temperature")]
    pub temperature: f32,
    /// NeuTTS top-k sampling (10–100, default 30).
    #[serde(default = "default_top_k")]
    pub top_k: u16,
    /// NeuTTS top-p sampling (0.5–1.0, default 0.9).
    #[serde(default = "default_top_p")]
    pub top_p: f32,
    /// Snap synthesised voice to the nearest in-key note.
    #[serde(default)]
    pub pitch_snap: bool,
}

fn default_voice_ref() -> String {
    "default".to_string()
}
fn default_temperature() -> f32 {
    0.8
}
fn default_top_k() -> u16 {
    30
}
fn default_top_p() -> f32 {
    0.9
}

impl TtsModuleState {
    pub fn new(id: u32) -> Self {
        Self {
            id,
            voice_ref: default_voice_ref(),
            temperature: default_temperature(),
            top_k: default_top_k(),
            top_p: default_top_p(),
            pitch_snap: false,
        }
    }
}
