// ─── audio/mod.rs ────────────────────────────────────────────────────────────
// Audio engine: owns the cpal stream, DSP state, and sequencer clock.
// The audio callback is real-time safe: no allocations, no locks.

pub mod analysis;
pub mod audio_load;
pub mod dsp;
pub mod gr_levels;
pub mod onset;
pub(crate) mod sf2_generators;
pub mod sf2_loader;
pub mod sfz_loader;
pub mod spectrum;
pub mod voice_meters;

use gr_levels::GrLevels;
use voice_meters::VoiceLevels;

pub use audio_load::load_audio_to_engine;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, SampleFormat, Stream, StreamConfig};
use parking_lot::Mutex;
use rtrb::{Consumer, Producer};
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};

use crate::sequencer::{ClockState, TriggerEvent, advance_clock};
use crate::state::{AppState, FxPlan, compile_fx_plan};

pub use dsp::{AudioParams, DspState};

// ─── Internal sample rate ────────────────────────────────────────────────────
// All DSP, samplers, scope/capture, and analysis run at this fixed rate.
// The cpal stream is opened at this rate when the device supports it;
// otherwise the audio callback linear-interp-resamples between the engine
// and device rates.  External WAVs / TTS output are also resampled to this
// rate on load.
//
// 48 kHz chosen because it's the default on modern Linux (PipeWire/ALSA),
// the standard for video/film work, and increasingly the default on macOS.
// Switching from 44.1 kHz in v0.7.7 removed a latent bug where analysis
// (spectrum, pitch detection, capture buffers) assumed 44.1 kHz while the
// audio callback actually ran at the device rate.

/// Internal sample rate as f32 — the canonical form used by DSP/analysis.
pub const SAMPLE_RATE: f32 = 48_000.0;

