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
- LLM runs locally via llama-server subprocess (official llama.cpp for Gemma/Qwen; PrismML fork for Bonsai 1-bit)
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
- 86 unit tests across 4 submodules (`seq_tests`, `state_tests`, `llm_tests`, split at 1000-line limit)
- 39 LLM integration tests in 3 suites: `llm_suite` (core), `llm_suite_style` (artist refs), `llm_suite_theory` (music theory + producer lingo)
- Pre-commit hook: fmt + clippy + tests + 1000-line LOC limit
- `scripts/run-tests.sh --coverage` → HTML coverage report (lcov)
- `scripts/run-llm-tests.sh` / `run-llm-style.sh` / `run-llm-theory.sh` — LLM test runners
- Cross-compile to Windows EXE via `cargo-xwin` + `scripts/build-all.sh`
- `scripts/download-models.sh` — Gemma 4 E4B (default), Bonsai 8B, Qwen3-8B, Qwen3-14B
- Windows `.bat` equivalents for all scripts (`start.bat`, `scripts/*.bat`)

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

- [x] **Codecov integration** — `.github/workflows/ci.yml` runs tests + tarpaulin + uploads lcov.info.
      Requires `CODECOV_TOKEN` secret in repo settings once published.

- [x] **OSC support** — UDP listener on `--osc` (port 57120) or `--osc-port N`. Addresses:
      `/impulse/<section>/<param> <value>`, `/impulse/sequencer/play|stop`,
      `/impulse/prompt <string>`. Works with Max/MSP, TouchOSC, Ableton, oscsend.

- [x] **Stem export** — "Export Stems" in File menu; renders bass/kit_a/kit_b/amen/noise/hoover/an1x separately.

- [x] **Project versioning** — StateHistory ring buffer (50 deep), Ctrl+Z/Y, Edit menu, LLM snapshots before apply.

### Later

- [ ] **FX routing: modular slots** — replace fixed chain with assignable FX nodes. Unlocks dub techno (FX as instrument), gabber (pitch env + clipper on kick), and ambient (slow filter automation).

- [ ] **Gabber kick voice** — pitch envelope + hard clipper on kick output. Needed for gabber/hardcore styles where the kick IS the bass.

- [ ] **Multiple voices** — `Vec<SynthVoice>`, each with its own sequencer + oscillator + filter. LLM can target "voice 2, more acid".

- [ ] **Multiple LLM instances** — one LLM per voice, or routing matrix.

- [ ] **Modular cable UI** (Reason-style rack flip) — Tab flips to back panel showing I/O ports + Bezier cables. Needs modular FX routing first.

- [ ] **Bloom post-process** — egui frame → wgpu render pass → Gaussian blur on bright pixels → additive blend. Gated by `UiPrefs.bloom_enabled`. Costs a GPU render pass per frame.

- [ ] **XY pad improvements** — show param name tooltip on cursor; use for any two correlated params.

- [ ] **Coqui TTS** — higher quality voice (Python subprocess or REST). Alternative to espeak-ng.

- [x] **Windows native scripts** — `start.bat`, `scripts/build-all.bat`, `scripts/run-tests.bat`, `scripts/run-llm-tests.bat`, `scripts/run-llm-style.bat`, `scripts/run-llm-theory.bat`, `scripts/download-models.bat`, `scripts/build-bonsai-server.bat`, `scripts/build-llama-server.bat`.

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

The llama-server backend is model-agnostic — swap the GGUF and update the model selector.
Gemma 4 E4B is the default: best test scores (39/39 integration tests), fast, compact.

| Model | Download | Size | VRAM | Notes |
|-------|----------|------|------|-------|
| **Gemma 4 E4B Q4_K_M** | `./scripts/download-models.sh` | ~4.6 GB | ~6 GB | **Default**; best accuracy, 39/39 tests |
| **Bonsai-8B Q1_0_g128** | `./scripts/download-models.sh bonsai` | ~1.1 GB | ~2 GB | Fallback; no CoT, needs PrismML server fork |
| **Qwen3-8B Q4_K_M** | `./scripts/download-models.sh qwen3` | ~5 GB | ~7 GB | Optional; chain-of-thought `/think` mode |
| **Qwen3-14B Q4_K_M** | `./scripts/download-models.sh qwen3-14b` | ~9 GB | ~11 GB | Optional large; needs 12 GB VRAM |

All models require a free HuggingFace account (`huggingface-cli login`).
