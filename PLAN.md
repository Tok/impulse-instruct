# Impulse Instruct - Roadmap

What's already built is documented in [docs/features.md](docs/features.md).
This file lists **future** work only - completed items are removed once
they ship and are reflected in `features.md`.

---

## Sequencer

## UI / UX


## FX — missing modules

Tier 1 (small, well-defined, no new infra):
- [x] **Flanger (`FxFlanger`)** — shipped.
- [x] **Brick-wall limiter (`FxLimiter`)** — shipped.
- [x] **State-variable filter (`FxFilter`)** — shipped (LP/BP/HP/Notch
  via mode selector; fields named `svf_*` to avoid collision with
  per-voice filter knobs).
- [x] **Comb resonator (`FxComb`)** — shipped.
- [x] **Tilt EQ (`FxTilt`)** — shipped.
- [x] **Transient designer (`FxTransient`)** — shipped.
- [x] **Exciter (`FxExciter`)** — shipped.
- [x] **Frequency shifter (`FxFreqShift`)** — shipped.  Two
  4-section parallel allpass cascades (H(z) = (a + z⁻²) / (1 +
  a·z⁻²) per section) produce the analytic-signal real / imag
  pair; complex-multiply with a `cos / sin` carrier at ±1000 Hz
  gives the SSB-shifted output.  Coefficients are the
  Hartmann-style pair (~1° phase error in 100 Hz–20 kHz).
  Feedback knob (capped at 0.85) + tanh-clamped feedback tap
  prevent runaway under sustained input.  Distinct from
  `FxPitchShift` — adds the same Hz to every component, so
  harmonics become inharmonic.
- [x] **Stereo widener (`FxWiden`)** — shipped via the
  master-stage latch pattern (mirrors `FxPan` / `FxConvReverb`'s
  side-latch idiom).  Chain step is a mono passthrough that flips
  `fx_widen_active = true` and copies the live haas / side / mix
  knobs; the master stage applies a Haas delay (0–30 ms ring on
  the L channel's mid component) plus side scaling (1–3× on the
  existing mid/side decomposition) before L/R recombination.
  Avoids converting the entire chain to stereo I/O.

Tier 2 (need a sidechain / second input port — solve once,
share):
- [x] **Sidechain port plumbing in `RackState`** — shipped.
  `PortKind::SidechainIn` (5th port kind), `connect_sidechain()`
  helper, cycle-check exemption (sidechain edges read with a
  one-sample delay so cycles are safe by construction).
  `FxPlan.sidechain_routes: HashMap<FxStep, SidechainSource>`
  with `SidechainSource::{Voice(ModuleKind), Fx(FxStep)}` —
  resolved by `compile_fx_plan` from `to.kind == SidechainIn`
  cables.  Audio thread carries a `SidechainSnap` snapshot
  alongside `VoiceSendsSnap` / feedback array, refreshed each
  sample after voices process.
- [x] **Vocoder (`FxVocoder`)** — shipped.  16-band channel
  vocoder (log-spaced 100 Hz → 8 kHz, fixed Q ≈ 3); per-band
  envelope follower on the modulator drives gain on the matching
  carrier band.  Knobs: BANDS (active fraction), CRR.MX (dry
  carrier blend for talkbox flavour), SENSE (detector gain),
  MIX.  Pairs with `NeuTts` for talkbox patches.
- [x] **Noise gate / ducker (`FxGate`)** — shipped.  Detector on
  sidechain (or main signal when unconnected — falls back to
  noise-gate flavour).  Asymmetric one-pole envelope drives
  threshold-gated gain reduction.  Knobs: THR (-60..0 dBFS),
  ATK (0.5..50 ms), REL (10..500 ms), DEPTH, MIX.
- [x] **Sidechain compressor mode on `FxCompressor`** — shipped.
  `compressor_sidechain` boolean flag; `process_with_detector`
  reads detector from the sidechain cable but applies gain
  reduction to the input.  Falls back gracefully to self-detect
  when no cable is connected.  Multiband sidechain not in V1 (the
  3-band path keeps self-detecting; users disable multiband when
  they enable sidechain).

Tier 3 (heavier — build once Tier 1+2 settle):
- [x] **Multitap delay (`FxMultitap`)** — shipped, 4 fixed taps
  with knob-controlled spread.  Per-tap pan + filter from the
  original spec is deferred — the simpler 4-tap mono variant
  covers the rhythmic-dub use case.
- [x] **Reverse delay (`FxRevDelay`)** — shipped, ping-pong
  segment buffer (one fills while the other plays back reversed).
