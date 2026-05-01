## Impulse Instruct — Roadmap

What's already built lives in [docs/features.md](docs/features.md).
This file is future work only — items move to features.md as they
ship, not held here as history.

---

## Refactor pass

Targeted, pre-emptive — only act when a file is within ~50 lines
of the 1000-cap or when a duplicated helper has 3+ copies.  Skip
the speculative reorgs.

- [ ] **Top files near cap** — watchlist for the next session's
  edits: `state/transitions.rs`, `audio/dsp/params.rs`,
  `llm/lanes.rs`, `sequencer/mod.rs`,
  `ui/rack_content_fx_extras.rs`, `audio/analysis.rs`,
  `llm/mod.rs`.  Split sibling-style only when an actual edit
  pushes one over.

---

## Intelligence

- [ ] **Test additional LLM models** — DeepSeek-R1-Distill-Qwen
  (7B / 14B), Qwen3 (8B / 14B), Gemma 4 26B-A4B (three quants
  available).  Head-to-head vs Gemma 4 E4B on the style + bass +
  theory suites.

---

## Demo recording (non-coding, audio capture)

- [ ] Acid demo re-record — two bass voices + delay/phaser/chorus/
  ringmod, bigger NeuTTS quant for the MC line.
- [ ] `setup-mc-singer.sh` — Jungle MC + TTS Singer through autotune.
- [ ] Preecho — agent writes drum anchors, build-up audible.
- [ ] LFO assignment — agent schedules filter sweep via per-voice
  bass LFO.
- [ ] Parameter ramp — gradual cutoff sweep across bars.
- [ ] Event stream — Huth-coloured note history scrolling live.
- [ ] D&B re-record — amen + reese + Salamander Grand piano stabs + MC
  (script: `demo/scenarios/style-dnb.sh`, streamlined kit: amen, bass,
  sample, reverb, delay).

---

## Known issues

None.
