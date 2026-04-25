// ─── audio/dsp/sample_instrument.rs ──────────────────────────────────────────
// Pitched sample-playback voice — load a single recording and replay it
// at a pitch derived from the played sequencer note (relative to a
// configurable root note).
//
// V1.1 adds a 4-stage ADSR envelope, optional loop points, and is set
// up so the surrounding UI / state can drive an auto-detect-root flow
// via the existing `detect_pitch_hz` helper.  V1's behaviour is
// preserved when the new fields take their defaults (no attack, full
// sustain, short release, loop the whole buffer).
//
// V2 (in progress) layers an SFZ multisample mode on top: when a list
// of `SfzRegionRuntime` is loaded, the trigger picks the region whose
// `lokey..=hikey` covers the played note and plays back from that
// region's pre-loaded buffer with `pitch_keycenter` as the resample
// root.  Single-WAV mode (no regions) preserves V1.1 behaviour exactly.

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

/// ADSR stages tracked per-trigger.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AdsrStage {
    Off,
    Attack,
    Decay,
    Sustain,
    Release,
}

pub struct SampleInstrumentVoice {
    /// Loaded mono sample data.  Empty when nothing is loaded.  In SFZ
    /// mode this is a pointer-copy of the active region's buffer,
    /// swapped at trigger time when the played note picks a different
    /// region.
    samples: Arc<Vec<f32>>,
    /// Read position in samples (fractional for linear interpolation).
    pos: f32,
    /// Source-recording reference Hz — set on `load()` from the user's
    /// `root_note` so the pitch ratio is always note-relative.  Defaults
    /// to C4 frequency until a sample is loaded.  In SFZ mode this is
    /// derived per-trigger from the matching region's `pitch_keycenter`.
    root_freq: f32,
    /// Current playback frequency (Hz) — set on trigger.
    freq: f32,
    /// 4-stage envelope state.
    adsr_stage: AdsrStage,
    /// Current envelope amplitude (0..1).
    adsr_value: f32,
    gate: bool,
    /// Per-step accent (0..=1).
    accent: f32,
    /// SFZ-mode region list.  Empty = single-WAV mode (V1.1 path).
    /// Trigger walks this for the first region whose `lokey..=hikey`
    /// covers the played note.  Pre-loaded buffers stay alive for the
    /// voice's lifetime — Arc keeps the swap allocation-free.
    regions: Vec<SfzRegionRuntime>,
    /// Per-trigger gain multiplier (linear) — set when the region's
    /// `volume_db` is applied so the process loop doesn't have to
    /// recompute the dB→linear conversion every sample.
    region_gain: f32,
}

impl SampleInstrumentVoice {
    pub fn new() -> Self {
        Self {
            samples: Arc::new(Vec::new()),
            pos: 0.0,
            root_freq: 261.625_56, // C4 — overwritten when the user sets root_note
            freq: 0.0,
            adsr_stage: AdsrStage::Off,
            adsr_value: 0.0,
            gate: false,
            accent: 0.0,
            regions: Vec::new(),
            region_gain: 1.0,
        }
    }

    /// Swap in a freshly-loaded mono sample buffer (single-WAV mode).
    /// Drops any SFZ regions previously loaded — the two modes are
    /// mutually exclusive at any given moment.  `data` must already be
    /// at the engine sample rate.
    pub fn load(&mut self, data: Arc<Vec<f32>>) {
        self.samples = data;
        self.pos = 0.0;
        self.adsr_stage = AdsrStage::Off;
        self.adsr_value = 0.0;
        self.regions.clear();
        self.region_gain = 1.0;
    }

    /// Switch into SFZ multisample mode.  Replaces any active single-WAV
    /// buffer; the next trigger picks the right region by note.  All
    /// regions' sample buffers must already be at the engine sample
    /// rate.
    pub fn load_sfz(&mut self, regions: Vec<SfzRegionRuntime>) {
        self.regions = regions;
        // Pre-clear playback state so the next trigger starts cleanly
        // — without this an in-flight release tail could continue
        // chewing through a stale buffer until the envelope completes.
        self.samples = Arc::new(Vec::new());
        self.pos = 0.0;
        self.adsr_stage = AdsrStage::Off;
        self.adsr_value = 0.0;
        self.region_gain = 1.0;
    }

