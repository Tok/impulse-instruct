// ─── llm/tts.rs ───────────────────────────────────────────────────────────────
// TTS output via espeak-ng.
//   speak()    — detached background process, no FX, lowest latency.
//   speak_fx() — renders to WAV, applies pitch-snap, pushes raw samples to ring buffer.
//                FX (reverb, bitcrush, etc.) are applied by the DSP via rack cables.

use parking_lot::Mutex;
use rtrb::Producer;
use std::sync::Arc;

use crate::state::{ConversationMode, McVoiceChar};

/// Voice rendering parameters shared between espeak and coqui paths.
pub struct TtsParams<'a> {
    pub pitch: u8,     // 0 = mode default; 1–99 override
    pub speed: u16,    // 0 = mode default; words/min override
    pub amplitude: u8, // 0 = default (100); 1–200 override
    pub voice_char: &'a McVoiceChar,
    pub randomise: bool,  // ±10% pitch/speed jitter per utterance
    pub pitch_snap: bool, // snap to nearest in-key note after rendering
    pub root_note: u8,
    pub scale: crate::state::Scale,
}

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

// ─── FX path: render → WAV → optional pitch-snap → audio ring buffer ──────────

/// Like `speak()` but renders to a WAV, optionally pitch-snaps, and pushes raw
/// samples to the audio ring buffer. FX (reverb, etc.) are applied downstream
/// by the DSP via the EspeakNgTts rack voice route. Falls back to `speak()` when
/// pitch-snap is off (lower latency, no temp-file round-trip).
pub fn speak_fx(
    text: &str,
    mode: &ConversationMode,
    p: &TtsParams<'_>,
    tts_tx: &Arc<Mutex<Producer<f32>>>,
) {
    // Fall back to direct playback when pitch-snap is off.
    if !p.pitch_snap {
        speak(
            text,
            mode,
            p.pitch,
            p.speed,
            p.amplitude,
            p.voice_char,
            p.randomise,
        );
        return;
    }

    let clean: String = text
        .chars()
        .filter(|c| c.is_ascii_graphic() || *c == ' ')
        .take(200)
        .collect();
    if clean.is_empty() {
        return;
    }

    let (default_pitch, default_speed, mode_voice) = match mode {
        ConversationMode::Mc => (60u8, 160u16, "en+m3"),
        ConversationMode::Dj => (40, 140, "en+m4"),
        ConversationMode::Producer => (50, 120, "en+m5"),
        ConversationMode::Off => return,
    };
    let voice = match p.voice_char {
        McVoiceChar::Auto => mode_voice,
        McVoiceChar::JungleMc => "en+m3",
        McVoiceChar::RaveAnnouncer => "en+m2",
        McVoiceChar::Robot => "en+m7",
        McVoiceChar::SmoothDj => "en+m4",
    };

    let ep = if p.pitch == 0 { default_pitch } else { p.pitch };
    let es = if p.speed == 0 { default_speed } else { p.speed };
    let ea = if p.amplitude == 0 { 100u8 } else { p.amplitude };

    let (ep, es) = if p.randomise {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .subsec_nanos();
        let r1 = ((nanos & 0xFFFF) as f32 / 0xFFFF as f32) * 2.0 - 1.0;
        let r2 = ((nanos >> 16) as f32 / 0xFFFF as f32) * 2.0 - 1.0;
        (
            ((ep as f32) * (1.0 + r1 * 0.10)).clamp(1.0, 99.0) as u8,
            ((es as f32) * (1.0 + r2 * 0.10)).clamp(80.0, 500.0) as u16,
        )
    } else {
        (ep, es)
    };

    // Render to a temp WAV file.
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos();
    let tmp_path = format!("/tmp/impulse_tts_{}.wav", nanos);

    let render_ok = std::process::Command::new("espeak-ng")
        .args([
            "-p",
            &ep.to_string(),
            "-s",
            &es.to_string(),
            "-a",
            &ea.to_string(),
            "-v",
            voice,
            "-w",
            &tmp_path,
            &clean,
        ])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .is_ok();

    if !render_ok {
        return;
    }

    // Decode WAV and apply FX.
    if let Some(mut samples) = read_wav_f32(&tmp_path) {
        // Pitch-snap: shift voice to nearest in-key note (T-Pain / jungle MC effect).
        if p.pitch_snap
            && let Some(hz) = detect_pitch_hz(&samples, 44100.0)
        {
            let detected_midi = (12.0 * (hz / 440.0).log2() + 69.0).round() as u8;
            let snapped_midi = crate::state::snap_to_scale(detected_midi, p.root_note, p.scale);
            let shift = snapped_midi as f32 - detected_midi as f32;
            samples = resample_pitch_shift(&samples, shift);
        }
        // Push raw samples — FX applied by the DSP via EspeakNgTts rack voice route.
        let mut tx = tts_tx.lock();
        for s in &samples {
            let _ = tx.push(*s);
        }
    }

    let _ = std::fs::remove_file(&tmp_path);
}

