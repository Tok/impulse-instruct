use serde::{Deserialize, Serialize};

use super::fx_defaults::*;

/// Parametric EQ band shape — shelf or peak.  Serialised as an integer
/// so the LLM schema / API can set it with a number.  The three variants
/// cover every band we care about: low-shelf (cuts/boosts everything
/// below `freq_hz`), bell/peak (Q-shaped boost or cut centred on
/// `freq_hz`), high-shelf (everything above).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ParamEqBandKind {
    LowShelf,
    Peak,
    HighShelf,
}

impl ParamEqBandKind {
    pub fn from_u8(v: u8) -> Self {
        match v {
            0 => Self::LowShelf,
            2 => Self::HighShelf,
            _ => Self::Peak,
        }
    }
    pub fn to_u8(self) -> u8 {
        match self {
            Self::LowShelf => 0,
            Self::Peak => 1,
            Self::HighShelf => 2,
        }
    }
}

/// One parametric-EQ band.  The active set is always 8; individual
/// bands can be `enabled: false` to bypass without losing their
/// stored freq/gain/Q so the user can A/B a band in and out.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct ParamEqBand {
    pub kind: ParamEqBandKind,
    /// Centre / corner frequency in Hz.  Clamped to (20, 20_000).
    pub freq_hz: f32,
    /// Band gain in dB.  Clamped to ±18 dB at DSP time.
    pub gain_db: f32,
    /// Q — band width for Peak, shelf slope for LowShelf / HighShelf.
    /// Clamped to [0.1, 10.0] so the cascade stays stable.
    pub q: f32,
    pub enabled: bool,
}

impl ParamEqBand {
    pub fn new(kind: ParamEqBandKind, freq_hz: f32, q: f32) -> Self {
        Self {
            kind,
            freq_hz,
            gain_db: 0.0,
            q,
            enabled: true,
        }
    }
}