/// Same rate as an integer — use for `Vec` lengths, WAV headers, and any
/// context where a `usize` / `u32` is expected.
pub const SAMPLE_RATE_HZ: u32 = 48_000;

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
    /// Load sample data into the wavetable voice.  The DSP splits
    /// `data` into 2048-sample frames at load time.
    LoadWavetable(Arc<Vec<f32>>),
    /// Load sample data into the SampleInstrument voice.  Plays the
    /// recording back at a pitch derived from the played note.
    LoadSampleInstrument(Arc<Vec<f32>>),
    /// Switch the SampleInstrument voice into SFZ multisample mode.
    /// The vec carries one entry per region in the parsed `.sfz`,
    /// each paired with its pre-loaded mono buffer (already resampled
    /// to the engine sample rate).  Triggers then pick the region
    /// covering the played note.
    LoadSampleInstrumentSfz(Vec<crate::audio::dsp::sample_instrument::SfzRegionRuntime>),
    /// Load an impulse-response into the convolution-reverb FX step.
    /// `channels` is 1 for mono, 2 for interleaved stereo; `reversed`
    /// stores the IR back-to-front for the reverse-reverb effect.
    /// The Arc keeps the call allocation-free on the audio thread — the
    /// DSP's FFT partition re-computation happens inside the handler
    /// before the callback sees the new IR.
    LoadImpulseResponse {
        data: Arc<Vec<f32>>,
        channels: u8,
        reversed: bool,
    },
    /// Drop the currently loaded impulse response.  The ConvReverb FX
    /// step falls back to its Phase 1 filter-only wet path.
    ClearImpulseResponse,
    /// Updated FX routing plan derived from the rack cable graph.
    /// Sent whenever the rack topology changes (connect/disconnect/enable).
    SetFxPlan(FxPlan),
    /// Snap the sequencer clock to a specific step boundary,
    /// resetting the sample accumulator.  Used by Ableton Link
    /// bar-phase alignment (`tick_link_sync`) to align our pattern
    /// with the network's bar boundary on enable.  Snapping the
    /// accumulator to 0 means the next block starts a fresh step
    /// rather than carrying over a partial sample count from the
    /// pre-snap position.
    SnapClock {
        step: usize,
    },
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
    /// Live polyphony readout for the SampleInstrument voice — count of
    /// active slots (0..=POLY_VOICES) updated once per audio callback.
    /// `Arc<AtomicU8>` instead of an rtrb byte stream because the UI
    /// only ever cares about the latest value, not the history.
    pub sample_instrument_poly: Arc<AtomicU8>,
    /// Per-voice peak-decay envelope levels for the `VoiceMeterStrip`
    /// viz module.  20 fixed slots indexed by `voice_meters::voice_meter_idx`.
    /// Audio thread updates the envelopes inside `process_block` from the
    /// per-voice bus signals + publishes once per audio callback; UI
    /// thread reads when painting the strip.
    pub voice_meters: Arc<voice_meters::VoiceLevels>,
    /// Momentary gain-reduction snapshot for the `GrHistory` viz —
    /// most-attenuating linear gain ratio across active dynamics FX
    /// (FxCompressor / FxLimiter / FxMultibandComp), updated by the
    /// audio thread's `apply_fx_chain` and published once per
    /// callback.  UI maintains its own scrolling ring buffer.
    pub gr_levels: Arc<gr_levels::GrLevels>,
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
        // Prefer SAMPLE_RATE_HZ if the device supports it so the callback can
        // skip resampling.  Otherwise fall back to the default-config rate
        // and let the callback's linear-interp resampler bridge.
        let target_rate = cpal::SampleRate(SAMPLE_RATE_HZ);
        let resolved_rate = match device.supported_output_configs() {
            Ok(configs) => configs
                .filter(|r| {
                    r.min_sample_rate() <= target_rate && target_rate <= r.max_sample_rate()
                })
                .map(|_| target_rate)
                .next()
                .unwrap_or_else(|| supported.sample_rate()),
            Err(_) => supported.sample_rate(),
        };
        let config = StreamConfig {
            channels: supported.channels(),
            sample_rate: resolved_rate,
            buffer_size: cpal::BufferSize::Default,
        };
        let device_rate = config.sample_rate.0 as f32;
        let channels = config.channels as usize;
        if device_rate != SAMPLE_RATE {
            log::warn!(
                "Device rate {} Hz doesn't match engine rate {} Hz — linear-interp resampling at I/O",
                device_rate,
                SAMPLE_RATE
            );
        }

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

        // Ring buffer: TTS processed audio → audio thread mix (≈6s at engine rate)
        let (tts_producer, tts_consumer) = rtrb::RingBuffer::<f32>::new(262144);
        let tts_tx = TtsSink {
            tx: Arc::new(Mutex::new(tts_producer)),
            // TTS sinks into the engine-rate mix (before device resample).
            target_sr: SAMPLE_RATE_HZ,
        };

        // Ring buffer: audio thread → MIDI clock output thread (1 byte per tick, 24 PPQN)
        let (mut midi_clock_tx, midi_clock_rx) = rtrb::RingBuffer::<u8>::new(512);

        // Ring buffer: audio thread → UI DSP load meter (one f32 per callback)
        let (mut dsp_load_tx, dsp_load_rx) = rtrb::RingBuffer::<f32>::new(256);

        // Atomic readout: SampleInstrument active-voice count.  Audio
        // thread writes once per callback; UI reads when painting the
        // poly meter.  Single byte covers POLY_VOICES = 8 with room to
        // spare and keeps the wire footprint at one Relaxed store + one
        // Relaxed load — cheaper than an rtrb byte stream the UI would
        // have to drain to find the latest value.
        let sample_instrument_poly = Arc::new(AtomicU8::new(0));
        let sample_instrument_poly_audio = Arc::clone(&sample_instrument_poly);

        // Per-voice meter levels — shared atomic array for the
        // VoiceMeterStrip viz module.  Audio thread updates inside
        // process_block + publishes once per callback; UI thread reads
        // when painting.
        let voice_meters = VoiceLevels::new();
        let voice_meters_audio = Arc::clone(&voice_meters);

        // Momentary gain-reduction snapshot for the GrHistory viz —
        // updated inside apply_fx_chain whenever a dynamics FX runs.
        let gr_levels = GrLevels::new();
        let gr_levels_audio = Arc::clone(&gr_levels);

        // Audio-thread-local DSP state — DSP always runs at SAMPLE_RATE, not
        // the device rate.  The callback resamples to the device rate at the
        // I/O boundary.
        let (initial_params, initial_fx_plan) = {
            let s = state.read();
            (AudioParams::from_app_state(&s), compile_fx_plan(&s.rack))
        };
        let mut dsp = DspState::new(
            SAMPLE_RATE,
            initial_params,
            initial_fx_plan,
            tts_consumer,
            voice_meters_audio,
            gr_levels_audio,
        );
        let mut clock = ClockState::default();
        let mut monitor_vol = 1.0_f32;
        // MIDI clock out: accumulator tracks fractional samples until next 0xF8 tick
        let mut midi_clock_acc = 0.0_f64;
        let mut midi_clock_running = false; // tracks sequencer running state for Start/Stop

        // Pre-allocated engine-rate buffer for DSP output.  Sized generously
        // (8192 frames × 2 channels = 16384 f32s ≈ 64 KB) so the callback
        // never allocates.  At 48 kHz, 8192 frames = 170 ms — far more than
        // any realistic cpal block size.
        const MAX_ENGINE_FRAMES: usize = 8192;
        let mut engine_buf: Vec<f32> = vec![0.0; MAX_ENGINE_FRAMES * channels];
        // Linear-interp resampler state: previous engine frame (carried
        // across callbacks so the first output frame of each callback can
        // interpolate between the last engine frame of the previous
        // callback and the first frame of the current one), plus the
        // fractional start position of the next output frame in the
        // current-callback virtual stream `[resample_prev, engine_buf[0],
        // engine_buf[1], ...]`.
        let mut resample_prev = [0.0_f32; 2];
        let mut resample_phase = 0.0_f32;
        let rate_ratio = SAMPLE_RATE / device_rate; // engine frames per device frame

        let state_clone = Arc::clone(&state);

        log::info!(
            "Opening audio stream ({} Hz device, {} Hz engine, {} ch)… (if stuck, kill stale impulse-instruct processes)",
            device_rate,
            SAMPLE_RATE,
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
                            AudioCommand::LoadWavetable(data) => dsp.load_wavetable(data),
                            AudioCommand::LoadSampleInstrument(data) => {
                                dsp.load_sample_instrument(data)
                            }
                            AudioCommand::LoadSampleInstrumentSfz(regions) => {
                                dsp.load_sample_instrument_sfz(regions)
                            }
                            AudioCommand::LoadImpulseResponse {
                                data,
                                channels: ch,
                                reversed,
                            } => dsp.load_impulse_response(data, ch, reversed),
                            AudioCommand::ClearImpulseResponse => dsp.clear_impulse_response(),
                            AudioCommand::SetFxPlan(plan) => dsp.set_fx_plan(plan),
                            AudioCommand::SnapClock { step } => {
                                clock.current_step = step;
                                clock.sample_accumulator = 0.0;
                            }
                        }
                    }

                    let device_block_frames = output.len() / channels;

                    // Engine-rate block size — how many frames the DSP must
                    // produce to cover this callback after resampling.  When
                    // device rate matches the engine rate this is just the
                    // device block; otherwise it accounts for the fractional
                    // resample phase carried from the previous callback.
                    let engine_block_frames = if (device_rate - SAMPLE_RATE).abs() < 0.01 {
                        device_block_frames
                    } else {
                        let next_phase_abs =
                            resample_phase + device_block_frames as f32 * rate_ratio;
                        (next_phase_abs.floor() as usize).clamp(1, MAX_ENGINE_FRAMES)
                    };

                    // Advance sequencer — snapshot seq state (no lock held during DSP)
                    let (
                        seq_snap,
                        chain_snap,
                        chain_overrides_snap,
                        chain_enabled,
                        chain_pos_snap,
                        chain_repeat_snap,
                        chain_loop_snap,
                    ) = {
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
                        (
                            seq,
                            s.chain.clone(),
                            s.chain_overrides.clone(),
                            s.chain_enabled,
                            s.chain_pos,
                            s.chain_repeat_count,
                            s.chain_loop,
                        )
                    };

                    let prev_loop_count = clock.loop_count;
                    let (new_clock, mut events) =
                        advance_clock(clock.clone(), &seq_snap, engine_block_frames, SAMPLE_RATE);
                    clock = new_clock;

                    // Propagate current_step back; advance chain on each loop boundary.
                    {
                        let mut s = state_clone.write();
                        // Increment monotonic step counter by the *actual*
                        // delta — `advance_clock` can cross multiple step
                        // boundaries in one block when block_size approaches
                        // or exceeds samples_per_step (high BPM / large
                        // engine block), so always adding 1 caused
                        // `global_step_count` to drift behind the audio
                        // clock.  The event-stream visualiser keys past
                        // notes by `global_step_count`, so the drift
                        // showed up as a per-pattern jitter on the playhead.
                        let prev_step = s.sequencer.current_step;
                        let curr_step = clock.current_step;
                        // `step_count_delta` is the polymeter-aware
                        // saturating diff — see its docs for the
                        // wrap-fallback removal history.  Pulled out as
                        // a pure helper so the delta math is testable
                        // without standing up an audio engine.
                        s.global_step_count =
                            s.global_step_count
                                .saturating_add(crate::sequencer::step_count_delta(
                                    prev_step, curr_step,
                                ));
                        s.sequencer.current_step = clock.current_step;
                        if clock.loop_count != prev_loop_count {
                            // Pattern morph in flight — progress it
                            // before the chain-advance logic that may
                            // create a *new* morph below.  Each tick
                            // replaces a growing fraction of step
                            // indices with the target pattern's
                            // same-index step (bit-reversal dispersal
                            // order); the final tick swaps wholesale
                            // and clears the morph.
                            if let Some(mut morph) = s.chain_morph.take() {
                                let new_seq =
                                    crate::state::morph_tick(s.sequencer.clone(), &mut morph);
                                s.sequencer = new_seq;
                                s.sequencer.current_step = clock.current_step;
                                if !morph.is_complete() {
                                    s.chain_morph = Some(morph);
                                }
                            }
                        }
                        if clock.loop_count != prev_loop_count
                            && chain_enabled
                            && !chain_snap.is_empty()
                        {
                            // Decision lives in the pure
                            // `classify_loop_boundary` (state/chain_advance.rs);
                            // the audio thread is just a dispatcher.
                            use crate::state::LoopBoundaryAction;
                            let cur_pos = chain_pos_snap % chain_snap.len();
                            let preview_next = (cur_pos + 1) % chain_snap.len();
                            let preview_slot = chain_snap[preview_next];
                            let preview_loaded = s
                                .pattern_bank
                                .get(preview_slot)
                                .cloned()
                                .unwrap_or_default();
                            let action = crate::state::classify_loop_boundary(
                                &chain_snap,
                                &chain_overrides_snap,
                                chain_pos_snap,
                                chain_repeat_snap,
                                chain_loop_snap,
                                Some(&preview_loaded),
                                s.sequencer.bpm,
                                s.sequencer.swing,
                            );
                            match action {
                                LoopBoundaryAction::None => {}
                                LoopBoundaryAction::BumpRepeatCount(n) => {
                                    s.chain_repeat_count = n;
                                }
                                LoopBoundaryAction::StopAtEnd => {
                                    // One-shot song just finished its
                                    // last slot's last repeat.  Stop
                                    // transport; leave chain_pos on the
                                    // final slot so the UI shows "we
                                    // stopped at the end".  `advance_clock`
                                    // already emitted step-0 note-ons for
                                    // the would-loop pattern; drop them so
                                    // the piece ends cleanly.  Gate-offs
                                    // survive so anything still sounding
                                    // gets a release.
                                    s.sequencer.running = false;
                                    s.chain_repeat_count = 0;
                                    events.retain(crate::sequencer::TriggerEvent::is_gate_off);
                                }
                                LoopBoundaryAction::AdvanceTo {
                                    next_pos,
                                    next_slot,
                                    morph_bars,
                                    eff_bpm,
                                    eff_swing,
                                    effective_style,
                                } => {
                                    let running = s.sequencer.running;
                                    // Auto-save current edits to the
                                    // active bank slot before switching.
                                    let current_edit = s.pattern_edit;
                                    let snap = s.sequencer.clone();
                                    if let Some(slot) = s.pattern_bank.get_mut(current_edit) {
                                        *slot = snap;
                                    }
                                    s.chain_pos = next_pos;
                                    s.chain_repeat_count = 0;
                                    let loaded =
                                        s.pattern_bank.get(next_slot).cloned().unwrap_or_default();
                                    let target = crate::state::build_advance_target(
                                        loaded,
                                        &seq_snap,
                                        chain_loop_snap,
                                        eff_bpm,
                                        eff_swing,
                                        running,
                                    );
                                    if morph_bars > 0 {
                                        // Morph path: keep the prior
                                        // sequencer playing and stash the
                                        // target for `morph_tick` to
                                        // progressively swap in.  BPM /
                                        // swing apply immediately so
                                        // transport changes don't lag.
                                        s.sequencer.bpm = target.bpm;
                                        s.sequencer.swing = target.swing;
                                        s.chain_morph =
                                            Some(crate::state::ChainMorph::new(target, morph_bars));
                                    } else {
                                        s.sequencer = target;
                                        s.chain_morph = None;
                                    }
                                    s.sequencer.current_step = clock.current_step;
                                    s.pattern_edit = next_slot;
                                    if effective_style.is_some() {
                                        let owned = std::mem::take(&mut *s);
                                        *s = crate::state::apply_pattern_style_on_advance(
                                            owned,
                                            effective_style.as_deref(),
                                        );
                                    }
                                }
                            }
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
                            let tick_interval = crate::midi::midi_clock_tick_interval_samples(
                                seq_snap.bpm,
                                SAMPLE_RATE,
                            );
                            midi_clock_acc += engine_block_frames as f64;
                            while midi_clock_acc >= tick_interval {
                                midi_clock_acc -= tick_interval;
                                midi_clock_tx.push(0xF8).ok(); // MIDI Clock
                            }
                        }
                    }

                    // Generate audio at engine rate into engine_buf (TTS duck
                    // + mix handled inside process_block).
                    let engine_slice = &mut engine_buf[..engine_block_frames * channels];
                    for s in engine_slice.iter_mut() {
                        *s = 0.0;
                    }
                    let t0 = std::time::Instant::now();
                    dsp.process_block(engine_slice, channels);
                    let dsp_us = t0.elapsed().as_micros() as f32;
                    // Budget = engine block duration in µs (device block is
                    // proportional so the load metric is invariant).
                    let budget_us = engine_block_frames as f32 / SAMPLE_RATE * 1_000_000.0;
                    if budget_us > 0.0 {
                        dsp_load_tx.push((dsp_us / budget_us).min(2.0)).ok();
                    }
                    sample_instrument_poly_audio
                        .store(dsp.sample_instrument_active(), Ordering::Relaxed);
                    if monitor_vol != 1.0 {
                        for s in engine_slice.iter_mut() {
                            *s *= monitor_vol;
                        }
                    }

                    // Write first channel to scope + capture + granular-tap
                    // *from the engine buffer* so analysis (spectrum, pitch
                    // detection, capture window sizing) operates at the
                    // advertised engine rate regardless of the device rate.
                    // The granular-tap is a wraparound ring — we push
                    // unconditionally, dropping the push (not the oldest
                    // sample) when full; the UI drains lazily.
                    for frame in engine_slice.chunks(channels) {
                        scope_tx.push(frame[0]).ok();
                        capture_tx.push(frame[0]).ok();
                        if granular_capture_tx.push(frame[0]).is_err() {
                            // Full: no-op.
                        }
                        if channels >= 2 {
                            stereo_tx.push(frame[0]).ok();
                            stereo_tx.push(frame[1]).ok();
                        }
                    }

                    // Resample engine_slice → output.  Fast path when the
                    // device rate already matches the engine rate (common on
                    // modern Linux/PipeWire).
                    if (device_rate - SAMPLE_RATE).abs() < 0.01 {
                        output.copy_from_slice(engine_slice);
                    } else {
                        // Linear interp with carry.  Virtual source stream
                        // is `[resample_prev, engine_slice[0], engine_slice[1], ...]`
                        // where resample_prev is the final engine frame of
                        // the previous callback.  resample_phase is the
                        // start position (in engine-frame units) of the
                        // first output frame of this callback.
                        for of in 0..device_block_frames {
                            let src = resample_phase + of as f32 * rate_ratio;
                            let i_f = src.floor();
                            let frac = src - i_f;
                            let i_int = i_f as i32;
                            for ch in 0..channels {
                                let a = if i_int <= 0 {
                                    resample_prev[ch]
                                } else {
                                    let idx = ((i_int - 1) as usize).min(engine_block_frames - 1);
                                    engine_slice[idx * channels + ch]
                                };
                                let b_int = i_int + 1;
                                let b = if b_int <= 0 {
                                    resample_prev[ch]
                                } else {
                                    let idx = ((b_int - 1) as usize).min(engine_block_frames - 1);
                                    engine_slice[idx * channels + ch]
                                };
                                output[of * channels + ch] = a + (b - a) * frac;
                            }
                        }
                        // Carry the last engine frame as the new `prev` so
                        // the next callback's first output frame can
                        // interpolate continuously across the boundary.
                        let last = (engine_block_frames - 1) * channels;
                        resample_prev[0] = engine_slice[last];
                        if channels >= 2 {
                            resample_prev[1] = engine_slice[last + 1];
                        }
                        // Carry the fractional start position.
                        let next_phase_abs =
                            resample_phase + device_block_frames as f32 * rate_ratio;
                        resample_phase =
                            (next_phase_abs - engine_block_frames as f32).clamp(0.0, 0.999_999);
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
        log::info!(
            "Audio engine started (device {}Hz, engine {}Hz, {} ch)",
            device_rate,
            SAMPLE_RATE,
            channels
        );

        Ok(Self {
            params_tx,
            scope_rx,
            capture_rx,
            granular_capture_rx,
            tts_tx,
            midi_clock_rx,
            dsp_load_rx,
            stereo_rx,
            sample_instrument_poly,
            voice_meters,
            gr_levels,
            sample_rate: config.sample_rate.0,
            block_size: 0, // determined at runtime in callback
            _stream: stream,
        })
    }
}

