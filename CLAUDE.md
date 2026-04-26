# Impulse Instruct - Claude Code Guide

## Build & run

```bash
cargo check                          # fast type-check, no binary
cargo build                          # debug build
cargo run                            # run (mock LLM, HTTP API on by default)
cargo run -- --no-api                # run without HTTP/MCP API
cargo run -- --model models/x.gguf --log debug
cargo run --features llm --release   # real LLM inference (needs libclang-dev)
cargo test                           # unit tests (split across src/tests/)
./start.sh                           # build + launch (release, mock LLM)
./start.sh --dev                     # build + launch (debug + verbose)
./scripts/run-tests.sh --coverage    # HTML coverage report
./scripts/build-all.sh               # Linux + Windows EXE → dist/
./scripts/download-models.sh         # fetch Gemma 4 E4B GGUF (~4.6 GB, default) + NeuTTS Air Q8 (~803 MB, optional)
./scripts/download-samples.sh <pack> # fetch CC-licensed sample packs (salamander/sso/vsco2/instruments-all + reference-only amen/textures/wavetables/impulses)
./scripts/run-llm-tests.sh           # all LLM integration suites (needs running model)
./scripts/run-llm-style.sh           # artist/genre reference tests only
./scripts/run-llm-theory.sh          # music theory + producer lingo tests only
./scripts/run-llm-bass.sh            # bass directive tests (multi-voice, accent/slide, subset, coverage)
```

Windows equivalents (`.bat` files mirror every `.sh` script):
```
start.bat                            # build + launch
scripts\run-tests.bat
scripts\build-all.bat
scripts\download-models.bat
scripts\download-samples.bat
scripts\build-llama-server.bat
scripts\run-llm-tests.bat
scripts\run-llm-style.bat
scripts\run-llm-theory.bat
scripts\run-llm-bass.bat
```

## Architecture - what lives where

| Path | Purpose |
|------|---------|
| `src/state/mod.rs` | Single `AppState` struct. All state transitions are **pure functions** at the bottom of this file. Start here when adding new parameters. |
| `src/audio/dsp.rs` | All DSP synthesis: 303 ladder filter, 808/909 voices, reverb, delay. Pure functions only - **no allocations inside `process_block()`**. |
| `src/audio/mod.rs` | cpal stream + rtrb ring buffer. Audio callback reads from rtrb, never touches `Arc<RwLock<AppState>>`. |
| `src/sequencer/mod.rs` | 16-step clock as a pure function: `advance_clock(ClockState, &SequencerState, block_size, sr) → (ClockState, Vec<TriggerEvent>)` |
| `src/llm/mod.rs` | LLM inference thread. Mock mode when no model file found. Real inference via `--features llm`. |
| `src/llm/prompt.rs` | System prompt builder + JSON schema for grammar-constrained generation. |
| `src/api/mod.rs` | axum HTTP/MCP API. Only starts when `--api` flag passed. |
| `src/midi/mod.rs` | midir input (CC→param mapping, NoteOn/Off → live record) + MIDI clock output (MidiClockOutput struct). |
| `src/ui/mod.rs` | egui app: 5 panels (Sequencer / 303 / 808 / 909 / FX) + AN1X + Hoover sub-panels. |
| `src/ui/theme.rs` | Grayscale palette - **all UI colors must satisfy R=G=B** (no tint). Huth *Farbige Noten* colors are the only exception (note highlights). See `docs/ui-design.md`. |
| `src/state/rack.rs` | Modular rack: `ModuleKind`, `RackModule`, `Cable`, `FxPlan`, cycle detection. See `docs/rack.md`. |
| `src/state/fx_plan.rs` | `compile_fx_plan()` — Kahn's topo-sort over FX cable graph into `FxPlan`. |
| `src/llm/vram.rs` | VRAM budget: model profiles, `estimate_vram()`, `would_exceed_vram()`, agent presets. |
| `src/ui/widgets.rs` | Chrome knob, glass slider, embossed button, step button, LED, XY pad, oscilloscope, ADSR visualizer. |
| `src/ui/panels/` | One file per synth panel (bass, 808, 909, hoover, an1x, fx, sequencer). |
| `src/state/transitions.rs` | Pure state transition functions (all the `toggle_*`, `set_*`, `apply_*`, `bank_*`, `chain_*` fns). |
| `src/tests/mod.rs` | Test submodule index. |
| `src/tests/seq_tests.rs` | Sequencer, euclidean, step array, probability tests. |
| `src/tests/state_tests.rs` | State, expand steps, transition, bank/chain tests. |
| `src/tests/llm_tests.rs` | Prompt, instruction, music theory, DSP tests. |

