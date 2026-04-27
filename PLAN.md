## Impulse Instruct — Roadmap

What's already built lives in [docs/features.md](docs/features.md).
This file is future work only — items move to features.md as they
ship, not held here as history.

---

## Next session — complete the FX / viz / modulation surface

Concrete additions to fill genuine gaps in the current inventory.
Pick from the top down; each row is one focused commit.

### SF2 follow-up

(SF2 modulator + envelope surface fully wired — pitch / filter / volume
LFO targets and the five-stage modulation envelope all shipped.  Next
candidates would be SF2 modulators 8.2 ("default modulators" — CC1
mod-wheel → vib-LFO depth, CC11 expression, etc.) but those are an
orthogonal subsystem from the generator-driven envelope work and not
on the critical path for any current demo.)

---

## Refactor pass (after the FX / viz / module batch)

Targeted, pre-emptive — only act when a file is within ~50 lines
of the 1000-cap or when a duplicated helper has 3+ copies.  Skip
the speculative reorgs.

- [ ] **Top files near cap** — check after this session's adds:
  `state/transitions.rs`, `audio/dsp/params.rs`, `llm/lanes.rs`,
  `sequencer/mod.rs`, `ui/rack_content_fx_extras.rs`,
  `audio/analysis.rs`, `llm/mod.rs`.  Split sibling-style only
  when an actual edit pushes one over.  (Most recent: SF2 mod-env
  ship pushed `audio/dsp/sample_instrument.rs` past 1000 lines;
  resolved by lifting the modulation surface — `RegionLfos` +
  `LfoSlotState` + `RegionModEnv` + `ModEnvState` — into a sibling
  `sample_instrument_modulation.rs`, leaving the voice file at 984.)
- [x] ~~**Shared `resample_mono`**~~ — folded: `sf2_loader.rs`
  now imports `audio_load::resample_mono_linear` (`pub(crate)`,
  slice input).  Single source, three unit tests covering the
  pass-through + half/double-rate cases.
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