// ─── WAV utilities ────────────────────────────────────────────────────────────

/// Load a 16-bit PCM WAV file, return mono f32 samples normalised to ±1 and
/// resampled to the engine rate (`SAMPLE_RATE_HZ`).  Returns `None` on any
/// parse or I/O error.
///
/// Read just the WAV header + data length from `path` without decoding the
/// full sample buffer.  Used by the UI to display size / length / channels
/// info without the cost of a full load.  Returns samples-after-resample
/// at the engine rate (approximate when source rate differs).
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
            sum += audio_load::i16_pcm_to_f32(raw);
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
        "Loaded WAV: {} ({} Hz, {} ch, {} frames → {} samples at {} Hz)",
        path,
        src_rate,
        channels,
        n_frames,
        out.len(),
        SAMPLE_RATE_HZ
    );
    Some(Arc::new(out))
}

/// Load a WAV file preserving its channel layout and resampling to the
/// engine rate.  Returns (samples, channels) where samples is mono or
/// interleaved stereo.  Mirrors `load_wav_to_44100` but keeps the
/// stereo content that the convolution reverb uses to drive its
/// `mid ± side` output from a stereo impulse response.
///
/// 16-bit PCM only — matches `load_wav_to_44100`'s constraint.  Returns
/// `None` for non-RIFF, non-WAVE, non-16-bit, or zero-length files.
pub fn load_wav_stereo_to_engine(path: &str) -> Option<(Arc<Vec<f32>>, u8)> {
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

    if data_start == 0 || bits != 16 || channels == 0 {
        return None;
    }
    // Clamp to mono or stereo — higher channel counts get downmixed to
    // stereo (L = ch0, R = ch1) since convolution reverb only wires
    // two-channel IRs, and saving arbitrary multichannel material is
    // niche enough to handle separately.
    let out_channels = channels.min(2) as usize;

    let frame_bytes = channels as usize * 2;
    let n_frames = data_len / frame_bytes;
    let mut interleaved = Vec::with_capacity(n_frames * out_channels);

    for i in 0..n_frames {
        let base = data_start + i * frame_bytes;
        for out_ch in 0..out_channels {
            let off = base + out_ch * 2;
            if off + 2 > bytes.len() {
                break;
            }
            let raw = i16::from_le_bytes(bytes[off..off + 2].try_into().ok()?);
            interleaved.push(audio_load::i16_pcm_to_f32(raw));
        }
    }

    // Resample to engine rate (linear interp) while preserving channel
    // layout.  Interleaved data means stride `out_channels` between
    // consecutive frames; the loop walks per-channel.
    let out: Vec<f32> = if src_rate == SAMPLE_RATE_HZ {
        interleaved
    } else {
        let ratio = src_rate as f32 / SAMPLE_RATE;
        let new_frames = (n_frames as f32 / ratio) as usize;
        let mut resampled = Vec::with_capacity(new_frames * out_channels);
        for i in 0..new_frames {
            let src = i as f32 * ratio;
            let idx = src as usize;
            let frac = src - idx as f32;
            for ch in 0..out_channels {
                let a = interleaved
                    .get(idx * out_channels + ch)
                    .copied()
                    .unwrap_or(0.0);
                let b = interleaved
                    .get((idx + 1) * out_channels + ch)
                    .copied()
                    .unwrap_or(0.0);
                resampled.push(a + (b - a) * frac);
            }
        }
        resampled
    };

    log::info!(
        "Loaded IR WAV: {} ({} Hz, {} ch, {} frames → {} ch-interleaved samples at {} Hz)",
        path,
        src_rate,
        out_channels,
        n_frames,
        out.len(),
        SAMPLE_RATE_HZ
    );
    Some((Arc::new(out), out_channels as u8))
}
