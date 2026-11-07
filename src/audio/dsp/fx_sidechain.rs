// ─── audio/dsp/fx_sidechain.rs ───────────────────────────────────────────────
// Sidechain-driven FX: gate / ducker and vocoder.  Lifted into its own
// file because `fx_extras.rs` was already at ~991 lines and the sidechain
// trio (Gate / Vocoder + Compressor sidechain mode) is a coherent slice.
//
// Also hosts `SidechainSnap`, the stack-friendly per-sample sidechain
// route snapshot the audio thread carries through `apply_fx_chain`.
// Living next to the sidechain DSP keeps the "what reads sidechain
// signals" set co-located.
//
// Both structs follow the same shape as the rest of the FX in
// `fx.rs` / `fx_extras.rs`: pub(crate) struct, allocation-free
// `process()`, all coefficients computed per-sample so parameter
// automation feels live.  The Compressor's sidechain mode is a flag
// added to the existing struct in `fx.rs`, not a new type.

use super::dsp_util::{MIX_BYPASS_THRESHOLD, db_to_lin};
use crate::state::{FxStep, ModuleKind, SidechainSource};

/// Max sidechain routes the audio thread tracks per block.  4 covers
/// every reasonable patch (one route per sidechain-capable FX kind).
pub(crate) const MAX_SIDECHAIN: usize = 4;

/// Stack-friendly sidechain-route snapshot.  Each entry carries:
/// - `targets[i]`: the `FxStep` whose sidechain port we're feeding;
/// - `sources[i]`: which kind of source the sidechain reads from;
/// - `voice_signals[i]`: the resolved current-sample voice signal (for
///   voice-source routes).  FX-source routes read from
///   `prev_fx_output` directly — this slot is unused.
///
/// Built once per sample after voices process and before FX run.  The
/// `apply_fx_chain` lookup is a linear scan — at typical sizes (0–4)
/// that's faster than a hash probe.
#[derive(Clone, Copy)]
pub(crate) struct SidechainSnap {
    pub(crate) targets: [FxStep; MAX_SIDECHAIN],
    pub(crate) sources: [SidechainSource; MAX_SIDECHAIN],
    pub(crate) voice_signals: [f32; MAX_SIDECHAIN],
    pub(crate) count: usize,
}

impl SidechainSnap {
    pub(crate) fn empty() -> Self {
        Self {
            targets: [FxStep::Waveshaper; MAX_SIDECHAIN],
            sources: [SidechainSource::Voice(ModuleKind::AcidBass); MAX_SIDECHAIN],
            voice_signals: [0.0; MAX_SIDECHAIN],
            count: 0,
        }
    }

    /// Refresh `voice_signals` from the per-bus values computed earlier
    /// in the sample.  Each entry whose source is `Voice(kind)` looks up
    /// the matching bus signal; FX-source entries are untouched (they
    /// read `prev_fx_output` directly inside `apply_fx_chain`).
    /// Buses for kinds outside the standard voice list collapse to 0 —
    /// surfaces as a user-fixable mis-routing.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn refresh_voice_signals(
        &mut self,
        bus_bass: f32,
        bus_808: f32,
        bus_909: f32,
        bus_hoover: f32,
        bus_pluck: f32,
        bus_wavetable: f32,
        bus_sample: f32,
        bus_an1x: f32,
        bus_amen: f32,
        bus_noise: f32,
        bus_granular: f32,
    ) {
        for i in 0..self.count {
            if let SidechainSource::Voice(kind) = self.sources[i] {
                self.voice_signals[i] = match kind {
                    ModuleKind::AcidBass => bus_bass,
                    ModuleKind::DrumKit808 => bus_808,
                    ModuleKind::DrumKit909 => bus_909,
                    ModuleKind::HooverLead => bus_hoover,
                    ModuleKind::PluckString => bus_pluck,
                    ModuleKind::WavetableVoice => bus_wavetable,
                    ModuleKind::SampleInstrument => bus_sample,
                    ModuleKind::An1xVoice => bus_an1x,
                    ModuleKind::AmenSampler => bus_amen,
                    ModuleKind::NoiseVoice => bus_noise,
                    ModuleKind::GranularTexture => bus_granular,
                    _ => 0.0,
                };
            }
        }
    }
}

// ─── Gate / ducker ───────────────────────────────────────────────────────────
//
// Two roles in one DSP block:
// 1. **Sidechain ducker** (sidechain cable connected): the detector
//    follows the *sidechain* signal — when it rises above the threshold,
//    the main signal is attenuated by `depth`.  Classic "kick ducks
//    pad" patch — kick → sidechain, pad → main.
// 2. **Noise gate** (sidechain unconnected): the detector follows the
//    *main* signal.  Below threshold the gain falls to `1 - depth`,
//    above threshold it rises to unity.  At depth = 1 a strict gate; at
//    depth = 0 a no-op.
//
// Smoothing uses asymmetric one-pole envelopes (fast attack, slow release
// — same shape as the existing `compress_band` in `fx.rs`).

