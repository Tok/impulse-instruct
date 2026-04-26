// ─── audio/dsp/params.rs ──────────────────────────────────────────────────────
// AudioParams snapshot — copied from AppState for the audio thread.
// This module has no side effects; all types are Copy/Clone.

use crate::state::{AppState, LfoTarget};

/// Walk the rack's Mod cables and emit a fixed-size array of compiled mod
/// routes for the audio thread to consume.  Each route resolves the source
/// LFO module to its slot index (position in the rack's LfoModule order) and
/// the destination Mod-In jack to its `LfoTarget` (Fixed slot or the user-
/// picked Selector value).  Routes whose source/target can't be resolved or
/// whose target is `None` are silently skipped.  The depth defaults to 1.0
/// (a per-cable depth knob is a future addition).
pub fn compile_mod_routes(s: &AppState) -> ([ModRouteCopy; MAX_MOD_ROUTES], u8) {
    use crate::state::{ModInput, ModuleKind, PortKind, mod_inputs};
    let mut routes = [ModRouteCopy::default(); MAX_MOD_ROUTES];
    let mut count = 0usize;
    let lfo_ids: Vec<u32> = s
        .rack
        .modules
        .iter()
        .filter(|m| m.kind == ModuleKind::LfoModule)
        .map(|m| m.id)
        .collect();
    for cable in &s.rack.cables {
        if count >= MAX_MOD_ROUTES {
            break;
        }
        if cable.from.kind != PortKind::Cv || cable.to.kind != PortKind::Mod {
            continue;
        }
        let Some(slot_idx) = lfo_ids.iter().position(|id| *id == cable.from.module_id) else {
            continue;
        };
        if slot_idx >= s.lfo.len() {
            continue;
        }
        let Some(target_module) = s.rack.modules.iter().find(|m| m.id == cable.to.module_id) else {
            continue;
        };
        let inputs = mod_inputs(target_module.kind);
        let depth_unipolar = target_module
            .mod_input_depths
            .get(cable.to.index as usize)
            .copied()
            .unwrap_or(1.0)
            .clamp(0.0, 1.0);
        let invert = target_module
            .mod_input_invert
            .get(cable.to.index as usize)
            .copied()
            .unwrap_or(false);
        let depth = if invert {
            -depth_unipolar
        } else {
            depth_unipolar
        };
        // Resolve the slot's effective target list — Fixed = its single
        // target, Selector = the multi-select Vec the user picked.
        let targets: &[LfoTarget] = match inputs.get(cable.to.index as usize) {
            Some(ModInput::Fixed(t)) => std::slice::from_ref(t),
            Some(ModInput::Selector) => target_module
                .mod_selectors
                .get(cable.to.index as usize)
                .map(|v| v.as_slice())
                .unwrap_or(&[]),
            None => continue,
        };
        for &t in targets {
            if t == LfoTarget::None || count >= MAX_MOD_ROUTES {
                continue;
            }
            routes[count] = ModRouteCopy {
                lfo_slot: slot_idx as u8,
                target_u8: lfo_target_to_u8(t),
                depth,
            };
            count += 1;
        }
    }
    (routes, count as u8)
}

