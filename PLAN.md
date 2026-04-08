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

- [x] **Long attack/release envelopes** — AN1X ADSR: attack up to 10s, release
  up to 30s, decay 8s; bass 303 decay extended to 5s; LLM schema updated
- [x] **Granular texture module** — new voice: loads WAV via AudioCommand, plays
  overlapping grains (up to 32) with Hann window, density/size/position/jitter/
  pitch scatter/spray params; full state/DSP/UI/LLM schema/rack integration
- [x] **Tape delay with modulation** — wow/flutter LFO modulates delay read
  position, tape saturation soft-clips feedback, max time extended to 2s;
  new params: delay_wow_flutter, delay_saturation
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

- [x] **llm_apply.rs tests** — 68 tests covering all top-level keys (bass,
  bass_voices, sequencer, kit_a, kit_b, fx, lfo, free_eg, noise, hoover, an1x,
  euclidean, rack routing, scope, step array utility)
- [x] **persistence.rs tests** — 25 tests: SessionData round-trip, apply_session
  field coverage, AppState serde, project save/load, format_llm_display
- [x] **transitions.rs tests** — apply_gabber_kick_preset added; connect_control
  and spawn_agent helper tests added
- [x] **Cable wiring helper** — `rack.connect_control(from_id, to_id)` replaces
  8-line PortRef boilerplate across 6 call sites
- [x] **Agent spawn helper** — `spawn_agent()` pure function in transitions.rs;
  wizard.rs and SpawnAgent handler refactored to use it
- [x] **Sweep state/ for impure code** — removed dead `sync_default_agent`;
  remaining `&mut AppState` in llm_helpers are internal to pure boundary
- [x] **Extract pure logic from UI** — `format_llm_display()` extracted from
  drain_llm_outputs into transitions.rs with 7 tests

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
