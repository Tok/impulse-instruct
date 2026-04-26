// ─── audio/dsp/process_block.rs ──────────────────────────────────────────────
// `DspState::process_block` — the per-buffer audio callback body.
// Extracted from `dsp/mod.rs` to keep that file under the 1000-line
// cap (the function is ~610 lines on its own).  Same `impl
// DspState` block, just split across two sibling files.
//
// Real-time invariants still apply: no allocations, no locks, no
// I/O.  All scratch storage is stack-allocated or pre-sized in
// `DspState`.

use super::fx::MAX_DELAY_SAMPLES;
use super::fx_math::{
    free_eg_value_at, gated_reverb_envelope_step, lfo_value_at, sidechain_duck,
    sidechain_envelope_step,
};
use super::fx_sidechain::{MAX_SIDECHAIN, SidechainSnap};
use super::mod_apply::apply_mod_target;
use super::ms_master::MsMasterParams;
use super::{DspState, FX_WIDEN_HAAS_MAX_SAMPLES, MAX_CHAIN, MAX_SENDS, VoiceSendsSnap};
use crate::state::{FxStep, ModuleKind};

impl DspState {
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
            // S&H re-latches on each phase wrap; lfo_value_at itself is
            // pure and just looks up the held value, so the latch lives here.
            if lp.waveform == crate::state::LfoWaveform::SampleAndHold && wrapped {
                self.lfo_sh_held[i] = self.lfo_noise.next();
            }
            let lfo_val = lfo_value_at(phase, lp.waveform, self.lfo_sh_held[i]);

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

