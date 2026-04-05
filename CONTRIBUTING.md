# Contributing to Impulse Instruct

Thank you for your interest in contributing! This guide is organized by contributor type.

---

## 👥 For Human Contributors

**TL;DR:** Don't stress about the strict rules below - they're mainly for AI coding assistants. Submit your PR even if some checks fail or formatting is off. We appreciate your contribution and may have bots tidy things up later.

### Philosophy

The strict code quality requirements (cargo fmt, clippy, test coverage, 1000-line file limits) are **primarily enforced for AI contributors** to maintain consistency and prevent AI-generated code bloat.

**As a human contributor:**
- ✅ Your domain knowledge and musical ideas are valuable
- ✅ It's OK to submit a PR that doesn't pass all checks
- ✅ We understand manual coding is different from AI-generated code
- ⚠️ Your PR might be reformatted by AI tools after merge
- ⚠️ Code may be restructured multiple times as patterns evolve

We'd rather have your contribution with imperfect formatting than no contribution at all. The bots can handle the cleanup.

### Quick Start

```bash
git clone https://github.com/Tok/impulse-instruct.git
cd impulse-instruct
./start.sh        # build + launch with mock LLM (no model needed)
cargo test        # run unit tests
```

### Development Workflow

1. **Create a branch** from `main`:
   ```bash
   git checkout main && git pull
   git checkout -b feature/your-feature-name
   ```

2. **Make your changes** (code, JSON, docs, whatever)

3. **Test if you can:**
   ```bash
   cargo test
   cargo check
   ```

4. **Submit PR** to `main`:
   - Describe what you changed and why
   - Mention any tests you ran
   - Note any checks that fail - we'll handle it

### Commit Message Format

```
type: brief description
```

**Types:** `feat`, `fix`, `docs`, `refactor`, `test`, `chore`

---

## 🎛️ For Style & Prompt Contributors (No Coding Required)

This is the area where **domain knowledge matters most** and technical skill is almost entirely optional. If you know electronic music production - genre history, synthesis techniques, rhythm patterns, scene culture - you can contribute directly to the intelligence that drives PULSE.

### What You Can Contribute

**`styles.json`** - the genre catalog PULSE draws from when you name a style.
**`instructions.json`** - the shortcut library for one-liner prompts ("make it darker", "remove the hihat").
**`config.json`** - the startup prompts that greet PULSE when it loads.

No Rust knowledge needed. No build tools required beyond a text editor and `cargo run` to verify.

---

### Adding or Improving a Style (`styles.json`)

Each style entry looks like this:

```json
{
  "id": "acid_classic",
  "name": "Acid Classic",
  "keywords": ["acid", "303", "roland", "chicago", "tb-303", "squelch"],
  "bpm_range": [120, 145],
  "brief": "Chicago acid house: 303 squelch, four-on-the-floor, snappy 808 snare.",
  "description": "Classic Chicago acid house from the late 1980s. The TB-303 ...",
  "seed_patterns": {
    "kick": [1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0],
    "snare": [0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0],
    "hihat": [1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0],
    "bass_steps": [1, 0, 0, 1, 0, 0, 1, 0, 0, 0, 1, 0, 0, 1, 0, 0],
    "bass_notes": [36, 0, 0, 43, 0, 0, 36, 0, 0, 0, 48, 0, 0, 43, 0, 0]
  },
  "suggested_root": "A",
  "suggested_scale": "minor"
}
```

**Fields:**

| Field | What it does |
|-------|-------------|
| `id` | Unique snake_case identifier - used in code, keep it stable |
| `name` | Human-readable display name |
| `keywords` | Words that trigger this style when typed in the prompt box |
| `bpm_range` | `[min, max]` BPM the LLM targets for this genre |
| `brief` | One-sentence description injected into shorter prompts |
| `description` | Full paragraph injected when the LLM needs more context - be specific: mention hardware, era, key artists (without naming them directly in output), synthesis techniques |
| `seed_patterns` | 16-step arrays that prime the sequencer - `1`=on, `0`=off; `bass_notes` are MIDI note numbers (0=rest) |
| `suggested_root` | Key root: `"A"` through `"G"`, with `#`/`b` if needed |
| `suggested_scale` | `"minor"`, `"major"`, `"pentatonic"`, `"dorian"`, `"phrygian"`, etc. |

**Tips for great style entries:**

- **Multiple setups for the same style are welcome.** Use suffixed IDs like `dub_techno_sparse`, `dub_techno_hypnotic`, `jungle_ragga`, `jungle_instrumental` - PULSE will pick based on keywords or the user can name them explicitly.
- **Be specific in `description`** - mention the hardware (TR-808, TB-303, Juno-106), the era, the geography, the tempo feel, and the filter/FX character. The LLM reads this to understand what "sounds right".
- **Seed patterns prime the feel** but the LLM overwrites them - think of them as "first impression" defaults, not locked values.
- **Keywords drive matching** - include slang, sub-genres, gear names, and artist-adjacent terms that a user might type.

**Styles we'd especially love:**