/// Encode an `LfoTarget` into a compact u8 opcode consumed by the audio
/// thread.  Stable IDs — adding new targets requires adding new codes here
/// AND a matching arm in `apply_mod_target` (src/audio/dsp/mod.rs).
pub fn lfo_target_to_u8(t: LfoTarget) -> u8 {
    use LfoTarget::*;
    match t {
        None => 0,
        // Legacy (1..16) — drive the existing LFO-module path.
        BassCutoff => 1,
        BassResonance => 2,
        BassPitch => 3,
        BassVolume => 4,
        ReverbMix => 5,
        DelayTime => 6,
        DelayFeedback => 7,
        ChorusMix => 8,
        ChorusRate => 9,
        Kick808Pitch => 10,
        PhaserRate => 11,
        PhaserDepth => 12,
        DistortionDrive => 13,
        MasterVolume => 14,
        An1xCutoff => 15,
        An1xPitch => 16,
        // Pan family.
        BassPan => 17,
        HooverPan => 18,
        NoisePan => 19,
        Kick808Pan => 20,
        Snare808Pan => 21,
        Hihat808Pan => 22,
        Kick909Pan => 23,
        Snare909Pan => 24,
        Hihat909Pan => 25,
        Clap909Pan => 26,
        An1xPan => 27,
        // FX knob expansion.
        ReverbSize => 28,
        ReverbDamp => 29,
        DelayMix => 30,
        ChorusDepth => 31,
        PhaserMix => 32,
        WaveshaperDrive => 33,
        WaveshaperMix => 34,
        DistortionMix => 35,
        BitcrushBits => 36,
        BitcrushRate => 37,
        BitcrushMix => 38,
        RingModFreq => 39,
        RingModMix => 40,
        EqLow => 41,
        EqMid => 42,
        EqHigh => 43,
        CompThresh => 44,
        CompRatio => 45,
        CompMix => 46,
        TapeDrive => 47,
        TapeMix => 48,
        TapeFlutter => 49,
        AutotuneAmount => 50,
        AutotuneMix => 51,
        // Drum extras (52..62) and sampler/granular (63..69).
        Kick808Decay => 52,
        Snare808Tone => 53,
        Snare808Decay => 54,
        Kick909Pitch => 55,
        Kick909Decay => 56,
        Snare909Tone => 57,
        Snare909Decay => 58,
        Clap909Decay => 59,
        AmenVolume => 60,
        AmenStart => 61,
        AmenGate => 62,
        GranularVolume => 63,
        GranularDensity => 64,
        GranularGrain => 65,
        GranularPos => 66,
        StereoWidth => 67,
        GabberKickPitch => 68,
        GabberKickDecay => 69,
        GabberKickClip => 70,
        GabberKickPan => 71,
        NeuTtsVolume => 72,
        FlangerRate => 73,
        FlangerDepth => 74,
        FlangerFeedback => 75,
        FlangerMix => 76,
        LimiterThreshold => 77,
        LimiterCeiling => 78,
        LimiterRelease => 79,
        LimiterLookahead => 80,
        SvfCutoff => 81,
        SvfResonance => 82,
        SvfDrive => 83,
        SvfMix => 84,
        CombPitch => 85,
        CombFeedback => 86,
        CombDamp => 87,
        CombMix => 88,
        TiltTilt => 89,
        TiltPivot => 90,
        TiltMix => 91,
        TransientAttack => 92,
        TransientSustain => 93,
        TransientMix => 94,
        ExciterAmount => 95,
        ExciterFreq => 96,
        ExciterMix => 97,
        MultitapTime => 98,
        MultitapSpread => 99,
        MultitapFeedback => 100,
        MultitapMix => 101,
        RevDelayTime => 102,
        RevDelayFeedback => 103,
        RevDelayMix => 104,
        TapeStopMix => 105,
        StutterRate => 106,
        StutterSlice => 107,
        StutterMix => 108,
        FreezeMix => 109,
        GateThreshold => 110,
        GateAttack => 111,
        GateRelease => 112,
        GateDepth => 113,
        GateMix => 114,
        VocoderBands => 115,
        VocoderCarrierMix => 116,
        VocoderSense => 117,
        VocoderMix => 118,
        WidenHaas => 119,
        WidenSide => 120,
        WidenMix => 121,
        FreqShiftAmount => 122,
        FreqShiftFeedback => 123,
        FreqShiftMix => 124,
        SampleVolume => 125,
        SamplePan => 126,
        SamplePitch => 127,
        SampleCutoff => 128,
    }
}

/// Maximum number of cable-declared modulation routes the audio thread
/// will process per block.  Bounded so the array stays Copy-friendly.
pub const MAX_MOD_ROUTES: usize = 32;

/// Cable-declared modulation route (Copy).  Says: "LFO slot N's value drives
/// target opcode T at depth D".  Compiled from rack Mod cables in
/// `AudioParams::from_app_state`.
#[derive(Clone, Copy, Debug, Default)]
pub struct ModRouteCopy {
    pub lfo_slot: u8,  // index into AudioParams.lfo (0..3)
    pub target_u8: u8, // opcode from `lfo_target_to_u8`
    pub depth: f32,    // bipolar depth multiplier
}

