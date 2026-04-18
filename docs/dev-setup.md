# Impulse Instruct - Developer Setup

Build instructions, architecture notes, HTTP API reference, and contributor setup.

---

## Requirements

| | |
|---|---|
| **GPU** | NVIDIA GPU with CUDA 12.x (tested: RTX 4070 Ti Super) |
| **VRAM** | ≥ 6 GB for Gemma 4 E4B |
| **OS** | Linux (Ubuntu 22.04+); Windows cross-compile via cargo-xwin |
| **Rust** | 1.85+ (edition 2024) |
| **C++ toolchain** | `build-essential cmake ninja-build` for llama-server builds |
| **TTS** (optional) | `apt install espeak-ng` for MC mode |
| **MP3 export** (optional) | `apt install ffmpeg` |
| **Terminal font** | JetBrains Mono, Fira Code, or any Nerd Font for the graphical banner |

---

## Quick start

```bash
# 1. Clone
git clone <repo> impulse-instruct && cd impulse-instruct

# 2. Build the inference server (one-time, ~3–5 min)
./scripts/build-llama-server.sh      # standard llama.cpp

# 3. Download a model (requires free HuggingFace account)
./scripts/download-models.sh         # Gemma 4 E4B (~4.6 GB, recommended)

# 4. Run
cargo run --release                  # real LLM inference
cargo run                            # mock mode (no model needed)
cargo run -- --api                   # with HTTP/MCP API on :8765
cargo run -- --log debug             # verbose logging
```

---

## Scripts

| Script | What it does |
|--------|-------------|
| `cargo run` | Build and launch (mock LLM, no API) |
| `cargo run -- --api` | Launch with HTTP/MCP API on port 8765 |
| `cargo run --release` | Release build with real LLM inference |
| `./start.sh` | Build + launch (release, mock LLM) |
| `./start.sh --dev` | Build + launch (debug + verbose) |
| `./scripts/build-llama-server.sh` | Build standard llama.cpp server |
| `./scripts/download-models.sh [model]` | Download GGUF model |
| `./scripts/run-tests.sh --coverage` | Unit tests + HTML coverage report |
| `./scripts/run-llm-tests.sh` | All LLM integration suites (needs running model) |
| `./scripts/run-llm-style.sh` | Artist/genre reference tests only |
| `./scripts/run-llm-theory.sh` | Music theory + producer lingo tests only |
| `./scripts/build-all.sh` | Linux + Windows EXE → `dist/` |

Windows `.bat` equivalents mirror every `.sh` script.

---

## Architecture

```
┌─────────────────────────────────────────────────┐
│  UI Thread (egui)                                │
│  reads/writes AppState via Arc<RwLock<>>         │
│  pushes AudioParams + AudioCommands via rtrb     │
└──────────────┬──────────────────────────────────┘
               │ rtrb ring buffer (lock-free)
               ▼
┌─────────────────────────────────────────────────┐
│  Audio Thread (cpal, real-time)                  │
│  sequencer clock → triggers → DSP → output       │
│  writes MIDI clock bytes to rtrb                 │
└─────────────────────────────────────────────────┘
               │ rtrb ring buffer (u8 bytes)
               ▼
┌─────────────────────────────────────────────────┐
│  MIDI Clock Out Thread (midir)                   │
│  drains rtrb → sends 0xF8/0xFA/0xFC bytes        │
└─────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────┐
│  LLM Thread                                      │
│  spawns llama-server subprocess                  │
│  prompt → HTTP → JSON params → apply_llm_update()│
│  SamplingParams (top_k, top_p, min_p, …)         │
└─────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────┐
│  MIDI In Thread (midir + ALSA)                   │
│  NoteOn/Off → bass synth trigger + live record   │
│  CC → synth params                               │
└─────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────┐
│  HTTP Thread (tokio, port 8765)                  │
│  REST + MCP endpoints → read/write AppState      │
│  only started with --api flag                    │
└─────────────────────────────────────────────────┘
```

All DSP is pure functions. The audio callback never allocates or locks. See `CLAUDE.md` for the full coding-style guide and invariants.

### Key source files

| Path | Purpose |
|------|---------|
| `src/state/mod.rs` | Single `AppState` struct; all state transitions are pure functions |
| `src/audio/dsp.rs` | All DSP: 303 ladder filter, 808/909 voices, reverb, delay |
| `src/audio/mod.rs` | cpal stream + rtrb ring buffer |
| `src/sequencer/mod.rs` | 16-step clock as a pure function |
| `src/llm/mod.rs` | LLM inference thread; `SamplingParams`; mock mode |
| `src/llm/prompt.rs` | System prompt builder + JSON schema |
| `src/api/mod.rs` | axum HTTP/MCP API |
| `src/ui/mod.rs` | egui app shell |
| `src/ui/panels/` | One file per synth panel |

---

## HTTP / MCP API

Start with `--api`:

```bash
cargo run -- --api                # API on :8765
cargo run -- --api --port 9000    # custom port
```

### Endpoints

```
GET  /api/state          Full AppState as JSON
GET  /api/schema         JSON Schema for all parameters
POST /api/prompt         { "prompt": "make it acid" }
POST /api/params         { "params": { "tb303": { "cutoff": 0.4 } } }
POST /api/lock           { "paths": ["tb303.cutoff"] }
POST /api/unlock         { "paths": ["tb303.cutoff"] }
POST /api/sequencer/play
POST /api/sequencer/stop
```

