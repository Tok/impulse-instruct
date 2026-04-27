## Impulse Instruct — Roadmap

What's already built lives in [docs/features.md](docs/features.md).
This file is future work only — items move to features.md as they
ship, not held here as history.

---

## Next session — complete the FX / viz / modulation surface

Concrete additions to fill genuine gaps in the current inventory.
Pick from the top down; each row is one focused commit.

### SF2 follow-up

- [ ] **Modulator / LFO generators — remaining targets**.  Pitch
  targets shipped (modLfoToPitch + vibLfoToPitch).  Remaining:
  modLfoToFilterFc, modLfoToVolume, modulation envelope
  (modEnvToPitch / modEnvToFilterFc).  Each requires per-slot
  state + per-sample apply in the SampleInstrument voice; mod
  envelope also needs the AHDSR state machine (5 timecents fields:
  delay/attack/hold/decay/release + sustain attenuation).

---

## Refactor pass (after the FX / viz / module batch)

Targeted, pre-emptive — only act when a file is within ~50 lines
of the 1000-cap or when a duplicated helper has 3+ copies.  Skip
the speculative reorgs.

- [ ] **Top files near cap** — check after this session's adds:
  `state/transitions.rs`, `audio/dsp/params.rs`, `llm/lanes.rs`,
  `sequencer/mod.rs`, `ui/rack_content_fx_extras.rs`,
  `audio/analysis.rs`, `llm/mod.rs`.  Split sibling-style only
  when an actual edit pushes one over.
- [ ] **Shared `resample_mono`** — `audio/audio_load.rs` and
  `audio/sf2_loader.rs` both carry near-identical linear-interp
  resamplers.  Lift into a shared helper once a third caller
  appears (or fold both into `audio/audio_load.rs` if the SF2
  loader can re-export).
- [ ] **Glass group helpers** — re-evaluate.  Previous note said
  "the inline pattern varies too much"; with more panels in
  place, the shared cases may have stabilised.

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
- [ ] D&B re-record — amen + reese + drone pad + MC.

---

## Known issues

None.
