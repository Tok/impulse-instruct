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
use super::dsp_util::{TuningSystem, midi_to_hz_tuned};
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
#[derive(Clone)]
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
    /// Monotonic counter set at trigger time — lowest = oldest slot.
    age: u64,
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
            age: 0,
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
}

impl SampleInstrumentVoice {
    pub fn new() -> Self {
        Self {
            slots: std::array::from_fn(|_| SampleInstrumentSlot::new()),
            single_samples: Arc::new(Vec::new()),
            single_root_freq: 261.625_56, // C4
            regions: Vec::new(),
            next_age: 0,
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

    /// First region whose key range covers `note`.  V1 is single-active
    /// — velocity layers + round-robin pick the right region among the
    /// matches in Stage 5.  Returns `None` when no region claims the
    /// note.
    fn pick_region(&self, note: u8) -> Option<usize> {
        self.regions
            .iter()
            .position(|r| r.region.matches_note(note))
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
    ) {
        let _ = slide;

        // Resolve the buffer + root + tune + gain for this trigger.
        // SFZ mode: pick a region; bail to silence if none matches.
        // Single-WAV mode: use the global buffer + root.
        let (samples, root_freq, region_offset_cents, region_gain) = if !self.regions.is_empty() {
            let Some(idx) = self.pick_region(note) else {
                return;
            };
            let r = &self.regions[idx];
            let region_offset = r.region.transpose as f32 * 100.0 + r.region.tune_cents;
            let v = r.region.volume_db.clamp(-60.0, 12.0);
            (
                r.samples.clone(),
                midi_to_hz_tuned(r.region.pitch_keycenter, tuning).max(20.0),
                region_offset,
                10.0_f32.powf(v / 20.0),
            )
        } else {
            (self.single_samples.clone(), self.single_root_freq, 0.0, 1.0)
        };

        let cents_ratio = 2.0_f32.powf((pitch_offset_cents + region_offset_cents) / 1200.0);
        let freq = (midi_to_hz_tuned(note, tuning) * cents_ratio).max(20.0);

        let i = self.allocate_slot();
        self.next_age = self.next_age.wrapping_add(1);
        let slot = &mut self.slots[i];
        slot.samples = samples;
        slot.root_freq = root_freq;
        slot.freq = freq;
        slot.region_gain = region_gain;
        slot.pos = 0.0;
        slot.adsr_stage = AdsrStage::Attack;
        slot.adsr_value = 0.0;
        slot.gate = true;
        slot.accent = accent.clamp(0.0, 1.0);
        slot.age = self.next_age;
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
            }
        }
    }

    /// Advance one slot's ADSR by one sample.
    fn step_adsr(slot: &mut SampleInstrumentSlot, sr: f32, p: &AudioParams) {
        let knob_to_secs = |knob: f32, lo: f32, hi: f32| -> f32 {
            let k = knob.clamp(0.0, 1.0);
            (lo + (hi - lo) * k).max(0.0005)
        };
        match slot.adsr_stage {
            AdsrStage::Off => {
                slot.adsr_value = 0.0;
            }
            AdsrStage::Attack => {
                let t = knob_to_secs(p.sample_attack, 0.0005, 1.5);
                let coef = (-1.0_f32 / (t * sr)).exp();
                slot.adsr_value = 1.0 - (1.0 - slot.adsr_value) * coef;
                if slot.adsr_value >= 0.999 {
                    slot.adsr_value = 1.0;
                    slot.adsr_stage = AdsrStage::Decay;
                }
            }
            AdsrStage::Decay => {
                let t = knob_to_secs(p.sample_decay, 0.005, 2.0);
                let coef = (-1.0_f32 / (t * sr)).exp();
                let target = p.sample_sustain.clamp(0.0, 1.0);
                slot.adsr_value = target + (slot.adsr_value - target) * coef;
                if (slot.adsr_value - target).abs() < 1e-3 {
                    slot.adsr_value = target;
                    slot.adsr_stage = AdsrStage::Sustain;
                }
            }
            AdsrStage::Sustain => {
                let target = p.sample_sustain.clamp(0.0, 1.0);
                slot.adsr_value += (target - slot.adsr_value) * 0.001;
            }
            AdsrStage::Release => {
                let t = knob_to_secs(p.sample_release, 0.005, 2.0);
                let coef = (-1.0_f32 / (t * sr)).exp();
                slot.adsr_value *= coef;
                if slot.adsr_value < 1e-5 {
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

        let rate = (slot.freq / slot.root_freq).clamp(0.05, 64.0);

        let ls = (p.sample_loop_start.clamp(0.0, 1.0) * n as f32) as usize;
        let le_raw = (p.sample_loop_end.clamp(0.0, 1.0) * n as f32) as usize;
        let le = le_raw.min(n.saturating_sub(1));
        let loop_active = p.sample_loop_enabled && le > ls + 1;

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

        let accent_gain = 1.0 + slot.accent * 0.4;
        sample * slot.adsr_value * accent_gain * slot.region_gain
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

    /// Number of currently-active slots (envelope past Off).  Used by
    /// tests to assert polyphony.
    #[cfg(test)]
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