## Coding style

Read **[docs/coding-guide.md](docs/coding-guide.md)** before writing significant new code
or refactoring. It covers pure functions, state transitions, audio callback invariants,
testing patterns, and the pre-commit checklist with examples.

Quick summary of the key rules:

- Core logic (state transitions, sequencer, DSP math) must be **pure functions** — same
  inputs → same output, no side effects, no hidden state.
- State transitions take ownership and return new state: `fn toggle_x(state: AppState) -> AppState`.
- Lock `Arc<RwLock<AppState>>` only for the duration of a `.clone()`, never across inference or I/O.
- **No allocations inside `process_block()` or the cpal callback.** No `Vec::new()`, no locks.
- Every new pure function gets a test in `src/tests/`.
- **Toolchain alignment**: the Rust version is pinned via `rust-toolchain.toml`
  to keep local pre-commit clippy and CI clippy on the same lint set. CI runs
  `cargo clippy -- -D warnings` against `@stable`; if local were one minor
  behind, new clippy lints would sneak past the pre-commit hook and fail CI
  (e.g. `collapsible_match` landed in 1.95). To bump Rust: edit the channel
  in `rust-toolchain.toml`, run `cargo clippy -- -D warnings` locally until
  clean, then commit. Don't add a `#[allow(clippy::...)]` to silence a CI
  failure — fix the underlying pattern instead.

---

## Key invariants - do not break these

1. **Audio callback is allocation-free.** No `Vec::new()`, no `.clone()`, no locks inside `process_block()` or the cpal callback closure.
2. **AppState is never locked from the audio thread.** Audio reads params via the rtrb `Consumer<AudioCommand>` only.
3. **State transitions are pure functions.** `apply_llm_update`, `toggle_drum_step`, `lock_param` etc. take ownership, return new state. No `&mut AppState` methods.
4. **LLM cannot override locked params.** `AppState.llm.locked_params: HashSet<String>` - checked in `apply_llm_update`. Touching a UI knob adds its dot-path to this set.
5. **HTTP API only starts with `--api`.** Don't start it unconditionally.

## Adding a new synth parameter

1. Add field to the relevant state struct in `src/state/mod.rs`
2. Add it to `AudioParams` snapshot in `src/audio/dsp.rs`
3. Handle it in `AudioParams::from_app_state()`
4. Use it in `DspState::process_block()` or a voice's `process()` method
5. Add to `apply_llm_update()` in `src/state/mod.rs`
6. Add to the JSON schema in `src/llm/prompt.rs` (`param_json_schema()`)
7. Add a knob/control in the relevant UI panel in `src/ui/mod.rs`

## Crate versions (locked)

- egui/eframe 0.28 - UI
- cpal 0.15 - audio I/O
- axum 0.7 - HTTP
- midir 0.9 - MIDI
- rtrb 0.3 - lock-free audio ring buffer
- llama-cpp-2 0.1 - optional, needs `libclang-dev cmake`

## LLM integration

Models (ranked by test suite results):
- **Gemma 4 E4B Q4_K_M** - default, 4.6 GB, best accuracy, passes all 39 LLM integration tests
- **Qwen3-8B / 14B** - optional, chain-of-thought capable; not recommended as default (heavier, no accuracy gain over Gemma 4)
- **Other GGUF models** (e.g. Llama variants) - technically compatible with llama-server but not evaluated; system prompt is not tuned for them. Users are free to experiment.

Server: `.llama-official-build/bin/llama-server` (standard llama.cpp), built via `./scripts/build-llama-server.sh`.

- Mock mode: runs without model, returns plausible JSON based on prompt keywords + instruction set
- Real mode: any GGUF model via llama-server subprocess; model selected at runtime via UI
- LLM outputs JSON only - step arrays use compact formats: index list `[0,4,8,12]` or inline `[1,0,…]` or clear `[]`
- JSON is applied via `apply_llm_update()` in `src/state/transitions.rs`, which respects `locked_params`
- `sanitize_json_structure()` in `src/llm/mod.rs` fixes common LLM output errors (LFO dot-notation, etc.) before parsing
- `max_tokens: 1200` - keep this high enough to avoid JSON truncation on complex responses

