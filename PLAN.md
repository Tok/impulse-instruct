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

Ordered by value.

### In progress / next

- [ ] **TTS rack module UI panel** - module card for EspeakNgTts/CoquiTts in the rack canvas showing voice/speed/pitch controls inline. Currently TTS is configured via the settings panel only; it should also surface in the rack like other modules.

- [ ] **Audio feedback Phase 2 improvements**
  - Auto-listen mode: AUTO toggle in LISTEN bar, fires every 4 jam cycles when heat > 0
  - Per-voice level bars: 8 mini bars (BAS/K-A/S-A/HH/K-B/S-B/CLP/AMN) from state volume params
  - Watch llama.cpp #21325 for Gemma 4 audio encoder PR; test when it lands

### Post-release

- [ ] **Multiple voices** - `Vec<SynthVoice>`, each with its own sequencer + oscillator + filter. LLM can target "voice 2, more acid".

- [ ] **Multiple LLM instances** - one LLM per voice, or a routing matrix.

- [ ] **Modular cable UI** (Reason-style rack flip) - Tab flips to back panel showing I/O ports + Bezier cables. Infrastructure exists; needs a dedicated interaction layer.

- [ ] **Bloom post-process / UI polish** - Bloom (egui to wgpu render pass, Gaussian blur on bright pixels, additive blend) is GPU-expensive and may not add much over existing chrome finish. Alternative: general UI polish - tighter layout, better contrast on inactive controls, smoother XY pad interactions. Decide when tackling.

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
