# Impulse Instruct - Roadmap

What's already built is documented in [docs/features.md](docs/features.md).
This file lists **future** work only - completed items are removed once
they ship and are reflected in `features.md`.

---

## DSP

- [ ] **Parametric EQ with curve editor** - replace the fixed 3-band EQ
  on the master with a draggable-node curve (4-6 nodes, shelf at the
  ends, bell in the middle).  Keeps the grayscale language; curve
  rendered over the spectrum analyser.
- [ ] **Standalone pitch-shifter FX** - grain-based pitch shift (reuse
  the preserve-pitch stretch code in `samplers.rs`) as a dedicated
  FxStep so lanes can patch it into an arbitrary voice.  Autotune
  already covers the "snap to key" use case; this covers harmonies
  and octave doubles.
- [ ] **Convolution reverb** - load a user-supplied impulse response
  from `samples/impulses/*.wav`.  FFT-based convolution, block size
  1024 for the 48 kHz engine.  Same `FxStep` slot shape as the
  existing Reverb.
- [ ] **Mid/side master processing** - mid/side split with separate
  width / EQ / saturation per side on the `MasterOutput` module.
  Opens up "wider than stereo" moves that stay mono-sum-safe.
- [ ] **Karplus-Strong plucked string voice** - cheap delay-line
  synthesis for acoustic-ish tones.  New `ModuleKind::PluckString`,
  single oscillator + damping knob + excitation noise burst.
  Fills the "dry melodic" gap between bass and an1x.
- [ ] **Wavetable voice** - single-table scan + pos/phase knobs, load
  user wavetables from `samples/wavetables/*.wav`.  Complements AN1X
  (analog) and Hoover (fixed-shape) with user-extensible character.

## Modulation

- [ ] **Native per-voice `BassLfoTarget::Pan` + phase offset** — the
  Bach demo's anti-phase pan sweep between voice 0 and voice 1
  currently runs as a 10 Hz Python loop over `/api/params`, because
  `BassLfoTarget` (Pitch / PulseWidth / FilterCutoff / Amplitude) has
  no `Pan` variant and `BassVoice` has no phase field.  Add `Pan` to
  the enum, a `lfo_phase: f32` (0..1 → 0..2π) to `BassVoice`, and
  wire both into the DSP — the scenario can then configure the
  anti-phase sweep once and let the audio thread animate it, zero
  HTTP traffic, zero state-lock contention.

## Sequencer

- [ ] **Polymeter** - per-voice step length independent of the global
  step count, so a 5-step bass line loops against a 16-step drum
  pattern for classic cross-rhythms.  Already have `*_len` keys in
  the LLM schema; implement the tick math + UI.
- [ ] **Per-step velocity curves** - promote `accent` from boolean-ish
  (0 / 1) to a real 0..1 scalar with a per-step slider lane in the UI,
  so "quiet-loud-quiet-medium" grooves are expressible without the
  LLM forcing an accent flip.
- [ ] **Conditional triggers** - per-step "fire only every Nth cycle"
  flags (N = 2 / 3 / 4) for Monome-style evolving patterns.  Stored
  as a 2-bit field per step; rendered as a small marker above the
  step pad.
- [ ] **Pattern morphing on chain advance** - smooth crossfade / step-
  by-step pattern swap instead of the current hard cut.  New
  `ChainSlotOverride::morph_bars` field (0 = hard cut, >0 = bars to
  crossfade over).
- [ ] **Rackable viz modules (visualizers as rack modules)** - move
  the header oscillographs / spectrum / event stream into optional
  visualizer rack modules so the user can add / remove / position
  them alongside the synth voices.  Header keeps the ring scope +
  spectrum as always-on "transport indicators"; everything else
  (bar oscilloscope, stereo meter, activity timeline, event stream
  variants) becomes a rackable module with the same cable-fed input
  model as SpectrumAnalyzer / StereoMeter already use.  The existing
  `draw_scope_colored` path in `scope_footer.rs` is kept alive in
  the header as a Preferences-toggleable fallback so it's available
  to wire into a rack module without reimplementing the phosphor
  trail rendering.
