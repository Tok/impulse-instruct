# Impulse Instruct - Implemented Features

A detailed log of what's built.

---

## v0.7.7-snapshot — model overhaul, jam loop, cycle viz, lane scoring

### Model lineup

- **Bonsai 8B + PrismML llama.cpp fork removed** — accuracy gap no longer
  worth the extra server binary, model download, and dual-fork branching.
  Pool now uses a single `.llama-official-build/bin/llama-server`.  Swarm/
  Crew/Voices presets converted to all-Gemma (same model, ref-counted, so
  a 5-agent Crew costs the same ~6 GB VRAM as Solo); Lite preset deleted.
- **Gemma 4 26B-A4B added as opt-in** — MoE (4B active / 26B total), same
  speed as E4B but much more knowledge.  Three quants exposed via
  `download-models.sh`: `gemma-26b` (UD-IQ4_XS, ~13.4 GB), `gemma-26b-q3`
  (UD-Q3_K_M, ~12.5 GB), `gemma-26b-iq2` (UD-IQ2_XXS, ~9.9 GB).  Quant-
  aware `ModelProfile` entries so the wizard estimates VRAM correctly.
  E4B remains the install default — it's the "works on any 6 GB GPU" floor.
- **NeuTTS Air Q8 is the new default TTS** — `./download-models.sh neutts`
  fetches Q8 (~803 MB) instead of Q4.  `neutts-server.py` searches Q8 first
  then Q4 so existing installs keep working.  `neutts-q4` alias still
  available.  Header comments on both `download-models.{sh,bat}` document
  the Python + espeak-ng host deps and link to the unsloth/Neuphonic HF
  repos for "drop a custom GGUF in models/" exploration.

### Model-switching infrastructure

- **Plugged the pool ref-count leak** — every `pool.acquire()` in the
  inference path now has a paired `pool.release()` at the tail of both
  pipeline + monolithic branches.  Servers actually unload at ref_count=0
  now; previous behaviour was monotonic growth.
- **Console = master switch** — `LlmInput::SwitchModel` resets every
  agent override to `None` and shuts down every server except the new
  global via `LlamaServerPool::shutdown_all_except`.  One canonical model
  by default; agents can re-add overrides via their dropdown.
- **`LlmInput::SwitchAgentModel { agent_id, old_path, new_path }`** —
  agent dropdown change carries the previous override so the LLM thread
  can update pool ref counts even after the UI optimistically wrote the
  new value to state.  Same instant-feedback pattern as the console.
- **Optimistic UI for both dropdowns** — console + per-agent dropdown
  clicks update state immediately, so the UI reflects the choice this
  frame instead of waiting for the LLM thread to drain its queue (could
  be 30+ s during a long pipeline turn).
- **Model picks persist** — autosave dirty-detection now hashes
  `state.llm.model_path` plus every `agent.model_path`; any change flips
  `session_dirty` so the existing session.json autosave catches model
  picks too (the rack-signature alone missed them).  Channel `bounded(16)`
  → `unbounded()` so model loads can't drop user prompts.
- **Wizard preset agents inherit the user's global model** — when a
  preset's `model_pattern` matches the current global, agents stay on
  `model_path = None` instead of getting pinned to the first alphabetical
  Gemma file via `find_model("gemma", ...)` (which used to silently load
  IQ2 alongside E4B and OOM the GPU).

### Jam loop

- **Heartbeat kickoff** — when `heat > 0` and the loop is dormant (no
  in-flight inference, no scheduled fire, not initialising), the UI fires
  one Infer to spark it.  500 ms cooldown stops re-fires while the LLM
  thread picks the message up.  Self-perpetuating from then via the
  existing `[jam_cycle_done]` re-fire; previously a fresh start with
  `heat > 0` sat silent until the user typed a prompt.
- **Stopped silently dropping commands** — input channel `bounded(16)`
  → `unbounded()`; removed a destructive `let _ = input_rx.try_recv();`
  in the monolithic jam path that consumed whichever message happened to
  be queued (incl. user prompts and SwitchAgentModel control messages).
- **Pipeline no longer overwrites the rack** — per-lane writeback was
  doing `s.rack = snapshot.rack.clone()`, silently restoring the
  pre-style-switch rack mid-pipeline.  Dropped that line — voice/FX
  lanes have no business mutating the rack.

### Per-lane lifecycle scoring (Phase 1)

- `state::LaneScore { score, last_changed_cycle, change_count }` keyed
  by `LaneKind::label()`, transient on `LlmState`.
- `llm/lane_eval.rs` — pure per-lane scoring functions:
    - bass: density (3–7 ideal) + variety + accent ratio + slide ratio
    - kits: full-coverage rule (kick + hat) + density bands
    - amen: presence + reasonable hit count
    - hoover / an1x: density + variety
    - fx: not all-zero / not all-one + mid-band knob ratio
    - settings: bpm + swing in plausible range
- Hook in `pipeline_events::handle_pipeline_event` LaneApplied: scores
  the apply against the rules we encode in the system prompt and stashes
  the score on `LlmState.lane_scores` (logged as `lane_eval: bass1 → 0.72`).
  Phase 1 is read-only; the weighted scheduler below consumes these.

### Weighted single-lane jam scheduler (Phase 2)

- `llm/lane_scheduler.rs` — weighted pick formula
  `weight = dynamism(lane) * (1 - score) * recency_decay * heat_jitter`.
  `lane_dynamism` bakes genre-neutral defaults (bass/kits high, settings
  low, rack always 0 — never scheduler-picked).  `recency_decay` is
  `1 - 1/(1+gap)` on `jam_cycle_count - last_changed_cycle`, so a
  just-fired lane zeros out until the next cycle.  `heat_jitter` adds a
  heat-scaled multiplier (0 at heat=0, up to ×1.6 at heat=1).
- `planner::jam_plan` assembles every live lane as a candidate, passes
  them to `pick_jam_lane`, wraps the result in a single-lane `LanePlan`
  (or empty → caller falls back to `default_plan`).
- `pipeline::run_pipeline` gained an `is_jam: bool` parameter; jam cycles
  (one_shot=false) go through `jam_plan` instead of the planner/default
  chain, so each cycle rewrites exactly one voice/kit rather than the
  whole rack.  High-scoring lanes "live longer" between rewrites; low
  scorers naturally surface for retry without a separate queue.
- Tiny no-deps `Xorshift32` seeded from wall-clock nanos — good enough
  for weighted sampling over a handful of lanes, deterministic under a
  fixed seed so the 21 scheduler tests pin every decision.

### Lane-score strip in the LLM console

- `ui/widgets/lane_scores.rs` — compact horizontal cell strip drawn
  directly under the cycle viz.  One cell per live lane on the rack
  (Settings + active bass voices + present kits + FX, in `default_plan`
  order), each showing the lane label, latest `lane_eval` score (two
  decimals), and a mini fill-bar.  Cells are fixed once the rack is
  wired, so new scores overwrite values in place rather than reflowing
  the widget each pipeline tick.
- The currently-inferring lane pulses with a grayscale ring so the strip
  mirrors the cycle viz's "this lane is working" cue.
- Hover any cell for a tooltip with the raw score, `change_count`, and
  "N cycles ago" bookkeeping from `LlmState.lane_scores` — useful for
  debugging why the Phase 2 scheduler picked (or skipped) a lane.
- Reserved 26 px strip; the cycle viz shrinks to match so the right
  panel layout (model bar, prompt, log) stays unchanged.

### Jam-via-API

- `POST /api/prompt` now honours a `"one_shot": false` field; the
  handler plumbs it through to `LlmInput::Infer` instead of hardcoding
  one-shot mode.  Default stays `true` so existing clients keep getting
  single-turn behaviour.
- With `one_shot: false`, the LLM worker emits `[jam_cycle_done]` after
  the pipeline finishes; the UI's drain picks it up and schedules the
  next agent's turn (requires `llm.heat > 0.0` for re-fire — heat is
  user-owned, so clients must set it via `/api/params` or the slider
  before starting a jam).