    /// True when at least one SFZ region is loaded — the voice is in
    /// multisample mode rather than single-WAV mode.
    pub fn is_sfz_mode(&self) -> bool {
        !self.regions.is_empty()
    }

    /// First region whose key range covers `note`.  V1 is single-active
    /// — velocity layers + round-robin pick the right region among the
    /// matches in Stage 5.  Returns `None` when no region claims the
    /// note (the trigger then plays nothing — silence is the right
    /// outcome for an out-of-range key).
    fn pick_region(&self, note: u8) -> Option<usize> {
        self.regions
            .iter()
            .position(|r| r.region.matches_note(note))
    }

    /// Borrow the loaded buffer (used by the UI thread to run pitch
    /// detection on the source recording).
    pub fn samples(&self) -> &Arc<Vec<f32>> {
        &self.samples
    }

    /// Update the source-recording root note (e.g. user re-tunes the
    /// instrument).  Recalculates `root_freq` so future triggers shift
    /// relative to the new reference.
    pub fn set_root_note(&mut self, midi: u8, tuning: TuningSystem) {
        self.root_freq = midi_to_hz_tuned(midi, tuning).max(20.0);
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
        // SFZ mode: select the matching region first; bail to silence
        // when the played note falls outside every region's range.
        if !self.regions.is_empty() {
            let Some(idx) = self.pick_region(note) else {
                self.adsr_stage = AdsrStage::Off;
                self.adsr_value = 0.0;
                self.gate = false;
                return;
            };
            let r = &self.regions[idx];
            self.samples = r.samples.clone();
            // Region's pitch_keycenter is the resample anchor; tune /
            // transpose offset the played note relative to it.
            let region_offset = r.region.transpose as f32 + r.region.tune_cents / 100.0;
            self.root_freq = midi_to_hz_tuned(r.region.pitch_keycenter, tuning).max(20.0);
            let cents_ratio = 2.0_f32.powf((pitch_offset_cents + region_offset * 100.0) / 1200.0);
            self.freq = (midi_to_hz_tuned(note, tuning) * cents_ratio).max(20.0);
            // dB → linear once per trigger so the per-sample loop is a
            // bare multiply.  SFZ allows positive volumes (rare); we
            // honour them but cap at +12 dB so a malformed file can't
            // blow the master.
            let v = r.region.volume_db.clamp(-60.0, 12.0);
            self.region_gain = 10.0_f32.powf(v / 20.0);
        } else {
            let cents_ratio = 2.0_f32.powf(pitch_offset_cents / 1200.0);
            self.freq = (midi_to_hz_tuned(note, tuning) * cents_ratio).max(20.0);
            self.region_gain = 1.0;
        }
        self.pos = 0.0;
        self.adsr_stage = AdsrStage::Attack;
        self.adsr_value = 0.0;
        self.gate = true;
        self.accent = accent.clamp(0.0, 1.0);
    }

    pub fn gate_off(&mut self) {
        self.gate = false;
        // Snap to release; if we were already past attack, the current
        // value is the starting amplitude for the release ramp.
        if self.adsr_stage != AdsrStage::Off {
            self.adsr_stage = AdsrStage::Release;
        }
    }

