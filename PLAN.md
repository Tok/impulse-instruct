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
- [x] **Reverb freeze / infinite hold** — `reverb_freeze` bool sets feedback=1.0
  and input=0.0, freezing the tail indefinitely for drone/ambient
- [x] **Pad presets** — 4 AN1X presets (warm pad, evolving texture, glass pad,
  sub drone); meditation style added; dark/space ambient baselines now enable
  AN1X with pad settings and tape delay wow/flutter
- [x] **Noise voice improvements** — AR envelope (attack 5s, release 10s),
  filter LFO (0.05–10 Hz), sample-and-hold modulation (0.5–20 Hz)
- [x] **Cross-modulation** — bass → AN1X pitch FM (±24 st), noise → bass filter
  cutoff; params xmod_bass_to_an1x_pitch, xmod_noise_to_filter in FxState

### DSP improvements

- [x] **Per-voice DSP params** — `BassVoiceParams` struct snapshotted per voice
  in AudioParams; Bass303::process takes per-voice params; voice 0 syncs
  with LFO/free-EG modulated values before processing
- [x] **Sidechain compression** — kick (808+909) ducks bass/pad/hoover/granular;
  sidechain_amount, sidechain_attack (0.1–50ms), sidechain_release (10–500ms)
- [x] **Multiband compressor** — 3-band crossover (200 Hz / 3 kHz) with
  independent per-band envelope followers; compressor_multiband param
- [x] **Stereo width control** — chorus-based decorrelation for stereo
  expansion; 0=mono, 0.5=normal, 1=wide; stereo_width param

### UI / UX

- [x] **Clickable footer mode toggles** — double-click Ctrl/Alt/Tab indicators
  to lock mode on; locks stored in egui temp data, read by widgets/zoom/cables
- [x] **Per-module collapse** — click title bar drag zone to collapse/expand;
  state stored per-module in egui temp data; content hidden when collapsed
- [x] **Module drag reorder** — drag ghost + insertion line indicator; undo
  support on reorder; core slot-swap logic was already working
- [x] **Keyboard shortcuts help overlay** — ? or F1 toggles a foreground overlay
  listing all keyboard shortcuts; close button or re-press to dismiss
- [x] **Undo for agent changes** — push_history() called before agent spawn
  and dismiss mutations, enabling Ctrl+Z to restore previous agent state

### Visualization

- [x] **Bloom / CRT post-process** — egui approximation: scan-line overlay
  (3px spacing, alpha 25) + edge vignette; toggled via crt_effect in UiPrefs
- [x] **Event queue ring** — scope ring: polar waveform plot of scope buffer
  with simulated write head marker; displayed next to linear oscilloscope

### Intelligence

- [x] **Agent memory** — agents persist _comment snippets in memory[] (max 20);
  memory injected into system prompt; survives session restart via session.json
- [x] **Style learning** — observe_user_edit() records "user prefers high/low X"
  into style_observations[]; injected into prompt as learned preferences
- [x] **Inter-agent messaging** — SendHint action via JSON `send_hint` field;
  hints queued in target's pending_hints[], injected into prompt on next cycle

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
