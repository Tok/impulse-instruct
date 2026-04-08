# Impulse Instruct - Roadmap

A smart synthesizer with a virtual production team inside. Multiple LLM agents
collaborate to write patterns, shape sound, and evolve tracks in real time.

What's already built is documented in [docs/features.md](docs/features.md).

---

## Next up

Ordered roughly by value. Branch: `develop`.

### Ambient / textural synthesis tools

The synth engine is strong on acid, techno, and breakbeat but weak on ambient,
drone, and textural work. These additions would close that gap:

- [ ] **Long attack/release envelopes** — AN1X and bass ADSR max times are too
  short for glacial pads; extend max attack to 10s, release to 30s
- [ ] **Granular texture module** — new voice: loads a WAV and plays overlapping
  grains with jitter, density, size, pitch scatter; great for ambient beds
- [ ] **Tape delay with modulation** — current delay is clean; add wow/flutter
  modulation, tape saturation on feedback, and longer max time (2s+)
- [ ] **Reverb freeze / infinite hold** — button or param that freezes the
  reverb tail indefinitely (feedback = 1.0), useful for drone/ambient pads
- [ ] **Pad presets** — AN1X presets for warm pad, evolving texture, glass pad,
  sub drone; LLM style entries for "ambient", "drone", "meditation"
- [ ] **Noise voice improvements** — envelope (attack/release), filter LFO,
  sample-and-hold modulation for rhythmic texture
- [ ] **Cross-modulation** — FM between voices (bass → AN1X pitch, noise →
  filter cutoff) for complex evolving textures

### DSP improvements

- [ ] **Per-voice DSP params** — voices 1-3 currently share synth params with
  voice 0; next step: per-voice `AudioParams` snapshot
- [ ] **Sidechain compression** — duck bass/pad under kick; configurable
  attack/release/ratio on the compressor with sidechain input
- [ ] **Multiband compressor** — split into 3 bands before compression for
  more controlled master bus processing
- [ ] **Stereo width control** — mid/side processing on master output

### UI / UX

- [ ] **Clickable footer mode toggles** — double-click [Ctrl]/[Alt]/[Tab]
  indicators to lock that mode on (zoom mode, lock mode, flip mode)
- [ ] **Per-module collapse** — click title bar to collapse a module card to
  just the title (saves vertical space in the rack)
- [ ] **Module drag reorder** — drag modules within a zone to reorder (partially
  implemented via title bar drag, needs polish)
- [ ] **Keyboard shortcuts help overlay** — ? key shows all shortcuts
- [ ] **Undo for agent changes** — agent spawn/dismiss/config changes should
  push to undo history

### Visualization

- [ ] **Bloom / CRT post-process** — Gaussian blur on bright pixels, scan-line
  overlay (needs wgpu render pass or egui approximation)
- [ ] **Event queue ring** — render the rtrb ring buffer as a circular display
  with moving read/write heads (diagnostic, low priority)

### Intelligence

- [ ] **Agent memory** — agents remember previous session context; persist
  conversation snippets across restarts
- [ ] **Style learning** — agent observes user edits and adapts its style
  preferences over time
- [ ] **Inter-agent messaging** — agents can send structured hints to each other
  ("drums agent: I'm building, raise the hat density")

### Refactor and test coverage (37% codecov - red badge)

Priority order — each item is a self-contained session task:

- [ ] **llm_apply.rs tests** — 580 lines, zero tests; pure functions that apply
  LLM JSON to AppState; test each top-level key (bass, kit_a, fx, sequencer...)
  with known JSON input and assert output state
- [ ] **persistence.rs tests** — save/load round-trip: serialize AppState,
  deserialize, compare fields
- [ ] **transitions.rs tests** — many preset/toggle functions lack tests;
  apply_gabber_kick_preset, set_hoover_step, etc.
- [ ] **Cable wiring helper** — `rack.connect_control(from_id, to_id)` to
  replace 8-line PortRef boilerplate in wizard.rs, mod.rs, rack.rs (4+ sites)
- [ ] **Agent spawn helper** — extract shared logic from wizard.rs and
  SpawnAgent handler; single function with persona/scope/model/wiring
- [ ] **Sweep state/ for impure code** — any `&mut AppState` methods should be
  refactored to pure `fn(AppState, ...) -> AppState` per coding-guide.md
- [ ] **Extract pure logic from UI** — drain_llm_outputs has display formatting
  and jam scheduling that could be pure functions

### Infrastructure

- [ ] **Windows code-signing** — unsigned `.exe` triggers SmartScreen

---

## Known Gaps (styles vs synth reality)

| Style | What it promises | What's still missing |
|-------|-----------------|----------------------|
| Hoover lead | Classic Human Resource vacuum-cleaner screech | Resonant sweep shape needs tuning |
| Ambient | Glacial filter sweeps, very slow LFO movement | Long envelopes, granular texture, reverb freeze |
| Dub techno | FX IS the music - send/return model | Per-voice FX buses wired; dedicated send/return workflow not yet surfaced |
| Drone | Sustained evolving textures | Granular module, cross-modulation, infinite reverb |

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
