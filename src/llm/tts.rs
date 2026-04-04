// ─── llm/tts.rs ───────────────────────────────────────────────────────────────
// TTS output via espeak-ng. Detached background process, no-op when not installed.

use crate::state::ConversationMode;

/// Speak `text` via espeak-ng in a detached background process.
/// `pitch` 0–99, `speed` words/min (80–500), `amplitude` 0–200 (100=default).
/// When any value is 0 the mode default is used.
/// No-ops silently if espeak-ng is not installed.
pub fn speak(text: &str, mode: &ConversationMode, pitch: u8, speed: u16, amplitude: u8) {
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

    let (default_pitch, default_speed, voice) = match mode {
        ConversationMode::Mc => (60u8, 160u16, "en+m3"), // high ragga MC
        ConversationMode::Dj => (40, 140, "en+m4"),      // hype DJ
        ConversationMode::Producer => (50, 120, "en+m5"), // calm producer
        ConversationMode::Off => return,
    };
    let p = if pitch == 0 { default_pitch } else { pitch };
    let s = if speed == 0 { default_speed } else { speed };
    let a = if amplitude == 0 { 100u8 } else { amplitude };

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