- Pipeline writeback is already surgical (the "don't clobber user-owned
  rack" guard landed earlier), so jam-via-API inherits that safety: no
  full-state replacement, rack / ui_prefs / llm_agents untouched.
- Log line now tags mode: `[API] prompt (jam) → BASS: …` vs
  `(one-shot)` for quick tailing.
- 4 new serde tests pin the default + field parsing.

### Per-agent cycle clock on agent cards

- `ui/widgets/agent_clock.rs` — 26 px mini clock-face living on each
  `LlmAgent` card.  Same grayscale language as the big `llm_cycle` in
  the LLM console: recessed screen bezel, 12-o'clock turn tick, a
  progress arc drawn from the agent's own `pipeline_progress` fraction,
  a pulse ring that animates independently of the arc so slow lanes
  still read as "alive", and a small outward wedge at 12 o'clock when
  this specific agent is the jam loop's next scheduled fire.
- Centre text cascades by signal strength: countdown `Ns` when
  scheduled next → `t/s` during inference on wider cells → `▶` glyph
  during inference on narrow cells → `#N` cycle count at rest →
  `·` idle.
- Replaces the previous pair of linear progress bars — the clock is
  the single per-agent status glyph now.  The "{done}/{total} lane"
  sub-label stays underneath for users who want the exact lane name.
- Ties into the big LLM-console cycle viz: the console shows
  round-robin context (which agent is about to fire next), each card's
  clock shows that agent's own work.  Between them, jam state is
  legible without hunting through the log.

### Melodic voice preecho (bass)

- `PreechoConfig` gained two melodic flags: `accent_ramp` and
  `slide_cascade`.  Drum preecho (`velocity_ramp` + `ratchet_ramp`)
  keeps its semantics; these are the TB-303 counterparts.
- `preecho_melodic(step, total_steps, cfg) -> (Option<f32>,
  Option<f32>)` is the pure core: returns `Some(accent_override)` on
  lead-in steps (linear ramp 0.3 → 1.0 from earliest to
  anchor-adjacent) and `Some(1.0)` for `slide` on the step
  immediately before an anchor (`d == 1`).  Anchor steps and
  non-lead-in steps return `(None, None)` so the user's stored
  accents/slides shine through.
- `sequencer::advance_clock` looks up `seq.preecho.get("bass")` per
  bass voice and applies the overrides before emitting
  `BassTrigger`.  The shared `"bass"` key covers every voice 0..3;
  per-voice keys aren't needed until the LLM starts wanting that
  level of control.
- `apply_preecho_voices` accepts the two new bools; partial updates
  (e.g. setting only `length`) preserve them.  `Bass` lane's
  `sequencer_subkeys` now includes `"preecho"` so pipeline filtering
  doesn't strip a bass-lane preecho update.
- Hoover/An1x are single-voice TB-303-step users too, but their
  `TriggerEvent` variants carry only `note`; pushing accent/slide
  through them is a structural change left as a follow-up (noted in
  `PLAN.md`).
- 11 new tests: 8 unit tests on `preecho_melodic` (wrap-around,
  multi-anchor nearest wins, both toggles composing), 1 end-to-end
  sequencer test confirming the ramp lands on `BassTrigger.accent`
  + cascade lands on `BassTrigger.slide`, 2 apply-layer tests for
  bass-key JSON + partial-update preservation.

### Per-slice amen reverse UI

- `draw_slice_reverse_strip` in `panels/amen.rs` — a per-slice direction
  row laid out just under the slice-order strip.  Each cell shows `→`
  forward or `←` reverse, tinted the same way as the order strip
  (active-slice highlight while the playhead sits on it).
- When `AmenState.slice_reverses` is empty, every cell shows the global
  `reverse` flag with a slightly dimmer glyph — "inherits global".  The
  first click on any cell populates the vec with the current global
  direction, then flips that slice; subsequent clicks are simple in-place
  flips.  A `RESET` button clears the vec back to inherit-global mode.
- Slice-count changes auto-resize the vec: clicking on a slice that
  didn't exist when the vec was first populated pads up to the new count
  with the current global direction before flipping.
- Ties into the state/DSP/params work that landed in 29b1ac2 — users
  can now drive the glitch-chop feature entirely from the panel without
  touching the API or LLM JSON.

### Per-slice amen reverse

- `AmenState.slice_reverses: Vec<bool>` — parallel to `slice_pitches` /
  `slice_volumes`.  Empty (default) → every slice inherits the global
  `reverse` flag (fully backwards-compatible).  Populated → entry N
  forces slice N's direction (`true` = reverse, `false` = forward),
  unused trailing slots fall back to global.
- `AudioParams.amen_slice_reverses: [i8; 16]` encodes the Vec with a
  `-1` sentinel for "inherit global"; `0` = forward, `1` = reverse.
  The DSP trigger consults this slot before falling back to the global
  flag, so specific slices can glitch backwards while the rest of the
  break plays forward — classic edit-era chop patterns.
- `apply_llm_update` takes `{"amen": {"slice_reverses": [true, false,
  ...]}}` (bools or 0/1 integers tolerated), `null` clears, truncates
  at 16.  Honours the `amen.slice_reverses` lock path.
- Backend-only for now — exposed via state + DSP + LLM apply + API; UI
  toggles on the Amen panel are listed as a follow-up in `PLAN.md`.
- 10 new tests pin the DSP per-slice override path (both directions +
  sentinel), the apply-layer bool/int/null handling, lock preservation,
  16-entry truncation, and the params i8 encoding.

### Lane fade-in ramp

- Phase 2 cycles only replace one voice at a time, which made pattern
  snaps much more noticeable.  `state::jam_tools::schedule_lane_fade_in`
  now dips the applied voice's volume to `LANE_FADE_FLOOR` (15 %) of
  its current value and schedules a bar-based `ParamRamp` back to
  target over `LANE_FADE_STEPS` (16 steps ≈ 1 bar in 4/4).
- Hooked into `pipeline_events::handle_pipeline_event` on `LaneApplied`;
  writes only `llm.active_ramps` to the shared state so it can't
  trample the voice fields the pipeline just landed.
- Single-voice lanes only: `Bass(0..3)`, `Hoover`, `An1x`, `Amen`.
  Kits (per-drum volumes, no master), FX, Settings, Modulation, and
  Rack no-op by design.  Voices under 0.02 volume or with a locked
  volume lock-path also no-op.
- `apply_param_by_path` gained a third-level `bass_voices.N.volume`
  branch so voices 1-3 reach the apply layer with the right nested
  JSON (`{"bass_voices": [null, ..., {"volume": v}]}`).
- Existing `ui_helpers::tick_ramps` already fires `push_audio_params`
  when ramps are active, so the fade actually reaches the audio thread
  without any new wiring.
- 8 new jam_tools tests pin paths, lock/silence no-ops, dedup on
  repeat apply, and an end-to-end mid-ramp voice-2 volume check.

### Retry-on-low-score queue (Phase 3)

- `LlmState.retry_queue: VecDeque<String>` — lane labels whose last
  `evaluate_lane` score came in at or below `RETRY_THRESHOLD` (0.3).
- `lane_eval::record_lane_score` enqueues on a bad score, deduping
  against any entry already in the queue so the "fresh failures first"
  order is preserved.  Queue capped at `RETRY_QUEUE_MAX` (4); overflow
  drops the oldest pending entry so a stuck-in-retry lane can't block
  fresh ones.
- `planner::jam_plan` drains the queue before running the Phase 2
  weighted picker: walks heads until a lane that's still live on the
  rack turns up, returns a single-lane plan with `from_retry: true`,
  and logs `retry_queue: popped bass1 …`.  Dead entries (lane's
  module left the rack since the score fired) are skipped, not
  returned — the rack is authoritative over the queue.
- `pipeline_events::handle_pipeline_event` reads `plan.from_retry` on
  `PlanReady` and calls `consume_retry_prefix_mut` to remove the
  consumed entry (plus any dead heads that were skipped) from the
  shared queue, so the next cycle doesn't re-pick the same lane
  unless it scores low again.
- 9 new Phase-3 tests in `lane_scheduler_tests.rs` pin the threshold,
  dedup, cap, and `jam_plan` retry-first ordering behaviour.

### Per-style lane dynamism overrides (Phase 4)

- `Style.lane_dynamism: HashMap<String, f32>` in `styles.json` — optional
  map overriding `lane_scheduler::baseline_dynamism` per genre.
- Lookup order on each pick: exact label (`"bass1"`) → group label
  (`"bass"`) → baked-in default.  A single `"bass": 0.9` entry covers
  every bass voice; per-voice entries still win over the group.
- `Rack` stays at 0 regardless of style — user-owned composition.
- `pick_jam_lane` resolves the active style via `StyleCatalog::find_by_id`
  and threads it through `compute_weight`; values outside `0..=1` are
  clamped.  Schema is wired and tested (6 new Phase-4 tests); populating
  the per-style maps in `styles.json` is left as a follow-up knob-twist.
- **Defensive plan filter** in `pipeline::run_pipeline` — drops any lane
  whose voice/module isn't currently live before the loop, so a stale
  planner output (e.g. after a mid-cycle style switch) doesn't burn an
  inference call on a no-op lane.

### Style → rack destructive sync

- `ui/style_rack.rs` rewritten to be destructive: voices and FX not in
  the spec are removed, missing ones added, then `arrange_canonical()`
  runs (the same ARRANGE-toolbar pass) so the rack stays compact after
  the churn.  Always-keep chrome (Sequencer / MasterOutput / LlmConsole
  / LlmAgent / NeuTts) is never touched, and the LFO floor is enforced
  (≥ 3 LfoModule instances always present).
- **Count notation** — entries support a trailing-digit count: `"bass2"`
  enables 2 bass voices via `sequencer.bass_voice_enabled`, `"lfo3"`
  loads 3 LFO modules.  Digits-only aliases (`"808"`, `"909"`) preserved.
  Repeated entries collapse via max-count.
- All 29 styles in `styles.json` now have a `rack_modules` field
  (5 pre-existing entries untouched; 24 added).  `styles.json` reformatted
  so primitive arrays render single-line — file dropped 3578 → 1341 lines.

### LLM console — round-robin cycle viz

- New `widgets::llm_cycle` widget on the LEFT side of the LLM console.
  Cycles → circles, top = 12 o'clock = round-robin start.  Square chip
  matching the ring oscilloscope's geometry (full panel height = same
  width, recessed-screen bezel, `theme::SLATE` / `theme::IRON` guides).
- Each enabled agent occupies one slot on the rim with its persona name
  outside; the inferring agent gets a flat in-screen dot (not a 3-D LED
  — it's "inside" a screen) plus expanding-ring "pings" for visible
  motion frame-to-frame.
- **Pipeline progress arc** sweeps clockwise from the inferring agent's
  slot as `lanes_done / total_lanes` grows, with a soft tween between
  lane completions and a bright tracer dot at the leading edge.
- **Cursor wedge** marks the next slot the round-robin will fire on.
- **Queue shadow** in `ImpulseApp` (UI-side approximation of the LLM
  input channel queue, broken down per-agent + a global bucket).  Pending
  Infer messages render as small dots inside the rim at the target
  agent's slot.  All UI try_send sites now route through a single
  `send_llm_infer` helper that bumps the shadow; transitions of agent
  `is_inferring` from false→true decrement (the LLM thread just popped a
  message off the channel).  Agent transitions handled before global to
  avoid double-decrementing when an agent-bound Infer flips both flags.

### LLM console — pipeline progress bar

- Two stacked horizontal bars under the model row: top = lane-completion
  fraction (gray 140), bottom = error fraction (gray 95, NOT red).
  Persistent (not flashing); fixed-width 100-px label slot to the right
  with `lane name` / `idle` / `done` / `N err` truncated at 14 chars
  with `…`.  Identical-shape mini-bars on each agent card (40×2 each
  with 1 px gap).

### Per-agent pipeline progress

- `LlmAgentState.pipeline_progress: Option<PipelineProgress>` (transient,
  `#[serde(skip)]`).  `pipeline_events::handle_pipeline_event` updates
  both global + per-agent slots when an inference is bound to an agent.
  Each agent card shows its own mini progress bar in the status spot
  during inference, taking precedence over the existing tok/s readout.

### Event stream — jitter, truncation, leaves

- **Playhead jitter eliminated.**  Two compounding bugs: (a) audio
  thread did `global_step_count += 1` per block but `advance_clock` can
  cross multiple step boundaries when block_size approaches step
  duration — fixed by adding the actual delta with `MAX_STEPS` wrap
  arithmetic; (b) `event_stream` used a sign-dependent `if off <
  -WRAP_SLACK { off += span }` fix-up which oscillates near the wrap
  boundary — replaced with `(step_idx - local_pos + WRAP_SLACK)
  .rem_euclid(span) - WRAP_SLACK`, deterministic.
- **Smooth-playhead state-read race fixed.**  `mod.rs` did the step-
  change detection in one state read (setting `last_step_time`); header
  did a SEPARATE state read for the smooth calc.  When the audio thread
  updated `global_step_count` between those, smooth_global jumped back-
  wards by ~1 step then snapped forward.  Snapshot `global_step_count`
  atomically with `last_step_time` into `last_step_global` and derive
  `smooth_global` from that, decoupling the playhead from live state.
- **Past-grid lines no longer disappear early.**  Loop iterated
  `0..(display_steps + 2)`; with `now_x` at 75 % from the left the past
  side needed `[-display_steps * past_frac, +display_steps * future_frac
  + steps]`.  Switched to a negative-to-positive `i` range with
  `rem_euclid` for bar/beat alignment.
- **Now-line moved to the golden-ratio split** (`1/φ ≈ 0.618` from
  the left, was 0.75) so past:future = 1.618:1 — past pane stays
  dominant while future grows from 25 % → 38 %.
- **ADSR envelope "leaf" behind each future note** — Y-symmetric filled
  shape tracing the voice's amp envelope (bass A-S-R, AN1X full ADSR,
  Hoover synthesised from `sweep_time`).  Punchy 303 stabs render as
  tight diamonds; pad-y AN1X notes show elongated leaves.

### LED polish

- **16-ring falloff** (was 8) with reshaped alpha curve so the halo
  fades to translucent quicker.  Lit core stays bright; bloom is gentler
  and stops competing with adjacent panel chrome.
- **Perceptual-luminance compensation** in `theme::led` — high-luminance
  colours (yellow / white / light cyan) get progressively reduced alpha
  above 0.4 luminance, so a yellow halo at the same nominal alpha now
  reads as subtly as a red one.  Floor at 0.45 so even white shows.
- **Module-card LED halo escapes panel border** — clip extended by
  `led_r * 6.0` on sides + down (and 0 px upward to avoid bleeding
  into the global header log scrolling past above), so the bloom
  bleeds into the inter-module gap as intended.  Same draw layer —
  cables / piano / drag previews still cover.
- **Agent-card LED on a foreground layer** — the persona-row indicator
  is painted via `ctx.layer_painter(LayerId::new(Order::Foreground, …))`
  so the persona TextEdit (drawn after) can no longer cover it.

### Misc

- **Per-agent model dropdown** on each agent card writes through the new
  `SwitchAgentModel` message instead of mutating state directly.  No
  more "set model in console, agent silently keeps the previous one."
- **Cycle viz lane-name label fixed-width** so the bar+label combo
  doesn't reflow as the current lane name cycles each pipeline tick.
- **NeuTTS Q8 prefer-then-fall-back** in `neutts-server.py` candidates
  list so existing Q4 installs continue to work without reconfiguration.

---

## v0.7.7-snapshot — lane-pipeline + prompt infrastructure

### Sequential lane pipeline (planner + per-voice calls)

User turns now fan out into a planner call + one focused inference per
voice slice, instead of one monolithic "generate everything" response.
Each lane ships short output (100–400 tokens) under a required-fields
JSON schema, so the model can't skip `bass_accents` / `bass_slides`
and can't truncate its pattern mid-array.

- **`LaneKind` enum** — `Settings / Bass(0..=3) / KitA / KitB / Amen /
  Hoover / An1x / Fx / Modulation / Rack`.  Each lane carries its own
  `output_keys`, `sequencer_subkeys`, `task_description`, and JSON
  schema.  `Bass(idx)` routes voice-0 through legacy `bass_*` fields
  and voices 1..=3 through `bass{N+1}_*` naming.