- [ ] **Paginated bank selector for >8 banks** - `MAX_BANKS` was
  raised to 64 so MIDI imports of longer pieces (e.g. Bach III at a
  32nd-note grid needs ~48 banks) can chain end-to-end.  The
  A-H card strip in `src/ui/panels/sequencer_header.rs` still
  renders only the first 8; banks I-onward are reachable from the
  chain but invisible in the UI.  Add pagination (‹ › arrows, or a
  page row) so users can see / edit any bank.  Machine-generated
  MIDI imports don't need editing so this is low priority, but
  anything that drives beyond bank H via `bank_write` / `bank_load`
  loses its in-place editing affordance.

## UI / UX

- [ ] **Rack mini-map** - bird's-eye navigator in a corner of the rack
  view showing the full module grid as thumbnails, with a draggable
  viewport rectangle for quick nav on tall racks (many agents + many
  FX).
- [ ] **Undo/redo timeline scrubber** - the undo stack already exists
  internally; surface it as a horizontal scrubber above the log so
  users can A/B compare past states visually instead of mashing Ctrl-Z
  blind.
- [ ] **Per-knob MIDI-learn** - right-click any knob -> "Learn MIDI CC"
  -> user sends a CC -> app binds it.  Stored in `UiPrefs` / session
  so it persists.  Current MIDI path is input-only on the sequencer;
  this extends it to every parameter.
- [ ] **Pattern snapshot slots (A/B/C/D)** - four one-shot slots that
  capture the full sequencer pattern state (all voices + steps +
  notes + accents).  Click to swap between them live.  Keyboard:
  Shift+1 / 2 / 3 / 4.  Great for live performance.
- [ ] **Keyboard shortcut overlay** - F1 (or ?) pops a translucent
  overlay listing every shortcut in groups (sequencer / rack /
  agents).  Reads the actual keybinding map so it can't drift.
- [ ] **LLM writeback diff viewer** - when a pipeline turn applies
  changes, show a collapsible "what changed this turn" panel: per-
  lane before/after values with highlighted deltas.  Helps users
  build intuition for what the LLM actually does per turn.
- [ ] **Automation lane overlay on the sequencer grid** - a toggled
  lane that shows LFO / free-EG / ramp values as a sparkline
  underneath the step grid, so the user sees the modulator's shape
  aligned to the beat grid.
- [ ] **Performance mode** - bigger knobs + touch-friendly targets,
  hide the LLM console and log, expose only the sequencer + master
  section.  Toggle via header button; state saved per-session so
  demos can launch straight into it.

## Intelligence

- [ ] **Test additional LLM models** - evaluate
  DeepSeek-R1-Distill-Qwen-7B / 14B and Qwen3-8B / 14B for JSON
  accuracy and music theory.  Gemma 4 26B-A4B is now downloadable
  (three quants); needs a head-to-head vs. E4B on the style + bass +
  theory suites.
- [ ] **Lane-score auto-tuner** - observe lane-score trends per style +
  persona combination, then nudge the planner's heuristic weighting
  toward the lanes that score higher.  Keeps the planner learning
  without retraining the model.
- [ ] **Per-lane few-shot example bank** - an editable JSON file of
  `{ prompt, output }` pairs per LaneKind that the pipeline injects
  into the relevant lane's prompt as in-context examples.  Lets the
  user steer a lane's style without touching the system prompt.
- [ ] **Agent personality evolution** - let `style_observations`
  trickle into the agent's system prompt over time (cap at N
  observations) so long-running agents develop a "feel" for what the
  user likes without needing explicit instruction edits.

## Integration

- [ ] **OSC API mirror** - port the HTTP API to an OSC server so
  TouchOSC / external controllers can drive the same endpoints
  without needing to speak HTTP.  Reuse the request-type structs
  (`/api/prompt`, `/api/params`, etc.) as the OSC address space.
