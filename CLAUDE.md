# Impulse Instruct — Claude Code Guide

## Build & run

```bash
cargo check                          # fast type-check, no binary
cargo build                          # debug build
cargo run                            # run (mock LLM, no model needed)
cargo run -- --api                   # enable HTTP API on :8765
cargo run -- --api --model models/x.gguf --log debug
cargo run --features llm --release   # real LLM inference (needs libclang-dev)
cargo test                           # unit tests (split across src/tests/)
./scripts/run-tests.sh --coverage    # HTML coverage report
./scripts/build-all.sh               # Linux + Windows EXE → dist/
./scripts/download-models.sh         # fetch Gemma 4 E4B GGUF (~4.6 GB, default, needs HF account)
./scripts/run-llm-tests.sh           # all LLM integration suites (needs running model)
./scripts/run-llm-style.sh           # artist/genre reference tests only
./scripts/run-llm-theory.sh          # music theory + producer lingo tests only
```

## Architecture — what lives where

| Path | Purpose |
|------|---------|
| `src/state/mod.rs` | Single `AppState` struct. All state transitions are **pure functions** at the bottom of this file. Start here when adding new parameters. |
| `src/audio/dsp.rs` | All DSP synthesis: 303 ladder filter, 808/909 voices, reverb, delay. Pure functions only — **no allocations inside `process_block()`**. |
| `src/audio/mod.rs` | cpal stream + rtrb ring buffer. Audio callback reads from rtrb, never touches `Arc<RwLock<AppState>>`. |
| `src/sequencer/mod.rs` | 16-step clock as a pure function: `advance_clock(ClockState, &SequencerState, block_size, sr) → (ClockState, Vec<TriggerEvent>)` |
| `src/llm/mod.rs` | LLM inference thread. Mock mode when no model file found. Real inference via `--features llm`. |
| `src/llm/prompt.rs` | System prompt builder + JSON schema for grammar-constrained generation. |
| `src/api/mod.rs` | axum HTTP/MCP API. Only starts when `--api` flag passed. |
| `src/midi/mod.rs` | midir input (CC→param mapping, NoteOn/Off → live record) + MIDI clock output (MidiClockOutput struct). |
| `src/ui/mod.rs` | egui app: 5 panels (Sequencer / 303 / 808 / 909 / FX) + AN1X + Hoover sub-panels. |
| `src/ui/theme.rs` | Grayscale palette — all colors are R=G=B. Huth Farbige Noten colors used for note highlights only. |
| `src/ui/widgets.rs` | Chrome knob, glass slider, embossed button, step button, LED, XY pad, oscilloscope, ADSR visualizer. |
| `src/ui/panels/` | One file per synth panel (bass, 808, 909, hoover, an1x, fx, sequencer). |
| `src/state/transitions.rs` | Pure state transition functions (all the `toggle_*`, `set_*`, `apply_*`, `bank_*`, `chain_*` fns). |
| `src/tests/mod.rs` | Test submodule index. |
| `src/tests/seq_tests.rs` | Sequencer, euclidean, step array, probability tests. |
| `src/tests/state_tests.rs` | State, expand steps, transition, bank/chain tests. |
| `src/tests/llm_tests.rs` | Prompt, instruction, music theory, DSP tests. |

## Coding style — functional patterns first

This project deliberately applies functional programming principles in Rust.
Follow these consistently when writing or modifying any code.

### Core logic must be pure functions

Business logic, DSP, sequencer math, and state transitions must be
**pure functions**: same input → same output, no hidden state, no side effects.

```rust
// CORRECT — pure, trivially testable
pub fn apply_llm_update(state: AppState, update: &serde_json::Value) -> AppState { ... }
pub fn advance_clock(clock: ClockState, seq: &SequencerState, ...) -> (ClockState, Vec<TriggerEvent>) { ... }
pub fn midi_to_hz(note: u8) -> f32 { ... }

// WRONG — mutates in place, hard to test, hard to reason about
pub fn apply_llm_update(&mut self, update: &serde_json::Value) { ... }
```

### State transitions by value, not by mutation

State structs derive `Clone`. Transition functions take ownership (or a clone)
and return new state. The caller replaces the old state.

```rust
// CORRECT
let state = toggle_drum_step(state, DrumVoice::Kick808, 3);

// WRONG
state.toggle_drum_step(DrumVoice::Kick808, 3);
```

### Immutable snapshots for cross-thread reads

When the LLM or audio thread needs to read shared state, snapshot it first,
then release the lock, then do the work on the owned copy.

```rust
// CORRECT — lock held for microseconds
let snapshot = state.read().clone();
// ... long inference or DSP work on snapshot ...

// WRONG — lock held across inference/audio work
let guard = state.read();
backend.infer(&guard); // blocks other writers for the entire call
```

### Side effects at the edges only

