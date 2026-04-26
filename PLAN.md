# Impulse Instruct — Roadmap

What's already built lives in [docs/features.md](docs/features.md).
This file lists **future work only** — completed items get moved into
`features.md` as they ship.

---

## Next session — kickoff items

The original three-pick queue is fully shipped (polyphony
meter, DJ filter, FM operator synth — see features.md).
Pick the next session's work from the wishlist sections
below.

Rough size guide: **DJ-style 3-band kill EQ** is the next
small/quick win; **multiband compressor** is the biggest
unshipped FX.  Voice-side, the additive synth and the modal /
struck physical model both fill distinct gaps the FM op
voice doesn't cover.

---

## SampleInstrument V2 — outstanding follow-ups

The 9 main stages are shipped (see features.md).  Stage 7.5
also shipped — drag-to-edit loop markers + SFZ zone selection +
per-zone inspector all wired (see features.md).  Remaining
slice:

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

(Continuous Link bar-phase drift correction shipped — see
features.md.  No remaining items in this section.)

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
- [ ] **Large-file watch list** (none over the 1000-line cap;
  just noting the largest ones in case a future change pushes
  one over).  Current top after the post-kickoff session:
  `src/tests/dsp_fx_tests.rs` (966),
  `src/llm/lanes.rs` (962),
  `src/ui/panels/sequencer.rs` (944),
  `src/state/transitions.rs` (925),
  `src/audio/analysis.rs` (924),
  `src/llm/mod.rs` (921),
  `src/ui/rack_content_fx_extras.rs` (918 — picked up DJ
  filter + dispatches for the previously-empty card group),
  `src/state/rack.rs` (916).  `src/audio/dsp/fx_extras.rs`
  was split this session — extracted the glitch family
  (TapeStop / Stutter / Freeze) into a sibling
  `fx_glitch.rs`; the parent dropped 989 → 628 lines, no
  behaviour change.  `audio/dsp/mod.rs`, `src/state/rack.rs`,
  and `src/ui/rack_content.rs` were split in earlier
  sessions (`process_block.rs`, `rack_wiring.rs`,
  `rack_content_conv_reverb.rs` siblings).

## Voices — wishlist

All previously-listed voice gaps have shipped; re-open here if a
new gap turns up.

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
