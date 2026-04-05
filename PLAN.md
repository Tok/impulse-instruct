# Impulse Instruct — Roadmap

A synthesizer with a tiny LLM living inside it that actually understands music.
PULSE listens, jams, evolves, and shouts at the crowd.

---

## Audio Feedback Loop — "PULSE Listens to Itself"

**Phase 1 is implemented.** The LISTEN button in the LLM strip captures up
to 10 seconds of audio, runs a per-band RMS + transient analysis
(`src/audio/analysis.rs`), shows the stats inline, and prepends a structured
text snapshot to the inference prompt. Responses are labelled **LISTEN →**
in the log.

**Phase 2 (real audio input to the model) is on hold.** As of April 2026,
llama.cpp does not support Gemma 4's audio encoder — and even when it does,
the encoder was trained on speech only, so musical audio may yield poor
results anyway. The text descriptor approach is likely the better fit for
mix/arrangement feedback regardless.

See **[docs/audio-feedback.md](docs/audio-feedback.md)** for full research
findings, PR numbers to watch, the API format once support lands, and an
alternative Ultravox-as-secondary-listener sketch.

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
- Jam mode — PULSE evolves the pattern autonomously; heat slider 0–100% gates/throttles jam rate
- Lock system — touch a knob to claim it; LLM won't override it
- Compact step arrays: index list `[0,4,8,12]` or inline `[1,0,0,0,…]` or clear `[]`
- Music theory grounding — root note + scale in system prompt, scale-snap on bass notes
- Instruction set — pre-written JSON templates for common phrases ("make an amen break", "remove claps", etc.)
- LFO dot-notation sanitization — handles malformed LLM output gracefully
- Sampling params exposed in settings: top_k, top_p, min_p, repeat_penalty, frequency_penalty, seed
- Reasoning (thinking) blocks shown in log (toggle in settings)
- **Audio feedback (Phase 1)** — LISTEN button captures audio, runs per-band RMS + transient analysis,
  prepends structured snapshot to prompt; response logged as `LISTEN →`

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
- Piano display — Huth *Farbige Noten* (1888) 12-color theory, C2–C5; Off/Piano/Full setting
- Huth sequencer cells (Full mode) — colored U-cup notation on bass/hoover/AN1X rows; gate-proportional height
- Model selector — scan `models/`, hot-swap without restart
- Reasoning toggle; thinking blocks shown in log (toggle)
- AI persona name — editable, used in system prompt
- LLM strip: LISTEN button + live audio analysis display (sub/low/mid/high RMS, peak, crest, transients)

### Testing & build
- 96 unit tests across 4 submodules (`seq_tests`, `state_tests`, `llm_tests`, `audio::analysis`, split at 1000-line limit)
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

### Next session — pick from here

- [ ] **FX routing: per-voice buses** — `compile_fx_plan()` now collects
      per-voice explicit FX routes from Voice→FX cables into `FxPlan.voice_routes`
      (infrastructure done, 3 new tests). DSP wiring: split voice mix into buses,
      use `voice_routes` to route each bus through its own FX chain instead of
      the global chain. Requires separate FX state per bus or send/return model.

- [x] **Audio feedback improvements** — Phase 1 is live. Done in Phase 2:
      - Auto-listen mode: AUTO toggle in LISTEN bar, fires every 4 jam cycles when heat > 0.
      - Per-voice level bars: 8 mini bars (BAS/K-A/S-A/HH/K-B/S-B/CLP/AMN) from state volume params.
      - Stereo width metric: deferred — audio is mono; needs stereo capture pipeline first.
      - Watch llama.cpp #21325 for Gemma 4 audio encoder PR; test when it lands.

- [x] **Gabber kick voice** — `clip` param (0–1) on both kicks: `(raw * (1 + clip*9)).clamp(-1,1)`.
      Hard flat-top distortion. CLIP knob in Kit A + Kit B panels, LLM-addressable via
      `kit_a.kick.clip` / `kit_b.kick.clip`. `#[serde(default)]` for backwards compat.

- [x] **XY pad improvements** — param name tooltip on cursor; arbitrary param pair selection.
      Right-click cycles pairs stored in egui temp memory; pair indicator (n/N) in corner.

- [x] **Coqui TTS** — higher quality MC voice. `TtsEngine` enum (EspeakNg / CoquiTts) in
      `LlmState`. `speak_coqui()` calls `tts --text … --out_path /tmp/…` CLI; auto-falls
      back to espeak-ng if binary not found. Engine toggle in TTS settings panel.

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

- [x] **Modular rack canvas** — replace 5-tab layout with zone-based horizontal module cards
      (Global / Voice / FxMod zones). RackState + Cable + PortRef in state. Module cards with
      chrome title bars, port jack circles, enable LED, remove button. Rack-rail zone separators
      with screw holes. Bezier cable overlay with 3D tube rendering (shadow + colour + specular).
      Drag-to-connect port interaction. No horizontal scroll — cards wrap to next row.

- [x] **FX routing: modular slots** — `compile_fx_plan()` topologically sorts FX cable
      graph → `FxPlan` sent to audio thread via `AudioCommand::SetFxPlan`. `process_block()`
      iterates the plan instead of a fixed chain. Default rack cables mirror the original
      serial order. Enable/disable/remove/connect/disconnect all recompile the plan live.
      Next step: per-voice FX buses (route individual voices to separate FX).