Pure core, effectful shell. I/O, threads, channels, and locks belong in:
- `main.rs` — thread spawning, channel wiring
- `audio/mod.rs` — cpal stream, rtrb writes
- `api/mod.rs` — HTTP handlers
- `llm/mod.rs` — inference thread loop

Everything under `state/`, `sequencer/`, and DSP voices in `audio/dsp.rs`
must be free of side effects.

### Every pure function gets a test

New pure functions go in the appropriate submodule under `src/tests/`:
- `seq_tests.rs` — sequencer, euclidean, step arrays, probability
- `state_tests.rs` — state transitions, bank/chain operations
- `llm_tests.rs` — prompt building, instruction set, music theory, DSP

If a function is pure, testing it is trivial — just call it with inputs and assert on outputs. No mocks needed.

Each test file has a **1000-line limit** enforced by the pre-commit hook. If a file approaches the limit, split it into a new submodule and add an entry to `src/tests/mod.rs`.

```rust
#[test]
fn my_new_thing_does_what_it_says() {
    let result = my_pure_fn(input_a, input_b);
    assert_eq!(result, expected);
}
```

### Avoid unnecessary abstraction

Don't create traits, wrapper types, or helper utilities for things that only
exist once. Three similar lines of code are better than a premature abstraction.
Extract only when the same logic is needed in three or more genuinely distinct places.

### No speculative features

Don't add parameters, config options, or code paths for hypothetical future
requirements. Build exactly what the current task needs.

---

## Key invariants — do not break these

1. **Audio callback is allocation-free.** No `Vec::new()`, no `.clone()`, no locks inside `process_block()` or the cpal callback closure.
2. **AppState is never locked from the audio thread.** Audio reads params via the rtrb `Consumer<AudioCommand>` only.
3. **State transitions are pure functions.** `apply_llm_update`, `toggle_drum_step`, `lock_param` etc. take ownership, return new state. No `&mut AppState` methods.
4. **LLM cannot override locked params.** `AppState.llm.locked_params: HashSet<String>` — checked in `apply_llm_update`. Touching a UI knob adds its dot-path to this set.
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

- egui/eframe 0.28 — UI
- cpal 0.15 — audio I/O
- axum 0.7 — HTTP
- midir 0.9 — MIDI
- rtrb 0.3 — lock-free audio ring buffer
- llama-cpp-2 0.1 — optional, needs `libclang-dev cmake`

## LLM integration

Models (ranked by test suite results):
- **Gemma 4 E4B Q4_K_M** — default, 4.6 GB, best accuracy, passes all 39 integration tests
- **Bonsai 8B Q1_0_g128** — 1.1 GB fallback, no chain-of-thought, requires PrismML llama-server fork
- **Qwen3-8B / 14B** — optional, chain-of-thought capable, slower but no accuracy advantage
- ~~Llama 3.1 8B~~ — removed, OOM crash under load

Server selection: Bonsai uses `.llama-build/bin/llama-server` (PrismML fork, Q1_0_g128 format).
All other models use `.llama-official-build/bin/llama-server` (standard llama.cpp).

- Mock mode: runs without model, returns plausible JSON based on prompt keywords + instruction set
- Real mode: any GGUF model via llama-server subprocess; model selected at runtime via UI
- LLM outputs JSON only — step arrays use compact formats: index list `[0,4,8,12]` or inline `[1,0,…]` or clear `[]`
- JSON is applied via `apply_llm_update()` in `src/state/transitions.rs`, which respects `locked_params`
- `sanitize_json_structure()` in `src/llm/mod.rs` fixes common LLM output errors (LFO dot-notation, etc.) before parsing
- `max_tokens: 1200` — keep this high enough to avoid JSON truncation on complex responses

## HTTP API (port 8765)

```
GET  /api/state          full AppState as JSON
GET  /api/schema         parameter JSON schema
POST /api/prompt         { "prompt": "make it acid" }
POST /api/params         { "params": { "tb303": { "cutoff": 0.4 } } }
POST /api/lock           { "paths": ["tb303.cutoff"] }
POST /api/unlock         { "paths": ["tb303.cutoff"] }
POST /api/sequencer/play
POST /api/sequencer/stop
```

## Not yet implemented

- Amen break sampler voice (load WAV, pitch + time-stretch)
- Autotune / pitch-snap on TTS output (rubberband-cli post-process)
- MIDI clock in (slave BPM to external)
- OSC support (Max/MSP, TouchOSC)
- Stem export (per-voice WAV)
- Project versioning (auto-save snapshots, revert history)
- Modular FX routing (currently a fixed chain)
- Gabber kick voice (pitch env + hard clipper)
- Bloom post-process (needs custom wgpu render pass)
- Alternate tuning tables (gamelan slendro etc.)
- Windows: `start.bat`, `build.bat`
