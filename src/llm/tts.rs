// ─── llm/tts.rs ───────────────────────────────────────────────────────────────
// TTS output via Qwen3-TTS server.
//   speak_tts() — POST text + voice params to the TTS server, push rendered
//                 audio samples to the ring buffer for DSP FX routing.

use crate::audio::TtsSink;
use crate::state::{TtsModuleState, tts_types::TTS_PORT};

/// Prepare LLM text for synthesis: drop control characters (newlines,
/// tabs, escape codes) and cap at 200 characters.  Any printable Unicode
/// is kept — Qwen3-TTS reads 10 languages, unlike the ASCII-only NeuTTS
/// path this replaced.  Pure.
pub fn sanitize_tts_text(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_whitespace() { ' ' } else { c })
        .filter(|c| !c.is_control())
        .take(200)
        .collect::<String>()
        .trim()
        .to_string()
}

/// Speak `text` via the Qwen3-TTS server, pushing rendered PCM samples to the
/// audio ring buffer.  FX (reverb, bitcrush, etc.) are applied downstream by
/// the DSP via the Tts rack voice route.
///
/// Starts the installed TTS server on demand; logs startup/synthesis failures.
pub fn speak_tts(text: &str, tts: &TtsModuleState, tts_tx: &TtsSink) {
    let clean = sanitize_tts_text(text);
    if clean.is_empty() {
        return;
    }

    // Build JSON payload for Qwen3-TTS.
    let payload = serde_json::json!({
        "text": clean,
        "voice_ref": tts.voice_ref,
        "temperature": tts.temperature,
        "top_k": tts.top_k,
        "top_p": tts.top_p,
    });

    let url = format!("http://127.0.0.1:{}/synthesize", TTS_PORT);

    // Spawn a blocking thread so we don't block the LLM inference loop.
    let sink = tts_tx.clone();
    let pitch_snap = tts.pitch_snap;
    let mic = tts.clone();
    let root_note = 0u8; // caller can extend later
    let scale = crate::state::Scale::default();
    std::thread::spawn(move || {
        static SPEECH: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let Ok(_guard) = SPEECH.try_lock() else {
            log::debug!("TTS: skipped overlapping speech request");
            return;
        };
        if !super::tts_service::ensure_server() {
            return;
        }
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_connect(Some(std::time::Duration::from_secs(2)))
            .timeout_global(Some(std::time::Duration::from_secs(30)))
            .http_status_as_error(false)
            .build()
            .into();
        let client = match agent.post(&url).send_json(&payload) {
            Ok(resp) if resp.status().is_success() => resp,
            Ok(resp) => {
                let code = resp.status().as_u16();
                let body = resp.into_body().read_to_string().unwrap_or_default();
                log::warn!(
                    "TTS /synthesize returned {} — voice_ref may be missing or invalid. Body: {}",
                    code,
                    body.chars().take(200).collect::<String>()
                );
                return;
            }
            Err(e) => {
                log::warn!("TTS server unreachable: {}", e);
                return;
            }
        };

        // Read WAV bytes from response body.
        let mut wav_bytes = Vec::new();
        if let Err(e) =
            std::io::Read::read_to_end(&mut client.into_body().into_reader(), &mut wav_bytes)
        {
            log::warn!("TTS: failed to read response body: {}", e);
            return;
        }
        if wav_bytes.is_empty() {
            log::warn!("TTS: server returned empty body");
            return;
        }

        let target_sr = sink.target_sr;
        if let Some(mut samples) = read_wav_f32_bytes(&wav_bytes, target_sr) {
            if samples.is_empty() {
                log::warn!("TTS: returned WAV decoded to zero samples");
                return;
            }
            if pitch_snap && let Some(hz) = detect_pitch_hz(&samples, target_sr as f32) {
                let detected_midi = crate::audio::dsp::hz_to_midi(hz).round() as u8;
                let snapped_midi = crate::state::snap_to_scale(detected_midi, root_note, scale);
                let shift = snapped_midi as f32 - detected_midi as f32;
                samples = resample_pitch_shift(&samples, shift);
            }
            if mic.pitch_st.abs() > 0.01 {
                samples = resample_pitch_shift(&samples, mic.pitch_st.clamp(-12.0, 12.0));
            }
            samples =
                super::tts_effects::process_mic(&samples, target_sr, mic.mic_drive, mic.room_mix);
            let sample_count = samples.len();
            let mut tx = sink.tx.lock();
            let mut pushed = 0usize;
            for s in &samples {
                if tx.push(*s).is_ok() {
                    pushed += 1;
                }
            }
            log::info!(
                "TTS: pushed {}/{} samples to ring buffer (target_sr={}Hz)",
                pushed,
                sample_count,
                target_sr,
            );
        } else {
            log::warn!(
                "TTS: failed to decode WAV response ({} bytes)",
                wav_bytes.len()
            );
        }
    });
}

/// Minimal PCM-16 WAV reader from in-memory bytes — returns mono f32 samples
/// resampled to `target_sr` Hz.  Stereo is folded to mono by averaging.
/// Returns `None` on any parse error (not a valid RIFF/WAV).
fn read_wav_f32_bytes(bytes: &[u8], target_sr: u32) -> Option<Vec<f32>> {
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
            sum += crate::audio::audio_load::i16_pcm_to_f32(raw);
        }
        mono.push(sum / channels as f32);
    }

    // Resample to target_sr using linear interpolation.
    if src_rate == target_sr || mono.is_empty() {
        return Some(mono);
    }
    let ratio = target_sr as f64 / src_rate as f64;
    let new_len = ((mono.len() as f64) * ratio).round() as usize;
    let mut out = Vec::with_capacity(new_len);
    let step = 1.0 / ratio; // src-sample step per output-sample
    for i in 0..new_len {
        let src_pos = i as f64 * step;
        let idx = src_pos as usize;
        let frac = (src_pos - idx as f64) as f32;
        let a = mono.get(idx).copied().unwrap_or(0.0);
        let b = mono.get(idx + 1).copied().unwrap_or(a);
        out.push(a + (b - a) * frac);
    }
    Some(out)
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
pub(crate) fn resample_pitch_shift(samples: &[f32], semitones: f32) -> Vec<f32> {
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
