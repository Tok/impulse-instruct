// ─── audio/dsp/sample_instrument.rs ──────────────────────────────────────────
// Pitched sample-playback voice — load a single recording (or an SFZ
// multisample bank) and replay it at a pitch derived from the played
// sequencer note.
//
// V1.1 added a 4-stage ADSR + loop points + auto-detect-root.  V2:
//   * SFZ multisample mode (Stage 1+2) — per-region sample buffers
//     selected by lokey..=hikey.
//   * Polyphony (Stage 3) — `POLY_VOICES` slots fed by an oldest-steal
//     allocator, so a release tail isn't cut short when the next step
//     fires.
//
// Single-WAV mode + monophonic sequencer playback are preserved
// exactly: each trigger picks a free / oldest slot, gate-off releases
// every gated slot, and the process loop sums all live slots.

use std::sync::Arc;

use super::AudioParams;
use super::dsp_util::{
    ADSR_TIME_MIN_S, ATTACK_HANDOVER_VALUE, AUDIBLE_HZ_MAX, AUDIBLE_HZ_MIN, RELEASE_OFF_VALUE,
    SUSTAIN_REACH_THRESHOLD, db_to_lin, one_pole_coef,
};
use super::dsp_util::{TuningSystem, midi_to_hz_tuned};
use super::formant_shifter::FormantShifter;
use super::fx_extras::{Svf, SvfMode};
use super::sample_instrument_modulation::{
    LfoSlotState, ModEnvState, RegionLfos, RegionModEnv, cb_to_linear_gain,
    cents_to_exact_rate_factor, cents_to_taylor_rate_factor, filter_cents_to_knob_delta,
    region_lfos_from, region_mod_env_from,
};
use crate::state::SfzRegion;

/// One region from a parsed `.sfz` file paired with its pre-loaded mono
/// audio buffer.  Built off the audio thread (UI / API loads + resamples
/// to engine SR), then handed to the voice via
/// `AudioCommand::LoadSampleInstrumentSfz`.  The audio thread reads but
/// never allocates against this list.
#[derive(Clone, Debug)]
pub struct SfzRegionRuntime {
    pub region: SfzRegion,
    pub samples: Arc<Vec<f32>>,
}

/// Polyphony cap.  Default per PLAN.md — 8 slots covers chord stabs,
/// release-tail overlap, and short stutters without over-allocating
/// state.  When every slot is busy, the trigger steals the oldest.
const POLY_VOICES: usize = 8;

/// Per-trigger playback recipe — resolved before slot allocation so the
/// hot-path slot fill is a straight copy.  Kept private; both the
/// single-WAV and SFZ-multi-region trigger paths build one of these.
struct TriggerShape {
    samples: Arc<Vec<f32>>,
    root_freq: f32,
    freq: f32,
    region_gain: f32,
    /// When `Some`, the slot's ADSR uses these region-supplied
    /// values instead of the global `sample_attack` / `sample_decay`
    /// / `sample_sustain` / `sample_release` knobs.  Tuple is
    /// `(attack_s, decay_s, sustain_level_0_to_1, release_s)`.
    /// Single-WAV mode passes `None` so live knob edits keep
    /// applying mid-note.
    region_adsr: Option<(f32, f32, f32, f32)>,
    /// When `Some`, the slot routes through its per-voice SVF with
    /// the region-supplied `(cutoff_knob, resonance_knob, mode)`
    /// instead of the global `sample_filter_*` knobs — and forces
    /// `mix = 1` so an explicit SFZ / SF2 filter applies regardless
    /// of the user's global filter-mix setting.  All values are
    /// pre-converted to the SVF's 0..1 knob range at trigger time.
    region_filter: Option<(f32, f32, SvfMode)>,
    /// Per-region loop override — `(mode, start_sample, end_sample)`.
    /// `Some` = the SFZ / SF2 region carries explicit loop info;
    /// the audio thread uses these bounds + mode and ignores the
    /// global `sample_loop_*` knobs for this slot.  `None` = the
    /// single-WAV / no-region path, which keeps consuming the
    /// global knobs so the user's loop window stays live.
    region_loop: Option<(crate::state::sfz::SfzLoopMode, usize, usize)>,
    /// Per-region LFO config — `(mod_freq_hz, mod_delay_s,
    /// mod_to_pitch_cents, vib_freq_hz, vib_delay_s,
    /// vib_to_pitch_cents)`.  `Some` only when at least one pitch
    /// depth is non-zero (no point running an LFO that doesn't
    /// modulate anything).  V1 wires the pitch targets only.
    region_lfos: Option<RegionLfos>,
    /// Per-region modulation envelope — `Some` only when at least one
    /// of the two depths (`to_pitch_cents`, `to_filter_cents`) is
    /// non-zero.  Mirrors the `region_lfos` activation gate so
    /// regions without a mod env stay on the bit-identical pre-modenv
    /// path.
    region_mod_env: Option<RegionModEnv>,
}

/// Convert a cutoff frequency in Hz into the 0..1 knob value the
/// `Svf` expects — same `20 × 900^knob` log mapping the SVF uses
/// internally, inverted.  Clamped so a hot knob can't scrub past
/// the SVF's audible range.
fn hz_to_svf_knob(hz: f32) -> f32 {
    let h = hz.clamp(AUDIBLE_HZ_MIN, AUDIBLE_HZ_MAX);
    (h / 20.0).log(900.0).clamp(0.0, 1.0)
}