pub(crate) struct Gate {
    /// Detector envelope state — the smoothed |sidechain| (or |main| when
    /// the sidechain port is unconnected).
    detect_env: f32,
    /// Smoothed gain factor we're currently applying to the main signal,
    /// trailing the threshold-driven target through attack / release.
    gain_env: f32,
}

impl Gate {
    pub(crate) fn new() -> Self {
        Self {
            detect_env: 0.0,
            gain_env: 1.0,
        }
    }

    /// `threshold`: 0–1 → −60..0 dBFS (level at which the gate opens).
    /// `attack`: 0–1 → 0.5..50 ms.  Time for the gate to open / close.
    /// `release`: 0–1 → 10..500 ms.  Time for gain to recover / decay.
    /// `depth`: 0–1.  How far the gain is pulled below unity when closed
    /// (0 = inactive, 1 = full mute when below threshold).
    /// `mix`: 0–1 wet/dry blend between the gated signal and the dry
    /// input.  At mix = 0 the FX is bypassed.
    /// `sidechain`: external detector signal.  When the FX has no
    /// sidechain cable connected, callers pass the main signal so the
    /// gate self-detects (noise-gate flavour).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn process(
        &mut self,
        input: f32,
        sidechain: f32,
        threshold: f32,
        attack: f32,
        release: f32,
        depth: f32,
        mix: f32,
        sr: f32,
    ) -> f32 {
        if mix < MIX_BYPASS_THRESHOLD || depth < 0.001 {
            return input;
        }
        // Detector envelope on the sidechain.  Attack/release are matched
        // to the gain envelope below — keeping them on the same time
        // constant keeps the gate's apparent reaction smooth.
        let att_ms = 0.5 + attack.clamp(0.0, 1.0) * 49.5;
        let rel_ms = 10.0 + release.clamp(0.0, 1.0) * 490.0;
        let att = (-1.0 / (sr * att_ms * 0.001)).exp();
        let rel = (-1.0 / (sr * rel_ms * 0.001)).exp();
        let level = sidechain.abs();
        self.detect_env = if level > self.detect_env {
            self.detect_env * att + level * (1.0 - att)
        } else {
            self.detect_env * rel + level * (1.0 - rel)
        };
        // Threshold is given as 0..1; map to −60..0 dBFS to match the
        // detector's linear amplitude.
        let thresh_db = -60.0 * (1.0 - threshold.clamp(0.0, 1.0));
        let thresh_lin = db_to_lin(thresh_db);
        let target = if self.detect_env > thresh_lin {
            1.0
        } else {
            (1.0 - depth.clamp(0.0, 1.0)).max(0.0)
        };
        // Asymmetric smoothing on the gain itself so the audible rise /
        // fall matches the user's attack / release knobs.
        let gain_alpha = if target > self.gain_env {
            1.0 - att
        } else {
            1.0 - rel
        };
        self.gain_env += gain_alpha * (target - self.gain_env);
        let wet = input * self.gain_env;
        input * (1.0 - mix) + wet * mix
    }
}

// ─── Vocoder ─────────────────────────────────────────────────────────────────
//
// Channel vocoder.  `BAND_COUNT` log-spaced bandpass pairs split the
// modulator (sidechain) and carrier (main) into per-band streams.  The
// modulator's envelope follower drives the gain on the matching carrier
// band; summed bands form the output.  Classic talkbox patch: TTS into
// sidechain, synth into main.
//
// 16 bands log-spaced from 100 Hz to 8 kHz is the canonical setting —
// enough resolution for intelligible vocals without heroic CPU.

pub(crate) const VOCODER_BAND_COUNT: usize = 16;

#[derive(Clone, Copy)]
struct VocBp {
    /// State-variable filter state — `lp` + `bp` recurrence pair, same
    /// topology as `Svf` in `fx_extras.rs`.  We only read the bandpass
    /// output here; the lowpass stage is needed for the recurrence.
    lp: f32,
    bp: f32,
    /// Cached `f` coefficient — computed once at construction and reused
    /// every sample (sample rate doesn't change at runtime).
    f: f32,
}

impl VocBp {
    fn new(center_hz: f32, sr: f32) -> Self {
        // SVF prewarp: f = 2 * sin(π * fc / sr).  Same form used by
        // `Svf::process` — Q is fixed inside the vocoder so we burn it
        // into a single resonance constant below.
        let f = (std::f32::consts::PI * center_hz / sr).sin() * 2.0;
        Self {
            lp: 0.0,
            bp: 0.0,
            f: f.clamp(0.0, 1.0),
        }
    }

