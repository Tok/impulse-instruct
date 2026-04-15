// ─── audio/dsp/mod.rs ── Pure DSP synthesis, no allocations in process_block()

mod bass303;
mod dsp_util;
mod fx;
mod mod_apply;
mod params;
mod rev_tap;
mod samplers;
mod voices;
use bass303::Bass303;
pub use dsp_util::midi_to_hz;
use dsp_util::*;
use fx::*;
use mod_apply::apply_mod_target;
pub use params::AudioParams;
use samplers::*;
use voices::*;

use crate::sequencer::TriggerEvent;
use crate::state::{DrumVoice, FxPlan, FxStep, ModuleKind};

use rev_tap::{REV_BUF_LEN, rev_tap_len_for_quant, step_rev_tap};

// ─── Full DSP state ───────────────────────────────────────────────────────────

pub struct DspState {
    // Voices
    bass: [Bass303; crate::state::MAX_BASS_VOICES],
    kick808: Kick,
    snare808: Snare,
    hihat_closed808: HiHat,
    hihat_open808: HiHat,
    tom_hi808: Kick,
    tom_mid808: Kick,
    tom_lo808: Kick,
    kick909: Kick,
    snare909: Snare,
    hihat_closed909: HiHat,
    hihat_open909: HiHat,
    clap909: Clap,
    rim909: Snare,
    // FX
    reverb: Reverb,
    delay: DelayLine,
    chorus: Chorus,
    phaser: Phaser,
    bitcrush_held: f32,
    bitcrush_counter: u32,
    // FX state
    compressor: Compressor,
    tape_sat: TapeSat,
    autotune: Autotune,
    ring_mod_phase: f32,
    eq: EqBands,
    noise_voice: NoiseVoice,
    granular: GranularVoice,
    hoover: HooverVoice,
    an1x: An1xVoice,
    amen: AmenVoice,
    // LFO state
    lfo_phases: [f32; 4],
    lfo_sh_held: [f32; 4],
    lfo_noise: NoiseGen,
    free_eg_phase: f32, // 0..1 through the 8-step EG period
    free_eg_done: bool, // true after one-shot completes
    prev_running: bool,
    // Per-voice velocity (set on trigger, applied to voice output)
    drum_velocity: [f32; 14],
    // Current params
    params: AudioParams,
    sample_rate: f32,
    // Compiled FX routing plan (updated via AudioCommand::SetFxPlan)
    fx_plan: FxPlan,
    // Gated reverb envelope: 1.0 = open, 0.0 = closed. Tracks transient
    // detection; decays to 0 at a rate set by p.reverb_gate_time.
    reverb_gate_env: f32,
    sidechain_env: f32, // 0–1 sidechain gain reduction envelope
    // TTS ring buffer consumer (lock-free; popped one sample per frame).
    tts_consumer: rtrb::Consumer<f32>,
    // Duck envelope: smoothly attenuates synth when TTS is active.
    tts_duck: f32,
    /// Per-step pan latched from the most recent BassTrigger event.
    /// 0.0 = no override (voice's static pan_303 wins); non-zero = use this
    /// instead.  Persists until the next trigger updates it.
    bass_step_pan: f32,
    /// Reverb / Delay reverse-tap buffers — separate per-FX so each can
    /// independently switch FWD / REV / MIRROR without interfering.  1 s
    /// circular buffer of dry input feeding each FX; the read tap walks
    /// backwards every sample, looping every REV_BUF_LEN samples (a
    /// continuously-rewinding tape).  Allocated on the heap once at
    /// DspState::new (no per-block allocations).
    rev_buf_reverb: Vec<f32>,
    rev_head_reverb: usize,
    rev_play_reverb: usize,
    rev_buf_delay: Vec<f32>,
    rev_head_delay: usize,
    rev_play_delay: usize,
    duck_attack: f32,
    duck_release: f32,
}

