# Impulse Instruct — Roadmap

What's already built is documented in [docs/features.md](docs/features.md).
This file lists **future** work only — completed items are removed once
they ship and are reflected in `features.md`.

---

## Agent tooling — gradual control & expressiveness

- **XY pad control** — expose cutoff/resonance pad as a first-class tool
  the agent can move.  Currently agents set values but the pad position
  doesn't visually track mid-change.
- **Melodic voice preecho** — now unblocked: `TB303Step.accent` and
  `.slide` are proportional `f32` 0..=1, which serves as the velocity-
  like ramp target.  Design a preecho mapping for bass/hoover/an1x
  that ramps `accent` 0.3 → 1.0 across the N steps before an anchor
  (and optionally cascades `slide` into the anchor).  Drum preecho
  already ships; this is the melodic counterpart.

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
- **Lane fade-in ramp** — when the pipeline lands a new pattern mid-jam,
  ramp the voice's volume up over ~1 bar instead of snapping the new
  loop on.  Core `fx.ramp{s}` machinery already exists (see
  `state/llm_apply.rs::apply_ramps`); needs a per-voice volume ramp
  triggered by `on_lane_applied` callback in the pipeline so the new
  kick fades in while the bass is still being written, etc.

## Sequencer

- **Pattern probability per step** — already implemented but LLM doesn't
  use it well; improve prompt guidance for probability-based patterns.
- **Song mode** — chain patterns with per-chain tempo/style transitions.
- **MIDI export** — export sequencer pattern as `.mid` file.
- **Preecho v2** — note approach (chromatic / scale-step / arp resolving
  to the anchor note), probability ramp, accent/slide trailing, curve
  shapes (exp / log), auto-length from gap between anchors.

## Intelligence

- **Lane lifecycle Phase 2 — weighted scheduler.**  Phase 1 ships
  `LlmState.lane_scores` populated by `lane_eval::evaluate_lane` after
  each successful lane apply, observation-only.  Phase 2 should pick
  the next jam-cycle's lane via weighted random where
  `weight = dynamism(lane, style) * (1 - score) * recency_decay *
  heat_jitter`, fire a single-lane plan instead of the full default,
  and let high-scoring lanes "live longer" between rewrites.  See
  `lane_eval.rs` for the per-lane scoring rules already in place.
- **Lane lifecycle Phase 3 — retry on low score.**  When
  `evaluate_lane` returns below a threshold (~0.3), push the same
  lane back to the front of the next-pick queue so a one-off bad
  output gets a do-over without blocking the round-robin.
- **Lane lifecycle Phase 4 — per-style dynamism in `styles.json`.**
  Optional `lane_dynamism: { "bass": 0.9, "kit_a": 0.85, "fx": 0.4,
  ... }` map per style with sensible defaults baked in (bass + drums
  high, settings low).  Phase 2's weighted picker reads from here.
- **Score viz in LLM console** — currently scores log via `log::info!`
  only.  Need a layout-stable widget (small per-lane grid below the
  cycle viz, or a hover-tooltip on each agent's slot) that doesn't
  reflow each pipeline tick.
- **Mid-pipeline live state checks** — `pipeline::run_pipeline` works
  on a snapshot.  When the user changes the rack mid-cycle, in-flight
  lanes for newly-removed modules still fire.  The defensive
  `lane_is_live_pub` filter at plan time helps but doesn't catch
  changes after the plan is built; needs an Arc<RwLock> or callback.
- **Agent conversation history** — multi-turn within a single jam
  cycle; agent sees its own previous outputs for coherent evolution.
- **Prompt templates per style** — styles can define custom prompt
  templates that replace the generic "generate all parameters" jam
  prompt.
- **VRAM-aware model fallback** — when spawn is rejected, auto-suggest
  or auto-select a lighter model that fits the remaining VRAM budget.
- **Test additional LLM models** — evaluate DeepSeek-R1-Distill-Qwen-7B
  /14B and Qwen3-8B/14B for JSON accuracy and music theory.  Gemma 4
  26B-A4B is now downloadable (three quants); needs a head-to-head
  vs. E4B on the style + bass + theory suites.
- **Jam-via-API** — currently API prompts are always one-shot (no jam
  loop).  Need safe jam support that doesn't do full-state replacement.
- **Style mc_lines/themes UI editor** — allow editing mc_lines and
  themes per style from UI preferences.
- **Auto-sync rack on app start to active style** — currently the
  rack reflects whatever was saved in session.json; if the user
  picked Classic Acid then customised the rack, restart preserves
  the customisation.  Open question whether to auto-sync on startup
  or leave it as an explicit re-pick from the dropdown.

## UI / UX

- **Touch mode improvements** — touch-paint mode for mobile/tablet;
  gesture support for zoom/scroll.
- **NeuTts mod targets** — Amen/Granular got per-voice LfoTarget
  variants; NeuTts still has none.  Its TTS bus volume isn't an
  `AudioParams` knob, so wiring needs a small audio-thread restructure.
- **Style-dependent rack defaults at wizard time** — partially
  addressed: picking a style in the LLM console now reshapes the rack
  via `style_rack::apply` (destructive, reads `rack_modules` from
  styles.json).  The wizard's `RACK_PRESETS` (Empty / Basic /
  Standard / Full) is still generic; could be replaced with a
  style-driven picker so initial setup matches the user's intended
  genre directly.
- **Per-agent cycle viz on agent cards** — Phase 2 of the LLM-console
  cycle widget.  A tiny clock-face mini-circle per agent card showing
  that agent's queue, its turn position in the round-robin, and its
  scheduled-next-fire countdown.  Console version already covers all
  agents on one ring; per-agent gives focus.
- **Agent overrides escape clip too** — `agent_card.rs` LED already
  uses a foreground layer.  Step-button / piano / knob LEDs are
  tightly bound to their parents and would leak past widget bounds
  with the same treatment — escalate per-site only when actually
  needed.
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
| App hangs intermittently after extended jamming | Suspected write-lock contention or runaway memory during long sessions; symptom: UI freezes, log stops emitting | Needs a reproduce-with-stack-trace pass; `apply_style_selection` and the pipeline writeback have been reduced in lock duration but the residual hang isn't pinned down |
| LLM-console LED occasionally overlaps the global header log | Module-card LED clip is bounded but not zero-upward; the header log scrolling past the LLM console module reads the bloom in front | `upward_pad = 0` removed the obvious case; if the LED is still visible in front of the header on scroll, the LED's draw layer needs to drop below the header panel's layer (would require painting LEDs on a separate background-priority layer, or moving the LED draw earlier in the frame) |
| Hoover doesn't sound like a hoover | DSP tuning, not a code bug | Needs filter sweep shape tuning |
| Pre-echo ignores melodic voices | Mapping not wired yet (velocity field blocker resolved — accent/slide are now proportional f32) | Planned (see "Melodic voice preecho" above) |
| NeuTts Selector mod jacks show only "—" | No NeuTts-specific LfoTarget yet | Needs TTS bus volume on AudioParams |
