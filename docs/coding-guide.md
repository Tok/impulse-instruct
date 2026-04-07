# Impulse Instruct - Coding Guide

This guide covers the functional programming patterns required throughout the codebase.
Read it before writing significant new code or refactoring existing code.
It is the authoritative reference for AI contributor requirements listed in CONTRIBUTING.md.

---

## The core rule: pure functions for core logic

Business logic, state transitions, sequencer math, and DSP math must be **pure functions**:
same inputs always produce the same output, no hidden state, no side effects.

```rust
// CORRECT — pure, testable, parallelisable
pub fn apply_llm_update(state: AppState, update: &serde_json::Value) -> AppState { ... }
pub fn advance_clock(clock: ClockState, seq: &SequencerState, block_size: usize, sr: f32)
    -> (ClockState, Vec<TriggerEvent>) { ... }
pub fn midi_to_hz(note: u8) -> f32 { 440.0 * 2.0_f32.powf((note as f32 - 69.0) / 12.0) }
pub fn toggle_drum_step(state: AppState, voice: DrumVoice, step: usize) -> AppState { ... }

// WRONG — mutates in place, hides state, impossible to test without a real AppState
pub fn apply_llm_update(&mut self, update: &serde_json::Value) { ... }
pub fn toggle_drum_step(&mut self, voice: DrumVoice, step: usize) { ... }
```

**How to tell if a function is pure:**

- It takes only its arguments (no `&self`, no global reads, no `Mutex` locks).
- It returns its result — it doesn't write to a field or channel as a side effect.
- You could call it twice with the same arguments and get the same answer both times.
- You could move it to a different thread and it would still work.

If any of those fail, the function is impure and should not live in `state/`, `sequencer/`,
or the DSP voice modules.

---

## State transitions by value, not mutation

State structs derive `Clone`. Transition functions take ownership of the old state and
return a new one. The caller replaces the binding.

```rust
// CORRECT — each line is an explicit state transition
let state = toggle_drum_step(state, DrumVoice::Kick808, 3);
let state = lock_param(state, "bass.cutoff");
let state = apply_llm_update(state, &json_response);

// WRONG — hides what changed, makes rollback impossible, breaks the pure-function rule
state.toggle_drum_step(DrumVoice::Kick808, 3);
state.lock_param("bass.cutoff");
state.apply_llm_update(&json_response);
```

Transition functions live in `src/state/transitions.rs`. When you add a new one:

1. Signature: `pub fn verb_noun(state: AppState, args...) -> AppState`
2. Body: `let mut s = state;` then mutate `s`, then `s` as the last expression.
3. Add a test in `src/tests/state_tests.rs`.

Example transition:

```rust
pub fn set_bass_cutoff(state: AppState, cutoff: f32) -> AppState {
    let mut s = state;
    s.bass.cutoff = cutoff.clamp(0.0, 1.0);
    s
}

#[test]
fn set_bass_cutoff_clamps() {
    let s = set_bass_cutoff(AppState::default(), 2.5);
    assert_eq!(s.bass.cutoff, 1.0);
    let s = set_bass_cutoff(s, -0.1);
    assert_eq!(s.bass.cutoff, 0.0);
}
```

---

## Immutable snapshots for cross-thread reads

The `Arc<RwLock<AppState>>` lock must be held for the shortest possible time.
When an LLM inference thread or HTTP handler needs to read state, snapshot it first,
release the lock, then do the work on the owned copy.

```rust
// CORRECT — lock held for a clone() call, then released
let snapshot = app_state.read().clone();
// ... long inference or HTTP serialisation on `snapshot` ...

// WRONG — lock held across an expensive operation, starving the UI thread
let guard = app_state.read();
let response = backend.infer(&guard);  // could take 5+ seconds
drop(guard);
```

The same principle applies when writing: compute the new state from the snapshot,
then take the write lock only to install it.

```rust
// CORRECT
let new_state = apply_llm_update(snapshot, &response);
*app_state.write() = new_state;

// WRONG — write lock held across computation
let mut guard = app_state.write();
guard.apply_llm_update(&response);
```

---

## Audio callback: zero allocations, no locks

`DspState::process_block()` and the cpal callback closure run on the audio thread.
They have hard real-time constraints:

**Never inside `process_block()` or the cpal closure:**

- `Vec::new()`, `.push()`, `.clone()`, `Box::new()`, `String::new()` — any heap allocation
- `Mutex::lock()`, `RwLock::read()`, `RwLock::write()` — any blocking
- `Arc::clone()` — ref-count increment is a memory barrier
- `println!()`, `eprintln!()` — I/O syscall, unbounded latency

**Correct pattern** — read params from the rtrb ring buffer, then use only stack values:

```rust
// In the cpal callback (audio/mod.rs):
while let Ok(cmd) = consumer.pop() {
    match cmd {
        AudioCommand::SetParams(p) => dsp.params = p,  // plain struct copy
        AudioCommand::SetFxPlan(plan) => dsp.fx_plan = plan,
        // ...
    }
}
dsp.process_block(&mut output_buffer);  // no alloc, no lock

// In process_block (audio/dsp/mod.rs):
// Snapshot the FX chain into a stack array before the frame loop.
// This releases the immutable borrow before mutable method calls.
const MAX_CHAIN: usize = 16;
let mut global_chain = [FxStep::Reverb; MAX_CHAIN];  // stack array, no alloc
let global_len = self.fx_plan.steps.len().min(MAX_CHAIN);
for (i, &step) in self.fx_plan.steps.iter().enumerate().take(MAX_CHAIN) {
    global_chain[i] = step;
}
```