        // ── CV sequencer ──────────────────────────────────────────────────────
        // Step-based modulation: each enabled slot reads the
        // sequencer's `current_step % 16` and applies the
        // bipolar-centered step value (× depth) to its target
        // opcode.  Walks in lock-step with the audio pattern.
        let cv_step = (p_base.sequencer_current_step as usize) % crate::state::CV_SEQ_STEPS;
        for slot in p_base.cv_seq.iter() {
            if !slot.enabled || slot.target == 0 {
                continue;
            }
            // Bipolar swing — 0.5 step value = 0 mod (no effect).
            let bipolar = (slot.step_values[cv_step] - 0.5) * 2.0;
            let mod_val = bipolar * slot.depth.clamp(0.0, 1.0);
            apply_mod_target(&mut p, slot.target, mod_val);
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
            let mod_val = free_eg_value_at(
                self.free_eg_phase,
                &p_base.free_eg_values,
                p_base.free_eg_depth,
            );
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
        let mut global_chain = [FxStep::Waveshaper; MAX_CHAIN];
        let mut global_len = 0usize;
        for (i, &s) in self.fx_plan.steps.iter().enumerate().take(MAX_CHAIN) {
            global_chain[i] = s;
            global_len += 1;
        }

        // Per-voice send snapshot — each voice can drive up to `MAX_SENDS`
        // parallel FX sub-chains, each with its own wet gain.  All-stack
        // so the frame loop iterates without touching HashMaps.
        let snap_sends = |kind: ModuleKind| -> VoiceSendsSnap {
            let mut out = VoiceSendsSnap {
                chains: [[FxStep::Waveshaper; MAX_CHAIN]; MAX_SENDS],
                chain_lens: [0usize; MAX_SENDS],
                gains: [1.0f32; MAX_SENDS],
                count: 0,
            };
            if let Some(sends) = self.fx_plan.voice_routes.get(&kind) {
                for (si, send) in sends.iter().enumerate().take(MAX_SENDS) {
                    for (ci, &step) in send.chain.iter().enumerate().take(MAX_CHAIN) {
                        out.chains[si][ci] = step;
                    }
                    out.chain_lens[si] = send.chain.len().min(MAX_CHAIN);
                    out.gains[si] = send.gain;
                    out.count += 1;
                }
            }
            out
        };
        let sends_bass = snap_sends(ModuleKind::AcidBass);
        let sends_808 = snap_sends(ModuleKind::DrumKit808);
        let sends_909 = snap_sends(ModuleKind::DrumKit909);
        let sends_hoover = snap_sends(ModuleKind::HooverLead);
        let sends_pluck = snap_sends(ModuleKind::PluckString);
        let sends_wavetable = snap_sends(ModuleKind::WavetableVoice);
        let sends_sample = snap_sends(ModuleKind::SampleInstrument);
        let sends_an1x = snap_sends(ModuleKind::An1xVoice);
        let sends_amen = snap_sends(ModuleKind::AmenSampler);
        let sends_noise = snap_sends(ModuleKind::NoiseVoice);
        let sends_theremin = snap_sends(ModuleKind::Theremin);
        let sends_pendulum = snap_sends(ModuleKind::Pendulum);
        let sends_fm_ops = snap_sends(ModuleKind::FmOpsVoice);
        let sends_additive = snap_sends(ModuleKind::AdditiveVoice);
        let sends_modal = snap_sends(ModuleKind::ModalVoice);
        let sends_chiptune = snap_sends(ModuleKind::ChiptuneVoice);
        let sends_vocal = snap_sends(ModuleKind::VocalVoice);
        let sends_granular = snap_sends(ModuleKind::GranularTexture);
        let sends_tts = snap_sends(ModuleKind::NeuTts);
        let have_voice_routes = !self.fx_plan.voice_routes.is_empty();

        // Snapshot sidechain routes into a stack-friendly array.  The
        // targets / sources are static for the block; only `voice_signals`
        // gets refreshed each sample once the voice buses have been
        // computed.  `apply_fx_chain` then walks this snap when it hits
        // a sidechain-capable FX step.
        let mut sidechain_snap = SidechainSnap::empty();
        for (target, source) in self.fx_plan.sidechain_routes.iter() {
            if sidechain_snap.count >= MAX_SIDECHAIN {
                break;
            }
            let i = sidechain_snap.count;
            sidechain_snap.targets[i] = *target;
            sidechain_snap.sources[i] = *source;
            sidechain_snap.count += 1;
        }

        // Snapshot feedback routes into a stack array — the audio thread
        // consults this every apply_fx_chain call without re-borrowing
        // self.fx_plan inside the frame loop.
        const MAX_FEEDBACK: usize = 16;
        let mut feedback_arr = [crate::state::FeedbackRoute {
            source: FxStep::Waveshaper,
            target: FxStep::Waveshaper,
            gain: 0.0,
        }; MAX_FEEDBACK];
        let mut feedback_len = 0usize;
        for (i, fr) in self
            .fx_plan
            .feedback_routes
            .iter()
            .enumerate()
            .take(MAX_FEEDBACK)
        {
            feedback_arr[i] = *fr;
            feedback_len += 1;
        }

        // Release the immutable borrow on self (via fx_plan) before the mutable frame loop.
        let _ = snap_sends;

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
            let gk = self.gabber_kick.process(&p, sr);
            let noise_out = self.noise_voice.process(sr, &p);
            let theremin_out = self.theremin.process(sr, &p);
            let pendulum_out = self.pendulum.process(sr, &p);
            let fm_ops_out = self.fm_ops.process(sr, &p);
            let additive_out = self.additive.process(sr, &p);
            let modal_out = self.modal.process(sr, &p);
            let chiptune_out = self.chiptune.process(sr, &p);
            let vocal_out = self.vocal.process(sr, &p);
            let hoover_out = if p.hoover_enabled {
                self.hoover.process(sr, &p)
            } else {
                0.0
            };
            let pluck_out = if p.pluck_enabled && p.rack_pluck {
                self.pluck.process(sr, &p)
            } else {
                0.0
            };
            let wavetable_out = if p.wavetable_enabled && p.rack_wavetable {
                self.wavetable.process(sr, &p)
            } else {
                0.0
            };
            let sample_out = if p.sample_enabled && p.rack_sample {
                self.sample_instrument.process(sr, &p)
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
                + tl808 * dv[6]
                + gk * dv[14];
            let bus_909 = k909 * dv[7]
                + s909 * dv[8]
                + hh909c * dv[9]
                + hh909o * dv[10]
                + clap * dv[11]
                + rim * dv[12];
            let bus_hoover = hoover_out;
            let bus_pluck = pluck_out;
            let bus_wavetable = wavetable_out;
            let bus_sample = sample_out;
            let bus_an1x = an1x_out;
            let bus_amen = amen_out * dv[13];
            let bus_noise = noise_out;
            let bus_theremin = theremin_out;
            let bus_pendulum = pendulum_out;
            let bus_fm_ops = fm_ops_out;
            let bus_additive = additive_out;
            let bus_modal = modal_out;
            let bus_chiptune = chiptune_out;
            let bus_vocal = vocal_out;
            let bus_granular = granular_out;

            // Sidechain compression: kick ducks bass/pad/hoover/granular
            let (bus_bass, bus_an1x, bus_hoover, bus_granular) = if p.sidechain_amount > 0.001 {
                let kick_level = (k808 * dv[0]).abs() + (k909 * dv[7]).abs();
                let att_ms = 0.1 + p.sidechain_attack * 49.9; // 0.1–50 ms
                let rel_ms = 10.0 + p.sidechain_release * 490.0; // 10–500 ms
                self.sidechain_env =
                    sidechain_envelope_step(self.sidechain_env, kick_level, att_ms, rel_ms, sr);
                let duck = sidechain_duck(self.sidechain_env, p.sidechain_amount);
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
                + bus_pluck
                + bus_wavetable
                + bus_sample
                + bus_an1x
                + bus_amen
                + bus_noise
                + bus_theremin
                + bus_pendulum
                + bus_fm_ops
                + bus_additive
                + bus_modal
                + bus_chiptune
                + bus_vocal
                + bus_granular)
                * 0.60;
            self.reverb_gate_env = gated_reverb_envelope_step(
                self.reverb_gate_env,
                detection_sum.abs(),
                p.reverb_gate_time,
                sr,
            );
            let gate_env = self.reverb_gate_env;

            sidechain_snap.refresh_voice_signals(
                bus_bass,
                bus_808,
                bus_909,
                bus_hoover,
                bus_pluck,
                bus_wavetable,
                bus_sample,
                bus_an1x,
                bus_amen,
                bus_noise,
                bus_granular,
            );

            // Route voices through FX chains and sum to output.
            // Fast path: no voice routes → apply global chain to full dry mix (unchanged behaviour).
            // Per-voice path: each bus through its own chain, then sum, then global chain.
            let fb = &feedback_arr[..feedback_len];
            let scs = &sidechain_snap;
            let synth_out = if !have_voice_routes {
                let dry = detection_sum; // already scaled 0.60 above
                self.apply_fx_chain(
                    dry,
                    &global_chain[..global_len],
                    fb,
                    scs,
                    &p,
                    delay_samples,
                    sr,
                    gate_env,
                )
            } else {
                // Each voice routes through every one of its parallel
                // sends; the helper sums the FX outputs and returns the
                // bus signal unchanged when the voice has no sends
                // (fallback to dry so un-wired voices stay audible).
                // The macro collapses 11 identical routed_X branches.
                macro_rules! route_or_dry {
                    ($bus:expr, $sends:ident) => {
                        if $sends.count > 0 {
                            self.route_voice_sends(
                                $bus,
                                &$sends,
                                fb,
                                scs,
                                &p,
                                delay_samples,
                                sr,
                                gate_env,
                            )
                        } else {
                            $bus
                        }
                    };
                }
                let routed_bass = route_or_dry!(bus_bass, sends_bass);
                let routed_808 = route_or_dry!(bus_808, sends_808);
                let routed_909 = route_or_dry!(bus_909, sends_909);
                let routed_hoover = route_or_dry!(bus_hoover, sends_hoover);
                let routed_an1x = route_or_dry!(bus_an1x, sends_an1x);
                let routed_amen = route_or_dry!(bus_amen, sends_amen);
                let routed_noise = route_or_dry!(bus_noise, sends_noise);
                let routed_theremin = route_or_dry!(bus_theremin, sends_theremin);
                let routed_pendulum = route_or_dry!(bus_pendulum, sends_pendulum);
                let routed_fm_ops = route_or_dry!(bus_fm_ops, sends_fm_ops);
                let routed_additive = route_or_dry!(bus_additive, sends_additive);
                let routed_modal = route_or_dry!(bus_modal, sends_modal);
                let routed_chiptune = route_or_dry!(bus_chiptune, sends_chiptune);
                let routed_vocal = route_or_dry!(bus_vocal, sends_vocal);
                let routed_granular = route_or_dry!(bus_granular, sends_granular);
                let routed_pluck = route_or_dry!(bus_pluck, sends_pluck);
                let routed_wavetable = route_or_dry!(bus_wavetable, sends_wavetable);
                let routed_sample = route_or_dry!(bus_sample, sends_sample);
                let mixed = (routed_bass
                    + routed_808
                    + routed_909
                    + routed_hoover
                    + routed_pluck
                    + routed_wavetable
                    + routed_sample
                    + routed_an1x
                    + routed_amen
                    + routed_noise
                    + routed_theremin
                    + routed_pendulum
                    + routed_fm_ops
                    + routed_additive
                    + routed_modal
                    + routed_chiptune
                    + routed_vocal
                    + routed_granular)
                    * 0.60;
                // Global chain after per-voice mixing
                self.apply_fx_chain(
                    mixed,
                    &global_chain[..global_len],
                    fb,
                    scs,
                    &p,
                    delay_samples,
                    sr,
                    gate_env,
                )
            };

            // TTS bus: pop one sample, apply TTS voice chain, duck synth and mix.
            // `tts_voice_volume` scales the bus post-FX — modulatable via
            // the `NeuTtsVolume` LFO target.  Scaling AFTER FX keeps duck
            // activity tied to the raw sample stream regardless of gain.
            let tts_raw = self.tts_consumer.pop().unwrap_or(0.0);
            let tts_fx = if tts_raw != 0.0 && sends_tts.count > 0 {
                self.route_voice_sends(
                    tts_raw,
                    &sends_tts,
                    fb,
                    scs,
                    &p,
                    delay_samples,
                    sr,
                    gate_env,
                )
            } else {
                tts_raw
            };
            let tts_sig = tts_fx * p.tts_voice_volume;
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
            // Per-voice bass pan-LFO contribution — each Bass303 sets
            // `pan_side` inside its `process` call when its LFO target
            // is `BassLfoTarget::Pan`.  Summed here so two voices
            // running anti-phase (lfo_phase 0 vs 0.5) produce
            // mono-safe stereo motion without any host automation.
            let bass_lfo_pan_side: f32 = self.bass.iter().map(|v| v.pan_side).sum();
            let pan_side = bus_bass * bass_pan * 0.5
                + bass_lfo_pan_side
                + k808 * dv[0] * p.pan_kick808 * 0.5
                + s808 * dv[1] * p.pan_snare808 * 0.5
                + (hh808c * dv[2] + hh808o * dv[3]) * p.pan_hihat808 * 0.5
                + k909 * dv[7] * p.pan_kick909 * 0.5
                + s909 * dv[8] * p.pan_snare909 * 0.5
                + (hh909c * dv[9] + hh909o * dv[10]) * p.pan_hihat909 * 0.5
                + clap * dv[11] * p.pan_clap909 * 0.5
                + gk * dv[14] * p.gabber_pan * 0.5
                + hoover_out * p.pan_hoover * 0.5
                + pluck_out * p.pluck_pan * 0.5
                + wavetable_out * p.wavetable_pan * 0.5
                + sample_out * p.sample_pan * 0.5
                + an1x_out * p.pan_an1x * 0.5
                + noise_out * p.pan_noise * 0.5
                + theremin_out * p.theremin_pan * 0.5
                + pendulum_out * p.pendulum_pan * 0.5
                + fm_ops_out * p.fm_ops_pan * 0.5
                + additive_out * p.additive_pan * 0.5
                + modal_out * p.modal_pan * 0.5
                + chiptune_out * p.chiptune_pan * 0.5
                + vocal_out * p.vocal_pan * 0.5;
            // Decay the Pan FxStep side-contribution when the step
            // hasn't run this sample, so switching it off stops the
            // auto-pan cleanly instead of latching the last side value.
            self.fx_pan_side *= 0.995;
            // ConvReverb side contribution decays the same way so the
            // wet tail drops to mono when the step stops running.
            let conv_reverb_side = self.conv_reverb.side;
            self.conv_reverb.side *= 0.995;
            // FxWiden / M/S ParamEq latches force the stereo path even
            // when nothing else is producing side, so the master can
            // apply Haas delay or M/S filtering to a mono mid.
            let widen_active = self.fx_widen_active;
            let ms_eq_active = self.param_eq_ms_active;
            self.fx_widen_active = false;
            self.param_eq_ms_active = false;
            let has_stereo = (p.stereo_width - 0.5).abs() > 0.01
                || granular_side.abs() > 0.001
                || pan_side.abs() > 0.0001
                || self.fx_pan_side.abs() > 0.0001
                || conv_reverb_side.abs() > 0.0001
                || widen_active
                || ms_eq_active;
            if channels >= 2 && has_stereo {
                let mid_raw = out;
                let chorus_side = self.chorus.read_tap(0.4) * 0.3;
                let w = p.stereo_width * 2.0;
                let gran_w = if p.granular_enabled {
                    p.granular_spray
                } else {
                    0.0
                };
                let side_raw = chorus_side * w
                    + granular_side * gran_w
                    + pan_side
                    + self.fx_pan_side
                    + conv_reverb_side;
                // Mid/side master processing: per-side gain + tilt EQ
                // + arctan saturation.  Runs after the raw (mid, side)
                // computation and before L/R recombination so every
                // voice + FX contribution is shaped by the same
                // mastering chain.
                let ms_params = MsMasterParams {
                    mid_gain: p.ms_mid_gain,
                    mid_tilt: p.ms_mid_tilt,
                    mid_sat: p.ms_mid_sat,
                    side_gain: p.ms_side_gain,
                    side_tilt: p.ms_side_tilt,
                    side_sat: p.ms_side_sat,
                };
                let (mut mid, mut side) = self.ms_master.process(mid_raw, side_raw, sr, ms_params);

                // M/S ParamEq: when the chain ParamEq step ran in M/S
                // mode this sample, route mid + side through their
                // dedicated cascades.  Same band list, but the user
                // hears different curves on the centre and the sides
                // — useful for surgical "tame the sides" mastering
                // moves without affecting the lead vocal centre.
                if ms_eq_active {
                    mid = self.param_eq_mid.process(mid, &p.param_eq_bands, sr);
                    side = self.param_eq_side.process(side, &p.param_eq_bands, sr);
                }

                // FxWiden: Haas delay on the L channel + side
                // scaling.  Push current mid into the ring; tap a
                // delayed sample for the L computation.  When the
                // chain step didn't run this sample, fall back to
                // current mid (no delay).
                let (mid_for_left, side_widened) = if widen_active {
                    let haas_amt = self.fx_widen_haas_amt.clamp(0.0, 1.0);
                    let mix = self.fx_widen_mix_amt.clamp(0.0, 1.0);
                    // 0..30 ms at the engine sample rate.
                    let haas_samples =
                        ((haas_amt * 0.030 * sr) as usize).min(FX_WIDEN_HAAS_MAX_SAMPLES - 1);
                    self.fx_widen_haas_buf[self.fx_widen_haas_pos] = mid;
                    let read = (self.fx_widen_haas_pos + FX_WIDEN_HAAS_MAX_SAMPLES - haas_samples)
                        % FX_WIDEN_HAAS_MAX_SAMPLES;
                    let mid_delayed = self.fx_widen_haas_buf[read];
                    self.fx_widen_haas_pos =
                        (self.fx_widen_haas_pos + 1) % FX_WIDEN_HAAS_MAX_SAMPLES;
                    let mid_left = mid * (1.0 - mix) + mid_delayed * mix;
                    let side_scale = 1.0 + self.fx_widen_side_amt.clamp(0.0, 1.0) * 2.0 * mix;
                    (mid_left, side * side_scale)
                } else {
                    // Keep the ring tracking mid even when the FX is
                    // off so re-engaging mid-bar doesn't pop.
                    self.fx_widen_haas_buf[self.fx_widen_haas_pos] = mid;
                    self.fx_widen_haas_pos =
                        (self.fx_widen_haas_pos + 1) % FX_WIDEN_HAAS_MAX_SAMPLES;
                    (mid, side)
                };
                let left = (mid_for_left + side_widened).clamp(-1.0, 1.0);
                let right = (mid - side_widened).clamp(-1.0, 1.0);
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
