# Impulse Instruct — Roadmap

What's already built lives in [docs/features.md](docs/features.md).
This file lists **future work only** — completed items get moved into
`features.md` as they ship.

---

## Next session — kickoff queue

Spectral gate STFT shipped this session (V1 BPF still the default,
toggle on the panel).  SampleInstrument V2 made progress on three
fronts: `.flac` / `.aiff` decode, REC-from-master-output button,
and the unified `load_audio_to_engine` loader (also used by the SFZ
region path so multi-format `.sfz` packs work).  Remaining:

1. **Demo recording — pick one scene**.  Multiple scenarios queued
   (acid re-record, MC singer, preecho, LFO assignment, parameter
   ramp, event stream, D&B re-record).  All are non-coding — just
   capturing audio against the existing scene scripts.

---

## SampleInstrument V2 — outstanding follow-ups

The 9 main stages are shipped (see features.md).  Stage 7.5 also
shipped — drag-to-edit loop markers + SFZ zone selection +
per-zone inspector all wired (see features.md).  Remaining slice:

- [ ] **`.sf2` — filter / modulator / LFO generators**.  Volume
  envelope already shipped (V2 follow-up: per-region ADSR
  override + timecents conversion).  Remaining: filter cutoff
  / Q (initialFilterFc / Q), modulation envelope, modulation
  LFO, vibrato LFO, sample modes (one-shot / loop continuous /
  loop until release), generator-to-target modulators.
- [ ] **Disk streaming for huge banks** — architectural change;
  V1 fits everything in memory which caps usable bank size.

(SF2 V1 parsing + preset picker + envelope generators,
`.flac` / `.aiff` decoding, sample recording from the audio
input, multi-mic / multi-position SFZ blends, and per-region
ADSR override all shipped — see features.md.)

## FX — still open

- [-] **Phase correlation strip (`CorrMeter`)** — *not separately
  shipped*; the existing `StereoMeter` already shows correlation +
  L/R balance as a horizontal strip, which covers the same use
  case more comprehensively.  Re-open if a slimmed-down
  correlation-only variant becomes necessary.
- [-] **Lissajous-3D / oscilloscope-3D depth** — *deferred*.  The
  shipped `StereoVectorscope` covers the goniometer use case;
  fading-polyline depth is pure eye-candy and can come back if
  visual demand surfaces.
(Shimmer mode on `FxConvReverb` + Spectral gate STFT version both
shipped — see `features.md`.  V1 BPF stays the default for the
spectral gate; toggle on the panel switches modes.)

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

(CV sequence visualiser, GRAN pitch-tracking trigger mode, and AI
patch morph UI dialog all shipped — see `features.md`.)

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

(All shipped — MIDI granuliser file-to-file, Vinyl FX start/stop
transient, AI patch morph UI dialog, and GRAN pitch-tracking
trigger mode all landed.  See `features.md`.)

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

(None currently — the long-standing Hoover-tuning issue shipped
this session; see `features.md` for what changed.)