- [x] **Stutter / glitch repeater (`FxStutter`)** — shipped,
  BPM-synced (1/4, 1/8, 1/16, 1/32 quartiles).
- [x] **Tape stop (`FxTapeStop`)** — shipped, mix knob doubles as
  ramp progress (0=normal, 1=halted) with darkening lowpass that
  tracks the slowing.
- [x] **Spectral freezer (`FxFreeze`)** — shipped.  Captures one FFT
  frame on rising-edge engage; resynths with random phases per hop
  via overlap-add (1024 FFT, 256 hop, Hann window).
- [x] **Cabinet IR mode** — shipped as a flag on `FxConvReverb`
  (`conv_reverb_cabinet: bool`).  When true, caps `conv_reverb_size`
  internally at 0.1 (10 % of loaded IR) and the file picker
  browses `samples/cabinets/` instead of `samples/impulses/`.
- [x] **Mid/side EQ flag on `FxParamEq`** — shipped via the same
  master-stage latch pattern.  When `param_eq_ms_mode` is on, the
  chain's ParamEq step is a passthrough that flags
  `param_eq_ms_active = true`; the master stage runs two extra
  `ParamEq` cascades (`param_eq_mid` + `param_eq_side`) on the
  decoded mid + side channels of the final L/R, using the same
  band list.  UI: `MN`/`M/S` toggle on the ParamEq band-readout
  strip.
- [-] **Shimmer mode flag on `FxConvReverb`** — *deferred*.
  Pitch-shift (+12 / +7) in the feedback loop is doable but needs
  careful integration with `ConvReverb`'s data flow + an extra
  `PitchShift` instance inside the convolver; bigger than a flag,
  smaller than a new module.  Pick up when ConvReverb gets
  attention next.

## Visualizations — new analysis modules

Tier 1 (cheap, reuse existing buffers):
- [x] **Goniometer / vectorscope (`StereoVectorscope`)** — shipped.
- [x] **LFO scope (`LfoScope`)** — shipped (V1 picks the first enabled
  LFO slot; CV-cable wiring of slot ↔ module is a follow-up).
- [x] **Tuner (`PitchTracker`)** — shipped, autocorrelation-based
  pitch detect with cents-off needle.
- [x] **Chord / key display (`ChordDisplay`)** — shipped, chroma
  vector + 24-template (major/minor) match.
- [x] **Per-voice activity heatmap** — shipped as an optional
  bottom-strip overlay on `EventStream`.  Reads `melodic_log` /
  `drum_log` queues; rows: BASS (folded across all bass voices) /
  AN1X / HOOV / KICK / SN / HAT / CLAP, time-binned per
  sequencer step and grayscale-shaded by recent-activity
  intensity.  Toggled via `ui_prefs.stream_heatmap` (default
  off; Preferences → Display → "Per-voice heatmap strip").
  `MelodicLogEntry` gained a `voice: MelodicVoice` field so the
  bins can split per source voice.

Tier 2 (heavier — guard with reduce-cost path):
- [x] **Spectrogram waterfall (`Spectrogram`)** — shipped, rolling
  FFT history rendered as a fresh `egui::ColorImage` per repaint
  with log-frequency Y axis.
- [x] **Loudness / LUFS meter (`LoudnessMeter`)** — shipped,
  K-weighted (BS.1770 hard-coded 48 kHz coefficients) momentary +
  short-term EMAs.  Integrated LUFS (gated) deferred — momentary
  + short-term covers the meter use case.
- [x] **Transport phase wheel (`PhaseWheel`)** — shipped, circular
  bar/beat indicator with beat-tick highlights.
- [-] **Phase correlation strip (`CorrMeter`)** — *not separately
  shipped*; the existing `StereoMeter` already shows correlation +
  L/R balance as a horizontal strip, which covers the same use
  case more comprehensively.  Re-open if a slimmed-down
  correlation-only variant becomes necessary.
- [-] **Lissajous-3D / oscilloscope-3D depth** — *deferred*; was
  marked "Optional" in the original plan.  The shipped
  `StereoVectorscope` (Tier 1) covers the goniometer use case;
  adding fading-polyline depth is pure eye-candy and can come
  back if visual demand surfaces.

## New module — `SampleInstrument` (V1 shipped)

V1 has shipped — see commit history.  V1 scope as built:

- New `ModuleKind::SampleInstrument` (Voice zone, single instance per
  rack, label "SAMPLER+").
- `SampleInstrumentState` (path, root_note, volume, pan,
  pitch_offset_cents) wired into `AppState`.
