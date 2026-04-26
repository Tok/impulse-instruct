# Impulse Instruct — Roadmap

What's already built lives in [docs/features.md](docs/features.md).
This file lists **future work only** — completed items get moved into
`features.md` as they ship.

---

## Next session — kickoff queue

The previous session shipped the full **Voices / FX / Visualizations /
Modulation wishlists** end-to-end (1 voice + 9 FX + 2 viz + 6 modulation
utilities + Phase 1 cv_buf cable infrastructure — see `features.md`).
Pick from the small / focused queue below.

1. **Hoover sound tuning** — the long-standing known issue at the
   bottom of this file.  DSP / filter-sweep shape work, not new
   surface area.  Concrete deliverable: an A/B test against a
   reference patch + a tweak commit on `src/audio/dsp/voices.rs`'s
   `HooverVoice`.
2. **GRAN pitch-tracking trigger mode** (deferred V2 from absurd
   queue).  Bird-song corpus already ships through the granular
   voice; today it plays at fixed pitch.  Add a `pitch_mappable`
   flag so played notes drive grain pitch — opens the door to
   melodic bird-call solos.
3. **AI patch morph UI dialog** (deferred V2 from absurd queue).
   `POST /api/morph` works; needs a small modal with prompt input
   + bars / calls knobs so it's discoverable from the menu.
4. **Demo recording — pick one scene**.  Multiple scenarios queued
   (acid re-record, MC singer, preecho, LFO assignment, parameter
   ramp, event stream, D&B re-record).  All are non-coding — just
   capturing audio against the existing scene scripts.

Rough size guide: 1 + 3 are small ships; 2 is medium-shaped (touches
the granular voice + sequencer trigger plumbing); 4 is recording, not
coding.

---

## SampleInstrument V2 — outstanding follow-ups

The 9 main stages are shipped (see features.md).  Stage 7.5 also
shipped — drag-to-edit loop markers + SFZ zone selection +
per-zone inspector all wired (see features.md).  Remaining slice:

- [ ] **Out-of-scope V1 items revisited** (per the original PLAN's
  "deliberately deferred" list): multi-mic / multi-position blends
  in SFZ, `.flac` / `.aiff` formats, `.sf2` parsing, disk streaming
  for huge banks, sample recording from the audio input.

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
- [-] **Spectral gate — true STFT version** — V1 ships an 8-band
  parallel-BPF approximation (per-band envelope + gate, subtractive
  recombination).  The textbook STFT version is deferred until FFT
  machinery lands in the codebase.

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
  one over).  Top after the wishlist-marathon session:
  `src/state/rack.rs` (988),
  `src/tests/dsp_fx_tests.rs` (966),
  `src/llm/lanes.rs` (962),
  `src/ui/rack_content_fx_extras.rs` (961),
  `src/ui/rack_content.rs` (955),
  `src/sequencer/mod.rs` (945),
  `src/ui/panels/sequencer.rs` (944),
  `src/audio/dsp/params.rs` (937),
  `src/state/transitions.rs` (925),
  `src/audio/analysis.rs` (924),
  `src/llm/mod.rs` (921),
  `src/state/fx.rs` (895),
  `src/audio/mod.rs` (895),
  `src/state/module_kind.rs` (893).
  This session's splits: `fx.rs` → `fx_defaults.rs`, `params.rs`
  → `lfo_target_opcode.rs` + `mod_compile.rs`,
  `rack_content_fx_extras.rs` → `rack_content_fx_lfo.rs`.  Adding
  another voice or a new modulator with knob-heavy state will
  need another split — `rack.rs` is the closest to the cap (988).

## Voices — wishlist

All previously-listed voice gaps have shipped (FM ops, additive,
modal, chiptune, vocal — see features.md).  Re-open here if a new
gap turns up.

## FX — wishlist

All previously-listed FX wishlist items have shipped (tremolo,
vibrato, ISO EQ, de-esser, resonator bank, tape echo, multiband
compressor, grain delay, spectral gate — see features.md).  V2
follow-ups carried per-feature in features.md:
- `FxSpectralGate` ships as an 8-band parallel-BPF approximation;
  the textbook STFT version is deferred until FFT machinery lands.
- Shimmer mode on `FxConvReverb` remains deferred.

## Visualizations — wishlist

- [ ] **CV sequence visualiser** — focused waveform / value-bar
  view of an assigned CV-seq slot's per-step output.  The CV
  sequencer module is shipped; this is the dedicated visualiser
  companion (the existing CV-seq panel already shows its 16 step
  bars in-place, so this is V2 polish rather than an unmet need).

## Modulation — wishlist

All previously-listed modulation utilities have shipped (CV
sequencer, Slew, Quantizer, Comparator, Sample-and-hold, Math).
The CV cable-routing infrastructure (`cv_buf` + per-utility
compile passes) was built up in Phase 1; each utility now
participates in the full graph — sources can chain through
utilities to drive any synth/FX param.

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
| Hoover doesn't sound like a hoover | DSP tuning, not a code bug | Needs filter sweep shape tuning (see Next-session kickoff #1) |