/// Default 8-band parametric EQ layout: two shelves bracketing six
/// peaks spread roughly-octavewise across 100 Hz–15 kHz.  All bands
/// start at 0 dB so the cascade is unity-gain — adding the module
/// doesn't colour the signal until the user moves a node.
pub fn default_param_eq_bands() -> [ParamEqBand; 8] {
    [
        ParamEqBand::new(ParamEqBandKind::LowShelf, 100.0, 0.7),
        ParamEqBand::new(ParamEqBandKind::Peak, 250.0, 1.0),
        ParamEqBand::new(ParamEqBandKind::Peak, 500.0, 1.0),
        ParamEqBand::new(ParamEqBandKind::Peak, 1_000.0, 1.0),
        ParamEqBand::new(ParamEqBandKind::Peak, 2_500.0, 1.0),
        ParamEqBand::new(ParamEqBandKind::Peak, 5_000.0, 1.0),
        ParamEqBand::new(ParamEqBandKind::Peak, 10_000.0, 1.0),
        ParamEqBand::new(ParamEqBandKind::HighShelf, 15_000.0, 0.7),
    ]
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FxState {
    pub reverb_size: f32, // 0–1 room size
    pub reverb_damp: f32, // 0–1 damping
    pub reverb_mix: f32,  // 0–1 wet/dry
    #[serde(default)]
    pub reverb_gate_time: f32, // 0 = no gate; 0.01–2.0 s gate close time (gated reverb)
    #[serde(default)]
    pub reverb_freeze: bool, // true = infinite hold, tail loops indefinitely
    /// Reverb time direction: 0=FWD (normal), 1=REV (preverb — reverb of a
    /// reversed input buffer; sounds like reverb that builds INTO the hit),
    /// 2=MIRROR (sum of forward + reverse).  Reverse and mirror require a
    /// 1 s circular buffer of past input.
    #[serde(default)]
    pub reverb_dir: u8,
    /// Beat division for the REV/MIRROR rewind cycle.  0=free 1 s,
    /// 1=1/4 bar (1 beat), 2=1/2 bar, 3=1 bar, 4=2 bars.  Snaps the
    /// reverse-tap loop length to the active BPM.
    #[serde(default)]
    pub reverb_rev_quant: u8,
    pub delay_time: f32,     // 0–1 → 0–2000 ms
    pub delay_feedback: f32, // 0–1
    pub delay_mix: f32,      // 0–1 wet/dry
    /// Delay time direction: 0=FWD (echoes after the dry hit), 1=REV
    /// (anti-echoes preceding the hit, via reversed input buffer),
    /// 2=MIRROR.
    #[serde(default)]
    pub delay_dir: u8,
    /// Same beat-division snap as `reverb_rev_quant` but for the delay's
    /// reverse tap.
    #[serde(default)]
    pub delay_rev_quant: u8,
    #[serde(default)]
    pub delay_wow_flutter: f32, // 0–1 tape wow/flutter depth
    #[serde(default)]
    pub delay_saturation: f32, // 0–1 tape saturation on feedback
    /// Dub-style infinite hold: when true, the delay line's feedback
    /// sustains at ~1.0 and new input is suppressed, so the echo loop
    /// carries on indefinitely without adding more material.  Mirrors
    /// `reverb_freeze`; paired with `delay_hpf` / `delay_lpf` it
    /// becomes the classic dub send/return chain.
    #[serde(default)]
    pub delay_freeze: bool,
    /// One-pole high-pass filter on the delay's feedback path (0–1).
    /// 0 = bypass; 1 = aggressive low cut that makes each echo
    /// progressively thinner, so the dub loop fades into airy
    /// whispers instead of piling up bass mud.
    #[serde(default)]
    pub delay_hpf: f32,
    /// One-pole low-pass filter on the delay's feedback path (0–1).
    /// 0 = bypass; 1 = heavy high cut (≈100 Hz cutoff) so echoes lose
    /// presence with every round-trip — the classic dub "drift into
    /// smoke" characteristic.
    #[serde(default)]
    pub delay_lpf: f32,
    pub distortion_drive: f32,     // 0–1
    pub distortion_mix: f32,       // 0–1 wet/dry
    pub compressor_threshold: f32, // 0–1 → -40–0 dB
    pub compressor_ratio: f32,     // 0–1 → 1:1–20:1
    pub compressor_mix: f32,       // 0–1 wet/dry (0 = bypassed)
    #[serde(default)]
    pub compressor_multiband: f32, // 0 = single band, >0 = 3-band (low/mid/high)
    /// Swap the envelope follower's attack / release time constants.
    /// Normal: 1 ms attack + 80 ms release clamps transients fast and
    /// releases slowly.  Reverse: 80 ms attack + 1 ms release lets the
    /// initial transient punch through while the sustain gets compressed,
    /// creating a perceived swell-into-hit shape (third FX with a
    /// reversal mode, alongside reverb and delay).
    #[serde(default)]
    pub compressor_reverse: bool,
    /// When true, the compressor's level detector reads from the
    /// sidechain audio input (cable into `PortKind::SidechainIn`)
    /// instead of the main signal.  Gain reduction still applies to
    /// the main signal — classic "kick ducks pad" patch when paired
    /// with a kick → sidechain cable.  Has no audible effect when no
    /// sidechain cable is connected (detector falls back to the main
    /// signal so the compressor behaves identically to non-sidechain
    /// mode).
    #[serde(default)]
    pub compressor_sidechain: bool,
    // ── Gate / ducker (FxGate, sidechain) ─────────────────────────────
    /// Gate threshold 0..1 → −60..0 dBFS.  Detector envelope must rise
    /// above this for the gate to open.
    #[serde(default = "default_gate_threshold")]
    pub gate_threshold: f32,
    /// Gate attack 0..1 → 0.5..50 ms (time for gain to recover when
    /// sidechain crosses threshold).
    #[serde(default = "default_gate_attack")]
    pub gate_attack: f32,
    /// Gate release 0..1 → 10..500 ms (time for gain to decay when
    /// sidechain falls below threshold).
    #[serde(default = "default_gate_release")]
    pub gate_release: f32,
    /// Gate depth 0..1 — fraction of unity gain pulled down when the
    /// gate is closed.  At 1.0 the gate fully mutes the signal; at 0.5
    /// it ducks by 6 dB; at 0 the FX is inactive.
    #[serde(default = "default_gate_depth")]
    pub gate_depth: f32,
    /// Gate wet/dry 0..1 (0 = bypass).
    #[serde(default)]
    pub gate_mix: f32,
    // ── Vocoder (FxVocoder, sidechain) ────────────────────────────────
    /// Vocoder bands-active 0..1 — fraction of the 16-band cascade that
    /// processes audio.  Lower values give a coarser, more "robotic"
    /// vocode; near 1.0 is the canonical full-resolution channel
    /// vocoder.
    #[serde(default = "default_vocoder_bands")]
    pub vocoder_bands: f32,
    /// Vocoder dry-carrier mix 0..1 — talkbox flavour rises with this.
    /// 0 = pure vocoder (band-summed only), 1 = full carrier blended
    /// alongside the vocoded bands.
    #[serde(default)]
    pub vocoder_carrier_mix: f32,
    /// Vocoder modulator-detector sense 0..1 → 0.5..5.0× detector gain.
    /// Higher values exaggerate the modulator's influence (clearer
    /// consonants but more pumping).
    #[serde(default = "default_vocoder_sense")]
    pub vocoder_sense: f32,
    /// Vocoder wet/dry 0..1 (0 = bypass).
    #[serde(default)]
    pub vocoder_mix: f32,
    pub master_volume: f32, // 0–1
    #[serde(default)]
    pub stereo_width: f32, // 0–1: 0=mono, 0.5=normal, 1=wide
    #[serde(default)]
    pub tuning: u8, // 0=12-TET, 1=just intonation, 2=slendro, 3=pelog
    #[serde(default)]
    pub xmod_bass_to_an1x_pitch: f32, // 0–1 bass osc → AN1X pitch FM depth
    #[serde(default)]
    pub xmod_noise_to_filter: f32, // 0–1 noise → bass filter cutoff mod depth
    #[serde(default)]
    pub sidechain_amount: f32, // 0–1 sidechain compression depth (kick ducks bass/pad)
    #[serde(default)]
    pub sidechain_attack: f32, // 0–1 → 0.1–50 ms attack
    #[serde(default)]
    pub sidechain_release: f32, // 0–1 → 10–500 ms release
    pub tape_drive: f32,    // 0–1 saturation amount
    pub tape_mix: f32,      // 0–1 wet/dry
    pub tape_flutter: f32,  // 0–1 wow/flutter depth
    #[serde(default)]
    pub master_pitch_st: f32, // -12..+12 semitones: global pitch offset for melodic voices
    pub bitcrush_bits: f32, // 0–1: 1.0 = full quality (bypass), 0.0 = 1-bit
    pub bitcrush_rate: f32, // 0–1: 0.0 = no decimation, 1.0 = extreme downsampling
    pub bitcrush_mix: f32,  // 0–1: wet/dry
    pub chorus_rate: f32,   // 0–1 → 0.1–8 Hz LFO rate
    pub chorus_depth: f32,  // 0–1 modulation depth
    pub chorus_mix: f32,    // 0–1 wet/dry
    pub phaser_rate: f32,   // 0–1 → 0.05–5 Hz LFO rate
    pub phaser_depth: f32,  // 0–1 sweep depth
    pub phaser_mix: f32,    // 0–1 wet/dry
    /// Flanger LFO rate — 0..1 → 0.05..4 Hz.
    #[serde(default = "default_flanger_rate")]
    pub flanger_rate: f32,
    /// Flanger sweep depth — 0..1 → 0..~9 ms swing on top of a 1 ms base
    /// delay.
    #[serde(default = "default_flanger_depth")]
    pub flanger_depth: f32,
    /// Flanger feedback — bipolar around 0.5 (no feedback).  0.0 →
    /// strong negative feedback (peaks become notches), 1.0 → strong
    /// positive feedback (resonant comb).
    #[serde(default = "default_flanger_feedback")]
    pub flanger_feedback: f32,
    /// Flanger wet/dry — 0..1.
    #[serde(default)]
    pub flanger_mix: f32,
    /// Brick-wall limiter threshold 0..1 → −24..0 dB.
    #[serde(default = "default_limiter_threshold")]
    pub limiter_threshold: f32,
    /// Brick-wall limiter ceiling 0..1 → −12..0 dB.
    #[serde(default = "default_limiter_ceiling")]
    pub limiter_ceiling: f32,
    /// Limiter release 0..1 → 5..500 ms.
    #[serde(default = "default_limiter_release")]
    pub limiter_release: f32,
    /// Limiter lookahead 0..1 → 0.5..10 ms peek window.
    #[serde(default = "default_limiter_lookahead")]
    pub limiter_lookahead: f32,
    /// State-variable filter cutoff 0..1 (log-mapped 20 Hz–18 kHz).
    /// Named `svf_*` to avoid collision with the per-voice `filter_*`
    /// fields (bass / an1x have their own filter knobs).
    #[serde(default = "default_svf_cutoff")]
    pub svf_cutoff: f32,
    /// SVF resonance 0..1 (Q ≈ 0.5..20).
    #[serde(default)]
    pub svf_resonance: f32,
    /// SVF pre-saturation drive 0..1.
    #[serde(default)]
    pub svf_drive: f32,
    /// SVF wet/dry mix 0..1.
    #[serde(default)]
    pub svf_mix: f32,
    /// SVF mode: 0=LP, 1=BP, 2=HP, 3=Notch.
    #[serde(default)]
    pub svf_mode: u8,
    /// Comb resonator pitch 0..1 (40 Hz–2 kHz log).
    #[serde(default = "default_comb_pitch")]
    pub comb_pitch: f32,
    /// Comb feedback 0..1.
    #[serde(default)]
    pub comb_feedback: f32,
    /// Comb damping 0..1 (lowpass on the feedback path).
    #[serde(default)]
    pub comb_damp: f32,
    /// Comb wet/dry mix 0..1.
    #[serde(default)]
    pub comb_mix: f32,
    /// Tilt EQ tilt 0..1 (0.5 = flat, 0 = bass-heavy, 1 = treble-heavy).
    #[serde(default = "default_ms_unity")]
    pub tilt_tilt: f32,
    /// Tilt EQ pivot 0..1 (200 Hz–5 kHz log).
    #[serde(default = "default_tilt_pivot")]
    pub tilt_pivot: f32,
    /// Tilt EQ wet/dry 0..1.
    #[serde(default)]
    pub tilt_mix: f32,
    /// Transient designer attack 0..1 (0.5 = flat, ±12 dB).
    #[serde(default = "default_ms_unity")]
    pub transient_attack: f32,
    /// Transient designer sustain 0..1 (0.5 = flat).
    #[serde(default = "default_ms_unity")]
    pub transient_sustain: f32,
    /// Transient designer wet/dry 0..1.
    #[serde(default)]
    pub transient_mix: f32,
    /// Exciter saturation amount 0..1.
    #[serde(default)]
    pub exciter_amount: f32,
    /// Exciter HP corner 0..1 (1 kHz–10 kHz log).
    #[serde(default = "default_exciter_freq")]
    pub exciter_freq: f32,
    /// Exciter wet/dry on the added harmonics 0..1.
    #[serde(default)]
    pub exciter_mix: f32,
    /// Multitap delay base time 0..1 → 1 ms..1 s.
    #[serde(default = "default_multitap_time")]
    pub multitap_time: f32,
    /// Multitap delay tap-spread 0..1 (0 = collapsed, 1 = even).
    #[serde(default = "default_multitap_spread")]
    pub multitap_spread: f32,
    /// Multitap feedback 0..1.
    #[serde(default)]
    pub multitap_feedback: f32,
    /// Multitap wet/dry 0..1.
    #[serde(default)]
    pub multitap_mix: f32,
    /// Reverse-delay segment length 0..1 → 50 ms..2 s.
    #[serde(default = "default_revdelay_time")]
    pub revdelay_time: f32,
    /// Reverse-delay feedback 0..1.
    #[serde(default)]
    pub revdelay_feedback: f32,
    /// Reverse-delay wet/dry 0..1.
    #[serde(default)]
    pub revdelay_mix: f32,
    /// Tape stop ramp progress 0..1 (also acts as effect "engage" — 0 =
    /// pass-through, 1 = fully halted).
    #[serde(default)]
    pub tapestop_mix: f32,
    /// Tape stop scratch-tail length 0..1 → 50 ms..2 s.
    #[serde(default = "default_tapestop_time")]
    pub tapestop_time: f32,
    /// Stutter / repeater rate quantisation 0..1 (mapped to 1/4, 1/8,
    /// 1/16, 1/32 by quartiles).
    #[serde(default = "default_stutter_rate")]
    pub stutter_rate: f32,
    /// Stutter slice fraction 0..1 — fraction of the period that's
    /// captured for the loop slice; the remainder of the period replays
    /// the captured slice.
    #[serde(default = "default_stutter_slice")]
    pub stutter_slice: f32,
    /// Stutter wet/dry 0..1.
    #[serde(default)]
    pub stutter_mix: f32,
    /// Spectral-freezer mix (also acts as engage trigger — > 0 captures
    /// the current FFT magnitudes and resynths with random phases).
    #[serde(default)]
    pub freeze_mix: f32,
    pub waveshaper_drive: f32, // 0–1 → soft-clip drive amount (pre-FX)
    pub waveshaper_mix: f32,   // 0–1 wet/dry
    pub ring_mod_freq: f32,    // 0–1 → 50–500 Hz carrier frequency
    pub ring_mod_mix: f32,     // 0–1 wet/dry
    pub eq_low_gain: f32,      // -1..+1 → -12..+12 dB low shelf (~200 Hz)
    pub eq_mid_gain: f32,      // -1..+1 → -12..+12 dB mid peak (~1 kHz)
    pub eq_hi_gain: f32,       // -1..+1 → -12..+12 dB high shelf (~5 kHz)
    #[serde(default)]
    pub autotune_amount: f32, // 0–1 → 0..+12 semitones upward pitch shift
    #[serde(default)]
    pub autotune_mix: f32, // 0–1 wet/dry
    /// Pan FX: centre bias (-1..+1), hard-pan at the extremes.
    #[serde(default)]
    pub fx_pan_pos: f32,
    /// Pan FX: auto-pan depth 0..1 (0 = static pan at fx_pan_pos).
    #[serde(default)]
    pub fx_pan_width: f32,
    /// Pan FX: auto-pan LFO rate 0..1 → 0.05..8 Hz.
    #[serde(default = "default_fx_pan_rate")]
    pub fx_pan_rate: f32,
    // ── Stereo widener (FxWiden, master-stage latch) ──────────────
    /// Haas-delay amount 0..1 → 0..30 ms delay applied to the L
    /// channel only at the master stage.  Creates the classic
    /// psychoacoustic-widening illusion without any side-channel
    /// computation.
    #[serde(default = "default_widen_haas")]
    pub widen_haas: f32,
    /// Side-channel scaling 0..1 → 1..4× scaling on the existing
    /// mid/side decomposition at the master stage.  At 0, side stays
    /// untouched; at 1, side is boosted 4× — wider but more
    /// mono-incompatible.
    #[serde(default)]
    pub widen_side: f32,
    /// Wet/dry blend 0..1 — multiplier on the widening effect.  0 =
    /// bypass (master skips Haas + side scaling); 1 = full effect.
    #[serde(default)]
    pub widen_mix: f32,
    // ── Frequency shifter (FxFreqShift, Hilbert SSB) ──────────────
    /// Shift amount 0..1 (0.5 = no shift / 0 Hz, mapped to ±1000 Hz
    /// linear).  Sign of the offset picks shift direction (up = subtract
    /// the imaginary projection, down = add).
    #[serde(default = "default_freq_shift_amount")]
    pub freq_shift_amount: f32,
    /// Feedback 0..1 → 0..0.95 of the previous shifted output mixed
    /// back into the input.  Builds Sean-Costello-style "shimmer
    /// ladders" when paired with reverb upstream.
    #[serde(default)]
    pub freq_shift_feedback: f32,
    /// Wet/dry 0..1 (0 = bypass).
    #[serde(default)]
    pub freq_shift_mix: f32,
    // ── Vinyl / cassette simulator ───────────────────────────────────────
    /// Surface-noise amplitude 0..1 (0 = silent, 1 ≈ -20 dBFS).
    #[serde(default)]
    pub vinyl_noise: f32,
    /// Wear / dullness 0..1.  Drives the high-shelf cutoff sweep —
    /// 0 = bright (transparent), 1 = dull (HF rolled off + cut).
    #[serde(default)]
    pub vinyl_wear: f32,
    /// Wet/dry mix 0..1 (0 = bypass).
    #[serde(default)]
    pub vinyl_mix: f32,
    /// Start / stop transient 0..1.  0 = at full speed (V1 steady-state
    /// vinyl colour); 1 = deck stopped (rate=0, silent).  Automate 0→1
    /// for a brake transient, 1→0 for a spin-up.  Layered on top of
    /// the colour stage so the transient *is* the vinyl, not a
    /// separate effect.
    #[serde(default)]
    pub vinyl_transient: f32,
    // ── DJ Filter ────────────────────────────────────────────────────────
    /// Single-knob morph 0..1 — 0 = LP heavy (cutoff low), 0.5 = BP
    /// at mid with the resonance peak, 1 = HP heavy (cutoff high).
    /// Default 0.5 so a freshly inserted module sits at the
    /// resonance-peak crossover, not on a hard LP / HP edge.
    #[serde(default = "default_dj_filter_morph")]
    pub dj_filter_morph: f32,
    /// Resonance 0..1 → Q.  At morph 0.5 the BP component dominates,
    /// so resonance reads as the classic "peak at the crossover".
    #[serde(default = "default_dj_filter_resonance")]
    pub dj_filter_resonance: f32,
    /// Wet/dry mix 0..1 (0 = bypass).
    #[serde(default)]
    pub dj_filter_mix: f32,
    // ── Tremolo ──────────────────────────────────────────────────────────
    /// LFO rate 0..1 → 0.1..12 Hz log-mapped.  Covers slow swelling
    /// at one end through helicopter-chop at the other.
    #[serde(default = "default_tremolo_rate")]
    pub tremolo_rate: f32,
    /// Modulation depth 0..1.  At 0 the LFO is unity (no AM); at 1
    /// the wet signal swings between full volume and silence.
    #[serde(default = "default_tremolo_depth")]
    pub tremolo_depth: f32,
    /// Waveshape morph 0..1 — 0 = sine (smooth swell), 1 = square
    /// (hard chop).  Implemented as tanh-clamping a scaled sine so
    /// the morph is continuous rather than a mode flag.
    #[serde(default)]
    pub tremolo_shape: f32,
    /// Wet/dry mix 0..1 (0 = bypass).  Acts as the engaged-or-not
    /// switch alongside `depth`; the cheap-bypass fast path checks
    /// this value first.
    #[serde(default)]
    pub tremolo_mix: f32,
    // ── 3-band ISO / kill EQ ────────────────────────────────────────────
    /// Low-band level 0..1.  Hard-kills (or passes) the band below
    /// the ~250 Hz crossover.  1 = full pass; 0 = silenced.
    #[serde(default = "default_iso_pass")]
    pub iso_low: f32,
    /// Mid-band level 0..1.  Implemented subtractively from the dry
    /// signal so the three bands sum to the input exactly when all
    /// three knobs are at 1.
    #[serde(default = "default_iso_pass")]
    pub iso_mid: f32,
    /// High-band level 0..1.  Hard-kills (or passes) the band above
    /// the ~2.5 kHz crossover.
    #[serde(default = "default_iso_pass")]
    pub iso_high: f32,
    /// Wet/dry mix 0..1 (0 = bypass).  Lets the user A/B the ISO'd
    /// signal against the original without dialling all three bands
    /// back to 1.
    #[serde(default)]
    pub iso_mix: f32,
    // ── Resonator bank ──────────────────────────────────────────────────
    /// Root pitch 0..1 → MIDI 24..96 (C1..C7).  Each of the six
    /// resonators is tuned to root + an interval defined by the
    /// `resbank_chord` preset.
    #[serde(default = "default_resbank_root")]
    pub resbank_root: f32,
    /// Chord preset 0..1 → quantized to one of six interval sets:
    /// 0=minor7, 1=major triad spread, 2=dom9, 3=open fifths,
    /// 4=octave stack, 5=cluster.  Discretised inside the DSP so
    /// the FX still uses one continuous f32 field (no separate
    /// u8 mode + cycle button).
    #[serde(default)]
    pub resbank_chord: f32,
    /// Resonance 0..1 → Q 1..50 — controls how pingy / sustained
    /// the resonators sound.  High Q = singing pitches; low Q =
    /// muted ringing.
    #[serde(default = "default_resbank_resonance")]
    pub resbank_resonance: f32,
    /// Wet/dry mix 0..1 (0 = bypass).
    #[serde(default)]
    pub resbank_mix: f32,
    // ── Spectral Gate ───────────────────────────────────────────────────
    /// True STFT mode flag.  V1 ships an 8-band parallel-BPF
    /// approximation (per-band envelope + gate, subtractive
    /// recombination); V2 adds an STFT-based path that does the
    /// textbook windowed-FFT → per-bin gate → IFFT → overlap-add.
    /// Default false so existing presets and sessions keep their
    /// V1 sound; flip to true for the higher-resolution textbook
    /// version.
    #[serde(default)]
    pub spec_stft: bool,
    /// Per-band threshold 0..1 — linear amplitude.  Any band
    /// whose envelope falls below this level gets gated toward
    /// silence; bands above stay open.  In STFT mode this is the
    /// per-bin amplitude threshold (still 0..1) — the bin's
    /// linear magnitude after the forward FFT.
    #[serde(default = "default_spec_thresh")]
    pub spec_thresh: f32,
    /// Gate release 0..1 → 10..2000 ms log-mapped.  Long values
    /// freeze low-level resonance after a transient hits — the
    /// "spectral hold" effect.  Short = quick spectral gating.
    #[serde(default = "default_spec_release")]
    pub spec_release: f32,
    /// Threshold tilt 0..1 across the spectrum.  0.5 = uniform;
    /// <0.5 = high bands gate more aggressively; >0.5 = low bands
    /// gate more aggressively.  Skew is gentle (max ±2× of base)
    /// so the knob feels musical rather than clipping bands into
    /// silence.
    #[serde(default = "default_spec_tilt")]
    pub spec_tilt: f32,
    /// Wet/dry mix 0..1 (0 = bypass).
    #[serde(default)]
    pub spec_mix: f32,
    // ── Grain Delay ─────────────────────────────────────────────────────
    /// Base delay 0..1 → 50..1000 ms log-mapped — centre of the
    /// grain cloud's read position.
    #[serde(default = "default_grain_delay")]
    pub grain_delay: f32,
    /// Grain length 0..1 → 20..200 ms.  Short = chorus / verb
    /// cloud, long = smeared delay tap.
    #[serde(default = "default_grain_size")]
    pub grain_size: f32,
    /// Scatter 0..1 — 0 = grains aligned in time + pitch (chorus
    /// around the baseline); 1 = wide pitch jitter (±1 octave) +
    /// position scatter (±50 % of base delay).
    #[serde(default = "default_grain_scatter")]
    pub grain_scatter: f32,
    /// Wet/dry mix 0..1 (0 = bypass).
    #[serde(default)]
    pub grain_mix: f32,
    // ── Multiband compressor ────────────────────────────────────────────
    /// Low-band threshold 0..1 — linear amplitude.  When the band's
    /// envelope rises above this level, downward compression
    /// engages.  Lower = more aggressive.  Default 1.0 (above the
    /// signal's peak, so the band is uncompressed until the user
    /// dials it down).
    #[serde(default = "default_mb_thresh")]
    pub mb_low_thresh: f32,
    /// Mid-band threshold 0..1.  Subtractive mid (band that's
    /// `LP_2500(input) - LP_250(input)`) follows the same shape.
    #[serde(default = "default_mb_thresh")]
    pub mb_mid_thresh: f32,
    /// High-band threshold 0..1.  Band above ~2.5 kHz.
    #[serde(default = "default_mb_thresh")]
    pub mb_high_thresh: f32,
    /// Wet/dry mix 0..1 (0 = bypass).
    #[serde(default)]
    pub mb_mix: f32,
    // ── Tape Echo ───────────────────────────────────────────────────────
    /// Delay time 0..1 → 25..1500 ms log-mapped.  Covers slap-back
    /// at the short end through dub-rhythm at the long end.  Free-
    /// running (not BPM-synced) so it sits easily on top of a
    /// sequenced groove without phasing against the grid.
    #[serde(default = "default_tape_echo_time")]
    pub tape_echo_time: f32,
    /// Feedback 0..1 → 0..0.95 of the previous wet output mixed
    /// back into the input.  Caps under unity even at full knob to
    /// avoid runaway when AGE saturation is also engaged.
    #[serde(default = "default_tape_echo_feedback")]
    pub tape_echo_feedback: f32,
    /// AGE 0..1 — single character knob.  Folds wow / flutter
    /// depth + tape saturation + HF rolloff together so the user
    /// dials "more worn-out tape" with one gesture.  0 = pristine
    /// digital echo; 1 = ruined cassette.
    #[serde(default = "default_tape_echo_age")]
    pub tape_echo_age: f32,
    /// Wet/dry mix 0..1 (0 = bypass).
    #[serde(default)]
    pub tape_echo_mix: f32,
    // ── De-esser ────────────────────────────────────────────────────────
    /// Sibilant centre frequency 0..1 → 3..12 kHz log-mapped.  HP
    /// detector + ducker centre — typical de-essing range with
    /// vocals.  Below 3 kHz starts ducking sibilance into the
    /// presence band; above 12 kHz misses most ess sounds.
    #[serde(default = "default_deess_freq")]
    pub deess_freq: f32,
    /// Threshold 0..1 → linear amplitude.  When the sibilant band's
    /// envelope rises above this level, the ducker engages.  Lower
    /// = more aggressive de-essing.
    #[serde(default = "default_deess_threshold")]
    pub deess_threshold: f32,
    /// Ducking amount 0..1 — how hard to attenuate when over
    /// threshold.  0 = none (FX is a no-op even with full mix);
    /// 1 = hard kill of the sibilant band on every over-threshold
    /// sample.
    #[serde(default = "default_deess_amount")]
    pub deess_amount: f32,
    /// Wet/dry mix 0..1 (0 = bypass).
    #[serde(default)]
    pub deess_mix: f32,
    // ── Vibrato ──────────────────────────────────────────────────────────
    /// LFO rate 0..1 → 0.1..10 Hz log-mapped.  Vibrato tops out
    /// lower than tremolo because hyper-fast pitch wobble crosses
    /// into FM-sideband territory where the effect stops reading
    /// as a vibrato.
    #[serde(default = "default_vibrato_rate")]
    pub vibrato_rate: f32,
    /// Modulation depth 0..1 → 0..±5 ms peak delay-time deviation
    /// (≈ 0..50 cents at 5 Hz).  At 0 the delay line has zero
    /// modulation (signal passes through with a static delay).
    #[serde(default = "default_vibrato_depth")]
    pub vibrato_depth: f32,
    /// Waveshape morph 0..1 — 0 = sine (smooth pitch curve),
    /// 1 = near-square (warbly pitch hops).  Same tanh-lerp as
    /// tremolo's `shape` for visual + sonic consistency.
    #[serde(default)]
    pub vibrato_shape: f32,
    /// Wet/dry mix 0..1 (0 = bypass).  Cheap-bypass fast path
    /// checks this first.
    #[serde(default)]
    pub vibrato_mix: f32,
    // ── Convolution Reverb ───────────────────────────────────────────────
    /// Wet/dry mix (0 = dry, 1 = 100 % wet).
    #[serde(default)]
    pub conv_reverb_mix: f32,
    /// IR truncation (0..1 of the loaded impulse length).  1.0 = full IR;
    /// lower values shorten the tail (equivalent to a gated reverb without
    /// the re-open behaviour).  0.0 falls back to a minimum of one partition
    /// so the convolution keeps running.
    #[serde(default = "default_conv_reverb_size")]
    pub conv_reverb_size: f32,
    /// Predelay in samples-at-time-of-processing (0..1 → 0..200 ms at the
    /// live engine sample rate).  Moves the onset of the wet signal
    /// relative to the dry so the reverb feels further back in the mix.
    #[serde(default)]
    pub conv_reverb_predelay: f32,
    /// One-pole low-pass on the wet signal (0..1 → fully open .. ~800 Hz).
    /// Darkens the reverb tail without affecting the dry.
    #[serde(default)]
    pub conv_reverb_damp: f32,
    /// One-pole high-pass on the wet signal (0..1 → bypass .. ~800 Hz).
    /// Removes mud so the wet blends cleanly under a busy mix.
    #[serde(default)]
    pub conv_reverb_lowcut: f32,
    /// Stereo width of the wet (0 = mono, 1 = full stereo from a stereo IR).
    /// For mono IRs this degrades gracefully to mono regardless of the knob.
    #[serde(default = "default_conv_reverb_width")]
    pub conv_reverb_width: f32,
    /// Play the loaded IR reversed (classic reverse-reverb effect).  The IR
    /// transform is recomputed when this toggles so no per-block cost.
    #[serde(default)]
    pub conv_reverb_reverse: bool,
    /// Filesystem path of the currently loaded impulse response.  Empty
    /// string = no IR loaded (the step acts as a wet-coloured dry pass).
    #[serde(default)]
    pub conv_reverb_ir_path: String,
    /// Cabinet-IR mode — UI hint that the loaded IR is a guitar / bass cab
    /// (short — typically <200 ms) rather than a hall reverb.  When true,
    /// the file picker browses `samples/cabinets/` instead of
    /// `samples/impulses/`, and `conv_reverb_size` is internally capped at
    /// 0.1 (10 % of the loaded IR) so even long impulses get treated as
    /// short cabinet responses.
    #[serde(default)]
    pub conv_reverb_cabinet: bool,
    /// Shimmer feedback depth 0..1.  When > 0 the wet output is pitch-
    /// shifted +12 semitones (one octave up) and folded back into the
    /// convolution input, producing the classic ambient-shimmer ladder.
    /// 0 = off (V1 behaviour), 1 = full shimmer (high feedback risk —
    /// internally clamped at the PitchShift module's safety cap).
    #[serde(default)]
    pub conv_reverb_shimmer: f32,
    /// 8-band parametric EQ — replaces the fixed 3-band EQ for the
    /// ParamEq FX module.  The existing `eq_low_gain` / `eq_mid_gain`
    /// / `eq_hi_gain` fields above stay live for `FxEq` (the legacy
    /// 3-knob card) so existing sessions don't have to migrate.
    #[serde(default = "default_param_eq_bands")]
    pub param_eq_bands: [ParamEqBand; 8],
    /// Mid/side mode flag for `FxParamEq`.  When true, the chain step
    /// is a passthrough (just records that M/S is requested) and the
    /// master stage runs two extra `ParamEq` cascades on the M and S
    /// channels of the final L/R buses.  When false, the standard
    /// in-chain mono cascade applies.  Same master-stage latch
    /// pattern as `FxWiden` / `FxPan`.
    #[serde(default)]
    pub param_eq_ms_mode: bool,
    /// Standalone pitch shifter — semitone offset stored directly so
    /// the LLM can write a musical value (`"pitch_shift_semi": 7`).
    /// Clamped to ±24 st at DSP time.
    #[serde(default)]
    pub pitch_shift_semi: f32,
    /// Pitch shifter fine tuning in cents (-100..+100) — added to the
    /// semitone offset at DSP time so users can detune the wet
    /// harmony by a few cents for doubled-voice thickening.
    #[serde(default)]
    pub pitch_shift_fine: f32,
    /// Pitch shifter wet/dry mix (0..1).  0 = bypass.
    #[serde(default)]
    pub pitch_shift_mix: f32,
    /// Pitch shifter feedback (0..1) — pipes the wet back into the
    /// input so repeated-shift stacks pile up (classic +7 st feedback
    /// ladders into +7, +14, +21 … harmonies).  Internally clamped
    /// to 0.95 to stop runaway overflow.
    #[serde(default)]
    pub pitch_shift_fbk: f32,
    // ── Mid/side master processing ────────────────────────────────────
    // 0..1 normalised.  Gain + tilt use 0.5 as the unity/flat detent
    // so the master stays transparent at the default rest position.
    // Saturation uses 0 as off.
    /// Mid-channel gain (0..1 → -12..+12 dB).  0.5 = unity.
    #[serde(default = "default_ms_unity")]
    pub ms_mid_gain: f32,
    /// Mid-channel tilt EQ (0..1 → bass-heavy..treble-heavy via
    /// opposing low-shelf / high-shelf pair at 200 Hz and 5 kHz).
    /// 0.5 = flat; 0.0 = −6 dB treble / +6 dB bass; 1.0 = the inverse.
    #[serde(default = "default_ms_unity")]
    pub ms_mid_tilt: f32,
    /// Mid-channel saturation (arctan soft clip; 0 = off).
    #[serde(default)]
    pub ms_mid_sat: f32,
    /// Side-channel gain — same mapping as `ms_mid_gain`.  Pulls back
    /// the side at 0, widens at 1.
    #[serde(default = "default_ms_unity")]
    pub ms_side_gain: f32,
    /// Side-channel tilt EQ — same mapping as `ms_mid_tilt`.
    /// Tilting side toward treble widens the air without thickening
    /// the low end (classic mastering move).
    #[serde(default = "default_ms_unity")]
    pub ms_side_tilt: f32,
    /// Side-channel saturation.
    #[serde(default)]
    pub ms_side_sat: f32,
    // ── Plate reverb ─────────────────────────────────────────────────────
    /// Tank time / decay scale 0..1.  Internally maps to the cross-feed
    /// gain inside the figure-of-eight tank, so larger values produce a
    /// longer tail without changing the (heap-allocated) delay-line
    /// lengths.  Distinct from `reverb_size` (Schroeder comb / allpass
    /// stack with a different decay character) and `conv_reverb_size`
    /// (truncates a loaded IR).
    #[serde(default = "default_plate_size")]
    pub plate_size: f32,
    /// One-pole LP coefficient inside each tank half — 0 = bright,
    /// 1 ≈ very dark.  Shapes the tail's HF rolloff.
    #[serde(default = "default_plate_damping")]
    pub plate_damping: f32,
    /// Input pre-AP gain 0..1 → 0..0.75.  Higher values produce denser,
    /// smoother early reflections.
    #[serde(default = "default_plate_diffusion")]
    pub plate_diffusion: f32,
    /// Wet/dry mix 0..1 (0 = bypass).
    #[serde(default)]
    pub plate_mix: f32,
    // ── Trance gate ──────────────────────────────────────────────────────
    /// 16-cell gate pattern as a u16 bitmask — bit N = cell N's gate
    /// state (1 = open, 0 = closed).  Default `0xAAAA` (alternating)
    /// engages with an audible eighth-note pulse at 1/16 rate.
    #[serde(default = "default_tg_pattern")]
    pub tg_pattern: u16,
    /// Cell traversal rate selector.  0 = 1/4, 1 = 1/8, 2 = 1/16,
    /// 3 = 1/32 — cells per bar respectively 4/8/16/32.  Default 1
    /// (1/8) — quarter-note feel at the default pattern.
    #[serde(default = "default_tg_rate")]
    pub tg_rate: u8,
    /// Per-cell-edge ramp 0..1 → 0.5..50 ms one-pole smoother.  Stops
    /// the gate from clicking on each transition.
    #[serde(default = "default_tg_smooth")]
    pub tg_smooth: f32,
    /// Wet/dry mix 0..1 (0 = bypass).
    #[serde(default)]
    pub tg_mix: f32,
    // ── Wavefolder ───────────────────────────────────────────────────────
    /// Pre-fold input gain 0..1 → 1..10×.  Drive is what pushes the
    /// signal above the fold threshold; without drive the FX is
    /// essentially a passthrough.
    #[serde(default = "default_wf_drive")]
    pub wf_drive: f32,
    /// DC offset 0..1 (0.5 = centred / symmetric fold).  Off-centre
    /// values produce asymmetric folding with a different harmonic
    /// content (more even-order energy).
    #[serde(default = "default_wf_bias")]
    pub wf_bias: f32,
    /// Fold-curve blend 0..1 — 0 = pure sine fold (Buchla character),
    /// 1 = pure triangle fold (Serge character).  Mid values cross-
    /// fade between the two curves for hybrid harmonics.
    #[serde(default = "default_wf_symmetry")]
    pub wf_symmetry: f32,
    /// Wet/dry mix 0..1 (0 = bypass).
    #[serde(default)]
    pub wf_mix: f32,
}

impl Default for FxState {
    fn default() -> Self {
        Self {
            reverb_size: 0.4,
            reverb_damp: 0.5,
            reverb_mix: 0.0,
            reverb_gate_time: 0.0,
            reverb_freeze: false,
            reverb_dir: 0,
            reverb_rev_quant: 0,
            delay_time: 0.375,
            delay_feedback: 0.4,
            delay_mix: 0.0,
            delay_dir: 0,
            delay_rev_quant: 0,
            delay_wow_flutter: 0.0,
            delay_saturation: 0.0,
            delay_freeze: false,
            delay_hpf: 0.0,
            delay_lpf: 0.0,
            distortion_drive: 0.0,
            distortion_mix: 0.0,
            compressor_threshold: 0.7,
            compressor_ratio: 0.3,
            compressor_mix: 0.0,
            compressor_multiband: 0.0,
            compressor_reverse: false,
            compressor_sidechain: false,
            gate_threshold: default_gate_threshold(),
            gate_attack: default_gate_attack(),
            gate_release: default_gate_release(),
            gate_depth: default_gate_depth(),
            gate_mix: 0.0,
            vocoder_bands: default_vocoder_bands(),
            vocoder_carrier_mix: 0.0,
            vocoder_sense: default_vocoder_sense(),
            vocoder_mix: 0.0,
            master_volume: 0.85,
            stereo_width: 0.5,
            tuning: 0,
            xmod_bass_to_an1x_pitch: 0.0,
            xmod_noise_to_filter: 0.0,
            sidechain_amount: 0.0,
            sidechain_attack: 0.1,
            sidechain_release: 0.3,
            tape_drive: 0.3,
            tape_mix: 0.0,
            tape_flutter: 0.2,
            master_pitch_st: 0.0,
            bitcrush_bits: 1.0,
            bitcrush_rate: 0.0,
            bitcrush_mix: 0.0,
            chorus_rate: 0.3,
            chorus_depth: 0.5,
            chorus_mix: 0.0,
            phaser_rate: 0.3,
            phaser_depth: 0.5,
            phaser_mix: 0.0,
            flanger_rate: default_flanger_rate(),
            flanger_depth: default_flanger_depth(),
            flanger_feedback: default_flanger_feedback(),
            flanger_mix: 0.0,
            limiter_threshold: default_limiter_threshold(),
            limiter_ceiling: default_limiter_ceiling(),
            limiter_release: default_limiter_release(),
            limiter_lookahead: default_limiter_lookahead(),
            svf_cutoff: default_svf_cutoff(),
            svf_resonance: 0.0,
            svf_drive: 0.0,
            svf_mix: 0.0,
            svf_mode: 0,
            comb_pitch: default_comb_pitch(),
            comb_feedback: 0.0,
            comb_damp: 0.0,
            comb_mix: 0.0,
            tilt_tilt: default_ms_unity(),
            tilt_pivot: default_tilt_pivot(),
            tilt_mix: 0.0,
            transient_attack: default_ms_unity(),
            transient_sustain: default_ms_unity(),
            transient_mix: 0.0,
            exciter_amount: 0.0,
            exciter_freq: default_exciter_freq(),
            exciter_mix: 0.0,
            multitap_time: default_multitap_time(),
            multitap_spread: default_multitap_spread(),
            multitap_feedback: 0.0,
            multitap_mix: 0.0,
            revdelay_time: default_revdelay_time(),
            revdelay_feedback: 0.0,
            revdelay_mix: 0.0,
            tapestop_mix: 0.0,
            tapestop_time: default_tapestop_time(),
            stutter_rate: default_stutter_rate(),
            stutter_slice: default_stutter_slice(),
            stutter_mix: 0.0,
            freeze_mix: 0.0,
            waveshaper_drive: 0.0,
            waveshaper_mix: 0.0,
            ring_mod_freq: 0.2,
            ring_mod_mix: 0.0,
            eq_low_gain: 0.0,
            eq_mid_gain: 0.0,
            eq_hi_gain: 0.0,
            autotune_amount: 0.0,
            autotune_mix: 0.0,
            fx_pan_pos: 0.0,
            fx_pan_width: 0.5,
            fx_pan_rate: default_fx_pan_rate(),
            widen_haas: default_widen_haas(),
            widen_side: 0.0,
            widen_mix: 0.0,
            freq_shift_amount: default_freq_shift_amount(),
            freq_shift_feedback: 0.0,
            freq_shift_mix: 0.0,
            vinyl_noise: 0.5,
            vinyl_wear: 0.3,
            vinyl_mix: 0.0,
            vinyl_transient: 0.0,
            dj_filter_morph: default_dj_filter_morph(),
            dj_filter_resonance: default_dj_filter_resonance(),
            dj_filter_mix: 0.0,
            tremolo_rate: default_tremolo_rate(),
            tremolo_depth: default_tremolo_depth(),
            tremolo_shape: 0.0,
            tremolo_mix: 0.0,
            vibrato_rate: default_vibrato_rate(),
            vibrato_depth: default_vibrato_depth(),
            vibrato_shape: 0.0,
            vibrato_mix: 0.0,
            iso_low: default_iso_pass(),
            iso_mid: default_iso_pass(),
            iso_high: default_iso_pass(),
            iso_mix: 0.0,
            deess_freq: default_deess_freq(),
            deess_threshold: default_deess_threshold(),
            deess_amount: default_deess_amount(),
            deess_mix: 0.0,
            resbank_root: default_resbank_root(),
            resbank_chord: 0.0,
            resbank_resonance: default_resbank_resonance(),
            resbank_mix: 0.0,
            tape_echo_time: default_tape_echo_time(),
            tape_echo_feedback: default_tape_echo_feedback(),
            tape_echo_age: default_tape_echo_age(),
            tape_echo_mix: 0.0,
            mb_low_thresh: default_mb_thresh(),
            mb_mid_thresh: default_mb_thresh(),
            mb_high_thresh: default_mb_thresh(),
            mb_mix: 0.0,
            grain_delay: default_grain_delay(),
            grain_size: default_grain_size(),
            grain_scatter: default_grain_scatter(),
            grain_mix: 0.0,
            spec_stft: false,
            spec_thresh: default_spec_thresh(),
            spec_release: default_spec_release(),
            spec_tilt: default_spec_tilt(),
            spec_mix: 0.0,
            conv_reverb_mix: 0.0,
            conv_reverb_size: default_conv_reverb_size(),
            conv_reverb_predelay: 0.0,
            conv_reverb_damp: 0.0,
            conv_reverb_lowcut: 0.0,
            conv_reverb_width: default_conv_reverb_width(),
            conv_reverb_reverse: false,
            conv_reverb_cabinet: false,
            conv_reverb_shimmer: 0.0,
            conv_reverb_ir_path: String::new(),
            param_eq_bands: default_param_eq_bands(),
            param_eq_ms_mode: false,
            pitch_shift_semi: 0.0,
            pitch_shift_fine: 0.0,
            pitch_shift_mix: 0.0,
            pitch_shift_fbk: 0.0,
            ms_mid_gain: default_ms_unity(),
            ms_mid_tilt: default_ms_unity(),
            ms_mid_sat: 0.0,
            ms_side_gain: default_ms_unity(),
            ms_side_tilt: default_ms_unity(),
            ms_side_sat: 0.0,
            plate_size: default_plate_size(),
            plate_damping: default_plate_damping(),
            plate_diffusion: default_plate_diffusion(),
            plate_mix: 0.0,
            tg_pattern: default_tg_pattern(),
            tg_rate: default_tg_rate(),
            tg_smooth: default_tg_smooth(),
            tg_mix: 0.0,
            wf_drive: default_wf_drive(),
            wf_bias: default_wf_bias(),
            wf_symmetry: default_wf_symmetry(),
            wf_mix: 0.0,
        }
    }
}