- DSP voice in `audio/dsp/sample_instrument.rs` — linear-interp
  resample, AR amp envelope, always-loops the buffer.  Pitch ratio =
  `2^((played_note − root_note) / 12)`.
- New `AudioCommand::LoadSampleInstrument(Arc<Vec<f32>>)` + handler.
- Own sequencer lane: `sequencer.sample_pattern` + `sample_steps`.
- New `TriggerEvent::SampleTrigger / SampleGateOff` with the standard
  gate-counter machinery in `sequencer/mod.rs`.
- Panel: ON/OFF + LOAD WAV + filename + ROOT note + VOL/PAN/TRIM knobs.
- LLM apply path covers `sample.{enabled, root_note, volume, pan,
  pitch_offset_cents, sample_steps, sample_notes}`.
- 7 unit tests cover defaults, ModuleKind metadata, voice load /
  trigger / resample, LLM apply.

V1.1 — shipped:

- [x] Auto-detect root note via `detect_pitch_hz` on Load button.
  Confidence ≥ 0.5 sets the root MIDI note; manual root knob still
  wins for subsequent edits.
- [x] Loop start / end fractions + `loop_enabled` toggle.  When
  `loop_end ≤ loop_start` (or toggle off) the voice plays one-shot
  and falls silent at the buffer end.
- [x] Full ADSR (4-stage state machine with attack / decay / sustain
  / release knobs).  Defaults match the V1 AR shape so older
  sessions preserve behaviour.
- [x] Sample lane rendered in the sequencer panel (SAMP row mirrors
  the Wavetable lane shape).
- [x] `/api/sample` HTTP endpoint with `path` or `random` body, plus
  `scan_sample_instrument_samples` / `pick_random_sample_instrument`
  helpers backing it.

V2 status — most stages shipped:

- [x] Stage 1 — SFZ parser (subset per `src/state/sfz.rs`).
- [x] Stage 2 — load SFZ + single-zone playback.
- [x] Stage 3 — polyphony (8 slots) + oldest-steal allocator.
- [x] Stage 4 — multi-zone overlap layering.
- [x] Stage 5 — velocity layers + round-robin.
- [x] Stage 6 — per-voice SVF (LP/BP/HP) + LFO routing
  (`SampleVolume` / `SamplePan` / `SamplePitch` / `SampleCutoff`).
- [x] Stage 7 — zone-map + waveform-thumb visualizer strip.
- [x] Stage 8 — formant-preserving pitch shift.  Per-slot
  phase-vocoder DSP in `src/audio/dsp/formant_shifter.rs`: STFT
  (FFT 512, hop 128, 75 % OLA), moving-average smoothed
  log-magnitude as the spectral envelope, whitened excitation
  shifted by ratio in the bin domain with phase-vocoder coherence,
  then re-multiplied by the *original* envelope so formants stay
  anchored.  Allocated up-front per slot; the realtime path is
  alloc-free.  Engages when `sample_formant_preserve` is on; the
  cheap linear-resample path stays the default.
- [x] Stage 9 — starter sample pack scaffolding
  (`samples/instruments/{,starter,README.md}` with author + free-pack
  source links).  Curation of the actual bundled CC0 content is the
  user's call.

Time-stretch decoupled from pitch (separate from formant preservation
in the original plan) is still TODO — currently the loop_start/end
window controls sustain duration but the engine has no real
time-stretch DSP.

## New module — `SampleInstrument` (V1 implementation notes)

A keyboard-playable sampler instrument: load one or more pitched
recordings (piano notes, vocal phrases, drum hits, ...) and play
them back across the keyboard, pitch-shifted to the incoming
sequencer / MIDI note. Fills the gap between existing modules:

| Existing       | Plays back                      | Pitch driven by         |
|----------------|---------------------------------|-------------------------|
| `WavetableVoice` | a *single-cycle* waveform     | phase increment         |
| `AmenSampler`    | breakbeat slices              | original pitch (slice rate) |
| `GranularTexture`| grain spray over a texture    | non-pitched / detune knob |
| **`SampleInstrument`** *(new)* | a pitched recording / multisample bank | sequencer note → resample |

### Decisions baked in

- **`.sfz` is the canonical multisample format.** Text-based,
  parseable in a few hundred lines, and the largest pool of
  free CC-licensed sample libraries already ships in `.sfz`
  (Salamander Grand, Sonatina). `.sf2` is *not* in scope —
  bigger spec, mostly GM-flavoured, and the conversion ecosystem
  to `.sfz` is solid if a user really needs an SF2.
