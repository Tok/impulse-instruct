# Impulse Instruct - Roadmap

What's already built is documented in [docs/features.md](docs/features.md).

---

## v0.7.1 — next release

### Agent tooling — gradual control & expressiveness

- [ ] **LFO-as-tool for agents** — agents can schedule an LFO on any target
  (cutoff, resonance, reverb mix, etc.) to introduce changes gradually
  instead of jumping to a value. JSON schema: `"lfo_assign": {"target": "bass.cutoff", "rate": 0.5, "depth": 0.3}`
- [x] **Parameter ramps** — agents can set a target value + ramp duration
  (e.g. "move cutoff from 0.3 to 0.8 over 4 bars"). Interpolated per UI frame.
  JSON: `"ramp": {"param": "bass.cutoff", "to": 0.8, "bars": 4}` or `"ramps": [...]`
- [ ] **XY pad control** — expose cutoff/resonance pad as a first-class
  tool the agent can move. Currently agents set values but the pad
  position doesn't visually track mid-change
- [ ] **ADSR envelope shaping** — expose attack/decay/sustain/release as
  agent-controllable parameters with the same ramp/LFO tooling
- [ ] **Moderate defaults in system prompt** — discourage extreme values
  (reverb mix > 0.5, delay feedback > 0.6, etc.) unless explicitly
  asked. Guide agents toward musical subtlety over dramatic resets
- [ ] **Velocity/volume awareness** — prompt guidance to keep drum volumes
  balanced; prevent clap/snare rush at uncomfortable levels

### DSP

- [ ] **Gabber kick voice** — dedicated voice (not just preset on 808 kick);
  extreme pitch envelope, hard clipper, layered transient
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

- [ ] **Agent conversation history** — multi-turn within a single jam cycle;
  agent sees its own previous outputs for coherent evolution
- [ ] **Prompt templates per style** — styles can define custom prompt
  templates that replace the generic "generate all parameters" jam prompt
- [ ] **VRAM-aware model fallback** — when spawn is rejected, auto-suggest or
  auto-select a lighter model that fits the remaining VRAM budget
- [ ] **Jam-via-API** — currently API prompts are always one_shot (no jam loop).
  Need safe jam support that doesn't do full-state replacement.

### Feedback & awareness

- [ ] **Auto-listen always on** — listen function (audio analysis → text)
  should default to ON, continuously describing what's playing. Detects
  extremes (snare rushes, heavy reverb, clipping) and makes them visible
  as text context to all agents. Self-correcting feedback loop instead of
  hard-banning patterns.
- [x] **LLM console: 8+ visible lines** — default height increased from 50px
  to 100px (~8 lines visible). Still resizable.

### UI / UX

- [x] **Oscilloscope ring → right of scope strip** — ring scope moved to right
  side of scope strip, enlarged from 40px to 80px. Panel height 48→88px.
- [x] **Round-robin indicator → log strip** — agent schedule display moved to
  right side of log strip in 2-row layout. More visible, near console output.
- [ ] **Module drag reorder** — insertion point indicator works; needs better
  width calculation (now uses screen width, was hardcoded 1200px)
- [ ] **Rack CV cables driving LFO targets** — cables are visual only; wiring
  them to actually change LFO target at DSP level
- [ ] **Preferences: tuning selector** — ComboBox exists in FX MASTER group;
  could also be in Preferences for discoverability
- [ ] **Touch mode improvements** — touch-paint mode for mobile/tablet;
  gesture support for zoom/scroll
- [ ] **observe_user_edit in remaining panels** — currently only bass panel;
  extend to 808, 909, hoover, AN1X, noise, granular, FX panels

### Demo recording

- [ ] **Demo: ADSR scene** — show an agent shaping bass envelope (attack/decay)
- [ ] **Demo: LFO assignment scene** — agent schedules a filter sweep via LFO
- [ ] **Demo: parameter ramp scene** — show gradual cutoff sweep over bars
- [ ] **Minimal rack wizard preset** — start with only seq + master + console
  (no default instruments), available as a wizard option

### Infrastructure

- [ ] **CI: run LLM integration tests on release** — currently manual;
  automate in GitHub Actions with a Gemma model cache
- [ ] **Codecov improvement** — currently ~37%; new test suites should push
  it higher once CI runs
- [ ] **Graceful shutdown** — catch SIGINT/SIGTERM, drop audio stream cleanly
  to prevent PipeWire resource leaks

---

## Known issues

| Issue | Cause | Status |
|-------|-------|--------|
| Wizard always shows on startup | By design — resume or start fresh | Working as intended |
| Hoover doesn't sound like a hoover | DSP tuning, not a code bug | Needs filter sweep shape tuning |
| Audio cables decorative only | DSP processes voices directly from state | Needs signal-path gating |