// ─── Coqui TTS path ───────────────────────────────────────────────────────────

/// Render `text` using the Coqui TTS CLI (`tts` binary), apply FX, and push to
/// the audio ring buffer.  Falls back silently to `speak_fx()` when:
///   - the `tts` binary is not in PATH, or
///   - synthesis fails for any reason.
///
/// Coqui TTS CLI usage: `tts --text "…" --out_path /tmp/out.wav`
/// Uses the default model that ships with Coqui TTS.
pub fn speak_coqui(
    text: &str,
    mode: &ConversationMode,
    p: &TtsParams<'_>,
    tts_tx: &Arc<Mutex<Producer<f32>>>,
) {
    let clean: String = text
        .chars()
        .filter(|c| c.is_ascii_graphic() || *c == ' ')
        .take(200)
        .collect();
    if clean.is_empty() {
        return;
    }

    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos();
    let tmp_path = format!("/tmp/impulse_coqui_{}.wav", nanos);

    let coqui_ok = std::process::Command::new("tts")
        .args(["--text", &clean, "--out_path", &tmp_path])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    if !coqui_ok {
        log::debug!("Coqui TTS unavailable — falling back to espeak-ng");
        speak_fx(text, mode, p, tts_tx);
        let _ = std::fs::remove_file(&tmp_path);
        return;
    }

    if let Some(mut samples) = read_wav_f32(&tmp_path) {
        if p.pitch_snap
            && let Some(hz) = detect_pitch_hz(&samples, 44100.0)
        {
            let detected_midi = (12.0 * (hz / 440.0).log2() + 69.0).round() as u8;
            let snapped_midi = crate::state::snap_to_scale(detected_midi, p.root_note, p.scale);
            let shift = snapped_midi as f32 - detected_midi as f32;
            samples = resample_pitch_shift(&samples, shift);
        }
        let mut tx = tts_tx.lock();
        for s in &samples {
            let _ = tx.push(*s);
        }
    }

    let _ = std::fs::remove_file(&tmp_path);
}

