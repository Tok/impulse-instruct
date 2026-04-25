// ─── audio/dsp/params_from.rs ────────────────────────────────────────────────
// `AudioParams::from_app_state` lives here on its own so params.rs stays
// focused on the data definitions (AudioParams / BassVoiceParams /
// ModRouteCopy / LfoParamsCopy) plus the `compile_mod_routes` /
// `lfo_target_to_u8` pure helpers.  The mapping from AppState to the
// audio-thread snapshot is 340-ish lines on its own and grows whenever a
// new synth parameter is exposed.

use crate::state::{AppState, BPM_MAX, BPM_MIN, LfoWaveform, ModuleKind};

use super::params::{
    AudioParams, BassVoiceParams, LfoParamsCopy, compile_mod_routes, lfo_target_to_u8,
};

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
            conv_reverb_mix: s.fx.conv_reverb_mix.clamp(0.0, 1.0),
            conv_reverb_size: s.fx.conv_reverb_size.clamp(0.0, 1.0),
            conv_reverb_predelay: s.fx.conv_reverb_predelay.clamp(0.0, 1.0),
            conv_reverb_damp: s.fx.conv_reverb_damp.clamp(0.0, 1.0),
            conv_reverb_lowcut: s.fx.conv_reverb_lowcut.clamp(0.0, 1.0),
            conv_reverb_width: s.fx.conv_reverb_width.clamp(0.0, 1.0),
            conv_reverb_reverse: s.fx.conv_reverb_reverse,
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
            mod_routes,
            mod_route_count,
            sequencer_running: s.sequencer.running,
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
            granular_enabled: s.granular.enabled,
            granular_volume: s.granular.volume,
            granular_density: s.granular.density,
            granular_grain_size: s.granular.grain_size,
            granular_position: s.granular.position,
            granular_position_jitter: s.granular.position_jitter,
            granular_pitch_scatter: s.granular.pitch_scatter,
            granular_spray: s.granular.spray,
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
                let mut arr = [f32::NAN; 16];
                for (i, p) in s.amen.slice_positions.iter().take(16).enumerate() {
                    arr[i] = p.clamp(0.0, 1.0);
                }
                arr
            },
            amen_slice_pitches: {
                let mut arr = [f32::NAN; 16];
                for (i, p) in s.amen.slice_pitches.iter().take(16).enumerate() {
                    arr[i] = p.clamp(-24.0, 24.0);
                }
                arr
            },
            amen_slice_volumes: {
                let mut arr = [f32::NAN; 16];
                for (i, v) in s.amen.slice_volumes.iter().take(16).enumerate() {
                    arr[i] = v.clamp(0.0, 2.0);
                }
                arr
            },
            amen_slice_reverses: {
                let mut arr = [-1_i8; 16];
                for (i, &rev) in s.amen.slice_reverses.iter().take(16).enumerate() {
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
