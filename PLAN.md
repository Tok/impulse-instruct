# Impulse Instruct - Roadmap

What's already built is documented in [docs/features.md](docs/features.md).

---

## v0.7.1 — next release

### DSP

- [ ] **Gabber kick voice** — dedicated voice (not just preset on 808 kick);
  extreme pitch envelope, hard clipper, layered transient
- [ ] **Per-voice FX sends** — route individual voices to specific FX modules
  via rack cables (data model exists, DSP routing partially wired)
- [ ] **Dub techno send/return** — dedicated send/return FX workflow for
  dub-style infinite delay feedback chains

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

### UI / UX

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
