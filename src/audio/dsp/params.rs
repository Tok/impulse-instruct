// ─── audio/dsp/params.rs ──────────────────────────────────────────────────────
// AudioParams snapshot — copied from AppState for the audio thread.
// This module has no side effects; all types are Copy/Clone.

/// Maximum number of cable-declared modulation routes the audio thread
/// will process per block.  Bounded so the array stays Copy-friendly.
pub const MAX_MOD_ROUTES: usize = 32;

/// Per-region amen slice array cap.  16 matches the UI's slice-count
/// selector (1/2/4/8/16) and the SF2 generator slot count.  Used as
/// the size of every `amen_slice_*` array in `AudioParams` and as the
/// `take(MAX_AMEN_SLICES)` cutoff when populating them from
/// variable-length state vecs.
pub const MAX_AMEN_SLICES: usize = 16;

/// Cable-declared modulation route (Copy).  Says: "the modulation
/// value at `cv_buf[source_buf_idx]` drives target opcode T at
/// depth D".  Compiled from rack Mod cables in
/// `AudioParams::from_app_state`.
///
/// `source_buf_idx` is allocated per-source-kind by the compile
/// pass.  Layout (see `MOD_BUF_SIZE`):
///   * 0..4 = LFO slots (in rack order)
///   * 4..8 = CV sequencer slots (in rack order)
///   * 8.. = utility module outputs (slew / quantizer / …) once
///     the V2 utility modules ship.
#[derive(Clone, Copy, Debug, Default)]
pub struct ModRouteCopy {
    /// Index into `AudioParams.cv_buf` of the source modulation
    /// value.  See module-layout doc on the field above.
    pub source_buf_idx: u8,
    pub target_u8: u8, // opcode from `lfo_target_to_u8`
    pub depth: f32,    // bipolar depth multiplier
}

/// Size of the per-block modulation source buffer.  Indices 0..4
/// hold LFO slot values; 4..8 hold CV sequencer values; the
/// utility module ranges (Slew / Quantizer / Comparator / Math /
/// S&H / TriggerDiv / LogicGate / FunctionGen / Crossfader) start
/// at 8 and walk up by `MOD_UTIL_SLOTS` per kind.  Bumped to 64 in
/// the LogicGate ship to leave headroom for the next utilities.
pub const MOD_BUF_SIZE: usize = 64;
/// Buf index where the LFO source range starts.
pub const MOD_BUF_LFO_BASE: usize = 0;
/// Buf index where the CV sequencer source range starts.
pub const MOD_BUF_CV_SEQ_BASE: usize = 4;
/// Number of slots reserved per utility kind.  Each rack instance
/// of a given utility kind maps to one slot in this range, in
/// rack order; the 5th instance stacks on the last slot.
pub const MOD_UTIL_SLOTS: usize = 4;
/// Slew utility output range starts here.
pub const MOD_BUF_SLEW_BASE: usize = 8;
/// Quantizer utility output range starts here.
pub const MOD_BUF_QUANTIZER_BASE: usize = 12;
/// Comparator utility output range starts here.
pub const MOD_BUF_COMPARATOR_BASE: usize = 16;
/// Sample-and-hold utility output range starts here.
pub const MOD_BUF_SAMPLE_HOLD_BASE: usize = 20;
/// Math utility output range starts here.
pub const MOD_BUF_MATH_BASE: usize = 24;
/// TriggerDiv utility output range starts here.
pub const MOD_BUF_TRIGGER_DIV_BASE: usize = 28;
/// LogicGate utility output range starts here.
pub const MOD_BUF_LOGIC_GATE_BASE: usize = 32;
/// FunctionGen utility output range starts here.
pub const MOD_BUF_FUNCTION_GEN_BASE: usize = 36;
/// Crossfader utility output range starts here.
pub const MOD_BUF_CROSSFADER_BASE: usize = 40;