impl DspState {
    pub fn new(
        sample_rate: f32,
        params: AudioParams,
        fx_plan: FxPlan,
        tts_consumer: rtrb::Consumer<f32>,
    ) -> Self {
        let mut p = params;
        p.sample_rate = sample_rate;
        Self {
            bass: std::array::from_fn(|_| Bass303::default()),
            kick808: Kick::new(0x1234),
            snare808: Snare::new(0x5678),
            hihat_closed808: HiHat::new(0x9abc),
            hihat_open808: HiHat::new(0xdef0),
            tom_hi808: Kick::new(0x1111),
            tom_mid808: Kick::new(0x2222),
            tom_lo808: Kick::new(0x3333),
            kick909: Kick::new(0xaaaa),
            snare909: Snare::new(0xbbbb),
            hihat_closed909: HiHat::new(0xcccc),
            hihat_open909: HiHat::new(0xdddd),
            clap909: Clap::new(0xeeee),
            rim909: Snare::new(0xffff),
            reverb: Reverb::new(),
            delay: DelayLine::new(),
            chorus: Chorus::new(),
            phaser: Phaser::new(),
            compressor: Compressor::new(),
            tape_sat: TapeSat::new(),
            autotune: Autotune::new(),
            ring_mod_phase: 0.0,
            eq: EqBands::new(sample_rate),
            bitcrush_held: 0.0,
            bitcrush_counter: 0,
            noise_voice: NoiseVoice::new(0x4015_EB3D),
            granular: GranularVoice::new(0xBEEF_CAFE),
            hoover: HooverVoice::new(),
            an1x: An1xVoice::new(),
            amen: AmenVoice::new(),
            lfo_phases: [0.0; 4],
            lfo_sh_held: [0.0; 4],
            lfo_noise: NoiseGen::new(0xCAFE_BABE),
            free_eg_phase: 0.0,
            free_eg_done: false,
            drum_velocity: [1.0; 14],
            prev_running: false,
            params: p,
            sample_rate,
            fx_plan,
            reverb_gate_env: 1.0,
            sidechain_env: 0.0,
            tts_consumer,
            tts_duck: 1.0,
            bass_step_pan: 0.0,
            // 1 s buffers @ 44.1 kHz — enough for musically useful reverse
            // character without massive memory.  Allocated once on the heap.
            rev_buf_reverb: vec![0.0; REV_BUF_LEN],
            rev_head_reverb: 0,
            rev_play_reverb: 0,
            rev_buf_delay: vec![0.0; REV_BUF_LEN],
            rev_head_delay: 0,
            rev_play_delay: 0,
            duck_attack: 1.0 - (-8.0_f32 / sample_rate).exp(),
            duck_release: 1.0 - (-2.0_f32 / sample_rate).exp(),
        }
    }

    pub fn set_fx_plan(&mut self, plan: FxPlan) {
        self.fx_plan = plan;
    }

