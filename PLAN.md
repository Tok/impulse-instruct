# Impulse Instruct - Roadmap

What's already built is documented in [docs/features.md](docs/features.md).

---

## v0.7.2 — next release

### Agent tooling — gradual control & expressiveness

- [ ] **LFO-as-tool for agents** — agents can schedule an LFO on any target
  (cutoff, resonance, reverb mix, etc.) to introduce changes gradually
  instead of jumping to a value. JSON schema: `"lfo_assign": {"target": "bass.cutoff", "rate": 0.5, "depth": 0.3}`
- [ ] **XY pad control** — expose cutoff/resonance pad as a first-class
  tool the agent can move. Currently agents set values but the pad
  position doesn't visually track mid-change
- [ ] **ADSR envelope shaping** — expose attack/decay/sustain/release as
  agent-controllable parameters with the same ramp/LFO tooling
- [ ] **Pan as LFO/ramp target** — all voice pan parameters must be
  addressable as LFO targets and ramp targets. Add `LfoTarget::Pan`
  variants and ensure `apply_llm_update` handles `"bass.pan"`,
  `"kit_a.kick.pan"`, etc.
- [ ] **Moderate defaults in system prompt** — discourage extreme values
  (reverb mix > 0.5, delay feedback > 0.6, etc.) unless explicitly
  asked. Guide agents toward musical subtlety over dramatic resets
- [ ] **Velocity/volume awareness** — prompt guidance to keep drum volumes
  balanced; prevent clap/snare rush at uncomfortable levels

### DSP

- [ ] **Gabber kick voice** — dedicated voice (not just preset on 808 kick);
  extreme pitch envelope, hard clipper, layered transient
- [ ] **Pan FX module** — insertable rack module for per-chain stereo placement.
  Knobs: pan position, width, auto-pan rate (LFO)
- [ ] **Pan in sequencer** — per-step pan value for bass voice (like velocity but L/R)
- [ ] **LFO target: StereoWidth** — modulate stereo width over time (auto-pan)
- [ ] **Per-voice FX sends** — route individual voices to specific FX modules
  via rack cables (data model exists, DSP routing partially wired)
- [ ] **Dub techno send/return** — dedicated send/return FX workflow for
  dub-style infinite delay feedback chains
- [ ] **Audio cables gate signal** — currently DSP processes voices regardless
  of cable routing. Audio cables should actually control signal flow.

### Sequencer

- [ ] **Pattern probability per step** — already implemented but LLM doesn't
  use it well; improve prompt guidance for probability-based patterns
- [ ] **Song mode** — chain patterns with per-chain tempo/style transitions
- [ ] **MIDI export** — export sequencer pattern as .mid file

### Intelligence

- [ ] **Total smart randomization** — one-click random setup: pick a random
  style, add appropriate instruments, set random (but musically coherent)
  parameters, generate a pattern. API: `POST /api/randomize`.
- [ ] **Agent conversation history** — multi-turn within a single jam cycle;
  agent sees its own previous outputs for coherent evolution
- [ ] **Prompt templates per style** — styles can define custom prompt
  templates that replace the generic "generate all parameters" jam prompt
- [ ] **VRAM-aware model fallback** — when spawn is rejected, auto-suggest or
  auto-select a lighter model that fits the remaining VRAM budget
- [ ] **Test additional LLM models** — evaluate DeepSeek-R1-Distill-Qwen-7B/14B
  and Qwen3-8B/14B for JSON accuracy and music theory understanding.
  Download scripts exist but models are not yet tuned or integration-tested.
- [ ] **Jam-via-API** — currently API prompts are always one_shot (no jam loop).
  Need safe jam support that doesn't do full-state replacement.
- [ ] **Style mc_lines/themes UI editor** — allow editing mc_lines and themes
  per style from the UI preferences
- [ ] **Style → rack auto-setup** — add `rack_modules` field to `styles.json`
  entries, listing which modules to add and how to wire them when a style
  is selected.

### TTS

- [ ] **Agent self-add TTS module** — if an agent is in MC/DJ mode and has no
  TTS module connected, it can add one to the rack and wire itself to it

### UI / UX

- [ ] **Rack CV cables driving LFO targets** — cables are visual only; wiring
  them to actually change LFO target at DSP level
- [ ] **Touch mode improvements** — touch-paint mode for mobile/tablet;
  gesture support for zoom/scroll

### Demo recording

- [ ] **`demo/scenarios/setup-mc-singer.sh`** — Jungle MC + TTS Singer through
  autotune. Non-deterministic, 100% agent-controlled.
- [ ] **ADSR scene** — agent shaping bass envelope (attack/decay)
- [ ] **LFO assignment scene** — agent schedules filter sweep via LFO
- [ ] **Parameter ramp scene** — gradual cutoff sweep over bars
- [ ] **Event stream scene** — Huth-colored note history scrolling in real time

### Refactoring

- [ ] **Panel typography constants** — define `FONT_XS`/`FONT_SM`/`FONT_MD`/`FONT_LG`
- [ ] **Panel spacing constants** — define `SPACING_SM`/`SPACING_MD`/`SPACING_LG`
- [ ] **Glass group helpers** — `glass_label(ui, text)`, `glass_group_height(ctrl)`
- [ ] **Module card constants** — `TITLE_BAR_H`, `CARD_ROUNDING`, `GLASS_ROUNDING`

### Infrastructure

- [ ] **CI: run LLM integration tests on release** — currently manual;
  automate in GitHub Actions with a Gemma model cache
- [ ] **Codecov improvement** — currently ~37%; target higher with new suites
- [ ] **Graceful shutdown** — catch SIGINT/SIGTERM, drop audio stream cleanly

---

## Known issues

| Issue | Cause | Status |
|-------|-------|--------|
| Wizard always shows on startup | By design — resume or start fresh | Working as intended |
| Hoover doesn't sound like a hoover | DSP tuning, not a code bug | Needs filter sweep shape tuning |
| Audio cables decorative only | DSP processes voices directly from state | Needs signal-path gating |
