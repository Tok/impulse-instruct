// ─── audio/dsp/params.rs ──────────────────────────────────────────────────────
// AudioParams snapshot — copied from AppState for the audio thread.
// This module has no side effects; all types are Copy/Clone.

use crate::state::{
    An1xLfoTarget, An1xWave, AppState, FilterMode, LfoTarget, LfoWaveform, ModuleKind,
};

/// Per-slot LFO configuration passed to the audio thread (Copy-safe).
#[derive(Clone, Copy, Debug)]
pub struct LfoParamsCopy {
    pub enabled: bool,
    pub waveform: u8,      // 0=Sine 1=Triangle 2=Saw 3=InvSaw 4=Square 5=S&H
    pub rate: f32,         // 0–1
    pub depth: f32,        // 0–1
    pub phase_offset: f32, // 0–1
    pub target: u8,        // 0=None 1=BassCutoff 2=BassResonance 3=BassPitch 4=BassVolume
                           // 5=ReverbMix 6=DelayTime 7=DelayFeedback 8=ChorusMix 9=ChorusRate 10=Kick808Pitch
}

/// Per-voice bass synth params — one per Bass303 instance.
#[derive(Clone, Copy, Debug)]
pub struct BassVoiceParams {
    pub cutoff: f32,
    pub resonance: f32,
    pub env_mod: f32,
    pub decay: f32,
    pub accent_level: f32,
    pub waveform_saw: bool,
    pub waveform_supersaw: bool,
    pub supersaw_detune: f32,
    pub supersaw_voices: u8,
    pub sub_osc_level: f32,
    pub portamento_time: f32,
    pub noise_mix: f32,
    pub osc_detune: f32,
    pub fm_ratio: f32,
    pub fm_depth: f32,
    pub distortion: f32,
    pub volume: f32,
    pub filter_mode: u8, // 0=LP, 1=HP, 2=BP
}

impl BassVoiceParams {
    fn from_bass_state(b: &crate::state::BassState) -> Self {
        use crate::state::{FilterMode, Waveform};
        Self {
            cutoff: b.cutoff,
            resonance: b.resonance,
            env_mod: b.env_mod,
            decay: b.decay,
            accent_level: b.accent_level,
            waveform_saw: b.waveform == Waveform::Saw,
            waveform_supersaw: b.waveform == Waveform::Supersaw,
            supersaw_detune: b.supersaw_detune,
            supersaw_voices: b.supersaw_voices,
            sub_osc_level: b.sub_osc_level,
            portamento_time: b.portamento_time,
            noise_mix: b.noise_mix,
            osc_detune: b.osc_detune,
            fm_ratio: b.fm_ratio,
            fm_depth: b.fm_depth,
            distortion: b.distortion,
            volume: b.volume,
            filter_mode: match b.filter_mode {
                FilterMode::Lowpass => 0,
                FilterMode::Highpass => 1,
                FilterMode::Bandpass => 2,
            },
        }
    }
}