### Examples

```bash
curl -X POST http://localhost:8765/api/prompt \
  -H "Content-Type: application/json" \
  -d '{"prompt": "make it acid"}'

curl -X POST http://localhost:8765/api/params \
  -H "Content-Type: application/json" \
  -d '{"params": {"bass": {"cutoff": 0.4, "resonance": 0.8}}}'

curl -X POST http://localhost:8765/api/lock \
  -H "Content-Type: application/json" \
  -d '{"paths": ["bass.cutoff"]}'
```

---

## Parameter reference

The full JSON Schema is available at runtime via `GET /api/schema`.

Key paths (all floats 0–1 unless noted):

| Path | Description |
|------|-------------|
| `bass.cutoff` | Filter cutoff (0=dark, 1=bright) |
| `bass.resonance` | Resonance / squelch |
| `bass.env_mod` | Filter envelope depth |
| `bass.decay` | Filter envelope decay |
| `bass.waveform` | `"Saw"` / `"Square"` / `"Supersaw"` |
| `bass.filter_mode` | `"Lowpass"` / `"Highpass"` / `"Bandpass"` |
| `sequencer.bpm` | Tempo (40–250 BPM) |
| `sequencer.swing` | Shuffle amount |
| `sequencer.root_note` | Key root (0=C … 11=B) |
| `sequencer.scale` | `"Major"` / `"NaturalMinor"` / `"Dorian"` / `"Pentatonic"` / … |
| `fx.reverb_mix` / `reverb_size` | Reverb wet/dry, room size |
| `fx.delay_mix` / `delay_feedback` / `delay_time` | Delay |
| `fx.bitcrush_mix` / `bitcrush_bits` / `bitcrush_rate` | Bitcrush |
| `fx.eq_low_gain` / `eq_mid_gain` / `eq_hi_gain` | 3-band EQ (−1..+1 → ±12 dB) |
| `lfo[0..3].rate` / `.depth` / `.target` / `.waveform` | LFO modulation matrix |

Step arrays accept three formats:
- Index list: `[0, 4, 8, 12]` - active step indices; all others cleared
- Inline: `[1,0,0,0,1,0,0,0,…]` - full 16-value boolean array
- Clear: `[]` - silence all steps

---

## Windows build

```bash
# On Linux host - cross-compile for Windows
sudo apt install clang lld cmake ninja-build
cargo install cargo-xwin
./scripts/build-all.sh
# → dist/impulse-instruct-windows-x86_64.exe
```

**Ubuntu LLVM symlink note:** Ubuntu installs LLVM tools with version suffixes
(`llvm-lib-18`, `llvm-dlltool-18`, etc.) but does not create plain-name aliases.
`cc-rs` (used by the `ring` crate) looks for `llvm-lib` without a suffix and will
fail with `failed to find tool "llvm-lib"`. Create the aliases once:

```bash
sudo ln -sf /usr/bin/llvm-lib-18     /usr/local/bin/llvm-lib
sudo ln -sf /usr/bin/llvm-dlltool-18 /usr/local/bin/llvm-dlltool
sudo ln -sf /usr/bin/llvm-ar-18      /usr/local/bin/llvm-ar
```

`build-all.sh` attempts this automatically and prints the exact commands if it
can't (e.g. no sudo in CI). Adjust the version suffix to match whatever
`ls /usr/bin/llvm-lib-*` returns on your machine.

---

## Models

| Model | Download | Size | VRAM | Notes |
|-------|----------|------|------|-------|
| **Gemma 4 E4B Q4_K_M** | `./scripts/download-models.sh` | ~4.6 GB | ~6 GB | Default — mobile-targeted Gemma 4, runs on any 6 GB GPU |
| **Gemma 4 26B-A4B UD-IQ4_XS** | `./scripts/download-models.sh gemma-26b` | ~13.4 GB | ~14 GB | MoE (4B active) — E4B speed, 26B knowledge; needs 16 GB |
| **Gemma 4 26B-A4B UD-Q3_K_M** | `./scripts/download-models.sh gemma-26b-q3` | ~12.5 GB | ~13 GB | Smaller quant of the MoE |
| **Gemma 4 26B-A4B UD-IQ2_XXS** | `./scripts/download-models.sh gemma-26b-iq2` | ~9.9 GB | ~10 GB | Smallest 26B-A4B quant |

All models use `.llama-official-build/bin/llama-server` (standard llama.cpp), built via `./scripts/build-llama-server.sh`.  The 26B-A4B GGUF files contain only the text transformer; the vision adapter is a separate `mmproj` file we do not download, so inference is text-only.

---

## Crate versions (locked)

| Crate | Version | Purpose |
|-------|---------|---------|
| egui / eframe | 0.28 | UI |
| cpal | 0.15 | Audio I/O |
| axum | 0.7 | HTTP API |
| midir | 0.9 | MIDI |
| rtrb | 0.3 | Lock-free audio ring buffer |
| parking_lot | latest | RwLock for AppState |
| crossbeam-channel | latest | Thread communication |
