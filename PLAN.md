# Impulse Instruct — Roadmap

What's already built is documented in [docs/features.md](docs/features.md).
This file lists **future** work only — completed items are removed once
they ship and are reflected in `features.md`.

---

## Agent tooling — gradual control & expressiveness


## DSP

- [ ] **Send-bus routing — multi-destination sends** — per-cable
  `audio_gain` and FX→FX feedback routes now ship (see `features.md`).
  What's still a follow-up is true SEND semantics: today a voice has
  one linear chain; the `voice_send_gain` map captures only the first
  Voice→FX cable's gain.  Genuine parallel sends (one voice → three
  different FX chains each with their own wet knob) need the voice
  route compiler to produce a list of chains per voice rather than
  one, and the audio thread to mix their outputs.  The feedback clamp
  + current per-cable gain are enough for dub-delay / shimmer reverb
  patches; parallel sends land when a genre demand surfaces.

## Sequencer

- [ ] **Song mode — timeline UI** — per-chain-slot overrides landed
  (state + audio thread + `POST /api/song`).  What's still missing is
  a proper timeline visualisation: a Gantt-style bar per slot with
  length / repeat-count / override indicators, drag-to-reorder, and a
  playhead scrubber.  Today the UI still shows the flat chain row; use
  the API or manual edits on `chain_overrides` until the timeline
  ships.

## Intelligence

- [ ] **Mid-pipeline live state checks** — `pipeline::run_pipeline`
  works on a snapshot.  When the user changes the rack mid-cycle,
  in-flight lanes for newly-removed modules still fire.  The
  defensive `lane_is_live_pub` filter at plan time helps but doesn't
  catch changes after the plan is built; needs an `Arc<RwLock>` or
  callback.
- [ ] **Agent conversation history** — multi-turn within a single jam
  cycle; agent sees its own previous outputs for coherent evolution.
- [ ] **Prompt templates per style** — styles can define custom
  prompt templates that replace the generic "generate all parameters"
  jam prompt.
- [ ] **VRAM-aware model fallback** — when a spawn is rejected,
  auto-suggest or auto-select a lighter model that fits the remaining
  VRAM budget.
- [ ] **Test additional LLM models** — evaluate
  DeepSeek-R1-Distill-Qwen-7B / 14B and Qwen3-8B / 14B for JSON
  accuracy and music theory.  Gemma 4 26B-A4B is now downloadable
  (three quants); needs a head-to-head vs. E4B on the style + bass +
  theory suites.
- [ ] **Style mc_lines / themes UI editor** — allow editing
  `mc_lines` and themes per style from UI preferences.
- [ ] **Auto-sync rack on app start to active style** — currently
  the rack reflects whatever was saved in `session.json`; if the
  user picked Classic Acid then customised the rack, restart
  preserves the customisation.  Open question whether to auto-sync
  on startup or leave it as an explicit re-pick from the dropdown.

## UI / UX

- [ ] **Touch mode improvements** — touch-paint mode for
  mobile / tablet; gesture support for zoom / scroll.
- [ ] **Agent overrides escape clip too** — `agent_card.rs` LED
  already uses a foreground layer.  Step-button / piano / knob LEDs
  are tightly bound to their parents and would leak past widget
  bounds with the same treatment — escalate per-site only when
  actually needed.
- [ ] **Real shaders for LEDs / oscilloscope phosphor** — would
  replace the current multi-circle software glow with a wgpu
  callback for a true HDR bloom + scanline effect.  Scoping this
  requires registering a custom render pipeline; it'd be its own
  subsystem.

## Demo recording

- [ ] **`demo/scenarios/setup-mc-singer.sh`** — Jungle MC + TTS
  Singer through autotune.  Non-deterministic, 100 % agent-controlled.
- [ ] **Preecho demo scene** — agent writes anchors into a drum
  pattern and you hear the build-up ramp into each downbeat.
- [ ] **LFO assignment scene** — agent schedules filter sweep via
  the per-voice bass LFO.
- [ ] **Parameter ramp scene** — gradual cutoff sweep over bars.
- [ ] **Event stream scene** — Huth-coloured note history scrolling
  in real time, with the new past-side log preserving past notes.
- [ ] **Re-record the D&B demo** — amen + reese + drone pad + MC
  scenario is ready; waiting on a clean recording run.

## Refactoring

- [ ] **Panel typography tiers** — if we do this, it should be an
  enum (`FontTier::{Xs, Sm, Md, Lg}` with `.px()`) rather than
  loose constants, so the variant set is closed and call sites
  can't accidentally introduce a 9.25 px one-off.  Only worth doing
  if we collapse the 11 distinct `.size(...)` values to a few
  canonical tiers (visual-design call).
- [ ] **Panel spacing tiers** — same shape.  Only worth it if we
  settle on a small number of canonical gaps.
- [ ] **Glass group helpers** — `glass_label(ui, text)` still to do
  (the inline pattern varies too much across panels for a single
  helper).
- [ ] **Large-file splits (remaining)** — the 2026-04 round pulled
  13 files under the 1000-line cap; top file is now
  `src/audio/dsp/samplers.rs` (973 lines, all one `AmenVoice` impl —
  not easily splittable into sibling voices).  Remaining candidates
  close to the 700-line proactive-split guidance:
  `src/ui/rack_canvas.rs` (919), `src/ui/header.rs` (913),
  `src/state/rack.rs` (893), `src/ui/panels/amen.rs` (892) — none
  are urgent but worth revisiting next round.

## Infrastructure

- [ ] **CI: run LLM integration tests on release** — currently
  manual; automate in GitHub Actions with a Gemma model cache.
- [ ] **Codecov improvement** — currently ~37 %; target higher with
  the new DSP, preecho, mod-overlay, and rack-reachability suites.

---

## Known issues

| Issue | Cause | Status |
|-------|-------|--------|
| LLM-console LED occasionally overlaps the global header log | Module-card LED clip is bounded but not zero-upward; the header log scrolling past the LLM console module reads the bloom in front | `upward_pad = 0` removed the obvious case; if the LED is still visible in front of the header on scroll, the LED's draw layer needs to drop below the header panel's layer (would require painting LEDs on a separate background-priority layer, or moving the LED draw earlier in the frame) |
| Hoover doesn't sound like a hoover | DSP tuning, not a code bug | Needs filter sweep shape tuning |