- **`build_lane_prompt(state, lane)`** — compact focused prompt
  (~1–2 k tokens) with state header, style brief, locked-params list,
  a `HARMONY` block for melodic lanes (key + in-key MIDI palette in
  C2–C3) and the lane's task description with concrete example rhythms.
- **`lane_schema(lane)`** — per-lane JSON Schema.  Required pattern
  arrays use `min_steps_array` (minItems ≥ 2) so grammar-constrained
  generation can't emit `[]`.  `additionalProperties: false` on every
  lane, so the server blocks off-scope fields at the token level.
- **`heuristic_plan(state, prompt)`** — deterministic pre-parser that
  catches narrow single-topic commands without calling the LLM.
  Recognises `"bass2"`, `"BASS 2"`, `"second bass"`, `"bass voice
  two"`, `"1st bass"`, `"bass one"` → specific Bass(idx); `"add
  reverb"` / `"more delay"` → Fx; `"808"` / `"kit a"` / `"kick a"` →
  KitA; `"909"` / `"clap"` → KitB.  Multi-topic or broad prompts fall
  through to the LLM planner.
- **`planner_plan`** — tiny LLM call (50–150 token output) with a
  13-lane enum schema + 7 rules, decides which lanes fire for broader
  prompts.  Bass expansion is enforced in code: any bass-containing
  plan auto-covers every active bass voice (so "change the bass"
  never leaves voice 2 silent).
- **`default_plan(state)`** — deterministic fallback when the planner
  LLM fails / returns empty.  Walks the rack in order `Settings →
  KitA → KitB → Amen → Bass(0..N) → Hoover → An1x → Fx`.
- **`run_pipeline`** — the executor.  For each lane: builds prompt +
  schema, calls `PipelineBackend::infer_lane_json`, filters output to
  the lane's scope, applies to `AppState`, fires an `on_lane_applied`
  callback.  Per-lane failures don't abort the pipeline.  `PoolBackend`
  adapts `LlamaServerPool` into the trait so the real server spawns
  the planner + lane calls on the live model.
- **Per-lane immediate writeback** — `on_lane_applied` in
  `run_llm_loop` commits each lane's changes to the shared
  `Arc<RwLock<AppState>>` the moment it lands.  The audio thread
  hears drums the second the `kit_a` lane finishes, without waiting
  for the bass or FX lanes.  Previously everything switched on at
  the end of the pipeline; now it builds audibly.
- **Jam-loop hand-off** — pipeline emits `[jam_cycle_done]` at the
  end of a non-one-shot turn, so the round-robin auto-jam keeps
  firing at `heat > 0`.
- **Empty-array guard** — when a lane emits a degenerate
  `"bass_steps": []`, the filter drops the field with a warn log so
  the previous pattern survives instead of getting silenced.
- **Style-is-user-owned** — Settings lane has no `settings.style`
  field; planner prompt explicitly forbids lanes that change style.
  User sets the style via the UI, the pipeline respects it.
- **Feature flag** — `LlmState.use_pipeline: bool` (default true).
  Preferences window exposes the toggle; when off the legacy
  monolithic path still runs for debugging.

### Prompt baseline — trim & bass voice expressivity

- **Monolithic prompt trimmed ~56 %** (10.8 K → 4.8 K tokens).  Cut
  MUSIC THEORY REFERENCE (scales/triads — model knows these),
  HEAT-AWARE MUTATION GUIDANCE (18 lines of redundant breakpoints),
  MUSICAL MODERATION prose (→ one-line summary), HOW TO INTERPRET
  INSTRUCTIONS / ACID JAM GUIDANCE lookup tables, WRONG-example
  block, LFO / FREE EG / EUCLIDEAN / RAMPS / FX docs (all
  condensed).  Themes / mc_lines omitted in producer mode.
  `current_json` state block minified (`to_string` not
  `to_string_pretty`).
- **Per-voice bass step arrays** — `bass2_steps/notes/accents/slides/
  pans`, `bass3_*`, `bass4_*`.  Each voice has its own lock path.
  Voice-0 still mirrors the legacy unnumbered keys + `bass_pattern`.
- **Proportional accent / slide** — `TB303Step.accent` and `.slide`
  are `f32` (0..=1), not `bool`.  DSP scales amp peak 0.8 → 1.0 with
  accent intensity, portamento time with slide intensity.  Event
  stream renders dot size by accent and trail length by slide.
  Schema accepts float arrays or index lists; bool arrays still work
  for backwards compat.  `de_bool_or_f32` serde adapter round-trips
  old project JSON.
- **Grammar-constrained output** — `response_format.type =
  "json_schema"` sent on every lane call, so llama.cpp compiles the
  lane schema into a GBNF grammar and enforces required fields at
  the token level.

### LLM infra

- **Context default 32 K → 64 K** — Gemma 4 E4B (128 K native) handles
  64 K comfortably.  Test harness matches.  ~11 K-token system prompt plus
  headroom for memory / style observations / multi-turn growth.
- **Prompt-prefix cache reuse** — `--cache-reuse 256` on server spawn
  + `cache_prompt: true` on every lane body.  Shared system-prompt
  prefix reused between calls: ~5 s prefill → ~0 s once warm.
- **NeuTTS excluded from integration suite** — `run-llm-tests.sh`
  hard-skips `*neutts*` / `*-tts*` models; they're voice clones, not
  chat LLMs.
- **Egui id-clash overlay off** — `ctx.options_mut(|o|
  o.warn_on_id_clash = false)` silences the "first use of ID …"
  debug labels dev builds were painting over widgets.
- **Wizard default → Full** — first launch / New Project lands on
  the everything-included rack layout.

### Event-stream polish

- **Drum-hit history** — parallel `drum_log: VecDeque<DrumLogEntry>`
  to the melodic one; past side of the event stream now renders
  drum past from the frozen log instead of wrapping the live
  pattern.  No more "drum wiped the second it's edited".
- **Wrap-slack fix** — 0.5-step slack on the cycle-wrap threshold
  in the future loop, bridges the 1–2 frame race between
  `current_step` advancing and the UI step listener pushing into the
  log.  Fixes the "blink at every cycle boundary" report.

### Header + small UI

- **TEMP chip** in the top header band — the Huth warm/cold display
  moved out of the event stream header so it's always visible
  regardless of the lower panel's size.  HEAT column shrinks
  34 → 26 cols to make room for TEMP 8.
- **Per-agent seed chip** on the agent card — mirrors the LLM
  Console's global SEED row but scoped per-agent.
- **Style-aware preset naming** — `Crew` preset re-labels itself
  in the wizard based on active style: jungle/dnb/uk-garage/dubstep
  → `Posse`; gabber/early-rave/darksynth/electro → `Squad`;
  synthwave/vaporwave/lo-fi hiphop → `Band`; ambient/baroque/idm →
  `Ensemble`.  Canonical preset ids stay `Crew` so API + tests are
  unaffected.
- **303 lane visibility fix** — sequencer panel now filters lanes
  by `bass_voices[vi].enabled` directly instead of via
  `sequencer.bass_voice_enabled`, which was only synced inside the
  audio-thread snapshot.  Toggling voice 2 from the bass panel
  correctly shows a second lane.
- **Piano LEDs** drop the 2nd/6th/7th tier — only tonic / 3rd / 5th
  render now for easier reading on small screens.
- **Startup auto-prompt** uses the selected style: "Create a pattern
  in the style of Acid House." replaces the old "Pick a style…"
  placeholder that was confusing the model.

### 303 DSP

