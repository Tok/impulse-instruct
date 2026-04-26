// ─── audio/dsp/mod.rs ── Pure DSP synthesis, no allocations in process_block()

pub mod additive;
pub mod an1x;
pub mod bass303;
pub mod chiptune;
pub mod conv_reverb;
mod dsp_util;
pub mod fm_ops;
pub mod formant_shifter;
pub mod fx;
pub mod fx_deesser;
pub mod fx_djfilter;
pub mod fx_extras;
pub mod fx_freq_shift;
pub mod fx_glitch;
pub mod fx_grain_delay;
pub mod fx_iso_eq;
pub mod fx_math;
pub mod fx_mb_comp;
pub mod fx_plate;
pub mod fx_resbank;
pub mod fx_sidechain;
pub mod fx_spectral_gate;
mod fx_step;
pub mod fx_tape_echo;
pub mod fx_tremolo;
pub mod fx_vibrato;
pub mod fx_vinyl;
pub mod gabber_kick;
pub mod granular_voice;
mod lfo_target_opcode;
pub mod mod_apply;
mod mod_compile;
pub mod modal;
pub mod ms_master;
pub mod param_eq;
mod params;
mod params_from;
pub mod pendulum;
pub mod pitch_shift;
pub mod pluck;
mod process_block;
mod rev_tap;
pub mod sample_instrument;
pub mod samplers;
pub mod theremin;
mod trigger_handler;
pub mod vocal;
pub mod voices;
pub mod wavetable;
use additive::AdditiveVoice;
use an1x::An1xVoice;
use bass303::Bass303;
use chiptune::ChiptuneVoice;
use conv_reverb::ConvReverb;
use dsp_util::*;
pub use dsp_util::{TuningSystem, hz_to_midi, midi_to_hz, midi_to_hz_tuned};
use fm_ops::FmOpsVoice;
use fx::*;
use fx_djfilter::DjFilter;
use fx_extras::*;
use fx_freq_shift::FreqShift;
use fx_glitch::*;
use modal::ModalVoice;
use vocal::VocalVoice;
// fx_math symbols (free_eg_value_at, lfo_value_at, sidechain_duck,
// sidechain_envelope_step, gated_reverb_envelope_step) are only used
// inside the extracted `process_block.rs`; pulled in there directly.
use fx_deesser::DeEsserFx;
use fx_grain_delay::GrainDelayFx;
use fx_iso_eq::IsoEqFx;
use fx_mb_comp::MultibandCompFx;
use fx_plate::PlateFx;
use fx_resbank::ResBankFx;
use fx_sidechain::{Gate, Vocoder};
use fx_spectral_gate::SpectralGateFx;
use fx_tape_echo::TapeEchoFx;
use fx_tremolo::TremoloFx;
use fx_vibrato::VibratoFx;
use fx_vinyl::VinylFx;
use gabber_kick::GabberKick;
use granular_voice::GranularVoice;
pub use lfo_target_opcode::lfo_target_to_u8;
pub use mod_compile::{
    compile_comparator_params, compile_math_params, compile_mod_routes, compile_quantizer_params,
    compile_sample_hold_params, compile_slew_params,
};
use ms_master::MsMaster;
use param_eq::ParamEq;
pub use params::{
    AudioParams, MAX_MOD_ROUTES, MOD_BUF_COMPARATOR_BASE, MOD_BUF_CV_SEQ_BASE, MOD_BUF_LFO_BASE,
    MOD_BUF_MATH_BASE, MOD_BUF_QUANTIZER_BASE, MOD_BUF_SAMPLE_HOLD_BASE, MOD_BUF_SIZE,
    MOD_BUF_SLEW_BASE, MOD_UTIL_SLOTS,
};
use pendulum::PendulumVoice;
use pitch_shift::PitchShift;
use pluck::PluckVoice;
pub use rev_tap::{FxDirection, FxRevQuant};
use sample_instrument::{SampleInstrumentVoice, SfzRegionRuntime};
use samplers::*;
use theremin::ThereminVoice;
use voices::*;
use wavetable::WavetableVoice;

use crate::state::{FX_STEP_COUNT, FxPlan, FxStep};

