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

### Next sprint — v0.6.0 queue

- [x] **Separate LLM heat slider** — `llm.temperature: f32` (0–2, default 0.9)
  added to `LlmState`; sent directly to llama-server; `llm.heat` remains the
  mutation-rate / top_p-widening control.  TEMP DragValue now appears in the
  LLM strip header.

- [x] **Even control spacing — extend to remaining panels** — all panels
  (drums Kit A/B, FX) already use `even_group_width()` / `glass_group_fill()`.
  Item was completed in a prior sprint.

---

### Post-release backlog

#### Rack & modular UI

- [ ] **Reason-style rack flip + modular cable UI** — Tab flips the view to a
  back panel.  Each module exposes labelled I/O ports (audio, CV, trigger,
  LLM-signal — see below).  Bezier cables drag between ports.  The cable graph
  already exists (`src/state/rack.rs`); the remaining work is:
  - Back-panel layout pass: position ports relative to module bounding boxes
  - Bezier cable renderer (egui `Painter` + cubic Bezier)
  - Drag-to-patch mouse interaction (port hover → highlight compatible ports,
    drag → preview cable, drop → `rack.connect()`)
  - Cable colours by signal type (audio = white, CV = amber, trigger = cyan,
    LLM = soft blue)

#### Multiple LLM instances (rackable LLM module)

The current LLM is a singleton fixed in the header.  The goal is to make it
a **rackable module** so multiple instances can run in parallel, each with
its own prompt focus and its own set of wired targets.

Design sketch (to be detailed with sequential-thinking MCP before
implementation):

- **`LlmModule` as a rack module kind** (`ModuleKind::LlmAgent`) — same
  cable graph, new port type `PortKind::LlmSignal`.
- Each `LlmModule` has:
  - A **scope** field: which instrument keys it is allowed to write
    (e.g. `["bass", "kit_a"]` or `["fx", "lfo[0]"]`).  Locked params still
    respected.
  - Its own **prompt context** / persona string (e.g. "you control only the
    bass line").
  - Its own **temperature** and **heat** knobs.
  - Its own **jam cycle** cadence (can differ from the global beat).
- **LLM-signal cables** connect an `LlmModule` output to the parameter-input
  port of another module.  The cable encodes which JSON key subtree the LLM
  output is routed into.  This replaces the current single `apply_llm_update`
  fan-out with a per-instance scoped apply.
- **Inference threading**: each `LlmModule` gets its own channel pair to the
  LLM worker thread (or a pool of workers if multiple models are loaded).
  The existing mock/real inference path is reused; scheduling is round-robin
  or priority-weighted.
- **UI**: the front panel shows a compact "PULSE mini" card per instance
  (persona name, heat, last output snippet).  The back panel shows LLM-signal
  ports alongside audio/CV ports.

This requires fleshing out with sequential-thinking MCP before coding.  Key
questions: single shared model vs. per-instance model weights, cable conflict
resolution (two LLMs targeting same param), UI real-estate for many instances.

#### Visualization & statistics modules

Current oscilloscope and note-colour log are a start.  Planned additions:

- [ ] **Spectrum analyser** — real-time FFT magnitude display (log-frequency
  x-axis, dB y-axis) as a rackable module.  Data fed from the existing
  capture ring buffer.
- [ ] **Stereo correlation meter** — phase correlation + L/R balance bar,
  rackable or always-on in the FX panel.
- [ ] **Pattern heatmap** — step-grid overlay showing how often each step fires
  (running average), useful for spotting probability drift.
- [ ] **LLM activity timeline** — scrolling log of which module fired, what
  param it changed, and by how much.  Replaces the flat text log with a
  structured, filterable view.
- [ ] **CPU / DSP load meter** — audio callback duration as a sparkline.

#### Visual treatment (post-process pass)

Bloom alone is not enough — a full post-process layer is needed to make the
UI feel alive at performance brightness:

- [ ] **Bloom** — Gaussian blur on bright (>threshold) pixels, additively
  blended back.  Needs a custom `wgpu` render pass outside egui's painter.
  GPU cost: one downsample + two separable blur passes.  Evaluate at 1080p
  on integrated GPU before committing.
- [ ] **Scan-line / CRT vignette** — subtle horizontal scan-line overlay and
  radial darkening at edges.  Can be a cheap fullscreen quad shader pass.
- [ ] **LED glow on active steps** — per-step additive glow ring around
  active step buttons.  Can be approximated in egui with layered circles if
  custom wgpu pass is too expensive.
- [ ] **Phosphor persistence on oscilloscope** — decay buffer so the waveform
  fades out rather than clearing each frame.

All of the above share the same custom render pass infrastructure.  Plan the
wgpu integration once and implement all effects in that pass.

#### Other

- [ ] **Multiple voices (per-voice DSP params)** — voices 1-3 currently share
  synth params (cutoff/resonance/etc.) with voice 0.  Next step: per-voice
  `AudioParams` snapshot so each voice can have independent timbre.

- [ ] **Gabber kick voice** — pitch-envelope ramp + hard clipper.  No existing
  voice fits; needs a new `GabberKick` struct in `src/audio/dsp/voices.rs`.

- [ ] **Windows code-signing** — unsigned `.exe` triggers SmartScreen.
  Requires EV certificate.  Low priority until meaningful Windows user base.

- [ ] **Smooth style transitions** — when the user changes style in the dropdown,
  parameters should ramp/lerp to new values instead of jumping instantly.  The
  `ParamRamp` / `active_ramps` infrastructure already exists; extend it to
  cover style preset application.  This applies to LLM-driven style changes
  (`settings.style`) as well as user-initiated dropdown changes.

- [ ] **Bipolar param_control variant** — `param_control` only handles 0–1
  normalised values.  Controls like `bass.osc_detune` (-1..+1 semitones) bypass
  the lock/focus system because they don't fit.  Add a bipolar mode to
  `param_control` (or a new `param_control_bipolar`) so all synth knobs support
  lock/focus/free toggle uniformly.

- [ ] **Event queue ring visualisation** — render the rtrb audio command ring
  buffer as a circular display with moving read/write heads, showing fill level
  and throughput.  Could be a rackable module or an always-on diagnostic in the
  footer/header.

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
