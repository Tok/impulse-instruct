# Impulse Instruct - Roadmap

What's already built is documented in [docs/features.md](docs/features.md).

---

## Shipped in v0.7.6

For a detailed log see [docs/features.md](docs/features.md).
Highlights of the cycle:

- **Per-knob modulation system** — third cable kind (`PortKind::Mod`),
  every module declares `mod_inputs(kind)`, multi-select target chips,
  per-cable depth + polarity, audio-thread routing via `ModRouteCopy`.
- **Gabber kick voice** — dedicated `ModuleKind::GabberKick` voice with
  pitch sweep, tanh saturator, transient click layer, 4 LFO targets.
- **Pan FX module** + **LFO target: StereoWidth** + **Tempo-quantized
  FX direction buffer** (`rev_tap_len_for_quant`).
- **TTS audibility fix** — sample-rate mismatch (24 kHz NeuTTS Air) +
  agent-triggered TTS hoisted out of the `param_update` gate.
- **Bass voice LFO panel row** + **per-step pan** (`TB303Step.pan`,
  PAN row in the sequencer).
- **Amen step → slice mapping** + stutter knob fix + waveform/wheel
  animation + chop randomization.
- **Style → rack auto-setup** + `POST /api/randomize` smart
  randomization.
- **Shell log colorization** + Huth filter fixes.
- **Quick-command pills** on agent card (REWRITE / VARI / FILL / etc).
- **Cable visual hierarchy** — three styles: audio (fattest), signal
  (mid, used by `PortKind::Cv` / `PortKind::Mod` and synthesised LFO
  cables), control (thinnest, AI agent links).
- **Preecho UI polish** — voice tabs sized like BANK / CHAIN slots,
  two-line layout (PRE-ECHO + tabs + strip on line 1; ON / LEN / VEL /
  RAT / CLEAR on line 2), strip stride mirrors the sequencer step
  grid exactly (item_spacing + bar/beat dividers), both rows
  left-align with the sequencer sliders above.
- **Sequencer PAN reset button** zeros every step's pan in one click.
- **Mod-overlay top clip** — back-panel mod sliders no longer paint
  over the header info panel or prompt strip when the rack scrolls.

### Regression coverage

- Per-voice LFO regression tests (`lfo_rate_hz`, `lfo_fade_step`).
- Preecho integration test
  (`preecho_scales_velocity_through_advance_clock`).
- Amen slice playback (7 tests against `AmenVoice`).
- LLM `rack.add` / `rack.remove` round-trips.

---

## v0.7.7 — next release

### Regression coverage

- [x] **`apply_llm_update` decomposition (round 1)** — extracted
  `apply_sequencer_globals`, `apply_amen_update`,
  `apply_euclidean_update` (and the `drum_voice_from_str` mapping)
  into `state/llm_apply_seq.rs`.  `state/llm_apply.rs` shrank
  874 → 726 lines.  +24 tests covering clamp ranges, locked-path
  preservation, scope filtering, end/start swap, slice-array
  truncation/clear, and the euclidean voice mapping.
- [x] **`apply_llm_update` decomposition (round 2)** — extracted
  `SeqScope`, `apply_melodic_lane_lens`, `apply_bass_notes`,
  `apply_bass_pans`, `apply_preecho_voices` into
  `state/llm_apply_seq.rs`.  `state/llm_apply.rs` shrank
  726 → 621 lines.  +25 tests in a new
  `tests/llm_apply_voice_tests.rs` covering scope resolution,
  scale-snap rounding, MIDI note clamping, voice-0 mirror,
  signed-pan clamps, anchor / length clamps, null-clears
  preecho entries, partial preecho updates preserve other fields.
- [x] **`unlocked_f32` range bug** — added `unlocked_f32_range`
  with explicit `min` / `max`; routed `amen.pitch` (±24) and
  `amen.source_bpm` (40–300) through it.  +4 regression tests
  covering the previously-broken full ranges.
- [x] **DSP fx_math extraction** — pulled the closed-form math
  out of `audio/dsp/mod.rs::apply_fx_step` and `process_block`
  into a new `audio/dsp/fx_math.rs` (waveshaper / drive /
  bitcrush step + sidechain envelope follower & ducker + gated
  reverb envelope + 6-waveform LFO lookup + 8-step free-EG
  interpolation).  `mod.rs` shrank 998 → 955 lines.  +31 tests.
- [x] **Voice DSP unit tests** — promoted `LadderFilter`,
  `OnePole`, `NoiseGen`, `Envelope`, `AdsrPhase` (+
  `adsr_samples`, `adsr_tick`, `osc_sample`, `drum_voice_idx`)
  to `pub(crate)` so `tests/dsp_tests.rs` can hit them
  directly.  +31 tests covering filter steady state +
  HF attenuation + saturation safety, one-pole convergence,
  noise-gen range/determinism/seed degenerate, envelope
  trigger/decay/deactivation, ADSR phase transitions, osc
  waveform shapes (saw/square/triangle/sine/noise), drum
  voice index layout (808 / 909 / amen / gabber blocks).