use rev_tap::REV_BUF_LEN;

/// Max FX steps per chain snapshot — sized to the union of every voice's
/// chain + the global chain.  Truncated at snap time on overflow.
pub(super) const MAX_CHAIN: usize = 16;
/// Max parallel sends per voice.  Three = dry + reverb + delay-style
/// split, with headroom for a third colour chain.
pub(super) const MAX_SENDS: usize = 3;

/// Stack-friendly per-voice send snapshot — mirrors `FxPlan.voice_routes`
/// on the audio thread without HashMap / Vec touches in the frame loop.
#[derive(Clone, Copy)]
pub(super) struct VoiceSendsSnap {
    pub(super) chains: [[FxStep; MAX_CHAIN]; MAX_SENDS],
    pub(super) chain_lens: [usize; MAX_SENDS],
    pub(super) gains: [f32; MAX_SENDS],
    pub(super) count: usize,
}
// `SidechainSnap` + MAX_SIDECHAIN now live in `fx_sidechain.rs`.

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
    gabber_kick: GabberKick,
    // FX
    reverb: Reverb,
    conv_reverb: ConvReverb,
    param_eq: ParamEq,
    pitch_shift: PitchShift,
    ms_master: MsMaster,
    delay: DelayLine,
    chorus: Chorus,
    phaser: Phaser,
    flanger: Flanger,
    limiter: Limiter,
    svf: Svf,
    comb: CombRes,
    tilt: Tilt,
    transient: Transient,
    exciter: Exciter,
    multitap: Multitap,
    rev_delay: RevDelay,
    tape_stop: TapeStop,
    stutter: Stutter,
    freeze: Freeze,
    gate: Gate,
    vocoder: Vocoder,
    freq_shift: FreqShift,
    vinyl: VinylFx,
    dj_filter: DjFilter,
    tremolo: TremoloFx,
    vibrato: VibratoFx,
    iso_eq: IsoEqFx,
    deesser: DeEsserFx,
    resbank: ResBankFx,
    tape_echo: TapeEchoFx,
    mb_comp: MultibandCompFx,
    grain_delay: GrainDelayFx,
    spectral_gate: SpectralGateFx,
    plate: PlateFx,
    bitcrush_held: f32,
    bitcrush_counter: u32,
    // FX state
    compressor: Compressor,
    tape_sat: TapeSat,
    autotune: Autotune,
    ring_mod_phase: f32,
    eq: EqBands,
    noise_voice: NoiseVoice,
    theremin: ThereminVoice,
    pendulum: PendulumVoice,
    fm_ops: FmOpsVoice,
    additive: AdditiveVoice,
    modal: ModalVoice,
    chiptune: ChiptuneVoice,
    vocal: VocalVoice,
    granular: GranularVoice,
    hoover: HooverVoice,
    pluck: PluckVoice,
    wavetable: WavetableVoice,
    sample_instrument: SampleInstrumentVoice,
    an1x: An1xVoice,
    amen: AmenVoice,
    // LFO state
    lfo_phases: [f32; 4],
    lfo_sh_held: [f32; 4],
    /// Cached Slew utility output value per slot.  Tracks the
    /// smoothed target across blocks so the rise / fall envelope
    /// is continuous rather than restarting at each callback.
    slew_state: [f32; crate::state::SLEW_SLOTS],
    /// Cached Sample-and-hold latch per slot.
    sample_hold_state: [f32; crate::state::SAMPLE_HOLD_SLOTS],
    /// Last sequencer step seen by `process_block`.  Used by S&H
    /// to detect step transitions (the "clock edge").
    prev_seq_step: u32,
    lfo_noise: NoiseGen,
    free_eg_phase: f32, // 0..1 through the 8-step EG period
    free_eg_done: bool, // true after one-shot completes
    prev_running: bool,
    // Per-voice velocity (set on trigger, applied to voice output)
    drum_velocity: [f32; 15],
    // Current params
    params: AudioParams,
    sample_rate: f32,
    // Compiled FX routing plan (updated via AudioCommand::SetFxPlan)
    fx_plan: FxPlan,
    // Gated reverb envelope: 1.0 = open, 0.0 = closed. Tracks transient
    // detection; decays to 0 at a rate set by p.reverb_gate_time.
    reverb_gate_env: f32,
    /// One-sample delay line per FxStep.  Written by `apply_fx_chain`
    /// after each FX runs; read by the same function on the next sample
    /// to realise FX→FX feedback routes (back-edges in the rack graph).
    /// Indexed by `fx_step_idx(step)` — dense array so no hash lookups
    /// in the audio callback.
    prev_fx_output: [f32; FX_STEP_COUNT],
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
    /// FxPan LFO phase (0..1) and last-sample side contribution.  The
    /// phase advances every time the Pan FxStep runs; `fx_pan_side` is
    /// read in the master stereo mix and decayed back to 0 when the
    /// step is inactive.
    fx_pan_phase: f32,
    fx_pan_side: f32,
    /// FxWiden master-stage latch.  The chain step is a passthrough
    /// that flips this true with the latest knob amounts; the master
    /// stage reads + applies them, then resets `fx_widen_active` for
    /// the next sample.  Same idiom as `fx_pan_side`.  Haas delay buf
    /// is a small ring on the L channel — sized to MAX_HAAS_SAMPLES at
    /// 48 kHz × 30 ms × headroom.
    fx_widen_active: bool,
    fx_widen_haas_amt: f32,
    fx_widen_side_amt: f32,
    fx_widen_mix_amt: f32,
    fx_widen_haas_buf: Vec<f32>,
    fx_widen_haas_pos: usize,
    /// Mid/side `FxParamEq` cascades — instantiated once, used only
    /// when `param_eq_ms_mode` is active and the chain's ParamEq step
    /// runs.  When idle, they stay zero-state and don't drift; their
    /// state initialises lazily on first use.  The chain ParamEq step
    /// is a passthrough in M/S mode so the master gets the dry signal
    /// to decode.
    param_eq_mid: ParamEq,
    param_eq_side: ParamEq,
    param_eq_ms_active: bool,
}

