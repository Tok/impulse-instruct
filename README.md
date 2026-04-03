# Impulse Instruct

An LLM-first audio synthesizer. The model has full control of the synth — you can override it at any time.

Built in Rust. Runs the [Bonsai 8B](https://huggingface.co/prism-ml/Bonsai-8B-gguf) 1-bit GGUF model locally.

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
- **HTTP/MCP API** — connect other LLMs or tools via `--api`

---

## Quick start

```bash
# 1. Clone and enter
git clone <repo> impulse-instruct && cd impulse-instruct

# 2. Run with mock LLM (no model needed, works immediately)
cargo run

# 3. Download real model and run with full LLM inference
#    Requires a free HuggingFace account — https://huggingface.co/join
./download-models.sh          # Linux/macOS (~1.1 GB 1-bit model)
sudo apt install libclang-dev cmake   # needed to compile llama.cpp bindings
cargo run --features llm --release
```

### MIDI keyboard setup (Linux)

The AKAI LPK25 (and most USB MIDI keyboards) work out of the box:

```bash
# Check the device is detected
aconnect -o
# Should show: client N: 'LPK25' [type=kernel]

# Required packages (usually already installed)
sudo apt install libasound2-dev alsa-utils
```

Impulse Instruct auto-connects to the first available MIDI input on startup.
The connected port name is shown in the log and at the bottom-right of the piano display.

---

## Scripts

| Script | What it does |
|--------|-------------|
| `cargo run` | Build and launch (mock LLM) |
| `cargo run -- --api` | Launch with HTTP/MCP API on port 8765 |
| `cargo run --features llm --release` | Real LLM inference |
| `./download-models.sh` | Download Bonsai 8B 1-bit GGUF |
| `./build-all.sh` | Build Linux + Windows release binaries into `dist/` |
| `cargo test` | Run 25 unit tests |

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

Start with `--api` to expose a REST interface on port 8765.

```bash
cargo run -- --api
cargo run -- --api --port 9000
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
│  LLM Thread (blocking inference)                 │
│  prompt → JSON params → apply_llm_update()       │
└─────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────┐
│  MIDI Thread (midir + ALSA)                      │
│  NoteOn/Off → pressed_notes + DSP trigger        │
└─────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────┐
│  HTTP Thread (tokio, optional --api)             │
│  REST endpoints → read/write AppState            │
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
