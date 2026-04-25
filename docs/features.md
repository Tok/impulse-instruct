# Impulse Instruct - Implemented Features

A detailed log of what's built.

---

### Auto-retry on lane failure with temperature bump

- When a lane inference returned an error (parse failure, repair
  giving up, or transient server error) the pipeline used to log it
  as `LaneFailed` and move on, leaving that lane silent for the
  whole turn.  Common failure mode: grammar-constrained decode hit a
  dead-end token sequence and the JSON came back truncated.
- New helper `infer_lane_with_retry` wraps every lane call: first
  attempt uses the caller's `SamplingParams` as-is; on Err it logs
  the first failure and retries once with `temperature +
  LANE_RETRY_TEMP_BUMP` (0.1, clamped to 2.0).  The bump perturbs
  the sampler enough to break out of the stuck-token mode that
  triggered the initial parse failure.
- One retry only — chained retries would stall the whole pipeline
  if a lane is fundamentally broken (e.g. schema mismatch).  Both-
  attempt failures still propagate to `LaneFailed` so the UI / logs
  see the original error path.
- 4 new pipeline tests cover the retry success path (lane applies
  on second attempt, retry runs at the bumped temperature),
  both-attempts-fail propagation, the clamp-to-2.0 invariant for
  high base temperatures, and a no-retry-on-first-success sanity
  check that locks the call count.  `MockBackend` gained a
  `MockResp::{Ok, Err}` queue type so failure can be injected at a
  specific call slot without touching the existing `with_responses`
  call sites.

### Rackable viz modules — bar oscilloscope + event stream as modules

- The header used to be the only home for the colored phosphor
  oscilloscope (`draw_scope_colored`) and the scrolling note /
  drum event stream.  Both are now rack modules, joining the
  pre-existing `SpectrumAnalyzer`, `StereoMeter`, and
  `ActivityTimeline` visualisers in the FX/Mod zone.
- New `ModuleKind::BarOscilloscope` (4×2 grid) wraps
  `scope_footer::draw_scope_colored` — same buffer + history
  source as the header path, so phosphor trails carry over without
  reimplementation.
- New `ModuleKind::EventStream` (6×2 grid) wraps
  `widgets::event_stream` and computes the smooth-step
  interpolation locally from `bpm` / `last_step_time` /
  `last_step_global` so the scroll speed matches the header.
- Both kinds opt out of audio output and mod inputs (visualisers
  read existing buffers / state directly), default to FxMod zone,
  and appear in the Add menu next to the other analysis modules.
- 2 new enum-method tests lock the FxMod placement, distinct
  labels, and the no-audio / no-mod-io invariant against future
  enum additions silently re-purposing them.

### Paginated bank selector — all 64 banks reachable from the strip

- `MAX_BANKS = 64` already supported chains up to that length (Bach
  III at a 32nd-note grid imports as ~48 banks), but the BANK strip
  in the sequencer header only rendered the first 8 — slots 9-64
  were edit-unreachable from the UI.
- The strip now paginates 8 slots at a time with `‹ ›` arrows and a
  page indicator (`1/8`, `2/8`, …).  Cells widen to 18 px on pages
  past the first so 2-digit numbers fit, and slot labels stay
  1-based throughout (`9`, `10`, …, `64`).
- Page state lives in egui's per-frame data — no AppState bloat.
  When the strip first renders it auto-jumps to whichever page
  contains `pattern_edit`, so machine-generated long imports flip
  into view without manual navigation; subsequent arrow clicks
  override the auto-track until the session ends.
- Bank cells keep their click-to-swap / right-click-to-write
  semantics, so authoring a bank past the original 8 works the
  same way as the first page.

### Pattern morphing on chain advance — step-by-step crossfade

- New `ChainSlotOverride.morph_bars: u8` field (0 = classic hard
  cut, 1..=8 = N-loop morph).  When the chain advances into a slot
  with `morph_bars > 0`, the audio thread keeps the prior pattern
  playing and stashes the new pattern as `AppState.chain_morph =
  Some(ChainMorph)`.
- On every subsequent loop boundary, `morph_tick()` increments
  `bars_done` and replaces a growing fraction of step indices with
  the target's same-index step.  Index ordering is **bit-reversal
  dispersal**: rank 0 → index 0, rank 1 → index N/2, rank 2 → index
  N/4, etc.  This makes the rhythm gain the new pattern's
  character evenly across the bar instead of swapping front-half /
  back-half.