/// Maximum Haas delay length in samples — 48 kHz × 50 ms (headroom
/// past the user-facing 0..30 ms range).  Heap-allocated once at
/// `DspState::new` so per-block paths stay allocation-free.
pub(super) const FX_WIDEN_HAAS_MAX_SAMPLES: usize = 2400;

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
            gabber_kick: GabberKick::new(0xab12),
            reverb: Reverb::new(),
            conv_reverb: ConvReverb::new(),
            param_eq: ParamEq::new(),
            pitch_shift: PitchShift::new(),
            ms_master: MsMaster::new(),
            delay: DelayLine::new(),
            chorus: Chorus::new(),
            phaser: Phaser::new(),
            flanger: Flanger::new(),
            limiter: Limiter::new(),
            svf: Svf::new(),
            comb: CombRes::new(),
            tilt: Tilt::new(sample_rate),
            transient: Transient::new(),
            exciter: Exciter::new(),
            multitap: Multitap::new(),
            rev_delay: RevDelay::new(),
            tape_stop: TapeStop::new(),
            stutter: Stutter::new(),
            freeze: Freeze::new(),
            gate: Gate::new(),
            vocoder: Vocoder::new(sample_rate),
            freq_shift: FreqShift::new(),
            vinyl: VinylFx::new(sample_rate),
            dj_filter: DjFilter::new(),
            tremolo: TremoloFx::new(),
            vibrato: VibratoFx::new(),
            iso_eq: IsoEqFx::new(),
            deesser: DeEsserFx::new(),
            resbank: ResBankFx::new(),
            tape_echo: TapeEchoFx::new(),
            mb_comp: MultibandCompFx::new(),
            grain_delay: GrainDelayFx::new(),
            spectral_gate: SpectralGateFx::new(),
            plate: PlateFx::new(sample_rate),
            compressor: Compressor::new(),
            tape_sat: TapeSat::new(),
            autotune: Autotune::new(),
            ring_mod_phase: 0.0,
            eq: EqBands::new(sample_rate),
            bitcrush_held: 0.0,
            bitcrush_counter: 0,
            noise_voice: NoiseVoice::new(0x4015_EB3D),
            theremin: ThereminVoice::new(),
            pendulum: PendulumVoice::new(),
            fm_ops: FmOpsVoice::new(),
            additive: AdditiveVoice::new(),
            modal: ModalVoice::new(),
            chiptune: ChiptuneVoice::new(),
            vocal: VocalVoice::new(),
            granular: GranularVoice::new(0xBEEF_CAFE),
            hoover: HooverVoice::new(),
            pluck: PluckVoice::new(),
            wavetable: WavetableVoice::new(),
            sample_instrument: SampleInstrumentVoice::new(),
            an1x: An1xVoice::new(),
            amen: AmenVoice::new(),
            lfo_phases: [0.0; 4],
            lfo_sh_held: [0.0; 4],
            slew_state: [0.0; crate::state::SLEW_SLOTS],
            sample_hold_state: [0.0; crate::state::SAMPLE_HOLD_SLOTS],
            prev_seq_step: u32::MAX,
            lfo_noise: NoiseGen::new(0xCAFE_BABE),
            free_eg_phase: 0.0,
            free_eg_done: false,
            drum_velocity: [1.0; 15],
            prev_running: false,
            params: p,
            sample_rate,
            fx_plan,
            reverb_gate_env: 1.0,
            prev_fx_output: [0.0; FX_STEP_COUNT],
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
            fx_pan_phase: 0.0,
            fx_pan_side: 0.0,
            fx_widen_active: false,
            fx_widen_haas_amt: 0.4,
            fx_widen_side_amt: 0.0,
            fx_widen_mix_amt: 0.0,
            fx_widen_haas_buf: vec![0.0; FX_WIDEN_HAAS_MAX_SAMPLES],
            fx_widen_haas_pos: 0,
            param_eq_mid: ParamEq::new(),
            param_eq_side: ParamEq::new(),
            param_eq_ms_active: false,
        }
    }

    pub fn set_fx_plan(&mut self, plan: FxPlan) {
        self.fx_plan = plan;
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

    /// Load mono samples (resampled to engine rate) into the wavetable
    /// voice.  The voice splits the buffer into 2048-sample frames at
    /// load time so the audio thread reads frames without further
    /// allocation.
    pub fn load_wavetable(&mut self, data: std::sync::Arc<Vec<f32>>) {
        self.wavetable.load(data);
    }

    /// Load a single mono buffer into the SampleInstrument voice.
    pub fn load_sample_instrument(&mut self, data: std::sync::Arc<Vec<f32>>) {
        self.sample_instrument.load(data);
    }
    /// Switch the SampleInstrument voice to SFZ multisample mode.
    pub fn load_sample_instrument_sfz(&mut self, regions: Vec<SfzRegionRuntime>) {
        self.sample_instrument.load_sfz(regions);
    }

    /// Live count of active voices in the SampleInstrument poly pool
    /// (0..=POLY_VOICES).  Sampled once per audio callback and surfaced
    /// to the UI poly-meter via an `AtomicU8`.
    pub fn sample_instrument_active(&self) -> u8 {
        self.sample_instrument.active_voice_count() as u8
    }

    /// Load a new impulse response into the convolution reverb.  Called
    /// outside the audio callback (via `AudioCommand::LoadImpulseResponse`)
    /// so allocation/FFT pre-computation in here is fine.  `channels`
    /// is 1 for mono IRs, 2 for interleaved stereo.  `reversed` stores
    /// the IR back-to-front for the classic reverse-reverb effect.
    pub fn load_impulse_response(
        &mut self,
        data: std::sync::Arc<Vec<f32>>,
        channels: u8,
        reversed: bool,
    ) {
        self.conv_reverb.load_ir(data, channels, reversed);
    }

    /// Drop any loaded impulse response — the wet path falls back to the
    /// filter-only Phase 1 behaviour.
    pub fn clear_impulse_response(&mut self) {
        self.conv_reverb.clear_ir();
    }

    // `handle_trigger` lives in `trigger_handler.rs` (extracted to
    // keep this file under the 1000-line cap).

    // `process_block` lives in `process_block.rs` — extracted from
    // this file so it stays under the 1000-line cap.  Same `impl
    // DspState` block, just split across two files.
}
