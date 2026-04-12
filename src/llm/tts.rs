// ─── llm/tts.rs ───────────────────────────────────────────────────────────────
// TTS output via NeuTTS Air server.
//   speak_neutts() — POST text + voice params to NeuTTS, push rendered audio
//                    samples to the ring buffer for DSP FX routing.

use parking_lot::Mutex;
use rtrb::Producer;
use std::sync::Arc;

use crate::state::{TtsModuleState, tts_types::NEUTTS_PORT};

/// Speak `text` via the NeuTTS Air server, pushing rendered PCM samples to the
/// audio ring buffer.  FX (reverb, bitcrush, etc.) are applied downstream by
/// the DSP via the NeuTts rack voice route.
///
/// No-ops silently if the NeuTTS server is unreachable.
pub fn speak_neutts(text: &str, tts: &TtsModuleState, tts_tx: &Arc<Mutex<Producer<f32>>>) {
    let clean: String = text
        .chars()
        .filter(|c| c.is_ascii_graphic() || *c == ' ')
        .take(200)
        .collect();
    if clean.is_empty() {
        return;
    }

    // Build JSON payload for NeuTTS Air.
    let payload = serde_json::json!({
        "text": clean,
        "voice_ref": tts.voice_ref,
        "temperature": tts.temperature,
        "top_k": tts.top_k,
        "top_p": tts.top_p,
    });

    let url = format!("http://127.0.0.1:{}/synthesize", NEUTTS_PORT);

    // Spawn a blocking thread so we don't block the LLM inference loop.
    let tts_tx = tts_tx.clone();
    let pitch_snap = tts.pitch_snap;
    let root_note = 0u8; // caller can extend later
    let scale = crate::state::Scale::default();
    std::thread::spawn(move || {
        let client = match ureq::AgentBuilder::new()
            .timeout_connect(std::time::Duration::from_secs(2))
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .post(&url)
            .send_json(&payload)
        {
            Ok(resp) => resp,
            Err(e) => {
                log::debug!("NeuTTS server unreachable: {}", e);
                return;
            }
        };

        // Read WAV bytes from response body.
        let mut wav_bytes = Vec::new();
        if client.into_reader().read_to_end(&mut wav_bytes).is_err() {
            return;
        }

        if let Some(mut samples) = read_wav_f32_bytes(&wav_bytes) {
            if pitch_snap && let Some(hz) = detect_pitch_hz(&samples, 44100.0) {
                let detected_midi = (12.0 * (hz / 440.0).log2() + 69.0).round() as u8;
                let snapped_midi = crate::state::snap_to_scale(detected_midi, root_note, scale);
                let shift = snapped_midi as f32 - detected_midi as f32;
                samples = resample_pitch_shift(&samples, shift);
            }
            let mut tx = tts_tx.lock();
            for s in &samples {
                let _ = tx.push(*s);
            }
        }
    });
}

/// Minimal PCM-16 WAV reader from in-memory bytes — returns mono f32 samples
/// normalised to +/-1.  Converts stereo to mono by averaging channels.
/// Returns `None` on any parse error (not a valid RIFF/WAV).
fn read_wav_f32_bytes(bytes: &[u8]) -> Option<Vec<f32>> {
    if bytes.len() < 44 {
        return None;
    }
    if &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return None;
    }

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
        pos += chunk_len + (chunk_len & 1);
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

    // Upsample to 44100 Hz if needed (2x linear interpolation for 22050->44100).
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
fn detect_pitch_hz(samples: &[f32], sample_rate: f32) -> Option<f32> {
    const WINDOW: usize = 4096;
    if samples.len() < WINDOW {
        return None;
    }
    let start = (samples.len() / 4).min(samples.len() - WINDOW);
    let w = &samples[start..start + WINDOW];

    let zero: f32 = w.iter().map(|s| s * s).sum();
    if zero < 1e-6 {
        return None;
    }

    let min_lag = (sample_rate / 500.0) as usize;
    let max_lag = (sample_rate / 50.0) as usize;

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
        return None;
    }

    Some(sample_rate / best_lag as f32)
}

/// Resample `samples` to shift pitch by `semitones` using linear interpolation.
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
