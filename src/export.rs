// ─── export.rs ────────────────────────────────────────────────────────────────
// Offline rendering: renders the current pattern to a WAV (and optionally MP3).
// No audio device needed — runs DspState + advance_clock in a tight loop.

use std::path::{Path, PathBuf};

use crate::audio::dsp::{AudioParams, DspState};
use crate::sequencer::{ClockState, advance_clock};
use crate::state::{AppState, DrumVoice, compile_fx_plan};

const EXPORT_SR: f32 = 44100.0;
const BLOCK_SIZE: usize = 512;

// ─── Render ───────────────────────────────────────────────────────────────────

/// Render `bars` bars of the current pattern offline.
/// Returns a mono f32 PCM buffer at 44100 Hz.
fn render_bars(state: &AppState, bars: u32) -> Vec<f32> {
    let mut params = AudioParams::from_app_state(state);
    params.sample_rate = EXPORT_SR;

    // Dummy TTS consumer — export never mixes live TTS audio.
    let (_tts_tx, tts_rx) = rtrb::RingBuffer::<f32>::new(1);
    let mut dsp = DspState::new(EXPORT_SR, params, compile_fx_plan(&state.rack), tts_rx);
    let mut clock = ClockState::default();

    // Force sequencer running for export regardless of UI play state
    let mut seq = state.sequencer.clone();
    seq.running = true;

    let beats_per_bar = 4.0_f32;
    let samples_per_beat = EXPORT_SR * 60.0 / seq.bpm;
    let total_samples = (bars as f32 * beats_per_bar * samples_per_beat) as usize;

    let mut out = vec![0.0f32; total_samples];
    let mut pos = 0usize;
    let mut block_buf = vec![0.0f32; BLOCK_SIZE];

    while pos < total_samples {
        let remaining = total_samples - pos;
        let n = remaining.min(BLOCK_SIZE);
        let block = &mut block_buf[..n];

        let (new_clock, events) = advance_clock(clock.clone(), &seq, n, EXPORT_SR);
        clock = new_clock;
        for event in events {
            dsp.handle_trigger(&event);
        }
        dsp.process_block(block, 1);

        out[pos..pos + n].copy_from_slice(block);
        pos += n;
    }

    out
}

// ─── WAV export ──────────────────────────────────────────────────────────────

/// Render and write a WAV file. Returns the path written.
pub fn export_wav(state: &AppState, bars: u32) -> Result<PathBuf, String> {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let path = PathBuf::from(format!("export-{}.wav", ts));

    let samples = render_bars(state, bars);
    write_wav(&path, &samples)?;
    Ok(path)
}

fn write_wav(path: &Path, samples: &[f32]) -> Result<(), String> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: EXPORT_SR as u32,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut writer =
        hound::WavWriter::create(path, spec).map_err(|e| format!("WAV create error: {e}"))?;
    for &s in samples {
        writer
            .write_sample(s)
            .map_err(|e| format!("WAV write error: {e}"))?;
    }
    writer
        .finalize()
        .map_err(|e| format!("WAV finalise error: {e}"))?;
    Ok(())
}

// ─── MP3 export ──────────────────────────────────────────────────────────────

