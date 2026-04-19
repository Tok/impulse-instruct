# Impulse Instruct — Roadmap

What's already built is documented in [docs/features.md](docs/features.md).
This file lists **future** work only — completed items are removed once
they ship and are reflected in `features.md`.

---

## Agent tooling — gradual control & expressiveness


## DSP


## Sequencer


## Intelligence

- [ ] **Test additional LLM models** — evaluate
  DeepSeek-R1-Distill-Qwen-7B / 14B and Qwen3-8B / 14B for JSON
  accuracy and music theory.  Gemma 4 26B-A4B is now downloadable
  (three quants); needs a head-to-head vs. E4B on the style + bass +
  theory suites.

## UI / UX


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

- [ ] **Glass group helpers** — `glass_label(ui, text)` still to do
  (the inline pattern varies too much across panels for a single
  helper).
- [ ] **Large-file splits (remaining)** — `src/audio/dsp/samplers.rs`
  (973 lines) is the top file now; its whole body is one
  `AmenVoice` impl that doesn't split cleanly into sibling voices.
  Next candidates near the proactive-split guidance:
  `src/ui/panels/amen.rs` (892 lines) — worth revisiting next round.

---

## Known issues

| Issue | Cause | Status |
|-------|-------|--------|
| LLM-console LED occasionally overlaps the global header log | Module-card LED clip is bounded but not zero-upward; the header log scrolling past the LLM console module reads the bloom in front | `upward_pad = 0` removed the obvious case; if the LED is still visible in front of the header on scroll, the LED's draw layer needs to drop below the header panel's layer (would require painting LEDs on a separate background-priority layer, or moving the LED draw earlier in the frame) |
| Hoover doesn't sound like a hoover | DSP tuning, not a code bug | Needs filter sweep shape tuning |