    /// Returns the bandpass output for the next sample.  Q is fixed at
    /// ~3 (q_inv = 0.33) so 16 bands at log spacing give sensible
    /// overlap without huge ringing.
    fn step(&mut self, x: f32) -> f32 {
        let q_inv = 0.33;
        let hp = x - self.lp - self.bp * q_inv;
        self.bp += self.f * hp;
        self.lp += self.f * self.bp;
        self.bp
    }
}

pub(crate) struct Vocoder {
    /// Per-band carrier filters (read main signal).
    carrier: [VocBp; VOCODER_BAND_COUNT],
    /// Per-band modulator filters (read sidechain).
    modulator: [VocBp; VOCODER_BAND_COUNT],
    /// Per-band envelope follower on the modulator's bandpass output.
    env: [f32; VOCODER_BAND_COUNT],
}

impl Vocoder {
    pub(crate) fn new(sr: f32) -> Self {
        // Log-space band centres from 100 Hz → 8 kHz.  Standard channel
        // vocoder layout — the low end captures fundamental + first
        // formants, the high end captures sibilance.
        let lo_hz = 100.0_f32;
        let hi_hz = 8000.0_f32;
        let log_lo = lo_hz.ln();
        let log_hi = hi_hz.ln();
        let mut carrier = [VocBp::new(0.0, sr); VOCODER_BAND_COUNT];
        let mut modulator = [VocBp::new(0.0, sr); VOCODER_BAND_COUNT];
        for i in 0..VOCODER_BAND_COUNT {
            let t = i as f32 / (VOCODER_BAND_COUNT - 1) as f32;
            let center = (log_lo + (log_hi - log_lo) * t).exp();
            carrier[i] = VocBp::new(center, sr);
            modulator[i] = VocBp::new(center, sr);
        }
        Self {
            carrier,
            modulator,
            env: [0.0; VOCODER_BAND_COUNT],
        }
    }

    /// `bands_active`: 0..1 — fraction of bands actually used.  At
    /// `bands_active * BAND_COUNT < 1` the FX is effectively bypassed
    /// (no bands contribute) — callers may want to force-bypass at
    /// `mix < 0.001` for a strict bypass.
    /// `carrier_mix`: 0..1 — how much dry carrier passes alongside the
    /// vocoded signal (talkbox flavour rises with this; pure vocoder
    /// stays near 0).
    /// `sense`: 0..1 — modulator envelope sensitivity (essentially a
    /// detector gain knob).
    /// `mix`: 0..1 wet/dry blend.
    /// `sidechain`: modulator signal.  When unconnected the caller
    /// passes the main signal — the vocoder then self-modulates, which
    /// behaves like a band-driven multi-resonant filter (mostly
    /// useless, but doesn't crash).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn process(
        &mut self,
        input: f32,
        sidechain: f32,
        bands_active: f32,
        carrier_mix: f32,
        sense: f32,
        mix: f32,
        sr: f32,
    ) -> f32 {
        if mix < MIX_BYPASS_THRESHOLD {
            return input;
        }
        // Envelope follower time constant — fixed at 20 ms attack / 80 ms
        // release.  Picked by ear: fast enough to track speech consonants,
        // slow enough that band gains don't crackle on transients.
        let att = (-1.0 / (sr * 0.02)).exp();
        let rel = (-1.0 / (sr * 0.08)).exp();
        let bands_active = bands_active.clamp(0.0, 1.0);
        let used: usize = ((bands_active * VOCODER_BAND_COUNT as f32).round() as usize)
            .clamp(1, VOCODER_BAND_COUNT);
        let sense_gain = 0.5 + sense.clamp(0.0, 1.0) * 4.5;
        let mut wet = 0.0;
        for i in 0..used {
            let mod_bp = self.modulator[i].step(sidechain) * sense_gain;
            let car_bp = self.carrier[i].step(input);
            let level = mod_bp.abs();
            self.env[i] = if level > self.env[i] {
                self.env[i] * att + level * (1.0 - att)
            } else {
                self.env[i] * rel + level * (1.0 - rel)
            };
            wet += car_bp * self.env[i];
        }
        // Per-band normalisation — divide by the count of active bands
        // so the wet level stays in the same ballpark as the dry,
        // regardless of how many bands the user enabled.  Empirical
        // 0.5× post-scaler matches the dry input level on a sine
        // carrier + sine modulator at Q ≈ 3.
        let wet = wet * 0.5 / used as f32;
        let blended = wet + input * carrier_mix.clamp(0.0, 1.0);
        input * (1.0 - mix) + blended * mix
    }
}
