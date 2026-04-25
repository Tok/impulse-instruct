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
- [ ] **Frequency shifter (`FxFreqShift`)** — Hilbert-pair
  single-sideband shift in Hz. Deferred from the tier-1 batch
  because a faithful Hilbert allpass cascade is genuinely subtle
  DSP (Olli Niemitalo's HilbertOLA-style coefficient design); a
  rushed implementation would sound off. Pick this back up with
  more attention than a batched-through tier-1 slot allows.
- [ ] **Stereo widener (`FxWiden`)** — Haas-delay + mid/side
  scaling for width. **Blocked on stereo FX pipeline** — the
  current `apply_fx_step(sig: f32) -> f32` is mono per-FX, with
  stereo only at the master stage. A widener fundamentally needs
  stereo I/O. Lift this once we add a stereo dispatch path (or a
  dual-channel FX trait alongside the mono one).

Tier 2 (need a sidechain / second input port — solve once,
share):
- [ ] **Sidechain port plumbing in `RackState`** — second cable
  kind `PortKind::SidechainIn` with its own routing. Required by
  the next three. Keep it backward-compatible with the existing
  audio cable graph; `compile_fx_plan()` already does Kahn's
  topo, just add the new edge type.
- [ ] **Vocoder (`FxVocoder`)** — N-band (16 default, 32 max)
  envelope follower on the sidechain modulating the main carrier.
  Pairs with `NeuTts` (TTS → modulator, bass/hoover → carrier).
- [ ] **Noise gate / ducker (`FxGate`)** — sidechain-aware gate.
  Sidechain = trigger; main = signal. Carves kick room out of a
  pad bus.
- [ ] **Sidechain compressor mode on `FxCompressor`** — the
  existing module gains a "sidechain enabled" flag rather than
  a new kind.

Tier 3 (heavier — build once Tier 1+2 settle):
- [ ] **Multitap / ping-pong delay (`FxMultitap`)** — 4–8 taps
  with per-tap level + pan + filter. Existing `FxDelay` is a
  single-tap tape model; this is the rhythmic / dub variant.
- [ ] **Reverse delay (`FxRevDelay`)** — buffer-then-reverse
  per beat. Different state model from `FxDelay` (segment
  accumulator), worth its own module.
- [ ] **Spectral freezer (`FxFreeze`)** — FFT a frame on
  trigger, hold magnitudes + randomised phase forever. Pure
  drone-pad button; trigger via mod cable / sequencer step.
- [ ] **Stutter / glitch repeater (`FxStutter`)** — beat-synced
  buffer repeat (1/4, 1/8, 1/16, 1/32) with optional
  pitch-down-on-repeat for tape-stop flavour. Trigger on a
  sequencer step or LFO gate.
- [ ] **Tape stop (`FxTapeStop`)** — pitch + lowpass cutoff
  ramp to zero on trigger; one-shot, releases. Could be
  triggered by a sequencer cell.
- [ ] **Cabinet / amp IR (`FxCabinet`)** — short-IR (≤ 200 ms)
  variant of `FxConvReverb` for guitar/bass cab simulation. Same
  convolution kernel, different IR-folder (`samples/cabinets/`)
  and a smaller buffer cap.
- [ ] **Mid/side EQ (`FxMsEq`)** — `FxParamEq` with a
  mid-or-side selector per band. Heavy reuse — could fold into
  the existing param-EQ module via a "ms mode" toggle rather
  than a new module.
- [ ] **Shimmer mode on `FxConvReverb`** — pitch-shift (+12 /
  +7) inside the feedback loop. A flag on the existing module,
  not a new module.

## Visualizations — new analysis modules

Tier 1 (cheap, reuse existing buffers):
- [x] **Goniometer / vectorscope (`StereoVectorscope`)** — shipped.
- [x] **LFO scope (`LfoScope`)** — shipped (V1 picks the first enabled
  LFO slot; CV-cable wiring of slot ↔ module is a follow-up).
- [x] **Tuner (`PitchTracker`)** — shipped, autocorrelation-based
  pitch detect with cents-off needle.
- [x] **Chord / key display (`ChordDisplay`)** — shipped, chroma
  vector + 24-template (major/minor) match.
- [ ] **Per-voice activity heatmap** — original plan said "overlay
  mode on `ActivityTimeline`", but `ActivityTimeline` is the LLM
  agent action log, not a per-voice trigger surface. The right
  source is the existing `EventStream`'s `MelodicLogEntry` /
  `DrumLogEntry` queues. Defer until we revisit the EventStream UI;
  a heatmap mode there would be a small, distinct slice.

Tier 2 (heavier — guard with reduce-cost path):
- [ ] **Spectrogram waterfall (`Spectrogram`)** — rolling FFT
  history (~5 s scroll). Render to `egui::ColorImage`; reuse
  `compute_spectrum()`. Heavier than the bars; reduce on
  demo-mode.
- [ ] **Loudness / LUFS meter (`LoudnessMeter`)** — short-term
  + integrated LUFS (K-weighted RMS over a sliding window).
  Distinct from peak/RMS in `StereoMeter`.
- [ ] **Phase correlation strip (`CorrMeter`)** — single-row
  -1..+1 correlation bar; cheaper than the goniometer if you
  only want a number, and pairs with it.
- [ ] **Lissajous-3D / oscilloscope-3Ddepth** — XY-with-trail
  rendered as a stack of fading polylines. Eye-candy version of
  the goniometer. Optional.
- [ ] **Transport phase wheel (`PhaseWheel`)** — circular bar /
  beat indicator with sub-divisions. UI-only; reads transport
  state. Companion to the `EventStream`.

## New module — `SampleInstrument` (full scope)

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