/// Minimal PCM-16 WAV reader — returns mono f32 samples normalised to ±1.
/// Converts stereo to mono by averaging channels.
/// Returns `None` on any parse error (not a valid RIFF/WAV).
fn read_wav_f32(path: &str) -> Option<Vec<f32>> {
    let bytes = std::fs::read(path).ok()?;
    if bytes.len() < 44 {
        return None;
    }
    if &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return None;
    }

    // Walk chunks looking for fmt and data.
    let mut pos = 12usize;
    let mut channels = 1u16;
    let mut src_rate = 22050u32;
    let mut bits = 16u16;
    let mut data_start = 0usize;
    let mut data_len = 0usize;

    while pos + 8 <= bytes.len() {
        let tag = &bytes[pos..pos + 4];
        let chunk_len = u32::from_le_bytes(bytes[pos + 4..pos + 8].try_into().ok()?) as usize;
        pos += 8;
        if tag == b"fmt " && chunk_len >= 16 {
            channels = u16::from_le_bytes(bytes[pos + 2..pos + 4].try_into().ok()?);
            src_rate = u32::from_le_bytes(bytes[pos + 4..pos + 8].try_into().ok()?);
            bits = u16::from_le_bytes(bytes[pos + 14..pos + 16].try_into().ok()?);
        } else if tag == b"data" {
            data_start = pos;
            data_len = chunk_len;
            break;
        }
        pos += chunk_len + (chunk_len & 1); // word-align
    }

    if data_start == 0 || bits != 16 {
        return None;
    }

    let frame_bytes = (channels as usize) * 2;
    let n_frames = data_len / frame_bytes;
    let mut mono = Vec::with_capacity(n_frames);

    for i in 0..n_frames {
        let base = data_start + i * frame_bytes;
        let mut sum = 0.0f32;
        for ch in 0..channels as usize {
            let off = base + ch * 2;
            if off + 2 > bytes.len() {
                break;
            }
            let raw = i16::from_le_bytes(bytes[off..off + 2].try_into().ok()?);
            sum += raw as f32 / 32768.0;
        }
        mono.push(sum / channels as f32);
    }

    // Upsample to 44100 Hz if needed (2× linear interpolation for 22050→44100).
    if src_rate == 22050 {
        let mut up = Vec::with_capacity(mono.len() * 2);
        for i in 0..mono.len() {
            let a = mono[i];
            let b = if i + 1 < mono.len() { mono[i + 1] } else { 0.0 };
            up.push(a);
            up.push((a + b) * 0.5);
        }
        return Some(up);
    }

    Some(mono)
}

/// Detect the dominant fundamental frequency (Hz) of `samples` at `sample_rate` Hz.
/// Uses normalised autocorrelation over a 4096-sample window from the middle of the
/// signal. Detects in the 50–500 Hz speech range. Returns `None` for silence or
/// clearly unpitched signals.
fn detect_pitch_hz(samples: &[f32], sample_rate: f32) -> Option<f32> {
    const WINDOW: usize = 4096;
    if samples.len() < WINDOW {
        return None;
    }
    // Start a quarter of the way in to skip any leading silence.
    let start = (samples.len() / 4).min(samples.len() - WINDOW);
    let w = &samples[start..start + WINDOW];

    let zero: f32 = w.iter().map(|s| s * s).sum();
    if zero < 1e-6 {
        return None; // silence
    }

    let min_lag = (sample_rate / 500.0) as usize; // 500 Hz max
    let max_lag = (sample_rate / 50.0) as usize; // 50 Hz min

    let mut best_lag = 0usize;
    let mut best_corr = 0.0_f32;

    for lag in min_lag..=max_lag.min(WINDOW / 2) {
        let corr: f32 = w[..WINDOW - lag]
            .iter()
            .zip(&w[lag..])
            .map(|(a, b)| a * b)
            .sum::<f32>()
            / zero;
        if corr > best_corr {
            best_corr = corr;
            best_lag = lag;
        }
    }

    if best_corr < 0.25 || best_lag == 0 {
        return None; // no clear fundamental
    }

    Some(sample_rate / best_lag as f32)
}

/// Resample `samples` to shift pitch by `semitones` using linear interpolation.
/// Positive = shift up, negative = shift down. Duration changes slightly (shorter
/// when shifting up) — acceptable for short TTS phrases.
fn resample_pitch_shift(samples: &[f32], semitones: f32) -> Vec<f32> {
    let ratio = 2.0_f32.powf(semitones / 12.0);
    let new_len = (samples.len() as f32 / ratio) as usize;
    let mut out = Vec::with_capacity(new_len);
    for i in 0..new_len {
        let src = i as f32 * ratio;
        let idx = src as usize;
        let frac = src - idx as f32;
        let a = samples.get(idx).copied().unwrap_or(0.0);
        let b = samples.get(idx + 1).copied().unwrap_or(0.0);
        out.push(a + (b - a) * frac);
    }
    out
}