// Per-utility ParamsCopy structs live in `params_utils.rs` (sibling)
// since this file crossed the 1000-line cap during the LogicGate ship.
// Re-exported below so consumers continue to import them from `params`.
pub use super::params_utils::{
    ComparatorParamsCopy, CrossfaderParamsCopy, CvSeqParamsCopy, FunctionGenParamsCopy,
    LfoParamsCopy, LogicGateParamsCopy, MathParamsCopy, QuantizerParamsCopy, SampleHoldParamsCopy,
    SlewParamsCopy, TriggerDivParamsCopy,
};

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
    pub conv_reverb_shimmer: f32,
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
    // CV sequencer slots — step-sequenced modulation.  Each slot's
    // `step_values[current_step % 16]` is read per-block and
    // applied to its target opcode at the configured depth.
    pub cv_seq: [CvSeqParamsCopy; crate::state::CV_SEQ_SLOTS],
    /// Slew utility slots — smooth incoming CV with separate
    /// attack / release time constants.  Read input from
    /// `cv_buf[cv_in_buf_idx]`, write output to
    /// `cv_buf[MOD_BUF_SLEW_BASE + i]`.
    pub slew: [SlewParamsCopy; crate::state::SLEW_SLOTS],
    /// Quantizer utility slots — snap incoming CV (interpreted
    /// as bipolar -1..+1 → -12..+12 semitones) to the nearest
    /// note in the configured scale.
    pub quantizer: [QuantizerParamsCopy; crate::state::QUANTIZER_SLOTS],
    pub comparator: [ComparatorParamsCopy; crate::state::COMPARATOR_SLOTS],
    pub trigger_div: [TriggerDivParamsCopy; crate::state::TRIGGER_DIV_SLOTS],
    pub logic_gate: [LogicGateParamsCopy; crate::state::LOGIC_GATE_SLOTS],
    pub function_gen: [FunctionGenParamsCopy; crate::state::FUNCTION_GEN_SLOTS],
    pub crossfader: [CrossfaderParamsCopy; crate::state::CROSSFADER_SLOTS],
    pub sample_hold: [SampleHoldParamsCopy; crate::state::SAMPLE_HOLD_SLOTS],
    pub math: [MathParamsCopy; crate::state::MATH_SLOTS],
    /// Per-block modulation source buffer.  The audio thread fills
    /// the LFO range (0..4), CV-seq range (4..8), and utility
    /// module ranges (8..32) before applying `mod_routes`.
    /// Allocated as `Default::default()` (zeros) on each block;
    /// stages overwrite their slots before any route reads.
    pub cv_buf: [f32; MOD_BUF_SIZE],
    /// Cable-declared modulation routes — populated from rack Mod cables.
    /// Each route says: "LFO slot N drives target T at depth D".
    pub mod_routes: [ModRouteCopy; MAX_MOD_ROUTES],
    pub mod_route_count: u8,
    pub sequencer_running: bool,
    /// Current sequencer step (0..15 for the canonical bar).
    /// Snapshotted per block; CV sequencer slots index their step
    /// table by `step % CV_SEQ_STEPS` to walk in lock-step with
    /// the audio pattern.
    pub sequencer_current_step: u32,
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
    pub vinyl_transient: f32,
    // DJ filter — single-knob LP↔BP↔HP morph.
    pub dj_filter_morph: f32,
    pub dj_filter_resonance: f32,
    pub dj_filter_mix: f32,
    // Tremolo — internal-LFO amplitude modulation.
    pub tremolo_rate: f32,
    pub tremolo_depth: f32,
    pub tremolo_shape: f32,
    pub tremolo_mix: f32,
    // Vibrato — internal-LFO pitch modulation via a small delay line.
    pub vibrato_rate: f32,
    pub vibrato_depth: f32,
    pub vibrato_shape: f32,
    pub vibrato_mix: f32,
    // 3-band ISO / kill EQ — DJ-style hard-kill bands.
    pub iso_low: f32,
    pub iso_mid: f32,
    pub iso_high: f32,
    pub iso_mix: f32,
    // De-esser — sidechain HP detector + ducker on the sibilant band.
    pub deess_freq: f32,
    pub deess_threshold: f32,
    pub deess_amount: f32,
    pub deess_mix: f32,
    // Resonator bank — six tuned BPF biquads in parallel (chord layer).
    pub resbank_root: f32,
    pub resbank_chord: f32,
    pub resbank_resonance: f32,
    pub resbank_mix: f32,
    // Tape echo — dub-style delay with wow/flutter/sat in the feedback loop.
    pub tape_echo_time: f32,
    pub tape_echo_feedback: f32,
    pub tape_echo_age: f32,
    pub tape_echo_mix: f32,
    // Multiband compressor — 3-band split + 3 independent dynamics.
    pub mb_low_thresh: f32,
    pub mb_mid_thresh: f32,
    pub mb_high_thresh: f32,
    pub mb_mix: f32,
    // Grain delay — granular feedback path (4 overlapping grains).
    pub grain_delay: f32,
    pub grain_size: f32,
    pub grain_scatter: f32,
    pub grain_mix: f32,
    // Spectral gate — per-band amplitude gating across an 8-band BPF bank.
    pub spec_stft: bool,
    pub spec_thresh: f32,
    pub spec_release: f32,
    pub spec_tilt: f32,
    pub spec_mix: f32,
    // Plate reverb — Dattorro figure-of-eight tank.
    pub plate_size: f32,
    pub plate_damping: f32,
    pub plate_diffusion: f32,
    pub plate_mix: f32,
    // Trance gate — 16-cell pattern-driven gate synced to the sequencer clock.
    pub tg_pattern: u16,
    pub tg_rate: u8,
    pub tg_smooth: f32,
    pub tg_mix: f32,
    // Wavefolder — West Coast triangle / sine fold distortion.
    pub wf_drive: f32,
    pub wf_bias: f32,
    pub wf_symmetry: f32,
    pub wf_mix: f32,
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
    // Modal / struck physical model — 8-mode resonator bank.
    pub modal_enabled: bool,
    pub modal_volume: f32,
    pub modal_pan: f32,
    pub modal_levels: [f32; crate::state::MODAL_MODES],
    pub modal_brightness: f32,
    pub modal_decay_scale: f32,
    pub modal_ratio_preset: u8,
    // Chiptune voice — SID-flavoured 3-osc.  Per-osc fields
    // bundled as flat slabs (waveform / level / ADSR each as
    // [u8/f32; 3]) so the audio thread can index them with the
    // osc loop without an indirection per field.
    pub chiptune_enabled: bool,
    pub chiptune_volume: f32,
    pub chiptune_pan: f32,
    pub chiptune_osc_waveform: [u8; crate::state::CHIPTUNE_OSCS],
    pub chiptune_osc_level: [f32; crate::state::CHIPTUNE_OSCS],
    pub chiptune_osc_attack: [f32; crate::state::CHIPTUNE_OSCS],
    pub chiptune_osc_decay: [f32; crate::state::CHIPTUNE_OSCS],
    pub chiptune_osc_sustain: [f32; crate::state::CHIPTUNE_OSCS],
    pub chiptune_osc_release: [f32; crate::state::CHIPTUNE_OSCS],
    pub chiptune_pulse_width: f32,
    pub chiptune_filter_cutoff: f32,
    pub chiptune_filter_resonance: f32,
    pub chiptune_filter_mode: u8,
    pub chiptune_filter_mix: f32,
    pub chiptune_ring_mod: bool,
    pub chiptune_sync: bool,
    // Vocal formant synth — saw source through 3 parallel
    // formant biquads.
    pub vocal_enabled: bool,
    pub vocal_volume: f32,
    pub vocal_pan: f32,
    pub vocal_vowel: u8,
    pub vocal_morph: f32,
    pub vocal_brightness: f32,
    pub vocal_formant_shift: f32,
    pub vocal_attack: f32,
    pub vocal_decay: f32,
    pub vocal_sustain: f32,
    pub vocal_release: f32,
    // Granular texture
    pub granular_enabled: bool,
    pub granular_volume: f32,
    pub granular_density: f32,
    pub granular_grain_size: f32,
    pub granular_position: f32,
    pub granular_position_jitter: f32,
    pub granular_pitch_scatter: f32,
    pub granular_spray: f32,
    pub granular_pitch_mappable: bool,
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
    pub rack_modal: bool,
    pub rack_chiptune: bool,
    pub rack_vocal: bool,
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
    /// Multi-mic blend 0..1 — mapped to a synthetic CC#1 value at
    /// trigger time so SFZ regions with `xfin_*cc1` / `xfout_*cc1`
    /// crossfade opcodes blend across mic positions.
    pub sample_mic_blend: f32,
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
    pub amen_slice_positions: [f32; MAX_AMEN_SLICES],
    /// Per-slice pitch-shift in semitones (−24..+24, NaN sentinel in slot 0
    /// = unused → all slices share the global amen_pitch).  Additive with
    /// amen_pitch and BPM-stretch, applied at trigger time.
    pub amen_slice_pitches: [f32; MAX_AMEN_SLICES],
    /// Per-slice volume multiplier (0..2, NaN sentinel in slot 0 = unused
    /// → all slices share the global amen_volume).  Applied multiplicatively.
    pub amen_slice_volumes: [f32; MAX_AMEN_SLICES],
    /// Per-slice playback direction override.  `-1` in a slot = inherit
    /// the global `amen_reverse` flag; `0` = force forward; `1` = force
    /// reverse.  All slots default to `-1` so unless the user / LLM
    /// populates `AmenState.slice_reverses`, the voice behaves exactly
    /// like the pre-per-slice version.
    pub amen_slice_reverses: [i8; MAX_AMEN_SLICES],
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