#[derive(Clone, Copy)]
pub struct AudioParams {
    // 303 — voice 0 params kept for LFO/free-EG modulation targets
    pub cutoff: f32,
    pub resonance: f32,
    pub env_mod: f32,
    pub decay_303: f32,
    pub accent_level: f32,
    pub waveform_saw: bool,       // true = saw, false = square
    pub waveform_supersaw: bool,  // true = supersaw (overrides waveform_saw)
    pub supersaw_detune: f32,     // 0–1 → 0–1 semitone spread
    pub supersaw_voices: u8,      // 2–7
    pub sub_osc_level: f32,       // 0–1 sub-oscillator mix level
    pub portamento_time_303: f32, // 0–1 → 10ms–500ms
    pub noise_mix_303: f32,       // 0–1 white noise into osc before filter
    pub osc_detune_303: f32,      // semitone offset -1..+1
    pub fm_ratio_303: f32,        // 0–1 → modulator/carrier ratio 0.5–8.0
    pub fm_depth_303: f32,        // 0–1 FM depth; 0 = off
    pub distortion_303: f32,
    pub volume_303: f32,
    // Per-voice bass params (voices 0–3)
    pub bass_voice_params: [BassVoiceParams; crate::state::MAX_BASS_VOICES],
    // 808 kick
    pub kick808_pitch: f32,
    pub kick808_decay: f32,
    pub kick808_punch: f32,
    pub kick808_volume: f32,
    pub kick808_pitch_env_depth: f32,
    pub kick808_pitch_env_time: f32,
    pub kick808_clip: f32,
    // 808 snare
    pub snare808_tone: f32,
    pub snare808_snappy: f32,
    pub snare808_decay: f32,
    pub snare808_volume: f32,
    // 808 hihats
    pub hihat_closed808_decay: f32,
    pub hihat_open808_decay: f32,
    pub hihat808_volume: f32,
    // 909 kick
    pub kick909_pitch: f32,
    pub kick909_decay: f32,
    pub kick909_punch: f32,
    pub kick909_volume: f32,
    pub kick909_pitch_env_depth: f32,
    pub kick909_pitch_env_time: f32,
    pub kick909_clip: f32,
    // 909 snare
    pub snare909_tone: f32,
    pub snare909_snappy: f32,
    pub snare909_decay: f32,
    pub snare909_volume: f32,
    // 909 hihat/clap
    pub hihat_closed909_decay: f32,
    pub hihat_open909_decay: f32,
    pub hihat909_volume: f32,
    pub clap909_decay: f32,
    pub clap909_volume: f32,
    // FX
    pub reverb_size: f32,
    pub reverb_damp: f32,
    pub reverb_mix: f32,
    pub reverb_gate_time: f32, // 0 = no gate; gate close time in seconds
    pub reverb_freeze: bool,
    pub delay_time: f32,
    pub delay_feedback: f32,
    pub delay_mix: f32,
    pub delay_wow_flutter: f32,
    pub delay_saturation: f32,
    pub distortion_drive: f32,
    pub distortion_mix: f32,
    pub master_volume: f32,
    // Cross-modulation
    pub xmod_bass_to_an1x_pitch: f32,
    pub xmod_noise_to_filter: f32,
    // Sidechain compression (kick ducks bass/pad)
    pub sidechain_amount: f32,
    pub sidechain_attack: f32,
    pub sidechain_release: f32,
    pub compressor_multiband: f32,
    pub stereo_width: f32, // 0=mono, 0.5=normal, 1=wide
    pub tuning: u8,        // 0=12-TET, 1=just, 2=slendro, 3=pelog
    // Bitcrush
    pub bitcrush_bits: f32,
    pub bitcrush_rate: f32,
    pub bitcrush_mix: f32,
    // Chorus
    pub chorus_rate: f32,
    pub chorus_depth: f32,
    pub chorus_mix: f32,
    // Phaser
    pub phaser_rate: f32,
    pub phaser_depth: f32,
    pub phaser_mix: f32,
    // Waveshaper
    pub waveshaper_drive: f32,
    pub waveshaper_mix: f32,
    // Ring modulator
    pub ring_mod_freq: f32,
    pub ring_mod_mix: f32,
    // 3-band EQ
    pub eq_low_gain: f32,
    pub eq_mid_gain: f32,
    pub eq_hi_gain: f32,
    // Autotune pitch shifter
    pub autotune_amount: f32,
    pub autotune_mix: f32,
    // Compressor
    pub compressor_threshold: f32,
    pub compressor_ratio: f32,
    pub compressor_mix: f32,
    // Tape saturation
    pub tape_drive: f32,
    pub tape_mix: f32,
    pub tape_flutter: f32,
    pub master_pitch_st: f32, // -12..+12 semitones added to all melodic voices
    // Filter mode (0=LP, 1=HP, 2=BP)
    pub filter_mode: u8,
    // Sample rate
    pub sample_rate: f32,
    // LFO
    pub lfo: [LfoParamsCopy; 4],
    pub sequencer_running: bool,
    pub lfo_pitch_mod_st: f32,
    // Free EG
    pub free_eg_enabled: bool,
    pub free_eg_values: [f32; 8],
    pub free_eg_period: f32, // seconds
    pub free_eg_depth: f32,  // 0–1 (bipolar: 0.5 = centre)
    pub free_eg_target: u8,  // same codes as LFO target
    pub free_eg_loop: bool,
    // Noise voice
    pub noise_voice_enabled: bool,
    pub noise_voice_volume: f32,
    pub noise_voice_color: f32,
    pub noise_voice_cutoff: f32,
    pub noise_attack: f32,
    pub noise_release: f32,
    pub noise_filter_lfo_rate: f32,
    pub noise_filter_lfo_depth: f32,
    pub noise_sh_rate: f32,
    pub noise_sh_depth: f32,
    // Granular texture
    pub granular_enabled: bool,
    pub granular_volume: f32,
    pub granular_density: f32,
    pub granular_grain_size: f32,
    pub granular_position: f32,
    pub granular_position_jitter: f32,
    pub granular_pitch_scatter: f32,
    pub granular_spray: f32,
    // Hoover lead
    pub hoover_enabled: bool,
    pub hoover_filter_start: f32,
    pub hoover_sweep_time: f32,
    pub hoover_resonance: f32,
    pub hoover_detune: f32,
    pub hoover_voices: u8,
    pub hoover_pitch_lfo_rate: f32,
    pub hoover_pitch_lfo_depth: f32,
    pub hoover_volume: f32,
    // AN1X voice
    pub an1x_enabled: bool,
    pub an1x_volume: f32,
    pub an1x_osc1_wave: u8, // 0=Saw 1=Square 2=Triangle 3=Sine 4=Noise
    pub an1x_osc1_level: f32,
    pub an1x_osc2_wave: u8,
    pub an1x_osc2_level: f32,
    pub an1x_osc2_detune: f32, // 0–1 (0.5 = unison)
    pub an1x_osc2_octave: i8,  // -2..+2
    pub an1x_sub_level: f32,
    pub an1x_ring_mod: bool,
    pub an1x_hard_sync: bool,
    pub an1x_filter_cutoff: f32,
    pub an1x_filter_resonance: f32,
    pub an1x_filter_mode: u8, // 0=LP 1=HP 2=BP
    pub an1x_filter_key_track: f32,
    pub an1x_filter_env_amount: f32,
    pub an1x_filter_attack: f32,
    pub an1x_filter_decay: f32,
    pub an1x_filter_sustain: f32,
    pub an1x_filter_release: f32,
    pub an1x_amp_attack: f32,
    pub an1x_amp_decay: f32,
    pub an1x_amp_sustain: f32,
    pub an1x_amp_release: f32,
    pub an1x_lfo_rate_hz: f32,      // Hz (0.01–20)
    pub an1x_lfo_depth: f32,        // 0–1
    pub an1x_lfo_target: u8,        // 0=Pitch 1=FilterCutoff 2=Amplitude
    pub an1x_lfo_delay: f32,        // 0–1 → 0–4s fade-in
    pub an1x_pitch_env_attack: f32, // 0–1
    pub an1x_pitch_env_decay: f32,  // 0–1
    pub an1x_pitch_env_amount: f32, // 0–1 (0.5=zero, ±24 st)
    pub an1x_drift: f32,            // 0–1
    pub an1x_glide_time: f32,       // 0–1 → 0–500ms
    pub an1x_glide_legato: bool,
    // Amen sampler
    pub amen_pitch: f32,  // semitones -24..+24
    pub amen_volume: f32, // 0–1
    pub amen_loop: bool,
    // Rack presence — only trigger / process voices that are in the rack
    pub rack_bass: bool,
    pub rack_drums808: bool,
    pub rack_drums909: bool,
    pub rack_amen: bool,
    pub rack_hoover: bool,
    pub rack_an1x: bool,
}