- **Single `.wav` mode is a degenerate `.sfz` of one zone.**
  Implement the SFZ engine first; the single-file path is just
  a one-zone synthetic SFZ generated on load.
- **Pitch detect: YIN.** Robust on inharmonic / percussive
  samples in a way autocorrelation isn't. Run once on load,
  off the audio thread.
- **Default zone = `Voice`.** Same as every other instrument.
  No new sub-zone; the rack is already crowded.
- **Polyphonic by default**, voice cap configurable
  (default 8). Mono / legato modes are knob options.

### Format support

- `.wav` — 8/16/24/32 bit int + 32-bit float, mono + stereo
  (already supported via `hound`).
- `.flac` — needs `claxon` or `symphonia` (small extra crate).
- `.aiff` — optional, `symphonia` covers it.
- `.sfz` — opcode subset:
  `<region>` / `<group>`,
  `sample`, `lokey`/`hikey`/`pitch_keycenter`,
  `lovel`/`hivel`,
  `loop_mode`, `loop_start`, `loop_end`,
  `volume`, `pan`,
  `seq_position` / `seq_length` (round-robin),
  `tune`, `transpose`,
  `ampeg_attack`/`decay`/`sustain`/`release`,
  `cutoff`, `resonance`, `fil_type`.
  Anything outside the subset is logged + ignored, not an error.
- Drag-drop from file manager.
- Built-in browser for `samples/instruments/<library>/...`.

### Sample mapping

- **Single-sample mode** — one file, one root note, full
  keyboard span.
- **Multisample mode** — SFZ-style key zones, optional
  crossfade across adjacent zones (smoother than hard
  switching when stretched ratios meet at a boundary).
- **Velocity layers** — N bands per key zone, crossfade
  across velocity (soft → loud takes from different
  recordings).
- **Round-robin** — cycle through N samples on repeated
  triggers per zone, masks the "machine-gun effect" on
  drums.
- **Per-zone offsets** — sample start, end, loop start,
  loop end, loop crossfade length.

### Pitch handling

- **Source-pitch detection — YIN.** Auto-runs on load on a
  worker thread; sets `root_note` if confident, falls back to
  filename heuristics (`*_C4.wav`, `*-A2.wav`, ...) then to
  manual.
- **Pitch-shift by ratio-resampling** through the
  linear-interp path the granular voice already uses (cheap,
  fine for ±1 octave).
- **4-point Hermite** for higher quality on bigger stretches
  — toggle in the per-instrument settings.
- **PSOLA / phase-vocoder** option for formant preservation
  on vocal samples — separate code path, only activated when
  the user opts in (heavier).
- **Tuning**: coarse semi + fine cents.
- **Glide / portamento** — per-voice frequency interpolation
  with rate knob.

### Voice architecture

- **Polyphony**: `N` voices (default 8). Voice stealing
  modes: `oldest`, `quietest`, `none-mono`.
- **Mono / legato** knob option (with portamento glide).
- **ADSR** envelope per voice (lifted from `wavetable.rs`).
- **Per-voice filter** — LP / BP / HP, cutoff + resonance +
  env amount + key-tracking. Reuse `Biquad` from `fx.rs`.
- **LFO routes** per voice: vol / pitch / cutoff / pan
  (same `LfoTarget` opcode dispatch the rest of the synth
  uses).

### Sample manipulation

- Reverse mode (per zone).
- Random start offset (granular feel).
- Time-stretch *decoupled* from pitch via phase vocoder —
  for sustained loops at any tempo.
- Crossfaded looping (loop start XF length).

### UI

- **Sample editor** — zoom-able waveform with draggable
  markers (start, end, loop start, loop end). Click
  waveform → preview at root pitch.
- **Drop zone / Browse… button** at the top of the card.
- **Zone map** — mini piano keyboard strip; drag samples
  onto keys to build a multisample.
- **Velocity layer strip** below the zone map.
- **Per-zone parameter inspector** — opens when a zone is
  selected.