- Step counts and BPM/swing apply immediately (transport changes
  shouldn't lag behind the morph); only the per-step active /
  velocity / note / accent / cond / probability fields evolve over
  the morph window.  Final tick snaps the live sequencer to the
  full target and clears `chain_morph`.
- Morphed voices: drum patterns (all kinds), bass voice patterns +
  legacy mirror, hoover, an1x, pluck, wavetable.
- New `set_chain_slot_morph()` transition (clamps 0..=8) and a
  matching `morph` drag-value in the chain-slot popover (next to
  `×repeats`).  `POST /api/song` accepts `morph_bars` inside each
  override entry — JSON path is `chain_overrides[i].morph_bars`.
- 13 morph tests cover bit-reversal rank purity / range,
  threshold semantics (no swap at 0%, full swap at 100%, quarter
  progress swaps a quarter), the dispersal-first invariant
  (index 0 always swaps first), final-tick wholesale replacement,
  intermediate-tick partial replacement on a 64-step pattern,
  step-count / tempo immutability mid-morph, bass voice 0
  progressive replacement, and `morph_bars` clamping (0→1, 100→8).

### Conditional triggers — per-step every-Nth-cycle gating

- New `cond: u8` field on `Step` and `TB303Step` (2-bit semantic:
  0 = always, 1/2/3 fire every 2nd / 3rd / 4th voice cycle).  The
  cycle index is `step / voice_steps` so each voice keeps its own
  loop count even under polymeter.
- DSP gate lives in `cond_gate(voice_cycle, cond)` in
  `src/sequencer/mod.rs` and is wired into all six trigger emitters
  (drum, bass, hoover, an1x, pluck, wavetable) — a step that fails
  the cond is skipped entirely (no probability roll, no event).
- LLM schema/apply: `bass_conds` / `bass2..4_conds` and
  `kick_a/b_conds`, `snare_a/b_conds`, `hat_c_conds`,
  `hat_o_conds`, `clap_conds`.  Same compact array format as
  accents/slides; clamped 0–3.
- Step button paints a tiny digit ("2" / "3" / "4") in the
  top-left corner when `cond > 0`, dimmer when the step is
  inactive.  Read-only for v1 — users author cond patterns via
  the LLM, click-to-cycle UI is a follow-up.
- 8 cond tests lock purity, defaults, drum gating (cond=1 fires
  2× over 4 cycles, cond=3 fires 3× over 12 cycles), bass gating,
  and the apply-path round-trip.

### Per-step velocity curves — fractional bass accents

- `TB303Step.accent` was always a `f32` and the LLM
  `bass_accents` schema accepted floats, but the sequencer panel
  rendered each step as a binary "A on / off" button and the only
  edit path was a 0↔1 toggle.  Every pipeline tick that re-applied
  user-touched defaults flipped fractional accents back to binary.
- Each ACCENT cell now paints a vertical fill from the bottom whose
  height matches the stored value (a 0.4 accent → 40%-tall bar).
  Click still toggles 0↔1 (preserves the binary muscle-memory),
  drag-vertical sets the value from cursor y (top → 1.0, bottom →
  0.0), hover + scroll-wheel adjusts in 0.1 increments.
- New pure transition `set_bass_accent_voice(state, vi, step,
  value)` writes a clamped 0..=1 directly.  Voice 0 mirrors into
  `bass_pattern` for legacy compat; voices 1-3 only touch their own
  `bass_patterns[N]` slot.
- 5 tests lock the legacy-mirror invariant, voice isolation,
  unit-interval clamp, out-of-range voice clamp, and a fractional
  `bass_accents` LLM round-trip.

### Polymeter — global tick no longer wraps at MAX_STEPS

- The voice-indexing math always did `step % voice_steps`, so per-
  voice step lengths could already differ — but the global tick
  counter wrapped at `MAX_STEPS = 64`, silently dropping one slot
  every 64 ticks for any voice whose length didn't divide 64.  A
  5-step bass against a 16-step kit jumped from bass slot 3 at
  tick 63 straight to slot 0 at tick 64, skipping slot 4 every
  cycle.
- This commit removes the wrap.  `step` keeps growing as `usize`;
  on 32-bit platforms it overflows after ~13 years of continuous
  play at 16 steps/sec, so an explicit wrap is unnecessary.  Voice
  indexing applies its own modulo and is unchanged.
- `loop_count` previously incremented when wrap-detection saw
  `step < prev_step`.  It now derives from
  `step / seq.steps.max(1)`, so a "loop" is one full cycle of the
  configured global step count.  Chain advancement (which fires
  when `loop_count` changes) and `prob_hit`'s seed variation keep
  working with the same semantics.
- `audio/mod.rs`: `global_step_count` delta simplifies to plain
  `curr - prev`; the wrap-fallback branch is gone.
- 3 new tests in `polymeter_tests.rs` lock the cross-rhythm
  contract: a 5-step bass against a 16-step kit fires its full
  LCM(5,16)=80 cycle correctly; voice indexing stays continuous
  across the would-be MAX_STEPS boundary (slot 3 → slot 4 → slot 0,
  no skip, no double-fire); `loop_count` tracks `seq.steps`
  cycles.  The existing wrap-test was renamed + inverted.

### Native bass LFO `Pan` target + per-voice `lfo_phase` offset

- **Replaces the Bach demo's 10 Hz Python pan loop** with a DSP
  path: two voices configured once, audio thread animates the
  stereo motion, zero HTTP traffic, zero state-lock contention.
- **`BassLfoTarget::Pan`** added to the enum (label "PAN", cycle
  rotation Amplitude → Pan → Off).  When selected, the LFO writes
  `output * lfo_value * 0.5` to the voice's `pan_side` field every
  sample; the master mixer sums per-voice `pan_side` into the
  global pan_side bus, additive on top of the existing `bus_bass *
  bass_pan * 0.5` static-pan path so non-Pan patches behave
  identically.
- **`bass.lfo_phase` 0..1** added to `BassState` — applied to the
  running LFO phase before the waveform lookup, so two voices
  sharing a rate can be 90° apart (0.25), anti-phase (0.5), etc.
  The Bach scenario now sets voice 0 to `phase=0.0` and voice 1 to
  `phase=0.5` and gets a perfect anti-phase pan sweep for free.
- **UI**: PHASE knob next to RATE / DEPTH on the bass LFO group.
- **LLM schema**: `bass.lfo_target ∈ {off,pitch,pwm,cutoff,amp,pan}`
  (with `stereo` accepted as an alias) + `bass.lfo_phase` 0..1.
  As part of the same change the previously-undocumented
  `lfo_rate` / `lfo_depth` / `lfo_delay` / `lfo_bpm_sync` /
  `lfo_sync_beats` got added to the bass schema too — they were
  always wired through `apply_bass_update` but the schema's
  `additionalProperties: false` was rejecting them.
- 7 tests cover the enum cycle / label, default state, LLM apply
  for the new fields + per-voice update path, lock respect on
  `bass.lfo_phase`, non-Pan targets leaving `pan_side` at zero,
  and the key behavioral check — two voices triggered at the same
  note with phases 0 / 0.5 produce per-sample `pan_side`
  contributions whose pair sum averages near zero across an 8 k-
  sample window while the individual peak stays audibly nonzero.

### Wavetable voice (Serum-format frame stacks, single-table scan)

- **New `ModuleKind::WavetableVoice`** — user-supplied `.wav` files
  are split at load time into 2048-sample single-cycle frames
  (Serum convention).  POSITION knob fractionally indexes the frame
  list with linear interpolation between adjacent frames; PHASE
  OFFSET shifts the read inside each frame so users can detune the
  cycle's start without retriggering.
- **DSP** (`src/audio/dsp/wavetable.rs`): per-trigger phase reset,
  per-sample phase advance at `freq * 2048 / sr`, lerp inside the
  active frame and lerp again across adjacent frames so POSITION
  sweeps morph smoothly.  Same fast-attack / slow-release amp
  envelope as Pluck masks retrigger clicks; accent gain follows
  the TB303Step accent for per-step articulation.
- **State** (`src/state/wavetable.rs`): enabled / position /
  phase_offset / volume / pan / `pitch_offset_semi` / `wave_path`.
  Pattern + step count live on SequencerState as `wavetable_pattern`
  / `wavetable_steps`.
- **UI**: panel mirrors Pluck — ON/OFF + LOAD WAV (portal-backed
  picker) + filename label; SCAN group (POSITION / PHASE) + MIX
  group (VOL / PAN / PITCH ±24 st with 0.5-detent knob).  The
  sequencer panel gains a WTBL row when an enabled WavetableVoice
  is in the rack and reaches MASTER.
- **File loading**: `AudioCommand::LoadWavetable` carries the
  resampled mono buffer to the DSP, which re-derives `frame_count`
  and Arc-swaps the buffer in place — no allocation on the audio
  thread.  UI polls `wavetable.wave_path` so `/api/wavetable`
  writes from external scripts surface in the audio path on the
  next frame.
- **API**: `POST /api/wavetable { path | random: true }` mirrors
  `/api/conv_reverb`; reads from `samples/wavetables/`.  README
  updated with pointers to free wavetable libraries.
- **LLM schema** exposes `wavetable.{enabled, position,
  phase_offset, volume, pan, pitch_offset_semi}` +
  `wavetable.wavetable_steps` / `wavetable.wavetable_notes`.
  Per-field lock paths via `apply_wavetable_update`.
- 11 tests cover defaults, sequencer pre-alloc, module flags, LLM
  voice + sequencer apply, lock respect, silent-without-table
  contract, audible-after-trigger, autocorrelation period match
  (440 Hz sine table → 109-sample period at 48 kHz, on-period
  correlation > 0.8 and clearly above half-period), and load-time
  frame-split correctness via POSITION-knob probing.

### Karplus-Strong plucked-string voice

- **New `ModuleKind::PluckString`** — melodic voice filling the dry-
  string gap between acid bass and AN1X pad.  Own sequencer lane
  (`pluck_steps` / `pluck_pattern`), own `PluckTrigger` +
  `PluckGateOff` events, own gate counter on `ClockState`.
- **DSP** (`src/audio/dsp/pluck.rs`): classic Karplus-Strong — 16 k-
  sample pre-allocated delay buffer, per-trigger white-noise
  excitation fill sized to `round(sr / freq(note))`, 2-tap averaging
  feedback `(y + prev) * 0.5 * damping` with a single read+overwrite
  pointer (one slot covers the full round-trip).  `brightness`
  applies a one-pole LP on the *output* tap — independent of the
  feedback damping so users can tame the raw edge without
  shortening the decay.  Fast-attack / slow-release amp envelope
  masks retrigger clicks; `accent` scales the peak gain per step.
- **State** (`src/state/pluck.rs`): enabled / damping / brightness /
  volume / pan / `pitch_offset_semi` (-24..+24).  Damping knob maps
  to a feedback coefficient in 0.92..0.995 — low end plucks fast,
  high end sustains for seconds.
- **UI**: voice panel with ON/OFF toggle + TONE group
  (damping / brightness) + MIX group (vol / pan / pitch offset
  mapped via the standard 0..1 knob with 0.5 at the zero-detent).
  Sequencer panel gains a PLUCK row when the rack contains an
  enabled PluckString module that reaches MASTER.
- **LLM schema**: `pluck.{enabled, damping, brightness, volume, pan,
  pitch_offset_semi}` + `pluck.pluck_steps` / `pluck.pluck_notes`
  for the sequencer lane.  Per-field lock paths honour UI touches
  through pipeline writebacks via `apply_pluck_update`.
- 9 tests: state defaults, sequencer pattern pre-alloc, module-kind
  flags, LLM voice-field + sequencer apply, per-field lock respect,
  silence before trigger, audible output after, and an auto-
  correlation check that the output's period matches `round(sr /
  freq)` (zero-crossing counting would inflate the rate from K-S's
  harmonic-rich noise burst, so the test correlates the signal
  against itself at the delay-line lag instead).

### Mid/side master processing on `MasterOutput`

- **Six new knobs on the master module**: GAIN / TILT / SAT per side,
  running after the raw (mid, side) computation and before L/R
  recombination in the audio master stage.  The existing
  `fx.stereo_width` still handles global width scaling; these add
  per-side tonal control + saturation character.
- **DSP** (`src/audio/dsp/ms_master.rs`): per-side 200 Hz low shelf +
  5 kHz high shelf pair whose gains track the tilt knob (opposing,
  ±6 dB), gain pre-trim (±12 dB), and arctan soft-clip saturator.
  Biquads reuse ParamEq's RBJ coefficient helper + dirty-check
  caching; flat defaults (0.5 on gain/tilt, 0 on sat) short-circuit
  to pass-through so an un-touched master adds no colour.  Soft-clip
  scaled by 2/π so over-driven input lands strictly inside ±1 and
  the downstream master clamp never hits the saturator output.
- **UI**: MasterOutput grid grows to 2 rows — row 1 keeps the
  original MASTER VOLUME + voice-presence strip, row 2 adds
  `M/S | MID G / MID T / MID S | SIDE G / SIDE T / SIDE S` knobs
  (the usual grayscale params).
- **LLM schema**: `fx.ms_{mid,side}_{gain,tilt,sat}` exposed with
  per-knob lock paths; `apply_fx_update` + `fx_field_mut` +
  `jam_tools` all wired for ramps.
- 10 tests cover flat-default transparency, ±12 dB gain mapping,
  side-gain narrowing, tilt raising a 6 kHz sine's RMS when
  treble-biased, saturation bypass at 0, mid-range lift at 1, and
  the strict ±1 bound on over-driven saturation.

### Standalone pitch-shifter FX (PSOLA, ±24 st + feedback)

- **New `FxPitchShift` module + `FxStep::PitchShift`** — dedicated
  continuous pitch shifter, distinct from `FxAutotune` (upward
  snap-to-key).  Covers harmonies (±7 st fifth), octave doubles
  (±12 st), and two-octave extremes for gabber / vaporwave stunts.
- **Two-grain overlap-add PSOLA with explicit grain respawn** in
  `src/audio/dsp/pitch_shift.rs`.  Each grain has its own triangular
  envelope + read position; when a grain's envelope wraps 1 → 0 the
  grain is silent, so the respawn places its read head one grain
  length behind the write.  The walk over the grain cycle stays in
  freshly-written audio for both upshift and downshift, eliminating
  the read-overtakes-write failure mode of naive single-envelope
  implementations.  16 k-sample ring, 2048-sample grains (~43 ms
  @ 48 kHz).
- **Feedback** (clamped to 0.95) pipes the wet back into the input
  so stacked shifts accumulate — `+7 st` with high `fbk` ladders
  into +7, +14, +21 … fifth-stack harmonies without re-instancing
  the module.
- **State fields**: `fx.pitch_shift_semi` (-24..+24 st), `_fine`
  (-100..+100 cents, additive), `_mix`, `_fbk`.  Stored as musical
  values so LLM writes feel natural (`{"pitch_shift_semi": 7}`);
  the UI knob maps them onto 0..1 with 0.5 at the zero-offset
  detent.
- **Card extracted** to `src/ui/rack_content_pitch_shift.rs`; the
  drag/cable helpers at the bottom of `rack_content.rs` also moved
  to `rack_content_drag.rs` in the same refactor so the main file
  has room for future FX cards.
- Schema + prompt + apply path wired through `fx.pitch_shift_*`;
  ramps work via `jam_tools`.
- 6 new tests: defaults, LLM write, lock respect, module flags, two
  bypass paths (mix=0, zero-semi), and the key correctness check —
  1 kHz sine through ±12 st produces wet at ~2 kHz / ~500 Hz
  measured by zero-crossing count (10 % window).

### 8-band parametric EQ with draggable curve editor

- **New `FxParamEq` module + `FxStep::ParamEq`** — lives alongside the
  legacy fixed 3-band `FxEq` so existing sessions don't need to
  migrate.  Rackable from the FX menu, allows multiple instances for
  send-return colour / master-bus shaping patches.
- **8 bands** — two shelves bracketing six peaks from 100 Hz to 15 kHz
  by default.  Per-band `{kind: LowShelf|Peak|HighShelf, freq_hz,
  gain_db, q, enabled}` stored in `FxState.param_eq_bands: [ParamEqBand; 8]`.
  Defaults at 0 dB so adding the module is transparent until the user
  moves a node; disabled bands stay dim but keep their freq/gain/Q
  stored for easy A/B.
- **DSP: Transposed-Direct-Form-II biquads** via the RBJ Audio EQ
  Cookbook coefficient formulas in `src/audio/dsp/param_eq.rs`.
  Per-band dirty-check caches the last-known source params so coefs
  only recompute when the UI / LLM moves a knob.  Zero-gain or
  disabled bands short-circuit to pass-through so the cascade stays
  cheap even at maximum band count.
- **Pure `band_magnitude` + `cascade_db`** helpers — the UI curve
  renderer and the DSP share one response function, so what you see
  on screen matches what the filter does.
- **Curve editor widget** (`src/ui/widgets/param_eq_curve.rs`) —
  log-freq × ±18 dB plot with an octave grid and ±6 / ±12 dB reference
  lines.  One draggable handle per band: primary drag moves freq +
  gain, scroll over a handle adjusts Q (narrower / wider), right-click
  cycles kind (shelf/peak), double-click toggles enabled.  Below the
  curve a monospaced readout strip lists every band's kind short-
  name + freq + gain dB; disabled bands render dimmed.
- **LLM schema**: `fx.param_eq_bands` is an 8-slot positional array;
  each entry is an object with `{kind: 0|1|2, freq, gain, q, enabled}`,
  with `null` skipping that slot.  Per-band lock paths
  (`fx.param_eq_bands.N.<field>`) honour user edits through pipeline
  writebacks, mirroring the `bass_voices.N` lock granularity.
- Recursion limit for the schema macro bumped to 512.
- 11 tests cover the coefficient math, the cascade DSP, the LLM
  apply path, per-band locking, and the module-kind wiring.

### Convolution reverb — IR-driven FxStep with stereo mid/side

- **New `FxStep::ConvReverb` + `ModuleKind::FxConvReverb`** — rackable
  FX module with MIX / SIZE / PREDELAY / DAMP / LOWCUT / WIDTH knobs +
  REVERSE toggle + LOAD IR picker on the front panel.  3-knob XY pad
  expansion mirrors the other FX cards; back-panel exposes 3 Selector
  mod-input jacks.  `allows_multiple()` so users can stack two
  ConvReverbs for send-return colour patches.
- **Partitioned overlap-save FFT convolution** via `rustfft` in
  `src/audio/dsp/conv_reverb.rs`.  IR split into 1024-sample partitions,
  forward-FFT'd at load time; per-block one forward FFT of
  `(prev || current)` input, freq-domain multiply-accumulate across an
  FDL ring buffer, one IFFT for mono / two for stereo, back half of
  the IFFT output is the valid wet.  Startup latency = 1 partition
  (21.3 ms at 48 kHz).  Fallback when no IR is loaded: predelay +
  LP (damp) + HP (lowcut) stub, so the module always responds audibly.
- **True stereo IRs** — `load_wav_stereo_to_engine` preserves the L/R
  split through resampling; `ConvReverb` convolves each channel
  separately and emits `mid = (L+R)/2` as the FX return + latches
  `side = (L-R)/2 · width` into the master mid/side bus (same path
  `fx_pan_side` / `granular_side` already use).  Mono IRs degrade
  gracefully (side = 0).
- **REVERSE flag** reverses the IR data before FFT so the partition
  cache stays branch-free in the hot path.  SIZE truncates the active
  partition count at process time (0 still keeps 1 partition so early
  reflections survive).  PREDELAY 0..200 ms via 16 k-sample ring
  buffer; fixed to be a true zero-delay read at knob=0.
- **POST `/api/conv_reverb` { path | random: true }** mirrors
  `/api/amen`: writes `fx.conv_reverb_ir_path`, UI polls and pushes
  `AudioCommand::LoadImpulseResponse { data, channels, reversed }`.
  Drop WAVs into `samples/impulses/`.  LLM schema exposes every knob
  + reverse flag at `fx.conv_reverb_*`; ramps and XY pairs wired
  through `apply_fx_update` / `fx_field_mut` / `jam_tools`.
- 14 tests in `src/tests/conv_reverb_tests.rs`: defaults, LLM writes,
  lock respect, rack-module flags, kind→step mapping, ramp start
  value, DSP bypass, wet blend bounds, load/clear safety, unit
  impulse identity, delayed dirac, reverse flip, stereo side split,
  size-knob truncation.

### Lane-local writeback — preserve `api_params` during jam cycles

- **The bug.** `run_pipeline`'s per-lane writeback in `src/llm/mod.rs`
  copied full sub-structs (`bass_voices`, `kit_a`, `kit_b`, `lfo`,
  `fx`, …) from the pipeline's start-of-pipeline snapshot back to the
  live `AppState`.  Any `api_params` or UI edit made *while* the
  pipeline was in flight got silently reverted when the lane finished
  — voice-2 `enabled` flipping back to false, kit volumes re-rising
  to defaults, LFO targets resetting to `None`, `fx.stereo_width`
  snapping back to 0.5.  The original code had a special-case
  exclusion for `rack` with a comment explaining the exact failure
  mode; every other user-owned field had been hit by the same trap.
- **The fix.** `on_lane_applied` callback signature changed from
  `FnMut(&AppState)` → `FnMut(&Value, &[String])`.  It now receives
  the lane's filtered JSON output + apply scope, and mod.rs replays
  that update against the LIVE state via `apply_llm_update`.  Only
  fields the lane actually emitted get mutated; user-originated
  changes survive across every lane boundary.
- Tests updated to the new `|_, _| {}` callback arity (5 call sites
  in `src/llm/pipeline.rs`).

### FX lane schema — phaser + ring modulator writable

- `fx` lane's JSON schema + system prompt now list `phaser_mix`,
  `phaser_rate`, `phaser_depth`, `ring_mod_mix`, `ring_mod_freq`.
  The fields and their `apply_llm_update` handlers had always
  existed in `FxState` + `src/state/llm_helpers.rs`, but the
  grammar-constrained lane schema didn't mention them, so the LLM
  answered phaser / ringmod asks by writing `reverb_mix` with a
  `_comment` noting the field wasn't in the allowed list.  Matching
  range hints added to the lane prompt (phaser_mix 0.15-0.45,
  ring_mod_mix 0.05-0.2 sparingly).

### Kit density caps in lane prompts

- `kit_a` and `kit_b` lane prompts now carry explicit per-voice
  density caps per 32-step bar: kick 6-10 hits, snare 2-6, clap 2-6,
  hihat_closed 6-10 (with a "avoid 16th runs; prefer offbeat 8ths"
  note), hihat_open 0-4.  Jam cycles were otherwise prone to
  emitting `hihat_a_steps: [1,2,3,6,7,10,11,14,15,…]` — 17 hits
  that piled on the master bus and forced the peak meter into the
  CLIPPING alert.  The cap is paired with a "more hits = more gain"
  rationale so the model understands why, and an "unless the user
  prompt explicitly asks for a busy pattern" clause so requests like
  "busier clap pattern" still land.

### Pipeline — voice-2 activation + heuristic tag strip

- `api/rack/agent` (the `add_agent` HTTP endpoint) now mirrors the
  wizard's model-inheritance guard: if the requested pattern already
  matches the globally-loaded model path, `model_path` stays `None`
  and the agent inherits instead of forcing a second llama-server.
  Without this, `add_agent PULSE gemma` was resolving to the first
  `gemma-*` GGUF in lexical order — on machines with both E4B and
  26B-A4B on disk, that was the 26B-A4B thinking variant, whose CoT
  exhausted max_tokens inside `<think>` and every lane failed with
  `content:""` + `finish_reason=length`.
- `llm/pipeline.rs`: the heuristic planner now strips a trailing
  ` /think` or ` /no_think` before matching.  The think tag is
  appended by the LLM worker for the inference server, but it was
  pushing otherwise-short prompts past the heuristic's 120-char
  sanity cap, forcing the slow LLM planner path and a fallback
  `default_plan` that drops newly-enabled bass voices.
- `llm/server_pool.rs`: lane `max_tokens` bumped 1600 → 3000 for
  reasoning-model headroom; when `content` is empty but
  `reasoning_content` holds JSON, parse+repair the reasoning text as
  a salvage path before surfacing an actionable "finish_reason=length
  — /no_think?" hint.
- `llm/pipeline_events.rs`: `LaneApplied` now emits an `LlmOutput`
  carrying the lane JSON as `param_update`, so the UI console
  renders the per-lane `"<persona>: …"` activity line.  The pipeline
  refactor had previously only sent bracket-prefixed status messages
  (`[plan: …]`, `[pipeline: …]`), which the drain filter deliberately
  hides — so the LLM console had gone silent between user prompts.

### Demo recorder — persistent sink capture + `--resume`

- `create_recording_sink` switched from `pw-cli create-node adapter`
  (ephemeral — the null-sink died the moment `pw-cli` exited, so the
  function always silently returned an empty string and the script
  fell through to raw stream capture) to `pactl load-module
  module-null-sink`.  The pactl module is hosted by the long-lived
  pipewire-pulse service, so the sink persists for the whole
  recording.  Module id stashed in `/tmp/impulse-record-sink.module`
  for a clean unload; a fallback scan of `pactl list short modules`
  cleans up sinks left behind by crashed runs.
- Capture uses `parecord --device=impulse-record.monitor` instead of
  `pw-record --target <node_id>`.  The latter reports "No such
  entity" for null-sink node ids and silently falls back to the
  default source (usually silence); parecord resolves the monitor by
  stable PulseAudio name.
- `narrate` fans out TTS playback to both the default sink (live
  monitoring) and the recording sink (capture lands in the mp4).
  Without this, the isolated-sink recording had no voice-over —
  narration went to the speakers, not the capture target.
- New `--resume` flag on `record-demo.sh`: picks the newest
  `YYYYMMDD_HHMMSS` directory under `demo/output/` as `BATCH_DIR`
  and points `TTS_DIR` at its cached `tts/` subfolder.  `tts_generate`
  already short-circuits on existing `${id}.wav` files, so a retry
  skips the ~5-minute TTS pre-gen step; only genuinely new narration
  lines re-synthesize.

---

### Rack visual polish — chrome tiles + LED z-order + card resizing

- **Zone backdrop.** The rack's empty cells now show a subtle per-cell
  chrome gradient (vertical light-grey top → darker-grey bottom via
  `epaint::Mesh`) plus a hairline separator, replacing the old blank
  void.  The existing dot grid paints on top of the tiles.  Works
  identically on the front panel and the back panel (flipped).
  `draw_zone_grid_dots` → `draw_zone_backdrop`; moved to run BEFORE
  each zone's card loop so cards and UI sit cleanly above the tiles.
- **LED halo z-index fix.**  The module-card LED's extended clip rect
  now intersects with `ctx.available_rect()` (the post-header central
  panel area) so the halo can bloom into the inter-module gap but can
  never escape upward into the header log when the LLM console /
  agent card scrolls past the rack's top edge.  The agent-card LED
  gets the same treatment by clipping its Foreground layer painter to
  `ctx.available_rect()`.
- **808 kit** — re-sized from (3, 4) to (4, 5); each voice wraps its
  knobs in a nested glass pane (fixed width so pads line up
  vertically) and a 1.8× bigger XY pad (90-144 px clamp vs the 909's
  50-80 px) anchored to the top of a `horizontal_top` row.
- **Delay FX** — (2, 1) → (2, 2) so the 5-button direction / reverse
  / quantise row no longer overflows the card's right edge.
- **Agent card layout.**  Left column wraps persona/model, progress
  sub-label, T/B controls and Scope; right column holds a big
  right-flushed round-robin clock (80 px, ≈3× the old 26 px inline
  size) spanning the full height of the left column.  Instructions,
  t/s badge, conv-mode, pills and prompt override continue full-width
  below the split.  `t/s` moved directly under Instructions.

### Back-panel mod-overlay tightening

- **Per-slot spacing, keyed off slot kind + card height.**
  - `PORT_SPACING_FIXED` = 24 px (polarity + slider + % row only).
  - `PORT_SPACING_SELECTOR` = 42 px (slider row + wrapped chip strip).
  - `PORT_SPACING_COMPACT` = 24 px — applied to Selectors on 1-cell
    cards where the chip strip **inlines onto** the slider row
    instead of wrapping below.
  - New `back_is_compact(kind)` helper keyed off `grid_size(...).1 <=
    1`; 1-cell FX (reverb / chorus / phaser / ring-mod / waveshaper /
    bitcrush / EQ / compressor / tape-sat / drive / autotune / pan)
    and the `NoiseVoice` drop into compact mode.
  - `back_strip_height` sums per-slot spacing so each card gets
    exactly the strip height it needs instead of a flat multiplier.
- **Slider widget.**  `interact_size.y` 10 → 8 px.  Width clamp upper
  raised 60 → 140 px on wide cards so the depth slider resolves small
  nudges on drum kits / 4-col FX.  Compact mode shrinks it further
  (14–40 px) to reserve room for the inline chips.
- **% readout centring.**  Now lives inside an
  `allocate_ui_with_layout(28 × 12, centered_and_justified)` slot so
  the label sits vertically centred on the slider row instead of
  floating at the top of the `horizontal_wrapped`.
- `mod_start_y` bumped 28 → 32 px so the first slider row clears the
  AUD / CV / CTL label text above it.

### Sequencer transport — preserve `running` across LLM writebacks

- New pure helper `state::transport::preserve_sequencer_transport(live,
  incoming)`: applies `incoming` onto `live`'s `SequencerState` but
  keeps `running` and `current_step` from the live copy.  Fixes the
  "play button turns off after a few beats" bug — the LLM pipeline
  captures a snapshot at inference start and writes the full
  sequencer back when the lane completes; if the user pressed Play
  after the snapshot was captured, the stale `running=false` clobbered
  the live `running=true`.  Startup one-shot prompt was the common
  trigger (~3 s inference, user hits Play in between).
- Routed through the helper at every wholesale writeback site:
  - `src/llm/mod.rs` (per-lane + monolithic paths).
  - `src/ui/llm_drain.rs` (jam_cycle_done handler).
  - `src/ui/llm_strip.rs` (style baseline writeback).
  - `src/llm/mock.rs` (full-state replace; inline save/restore).
- 3 regression tests in `tests/transport_tests.rs`: live-running vs
  stopped-incoming, live-stopped vs running-incoming, and
  non-transport fields still landing.

### Song mode — timeline UI

- New `draw_song_timeline` row sits below the compact bank/chain row.
  Each chain slot renders as a Gantt-style bar (78×22 px) showing:
  pattern letter, `×N` repeat badge (when > 1), style override tag,
  BPM override tag.  The currently-playing slot gets a `theme::CHALK`
  frame + a thin playhead line whose x-position reflects how many
  repeats of the slot have played so far.
- Drag a bar onto another to reorder — swaps chain positions and their
  overrides together via the new `swap_chain_slots` transition (plus 2
  tests: atomic swap + out-of-bounds no-op).  The visual follows the
  pointer through the drag so the user sees the reorder live.
- Click a bar to open an inline popover that edits the slot's
  overrides: `×repeats` (1..=64), `style` dropdown (— / any style),
  `bpm` checkbox + drag-value (40..=300, only applied when the
  checkbox is lit).  `Clear all overrides` button reverts the slot
  to plain chain-position behaviour.
- Empty chain renders a hint line ("push bank slots above to compose
  a song") so the section doesn't silently disappear when a user
  hasn't built anything yet.

### Send-bus multi-destination sends

- `FxPlan.voice_routes` switches from `HashMap<ModuleKind, Vec<FxStep>>`
  (single chain per voice) to `HashMap<ModuleKind, Vec<VoiceSend>>`
  where each `VoiceSend { chain, gain }` is an independent parallel
  branch.  The old `voice_send_gain` map is removed — gain lives
  inside each send now.
- `compile_fx_plan`: every Voice→FX cable becomes its own `VoiceSend`,
  not just the first.  Classic "bass → reverb at 30% + bass →
  delay at 50%" patches now compile correctly.
- Audio thread: new `DspState::route_voice_sends` helper sums the
  output of every send for a voice.  Stack-friendly
  `VoiceSendsSnap { chains[MAX_SENDS][MAX_CHAIN], gains, count }`
  snapshot means the per-frame loop does zero HashMap touches.
  `MAX_SENDS = 3` covers dry + reverb send + delay send with
  headroom.
- 2 new / updated tests: `voice_send_gain_captured_on_voice_fx_cable`
  verifies single-send gain survives the refactor;
  `multiple_voice_fx_cables_produce_parallel_sends` proves two
  cables from the same voice produce two `VoiceSend` entries with
  distinct chains and gains.

### Mid-pipeline live state checks

- `run_pipeline` + `run_pipeline_via_pool` gain an optional
  `live_state: Option<&Arc<RwLock<AppState>>>` parameter.  When set,
  the lane loop re-checks `lane_is_live_pub` against the shared
  state right before firing each lane — catches modules that were
  removed / disabled between the plan-time filter and the inference
  call.  `None` preserves the pre-refactor snapshot-only behaviour
  for tests and one-shot turns.
- New `PipelineEvent::LaneSkipped { lane, reason }` variant — the
  progress UI ticks `lanes_done` without bumping `failed_count`, so
  mid-cycle removals aren't framed as model errors.
- Wired into the real inference loop so a lane for a just-removed
  module is skipped before burning an inference call.  2 new tests
  (`pipeline_skips_lane_when_module_removed_mid_cycle` confirms the
  skip fires when the live rack changes; `_keeps_snapshot_behaviour`
  confirms `None` is a pure pass-through).

### UX — touch paint, gesture zoom, LED escape, style auto-sync

- **Step-button drag-paint.**  `step_button` now returns
  `Option<bool>` (the desired active state).  Clicking is unchanged;
  pressing-and-dragging locks a paint direction at drag start
  (inactive → paint-on, active → paint-off) and applies it
  idempotently to every step the pointer enters — the natural
  behaviour for laying long hat runs or carving sections on touch
  devices.  Gesture state lives in a single egui-memory key so two
  grids can't cross-paint.
- **Multi-touch gestures on the rack canvas.**  `ctx.multi_touch()`
  is read in `rack_scroll`: two-finger vertical pan drives the
  rack `ScrollArea` offset; pinch-zoom scales `ui_prefs.ui_scale`
  (clamped 0.5..=3.0×) — tablet users can now steer the rack
  without chrome.
- **LED halos escape widget bounds.**  Step-button current-cursor
  bloom + scale-degree LED dot, plus piano-panel scale-degree LEDs,
  paint via `Order::Foreground` layer painters.  Mirrors the fix
  `agent_card.rs` applied earlier — halos no longer clip at step /
  key edges.
- **Auto-sync rack to active style on startup (opt-in).**  New
  `UiPrefs.autosync_rack_on_start` toggle (Preferences → Controls →
  Startup).  When on and a genre style is active (not `__free__` /
  `__custom__`), app launch calls `style_rack::apply` with the
  style's `rack_modules` so restarting in Classic Acid never leaves
  a Hoover behind.  Off by default — existing users keep their
  customised rack.  Round-trips through `session.json`.

### Intelligence — agent memory, style prompts, VRAM fallback, overrides

- **Agent conversation history.** `LlmAgentState.recent_outputs`
  (VecDeque, cap `AGENT_RECENT_OUTPUTS_MAX = 3`) accumulates one-line
  condensed summaries of each cycle's output (`_thinking` →
  `_comment` → truncated raw text).  Injected into the next prompt
  as `[cycle -N]` lines alongside the existing memory / hint trail,
  so agents evolve coherently instead of treating every jam cycle
  as a blank slate.  New `push_agent_recent_output` transition + 3
  tests (append+cap, empty no-op, unknown-id no-op).
- **Style prompt templates.** Styles gain an optional
  `jam_prompt_template: Option<String>` field.  When set, every jam
  re-trigger uses it instead of the generic
  "continue jamming, evolve the pattern" directive.  26 styles
  shipped with genre-flavoured templates via a bulk-edit script,
  e.g. *jungle* → "chop the amen differently, tighten the reese,
  add a snare fill".  Single `jam_prompt_for_active_style()` helper
  funnels all three re-trigger sites.
- **VRAM-aware model fallback.**  `pick_fallback_model(agents,
  global_model, candidate, available, vram_total_mb) -> Option<String>`
  picks the heaviest-yet-fitting lighter model when the spawn
  candidate blows the VRAM budget.  Wired into the `SpawnAgent`
  action handler so agent-initiated spawns gracefully downgrade
  instead of failing silently.  CPU mode (vram_total_mb = 0) is a
  no-op; never picks same-or-heavier models.  4 new tests.
- **Per-style mc_lines / themes overrides.**  New `StyleOverride
  { mc_lines, themes }` on `AppState.style_overrides: HashMap<String,
  StyleOverride>`; `effective_mc_lines` / `effective_themes` helpers
  resolve override-first, baseline-fallback.  UI editor in
  Preferences → AI → Personality lets users pick a style, edit both
  fields, save or revert.  Empty override = explicit clear, not
  fallback — so a user can silence a style's MC vocab without
  touching `styles.json`.  5 new tests.

### File menu — Open / Recent projects / style-seeded wizard

- File menu grows an `Open project…` entry that opens a native file
  dialog via `rfd` (new dep).  The picker is filtered to `.json`.
  Drop-in replacement for the old "Load latest" shortcut, which stays
  as a one-click fallback for the common case.
- `Recent projects` sub-menu lists up to 10 `project-*.json` files
  from the working directory, newest first.  Entries route through
  the same `load_project_from_path` helper as "Open…" and "Load
  latest" so error handling / logging stay uniform.
- `list_recent_projects_in(dir)` is the pure helper the UI calls with
  `"."` and tests exercise with a tempdir — 2 new tests cover the
  "newest first" ordering + filter, and the missing-dir case.
- Wizard gets an optional "or seed from style" dropdown that lets
  users pick a genre at onboarding.  When set, the rack is reshaped
  from that style's `rack_modules` via the existing
  `style_rack::apply` pipeline, baseline params are stamped, and
  `llm.active_style` is pinned — so the first jam cycle already
  inherits the genre.  Generic `RACK_PRESETS` still picked by default
  for users who don't have a style in mind.

### Send-bus routing — per-cable gain + FX→FX feedback

- Every audio `Cable` gains a `audio_gain: f32` field (default 1.0,
  range 0..=1.5).  Forward Voice→FX cables use it as a per-voice send
  amount on the first FX of the voice's chain; the rest of the chain
  processes at unity.  Captured in `FxPlan.voice_send_gain`.
- `would_create_audio_cycle` loosened to accept FX→FX cycles while
  still rejecting cycles that touch a voice / master / LLM module —
  musical feedback only makes sense between effects.  Non-FX cycles
  continue to fail-closed in both `connect()` and `strip_audio_cycles`.
- Cycle-closing FX→FX cables are classified as feedback edges at
  compile time and stored in `FxPlan.feedback_routes`.  The graph
  builder picks the back-edge deterministically (first-cable-wins
  forward DAG, rest become feedback) so saves round-trip stably.
- Audio-thread implementation: `DspState.prev_fx_output: [f32; 13]`
  keeps the previous sample of every FX type.  `apply_fx_chain`
  mixes `prev_fx_output[source] * gain` into the target's input
  before processing, then writes the fresh output back.  The implicit
  one-sample delay across samples makes the loop algebraically well-
  defined; user `audio_gain` is clamped to `FEEDBACK_GAIN_MAX` (0.95)
  at compile time so the graph can't diverge regardless of input.
- API: `POST /api/rack/cable { audio_gain }` sets the gain at cable
  creation; `POST /api/rack/cable_gain { from, to, gain }` updates an
  existing cable.  Feedback clamping applies automatically when the
  cable turns out to be a back-edge.
- 4 new tests cover FX-only-cycle acceptance, voice→voice rejection,
  feedback-gain clamping, and voice_send_gain capture.  Two existing
  tests (`cycle_rejected_by_connect`, `strip_audio_cycles_removes_cycle`)
  were flipped / deleted to reflect the new semantics.

### Song mode — per-chain-slot overrides

- `ChainSlotOverride { bpm, style, repeats }` parallels the chain vec.
  Missing / default entries preserve v1 behaviour (pattern's own
  `pattern_style` + `pattern_bpm_apply`).  The same pattern-bank slot
  can now appear twice in a chain with different overrides — e.g. the
  same 16-step loop at 128 BPM then again at 160 BPM for the outro.
- Audio-thread advance honours `repeats` (1..=64) by holding the slot
  through N pattern loops before moving on, tracked via a new
  `chain_repeat_count` counter in `AppState`.  Style overrides feed
  the existing `apply_pattern_style_on_advance` hook; BPM overrides
  force the tempo regardless of the pattern's `pattern_bpm_apply`
  flag, so v1 pattern-based transitions keep working untouched.
- New API: `POST /api/song { chain, overrides, enabled }` and
  `GET /api/song` for state snapshots.  7 new transition tests cover
  clamping (BPM 40–300, repeats 1–64), out-of-bounds no-ops, and
  atomic `set_song` replacement.
- UI still shows the flat chain row.  A proper timeline-view editor
  (Gantt bar per slot + drag-reorder + playhead scrubber) is on the
  roadmap.

### Per-step drum probability — LLM-writable

- `sequencer.drum_probabilities: { voice: [p0, p1, ...] }` exposes
  `Step.probability` (0..=1, default 1.0) to the LLM / API.  Same
  shape as `drum_ratchets`: one float array per voice key (`kick_a`,
  `snare_a`, `hihat_a`, `kick_b`, `snare_b`, `clap_b`, `hihat_b`).
  Out-of-range values clamp to `[0, 1]`; missing arrays preserve the
  stored values.
- Prompt now documents the four canonical use cases: humanised hats,
  ghost snares, tension-building under density collapses, and
  conditional fills — so the model reaches for probability instead of
  muting a step to achieve the same sparseness statically.
- Schema entry uses the shared `intensity_array` so grammar-constrained
  generation can emit it directly.  `preecho.<voice>.probability_ramp`
  remains the quick-win shortcut for lead-in windows.

### XY pad — first-class agent path

- Every FX effect gets a `fx.<name>_xy: [x, y]` JSON path that writes
  both knobs of the canonical Pair-0 pad in one update.  Individual
  knob paths (`fx.reverb_size`, etc.) still work — the XY paths are
  additions, not replacements.  Pair-1 / Pair-2 combinations stay
  reachable via the individual knob paths.
- Supported pads: `reverb_xy`, `delay_xy`, `chorus_xy`, `phaser_xy`,
  `ring_mod_xy`, `waveshaper_xy`, `bitcrush_xy`, `eq_xy`,
  `compressor_xy`, `tape_xy`, `distortion_xy`, `autotune_xy`,
  `fx_pan_xy`.
- Lock paths compose: locking `fx.reverb_xy` blocks the pad but leaves
  individual knobs writeable; locking `fx.reverb_size` still lets the
  pad move the Y axis (`reverb_damp`) without silently bypassing the
  lock.  5 new tests, 13-pad smoke suite.

### NeuTts bus volume + LFO target

- `TtsModuleState.volume` (0.0..=1.5, default 1.0) scales the TTS
  ring-buffer output before it hits the master mix.  Pipes through
  `AudioParams.tts_voice_volume` at frame boundary so the value is
  live-editable and modulatable.
- `LfoTarget::NeuTtsVolume` added (opcode 72, label `TTS.VOL`).  The
  three Selector mod-jacks on the NeuTts back panel now have a real
  target to route to — previously the selector dropdown was empty and
  the jacks showed "—".
- UI: the NeuTts front panel grows a `VOLUME` row under `TOP-P`,
  matching the Amen/Granular pattern.  Audio-thread cost: one extra
  float multiply per frame on the TTS bus.

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

### Melodic voice preecho (bass / hoover / an1x)

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
- Hoover and An1x consume the same overrides under the `"hoover"` and
  `"an1x"` preecho keys.  Their `TriggerEvent` variants carry `accent`
  and `slide` fields, and their voices scale output gain by accent (up
  to +30 % on full accent) plus extend glide time by slide (Hoover
  runs a 10–160 ms exponential approach; An1x uses
  `max(global_glide_time, slide)` so a cascade step audibly smears
  even when the global glide is off).
- 11 new tests: 8 unit tests on `preecho_melodic` (wrap-around,
  multi-anchor nearest wins, both toggles composing), 1 end-to-end
  sequencer test confirming the ramp lands on `BassTrigger.accent`
  + cascade lands on `BassTrigger.slide`, 2 apply-layer tests for
  bass-key JSON + partial-update preservation.

### Pre-echo v2

- `RampCurve` enum (`Linear` / `Exp` / `Log` / `Cosine`) shapes every
  scalar ramp (velocity / ratchet / probability / accent) via a
  `curve.apply(pos) -> f32` helper — slow-starts, fast-starts, and
  smoothstep ease-in/out in addition to v1's pure linear.  Linear is
  the default so existing configs read identically.
- `probability_ramp` overrides `Step.probability` across the lead-in
  (0.3 at earliest step → 1.0 at anchor-adjacent, curved).  Leading
  steps fire less often, building up density toward the anchor
  without user bookkeeping.
- `auto_length`: when lit, the lead-in window for each anchor is
  `gap_to_prev_anchor − 1` (wrap-aware), so uneven anchor spacings
  produce variable-length build-ups without per-anchor config.
  Single-anchor configs fall back to `length.max(4)` so the toggle
  can't silently disable the effect.
- `preecho_scale` + `preecho_melodic` collapsed into one
  `preecho_apply(step, total, cfg) -> PreechoApply` that returns a
  single struct with `velocity_mul` / `ratchet_add` /
  `probability_override` / `accent_override` / `slide_override` —
  drums read the first three, bass the melodic pair, and future
  hoover / an1x callers get one entry point.
- UI picks up a CURVE dropdown, AUTO toggle, PROB / ACC / SLD
  toggles on a new third row of the preecho editor (the first two
  rows stay as-is: voice tabs + anchor strip, then ON / LEN / VEL /
  RAT / CLEAR).  Accent / slide ramps were in the v1 config but
  never exposed in the panel — they're surfaced now alongside the
  new v2 toggles so the whole modulation vocabulary is editable.
- `note_approach` (melodic voices: bass / hoover / an1x) rewrites
  lead-in step notes so they resolve into the anchor note.  Modes:
  `Chromatic` (d-th step = anchor − d semitones), `Scale` (− d
  scale-degrees under the project's active root / scale), `Arp` (− 2·d
  scale-degrees, outlining a triad below the anchor).  Pure resolver
  `resolve_note_shift(anchor_note, shift, root, scale) -> u8` lives
  next to `preecho_apply`; it never allocates and is safe to call from
  the audio thread.  UI exposes it as an `OFF / CHR / SCL / ARP`
  dropdown shown only on the bass / hoover / an1x tabs (drum tabs
  store slice indices in `TB303Step.note`, so a pitch shift on those
  would be meaningless).

### Pitch-preserving BPM stretch on amen (granular v1)

- `AmenState.bpm_stretch_preserve: bool` pairs with the existing
  `bpm_stretch`.  When both are on, `AmenVoice::process` runs a
  granular time-stretch: the per-sample read rate stays at native
  pitch (no BPM-ramped `extra_pitch`), and at every grain boundary
  (`AMEN_GRAIN_LEN` = 2048 samples ≈ 46 ms at 44.1 kHz) the read
  position jumps by `(host_bpm / source_bpm - 1) * GRAIN_LEN` in the
  direction of playback so the average source advance per output
  sample matches the host-to-source ratio.
- Keeps per-slice pitch overrides composable: a slice that wanted
  +12 semitones still gets them; only the BPM's pitch baggage is
  moved out of `extra_pitch` and into the grain scheduler.
- Slice boundaries are enforced — rewinds past `slice_start` wrap to
  the tail, skips past `slice_end` wrap to the head, so the stretcher
  stays within the currently playing slice instead of marching into
  the next one.
- Stretch ratio clamps to `0.25..=4.0` so extreme host/source ratios
  don't explode the grain math.
- UI: a PITCH / TUNE toggle sits next to STRETCH / FREE in the Amen
  panel's BPM row.  PITCH engages granular; TUNE keeps the classic
  resample that pitches with tempo.  The toggle stays disabled
  until STRETCH is on (preserve without stretch is a no-op).
- **v2 crossfade** eliminates the v1 splice click.  During the last
  `AMEN_GRAIN_FADE` samples (256 ≈ 5.8 ms at 44.1 kHz) of each grain,
  the output linearly blends from the current read at `self.pos` toward
  the lookahead read at `self.pos + jump` (the predicted post-splice
  read position, wrapped at slice boundaries via the new shared
  `wrap_into_slice` helper).  At the splice, `self.pos` jumps to the
  same target the crossfade was heading toward — the output curve is
  continuous through the boundary.  Splice sample-to-sample delta
  drops from ~600× the pre-splice slope to under 10× — below audible
  click threshold for all reasonable stretch ratios.
- 4 new DSP tests: trigger captures both flags correctly, preserve
  mode zeroes out `extra_pitch`, classic mode still applies the
  log2-based pitch shift, grain boundary actually rewinds the read
  position relative to classic mode.

### Reverse-mode compressor

- `FxState.compressor_reverse: bool` — swaps the envelope follower's
  attack and release time constants inside `Compressor::compress_band`.
  Normal shape (1 ms attack + 80 ms release) clamps transients fast
  and releases slowly.  Reverse shape (80 ms / 1 ms) lets the initial
  transient punch through while the envelope slowly catches up and
  clamps the sustain — classic reverse-compression swell-into-hit
  without any look-ahead.
- Third FX with a reversal mode alongside `reverb_dir` and `delay_dir`.
  UI: `REVERSE` toggle under the RATIO / MULTI row in the COMP glass
  pane on the FX panel.  LLM / API accept `{"fx":
  {"compressor_reverse": true}}`; honours the
  `fx.compressor_reverse` lock path.
- 4 new DSP tests pin the asymmetric envelope behaviour: slow rise,
  fast fall, initial transient preserved, sustain still clamped.

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
- **Song mode style transitions** - `SequencerState.pattern_style: Option<String>` per bank slot; when the chain auto-advances into a slot whose style is `Some(id)`, `apply_pattern_style_on_advance` sets the global `llm.active_style` + propagates to unlocked agents so the chain can drive genre shifts mid-song.  Picker lives at the end of the pattern-bank row (sequencer_chain.rs); persists with the project JSON like any other sequencer field.
- **Song mode tempo transitions** - per-slot `pattern_bpm_apply: bool` opt-in (default `false`); when lit, `chain_advance_transport` drops the prior bpm/swing and adopts the loaded slot's own values on chain advance.  Default-off means existing chain projects upgrade without surprise tempo jumps; flipped via a `BPM⇥` chip next to the style picker.  `running` is always preserved regardless of the flag so the chain never pauses mid-song.
- Live record - MIDI keyboard writes directly into steps
- Time signature selector (4/4, 3/4, 5/4, 6/8, 7/8...)
- Mute/solo per row, pattern copy/paste
- **MIDI export** — `src/midi/export.rs` serialises the active pattern to a Standard MIDI File (Type 1, PPQ 480).  Track 0 carries the tempo + time-signature meta; drums merge onto channel 10 via a GM kit map (`drum_voice_to_gm_note`); each melodic voice (bass / hoover / an1x) lands on its own channel with accent → velocity (64 baseline + up to +63) and `TB303Step.gate` → note length.  Patterns without any active steps don't emit a track so the SMF stays clean in a DAW.  Triggered via the `MIDI ⇩` button at the end of the pattern-bank row (writes `pattern-<unix_secs>.mid` to cwd) or via `POST /api/midi/export { path? }` for scripted exports.

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
- **Expandable FX XY pad** - `RackModule.pad_expanded` (persisted, defaults to `false`) + `RackModule.pad_pair` (u8, 0/1/2) on every FX kind; `ModuleKind::supports_xy_pad()` gates both; chevron (▾/▸) in the title bar expands per-instance and calls `arrange_grid()` so neighbours reflow.  2-knob FX (Autotune, Drive, Waveshaper, RingMod) show a direct pad; 3-knob FX (Reverb, Delay, Chorus, Phaser, EQ, Compressor, TapeSat, Bitcrush, Pan) show a glass-wrapped pad + side-mounted `A × B ↻` cycle chip covering all A/B · A/C · B/C pairs (pad's right-click cycle still works).  `render_two_pad` / `render_three_pad` factor the layout; `POST /api/rack/pad { id, expanded?, pair? }` + `"rack": {"pad": [{"kind": "reverb", "expanded": true, "pair": 1}]}` in LLM JSON make pads addressable from scripts and agents.

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
- **Dub delay send/return** — `delay_freeze` mirrors `reverb_freeze` (input suppressed, feedback pinned to ~1.0 for infinite hold); `delay_hpf` + `delay_lpf` are one-pole filters on the feedback path so each repeat loses highs / lows on every round-trip. Classic dub "drift into smoke" chain: seed a voice, engage freeze, tweak filters to shape the tail. `styles.json`'s `dub_techno` baseline seeds these fields and adds TapeSat to the default rack. UI: HPF/LPF knobs + `FRZ` toggle on the Delay card, alongside the direction / rev-quant buttons. LLM-addressable as `fx.delay_hpf`, `fx.delay_lpf`, `fx.delay_freeze`.
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

- **987 unit tests** across ~30 test files (up from 479 milestone)
- **2026-04 refactor round** — 13 proactive file splits when the largest sources approached the 1000-line pre-commit cap.  Tests: `rack_tests` → +`rack_reach_tests`, `llm_apply_extra_tests` → +`llm_apply_seq_tests`, `dsp_tests` → +`dsp_voice_primitives_tests`, `llm_tests` → +`llm_plumbing_tests`.  Library: `llm/mod.rs` → +`types.rs`, `api/mod.rs` → +`preset.rs`, `audio/dsp/params.rs` → +`params_from.rs`, `audio/dsp/voices.rs` → +`an1x.rs`, `audio/dsp/mod.rs` → +`fx_step.rs`, `llm/planner.rs` → +`planner_heuristic.rs`, `ui/mod.rs` → +`app_update.rs`, `ui/widgets/mod.rs` → +`knob.rs`.  Top-file count dropped 982 → 973; only one file still above 950 (`audio/dsp/samplers.rs`, one self-contained `AmenVoice`).  Added `planner_tests.rs` (18 tests covering `lane_from_label` / `lane_is_live_pub` / `heuristic_plan` — previously 0 coverage on a 964-line file).
- **`rack.connect_control(from_id, to_id)`** - replaces 8-line PortRef boilerplate at 6 call sites
- **`spawn_agent()` pure function** - transitions.rs; wizard.rs and SpawnAgent handler refactored to use it
- **`format_llm_display()` pure function** - extracted from drain_llm_outputs into transitions.rs
- **`BassVoiceParams` struct** - per-voice AudioParams snapshot
- **Bass303 extracted** to `src/audio/dsp/bass303.rs` (line-limit split)
- **DSP utilities extracted** to `src/audio/dsp/dsp_util.rs` (midi_to_hz, tanh)
- **Samplers extracted** to `src/audio/dsp/samplers.rs` (AmenVoice, GranularVoice)
- **Dead code removed** - `sync_default_agent`
- **Windows code-signing** - signtool step in `build-all.bat` (set SIGN_CERT + SIGN_PASS)
