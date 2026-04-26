// ─── state/module_kind.rs ─────────────────────────────────────────────────────
// `ModuleKind` + `Zone` — the enumeration of every instantiable rack module
// type and the vertical layout zones.  Extracted from rack.rs to keep that
// file under the 1000-line cap.  Layout/label/grid metadata is exhaustive per
// variant here; adding a new kind forces every match to be updated.

use serde::{Deserialize, Serialize};

// ─── Module kinds ─────────────────────────────────────────────────────────────

/// Every instantiable module type in the rack.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ModuleKind {
    // ── Voice modules ─────────────────────────────────────────────────────────
    AcidBass,
    DrumKit808,
    DrumKit909,
    HooverLead,
    PluckString,
    WavetableVoice,
    /// Pitched sample-playback instrument — load a single recording and
    /// retune it across the keyboard via ratio resampling.  Distinct from
    /// `AmenSampler` (plays slices at original pitch) and `WavetableVoice`
    /// (single-cycle frame scan).  Single instance per rack in V1.
    SampleInstrument,
    An1xVoice,
    AmenSampler,
    NoiseVoice,
    /// Theremin — XY-pad-driven continuous-pitch oscillator with
    /// portamento glide.  Absurd-queue voice; no sequencer hookup
    /// in V1, the user "plays" it by dragging the pad on its panel.
    Theremin,
    /// Pendulum — two near-tuned sine oscillators that beat
    /// acoustically.  Absurd-queue voice; no sequencer hookup,
    /// drone voice driven by knobs (base pitch + detune Hz).
    Pendulum,
    /// FM operator synth — 4-op DX7-flavoured voice.  Sequencer-
    /// driven (unlike Theremin / Pendulum which are knob-played).
    /// Plugs the bell / E-piano / FM-bass gap that the existing
    /// AN1X subtractive voice and SAMPLER+ recordings can't fill.
    FmOpsVoice,
    /// Additive synth — 16 sine partials at integer multiples of
    /// the played note, with per-harmonic level sliders.  Distinct
    /// from `WavetableVoice`: the user draws the spectrum directly
    /// instead of scanning pre-baked frames.  Sequencer-driven.
    AdditiveVoice,
    /// Modal / struck-physical-model synth — 8 parallel resonant
    /// biquads excited by a short noise burst on each trigger.
    /// Idiomatic for marimba / bell / glass / metal-bar timbres
    /// the harmonic-stack voices (Additive, AN1X) can't reach.
    /// Sequencer-driven.
    ModalVoice,
    /// Chiptune voice — SID-flavoured (Commodore 64) 3-osc
    /// (saw / triangle / pulse / noise) with per-osc ADSR,
    /// shared pulse-width + resonant filter, ring-mod and
    /// hard-sync.  Sequencer-driven.
    ChiptuneVoice,
    /// Vocal formant synth — saw source through 3 parallel
    /// resonant bandpass filters tuned to vowel-formant
    /// frequencies.  Sings A / E / I / O / U without a phoneme
    /// model.  Sequencer-driven; distinct from `NeuTts`.
    VocalVoice,
    GranularTexture,
    /// Dedicated hardcore-style kick voice — distinct from the 808/909 kicks.
    GabberKick,
    // ── TTS voice module (NeuTTS Air) ───────────────────────────────────────
    #[serde(alias = "EspeakNgTts", alias = "CoquiTts")]
    NeuTts,
    // ── Sequencer ─────────────────────────────────────────────────────────────
    /// Main step sequencer — drives all voice modules.
    StepSequencer,
    // ── FX modules ────────────────────────────────────────────────────────────
    FxReverb,
    FxDelay,
    FxChorus,
    FxPhaser,
    FxFlanger,
    FxLimiter,
    FxFilter,
    FxComb,
    FxTilt,
    FxTransient,
    FxExciter,
    FxMultitap,
    FxRevDelay,
    FxTapeStop,
    FxStutter,
    FxFreeze,
    FxRingMod,
    FxWaveshaper,
    FxBitcrush,
    FxEq,
    FxCompressor,
    /// Sidechain ducker / noise gate — envelope follower on a sidechain
    /// audio input drives the gain on the main signal.  When sidechain
    /// is unconnected falls back to the main signal as its own detector
    /// (acts as a simple noise gate).  Tier 2 sidechain FX.
    FxGate,
    /// 16-band vocoder — N-band envelope follower on the sidechain
    /// modulates per-band amplitude on the main carrier.  Pairs with
    /// `NeuTts` for talkbox-flavoured patches (TTS → modulator,
    /// bass / hoover → carrier).  Tier 2 sidechain FX.
    FxVocoder,
    FxTapeSat,
    FxDrive,
    FxAutotune,
    FxPan,
    /// Stereo widener — Haas delay (short L-channel offset) +
    /// mid/side side-scaling.  Implemented as a master-stage latch
    /// (the chain step is a passthrough that flags the master to
    /// apply Haas + side scaling on the final L/R, mirroring the
    /// FxPan "side latch" pattern).  Tier 1 stereo FX.
    FxWiden,
    FxConvReverb,
    FxParamEq,
    FxPitchShift,
    /// Single-sideband frequency shifter (Hilbert-pair allpass
    /// cascade + complex-multiplied carrier).  Distinct from
    /// `FxPitchShift` (which preserves harmonic ratios): adds the
    /// same Hz to every component, so harmonics become inharmonic
    /// and timbres turn metallic / bell-like.  Tier 1 carved-out
    /// from the original sidechain batch — the Hilbert design is
    /// subtle enough to deserve its own slot.
    FxFreqShift,
    /// Vinyl / cassette simulator — surface noise + dull EQ shape.
    /// Steady-state analog-tape character; distinct from
    /// `FxTapeStop` (which models the brake transient).
    FxVinyl,
    /// DJ filter — single-knob LP↔BP↔HP morph with a resonance
    /// peak at the crossover.  Live-friendly performance FX; smaller
    /// surface than `FxFilter` (which has separate cutoff / mode
    /// controls), and the cutoff sweep is baked into the morph.
    FxDjFilter,
    /// Tremolo — periodic amplitude modulation by an internal LFO.
    /// Distinct from `FxPan` (left/right balance LFO, no level swing)
    /// and from chorus/flanger (delay-line modulation).  Sine →
    /// square morph covers slow swell through helicopter-chop.
    FxTremolo,
    /// Vibrato — periodic pitch modulation via a small modulated
    /// delay line.  Pitch-domain cousin of `FxTremolo`.  Distinct
    /// from `FxChorus` (multiple delay taps mixed with dry for
    /// thickening) — vibrato is a single tap with no dry blend in
    /// the wet path so the user hears pure pitch wobble.
    FxVibrato,
    /// 3-band ISO / kill EQ — DJ-style hard-kill bands (low /
    /// mid / high) at ~250 Hz and ~2.5 kHz crossovers.
    /// Subtractive mid (dry minus low minus high) so the three
    /// bands sum to the input exactly when all knobs are at 1.
    /// Distinct from `FxEq` (3-band fixed-shelf with continuous
    /// gain knobs) — ISO is for live performance kills, not
    /// shelf shaping.
    FxIsoEq,
    /// De-esser — sidechain HP detector → narrow ducker on the
    /// sibilant band.  Specialist tool for vocal / hat material.
    /// Distinct from `FxCompressor` (broadband dynamics): only
    /// the sibilant band is attenuated, the rest of the signal
    /// passes untouched.
    FxDeEsser,
    /// Resonator bank — six tuned BPF biquads in parallel turn
    /// any input into a chord layer.  Karplus-on-input character.
    /// Distinct from `FxComb` (single tuned-delay resonator) —
    /// six simultaneous pitches at once, picked by the chord
    /// preset knob, with pitch governed by the root knob rather
    /// than tracking the input.
    FxResBank,
    /// Spectral Gate — per-band amplitude gating across an
    /// 8-band log-spaced filter bank (~80 Hz to ~16 kHz).
    /// Distinct from `FxGate` (broadband single-band) and
    /// `FxFreeze` (held buffer / spectral freeze of the entire
    /// signal): each band has its own envelope follower, so a
    /// loud kick can pass while quiet ambient air is gated.
    /// V1 uses parallel BPFs; the textbook STFT version is
    /// deferred until FFT machinery lands.
    FxSpectralGate,
    /// Grain Delay — granular feedback path.  Distinct from
    /// `FxMultitap` (rhythmic taps), `FxFreeze` (held buffer),
    /// and `FxDelay` (single tap).  Reads short Hann-windowed
    /// grains scattered in time + pitch around a baseline delay,
    /// producing a chorused, smeared echo cloud.
    FxGrainDelay,
    /// Multiband compressor — 3-band split + 3 independent
    /// downward compressors.  Distinct from `FxCompressor`
    /// (broadband single-band): each band has its own threshold,
    /// so the user can tame a boomy low end without flattening
    /// the air or vice versa.  Mastering-grade dynamics.
    FxMultibandComp,
    /// Tape Echo — dub-style delay with wow / flutter / saturation
    /// baked into the feedback path.  Distinct from `FxDelay`
    /// (clean digital with character knobs that have to be dialed
    /// in separately), `FxTapeSat` (no delay), and `FxMultitap`
    /// (rhythmic taps).  One AGE knob folds wow + flutter + sat
    /// HF rolloff together so users dial "more worn-out tape" with
    /// one gesture.
    FxTapeEcho,
    // ── Analysis ──────────────────────────────────────────────────────────────
    SpectrumAnalyzer,
    StereoMeter,
    ActivityTimeline,
    /// Phosphor-style colored bar oscilloscope — mirrors the header
    /// `draw_scope_colored` widget but lives as a rack module so users
    /// can park it anywhere in the FX/Mod zone.  Reads the global
    /// scope buffer + history; no audio input cable needed.
    BarOscilloscope,
    /// Stereo vectorscope / goniometer — XY plot of L vs R from the
    /// engine's interleaved stereo buffer.  Reads `app.stereo_buf`
    /// (already maintained for the header phase-correlation strip).
    /// Pure UI; no audio thread interaction.
    StereoVectorscope,
    /// LFO output trace — waveform of the LFO module connected via
    /// the back-panel CV cable, rendered with the same phosphor look
    /// as the bar oscilloscope.  No audio path.
    LfoScope,
    /// Pitch tracker / tuner — autocorrelation pitch on the master
    /// scope buffer, displays note name + cents-off needle.  Cheap
    /// reuse of `detect_pitch_hz` in src/audio/analysis.rs.
    PitchTracker,
    /// Chord / key display — chroma-vector folding of the existing
    /// spectrum, matched against 24 major+minor triad templates.
    /// Reuses `compute_spectrum` so adding the module is essentially
    /// free DSP-wise.
    ChordDisplay,
    /// Spectrogram waterfall — rolling FFT history rendered as a
    /// scrolling 2D ColorImage (time on X, frequency on Y).  Heavier
    /// than the static spectrum bars; the UI paints a fixed-height
    /// pixel column per frame and recycles a `ColorImage` between
    /// repaints to keep the per-frame allocation bounded.
    Spectrogram,
    /// Loudness / LUFS meter — K-weighted (BS.1770) momentary + short-term
    /// loudness display.  Distinct from the per-frame peak / RMS in
    /// `StereoMeter` because LUFS is the integrated, perceptually
    /// weighted measure mastering engineers actually target.
    LoudnessMeter,
    /// Transport phase wheel — circular bar / beat indicator with
    /// sub-divisions, rendered from the current sequencer step + step
    /// fraction.  UI-only; no DSP.  Companion to `EventStream` —
    /// glanceable indicator of where in the bar we are.
    PhaseWheel,
    /// Real-time note / drum event stream — mirrors the header event
    /// stream widget.  Rendered as a rack module so multiple instances
    /// can show different scroll speeds / focus voices in future
    /// iterations.  V1: one canonical instance reads the global
    /// AppState log buffers.
    EventStream,
    /// Pattern density heatmap — rows = sequencer voices, columns =
    /// 16 step positions, brightness = active state of the step.
    /// Glanceable "where are the busy parts" overview across all
    /// voices in the current pattern; complementary to the focused
    /// per-voice sequencer panels.  Pure UI; no DSP, no audio I/O.
    PatternHeatmap,
    /// Onset / beat-grid overlay — rolling envelope of the master
    /// audio for the last bar with the 16 sequencer step-tick
    /// vertical lines drawn through it.  Glanceable groove drift
    /// indicator: peaks aligned with ticks = on grid, peaks
    /// drifting before / after = early / late strikes.  Pure UI;
    /// reads `granular_tap` (already maintained for the CAPTURE
    /// path) so no new DSP plumbing.
    OnsetGrid,
    // ── Modulation ────────────────────────────────────────────────────────────
    LfoModule,
    /// CV sequencer — 16-step CV pattern that drives a chosen
    /// modulation target, advancing in lock-step with the audio
    /// step clock.  Distinct from `LfoModule` (continuous
    /// waveform): a hand-drawn per-step value table for stepped
    /// filter sweeps, gate-like duck patterns, pitch transposition.
    CvSequencer,
    /// CV-sequence visualiser — focused, dedicated viz for a
    /// CvSequencer slot's per-step output.  Renders the
    /// scaled/bipolar value (`(step - 0.5) * 2 * depth`) as a
    /// stair-step waveform with a live playhead, picking up its
    /// source slot via incoming CV cable from a `CvSequencer`
    /// (or falling back to the first enabled slot when unwired).
    /// V2 polish — the existing CvSequencer panel already shows
    /// 16 step bars in-place; this is the dedicated companion
    /// for users who want a clean focused readout while
    /// performing.
    CvSeqScope,
    /// Slew / glide CV utility — smooths an incoming CV signal
    /// with separate attack (rise) and release (fall) time
    /// constants.  Distinct from the bass voice's portamento:
    /// works on any CV stream cabled into its Mod-In, then the
    /// smoothed output cables out to a synth / FX param.
    Slew,
    /// Quantizer CV utility — snap an incoming bipolar CV
    /// (-1..+1 → -12..+12 semitones) to the nearest note in the
    /// configured scale.  Useful for turning continuous LFO
    /// sweeps into pitch-quantised arpeggios.
    Quantizer,
    /// Comparator CV utility — output 1.0 when input > threshold,
    /// 0.0 otherwise.  Turns an LFO / envelope into a gate
    /// signal driving another modulation target.
    Comparator,
    /// Sample-and-hold CV utility — latch the incoming CV value
    /// on each new sequencer step (the "clock edge"), hold until
    /// the next step.  Distinct from the LFO's S&H waveform
    /// (which re-latches on its own LFO phase wrap): this one
    /// re-latches on the audio sequencer's grid so the held
    /// value is always musically aligned.
    SampleHold,
    /// Math CV utility — combine two CV inputs with a chosen
    /// operation (add / multiply / blend / max / min).  Two
    /// Mod-In ports per instance.
    Math,
    LlmAgent,
    // ── LLM console (singleton, Global zone) ──────────────────────────────
    LlmConsole,
    //── Utility ───────────────────────────────────────────────────────────────
    MasterOutput,
}

