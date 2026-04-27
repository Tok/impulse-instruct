// ─── audio/dsp/params_from.rs ────────────────────────────────────────────────
// `AudioParams::from_app_state` lives here on its own so params.rs stays
// focused on the data definitions (AudioParams / BassVoiceParams /
// ModRouteCopy / LfoParamsCopy) plus the `compile_mod_routes` /
// `lfo_target_to_u8` pure helpers.  The mapping from AppState to the
// audio-thread snapshot is 340-ish lines on its own and grows whenever a
// new synth parameter is exposed.

use crate::state::{AppState, BPM_MAX, BPM_MIN, LfoWaveform, ModuleKind};

use super::lfo_target_opcode::lfo_target_to_u8;
use super::mod_compile::{
    compile_comparator_params, compile_crossfader_params, compile_function_gen_params,
    compile_logic_gate_params, compile_math_params, compile_mod_routes, compile_quantizer_params,
    compile_sample_hold_params, compile_slew_params, compile_trigger_div_params,
};
use super::params::{AudioParams, BassVoiceParams, LfoParamsCopy, MAX_AMEN_SLICES};

impl AudioParams {
    pub fn from_app_state(s: &AppState) -> Self {
        let bass = &s.bass_voices[0].synth;
        let bvp: [BassVoiceParams; crate::state::MAX_BASS_VOICES] =
            std::array::from_fn(|i| BassVoiceParams::from_bass_state(&s.bass_voices[i].synth));
        let (mod_routes, mod_route_count) = compile_mod_routes(s);
        Self {
            cutoff: bass.cutoff,
            resonance: bass.resonance,
            env_mod: bass.env_mod,
            decay_303: bass.decay,
            accent_level: bass.accent_level,
            waveform_saw: bass.waveform == crate::state::Waveform::Saw,
            waveform_supersaw: bass.waveform == crate::state::Waveform::Supersaw,
            supersaw_detune: bass.supersaw_detune,
            supersaw_voices: bass.supersaw_voices,
            sub_osc_level: bass.sub_osc_level,
            portamento_time_303: bass.portamento_time,
            noise_mix_303: bass.noise_mix,
            osc_detune_303: bass.osc_detune,
            fm_ratio_303: bass.fm_ratio,
            fm_depth_303: bass.fm_depth,
            distortion_303: bass.distortion,
            volume_303: bass.volume,
            pan_303: bass.pan,
            bass_voice_params: bvp,
            kick808_pitch: s.kit_a.kick.pitch,
            kick808_decay: s.kit_a.kick.decay,
            kick808_punch: s.kit_a.kick.punch,
            kick808_volume: s.kit_a.kick.volume,
            kick808_pitch_env_depth: s.kit_a.kick.pitch_env_depth,
            kick808_pitch_env_time: s.kit_a.kick.pitch_env_time,
            kick808_clip: s.kit_a.kick.clip,
            snare808_tone: s.kit_a.snare.tone,
            snare808_snappy: s.kit_a.snare.snappy,
            snare808_decay: s.kit_a.snare.decay,
            snare808_volume: s.kit_a.snare.volume,
            hihat_closed808_decay: s.kit_a.hihat_closed.decay,
            hihat_open808_decay: s.kit_a.hihat_open.decay,
            hihat808_volume: s.kit_a.hihat_closed.volume,
            kick909_pitch: s.kit_b.kick.pitch,
            kick909_decay: s.kit_b.kick.decay,
            kick909_punch: s.kit_b.kick.punch,
            kick909_volume: s.kit_b.kick.volume,
            kick909_pitch_env_depth: s.kit_b.kick.pitch_env_depth,
            kick909_pitch_env_time: s.kit_b.kick.pitch_env_time,
            kick909_clip: s.kit_b.kick.clip,
            snare909_tone: s.kit_b.snare.tone,
            snare909_snappy: s.kit_b.snare.snappy,
            snare909_decay: s.kit_b.snare.decay,
            snare909_volume: s.kit_b.snare.volume,
            hihat_closed909_decay: s.kit_b.hihat_closed.decay,
            hihat_open909_decay: s.kit_b.hihat_open.decay,
            hihat909_volume: s.kit_b.hihat_closed.volume,
            clap909_decay: s.kit_b.clap.decay,
            clap909_volume: s.kit_b.clap.volume,
            gabber_pitch: s.gabber_kick.pitch,
            gabber_decay: s.gabber_kick.decay,
            gabber_pitch_env_depth: s.gabber_kick.pitch_env_depth,
            gabber_pitch_env_time: s.gabber_kick.pitch_env_time,
            gabber_clip: s.gabber_kick.clip.clamp(0.0, 1.0),
            gabber_transient: s.gabber_kick.transient.clamp(0.0, 1.0),
            gabber_volume: s.gabber_kick.volume.clamp(0.0, 1.5),
            gabber_pan: s.gabber_kick.pan.clamp(-1.0, 1.0),
            pan_kick808: s.kit_a.kick.pan,
            pan_snare808: s.kit_a.snare.pan,
            pan_hihat808: s.kit_a.hihat_closed.pan,
            pan_kick909: s.kit_b.kick.pan,
            pan_snare909: s.kit_b.snare.pan,
            pan_hihat909: s.kit_b.hihat_closed.pan,
            pan_clap909: s.kit_b.clap.pan,
            pan_hoover: s.hoover.pan,
            pan_an1x: s.an1x.pan,
            pan_noise: s.noise_voice.pan,
            reverb_size: s.fx.reverb_size,
            reverb_damp: s.fx.reverb_damp,
            reverb_mix: s.fx.reverb_mix,
            reverb_gate_time: s.fx.reverb_gate_time,
            reverb_freeze: s.fx.reverb_freeze,
            reverb_dir: crate::audio::dsp::FxDirection::from_u8(s.fx.reverb_dir),
            reverb_rev_quant: crate::audio::dsp::FxRevQuant::from_u8(s.fx.reverb_rev_quant),
            delay_time: s.fx.delay_time,
            delay_feedback: s.fx.delay_feedback,
            delay_mix: s.fx.delay_mix,
            delay_dir: crate::audio::dsp::FxDirection::from_u8(s.fx.delay_dir),
            delay_rev_quant: crate::audio::dsp::FxRevQuant::from_u8(s.fx.delay_rev_quant),
            delay_wow_flutter: s.fx.delay_wow_flutter,
            delay_saturation: s.fx.delay_saturation,
            delay_freeze: s.fx.delay_freeze,
            delay_hpf: s.fx.delay_hpf,
            delay_lpf: s.fx.delay_lpf,
            distortion_drive: s.fx.distortion_drive,
            distortion_mix: s.fx.distortion_mix,
            master_volume: s.fx.master_volume,
            xmod_bass_to_an1x_pitch: s.fx.xmod_bass_to_an1x_pitch,
            xmod_noise_to_filter: s.fx.xmod_noise_to_filter,
            sidechain_amount: s.fx.sidechain_amount,
            sidechain_attack: s.fx.sidechain_attack,
            sidechain_release: s.fx.sidechain_release,
            compressor_multiband: s.fx.compressor_multiband,
            stereo_width: s.fx.stereo_width,
            tuning: crate::audio::dsp::TuningSystem::from_u8(s.fx.tuning),
            bitcrush_bits: s.fx.bitcrush_bits,
            bitcrush_rate: s.fx.bitcrush_rate,
            bitcrush_mix: s.fx.bitcrush_mix,
            chorus_rate: s.fx.chorus_rate,
            chorus_depth: s.fx.chorus_depth,
            chorus_mix: s.fx.chorus_mix,
            phaser_rate: s.fx.phaser_rate,
            phaser_depth: s.fx.phaser_depth,
            phaser_mix: s.fx.phaser_mix,
            flanger_rate: s.fx.flanger_rate,
            flanger_depth: s.fx.flanger_depth,
            flanger_feedback: s.fx.flanger_feedback,
            flanger_mix: s.fx.flanger_mix,
            limiter_threshold: s.fx.limiter_threshold.clamp(0.0, 1.0),
            limiter_ceiling: s.fx.limiter_ceiling.clamp(0.0, 1.0),
            limiter_release: s.fx.limiter_release.clamp(0.0, 1.0),
            limiter_lookahead: s.fx.limiter_lookahead.clamp(0.0, 1.0),
            svf_cutoff: s.fx.svf_cutoff.clamp(0.0, 1.0),
            svf_resonance: s.fx.svf_resonance.clamp(0.0, 1.0),
            svf_drive: s.fx.svf_drive.clamp(0.0, 1.0),
            svf_mix: s.fx.svf_mix.clamp(0.0, 1.0),
            svf_mode: s.fx.svf_mode.min(3),
            comb_pitch: s.fx.comb_pitch.clamp(0.0, 1.0),
            comb_feedback: s.fx.comb_feedback.clamp(0.0, 1.0),
            comb_damp: s.fx.comb_damp.clamp(0.0, 1.0),
            comb_mix: s.fx.comb_mix.clamp(0.0, 1.0),
            tilt_tilt: s.fx.tilt_tilt.clamp(0.0, 1.0),
            tilt_pivot: s.fx.tilt_pivot.clamp(0.0, 1.0),
            tilt_mix: s.fx.tilt_mix.clamp(0.0, 1.0),
            transient_attack: s.fx.transient_attack.clamp(0.0, 1.0),
            transient_sustain: s.fx.transient_sustain.clamp(0.0, 1.0),
            transient_mix: s.fx.transient_mix.clamp(0.0, 1.0),
            exciter_amount: s.fx.exciter_amount.clamp(0.0, 1.0),
            exciter_freq: s.fx.exciter_freq.clamp(0.0, 1.0),
            exciter_mix: s.fx.exciter_mix.clamp(0.0, 1.0),
            multitap_time: s.fx.multitap_time.clamp(0.0, 1.0),
            multitap_spread: s.fx.multitap_spread.clamp(0.0, 1.0),
            multitap_feedback: s.fx.multitap_feedback.clamp(0.0, 1.0),
            multitap_mix: s.fx.multitap_mix.clamp(0.0, 1.0),
            revdelay_time: s.fx.revdelay_time.clamp(0.0, 1.0),
            revdelay_feedback: s.fx.revdelay_feedback.clamp(0.0, 1.0),
            revdelay_mix: s.fx.revdelay_mix.clamp(0.0, 1.0),
            tapestop_mix: s.fx.tapestop_mix.clamp(0.0, 1.0),
            tapestop_time: s.fx.tapestop_time.clamp(0.0, 1.0),
            stutter_rate: s.fx.stutter_rate.clamp(0.0, 1.0),
            stutter_slice: s.fx.stutter_slice.clamp(0.05, 1.0),
            stutter_mix: s.fx.stutter_mix.clamp(0.0, 1.0),
            freeze_mix: s.fx.freeze_mix.clamp(0.0, 1.0),
            waveshaper_drive: s.fx.waveshaper_drive,
            waveshaper_mix: s.fx.waveshaper_mix,
            ring_mod_freq: s.fx.ring_mod_freq,
            ring_mod_mix: s.fx.ring_mod_mix,
            eq_low_gain: s.fx.eq_low_gain,
            eq_mid_gain: s.fx.eq_mid_gain,
            eq_hi_gain: s.fx.eq_hi_gain,
            autotune_amount: s.fx.autotune_amount,
            autotune_mix: s.fx.autotune_mix,
            fx_pan_pos: s.fx.fx_pan_pos.clamp(-1.0, 1.0),
            fx_pan_width: s.fx.fx_pan_width.clamp(0.0, 1.0),
            fx_pan_rate: s.fx.fx_pan_rate.clamp(0.0, 1.0),
            widen_haas: s.fx.widen_haas.clamp(0.0, 1.0),
            widen_side: s.fx.widen_side.clamp(0.0, 1.0),
            widen_mix: s.fx.widen_mix.clamp(0.0, 1.0),
            freq_shift_amount: s.fx.freq_shift_amount.clamp(0.0, 1.0),
            freq_shift_feedback: s.fx.freq_shift_feedback.clamp(0.0, 1.0),
            freq_shift_mix: s.fx.freq_shift_mix.clamp(0.0, 1.0),
            vinyl_noise: s.fx.vinyl_noise.clamp(0.0, 1.0),
            vinyl_wear: s.fx.vinyl_wear.clamp(0.0, 1.0),
            vinyl_mix: s.fx.vinyl_mix.clamp(0.0, 1.0),
            vinyl_transient: s.fx.vinyl_transient.clamp(0.0, 1.0),
            dj_filter_morph: s.fx.dj_filter_morph.clamp(0.0, 1.0),
            dj_filter_resonance: s.fx.dj_filter_resonance.clamp(0.0, 1.0),
            dj_filter_mix: s.fx.dj_filter_mix.clamp(0.0, 1.0),
            tremolo_rate: s.fx.tremolo_rate.clamp(0.0, 1.0),
            tremolo_depth: s.fx.tremolo_depth.clamp(0.0, 1.0),
            tremolo_shape: s.fx.tremolo_shape.clamp(0.0, 1.0),
            tremolo_mix: s.fx.tremolo_mix.clamp(0.0, 1.0),
            vibrato_rate: s.fx.vibrato_rate.clamp(0.0, 1.0),
            vibrato_depth: s.fx.vibrato_depth.clamp(0.0, 1.0),
            vibrato_shape: s.fx.vibrato_shape.clamp(0.0, 1.0),
            vibrato_mix: s.fx.vibrato_mix.clamp(0.0, 1.0),
            iso_low: s.fx.iso_low.clamp(0.0, 1.0),
            iso_mid: s.fx.iso_mid.clamp(0.0, 1.0),
            iso_high: s.fx.iso_high.clamp(0.0, 1.0),
            iso_mix: s.fx.iso_mix.clamp(0.0, 1.0),
            deess_freq: s.fx.deess_freq.clamp(0.0, 1.0),
            deess_threshold: s.fx.deess_threshold.clamp(0.0, 1.5),
            deess_amount: s.fx.deess_amount.clamp(0.0, 1.0),
            deess_mix: s.fx.deess_mix.clamp(0.0, 1.0),
            resbank_root: s.fx.resbank_root.clamp(0.0, 1.0),
            resbank_chord: s.fx.resbank_chord.clamp(0.0, 1.0),
            resbank_resonance: s.fx.resbank_resonance.clamp(0.0, 1.0),
            resbank_mix: s.fx.resbank_mix.clamp(0.0, 1.0),
            tape_echo_time: s.fx.tape_echo_time.clamp(0.0, 1.0),
            tape_echo_feedback: s.fx.tape_echo_feedback.clamp(0.0, 1.0),
            tape_echo_age: s.fx.tape_echo_age.clamp(0.0, 1.0),
            tape_echo_mix: s.fx.tape_echo_mix.clamp(0.0, 1.0),
            mb_low_thresh: s.fx.mb_low_thresh.clamp(0.0, 1.5),
            mb_mid_thresh: s.fx.mb_mid_thresh.clamp(0.0, 1.5),
            mb_high_thresh: s.fx.mb_high_thresh.clamp(0.0, 1.5),
            mb_mix: s.fx.mb_mix.clamp(0.0, 1.0),
            grain_delay: s.fx.grain_delay.clamp(0.0, 1.0),
            grain_size: s.fx.grain_size.clamp(0.0, 1.0),
            grain_scatter: s.fx.grain_scatter.clamp(0.0, 1.0),
            grain_mix: s.fx.grain_mix.clamp(0.0, 1.0),
            spec_stft: s.fx.spec_stft,
            spec_thresh: s.fx.spec_thresh.clamp(0.0, 1.0),
            spec_release: s.fx.spec_release.clamp(0.0, 1.0),
            spec_tilt: s.fx.spec_tilt.clamp(0.0, 1.0),
            spec_mix: s.fx.spec_mix.clamp(0.0, 1.0),
            plate_size: s.fx.plate_size.clamp(0.0, 1.0),
            plate_damping: s.fx.plate_damping.clamp(0.0, 1.0),
            plate_diffusion: s.fx.plate_diffusion.clamp(0.0, 1.0),
            plate_mix: s.fx.plate_mix.clamp(0.0, 1.0),
            tg_pattern: s.fx.tg_pattern,
            tg_rate: s
                .fx
                .tg_rate
                .min(crate::audio::dsp::fx_trance_gate::TG_RATE_COUNT - 1),
            tg_smooth: s.fx.tg_smooth.clamp(0.0, 1.0),
            tg_mix: s.fx.tg_mix.clamp(0.0, 1.0),
            wf_drive: s.fx.wf_drive.clamp(0.0, 1.0),
            wf_bias: s.fx.wf_bias.clamp(0.0, 1.0),
            wf_symmetry: s.fx.wf_symmetry.clamp(0.0, 1.0),
            wf_mix: s.fx.wf_mix.clamp(0.0, 1.0),
            param_eq_ms_mode: s.fx.param_eq_ms_mode,
            conv_reverb_mix: s.fx.conv_reverb_mix.clamp(0.0, 1.0),
            // Cabinet mode caps the IR length at 10 % of the loaded
            // impulse so even hall-length recordings get treated as
            // short cab responses.
            conv_reverb_size: if s.fx.conv_reverb_cabinet {
                s.fx.conv_reverb_size.clamp(0.0, 0.1)
            } else {
                s.fx.conv_reverb_size.clamp(0.0, 1.0)
            },
            conv_reverb_predelay: s.fx.conv_reverb_predelay.clamp(0.0, 1.0),
            conv_reverb_damp: s.fx.conv_reverb_damp.clamp(0.0, 1.0),
            conv_reverb_lowcut: s.fx.conv_reverb_lowcut.clamp(0.0, 1.0),
            conv_reverb_width: s.fx.conv_reverb_width.clamp(0.0, 1.0),
            conv_reverb_reverse: s.fx.conv_reverb_reverse,
            conv_reverb_shimmer: s.fx.conv_reverb_shimmer.clamp(0.0, 1.0),
            param_eq_bands: s.fx.param_eq_bands,
            pitch_shift_semi: s.fx.pitch_shift_semi.clamp(-24.0, 24.0),
            pitch_shift_fine: s.fx.pitch_shift_fine.clamp(-100.0, 100.0),
            pitch_shift_mix: s.fx.pitch_shift_mix.clamp(0.0, 1.0),
            pitch_shift_fbk: s.fx.pitch_shift_fbk.clamp(0.0, 1.0),
            ms_mid_gain: s.fx.ms_mid_gain.clamp(0.0, 1.0),
            ms_mid_tilt: s.fx.ms_mid_tilt.clamp(0.0, 1.0),
            ms_mid_sat: s.fx.ms_mid_sat.clamp(0.0, 1.0),
            ms_side_gain: s.fx.ms_side_gain.clamp(0.0, 1.0),
            ms_side_tilt: s.fx.ms_side_tilt.clamp(0.0, 1.0),
            ms_side_sat: s.fx.ms_side_sat.clamp(0.0, 1.0),
            compressor_threshold: s.fx.compressor_threshold,
            compressor_ratio: s.fx.compressor_ratio,
            compressor_mix: s.fx.compressor_mix,
            compressor_reverse: s.fx.compressor_reverse,
            compressor_sidechain: s.fx.compressor_sidechain,
            gate_threshold: s.fx.gate_threshold.clamp(0.0, 1.0),
            gate_attack: s.fx.gate_attack.clamp(0.0, 1.0),
            gate_release: s.fx.gate_release.clamp(0.0, 1.0),
            gate_depth: s.fx.gate_depth.clamp(0.0, 1.0),
            gate_mix: s.fx.gate_mix.clamp(0.0, 1.0),
            vocoder_bands: s.fx.vocoder_bands.clamp(0.0, 1.0),
            vocoder_carrier_mix: s.fx.vocoder_carrier_mix.clamp(0.0, 1.0),
            vocoder_sense: s.fx.vocoder_sense.clamp(0.0, 1.0),
            vocoder_mix: s.fx.vocoder_mix.clamp(0.0, 1.0),
            tape_drive: s.fx.tape_drive,
            tape_mix: s.fx.tape_mix,
            tape_flutter: s.fx.tape_flutter,
            master_pitch_st: s.fx.master_pitch_st,
            filter_mode: bass.filter_mode,
            sample_rate: crate::audio::SAMPLE_RATE,
            lfo: {
                let mut arr = [LfoParamsCopy {
                    enabled: false,
                    waveform: LfoWaveform::Sine,
                    rate: 0.2,
                    depth: 0.3,
                    phase_offset: 0.0,
                    target: 0,
                }; 4];
                for (i, slot) in s.lfo.iter().enumerate() {
                    arr[i] = LfoParamsCopy {
                        enabled: slot.enabled,
                        waveform: slot.waveform,
                        rate: slot.rate,
                        depth: slot.depth,
                        phase_offset: slot.phase_offset,
                        target: lfo_target_to_u8(slot.target),
                    };
                }
                arr
            },
            cv_seq: {
                let mut arr =
                    [super::params::CvSeqParamsCopy::default(); crate::state::CV_SEQ_SLOTS];
                for (i, slot) in s.cv_seq.iter().enumerate() {
                    arr[i] = super::params::CvSeqParamsCopy {
                        enabled: slot.enabled,
                        step_values: slot.step_values,
                        depth: slot.depth.clamp(0.0, 1.0),
                        target: lfo_target_to_u8(slot.target),
                    };
                }
                arr
            },
            slew: compile_slew_params(s),
            quantizer: compile_quantizer_params(s),
            comparator: compile_comparator_params(s),
            sample_hold: compile_sample_hold_params(s),
            math: compile_math_params(s),
            trigger_div: compile_trigger_div_params(s),
            logic_gate: compile_logic_gate_params(s),
            function_gen: compile_function_gen_params(s),
            crossfader: compile_crossfader_params(s),
            mod_routes,
            mod_route_count,
            // cv_buf is filled per block by `process_block` from
            // LFO / CvSeq / utility evaluations.  Initialise to
            // zeros so no stale values leak between blocks.
            cv_buf: [0.0; super::params::MOD_BUF_SIZE],
            sequencer_running: s.sequencer.running,
            sequencer_current_step: s.sequencer.current_step as u32,
            // MPE expression — bend folded as ±2 semitones (GM
            // standard); pressure / timbre carried as 0..=1 for the
            // bass voice's per-block additive modulation.
            mpe_bend_st: s.mpe.pitch_bend.clamp(-1.0, 1.0) * 2.0,
            mpe_pressure: s.mpe.pressure.clamp(0.0, 1.0),
            mpe_timbre: s.mpe.timbre.clamp(0.0, 1.0),
            lfo_pitch_mod_st: 0.0,
            an1x_pitch_mod_st: 0.0,
            free_eg_enabled: s.free_eg.enabled,
            free_eg_values: s.free_eg.values,
            free_eg_period: 0.5 * 64.0_f32.powf(s.free_eg.period), // 0→0.5s, 1→32s
            free_eg_depth: s.free_eg.depth,
            free_eg_target: lfo_target_to_u8(s.free_eg.target),
            free_eg_loop: s.free_eg.loop_mode,
            noise_voice_enabled: s.noise_voice.enabled,
            noise_voice_volume: s.noise_voice.volume,
            noise_voice_color: s.noise_voice.color,
            noise_voice_cutoff: s.noise_voice.cutoff,
            noise_attack: s.noise_voice.attack,
            noise_release: s.noise_voice.release,
            noise_filter_lfo_rate: s.noise_voice.filter_lfo_rate,
            noise_filter_lfo_depth: s.noise_voice.filter_lfo_depth,
            noise_sh_rate: s.noise_voice.sh_rate,
            noise_sh_depth: s.noise_voice.sh_depth,
            theremin_enabled: s.theremin.enabled,
            theremin_x: s.theremin.x.clamp(0.0, 1.0),
            theremin_y: s.theremin.y.clamp(0.0, 1.0),
            theremin_portamento: s.theremin.portamento.clamp(0.0, 1.0),
            theremin_brightness: s.theremin.brightness.clamp(0.0, 1.0),
            theremin_volume: s.theremin.volume.clamp(0.0, 1.5),
            theremin_pan: s.theremin.pan.clamp(-1.0, 1.0),
            pendulum_enabled: s.pendulum.enabled,
            pendulum_base_pitch: s.pendulum.base_pitch.clamp(0.0, 1.0),
            pendulum_detune_hz: s.pendulum.detune_hz.clamp(0.0, 1.0),
            pendulum_mix: s.pendulum.mix.clamp(0.0, 1.0),
            pendulum_volume: s.pendulum.volume.clamp(0.0, 1.5),
            pendulum_pan: s.pendulum.pan.clamp(-1.0, 1.0),
            fm_ops_enabled: s.fm_ops.enabled,
            fm_ops_volume: s.fm_ops.volume.clamp(0.0, 1.5),
            fm_ops_pan: s.fm_ops.pan.clamp(-1.0, 1.0),
            fm_ops_algorithm: s.fm_ops.algorithm.min(crate::state::FM_ALGORITHM_COUNT - 1),
            fm_ops_feedback: s.fm_ops.feedback.clamp(0.0, 1.0),
            fm_ops_op1_ratio: s.fm_ops.op1.ratio.clamp(0.0, 1.0),
            fm_ops_op1_level: s.fm_ops.op1.level.clamp(0.0, 1.0),
            fm_ops_op1_attack: s.fm_ops.op1.attack.clamp(0.0, 1.0),
            fm_ops_op1_decay: s.fm_ops.op1.decay.clamp(0.0, 1.0),
            fm_ops_op1_sustain: s.fm_ops.op1.sustain.clamp(0.0, 1.0),
            fm_ops_op1_release: s.fm_ops.op1.release.clamp(0.0, 1.0),
            fm_ops_op2_ratio: s.fm_ops.op2.ratio.clamp(0.0, 1.0),
            fm_ops_op2_level: s.fm_ops.op2.level.clamp(0.0, 1.0),
            fm_ops_op2_attack: s.fm_ops.op2.attack.clamp(0.0, 1.0),
            fm_ops_op2_decay: s.fm_ops.op2.decay.clamp(0.0, 1.0),
            fm_ops_op2_sustain: s.fm_ops.op2.sustain.clamp(0.0, 1.0),
            fm_ops_op2_release: s.fm_ops.op2.release.clamp(0.0, 1.0),
            fm_ops_op3_ratio: s.fm_ops.op3.ratio.clamp(0.0, 1.0),
            fm_ops_op3_level: s.fm_ops.op3.level.clamp(0.0, 1.0),
            fm_ops_op3_attack: s.fm_ops.op3.attack.clamp(0.0, 1.0),
            fm_ops_op3_decay: s.fm_ops.op3.decay.clamp(0.0, 1.0),
            fm_ops_op3_sustain: s.fm_ops.op3.sustain.clamp(0.0, 1.0),
            fm_ops_op3_release: s.fm_ops.op3.release.clamp(0.0, 1.0),
            fm_ops_op4_ratio: s.fm_ops.op4.ratio.clamp(0.0, 1.0),
            fm_ops_op4_level: s.fm_ops.op4.level.clamp(0.0, 1.0),
            fm_ops_op4_attack: s.fm_ops.op4.attack.clamp(0.0, 1.0),
            fm_ops_op4_decay: s.fm_ops.op4.decay.clamp(0.0, 1.0),
            fm_ops_op4_sustain: s.fm_ops.op4.sustain.clamp(0.0, 1.0),
            fm_ops_op4_release: s.fm_ops.op4.release.clamp(0.0, 1.0),
            additive_enabled: s.additive.enabled,
            additive_volume: s.additive.volume.clamp(0.0, 1.5),
            additive_pan: s.additive.pan.clamp(-1.0, 1.0),
            additive_levels: {
                let mut a = [0.0_f32; crate::state::ADDITIVE_HARMONICS];
                for (i, slot) in a.iter_mut().enumerate() {
                    *slot = s.additive.levels[i].clamp(0.0, 1.0);
                }
                a
            },
            additive_attack: s.additive.attack.clamp(0.0, 1.0),
            additive_decay: s.additive.decay.clamp(0.0, 1.0),
            additive_sustain: s.additive.sustain.clamp(0.0, 1.0),
            additive_release: s.additive.release.clamp(0.0, 1.0),
            modal_enabled: s.modal.enabled,
            modal_volume: s.modal.volume.clamp(0.0, 1.5),
            modal_pan: s.modal.pan.clamp(-1.0, 1.0),
            modal_levels: {
                let mut a = [0.0_f32; crate::state::MODAL_MODES];
                for (i, slot) in a.iter_mut().enumerate() {
                    *slot = s.modal.levels[i].clamp(0.0, 1.0);
                }
                a
            },
            modal_brightness: s.modal.brightness.clamp(0.0, 1.0),
            modal_decay_scale: s.modal.decay_scale.clamp(0.0, 1.0),
            modal_ratio_preset: s
                .modal
                .ratio_preset
                .min(crate::state::MODAL_RATIO_PRESETS - 1),
            chiptune_enabled: s.chiptune.enabled,
            chiptune_volume: s.chiptune.volume.clamp(0.0, 1.5),
            chiptune_pan: s.chiptune.pan.clamp(-1.0, 1.0),
            chiptune_osc_waveform: [
                s.chiptune
                    .osc1
                    .waveform
                    .min(crate::state::CHIPTUNE_WAVEFORMS - 1),
                s.chiptune
                    .osc2
                    .waveform
                    .min(crate::state::CHIPTUNE_WAVEFORMS - 1),
                s.chiptune
                    .osc3
                    .waveform
                    .min(crate::state::CHIPTUNE_WAVEFORMS - 1),
            ],
            chiptune_osc_level: [
                s.chiptune.osc1.level.clamp(0.0, 1.0),
                s.chiptune.osc2.level.clamp(0.0, 1.0),
                s.chiptune.osc3.level.clamp(0.0, 1.0),
            ],
            chiptune_osc_attack: [
                s.chiptune.osc1.attack.clamp(0.0, 1.0),
                s.chiptune.osc2.attack.clamp(0.0, 1.0),
                s.chiptune.osc3.attack.clamp(0.0, 1.0),
            ],
            chiptune_osc_decay: [
                s.chiptune.osc1.decay.clamp(0.0, 1.0),
                s.chiptune.osc2.decay.clamp(0.0, 1.0),
                s.chiptune.osc3.decay.clamp(0.0, 1.0),
            ],
            chiptune_osc_sustain: [
                s.chiptune.osc1.sustain.clamp(0.0, 1.0),
                s.chiptune.osc2.sustain.clamp(0.0, 1.0),
                s.chiptune.osc3.sustain.clamp(0.0, 1.0),
            ],
            chiptune_osc_release: [
                s.chiptune.osc1.release.clamp(0.0, 1.0),
                s.chiptune.osc2.release.clamp(0.0, 1.0),
                s.chiptune.osc3.release.clamp(0.0, 1.0),
            ],
            chiptune_pulse_width: s.chiptune.pulse_width.clamp(0.05, 0.95),
            chiptune_filter_cutoff: s.chiptune.filter_cutoff.clamp(0.0, 1.0),
            chiptune_filter_resonance: s.chiptune.filter_resonance.clamp(0.0, 1.0),
            chiptune_filter_mode: s
                .chiptune
                .filter_mode
                .min(crate::state::CHIPTUNE_FILTER_MODES - 1),
            chiptune_filter_mix: s.chiptune.filter_mix.clamp(0.0, 1.0),
            chiptune_ring_mod: s.chiptune.ring_mod,
            chiptune_sync: s.chiptune.sync,
            vocal_enabled: s.vocal.enabled,
            vocal_volume: s.vocal.volume.clamp(0.0, 1.5),
            vocal_pan: s.vocal.pan.clamp(-1.0, 1.0),
            vocal_vowel: s.vocal.vowel.min(crate::state::VOCAL_VOWEL_PRESETS - 1),
            vocal_morph: s.vocal.morph.clamp(0.0, 1.0),
            vocal_brightness: s.vocal.brightness.clamp(0.0, 1.0),
            vocal_formant_shift: s.vocal.formant_shift.clamp(0.0, 1.0),
            vocal_attack: s.vocal.attack.clamp(0.0, 1.0),
            vocal_decay: s.vocal.decay.clamp(0.0, 1.0),
            vocal_sustain: s.vocal.sustain.clamp(0.0, 1.0),
            vocal_release: s.vocal.release.clamp(0.0, 1.0),
            granular_enabled: s.granular.enabled,
            granular_volume: s.granular.volume,
            granular_density: s.granular.density,
            granular_grain_size: s.granular.grain_size,
            granular_position: s.granular.position,
            granular_position_jitter: s.granular.position_jitter,
            granular_pitch_scatter: s.granular.pitch_scatter,
            granular_spray: s.granular.spray,
            granular_pitch_mappable: s.granular.pitch_mappable,
            pluck_enabled: s.pluck.enabled,
            pluck_damping: s.pluck.damping.clamp(0.0, 1.0),
            pluck_brightness: s.pluck.brightness.clamp(0.0, 1.0),
            pluck_volume: s.pluck.volume.clamp(0.0, 1.5),
            pluck_pan: s.pluck.pan.clamp(-1.0, 1.0),
            pluck_pitch_offset_semi: s.pluck.pitch_offset_semi.clamp(-24.0, 24.0),
            rack_pluck: s
                .rack
                .modules
                .iter()
                .any(|m| m.kind == ModuleKind::PluckString && m.enabled),
            wavetable_enabled: s.wavetable.enabled,
            wavetable_position: s.wavetable.position.clamp(0.0, 1.0),
            wavetable_phase_offset: s.wavetable.phase_offset.clamp(0.0, 1.0),
            wavetable_volume: s.wavetable.volume.clamp(0.0, 1.5),
            wavetable_pan: s.wavetable.pan.clamp(-1.0, 1.0),
            wavetable_pitch_offset_semi: s.wavetable.pitch_offset_semi.clamp(-24.0, 24.0),
            rack_wavetable: s
                .rack
                .modules
                .iter()
                .any(|m| m.kind == ModuleKind::WavetableVoice && m.enabled),
            sample_enabled: s.sample_instrument.enabled,
            sample_root_note: s.sample_instrument.root_note,
            sample_volume: s.sample_instrument.volume.clamp(0.0, 1.5),
            sample_pan: s.sample_instrument.pan.clamp(-1.0, 1.0),
            sample_pitch_offset_cents: s.sample_instrument.pitch_offset_cents.clamp(-100.0, 100.0),
            rack_sample: s
                .rack
                .modules
                .iter()
                .any(|m| m.kind == ModuleKind::SampleInstrument && m.enabled),
            rack_fm_ops: s
                .rack
                .modules
                .iter()
                .any(|m| m.kind == ModuleKind::FmOpsVoice && m.enabled),
            rack_additive: s
                .rack
                .modules
                .iter()
                .any(|m| m.kind == ModuleKind::AdditiveVoice && m.enabled),
            rack_modal: s
                .rack
                .modules
                .iter()
                .any(|m| m.kind == ModuleKind::ModalVoice && m.enabled),
            rack_chiptune: s
                .rack
                .modules
                .iter()
                .any(|m| m.kind == ModuleKind::ChiptuneVoice && m.enabled),
            rack_vocal: s
                .rack
                .modules
                .iter()
                .any(|m| m.kind == ModuleKind::VocalVoice && m.enabled),
            sample_attack: s.sample_instrument.attack.clamp(0.0, 1.0),
            sample_decay: s.sample_instrument.decay.clamp(0.0, 1.0),
            sample_sustain: s.sample_instrument.sustain.clamp(0.0, 1.0),
            sample_release: s.sample_instrument.release.clamp(0.0, 1.0),
            sample_loop_start: s.sample_instrument.loop_start.clamp(0.0, 1.0),
            sample_loop_end: s.sample_instrument.loop_end.clamp(0.0, 1.0),
            sample_loop_enabled: s.sample_instrument.loop_enabled,
            sample_filter_cutoff: s.sample_instrument.filter_cutoff.clamp(0.0, 1.0),
            sample_filter_resonance: s.sample_instrument.filter_resonance.clamp(0.0, 1.0),
            sample_filter_mode: s.sample_instrument.filter_mode.min(2),
            sample_filter_mix: s.sample_instrument.filter_mix.clamp(0.0, 1.0),
            sample_formant_preserve: s.sample_instrument.formant_preserve,
            sample_time_stretch: s.sample_instrument.time_stretch.clamp(0.25, 4.0),
            sample_mellotron_mode: s.sample_instrument.mellotron_mode,
            sample_mellotron_flutter: s.sample_instrument.mellotron_flutter.clamp(0.0, 1.0),
            sample_mic_blend: s.sample_instrument.mic_blend.clamp(0.0, 1.0),
            hoover_enabled: s.hoover.enabled,
            hoover_filter_start: s.hoover.filter_start,
            hoover_sweep_time: s.hoover.sweep_time.clamp(0.1, 4.0),
            hoover_resonance: s.hoover.resonance,
            hoover_detune: s.hoover.detune,
            hoover_voices: s.hoover.voices,
            hoover_pitch_lfo_rate: s.hoover.pitch_lfo_rate,
            hoover_pitch_lfo_depth: s.hoover.pitch_lfo_depth,
            hoover_volume: s.hoover.volume,
            an1x_enabled: s.an1x.enabled,
            an1x_volume: s.an1x.volume,
            an1x_osc1_wave: s.an1x.osc1_wave,
            an1x_osc1_level: s.an1x.osc1_level,
            an1x_osc2_wave: s.an1x.osc2_wave,
            an1x_osc2_level: s.an1x.osc2_level,
            an1x_osc2_detune: s.an1x.osc2_detune,
            an1x_osc2_octave: s.an1x.osc2_octave,
            an1x_sub_level: s.an1x.sub_level,
            an1x_ring_mod: s.an1x.ring_mod,
            an1x_hard_sync: s.an1x.hard_sync,
            an1x_filter_cutoff: s.an1x.filter_cutoff,
            an1x_filter_resonance: s.an1x.filter_resonance,
            an1x_filter_mode: s.an1x.filter_mode,
            an1x_filter_key_track: s.an1x.filter_key_track,
            an1x_filter_env_amount: s.an1x.filter_env_amount,
            an1x_filter_attack: s.an1x.filter_attack,
            an1x_filter_decay: s.an1x.filter_decay,
            an1x_filter_sustain: s.an1x.filter_sustain,
            an1x_filter_release: s.an1x.filter_release,
            an1x_amp_attack: s.an1x.amp_attack,
            an1x_amp_decay: s.an1x.amp_decay,
            an1x_amp_sustain: s.an1x.amp_sustain,
            an1x_amp_release: s.an1x.amp_release,
            an1x_lfo_rate_hz: if s.an1x.lfo_bpm_sync && s.an1x.lfo_sync_beats > 0.0 {
                (s.sequencer.bpm / 60.0) / s.an1x.lfo_sync_beats
            } else {
                0.01 + s.an1x.lfo_rate * s.an1x.lfo_rate * 19.99
            },
            an1x_lfo_depth: s.an1x.lfo_depth,
            an1x_lfo_target: s.an1x.lfo_target,
            an1x_lfo_delay: s.an1x.lfo_delay,
            an1x_pitch_env_attack: s.an1x.pitch_env_attack,
            an1x_pitch_env_decay: s.an1x.pitch_env_decay,
            an1x_pitch_env_amount: s.an1x.pitch_env_amount,
            an1x_drift: s.an1x.drift,
            an1x_glide_time: s.an1x.glide_time,
            an1x_glide_legato: s.an1x.glide_legato,
            amen_pitch: s.amen.pitch,
            amen_volume: s.amen.volume,
            amen_loop: s.amen.loop_mode,
            amen_slice_count: s.amen.slice_count.max(1),
            amen_start_offset: s.amen.start_offset.clamp(0.0, 1.0),
            amen_end_offset: s.amen.end_offset.clamp(0.0, 1.0),
            amen_reverse: s.amen.reverse,
            amen_gate: s.amen.gate.clamp(0.05, 1.0),
            amen_stutter: s.amen.stutter.min(4),
            amen_slice_positions: {
                let mut arr = [f32::NAN; MAX_AMEN_SLICES];
                for (i, p) in s
                    .amen
                    .slice_positions
                    .iter()
                    .take(MAX_AMEN_SLICES)
                    .enumerate()
                {
                    arr[i] = p.clamp(0.0, 1.0);
                }
                arr
            },
            amen_slice_pitches: {
                let mut arr = [f32::NAN; MAX_AMEN_SLICES];
                for (i, p) in s
                    .amen
                    .slice_pitches
                    .iter()
                    .take(MAX_AMEN_SLICES)
                    .enumerate()
                {
                    arr[i] = p.clamp(-24.0, 24.0);
                }
                arr
            },
            amen_slice_volumes: {
                let mut arr = [f32::NAN; MAX_AMEN_SLICES];
                for (i, v) in s
                    .amen
                    .slice_volumes
                    .iter()
                    .take(MAX_AMEN_SLICES)
                    .enumerate()
                {
                    arr[i] = v.clamp(0.0, 2.0);
                }
                arr
            },
            amen_slice_reverses: {
                let mut arr = [-1_i8; MAX_AMEN_SLICES];
                for (i, &rev) in s
                    .amen
                    .slice_reverses
                    .iter()
                    .take(MAX_AMEN_SLICES)
                    .enumerate()
                {
                    arr[i] = if rev { 1 } else { 0 };
                }
                arr
            },
            amen_source_bpm: s.amen.source_bpm.clamp(BPM_MIN, BPM_MAX),
            amen_bpm_stretch: s.amen.bpm_stretch,
            amen_bpm_stretch_preserve: s.amen.bpm_stretch_preserve,
            sequencer_bpm: s.sequencer.bpm,
            rack_bass: s
                .rack
                .modules
                .iter()
                .any(|m| m.kind == ModuleKind::AcidBass && m.enabled),
            rack_drums808: s
                .rack
                .modules
                .iter()
                .any(|m| m.kind == ModuleKind::DrumKit808 && m.enabled),
            rack_drums909: s
                .rack
                .modules
                .iter()
                .any(|m| m.kind == ModuleKind::DrumKit909 && m.enabled),
            rack_amen: s
                .rack
                .modules
                .iter()
                .any(|m| m.kind == ModuleKind::AmenSampler && m.enabled),
            rack_hoover: s
                .rack
                .modules
                .iter()
                .any(|m| m.kind == ModuleKind::HooverLead && m.enabled),
            rack_an1x: s
                .rack
                .modules
                .iter()
                .any(|m| m.kind == ModuleKind::An1xVoice && m.enabled),
            rack_gabber_kick: s
                .rack
                .modules
                .iter()
                .any(|m| m.kind == ModuleKind::GabberKick && m.enabled),
            // TTS bus volume — take the first TTS module's volume as the
            // bus-level gain.  Multiple NeuTts modules share a single
            // ring buffer (they all push into `tts_tx`), so this is a
            // bus-wide multiplier rather than per-instance.
            tts_voice_volume: s
                .tts_modules
                .first()
                .map(|m| m.volume.clamp(0.0, 1.5))
                .unwrap_or(1.0),
        }
    }
}