/// Render to WAV then transcode to MP3 via `ffmpeg`.
/// Returns the MP3 path on success. Falls back to WAV path if ffmpeg is absent.
pub fn export_mp3(state: &AppState, bars: u32) -> Result<PathBuf, String> {
    let wav_path = export_wav(state, bars)?;

    let mp3_path = wav_path.with_extension("mp3");
    let status = std::process::Command::new("ffmpeg")
        .args([
            "-y",
            "-i",
            wav_path.to_str().unwrap_or("export.wav"),
            "-codec:a",
            "libmp3lame",
            "-qscale:a",
            "2", // ~190 kbps VBR
            mp3_path.to_str().unwrap_or("export.mp3"),
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();

    match status {
        Ok(s) if s.success() => {
            // Remove the intermediate WAV
            let _ = std::fs::remove_file(&wav_path);
            Ok(mp3_path)
        }
        Ok(_) => Err(format!(
            "ffmpeg exited with error — WAV saved at {}",
            wav_path.display()
        )),
        Err(_) => Err(format!(
            "ffmpeg not found — WAV saved at {} (install ffmpeg to export MP3)",
            wav_path.display()
        )),
    }
}

// ─── Stem export ─────────────────────────────────────────────────────────────

/// A named audio stem (voice label + rendered PCM).
pub struct Stem {
    pub name: String,
    pub samples: Vec<f32>,
}

/// Render per-voice stems for `bars` bars.
/// Each stem isolates one voice group by zeroing out all others.
/// Returns one Stem per active group (skips disabled voices).
pub fn render_stems(state: &AppState, bars: u32) -> Vec<Stem> {
    let mut stems = Vec::new();

    // Helper: zero all drum volumes in a state clone.
    let silence_drums = |mut s: AppState| -> AppState {
        for v in DrumVoice::ALL {
            s = DrumVoice::set_volume(*v, s, 0.0);
        }
        s
    };

    // Helper: zero all melodic voice volumes in a state clone.
    let silence_melodic = |mut s: AppState| -> AppState {
        s.bass.volume = 0.0;
        s.noise_voice.volume = 0.0;
        s.hoover.volume = 0.0;
        s.an1x.volume = 0.0;
        s.amen.volume = 0.0;
        s
    };

    // ── Bass (TB-303) ─────────────────────────────────────────────────────────
    if state.bass.volume > 0.01 {
        let mut s = silence_drums(state.clone());
        s.noise_voice.volume = 0.0;
        s.hoover.volume = 0.0;
        s.an1x.volume = 0.0;
        s.amen.volume = 0.0;
        stems.push(Stem {
            name: "bass".into(),
            samples: render_bars(&s, bars),
        });
    }

    // ── Kit A (808-style) ─────────────────────────────────────────────────────
    {
        let kit_a_voices = [
            DrumVoice::Kick808,
            DrumVoice::Snare808,
            DrumVoice::HihatClosed808,
            DrumVoice::HihatOpen808,
            DrumVoice::TomHi808,
            DrumVoice::TomMid808,
            DrumVoice::TomLo808,
        ];
        let active = kit_a_voices.iter().any(|v| v.get_volume(state) > 0.01);
        if active {
            let mut s = silence_melodic(state.clone());
            for v in DrumVoice::ALL {
                if !kit_a_voices.contains(v) {
                    s = DrumVoice::set_volume(*v, s, 0.0);
                }
            }
            stems.push(Stem {
                name: "kit_a".into(),
                samples: render_bars(&s, bars),
            });
        }
    }

    // ── Kit B (909-style) ─────────────────────────────────────────────────────
    {
        let kit_b_voices = [
            DrumVoice::Kick909,
            DrumVoice::Snare909,
            DrumVoice::HihatClosed909,
            DrumVoice::HihatOpen909,
            DrumVoice::Clap909,
            DrumVoice::Rim909,
        ];
        let active = kit_b_voices.iter().any(|v| v.get_volume(state) > 0.01);
        if active {
            let mut s = silence_melodic(state.clone());
            for v in DrumVoice::ALL {
                if !kit_b_voices.contains(v) {
                    s = DrumVoice::set_volume(*v, s, 0.0);
                }
            }
            stems.push(Stem {
                name: "kit_b".into(),
                samples: render_bars(&s, bars),
            });
        }
    }

    // ── Amen sampler ─────────────────────────────────────────────────────────
    if state.amen.volume > 0.01 {
        let mut s = silence_melodic(state.clone());
        s.amen.volume = state.amen.volume; // restore amen
        s = silence_drums(s);
        s = DrumVoice::set_volume(DrumVoice::Amen, s, state.amen.volume);
        stems.push(Stem {
            name: "amen".into(),
            samples: render_bars(&s, bars),
        });
    }

    // ── Noise voice ───────────────────────────────────────────────────────────
    if state.noise_voice.enabled && state.noise_voice.volume > 0.01 {
        let mut s = silence_drums(state.clone());
        s.bass.volume = 0.0;
        s.hoover.volume = 0.0;
        s.an1x.volume = 0.0;
        s.amen.volume = 0.0;
        stems.push(Stem {
            name: "noise".into(),
            samples: render_bars(&s, bars),
        });
    }

    // ── Hoover ────────────────────────────────────────────────────────────────
    if state.hoover.enabled && state.hoover.volume > 0.01 {
        let mut s = silence_drums(state.clone());
        s.bass.volume = 0.0;
        s.noise_voice.volume = 0.0;
        s.an1x.volume = 0.0;
        s.amen.volume = 0.0;
        stems.push(Stem {
            name: "hoover".into(),
            samples: render_bars(&s, bars),
        });
    }

    // ── AN1X ─────────────────────────────────────────────────────────────────
    if state.an1x.enabled && state.an1x.volume > 0.01 {
        let mut s = silence_drums(state.clone());
        s.bass.volume = 0.0;
        s.noise_voice.volume = 0.0;
        s.hoover.volume = 0.0;
        s.amen.volume = 0.0;
        stems.push(Stem {
            name: "an1x".into(),
            samples: render_bars(&s, bars),
        });
    }

    stems
}

/// Render stems and write each to `<prefix>-<name>.wav`.
/// Returns the list of paths written on success, or the first error.
pub fn export_stems(state: &AppState, bars: u32, prefix: &str) -> Result<Vec<PathBuf>, String> {
    let stems = render_stems(state, bars);
    let mut paths = Vec::new();
    for stem in stems {
        let path = PathBuf::from(format!("{}-{}.wav", prefix, stem.name));
        write_wav(&path, &stem.samples)?;
        paths.push(path);
    }
    Ok(paths)
}
