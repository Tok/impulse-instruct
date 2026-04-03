# Impulse Instruct — Claude Code Guide

## Build & run

```bash
cargo check                          # fast type-check, no binary
cargo build                          # debug build
cargo run                            # run (mock LLM, no model needed)
cargo run -- --api                   # enable HTTP API on :8765
cargo run -- --api --model models/x.gguf --log debug
cargo run --features llm --release   # real LLM inference (needs libclang-dev)
cargo test                           # 13 unit tests
./run-tests.sh --coverage            # HTML coverage report
./build-all.sh                       # Linux + Windows EXE → dist/
./download-models.sh                 # fetch Bonsai 8B GGUF (~1.1 GB 1-bit model, needs HF account)
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
| `src/midi/mod.rs` | midir input, CC→param mapping. |
| `src/ui/mod.rs` | egui app: 5 panels (Sequencer / 303 / 808 / 909 / FX). |
| `src/ui/theme.rs` | Grayscale palette — all colors are R=G=B. Color will be used for highlights later. |
| `src/ui/widgets.rs` | Rotary knob, step button, LED, section header. |
| `src/tests.rs` | Unit tests for pure functions. |

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

New pure functions go in `src/tests.rs`. If a function is pure, testing it
is trivial — just call it with inputs and assert on outputs. No mocks needed.

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

- Mock mode: runs without model, returns plausible JSON based on prompt keywords
- Real mode: requires `models/Bonsai-8B.gguf` (1-bit Q1_0_g128, PrismML format) + PrismML llama-server
- LLM outputs JSON only, constrained by the schema in `src/llm/prompt.rs`
- JSON is applied via `apply_llm_update()` which respects `locked_params`

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

## Planned / not yet implemented

- Real llama-cpp-2 inference path (mock only for now)
- MIDI wiring to AppState mutations
- Per-step velocity in UI
- Bitcrush FX
- Alternate tuning tables (gamelan slendro etc.)
- Windows: `start.bat`, `build.bat`
