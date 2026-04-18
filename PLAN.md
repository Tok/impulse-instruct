# Impulse Instruct — Roadmap

What's already built is documented in [docs/features.md](docs/features.md).
This file lists **future** work only — completed items are removed once
they ship and are reflected in `features.md`.

---

## Agent tooling — gradual control & expressiveness

- **XY pad control** — expose cutoff/resonance pad as a first-class tool
  the agent can move.  Currently agents set values but the pad position
  doesn't visually track mid-change.
- **Melodic voice preecho** — TB303Step has no velocity field, only
  accent/slide.  Design a preecho mapping for bass/hoover/an1x that uses
  accent-ramping or slide-cascading instead of velocity scaling.

## DSP

- **Dub techno send/return** — dedicated send/return FX workflow for
  dub-style infinite delay feedback chains.
- **Pitch-preserving BPM stretch on amen** — current stretch shifts
  pitch; phase-vocoder / granular stretch for tempo match without pitch
  change.
- **Per-slice playback direction on amen** — currently reverse is a
  global flag; per-slice reverse would enable edit-era glitch patterns.
- **Reverse mode for compressor envelope** — third FX worth reversing.
  Would give "reverse compression" swell-into-hit transient shaping.

## Sequencer

- **Pattern probability per step** — already implemented but LLM doesn't
  use it well; improve prompt guidance for probability-based patterns.
- **Song mode** — chain patterns with per-chain tempo/style transitions.
- **MIDI export** — export sequencer pattern as `.mid` file.
- **Preecho v2** — note approach (chromatic / scale-step / arp resolving
  to the anchor note), probability ramp, accent/slide trailing, curve
  shapes (exp / log), auto-length from gap between anchors.

## Intelligence

- **Agent conversation history** — multi-turn within a single jam
  cycle; agent sees its own previous outputs for coherent evolution.
- **Prompt templates per style** — styles can define custom prompt
  templates that replace the generic "generate all parameters" jam
  prompt.
- **VRAM-aware model fallback** — when spawn is rejected, auto-suggest
  or auto-select a lighter model that fits the remaining VRAM budget.
- **Test additional LLM models** — evaluate DeepSeek-R1-Distill-Qwen-7B
  /14B and Qwen3-8B/14B for JSON accuracy and music theory.
- **Jam-via-API** — currently API prompts are always one-shot (no jam
  loop).  Need safe jam support that doesn't do full-state replacement.
- **Style mc_lines/themes UI editor** — allow editing mc_lines and
  themes per style from UI preferences.

## UI / UX

- **Touch mode improvements** — touch-paint mode for mobile/tablet;
  gesture support for zoom/scroll.
- **NeuTts mod targets** — Amen/Granular got per-voice LfoTarget
  variants; NeuTts still has none.  Its TTS bus volume isn't an
  `AudioParams` knob, so wiring needs a small audio-thread restructure.
- **Style-dependent rack defaults** — `Full` is one shape today; future
  presets could be style-aware (Jungle / D&B / Acid / Gabber starter
  kits) instead of the generic Empty / Basic / Standard / Full ladder.
- **Project picker** — File menu currently loads the newest
  `project-*.json` from cwd.  A real picker (rfd or in-app file dialog)
  would let the user pick any saved project.
- **Recent projects** sub-menu listing saved sessions.
- **Real shaders for LEDs / oscilloscope phosphor** — would replace the
  current multi-circle software glow with a wgpu callback for a true
  HDR bloom + scanline effect.  Scoping this requires registering a
  custom render pipeline; it'd be its own subsystem.

## Demo recording

- **`demo/scenarios/setup-mc-singer.sh`** — Jungle MC + TTS Singer
  through autotune.  Non-deterministic, 100% agent-controlled.
- **Preecho demo scene** — agent writes anchors into a drum pattern
  and you hear the build-up ramp into each downbeat.
- **LFO assignment scene** — agent schedules filter sweep via the
  per-voice bass LFO.
- **Parameter ramp scene** — gradual cutoff sweep over bars.
- **Event stream scene** — Huth-coloured note history scrolling in
  real time, with the new past-side log preserving past notes.
- **Re-record the D&B demo** — amen + reese + drone pad + MC scenario
  is ready; waiting on a clean recording run.

## Refactoring

- **Panel typography tiers** — if we do this, it should be an enum
  (`FontTier::{Xs, Sm, Md, Lg}` with `.px()`) rather than loose
  constants, so the variant set is closed and call sites can't
  accidentally introduce a 9.25 px one-off.  Only worth doing if we
  collapse the 11 distinct `.size(...)` values to a few canonical
  tiers (visual-design call).
- **Panel spacing tiers** — same shape.  Only worth it if we settle
  on a small number of canonical gaps.
- **Glass group helpers** — `glass_label(ui, text)` still to do (the
  inline pattern varies too much across panels for a single helper).

## Infrastructure

- **CI: run LLM integration tests on release** — currently manual;
  automate in GitHub Actions with a Gemma model cache.
- **Codecov improvement** — currently ~37 %; target higher with the
  new DSP, preecho, mod-overlay, and rack-reachability suites.

---

## Known issues

| Issue | Cause | Status |
|-------|-------|--------|
| Hoover doesn't sound like a hoover | DSP tuning, not a code bug | Needs filter sweep shape tuning |
| Pre-echo ignores melodic voices | TB303Step has no velocity field | Planned (see Sequencer above) |
| NeuTts Selector mod jacks show only "—" | No NeuTts-specific LfoTarget yet | Needs TTS bus volume on AudioParams |
