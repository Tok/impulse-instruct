<p align="center">
  <img src="docs/header.svg" alt="Impulse Instruct" width="800"/>
</p>

# Impulse Instruct

A synthesizer with a tiny LLM living inside of it. **PULSE** runs locally and has full control of the sound — it listens to what you say, jams autonomously, and responds to the music in real time. You keep the parameters you want; it owns everything else.

Built in Rust. Supports [Bonsai 8B](https://huggingface.co/prism-ml/Bonsai-8B-gguf) (1-bit, 1.1 GB, fits in 2 GB VRAM) and larger models via any GGUF-compatible llama-server.

> **Requires an NVIDIA GPU (CUDA) for real inference.** The app runs in mock mode without a model — the synth, sequencer, MIDI, and API all work, but responses are keyword-based rather than generated.

---

## What it does

**Bass synthesizer**
- Saw / Square / Supersaw (detuned unison) oscillator; sub-oscillator; white noise source; FM pair
- Moog-style 4-pole ladder filter — LP / HP / BP modes; filter key tracking
- Portamento / glide time; per-step accent and slide; waveshaper (pre-filter tanh saturation)
- Internal overdrive; TB-303 envelope with env mod + decay

**Hoover lead**
- Supersaw → aggressive highpass filter sweep, pitch LFO for the "wailing" character
- Named after the vacuum cleaner drone on Human Resource "Dominator" (1991)

**AN1X-style virtual analog voice** (Boards of Canada / warm VA aesthetic)
- Dual oscillator (Saw, Square/PWM, Triangle, Sine, Noise); OSC2 detune coarse + fine; hard sync; ring mod
- Filter ADSR + amplitude ADSR + pitch envelope; per-voice LFO × 2 with delay and fade-in
- Pitch drift — subtle random LFO for tape-instability character; glide (always or legato)
- Free EG — 8-step drawable envelope (drag bars), runs alongside LFOs

**Drum machines**
- Kit A — kick (808-style with pitch envelope), snare, hihat × 2, toms
- Kit B — kick, snare, hihat × 2, clap, rim (909-style)
- Up to 64-step sequencer; per-voice velocity lanes; swing; euclidean rhythm generator

**Standalone noise voice** — white / pink / brown; volume + color + filter cutoff; LLM-addressable for ambient drones, breath, texture layers

**FX chain** (all LLM-wireable)
- Reverb (Schroeder/Freeverb), delay, chorus/ensemble, phaser (4-stage all-pass)
- Waveshaper (pre-FX tanh saturation), ring modulator (50–500 Hz carrier)
- 3-band EQ: low shelf 200 Hz · mid peak 1 kHz · high shelf 5 kHz (biquad)
- Bitcrush (bit depth + sample rate reduction), tape saturation, master drive
- Master compressor/limiter

**LFO system** — 4 independent slots, each wireable to any parameter (bass cutoff, resonance, pitch, volume, reverb, delay, chorus, kick pitch…); BPM sync; phase resets on transport start

**Intelligence**
- **PULSE** — the AI inside; runs locally, generates JSON parameter updates in real time
- **Jam mode** — on by default; PULSE evolves the pattern autonomously, endlessly
- **Lock system** — touch a knob and it's yours; PULSE won't override it
- **Instruction set** — pre-written JSON templates for common phrases ("make an amen break", "remove claps", "acid bass", "BoC vibes"…)
- **Music theory grounding** — root note + scale injected into system prompt; bass notes snap to scale

**TTS / MC mode** — espeak-ng speaks PULSE's comments as a jungle MC; voice characters (Jungle MC, Rave Announcer, Robot, Smooth DJ); TTS ducks under music; routed through reverb + optional bitcrush

**I/O**
- MIDI in — auto-connects to first USB keyboard; notes trigger bass synth and write into live record; CC knobs map to synth params
- MIDI clock out — 24 PPQN, syncs external hardware and DAWs
- Export — WAV (32-bit float) and MP3 (via ffmpeg)
- Project save/load — JSON snapshots
- HTTP/MCP API — REST interface on port 8765 (`--api` flag)
- Piano display — Huth *Farbige Noten* color theory (1888); C2–C5 keyboard

---

## Requirements

| | |
|---|---|
| **GPU** | NVIDIA GPU with CUDA 12.x (tested: RTX 4070 Ti Super) |
| **VRAM** | ≥ 2 GB for Bonsai 8B; ≥ 7 GB for Qwen3-8B Q4 |
| **OS** | Linux (Windows cross-compile via cargo-xwin) |
| **Rust** | 1.85+ (edition 2024) |
| **TTS** (optional) | `apt install espeak-ng` for MC mode |
| **MP3 export** (optional) | `apt install ffmpeg` |
| **Terminal font** | JetBrains Mono, Fira Code, or any Nerd Font for the graphical banner. Falls back to ASCII automatically when UTF-8 is not detected. |

---

## Quick start

```bash
# 1. Clone
git clone <repo> impulse-instruct && cd impulse-instruct

# 2. Build the Bonsai inference server (one-time, ~3 min)
#    Requires: git cmake build-essential cuda-toolkit-12-x
./build-bonsai-server.sh

# 3. Download a model (requires free HuggingFace account)
./download-models.sh              # Bonsai 8B (~1.1 GB, default, fastest)
./download-models.sh qwen3        # Qwen3-8B Q4_K_M (~5 GB, ~5× better quality)
./download-models.sh qwen3-14b    # Qwen3-14B Q4_K_M (~9 GB, best reasoning)
./download-models.sh gemma4       # Gemma 4 4B Q4 (~3 GB, fast + strong JSON)
./download-models.sh llama31      # Llama 3.1 8B Q4_K_M (~5 GB, excellent JSON)

# 4. Run
cargo run --release               # real LLM inference
cargo run                         # mock mode (no model needed)
```

**No GPU / no model?** The app still runs in mock mode — synth, sequencer, MIDI, and API all work, but PULSE responds with keyword-based presets instead of model-generated output.

MIDI auto-connects on startup. Requires `libasound2-dev` on Linux. Notes trigger the bass synth live and write into the current sequencer step when live record is on. Standard CC knobs map to synth params.

---

## Scripts

| Script | What it does |
|--------|-------------|
| `cargo run` | Build and launch (mock LLM, no API) |
| `cargo run -- --api` | Launch with HTTP/MCP API on port 8765 |
| `cargo run -- --api --port 9000` | API on custom port |
| `cargo run --release` | Release build with real LLM (needs llama-server) |
| `./build-bonsai-server.sh` | Build PrismML llama-server for Bonsai 8B |
| `./download-models.sh [model]` | Download GGUF model (bonsai / qwen3 / qwen3-14b / gemma4 / llama31) |
| `./run-llm-tests.sh` | Run real Bonsai integration test suite |
| `./run-tests.sh --coverage` | Unit tests + HTML coverage report |
| `./build-all.sh` | Build Linux + Windows release binaries into `dist/` |
| `cargo test` | Run 75 unit tests |

---

## Models

| Model | Download | Size | VRAM | Notes |
|-------|----------|------|------|-------|
| **Bonsai-8B** | `./download-models.sh` | ~1.1 GB | ~2 GB | Default; fits any NVIDIA GPU; lowest quality |
| **Qwen3-8B Q4_K_M** | `./download-models.sh qwen3` | ~5 GB | ~7 GB | ~5× better; supports `/think` reasoning mode |
| **Qwen3-14B Q4_K_M** | `./download-models.sh qwen3-14b` | ~9 GB | ~11 GB | Best musical reasoning; needs 12 GB VRAM |
| **Gemma 4 4B Q4_K_M** | `./download-models.sh gemma4` | ~3 GB | ~5 GB | Fast; strong structured JSON output |
| **Llama 3.1 8B Q4_K_M** | `./download-models.sh llama31` | ~5 GB | ~7 GB | Excellent JSON compliance |

All models require a free [HuggingFace](https://huggingface.co/join) account. Switch models at runtime via the model selector in prefs — no restart needed.

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

Start with `--api`:

```bash
cargo run -- --api                # API on :8765
cargo run -- --api --port 9000    # custom port
```

> **Why does a synthesizer have a web server?** Bonsai 8B uses a 1-bit quantisation format (`Q1_0_g128`) that requires PrismML's custom llama.cpp fork. Rather than embedding that C++ library into the Rust binary, Impulse Instruct spawns `llama-server` as a subprocess and talks to it over a local HTTP connection. The same port doubles as an MCP-compatible API so external tools can control the synth.

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
# Ask PULSE
curl -X POST http://localhost:8765/api/prompt \
  -H "Content-Type: application/json" \
  -d '{"prompt": "make it acid"}'

# Set params directly
curl -X POST http://localhost:8765/api/params \
  -H "Content-Type: application/json" \
  -d '{"params": {"bass": {"cutoff": 0.4, "resonance": 0.8}}}'

# Lock a param so LLM cannot touch it
curl -X POST http://localhost:8765/api/lock \
  -H "Content-Type: application/json" \
  -d '{"paths": ["bass.cutoff"]}'
```

---

## Parameters

The full JSON Schema for every parameter is available at runtime:

```bash
curl http://localhost:8765/api/schema
```

Key paths (all floats 0–1 unless noted):

| Path | Description |
|------|-------------|
| `bass.cutoff` | Filter cutoff (0=dark, 1=bright) |
| `bass.resonance` | Resonance / squelch |
| `bass.env_mod` | Filter envelope depth |
| `bass.decay` | Filter envelope decay |
| `bass.accent_level` | Accent boost intensity |
| `bass.waveform` | `"Saw"` / `"Square"` / `"Supersaw"` |
| `bass.filter_mode` | `"Lowpass"` / `"Highpass"` / `"Bandpass"` |
| `bass.supersaw_detune` | Unison spread (semitones) |
| `bass.supersaw_voices` | Unison voice count (2–7) |
| `bass.sub_osc_level` | Sub-oscillator mix |
| `bass.osc_detune` | Oscillator pitch offset −1..+1 semitones |
| `bass.noise_mix` | White noise into filter |
| `bass.portamento_time` | Glide time (0=10ms, 1=500ms) |
| `bass.distortion` | Internal overdrive |
| `sequencer.bpm` | Tempo (40–250 BPM) |
| `sequencer.swing` | Shuffle amount |
| `sequencer.steps` | Active step count (8–64) |
| `sequencer.root_note` | Key root (0=C … 11=B) |
| `sequencer.scale` | `"Major"` / `"NaturalMinor"` / `"Dorian"` / `"Pentatonic"` / … |
| `fx.reverb_mix` / `reverb_size` | Reverb wet/dry, room size |
| `fx.delay_mix` / `delay_feedback` / `delay_time` | Delay |
| `fx.chorus_mix` / `chorus_rate` / `chorus_depth` | Chorus |
| `fx.phaser_mix` / `phaser_rate` / `phaser_depth` | Phaser |
| `fx.bitcrush_mix` / `bitcrush_bits` / `bitcrush_rate` | Bitcrush |
| `fx.eq_low_gain` / `eq_mid_gain` / `eq_hi_gain` | 3-band EQ (−1..+1 → ±12 dB) |
| `fx.master_volume` | Output level |
| `lfo[0..3].rate` / `.depth` / `.target` / `.waveform` | LFO modulation matrix |

Step arrays accept three formats to save tokens:
- Index list: `[0, 4, 8, 12]` — active step indices; all others cleared
- Inline: `[1,0,0,0,1,0,0,0,…]` — 16 values (0/1 or true/false)
- Clear: `[]` — silence all steps

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
│  spawns llama-server subprocess (PrismML fork)   │
│  prompt → HTTP → JSON params → apply_llm_update()│
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

All DSP is pure functions. The audio callback never allocates or locks.

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

**Bonsai 8B** by [prism-ml](https://huggingface.co/prism-ml/Bonsai-8B-gguf) — Apache 2.0

The model is not bundled with the binary. A free HuggingFace account is required to download it.

---

## License

MIT — see LICENSE  
Bonsai 8B model: Apache 2.0 — credit to [prism-ml](https://huggingface.co/prism-ml)
