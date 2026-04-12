# Impulse Instruct - Roadmap

What's already built is documented in [docs/features.md](docs/features.md).

---


## v0.7.4 — next release

### TOP PRIORITY — start with a refactoring session

Before adding new features, spend a focused session on file splits and
test coverage for logic added late in 0.7.3. Ship foundation first.

- [ ] **Split `state/rack.rs`** (currently 999 LOC, AT the 1000 limit) —
  extract pure layout logic into `state/rack_layout.rs`:
  - `arrange_grid()` + the order() type-priority map
  - `find_free_position()` + occupancy scan
  - center-bias pass
  - `strip_audio_cycles()` if it fits cleanly
  Pure layout code, no behavior change, unblocks any further edit on rack.
- [ ] **Split `state/mod.rs`** (971 LOC) — big struct defs and default
  helpers dominate; move defaults into `state/defaults.rs`. The old
  memory-flagged "AT 1000" priority.
- [ ] **Regression tests for the scope bug fixed in 5ec3bb2**:
    - `bass_scope_writes_bass_steps_and_notes`
    - `bass_scope_cannot_write_bpm_or_swing`
    - `kit_a_scope_cannot_touch_kit_b_patterns`
    - `kit_b_scope_cannot_touch_kit_a_patterns`
    - `drum_lengths_respects_kit_scope_per_key`
    - `hoover_scope_can_write_hoover_len`
    - `sequencer_scope_grants_all_fields_backwards_compat`
    - `empty_scope_grants_everything`
    These lock in the per-voice scope dispatch we just added — the bug
    was silent for multiple releases, don't let it regress.
- [ ] **Layout regression tests for the rack-centering fix in 781a736**:
    - `arrange_grid_places_303_between_drums` — verify AcidBass lands
      in grid_col between DrumKit808 and DrumKit909 when all three
      present
    - `add_module_stacks_without_overlap` — occupancy grid correctness
    - `center_bias_shifts_row_bands_toward_center`
- [ ] **Extract pure helpers in `llm_apply.rs` (744 LOC)** — the per-voice
  dispatch I added this session is ripe for helper extraction (e.g.
  `apply_bass_sequencer_fields(s, seq, locked) -> AppState`). Each
  helper gets its own test.

### Agent tooling — gradual control & expressiveness

- [ ] **LFO-as-tool for agents** — agents can schedule an LFO on any target
  (cutoff, resonance, reverb mix, etc.) to introduce changes gradually
  instead of jumping to a value. JSON schema: `"lfo_assign": {"target": "bass.cutoff", "rate": 0.5, "depth": 0.3}`
- [ ] **XY pad control** — expose cutoff/resonance pad as a first-class
  tool the agent can move. Currently agents set values but the pad
  position doesn't visually track mid-change
- [ ] **ADSR envelope shaping** — expose attack/decay/sustain/release as
  agent-controllable parameters with the same ramp/LFO tooling
- [ ] **Pan as LFO/ramp target** — all voice pan parameters must be
  addressable as LFO targets and ramp targets. Add `LfoTarget::Pan`
  variants and ensure `apply_llm_update` handles `"bass.pan"`,
  `"kit_a.kick.pan"`, etc.

### DSP

- [ ] **Gabber kick voice** — dedicated voice (not just preset on 808 kick);
  extreme pitch envelope, hard clipper, layered transient
- [ ] **Pan FX module** — insertable rack module for per-chain stereo placement.
  Knobs: pan position, width, auto-pan rate (LFO)
- [ ] **Pan in sequencer** — per-step pan value for bass voice (like velocity but L/R)
- [ ] **LFO target: StereoWidth** — modulate stereo width over time (auto-pan)
- [ ] **Per-voice FX sends** — route individual voices to specific FX modules
  via rack cables (data model exists, DSP routing partially wired)
- [ ] **Dub techno send/return** — dedicated send/return FX workflow for
  dub-style infinite delay feedback chains

### Sequencer

- [ ] **Pattern probability per step** — already implemented but LLM doesn't
  use it well; improve prompt guidance for probability-based patterns
- [ ] **Song mode** — chain patterns with per-chain tempo/style transitions
- [ ] **MIDI export** — export sequencer pattern as .mid file

### Intelligence

- [ ] **Total smart randomization** — one-click random setup: pick a random
  style, add appropriate instruments, set random (but musically coherent)
  parameters, generate a pattern. API: `POST /api/randomize`.
