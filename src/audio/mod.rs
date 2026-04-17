// ─── audio/mod.rs ────────────────────────────────────────────────────────────
// Audio engine: owns the cpal stream, DSP state, and sequencer clock.
// The audio callback is real-time safe: no allocations, no locks.

pub mod analysis;
pub mod dsp;
pub mod onset;
pub mod spectrum;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, SampleFormat, Stream, StreamConfig};
use parking_lot::Mutex;
use rtrb::{Consumer, Producer};
use std::sync::Arc;

use crate::sequencer::{ClockState, TriggerEvent, advance_clock};
use crate::state::{AppState, FxPlan, compile_fx_plan};

pub use dsp::{AudioParams, DspState};

// ─── Internal sample rate ────────────────────────────────────────────────────
// All DSP, samplers and captures run at this fixed rate.  External files
// (WAV, TTS output) are resampled on load.  The device rate is tracked
// separately on the cpal stream and only matters at the I/O boundary.

/// Internal sample rate as f32 — the canonical form used by DSP/analysis.
pub const SAMPLE_RATE: f32 = 44_100.0;

/// Same rate as an integer — use for `Vec` lengths, WAV headers, and any
/// context where a `usize` / `u32` is expected.
pub const SAMPLE_RATE_HZ: u32 = 44_100;

/// Handle for pushing TTS audio to the DSP mix bus.
/// Carries the target sample rate so the TTS pipeline can resample its WAV
/// output to match the device rate — otherwise 24 kHz NeuTTS output played at
/// 48 kHz ends up chipmunked and half-length (perceived as silence).
#[derive(Clone)]
pub struct TtsSink {
    pub tx: Arc<Mutex<Producer<f32>>>,
    pub target_sr: u32,
}

// ─── Messages sent from UI/HTTP thread to audio thread ───────────────────────

pub enum AudioCommand {
    UpdateParams(Box<AudioParams>),
    Trigger(TriggerEvent),
    /// Live monitor gain (0.0–1.0). Applied after DSP, not saved to state,
    /// never reaches the export path — exports always render at full volume.
    SetMonitorVolume(f32),
    /// Load new sample data into the amen/WAV sampler voice.
    /// The Arc is just a pointer copy — allocation-free in the audio callback.
    LoadSampler(Arc<Vec<f32>>),
    /// Load sample data into the granular texture voice.
    LoadGranular(Arc<Vec<f32>>),
    /// Updated FX routing plan derived from the rack cable graph.
    /// Sent whenever the rack topology changes (connect/disconnect/enable).
    SetFxPlan(FxPlan),
}

// ─── Audio Engine ─────────────────────────────────────────────────────────────

pub struct AudioEngine {
    pub params_tx: Producer<AudioCommand>,
    pub scope_rx: Consumer<f32>,
    /// Audio capture ring buffer — same samples as scope but larger window
    /// (~10 s). Drain in the UI thread and pass to `analysis::analyse_audio`.
    pub capture_rx: Consumer<f32>,
    /// TTS processed audio pushed by the LLM thread, mixed into the output.
    pub tts_tx: TtsSink,
    /// MIDI clock bytes (0xF8/0xFA/0xFC) produced by the audio thread.
    /// Drain this in a dedicated thread and forward to a MIDI output port.
    pub midi_clock_rx: Consumer<u8>,
    /// DSP load fraction (0.0–1.0) per callback invocation.
    /// Drain in UI thread for sparkline display.
    pub dsp_load_rx: Consumer<f32>,
    /// Interleaved L,R stereo samples for correlation meter.
    pub stereo_rx: Consumer<f32>,
    /// Rolling ~15s tap of master output mono for the granular panel's
    /// CAPTURE button.  Drained by the UI only while a capture is active.
    pub granular_capture_rx: Consumer<f32>,
    /// Negotiated sample rate (Hz).
    pub sample_rate: u32,
    /// Audio callback block size (frames).
    pub block_size: u32,
    _stream: Stream, // kept alive
}

