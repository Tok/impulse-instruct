// ─── audio/dsp/fx_step.rs ────────────────────────────────────────────────────
// `DspState::apply_fx_step` + `apply_fx_chain` — the single-FX dispatch
// function plus the small convenience that walks a chain by calling it.
// Lifted out of dsp/mod.rs to keep that file under the 1000-line cap;
// the match arm set grows every time a new FX ships.

use crate::state::{FeedbackRoute, FxStep, SidechainSource};

use super::DspState;
use super::fx_math::{BitcrushState, bitcrush_step, drive_step, waveshaper_step};
use super::params::AudioParams;
use super::rev_tap::{rev_tap_len_for_quant, step_rev_tap};

impl DspState {
    /// Apply one FX step to `sig` and return the result.
    /// `sidechain` is the resolved sidechain audio sample for this step
    /// (read from the previous-sample voice / FX cache via
    /// `apply_fx_chain`), or `sig` itself when the step has no sidechain
    /// route.  Most FX ignore it; only Gate / Vocoder / Compressor
    /// (sidechain mode) consume it.
    /// Must not hold any borrow on `self.fx_plan` when called.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn apply_fx_step(
        &mut self,
        step: FxStep,
        sig: f32,
        sidechain: f32,
        p: &AudioParams,
        delay_samples: usize,
        sr: f32,
        gate_env: f32,
    ) -> f32 {
        match step {
            FxStep::Waveshaper => waveshaper_step(sig, p.waveshaper_drive, p.waveshaper_mix),
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
                    use super::FxDirection;
                    let wet = match p.reverb_dir {
                        FxDirection::Reverse => r.process(rev_in, sz, dp, fz),
                        FxDirection::Mirror => {
                            r.process(sig, sz, dp, fz) + r.process(rev_in, sz, dp, false) * 0.7
                        }
                        FxDirection::Forward => r.process(sig, sz, dp, fz),
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
                let (ds, fb, wf, sat, fz, hpf, lpf) = (
                    delay_samples,
                    p.delay_feedback,
                    p.delay_wow_flutter,
                    p.delay_saturation,
                    p.delay_freeze,
                    p.delay_hpf,
                    p.delay_lpf,
                );
                use super::FxDirection;
                let wet = match p.delay_dir {
                    FxDirection::Reverse => {
                        d.process_tape(rev_in, ds, fb, wf, sat, fz, hpf, lpf, sr)
                    }
                    FxDirection::Mirror => {
                        d.process_tape(sig, ds, fb, wf, sat, fz, hpf, lpf, sr)
                            + d.process_tape(rev_in, ds, fb, wf, sat, fz, hpf, lpf, sr) * 0.7
                    }
                    FxDirection::Forward => d.process_tape(sig, ds, fb, wf, sat, fz, hpf, lpf, sr),
                };
                sig * (1.0 - p.delay_mix) + wet * p.delay_mix
            }
            FxStep::Bitcrush => {
                let state = BitcrushState {
                    held: self.bitcrush_held,
                    counter: self.bitcrush_counter,
                };
                let (next, out) =
                    bitcrush_step(state, sig, p.bitcrush_bits, p.bitcrush_rate, p.bitcrush_mix);
                self.bitcrush_held = next.held;
                self.bitcrush_counter = next.counter;
                out
            }
            FxStep::Chorus => {
                self.chorus
                    .process(sig, p.chorus_rate, p.chorus_depth, p.chorus_mix, sr)
            }
            FxStep::Phaser => {
                self.phaser
                    .process(sig, p.phaser_rate, p.phaser_depth, p.phaser_mix, sr)
            }
            FxStep::Flanger => self.flanger.process(
                sig,
                p.flanger_rate,
                p.flanger_depth,
                p.flanger_feedback,
                p.flanger_mix,
                sr,
            ),
            FxStep::Limiter => self.limiter.process(
                sig,
                p.limiter_threshold,
                p.limiter_ceiling,
                p.limiter_release,
                p.limiter_lookahead,
                sr,
            ),
            FxStep::Filter => self.svf.process(
                sig,
                p.svf_cutoff,
                p.svf_resonance,
                p.svf_drive,
                p.svf_mode,
                p.svf_mix,
                sr,
            ),
            FxStep::Comb => self.comb.process(
                sig,
                p.comb_pitch,
                p.comb_feedback,
                p.comb_damp,
                p.comb_mix,
                sr,
            ),
            FxStep::Tilt => self
                .tilt
                .process(sig, p.tilt_tilt, p.tilt_pivot, p.tilt_mix),
            FxStep::Transient => self.transient.process(
                sig,
                p.transient_attack,
                p.transient_sustain,
                p.transient_mix,
                sr,
            ),
            FxStep::Exciter => {
                self.exciter
                    .process(sig, p.exciter_amount, p.exciter_freq, p.exciter_mix, sr)
            }
            FxStep::Multitap => self.multitap.process(
                sig,
                p.multitap_time,
                p.multitap_spread,
                p.multitap_feedback,
                p.multitap_mix,
                sr,
            ),
            FxStep::RevDelay => self.rev_delay.process(
                sig,
                p.revdelay_time,
                p.revdelay_feedback,
                p.revdelay_mix,
                sr,
            ),
            FxStep::TapeStop => self
                .tape_stop
                .process(sig, p.tapestop_mix, p.tapestop_time, sr),
            FxStep::Stutter => self.stutter.process(
                sig,
                p.stutter_rate,
                p.stutter_slice,
                p.stutter_mix,
                p.sequencer_bpm,
                sr,
            ),
            FxStep::Freeze => self.freeze.process(sig, p.freeze_mix, sr),
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
            FxStep::Compressor => {
                // In sidechain mode the level detector reads `sidechain`
                // (the cable-fed signal) but the gain reduction still
                // applies to `sig`.  Compressor::process takes a single
                // input + handles internal envelope state — we feed it
                // the right detector via `process_with_detector`.
                let detector = if p.compressor_sidechain {
                    sidechain
                } else {
                    sig
                };
                self.compressor.process_with_detector(
                    sig,
                    detector,
                    p.compressor_threshold,
                    p.compressor_ratio,
                    p.compressor_mix,
                    p.compressor_multiband,
                    p.compressor_reverse,
                    sr,
                )
            }
            FxStep::TapeSat => {
                self.tape_sat
                    .process(sig, p.tape_drive, p.tape_mix, p.tape_flutter, sr)
            }
            FxStep::Drive => drive_step(sig, p.distortion_drive, p.distortion_mix),
            FxStep::Autotune => self
                .autotune
                .process(sig, p.autotune_amount, p.autotune_mix),
            FxStep::Pan => {
                // Auto-pan — mono-in / mono-out FX that latches a per-sample
                // side contribution into self.fx_pan_side for the master
                // stage to mix into the stereo output.  Signal itself is
                // passed through unchanged so chain order doesn't matter.
                let rate_hz = 0.05 + p.fx_pan_rate * 7.95;
                self.fx_pan_phase = (self.fx_pan_phase + rate_hz / sr).rem_euclid(1.0);
                let lfo = (self.fx_pan_phase * std::f32::consts::TAU).sin();
                let pan_mult = (p.fx_pan_pos + lfo * p.fx_pan_width).clamp(-1.0, 1.0);
                // Side signal carries the same sign as pan_mult * sig — the
                // master mix adds this to `side` so left = mid+side,
                // right = mid-side produces a proper L/R swing.
                self.fx_pan_side = sig * pan_mult * 0.5;
                sig
            }
            FxStep::ConvReverb => self.conv_reverb.process(
                sig,
                p.conv_reverb_mix,
                p.conv_reverb_predelay,
                p.conv_reverb_damp,
                p.conv_reverb_lowcut,
                p.conv_reverb_size,
                p.conv_reverb_width,
                sr,
            ),
            FxStep::ParamEq => {
                // M/S mode: skip the chain cascade and latch a flag so
                // the master stage runs separate cascades on the
                // decoded mid + side channels.  Passthrough here
                // because there's no L/R available in the mono chain.
                if p.param_eq_ms_mode {
                    self.param_eq_ms_active = true;
                    sig
                } else {
                    self.param_eq.process(sig, &p.param_eq_bands, sr)
                }
            }
            FxStep::PitchShift => self.pitch_shift.process(
                sig,
                p.pitch_shift_semi,
                p.pitch_shift_fine,
                p.pitch_shift_mix,
                p.pitch_shift_fbk,
            ),
            FxStep::Gate => self.gate.process(
                sig,
                sidechain,
                p.gate_threshold,
                p.gate_attack,
                p.gate_release,
                p.gate_depth,
                p.gate_mix,
                sr,
            ),
            FxStep::Vocoder => self.vocoder.process(
                sig,
                sidechain,
                p.vocoder_bands,
                p.vocoder_carrier_mix,
                p.vocoder_sense,
                p.vocoder_mix,
                sr,
            ),
            FxStep::Widen => {
                // Passthrough that latches the active widen knobs so
                // the master stage applies Haas + side scaling on the
                // final L/R after the per-voice pan side has already
                // been computed.  Same idiom as FxStep::Pan.
                if p.widen_mix > 0.001 {
                    self.fx_widen_active = true;
                    self.fx_widen_haas_amt = p.widen_haas;
                    self.fx_widen_side_amt = p.widen_side;
                    self.fx_widen_mix_amt = p.widen_mix;
                }
                sig
            }
            FxStep::FreqShift => self.freq_shift.process(
                sig,
                p.freq_shift_amount,
                p.freq_shift_feedback,
                p.freq_shift_mix,
                sr,
            ),
            FxStep::Vinyl => self
                .vinyl
                .process(sig, p.vinyl_noise, p.vinyl_wear, p.vinyl_mix),
            FxStep::DjFilter => self.dj_filter.process(
                sig,
                p.dj_filter_morph,
                p.dj_filter_resonance,
                p.dj_filter_mix,
                sr,
            ),
            FxStep::Tremolo => self.tremolo.process(
                sig,
                p.tremolo_rate,
                p.tremolo_depth,
                p.tremolo_shape,
                p.tremolo_mix,
                sr,
            ),
            FxStep::Vibrato => self.vibrato.process(
                sig,
                p.vibrato_rate,
                p.vibrato_depth,
                p.vibrato_shape,
                p.vibrato_mix,
                sr,
            ),
            FxStep::IsoEq => self
                .iso_eq
                .process(sig, p.iso_low, p.iso_mid, p.iso_high, p.iso_mix, sr),
            FxStep::DeEsser => self.deesser.process(
                sig,
                p.deess_freq,
                p.deess_threshold,
                p.deess_amount,
                p.deess_mix,
                sr,
            ),
            FxStep::ResBank => self.resbank.process(
                sig,
                p.resbank_root,
                p.resbank_chord,
                p.resbank_resonance,
                p.resbank_mix,
                sr,
            ),
            FxStep::TapeEcho => self.tape_echo.process(
                sig,
                p.tape_echo_time,
                p.tape_echo_feedback,
                p.tape_echo_age,
                p.tape_echo_mix,
                sr,
            ),
        }
    }

    /// Route a voice bus through every one of its parallel sends, summing
    /// the results.  Returns the original `sig` unchanged when the voice
    /// has zero configured sends (caller then routes through the global
    /// chain via the fast path).  Each send is one independent
    /// `apply_fx_chain` call, so N sends cost roughly N× the FX work.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn route_voice_sends(
        &mut self,
        sig: f32,
        snap: &super::VoiceSendsSnap,
        feedback_routes: &[FeedbackRoute],
        sidechain_snap: &super::fx_sidechain::SidechainSnap,
        p: &AudioParams,
        delay_samples: usize,
        sr: f32,
        gate_env: f32,
    ) -> f32 {
        if snap.count == 0 {
            return sig;
        }
        let mut out = 0.0;
        for i in 0..snap.count {
            let chain = &snap.chains[i][..snap.chain_lens[i]];
            out += self.apply_fx_chain(
                sig * snap.gains[i],
                chain,
                feedback_routes,
                sidechain_snap,
                p,
                delay_samples,
                sr,
                gate_env,
            );
        }
        out
    }

    /// Apply an FX chain to `sig`.  `feedback_routes` is typically
    /// `&plan.feedback_routes`; pass `&[]` for chains that should skip
    /// feedback (most call sites don't need it).
    ///
    /// Before each step runs we mix in `prev_fx_output[src] * gain` for
    /// every route whose `target` matches this step; after it runs we
    /// write the step's output to `prev_fx_output[step]`.  The implicit
    /// one-sample delay across samples makes the feedback loop
    /// algebraically well-defined — no instantaneous cycles — so the
    /// graph can't blow up numerically.  The compile-time clamp to
    /// `FEEDBACK_GAIN_MAX` (0.95) preserves the stability margin.
    ///
    /// `sidechain_snap` resolves the per-step sidechain signal: for steps
    /// with a `SidechainSource::Voice(kind)` route, it pulls from the
    /// per-sample voice cache; for `SidechainSource::Fx(step)`, it pulls
    /// from `prev_fx_output` (one-sample delay, same machinery as
    /// feedback edges).  Steps with no sidechain route get `sig` itself
    /// — keeps unconnected gate / vocoder / compressor sane (gate
    /// becomes a noise gate, etc.).
    #[allow(clippy::too_many_arguments)]
    pub(super) fn apply_fx_chain(
        &mut self,
        mut sig: f32,
        chain: &[FxStep],
        feedback_routes: &[FeedbackRoute],
        sidechain_snap: &super::fx_sidechain::SidechainSnap,
        p: &AudioParams,
        delay_samples: usize,
        sr: f32,
        gate_env: f32,
    ) -> f32 {
        for &step in chain {
            for fr in feedback_routes {
                if fr.target == step {
                    sig += self.prev_fx_output[fr.source.idx()] * fr.gain;
                }
            }
            // Resolve sidechain source for this step.  The lookup walks
            // the small (≤MAX_SIDECHAIN) snapshot array — linear scan is
            // faster than a HashMap probe for typical sizes (0–4
            // entries).
            let mut sidechain = sig;
            for i in 0..sidechain_snap.count {
                if sidechain_snap.targets[i] == step {
                    sidechain = match sidechain_snap.sources[i] {
                        SidechainSource::Voice(_) => sidechain_snap.voice_signals[i],
                        SidechainSource::Fx(src) => self.prev_fx_output[src.idx()],
                    };
                    break;
                }
            }
            sig = self.apply_fx_step(step, sig, sidechain, p, delay_samples, sr, gate_env);
            self.prev_fx_output[step.idx()] = sig;
        }
        sig
    }
}