- **Card grid size** — likely 4×4 (denser than the voices,
  comparable to `An1xVoice`'s 6×6 footprint). Tune once
  the editor lays out.

### Integration

- **Own sequencer lane** — `sample.sample_steps` +
  `sample.sample_notes`, mirroring the wavetable lane.
- **MIDI input** — NoteOn/NoteOff already pluggable through
  the existing midir handler.
- **LLM control** — full state in `param_json_schema()`.
  Locked-param support like every other module.
- **Default zone** — `Voice`.
- **`allows_multiple()` = true** — racks may want a piano
  + a vocal phrases + a drum-kit instrument simultaneously.

### Free libraries to ship a "starter" pack

- **Salamander Grand Piano** (CC-BY 3.0) — the canonical
  free piano `.sfz`.
- **Sonatina Symphonic Orchestra** (CC0) — strings, winds,
  brass.
- **Versilian VSCO 2 CE** (CC0) — alt orchestral.
- A custom small **`samples/instruments/starter/`** folder
  with a handful of CC0 single-shot `.wav`s (a piano C4, a
  marimba C4, a vocal "ah" A4) so the module isn't empty
  out of the box.

### Implementation order

1. SFZ parser (subset above) → opcode `Region` struct.
2. Single-zone playback path (load `.wav` → resample by
   ratio → ADSR → vol/pan/pitch).
3. Polyphony + voice stealing.
4. YIN auto-detect + filename heuristic fallback.
5. Loop points (with crossfade).
6. Multi-zone key mapping + crossfade.
7. Velocity layers + round-robin.
8. Per-voice filter + LFO routing.
9. UI: card layout + waveform editor + zone map + browser.
10. PSOLA / phase-vocoder formant-preserving shift (opt-in).
11. Time-stretch decoupled from pitch.
12. Starter pack curation.

### Out of scope (deliberately deferred)

- Multi-mic / multi-position recordings (close / room / hall
  blends — Salamander has these, we'll just pick one per
  zone for now).
- Disk streaming for huge banks — load to RAM, cap the
  bank size, document the limit.
- `.sf2` parsing.
- Sample recording directly into the instrument from the
  audio input (separate module, future work).

## Intelligence

- [ ] **Test additional LLM models** - evaluate
  DeepSeek-R1-Distill-Qwen-7B / 14B and Qwen3-8B / 14B for JSON
  accuracy and music theory.  Gemma 4 26B-A4B is now downloadable
  (three quants); needs a head-to-head vs. E4B on the style + bass +
  theory suites.

## Integration

- [ ] **Ableton Link tempo sync** - bidirectional BPM + bar-phase
  sync via the `ableton_link` crate.  Useful for jamming alongside
  Live / Ableton Push or another synth setup.

## Agent tooling - gradual control & expressiveness


## Refactoring

- [ ] **Glass group helpers** - `glass_label(ui, text)` still to do
  (the inline pattern varies too much across panels for a single
  helper).
- [ ] **Large-file splits (remaining)** - top remaining files (none
  over cap, all just for the watch list):
  `src/ui/panels/sequencer.rs` (996), `src/state/rack.rs` (924),
  `src/llm/mod.rs` (921), `src/state/transitions.rs` (915),
  `src/llm/lanes.rs` (901), `src/audio/dsp/mod.rs` (898).  The
  sequencer panel is UI and hard to split cleanly; rack.rs is
  intentionally object-oriented (RackState owns its own coherence
  invariants); llm/mod.rs is one big run_llm_loop with thread
  plumbing.  The candidates with the cleanest split are
  `state/transitions.rs` (could group song-mode helpers + bank /
  chain into a sibling) and `audio/dsp/mod.rs` (DspState process
  block could split per-voice mix branches).

## Demo recording

- [ ] **Next acid demo re-record** - showcase the **two bass voices**
  (V1 + V2 playing complementary lines), plus FX routes that last
  session's demo didn't cover (delay/phaser/chorus/ringmod).  Use the
  bigger NeuTTS quant for the MC/vocal line.  **Bonsai references
  removed** from the demo script (module no longer in the codebase).
- [ ] **`demo/scenarios/setup-mc-singer.sh`** - Jungle MC + TTS
  Singer through autotune.  Non-deterministic, 100 % agent-controlled.
- [ ] **Preecho demo scene** - agent writes anchors into a drum
  pattern and you hear the build-up ramp into each downbeat.
- [ ] **LFO assignment scene** - agent schedules filter sweep via
  the per-voice bass LFO.
- [ ] **Parameter ramp scene** - gradual cutoff sweep over bars.
- [ ] **Event stream scene** - Huth-coloured note history scrolling
  in real time, with the new past-side log preserving past notes.
- [ ] **Re-record the D&B demo** - amen + reese + drone pad + MC
  scenario is ready; waiting on a clean recording run.

---

## Known issues

| Issue | Cause | Status |
|-------|-------|--------|
| Hoover doesn't sound like a hoover | DSP tuning, not a code bug | Needs filter sweep shape tuning |
