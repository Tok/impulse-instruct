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

- [x] **TTS rack module UI panel** - done: `src/ui/panels/tts.rs`, wired into `draw_voice_content()`.

- [x] **Audio feedback Phase 2 improvements** - done: AUTO toggle fires every 4 jam cycles, 8 per-voice level bars in LISTEN strip. Watch llama.cpp #21325 for Gemma 4 audio encoder PR.

### CI/CD + supply-chain security

- [x] **codecov runs on `develop`** — CI workflow extended to `push/PR` on both `main` and `develop` branches.

- [x] **CI-built release binaries + SHA-256 + SLSA attestation** — the `release` job in `ci.yml` fires on `refs/tags/v*` and:
  1. Builds Linux and Windows binaries entirely inside GitHub Actions (no maintainer-local builds land in releases).
  2. Produces a `.sha256` sidecar for each artifact — users can verify with `sha256sum -c`.
  3. Attests build provenance via `actions/attest-build-provenance` (SLSA level 2), giving a cryptographically signed link from artifact → commit → workflow run.  Users verify with: `gh attestation verify <file> --repo Tok/impulse-instruct`
  
  This directly counters the class of attack where a maintainer's machine is compromised and a swapped binary is uploaded before a release is published.

- [ ] **Snapshot versioning** — adopt `0.5.7-dev` (or `-SNAPSHOT` / `-alpha.N`) for commits on `develop` so nightlies are clearly distinguished from tagged releases.  Cargo convention: use `0.5.8-alpha.1` style pre-release suffixes in `Cargo.toml` on the develop branch, bumping to a clean version only on release tags.  A small `scripts/bump-version.sh` can automate the pattern.

- [ ] **Windows code-signing** — the Windows `.exe` is currently unsigned; SmartScreen may warn on first run.  Acquiring an EV code-signing certificate and wiring it into the CI `release` job is the correct fix, but requires a paid certificate.  Low priority until there are enough Windows users to matter.

### Post-release

- [ ] **Multiple voices** - `Vec<SynthVoice>`, each with its own sequencer + oscillator + filter. LLM can target "voice 2, more acid".

- [ ] **Multiple LLM instances** - one LLM per voice, or a routing matrix.

- [ ] **Modular cable UI** (Reason-style rack flip) - Tab flips to back panel showing I/O ports + Bezier cables. Infrastructure exists; needs a dedicated interaction layer.

- [ ] **UI/UX rework** - Full layout and interaction quality pass. See **[docs/ui-rework.md](docs/ui-rework.md)** for the full issue list. Highest-priority items: (1) cables must not occlude module controls, (2) voice module cards are too narrow, (3) LLM strip needs collapse, (4) module panel internals need skeuomorphic depth — knobs, pads, sliders as physical objects not flat rectangles.

- [ ] **LLM tuning tab in Preferences** - The current settings panel mixes model/context/sampling/personality into a single scrollable list. Split into named tabs: Model, Sampling, Personality, TTS. The Sampling tab exposes ctx_size, top_k, top_p, min_p, repeat_penalty, freq_penalty, seed as an "Advanced / experimental" section, replacing the scroll-through layout. Useful for power users experimenting with different models whose defaults differ from Gemma.

- [ ] **Bloom post-process / UI polish** - Bloom (egui to wgpu render pass, Gaussian blur on bright pixels, additive blend) is GPU-expensive and may not add much over existing chrome finish. Evaluate after the ui-rework pass.

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