impl ModuleKind {
    /// Short display label shown in the module card title bar.
    pub fn label(self) -> &'static str {
        match self {
            Self::AcidBass => "BASS SYNTH",
            Self::DrumKit808 => "808 KIT",
            Self::DrumKit909 => "909 KIT",
            Self::HooverLead => "HOOVER",
            Self::PluckString => "PLUCK",
            Self::WavetableVoice => "WAVETABLE",
            Self::SampleInstrument => "SAMPLER+",
            Self::An1xVoice => "AN1X",
            Self::AmenSampler => "AMEN",
            Self::NoiseVoice => "NOISE",
            Self::Theremin => "THEREMIN",
            Self::Pendulum => "PENDULUM",
            Self::FmOpsVoice => "FM OPS",
            Self::AdditiveVoice => "ADDITIVE",
            Self::ModalVoice => "MODAL",
            Self::ChiptuneVoice => "CHIPTUNE",
            Self::VocalVoice => "VOCAL",
            Self::GranularTexture => "GRANULAR",
            Self::GabberKick => "GABBER KICK",
            Self::NeuTts => "TTS VOICE",
            Self::StepSequencer => "SEQUENCER",
            Self::FxReverb => "REVERB",
            Self::FxDelay => "DELAY",
            Self::FxChorus => "CHORUS",
            Self::FxPhaser => "PHASER",
            Self::FxFlanger => "FLANGER",
            Self::FxLimiter => "LIMITER",
            Self::FxFilter => "FILTER",
            Self::FxComb => "COMB",
            Self::FxTilt => "TILT EQ",
            Self::FxTransient => "TRANSIENT",
            Self::FxExciter => "EXCITER",
            Self::FxMultitap => "MULTITAP",
            Self::FxRevDelay => "REV DELAY",
            Self::FxTapeStop => "TAPE STOP",
            Self::FxStutter => "STUTTER",
            Self::FxFreeze => "FREEZE",
            Self::FxRingMod => "RING MOD",
            Self::FxWaveshaper => "WAVESHAPER",
            Self::FxBitcrush => "BITCRUSH",
            Self::FxEq => "EQ",
            Self::FxCompressor => "COMPRESSOR",
            Self::FxGate => "GATE",
            Self::FxVocoder => "VOCODER",
            Self::FxTapeSat => "TAPE SAT",
            Self::FxDrive => "DRIVE",
            Self::FxAutotune => "AUTOTUNE",
            Self::FxPan => "PAN",
            Self::FxWiden => "WIDEN",
            Self::FxConvReverb => "CONV REV",
            Self::FxParamEq => "PARAM EQ",
            Self::FxPitchShift => "PITCH",
            Self::FxFreqShift => "FREQSHIFT",
            Self::FxVinyl => "VINYL",
            Self::FxDjFilter => "DJ FILTER",
            Self::FxTremolo => "TREMOLO",
            Self::FxVibrato => "VIBRATO",
            Self::FxIsoEq => "ISO EQ",
            Self::FxDeEsser => "DE-ESSER",
            Self::FxResBank => "RES BANK",
            Self::FxTapeEcho => "TAPE ECHO",
            Self::FxMultibandComp => "MB COMP",
            Self::FxGrainDelay => "GRAIN DEL",
            Self::FxSpectralGate => "SPEC GATE",
            Self::SpectrumAnalyzer => "SPECTRUM",
            Self::StereoMeter => "STEREO METER",
            Self::ActivityTimeline => "TIMELINE",
            Self::BarOscilloscope => "OSCILLOSCOPE",
            Self::StereoVectorscope => "GONIOMETER",
            Self::LfoScope => "LFO SCOPE",
            Self::PitchTracker => "TUNER",
            Self::ChordDisplay => "CHORD",
            Self::Spectrogram => "SPECTROGRAM",
            Self::LoudnessMeter => "LUFS",
            Self::PhaseWheel => "PHASE",
            Self::EventStream => "EVENT STREAM",
            Self::PatternHeatmap => "PATTERN MAP",
            Self::OnsetGrid => "ONSET GRID",
            Self::LfoModule => "LFO",
            Self::CvSequencer => "CV SEQ",
            Self::CvSeqScope => "CV SCOPE",
            Self::Slew => "SLEW",
            Self::Quantizer => "QUANTIZER",
            Self::Comparator => "COMPARATOR",
            Self::SampleHold => "S&H",
            Self::Math => "MATH",
            Self::LlmAgent => "LLM AGENT",
            Self::LlmConsole => "LLM CONSOLE",
            Self::MasterOutput => "MASTER",
        }
    }

    /// Grid size in (columns, rows) for the 12-column rack grid.
    /// Full-width modules use `grid_cols` for width; all others are fixed.
    /// Height is enforced as a minimum — content taller than this grows naturally.
    pub fn grid_size(self, grid_cols: u8) -> (u8, u8) {
        match self {
            //                                     W     H
            Self::StepSequencer => (grid_cols, 2),
            // LlmConsole: 2 rows.  The content (square cycle-viz widget +
            // lane-score strip + prompt/log panel) needs more than a
            // single grid cell of height at typical col_w values —
            // rendered content otherwise spills past its allocated slot
            // into the zone below, where the next zone's backdrop / the
            // first card paints over the overflow and the console
            // partly disappears.
            Self::LlmConsole => (grid_cols, 2),
            // Master: single row with MASTER volume + 6 mid/side
            // rotaries + the voice-presence strip on the right.
            Self::MasterOutput => (grid_cols, 1),
            Self::AcidBass => (4, 7),
            // Drum kits are 4 rows tall so the 3 per-voice glass groups
            // (kick / snare / hihat or clap) each with an XY pad beside
            // their knobs don't get clipped / scrollbar'd.
            Self::DrumKit808 => (4, 5),
            Self::DrumKit909 => (4, 4),
            Self::HooverLead => (4, 2),
            // Pluck — few knobs (damp/bright/vol/pan/offset + on-off),
            // comfortably fits in 3x2.
            Self::PluckString => (3, 2),
            // Wavetable — pos/phase + vol/pan/pitch + LOAD button +
            // filename label.  Same 3x2 envelope as Pluck.
            Self::WavetableVoice => (3, 2),
            // Sample Instrument — V2 Stage 6 added a per-voice filter
            // row, Stage 7 added a viz strip (zone map in SFZ mode,
            // waveform thumbnail in single-WAV mode); the card now
            // sits at 3×5.
            Self::SampleInstrument => (3, 5),
            Self::An1xVoice => (6, 6),
            Self::AmenSampler => (3, 3),
            Self::NoiseVoice => (2, 1),
            // Theremin: 3 cols wide so the XY pad has room to be
            // square-ish; 3 rows tall — pad takes most of it,
            // bottom row hosts the four small knobs.
            Self::Theremin => (3, 3),
            // Pendulum: 3×2 — top row knob bank (PITCH / DETUNE /
            // MIX / VOL / PAN), bottom row carries the live beat-rate
            // readout label.
            Self::Pendulum => (3, 2),
            // FM operator synth — header row + 4 op rows (each with
            // 6 knobs: RATIO / LEVEL / ATTACK / DECAY / SUSTAIN /
            // RELEASE).  6 cols wide gives the per-op rows breathing
            // room; 5 rows tall fits header + 4 ops.
            Self::FmOpsVoice => (6, 5),
            // Additive synth — header row (ON/OFF + ADSR + vol +
            // pan) above a 16-bar harmonic histogram.  6 cols wide
            // so the histogram has ~30 px per partial slider with
            // breathing room; 3 rows tall is plenty.
            Self::AdditiveVoice => (6, 3),
            // Modal voice — same shape as Additive: header row
            // (ON/OFF + voice fields + preset cycle) above an
            // 8-bar mode-amplitude histogram.  6×3 keeps the
            // histogram sliders comfortable to drag without
            // dominating the rack.
            Self::ModalVoice => (6, 3),
            // Chiptune voice — header row (ON/OFF + flags) +
            // 3 oscillator rows + 1 filter row.  6 cols wide
            // gives each oscillator's WAVE+LEVEL+ADSR (6 knobs)
            // breathing room.
            Self::ChiptuneVoice => (6, 5),
            // Vocal voice — single-row voice (only 7 controls);
            // header row + ADSR + vowel cycle + brightness +
            // shift fits 5 cols × 3 rows comfortably.
            Self::VocalVoice => (5, 3),
            Self::GranularTexture => (3, 2),
            Self::GabberKick => (3, 2),
            Self::LlmAgent => (3, 2),
            Self::NeuTts => (2, 3),
            Self::SpectrumAnalyzer => (4, 2),
            Self::ActivityTimeline => (4, 2),
            Self::StereoMeter => (2, 1),
            // Bar oscilloscope — wide enough that its phosphor trails
            // read clearly without dominating the rack.  Same envelope
            // as the spectrum module so they line up when stacked.
            Self::BarOscilloscope => (4, 2),
            // Stereo vectorscope — square card so the lissajous lobes
            // aren't squashed into an oval.
            Self::StereoVectorscope => (2, 2),
            // LFO scope — same 4×2 envelope as the bar scope so the
            // two viz modules line up visually when stacked.
            Self::LfoScope => (4, 2),
            // Tuner — compact, single big note + cents needle.
            Self::PitchTracker => (2, 2),
            // Chord display — slightly wider so "Cm7" / "F#maj"
            // readouts have room to breathe.
            Self::ChordDisplay => (3, 2),
            // Spectrogram — wide so the time axis has room to scroll;
            // matches the spectrum-analyzer footprint at 4×2 so the two
            // align visually when stacked.
            Self::Spectrogram => (4, 2),
            // LUFS — single-row meter with two big numbers.
            Self::LoudnessMeter => (2, 2),
            // Phase wheel — square so the circle isn't squashed.
            Self::PhaseWheel => (2, 2),
            // Event stream — wider than the analysis modules so notes
            // have horizontal room to scroll past.  Two rows tall so
            // the future / past split is legible.
            Self::EventStream => (6, 2),
            // Pattern heatmap — 16 step columns + a left label column;
            // tall enough to show several voice rows comfortably.
            Self::PatternHeatmap => (5, 3),
            // Onset grid — wide envelope strip; one row tall is enough.
            Self::OnsetGrid => (5, 2),
            Self::LfoModule => (2, 2),
            // CV sequencer — 16 step bars need horizontal room.
            Self::CvSequencer => (5, 2),
            // CV-seq scope — same shape as LfoScope (4×2) so the
            // companion viz reads as a sibling.
            Self::CvSeqScope => (4, 2),
            // Slew — 2 knobs, compact card.
            Self::Slew => (2, 1),
            // Quantizer — root + scale dropdowns, 2×1 fits.
            Self::Quantizer => (2, 1),
            // Comparator — single threshold knob.
            Self::Comparator => (2, 1),
            // S&H — knobless, just an enabled toggle.
            Self::SampleHold => (2, 1),
            // Math — op cycle button + blend knob, 2×1 fits.
            Self::Math => (2, 1),
            // FX modules — exhaustive so new variants cause a compile error
            Self::FxDelay => (2, 2), // 5-button row can't fit in 1 row
            // Convolution Reverb — 6 knobs in a glass-grouped 2-row
            // bank (3 + 3) and a button row beneath (REV / LOAD IR /
            // clear / filename).  3 cols wide keeps the card the
            // same footprint as the rest of the FX strip; 3 rows
            // gives the prominent LOAD IR button breathing room.
            Self::FxConvReverb => (3, 3),
            // Param EQ — curve editor dominates the card; 4 cols gives
            // the 20 Hz–20 kHz axis room to breathe, 4 rows leaves room
            // for the curve + band-param readout strip below it.
            Self::FxParamEq => (4, 4),
            // PitchShift — SHIFT / MIX / FBK on row 1, FINE on row 2
            // so the semitone knob doesn't crowd the cents readout.
            Self::FxPitchShift => (2, 2),
            // FreqShift — 3 knobs (SHIFT, FEEDBACK in a glass group +
            // big MIX) all in one row, so 2×1 is enough.
            Self::FxFreqShift => (2, 1),
            // Flanger — 4 knobs (rate / depth / feedback / mix) is one
            // more than the phaser bundle, so it gets a 2×2 footprint
            // like the delay rather than the 2×1 used by the smaller FX.
            Self::FxFlanger => (2, 2),
            // Limiter, Filter, Comb each have 4 knobs (some plus a mode
            // selector); they need 2×2 to fit two rows.
            Self::FxLimiter => (2, 2),
            Self::FxFilter => (2, 2),
            Self::FxComb => (2, 2),
            // 4 knobs each — 2 rows of 2.
            Self::FxMultitap => (2, 2),
            Self::FxRevDelay => (2, 2),
            Self::FxStutter => (2, 2),
            Self::FxReverb
            | Self::FxChorus
            | Self::FxPhaser
            | Self::FxTilt
            | Self::FxTransient
            | Self::FxExciter
            | Self::FxTapeStop
            | Self::FxFreeze
            | Self::FxRingMod
            | Self::FxWaveshaper
            | Self::FxBitcrush
            | Self::FxEq
            | Self::FxCompressor
            | Self::FxTapeSat
            | Self::FxDrive
            | Self::FxAutotune
            | Self::FxPan
            | Self::FxWiden
            | Self::FxVinyl
            | Self::FxDjFilter
            | Self::FxTremolo
            | Self::FxVibrato
            | Self::FxIsoEq
            | Self::FxDeEsser
            | Self::FxResBank
            | Self::FxTapeEcho
            | Self::FxMultibandComp
            | Self::FxGrainDelay
            | Self::FxSpectralGate => (2, 1),
            // Gate has 4 knobs (threshold / attack / release / depth) +
            // mix — 2 rows.
            Self::FxGate => (2, 2),
            // Vocoder — 4 knobs (BANDS / CARRIER / SENSE / MIX) all on
            // one row; 2×1 matches the rest of the Tier-1 FX strip.
            Self::FxVocoder => (2, 1),
        }
    }

    /// Which zone this module belongs to by default.
    pub fn default_zone(self) -> Zone {
        match self {
            Self::LlmConsole | Self::LlmAgent => Zone::Ai,
            Self::StepSequencer | Self::MasterOutput => Zone::Global,
            Self::AcidBass
            | Self::DrumKit808
            | Self::DrumKit909
            | Self::HooverLead
            | Self::PluckString
            | Self::WavetableVoice
            | Self::SampleInstrument
            | Self::An1xVoice
            | Self::AmenSampler
            | Self::NoiseVoice
            | Self::Theremin
            | Self::Pendulum
            | Self::FmOpsVoice
            | Self::AdditiveVoice
            | Self::ModalVoice
            | Self::ChiptuneVoice
            | Self::VocalVoice
            | Self::GranularTexture
            | Self::NeuTts
            | Self::GabberKick => Zone::Voice,
            Self::FxReverb
            | Self::FxDelay
            | Self::FxChorus
            | Self::FxPhaser
            | Self::FxFlanger
            | Self::FxLimiter
            | Self::FxFilter
            | Self::FxComb
            | Self::FxTilt
            | Self::FxTransient
            | Self::FxExciter
            | Self::FxMultitap
            | Self::FxRevDelay
            | Self::FxTapeStop
            | Self::FxStutter
            | Self::FxFreeze
            | Self::FxRingMod
            | Self::FxWaveshaper
            | Self::FxBitcrush
            | Self::FxEq
            | Self::FxCompressor
            | Self::FxGate
            | Self::FxVocoder
            | Self::FxTapeSat
            | Self::FxDrive
            | Self::FxAutotune
            | Self::FxPan
            | Self::FxWiden
            | Self::FxConvReverb
            | Self::FxParamEq
            | Self::FxPitchShift
            | Self::FxFreqShift
            | Self::FxVinyl
            | Self::FxDjFilter
            | Self::FxTremolo
            | Self::FxVibrato
            | Self::FxIsoEq
            | Self::FxDeEsser
            | Self::FxResBank
            | Self::FxTapeEcho
            | Self::FxMultibandComp
            | Self::FxGrainDelay
            | Self::FxSpectralGate
            | Self::SpectrumAnalyzer
            | Self::StereoMeter
            | Self::ActivityTimeline
            | Self::BarOscilloscope
            | Self::StereoVectorscope
            | Self::LfoScope
            | Self::PitchTracker
            | Self::ChordDisplay
            | Self::Spectrogram
            | Self::LoudnessMeter
            | Self::PhaseWheel
            | Self::EventStream
            | Self::PatternHeatmap
            | Self::OnsetGrid
            | Self::LfoModule
            | Self::CvSequencer
            | Self::CvSeqScope
            | Self::Slew
            | Self::Quantizer
            | Self::Comparator
            | Self::SampleHold
            | Self::Math => Zone::FxMod,
        }
    }

    /// True if this module produces an audio bus signal (voice or FX).  Used by
    /// the back-panel "reaches MASTER" indicator — modules without an audio
    /// output (sequencer, LFO, agents, meters) are not part of the audio
    /// graph and their LED is hidden entirely.
    pub fn has_audio_output(self) -> bool {
        matches!(
            self,
            Self::AcidBass
                | Self::HooverLead
                | Self::PluckString
                | Self::WavetableVoice
                | Self::SampleInstrument
                | Self::DrumKit808
                | Self::DrumKit909
                | Self::AmenSampler
                | Self::GranularTexture
                | Self::GabberKick
                | Self::NoiseVoice
                | Self::Theremin
                | Self::Pendulum
                | Self::FmOpsVoice
                | Self::AdditiveVoice
                | Self::ModalVoice
                | Self::ChiptuneVoice
                | Self::VocalVoice
                | Self::An1xVoice
                | Self::NeuTts
                | Self::FxReverb
                | Self::FxDelay
                | Self::FxChorus
                | Self::FxPhaser
                | Self::FxFlanger
                | Self::FxLimiter
                | Self::FxFilter
                | Self::FxComb
                | Self::FxTilt
                | Self::FxTransient
                | Self::FxExciter
                | Self::FxMultitap
                | Self::FxRevDelay
                | Self::FxTapeStop
                | Self::FxStutter
                | Self::FxFreeze
                | Self::FxRingMod
                | Self::FxWaveshaper
                | Self::FxBitcrush
                | Self::FxEq
                | Self::FxCompressor
                | Self::FxGate
                | Self::FxVocoder
                | Self::FxTapeSat
                | Self::FxDrive
                | Self::FxAutotune
                | Self::FxPan
                | Self::FxWiden
                | Self::FxConvReverb
                | Self::FxParamEq
                | Self::FxPitchShift
                | Self::FxFreqShift
                | Self::FxVinyl
                | Self::FxDjFilter
                | Self::FxTremolo
                | Self::FxVibrato
                | Self::FxIsoEq
                | Self::FxDeEsser
                | Self::FxResBank
                | Self::FxTapeEcho
                | Self::FxMultibandComp
                | Self::FxGrainDelay
                | Self::FxSpectralGate
        )
    }

    /// True if this module exposes a sidechain audio input port.  Sidechain
    /// FX consume a tap from another voice / FX as their detector or
    /// modulator without that source feeding the forward chain.  Used
    /// by the rack UI (renders the extra input jack) and by
    /// `compile_fx_plan` (treats the cable as a sidechain edge instead
    /// of a regular send).
    pub fn has_sidechain_in(self) -> bool {
        matches!(self, Self::FxCompressor | Self::FxGate | Self::FxVocoder)
    }

    /// True for FX modules that offer an XY pad in the expanded view.
    /// Modules returning true get an extra grid row when `pad_expanded` is
    /// true on their `RackModule`, plus a chevron toggle in the title bar.
    pub fn supports_xy_pad(self) -> bool {
        matches!(
            self,
            Self::FxReverb
                | Self::FxDelay
                | Self::FxChorus
                | Self::FxPhaser
                | Self::FxFlanger
                | Self::FxLimiter
                | Self::FxFilter
                | Self::FxComb
                | Self::FxTilt
                | Self::FxTransient
                | Self::FxExciter
                | Self::FxMultitap
                | Self::FxRevDelay
                | Self::FxTapeStop
                | Self::FxStutter
                | Self::FxFreeze
                | Self::FxRingMod
                | Self::FxWaveshaper
                | Self::FxBitcrush
                | Self::FxEq
                | Self::FxCompressor
                | Self::FxGate
                | Self::FxVocoder
                | Self::FxTapeSat
                | Self::FxDrive
                | Self::FxAutotune
                | Self::FxPan
                | Self::FxWiden
                | Self::FxConvReverb
                | Self::FxPitchShift
                | Self::FxFreqShift
                | Self::FxVinyl
                | Self::FxDjFilter
                | Self::FxTremolo
                | Self::FxVibrato
                | Self::FxIsoEq
                | Self::FxDeEsser
                | Self::FxResBank
                | Self::FxTapeEcho
                | Self::FxMultibandComp
                | Self::FxGrainDelay
                | Self::FxSpectralGate
        )
    }

    /// Whether this module type may have more than one instance in the rack.
    pub fn allows_multiple(self) -> bool {
        matches!(
            self,
            Self::FxReverb
                | Self::FxDelay
                | Self::FxChorus
                | Self::FxPhaser
                | Self::FxFlanger
                | Self::FxLimiter
                | Self::FxFilter
                | Self::FxComb
                | Self::FxTilt
                | Self::FxTransient
                | Self::FxExciter
                | Self::FxMultitap
                | Self::FxRevDelay
                | Self::FxTapeStop
                | Self::FxStutter
                | Self::FxFreeze
                | Self::FxRingMod
                | Self::FxWaveshaper
                | Self::FxBitcrush
                | Self::FxEq
                | Self::FxCompressor
                | Self::FxGate
                | Self::FxVocoder
                | Self::FxTapeSat
                | Self::FxDrive
                | Self::FxAutotune
                | Self::FxPan
                | Self::FxWiden
                | Self::FxConvReverb
                | Self::FxParamEq
                | Self::FxPitchShift
                | Self::FxFreqShift
                | Self::FxVinyl
                | Self::FxDjFilter
                | Self::LfoModule
                | Self::CvSequencer
                | Self::CvSeqScope
                | Self::Slew
                | Self::Quantizer
                | Self::Comparator
                | Self::SampleHold
                | Self::Math
                | Self::LlmAgent
        )
    }
}

// ─── Zone ────────────────────────────────────────────────────────────────────

/// Fixed grid column count for the rack layout.
pub const GRID_COLS: u8 = 12;

/// The vertical zone a module lives in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Zone {
    /// LLM console + agents.
    Ai,
    /// Clock/sequencer, master output — the main audio strip.
    /// Labelled "MAIN AUDIO" in the UI; the enum name is kept for serde
    /// backward compat with pre-split sessions.
    Global,
    /// Voice / instrument modules.
    Voice,
    /// FX processors and modulation sources.
    FxMod,
}

impl Zone {
    /// Lowercase scroll-target name (`"ai"`, `"global"`, `"voice"`, `"fxmod"`).
    pub fn scroll_name(self) -> &'static str {
        match self {
            Zone::Ai => "ai",
            Zone::Global => "global",
            Zone::Voice => "voice",
            Zone::FxMod => "fxmod",
        }
    }
}
