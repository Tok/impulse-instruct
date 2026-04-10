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
- [ ] **Style → rack auto-setup** — add `rack_modules` field to `styles.json`
  entries, listing which modules to add and how to wire them when a style is
  selected. E.g. `"rack_modules": ["bass", "808", "reverb", "delay"]` for acid,
  `"rack_modules": ["an1x", "reverb"]` for baroque. Selecting a style resets
  the rack and adds the listed modules automatically.

### TTS as rack module (refactor)

- [x] **TTS settings per module** — `TtsModuleState` struct with engine, pitch,
  speed, amplitude, voice_char, randomise, pitch_snap. Stored in
  `AppState.tts_modules`, keyed by rack module id.
- [x] **Agent → TTS routing** — inference path checks for control cables from
  agent to TTS modules. No cable = no speech. Settings read from module.
- [x] **Global TTS fields deprecated** — legacy `tts_*` fields on `LlmState`
  kept for session.json backward compat (serde default), ignored at runtime.
  TTS panel reads/writes module state.
- [x] **API `tts: true`** — auto-adds a TTS module and wires a control cable
  from the agent.
- [ ] **Agent self-add TTS module** — if an agent is in MC/DJ mode and has no
  TTS module connected, it can add one to the rack and wire itself to it
  (unless restricted by scope or a `can_modify_rack: false` flag). This is
  a rack tool-use action, not implicit — the agent requests it via JSON.

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

### Demo recording — next implementation priority

#### Infrastructure (do first)

- [x] **CoquiTTS for narration** — `tts_generate()` tries CoquiTTS first, falls
  back to espeak-ng. Configurable via `TTS_MODEL` env var.
- [x] **Scenario directory** — `demo/scenarios/` with `--scenario X` flag on
  `record-demo.sh` (default: intro). High-level DSL in `lib.sh` for readable scripts.
- [x] **`--skip-video` flag** — run scenario without ffmpeg recording, for headless
  style verification.
- [x] **`--skip-narration` rename** — `--skip-narration` alias added alongside
  `--no-tts`.

#### Style demo scripts (tutorial + verification)

Each style demo follows a template:
1. Reset rack → add instruments appropriate for the style
2. Narrate what we're building and why
3. Send prompts to agents with style-specific instructions
4. Show the agents working (wait for inference, scroll to instruments)
5. Demonstrate key features (ramps, LFO, filter sweeps)
6. Use `POST /api/scroll { "target": "bass", "collapse_others": true }` to focus
7. End with a jam session showing the style in action

Scripts to create:
- [x] **`demo/scenarios/style-acid.sh`** — raw acid: 303 squelch, 808 percussion,
  ramps, parameter locking. 8 scenes.
- [x] **`demo/scenarios/style-dnb.sh`** — 170 BPM, reese bass, rolling hats. 7 scenes.
- [x] **`demo/scenarios/style-ambient.sh`** — AN1X pads, deep reverb, slow ramps. 6 scenes.
- [x] **`demo/scenarios/style-techno.sh`** — kick-driven, minimal, tension/release. 7 scenes.
- [x] **`demo/scenarios/style-bach.sh`** — AN1X only, no drums, classical counterpoint,
  cathedral reverb. 7 scenes.

#### Setup demo scripts (capability showcase)

- [ ] **`demo/scenarios/setup-mc-singer.sh`** — Jungle MC + TTS Singer through
  autotune. Non-deterministic, 100% agent-controlled. Shows: multi-agent, TTS
  pitch-snap pipeline, creative potential. Narrated as "watch what happens."
- [x] **`demo/scenarios/setup-multi-agent.sh`** — BASS + DRUMS + FX specialist
  agents with Bonsai models. 8 scenes: scoping, cable wiring, independent evolution.
- [x] **`demo/scenarios/style-synthwave.sh`** — AN1X pads, arpeggiated bass, 808,
  TTS MC agent spitting neon poetry via Bonsai. 8 scenes.
- [x] **`demo/scenarios/setup-ramp-lfo.sh`** — Parameter ramps: single, multiple,
  build/drop dynamics. 6 scenes.

#### Feature demo scenes (can be standalone or embedded)

- [ ] **ADSR scene** — agent shaping bass envelope (attack/decay)
- [ ] **LFO assignment scene** — agent schedules filter sweep via LFO
- [ ] **Parameter ramp scene** — gradual cutoff sweep over bars
- [ ] **Event stream scene** — Huth-colored note history scrolling in real time
- [ ] **Collapse/focus scene** — API collapse_others to isolate a section

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
