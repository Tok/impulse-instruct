# Impulse Instruct — Roadmap

What's already built lives in [docs/features.md](docs/features.md).
This file lists **future work only** — completed items get moved into
`features.md` as they ship.

---

## Next session — kickoff items

One pick queued for the next session (the big one — polyphony
meter and DJ filter from this queue both shipped, see
features.md).

- [ ] **FM operator synth** (voices wishlist) — biggest visible
  new feature.  4-op DX7-flavoured voice plugs the gap between
  the existing AN1X (subtractive) and SAMPLER+ (samples) for
  bell / E-piano / FM-bass tones.  Real DSP build — phase
  modulation chain + ADSR per op + algorithm selector.

The wishlist + remaining sections below carry the rest of the
unshipped roadmap.

---

## SampleInstrument V2 — outstanding follow-ups

The 9 main stages are shipped (see features.md).  Remaining slices:

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

## Integration — open

- [ ] **Continuous Link bar-phase drift correction**.  V2 added
  bar-phase snap on the off→on transition (see features.md);
  long-running drift correction during a session is still open —
  needs a tolerance window and re-snap policy that doesn't
  disturb a stable performance.

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
  over).  Current top after the absurd-queue session:
  `src/audio/dsp/fx_extras.rs` (989 — closest to the cap; one
  more FX struct lands it over),
  `src/tests/dsp_fx_tests.rs` (966),
  `src/llm/lanes.rs` (962),
  `src/ui/panels/sequencer.rs` (944),
  `src/ui/rack_content.rs` (931 — picked up Theremin / Pendulum /
  Vinyl dispatch this session),
  `src/state/transitions.rs` (925),
  `src/llm/mod.rs` (921).  Cleanest split candidate is
  `fx_extras.rs` (per-FX struct → sibling per family —
  saturation / EQ / dynamics).  `audio/dsp/mod.rs` and
  `src/state/rack.rs` were split out in earlier sessions
  (`process_block.rs` and `rack_wiring.rs` siblings); both now
  sit comfortably under the cap.

## Voices — wishlist

Modules that would plug a real gap in the current voice palette.

- [ ] **FM operator synth** (DX7-style 4- or 6-op).  Closest gap to
  the existing AN1X subtractive — DX-flavoured bell / E-piano /
  bass tones don't reproduce well from the current stack.
- [ ] **Additive synth** with per-harmonic level sliders.  Distinct
  from wavetable: the user draws the spectrum directly instead of
  scanning frames.
- [ ] **Modal / struck physical model** — mass-spring bank for
  marimba, bell, glass.  Cheap N-mode resonator excited by an
  impulse / noise burst.
- [ ] **Chiptune voice** — 2× pulse + triangle + LFSR noise +
  optional 1-bit DPCM, NES-authentic.  Pairs well with the existing
  step sequencer for tracker workflows.
- [ ] **Vocal formant synth** — formant filter bank around an
  oscillator.  Distinct from `NeuTts` — sings vowels without
  needing a phoneme model.

## FX — wishlist

- [ ] **Multiband compressor** (3-band split with per-band
  ratio/threshold).  Mastering-grade dynamics that the single-band
  `FxCompressor` can't shape.
- [ ] **De-esser** — sidechain HP → narrow compress on the sibilant
  band.  Specialist tool for vocal / hat material.
- [ ] **Resonator bank** — 6 tuned resonant filters → pitched chord
  layer from any input.  Karplus-on-input character.
- [ ] **Grain delay** — granular feedback path; distinct from
  `FxMultitap` (rhythmic taps) and `FxFreeze` (held buffer).
- [ ] **3-band ISO / kill EQ** — DJ-style hard-kill bands (low /
  mid / high).  Cheap but invaluable for live cuts.
- [ ] **Spectral gate** — per-bin gate on STFT magnitude (not a
  global threshold).  Pairs with `FxFreeze`'s spectral path.
- [ ] **Tape echo** — dedicated, with wow / flutter + saturation
  inside the feedback loop.  Distinct from `FxTapeSat` (no
  delay) and `FxDelay` (no character).
- [ ] **Tremolo** + **vibrato** as their own modules.  Currently
  approximated via Chorus / Pan; users typing "add a tremolo"
  expect a dedicated knob.
- [ ] **Shimmer mode flag on `FxConvReverb`** (already deferred
  above — repeated here so the FX wishlist scans complete).

## Visualizations — wishlist

- [ ] **Pattern density heatmap** — 16 steps × N bars grid,
  brightness = note density per voice.  Quick "where are the
  busy parts" read across a long song.
- [ ] **Onset / beat-grid overlay** — detected onsets vs sequencer
  grid (already have `audio/onset.rs`).  Debug tool for groove /
  late-strike discussions.
- [ ] **CV sequence visualiser** — paired with the
  modulation-wishlist CV sequencer below if shipped.

## Modulation — wishlist

- [ ] **CV sequencer** — 16-step CV pattern module distinct from
  the audio sequencer.  Outputs CV that other modules can patch
  in for envelope / pitch automation.
- [ ] **Slew / glide module** — smooth a CV with separate
  rise / fall times.  Currently glide is bass-only.
- [ ] **Quantizer** — snap a CV to the nearest scale note.
- [ ] **Comparator / threshold** — CV → gate when above a level.
- [ ] **Math** — combine two CVs (add / multiply / blend).  Opens
  patches the rack can't currently express (e.g. one LFO scaling
  another).
- [ ] **Sequenced sample-and-hold** — externally clockable S&H
  module, distinct from the LFO's S&H waveform option.

## Absurd / unusual — all shipped

Eurorack patch generator, Theremin, Mellotron mode, AI patch
morph, Pendulum, Vinyl/cassette FX, bird-song corpus, MIDI
granuliser — all V1-shipped (see features.md).  Deferred V2
follow-ups carried in the per-feature sections of features.md;
re-open here if any of them grow legs.

## Deferred V2 follow-ups (from the absurd-queue ship)

- [ ] **AI patch morph — UI dialog**.  V1 is API-only
  (`POST /api/morph`); a small modal with prompt input + bars /
  calls knobs would make it discoverable from the menu.
- [ ] **MIDI granuliser — file-to-file mode**.  V1 scatters the
  running sequencer pattern in place; a `granulise_smf_bytes`
  wrapper would let users pre-process MIDI clips offline.
- [ ] **Vinyl FX — start / stop transient**.  V1 covers the
  steady-state colour; the ramp-up / brake transient was
  deliberately deferred (FxTapeStop overlap).  Could come back
  as a dedicated knob if users want both.
- [ ] **GRAN — pitch-tracking trigger mode**.  Bird-song corpus
  ships, but the granular voice plays at fixed pitch; a
  `pitch_mappable` flag would let played notes drive grain
  pitch for melodic bird-call solos.

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