- [x] **FX DSP unit tests** — promoted `Biquad` (+ `low_shelf`
  / `high_shelf` / `peak`), `EqBands`, and
  `Compressor::compress_band` to `pub(crate)`.  +22 tests in a
  new `tests/dsp_fx_tests.rs` covering closed-form gain at DC
  and Nyquist (low / high shelf, peak), 3-band EQ unity
  passthrough at zero gain, 12 dB boost / cut response,
  compressor sub- vs supra-threshold behaviour, envelope
  follower rise + decay, ratio=1:1 passthrough, finiteness
  on silent input.
- [x] **Mod-target dispatcher tests** — `audio/dsp/mod_apply::
  apply_mod_target` is the central LFO/Free-EG → param
  dispatcher (71 opcodes).  +30 tests in a new
  `tests/dsp_mod_apply_tests.rs` covering opcode 0 + unknown
  no-ops, unipolar clamps (cutoff / reverb_mix /
  delay_feedback at 0.99), scaled mods (delay_time / kick808
  pitch / distortion drive use 0.5×; master_volume uses 0.3×
  with 1.5 ceiling), pitch accumulators (`+=` not clamp-set,
  12-st scaling on opcodes 3 + 16), bipolar pan clamps for
  every drum + 303 + an1x slot, signed EQ clamps, special
  ranges (amen_gate floor 0.05; amen / granular / 303 volume
  ceiling 1.5), stereo_width 0.5× scaling, gabber kick
  targets, and isolation tests asserting no opcode silently
  scribbles outside its declared field.
- [x] **Untested agent-state transitions** — added 20 tests
  in `tests/helpers_tests.rs` for `apply_agent_mode_and_tts`
  (mode parsing producer/dj/mc + case-insensitivity + unknown
  string keeps existing, TTS=true adds a NeuTts module + wires
  a control cable + sets scroll target, TTS=false leaves the
  rack alone), `observe_user_edit` (high/low recording,
  mid-range filter, dedup-on-repeated-param, cap at
  STYLE_OBS_MAX, propagation to every agent),
  `push_agent_memory` (append, unknown-id no-op, cap at
  AGENT_MEMORY_MAX with oldest-dropped semantics), and
  `format_llm_display` (raw text fallthrough when no update,
  Off mode lists keys even with comment, non-Off uses
  comment, fallback to keys when comment missing).
- [x] **GranularVoice + GabberKick + log_fmt** — promoted
  GranularVoice and GabberKick to `pub(crate)`, added the
  `samplers` and `gabber_kick` submodules to the public DSP
  API.  +19 tests in a new `tests/dsp_voice_extra_tests.rs`:
  GranularVoice silence-without-sample / silence-with-empty-
  buffer / volume=0 short-circuit / audible after load+volume,
  pitch-scatter + jitter stays finite, silent input → silent
  output.  GabberKick silent-before-trigger, audible-after,
  decays-to-silence, clip param pushes peak near unity,
  zero-volume short-circuit.  log_fmt::starts_with_persona:
  uppercase + digit + underscore positives, lowercase /
  no-colon-space / empty / single-char / leading-bracket
  rejections.
- [x] **Stateful FX processors** — promoted Reverb,
  DelayLine, Chorus, TapeSat, Phaser, Autotune to
  `pub(crate)`.  +18 tests in `tests/dsp_fx_tests.rs`:
  Reverb silent-input passthrough + impulse-decays + freeze
  holds the tail + stays finite under continuous drive;
  DelayLine impulse resurfaces at the chosen offset + feedback
  produces a decaying echo train + stays finite at high
  feedback / saturation; Chorus mix=0 passthrough + finite
  full-wet + read_tap returns finite values; TapeSat mix=0
  passthrough + finite at full drive; Phaser mix=0 passthrough
  + audible full wet; Autotune amount=0 / mix=0 passthrough
  + finite under full pitch shift + audible after priming.
- [x] **At-cap files** — all five split, every file now
  comfortably under 1000:
  - `src/ui/mod.rs` (1000 → 685) — `drain_llm_outputs` →
    `ui/llm_drain.rs`.
  - `ui/llm_strip.rs` (998 → 557) — Huth note-colorizer + 12
    tests → `ui/llm_log_color.rs`.
  - `ui/rack_canvas.rs` (997 → 858) — 12-col grid helpers →
    `ui/rack_grid.rs`.
  - `ui/panels/bass.rs` (995 → 788) — WAVE/FILT/PRESET + Supersaw
    row → `panels/bass_wave.rs`; LOCKED-params strip →
    `panels/bass_locks.rs`.  Follows the existing `bass_noise.rs`
    sibling-helper pattern — each section re-reads its own
    snapshot from AppState rather than threading locals.
  - `ui/panels/sequencer.rs` (992 → 742) — Line-1 (BPM row) and
    Line-2 (Key/Scale/Swing) header strips → `panels/
    sequencer_header.rs` (`draw_line_1`, `draw_line_2`).

### Agent tooling — gradual control & expressiveness

