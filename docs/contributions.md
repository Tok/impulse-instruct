# Contributing to Impulse Instruct

## What's most useful right now

### Style and tuning contributions

The LLM integration tests in `src/llm_suite_style.rs` check whether PULSE responds correctly to genre and artist references - "make it BoC", "go full gabber", "darkstep energy". Each test fires a real prompt against a running model and asserts on specific parameter outcomes.

The most valuable contributions are **new style entries and failing test cases**:

- A style entry that reliably translates "jungle" into a correct Amen pattern + sub pressure + fast BPM
- Tests that catch regressions when a model update makes PULSE forget that minimal techno means sparse, not full
- Reference sets from sub-genres that aren't covered yet (UK hardcore, footwork, dungeon synth, cumbia, etc.)

The style catalog lives in `src/llm/styles.json`. Each entry has:
```json
{
  "id": "jungle",
  "name": "Jungle / DnB",
  "brief": "...",
  "full": "..."
}
```

The `full` field is injected into the system prompt when the style is active. Good `full` entries are specific about sound design decisions, not just vibes - they tell the LLM *which parameters* to move and *why*.

### Music theory test suite

`src/llm_suite_theory.rs` tests PULSE's understanding of producer terminology:

- "make it more acidic" → filter cutoff stays low, resonance goes up, env mod increases
- "add tension" → specific EQ / FX / sequence choices
- "drop the root on beat 1" → specific step placement

New tests here help the model stay coherent across updates and identify gaps in the system prompt.

To run the suites:
```bash
./scripts/run-llm-tests.sh      # all suites (needs a running model)
./scripts/run-llm-style.sh      # style/artist reference tests only
./scripts/run-llm-theory.sh     # music theory + producer lingo tests only
```

### Synth voice tuning

Some voices are rough. The hoover lead is the most obvious gap - it doesn't yet sound like the classic Human Resource / Dominator vacuum cleaner screech. Getting it there requires tuning the supersaw → highpass sweep → pitch LFO chain, and probably a dedicated resonant sweep shape.

If you know the original signal chain and want to help dial it in, the relevant code is in `src/audio/dsp/hoover.rs` (if it exists) or the hoover voice section of `src/audio/dsp/mod.rs`. Parameter ranges are set in `src/state/mod.rs` under `HooverState`.

### Bug reports and edge cases

- Patterns that crash the JSON parser (LLM output malformed in a reproducible way)
- Steps where the lock system doesn't correctly prevent an LLM override
- Audio glitches in specific FX chain configurations

Open an issue with the prompt that triggered it, the model version, and the log output from the LLM strip.

## Code contributions

Read `CLAUDE.md` first. The short version:

- Business logic and DSP must be **pure functions** - same input → same output, no side effects
- Every new pure function needs a test in `src/tests/`
- No files over 1000 lines (enforced by pre-commit)
- `cargo fmt` + `cargo clippy -- -D warnings` must pass

The project is written in Rust and uses egui for the UI, cpal for audio, and llama.cpp (via llama-server) for inference.

## What we are not looking for right now

- Bloom post-processing / GPU effects
- Additional synthesis architectures (we have enough voices to tune first)
- Alternative LLM backends

These may come later, but they're not blocking anything important.
