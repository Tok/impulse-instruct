# Impulse Instruct — Roadmap

A synthesizer with a tiny LLM living inside it that actually understands music.
PULSE listens, jams, evolves, and shouts at the crowd.

---

## What's Built

### Core synth
- **Bass synth** — saw/square/supersaw oscillator, 4-pole Moog ladder filter (LP/HP/BP), sub-osc, noise, FM pair, portamento, waveshaper, overdrive, per-step accent + slide
- **Hoover lead** — supersaw → aggressive highpass sweep, pitch LFO, dedicated voice in UI
- **AN1X-style VA voice** — dual OSC (saw/square/tri/sin/noise), OSC2 coarse+fine detune, hard sync, ring mod, sub-osc, 3 filter modes, ADSR × 2, pitch envelope, per-voice LFO × 2 with delay/fade, pitch drift, free EG (8-step drawable envelope)
- **Drum machines** — Kit A (808-style: kick with pitch envelope, snare, hihat × 2, toms) + Kit B (909-style: kick, snare, hihat × 2, clap, rim)
- **Standalone noise voice** — white / pink / brown, volume + color + cutoff, LLM-addressable
- **LFO matrix** — 4 independent slots, any waveform, wireable to any parameter, BPM sync, phase reset on transport start

### Sequencer
- 16-step base, variable step count per pattern (8 / 16 / 32 / 64), swing
- Per-voice step counts for polyrhythm (kick 16, hihat 12, bass 7…)
- Per-step: velocity, probability (0–100%), ratchet (1–4×), accent, slide
- Euclidean rhythm generator
- Pattern bank (8 slots), chain playback (up to 8 patterns in sequence)
- Live record — MIDI keyboard writes directly into steps
- Time signature selector (4/4, 3/4, 5/4, 6/8, 7/8…)
- Mute/solo per row, pattern copy/paste

### FX chain
- Reverb, delay, chorus/ensemble, phaser (4-stage all-pass), ring modulator
- Waveshaper (pre-FX tanh), bitcrush (bit depth + sample rate), EQ (3-band biquad)
- Master compressor/limiter, tape saturation, drive

### Intelligence
- LLM runs locally via llama-server subprocess (PrismML CUDA fork for Bonsai 1-bit)
- Mock mode when no model present — keyword-based, synth still fully functional
- Jam mode — PULSE evolves the pattern autonomously
- Lock system — touch a knob to claim it; LLM won't override it
- Compact step arrays: index list `[0,4,8,12]` or inline `[1,0,0,0,…]` or clear `[]`
- Music theory grounding — root note + scale in system prompt, scale-snap on bass notes
- Instruction set — pre-written JSON templates for common phrases ("make an amen break", "remove claps", etc.)
- LFO dot-notation sanitization — handles malformed LLM output gracefully

### TTS / MC mode
- espeak-ng backend — speaks PULSE's `mc_line` field as a jungle MC
- Per-character pitch/speed ± 10% — avoids robotic monotone
- TTS settings: pitch, speed, amplitude sliders
- MC voice characters: Jungle MC, Rave Announcer, Robot, Smooth DJ
- TTS ducks under music via volume envelope
- Output routed through reverb + optional bitcrush

### I/O
- MIDI in — NoteOn/Off → bass synth + live record; CC → synth params; Start/Stop → transport
- MIDI clock out — 24 PPQN, sent on dedicated thread via rtrb ring buffer (alloc-free audio path)
- WAV export (32-bit float), MP3 export (ffmpeg)
- HTTP/MCP REST API on port 8765 (`--api` flag)
- Project save/load — JSON snapshots

### UI
- 5 panels: Sequencer / Bass (303) / 808 / 909 / FX; AN1X and Hoover in sequencer area
- Chrome knobs, glass sliders, embossed buttons (neumorphic grayscale)
- Velocity lanes below each step row (drag bars)
- XY pads (CUT×RES, ENV×DEC, REVERB mix×size, DELAY mix×feedback, 808 PITCH×DECAY)
- Oscilloscope strip (rolling 512-sample waveform)
- ADSR envelope visualizer (interactive — drag zones)
- Piano display — Huth *Farbige Noten* (1888) 12-color theory, C2–C5
- Model selector — scan `models/`, hot-swap without restart
- Reasoning toggle (Qwen3 `/think` mode)
- AI persona name — editable, used in system prompt

### Testing & build
- 75 unit tests across 4 submodules (`seq_tests`, `state_tests`, `llm_tests`, split at 1000-line limit)
- Pre-commit hook: fmt + clippy + tests + 1000-line LOC limit
- `run-tests.sh --coverage` → HTML coverage report (lcov)
- Cross-compile to Windows EXE via `cargo-xwin` + `build-all.sh`
- `download-models.sh` — Bonsai 8B (default), Qwen3-8B, Qwen3-14B, Gemma 4, Llama 3.1