- **Slide envelope retrigger** — slide steps no longer skip envelope
  attack.  Previous behaviour (legato with no re-attack) produced
  silent slides on percussive 303 patches where `amp_sustain ≈ 0`;
  now every trigger re-attacks while preserving `self.freq` so the
  pitch still glides into the target.  LFO fade-in stays legato
  (doesn't reset on slide-linked chains).

---

## v0.7.7 — UI overhaul cycle

### Header redesign

- 105-column virtual grid shared by both header strips so chip widths
  line up across the transport bar and the lower log/scope band.
- Top header: `LOGO` split into `TITLE` (15 pt strong) + `STATUS`
  (5-column dB table for sub/low/mid/hi/peak, colour-coded by signal
  strength) + `WARN` (rotating alert lines / "OK") chips, plus
  centred BPM, compact STOP/REC, HEAT, MUTE+MON, VRAM/RAM.
- Lower band: free-form layout — square ring oscilloscope on the
  right (= panel height), centre column defaults to ~40 % of width
  (bar oscilloscope on top, event stream below), log fills the rest,
  with a draggable splitter on the log/centre seam that persists for
  the session.
- Global log embossed with `theme::draw_screen_panel` (DEEP fill,
  slightly lighter than the screen `VOID` of the oscilloscopes).
- All TLA labels spelled out: `MON → MONITOR`, `ARR → ARRANGE`,
  `CTX → CONTEXT`, `RST → RESET`, `TS → TIME SIG.`, `THK → THINK`,
  `PRD → PRODUCER`, `MASTER VOL → MASTER VOLUME`, `P.DPT → P.DEPTH`,
  `P.TIM → P.TIME`, `RESO → RESONANCE`, `ENVMOD → ENV. MOD`,
  `FWD → FORWARD`, `REV → REVERSE`, `MIR → MIRROR`.
- Audio-analysis "near clip" warning tightened from a 2 dB to a 1 dB
  window so default-volume material stops tripping it.

### LED skeuomorphism

- New `theme::led(painter, center, radius, color, intensity)` —
  5-ring concentric falloff with very transparent outermost ring,
  hot-spot brightening toward white, dark housing rim, top-left
  specular highlight.
- New `theme::led_dark` — inverse-light variant for bright surfaces
  (used by piano white-key scale dots when Huth coloring is off).
- New `theme::led_flat` — 2D variant used inside the event stream so
  dots don't read as physical raised buttons.
- Module-card title-bar LED on both front and back panels — same
  chrome, only renders on modules that emit audio
  (`ModuleKind::has_audio_output()`), lit when
  `enabled && reaches_master`.  Front-panel title shifts +18 px past
  the LED so wide names like "BASS SYNTH" don't lose their leading
  character.
- Hover tooltip explains the indicator: *Audio path indicator —
  lit when this module is enabled and its audio reaches MASTER*.

### Rack reachability + wiring

- `RackState::reaches_master(module_id) -> bool` — pure BFS over
  audio cables (out → in), only stepping through enabled modules.
  `MasterOutput` counts as reachable even if its own enabled flag
  is unset.
- `wire_default_cables` no longer chains all 12 FX serially with no
  terminus.  New strategy: voices → MASTER (dry direct), TTS →
  Reverb (sends), Reverb → MASTER and Delay → MASTER.  All other FX
  live in the rack but stay unwired (transparent placeholders the
  user patches in).
- 16 unit tests covering `reaches_master` and the sequencer
  lane-visibility predicate (`module enabled AND reaches_master`).

### Sequencer lane visibility

- Sequencer panel uses the same predicate the LED does.  Hoover,
  GabberKick, AmenSampler, etc. only get a lane when the
  corresponding module is in the rack, enabled, AND patched into
  the audio path — orphan modules don't take row space.
- Dynamic-height calculation auto-shrinks to match: no empty rows,
  no whitespace when a voice is unpatched.
- `Full` rack preset gains GabberKick.  Wizard now enables
  `bass_voice_enabled[1]` whenever the chosen preset includes
  `AcidBass`, so two 303 lanes are on by default.

### Event stream history

- Notes for all melodic voices: bass voices (multi-voice 303),
  AN1X, and Hoover are folded into the auto-range and rendered as
  the same Huth-coloured dots.
- New `MelodicLogEntry` ring buffer in `ImpulseApp` (cap 256, ≈ 8
  bars at 32-step patterns).  Each sequencer step transition
  snapshots the active notes from every melodic pattern and stamps
  them with the current `global_step_count`.
- Render split: past (offset ≤ 0) reads from the frozen log;
  future (offset > 0) reads from the live pattern.  Pattern
  mutations after the fact don't erase or shift visible past
  notes — once a note has fired it scrolls left until off-screen.
- Per-voice cycle length honoured for the future side
  (`bass_voice_steps`, `an1x_steps`, `hoover_steps`).

### Huth temperature display

- `NOTE_TEMP[12]` per-semitone warm/cold scalar derived from
  cos(hue − 60°); warm pole F-orange (+1.0), cold pole C-blue
  (−1.0).
- `audio::spectrum::spectrum_temperature(magnitudes, bin_hz, semi_temps)`
  pure fn weighted by FFT bin magnitude across 30 Hz – 5 kHz.
- `state::sequencer_state::pattern_temperature_acc` does the same
  for melodic patterns weighted by `gate × accent`.
- Event stream gains a TEMP strip — blue→neutral→orange gradient
  with a live needle (spectrum) and a small bank tick (pattern
  data) plus a numeric readout.  Hover tooltip explains both
  markers.

### Mod-overlay (back-panel LFO chip strip)

- 5-ring LED falloff for chips; `led_flat` for inside-display dots.
- Card-width-aware wrap budget — `back_card_w` published into
  ctx-temp by `module_card_back`, consumed by `module_card_mod`.
- Slider width derived from stable `overlay_max_w` (clamped 20-60
  px), so it always fits and never jitters as wrapped chips reflow.
- Chip text 6.5 pt + tighter button padding so drum-kit Selectors
  pack densely and wrap into 2 rows only when they have to.
- Anchor on the same row as the jack/label (right of label), so a
  wrapped chip strip doesn't push the next jack off-screen and
  `PORT_SPACING = 32` stays tight.
- Z-clip extended (`screen_bottom − 105 − 70`) so wrapped chip
  strips never punch through the keyboard panel when the rack is
  scrolled.

### Per-agent seed (mirrors style)

- `LlmAgentState` gains `seed: i64` (default −1 = random) and
  `seed_locked: bool`.
- `propagate_seed(state, seed)` writes the global `LlmState.seed`
  and copies it to every agent whose `seed_locked == false`.
- LLM Console gains a SEED row under STYLE: lock-aware label,
  custom-formatted DragValue (`random` for −1), RANDOM button.
- Inference path reads `agent.seed` instead of `LlmState.seed` when
  an `agent_id` is in scope.

### File / project flow

- File menu gains **New project** (re-opens the wizard) and **Load
  latest project** (newest `project-*.json` in cwd, no rfd dep).
- Wizard auto-skips on subsequent launches when the saved session
  has `wizard_done == true`.
- Stray "Bars:" DragValue removed from the File menu.

### Heat — chaos mode

- `LlmState` default heat 0.4 → 0.5.
- 5-band heat guidance in the system prompt: `<0.25` minimal,
  `0.25-0.5` balanced, `0.5-0.75` bold (FX automation kicks in),
  `0.75-0.95` chaotic (extreme drives, dense ratchets), `≥0.95`
  *anything goes — break the rules, overdrive everything, ramp
  every parameter*.
- `mock_response` jam curve re-tuned to the same ladder.

### Refactoring

- `module_card.rs` split into `module_card.rs` (front) and
  `module_card_back.rs` (back).  `module_card.rs` re-exports
  `module_card_back` so existing call sites keep compiling.
- `focused_title_bg` + `draw_focus_shine` made `pub(super)`.

---

## v0.7.6 release polish

### Cable visual hierarchy

Three patch-cable styles, layered back → front so the most semantically
important paths read on top:

- **Audio cable** — fattest 3D tube (4.5 px body / 2.5 px core, gray
  155/185), used for `PortKind::Audio` connections.
- **Signal cable** — `draw_signal_cable` in `src/ui/rack_cables.rs`,
  sized between audio and AI control (3.5 px body / 1.8 px core, gray
  125/155, lighter shadow + softer specular).  Used for
  `PortKind::Cv` and `PortKind::Mod` cables and the synthesised LFO
  cables — modulation reads as a thinner secondary path next to the
  audio routing.
- **Control cable** — thinnest dark cable (2.0 / 1.0 gray 90/120) for
  LLM agent → module control links.  Drawn last so it sits visually
  on top.

### Sequencer PAN row reset

A small `○` button next to the PAN row label zeros every step's pan
in one click.  Right-click on a single cell still resets just that
step.  Layout math ensures the step grid stays aligned with the bass
row above.

### Pre-echo header refinements

- **Voice tabs sized like BANK / CHAIN slots** — `add_sized([38, 14])`
  with monospace size 8.0 so the sequencer's two header strips
  visually align.  Width 38 fits the longest voice label ("hoover").
- **Two-line layout** — line 1 = PRE-ECHO label + voice tabs +
  right-justified anchor strip; line 2 = ON / LEN / VEL / RAT /
  CLEAR.  The split lets the strip take the full panel width without
  competing with trailing controls.
- **Left-aligned with the sliders above** — both rows emit the same
  `10 + 10 + (SEQ_LABEL_W − 20)` prefix as the bass / drum rows so
  the controls start where the sliders do.  PRE-ECHO label is painted
  directly into the label slot.
- **Anchor strip stride mirrors the sequencer step grid exactly** —
  per-cell stride is `cell_w + item_spacing.x` plus 4 px at every bar
  boundary and 2 px at every non-bar 4-step boundary.  Cumulative
  `step_x` array drives both drawing and click hit-testing so anchors
  land on the same cell visually and on click.

### Mod-overlay top clip

`draw_mod_selector_dropdowns` takes a `canvas_rect` parameter and
skips any back-panel jack whose anchor scrolls above
`canvas_rect.min.y`.  Mirrors the existing bottom-edge skip (piano /
footer reserved height) so the Foreground `egui::Area` no longer
paints over the header info panel or the prompt strip when the rack
scrolls.

---

## v0.7.5-snapshot — continued (post-snapshot session)

### Per-knob modulation system

- **Third cable kind** — `PortKind::Mod` distinct from Audio / CV /
  Control.  An LFO module's CV output can patch into any specific
  knob via dedicated mod-input jacks on the back panel.
- **`mod_inputs(kind)` interface** — every `ModuleKind` declares a
  list of `ModInput::Fixed(LfoTarget)` (dedicated per-knob jack) or
  `ModInput::Selector` (generic jack with a target picker).  The
  exhaustive match enforces the contract: adding a new kind forces
  the author to declare its mod interface, even if empty.
- **47+ LfoTarget variants** — every modulatable knob is named:
  bass cutoff/reso/pitch/volume/pan, AN1X cutoff/pitch/pan, per-drum
  pan + decay (808 + 909), reverb size/damp/mix, delay time/feedback/
  mix, chorus rate/depth/mix, phaser, waveshaper, drive, bitcrush,
  ringmod, EQ, compressor, tape sat, autotune, amen volume/start/
  gate, granular volume/density/grain/position, master volume.
- **Multi-select Selector chips** — `RackModule.mod_selectors:
  Vec<Vec<LfoTarget>>` per slot.  Each chip toggles one target on/
  off; a `—` meta-chip toggles all on/off at once.
- **Per-cable depth (`%`) + polarity (`+ / −`)** — slider 0–1 with
  visible % label and an inversion toggle so a single mod can drive
  the target up or down without changing the source.
- **Cable-only LFO activation** — an LFO slot's phase still runs
  even when its built-in `target` is None, as long as a Mod cable
  sources from that slot.
- **Audio-thread routing** — `ModRouteCopy` (lfo_slot, target_u8,
  depth) array snapshot in `AudioParams`; `apply_mod_target` shared
  dispatch handles 67 opcodes (legacy + new).  No per-block
  allocations.
- **AN1X pitch DSP route** wired (was a stub) via
  `AudioParams.an1x_pitch_mod_st`.
- **HTTP API**: `POST /api/rack/mod_cable | mod_target | mod_depth`
  with case-insensitive target name parsing.
- **LLM JSON**: `rack.mod_cable: [{from_lfo, to, slot, depth?,
  targets?}]` action handled by
  `state::modulation::apply_llm_mod_cable_entry`.

### TTS — audible again, agent-triggered

- **Sample-rate fix** — NeuTTS Air outputs 24 kHz WAV; the reader
  only upsampled the legacy 22050 → 44100 case, so 24 kHz audio
  played at 2× speed and was perceived as silence.  New `TtsSink {
  tx, target_sr }` carries the device rate; `read_wav_f32_bytes`
  does generic linear resampling.
- **Agent-triggered TTS** — `speak_neutts` was gated behind `if let
  Some(param_update)`, so MC agents that emit only `mc_line`
  (no param change) never fired TTS.  Hoisted out of the gate.
- **Shell log gets the line** — agent `mc_line` now also reaches
  `log::info!`.
- **Warn log** when an MC agent speaks but no NeuTts module is wired.

### Reverb + Delay direction toggle (FWD / REV / MIRROR)

- Per-FX 1 s circular input buffer feeds a continuously-rewinding
  reverse tap.  REV mode processes the reversed tap → preverb swell
  / anti-echoes preceding the dry hit.  MIRROR sums forward + reverse
  (reverse weighted 0.7 so it doesn't dominate).
- Compact 3-state cycle button on the FX panel.
- **Caveat**: rewind cycle is fixed at ~1 s — tempo-quantized buffer
  size is a future improvement.

### Bass voice — LFO panel + per-step pan

- **LFO panel row** — TARGET (Off → Pitch → PWM → Cutoff → Amp) and
  WAVE (SIN/TRI/SAW/↓SW/SQR/S&H) cycle buttons, SYNC toggle (●/○),
  RATE-or-BEATS knob, DEPTH knob.  Maps to the existing `bass.lfo_*`
  fields the LLM could already write.
- **Per-step pan** — `TB303Step.pan: f32` (-1..1, 0 = use voice
  static).  `TriggerEvent::BassTrigger.pan` plumbed to DSP, latched
  per trigger and used in the per-voice pan mix.  LLM JSON:
  `sequencer.bass_pans: [...]`.

### Amen — looping + rearranging + clearer playback

- **Loop by default** — `AmenState.loop_mode` flips to true.
- **Slice ORDER strip** — `SequencerState.amen_slice_order: Vec<u8>`
  maps step index → slice index (empty = identity).  Per-cell click
  cycles 1..slice_count.  Auto-resizes when SLICES count changes.
  RESET clears.
- **Step → slice mapping** — when `step.slice == 0`, the sequencer
  substitutes `slice_order[step % len]` (or `step` when empty), so
  step 4 plays slice 4.  Single-enabled-step patterns no longer
  always re-fire slice 1.
- **Pulsing now-playing wedge** + slice number labels inside each
  ring + matching highlight on the ORDER strip cell.
- **Direction indicator** swapped from ▶ / ◀ (looks like a play
  button) to ↻ / ↺ (rotation arrows).

### LLM agent card — quick-command pills

- 7 pills on the agent card (REWRITE / VARI / FILL / SPARSE / BUSY /
  BRIGHT / DARK) fire one-shot `LlmInput::Infer` scoped to the agent.
  The agent's existing scope (control cables) is honoured by the LLM
  loop, so each pill lands inside the agent's sandbox.

### Style → rack auto-setup

- **`Style.rack_modules: Vec<String>`** — selecting a style adds the
  missing modules non-destructively (existing kinds are kept).  Calls
  `wire_default_cables()` once after additions; pushes a recomputed
  FxPlan; logs "Style rack: + bass, amen, …".
- Seeded for `acid_classic` / `jungle` / `drum_and_bass` / `gabber` /
  `dub_techno`; other styles inherit empty default until filled in.

### Smart randomization — `POST /api/randomize`

- Picks a random style (SystemTime nanos % len, no rand-crate
  dependency), applies baseline params, adds rack modules, sets
  active_style + propagates to non-locked agents, fires
  `LlmInput::Infer` with "FULL RESET to <name>".

### Shell log colourisation + Huth filter fixes

- **Shell log** routes through `log_fmt::colorize` with grayscale
  line colours matching the in-UI log (`CHALK / HAZE / FOG / SMOKE /
  ASH`) plus Huth note-colour highlights.  Auto-disables on non-TTY
  or when `NO_COLOR` is set.
- **Model filenames** like `gemma-4-E4B-it-Q4_K_M.gguf` no longer
  colour `E4` as a note (word-boundary check after the octave digit
  rejects `E4B`).
- **`44100 Hz`** colours as one full token instead of being parsed
  as embedded `4100 Hz` blue (left word-boundary on the digit scan
  + dropped the upper Hz cap; semitone class wraps cleanly).
- **Persona prefix** — agent response lines `PULSE -> Hi` rewritten
  to `PULSE: Hi`; line-colour detection updated.

### Back-panel layout overhaul

- AUD / CV / CTL ports share a single horizontal top row of the
  strip with labels below each circle; in/out badges disambiguated
  (`AUD IN` / `AUD OUT`, etc.).
- Mod jacks stack vertically below the row; per-jack overlay anchored
  *below* the jack so the top-row labels stay visible.
- Adaptive strip height grows with mod-input count; (1,2)-grid FX
  cards no longer clip 5-jack stacks.
- `LFO #N` slot label in the title bar + `#N` on the CV-OUT jack so
  multiple LFO instances are individually identifiable.
- Mod overlay skips rendering when its anchor would land in the
  bottom-105 px reserved for the piano panel — piano always stays on
  top.

### Drum panel scaling

- `draw_kit_a` / `draw_kit_b` now use `ControlPrefs::from_prefs_scaled`
  so per-module scale (Ctrl+scroll) takes effect; the 808 XY pad
  hit-region matches the visual after shrinking.

---

## v0.7.5-snapshot additions (36 commits since v0.7.4)

### AMEN sampler — proper break chopper

- **Slice model** — each trigger plays one slice of the loaded WAV,
  not the whole sample.  slice_count: 1/2/4/8/16.  Per-drum-step
  `slice` field selects which slice fires (0 = auto-advance).
- **Gate + stutter + reverse** — per-slice gate fraction cuts
  playback short for stuttery pulses; stutter (0–4) retriggers
  the slice; reverse flips direction globally.
- **Transient auto-slicing** — AUTO button runs an energy-based
  onset detector on the loaded sample and populates
  `slice_positions` (normalised 0..1) with the detected times.
  RESET clears back to equal divisions.
- **Per-slice pitch + volume** — 16-slot arrays on AmenState;
  agents can write `slice_pitches` / `slice_volumes` to vary
  individual slices across a chopped pattern.
- **BPM-stretch to host tempo** — source_bpm + bpm_stretch
  together pitch the sample to match sequencer.bpm.  Classic
  drumbreak treatment (pitch follows tempo; pitch-preserving
  stretch deferred).
- **Waveform thumbnail + slice wheel** — the panel shows a
  min/max waveform strip with slice markers and start/end region
  shading, plus a circular slice wheel with the currently-playing
  slice lit up.  Placeholder rect when no sample loaded so the
  layout doesn't jitter on load.
- **Sample discovery** — `samples/amen/` directory with GET /
  RANDOM / LOAD / PLAY buttons, scrollable dropdown picker,
  metadata strip (duration / channels / bit depth / source rate /
  file size), archive.org GET button linking to the amen-breaks
  collection.
- **POST /api/amen** — accepts `{ "path": "..." }` or
  `{ "random": true }` so scenarios can swap samples mid-jam.
- **LLM schema** — full `amen.*` object writable from agent JSON:
  slice_count, start_offset, end_offset, reverse, gate, stutter,
  loop_mode, pitch, volume, slice_positions, slice_pitches,
  slice_volumes, source_bpm, bpm_stretch.  Plus
  `sequencer.amen_steps` + `sequencer.amen_slices` for chopped
  patterns.

### Granular texture voice — CAPTURE workflow

- **Live master-output ring buffer** — audio thread always pushes
  the master output mono into a dedicated 15s ring.  UI drains
  into a 3s rolling tap every frame.
- **Live waveform strip** — 260×66 px min/max viz scrolling
  oldest-left → newest-right with a CHALK cursor at the freshest
  sample.
- **CAPTURE button** — freezes current tap contents as the
  granular voice's source.  In-memory only; path becomes
  `«captured»` so the disk-load auto-sync skips it.
- **Texture samples directory** — `samples/textures/` with a
  GET button linking to archive.org/details/opensource_audio;
  RANDOM / LOAD buttons mirror the amen panel.
- **POST /api/granular** — same shape as /api/amen for picking
  or randomising texture source.

### Bass voice → SH-101 territory

- **Full ADSR** on both amp and filter envelopes — `amp_attack`,
  `amp_sustain`, `amp_release`, `filter_attack`, `filter_sustain`,
  `filter_release`.  Legacy `decay` still drives the filter env
  time for 303-style decay-only squelch.  Backward-compat via
  serde defaults.
- **PWM** — pulse width modulatable on the square waveform
  (0.05..0.95, centered 0.5 = classic square).  Narrow pulses
  give the reedy 101 sound.
- **Per-voice LFO** — dedicated modulator with routable targets:
  Pitch (±2 st), PulseWidth (±0.45), FilterCutoff (±0.5),
  Amplitude (±50% tremolo).  Free-rate (0.01–20 Hz) or BPM-sync.
  Sine / Triangle / Saw / Inv-Saw / Square waveforms.  Fade-in
  resets per note to honor lfo_delay.

### Pre-echo pattern modulator

- **Anchor-driven lead-ins** — declare anchor step indices per
  voice; the N steps before each anchor get a build-up ramp
  (velocity 0.3→1.0 and/or ratchet 1→4).  Wrap-aware: tail of
  the bar leads into step-0 downbeat.
- **Per-voice configs** —
  `sequencer.preecho[kit_a|kit_b|amen|bass|hoover|an1x]` with
  `{enabled, anchors, length, velocity_ramp, ratchet_ramp}`.
  Applied inline in `advance_clock` at trigger time (drums for
  v1; melodic voices pass through unchanged).
- **UI** — compact single-row section at the bottom of the
  sequencer panel with voice tabs, a clickable 21×21 square-cell
  anchor strip (live lead-in preview), LEN drag, ON/OFF,
  VEL / RAT toggles, CLEAR.  8 pure-function tests on the
  scaling math.

### TTS panel overhaul

- **Server status** with polling /health check, inline ONLINE /
  OFFLINE indicator, one-click **START** button that spawns
  `scripts/neutts-server.py` on port 8770 as a detached
  subprocess.  Uses `.neutts-venv/bin/python` if present.
- **SAY field + button** — type a line, synthesise immediately
  through NeuTTS with the module's voice_ref / temp / top-k /
  top-p.  Enter also fires.  Empty SAY prompts the controlling
  agent to improvise in character (rhyme / shout / sung hook /
  ROBOT bleep, whatever fits).
- **ASK row** — THEME / RHYME / SING buttons send persona-
  aware prompts to the controlling agent with the active
  style's themes appended.
- **Conditioning preview** — shows the first line of
  `voices/<voice_ref>.txt` under the voice selector.
- **Voice reference discovery** — `voices/` directory GET
  button opens archive.org/details/librivoxaudio as the clean-
  single-speaker source recommendation.  README docs Common
  Voice and the MC-character search-term caveat (music
  underneath clones badly).

### Rack + module changes

- **LLM action surface** — `rack.add` / `rack.remove` let
  agents create/delete modules from JSON.  `spawn_agent` gains
  `mode` ("off" / "producer" / "dj" / "mc") and `tts` fields;
  mode=mc auto-wires a NeuTts module and a control cable.
- **POST /api/style** — set active style + propagate to
  unlocked agents (fixes prior-session style bleed).
- **parse_module_kind** moved to `state/rack_scope.rs`, shared
  between HTTP API and LLM rack path.
- **AmenSampler panel redesign** — 3×3 module, grouped knobs,
  square anchor cells, slice wheel with forward/reverse hub
  glyph, waveform placeholder reserves space so loading
  doesn't jitter the layout.
- **GranularTexture** module 3×1 → 3×2 to fit the live ring
  viz.
- **AN1X panel padding** — F.ENV and A.ENV ADSR visualisers
  wrapped in (8, 6) inner-margin frames.

### Demo scenarios

- **D&B style-dnb.sh** rewritten — 10 scenes, amen chopping,
  AN1X as drone pad not lead, bass as reese, MC scene via API
  that actually plays through NeuTTS (server kept alive).
- **record-demo.sh** reorder — app launches before TTS pre-gen
  so llama-server warms concurrently; `wait_for_llm` before
  starting capture so clips don't begin with dead air.
- **set_style / reset_all helpers** in demo/lib.sh prevent
  prior-session style bleed.

### Log + prompt polish

- **► marker** on MC lines (replaces ambiguous ◆).
- **Kit A / Kit B ignore rule** — the log's Huth note colorizer
  skips bare letters preceded by "kit" / "pad" / "part" /
  "bank" / "slot".  Prevents "Kit A" being painted as a note.
- **Seed pattern length** — prompt now reports the seed's
  actual length dynamically (was hardcoded "16-step").

### Ops / release

- **v0.7.4 shipped** — 36 commits ago; CI bundles release zips
  as `impulse-instruct-vX.Y.Z-{linux,windows}-x86_64.zip` with
  end-user start scripts.
- **`scripts/download-models.{sh,bat}`** at release-zip root
  (no longer in scripts/ subdir).  Manual-download path
  primary; URL fallback when CLI tools missing.
- **`/samples/amen/` and `/samples/textures/`** directories
  tracked via .gitkeep; contents gitignored.  `samples/
  README.md` points at archive.org + freesound.

---

## v0.7.3 additions (23 commits since v0.7.2)

### LLM control flow

- **Scoped agents can rewrite their voice's sequencer** — the `sequencer.*`
  update block was gated entirely by `in_scope("sequencer")`, so every
  scoped agent (BASS, DRUMS, …) silently dropped `bass_steps` / `bass_notes` /
  `drum_lengths` / per-kit step arrays. Per-voice sequencer fields now
  dispatch by the voice's own scope (`bass_* → "bass"`, `kick_a_steps →
  "kit_a"`, etc.); global fields still require `"sequencer"` scope
- **Heat is user-only** — `settings.heat` emitted by the LLM is ignored.
  Heat is a user vibes knob, not an agent action. Prompt doc updated
  to match
- **Heat actually chaotic at 1.0** — previous effect was a 3% top_p nudge.
  Heat now scales temperature ×(1 + h·0.8), top_p toward 1.0, min_p floor
  ×(1 − h·0.9), and frequency_penalty + h·0.4 (which also discourages
  repeated-root fallbacks like the old all-Cs bass issue)
- **MUSICAL MODERATION prompt section** — concrete safe ranges for FX
  (reverb/delay/chorus/distortion mix + feedback/drive), drum velocities
  (kick > snare > clap > hats), and bass aggression (resonance ≤ 0.85
  unless asked). Agents default to restraint unless heat > 0.7 or the
  user literally asks for "wild / insane / max / destroy"
- **Sparser default bass density** — 1/4–1/2 (8–14 notes per 32 steps)
  replaces 1/3–2/3 (10–22). Style-specific table overrides: Bach stays
  dense (18–28), acid 10–16, techno/minimal 6–10, deep house/ambient 4–8
- **Free-mode prompt teaches the bank** — even without a style, agents
  now commit to root+scale and spread ≥ 3 distinct pitches across each
  half of the bass loop, respecting `sequencer.steps`

### UI

- **Ctrl+click cycles knob lock mode** — replaces Alt+click (which
  collided with OS menus) and the tooltip-advertised right-click (which
  the code didn't accept). Works with the footer Ctrl lock too so
  pointer-only users can toggle without a keyboard
- **Style-based lock indication, no badges** — Free = chrome, LlmFocus =
  brightened chrome, UserOwned = flat knob with visible spokes. Tooltip
  only appears on non-default modes to keep untouched knobs silent
- **Full-word knob labels** — CUT→CUTOFF, RES→RESO, ENV→ENVMOD,
  DEC→DECAY, ACC→ACCENT, DRV→DRIVE, VOL→VOLUME, GLD→GLIDE, NSE→NOISE,
  DTN→DETUNE, DAMP→DAMPING, FDBK→FEEDBACK, FMD→FM.DEPTH, FMR→FM.RATIO,
  and LFO targets (DLY.T→DELAY.TIME, etc.) across every panel and the
  rack's FX mini-cards
- **Ring scope phosphor matches bar** — both use history trails (gray
  15→90, stroke 1.0→1.8) with CHALK current frame; the single-frame glow
  underlay is gone
- **303 centered in the rack** — canonical voice order swapped so
  AcidBass (11) sits between DrumKit808 (10) and DrumKit909 (12),
  matching pitch register and making the classic 3-voice rack visually
  balanced regardless of insertion order
- **Wordmark bullet** — title bar + About dialog read `IMPULSE • INSTRUCT`
  instead of `◆ IMPULSE INSTRUCT`
- **Header polish** — MON slider widened to match HEAT; VRAM/RAM bars
  enlarged; log colored by role (user / agent / system / api)
- **Piano labels** — top two octaves labeled; hover reveals frequency
- **Alt footer indicator removed** — Ctrl carries the lock workflow;
  physical Alt still hides cables

### Graceful shutdown

- **SIGINT / SIGTERM handler** — Rust's Drop doesn't run on signals, so
  Ctrl-C on the running app used to orphan the llama-server child and
  its VRAM. A dedicated signal-handler thread now `sigwait()`s and
  `pkill`s `llama-server … --model` (SIGTERM, then SIGKILL 200 ms
  later) before the process exits

### Demo recording

- **Reliable llama-server cleanup between runs** — the demo script's
  cleanup trap now SIGTERM-then-SIGKILLs the app with a 3-second grace
  window for Drop, then `pkill`s orphans
- **Female narrator + longer subtitle display** — intro TTS voice
  swap, reading-time-friendly subtitle durations, intro line tweaks
- **Runtime-timestamped SRT** — subtitles derive from actual narrate()
  playback timestamps, no drift vs. the recorded audio
- **LFO scene** — adds an LFO module and scrolls to it so the card is
  visible before the modulation starts
- **TTS retry + server restart** — up to 10× with server bounce;
  graceful handling of missing WAVs in narration
- **Free & open source outro line**

---

## v0.7.2 additions (105 commits since v0.7.1)

### UI rework — 12-column RPG-inventory rack

- **12-col grid rack** — modules snap to a fixed column grid with bin-packing
  placement; `arrange_grid()` runs a center-bias pass so zones stay visually
  balanced instead of piling against the left edge; `add_module()` re-runs
  the full layout on every API/demo add so new modules land centered
- **AI / MAIN AUDIO zone split** — `Zone::Global` was too catch-all. Split
  into `Zone::Ai` (LLM console + agents, always on top, agents now pack
  directly under the console) and `Zone::Global` rebranded "MAIN AUDIO"
  (sequencer + master). Four tabs total: AI / MAIN AUDIO / VOICES /
  FX+MOD. Old sessions migrate zones on load via `persistence::apply_session`
- **Module remove with confirmation** — centered dialog on all non-core
  modules; disconnects cables and cleans up agents automatically
- **Drag overlap prevention** — AABB collision check rejects drops onto
  occupied grid cells; red ghost overlay for blocked positions
- **Dynamic sequencer height** — sequencer grid cell pixel-sized from
  per-lane actual heights (step row, accent/slide marker rows, drum
  vel/prob/ratchet sub-lanes) rather than a coarse "2-physical-rows =
  1-grid-row" heuristic; cell stays exactly as tall as content needs
- **Flip-scroll behaviour** — first rack flip scrolls to master, second to
  agent; extracted to `src/ui/flip.rs`
- **Rack presets in wizard** — Empty/Basic/Standard/Full; wizard renamed
  "Rack Setup"; `from_preset()` wires default cables so fresh presets are
  audible immediately

### Sequencer — wrap, alignment, new sliders

- **32-step-per-row wrap** — `STEPS_PER_ROW = 32`; 1..=32 steps render on
  one row, 33..=64 wrap into 2 rows of 32 each; odd time signatures keep
  correct beat spacing via absolute-index beat dividers
- **Exact-size prefix** — every row (bass / accent / slide / hoover / an1x
  / drums) emits an identical 5-widget prefix through
  `allocate_exact_size`, `fixed_label`, `fixed_slider`, and `fixed_space`
  helpers; cells share one x anchor across voices and sub-rows (no more
  drum rows drifting half a step right of bass)
- **Volume/accent/slide sliders in the sequencer** — bass row shows bass
  volume; ACCENT row shows `bass.accent_level`; SLIDE row shows
  `bass.portamento_time`; HOOVER and AN1X rows show their own volumes;
  every slider uses `SEQ_VOL_W = 330 px` with `style.spacing.slider_width`
  overridden so the widget renders at the full reserved width
- **Header label alignment** — BPM and SWING labels use identical
  fixed-width slots so they left-align vertically across rows; `fixed_slider`
  drives both at `HDR_SLIDER_W = 600 px`
- **Per-voice step-count editor** — drag/double-click the `02`-style
  count widget to change a drum voice's length independently of global
  `sequencer.steps`
- **Step set matches bank** — rendering stops exactly at `seq_steps`;
  disabled "ghost" cells past the configured length are gone

### Audio cables actually route

- **Cable topology filter** — `compile_fx_plan()` walks the audio-cable
  graph and includes only FX modules reachable from a voice (or from
  another reachable FX). Disconnect a reverb from the chain → reverb
  stops processing. No more "visual lie" where cables implied routing
  that DSP ignored
- **Visual dimming** — modules not in the compiled FxPlan render dimmed on
  the back panel so it's obvious which ones don't see audio
- **`wire_default_cables()` reusable** — called by `RackState::default()`,
  `RackState::from_preset()`, and by `apply_session()` as a migration for
  old sessions with 0 cables; ensures wizard Presets produce an audible
  signal path on first flip
- **Cycle-safe connect** — `connect()` rejects audio cables that would
  create cycles; `strip_audio_cycles()` sanitises session data on load

### TTS — NeuTTS Air replaces Coqui

- **NeuTTS Air voice cloning** — local GGUF model (~527 MB), persistent
  Python HTTP server on port 8770; voice identity cloned from a 3–15 s
  reference clip; single `ModuleKind::NeuTts` with per-module settings
  (voice_ref, temperature, top_k, top_p); Coqui/direct-espeak paths removed
- **n_ctx bumped 2048 → 32768** via `NeuTTSWide` subclass overriding
  `_load_backbone`; matches Qwen 0.5B's training context so long sentences
  stop garbling. Overridable via `NEUTTS_CTX` env var for low-VRAM setups
- **Voice reference generator** — `scripts/generate-voices.sh` produces
  `voices/default.wav`, `mc.wav`, `dj.wav`, `robot.wav` from espeak
  rendering; integrated into `scripts/download-models.sh` setup flow
- **Smart pitch snap** — optional per-clip pitch detection + resample to
  nearest in-key note (`tts.pitch_snap`)

### Demo recording pipeline

- **`demo/record-demo.sh`** — full orchestration: pre-generate TTS, launch
  app with `--skip-wizard --fresh-session`, start h264_nvenc capture with
  `-pix_fmt yuv420p -vf "crop=trunc(iw/2)*2:trunc(ih/2)*2"`, run scenario,
  re-encode with `-sws_flags "lanczos+accurate_rnd+full_chroma_int+full_chroma_inp"`
- **Pre-generated SRT** — `pregenerate_srt` parses the scenario
  (`say` / `narrate` / `scene` / `pause` / `wait_seconds`) and emits a
  complete SRT before recording starts, independent of runtime timing;
  durations use `max(clip_duration, reading_time)` so subtitles stay
  on-screen long enough even if NeuTTS truncated the audio
- **Resilient TTS pre-gen** — `tts_generate` retries up to 3× with a 120 s
  curl `--max-time`; pre-gen pass tracks ok/failed counts and prints the
  missing clip IDs at the end so silent NeuTTS failures don't slip
  through; handles both `narrate "id" "text"` and high-level
  `say "text"` (auto-ID `auto_NNN_<slug>`) in scenarios
- **NeuTTS server stops after pre-gen** — frees GPU memory for the LLM
  during recording; runtime playback uses cached WAVs via paplay
- **`--fresh-session` flag** — ignores saved session, starts with the
  Empty rack preset so demos never inherit the user's setup
- **TTS + audio routed to batch dir** — per-recording `tts/` subdirectory,
  separated from the permanent `voices/` reference clips

### LLM agent improvements

- **AI zone** — console + agents live together, agents pack directly
  under the console after adding. Adding via API auto-scrolls to the AI
  zone so the new agent is visible
- **Current-state pattern-length awareness** — prompt `CURRENT STATE` JSON
  exposes live `bass_len`, `hoover_len`, `an1x_len`, and per-voice
  `drum_lengths` (keyed by schema names); agents stop assuming 16 steps
  and actually use the configured length
- **Voice-specific rhythm guidance** — prompt split into DRUM PATTERNS
  (909 = pin the 4OTF grid; 808 = almost 4OTF with 1–2 tweaks) and BASS
  PATTERNS (syncopated, 1/3–2/3 density target, "do not copy the kick
  grid", concrete off-grid examples, both halves equally active, at least
  3 distinct scale pitches per loop)
- **Fixed-height JSON preview on agent card** — 6-row painter-clipped
  viewport (replaces growing TextEdit / ScrollArea that leaked into
  neighbouring cards); long responses truncate with an ellipsis
- **Knob style reflects lock state** — chrome for Free, darkened chrome
  for UserOwned (locked), flat/brushed for LlmFocus (focused); mode
  dispatch in `param_control`

### Infrastructure / refactors

- **File-size split for 1000-line limit** — `ui/rack_ai.rs` (AI zone
  rendering), `ui/flip.rs` (rack flip logic), `state/fx_plan.rs`
  (topo-sort), `state/persistence.rs` migration hooks
- **Zone migration** — `apply_session()` re-applies `default_zone()` per
  module on load so pre-split sessions land in the correct AI / MAIN
  AUDIO / VOICE / FX+MOD tabs automatically
- **API `/scroll` + `/collapse`** extended for the 4-tab zone layout
  (`ai`, `main`/`global`/`mainaudio`, `voice`, `fxmod`)

---

## Core synth

- **Bass synth** - saw/square/supersaw oscillator, 4-pole Moog ladder filter (LP/HP/BP), sub-osc, noise, FM pair, portamento, waveshaper, overdrive, per-step accent + slide
- **Hoover lead** - supersaw into aggressive highpass sweep, pitch LFO, dedicated voice in UI
- **AN1X-style VA voice** - dual OSC (saw/square/tri/sin/noise), OSC2 coarse+fine detune, hard sync, ring mod, sub-osc, 3 filter modes, ADSR x 2, pitch envelope, per-voice LFO x 2 with delay/fade, pitch drift, free EG (8-step drawable envelope)
- **Drum machines** - Kit A (808-style: kick with pitch envelope, snare, hihat x 2, toms) + Kit B (909-style: kick, snare, hihat x 2, clap, rim)
- **Standalone noise voice** - white/pink/brown, volume + color + cutoff, AR envelope (5s attack, 10s release), filter LFO (0.05-10 Hz), sample-and-hold modulation (0.5-20 Hz), LLM-addressable
- **Amen break sampler voice** - DrumVoice::Amen in sequencer, linear-interp playback, AudioCommand::LoadSampler, AMEN tab with path/pitch/volume/loop UI
- **Gabber kick** - CLIP knob on both kicks: hard flat-top distortion, LLM-addressable via `kit_a.kick.clip` / `kit_b.kick.clip`
- **LFO matrix** - 4 independent slots, any waveform, wireable to any parameter, BPM sync, phase reset on transport start

## Sequencer

- 16-step base, variable step count per pattern (8/16/32/64), swing
- Per-voice step counts for polyrhythm (kick 16, hihat 12, bass 7...)
- Per-step: velocity, probability (0-100%), ratchet (1-4x), accent, slide
- Euclidean rhythm generator
- Pattern bank (8 slots), chain playback (up to 8 patterns in sequence)
- Live record - MIDI keyboard writes directly into steps
- Time signature selector (4/4, 3/4, 5/4, 6/8, 7/8...)
- Mute/solo per row, pattern copy/paste

## FX chain and routing

- Reverb, delay, chorus/ensemble, phaser (4-stage all-pass), ring modulator
- Waveshaper (pre-FX tanh), bitcrush (bit depth + sample rate), EQ (3-band biquad)
- Master compressor/limiter, tape saturation, drive
- **Modular rack** - zone-based module cards (Global/Voice/FxMod zones), RackState + Cable + PortRef, Bezier cable overlay with 3D tube rendering
- **Cable drag-to-patch** - click+drag from any port to create a cable; right-click a port to disconnect all cables on it; port hover glow (white halo idle, pulsing ring on valid targets, faster pulse when hovered); PointingHand/Crosshair cursor feedback; scroll area disabled near ports so drag never gets stolen
- **FX plan compilation** - `compile_fx_plan()` topologically sorts the cable graph into a `FxPlan`; `process_block()` iterates the plan instead of a fixed chain; default rack cables mirror the original serial order
- **Cable cycle detection** - `connect()` rejects audio cables that would create cycles (BFS reachability check); `strip_audio_cycles()` sanitizes session data on load; grayscale cable colors (R=G=B)
- **Per-voice FX buses** - voice mix split into 8 buses (AcidBass, DrumKit808, DrumKit909, HooverLead, An1xVoice, AmenSampler, NoiseVoice, GranularTexture) + TTS bus; each routed through its compiled chain before the global chain
- **Gated reverb** - `fx.reverb_gate_time` (0-2 s), GATE knob in FX panel
- **Master pitch offset** - `fx.master_pitch_st` (+-12 st), PITCH knob in MASTER group
- **Autotune FX module** - `ModuleKind::FxAutotune`; two-head grain overlap-add pitch shifter (`fx.autotune_amount` 0–1 → 0..+12 st, `fx.autotune_mix`); pre-allocated 4096-sample ring buffer (no audio-thread allocations); LLM-addressable via `fx.autotune_amount` / `fx.autotune_mix`

## Intelligence

- LLM runs locally via llama-server subprocess (official llama.cpp build)
- Jam mode - PULSE evolves the pattern autonomously; heat slider 0-100% gates/throttles jam rate
- Behaviour templates: "build", "drop", "breakdown", "tension", "euphoric"
- Lock system - touch a knob to claim it; LLM won't override it
- Compact step arrays: index list `[0,4,8,12]` or inline `[1,0,0,0,...]` or clear `[]`
- Music theory grounding - root note + scale in system prompt, scale-snap on bass notes
- Instruction set - pre-written JSON templates for common phrases ("make an amen break", "remove claps", etc.)
- LFO dot-notation sanitization - handles malformed LLM output gracefully
- Sampling params exposed in settings: top_k, top_p, min_p, repeat_penalty, frequency_penalty, seed
- Reasoning (thinking) blocks shown in log (toggle)
- AI persona name - editable, used in system prompt
- **LLM jam tools** - ramp scheduling (`"ramp"` key), behaviour templates, heat-aware guidance in prompt
- **Internal music API** - `src/music_api/mod.rs`; all 10 ChordQuality variants, amen_pattern, scale_run, random_diatonic_chord; LLM dispatches via `"music_api"` JSON block
- **Audio feedback (Phase 1)** - LISTEN button captures audio, runs per-band RMS + transient analysis, prepends structured snapshot to prompt; response logged as `LISTEN ->`

## Multi-agent production team

- **Multiple LLM agents** - each agent has its own persona, model, scope, heat, temperature, conversation mode, style, and user instructions
- **Multi-model server pool** - `LlamaServerPool` manages N llama-server processes (ports 8766+), ref-counted per model; agents sharing a model share a single server
- **Per-agent model selector** - dropdown on each agent card; `None` inherits global default
- **Round-robin scheduling** - agents take turns during jam cycles; only enabled rack modules participate
- **Cable-driven scope** - `PortKind::Control` cables from agent to module define what each agent may control; `scope_from_control_cables()` resolves scope at inference time; empty scope = agent controls everything
- **Dynamic spawning** - agents can request new agents (`LlmAction::SpawnAgent`) or dismiss themselves (`LlmAction::DismissAgent`) via JSON; gated by `agent_autonomy` flag; auto-wire control cables on spawn
- **VRAM budget module** - `src/llm/vram.rs` with model profiles (Gemma, DeepSeek, Qwen3), VRAM estimates, and preset configurations
- **VRAM budget guard** - `would_exceed_vram()` rejects agent spawns that would exceed GPU memory; checked at SpawnAgent action + server pool acquire; prevents silent OOM crashes
- **Startup wizard** - always shows on startup; resume last session or start fresh with a preset (Solo/Duo/Swarm/Crew/Voices); GPU VRAM detection + budget bar
- **VRAM estimate on agent cards** - shows `~X.XG VRAM` below model selector
- **Agent persona in log** - output and thinking lines show the correct agent persona name, not the global singleton
- **Console routes to agents** - typed prompts go to the first enabled agent instead of bypassing the agent system

## TTS / MC mode

- **NeuTTS Air voice cloning** — local GGUF model (~527MB), persistent Python server on port 8770; voice identity from 3-15s reference audio clips; per-module settings (voice reference, temperature, top-k, top-p)
- **TTS as rack module** — agents speak through TTS modules connected via control cables; no cable = no speech; single `ModuleKind::NeuTts` replaces old espeak/coqui dual-engine system
- Pitch-snap — synthesised voice quantised to nearest in-key note (autocorrelation pitch detection + resampling)
- API `"tts": true` on agent creation auto-adds a TTS module and wires it

## Style catalog (`styles.json`)

29 genre styles with the following fields (all user-editable):

| Field | Description |
|-------|-------------|
| `id`, `name` | Identifier and display name |
| `keywords` | Trigger words for auto-detection from prompts |
| `bpm_range` | Informational BPM range |
| `brief` | Short creative brief (~50 tokens) for smaller models |
| `description` | Full creative brief (~150 tokens) |
| `seed_patterns` | 16-step starter patterns (kick, snare, hihat, bass) |
| `suggested_root`, `suggested_scale` | Tonic and scale suggestion |
| `baseline_params` | Parameter reset applied when style is selected |
| `mc_lines` | Example MC/DJ lines for this style (optional, fed to MC-mode agents as reference) |
| `themes` | Topic words for singer/rapper agents (optional, gives creative direction) |

`mc_lines` and `themes` are injected into the system prompt — mc_lines only for MC/DJ conversation modes, themes for all modes. Styles that don't suit vocal content (minimal techno, IDM) omit these fields.

## Real-time mix observer

Continuous audio + pattern analysis running every ~2s. Results shown in the header bar and injected into every LLM system prompt as `AUDIO: ...` context. Agents see the mix state and can self-correct.

**Audio-level checks:**
- CLIPPING (peak > -1dB), near clip (peak > -3dB)
- sub overload, harsh highs, mid overload (band RMS thresholds)
- muddy low end (low >> mid by 20dB)
- over-compressed (crest < 3dB)
- near silence (peak < -40dB)
- snare rush (high RMS + fast transients)

**Pattern/mix checks:**
- bass very dense (>80% steps active)
- bass sparse (≤2 steps in 16)
- bass monotone (all active notes identical)
- no bass notes / no kick (while sequencer running)
- reverb high / delay feedback high / heavy distortion (FX extremes)

Alerts cycle in the header (2 at a time, rotating each second). Multiple alerts joined in LLM context with `!!` prefix.

## I/O

- MIDI in - NoteOn/Off to bass synth + live record; CC to synth params; Start/Stop to transport; MIDI clock in with 8-pulse rolling average BPM sync
- MIDI clock out - 24 PPQN, sent on dedicated thread via rtrb ring buffer (alloc-free audio path)
- WAV export (32-bit float), MP3 export (ffmpeg)
- Stem export - renders bass/kit_a/kit_b/amen/noise/hoover/an1x separately
- HTTP/MCP REST API on port 8765 (`--api` flag)
- OSC input - UDP listener on `--osc` (port 57120) or `--osc-port N`; addresses `/impulse/<section>/<param>`, `/impulse/sequencer/play|stop`, `/impulse/prompt`
- Project save/load - JSON snapshots; StateHistory ring buffer (50 deep), Ctrl+Z/Y, Edit menu, LLM snapshots before apply

## UI

- **12-column grid rack** - RPG-inventory-style module placement with snap-to-grid drag and drop; bin-packing auto-arrange with center-biased positioning; per-zone dynamic height
- **Two knob styles** - chrome (concentric rings, scale marks, glint arc) and flat/brushed (radial spokes, knurled edge, hub disc); freely mixable via `ControlPrefs::flat()`; fixed sizes (KNOB_PX=55, PAD_PX=34)
- **Knob value arc** - 270-degree outer range ring on all knobs showing full range with filled portion up to current value
- **Module remove with confirmation** - centered dialog on all non-core modules; disconnects cables and cleans up agents
- **Drag overlap prevention** - AABB collision check rejects drops onto occupied grid cells; red ghost overlay for blocked positions
- **Right-justified PAN sliders** - all voice panels (bass, 808, 909, AN1X, hoover, noise)
- **Right-justified step grids** - sequencer step buttons pushed to right edge via computed spacer
- **Full sequencer labels** - BANK, CHAIN, STEPS, SWING, SNAP, ACCENT, SLIDE; drum voices: 808 KICK, 909 CLOSED HH, etc.
- **Wider sequencer sliders** - BPM/SWING 200px, drum volume 100px
- **Uniform glass pane heights** - per-row min_height in hoover, AN1X, bass, 808, 909
- **Rack presets in wizard** - Empty/Basic/Standard/Full; wizard renamed "Rack Setup"
- **3x scroll speed** - mouse wheel boost for faster rack navigation
- 5 panels: Sequencer / Bass (303) / 808 / 909 / FX; AN1X and Hoover in sequencer area
- Chrome knobs, glass sliders, embossed buttons (neumorphic grayscale)
- **Skeuomorphic step buttons** - active inset well (debossed 2px) with inverted edge highlights; velocity bloom over inset; chrome knob well shadow + catch-light
- Velocity lanes below each step row (drag bars)
- XY pads (CUT x RES, ENV x DEC, REVERB mix x size, DELAY mix x feedback, 808 PITCH x DECAY); pair indicator in corner
- Oscilloscope strip (rolling 512-sample waveform) + ring scope (polar plot, single-polyline, write-head dot)
- ADSR envelope visualizer (interactive - drag zones)
- Piano display - Huth *Farbige Noten* (1888) 12-color theory, C2-C5; Off/Piano/Full setting
- Huth sequencer cells (Full mode) - colored U-cup notation on bass/hoover/AN1X rows; gate-proportional height
- Model selector - scan `models/`, hot-swap without restart
- Reasoning toggle; thinking blocks shown in log
- LLM strip: LISTEN button + live audio analysis display (sub/low/mid/high RMS, peak, crest, transients); collapsible to prompt row only (▲/▼ toggle)
- **Rack canvas** - zone-based horizontal module cards with Bezier cable overlay; responsive voice card grid (1/2/3 columns adaptive); Tab/toolbar toggle for cables
- **Cable signal animation** - normalised to arc length (constant perceived speed regardless of cable length); 2-5 dots per cable based on length
- **LFO visual cables** - active LFO slots synthesise rack cables from state (lfo.target → ModuleKind mapping) so LFO connections show without needing a rack cable entry
- **Central touch-paint mode** - `· / U / F` toolbar row; clicking a knob paints its param mode when mode is active; replaces broken right-click cycling
- **UI preferences** - UI scale (0.5–3.0×, instant via pixels_per_point), Huth style, CRT effect, phosphor settings; persisted in session.json
- **Responsive header** - heat slider fills remaining width; COOL/WARM/HOT/FIRE/CHAOS tier labels with color ramp; monitor volume labelled MON (listen-only, not export)
- **Zone visual hierarchy** - zone rails (Global/Voices/FX+Mod) have distinct gray backgrounds (24/18/14); module cards have 6px side + 8px top/bottom inner margin; 3-dot drag handle in every title bar
- **Per-zone collapse** - each zone rail has ▶/▼ toggle; collapses all cards in that zone to recover screen space
- **Preferences AI sub-tabs** - AI tab split into Model / Sampling / Personality / TTS sub-tabs; Sampling labelled "experimental"
- **Huth note coloring in log** - in-UI log colorizes note names (C4, A#3), frequencies (440Hz), MIDI context (note 60) with Huth palette; `colorize_log()` in `llm_strip.rs`; text remains selectable/copy-paste-able; safe word-boundary guards prevent false positives (D&B, E-flat etc.); quality word extension colors "A minor", "G major" as a single span
- **Log level persistence** - `log_level_idx` persisted in `session.json`; survives restarts
- **Skeuomorphic XY pad** — thick beveled outer frame (raised panel, inset rubber well), corner tick marks, rubber nub cursor with layered dome, specular catch-light, and hover glow ring; Y axis label/value overlaid inside pad; no left label strip
- **Centered module layout** — knobs and controls center-align horizontally within glass groups and rack module cards (no more left-clustering dead space)
- **Fixed control sizes** — knobs (55px), step buttons (34px), XY pads (172px), ADSR displays (77px); constants in `ui_prefs.rs`
- **Rounded sequencer step buttons** — rounding increased to 22% of pad size; neumorphic bevel uses rect_stroke pairs so highlights follow the rounded shape
- **Scaled envelope display** — decay/ADSR height scales with XY pad size (30% of xy_size, configurable via ENV HEIGHT override); width spans both pads
- **Huth ANSI terminal output** — `log::info!` LLM response lines and thinking tokens emit ANSI 24-bit color escape codes for note names, frequencies, and MIDI numbers when stdout is a TTY; matches in-UI log colorization
- **Huth piano key labels** — white and black key labels on the piano display use their Huth chromatic color instead of a flat gray
- **Header heat slider width** — heat slider fills all available header width; tier name (COOL/WARM/HOT/FIRE/CHAOS) and percentage painted as overlays on the slider rather than consuming separate fixed allocations
- **VRAM/RAM bar visibility** — memory bars drawn with an explicit gray-38 track so the full bar extent is always visible on the dark background; fill brightens to gray-160 above 85% usage
- **show_cables default on** — rack cables shown by default for new sessions
- **Thinking token UX** — toggle button label shows `{persona} (think)`; thinking lines rendered in a darker gray in the in-UI log; thinking forwarded to console via `log::info!`
- **Huth note labels in step cells** — active bass/hoover/AN1X step buttons show the note name (e.g. "C4") in Huth color above the velocity dot; `huth_note_cell` shows label at top-center; only when pad size ≥ 26 px
- **Per-voice FX send matrix** — compact grid at top of FX panel: voice rows (BASS/808/909/HOV/AN1X/AMEN/NOISE) × FX columns (REV/DLY/CHR/PHS/WVS/BIT/EQ/CMP/TAPE/DRV/RING/AUTO); click cell to toggle rack cable and recompile FX plan immediately
- **Autosave interval setting** — Preferences → System tab; Immediate / 5s / 30s / Manual; throttled via `last_save_time`; persisted in session.json
- **Even control spacing** — `even_group_width()` + `glass_group_fill()` helpers distribute glass groups evenly across panel width; applied to drum panels (Kit A/B) and FX panel (max 4 cols)
- **Hoover LP+BP mix** — Chamberlin SVF now mixes lowpass (body) with bandpass (resonant peak); amount scales with resonance param; tanh soft-clip prevents harshness; tighter q curve
- **Separate LLM temperature slider** — `llm.temperature: f32` (0–2, default 0.9) is now a first-class field decoupled from `llm.heat` (mutation rate); temperature is sent directly to llama-server; TEMP DragValue appears in the LLM strip header alongside the HEAT slider

## Intelligence

- Heat controls mutation rate and top_p widening (top_p widens with heat); CHAOS tier (≥90%) adds explicit "maximum disorder" instruction to system prompt
- TEMP slider (0–2) controls inference sampling temperature independently of heat; default 0.9

## Testing and build

- Unit tests across submodules (seq_tests, state_tests, llm_tests, audio::analysis, jam_tools_tests, music_api_tests, ui::note, ui::llm_strip), split at 1000-line limit per file
- 479 unit tests total
- 39 LLM integration tests in 3 suites: `llm_suite` (core), `llm_suite_style` (artist refs), `llm_suite_theory` (music theory + producer lingo)
- Pre-commit hook: fmt + clippy + tests + 1000-line LOC limit
- `scripts/run-tests.sh --coverage` - HTML coverage report (lcov)
- Cross-compile to Windows EXE via `cargo-xwin` + `scripts/build-all.sh`
- `scripts/download-models.sh` - Gemma 4 E4B (default), Qwen3-8B, Qwen3-14B, DeepSeek-R1 7B/14B
- Windows `.bat` equivalents for all scripts (`start.bat`, `scripts/*.bat`)
- **CI/CD security** - `ci.yml` runs tests + tarpaulin + Codecov on `main` and `develop`; `release` job on `v*` tags builds Linux+Windows in GH Actions (no local builds), attaches `.sha256` sidecars and SLSA level-2 build provenance attestation
- Release zips include start scripts (`start.sh`/`start.bat`) and download helpers

## v0.6.x additions

### Analysis modules (rackable, FxMod zone)

- **Spectrum analyser** (`ModuleKind::SpectrumAnalyzer`) - 1024-point FFT via rustfft, 64 logarithmic frequency bands (20 Hz - 20 kHz), exponential smoothing knob, peak-hold markers with slow decay, grayscale bar display, 320px wide
- **Stereo correlation meter** (`ModuleKind::StereoMeter`) - phase correlation bar (-1 to +1) and L/R balance indicator; stereo ring buffer from audio callback; `stereo_correlation()` pure function in analysis.rs
- **Activity timeline** (`ModuleKind::ActivityTimeline`) - structured scrollable log of agent actions with relative timestamps, action tags (RSP/THK/UPD/NEW/DEL/YOU/SYS), persona names, 500-entry rolling buffer

### Presets and controls

- **Gabber kick preset** - `apply_gabber_kick_preset()`: extreme pitch sweep (0.9 depth, 0.6 time), heavy clip (0.8), button in Kit A panel
- **Bipolar param_control** - `param_control_bipolar()` maps -1..+1 to 0..1 for knob display; bass osc_detune now uses knob instead of DragValue
- **Step probability indicator** - active step buttons show a corner dot when probability < 100%; brightness scales with probability

### Per-module scaling and layout

- **Context-sensitive Ctrl+MW zoom** - over a module card: scales all modules of that kind; over empty space: global UI scale; `detect_ctrl_zoom()` with `ZoomTarget` enum
- **Per-kind scale storage** - `HashMap<ModuleKind, f32>` on ImpulseApp; scale affects content (knobs, margins, spacing) but not title bar height
- **View menu** - Compact All (0.6x), Expand All (1.0x), Arrange (canonical order), Reset Layout (clear + arrange); `arrange_canonical()` on RackState

### Lock state visualization

- **Knob mode visuals** - body darker when UserOwned, brighter when LlmFocus; catch-light and chrome rim shimmer at 1 Hz on Focus knobs (grayscale animated)
- **Slider mode tinting** - track background darker (U) / brighter (F); fill color varies per mode
- **Ctrl+click cycling** - Ctrl+click any knob cycles Free / UserOwned / LlmFocus; sliders have a dedicated ·/U/F mode button

### Footer and header

- **Footer mode indicators** - [Ctrl] [Tab:BACK] with tooltips; highlight when active
- **Header agent status** - compact round-robin display after HEAT slider; pulsing dot + persona name per enabled agent; bright when inferring, dim when idle

### Wizard improvements

- Removed redundant Skip button; "Resume" shown only with prior session
- Fresh install requires preset selection ("Start" disabled until chosen)
- Rack hidden + sequencer stopped while wizard is visible
- Clean-slate preset application (removes all existing agents first)

### Ambient / textural synthesis

- **Long envelopes** - AN1X ADSR attack up to 10s, release up to 30s for glacial pads; bass 303 decay extended to 5s
- **Granular texture module** (`ModuleKind::GranularTexture`) - new voice: loads WAV via `AudioCommand::LoadGranular`, plays up to 32 overlapping Hann-windowed grains with density, size, position, jitter, pitch scatter, spray params; true stereo output with per-grain pan law; full rack/UI/LLM integration
- **Tape delay with modulation** - wow/flutter LFO modulates delay read position (fractional interpolation), soft-clip tape saturation on feedback, max time extended to 2s; `delay_wow_flutter`, `delay_saturation` params
- **Reverb freeze** - `reverb_freeze` bool sets comb feedback to 1.0 and input to 0.0; tail holds indefinitely for drone/ambient
- **Pad presets** - 4 AN1X presets: warm pad, evolving texture, glass pad, sub drone; meditation style in styles.json; dark/space ambient baselines now enable AN1X with pad settings
- **Noise voice improvements** - AR envelope (attack 5s, release 10s), filter LFO (0.05-10 Hz), sample-and-hold modulation (0.5-20 Hz) for rhythmic texture
- **Cross-modulation** - bass osc → AN1X pitch FM (±24 st), noise → bass filter cutoff; `xmod_bass_to_an1x_pitch`, `xmod_noise_to_filter` params

### DSP improvements

- **Per-voice bass params** - `BassVoiceParams` struct snapshotted independently for all 4 bass voices; each voice reads its own cutoff/resonance/waveform/filter mode; voice 0 synced with LFO/free-EG modulation
- **Sidechain compression** - kick (808+909) ducks bass/pad/hoover/granular; `sidechain_amount`, `sidechain_attack` (0.1-50ms), `sidechain_release` (10-500ms)
- **Multiband compressor** - 3-band crossover at 200 Hz / 3 kHz with independent per-band envelope followers; `compressor_multiband` param toggles mode
- **Stereo width control** - chorus-based decorrelation on master output; `stereo_width` (0=mono, 0.5=normal, 1=wide)

### UI/UX improvements

- **Clickable footer mode toggles** - double-click Ctrl/Alt/Tab indicators to lock mode on without holding key; locks stored in egui temp data, read by zoom/widgets/cables
- **Per-module collapse** - click title bar drag zone to collapse/expand module cards; state stored in egui temp data per module ID
- **Module drag reorder polish** - insertion line indicator during drag; undo support on reorder
- **Keyboard shortcuts help overlay** - ? or F1 toggles foreground overlay listing all shortcuts
- **Undo for agent changes** - `push_history()` before agent spawn/dismiss mutations

### Visualization

- **CRT scan-line overlay** - scan lines (6px spacing, alpha 18) + edge vignette; toggled via `crt_effect` in UiPrefs
- **Ring scope** - polar waveform plot of scope buffer with simulated write-head marker; displayed alongside linear oscilloscope

### Intelligence improvements

- **Agent memory** - `_comment` snippets persisted in per-agent `memory[]` (max 20); injected into system prompt section; survives session restart via session.json serialization
- **Style learning** - `observe_user_edit()` records "user prefers high/low X" into `style_observations[]` (max 10); injected as learned preferences in system prompt; wired into bass panel (fires on extreme knob positions >0.7 or <0.3)
- **Inter-agent messaging** - `SendHint` LlmAction via JSON `send_hint` field; hints queued in target agent's `pending_hints[]` (max 5); consumed on next inference cycle and injected into prompt

### Refactoring and test coverage

- **479 unit tests**; suites: `llm_apply_tests` (68), `persistence_tests` (25), `helpers_tests` (7), `music_tests` (13), `dsp_tests` (16), `fx_plan_tests` (7), `vram_tests` (9)
- **`rack.connect_control(from_id, to_id)`** - replaces 8-line PortRef boilerplate at 6 call sites
- **`spawn_agent()` pure function** - transitions.rs; wizard.rs and SpawnAgent handler refactored to use it
- **`format_llm_display()` pure function** - extracted from drain_llm_outputs into transitions.rs
- **`BassVoiceParams` struct** - per-voice AudioParams snapshot
- **Bass303 extracted** to `src/audio/dsp/bass303.rs` (line-limit split)
- **DSP utilities extracted** to `src/audio/dsp/dsp_util.rs` (midi_to_hz, tanh)
- **Samplers extracted** to `src/audio/dsp/samplers.rs` (AmenVoice, GranularVoice)
- **Dead code removed** - `sync_default_agent`
- **Windows code-signing** - signtool step in `build-all.bat` (set SIGN_CERT + SIGN_PASS)
