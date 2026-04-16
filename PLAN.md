# Impulse Instruct - Roadmap

What's already built is documented in [docs/features.md](docs/features.md).

---

## Shipped since v0.7.4

For a detailed log see [docs/features.md](docs/features.md).
Quick list of roadmap items that became real:

- [x] **ADSR envelope shaping** — done on the bass voice (full ADSR
  on both amp + filter envelopes, replacing the single `decay` knob).
  AN1X already had ADSR; hoover remains decay-only for now.
- [x] **LFO-as-tool for agents** — the bass voice gained a per-voice
  LFO with routable targets (pitch / PWM / cutoff / amp), accessible
  via `bass.lfo_target` / `lfo_rate` / `lfo_depth` / `lfo_waveform` /
  `lfo_bpm_sync` / `lfo_sync_beats`.
- [x] **Agent self-add TTS module** — `spawn_agent` action with
  `mode: "mc"` auto-adds a NeuTts module and wires the control cable.
  Same path the HTTP API uses.
- [x] **AMEN break chopper** — slicing, transient auto-detect,
  per-slice pitch/volume, BPM stretch, reverse, gate, stutter,
  waveform thumbnail + slice wheel visualisation.
- [x] **Granular CAPTURE workflow** — freeze master-output tap into
  the granular voice; live scrolling ring-buffer viz.
- [x] **Pre-echo pattern modulator** — anchor-driven lead-in
  reinforcement with compact UI (clickable step strip).
- [x] **Per-voice FX sends via rack cables** — DSP routing was
  already wired; demo/D&B scenario now uses amen → reverb as a
  parallel send.
- [x] **Pan as LFO/ramp target** — bass pan was agent-writable
  already; extended and confirmed for the per-voice LFO.

---

## v0.7.6 — next release

### Shipped this cycle

- [x] **TTS audibility fix** — root cause was a sample-rate mismatch:
  NeuTTS Air outputs 24 kHz WAV but the reader only upsampled the
  legacy 22050 → 44100 case.  TtsSink now carries the device target
  rate; `read_wav_f32_bytes` does generic linear resampling.
- [x] **Agent-triggered TTS** — `speak_neutts` call hoisted out of
  the `param_update` gate so MC agents that emit only `mc_line` (no
  param change) still fire TTS.  Warn-log when an MC agent speaks
  but no NeuTts module is wired.
- [x] **Per-knob modulation system** — third cable kind
  (`PortKind::Mod`); every module declares `mod_inputs(kind)`; back
  panel renders Fixed / Selector jacks with multi-select target
  chips, depth slider with `+ / −` polarity toggle and `%` label;
  cables compile into per-block `ModRouteCopy` array; DSP applies
  via shared `apply_mod_target` dispatch.  HTTP API + LLM JSON
  exposure landed.
- [x] **Bass voice LFO panel row** — manual knobs for the
  `bass.lfo_*` params the LLM could already write.
- [x] **Amen step → slice mapping** — when `step.slice == 0`, the
  sequencer substitutes vstep+1 so step N plays slice N (the
  obvious break-chopping behaviour).
- [x] **Shell log colorization** — env_logger format routes through
  `log_fmt::colorize` with grayscale line colors + Huth note
  highlights matching the in-UI log.
- [x] **Huth filter fixes** — model filenames like
  `gemma-4-E4B-it-Q4_K_M.gguf` no longer colour as notes; `44100 Hz`
  colours as one full token instead of embedded blue.
- [x] **Pan in sequencer** — per-step pan on TB303Step, plumbed
  through BassTrigger + DSP step-pan latch, LLM `bass_pans` array.
- [x] **Quick-command pills on agent card** — REWRITE / VARI / FILL
  / SPARSE / BUSY / BRIGHT / DARK pills fire one-shot prompts scoped
  to the agent.
- [x] **Style → rack auto-setup** — Style.rack_modules walked on
  style switch (non-destructive); seeded for acid_classic / jungle /
  drum_and_bass / gabber / dub_techno.
- [x] **POST /api/randomize** — one-click smart randomization: picks
  a random style, applies baseline + rack modules, kicks the LLM
  into a "FULL RESET to <style>" generate.

Re-record the D&B demo with a working MC line — still pending a
clean recording pass.

### Regression coverage

Logic added in 0.7.4/0.7.5 (amen slicing, bass LFO, preecho, rack
mutations from LLM) is covered by unit tests for the pure pieces but
not yet end-to-end.  Before adding another large feature batch,
write integration-style tests against a mocked DSP and AppState:

- [x] **Per-voice LFO regression tests** — `lfo_rate_hz` and
  `lfo_fade_step` extracted as pure helpers; tests cover free vs.
  bpm-sync rate (incl. clamping), waveform endpoints, and a
  fade-in ramp duration check at 44.1 kHz.
- [x] **Preecho integration tests** — `preecho_scales_velocity_through_advance_clock`
  drives a 16-step kick pattern through the real `advance_clock`,
  asserts anchor + lead-in + outside-window velocities match the
  ramp math.
- [x] **Amen slice playback tests** — 7 tests against `AmenVoice`
  (forward, reverse, slice indexing, custom positions,
  auto-advance, stutter inside slice budget).  Surfaced and fixed
  a real bug: reverse playback at `pos == send-1` tripped the
  out-of-bounds neighbour-read kill-switch on the first sample.
