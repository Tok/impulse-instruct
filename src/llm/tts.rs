// ─── llm/tts.rs ───────────────────────────────────────────────────────────────
// TTS output via espeak-ng. Detached background process, no-op when not installed.

use crate::state::{ConversationMode, McVoiceChar};

/// Speak `text` via espeak-ng in a detached background process.
/// `pitch` 0–99, `speed` words/min (80–500), `amplitude` 0–200 (100=default).
/// When any numeric value is 0 the mode default is used.
/// `voice_char` overrides the espeak-ng voice variant; Auto follows the mode default.
/// `randomise` applies ±10% jitter to pitch and speed each call.
/// No-ops silently if espeak-ng is not installed.
pub fn speak(
    text: &str,
    mode: &ConversationMode,
    pitch: u8,
    speed: u16,
    amplitude: u8,
    voice_char: &McVoiceChar,
    randomise: bool,
) {
    // Sanitise: strip anything that could be shell-injected.  We pass the
    // text as a single argument (no shell involved), but strip control chars.
    let clean: String = text
        .chars()
        .filter(|c| c.is_ascii_graphic() || *c == ' ')
        .take(200)
        .collect();
    if clean.is_empty() {
        return;
    }

    let (default_pitch, default_speed, mode_voice) = match mode {
        ConversationMode::Mc => (60u8, 160u16, "en+m3"), // high ragga MC
        ConversationMode::Dj => (40, 140, "en+m4"),      // hype DJ
        ConversationMode::Producer => (50, 120, "en+m5"), // calm producer
        ConversationMode::Off => return,
    };

    // Voice character overrides mode voice when not Auto.
    let voice = match voice_char {
        McVoiceChar::Auto => mode_voice,
        McVoiceChar::JungleMc => "en+m3",      // fast, high ragga
        McVoiceChar::RaveAnnouncer => "en+m2", // loud rapid hype
        McVoiceChar::Robot => "en+m7",         // robotic/flat
        McVoiceChar::SmoothDj => "en+m4",      // smooth mid-low
    };

    let p = if pitch == 0 { default_pitch } else { pitch };
    let s = if speed == 0 { default_speed } else { speed };
    let a = if amplitude == 0 { 100u8 } else { amplitude };

    // ±10% jitter using subsecond timestamp as a cheap noise source.
    let (p, s) = if randomise {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .subsec_nanos();
        // Two independent values from high/low halves of nanos.
        let r1 = ((nanos & 0xFFFF) as f32 / 0xFFFF as f32) * 2.0 - 1.0; // -1..1
        let r2 = ((nanos >> 16) as f32 / 0xFFFF as f32) * 2.0 - 1.0;
        let jp = ((p as f32) * (1.0 + r1 * 0.10)).clamp(1.0, 99.0) as u8;
        let js = ((s as f32) * (1.0 + r2 * 0.10)).clamp(80.0, 500.0) as u16;
        (jp, js)
    } else {
        (p, s)
    };

    // Spawn and detach — we don't wait for it.
    let _ = std::process::Command::new("espeak-ng")
        .args([
            "-p",
            &p.to_string(),
            "-s",
            &s.to_string(),
            "-a",
            &a.to_string(),
            "-v",
            voice,
            &clean,
        ])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
}