- [ ] **Agent conversation history** — multi-turn within a single jam cycle;
  agent sees its own previous outputs for coherent evolution
- [ ] **Prompt templates per style** — styles can define custom prompt
  templates that replace the generic "generate all parameters" jam prompt
- [ ] **VRAM-aware model fallback** — when spawn is rejected, auto-suggest or
  auto-select a lighter model that fits the remaining VRAM budget
- [ ] **Test additional LLM models** — evaluate DeepSeek-R1-Distill-Qwen-7B/14B
  and Qwen3-8B/14B for JSON accuracy and music theory understanding.
  Download scripts exist but models are not yet tuned or integration-tested.
- [ ] **Jam-via-API** — currently API prompts are always one_shot (no jam loop).
  Need safe jam support that doesn't do full-state replacement.
- [ ] **Style mc_lines/themes UI editor** — allow editing mc_lines and themes
  per style from the UI preferences
- [ ] **Style → rack auto-setup** — add `rack_modules` field to `styles.json`
  entries, listing which modules to add and how to wire them when a style
  is selected.
- [ ] **Style-aware agent-preset naming** — the default multi-agent setup
  is called "Crew" generically. Styles should be able to override the
  preset display name in `styles.json` (`agent_preset_label` or similar)
  so e.g. disco/jazz presents it as "Band", hip-hop as "Posse", techno
  as "Squad", ambient as "Ensemble". Purely a labelling concern — the
  underlying preset stays the same.

### TTS

- [ ] **Agent self-add TTS module** — if an agent is in MC/DJ mode and has no
  TTS module connected, it can add one to the rack and wire itself to it

### UI / UX

- [ ] **Quick-command buttons on LLM agent card** — one-click shortcuts for
  common re-prompts so users don't have to retype. Small row of pill
  buttons on each agent card that POST the mapped prompt to the agent's
  scope. Starter set:
    - *Rewrite melody* → "rewrite the bass/lead melody, keep the rhythm"
    - *Rewrite rhythm* → "rewrite the step pattern, keep the notes"
    - *Rewrite both* → "rewrite rhythm and melody from scratch"
    - *Variation* → "subtle variation of the current pattern"
    - *Fill* → "add a fill in the last bar"
    - *Sparser* / *Busier* → density tweaks
    - *Brighter* / *Darker* → timbre tweaks (cutoff / reverb)
    - *Swap style* → opens the style picker
  Buttons should respect the agent's scope (only the BASS agent's
  *Rewrite melody* touches bass) and be configurable per-persona in
  future. Consider a compact ⋯ menu for less-common commands.
- [ ] **Rack CV cables driving LFO targets** — cables are visual only; wiring
  them to actually change LFO target at DSP level
- [ ] **Touch mode improvements** — touch-paint mode for mobile/tablet;
  gesture support for zoom/scroll

### Demo recording

- [ ] **`demo/scenarios/setup-mc-singer.sh`** — Jungle MC + TTS Singer through
  autotune. Non-deterministic, 100% agent-controlled.
- [ ] **ADSR scene** — agent shaping bass envelope (attack/decay)
- [ ] **LFO assignment scene** — agent schedules filter sweep via LFO
- [ ] **Parameter ramp scene** — gradual cutoff sweep over bars
- [ ] **Event stream scene** — Huth-colored note history scrolling in real time

### Refactoring

- [ ] **Panel typography constants** — define `FONT_XS`/`FONT_SM`/`FONT_MD`/`FONT_LG`
- [ ] **Panel spacing constants** — define `SPACING_SM`/`SPACING_MD`/`SPACING_LG`
- [ ] **Glass group helpers** — `glass_label(ui, text)`, `glass_group_height(ctrl)`
- [ ] **Module card constants** — `TITLE_BAR_H`, `CARD_ROUNDING`, `GLASS_ROUNDING`

### Infrastructure

- [ ] **CI: run LLM integration tests on release** — currently manual;
  automate in GitHub Actions with a Gemma model cache
- [ ] **Codecov improvement** — currently ~37%; target higher with new suites

---

## Known issues

| Issue | Cause | Status |
|-------|-------|--------|
| Wizard always shows on startup | By design — resume or start fresh | Working as intended |
| Hoover doesn't sound like a hoover | DSP tuning, not a code bug | Needs filter sweep shape tuning |