- [ ] **Ableton Link tempo sync** - bidirectional BPM + bar-phase
  sync via the `ableton_link` crate.  Useful for jamming alongside
  Live / Ableton Push or another synth setup.
- [ ] **Recording -> auto-chop -> AmenSampler** - one-click record a
  loop from the app's own master bus, run `detect_onsets`, load it
  straight into AmenSampler with auto slice positions.  Lets the
  user sample their own jam back into the break rotation.
- [ ] **WebSocket state push** - mirror `/api/state` over a WebSocket
  so external web dashboards or the live-coding editor can observe
  changes without polling.
- [ ] **MPE / MIDI 2.0 input** - the `midir` path only takes Note
  On/Off + CC today.  Adding per-note pitch / pressure / slide lets
  the bass voice become a proper MPE instrument instead of the
  current monophonic step driver.

## Agent tooling - gradual control & expressiveness

- [ ] **Cross-agent broadcast hints** - `send_hint` already exists for
  single-target agent-to-agent messaging; add a broadcast variant that
  fans a hint out to every agent matching a scope string (`"bass"` ->
  every enabled bass agent).  Useful for "everyone go half-time for
  the next 8 bars" one-shots.
- [ ] **Persona library** - save / load named agent configurations
  (persona + instructions + prompt override + conv mode + temp).
  Ships with a handful of curated personas; user can stamp their own.
  Loaded from `~/.impulse_instruct/personas/*.json` so they survive
  session reloads.
- [ ] **Auto-retry with temperature bump** - when a lane's JSON fails
  to parse after repair, retry the same lane once with temperature +
  0.1 before falling through to the `default_plan` fallback.  Should
  reduce the "model got stuck on one lane so the whole turn stalled"
  failure mode.
- [ ] **Per-agent token-budget tracking** - carry prompt / completion
  tokens per cycle on `LlmAgentState` and surface the running total +
  per-cycle average on the agent card.  Lets the user see which
  agents are dominating VRAM / throughput.
- [ ] **Agent sleep mode** - an explicit "sleeping" state that unloads
  the agent's server process (or demotes it to a shared pool slot)
  until heat rises or the round-robin reaches it.  Saves VRAM for
  specialists that only need to fire occasionally.

## Refactoring

- [ ] **Glass group helpers** - `glass_label(ui, text)` still to do
  (the inline pattern varies too much across panels for a single
  helper).
- [ ] **Large-file splits (remaining)** - top remaining file now is
  `src/llm/pipeline.rs` (938 lines), followed by `src/llm/mod.rs`
  (914) and `src/state/rack.rs` (897).  None over cap, but worth
  watching on the next feature round.

## Demo recording

- [ ] **Next acid demo re-record** - showcase the **two bass voices**
  (V1 + V2 playing complementary lines), plus FX routes that last
  session's demo didn't cover (delay/phaser/chorus/ringmod).  Use the
  bigger NeuTTS quant for the MC/vocal line.  **Bonsai references
  removed** from the demo script (module no longer in the codebase).
- [ ] **`demo/scenarios/setup-mc-singer.sh`** - Jungle MC + TTS
  Singer through autotune.  Non-deterministic, 100 % agent-controlled.
- [ ] **Preecho demo scene** - agent writes anchors into a drum
  pattern and you hear the build-up ramp into each downbeat.
- [ ] **LFO assignment scene** - agent schedules filter sweep via
  the per-voice bass LFO.
- [ ] **Parameter ramp scene** - gradual cutoff sweep over bars.
- [ ] **Event stream scene** - Huth-coloured note history scrolling
  in real time, with the new past-side log preserving past notes.
- [ ] **Re-record the D&B demo** - amen + reese + drone pad + MC
  scenario is ready; waiting on a clean recording run.

---

## Known issues

| Issue | Cause | Status |
|-------|-------|--------|
| Hoover doesn't sound like a hoover | DSP tuning, not a code bug | Needs filter sweep shape tuning |