    /// Apply one FX step to `sig` and return the result.
    /// Must not hold any borrow on `self.fx_plan` when called.
    fn apply_fx_step(
        &mut self,
        step: FxStep,
        sig: f32,
        p: &AudioParams,
        delay_samples: usize,
        sr: f32,
        gate_env: f32,
    ) -> f32 {
        match step {
            FxStep::Waveshaper => {
                if p.waveshaper_mix > 0.001 {
                    let drive = p.waveshaper_drive * 8.0 + 1.0;
                    let shaped = tanh(sig * drive) / tanh(drive);
                    sig * (1.0 - p.waveshaper_mix) + shaped * p.waveshaper_mix
                } else {
                    sig
                }
            }
            FxStep::Reverb => {
                if p.reverb_mix > 0.001 || p.reverb_freeze {
                    let rev_in = step_rev_tap(
                        &mut self.rev_buf_reverb,
                        &mut self.rev_head_reverb,
                        &mut self.rev_play_reverb,
                        sig,
                        rev_tap_len_for_quant(p.reverb_rev_quant, sr, p.sequencer_bpm),
                    );
                    let r = &mut self.reverb;
                    let (sz, dp, fz) = (p.reverb_size, p.reverb_damp, p.reverb_freeze);
                    let wet = match p.reverb_dir {
                        1 => r.process(rev_in, sz, dp, fz),
                        2 => r.process(sig, sz, dp, fz) + r.process(rev_in, sz, dp, false) * 0.7,
                        _ => r.process(sig, sz, dp, fz),
                    };
                    if fz {
                        wet
                    } else {
                        sig * (1.0 - p.reverb_mix) + wet * p.reverb_mix * gate_env
                    }
                } else {
                    sig
                }
            }
            FxStep::Delay => {
                let rev_in = step_rev_tap(
                    &mut self.rev_buf_delay,
                    &mut self.rev_head_delay,
                    &mut self.rev_play_delay,
                    sig,
                    rev_tap_len_for_quant(p.delay_rev_quant, sr, p.sequencer_bpm),
                );
                let d = &mut self.delay;
                let (ds, fb, wf, sat) = (
                    delay_samples,
                    p.delay_feedback,
                    p.delay_wow_flutter,
                    p.delay_saturation,
                );
                let wet = match p.delay_dir {
                    1 => d.process_tape(rev_in, ds, fb, wf, sat, sr),
                    2 => {
                        d.process_tape(sig, ds, fb, wf, sat, sr)
                            + d.process_tape(rev_in, ds, fb, wf, sat, sr) * 0.7
                    }
                    _ => d.process_tape(sig, ds, fb, wf, sat, sr),
                };
                sig * (1.0 - p.delay_mix) + wet * p.delay_mix
            }
            FxStep::Bitcrush => {
                if p.bitcrush_mix > 0.01 {
                    let hold_frames = (1.0 + p.bitcrush_rate * 15.0) as u32;
                    if self.bitcrush_counter == 0 {
                        let bits = (1.0 + p.bitcrush_bits * 15.0).round().max(1.0);
                        let scale = (1u32 << (bits as u32 - 1)) as f32;
                        self.bitcrush_held = (sig * scale).round() / scale;
                        self.bitcrush_counter = hold_frames;
                    } else {
                        self.bitcrush_counter -= 1;
                    }
                    sig * (1.0 - p.bitcrush_mix) + self.bitcrush_held * p.bitcrush_mix
                } else {
                    sig
                }
            }
            FxStep::Chorus => {
                self.chorus
                    .process(sig, p.chorus_rate, p.chorus_depth, p.chorus_mix, sr)
            }
            FxStep::Phaser => {
                self.phaser
                    .process(sig, p.phaser_rate, p.phaser_depth, p.phaser_mix, sr)
            }
            FxStep::RingMod => {
                if p.ring_mod_mix > 0.001 {
                    let freq_hz = 50.0 + p.ring_mod_freq * 450.0;
                    self.ring_mod_phase += freq_hz / sr;
                    if self.ring_mod_phase >= 1.0 {
                        self.ring_mod_phase -= 1.0;
                    }
                    let carrier = (self.ring_mod_phase * std::f32::consts::TAU).sin();
                    let ring = sig * carrier;
                    sig * (1.0 - p.ring_mod_mix) + ring * p.ring_mod_mix
                } else {
                    sig
                }
            }
            FxStep::Eq => self
                .eq
                .process(sig, p.eq_low_gain, p.eq_mid_gain, p.eq_hi_gain),
            FxStep::Compressor => self.compressor.process(
                sig,
                p.compressor_threshold,
                p.compressor_ratio,
                p.compressor_mix,
                p.compressor_multiband,
                sr,
            ),
            FxStep::TapeSat => {
                self.tape_sat
                    .process(sig, p.tape_drive, p.tape_mix, p.tape_flutter, sr)
            }
            FxStep::Drive => {
                if p.distortion_drive > 0.01 {
                    let drive = p.distortion_drive * 6.0 + 1.0;
                    let dist = tanh(sig * drive);
                    sig * (1.0 - p.distortion_mix) + dist * p.distortion_mix
                } else {
                    sig
                }
            }
            FxStep::Autotune => self
                .autotune
                .process(sig, p.autotune_amount, p.autotune_mix),
        }
    }

    fn apply_fx_chain(
        &mut self,
        mut sig: f32,
        chain: &[FxStep],
        p: &AudioParams,
        delay_samples: usize,
        sr: f32,
        gate_env: f32,
    ) -> f32 {
        for &step in chain {
            sig = self.apply_fx_step(step, sig, p, delay_samples, sr, gate_env);
        }
        sig
    }

    pub fn update_params(&mut self, p: AudioParams) {
        let mut p = p;
        p.sample_rate = self.sample_rate;
        self.params = p;
    }

    /// Load new sample data into the amen voice. Called from the audio command handler
    /// (outside process_block) when the user picks a new WAV file.
    pub fn load_amen(&mut self, data: std::sync::Arc<Vec<f32>>) {
        self.amen.load(data);
    }

    pub fn load_granular(&mut self, data: std::sync::Arc<Vec<f32>>) {
        self.granular.load(data);
    }