/// Convert a resonance in dB into the 0..1 knob value the `Svf`
/// expects (Q ≈ 0.5..20 mapped linear to 0..1).  Q_linear =
/// 10^(dB / 20); knob = (Q − 0.5) / 19.5.
fn db_to_svf_resonance_knob(db: f32) -> f32 {
    let q_linear = db_to_lin(db.clamp(0.0, 40.0));
    ((q_linear - 0.5) / 19.5).clamp(0.0, 1.0)
}

/// Map an `SfzFilType` to the `Svf::process` mode.  SFZ only defines
/// the three pole-pair filter types (LP/BP/HP); SVF's Notch isn't
/// reachable from an SFZ region.
fn sfz_fil_type_to_svf_mode(t: crate::state::sfz::SfzFilType) -> SvfMode {
    use crate::state::sfz::SfzFilType;
    match t {
        SfzFilType::Lpf2p => SvfMode::Lowpass,
        SfzFilType::Bpf2p => SvfMode::Bandpass,
        SfzFilType::Hpf2p => SvfMode::Highpass,
    }
}

/// ADSR stages tracked per-trigger.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AdsrStage {
    Off,
    Attack,
    Decay,
    Sustain,
    Release,
}

/// Per-voice playback state — one slot in the polyphony pool.  Tracks
/// the active sample buffer, read position, ADSR stage, and an `age`
/// counter the allocator uses to pick the oldest slot when stealing.
struct SampleInstrumentSlot {
    samples: Arc<Vec<f32>>,
    pos: f32,
    root_freq: f32,
    freq: f32,
    adsr_stage: AdsrStage,
    adsr_value: f32,
    gate: bool,
    accent: f32,
    region_gain: f32,
    /// Per-slot ADSR override resolved at trigger time.  `Some` =
    /// region (SFZ / SF2) supplied explicit ampeg values; `None` =
    /// fall through to the global `sample_attack`/`_decay`/`_sustain`
    /// /`_release` knobs (which the single-WAV path uses, keeping
    /// live knob edits responsive).  Tuple matches `TriggerShape`.
    region_adsr: Option<(f32, f32, f32, f32)>,
    /// Per-slot filter override.  Same idiom as `region_adsr` —
    /// `Some` forces the SVF to apply with the region's settings
    /// regardless of the global filter-mix knob; `None` defers to
    /// the global `sample_filter_*` params.
    region_filter: Option<(f32, f32, SvfMode)>,
    /// Per-slot loop override — see `TriggerShape.region_loop`.
    region_loop: Option<(crate::state::sfz::SfzLoopMode, usize, usize)>,
    /// Per-slot SF2 LFO modulation — see `TriggerShape.region_lfos`.
    region_lfos: Option<RegionLfos>,
    /// Running LFO state (mod + vib phases + delay countdowns).
    /// Lives independently of `region_lfos` so two simultaneous
    /// notes don't lock-step their wobble.
    lfo_state: LfoSlotState,
    /// Per-slot SF2 modulation envelope config + running state.
    /// Config is `Some` when the region declares any non-zero depth
    /// target; `mod_env_state` is always live but stays `Off` when
    /// `region_mod_env` is `None` so the per-sample step is a single
    /// match arm in the inactive case.
    region_mod_env: Option<RegionModEnv>,
    mod_env_state: ModEnvState,
    /// Monotonic counter set at trigger time — lowest = oldest slot.
    age: u64,
    /// Per-slot SVF state for the per-voice filter.  Each slot keeps
    /// independent state so polyphonic voices don't bleed integrator
    /// values into each other.  Constant per-process Hz cost — the
    /// SVF is allocation-free.
    filter: Svf,
    /// Per-slot phase-vocoder state for formant-preserving pitch
    /// shift.  Allocated once at voice construction; only used when
    /// `AudioParams.sample_formant_preserve` is on.  When the flag is
    /// off, `process_slot` uses the cheap linear-resample path.
    formant: FormantShifter,
    /// Cached pitch ratio computed at trigger time — used by the
    /// formant-preserve path.  In SFZ mode this absorbs region
    /// transpose / tune cents so the shifter ratio matches the
    /// played note's effective pitch shift relative to the source.
    pitch_ratio: f32,
    /// Per-slot Mellotron tape-flutter LFO phase (0..1).  Random
    /// start phase per trigger so two simultaneous notes don't
    /// flutter in lockstep — the iconic Mellotron warble comes
    /// from per-tape independent wobble.
    flutter_phase: f32,
    /// Per-slot LFO rate jitter (Hz, 1.0..3.0).  Set on trigger so
    /// each voice's wobble has its own pace; together with
    /// `flutter_phase` this defeats the perfect-sync that would
    /// give the bank a synthesised feel.
    flutter_rate_hz: f32,
    /// Spin-up envelope value 0..1 — multiplies into the read
    /// rate when `mellotron_mode` is on.  Resets to 0 on trigger
    /// and ramps to 1 over ~80 ms (motor coming up to speed),
    /// pulling the pitch from "noticeably flat" up to nominal.
    spinup: f32,
}