    /// Advance the ADSR envelope by one sample.  Time-constant mapping:
    /// attack/decay/release knobs 0..1 → ~0.5..2000 ms.
    fn step_adsr(&mut self, sr: f32, p: &AudioParams) {
        // Per-stage exponential targets.  We use one-pole-style time
        // constants so longer values feel logarithmic rather than
        // linear, matching the rest of the engine's envelope shape.
        let knob_to_secs = |knob: f32, lo: f32, hi: f32| -> f32 {
            let k = knob.clamp(0.0, 1.0);
            (lo + (hi - lo) * k).max(0.0005)
        };
        match self.adsr_stage {
            AdsrStage::Off => {
                self.adsr_value = 0.0;
            }
            AdsrStage::Attack => {
                let t = knob_to_secs(p.sample_attack, 0.0005, 1.5);
                let coef = (-1.0_f32 / (t * sr)).exp();
                self.adsr_value = 1.0 - (1.0 - self.adsr_value) * coef;
                if self.adsr_value >= 0.999 {
                    self.adsr_value = 1.0;
                    self.adsr_stage = AdsrStage::Decay;
                }
            }
            AdsrStage::Decay => {
                let t = knob_to_secs(p.sample_decay, 0.005, 2.0);
                let coef = (-1.0_f32 / (t * sr)).exp();
                let target = p.sample_sustain.clamp(0.0, 1.0);
                self.adsr_value = target + (self.adsr_value - target) * coef;
                if (self.adsr_value - target).abs() < 1e-3 {
                    self.adsr_value = target;
                    self.adsr_stage = AdsrStage::Sustain;
                }
            }
            AdsrStage::Sustain => {
                // Track sustain knob in case the user moves it during a
                // held note — small low-pass to avoid clicks.
                let target = p.sample_sustain.clamp(0.0, 1.0);
                self.adsr_value += (target - self.adsr_value) * 0.001;
            }
            AdsrStage::Release => {
                let t = knob_to_secs(p.sample_release, 0.005, 2.0);
                let coef = (-1.0_f32 / (t * sr)).exp();
                self.adsr_value *= coef;
                if self.adsr_value < 1e-5 {
                    self.adsr_value = 0.0;
                    self.adsr_stage = AdsrStage::Off;
                }
            }
        }
    }

    pub fn process(&mut self, sr: f32, p: &AudioParams) -> f32 {
        let n = self.samples.len();
        if n < 2 || self.adsr_stage == AdsrStage::Off {
            return 0.0;
        }

        // Run the envelope first so the sample read sees the current
        // amplitude — gate-off transitions handled in `gate_off()`.
        self.step_adsr(sr, p);

        // Resample rate: how many source samples per output sample.
        let rate = (self.freq / self.root_freq).clamp(0.05, 64.0);

        // Resolve loop window in sample indices.  loop_end ≤ loop_start
        // implicitly disables the loop (treat as one-shot).
        let ls = (p.sample_loop_start.clamp(0.0, 1.0) * n as f32) as usize;
        let le_raw = (p.sample_loop_end.clamp(0.0, 1.0) * n as f32) as usize;
        let le = le_raw.min(n.saturating_sub(1));
        let loop_active = p.sample_loop_enabled && le > ls + 1;

        // Linear-interp read at `pos`.  Clamp to ±1 of buffer end so we
        // never index past `n - 1` even with float rounding.
        let pos_idx = (self.pos as usize).min(n - 1);
        let frac = self.pos - pos_idx as f32;
        let s0 = self.samples[pos_idx];
        let s1 = self.samples[(pos_idx + 1).min(n - 1)];
        let sample = s0 + (s1 - s0) * frac;

        // Advance position; wrap or stop based on loop state.
        self.pos += rate;
        if loop_active {
            let end_f = le as f32;
            let start_f = ls as f32;
            while self.pos >= end_f {
                self.pos -= end_f - start_f;
            }
        } else if self.pos >= n as f32 - 1.0 {
            // One-shot — pin at end and let the envelope decay to zero.
            self.pos = n as f32 - 1.0;
            if self.adsr_stage != AdsrStage::Release {
                self.adsr_stage = AdsrStage::Release;
            }
        }

        let accent_gain = 1.0 + self.accent * 0.4;
        sample * self.adsr_value * accent_gain * self.region_gain * p.sample_volume.clamp(0.0, 1.5)
    }
}

impl Default for SampleInstrumentVoice {
    fn default() -> Self {
        Self::new()
    }
}