- [x] **LLM rack.add / rack.remove** — round-trip tests for
  `rack.add` (auto-cables to master) and `rack.remove` (cleans up
  cables touching the removed module), plus a same-pass add+remove
  round-trip that restores the original module count.
- [ ] **State size-limit watch** — `state/mod.rs` keeps bumping
  against the 1000-line cap.  Extract `bass` / `amen` / `preecho`
  accessors into dedicated modules so the core mod.rs stays lean.

### Agent tooling — gradual control & expressiveness

- [ ] **XY pad control** — expose cutoff/resonance pad as a
  first-class tool the agent can move.  Currently agents set values
  but the pad position doesn't visually track mid-change.
- [ ] **Melodic voice preecho** — TB303Step has no velocity field,
  only accent/slide.  Design a preecho mapping for bass/hoover/an1x
  that uses accent-ramping or slide-cascading instead of velocity
  scaling.

### DSP

- [x] **Gabber kick voice** — dedicated `ModuleKind::GabberKick` with
  its own DSP voice (50–110 Hz base, 1×–13× pitch sweep, hard clip
  into tanh saturator), transient click layer (HPF noise burst with
  its own 8 ms envelope), 4 LfoTarget variants (Pitch / Decay / Clip
  / Pan), and a dedicated UI panel with PITCH / DECAY / P.DEPTH /
  P.TIME / CLIP / TRANS / VOLUME / PAN.  Wired through the full
  stack: DrumVoice → AudioParams → apply_llm_update → rack picker →
  gabber style.
- [x] **Pan FX module** — `ModuleKind::FxPan` + `FxStep::Pan`.
  Pass-through on the mono signal; writes a per-sample auto-pan
  side contribution (`fx_pan_side`) into the master stereo mix
  via LFO (pos + width * sin(2π·rate·t)).  POS / WIDTH / RATE
  knobs, LLM plumbing (`fx.fx_pan_*`), rack parse + name matcher.
- [x] **LFO target: StereoWidth** — `LfoTarget::StereoWidth`
  (opcode 67) lands the recipe in all 6 places; `apply_mod_target`
  scales by 0.5 so a full-depth LFO sweeps the whole 0..1 range.
- [ ] **Dub techno send/return** — dedicated send/return FX
  workflow for dub-style infinite delay feedback chains.
- [ ] **Pitch-preserving BPM stretch on amen** — current stretch
  shifts pitch; phase-vocoder / granular stretch for when you want
  to match tempo without the pitch change.
- [ ] **Per-slice playback direction on amen** — currently reverse
  is a global flag; per-slice reverse would enable edit-era glitch
  patterns.
- [x] **Tempo-quantized FX direction buffer** — `rev_tap_len_for_quant`
  snaps the reverse-tap loop length to a beat division (1s free,
  1/4 bar, 1/2, 1 bar, 2 bars).  `reverb_rev_quant` / `delay_rev_quant`
  in FxState, plumbed through AudioParams; a small quant-cycle button
  sits next to each FWD/REV/MIRROR control.  Extracted the reverse-tap
  buffer helpers into `src/audio/dsp/rev_tap.rs` with 5 unit tests.
- [ ] **Reverse mode for compressor envelope** — third FX worth
  reversing (envelope follower).  Would give "reverse compression"
  swell-into-hit transient shaping.

### Amen — shipped this session

- [x] **Stutter knob fix** — gate window now scaled by `1/(stutter+1)`
  so retriggers fit inside one slice budget instead of running past
  the next step.  Stutter=4 plays five hits in one slice's worth of
  time as expected.
- [x] **Display fixes** — waveform `use_custom` now requires
  `positions.len() >= slice_count`, NaN/Inf positions are filtered,
  active-slice highlight falls back to start/end offsets when a
  custom position is missing or non-finite.
- [x] **Chop randomization** — RAND button next to RESET on the
  ORDER strip; SystemTime-seeded Fisher-Yates shuffle of `0..n`.
- [x] **Animate the wheel** — slice trail with half-bar fade per
  wedge + smoothly rotating outer notch eased toward the active
  slice's mid-angle (shortest-path interpolation).
- [x] **Animate the waveform** — active-slice highlight alpha
  fades out over one step duration; playhead cursor sweeps across
  the active slice's range as it plays.

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
- [x] **Preecho step-strip lane alignment** — strip cells now use
  the sequencer's `effective_pad_px()` (so widths match) and the
  whole strip is right-justified to the panel's right edge with
  the trailing controls (LEN/VEL/RAT/CLEAR/ON) moved to the left
  of it.  Anchor cells line up with the step columns above.
- [x] **Per-step bass pan UI lane** — PAN row added to the bass
  voice section (BASS / ACCENT / SLIDE / PAN).  Drag horizontally
  on a cell to set TB303Step.pan, right-click resets to centre.
  Cell painting factored into `pan_cell` in sequencer_chain.rs to
  keep sequencer.rs under the 1000-line limit.
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
| Pre-echo ignores melodic voices | TB303Step has no velocity field | Planned for v0.7.6 |
| Reverb/Delay REV "rewinds" every ~1 s | Fixed-length reverse-tap buffer | Planned: tempo-quantized buffer length |
| NeuTts Selector mod jacks show only "—" | No NeuTts-specific LfoTarget yet | Needs TTS bus volume on AudioParams |