impl SampleInstrumentSlot {
    fn new() -> Self {
        Self {
            samples: Arc::new(Vec::new()),
            pos: 0.0,
            root_freq: 261.625_56, // C4
            freq: 0.0,
            adsr_stage: AdsrStage::Off,
            adsr_value: 0.0,
            gate: false,
            accent: 0.0,
            region_gain: 1.0,
            region_adsr: None,
            region_filter: None,
            region_loop: None,
            region_lfos: None,
            lfo_state: LfoSlotState::new(),
            region_mod_env: None,
            mod_env_state: ModEnvState::new(),
            age: 0,
            filter: Svf::new(),
            formant: FormantShifter::new(),
            pitch_ratio: 1.0,
            flutter_phase: 0.0,
            flutter_rate_hz: 2.0,
            spinup: 1.0,
        }
    }

    fn is_active(&self) -> bool {
        self.adsr_stage != AdsrStage::Off
    }
}

pub struct SampleInstrumentVoice {
    /// Polyphony pool — `POLY_VOICES` slots, allocator picks free ones
    /// first then steals the oldest when full.
    slots: [SampleInstrumentSlot; POLY_VOICES],
    /// Single-WAV mode buffer.  Empty when nothing is loaded or when
    /// SFZ mode is active.
    single_samples: Arc<Vec<f32>>,
    /// Reference Hz for single-WAV mode (set by `set_root_note`).
    single_root_freq: f32,
    /// SFZ-mode region list.  Empty = single-WAV mode (V1.1 path).
    regions: Vec<SfzRegionRuntime>,
    /// Global trigger counter — incremented per trigger so newer slots
    /// have higher `age`.
    next_age: u64,
    /// Round-robin counter — incremented per trigger.  When a matching
    /// SFZ region has `seq_length > 0`, only the one whose
    /// `seq_position` equals `(rr_counter % seq_length) + 1` (1-indexed
    /// per the SFZ spec) fires.  This is a single global counter — for
    /// instruments with multiple distinct RR groups (different
    /// `seq_length` values), the cycles interleave but the per-group
    /// rotation behaviour is preserved.  Strict per-group counters
    /// would need a HashMap which the audio thread can't allocate.
    rr_counter: u32,
}

impl SampleInstrumentVoice {
    pub fn new() -> Self {
        Self {
            slots: std::array::from_fn(|_| SampleInstrumentSlot::new()),
            single_samples: Arc::new(Vec::new()),
            single_root_freq: 261.625_56, // C4
            regions: Vec::new(),
            next_age: 0,
            rr_counter: 0,
        }
    }

    /// Swap in a freshly-loaded mono sample buffer (single-WAV mode).
    /// Drops any SFZ regions previously loaded — the two modes are
    /// mutually exclusive at any given moment.  In-flight slots are
    /// silenced so the next trigger starts cleanly with the new
    /// buffer.
    pub fn load(&mut self, data: Arc<Vec<f32>>) {
        self.single_samples = data;
        self.regions.clear();
        self.silence_all();
    }

    /// Switch into SFZ multisample mode.  Replaces any active single-WAV
    /// buffer; the next trigger picks the right region by note.  All
    /// regions' sample buffers must already be at the engine sample
    /// rate.
    pub fn load_sfz(&mut self, regions: Vec<SfzRegionRuntime>) {
        self.regions = regions;
        self.silence_all();
    }

    fn silence_all(&mut self) {
        for s in &mut self.slots {
            s.adsr_stage = AdsrStage::Off;
            s.adsr_value = 0.0;
            s.gate = false;
            s.pos = 0.0;
            s.mod_env_state = ModEnvState::new();
        }
    }

    /// True when at least one SFZ region is loaded — the voice is in
    /// multisample mode rather than single-WAV mode.
    pub fn is_sfz_mode(&self) -> bool {
        !self.regions.is_empty()
    }

    /// Borrow the loaded single-WAV buffer (used by the UI thread to
    /// run pitch detection on the source recording).  In SFZ mode this
    /// returns the empty buffer — the per-region buffers are addressed
    /// individually by the caller.
    pub fn samples(&self) -> &Arc<Vec<f32>> {
        &self.single_samples
    }

    /// Update the source-recording root note for single-WAV mode.  In
    /// SFZ mode each region carries its own `pitch_keycenter` so this
    /// is a no-op for that path.
    pub fn set_root_note(&mut self, midi: u8, tuning: TuningSystem) {
        self.single_root_freq = midi_to_hz_tuned(midi, tuning).max(20.0);
    }

