// ─── audio/mod.rs ────────────────────────────────────────────────────────────
// Audio engine: owns the cpal stream, DSP state, and sequencer clock.
// The audio callback is real-time safe: no allocations, no locks.

pub mod dsp;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, SampleFormat, Stream, StreamConfig};
use parking_lot::Mutex;
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
    /// TTS processed audio pushed by the LLM thread, mixed into the output.
    pub tts_tx: Arc<Mutex<Producer<f32>>>,
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

        // Ring buffer: TTS processed audio → audio thread mix (≈6s @ 44100Hz)
        let (tts_producer, mut tts_consumer) = rtrb::RingBuffer::<f32>::new(262144);
        let tts_tx = Arc::new(Mutex::new(tts_producer));

        // Audio-thread-local DSP state
        let initial_params = {
            let s = state.read();
            AudioParams::from_app_state(&s)
        };
        let mut dsp = DspState::new(sample_rate, initial_params);
        let mut clock = ClockState::default();
        let mut monitor_vol = 1.0_f32;
        // TTS duck envelope: 1.0 = full synth, 0.3 = ducked under TTS voice
        let mut tts_duck = 1.0_f32;
        let duck_target = 0.35_f32; // synth level when TTS is speaking
        let duck_attack = 1.0 - (-8.0_f32 / sample_rate).exp(); // ~fast
        let duck_release = 1.0 - (-2.0_f32 / sample_rate).exp(); // ~slow

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
                    let (seq_snap, chain_snap, chain_enabled, chain_pos_snap) = {
                        let s = state_clone.read();
                        (
                            s.sequencer.clone(),
                            s.chain.clone(),
                            s.chain_enabled,
                            s.chain_pos,
                        )
                    };

                    let prev_loop_count = clock.loop_count;
                    let (new_clock, events) =
                        advance_clock(clock.clone(), &seq_snap, block, sample_rate);
                    clock = new_clock;

                    // Propagate current_step back; advance chain on each loop boundary.
                    {
                        let mut s = state_clone.write();
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

                    // Generate audio, then apply monitor gain
                    dsp.process_block(output, channels);
                    if monitor_vol != 1.0 {
                        for s in output.iter_mut() {
                            *s *= monitor_vol;
                        }
                    }

                    // Duck synth and mix in TTS audio (lock-free pop per frame)
                    let tts_active = tts_consumer.slots() > 0;
                    for frame in output.chunks_mut(channels) {
                        // Smooth duck gain toward target
                        let target = if tts_active { duck_target } else { 1.0 };
                        let coeff = if tts_duck > target {
                            duck_attack
                        } else {
                            duck_release
                        };
                        tts_duck += (target - tts_duck) * coeff;
                        // Apply duck to synth frame
                        for ch in frame.iter_mut() {
                            *ch *= tts_duck;
                        }
                        // Add TTS sample
                        if let Ok(tts_s) = tts_consumer.pop() {
                            for ch in frame.iter_mut() {
                                *ch += tts_s;
                            }
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
            tts_tx,
            _stream: stream,
        })
    }
}
