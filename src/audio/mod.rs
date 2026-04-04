// ─── audio/mod.rs ────────────────────────────────────────────────────────────
// Audio engine: owns the cpal stream, DSP state, and sequencer clock.
// The audio callback is real-time safe: no allocations, no locks.

pub mod dsp;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, SampleFormat, Stream, StreamConfig};
use rtrb::{Consumer, Producer};
use std::sync::Arc;

use crate::sequencer::{ClockState, TriggerEvent, advance_clock};
use crate::state::AppState;

pub use dsp::{AudioParams, DspState};

// ─── Messages sent from UI/HTTP thread to audio thread ───────────────────────

pub enum AudioCommand {
    UpdateParams(Box<AudioParams>),
    Trigger(TriggerEvent),
    /// Live monitor gain (0.0–1.0). Applied after DSP, not saved to state,
    /// never reaches the export path — exports always render at full volume.
    SetMonitorVolume(f32),
}

// ─── Audio Engine ─────────────────────────────────────────────────────────────

pub struct AudioEngine {
    pub params_tx: Producer<AudioCommand>,
    pub scope_rx: Consumer<f32>,
    _stream: Stream, // kept alive
}

impl AudioEngine {
    pub fn new(state: Arc<parking_lot::RwLock<AppState>>) -> anyhow::Result<Self> {
        let host = cpal::default_host();
        let device: Device = host
            .default_output_device()
            .ok_or_else(|| anyhow::anyhow!("No audio output device found"))?;

        let supported = device.default_output_config()?;
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

        // Audio-thread-local DSP state
        let initial_params = {
            let s = state.read();
            AudioParams::from_app_state(&s)
        };
        let mut dsp = DspState::new(sample_rate, initial_params);
        let mut clock = ClockState::default();
        let mut monitor_vol = 1.0_f32;

        let state_clone = Arc::clone(&state);

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
                        }
                    }

                    let block = output.len() / channels;

                    // Advance sequencer — snapshot seq state (no lock held during DSP)
                    let (bpm, running, seq_snap) = {
                        let s = state_clone.read();
                        (s.sequencer.bpm, s.sequencer.running, s.sequencer.clone())
                    };

                    let _ = (bpm, running); // used inside advance_clock via seq_snap
                    let (new_clock, events) =
                        advance_clock(clock.clone(), &seq_snap, block, sample_rate);
                    clock = new_clock;

                    // Propagate current_step back so the UI cursor animates.
                    // Brief write of a single usize — no allocation, releases immediately.
                    {
                        let mut s = state_clone.write();
                        s.sequencer.current_step = clock.current_step;
                    }

                    for event in events {
                        dsp.handle_trigger(&event);
                    }

                    // Generate audio, then apply monitor gain
                    dsp.process_block(output, channels);
                    if monitor_vol != 1.0 {
                        for s in output.iter_mut() {
                            *s *= monitor_vol;
                        }
                    }

                    // Write first channel of each frame to scope ring buffer
                    for frame in output.chunks(channels) {
                        scope_tx.push(frame[0]).ok(); // non-blocking, drop if full
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
            _stream: stream,
        })
    }
}