- [ ] **XY pad control** — expose cutoff/resonance pad as a
  first-class tool the agent can move.  Currently agents set values
  but the pad position doesn't visually track mid-change.
- [ ] **Melodic voice preecho** — TB303Step has no velocity field,
  only accent/slide.  Design a preecho mapping for bass/hoover/an1x
  that uses accent-ramping or slide-cascading instead of velocity
  scaling.

### DSP

- [ ] **Dub techno send/return** — dedicated send/return FX
  workflow for dub-style infinite delay feedback chains.
- [ ] **Pitch-preserving BPM stretch on amen** — current stretch
  shifts pitch; phase-vocoder / granular stretch for when you want
  to match tempo without the pitch change.
- [ ] **Per-slice playback direction on amen** — currently reverse
  is a global flag; per-slice reverse would enable edit-era glitch
  patterns.
- [ ] **Reverse mode for compressor envelope** — third FX worth
  reversing (envelope follower).  Would give "reverse compression"
  swell-into-hit transient shaping.

### Sequencer

- [ ] **Pattern probability per step** — already implemented but
  LLM doesn't use it well; improve prompt guidance for
  probability-based patterns.
- [ ] **Song mode** — chain patterns with per-chain tempo/style
  transitions.
- [ ] **MIDI export** — export sequencer pattern as .mid file.
- [ ] **Preecho v2** — note approach (chromatic / scale-step / arp
  resolving to the anchor note), probability ramp, accent/slide
  trailing, curve shapes (exp / log), auto-length from gap between
  anchors.

### Intelligence

- [ ] **Agent conversation history** — multi-turn within a single
  jam cycle; agent sees its own previous outputs for coherent
  evolution.
- [ ] **Prompt templates per style** — styles can define custom
  prompt templates that replace the generic "generate all
  parameters" jam prompt.
- [ ] **VRAM-aware model fallback** — when spawn is rejected,
  auto-suggest or auto-select a lighter model that fits the
  remaining VRAM budget.
- [ ] **Test additional LLM models** — evaluate
  DeepSeek-R1-Distill-Qwen-7B/14B and Qwen3-8B/14B for JSON
  accuracy and music theory understanding.
- [ ] **Jam-via-API** — currently API prompts are always
  one_shot (no jam loop).  Need safe jam support that doesn't do
  full-state replacement.
- [ ] **Style mc_lines/themes UI editor** — allow editing
  mc_lines and themes per style from the UI preferences.
- [ ] **Style-aware agent-preset naming** — styles override the
  default multi-agent setup name ("Crew" → "Band" / "Posse" /
  "Squad" / "Ensemble" per style).

### UI / UX

- [ ] **Touch mode improvements** — touch-paint mode for
  mobile/tablet; gesture support for zoom/scroll.
- [ ] **NeuTts mod targets** — Amen/Granular got per-voice
  LfoTarget variants this cycle but NeuTts still has none; its TTS
  bus volume isn't an `AudioParams` knob, so wiring needs a small
  audio-thread-side restructure.

### Demo recording

- [ ] **`demo/scenarios/setup-mc-singer.sh`** — Jungle MC + TTS
  Singer through autotune.  Non-deterministic, 100%
  agent-controlled.
- [ ] **Preecho demo scene** — agent writes anchors into a drum
  pattern and you hear the build-up ramp into each downbeat.
- [ ] **LFO assignment scene** — agent schedules filter sweep via
  the per-voice bass LFO.
- [ ] **Parameter ramp scene** — gradual cutoff sweep over bars.
- [ ] **Event stream scene** — Huth-colored note history scrolling
  in real time.
- [ ] **Re-record the D&B demo** — the 0.7.5 scenario rewrite
  with amen + reese + drone pad + MC is ready; waiting on a clean
  recording run.

### Refactoring

- [ ] **Panel typography constants** — define
  `FONT_XS`/`FONT_SM`/`FONT_MD`/`FONT_LG`.
- [ ] **Panel spacing constants** — define
  `SPACING_SM`/`SPACING_MD`/`SPACING_LG`.
- [ ] **Glass group helpers** — `glass_label(ui, text)`,
  `glass_group_height(ctrl)`.
- [ ] **Module card constants** — `TITLE_BAR_H`, `CARD_ROUNDING`,
  `GLASS_ROUNDING`.

### Infrastructure

- [ ] **CI: run LLM integration tests on release** — currently
  manual; automate in GitHub Actions with a Gemma model cache.
- [ ] **Codecov improvement** — currently ~37%; target higher with
  the new DSP and preecho suites.

---

## Known issues

| Issue | Cause | Status |
|-------|-------|--------|
| Wizard always shows on startup | By design — resume or start fresh | Working as intended |
| Hoover doesn't sound like a hoover | DSP tuning, not a code bug | Needs filter sweep shape tuning |
| Pre-echo ignores melodic voices | TB303Step has no velocity field | Planned for v0.7.7 |
| NeuTts Selector mod jacks show only "—" | No NeuTts-specific LfoTarget yet | Needs TTS bus volume on AudioParams |
