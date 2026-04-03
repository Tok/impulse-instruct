# Impulse Instruct

A synthesizer with a tiny LLM living inside it. Bonsai 8B runs locally and has full control of the sound — it listens to what you say, jams autonomously, and responds to the music in real time. You keep the keys you want; it owns everything else.

Built in Rust. Runs [Bonsai 8B](https://huggingface.co/prism-ml/Bonsai-8B-gguf) — a 1-bit, 1.1 GB model — entirely on your GPU.

> **Requires an NVIDIA GPU (CUDA).** CPU inference is ~0.4 tok/s — not usable in practice. The app falls back to mock mode without a working llama-server.

---

## What it does

- **Bass synthesizer** — ladder filter, saw/square oscillator, accent & slide
- **Drum Kit A** — kick, snare, hihat, toms (808-style analog modeling)
- **Drum Kit B** — kick, snare, hihat, clap, rim (909-style)
- **16-step sequencer** — sample-accurate clock, per-voice patterns
- **FX chain** — reverb, delay, drive, master volume
- **Piano display** — Huth *Farbige Noten* color theory (1888); C2–C5 keyboard lights up on MIDI input and sequencer playback
- **MIDI input** — auto-connects to first USB MIDI keyboard (class-compliant, tested with AKAI LPK25); live play triggers the bass synth
- **LLM control** — model continuously generates JSON parameter updates
- **Lock system** — touch a knob and it's yours; the LLM won't override it
- **Jam mode** — LLM evolves the pattern autonomously
- **Export** — WAV (32-bit float) and MP3 (via ffmpeg); offline render, no audio device needed
- **Project save/load** — JSON snapshots via File menu
- **HTTP/MCP API** — REST interface on port 8765 (on by default; `--no-api` to disable)

---

## Requirements

| | |
|---|---|
| **GPU** | NVIDIA GPU with CUDA 12.x (tested: RTX 4070 Ti Super) |
| **VRAM** | ≥ 2 GB (model is 1.1 GB, fits entirely in VRAM) |
| **OS** | Linux (Windows build possible via cargo-xwin, untested) |
| **Rust** | 1.85+ (edition 2024) |

## Quick start

```bash
# 1. Clone and enter
git clone <repo> impulse-instruct && cd impulse-instruct

# 2. Build the Bonsai inference server (one-time, ~3 min)
#    Requires: git cmake build-essential cuda-toolkit-12-x
./build-bonsai-server.sh

# 3. Download Bonsai 8B (~1.1 GB, requires free HuggingFace account)
./download-models.sh

# 4. Run
cargo run --release
```

**No GPU / no model?** The app still runs in mock mode — the synth, sequencer, MIDI, and API all work, but responses are keyword-based rather than model-generated.

MIDI (tested with AKAI LPK25, any class-compliant USB keyboard works) auto-connects on startup. Requires `libasound2-dev` on Linux. Notes trigger the bass synth live and write into the current sequencer step. Standard CC knobs map to synth params (filter, resonance, env mod, decay, FX). MIDI Start/Stop control the sequencer transport.

---

## Scripts

| Script | What it does |
|--------|-------------|
| `cargo run` | Build and launch (mock LLM, API on port 8765) |
| `cargo run -- --no-api` | Launch without HTTP/MCP API |
| `cargo run --release` | Release build with real LLM (needs llama-server) |
| `./build-bonsai-server.sh` | Build PrismML llama-server for Bonsai 8B support |
| `./download-models.sh` | Download Bonsai 8B 1-bit GGUF |
| `./run-llm-tests.sh` | Run real Bonsai integration test suite |
| `./build-all.sh` | Build Linux + Windows release binaries into `dist/` |
| `cargo test` | Run unit tests |

---

## Farbige Noten — Color Theory

The piano display uses Ch. A. B. Huth's *Farbige Noten* (Hamburg 1888–1889), a 12-color system where each chromatic semitone maps counter-clockwise around the RYB color wheel starting from Blue at C.

| Note | Color | RYB |
|------|-------|-----|
| C   | Blue         | 240° |
| C#  | Cyan-Blue    | 210° |
| D   | Green/Teal   | 180° |
| D#  | Yellow-Green | 150° |
| E   | Yellow       | 120° |
| F   | Orange       |  60° |
| F#  | Vermilion    |  30° |
| G   | Rose         | 350° |
| G#  | Carmine      | 320° |
| A   | Lilac-Violet | 290° |
| A#  | Purple       | 265° |
| B   | Indigo       | 245° |

Complementary colors (directly opposite on wheel) correspond to tritone intervals — e.g. Blue C ↔ Orange F#. See `docs/colorful-notes.md` for the full theory.

---

## HTTP / MCP API

The REST interface starts automatically on port 8765. Use `--no-api` to disable it, or `--port` to change the port.

> **Why does a synthesizer start a web server?** Bonsai 8B uses a 1-bit quantisation format (`Q1_0_g128`) that requires PrismML's custom llama.cpp fork to run. Rather than embedding that C++ library directly into the Rust binary — which would couple us tightly to a moving fork and complicate builds — Impulse Instruct spawns `llama-server` as a subprocess and talks to it over a local HTTP connection. The same port also doubles as an MCP-compatible API so other tools can connect to the synth.

```bash
cargo run                         # API on :8765
cargo run -- --port 9000          # API on :9000
cargo run -- --no-api             # no API
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
  -d '{"params": {"bass": {"cutoff": 0.4, "resonance": 0.8}}}'

# Lock a param so LLM can not touch it
curl -X POST http://localhost:8765/api/lock \
  -H "Content-Type: application/json" \
  -d '{"paths": ["bass.cutoff"]}'
```

---

## Parameters

All floats 0.0–1.0 unless noted.

| Path | Description |
|------|-------------|
| `bass.cutoff` | Filter cutoff (0=dark, 1=bright) |
| `bass.resonance` | Resonance / squelch (high = acid) |
| `bass.env_mod` | Filter envelope depth |
| `bass.decay` | Filter envelope decay |
| `bass.accent_level` | Accent boost |
| `bass.waveform` | `"Saw"` or `"Square"` |
| `bass.distortion` | Internal overdrive |
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
# dist/impulse-instruct-windows-x86_64.exe
```

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
│  LLM Thread                                      │
│  spawns llama-server subprocess (PrismML fork)   │
│  prompt → HTTP → JSON params → apply_llm_update()│
└─────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────┐
│  MIDI Thread (midir + ALSA)                      │
│  NoteOn/Off → pressed_notes + DSP trigger        │
└─────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────┐
│  HTTP Thread (tokio, default port 8765)          │
│  REST endpoints → read/write AppState            │
│  disabled with --no-api                          │
└─────────────────────────────────────────────────┘
```

All DSP is pure functions. The audio callback never allocates or locks.

---

## Model

**Bonsai 8B** by [prism-ml](https://huggingface.co/prism-ml/Bonsai-8B-gguf) — Apache 2.0

The model is not bundled with the binary. A free **HuggingFace account** is required to download it.

```bash
./download-models.sh   # 1-bit GGUF, ~1.1 GB
```

---

## License

MIT — see LICENSE  
Bonsai 8B model: Apache 2.0 — credit to [prism-ml](https://huggingface.co/prism-ml)
