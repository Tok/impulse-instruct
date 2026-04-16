// ─── audio/dsp/params.rs ──────────────────────────────────────────────────────
// AudioParams snapshot — copied from AppState for the audio thread.
// This module has no side effects; all types are Copy/Clone.

use crate::state::{
    An1xLfoTarget, An1xWave, AppState, FilterMode, LfoTarget, LfoWaveform, ModuleKind,
};

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
    // ADSR shaping (101-style).  Defaults preserve 303 behavior.
    pub amp_attack: f32,     // 0–1 → 0–1s
    pub amp_sustain: f32,    // 0–1
    pub amp_release: f32,    // 0–1 → 0–2s
    pub filter_attack: f32,  // 0–1 → 0–0.5s
    pub filter_sustain: f32, // 0–1
    pub filter_release: f32, // 0–1 → 0–2s
    pub pulse_width: f32,    // 0.05..0.95 (centered at 0.5 = square)
    // Per-voice LFO — SH-101 style.  target=0 (Off) disables.
    pub lfo_target: u8,   // 0=Off 1=Pitch 2=PWM 3=Cutoff 4=Amp
    pub lfo_rate: f32,    // 0–1 → 0.01–20 Hz (free)
    pub lfo_depth: f32,   // 0–1
    pub lfo_waveform: u8, // mirror of LfoWaveform index (Sine=1, Tri=2, Saw=3, InvSaw=4, Square=5)
    pub lfo_delay: f32,   // 0–1 → 0–4 s fade-in
    pub lfo_bpm_sync: bool,
    pub lfo_sync_beats: f32,
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
            amp_attack: b.amp_attack.clamp(0.0, 1.0),
            amp_sustain: b.amp_sustain.clamp(0.0, 1.0),
            amp_release: b.amp_release.clamp(0.0, 1.0),
            filter_attack: b.filter_attack.clamp(0.0, 1.0),
            filter_sustain: b.filter_sustain.clamp(0.0, 1.0),
            filter_release: b.filter_release.clamp(0.0, 1.0),
            pulse_width: b.pulse_width.clamp(0.05, 0.95),
            lfo_target: match b.lfo_target {
                crate::state::BassLfoTarget::Off => 0,
                crate::state::BassLfoTarget::Pitch => 1,
                crate::state::BassLfoTarget::PulseWidth => 2,
                crate::state::BassLfoTarget::FilterCutoff => 3,
                crate::state::BassLfoTarget::Amplitude => 4,
            },
            lfo_rate: b.lfo_rate.clamp(0.0, 1.0),
            lfo_depth: b.lfo_depth.clamp(0.0, 1.0),
            lfo_waveform: match b.lfo_waveform {
                crate::state::LfoWaveform::Sine => 1,
                crate::state::LfoWaveform::Triangle => 2,
                crate::state::LfoWaveform::Saw => 3,
                crate::state::LfoWaveform::InvSaw => 4,
                crate::state::LfoWaveform::Square => 5,
                // SampleAndHold not supported by the bass voice LFO yet;
                // fall back to square (closest stepped-ish behavior).
                _ => 5,
            },
            lfo_delay: b.lfo_delay.clamp(0.0, 1.0),
            lfo_bpm_sync: b.lfo_bpm_sync,
            lfo_sync_beats: b.lfo_sync_beats.clamp(0.03125, 16.0),
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
    pub reverb_dir: u8,
    /// Beat-division snap for the reverse-tap loop length.
    pub reverb_rev_quant: u8,
    pub delay_time: f32,
    pub delay_feedback: f32,
    pub delay_mix: f32,
    /// 0=FWD, 1=REV (anti-echo), 2=MIRROR.
    pub delay_dir: u8,
    pub delay_rev_quant: u8,
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
    // Pan FX
    pub fx_pan_pos: f32,
    pub fx_pan_width: f32,
    pub fx_pan_rate: f32,
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
    /// Cable-declared modulation routes — populated from rack Mod cables.
    /// Each route says: "LFO slot N drives target T at depth D".
    pub mod_routes: [ModRouteCopy; MAX_MOD_ROUTES],
    pub mod_route_count: u8,
    pub sequencer_running: bool,
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
    /// BPM the source sample was originally recorded at.  Used only when
    /// amen_bpm_stretch is true.
    pub amen_source_bpm: f32,
    /// Stretch sample playback to match the host BPM (non-pitch-preserving —
    /// the sample is resampled, which also shifts its pitch).  For classic
    /// D&B drumbreak treatment, leave this on and accept the pitch shift.
    pub amen_bpm_stretch: bool,
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
}

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
            reverb_dir: s.fx.reverb_dir.min(2),
            reverb_rev_quant: s.fx.reverb_rev_quant.min(4),
            delay_time: s.fx.delay_time,
            delay_feedback: s.fx.delay_feedback,
            delay_mix: s.fx.delay_mix,
            delay_dir: s.fx.delay_dir.min(2),
            delay_rev_quant: s.fx.delay_rev_quant.min(4),
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
            fx_pan_pos: s.fx.fx_pan_pos.clamp(-1.0, 1.0),
            fx_pan_width: s.fx.fx_pan_width.clamp(0.0, 1.0),
            fx_pan_rate: s.fx.fx_pan_rate.clamp(0.0, 1.0),
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
            amen_source_bpm: s.amen.source_bpm.clamp(40.0, 300.0),
            amen_bpm_stretch: s.amen.bpm_stretch,
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
        }
    }
}