/// Per-slot LFO configuration passed to the audio thread (Copy-safe).
#[derive(Clone, Copy, Debug)]
pub struct LfoParamsCopy {
    pub enabled: bool,
    pub waveform: crate::state::LfoWaveform,
    pub rate: f32,         // 0–1
    pub depth: f32,        // 0–1
    pub phase_offset: f32, // 0–1
    pub target: u8,        // opcode from `lfo_target_to_u8`
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
    pub filter_mode: crate::state::FilterMode,
    // ADSR shaping (101-style).  Defaults preserve 303 behavior.
    pub amp_attack: f32,     // 0–1 → 0–1s
    pub amp_sustain: f32,    // 0–1
    pub amp_release: f32,    // 0–1 → 0–2s
    pub filter_attack: f32,  // 0–1 → 0–0.5s
    pub filter_sustain: f32, // 0–1
    pub filter_release: f32, // 0–1 → 0–2s
    pub pulse_width: f32,    // 0.05..0.95 (centered at 0.5 = square)
    // Per-voice LFO — SH-101 style.
    pub lfo_target: crate::state::BassLfoTarget,
    pub lfo_rate: f32,  // 0–1 → 0.01–20 Hz (free)
    pub lfo_depth: f32, // 0–1
    pub lfo_waveform: crate::state::LfoWaveform,
    pub lfo_delay: f32, // 0–1 → 0–4 s fade-in
    pub lfo_bpm_sync: bool,
    pub lfo_sync_beats: f32,
    /// Phase offset 0..1 added to the running LFO phase before the
    /// waveform lookup — drives anti-phase / multi-voice stereo
    /// effects when paired with `BassLfoTarget::Pan`.
    pub lfo_phase: f32,
}

