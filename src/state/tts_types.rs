// ─── state/tts.rs ────────────────────────────────────────────────────────────
// TTS-related types: conversation mode, voice character presets, per-module
// TTS state.  Extracted from state/mod.rs to stay under the line limit.

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

/// espeak-ng voice character preset for TTS output.
/// Auto = follow ConversationMode's default. Others override the voice selection.
#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
pub enum McVoiceChar {
    #[default]
    Auto, // follow ConversationMode defaults
    JungleMc,      // fast, high-pitched ragga MC (en+m3)
    RaveAnnouncer, // loud, rapid hype announcer (en+m2)
    Robot,         // robotic, flat, low (en+m7)
    SmoothDj,      // mid-low smooth DJ (en+m4)
}

/// EspeakNg: always available, low quality, zero setup.
/// CoquiTts: higher quality neural voice; requires `tts` CLI in PATH. Falls
///           back to espeak-ng automatically if the binary is not found.
#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
pub enum TtsEngine {
    #[default]
    EspeakNg,
    CoquiTts,
}

/// Per-module TTS settings.  Keyed by the TTS rack module's `id` in
/// `AppState.tts_modules`.  An agent speaks through a TTS module when
/// a control cable connects the agent to the TTS module.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TtsModuleState {
    pub id: u32,
    pub engine: TtsEngine,
    pub pitch: u8,               // 0 = mode default; 1–99 override
    pub speed: u16,              // 0 = mode default; words/min override
    pub amplitude: u8,           // 0 = default (100); 1–200 override
    pub voice_char: McVoiceChar, // voice character preset
    pub randomise: bool,         // ±10% pitch/speed jitter per utterance
    pub pitch_snap: bool,        // snap voice to nearest in-key note
}

impl TtsModuleState {
    pub fn new(id: u32) -> Self {
        Self {
            id,
            engine: TtsEngine::EspeakNg,
            pitch: 0,
            speed: 0,
            amplitude: 0,
            voice_char: McVoiceChar::Auto,
            randomise: false,
            pitch_snap: false,
        }
    }
}