impl AudioParams {
    pub fn from_app_state(s: &AppState) -> Self {
        let bass = &s.bass_voices[0].synth;
        let bvp: [BassVoiceParams; crate::state::MAX_BASS_VOICES] =
            std::array::from_fn(|i| BassVoiceParams::from_bass_state(&s.bass_voices[i].synth));
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
            reverb_size: s.fx.reverb_size,
            reverb_damp: s.fx.reverb_damp,
            reverb_mix: s.fx.reverb_mix,
            reverb_gate_time: s.fx.reverb_gate_time,
            reverb_freeze: s.fx.reverb_freeze,
            delay_time: s.fx.delay_time,
            delay_feedback: s.fx.delay_feedback,
            delay_mix: s.fx.delay_mix,
            delay_wow_flutter: s.fx.delay_wow_flutter,
            delay_saturation: s.fx.delay_saturation,
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
            tuning: s.fx.tuning,
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
            compressor_threshold: s.fx.compressor_threshold,
            compressor_ratio: s.fx.compressor_ratio,
            compressor_mix: s.fx.compressor_mix,
            tape_drive: s.fx.tape_drive,
            tape_mix: s.fx.tape_mix,
            tape_flutter: s.fx.tape_flutter,
            master_pitch_st: s.fx.master_pitch_st,
            filter_mode: match bass.filter_mode {
                FilterMode::Lowpass => 0,
                FilterMode::Highpass => 1,
                FilterMode::Bandpass => 2,
            },
            sample_rate: 44100.0,
            lfo: {
                let mut arr = [LfoParamsCopy {
                    enabled: false,
                    waveform: 0,
                    rate: 0.2,
                    depth: 0.3,
                    phase_offset: 0.0,
                    target: 0,
                }; 4];
                for (i, slot) in s.lfo.iter().enumerate() {
                    arr[i] = LfoParamsCopy {
                        enabled: slot.enabled,
                        waveform: match slot.waveform {
                            LfoWaveform::Sine => 0,
                            LfoWaveform::Triangle => 1,
                            LfoWaveform::Saw => 2,
                            LfoWaveform::InvSaw => 3,
                            LfoWaveform::Square => 4,
                            LfoWaveform::SampleAndHold => 5,
                        },
                        rate: slot.rate,
                        depth: slot.depth,
                        phase_offset: slot.phase_offset,
                        target: match slot.target {
                            LfoTarget::None => 0,
                            LfoTarget::BassCutoff => 1,
                            LfoTarget::BassResonance => 2,
                            LfoTarget::BassPitch => 3,
                            LfoTarget::BassVolume => 4,
                            LfoTarget::ReverbMix => 5,
                            LfoTarget::DelayTime => 6,
                            LfoTarget::DelayFeedback => 7,
                            LfoTarget::ChorusMix => 8,
                            LfoTarget::ChorusRate => 9,
                            LfoTarget::Kick808Pitch => 10,
                            LfoTarget::PhaserRate => 11,
                            LfoTarget::PhaserDepth => 12,
                            LfoTarget::DistortionDrive => 13,
                            LfoTarget::MasterVolume => 14,
                            LfoTarget::An1xCutoff => 15,
                            LfoTarget::An1xPitch => 16,
                        },
                    };
                }
                arr
            },
            sequencer_running: s.sequencer.running,
            lfo_pitch_mod_st: 0.0,
            free_eg_enabled: s.free_eg.enabled,
            free_eg_values: s.free_eg.values,
            free_eg_period: 0.5 * 64.0_f32.powf(s.free_eg.period), // 0→0.5s, 1→32s
            free_eg_depth: s.free_eg.depth,
            free_eg_target: match s.free_eg.target {
                LfoTarget::None => 0,
                LfoTarget::BassCutoff => 1,
                LfoTarget::BassResonance => 2,
                LfoTarget::BassPitch => 3,
                LfoTarget::BassVolume => 4,
                LfoTarget::ReverbMix => 5,
                LfoTarget::DelayTime => 6,
                LfoTarget::DelayFeedback => 7,
                LfoTarget::ChorusMix => 8,
                LfoTarget::ChorusRate => 9,
                LfoTarget::Kick808Pitch => 10,
                LfoTarget::PhaserRate => 11,
                LfoTarget::PhaserDepth => 12,
                LfoTarget::DistortionDrive => 13,
                LfoTarget::MasterVolume => 14,
                LfoTarget::An1xCutoff => 15,
                LfoTarget::An1xPitch => 16,
            },
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
            an1x_osc1_wave: match s.an1x.osc1_wave {
                An1xWave::Saw => 0,
                An1xWave::Square => 1,
                An1xWave::Triangle => 2,
                An1xWave::Sine => 3,
                An1xWave::Noise => 4,
            },
            an1x_osc1_level: s.an1x.osc1_level,
            an1x_osc2_wave: match s.an1x.osc2_wave {
                An1xWave::Saw => 0,
                An1xWave::Square => 1,
                An1xWave::Triangle => 2,
                An1xWave::Sine => 3,
                An1xWave::Noise => 4,
            },
            an1x_osc2_level: s.an1x.osc2_level,
            an1x_osc2_detune: s.an1x.osc2_detune,
            an1x_osc2_octave: s.an1x.osc2_octave,
            an1x_sub_level: s.an1x.sub_level,
            an1x_ring_mod: s.an1x.ring_mod,
            an1x_hard_sync: s.an1x.hard_sync,
            an1x_filter_cutoff: s.an1x.filter_cutoff,
            an1x_filter_resonance: s.an1x.filter_resonance,
            an1x_filter_mode: match s.an1x.filter_mode {
                FilterMode::Lowpass => 0,
                FilterMode::Highpass => 1,
                FilterMode::Bandpass => 2,
            },
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
            an1x_lfo_target: match s.an1x.lfo_target {
                An1xLfoTarget::Pitch => 0,
                An1xLfoTarget::FilterCutoff => 1,
                An1xLfoTarget::Amplitude => 2,
            },
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
        }
    }
}