impl BassVoiceParams {
    pub(super) fn from_bass_state(b: &crate::state::BassState) -> Self {
        use crate::state::Waveform;
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
            filter_mode: b.filter_mode,
            amp_attack: b.amp_attack.clamp(0.0, 1.0),
            amp_sustain: b.amp_sustain.clamp(0.0, 1.0),
            amp_release: b.amp_release.clamp(0.0, 1.0),
            filter_attack: b.filter_attack.clamp(0.0, 1.0),
            filter_sustain: b.filter_sustain.clamp(0.0, 1.0),
            filter_release: b.filter_release.clamp(0.0, 1.0),
            pulse_width: b.pulse_width.clamp(0.05, 0.95),
            lfo_target: b.lfo_target,
            lfo_rate: b.lfo_rate.clamp(0.0, 1.0),
            lfo_depth: b.lfo_depth.clamp(0.0, 1.0),
            // Pass the enum through verbatim — the bass voice's
            // `lfo_wave` dispatch matches on LfoWaveform directly.
            // SampleAndHold isn't modelled by the per-voice LFO; the
            // dispatch falls back to sine for it.
            lfo_waveform: b.lfo_waveform,
            lfo_delay: b.lfo_delay.clamp(0.0, 1.0),
            lfo_bpm_sync: b.lfo_bpm_sync,
            lfo_sync_beats: b.lfo_sync_beats.clamp(0.03125, 16.0),
            lfo_phase: b.lfo_phase.rem_euclid(1.0),
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
    pub pan_303: f32, // -1 L .. +1 R
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
    // Gabber kick (dedicated hardcore-kick voice, distinct from 808/909)
    pub gabber_pitch: f32,
    pub gabber_decay: f32,
    pub gabber_pitch_env_depth: f32,
    pub gabber_pitch_env_time: f32,
    pub gabber_clip: f32,
    pub gabber_transient: f32,
    pub gabber_volume: f32,
    pub gabber_pan: f32,
    // Per-voice pan (-1 L .. +1 R, 0 center)
    pub pan_kick808: f32,
    pub pan_snare808: f32,
    pub pan_hihat808: f32,
    pub pan_kick909: f32,
    pub pan_snare909: f32,
    pub pan_hihat909: f32,
    pub pan_clap909: f32,
    pub pan_hoover: f32,
    pub pan_an1x: f32,
    pub pan_noise: f32,
    // FX
    pub reverb_size: f32,
    pub reverb_damp: f32,
    pub reverb_mix: f32,
    pub reverb_gate_time: f32, // 0 = no gate; gate close time in seconds
    pub reverb_freeze: bool,
    /// 0=FWD, 1=REV (preverb), 2=MIRROR.
    pub reverb_dir: super::FxDirection,
    /// Beat-division snap for the reverse-tap loop length.
    pub reverb_rev_quant: super::FxRevQuant,
    pub delay_time: f32,
    pub delay_feedback: f32,
    pub delay_mix: f32,
    /// 0=FWD, 1=REV (anti-echo), 2=MIRROR.
    pub delay_dir: super::FxDirection,
    pub delay_rev_quant: super::FxRevQuant,
    pub delay_wow_flutter: f32,
    pub delay_saturation: f32,
    /// Dub send/return: infinite hold on the delay.  When true, feedback
    /// pins at 1.0 and new input is suppressed.
    pub delay_freeze: bool,
    /// One-pole HPF on the delay's feedback path (0–1, 0 = bypass).
    pub delay_hpf: f32,
    /// One-pole LPF on the delay's feedback path (0–1, 0 = bypass).
    pub delay_lpf: f32,
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
    pub tuning: super::TuningSystem,
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
    // Flanger
    pub flanger_rate: f32,
    pub flanger_depth: f32,
    pub flanger_feedback: f32,
    pub flanger_mix: f32,
    // Brick-wall limiter
    pub limiter_threshold: f32,
    pub limiter_ceiling: f32,
    pub limiter_release: f32,
    pub limiter_lookahead: f32,
    // State-variable filter (FxFilter module)
    pub svf_cutoff: f32,
    pub svf_resonance: f32,
    pub svf_drive: f32,
    pub svf_mix: f32,
    pub svf_mode: u8,
    // Comb resonator
    pub comb_pitch: f32,
    pub comb_feedback: f32,
    pub comb_damp: f32,
    pub comb_mix: f32,
    // Tilt EQ
    pub tilt_tilt: f32,
    pub tilt_pivot: f32,
    pub tilt_mix: f32,
    // Transient designer
    pub transient_attack: f32,
    pub transient_sustain: f32,
    pub transient_mix: f32,
    // Exciter
    pub exciter_amount: f32,
    pub exciter_freq: f32,
    pub exciter_mix: f32,
    // Multitap delay
    pub multitap_time: f32,
    pub multitap_spread: f32,
    pub multitap_feedback: f32,
    pub multitap_mix: f32,
    // Reverse delay
    pub revdelay_time: f32,
    pub revdelay_feedback: f32,
    pub revdelay_mix: f32,
    // Tape stop
    pub tapestop_mix: f32,
    pub tapestop_time: f32,
    // Stutter
    pub stutter_rate: f32,
    pub stutter_slice: f32,
    pub stutter_mix: f32,
    // Spectral freezer
    pub freeze_mix: f32,
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
    // Pan FX
    pub fx_pan_pos: f32,
    pub fx_pan_width: f32,
    pub fx_pan_rate: f32,
    // Stereo widener (master-stage latch)
    pub widen_haas: f32,
    pub widen_side: f32,
    pub widen_mix: f32,
    // Frequency shifter (Hilbert SSB)
    pub freq_shift_amount: f32,
    pub freq_shift_feedback: f32,
    pub freq_shift_mix: f32,
    /// Mid/side mode flag for `FxParamEq` — see `FxState.param_eq_ms_mode`.
    pub param_eq_ms_mode: bool,
    // Convolution reverb
    pub conv_reverb_mix: f32,
    pub conv_reverb_size: f32,
    pub conv_reverb_predelay: f32,
    pub conv_reverb_damp: f32,
    pub conv_reverb_lowcut: f32,
    pub conv_reverb_width: f32,
    pub conv_reverb_reverse: bool,
    // Parametric EQ (8-band cascade)
    pub param_eq_bands: [crate::state::ParamEqBand; 8],
    // Standalone pitch shifter
    pub pitch_shift_semi: f32,
    pub pitch_shift_fine: f32,
    pub pitch_shift_mix: f32,
    pub pitch_shift_fbk: f32,
    // Mid/side master knobs (all 0..1)
    pub ms_mid_gain: f32,
    pub ms_mid_tilt: f32,
    pub ms_mid_sat: f32,
    pub ms_side_gain: f32,
    pub ms_side_tilt: f32,
    pub ms_side_sat: f32,
    // Compressor
    pub compressor_threshold: f32,
    pub compressor_ratio: f32,
    pub compressor_mix: f32,
    /// See `FxState.compressor_reverse`.  When true, the envelope
    /// follower's attack and release time constants are swapped inside
    /// `Compressor::compress_band`.
    pub compressor_reverse: bool,
    /// See `FxState.compressor_sidechain`.  When true and a sidechain
    /// cable feeds the compressor, the level detector reads the
    /// sidechain signal instead of the input.  Gain reduction still
    /// applies to the input.
    pub compressor_sidechain: bool,
    // Gate / ducker (sidechain FX)
    pub gate_threshold: f32,
    pub gate_attack: f32,
    pub gate_release: f32,
    pub gate_depth: f32,
    pub gate_mix: f32,
    // Vocoder (sidechain FX)
    pub vocoder_bands: f32,
    pub vocoder_carrier_mix: f32,
    pub vocoder_sense: f32,
    pub vocoder_mix: f32,
    // Tape saturation
    pub tape_drive: f32,
    pub tape_mix: f32,
    pub tape_flutter: f32,
    pub master_pitch_st: f32, // -12..+12 semitones added to all melodic voices
    pub filter_mode: crate::state::FilterMode,
    // Sample rate
    pub sample_rate: f32,
    // LFO
    pub lfo: [LfoParamsCopy; 4],
    /// Cable-declared modulation routes — populated from rack Mod cables.
    /// Each route says: "LFO slot N drives target T at depth D".
    pub mod_routes: [ModRouteCopy; MAX_MOD_ROUTES],
    pub mod_route_count: u8,
    pub sequencer_running: bool,
    /// MPE per-note pitch bend, semitones.  Populated from
    /// `AppState.mpe.pitch_bend` × the configured bend range
    /// (currently fixed at ±2 semitones, the GM standard).  Added
    /// to the bass voice's running pitch each block; voice 0 only
    /// for V1 since MPE controllers default to monophonic
    /// expression on the lower zone master.
    pub mpe_bend_st: f32,
    /// MPE channel pressure, 0..=1.  Folded additively into the
    /// bass voice's accent envelope so harder pressure → louder /
    /// brighter on the next note trigger.
    pub mpe_pressure: f32,
    /// MPE timbre (CC74), 0..=1.  Mixed into the bass cutoff with
    /// a small additive offset so a Y-axis push opens the filter
    /// without overriding the user's set cutoff knob entirely.
    pub mpe_timbre: f32,
    pub lfo_pitch_mod_st: f32,
    /// AN1X pitch modulation (semitones) accumulated this block from the
    /// cable-routed mod system (LfoTarget::An1xPitch / opcode 16).  Added
    /// on top of pitch_st in An1xVoice::process.
    pub an1x_pitch_mod_st: f32,
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
    // Theremin — XY-pad-driven sine voice with portamento.
    pub theremin_enabled: bool,
    pub theremin_x: f32,
    pub theremin_y: f32,
    pub theremin_portamento: f32,
    pub theremin_brightness: f32,
    pub theremin_volume: f32,
    pub theremin_pan: f32,
    // Vinyl / cassette FX — surface noise + dull EQ shape.
    pub vinyl_noise: f32,
    pub vinyl_wear: f32,
    pub vinyl_mix: f32,
    // DJ filter — single-knob LP↔BP↔HP morph.
    pub dj_filter_morph: f32,
    pub dj_filter_resonance: f32,
    pub dj_filter_mix: f32,
    // Pendulum — two near-tuned sines that beat acoustically.
    pub pendulum_enabled: bool,
    pub pendulum_base_pitch: f32,
    pub pendulum_detune_hz: f32,
    pub pendulum_mix: f32,
    pub pendulum_volume: f32,
    pub pendulum_pan: f32,
    // FM operator synth — 4-op DX7-flavoured voice.  Per-op fields
    // bundled per-op for cache locality on the audio thread.
    pub fm_ops_enabled: bool,
    pub fm_ops_volume: f32,
    pub fm_ops_pan: f32,
    pub fm_ops_algorithm: u8,
    pub fm_ops_feedback: f32,
    pub fm_ops_op1_ratio: f32,
    pub fm_ops_op1_level: f32,
    pub fm_ops_op1_attack: f32,
    pub fm_ops_op1_decay: f32,
    pub fm_ops_op1_sustain: f32,
    pub fm_ops_op1_release: f32,
    pub fm_ops_op2_ratio: f32,
    pub fm_ops_op2_level: f32,
    pub fm_ops_op2_attack: f32,
    pub fm_ops_op2_decay: f32,
    pub fm_ops_op2_sustain: f32,
    pub fm_ops_op2_release: f32,
    pub fm_ops_op3_ratio: f32,
    pub fm_ops_op3_level: f32,
    pub fm_ops_op3_attack: f32,
    pub fm_ops_op3_decay: f32,
    pub fm_ops_op3_sustain: f32,
    pub fm_ops_op3_release: f32,
    pub fm_ops_op4_ratio: f32,
    pub fm_ops_op4_level: f32,
    pub fm_ops_op4_attack: f32,
    pub fm_ops_op4_decay: f32,
    pub fm_ops_op4_sustain: f32,
    pub fm_ops_op4_release: f32,
    // Additive synth — 16-partial harmonic series with per-harmonic
    // level + voice-wide ADSR.
    pub additive_enabled: bool,
    pub additive_volume: f32,
    pub additive_pan: f32,
    pub additive_levels: [f32; crate::state::ADDITIVE_HARMONICS],
    pub additive_attack: f32,
    pub additive_decay: f32,
    pub additive_sustain: f32,
    pub additive_release: f32,
    // Granular texture
    pub granular_enabled: bool,
    pub granular_volume: f32,
    pub granular_density: f32,
    pub granular_grain_size: f32,
    pub granular_position: f32,
    pub granular_position_jitter: f32,
    pub granular_pitch_scatter: f32,
    pub granular_spray: f32,
    // Karplus-Strong pluck voice
    pub pluck_enabled: bool,
    pub pluck_damping: f32,
    pub pluck_brightness: f32,
    pub pluck_volume: f32,
    pub pluck_pan: f32,
    pub pluck_pitch_offset_semi: f32,
    pub rack_pluck: bool,
    // Wavetable voice
    pub wavetable_enabled: bool,
    pub wavetable_position: f32,
    pub wavetable_phase_offset: f32,
    pub wavetable_volume: f32,
    pub wavetable_pan: f32,
    pub wavetable_pitch_offset_semi: f32,
    pub rack_wavetable: bool,
    // Sample Instrument voice
    pub sample_enabled: bool,
    pub sample_root_note: u8,
    pub sample_volume: f32,
    pub sample_pan: f32,
    pub sample_pitch_offset_cents: f32,
    pub rack_sample: bool,
    pub rack_fm_ops: bool,
    pub rack_additive: bool,
    // ADSR envelope
    pub sample_attack: f32,
    pub sample_decay: f32,
    pub sample_sustain: f32,
    pub sample_release: f32,
    // Loop window
    pub sample_loop_start: f32,
    pub sample_loop_end: f32,
    pub sample_loop_enabled: bool,
    // Per-voice filter (V2 Stage 6)
    pub sample_filter_cutoff: f32,
    pub sample_filter_resonance: f32,
    pub sample_filter_mode: u8,
    pub sample_filter_mix: f32,
    /// Formant-preserving pitch shift opt-in (V2 Stage 8).  When true,
    /// the slot reads the source at rate = 1 and routes through a
    /// per-slot phase-vocoder shifter that does the pitch transform
    /// in the spectral domain with envelope flatten/restore.
    pub sample_formant_preserve: bool,
    /// Time-stretch ratio decoupled from pitch.  1.0 = source's
    /// native tempo.  Engages the spectral processor automatically
    /// when != 1.0 (so the cheap linear-resample path doesn't need
    /// to be flipped manually).  Combined with formant-preserve via
    /// formant ratio = pitch_ratio / time_stretch, so output pitch
    /// stays at the played note while playback speed scales by
    /// time_stretch.
    pub sample_time_stretch: f32,
    /// Mellotron mode opt-in.  When true the slot's playback gains
    /// tape-loop character: per-slot pitch flutter, spin-up
    /// transient on attack, and mild tanh saturation on output.
    pub sample_mellotron_mode: bool,
    /// Flutter depth 0..1 — scales the pitch wobble when
    /// `sample_mellotron_mode` is on.  1.0 → ±~40 cents at the
    /// LFO peak.
    pub sample_mellotron_flutter: f32,
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
    pub an1x_osc1_wave: crate::state::An1xWave,
    pub an1x_osc1_level: f32,
    pub an1x_osc2_wave: crate::state::An1xWave,
    pub an1x_osc2_level: f32,
    pub an1x_osc2_detune: f32, // 0–1 (0.5 = unison)
    pub an1x_osc2_octave: i8,  // -2..+2
    pub an1x_sub_level: f32,
    pub an1x_ring_mod: bool,
    pub an1x_hard_sync: bool,
    pub an1x_filter_cutoff: f32,
    pub an1x_filter_resonance: f32,
    pub an1x_filter_mode: crate::state::FilterMode,
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
    pub an1x_lfo_rate_hz: f32, // Hz (0.01–20)
    pub an1x_lfo_depth: f32,   // 0–1
    pub an1x_lfo_target: crate::state::An1xLfoTarget,
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
    pub amen_slice_count: u8,   // 1 = whole sample; 2/4/8/16 = break-chop
    pub amen_start_offset: f32, // 0..1 of sample
    pub amen_end_offset: f32,   // 0..1 of sample
    pub amen_reverse: bool,
    pub amen_gate: f32,   // 0..1 of slice duration
    pub amen_stutter: u8, // extra retriggers per step
    /// Custom slice start positions (normalized 0..1 of full sample).
    /// Sentinel: NaN in slot [0] = unused, fall back to equal divisions.
    /// Entries 0..amen_slice_count hold explicit start positions, in
    /// ascending order.  Max 16 slices (matches UI cap).
    pub amen_slice_positions: [f32; 16],
    /// Per-slice pitch-shift in semitones (−24..+24, NaN sentinel in slot 0
    /// = unused → all slices share the global amen_pitch).  Additive with
    /// amen_pitch and BPM-stretch, applied at trigger time.
    pub amen_slice_pitches: [f32; 16],
    /// Per-slice volume multiplier (0..2, NaN sentinel in slot 0 = unused
    /// → all slices share the global amen_volume).  Applied multiplicatively.
    pub amen_slice_volumes: [f32; 16],
    /// Per-slice playback direction override.  `-1` in a slot = inherit
    /// the global `amen_reverse` flag; `0` = force forward; `1` = force
    /// reverse.  All slots default to `-1` so unless the user / LLM
    /// populates `AmenState.slice_reverses`, the voice behaves exactly
    /// like the pre-per-slice version.
    pub amen_slice_reverses: [i8; 16],
    /// BPM the source sample was originally recorded at.  Used only when
    /// amen_bpm_stretch is true.
    pub amen_source_bpm: f32,
    /// Stretch sample playback to match the host BPM (non-pitch-preserving —
    /// the sample is resampled, which also shifts its pitch).  For classic
    /// D&B drumbreak treatment, leave this on and accept the pitch shift.
    pub amen_bpm_stretch: bool,
    /// See `AmenState.bpm_stretch_preserve`.  When true alongside
    /// `amen_bpm_stretch`, the voice switches to the granular stretch
    /// path in `AmenVoice::process` instead of the resample-based one.
    pub amen_bpm_stretch_preserve: bool,
    /// Host/sequencer BPM — mirror of s.sequencer.bpm.  Used by the amen
    /// voice for tempo-matching; other voices sync via different paths.
    pub sequencer_bpm: f32,
    // Rack presence — only trigger / process voices that are in the rack
    pub rack_bass: bool,
    pub rack_drums808: bool,
    pub rack_drums909: bool,
    pub rack_amen: bool,
    pub rack_hoover: bool,
    pub rack_an1x: bool,
    pub rack_gabber_kick: bool,
    /// NeuTts output bus gain (0..=1.5).  Scales the TTS ring-buffer
    /// signal before FX and master mixing.  Derived from the first
    /// `TtsModuleState.volume` at frame boundary; exposed as an
    /// `AudioParams` field so it's modulatable via the `NeuTtsVolume`
    /// LFO target like any other DSP knob.
    pub tts_voice_volume: f32,
}
