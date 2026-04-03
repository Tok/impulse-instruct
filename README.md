# Impulse Instruct

An LLM-first audio synthesizer. The model has full control of the synth — you can override it at any time.

Built in Rust. Runs the [Bonsai 8B](https://huggingface.co/prism-ml/Bonsai-8B-gguf) 1-bit GGUF model locally.

---

## What it does

- **TB-303** bass synthesizer — ladder filter, saw/square oscillator, accent & slide
- **TR-808** drum machine — kick, snare, hihat, toms (analog-modeled)
- **TR-909** drum machine — kick, snare, hihat, clap, rim
- **16-step sequencer** — sample-accurate clock, per-voice patterns
- **FX chain** — reverb, delay, drive, master volume
- **MIDI I/O** — CC-to-param mapping, note input
- **LLM control** — model continuously generates JSON parameter updates
- **Lock system** — touch a knob and it's yours; the LLM won't override it
- **Jam mode** — LLM evolves the pattern autonomously
- **HTTP/MCP API** — connect other LLMs or tools via `--api`

---

## Quick start

```bash
# 1. Clone and enter
git clone <repo> impulse-instruct && cd impulse-instruct

# 2. Run with mock LLM (no model needed)
./start.sh

# 3. Download real model and run with full LLM inference
./download-models.sh
sudo apt install libclang-dev cmake   # needed to compile llama.cpp bindings
./start.sh --llm
```

---

## Scripts

| Script | What it does |
|--------|-------------|
| `./start.sh` | Build and launch (mock LLM by default) |
| `./start.sh --llm` | Launch with real LLM inference |
| `./start.sh --api` | Enable HTTP/MCP API on port 8765 |
| `./start.sh --dev` | Debug build + verbose logging |
| `./download-models.sh` | Download Bonsai 8B Q4_K_M (~5 GB) |
| `./download-models.sh Q3_K_M` | Smaller quantization (~4 GB) |
| `./download-models.sh --list` | List available quantizations |
| `./build-all.sh` | Build Linux + Windows release binaries into `dist/` |
| `./run-tests.sh` | Run 13 unit tests |
| `./run-tests.sh --coverage` | Tests + HTML coverage report |
| `./run-tests.sh --watch` | Re-run on file changes |

---

## HTTP / MCP API

Start with `--api` to expose a REST interface on port 8765. The API link appears in the top-right corner of the UI.

```bash
./start.sh --api
./start.sh --api --port 9000   # custom port
```

### Endpoints

```
GET  /api/state                  Full synth state as JSON
GET  /api/schema                 JSON Schema for all parameters
POST /api/prompt                 Send a prompt to the LLM
POST /api/params                 Directly set parameters
POST /api/lock                   Lock params from LLM override
POST /api/unlock                 Unlock params
POST /api/sequencer/play         Start sequencer
POST /api/sequencer/stop         Stop sequencer
```

### Examples

```bash
# Ask the LLM
curl -X POST http://localhost:8765/api/prompt \
  -H "Content-Type: application/json" \
  -d '{"prompt": "make it acid"}'

# Set params directly
curl -X POST http://localhost:8765/api/params \
  -H "Content-Type: application/json" \
  -d '{"params": {"tb303": {"cutoff": 0.4, "resonance": 0.8}}}'

# Lock a param so LLM can't touch it
curl -X POST http://localhost:8765/api/lock \
  -H "Content-Type: application/json" \
  -d '{"paths": ["tb303.cutoff"]}'

# Get full schema (useful for MCP tool definitions)
curl http://localhost:8765/api/schema
```

---

## Parameters

All floats 0.0–1.0 unless noted.

| Path | Description |
|------|-------------|
| `tb303.cutoff` | Filter cutoff (0=dark, 1=bright) |
| `tb303.resonance` | Resonance / squelch (high = acid) |
| `tb303.env_mod` | Filter envelope depth |
| `tb303.decay` | Filter envelope decay |
| `tb303.accent_level` | Accent boost |
| `tb303.waveform` | `"Saw"` or `"Square"` |
| `tb303.distortion` | Internal overdrive |
| `sequencer.bpm` | Tempo (40–250 BPM) |
| `fx.reverb_size` | Room size |
| `fx.reverb_mix` | Reverb wet/dry |
| `fx.delay_time` | Delay time (0–1000ms) |
| `fx.delay_feedback` | Delay repeats |
| `fx.delay_mix` | Delay wet/dry |
| `fx.distortion_drive` | Master bus drive |
| `fx.master_volume` | Output level |

---

## Windows build

```bash
# On Linux host — cross-compile for Windows
sudo apt install clang lld cmake ninja-build
cargo install cargo-xwin
./build-all.sh
# → dist/impulse-instruct-windows-x86_64.exe
```

---

## Model

**Bonsai 8B** by [prism-ml](https://huggingface.co/prism-ml/Bonsai-8B-gguf)  
License: Apache 2.0

The model is not bundled with the binary. Download it with `./download-models.sh`.

---

## Architecture

```
┌─────────────────────────────────────────────────┐
│  UI Thread (egui)                                │
│  reads/writes AppState, pushes AudioParams       │
└──────────────┬──────────────────────────────────┘
               │ rtrb ring buffer (lock-free)
               ▼
┌─────────────────────────────────────────────────┐
│  Audio Thread (cpal, real-time)                  │
│  sequencer clock → triggers → DSP → output       │
└─────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────┐
│  LLM Thread (blocking inference)                 │
│  prompt → JSON params → apply_llm_update()       │
└─────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────┐
│  HTTP Thread (tokio, optional --api)             │
│  REST endpoints → read/write AppState            │
└─────────────────────────────────────────────────┘
```

All DSP is pure functions. The audio callback never allocates or locks.

---

## License

MIT — see LICENSE  
Bonsai 8B model: Apache 2.0 — credit to [prism-ml](https://huggingface.co/prism-ml)