---

## What's Left

Ordered by value — tackle roughly from top to bottom.

### Immediate

- [x] **Autotune / pitch-snap on TTS** — pitch-quantize espeak-ng output to current key/scale
      ("T-Pain" jungle toaster effect). Pure Rust: autocorrelation pitch detect → snap via
      `snap_to_scale` → linear-interpolation resample. "Pitch snap" toggle in TTS settings.

- [x] **Amen break sampler voice** — DrumVoice::Amen in sequencer, AmenVoice DSP with linear-interp
      playback, AudioCommand::LoadSampler, AMEN tab with path/pitch/volume/loop UI.
      (Pitch-shift only — duration changes with pitch; rubberband time-stretch is a later item.)

- [x] **Envelope visualization in bass panel** — decay_display widget already in bass.rs:343.

### Near-term

- [x] **MIDI clock in** — 8-pulse rolling average, SYNC button in BPM row, resets on Start/Stop.

- [ ] **Codecov integration** — before publishing to GitHub.
      See: https://github.com/codecov/example-rust
      Add `codecov` step to CI workflow, upload `lcov.info` from `./run-tests.sh --coverage`.

- [ ] **OSC support** — connect to Max/MSP, Ableton, TouchOSC. Pairs well with the MCP API.

- [ ] **Stem export** — per-voice WAV files. Route each voice to its own output buffer before mix.

- [ ] **Project versioning** — auto-save snapshots on pattern change, revert history (ring buffer of N states).

### Later

- [ ] **FX routing: modular slots** — replace fixed chain with assignable FX nodes. Unlocks dub techno (FX as instrument), gabber (pitch env + clipper on kick), and ambient (slow filter automation).

- [ ] **Gabber kick voice** — pitch envelope + hard clipper on kick output. Needed for gabber/hardcore styles where the kick IS the bass.

- [ ] **Multiple voices** — `Vec<SynthVoice>`, each with its own sequencer + oscillator + filter. LLM can target "voice 2, more acid".

- [ ] **Multiple LLM instances** — one LLM per voice, or routing matrix.

- [ ] **Modular cable UI** (Reason-style rack flip) — Tab flips to back panel showing I/O ports + Bezier cables. Needs modular FX routing first.

- [ ] **Bloom post-process** — egui frame → wgpu render pass → Gaussian blur on bright pixels → additive blend. Gated by `UiPrefs.bloom_enabled`. Costs a GPU render pass per frame.

- [ ] **XY pad improvements** — show param name tooltip on cursor; use for any two correlated params.

- [ ] **Coqui TTS** — higher quality voice (Python subprocess or REST). Alternative to espeak-ng.

- [ ] **Windows native scripts** — `start.bat`, `build.bat` for users who don't have WSL.

---

## Known Gaps (styles vs synth reality)

| Style | What it promises | What's still missing |
|-------|-----------------|----------------------|
| Jungle / DnB | Amen break energy | Sampler voice |
| Dub Techno | FX IS the music | Modular FX routing |
| Gabber | Kick IS the bass | Gabber kick voice (pitch env + clipper) |
| Synthwave | Gated reverb snare | Reverb gate / envelope on drum channel |
| Vaporwave | Pitch-shifted, woozy | Pitch control on output, pitch drift LFO on master |
| Ambient | Glacial filter sweeps | LFO automation is wired; needs longer attack/decay times |

---

## Model Options

Bonsai-8B is Qwen3-8B compressed to 1-bit (Q1_0_g128, PrismML format, 1.1 GB).
Quality ceiling is low but it fits in 2 GB VRAM. The llama-server backend is model-agnostic
— just swap the GGUF file and update the model selector in prefs.

| Model | Download | Size | VRAM | Notes |
|-------|----------|------|------|-------|
| **Bonsai-8B** | `./download-models.sh` | ~1.1 GB | ~2 GB | Default; fastest, lowest quality |
| **Qwen3-8B Q4_K_M** | `./download-models.sh qwen3` | ~5 GB | ~7 GB | ~5× better; supports `/think` reasoning |
| **Qwen3-14B Q4_K_M** | `./download-models.sh qwen3-14b` | ~9 GB | ~11 GB | Best musical reasoning; needs 12 GB VRAM |
| **Gemma 4 E4B Q4_K_M** | `./download-models.sh gemma4` | ~5 GB | ~7 GB | Fast; strong structured JSON output |
| **Llama 3.1 8B Q4_K_M** | `./download-models.sh llama31` | ~5 GB | ~7 GB | Excellent JSON compliance |

All models require a free HuggingFace account (`huggingface-cli login`).
