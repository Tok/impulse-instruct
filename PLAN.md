# Impulse Instruct - Roadmap

A smart synthesizer with a virtual production team inside. Multiple LLM agents
collaborate to write patterns, shape sound, and evolve tracks in real time.

What's already built is documented in [docs/features.md](docs/features.md).

---

## v0.6.0 — Multi-agent release (in progress)

All core multi-agent features are implemented.  Remaining before tagging:

- [ ] Merge `develop` into `main` (55 commits ahead, rebase to resolve 2 behind)
- [ ] Update `Cargo.toml` version to `0.6.0`
- [ ] Tag `v0.6.0` and push

---

## Completed in v0.6.0 cycle

### Multi-model agent infrastructure (Phase 1–3)

- [x] **Server pool** — `LlamaServerPool` manages N llama-server processes,
  ref-counted per model; per-agent `model_path: Option<String>`
- [x] **VRAM budget + startup wizard** — `src/llm/vram.rs` model profiles +
  6 presets (Solo/Duo/Swarm/Band/Voices/Lite); first-launch wizard with GPU
  detection, VRAM budget bar, "Resume last session" default
- [x] **Dynamic agent spawning** — `LlmAction::SpawnAgent` / `DismissAgent`;
  auto-wire control cables on spawn; gated by `agent_autonomy`
- [x] **Cable-driven scope** — Control cables define agent scope; system
  prompt constraint + `apply_llm_update()` enforcement
- [x] **Agent cards** — self-contained: model selector, persona, style,
  conversation mode, thinking toggle, user instructions, VRAM estimate
- [x] **Console → agent routing** — prompts go to first enabled agent;
  log shows per-agent persona names

### Other v0.6.0 features

- [x] Rackable LLM agents + LLM Console module
- [x] Rack flip (front knobs / back cables)
- [x] Separate heat/temperature sliders
- [x] Smooth style transitions via `ParamRamp`
- [x] DSP load sparkline, phosphor oscilloscope
- [x] Centered module card layout, row fill/centering
- [x] Volatile rack_flipped (always starts front view)

---

## Audio Feedback Loop

**Phase 1 is implemented.** LISTEN button captures audio, runs per-band RMS +
transient analysis, prepends structured snapshot to prompt.

**Phase 2 (real audio input to the model) is on hold.** llama.cpp does not yet
support Gemma 4's audio encoder. See [docs/audio-feedback.md](docs/audio-feedback.md).

---

## Future (v0.6.1+)

#### Visualization & statistics modules

- [ ] **Spectrum analyser** — real-time FFT magnitude display (rackable module)
- [ ] **Stereo correlation meter** — phase correlation + L/R balance bar
- [ ] **Pattern heatmap** — step-grid overlay showing fire frequency
- [ ] **LLM activity timeline** — structured, filterable agent activity log

#### Visual treatment (post-process pass)

- [ ] **Bloom** — Gaussian blur on bright pixels, additive blend (needs wgpu)
- [ ] **Scan-line / CRT vignette** — cheap fullscreen quad shader
- [ ] **LED glow on active steps** — per-step additive glow ring
  (can approximate in egui with layered circles if wgpu too expensive)

#### Other

- [ ] **Multiple voices (per-voice DSP params)** — voices 1-3 currently share
  synth params with voice 0; next step: per-voice `AudioParams` snapshot

- [ ] **Gabber kick voice** — pitch-envelope ramp + hard clipper

- [ ] **Windows code-signing** — unsigned `.exe` triggers SmartScreen

- [ ] **Bipolar param_control variant** — for `bass.osc_detune` (-1..+1 st)
  and similar bipolar controls that bypass lock/focus

- [ ] **Event queue ring visualisation** — render the rtrb ring buffer as a
  circular display with moving read/write heads

---

## Known Gaps (styles vs synth reality)

| Style | What it promises | What's still missing |
|-------|-----------------|----------------------|
| Hoover lead | Classic Human Resource vacuum-cleaner screech | Resonant sweep shape needs tuning |
| Ambient | Glacial filter sweeps, very slow LFO movement | Long attack/decay times; LFO automation wired but not reliable |
| Dub techno | FX IS the music - send/return model | Per-voice FX buses wired; dedicated send/return workflow not yet surfaced |

Acid bass works well. 808/909 drums work well. The gap between what PULSE intends and what the synth produces is where most roughness lives.

---

## Model Options

The llama-server backend is model-agnostic - swap the GGUF and update the model selector.
Gemma 4 E4B is the default: best test scores (39/39 integration tests), fast, compact.

| Model | Download | Size | VRAM | Notes |
|-------|----------|------|------|-------|
| **Gemma 4 E4B Q4_K_M** | `./scripts/download-models.sh` | ~4.6 GB | ~6 GB | **Default**; best accuracy, 39/39 tests |
| **Bonsai-8B Q1_0_g128** | `./scripts/download-models.sh bonsai` | ~1.1 GB | ~2 GB | Lightweight fallback; no CoT, needs PrismML server fork |
| **DeepSeek-R1-Distill-Qwen-7B** | `./scripts/download-models.sh deepseek-r1-7b` | ~5 GB | ~7 GB | CoT capable, Qwen2.5 base; MIT license |
| **DeepSeek-R1-Distill-Qwen-14B** | `./scripts/download-models.sh deepseek-r1-14b` | ~9 GB | ~11 GB | CoT, higher quality; needs 12 GB VRAM |
| **Qwen3-8B Q4_K_M** | `./scripts/download-models.sh qwen3` | ~5 GB | ~7 GB | Optional; chain-of-thought; not recommended (heavier, no accuracy gain over Gemma 4) |
| **Qwen3-14B Q4_K_M** | `./scripts/download-models.sh qwen3-14b` | ~9 GB | ~11 GB | Optional large; needs 12 GB VRAM |
| Any other GGUF | drop in `models/` | varies | varies | Technically compatible; prompt not tuned for most. See [docs/contributions.md](docs/contributions.md) for how to benchmark. |

All models require a free HuggingFace account (`huggingface-cli login`).
