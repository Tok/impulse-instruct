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

### TOP PRIORITY — make TTS audibly work again

TTS regressed during the 0.7.5 session — runtime mc_line synthesis
is silent even when:

- the NeuTTS Python server is up (panel status shows ONLINE),
- the voice_ref points at a valid `voices/<name>.wav`,
- the log shows `► <mc_line>` (LLM produced the line) and
  `[TTS] module=<id>: <text>` (speak_neutts was called),
- the `NeuTTS: pushed N/M samples to ring buffer` confirmation
  appears (samples reached the tts ring buffer).

Despite all that, nothing comes out of the speakers.  So the break
is downstream of the ring-buffer push — either the DSP mix path
isn't consuming tts_consumer, the TTS voice's FX chain isn't
routing to master, or the ducking logic is zeroing the signal.

Debug plan:
- [ ] **Log tts_consumer pops at DSP side** — add a periodic
  `log::trace` that dumps popped sample count per block, so we can
  confirm the audio thread is actually draining.
- [ ] **Check FX-plan routing when NeuTts module is present** —
  compile_fx_plan returns chain_tts; verify it's populated and
  that handle_trigger isn't ignoring it.
- [ ] **Audit the duck envelope** — `tts_duck` smoothing starts
  at what value?  If it starts at 0 and never ramps up, the
  synth-out wins but tts_sig contribution is zeroed by
  (synth_out * tts_duck + tts_sig) math … actually that formula
  adds tts_sig unmodulated, so ducking only attenuates synth not
  TTS.  Still, double-check.
- [ ] **Sanity: feed a DC pulse directly into tts_tx** — click SAY
  with a known WAV and scope the master output for the expected
  duration.  If still silent, the break is in the DSP; if audible,
  something in speak_neutts is producing silence or wrong-format
  WAV bytes.
- [ ] **Check WAV decode** — the read_wav_f32_bytes parser may be
  rejecting the NeuTTS Air output format (different bit depth /
  container).
- [ ] **Add an integration test** — construct a DspState with a
  NeuTts module, push known samples to tts_tx, run process_block
  for N blocks, assert non-zero output.

Once fixed, re-record the D&B demo with a working MC line.

### Regression coverage

Logic added in 0.7.4/0.7.5 (amen slicing, bass LFO, preecho, rack
mutations from LLM) is covered by unit tests for the pure pieces but
not yet end-to-end.  Before adding another large feature batch,
write integration-style tests against a mocked DSP and AppState:

- [ ] **Per-voice LFO regression tests** — route sweep for each
  target, fade-in honors lfo_delay, bpm_sync rate matches expected
  Hz for known BPMs.
- [ ] **Preecho integration tests** — build a SequencerState with
  an active preecho config, step advance_clock by known N samples,
  assert emitted DrumTrigger velocities match the ramp math.
- [ ] **Amen slice playback tests** — with a known sample buffer,
  trigger slice N and assert the read-range matches.
- [ ] **LLM rack.add / rack.remove** — round-trip an agent JSON
  through apply_llm_update and assert the rack ends up in the
  expected state.
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

- [ ] **Gabber kick voice** — dedicated voice (not just preset on
  808 kick); extreme pitch envelope, hard clipper, layered transient.
- [ ] **Pan FX module** — insertable rack module for per-chain
  stereo placement.  Knobs: pan position, width, auto-pan rate
  (LFO).
- [ ] **Pan in sequencer** — per-step pan value for bass voice
  (like velocity but L/R).
- [ ] **LFO target: StereoWidth** — modulate stereo width over
  time (auto-pan).
- [ ] **Dub techno send/return** — dedicated send/return FX
  workflow for dub-style infinite delay feedback chains.
- [ ] **Pitch-preserving BPM stretch on amen** — current stretch
  shifts pitch; phase-vocoder / granular stretch for when you want
  to match tempo without the pitch change.
- [ ] **Per-slice playback direction on amen** — currently reverse
  is a global flag; per-slice reverse would enable edit-era glitch
  patterns.

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

- [ ] **Total smart randomization** — one-click random setup: pick
  a random style, add appropriate instruments, set random (but
  musically coherent) parameters, generate a pattern.
  API: `POST /api/randomize`.
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
- [ ] **Style → rack auto-setup** — add `rack_modules` field to
  `styles.json` entries, listing which modules to add and how to
  wire them when a style is selected.
- [ ] **Style-aware agent-preset naming** — styles override the
  default multi-agent setup name ("Crew" → "Band" / "Posse" /
  "Squad" / "Ensemble" per style).

### UI / UX

- [ ] **Quick-command buttons on LLM agent card** — pill buttons
  for common re-prompts (Rewrite melody / rhythm / both,
  Variation, Fill, Sparser, Busier, Brighter, Darker, Swap style).
  Respect agent scope.
- [ ] **Rack CV cables driving LFO targets** — cables are visual
  only; wire them to actually change LFO target at DSP level.
- [ ] **Touch mode improvements** — touch-paint mode for
  mobile/tablet; gesture support for zoom/scroll.
- [ ] **Bass voice LFO panel row** — the LLM can write
  `bass.lfo_*` today but the panel doesn't have knobs for manual
  control yet.
- [ ] **Preecho step-strip lane alignment** — the strip lives in
  the preecho section; aligning it with the sequencer's step
  grid above would make the anchors read more obviously as
  "positions in the pattern".

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
