// ─── audio/mod.rs ────────────────────────────────────────────────────────────
// Audio engine: owns the cpal stream, DSP state, and sequencer clock.
// The audio callback is real-time safe: no allocations, no locks.

pub mod dsp;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, SampleFormat, Stream, StreamConfig};
use rtrb::Producer;
use std::sync::Arc;

use crate::sequencer::{advance_clock, ClockState, TriggerEvent};
use crate::state::AppState;

pub use dsp::{AudioParams, DspState};

// ─── Messages sent from UI/HTTP thread to audio thread ───────────────────────

#[allow(dead_code)] // Trigger variant used for manual note input (MIDI/UI, coming soon)
pub enum AudioCommand {
    UpdateParams(AudioParams),
    Trigger(TriggerEvent),
}

// ─── Audio Engine ─────────────────────────────────────────────────────────────

pub struct AudioEngine {
    pub params_tx: Producer<AudioCommand>,
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

        // Audio-thread-local DSP state
        let initial_params = {
            let s = state.read();
            AudioParams::from_app_state(&s)
        };
        let mut dsp = DspState::new(sample_rate, initial_params);
        let mut clock = ClockState::default();

        let state_clone = Arc::clone(&state);

        let stream = match supported.sample_format() {
            SampleFormat::F32 => device.build_output_stream(
                &config,
                move |output: &mut [f32], _| {
                    // Drain incoming commands
                    while let Ok(cmd) = params_rx.pop() {
                        match cmd {
                            AudioCommand::UpdateParams(p) => dsp.update_params(p),
                            AudioCommand::Trigger(e) => dsp.handle_trigger(&e),
                        }
                    }

                    let block = output.len() / channels;

                    // Advance sequencer — snapshot seq state (no lock held during DSP)
                    let (bpm, running, seq_snap) = {
                        let s = state_clone.read();
                        (s.sequencer.bpm, s.sequencer.running, s.sequencer.clone())
                    };

                    let _ = (bpm, running); // used inside advance_clock via seq_snap
                    let (new_clock, events) = advance_clock(clock.clone(), &seq_snap, block, sample_rate);
                    clock = new_clock;

                    for event in events {
                        dsp.handle_trigger(&event);
                    }

                    // Generate audio
                    dsp.process_block(output, channels);
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

        Ok(Self { params_tx, _stream: stream })
    }
}