    pub fn handle_trigger(&mut self, event: &TriggerEvent) {
        use crate::sequencer::TriggerEvent::*;
        match event {
            DrumTrigger {
                voice,
                velocity,
                slice,
            } => {
                let in_rack = match voice {
                    DrumVoice::Kick808
                    | DrumVoice::Snare808
                    | DrumVoice::HihatClosed808
                    | DrumVoice::HihatOpen808
                    | DrumVoice::TomHi808
                    | DrumVoice::TomMid808
                    | DrumVoice::TomLo808 => self.params.rack_drums808,
                    DrumVoice::Kick909
                    | DrumVoice::Snare909
                    | DrumVoice::HihatClosed909
                    | DrumVoice::HihatOpen909
                    | DrumVoice::Clap909
                    | DrumVoice::Rim909 => self.params.rack_drums909,
                    DrumVoice::Amen => self.params.rack_amen,
                };
                if !in_rack {
                    return;
                }
                self.drum_velocity[voices::drum_voice_idx(voice)] = velocity.clamp(0.0, 1.0);
                match voice {
                    DrumVoice::Kick808 => self.kick808.trigger(),
                    DrumVoice::Snare808 => self.snare808.trigger(),
                    DrumVoice::HihatClosed808 => self.hihat_closed808.trigger(),
                    DrumVoice::HihatOpen808 => self.hihat_open808.trigger(),
                    DrumVoice::TomHi808 => self.tom_hi808.trigger(),
                    DrumVoice::TomMid808 => self.tom_mid808.trigger(),
                    DrumVoice::TomLo808 => self.tom_lo808.trigger(),
                    DrumVoice::Kick909 => self.kick909.trigger(),
                    DrumVoice::Snare909 => self.snare909.trigger(),
                    DrumVoice::HihatClosed909 => self.hihat_closed909.trigger(),
                    DrumVoice::HihatOpen909 => self.hihat_open909.trigger(),
                    DrumVoice::Clap909 => self.clap909.trigger(),
                    DrumVoice::Rim909 => self.rim909.trigger(),
                    DrumVoice::Amen => self.amen.trigger(
                        *slice,
                        self.params.amen_slice_count,
                        self.params.amen_start_offset,
                        self.params.amen_end_offset,
                        self.params.amen_reverse,
                        self.params.amen_gate,
                        self.params.amen_stutter,
                        &self.params.amen_slice_positions,
                        &self.params.amen_slice_pitches,
                        &self.params.amen_slice_volumes,
                        self.params.amen_bpm_stretch,
                        self.params.amen_source_bpm,
                        self.params.sequencer_bpm,
                    ),
                }
            }
            BassTrigger {
                voice_idx,
                note,
                accent,
                slide,
                gate_samples: _,
                pan,
            } => {
                if self.params.rack_bass && *voice_idx < crate::state::MAX_BASS_VOICES {
                    self.bass[*voice_idx].trigger(*note, *accent, *slide, self.params.tuning);
                    self.bass_step_pan = pan.clamp(-1.0, 1.0);
                }
            }
            BassGateOff { voice_idx } => {
                if self.params.rack_bass && *voice_idx < crate::state::MAX_BASS_VOICES {
                    self.bass[*voice_idx].gate_off();
                }
            }
            HooverTrigger { note } => {
                if self.params.rack_hoover {
                    self.hoover.trigger(*note, self.params.tuning);
                }
            }
            HooverGateOff => {
                if self.params.rack_hoover {
                    self.hoover.gate_off();
                }
            }
            An1xTrigger { note } => {
                if self.params.rack_an1x {
                    self.an1x.trigger(*note, self.sample_rate, &self.params);
                }
            }
            An1xGateOff => {
                if self.params.rack_an1x {
                    self.an1x.gate_off();
                }
            }
        }
    }