- [x] **Gabber kick voice** — CLIP knob on both kicks. Hard-clip drive: boost then clamp to ±1.

- [ ] **TTS as rack modules** — replace the current single `TtsEngine` enum with two proper
      rack module kinds: `EspeakNgTts` and `CoquiTts`. Each has audio output ports so
      it can be patched into the FX chain like any other module. Module front panel shows
      voice/speed/pitch controls and a SYNC button (triggers a phrase on every N bars).
      Personality (MC / DJ / Producer / Off) moves into a separate `PersonalityModule` that
      can be connected to any TTS node — user drops the module they want, LLM can do the same.
      The current `tts_engine` field + UI settings panel become a migration shim until rack
      adoption is complete.

- [x] **LLM jam tools** — done: ramp scheduling (`"ramp"` key), behaviour templates
      ("build", "drop", "breakdown", "tension", "euphoric"), heat-aware guidance in prompt.
      19 tests in `jam_tools_tests.rs`.

- [x] **Internal music API** — `src/music_api/mod.rs`; all 10 ChordQuality variants,
      amen_pattern(heat, seed), scale_run(root, scale, direction, seed),
      random_diatonic_chord; LLM dispatches via `"music_api"` JSON block; 47 tests.

- [ ] **Multiple voices** — `Vec<SynthVoice>`, each with its own sequencer + oscillator + filter. LLM can target "voice 2, more acid".

- [ ] **Multiple LLM instances** — one LLM per voice, or routing matrix.

- [ ] **Modular cable UI** (Reason-style rack flip) — Tab flips to back panel showing I/O ports + Bezier cables. Needs modular FX routing first.

- [ ] **Bloom post-process / UI polish** — Current knob highlights already look good.
      Re-evaluate: Bloom (egui → wgpu render pass → Gaussian blur on bright pixels →
      additive blend) is GPU-expensive and may not add much over existing chrome finish.
      Alternative: spend the effort on general UI polish — tighter layout, better colour
      contrast on inactive controls, smoother XY pad interactions. Decide when tackling.

- [x] **XY pad improvements** — done: tooltip, pair cycling, pair indicator.

- [x] **Coqui TTS** — done: TtsEngine enum, speak_coqui(), engine toggle in TTS settings.

- [x] **Windows native scripts** — `start.bat`, `scripts/build-all.bat`, `scripts/run-tests.bat`, `scripts/run-llm-tests.bat`, `scripts/run-llm-style.bat`, `scripts/run-llm-theory.bat`, `scripts/download-models.bat`, `scripts/build-bonsai-server.bat`, `scripts/build-llama-server.bat`.

---

## Known Gaps (styles vs synth reality)

| Style | What it promises | What's still missing |
|-------|-----------------|----------------------|
| Jungle / DnB | Amen break energy | Sampler voice |
| Dub Techno | FX IS the music | Modular FX routing |
| Gabber | Kick IS the bass | ~~Gabber kick voice (pitch env + clipper)~~ done — CLIP knob on both kicks |
| Synthwave | Gated reverb snare | ~~Reverb gate~~ done — `fx.reverb_gate_time` (0–2s), GATE knob in FX panel |
| Vaporwave | Pitch-shifted, woozy | ~~Master pitch~~ done — `fx.master_pitch_st` (±12 st), PITCH knob in MASTER group; LFO target 3 = bass pitch which subsumes drift |
| Ambient | Glacial filter sweeps | LFO automation is wired; needs longer attack/decay times |

---

## Model Options

The llama-server backend is model-agnostic — swap the GGUF and update the model selector.
Gemma 4 E4B is the default: best test scores (39/39 integration tests), fast, compact.

| Model | Download | Size | VRAM | Notes |
|-------|----------|------|------|-------|
| **Gemma 4 E4B Q4_K_M** | `./scripts/download-models.sh` | ~4.6 GB | ~6 GB | **Default**; best accuracy, 39/39 tests |
| **Bonsai-8B Q1_0_g128** | `./scripts/download-models.sh bonsai` | ~1.1 GB | ~2 GB | Lightweight fallback; no CoT, needs PrismML server fork |
| **DeepSeek-R1-Distill-Qwen-7B** | `./scripts/download-models.sh deepseek-r1-7b` | ~5 GB | ~7 GB | CoT capable, Qwen2.5 base; MIT license; newer distills may exist |
| **DeepSeek-R1-Distill-Qwen-14B** | `./scripts/download-models.sh deepseek-r1-14b` | ~9 GB | ~11 GB | CoT, higher quality; needs 12 GB VRAM |
| **Qwen3-8B Q4_K_M** | `./scripts/download-models.sh qwen3` | ~5 GB | ~7 GB | Optional; `/think` chain-of-thought mode; not recommended (heavier, no accuracy gain over Gemma 4) |
| **Qwen3-14B Q4_K_M** | `./scripts/download-models.sh qwen3-14b` | ~9 GB | ~11 GB | Optional large; needs 12 GB VRAM |
| Any other GGUF | drop in `models/` | varies | varies | Technically compatible; prompt not tuned. Llama variants were tested — may work with user tuning. |

All models require a free HuggingFace account (`huggingface-cli login`).
