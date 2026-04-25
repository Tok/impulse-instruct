# Impulse Instruct - Roadmap

What's already built is documented in [docs/features.md](docs/features.md).
This file lists **future** work only - completed items are removed once
they ship and are reflected in `features.md`.

---

## Sequencer

## UI / UX


## Intelligence

- [ ] **Test additional LLM models** - evaluate
  DeepSeek-R1-Distill-Qwen-7B / 14B and Qwen3-8B / 14B for JSON
  accuracy and music theory.  Gemma 4 26B-A4B is now downloadable
  (three quants); needs a head-to-head vs. E4B on the style + bass +
  theory suites.

## Integration

- [ ] **Ableton Link tempo sync** - bidirectional BPM + bar-phase
  sync via the `ableton_link` crate.  Useful for jamming alongside
  Live / Ableton Push or another synth setup.
- [ ] **MPE / MIDI 2.0 input — DSP integration** - the parser /
  channel-pressure / dispatch wiring shipped (PitchBend +
  ChannelPressure + per-note CC74 land in `AppState.mpe`).  Routing
  these to per-note bass voice modulation (per-note pitch bend,
  pressure → accent, timbre → cutoff) is the follow-up.

## Agent tooling - gradual control & expressiveness


## Refactoring

- [ ] **Glass group helpers** - `glass_label(ui, text)` still to do
  (the inline pattern varies too much across panels for a single
  helper).
- [ ] **Large-file splits (remaining)** - top remaining file now is
  `src/llm/pipeline.rs` (938 lines), followed by `src/llm/mod.rs`
  (914) and `src/state/rack.rs` (897).  None over cap, but worth
  watching on the next feature round.

## Demo recording

- [ ] **Next acid demo re-record** - showcase the **two bass voices**
  (V1 + V2 playing complementary lines), plus FX routes that last
  session's demo didn't cover (delay/phaser/chorus/ringmod).  Use the
  bigger NeuTTS quant for the MC/vocal line.  **Bonsai references
  removed** from the demo script (module no longer in the codebase).
- [ ] **`demo/scenarios/setup-mc-singer.sh`** - Jungle MC + TTS
  Singer through autotune.  Non-deterministic, 100 % agent-controlled.
- [ ] **Preecho demo scene** - agent writes anchors into a drum
  pattern and you hear the build-up ramp into each downbeat.
- [ ] **LFO assignment scene** - agent schedules filter sweep via
  the per-voice bass LFO.
- [ ] **Parameter ramp scene** - gradual cutoff sweep over bars.
- [ ] **Event stream scene** - Huth-coloured note history scrolling
  in real time, with the new past-side log preserving past notes.
- [ ] **Re-record the D&B demo** - amen + reese + drone pad + MC
  scenario is ready; waiting on a clean recording run.

---

## Known issues

| Issue | Cause | Status |
|-------|-------|--------|
| Hoover doesn't sound like a hoover | DSP tuning, not a code bug | Needs filter sweep shape tuning |