If you find yourself reaching for a `Vec` inside the frame loop, the design is wrong.
Preallocate in `DspState::new()` and reuse across frames.

---

## Side effects belong at the edges

The architecture is a pure core wrapped in an effectful shell:

```
main.rs           ← thread spawning, channel wiring
audio/mod.rs      ← cpal stream setup, rtrb writes
api/mod.rs        ← HTTP handlers (axum)
llm/mod.rs        ← inference thread loop
midi/mod.rs       ← midir event loop

     ↓ sends commands / snapshots ↓

state/            ← pure AppState transitions (NO side effects)
sequencer/        ← pure clock advance (NO side effects)
audio/dsp/        ← pure DSP math (NO allocations)
```

Anything under `state/`, `sequencer/`, or `audio/dsp/voices/` must have zero I/O,
zero channel sends, zero thread spawns. If you need to notify another thread about a
state change, do it in the caller (in `llm/mod.rs` or `ui/mod.rs`) after the pure
transition returns.

---

## Every pure function gets a test

Testing pure functions is trivial — no mocks, no fixtures, no async runtime:

```rust
// src/tests/state_tests.rs
#[test]
fn set_bass_cutoff_clamps_above_one() {
    let s = set_bass_cutoff(AppState::default(), 1.5);
    assert_eq!(s.bass.cutoff, 1.0);
}

#[test]
fn toggle_drum_step_flips_on_and_off() {
    let s = toggle_drum_step(AppState::default(), DrumVoice::Kick808, 0);
    assert!(s.sequencer.drum_patterns[&DrumVoice::Kick808][0].active);
    let s = toggle_drum_step(s, DrumVoice::Kick808, 0);
    assert!(!s.sequencer.drum_patterns[&DrumVoice::Kick808][0].active);
}
```

**Where tests live:**

| File | Tests for |
|------|-----------|
| `src/tests/seq_tests.rs` | Sequencer, euclidean rhythm, step arrays, probability |
| `src/tests/state_tests.rs` | State transitions, bank/chain, rack compilation |
| `src/tests/llm_tests.rs` | Prompt building, instruction matching, music theory, DSP math |

Each file has a **1000-line limit** enforced by the pre-commit hook.

**Split proactively, not reactively.** When a file reaches ~700 lines, or when you're
adding a cohesive block of functionality (a new wizard, a new panel, a new test suite),
create a new file immediately rather than appending to an existing one and waiting for
the hook to reject the commit.  A file that's 400 + 400 lines across two modules is
easier to navigate than an 800-line file that will need emergency surgery at 1000.

When splitting tests, add the new file to `src/tests/mod.rs`.

**What needs a test:**

- Every new function in `state/transitions.rs`
- Every new function in `sequencer/mod.rs`
- Every music-theory helper in `llm/`
- Any DSP math function with a closed-form expected value

**What doesn't need a test:**

- Functions that only call egui drawing functions
- The cpal callback and rtrb wiring in `audio/mod.rs`
- HTTP route handlers in `api/mod.rs`
- MIDI event dispatch in `midi/mod.rs`

These are excluded from coverage in `Cargo.toml` under `[package.metadata.tarpaulin]`
because they require hardware (audio device, MIDI port, network socket) to run.

---

## Refactoring impure code to pure

When you encounter an `&mut AppState` method in older code, the refactor is mechanical:

**Step 1:** Extract the method body into a free function.

```rust
// Before (method on AppState)
impl AppState {
    pub fn toggle_live_record(&mut self) {
        self.sequencer.live_record = !self.sequencer.live_record;
    }
}

// After (pure function in transitions.rs)
pub fn toggle_live_record(state: AppState) -> AppState {
    let mut s = state;
    s.sequencer.live_record = !s.sequencer.live_record;
    s
}
```

**Step 2:** Update all call sites to use the functional form.

```rust
// Before
app_state.write().toggle_live_record();

// After
let new_state = toggle_live_record(app_state.read().clone());
*app_state.write() = new_state;
```

**Step 3:** Add a test.

```rust
#[test]
fn toggle_live_record_flips_flag() {
    let s = AppState::default();
    assert!(!s.sequencer.live_record);
    let s = toggle_live_record(s);
    assert!(s.sequencer.live_record);
    let s = toggle_live_record(s);
    assert!(!s.sequencer.live_record);
}
```

---

## Abstraction rules

- Don't create a trait for something that only has one implementation.
- Don't add `Option<>` wrappers or config flags for hypothetical future requirements.
- **Do** extract a helper when the same structural pattern appears in 2+ places and
  the abstraction is obvious (same types, same shape, same intent).  Don't wait for
  a third occurrence if the duplication is already causing maintenance burden.
- **Do** extract helpers that reduce boilerplate when the pattern is noisy enough to
  obscure the logic.  Example: `rack.connect_control(from_id, to_id)` is worth
  creating even at 2 call sites if each call site expands to 8 lines of PortRef
  construction.
- Three similar lines of code are fine.  Fifteen similar lines repeated across
  four files is not — that's a missed abstraction, not a premature one.

---

## Pre-commit checklist

Before every commit, verify:

- [ ] `cargo fmt` applied to all changed files
- [ ] `cargo clippy -- -D warnings` reports 0 errors
- [ ] `cargo clippy --tests -- -D warnings` reports 0 errors
- [ ] `cargo test` passes (274+ tests, all green)
- [ ] New pure functions have a test in `src/tests/`
- [ ] No test file exceeds 1000 lines
- [ ] No allocations inside `process_block()` or the cpal callback
- [ ] State transitions are free functions, not `&mut self` methods
- [ ] Commit message follows `type: description` format with `Co-Authored-By` trailer
