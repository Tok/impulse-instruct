# Impulse Instruct — Roadmap

What's already built lives in [docs/features.md](docs/features.md).
This file lists **future work only** — completed items get moved into
`features.md` as they ship.

---

## Next session — kickoff items

These two are explicitly queued for the next session:

- [ ] **Sample-pack download helper(s)** — mirror the
  `scripts/download-models.sh` / `download-models.bat` pattern for the
  CC-licensed instrument packs the SAMPLER+ module can play
  (Salamander Grand, Sonatina Symphonic Orchestra, VSCO 2 CE).
  See `samples/instruments/README.md` for the source URLs already
  curated.  Likely shape: `scripts/download-instruments.sh` with a
  `--pack <name>` selector, fetching into
  `samples/instruments/<pack_name>/` so the existing path-scan
  resolves automatically.  Same idea probably wants extending to
  amen / textures / wavetables / IRs (one umbrella
  `download-samples.sh` with sub-commands).
- [ ] **Audit the `Full` rack preset** —
  `src/state/rack_presets.rs::RACK_PRESETS[2] (name = "Full")` was
  authored before the V2 module sprint and is missing every module
  added since.  Concretely, it should pick up:
  - **Voices**: `PluckString`, `WavetableVoice`, `SampleInstrument`
  - **FX**: `FxLimiter`, `FxFilter`, `FxComb`, `FxTilt`,
    `FxTransient`, `FxExciter`, `FxMultitap`, `FxRevDelay`,
    `FxTapeStop`, `FxStutter`, `FxFreeze`, `FxConvReverb`,
    `FxParamEq`, `FxPitchShift`, `FxFreqShift`, `FxWiden`,
    `FxGate`, `FxVocoder`, `FxPan`
  - **Analysis / viz**: `SpectrumAnalyzer`, `StereoMeter`,
    `ActivityTimeline`, `BarOscilloscope`, `StereoVectorscope`,
    `LfoScope`, `PitchTracker`, `ChordDisplay`, `Spectrogram`,
    `LoudnessMeter`, `PhaseWheel`, `EventStream`
  Decide if "Full" really wants *every* module (could be visually
  overwhelming) or a curated "showcase" selection — and confirm
  the smaller presets ("Solo / Duo / Swarm / Crew") still feel
  right next to the new modules.

---

## SampleInstrument V2 — outstanding follow-ups

The 9 main stages are shipped (see features.md).  Remaining slices:

- [ ] **Time-stretch decoupled from pitch**.  Distinct from formant
  preservation — the user wants to play a sustained loop at a
  different tempo without changing pitch.  Phase-vocoder time-
  stretch (or epoch-detected PSOLA with synth-hop ≠ analysis-hop)
  in the same family as the existing `FormantShifter`.  Probably
  shares the FFT scaffolding.
- [ ] **Drag-to-edit on the visualizer strip** (Stage 7.5).  Drag
  loop-start / loop-end markers on the waveform thumb; click a
  region in the SFZ zone-map to select it + open a per-zone
  parameter inspector.  V1 of the strip is read-only.
- [ ] **Out-of-scope V1 items revisited** (per the original
  PLAN's "deliberately deferred" list): multi-mic / multi-position
  blends in SFZ, `.flac` / `.aiff` formats, `.sf2` parsing, disk
  streaming for huge banks, sample recording from the audio input.

## FX — still open

- [-] **Shimmer mode flag on `FxConvReverb`** — *deferred*.
  Pitch-shift (+12 / +7) in the feedback loop is doable but needs
  careful integration with `ConvReverb`'s data flow + an extra
  `PitchShift` instance inside the convolver — bigger than a flag,
  smaller than a new module.  Pick up when ConvReverb gets
  attention next.
- [-] **Phase correlation strip (`CorrMeter`)** — *not separately
  shipped*; the existing `StereoMeter` already shows correlation +
  L/R balance as a horizontal strip, which covers the same use
  case more comprehensively.  Re-open if a slimmed-down
  correlation-only variant becomes necessary.
- [-] **Lissajous-3D / oscilloscope-3D depth** — *deferred*.  The
  shipped `StereoVectorscope` covers the goniometer use case;
  fading-polyline depth is pure eye-candy and can come back if
  visual demand surfaces.

## Visualizations — open

- [ ] **CV-cable wiring of LFO slot ↔ scope module** (LfoScope V1
  picks the first enabled LFO slot).  Real selection by following
  the rack cable graph.

## Integration — open

- [ ] **Ableton Link bar-phase alignment** (Link V2).  Tempo sync
  shipped; bar-phase needs threading Link's quantum (typically
  4 beats) through the sequencer clock advance so our step
  counter snaps to the network's bar boundary on enable / on
  long-running drift.

## Intelligence

- [ ] **Test additional LLM models** — evaluate
  DeepSeek-R1-Distill-Qwen-7B / 14B and Qwen3-8B / 14B for JSON
  accuracy and music theory.  Gemma 4 26B-A4B is now downloadable
  (three quants); needs a head-to-head vs. E4B on the style + bass
  + theory suites.

## Refactoring

- [ ] **Glass group helpers** — `glass_label(ui, text)` still to do
  (the inline pattern varies too much across panels for a single
  helper).
- [ ] **Large-file watch list** (none over the 1000-line cap; just
  noting the largest ones in case a future change pushes one
  over):
  `src/ui/panels/sequencer.rs`, `src/state/rack.rs`,
  `src/llm/mod.rs`, `src/state/transitions.rs`, `src/llm/lanes.rs`,
  `src/audio/dsp/mod.rs`.  The cleanest split candidates remain
  `state/transitions.rs` (song-mode helpers + bank/chain into a
  sibling) and `audio/dsp/mod.rs` (DspState process block per-voice
  mix branches).

## Demo recording

- [ ] **Next acid demo re-record** — showcase the **two bass voices**
  (V1 + V2 playing complementary lines), plus FX routes that last
  session's demo didn't cover (delay / phaser / chorus / ringmod).
  Use the bigger NeuTTS quant for the MC / vocal line.  **Bonsai
  references removed** from the demo script (module no longer in
  the codebase).
- [ ] **`demo/scenarios/setup-mc-singer.sh`** — Jungle MC + TTS
  Singer through autotune.  Non-deterministic, 100 % agent-controlled.
- [ ] **Preecho demo scene** — agent writes anchors into a drum
  pattern and you hear the build-up ramp into each downbeat.
- [ ] **LFO assignment scene** — agent schedules filter sweep via
  the per-voice bass LFO.
- [ ] **Parameter ramp scene** — gradual cutoff sweep over bars.
- [ ] **Event stream scene** — Huth-coloured note history scrolling
  in real time, with the new past-side log preserving past notes.
- [ ] **Re-record the D&B demo** — amen + reese + drone pad + MC
  scenario is ready; waiting on a clean recording run.

---

## Known issues

| Issue | Cause | Status |
|-------|-------|--------|
| Hoover doesn't sound like a hoover | DSP tuning, not a code bug | Needs filter sweep shape tuning |