impl AudioEngine {
    pub fn new(state: Arc<parking_lot::RwLock<AppState>>) -> anyhow::Result<Self> {
        log::info!("  cpal: getting default host…");
        let host = cpal::default_host();
        log::info!("  cpal: getting default output device…");
        let device: Device = host
            .default_output_device()
            .ok_or_else(|| anyhow::anyhow!("No audio output device found"))?;
        log::info!("  cpal: querying output config…");
        // default_output_config can hang on PipeWire — run with timeout
        let supported = {
            let (tx, rx) = std::sync::mpsc::channel();
            let dev = device.clone();
            std::thread::spawn(move || {
                let _ = tx.send(dev.default_output_config());
            });
            match rx.recv_timeout(std::time::Duration::from_secs(5)) {
                Ok(Ok(cfg)) => cfg,
                Ok(Err(e)) => return Err(anyhow::anyhow!("Audio config error: {e}")),
                Err(_) => {
                    return Err(anyhow::anyhow!(
                        "Audio device timed out (5s). PipeWire/PulseAudio may be unresponsive. \
                     Try: systemctl --user restart pipewire"
                    ));
                }
            }
        };
        let config = StreamConfig {
            channels: supported.channels(),
            sample_rate: supported.sample_rate(),
            buffer_size: cpal::BufferSize::Default,
        };
        let sample_rate = config.sample_rate.0 as f32;
        let channels = config.channels as usize;

        // Ring buffer: UI → audio thread
        let (params_tx, mut params_rx) = rtrb::RingBuffer::<AudioCommand>::new(256);

        // Ring buffer: audio thread → scope display
        let (mut scope_tx, scope_rx) = rtrb::RingBuffer::<f32>::new(4096);

        // Ring buffer: audio thread → capture/analysis (≈10 s at engine rate)
        let (mut capture_tx, capture_rx) =
            rtrb::RingBuffer::<f32>::new(SAMPLE_RATE_HZ as usize * 10);

        // Ring buffer: audio thread → granular CAPTURE button (≈15 s mono).
        // Always populated with the current master output; the UI drains it
        // only while a capture is active.  Separate from capture_rx because
        // that one's already consumed by the analyzer + LLM strip.
        let (mut granular_capture_tx, granular_capture_rx) =
            rtrb::RingBuffer::<f32>::new(SAMPLE_RATE_HZ as usize * 15);

        // Ring buffer: audio thread → stereo correlation meter (interleaved L,R pairs)
        let (mut stereo_tx, stereo_rx) = rtrb::RingBuffer::<f32>::new(8192);

        // Ring buffer: TTS processed audio → audio thread mix (≈6s @ 44100Hz)
        let (tts_producer, tts_consumer) = rtrb::RingBuffer::<f32>::new(262144);
        let tts_tx = TtsSink {
            tx: Arc::new(Mutex::new(tts_producer)),
            target_sr: config.sample_rate.0,
        };

        // Ring buffer: audio thread → MIDI clock output thread (1 byte per tick, 24 PPQN)
        let (mut midi_clock_tx, midi_clock_rx) = rtrb::RingBuffer::<u8>::new(512);

        // Ring buffer: audio thread → UI DSP load meter (one f32 per callback)
        let (mut dsp_load_tx, dsp_load_rx) = rtrb::RingBuffer::<f32>::new(256);

        // Audio-thread-local DSP state
        let (initial_params, initial_fx_plan) = {
            let s = state.read();
            (AudioParams::from_app_state(&s), compile_fx_plan(&s.rack))
        };
        let mut dsp = DspState::new(sample_rate, initial_params, initial_fx_plan, tts_consumer);
        let mut clock = ClockState::default();
        let mut monitor_vol = 1.0_f32;
        // MIDI clock out: accumulator tracks fractional samples until next 0xF8 tick
        let mut midi_clock_acc = 0.0_f64;
        let mut midi_clock_running = false; // tracks sequencer running state for Start/Stop

        let state_clone = Arc::clone(&state);

        log::info!(
            "Opening audio stream ({} Hz, {} ch)… (if stuck, kill stale impulse-instruct processes)",
            sample_rate,
            channels
        );
        let stream = match supported.sample_format() {
            SampleFormat::F32 => device.build_output_stream(
                &config,
                move |output: &mut [f32], _| {
                    // Drain incoming commands
                    while let Ok(cmd) = params_rx.pop() {
                        match cmd {
                            AudioCommand::UpdateParams(p) => dsp.update_params(*p),
                            AudioCommand::Trigger(e) => dsp.handle_trigger(&e),
                            AudioCommand::SetMonitorVolume(v) => monitor_vol = v,
                            AudioCommand::LoadSampler(data) => dsp.load_amen(data),
                            AudioCommand::LoadGranular(data) => dsp.load_granular(data),
                            AudioCommand::SetFxPlan(plan) => dsp.set_fx_plan(plan),
                        }
                    }

                    let block = output.len() / channels;

                    // Advance sequencer — snapshot seq state (no lock held during DSP)
                    let (seq_snap, chain_snap, chain_enabled, chain_pos_snap) = {
                        let s = state_clone.read();
                        let mut seq = s.sequencer.clone();
                        // Sync voice-enabled flags from AppState.bass_voices into the snapshot.
                        for (i, v) in s
                            .bass_voices
                            .iter()
                            .enumerate()
                            .take(crate::state::MAX_BASS_VOICES)
                        {
                            seq.bass_voice_enabled[i] = v.enabled;
                        }
                        (seq, s.chain.clone(), s.chain_enabled, s.chain_pos)
                    };

                    let prev_loop_count = clock.loop_count;
                    let (new_clock, events) =
                        advance_clock(clock.clone(), &seq_snap, block, sample_rate);
                    clock = new_clock;

                    // Propagate current_step back; advance chain on each loop boundary.
                    {
                        let mut s = state_clone.write();
                        // Increment monotonic step counter when the step advances.
                        if clock.current_step != s.sequencer.current_step {
                            s.global_step_count += 1;
                        }
                        s.sequencer.current_step = clock.current_step;
                        if clock.loop_count != prev_loop_count
                            && chain_enabled
                            && !chain_snap.is_empty()
                        {
                            let next_slot = chain_snap[chain_pos_snap % chain_snap.len()];
                            let bpm = s.sequencer.bpm;
                            let swing = s.sequencer.swing;
                            let running = s.sequencer.running;
                            // Auto-save current edits to the active bank slot before switching.
                            let current_edit = s.pattern_edit;
                            let snap = s.sequencer.clone();
                            if let Some(slot) = s.pattern_bank.get_mut(current_edit) {
                                *slot = snap;
                            }
                            s.chain_pos = (chain_pos_snap + 1) % chain_snap.len();
                            s.sequencer =
                                s.pattern_bank.get(next_slot).cloned().unwrap_or_default();
                            s.sequencer.bpm = bpm;
                            s.sequencer.swing = swing;
                            s.sequencer.running = running;
                            s.sequencer.current_step = clock.current_step;
                            s.pattern_edit = next_slot;
                        }
                    }

                    for event in events {
                        dsp.handle_trigger(&event);
                    }

                    // MIDI clock out — 24 PPQN ticks + Start/Stop transport messages
                    {
                        let running_now = seq_snap.running;
                        if running_now && !midi_clock_running {
                            midi_clock_acc = 0.0;
                            midi_clock_tx.push(0xFA).ok(); // MIDI Start
                        } else if !running_now && midi_clock_running {
                            midi_clock_tx.push(0xFC).ok(); // MIDI Stop
                        }
                        midi_clock_running = running_now;

                        if running_now {
                            // tick_interval = sr * 60 / (bpm * 24 PPQN)
                            let tick_interval = (sample_rate as f64 * 60.0)
                                / (seq_snap.bpm as f64 * crate::midi::MIDI_CLOCK_PPQN);
                            midi_clock_acc += block as f64;
                            while midi_clock_acc >= tick_interval {
                                midi_clock_acc -= tick_interval;
                                midi_clock_tx.push(0xF8).ok(); // MIDI Clock
                            }
                        }
                    }

                    // Generate audio (TTS duck + mix now handled inside process_block).
                    let t0 = std::time::Instant::now();
                    dsp.process_block(output, channels);
                    let dsp_us = t0.elapsed().as_micros() as f32;
                    // Budget = block_duration in µs
                    let budget_us = block as f32 / sample_rate * 1_000_000.0;
                    if budget_us > 0.0 {
                        dsp_load_tx.push((dsp_us / budget_us).min(2.0)).ok();
                    }
                    if monitor_vol != 1.0 {
                        for s in output.iter_mut() {
                            *s *= monitor_vol;
                        }
                    }

                    // Write first channel to scope + capture + granular-tap;
                    // both channels to stereo meter.  The granular-tap is a
                    // wraparound ring — we push unconditionally, overwriting
                    // oldest content when full, so the UI always has the last
                    // ~15 s of master output to grab on demand.
                    for frame in output.chunks(channels) {
                        scope_tx.push(frame[0]).ok();
                        capture_tx.push(frame[0]).ok();
                        // Drop-oldest behavior: when the ring is full, pop
                        // one to make space.  No std::thread::block needed —
                        // pop is non-blocking on rtrb.
                        if granular_capture_tx.push(frame[0]).is_err() {
                            // Full: push failed, no-op (the UI will have
                            // already drained when user clicked CAPTURE).
                        }
                        if channels >= 2 {
                            stereo_tx.push(frame[0]).ok();
                            stereo_tx.push(frame[1]).ok();
                        }
                    }
                },
                |err| log::error!("Audio stream error: {}", err),
                None,
            )?,
            fmt => {
                return Err(anyhow::anyhow!("Unsupported sample format: {:?}", fmt));
            }
        };

        stream.play()?;
        log::info!("Audio engine started at {}Hz, {} ch", sample_rate, channels);

        Ok(Self {
            params_tx,
            scope_rx,
            capture_rx,
            granular_capture_rx,
            tts_tx,
            midi_clock_rx,
            dsp_load_rx,
            stereo_rx,
            sample_rate: config.sample_rate.0,
            block_size: 0, // determined at runtime in callback
            _stream: stream,
        })
    }
}

