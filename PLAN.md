# Impulse Instruct — Roadmap

What's already built is documented in [docs/features.md](docs/features.md).
This file lists **future** work only — completed items are removed once
they ship and are reflected in `features.md`.

---

## Agent tooling — gradual control & expressiveness

- **FX XY-pad overhaul** — in progress.  Infra + Autotune landed in
  this session: per-instance `RackModule.pad_expanded` (serde default
  true) + `ModuleKind::supports_xy_pad()` gate + effective-grid-size
  plumbing in `arrange_grid` / `find_free_position`; title-bar
  chevron toggle reflows the rack on click.  Remaining:
    - Roll out XY pads to remaining FX (3-knob FX use A/B · A/C · B/C
      cycling via the `xy_pad` widget's existing `num_pairs` param;
      2-knob FX get a direct pad).  Each rollout flips that kind's
      `supports_xy_pad()` arm + adds a pad render to `draw_fx_content`.
      Order: Reverb (3-knob template), then Delay, Chorus, Phaser, EQ,
      Sidechain, Tape, Master, Bitcrush, then Compressor + Shape.
    - Regroup the 808 and 909 glass panes so linked knob clusters
      (kick: pitch+decay+punch, snare: tone+snappy+decay, etc.) sit
      next to their XY pads rather than scattered across the panel.
    - Agent side: verify pad position already tracks agent-driven
      knob changes (the widget derives x/y from the live values each
      frame).  If confirmed, no-op; otherwise add `fx.<name>_xy: [x,y]`
      as a first-class path.
- **Hoover / An1x preecho** — bass voices now consume
  `PreechoConfig.accent_ramp` + `slide_cascade` via the shared `"bass"`
  voice key.  Hoover/An1x `TriggerEvent` variants don't carry accent
  or slide yet, so extending them (and the matching DSP consumers)
  stayed out of the first melodic-preecho commit.

## DSP

- **Dub techno send/return** — dedicated send/return FX workflow for
  dub-style infinite delay feedback chains.

## Sequencer

- **Pattern probability per step** — already implemented but LLM doesn't
  use it well; improve prompt guidance for probability-based patterns.
- **Song mode** — chain patterns with per-chain tempo/style transitions.
- **MIDI export** — export sequencer pattern as `.mid` file.
- **Preecho v2** — note approach (chromatic / scale-step / arp resolving
  to the anchor note), probability ramp, accent/slide trailing, curve
  shapes (exp / log), auto-length from gap between anchors.

## Intelligence

- **Populate per-style `lane_dynamism` maps in `styles.json`.**  The
  schema + scheduler hookup landed in Phase 4, but every style still
  runs on the baked-in defaults.  Fill in genre-appropriate overrides
  (e.g. ambient = bass 0.4 / fx 0.9, hard techno = kit_a 0.95 / fx 0.2).
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
| LLM-console LED occasionally overlaps the global header log | Module-card LED clip is bounded but not zero-upward; the header log scrolling past the LLM console module reads the bloom in front | `upward_pad = 0` removed the obvious case; if the LED is still visible in front of the header on scroll, the LED's draw layer needs to drop below the header panel's layer (would require painting LEDs on a separate background-priority layer, or moving the LED draw earlier in the frame) |
| Hoover doesn't sound like a hoover | DSP tuning, not a code bug | Needs filter sweep shape tuning |
| Pre-echo ignores Hoover and An1x | Their `TriggerEvent` variants carry only `note` (no accent/slide); bass melodic preecho already ships | Planned (see "Hoover / An1x preecho" above) |
| NeuTts Selector mod jacks show only "—" | No NeuTts-specific LfoTarget yet | Needs TTS bus volume on AudioParams |