- Reggaeton / dembow
- UK drill
- Afrobeats / Afroswing
- Footwork / juke
- Kuduro
- Grime
- Cumbia digital / chicha
- Nigerian Alté
- Frenchcore
- Psytrance (full-on, darkpsy, prog)
- New beat
- EBM / industrial
- Italo disco
- Hi-NRG
- Rave sub-genres: makina, happy hardcore, bouncy techno

---

### Adding or Improving an Instruction (`instructions.json`)

Instructions are shortcut templates - when the user types something that matches the keywords, the app applies the `params` JSON directly without calling the LLM. Fast, deterministic, and offline-capable.

```json
{
  "id": "add_reverb",
  "keywords": ["reverb", "wet", "space", "roomy", "echo", "reverb up"],
  "comment": "Push reverb mix up",
  "params": {
    "fx": {
      "reverb_mix": 0.6,
      "reverb_size": 0.75
    }
  }
}
```

**Tips:**

- `keywords` are matched by score - multi-word phrases score higher than single words, so `"more reverb"` outweighs just `"reverb"`. Include natural-language variations of the same intent.
- `params` uses the same JSON path structure as the HTTP API (`/api/params`). Check `GET /api/schema` for the full list of paths.
- Instructions complement the LLM - they handle the fast, unambiguous cases ("remove kick", "clean FX") so the LLM can focus on creative decisions.
- **Common gaps:** genre-specific FX presets ("dub echo", "gabber distortion"), performance shortcuts ("full volume", "cut all FX"), drum variation shortcuts.

---

### Testing Your JSON Changes

You don't need to write a test. Just:

1. Run `./start.sh` (or `cargo run`)
2. Type a prompt that should trigger your style or instruction
3. Verify PULSE responds the way you'd expect

If your entry causes a parse error, `cargo run` will log it clearly at startup.

To check the full schema of accepted params:

```bash
cargo run &
curl http://localhost:8765/api/schema | python3 -m json.tool | less
```

---

## 🤖 For AI Contributors (Claude, GPT, etc.)

**This section is for AI coding assistants and autonomous agents.**

### Required Reading

All AI contributors MUST read and follow **[CLAUDE.md](CLAUDE.md)** for:
- Functional programming requirements (pure functions, immutable state transitions)
- Audio callback constraints (zero allocations, no locks)
- Testing requirements (every pure function gets a test)
- 1000-line file limit per test submodule
- Commit message format

### Strict Requirements for AI

Unlike human contributors, AI tools are expected to:
- ✅ **Format all code** with `cargo fmt` before committing
- ✅ **Pass `cargo clippy`** - no warnings allowed
- ✅ **Pass all unit tests** - `cargo test`
- ✅ **Add tests** for every new pure function
- ✅ **Respect the 1000-line limit** per test file - split into a new submodule if approaching it
- ✅ **Never allocate inside `process_block()`** - the audio callback is allocation-free
- ✅ **Never lock `AppState` from the audio thread**
- ✅ **Keep state transitions as pure functions** - no `&mut AppState` methods
- ✅ **Run the pre-commit hook** which enforces all of the above automatically

### Why Stricter for AI?

AI can generate perfectly formatted code consistently, write comprehensive tests automatically, and follow complex rules without cognitive load. Humans shouldn't be held to the same standard because manual coding has different constraints.

### AI Workflow

1. **Read CLAUDE.md** thoroughly before starting
2. **Check git status** to understand current state
3. **Run tests** before making changes: `cargo test`
4. **Make changes** following all quality rules
5. **Run pre-commit checks** (the hook runs automatically on `git commit`):
   ```bash
   cargo fmt && cargo clippy && cargo test
   ```
6. **Verify all checks pass** before committing

### AI Pre-Commit Checklist

Before every commit, verify:
- [ ] `cargo fmt` applied to all changed files
- [ ] `cargo clippy` reports 0 warnings
- [ ] `cargo test` passes
- [ ] New pure functions have corresponding tests in `src/tests/`
- [ ] No test file exceeds 1000 lines
- [ ] No allocations inside `process_block()` or cpal callback
- [ ] State transitions are pure functions (take ownership, return new state)
- [ ] Commit message follows conventional format with `Co-Authored-By` trailer

---

## Development Resources

### Documentation

- **[CLAUDE.md](CLAUDE.md)** - Development guide (AI-focused but useful for all)
- **[PLAN.md](PLAN.md)** - Roadmap and what's built vs. what's left

### Running Tests

```bash
cargo test                           # unit tests
./scripts/run-tests.sh --coverage    # HTML coverage report (Linux)
./scripts/run-llm-tests.sh           # LLM integration suite (needs llama-server)
./scripts/run-llm-style.sh           # artist/genre reference tests only
./scripts/run-llm-theory.sh          # music theory + producer lingo tests only
```

### Code Quality

```bash
cargo fmt                            # format
cargo clippy                         # lint
cargo check                          # fast type-check
```

---

## Questions or Issues?

- **Bug reports:** Open an issue with steps to reproduce
- **Style ideas:** Open an issue or PR directly against `styles.json`
- **Feature requests:** Open an issue with a use-case description
- **Not sure if it's a bug?** Open an issue anyway

**Thank you for contributing to Impulse Instruct!**

Whether you're a music producer tweaking JSON, a synth nerd fixing a DSP bug, or an AI assistant following the rules - your contribution makes PULSE smarter and more musical. 🎛️
