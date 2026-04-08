# Impulse Instruct - Roadmap

What's already built is documented in [docs/features.md](docs/features.md).

---

## v0.7.0 — released

- [x] Verify app starts after reboot (cyclic cable fix)
- [x] VRAM budget guard
- [x] 431 tests, all green
- [x] Merge develop → main

---

## v0.7.1 — next release

### Polish — quick wins

- [ ] **Wire observe_user_edit into panel code** — currently reverted from
  push_audio_params (caused deadlock); needs a separate call path that only
  fires from direct knob edits, not system code
- [ ] **Granular spray as true stereo** — currently adds to stereo_width;
  ideally the GranularVoice returns (L, R) pair using per-grain pan values
- [ ] **Session migration: auto-wire new voice modules** — NoiseVoice and
  GranularTexture get added to rack by migration but don't get FX cables
  wired (only control cables are auto-wired)
- [ ] **CRT overlay performance** — scan-line overlay draws ~360 lines/frame;
  consider rendering to texture or reducing line density
- [ ] **Ring scope** — disabled for performance (256 line segments/frame);
  rewrite with a single polyline or texture-based approach

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
| Agents override user's style choice | Agent sends SetStyle action | Fixed: `style_lock` (default: on) |
| Hoover doesn't sound like a hoover | DSP tuning, not a code bug | Needs filter sweep shape tuning |

---

## v0.7.0 changelog

### Refactor & coverage
- llm_apply_tests (68), persistence_tests (25), helpers_tests, music_tests (13),
  dsp_tests (16), fx_plan_tests, vram_tests (9) — 431 tests total
- `connect_control`, `spawn_agent`, `format_llm_display`, `propagate_style` helpers
- Bass303, samplers, dsp_util, fx_plan extracted to separate files

### Ambient / textural synthesis
- Long envelopes (10s attack, 30s release), granular voice (32 grains, WAV load),
  tape delay (wow/flutter + saturation), reverb freeze, 4 pad presets,
  noise improvements (AR env, filter LFO, S&H), cross-modulation

### DSP improvements
- Per-voice bass params, sidechain compression, multiband compressor, stereo width

### UI / UX
- Footer mode locks, module collapse, shortcuts overlay, undo for agents,
  heat slider, % displays (HEAT/MON/VRAM/RAM), style lock
- Grayscale cable colors (R=G=B), wizard always shows on startup

### Visualization
- CRT scan-line overlay, ring scope (disabled for performance)

### Intelligence
- Agent memory (20 snippets), style learning, inter-agent messaging (SendHint)
- VRAM budget guard (estimate_total_vram, would_exceed_vram, pool acquire_with_vram)

### Infrastructure
- Version bump to v0.7.0, Windows code-signing in build script
- Cyclic cable detection (connect rejects, session strips, compile_fx_plan safe)
- 5s audio timeout, startup diagnostic logging
