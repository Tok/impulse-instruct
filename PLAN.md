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

### Recently completed (develop, not yet merged to main)

- [x] **Separate LLM heat/temperature sliders** — per-agent heat + temp controls
- [x] **Even control spacing** — all panels use `even_group_width()` / `glass_group_fill()`
- [x] **Reason-style rack flip** — Tab flips front (knobs) ↔ back (ports + Bezier cables)
- [x] **Rackable LLM agents (Phase 1–3)** — `ModuleKind::LlmAgent`, per-agent state,
  round-robin scheduling, scoped prompts, scoped `apply_llm_update`, interactive
  scope editor (checkbox grid), editable persona/heat/temp/bars on the card
- [x] **Header simplified** — LLM status/heat/temp removed (now per-agent in rack)
- [x] **DSP load meter** — sparkline in footer
- [x] **Phosphor persistence** — oscilloscope waveform decay trail
- [x] **Smooth style transitions** — `schedule_baseline_ramps()` via ParamRamp
- [x] **WASD as arrow keys** — setting in Preferences → Controls → Keyboard
- [x] **Knob arrow-key control** — left/right cursors adjust hovered knobs/sliders
- [x] **25 new tests** — json_repair, split_thinking, extract_llm_actions, baseline ramps
- [x] **Refactored** — `extract_llm_actions()` pure function, `AudioChannels` struct,
  `scope_footer.rs` extracted, cable overlay extracted to `rack_cables.rs`
- [x] **Multi-model server pool (Phase 1)** — `LlamaServerPool` manages N
  llama-server processes on ports 8766+, ref-counted per model, per-agent
  `model_path: Option<String>` with dropdown on agent card, 10 new tests

---

### Next sprint — Multi-model agent infrastructure (Phase 2–3)

Phase 1 (server pool + per-agent model) is done.  Remaining:

#### VRAM budget + startup wizard (Phase 2)

- **VRAM budget** — detect available GPU memory (nvidia-smi / rocm-smi),
  compute how many models fit.  Typical configs:
  - 8 GB VRAM → 1× Gemma 4 E4B (~6 GB)
  - 12 GB → 1× Gemma + 2× Bonsai (~6+2+2 = 10 GB)
  - 16 GB → 2× Gemma or 1× Gemma + 4× Bonsai
  - 24 GB → 2× Gemma + 4× Bonsai or 1× 14B + 2× Bonsai
- **Startup wizard** — on first launch (or when no agents exist), show a
  modal: "How many agents? Which models?" with VRAM indicator.  Presets:
  "Solo (1× Gemma)", "Duo (2× Gemma)", "Swarm (1× Gemma + 3× Bonsai)".

#### Dynamic agent spawning (Phase 3)

- Agents can request more agents via the `settings` JSON key:
  `{ "settings": { "spawn_agent": { "persona": "BASS BRAIN", "scope": ["bass"], "model": "bonsai" } } }`
- The UI creates a new `LlmAgent` rack module + `LlmAgentState` from this.
- Agents can also dismiss themselves: `{ "settings": { "dismiss": true } }`
- MC/DJ mode should be a separate agent instance with its own persona.

#### Cable-driven scope (Phase 3 stretch)

- Drawing a CV cable from LlmAgent to AcidBass auto-adds "bass" to scope
- Removing the cable removes from scope
- Visual: cable colours encode scope (blue = LLM signal)

---

### Post-release backlog

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