// ─── WAV utilities ────────────────────────────────────────────────────────────

/// Load a 16-bit PCM WAV file, return mono f32 samples normalised to ±1 and
/// resampled to 44100 Hz. Returns `None` on any parse or I/O error.
/// Read just the WAV header + data length from `path` without decoding the
/// full sample buffer.  Used by the UI to display size / length / channels
/// info without the cost of a full load.  Returns samples-after-resample
/// at 44.1 kHz (approximate when source rate differs).
pub fn read_wav_meta(path: &str) -> Option<crate::state::AmenMeta> {
    let bytes = std::fs::read(path).ok()?;
    let file_bytes = bytes.len() as u64;
    if bytes.len() < 44 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return None;
    }
    let mut pos = 12usize;
    let mut channels = 1u16;
    let mut src_rate = SAMPLE_RATE_HZ;
    let mut bits = 16u16;
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
            data_len = chunk_len;
            break;
        }
        pos += chunk_len + (chunk_len & 1);
    }
    let frame_bytes = (channels as usize) * (bits as usize / 8).max(1);
    let n_frames = data_len.checked_div(frame_bytes).unwrap_or(0);
    // Samples after internal resample to the engine rate (approx).
    let samples_44k = if src_rate == SAMPLE_RATE_HZ {
        n_frames
    } else {
        ((n_frames as f64) * SAMPLE_RATE as f64 / (src_rate as f64)).round() as usize
    };
    Some(crate::state::AmenMeta {
        samples: samples_44k,
        src_rate,
        channels,
        bits,
        file_bytes,
    })
}

pub fn load_wav_to_44100(path: &str) -> Option<Arc<Vec<f32>>> {
    let bytes = std::fs::read(path).ok()?;
    if bytes.len() < 44 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return None;
    }

    let mut pos = 12usize;
    let mut channels = 1u16;
    let mut src_rate = SAMPLE_RATE_HZ;
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

    let frame_bytes = channels as usize * 2;
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

    // Resample to the engine rate if needed.
    let out = if src_rate == SAMPLE_RATE_HZ {
        mono
    } else {
        let ratio = src_rate as f32 / SAMPLE_RATE;
        let new_len = (mono.len() as f32 / ratio) as usize;
        (0..new_len)
            .map(|i| {
                let src = i as f32 * ratio;
                let idx = src as usize;
                let frac = src - idx as f32;
                let a = mono.get(idx).copied().unwrap_or(0.0);
                let b = mono.get(idx + 1).copied().unwrap_or(0.0);
                a + (b - a) * frac
            })
            .collect()
    };

    log::info!(
        "Loaded WAV: {} ({} Hz, {} ch, {} frames → {} samples at 44100 Hz)",
        path,
        src_rate,
        channels,
        n_frames,
        out.len()
    );
    Some(Arc::new(out))
}