## HTTP API (port 8765)

```
GET  /api/state          full AppState as JSON
GET  /api/schema         parameter JSON schema
POST /api/prompt         { "prompt": "make it acid" }           (one_shot default true)
POST /api/prompt         { "prompt": "jam", "one_shot": false }  (self-perpetuates while heat>0)
POST /api/params         { "params": { "tb303": { "cutoff": 0.4 } } }
POST /api/lock           { "paths": ["tb303.cutoff"] }
POST /api/unlock         { "paths": ["tb303.cutoff"] }
POST /api/sequencer/play
POST /api/sequencer/stop
GET  /api/song                                    returns { chain, overrides, enabled, pos, repeat_count }
POST /api/song           { "chain": [0,1,2], "overrides": [{ "style": "jungle", "repeats": 2 }, {}, { "bpm": 140.0 }], "enabled": true }
POST /api/scroll         { "target": "voice" }  (global/voice/fxmod/bass/808/fx/…)
POST /api/scroll         { "target": "bass", "collapse_others": true }  focus mode
POST /api/preset         { "name": "Crew" }     (Solo/Duo/Swarm/Crew/Voices)
POST /api/style          { "id": "drum_and_bass" }  set global style + propagate to agents (id=null clears)
POST /api/randomize                              random style + auto-rack + LLM "generate from scratch"
POST /api/amen           { "path": "samples/amen/foo.wav" }  load a specific amen sample
POST /api/amen           { "random": true }     load a random sample from samples/amen/
POST /api/granular       { "path": "samples/textures/pad.wav" }  load a granular texture
POST /api/granular       { "random": true }     load a random sample from samples/textures/
POST /api/sample         { "path": "samples/instruments/piano_C4.wav" }  load a specific sample-instrument recording
POST /api/sample         { "random": true }     load a random sample from samples/instruments/
POST /api/flip           { "show_back": true }   (true=cables, false=knobs)
POST /api/rack/reset                              strip to sequencer + master + console
POST /api/rack/random    { "seed": 42 }            wipe + drop random voices/FX/LFOs (Eurorack patch generator); seed optional
POST /api/morph          { "prompt": "evolve from cathedral to dystopia", "bars": 8, "calls": 8 }   AI patch morph — schedules N LLM nudges across N bars (calls defaults to bars; capped at 4× bars)
POST /api/midi/granulise { "voice": "bass", "voice_index": 0, "density": 0.7, "repeat_chance": 0.2, "pitch_jitter_st": 5, "seed": 42 }   MIDI granuliser — scatter triggers in the named voice's pattern (density drops, repeat doubles, pitch_jitter_st transposes ±N st)
POST /api/rack/add       { "kind": "808" }        add module, returns { "id": N }
POST /api/rack/agent     { "persona": "BASS", "scope": ["bass"], "model": "gemma", "mode": "mc", "tts": true }
POST /api/rack/cable     { "from": 1, "to": 5, "kind": "audio", "audio_gain": 0.4 }  connect modules (default kind: control; audio_gain 0..1.5, optional)
POST /api/rack/cable_gain{ "from": 1, "to": 5, "gain": 0.8 }  set per-cable audio gain (feedback-edge clamp applies automatically)
POST /api/rack/mod_cable { "from": 7, "to": 1, "slot": 0, "depth": 0.5 }  LFO→Mod-In jack
POST /api/rack/mod_target{ "module": 1, "slot": 0, "targets": ["BassPan", "BassCutoff"] }
POST /api/rack/mod_depth { "module": 1, "slot": 0, "depth": 0.5 }  per-jack depth 0..1
POST /api/rack/remove    { "id": 5 }              remove module + its cables
POST /api/rack/pad       { "id": 5, "expanded": true, "pair": 1 }  XY pad expand + pair
POST /api/midi/export    { "path": "pattern.mid" }  export sequencer as SMF (Type 1)
POST /api/rack/collapse  { "action": "all" }      all/none/global/voice/fxmod
```

## Not yet implemented

- Rack CV cables driving LFO targets (cables are visual; LFO targets set via state field)
- Multiple LLM instances per inference (multi-turn within one cycle)