    /// Indices of every region matching `note` + `velocity` + the
    /// current round-robin counter.  Velocity layers (lovel..=hivel)
    /// and seq_position/seq_length cycling both apply here — V2 Stage
    /// 5.  Multiple regions can still match (overlapping layers from
    /// Stage 4), so each one fires its own slot.
    fn pick_regions(&self, note: u8, velocity: u8, rr: u32) -> impl Iterator<Item = usize> + '_ {
        self.regions
            .iter()
            .enumerate()
            .filter(move |(_, r)| {
                if !r.region.matches_note(note) {
                    return false;
                }
                if !r.region.matches_velocity(velocity) {
                    return false;
                }
                // Round-robin: only fire when the global counter modulo
                // seq_length matches seq_position (SFZ uses 1-indexed
                // positions).  seq_length = 0 means the region isn't
                // part of an RR group and always fires.
                if r.region.seq_length > 0 {
                    let pos = (rr % r.region.seq_length as u32) as u8 + 1;
                    return r.region.seq_position == pos;
                }
                true
            })
            .map(|(i, _)| i)
    }

    /// Pick a slot to use for the next trigger: prefer any `Off` slot,
    /// otherwise steal the slot with the lowest `age` (oldest).  The
    /// allocator never returns `None` — `POLY_VOICES > 0`, so there's
    /// always a slot.
    fn allocate_slot(&mut self) -> usize {
        if let Some(i) = self.slots.iter().position(|s| !s.is_active()) {
            return i;
        }
        // Steal oldest — `age` is monotonic, lowest = first triggered.
        let mut idx = 0usize;
        let mut min_age = u64::MAX;
        for (i, s) in self.slots.iter().enumerate() {
            if s.age < min_age {
                min_age = s.age;
                idx = i;
            }
        }
        idx
    }

    pub fn trigger(
        &mut self,
        note: u8,
        tuning: TuningSystem,
        accent: f32,
        slide: f32,
        pitch_offset_cents: f32,
        mic_blend: f32,
    ) {
        let _ = slide;

        if !self.regions.is_empty() {
            // SFZ mode: collect every matching region's resolved
            // playback shape, then fire one slot per match.  Overlapping
            // regions (intentional in orchestral patches) all sound
            // together; non-overlapping regions still get one slot
            // each — the typical Salamander-style "one region per
            // key band" case is unchanged.
            //
            // Stack-allocated `[Option<...>; POLY_VOICES]` because the
            // audio thread must stay allocation-free; matches past
            // POLY_VOICES are dropped (the polyphony cap absorbs them
            // if the user's SFZ tries to layer more).
            //
            // Velocity for SFZ filtering: accent maps non-accented
            // (0.0) → vel 64 (mf), full accent (1.0) → vel 127 (fff).
            // Lower-vel layers stay reachable from MIDI / API input
            // (which can pass arbitrary velocities once that path
            // lands); for now the sequencer accent covers the typical
            // mf..fff dynamic.
            let velocity = (64.0 + accent.clamp(0.0, 1.0) * 63.0).round() as u8;
            let rr = self.rr_counter;
            self.rr_counter = self.rr_counter.wrapping_add(1);
            // Map the user's mic_blend knob to the synthetic CC#1
            // value the crossfade math expects.  V1 multi-mic is
            // pinned to CC#1 (mod wheel — the standard SFZ
            // multi-mic convention); a future V2 can route the
            // knob to other CC numbers.
            let mic_cc1 = (mic_blend.clamp(0.0, 1.0) * 127.0).round() as u8;
            let mut pending: [Option<TriggerShape>; POLY_VOICES] = std::array::from_fn(|_| None);
            let mut count = 0usize;
            for idx in self.pick_regions(note, velocity, rr) {
                if count >= POLY_VOICES {
                    break;
                }
                let r = &self.regions[idx];
                let cf_gain = r.region.cc1_crossfade_gain(mic_cc1);
                // Skip silent regions — both the crossfade fully
                // attenuating and a future degenerate xfin/xfout
                // setup land here.  Saves a slot allocation when
                // 6/8 mic positions in a stack are inactive.
                if cf_gain < 1e-4 {
                    continue;
                }
                let region_offset = r.region.transpose as f32 * 100.0 + r.region.tune_cents;
                let cents_ratio = 2.0_f32.powf((pitch_offset_cents + region_offset) / 1200.0);
                let v = r.region.volume_db.clamp(-60.0, 12.0);
                // Region-supplied ADSR override — `Some` only when at
                // least one ampeg field diverges from the SFZ /
                // SF2-default "instant" envelope.  Sustain is stored
                // as 0..100 percent; convert to the 0..1 level the
                // DSP expects.
                let region_adsr = if r.region.ampeg_attack_s > 0.0
                    || r.region.ampeg_decay_s > 0.0
                    || r.region.ampeg_release_s > 0.0
                    || (r.region.ampeg_sustain_pct - 100.0).abs() > 0.01
                {
                    Some((
                        r.region.ampeg_attack_s.max(0.0),
                        r.region.ampeg_decay_s.max(0.0),
                        (r.region.ampeg_sustain_pct.clamp(0.0, 100.0) / 100.0),
                        r.region.ampeg_release_s.max(0.0),
                    ))
                } else {
                    None
                };
                // Per-region filter override.  Build a pre-converted
                // (cutoff_knob, resonance_knob, mode) triple when the
                // region declares an SFZ / SF2 filter; the slot DSP
                // forces mix=1 in that branch so the filter applies
                // even when the user's global mix is at 0.
                let region_filter = match (&r.region.fil_type, r.region.cutoff_hz) {
                    (Some(t), hz) if hz > 20.0 => Some((
                        hz_to_svf_knob(hz),
                        db_to_svf_resonance_knob(r.region.resonance_db),
                        sfz_fil_type_to_svf_mode(*t),
                    )),
                    _ => None,
                };
                // Per-region loop override.  `Some` only when the
                // region declares a real loop mode (Continuous /
                // Sustain) AND the SHDR / SFZ supplied a usable
                // loop window.  NoLoop / OneShot fall back to None
                // so the playback path treats the slot as a one-shot
                // (read to sample end, then enter Release).
                let buf_n = r.samples.len();
                let region_loop = match r.region.loop_mode {
                    crate::state::sfz::SfzLoopMode::LoopContinuous
                    | crate::state::sfz::SfzLoopMode::LoopSustain => {
                        let ls = r.region.loop_start.unwrap_or(0) as usize;
                        let le = r
                            .region
                            .loop_end
                            .map(|v| v as usize)
                            .unwrap_or(buf_n.saturating_sub(1));
                        if le > ls + 1 && le < buf_n {
                            Some((r.region.loop_mode, ls, le))
                        } else {
                            None
                        }
                    }
                    _ => None,
                };
                // SF2 modulation surface — pure builders return `Some`
                // only when at least one depth target reaches the
                // activation threshold.  Activation gate logic +
                // value clamps live in the modulation sibling so
                // they're testable in isolation.
                let region_lfos = region_lfos_from(&r.region);
                let region_mod_env = region_mod_env_from(&r.region);
                pending[count] = Some(TriggerShape {
                    samples: r.samples.clone(),
                    root_freq: midi_to_hz_tuned(r.region.pitch_keycenter, tuning).max(20.0),
                    freq: (midi_to_hz_tuned(note, tuning) * cents_ratio).max(20.0),
                    region_gain: db_to_lin(v) * cf_gain,
                    region_adsr,
                    region_filter,
                    region_loop,
                    region_lfos,
                    region_mod_env,
                });
                count += 1;
            }
            for shape in pending.iter().take(count).flatten() {
                self.fire_slot(shape, accent);
            }
        } else {
            let cents_ratio = 2.0_f32.powf(pitch_offset_cents / 1200.0);
            let shape = TriggerShape {
                samples: self.single_samples.clone(),
                root_freq: self.single_root_freq,
                freq: (midi_to_hz_tuned(note, tuning) * cents_ratio).max(20.0),
                region_gain: 1.0,
                // Single-WAV mode keeps the V1 behaviour: the slot's
                // ADSR reads from the global knobs each sample, so
                // turning the attack knob mid-note is audible
                // immediately.  No region overrides apply here.
                region_adsr: None,
                region_filter: None,
                region_loop: None,
                region_lfos: None,
                region_mod_env: None,
            };
            self.fire_slot(&shape, accent);
        }
    }

    /// Stamp `shape` onto a fresh slot, allocating + bumping the age
    /// counter.  Centralised so single-WAV and per-region SFZ paths
    /// stay in lock-step.
    fn fire_slot(&mut self, shape: &TriggerShape, accent: f32) {
        let i = self.allocate_slot();
        self.next_age = self.next_age.wrapping_add(1);
        let slot = &mut self.slots[i];
        slot.samples = shape.samples.clone();
        slot.root_freq = shape.root_freq;
        slot.freq = shape.freq;
        slot.region_gain = shape.region_gain;
        slot.region_adsr = shape.region_adsr;
        slot.region_filter = shape.region_filter;
        slot.region_loop = shape.region_loop;
        slot.region_lfos = shape.region_lfos;
        slot.region_mod_env = shape.region_mod_env;
        // Reset LFO phases + delay countdowns on every trigger so
        // each new note starts the LFO cycle from zero.
        match shape.region_lfos {
            Some(lfos) => slot.lfo_state.trigger(&lfos),
            None => slot.lfo_state = LfoSlotState::new(),
        }
        // Reset the modulation envelope.  When the region carries a
        // mod env the state machine enters Delay (or Attack if the
        // delay is zero); otherwise it stays Off and the per-sample
        // step short-circuits.
        if let Some(env) = shape.region_mod_env {
            slot.mod_env_state.trigger(&env);
        } else {
            slot.mod_env_state = ModEnvState::new();
        }
        slot.pos = 0.0;
        slot.adsr_stage = AdsrStage::Attack;
        slot.adsr_value = 0.0;
        slot.gate = true;
        slot.accent = accent.clamp(0.0, 1.0);
        slot.age = self.next_age;
        // Cache the pitch ratio + reset the formant shifter so the
        // new note starts with clean state.  When formant_preserve
        // is on, `process_slot` reads the source at rate=1 and
        // routes through this shifter; otherwise it's untouched.
        slot.pitch_ratio = (shape.freq / shape.root_freq).clamp(0.05, 64.0);
        slot.formant.reset();
        // Mellotron-mode init: randomise the flutter LFO so two
        // simultaneous notes don't wobble in lockstep, and reset
        // the spin-up envelope so the new note starts slightly
        // flat and ramps to nominal pitch.  Cheap pseudo-random
        // from the slot's own age + pos pointer — no allocations,
        // no per-process RNG state.
        let r = (self.next_age.wrapping_mul(0x9E37_79B9_7F4A_7C15)) as u32;
        slot.flutter_phase = (r as f32 / u32::MAX as f32).fract();
        slot.flutter_rate_hz = 1.0 + ((r >> 16) as f32 / u16::MAX as f32) * 2.0;
        slot.spinup = 0.0;
    }

    /// Release every currently-gated slot.  Slots already in Release
    /// (post-gate-off, decaying) keep their tail; only slots with
    /// `gate = true` flip into Release.  Matches the
    /// monophonic-sequencer behaviour from V1.1 — the sequencer's
    /// SampleGateOff event still works as expected, while polyphony
    /// adds the option to overlap notes via API/MIDI before the
    /// gate-off arrives.
    pub fn gate_off(&mut self) {
        for s in &mut self.slots {
            if s.gate {
                s.gate = false;
                if s.adsr_stage != AdsrStage::Off {
                    s.adsr_stage = AdsrStage::Release;
                }
                // Mod envelope releases independently of the volume
                // envelope so its own decay tail can outlast (or be
                // shorter than) the audible release.
                s.mod_env_state.release();
            }
        }
    }

    /// Advance one slot's ADSR by one sample.  When the slot carries
    /// a `region_adsr` override (SFZ / SF2 path), the four times +
    /// sustain level come from the region; otherwise the global
    /// `sample_attack`/`_decay`/`_sustain`/`_release` knobs win
    /// (single-WAV path, with live knob edits).
    fn step_adsr(slot: &mut SampleInstrumentSlot, sr: f32, p: &AudioParams) {
        let knob_to_secs = |knob: f32, lo: f32, hi: f32| -> f32 {
            let k = knob.clamp(0.0, 1.0);
            (lo + (hi - lo) * k).max(ADSR_TIME_MIN_S)
        };
        let (attack_s, decay_s, sustain, release_s) = match slot.region_adsr {
            Some((a, d, s, r)) => (
                a.max(ADSR_TIME_MIN_S),
                d.max(ADSR_TIME_MIN_S),
                s.clamp(0.0, 1.0),
                r.max(ADSR_TIME_MIN_S),
            ),
            None => (
                knob_to_secs(p.sample_attack, 0.0005, 1.5),
                knob_to_secs(p.sample_decay, 0.005, 2.0),
                p.sample_sustain.clamp(0.0, 1.0),
                knob_to_secs(p.sample_release, 0.005, 2.0),
            ),
        };
        match slot.adsr_stage {
            AdsrStage::Off => {
                slot.adsr_value = 0.0;
            }
            AdsrStage::Attack => {
                let coef = one_pole_coef(attack_s, sr);
                slot.adsr_value = 1.0 - (1.0 - slot.adsr_value) * coef;
                if slot.adsr_value >= ATTACK_HANDOVER_VALUE {
                    slot.adsr_value = 1.0;
                    slot.adsr_stage = AdsrStage::Decay;
                }
            }
            AdsrStage::Decay => {
                let coef = one_pole_coef(decay_s, sr);
                slot.adsr_value = sustain + (slot.adsr_value - sustain) * coef;
                if (slot.adsr_value - sustain).abs() < SUSTAIN_REACH_THRESHOLD {
                    slot.adsr_value = sustain;
                    slot.adsr_stage = AdsrStage::Sustain;
                }
            }
            AdsrStage::Sustain => {
                slot.adsr_value += (sustain - slot.adsr_value) * 0.001;
            }
            AdsrStage::Release => {
                let coef = one_pole_coef(release_s, sr);
                slot.adsr_value *= coef;
                if slot.adsr_value < RELEASE_OFF_VALUE {
                    slot.adsr_value = 0.0;
                    slot.adsr_stage = AdsrStage::Off;
                }
            }
        }
    }

    /// Run one slot for one sample, returning its contribution to the
    /// summed voice output.  Inactive slots return 0 cheaply.
    fn process_slot(slot: &mut SampleInstrumentSlot, sr: f32, p: &AudioParams) -> f32 {
        let n = slot.samples.len();
        if n < 2 || slot.adsr_stage == AdsrStage::Off {
            return 0.0;
        }
        Self::step_adsr(slot, sr, p);

        // Three read-rate / pitch-shift modes:
        //
        //   * Cheap path (formant_preserve OFF, time_stretch == 1.0):
        //     read at `pitch_ratio`, no spectral processing.  Pitch
        //     and tempo are coupled — V1.1 behaviour.
        //
        //   * Formant-preserve path (formant_preserve ON,
        //     time_stretch == 1.0): read at rate=1, FormantShifter at
        //     `pitch_ratio` does the spectral pitch shift with
        //     envelope flatten/restore.
        //
        //   * Time-stretch path (time_stretch != 1.0): read at
        //     `time_stretch` and FormantShifter at
        //     `pitch_ratio / time_stretch`.  Reading at rate r already
        //     pitch-shifts the input by r, so the shifter compensates
        //     to land on the played note's pitch_ratio.  Net effect:
        //     playback duration scales by 1/time_stretch while output
        //     pitch stays at the played note.  Engages the spectral
        //     processor automatically — the cheap path can't decouple
        //     pitch from speed.
        let stretch_active = (p.sample_time_stretch - 1.0).abs() > 0.001;
        let use_spectral = p.sample_formant_preserve || stretch_active;
        let (mut rate, shift_ratio) = if !use_spectral {
            (slot.pitch_ratio, 1.0)
        } else if !stretch_active {
            (1.0, slot.pitch_ratio)
        } else {
            let ts = p.sample_time_stretch.clamp(0.25, 4.0);
            (ts, slot.pitch_ratio / ts)
        };

        // Mellotron mode: per-slot pitch flutter + spin-up
        // transient.  Modulates the read rate directly so the
        // wobble lands without going through the spectral
        // processor (cheaper, and matches the analog source).
        if p.sample_mellotron_mode {
            // Spin-up envelope: ~80 ms exponential ramp from 0 → 1.
            // Multiplies into the rate so each note starts ~half
            // a semitone flat and rises to nominal — the iconic
            // "motor coming up to speed" gesture.  With a one-pole
            // lowpass shape the audible glide is more natural than
            // a linear ramp.
            let spinup_t = 0.080_f32; // 80 ms
            let spinup_coef = one_pole_coef(spinup_t, sr);
            slot.spinup = 1.0 + (slot.spinup - 1.0) * spinup_coef;
            // Cap pitch dip at ~6 % (about a semitone) so the
            // ramp is audible but not jarring.
            let spinup_factor = 0.94 + 0.06 * slot.spinup.clamp(0.0, 1.0);
            rate *= spinup_factor;

            // Flutter LFO.  Triangle is gentler than sine for
            // tape-wobble; our use of `(2.0 * (phase - 0.5)).abs()`
            // produces a 0..1 triangle that we centre on 0.
            slot.flutter_phase += slot.flutter_rate_hz / sr;
            if slot.flutter_phase >= 1.0 {
                slot.flutter_phase -= 1.0;
            }
            let tri = 2.0 * (slot.flutter_phase - 0.5).abs() - 0.5; // -0.5..+0.5
            // Depth knob 0..1 → ±~40 cents at peak.  cents → ratio:
            // 2^(cents/1200).  Approximate via the Taylor series
            // 1 + cents * ln(2)/1200 since the deviation is small.
            let depth = p.sample_mellotron_flutter.clamp(0.0, 1.0);
            let cents = tri * 80.0 * depth; // ±40 cents at depth=1
            let flutter_factor = 1.0 + cents * 0.000_577_6; // ln(2)/1200
            rate *= flutter_factor;
        }

        // SF2 LFO modulation — mod LFO drives three targets (pitch /
        // filter cutoff / volume), vib LFO drives pitch only.  Both
        // LFOs have a per-spec delay (silent until elapsed) + their
        // own frequency.  `step()` returns the raw sine values so
        // the filter + volume stages downstream can read them
        // without re-evaluating the phase.  Independent state per
        // slot keeps polyphonic voices from lock-stepping.
        let (mod_lfo_value, vib_lfo_value) = match slot.region_lfos {
            Some(lfos) => slot.lfo_state.step(&lfos, sr),
            None => (0.0, 0.0),
        };
        if let Some(lfos) = slot.region_lfos {
            // Pitch target — mod + vib depths sum into one cents
            // offset.  Taylor-style conversion is fine here since the
            // depths are clamped to ±1200 cents at construction.
            let total_cents =
                mod_lfo_value * lfos.mod_to_pitch_cents + vib_lfo_value * lfos.vib_to_pitch_cents;
            rate *= cents_to_taylor_rate_factor(total_cents);
        }

        // SF2 modulation envelope — five-stage AHDSR shared between
        // pitch + filter targets.  `step` returns a 0..1 value that
        // the depth fields scale into cents.  Done once per sample
        // here so both the rate-modulation block above and the
        // filter-cutoff block below read the same instantaneous env.
        // Pitch uses exact `2^(cents/1200)` rather than the LFO's
        // Taylor approximation because the env can swing ±12000
        // cents (10 octaves), well outside the small-angle range.
        let mod_env_value = match slot.region_mod_env {
            Some(env) => slot.mod_env_state.step(&env, sr),
            None => 0.0,
        };
        if let Some(env) = slot.region_mod_env {
            let env_pitch_cents = mod_env_value * env.to_pitch_cents;
            if env_pitch_cents.abs() > 0.05 {
                rate *= cents_to_exact_rate_factor(env_pitch_cents);
            }
        }

        // Resolve loop bounds + active flag.  Per-region override
        // (SFZ / SF2 path) wins when present; falls back to the
        // global `sample_loop_*` knobs for the single-WAV path.
        // LoopSustain mode releases the loop hold once the slot's
        // gate flips false — at that point the read head spills past
        // `le` and the trailing tail plays through to sample end
        // (the standard SF2 "loop until release" semantics).
        let (ls, le, loop_active) = if let Some((mode, rl_s, rl_e)) = slot.region_loop {
            let mode_loops = match mode {
                crate::state::sfz::SfzLoopMode::LoopContinuous => true,
                crate::state::sfz::SfzLoopMode::LoopSustain => slot.gate,
                _ => false,
            };
            (
                rl_s.min(n.saturating_sub(2)),
                rl_e.min(n.saturating_sub(1)),
                mode_loops && rl_e > rl_s + 1,
            )
        } else {
            let ls = (p.sample_loop_start.clamp(0.0, 1.0) * n as f32) as usize;
            let le_raw = (p.sample_loop_end.clamp(0.0, 1.0) * n as f32) as usize;
            let le = le_raw.min(n.saturating_sub(1));
            (ls, le, p.sample_loop_enabled && le > ls + 1)
        };

        let pos_idx = (slot.pos as usize).min(n - 1);
        let frac = slot.pos - pos_idx as f32;
        let s0 = slot.samples[pos_idx];
        let s1 = slot.samples[(pos_idx + 1).min(n - 1)];
        let sample = s0 + (s1 - s0) * frac;

        slot.pos += rate;
        if loop_active {
            let end_f = le as f32;
            let start_f = ls as f32;
            while slot.pos >= end_f {
                slot.pos -= end_f - start_f;
            }
        } else if slot.pos >= n as f32 - 1.0 {
            slot.pos = n as f32 - 1.0;
            if slot.adsr_stage != AdsrStage::Release {
                slot.adsr_stage = AdsrStage::Release;
            }
        }

        // Apply spectral pitch shift (formant-preserving + optional
        // time-stretch compensation) before ADSR + accent so the
        // envelope shapes the post-shift signal — matches the V1.1
        // path's tone (filter / amp envelope last).
        let voiced = if use_spectral {
            slot.formant.process(sample, shift_ratio)
        } else {
            sample
        };

        let accent_gain = 1.0 + slot.accent * 0.4;
        // Mellotron tape saturation: gentle tanh shaping that
        // compresses transients and adds even-order warmth.
        // Off when the flag isn't set so non-Mellotron sessions
        // are bit-identical to the V1.1 path.
        let voiced = if p.sample_mellotron_mode {
            (voiced * 1.4).tanh() * 0.85
        } else {
            voiced
        };
        // modLfoToVolume — symmetric tremolo around the carrier
        // amplitude.  Centibels of attenuation polarity matches
        // FluidSynth: positive depth boosts at the LFO peak.  Skip
        // the powf when the depth is inert so the no-mod path
        // stays bit-identical to V1.
        let vol_lfo_gain = match slot.region_lfos {
            Some(lfos) if lfos.mod_to_volume_cb.abs() > 0.5 => {
                cb_to_linear_gain(mod_lfo_value * lfos.mod_to_volume_cb)
            }
            _ => 1.0,
        };
        let dry = voiced * slot.adsr_value * accent_gain * slot.region_gain * vol_lfo_gain;
        // Per-voice SVF — applied before the global volume.  Three
        // shapes:
        //   * Region filter override (`Some`): force-apply the SVF
        //     with the region's cutoff / resonance / mode at mix=1.
        //     SF2 / SFZ regions that explicitly declare a filter
        //     should always shape the sound regardless of the
        //     user's global mix knob.  When the region's mod LFO
        //     also targets the cutoff, the knob is offset linearly
        //     by `mod_value × depth_cents × C` (closed-form: the
        //     SVF's `hz = 20 × 900^knob` mapping turns a cents
        //     shift into a constant-knob delta — derivation below).
        //   * Global filter active (`p.sample_filter_mix > 0`):
        //     V1.1 behaviour — knob-driven shared filter.  No SF2
        //     mod-LFO-to-filter applies here; the SF2 generator
        //     targets the region's filter only.
        //   * Otherwise: bypass.  Integrator state stays at rest
        //     so the cost of a bypassed filter is one branch.
        if let Some((cutoff, resonance, mode)) = slot.region_filter {
            // Both the mod LFO (sin × depth) and the mod envelope
            // (env × depth) contribute additive cents that get
            // converted to a 0..1 knob delta via the closed-form
            // `filter_cents_to_knob_delta` helper — no `powf` per
            // sample, so the filter modulation cost is flat.
            let lfo_filter_cents = match slot.region_lfos {
                Some(lfos) => mod_lfo_value * lfos.mod_to_filter_cents,
                None => 0.0,
            };
            let env_filter_cents = match slot.region_mod_env {
                Some(env) => mod_env_value * env.to_filter_cents,
                None => 0.0,
            };
            let filter_cents = lfo_filter_cents + env_filter_cents;
            let modulated_cutoff = if filter_cents.abs() > 0.5 {
                (cutoff + filter_cents_to_knob_delta(filter_cents)).clamp(0.0, 1.0)
            } else {
                cutoff
            };
            slot.filter
                .process(dry, modulated_cutoff, resonance, 0.0, mode, 1.0, sr)
        } else if p.sample_filter_mix > 0.001 {
            slot.filter.process(
                dry,
                p.sample_filter_cutoff,
                p.sample_filter_resonance,
                0.0,
                SvfMode::from_u8(p.sample_filter_mode),
                p.sample_filter_mix,
                sr,
            )
        } else {
            dry
        }
    }

    pub fn process(&mut self, sr: f32, p: &AudioParams) -> f32 {
        let mut out = 0.0;
        for slot in &mut self.slots {
            out += Self::process_slot(slot, sr, p);
        }
        // Apply the user's volume knob once at the sum stage so the
        // polyphony scale is consistent with V1.1 (volume scales the
        // total output, not each slot).
        out * p.sample_volume.clamp(0.0, 1.5)
    }

    /// Number of currently-active slots (envelope past Off).  Read once
    /// per audio callback and surfaced to the UI poly-meter; tests also
    /// use it to assert the stealing path.
    pub fn active_voice_count(&self) -> usize {
        self.slots.iter().filter(|s| s.is_active()).count()
    }

    /// Total polyphony cap — exposed for tests that want to verify the
    /// stealing path runs once every slot is busy.
    #[cfg(test)]
    pub const POLY_VOICES: usize = POLY_VOICES;
}

impl Default for SampleInstrumentVoice {
    fn default() -> Self {
        Self::new()
    }
}