    /// Process one buffer — pure computation, no I/O, no allocation.
    pub fn process_block(&mut self, output: &mut [f32], channels: usize) {
        let p_base = self.params;
        let sr = self.sample_rate;
        let block_size = output.len() / channels.max(1);

        // Phase reset: when sequencer transitions from stopped to running, reset all LFO phases
        if p_base.sequencer_running && !self.prev_running {
            self.lfo_phases = [0.0; 4];
            self.free_eg_phase = 0.0;
            self.free_eg_done = false;
        }
        self.prev_running = p_base.sequencer_running;

        // Advance LFO phases once per block and apply modulation to a working params copy
        let mut p = p_base;
        for i in 0..4 {
            let lp = p_base.lfo[i];
            // Run this slot if it's directly enabled OR a cable-routed mod
            // sources from it — otherwise nothing depends on its phase.
            let has_route = p_base
                .mod_routes
                .iter()
                .take(p_base.mod_route_count as usize)
                .any(|r| r.lfo_slot as usize == i);
            if !lp.enabled && !has_route {
                continue;
            }
            let rate_hz = 0.01 + lp.rate * 19.99;
            let phase_inc = rate_hz * (block_size as f32 / sr);
            let old_phase = self.lfo_phases[i];
            self.lfo_phases[i] = (old_phase + phase_inc) % 1.0;
            let wrapped = self.lfo_phases[i] < old_phase; // true when phase wrapped

            let phase = (self.lfo_phases[i] + lp.phase_offset) % 1.0;
            let lfo_val = match lp.waveform {
                0 => (phase * std::f32::consts::TAU).sin(),
                1 => 1.0 - 4.0 * (phase - 0.5).abs(),
                2 => phase * 2.0 - 1.0,
                3 => 1.0 - phase * 2.0,
                4 => {
                    if phase < 0.5 {
                        1.0
                    } else {
                        -1.0
                    }
                }
                _ => {
                    // S&H: update held value on phase wrap
                    if wrapped {
                        self.lfo_sh_held[i] = self.lfo_noise.next();
                    }
                    self.lfo_sh_held[i]
                }
            };

            // Slot's built-in target only fires when the slot is enabled.
            // Cable-routed mods always fire — that's the user's intent when
            // they patched a cable from this slot.
            if lp.enabled {
                let mod_val = lfo_val * lp.depth;
                apply_mod_target(&mut p, lp.target, mod_val);
            }
            for r in p_base
                .mod_routes
                .iter()
                .take(p_base.mod_route_count as usize)
            {
                if r.lfo_slot as usize == i {
                    apply_mod_target(&mut p, r.target_u8, lfo_val * r.depth);
                }
            }
        }

        // ── Free EG ───────────────────────────────────────────────────────────
        if p_base.free_eg_enabled && p_base.free_eg_target != 0 && !self.free_eg_done {
            // Advance phase by one block
            let phase_inc = (block_size as f32 / sr) / p_base.free_eg_period.max(0.001);
            self.free_eg_phase += phase_inc;
            if self.free_eg_phase >= 1.0 {
                if p_base.free_eg_loop {
                    self.free_eg_phase -= 1.0;
                } else {
                    self.free_eg_phase = 1.0;
                    self.free_eg_done = true;
                }
            }
            // Interpolate between the 8 steps
            let pos = self.free_eg_phase * 7.0; // 0..7
            let idx = pos.floor() as usize;
            let frac = pos.fract();
            let v0 = p_base.free_eg_values[idx.min(7)];
            let v1 = p_base.free_eg_values[(idx + 1).min(7)];
            let level = v0 + (v1 - v0) * frac; // 0..1
            let bipolar_depth = (p_base.free_eg_depth - 0.5) * 2.0; // -1..+1
            let mod_val = level * bipolar_depth;
            apply_mod_target(&mut p, p_base.free_eg_target, mod_val);
        }

        if p.master_pitch_st.abs() > 0.001 {
            p.lfo_pitch_mod_st += p.master_pitch_st;
        }

        // Sync voice 0's per-voice params with LFO/free-EG modulated values
        p.bass_voice_params[0].cutoff = p.cutoff;
        p.bass_voice_params[0].resonance = p.resonance;

        let delay_samples =
            (p.delay_time * sr * 2.0).clamp(1.0, MAX_DELAY_SAMPLES as f32 - 2.0) as usize;

        // Snapshot FX chains into stack arrays (releases borrow on self.fx_plan).
        const MAX_CHAIN: usize = 16;
        let mut global_chain = [FxStep::Waveshaper; MAX_CHAIN];
        let mut global_len = 0usize;
        for (i, &s) in self.fx_plan.steps.iter().enumerate().take(MAX_CHAIN) {
            global_chain[i] = s;
            global_len += 1;
        }

        // Per-voice route snapshots — copy from HashMap (all Copy, no allocation).
        let snap_route = |kind: ModuleKind| -> ([FxStep; MAX_CHAIN], usize) {
            let mut arr = [FxStep::Waveshaper; MAX_CHAIN];
            let mut len = 0usize;
            if let Some(steps) = self.fx_plan.voice_routes.get(&kind) {
                for (i, &s) in steps.iter().enumerate().take(MAX_CHAIN) {
                    arr[i] = s;
                    len += 1;
                }
            }
            (arr, len)
        };
        let (chain_bass, bass_len) = snap_route(ModuleKind::AcidBass);
        let (chain_808, d808_len) = snap_route(ModuleKind::DrumKit808);
        let (chain_909, d909_len) = snap_route(ModuleKind::DrumKit909);
        let (chain_hoover, hov_len) = snap_route(ModuleKind::HooverLead);
        let (chain_an1x, an1x_len) = snap_route(ModuleKind::An1xVoice);
        let (chain_amen, amen_len) = snap_route(ModuleKind::AmenSampler);
        let (chain_noise, noise_len) = snap_route(ModuleKind::NoiseVoice);
        let (chain_granular, gran_len) = snap_route(ModuleKind::GranularTexture);
        let (chain_tts, tts_len) = snap_route(ModuleKind::NeuTts);
        let have_voice_routes = !self.fx_plan.voice_routes.is_empty();
        // Release the immutable borrow on self (via fx_plan) before the mutable frame loop.
        let _ = snap_route;

        for frame in output.chunks_mut(channels) {
            // Mix all voices to mono
            let bass_out = self
                .bass
                .iter_mut()
                .enumerate()
                .map(|(i, v)| v.process(&p, &p.bass_voice_params[i]))
                .sum::<f32>();

            let dv = self.drum_velocity; // copy, not borrow (avoids lifetime conflict with &mut self)
            let k808 = self.kick808.process(
                p.kick808_pitch,
                p.kick808_decay,
                p.kick808_punch,
                p.kick808_volume,
                p.kick808_pitch_env_depth,
                p.kick808_pitch_env_time,
                p.kick808_clip,
                sr,
            );
            let s808 = self.snare808.process(
                p.snare808_tone,
                p.snare808_snappy,
                p.snare808_decay,
                p.snare808_volume,
                sr,
            );
            let hh808c =
                self.hihat_closed808
                    .process(p.hihat_closed808_decay, 0.8, p.hihat808_volume, sr);
            let hh808o =
                self.hihat_open808
                    .process(p.hihat_open808_decay, 0.75, p.hihat808_volume, sr);
            let th808 = self
                .tom_hi808
                .process(0.7, 0.4, 0.6, 0.7, 0.5, 0.2, 0.0, sr);
            let tm808 = self
                .tom_mid808
                .process(0.5, 0.45, 0.6, 0.7, 0.5, 0.2, 0.0, sr);
            let tl808 = self
                .tom_lo808
                .process(0.3, 0.5, 0.6, 0.7, 0.5, 0.2, 0.0, sr);

            let k909 = self.kick909.process(
                p.kick909_pitch,
                p.kick909_decay,
                p.kick909_punch,
                p.kick909_volume,
                p.kick909_pitch_env_depth,
                p.kick909_pitch_env_time,
                p.kick909_clip,
                sr,
            );
            let s909 = self.snare909.process(
                p.snare909_tone,
                p.snare909_snappy,
                p.snare909_decay,
                p.snare909_volume,
                sr,
            );
            let hh909c =
                self.hihat_closed909
                    .process(p.hihat_closed909_decay, 0.85, p.hihat909_volume, sr);
            let hh909o =
                self.hihat_open909
                    .process(p.hihat_open909_decay, 0.8, p.hihat909_volume, sr);
            let clap = self.clap909.process(p.clap909_decay, p.clap909_volume, sr);
            let rim = self.rim909.process(0.7, 0.3, 0.15, 0.75, sr);
            let noise_out = self.noise_voice.process(sr, &p);
            let hoover_out = if p.hoover_enabled {
                self.hoover.process(sr, &p)
            } else {
                0.0
            };
            // Cross-modulation: bass → AN1X pitch (one-sample delay via bass_out)
            if p.xmod_bass_to_an1x_pitch > 0.001 {
                p.lfo_pitch_mod_st += bass_out * p.xmod_bass_to_an1x_pitch * 24.0;
            }
            // Cross-modulation: noise → bass filter cutoff (for next frame)
            if p.xmod_noise_to_filter > 0.001 {
                p.cutoff = (p.cutoff + noise_out * p.xmod_noise_to_filter * 0.5).clamp(0.0, 1.0);
            }
            let an1x_out = if p.an1x_enabled {
                self.an1x.process(sr, &p)
            } else {
                0.0
            };
            let amen_out = self.amen.process(p.amen_pitch, p.amen_volume, p.amen_loop);
            let (granular_out, granular_side) = if p.granular_enabled {
                let (gl, gr) = self.granular.process(
                    p.granular_volume,
                    p.granular_density,
                    p.granular_grain_size,
                    p.granular_position,
                    p.granular_position_jitter,
                    p.granular_pitch_scatter,
                    sr,
                );
                ((gl + gr) * 0.5, (gl - gr) * 0.5)
            } else {
                (0.0, 0.0)
            };

            // Per-voice bus sums (scaled to prevent clipping)
            let bus_bass = bass_out;
            let bus_808 = k808 * dv[0]
                + s808 * dv[1]
                + hh808c * dv[2]
                + hh808o * dv[3]
                + th808 * dv[4]
                + tm808 * dv[5]
                + tl808 * dv[6];
            let bus_909 = k909 * dv[7]
                + s909 * dv[8]
                + hh909c * dv[9]
                + hh909o * dv[10]
                + clap * dv[11]
                + rim * dv[12];
            let bus_hoover = hoover_out;
            let bus_an1x = an1x_out;
            let bus_amen = amen_out * dv[13];
            let bus_noise = noise_out;
            let bus_granular = granular_out;

            // Sidechain compression: kick ducks bass/pad/hoover/granular
            let (bus_bass, bus_an1x, bus_hoover, bus_granular) = if p.sidechain_amount > 0.001 {
                let kick_level = (k808 * dv[0]).abs() + (k909 * dv[7]).abs();
                let att_ms = 0.1 + p.sidechain_attack * 49.9; // 0.1–50 ms
                let rel_ms = 10.0 + p.sidechain_release * 490.0; // 10–500 ms
                let att_coeff = (-1.0 / (att_ms * 0.001 * sr)).exp();
                let rel_coeff = (-1.0 / (rel_ms * 0.001 * sr)).exp();
                if kick_level > self.sidechain_env {
                    self.sidechain_env = kick_level + (self.sidechain_env - kick_level) * att_coeff;
                } else {
                    self.sidechain_env *= rel_coeff;
                }
                let duck = 1.0 - (self.sidechain_env * p.sidechain_amount * 4.0).min(1.0);
                (
                    bus_bass * duck,
                    bus_an1x * duck,
                    bus_hoover * duck,
                    bus_granular * duck,
                )
            } else {
                (bus_bass, bus_an1x, bus_hoover, bus_granular)
            };

            // Gated reverb: detect transient from pre-FX dry signal.
            // When gate_time > 0, re-open gate on transients; close exponentially.
            let detection_sum = (bus_bass
                + bus_808
                + bus_909
                + bus_hoover
                + bus_an1x
                + bus_amen
                + bus_noise
                + bus_granular)
                * 0.60;
            if p.reverb_gate_time > 0.001 {
                if detection_sum.abs() > 0.08 {
                    self.reverb_gate_env = 1.0;
                } else {
                    let decay = (-1.0 / (p.reverb_gate_time * sr)).exp();
                    self.reverb_gate_env *= decay;
                }
            } else {
                self.reverb_gate_env = 1.0;
            }
            let gate_env = self.reverb_gate_env;

            // Route voices through FX chains and sum to output.
            // Fast path: no voice routes → apply global chain to full dry mix (unchanged behaviour).
            // Per-voice path: each bus through its own chain, then sum, then global chain.
            let synth_out = if !have_voice_routes {
                let dry = detection_sum; // already scaled 0.60 above
                self.apply_fx_chain(
                    dry,
                    &global_chain[..global_len],
                    &p,
                    delay_samples,
                    sr,
                    gate_env,
                )
            } else {
                let routed_bass = if bass_len > 0 {
                    self.apply_fx_chain(
                        bus_bass,
                        &chain_bass[..bass_len],
                        &p,
                        delay_samples,
                        sr,
                        gate_env,
                    )
                } else {
                    bus_bass
                };
                let routed_808 = if d808_len > 0 {
                    self.apply_fx_chain(
                        bus_808,
                        &chain_808[..d808_len],
                        &p,
                        delay_samples,
                        sr,
                        gate_env,
                    )
                } else {
                    bus_808
                };
                let routed_909 = if d909_len > 0 {
                    self.apply_fx_chain(
                        bus_909,
                        &chain_909[..d909_len],
                        &p,
                        delay_samples,
                        sr,
                        gate_env,
                    )
                } else {
                    bus_909
                };
                let routed_hoover = if hov_len > 0 {
                    self.apply_fx_chain(
                        bus_hoover,
                        &chain_hoover[..hov_len],
                        &p,
                        delay_samples,
                        sr,
                        gate_env,
                    )
                } else {
                    bus_hoover
                };
                let routed_an1x = if an1x_len > 0 {
                    self.apply_fx_chain(
                        bus_an1x,
                        &chain_an1x[..an1x_len],
                        &p,
                        delay_samples,
                        sr,
                        gate_env,
                    )
                } else {
                    bus_an1x
                };
                let routed_amen = if amen_len > 0 {
                    self.apply_fx_chain(
                        bus_amen,
                        &chain_amen[..amen_len],
                        &p,
                        delay_samples,
                        sr,
                        gate_env,
                    )
                } else {
                    bus_amen
                };
                let routed_noise = if noise_len > 0 {
                    self.apply_fx_chain(
                        bus_noise,
                        &chain_noise[..noise_len],
                        &p,
                        delay_samples,
                        sr,
                        gate_env,
                    )
                } else {
                    bus_noise
                };
                let routed_granular = if gran_len > 0 {
                    self.apply_fx_chain(
                        bus_granular,
                        &chain_granular[..gran_len],
                        &p,
                        delay_samples,
                        sr,
                        gate_env,
                    )
                } else {
                    bus_granular
                };
                let mixed = (routed_bass
                    + routed_808
                    + routed_909
                    + routed_hoover
                    + routed_an1x
                    + routed_amen
                    + routed_noise
                    + routed_granular)
                    * 0.60;
                // Global chain after per-voice mixing
                self.apply_fx_chain(
                    mixed,
                    &global_chain[..global_len],
                    &p,
                    delay_samples,
                    sr,
                    gate_env,
                )
            };

            // TTS bus: pop one sample, apply TTS voice chain, duck synth and mix.
            let tts_raw = self.tts_consumer.pop().unwrap_or(0.0);
            let tts_sig = if tts_raw != 0.0 && tts_len > 0 {
                self.apply_fx_chain(
                    tts_raw,
                    &chain_tts[..tts_len],
                    &p,
                    delay_samples,
                    sr,
                    gate_env,
                )
            } else {
                tts_raw
            };
            // Smooth duck envelope: attenuate synth when TTS active
            let tts_active = tts_raw != 0.0 || self.tts_consumer.slots() > 0;
            let duck_target = if tts_active { 0.35_f32 } else { 1.0 };
            let coeff = if self.tts_duck > duck_target {
                self.duck_attack
            } else {
                self.duck_release
            };
            self.tts_duck += (duck_target - self.tts_duck) * coeff;

            let out = ((synth_out * self.tts_duck + tts_sig) * p.master_volume).clamp(-1.0, 1.0);

            // Per-voice pan → side signal (computed before FX borrows self).
            // Bass pan: per-step override (latched from BassTrigger.pan)
            // wins when non-zero, otherwise the voice's static pan_303.
            let bass_pan = if self.bass_step_pan.abs() > 0.0001 {
                self.bass_step_pan
            } else {
                p.pan_303
            };
            let pan_side = bus_bass * bass_pan * 0.5
                + k808 * dv[0] * p.pan_kick808 * 0.5
                + s808 * dv[1] * p.pan_snare808 * 0.5
                + (hh808c * dv[2] + hh808o * dv[3]) * p.pan_hihat808 * 0.5
                + k909 * dv[7] * p.pan_kick909 * 0.5
                + s909 * dv[8] * p.pan_snare909 * 0.5
                + (hh909c * dv[9] + hh909o * dv[10]) * p.pan_hihat909 * 0.5
                + clap * dv[11] * p.pan_clap909 * 0.5
                + hoover_out * p.pan_hoover * 0.5
                + an1x_out * p.pan_an1x * 0.5
                + noise_out * p.pan_noise * 0.5;
            let has_stereo = (p.stereo_width - 0.5).abs() > 0.01
                || granular_side.abs() > 0.001
                || pan_side.abs() > 0.0001;
            if channels >= 2 && has_stereo {
                let mid = out;
                let chorus_side = self.chorus.read_tap(0.4) * 0.3;
                let w = p.stereo_width * 2.0;
                let gran_w = if p.granular_enabled {
                    p.granular_spray
                } else {
                    0.0
                };
                let side = chorus_side * w + granular_side * gran_w + pan_side;
                let left = (mid + side).clamp(-1.0, 1.0);
                let right = (mid - side).clamp(-1.0, 1.0);
                frame[0] = left;
                frame[1] = right;
                for s in frame.iter_mut().skip(2) {
                    *s = out;
                }
            } else {
                for s in frame.iter_mut() {
                    *s = out;
                }
            }
        }
    }
}
