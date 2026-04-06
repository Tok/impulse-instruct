# Impulse Instruct - Roadmap

A synthesizer with a tiny LLM living inside it that actually understands music.
PULSE listens, jams, evolves, and shouts at the crowd.

What's already built is documented in [docs/features.md](docs/features.md).

---

## Audio Feedback Loop - "PULSE Listens to Itself"

**Phase 1 is implemented.** The LISTEN button in the LLM strip captures up
to 10 seconds of audio, runs a per-band RMS + transient analysis
(`src/audio/analysis.rs`), shows the stats inline, and prepends a structured
text snapshot to the inference prompt. Responses are labelled **LISTEN ->**
in the log.

**Phase 2 (real audio input to the model) is on hold.** As of April 2026,
llama.cpp does not support Gemma 4's audio encoder - and even when it does,
the encoder was trained on speech only, so musical audio may yield poor
results anyway. The text descriptor approach is likely the better fit for
mix/arrangement feedback regardless.

See **[docs/audio-feedback.md](docs/audio-feedback.md)** for full research
findings, PR numbers to watch, the API format once support lands, and an
alternative Ultravox-as-secondary-listener sketch.

---

## What's Left

Ordered by value.  Branch: **`develop`** (merge to `main` only for tagged releases).

---

### Completed this sprint (develop, not yet merged to main)

All items shipped in this sprint are documented in [docs/features.md](docs/features.md).

---

### Next sprint — v0.5.9 queue

- [ ] **Sequencer piano-roll note names** — melodic step rows (bass, hoover, AN1X) should show the note name (`C4`, `A#3`) inside each active step cell (or as a tooltip), colored with Huth.

- [ ] **Per-voice FX send UI** — surface the voice→FX cable routing in a compact matrix view (voice rows × FX columns, checkbox grid), so users can easily route individual voices to specific FX without navigating the full rack overlay.

- [ ] **Session autosave interval setting** — currently saves on every state change (throttled); add a Preferences option: immediate / 5 s / 30 s / manual only.

- [ ] **Even control spacing / responsive layout** — knobs and sliders in mixed rows should distribute remaining width evenly rather than left-aligning with gaps. egui 0.28 has no built-in flex; requires a pre-pass that computes per-control width from `ui.available_width()` before the draw pass. Must remain consistent when switching knob↔slider mode and across all pad/knob size settings.

---

### Post-release backlog

- [ ] **Multiple voices** — `Vec<SynthVoice>`, each with its own sequencer + oscillator + filter.  LLM can target "voice 2, more acid".

- [ ] **Multiple LLM instances** — one LLM per voice, or a routing matrix.

- [ ] **Modular cable UI** (Reason-style rack flip) — Tab flips to back panel showing I/O ports + Bezier cables.  Drag-to-patch interaction is now live; remaining work is the visual back-panel flip.

- [ ] **Separate LLM heat slider** — decouple "LLM temperature" from "jam energy / mutation rate" so they can be tuned independently.  Currently both are driven by the single heat value.

- [ ] **Bloom post-process** — Gaussian blur + additive blend on bright pixels.  Needs custom wgpu render pass; GPU-expensive.  Evaluate after ui-rework pass.

- [ ] **Windows code-signing** — unsigned `.exe` triggers SmartScreen.  Requires EV certificate.  Low priority until meaningful Windows user base.

- [ ] **Alternate tuning tables** — gamelan slendro, just intonation, etc.  Data-modelled; not wired into DSP.

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
