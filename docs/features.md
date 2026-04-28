# Impulse Instruct - Implemented Features

A detailed log of what's built.

---

### SF2 modulation surface complete — mod-LFO filter/volume targets + modulation envelope

Closes the rest of the SF2 generator-driven modulation wishlist.
Pitch targets shipped earlier; this batch adds the remaining
mod-LFO destinations and the five-stage modulation envelope so SF2
patches that script filter wobble, tremolo, plucked-string filter
sweeps, etc. all play through correctly per spec.

**modLfoToFilterFc + modLfoToVolume.**  The mod-LFO now drives all
three of its targets — pitch (already shipped), filter cutoff, and
volume.  Filter modulation rides on top of the existing per-region
filter; cutoff knob is offset linearly via the closed-form
`cents · ln(2)/(1200·ln(900)) ≈ 8.491e-5` constant (no `powf` per
sample, so the cost stays flat).  Volume modulation is a symmetric
tremolo via `10^(lfo · cb / 200)` — positive depth boosts at the
LFO peak (FluidSynth polarity convention).

**Modulation envelope (Delay → Attack → Hold → Decay → Sustain →
Release).**  Five-stage AHDSR shared between two depth targets:
- modEnvToPitch (gen 7) — cents at full env value, exact
  `2^(cents/1200)` rate factor (the env can swing ±10 octaves so
  the small-angle Taylor approximation used by the LFO is too loose).
- modEnvToFilterFc (gen 11) — cents of cutoff swing, same
  closed-form knob delta as the LFO filter target.

Mod-env stage curves are exponential (matching the existing voice
ADSR), with sustain expressed as the linear 0..1 level the env
decays to (converted from SF2's 0.1 % attenuation units at parse
time).  Five timecents fields (delay/attack/hold/decay/release) +
sustain depth + two depth targets = ten new SF2 generators wired.

**Sibling-style file split.**  Adding the env to
`sample_instrument.rs` pushed it past the 1000-line cap, so the
entire SF2 modulation surface — `RegionLfos`, `LfoSlotState`,
`RegionModEnv`, `ModEnvState`, plus the four pure helpers
(`cents_to_taylor_rate_factor`, `cents_to_exact_rate_factor`,
`filter_cents_to_knob_delta`, `cb_to_linear_gain`) and two
builders (`region_lfos_from`, `region_mod_env_from`) — now lives in
a sibling `sample_instrument_modulation.rs`.  Voice slot holds
`lfo_state: LfoSlotState` + `mod_env_state: ModEnvState`; per-
sample LFO advance becomes a single `step()` call.

**Tests.**  +24 across the surface: 8 new SF2 absorb tests for the
new generators (gen 13 / 14 / 7 / 11 / 25–30); 5 unit tests on
`ModEnvState` covering each stage transition; 4 builder tests on
the activation gates + clamps; 5 voice-level integration tests
(modLfoToVolume RMS divergence, modLfoToFilterFc sweep, modEnvToPitch
shift, modEnvToFilterFc sweep, no-mod-env regression bit-equal).

**Module file count.**  +1 new sibling source file
(`sample_instrument_modulation.rs`); +1 new tests file
(`sample_instrument_modulation_tests.rs`).  Voice file:
908 → 938 lines after the lift.

This entry closes the SF2 wishlist line in PLAN.md.  All four
generator-driven modulation targets (pitch / filter / volume / env)
are now wired; only the SF2 spec's 8.2 "default modulators"
(MIDI-CC-driven controls like CC1 → vib depth, CC11 → expression)
remain, and they're an orthogonal subsystem rather than a follow-up.

Files: `src/audio/sf2_generators.rs`, `src/audio/sf2_loader.rs`,
`src/state/sfz.rs`, `src/audio/dsp/sample_instrument.rs`,
`src/audio/dsp/sample_instrument_modulation.rs` (new),
`src/tests/sample_instrument_sfz_tests.rs`,
`src/tests/sample_instrument_modulation_tests.rs` (new).

---

### SF2 LFO pitch modulation — modLfoToPitch + vibLfoToPitch

The SF2 loader and SampleInstrument audio thread now honour the
per-region pitch-LFO generators (modLfoToPitch=5,
vibLfoToPitch=6) along with their timing (delay + frequency).
SF2 patches that ship sustained / vocal / string samples with
natural breath / vibrato now wobble per spec instead of holding
dead-flat pitch.

V1 scope is **pitch targets only** — modLfoToFilterFc /
modLfoToVolume / modEnvToPitch / modEnvToFilterFc are deferred
to a follow-up (PLAN entry).

**SF2 loader.**  Six new generator opcodes parsed:
`delayModLFO=21` (timecents), `freqModLFO=22` (absolute cents,
`hz = 8.176 × 2^(cents/1200)`), `modLfoToPitch=5` (cents),
plus the matching vibrato-LFO trio (23, 24, 6).  All round-trip
via `Generators.absorb()`; `build_region` emits the converted
Hz / seconds / cents into the new SfzRegion fields.

**SfzRegion fields.**  `mod_lfo_freq_hz`, `mod_lfo_delay_s`,
`mod_lfo_to_pitch_cents`, `vib_lfo_freq_hz`, `vib_lfo_delay_s`,
`vib_lfo_to_pitch_cents`.  Defaults: 8.176 Hz (SF2 spec
default), 0 s delay, 0 cents depth — so regions without these
generators are bit-identical to pre-LFO behaviour.

**Audio thread.**  New `RegionLfos` Copy struct + per-slot
state in `SampleInstrumentSlot`:
- `mod_lfo_phase`, `mod_lfo_delay_remain_s`
- `vib_lfo_phase`, `vib_lfo_delay_remain_s`

`fire_slot` resets phases + seeds delays from the region.  In
`process_slot`, after the existing rate calc + Mellotron flutter,
each LFO advances its phase, decrements its delay, and emits a
sin-wave cents offset (only after delay elapsed).  Both depths
sum into a single rate factor via the small-angle approximation
`1 + cents · ln(2)/1200` (same trick the Mellotron flutter
path uses; accurate to <0.01 % within the ±1200 cents clamp
applied at trigger time).

**Tests.**  +4: SF2 loader's `Generators::absorb` round-trips
all 6 LFO + sample-modes opcodes (2 tests); audio-thread
behaviour — modLfoToPitch perturbs the rendered output vs a
zero-depth baseline; vibLfoToPitch's delay generator suppresses
modulation early in the trace.  Suite **2301 → 2305**.

This entry partially closes the SF2 wishlist line in PLAN.md
(pitch targets shipped; filter / volume / mod envelope deferred).

Files: `src/audio/sf2_loader.rs`, `src/state/sfz.rs`,
`src/audio/dsp/sample_instrument.rs`,
`src/tests/sample_instrument_sfz_tests.rs`.

---

### SF2 sample loop modes — per-region loop honour

The SF2 loader and SampleInstrument audio thread now honour per-
region loop info from `sampleModes` (generator 54).  Earlier
ships passed loop_start / loop_end through into `SfzRegion` but
the audio path always looped (or always didn't, depending on the
global `sample_loop_enabled` knob); now SF2 patches that ship
sustained / one-shot / release-tail samples behave per spec.

**SF2 loader.**  New `GEN_SAMPLE_MODES = 54` constant + parser:
- 0 / 2 / absent → `SfzLoopMode::NoLoop`.
- 1 → `SfzLoopMode::LoopContinuous`.
- 3 → `SfzLoopMode::LoopSustain` (loop while gate held, then
  play release tail).

`build_region` now sets `region.loop_mode` instead of leaving it
at the SfzRegion default.

**Audio thread.**  `TriggerShape` and `SampleInstrumentSlot`
gain a `region_loop: Option<(SfzLoopMode, usize, usize)>` field
mirroring the existing `region_adsr` / `region_filter` override
pattern:
- `Some` only when the region declares LoopContinuous /
  LoopSustain AND has a usable loop window.
- Single-WAV mode passes `None` so the global `sample_loop_*`
  knobs stay live (preserving V1 behaviour).

`process_slot`'s loop branch consumes `slot.region_loop` first;
LoopContinuous loops while alive, LoopSustain loops while
`slot.gate` is true and falls through to play-to-end after
gate-off.  Falls back to global knobs for the no-override path.

**Tests.**  +3 in `tests/sample_instrument_sfz_tests.rs`:
LoopContinuous override survives `sample_loop_enabled = false`;
LoopSustain releases past `le` after `gate_off()` then drains;
NoLoop region must not loop even when global loop is enabled.
Suite **2298 → 2301**.

This entry closes out the second SF2 wishlist line in PLAN.md
(the first remaining is mod env / mod LFO / vib LFO generators).

---

### Crossfader CV utility (`Crossfader`)

Single-knob A/B blend between two CV sources.  More direct
than Math's Blend mode for the common A/B case — one MIX knob,
no op selector.

**State.**  `CrossfaderSlot { enabled, mix }` with
`CROSSFADER_SLOTS = 4`.  Default mix = 0.5 (centred).

**Audio thread.**  `MOD_BUF_CROSSFADER_BASE = 40`.  Two CV
inputs (A, B) per slot via `cable.to.index` (0 = A, 1 = B),
same dispatch as Math / LogicGate.  `eval_crossfader` in
`process_block_util.rs`: `out = a * (1 - mix) + b * mix`.
Disabled = passthrough of A.

**UI.**  ON/OFF toggle + MIX knob.  Caption: "CV A · CV B →
lerp(A, B, MIX) → CV out".

**Wiring.**  Standard utility ritual.  ModuleKind `Crossfader`
("XFADE" label, 2×1 grid, FxMod zone, sort-group 35,
Selector × 2 mod inputs, PortKind::Cv output).  Aliases
crossfader / xfade / xfader / ablend / abblend / ab.  Allows
multiple.

This entry closes out the modulation-utilities wishlist
section in PLAN.md (TriggerDiv → LogicGate → FunctionGen →
Crossfader landed in successive ships of this session).

**Tests.**  +6 (2 state-side: defaults, slot round-trip; +4
module: label, alias parsing, FxMod zone, allows_multiple).
Suite **2292 → 2298**.

Files: new `src/state/crossfader.rs`, new
`src/ui/panels/crossfader.rs`, new
`src/tests/crossfader_tests.rs`.

---

### FunctionGen CV utility (`FunctionGen`)

Re-triggerable AR envelope with curve shaping — Maths-style:
gate-in fires the envelope on each rising edge, output is a
0..1 shaped envelope.  Distinct from `LfoModule` (free-running)
and `CvSequencer` (step-table); fills the "transient envelope"
gap for plucks / drum sounds / synth attack tails when a gate
signal needs an audio-rate ADSR-shaped CV without burning a
full bass voice.

**State.**  `FunctionGenSlot { enabled, attack, release,
curve }` with `FUNCTION_GEN_SLOTS = 4`.  Knobs all 0..1:
attack 0..1 s, release 0..3 s, curve centred at 0.5 (linear);
<0.5 = log/concave, >0.5 = exp/convex.

**Audio thread.**  `MOD_BUF_FUNCTION_GEN_BASE = 36`.  Per-slot
state machine in DspState:
- `function_gen_phase: [f32; 4]` — current segment phase 0..1.
- `function_gen_state: [u8; 4]` — 0=idle, 1=attack, 2=release.
- `function_gen_prev: [f32; 4]` — previous-input cache for
  rising-edge detection at the 0.5 threshold.

`process_block` evaluates after LogicGate: rising-edge → reset
phase + enter attack; phase advances by `block_dt /
segment_dur`; on completion attack→release→idle.  Curve knob
maps to a power-of-x exponent (k=1 linear; <0.5 → k>1; >0.5 →
k<1) applied to the phase before output, giving symmetric log /
exp shaping on both segments.

**UI.**  ON/OFF toggle + ATK / REL / CURVE knob row.  No XY
pad.  Caption: "Gate in → AR envelope out".

**Wiring.**  Standard 17-file utility ritual.  ModuleKind
`FunctionGen` ("FUNC GEN" label, **2×2 grid** to fit 3 knobs +
header row, FxMod zone, sort-group 35, Selector × 1 mod input,
PortKind::Cv output).  Aliases functiongen / function_gen /
funcgen / ar / ad / envelope / maths.  Allows multiple — chain
separate envelopes for amp + filter, etc.

**Tests.**  +6 (2 state-side: defaults, slot round-trip; +4
module: label, alias parsing, FxMod zone, allows_multiple).
Suite **2286 → 2292**.

Files: new `src/state/function_gen.rs`, new
`src/ui/panels/function_gen.rs`, new
`src/tests/function_gen_tests.rs`.

---

### LogicGate CV utility (`LogicGate`)

Boolean combinator for gate-domain CV — AND / OR / XOR over two
gate inputs.  Distinct from Math (continuous CV arithmetic): the
output is always 0.0 or 1.0, suitable for driving downstream
gate-consuming utilities (TriggerDiv, Comparator, Sample-and-
hold).  Pairs with TriggerDiv for layered rhythmic logic — patch
two divider outputs through AND for "fires only when both happen
on the same step", or XOR for "fires on the steps where exactly
one fires".

**State.**  New `state::logic_gate` module: `LogicGateSlot {
enabled, op: LogicOp }` with `LOGIC_GATE_SLOTS = 4` and a
`LogicOp { And, Or, Xor }` enum (with `next()` cycle helper +
`name()` for UI labels).

**Audio thread.**  `MOD_BUF_SIZE` bumped from **32 → 64** to
make room for the remaining utility ships (LogicGate +
FunctionGen + Crossfader + future).  New
`MOD_BUF_LOGIC_GATE_BASE = 32`.  Two CV inputs per slot
resolved by `cable.to.index` (0 = A, 1 = B), same dispatch as
Math.  `process_block` evaluates after TriggerDiv: read both
inputs, apply boolean op (`>= 0.5` threshold for "high"),
output 1.0 / 0.0.  Disabled = passthrough of input A.

**UI.**  ON/OFF toggle + AND/OR/XOR cycle button.  No XY pad.
Caption: "Gate A · Gate B → bool → Gate out".

**Wiring.**  Standard 17-file utility ritual.  ModuleKind
`LogicGate` ("LOGIC" label, 2×1 grid, FxMod zone, sort-group
35, Selector × 2 mod inputs, PortKind::Cv output).  Aliases
logicgate / logic_gate / logic / boolean / andorxor.  Allows
multiple — chain AND with XOR for composite logic.

**Tests.**  +8 (4 state-side: defaults, slot round-trip, op
next() cycles three states, op names; +4 module: label, alias
parsing, FxMod zone, allows_multiple).  Suite **2278 → 2286**.

Files: new `src/state/logic_gate.rs`, new
`src/ui/panels/logic_gate.rs`, new
`src/tests/logic_gate_tests.rs`.

---

### TriggerDiv CV utility (`TriggerDiv`)

Clock divider modulation utility — fires the output gate every
Nth rising edge of the input.  Distinct from `Comparator`
(threshold gate, no division) and from the sequencer's per-step
clock: this divides any incoming gate stream (LFO with Square
waveform, CvSequencer, comparator output, even another TriggerDiv)
by a configurable ratio.  Enables polyrhythmic patches
(3-against-4, 5-against-4, 7-against-4) without changing the
sequencer's pattern length.

**State.**  New `state::trigger_div` module: `TriggerDivSlot {
enabled, ratio: u8 }` with `TRIGGER_DIV_SLOTS = 4` slots and a
fixed `TRIGGER_DIV_RATIOS = [2, 3, 4, 5, 7]` table.  A
`nearest_trigger_div_ratio` helper snaps arbitrary inputs to the
nearest valid ratio so the audio thread's `% ratio` arithmetic
stays exhaustive.

**Audio thread.**  New `MOD_BUF_TRIGGER_DIV_BASE = 28` slot in
the cv_buf (last available range under the current
`MOD_BUF_SIZE = 32`; the next utility ship will need to bump it).
Per-slot edge-detection state in DspState:
- `trigger_div_count: [u32; 4]` — running count per slot.
- `trigger_div_prev: [f32; 4]` — previous-input cache for
  rising-edge detection at the 0.5 threshold.

`process_block` evaluates after the Sample-and-Hold stage:
detect rising edge → increment count → output 1.0 when input is
high AND count is a multiple of ratio (gate-shaped output that
matches the input gate width on kept fires, fully off on
skipped). Disabled = passthrough.

**UI thread.**  New `panels::trigger_div` module with ON/OFF
toggle + ratio cycle button (`÷N` label cycles 2→3→4→5→7→2 on
click).  No XY pad.  Sub-line caption: "Gate in → 1 every Nth
pulse → Gate out".

**Wiring.**  Standard 17-file utility ritual.  ModuleKind
`TriggerDiv` ("TRIG DIV" label, 2×1 grid, FxMod zone, sort-group
35 next to the other CV utilities, Selector × 1 mod input,
PortKind::Cv output).  Aliases triggerdiv / trigger_div / trigdiv
/ clockdivider / clockdiv / divider.  Allows multiple — multiple
divider instances on different ratios is the whole point.

Palette catch-up: the menus in `rack_canvas_menus.rs` had drifted
behind the recent FX/viz ships (FxPlate, FxTranceGate,
FxWaveFolder, VoiceMeterStrip, GrHistory were all missing); this
ship adds those entries alongside TriggerDiv so the user-facing
"Add module" picker stays current.

**Tests.**  +9 (5 state-side: defaults, slot round-trip, ratio
snap helper round-trips for table members, snap helper picks
closest for off-table inputs, snap helper outputs are always
table members; +4 module: label, alias parsing, FxMod zone,
allows_multiple).  Suite **2269 → 2278**.

Files: new `src/state/trigger_div.rs`, new
`src/ui/panels/trigger_div.rs`, new
`src/tests/trigger_div_tests.rs`.

---

### Gain-reduction history viz (`GrHistory`)

Rolling waveform of the momentary gain reduction across the
dynamics FX (`FxCompressor` / `FxLimiter` / `FxMultibandComp`).
Distinct from `LoudnessMeter` (output level only) — this shows
how hard the chain is being compressed.

**Audio thread.**  New `audio::gr_levels` module with
`Arc<GrLevels>` carrying a single `AtomicU32` storing the
linear gain ratio (1.0 = no reduction; 0.5 ≈ −6 dB).  Inside
`apply_fx_chain`, after each FX step that's a dynamics
processor (Compressor / Limiter / MultibandComp), the
`pre.abs() / post.abs()` ratio is captured and snap-down
tracked into `gr_env` (most-attenuating wins).  Once per
audio callback the envelope is decayed toward unity with a
~200 ms release coefficient computed from block size, then
published to the shared atomic.

The post-FX-step ratio is the truthful "what actually
happened to this signal" measure — picks up wet/dry mix,
sidechain detector switching, etc., without modifying the
individual dynamics processors.

**UI thread.**  New `panels::gr_history` module reads the
atomic at each repaint, appends to a 480-sample
`VecDeque<f32>` ring (~8 sec at 60 fps), and paints a
downward-falling trace with reference rails at -3 / -6 / -12 /
-18 dB.  Trace brightness scales with attenuation (eye
attracted to active GR).  Floor at -24 dB on the y-axis.
Header readout shows the current GR in dB.

**Wiring.**  `Arc<GrLevels>` constructed in `AudioEngine::new`
alongside `voice_meters`, threaded through DspState's
constructor and the UI's `AudioChannels` → `ImpulseApp.gr_levels`.
Standard ModuleKind ritual: `GrHistory` ("GR HISTORY" label,
4×2 grid matching the spectrum / scope viz envelope, sort-group
33 next to LoudnessMeter so the dynamics-domain meters
cluster).  Aliases grhistory / gr_history / gainreduction /
gain_reduction / gr / grscope.  Singleton.

**Tests.**  +5 in `audio::gr_levels` (atomic round-trip,
default unity, dB conversion at unity / half / floor cases),
+5 state-side (label, alias parsing, FxMod zone, no audio
output, singleton).  Suite **2259 → 2269**.

Files: new `src/audio/gr_levels.rs`, new
`src/ui/panels/gr_history.rs`, new
`src/tests/gr_history_tests.rs`.

---

### Voice meter strip viz (`VoiceMeterStrip`)

Per-voice level meters for the rack viz pane — one mini-meter
per active voice.  Distinct from `StereoMeter` and
`LoudnessMeter` (which show the master sum only); this strip
exposes "which voice is contributing what" at a glance.

**Audio thread.**  New `audio::voice_meters` module:
`Arc<VoiceLevels>` carrying 20 `AtomicU32` slots indexed by
`voice_meter_idx(kind)`.  `DspState` now owns a per-voice
peak-decay envelope (`voice_envs: [f32; 20]`) updated every
sample inside `process_block` from each voice bus signal
(bus_bass / bus_808 / bus_909 / bus_hoover / bus_pluck /
bus_wavetable / bus_sample / bus_an1x / bus_amen / bus_noise /
theremin_out / pendulum_out / fm_ops_out / additive_out /
modal_out / chiptune_out / vocal_out / bus_granular / gk /
tts_raw).  Decay constant = 0.99988 (~83 ms half-life @ 48 kHz)
gives a Hold-Until-Reset feel that reads as responsive without
chasing per-sample noise.  Once per audio callback the
envelopes are published into the shared atomic array as
`f32::to_bits` (Relaxed; lock-free; cheaper than an rtrb
stream the UI would have to drain).

**UI thread.**  New `panels::voice_meter_strip` module reads
the atomic slots, filters to voice kinds currently in the
rack, and paints a vertical level bar + tiny label per
voice.  Hybrid linear-then-log mapping keeps overshoots
visible without saturating the bar; unity tick at 0 dBFS-ish
for visual reference.  Empty rack → "no voices in rack"
placeholder text.

**Wiring.**  `Arc<VoiceLevels>` constructed in
`AudioEngine::new`, cloned to both the audio-thread DSP path
and the UI thread (`AudioChannels.voice_meters` →
`ImpulseApp.voice_meters`).  Mirrors the existing
`sample_instrument_poly` pattern (atomic shared between
audio + UI for "latest value only" data).  Standard ModuleKind
ritual: `VoiceMeterStrip` ("VOICE LEVELS" label, **6×2 grid**
to host ~10 meter cells in a row + label row underneath,
sort-group 33 next to `StereoMeter`).  Aliases voicemeter /
voicemeterstrip / voicelevels / levels / etc.  Singleton
(multiple strips would be redundant — they'd all read the same
atomic array).

**Tests.**  +5 in `audio::voice_meters` (atomic round-trip
preserves f32 bits, out-of-range read returns 0, out-of-range
write silently dropped, `voice_meter_idx` round-trips for
every voice kind into a unique slot with non-empty label,
`voice_meter_idx` returns None for non-voice kinds), +5
state-side (label, alias parsing, lives in FxMod zone, no
audio output, singleton).  Suite **2249 → 2259**.

Files: new `src/audio/voice_meters.rs`, new
`src/ui/panels/voice_meter_strip.rs`, new
`src/tests/voice_meter_strip_tests.rs`.

---

### Wavefolder FX (`FxWaveFolder`)

West Coast (Buchla / Serge / Make Noise) fold distortion.
Distinct from the clip / drive / saturation / waveshaper bank
already in place: those compress signal into a soft / hard
ceiling, the fold instead reflects it when it crosses
±threshold, multiplying harmonics into the bright complex
timbres these West Coast modules are known for.

**DSP shape.**  Closed-form fold curves — no iteration,
allocation-free, cheap-bypass when `wf_mix < 0.001`.  Two
fold curves are computed every sample and blended via the
SYMMETRY knob:
- **Triangle fold** (symmetry = 1) — sharp Serge-style
  reflections around ±FOLD_THRESHOLD.  Closed form via
  `(x + t) / 2t` normalisation + parity-based ramp; produces
  pure triangle waves at high drive.
- **Sine fold** (symmetry = 0) — `sin(x · π / 2t) · t` —
  Buchla-style smoother fold curve, more even-order energy.

DRIVE pre-multiplies the input (1..10×); BIAS adds a DC
offset (centre 0.5 = symmetric fold); SYMMETRY blends the
two curves; MIX is the standard wet/dry.  Both fold curves
are bounded by ±FOLD_THRESHOLD (1.0), so the wet output is
loudness-controlled regardless of drive.

**Knobs.**  4 knobs, all 0..1:
- `wf_drive` 0..1 → 1..10× pre-fold gain.  Default 0.4
  (~4×) so the fold engages immediately on a typical ±0.5
  input.
- `wf_bias` 0..1 (knob centre 0.5 = no offset).  Off-centre
  values shift the fold asymmetrically, producing more
  even-order harmonics.  Default 0.5.
- `wf_symmetry` 0..1 — sine ↔ triangle blend.  Default 0.5
  (50/50).
- `wf_mix` 0..1 wet/dry.  Default 0.

**Wiring.**  Standard FX add ritual — `FxStep::WaveFolder`
(idx 45, `FX_STEP_COUNT` bumped to 46), `ModuleKind::FxWaveFolder`
("WAVEFOLDER" label, 2×1 grid, sort-group 29 next to
`FxTapeSat` / `FxDrive` — the saturation / colour family).
Aliases wavefolder / wave_folder / fxwavefolder / fold /
westcoast / buchla / serge.

**Tests.**  +5 DSP (mix=0 bypass, output bounded by threshold,
high drive sweeps wide range, low-drive zero-bias passthrough,
bias offset changes output), +6 state-side (defaults engaged
with mix=0, llm-apply all 4 knobs, lock honoured, FxStep
mapping, label, alias).  Suite **2238 → 2249**.

Files: new `src/audio/dsp/fx_wavefolder.rs`, new
`src/tests/fx_wavefolder_tests.rs`.

---

### Trance gate FX (`FxTranceGate`)

Pattern-driven 16-cell gate synced to the sequencer clock.
Distinct from `FxGate` (envelope-driven sidechain ducker /
noise gate): cell traversal is rhythmic rather than amplitude-
triggered, with a per-cell-edge ramp to suppress clicks.
Classic trance / EDM "chopped pad" effect.

**DSP shape.**  Pattern stored as a `u16` bitmask (bit 0 =
cell 0, bit 15 = cell 15).  Cell index derived from
`p.sequencer_current_step` + an internal sub-step phase
counter (samples elapsed since the last step boundary, reset
on `prev_seq_step` change).  Cell traversal rate is 1/4, 1/8,
1/16, or 1/32 of a bar (4 / 8 / 16 / 32 cells per bar);
sub-step resolution at 1/32 keeps the gate aligned to half-
sequencer-step boundaries derived from BPM.

On cell-index change the target gate amplitude latches from
the bit lookup; a one-pole smoother ramps the live gate
toward target each sample.  Cheap-bypass when `tg_mix < 0.001`.

**Knobs.**
- `tg_pattern` (u16) — 16-bit gate pattern.  Default `0xAAAA`
  (alternating odd cells active).
- `tg_rate` (u8 0..3) — cell-rate selector (1/4, 1/8, 1/16, 1/32).
  Default 1 (1/8) so the default pattern reads as a quarter-note
  pulse.
- `tg_smooth` 0..1 → 0.5..50 ms one-pole smoother time constant.
  Default 0.2 (~10 ms — clean rhythmic chop without click).
- `tg_mix` 0..1 wet/dry.  Default 0 so insert is no-op until
  dialed in.

**Wiring.**  Standard FX add ritual — `FxStep::TranceGate`
(idx 44, `FX_STEP_COUNT` bumped to 45), `ModuleKind::FxTranceGate`
("TRANCE GATE" label, **4×2 grid** so the 16 step toggles fit
in the top row + control row underneath, sort-group 23 next to
`FxStutter` / `FxTapeStop` — the rhythmic / pattern-chop
family).  Aliases trancegate / trance_gate / fxtrancegate /
trance / patterngate / stepgate.

UI is custom (not the templated 4-knob shape): top row paints
16 small `1`–`16` step toggles (click toggles the bit), bottom
row has a RATE cycle button + SMOOTH knob + MIX knob.  No XY
pad — the pattern is the primary surface.

Modulation: 2 selector jacks (smooth + mix only — pattern is a
bitmask and rate is a discrete selector, neither suited to LFO
modulation).

**Tests.**  +5 DSP (mix=0 bypass, all-on pattern transparent at
full mix, all-off silences at full mix, alternating pattern
visits both states across step changes, output bounded under
chopping), +7 state-side (defaults are alternating @ 1/8, llm-
apply all 4 fields, rate above max clamps, lock honoured,
FxStep mapping, label, alias).  Suite **2226 → 2238**.

Files: new `src/audio/dsp/fx_trance_gate.rs`, new
`src/tests/fx_trance_gate_tests.rs`.

---

### Plate reverb FX (`FxPlate`)

Dattorro-style plate reverb — figure-of-eight tank of modulated
allpasses + delays + LP damping.  Distinct from the existing
`FxReverb` (Schroeder parallel-comb + series-allpass; brighter,
less dense character) and `FxConvReverb` (IR-driven, file-loaded).
Captures the dense, slightly metallic Lexicon / EMT plate
character that the prior two reverbs can't reach.

**DSP shape.**  Input bandwidth-limit → 4-stage all-pass
diffusion network (lengths 142 / 107 / 379 / 277 samples scaled
to engine sr) → figure-of-eight tank: each half has a modulated
all-pass (LFO depth ±8 samples at ~1 Hz) + delay line + one-pole
LP damping inside the loop + decay all-pass + delay line.
Cross-feed between halves makes the loop topology a figure-eight
(implicit one-sample delay from `feedback_l/r` registers keeps it
stable).  Output mixer reads 7 fixed taps per half (Dattorro's
standard pattern) and folds the stereo image down to mono for
the chain step.

Allocation-free hot path; all delay buffers heap-allocated once
in `new()` and indexed via mask-wrap (power-of-two sizes:
PLATE_DELAY_LEN = 8192, PLATE_AP_LEN = 4096).

**Knobs.**  4 knobs, all 0..1 unipolar:
- `plate_size` 0..1 — tank time / decay scale, mapped to the
  cross-feed gain (0 → 0.4×base, 1 → 1.0×base of the conservative
  PLATE_TANK_GAIN = 0.5 cap).  Larger = longer tail without
  reallocating delay lines.
- `plate_damping` 0..1 — one-pole LP coefficient inside each
  tank half.  0 = bright / metallic, 1 ≈ very dark.
- `plate_diffusion` 0..1 → input pre-AP gain 0..0.75 (with the
  last two pre-APs at 0.625× per Dattorro's paper).  Higher =
  denser early reflections.
- `plate_mix` 0..1 wet/dry with cheap-bypass fast path
  (mix < 0.001).  Default 0 so a freshly inserted FX is no-op
  until dialed in.

Defaults pick a medium plate (size 0.55, damping 0.4, diffusion
0.7) so flipping mix > 0 immediately produces audible output
without further knob-twiddling.

**Wiring.**  Standard FX add ritual — `FxStep::Plate` (idx 43,
`FX_STEP_COUNT` bumped to 44), `ModuleKind::FxPlate` ("PLATE"
label, 2×1 grid, sort-group 37 next to `FxConvReverb` so the
two non-Schroeder reverbs cluster).  Aliases plate / fxplate /
platereverb / plate_reverb / emt / lexicon.  Renders in the
fx_lfo cluster (4-knob shape).

**Tests.**  +5 DSP (mix=0 bypass, audible tail from impulse,
output bounded under full drive 1 s, mix=0 fast-path even at
full drive, larger size = more late-tail energy), +6 state-side
(defaults are medium plate / mix=0, llm-apply all 4 knobs, lock
honoured, FxStep mapping, label, alias).  Suite **2213 → 2226**.

Files: new `src/audio/dsp/fx_plate.rs`, new
`src/tests/fx_plate_tests.rs`.

---

### SampleInstrument — SF2 filter generators + per-region filter override

Pair of changes — SF2 filter generators on the parser side,
per-region filter override on the DSP side.  Together they mean
SF2 banks (and SFZ regions with `fil_type` / `cutoff_hz` /
`resonance_db` set) shape the per-note tone independently of the
user's global filter knobs.

- New SF2 generator opcodes parsed: `initialFilterFc` (8) and
  `initialFilterQ` (9).  Cents → Hz via
  `8.176 × 2^(cents / 1200)`; centibels → dB via `cB / 10`.  SF2
  spec default 13500 cents (~20 kHz, "filter off") collapses
  back to "no filter" so SoundFonts without filter generators
  pass through unchanged.
- `SampleInstrumentSlot` and `TriggerShape` gain
  `region_filter: Option<(cutoff_knob, resonance_knob, mode)>`.
  Mirrors the per-region ADSR shape: `Some` = SFZ / SF2 region
  with `fil_type` set; `None` = single-WAV / unfiltered region.
- `process_slot` is now 3-way: region filter override (force-
  apply with `mix=1`) → global filter knobs (V1.1 path) →
  bypass.  An explicit SFZ / SF2 filter shapes the sound
  regardless of the user's global mix knob.
- New helpers: `hz_to_svf_knob`, `db_to_svf_resonance_knob`,
  `sfz_fil_type_to_svf_mode` — convert from the human / spec
  units that SfzRegion carries to the 0..1 / u8 the SVF expects.

2 new tests: a region with a 100 Hz LPF measurably attenuates an
8 kHz sine compared to the same buffer through a no-filter
region; the filter generator absorption captures both
`initialFilterFc` and `initialFilterQ` correctly.

### SampleInstrument — SF2 envelope generators (V2 follow-up)

Builds on the per-region ADSR override (immediately prior).
SF2 stores volume-envelope timing as **timecents**
(`secs = 2^(tc / 1200)`) and sustain as **centibels of
attenuation** from peak.  The loader now parses the four volume-
envelope generators and writes them into `SfzRegion.ampeg_*` so
the DSP path's per-region override applies automatically.

- New generator opcodes parsed: `attackVolEnv` (34),
  `decayVolEnv` (36), `sustainVolEnv` (37), `releaseVolEnv`
  (38).  Generator absent → SfzRegion fields stay at defaults
  (the DSP falls through to global knobs).  Spec sentinel of
  -12000 timecents (~1 ms = "instant") is also treated as
  "default" — the audible result with -12000 is indistinguishable
  from "no envelope" so collapsing them avoids accidentally
  routing real samples through a 1 ms attack.
- Sustain conversion: `pct = 10^(-cB / 200) × 100` (proper
  amplitude attenuation curve, so 200 cB = 20 dB → ~10 %
  sustain).  0 cB → 100 % full sustain.
- New `sf2_timecents_to_secs` helper handles both the absent and
  sentinel cases uniformly.
- Generators struct gains `attack_vol_env_tc`, `decay_vol_env_tc`,
  `sustain_vol_env_cb`, `release_vol_env_tc` as `Option<i16>` so
  preset-zone → instrument-zone clone+absorb cleanly inherits
  inheriting values.

3 new unit tests: timecents sentinel + None map to 0; standard
timecents (-1200, 0, 1200) round-trip to expected seconds; the
absorb path captures all four volEnv opcodes correctly.

### SampleInstrument — per-region ADSR override (DSP)

`SfzRegion.ampeg_attack_s` / `ampeg_decay_s` / `ampeg_sustain_pct`
/ `ampeg_release_s` were parsed but ignored.  Wiring them into
the slot's ADSR closes the gap for SFZ regions immediately and
gives SF2 envelope generators a place to land.

- `TriggerShape` and `SampleInstrumentSlot` gain a
  `region_adsr: Option<(attack_s, decay_s, sustain_level,
  release_s)>` field.  `Some` = SFZ / SF2 region with at least
  one non-default `ampeg_*` field; `None` = single-WAV path.
- `step_adsr` reads from the slot's override when present; falls
  back to `params.sample_attack` / `_decay` / `_sustain` /
  `_release` (the live global knobs) otherwise.  Single-WAV
  mode keeps the V1 behaviour where rotating the attack knob
  mid-note is audible immediately.
- Sustain conversion: SfzRegion stores sustain as 0..100
  percent, the DSP wants 0..1 — converted at trigger time.

2 new unit tests: a 1 ms region release overrides a long global
release (~2 s); a region with all `ampeg_*` at default leaves
the global knob in charge so the short global release fires.

### SampleInstrument — `.sf2` preset picker (V2 follow-up)

V1 SF2 loaded only the first preset.  Most SoundFont banks pack
dozens of presets (drum kit, piano, organ, strings, ...), so a
combo box on the SampleInstrument panel surfaces the rest.

- New `Sf2PresetInfo { name, bank, preset }` exposed by the
  loader; `parse_sf2_presets(bytes)` walks the `phdr` chunk and
  returns one entry per real preset (skipping the EOP sentinel).
- New `parse_sf2_preset_regions(bytes, idx)` — preset-indexed
  variant of the V1 byte parser; out-of-range index returns
  `None` so the caller distinguishes "no such preset" from
  "preset has no playable regions".
- New `load_sf2_presets(path)` and `load_sf2_preset(path, idx)`
  — disk-I/O wrappers used by the panel for the picker render
  + the on-change reload.
- ImpulseApp gains `sample_sf2_presets` (list) +
  `sample_sf2_preset_idx` (selection).  Populated on `.sf2`
  load, cleared on `.sfz` / single-WAV / REC capture so the
  picker only renders when an SF2 is actually active.
- Combo box shows `idx: NAME  (bN/pP)` per entry so the user
  sees both the human name and the MIDI bank/program for
  scripting.  Selection change re-parses the bank for the
  chosen preset and pushes the new region list to the audio
  thread.

4 new unit tests: preset list returns one entry for the
single-preset fixture; the indexed entry point at idx=0 matches
the legacy `parse_sf2_bytes` output bit-for-bit; out-of-range
idx returns `None`; `preset_name_from_bytes` trims trailing
nulls correctly.

### SampleInstrument — `.sf2` SoundFont parsing (V1)

Hand-rolled SF2 parser — no new deps, lives entirely in
`src/audio/sf2_loader.rs`.  Parses the RIFF chunk hierarchy
(sfbk → INFO / sdta / pdta), walks the
preset → instrument → sample chain, and converts each playable
region into the existing `SfzRegionRuntime` shape so the
SampleInstrument's SFZ trigger path handles SF2 content with
zero changes.

V1 scope:
- RIFF walker for the sfbk container; sub-chunk extractor
  shared across `pdta` records.
- Generator opcodes honoured: `keyRange` (43), `velRange` (44),
  `instrument` link (41), `sampleID` link (53),
  `overridingRootKey` (58), `coarseTune` (51), `fineTune` (52),
  `pan` (17), `initialAttenuation` (48).
- Generator chain layered preset-zone → instrument-zone via
  clone + absorb, matching the SF2 spec's "global zone applies
  to subsequent zones until overridden" idiom.
- Sample data decoded from the `smpl` chunk (16-bit signed PCM,
  big banks share the buffer via index slicing) and resampled
  per-sample to the engine rate.  Per-sample cache so multi-zone
  SF2s only decode each sample once.
- `.sf2` added to the SampleInstrument file picker;
  `load_sample_instrument_path` routes by extension to the
  matching loader.
- Loads only the first preset.  Most SF2 files pack many
  presets (drum kit, piano, organ, etc.) — V1 = "audition the
  first preset", future V2 adds a preset picker.

Deferred to V2:
- Envelopes, filters, modulators, LFOs, sample modes — the
  per-note shaping falls back to SfzRegion defaults; the audio
  still plays, the per-note attack / release just isn't shaped
  by SF2 generators.
- Preset selection (V1 = first preset only).
- Disk streaming for huge banks (carried forward).

4 unit tests with a synthesised in-memory SF2: round-trip
through the parser yields one region with the expected key
range / root note / sample buffer; rejects bad magic and
truncated input; generator absorption handles keyRange + tune
+ root-key sentinels.

### SampleInstrument — multi-mic / multi-position SFZ blends

V2 SampleInstrument follow-up.  V1 SFZ honoured one sample per
region; multi-mic packs (close / room / ambient) need a way to
crossfade across mic positions.  Implements the canonical SFZ
**CC#1 (mod wheel) crossfade** convention so off-the-shelf
multi-mic packs work without modification.

- New SfzRegion fields `xfin_lo_cc1`, `xfin_hi_cc1`,
  `xfout_lo_cc1`, `xfout_hi_cc1` parse the standard SFZ opcodes
  `xfin_locc1`, `xfin_hicc1`, `xfout_locc1`, `xfout_hicc1`.
- Per-region `cc1_crossfade_gain(cc)` helper combines the xfin
  ramp (0 → 1 across `xfin_lo..xfin_hi`) with the xfout ramp
  (1 → 0 across `xfout_lo..xfout_hi`); the product is the
  region's gain at this CC.  Defaults represent "no crossfade"
  → gain = 1 for any CC, so SFZs without multi-mic markup are
  bit-identical to V1.
- New `SampleInstrumentState.mic_blend` (0..1) drives a
  synthetic CC#1 value at trigger time — the user's blend knob
  picks the active mic position across the pack.
- Trigger path skips silent regions early (cf_gain < 1e-4) to
  save slot allocations when 6/8 mic positions in a stack are
  inactive at the current blend value.
- New MIC BLEND knob on the SampleInstrument panel.  LLM schema
  + `apply_llm_update` honour the new field; lockable.

5 new unit tests cover defaults staying at unity, xfin / xfout
ramps in isolation, the combined window shape, and a 3-region
multi-mic SFZ round-trip through the parser.

### Refactor: split `arrange_grid` into sibling `rack_arrange.rs`

`rack.rs` sat at 992/1000 lines — one bad ModuleKind add away
from breaking the cap.  Lifted `arrange_grid` (~320 lines, the
canonical sort + 2D bin-pack) into a new sibling
`rack_arrange.rs`, mirroring the existing `rack_wiring.rs`
impl-symmetry idiom.  `dyn_height_override` becomes
`pub(super)` so the sibling can read the StepSequencer dynamic-
height override during the bin-pack.  Behaviour unchanged; pure
refactor.  Result: rack.rs now at 672 lines (~330 headroom).

### SampleInstrument — REC button (master-output capture)

V2 SampleInstrument follow-up.  Mirrors the AmenSampler's REC→CHOP
button: freezes the shared master-output ring buffer (the same
`granular_tap` the AmenSampler and granular CAPTURE buttons read)
and hands the captured material to the SampleInstrument as the
loaded source.

- New REC button next to LOAD on the SampleInstrument panel.
- Auto-detect-root runs on the captured material — same path the
  disk-load takes — so the instrument tunes itself to the
  recording.  Confidence threshold 0.5 prevents noise from
  mis-tuning the root note.
- Synthetic `«rec»` path label so the API-poll auto-reload
  doesn't try to re-read from disk.
- Captured-buffer mode replaces SFZ regions cleanly; UI region
  cache + selection are cleared so the previous bank's
  metadata doesn't bleed through.
- In-memory only — no file written.

### SampleInstrument — `.flac` / `.aiff` format support

V2 SampleInstrument follow-up.  V1 only handled `.wav`; this
expands the accepted file types to FLAC and AIFF/AIFC.

- New `audio::audio_load::load_audio_to_engine(path)` dispatches by
  extension to the appropriate decoder, downmixes to mono,
  resamples to the engine rate.
- FLAC via `claxon` (pure Rust, no native deps) — supports any
  bit depth + channel count + sample rate the format allows.
- AIFF via a hand-rolled parser in `audio_load` — 16-bit PCM,
  mono or multichannel (downmixed), big-endian PCM (the standard
  AIFF interpretation; AIFC compression beyond `NONE`/`sowt` is
  rejected with a warning).
- Unified loader replaces `load_wav_to_44100` at the
  SampleInstrument call sites + the SFZ region loader so
  `region.sample` references inside `.sfz` packs can target FLAC
  and AIFF too.
- File-picker grows the new extensions: `.wav`, `.sfz`, `.flac`,
  `.aif`, `.aiff`, `.aifc`.

8 new tests cover the AIFF 80-bit IEEE 754 sample-rate decode
(44100 / 48000 / zero / negative reject), the dispatch routing,
and a full AIFF round-trip via a synthesised in-memory file.

### Spectral gate — true STFT version

V2 of `FxSpectralGate`.  V1 ships an 8-band parallel-BPF
approximation; V2 adds a textbook STFT path: windowed FFT →
per-bin amplitude gate → IFFT → overlap-add.

- New `spec_stft` boolean flag (default false — V1 BPF mode is
  preserved as the default).  UI toggle on the FxSpectralGate
  card flips between BPF / STFT.
- 1024-point Hann-windowed FFT, hop 256 (75 % overlap, COLA-
  compliant for Hann²).  At 48 kHz this gives ~47 Hz/bin and
  ~16 ms latency.
- Per-bin gate envelope with frame-rate attack (~5 ms) and
  user-controlled release (10–2000 ms).  Tilt knob skews the
  threshold across the bin range — same gentle ±2× shape as the
  BPF path so the knob feel is consistent across modes.
- Allocation-free in the audio callback: FFT plans, scratch,
  Hann window, input/output rings, and per-bin gate state all
  owned by the struct.

3 new STFT-mode tests cover passthrough at threshold=0, silence
on quiet-input + high-threshold, and bounded output under
full drive.

### CV-sequence visualiser (`CvSeqScope`)

V2 polish — the existing `CvSequencer` panel already shows its 16
step bars in-place, but a focused readout (visible at a glance,
with a live playhead) reads better while performing.

- New `ModuleKind::CvSeqScope` — 4×2 viz module sized to mirror
  `LfoScope`, drops in alongside the LFO scope in the menu and
  the rack-render order pool.
- Stair-step trace of `(step - 0.5) * 2 * depth` — the actual
  bipolar mod value the audio thread sends to the target, not
  the raw 0..1 step.  Playhead column highlights the live step;
  beat-major bars (every 4th) lift slightly.
- Source-slot picking via the cable graph: a CV cable from any
  `CvSequencer` to the scope selects that slot.  Unwired falls
  back to the first enabled CV-seq slot — same idiom as
  `LfoScope` so the two reads consistently.
- Pure-state `cv_seq_slot_from_cables` helper + four unit tests
  covering the no-cable / single-source / positional-rank /
  ignore-non-source contracts.

### Vinyl FX — start/stop transient

V2 follow-up to the steady-state vinyl colour V1.  Adds a
`TRANSIENT` knob (0..1) that drives a rate-modulated playback
through the existing colour stage:

- 0 = at full speed (V1 behaviour, no rate modulation —
  bit-equal to the pre-V2 path at this value).
- 1 = deck stopped (rate=0, output silenced).
- Curve is `(1 - t)^2` — same perceptual log slow-down used by
  `FxTapeStop`, so a linear knob sweep feels natural.

Differentiated from `FxTapeStop` by always layering the vinyl
colour (warmth shelves + surface noise) on top of the rate ramp;
TapeStop is transparent.  Users automate `transient` 0→1 for a
brake transient or 1→0 for a spin-up; the steady-state colour
applies once the ramp completes.

Internal: 24 k-sample delay buffer (0.5 s @ 48 kHz), fractional
read head with linear interp.  Read head re-anchors when
transient drops to 0 so the V1 path is preserved bit-equal.

### Shimmer mode on `FxConvReverb`

V2 follow-up — adds a `SHIMMER` knob (0..1) that pitch-shifts the
wet output +12 semitones (one octave up) and folds it back into
the convolution input on the next sample.  Classic ambient-shimmer
ladder: each pass through the IR feeds a higher octave of the
previous tail, building a cathedral of overtones.

- Internal `PitchShift` instance dedicated to the shimmer path,
  fixed at +12 ST (V1 of the shimmer flag is one ladder, not
  chord stacking).
- One-sample delay via `last_wet_for_shimmer` breaks the
  algebraic loop.
- Stash point is the post-tone-shaping mid (after damp / lowcut)
  so the shimmer ladder inherits the user's filter cuts —
  prevents harsh build-up in already-attenuated bands.
- Feedback path falls back to a no-op pitch-shifter call when
  shimmer=0 to keep the ring buffer current; the V1 path is
  bit-equal at depth 0.

Two new unit tests: shimmer=0 matches V1 output bit-exactly, and
shimmer=0.7 measurably boosts the +12 ST FFT bin over the
no-shimmer baseline.

### MIDI granuliser — file-to-file mode

V2 follow-up to absurd-queue feature #8.  V1 scattered triggers
inside the running sequencer pattern; this adds a
`granulise_smf_bytes` wrapper so users can pre-process MIDI clips
offline:

- New `granulise_smf_bytes(bytes, opts) -> Result<Vec<u8>, String>`
  — parses the SMF, runs the granuliser over every melodic lane
  the exporter writes (bass / hoover / an1x), re-emits SMF bytes.
- Bridges the import/export round-trip lossiness by mirroring
  `bass_patterns[1]` → `hoover_pattern` after import, so a
  2-track source survives (RH stays on bass; LH ends up on
  hoover).
- `POST /api/midi/granulise` grows an `in_path` / `out_path`
  shape — when `in_path` is provided, the handler reads SMF
  bytes from disk, granulises, writes to `out_path` (defaulting
  to `in_path` for in-place edits).  No live state is touched
  on this path.

Three round-trip tests: density=1 preserves the RH lane,
density=0 produces an empty SMF that the importer rejects with
the expected error, and garbage-input bubbles up as an Err.

### Hoover voice tuning — PWM + sub + pitch dip

Closes the long-standing "doesn't sound like a hoover" known issue
in PLAN.md.  The voice was a clean supersaw → resonant LP/BP mix —
clinically correct, but missing the Alpha Juno character.  Three
internal tweaks land the canonical Human Resource "Dominator" /
"What The" patch sound without changing the external param surface:

- **PWM pulse blended with the saw stack.**  A separate pulse
  oscillator at the fundamental, with PW sweeping ±0.35 around 0.5
  driven by the same slow LFO that already modulated pitch (one LFO
  doing both, like the original analog).  This is the missing vowel
  / formant character that defines the hoover sound.
- **Sub-octave sine.**  One octave below at ~0.18 amplitude — adds
  body without aliasing.  Sine, not square, so it stays clean
  through the resonant filter.
- **Pitch-dip envelope on attack.**  Brief downward swoop (~30 ms
  decay, –0.6 ST depth) that gives the hoover its characteristic
  "wow" transient.  Internal envelope, no new parameter — this
  shape is part of the hoover identity, not a knob the user
  needs.

All three changes are confined to `HooverVoice::process`; the
`HooverState` surface is unchanged so existing presets and the LLM
schema keep working bit-identically.

### GRAN pitch-tracking trigger mode (deferred V2 from absurd queue)

V1 of the granular voice (absurd queue) shipped with fixed-pitch
grains driven by `pitch_scatter` only.  The deferred V2
follow-up adds melodic playback so the bird-song corpus (and any
loaded WAV) can be played from the keyboard:

- New `granular.pitch_mappable` boolean.  When true, MIDI NoteOn
  routes alongside the bass to a `TriggerEvent::GranularPitch
  { note }` that sets a base-note transposition on the granular
  voice; every spawned grain inherits the played pitch.  Reference
  is C4 (MIDI 60).
- Internal `base_note_st` field on `GranularVoice` + `set_base_note`
  method — additive form means free-running texture mode (the
  default) is bit-identical to V1.
- LLM schema + `apply_llm_update` honour the new flag (lockable
  like every other state field); the granular panel grows a
  "PITCH MAP" checkbox.

Opens the door to melodic bird-call solos and turns the granular
voice into a one-shot sample-instrument when paired with a clean
loop.

### AI patch morph — UI dialog (deferred V2 from absurd queue)

V1 of AI patch morph (absurd queue #4) shipped as `POST /api/morph`
only — discoverable from a script but invisible from the menu.
The deferred V2 dialog wraps that flow in a small modal accessible
from `Edit → AI Patch Morph...`:

- Prompt text field + `BARS` (1–64) and `CALLS` (1..=bars*4)
  spinners — same envelope the API enforces.  Soft cap on calls
  re-clamps when bars shrinks.
- Live progress view: when a morph is already in flight, the
  dialog reads `state.patch_morph` and renders a step counter +
  fill bar instead of the input form, with `Stop` / `Hide` buttons.
- Both API and UI now route through the new
  `PatchMorphState::start` constructor — pure helper so the
  bar-interval math + `last_step_fired` seed convention live
  in one place rather than being duplicated.

---

### Modulation utility cluster (Comparator + S&H + Math)

Last three items from the Modulation wishlist, shipped
together because they all share the cable-graph compile
infrastructure built up in the Slew + Quantizer phases.
With these, the modulation graph supports the full Eurorack
utility set: any LFO / CV-seq output can chain through any
combination of slew → quantize → compare → S&H → math
operations before driving a synth/FX target.

**Comparator (`ModuleKind::Comparator`).**  Outputs 1.0 when
the input CV exceeds `threshold`, 0.0 otherwise.  Single
threshold knob; per-block lookup.  Useful for turning an
envelope or LFO into a gate signal that drives some other
modulation target.

**Sample-and-hold (`ModuleKind::SampleHold`).**  Latches the
incoming CV value on each new sequencer step (the "clock
edge"), holds it until the next step.  Distinct from the
LFO's S&H waveform option (which re-latches on its own LFO
phase wrap): this one re-latches on the audio sequencer's
grid so the held value is always musically aligned with the
bar.  Knobless — just an enabled toggle; the timing is the
sequencer's.

**Math (`ModuleKind::Math`).**  Combines two CV inputs with a
chosen operation: Add, Multiply, Blend (lerp), Max, Min.
Two Mod-In ports per instance (cable to index 0 = A, index
1 = B).  Op cycle button + blend knob in the panel.  First
utility module with > 1 Mod-In jack — the cable compile pass
in `mod_compile.rs` switches on `cable.to.index` to resolve
each input independently.

**Compile-pass refactor.**  The earlier per-utility
`CvSourceMaps::resolve` helper grew to handle every CV-out
emitter: LFO, CvSequencer, Slew, Quantizer, Comparator,
SampleHold, Math.  All seven kinds resolve through a single
shared lookup so the wiring scales without per-utility
duplication.

**Audio-thread state.**  DspState gains
`sample_hold_state: [f32; 4]` (latch value per slot) and
`prev_seq_step: u32` (last seen sequencer step, used to
detect step transitions).  Allocation-free.

**ModuleKind metadata.**  All three modules are 2×1 grid,
FxMod zone, sort-group 35 (modulation cluster).  Aliases
comparator / compare / threshold; samplehold / sample_hold /
snh; math / mathmodule / cvmath.

**Tests.**  +16 (state defaults + slot round-trips + label/
zone/aliases × 3 modules; LFO→utility input compile + utility→
synth route compile; Math.A vs Math.B independent input
resolution; Math op-cycle visiting all 5 variants).
2151 → 2167 lib tests passing.

This entry closes out the Modulation wishlist.  Combined with
Phase 1 (cv_buf + multi-source mod_routes) and Phase 2.1/2.2
(Slew + Quantizer), the rack now has a full Eurorack-style
CV pipeline: any modulation source can chain through any
combination of utilities before driving a synth / FX param.

Files: new `src/state/comparator.rs`,
`src/state/sample_hold.rs`, `src/state/math_module.rs`;
new panels for each in `src/ui/panels/`; consolidated tests
in `src/tests/cv_utility_tests.rs`.

---

### CV sequencer module (`CvSequencer`)

First entry from the Modulation wishlist.  16-step CV pattern
that drives a chosen `LfoTarget` parameter, advancing in
lock-step with the audio step clock.  Distinct from
`LfoModule` (continuous waveform: sine, saw, etc.) — the CV
sequencer is a hand-drawn per-step value table, useful for
stepped modulation patterns (filter sweeps that change every
step, gate-like duck patterns, pitch transposition tables).

**Engine.**  Four CV-seq slots in `AppState.cv_seq[]` (mirrors
the four LFO slots), each with `enabled`, `step_values:
[f32; 16]`, `target: LfoTarget`, `depth: f32`.  Audio thread
reads `current_step % 16` per block and applies
`(value - 0.5) * 2.0 * depth` to the target opcode via the
existing `apply_mod_target` dispatch — same path the LFO uses.
The 0.5 centre means a flat-row pattern leaves the target
untouched; bars above 0.5 push positive, below 0.5 push
negative.

**Per-instance slot mapping.**  Multiple `CvSequencer` rack
modules share the four backing slots; each instance maps to
the slot matching its rack-order position (same idiom as the
existing `LfoModule`).  Instance 5+ stacks on slot 4 — UI
edits go through, but only the most-recently-registered slot
is audible.

**UI.**  16 vertical step bars (click + drag to set value),
beat markers (every 4 steps) brighter, playhead column
highlighted with a top-and-bottom band that reads through
both empty and active bars.  Header row: ON/OFF toggle,
target cycle button (reuses the LFO panel's `TARGET_LABELS`
and `next_target` — promoted to `pub(crate)` so the new panel
doesn't duplicate the table), depth `DragValue` 0..1.

**Wiring.**  `ModuleKind::CvSequencer` ("CV SEQ" label, 5×2
grid, FxMod zone, sort-group 35 alongside `LfoModule`).
Aliases cvsequencer / cvseq / cv_seq / stepcv / cv.  No audio
bus output (CV-only); included in the cycle-detect "CV-source"
matchset alongside `LfoModule`.

**Tests.**  +6 — defaults are disabled with neutral 0.5 step
values, default round-trips, step-count matches the canonical
bar, label, zone + no-audio-output, alias parsing.  Suite
**2128 → 2134**.

Files: new `src/state/cv_seq.rs`, new `src/ui/panels/cv_seq.rs`,
new `src/tests/cv_seq_tests.rs`.

---

### Onset / beat-grid overlay viz (`OnsetGrid`)

Second entry from the Visualizations wishlist.  Glanceable
groove-drift indicator: a rolling RMS envelope of the master
audio for the last bar, overlaid with 16 vertical step-tick
marks.  Peaks aligned with ticks = on grid; peaks drifting
before / after = early / late strikes.

**DSP shape.**  Reads `granular_tap` (already maintained for
the CAPTURE path — 3 s mono ring buffer of the master output)
under the existing UI lock-free constraint, so no new audio-
thread plumbing.  Walks the most recent `bar_samples` of the
ring buffer (derived from live BPM × 4 beats), bucketed into
4 ms RMS windows.  Cheap energy-rise peak picker marks
buckets above 40 % of the window peak as onsets.  Falls back
gracefully when the ring is empty (renders "no audio yet").

**Layout.**  One row of 16 step ticks (every 1/16 of the bar);
4 of them (the beats) drawn brighter so the user reads bar
structure at a glance.  Envelope rendered as filled grey bars
below; onset markers as bright dots above.  Live playhead
cursor cuts across both for a "where are we right now" read.
BPM readout in the corner.

**Wiring.**  `ModuleKind::OnsetGrid` ("ONSET GRID" label, 5×2
grid, FxMod zone, sort-group 51 next to PatternHeatmap).
Aliases onsetgrid / onset_grid / groove / groovecheck /
onsets.  Pure UI — no DSP, no audio I/O, no LLM apply path.

**Tests.**  +3 DSP-side (envelope tracks a single transient,
peak picking returns local maxima above threshold, empty
buffer guarded), +3 state-side (label, zone + no-audio-IO,
alias parsing).  Suite **2122 → 2128**.

Files: new `src/ui/panels/onset_grid.rs`, new
`src/tests/onset_grid_tests.rs`.

---

### Pattern density heatmap viz (`PatternHeatmap`)

First entry from the Visualizations wishlist.  Glanceable
"where are the busy parts" overview across all sequencer
voices in the current pattern; complementary to the focused
per-voice sequencer panels.

**Layout.**  Rows = each voice with at least one active step
in the first 16 positions; empty voices are suppressed so the
heatmap stays dense.  Columns = the canonical 16-step window.
Each cell is a small grey rect; brightness scales with the
step's velocity / accent.  Beat markers (every 4 steps) get
a slight background lift so the user reads bar structure at
a glance.  Playhead column is highlighted with a top-and-
bottom band that reads through both empty and active cells.

**Voices covered.**  All 15 drum voices (808 + 909 + Amen +
Gabber Kick), bass (with per-voice expansion when multiple
bass voices are enabled), hoover, AN1X, pluck, wavetable,
sample, FM ops, additive, modal, chiptune, vocal.  Single
read lock acquires a snapshot of intensities; the renderer
draws from the snapshot so no borrows leak across the layout
pass.

**Wiring.**  `ModuleKind::PatternHeatmap` ("PATTERN MAP"
label, 5×3 grid, FxMod zone, sort-group 50 alongside the
EventStream / ActivityTimeline cluster).  Aliases
patternheatmap / patternmap / heatmap / patterndensity /
patterns.  Pure UI — no DSP, no audio I/O, no modulation
jacks, no LLM apply path.  Reads `app.state.read().sequencer`
once per frame.

**Tests.**  +3 — label, zone + no-audio-IO, alias parsing.
Suite **2119 → 2122**.

Files: new `src/ui/panels/pattern_heatmap.rs`, new
`src/tests/pattern_heatmap_tests.rs`.

---

### Spectral Gate FX (`FxSpectralGate`)

Last item on the FX wishlist — per-band amplitude gating
across an 8-band log-spaced filter bank (~80 Hz to ~16 kHz).
Distinct from `FxGate` (broadband single-band envelope gate)
and `FxFreeze` (held buffer / spectral freeze of the entire
signal): each band has its own envelope follower, so a loud
kick can pass while quiet ambient air is gated.

V1 takes the **pragmatic route**: 8 RBJ constant-0-dB-peak-gain
BPFs in parallel rather than a textbook STFT.  The codebase
doesn't have FFT machinery yet, so this approximation gets
the spectrally-selective character without the new
infrastructure.  V2 follow-up = real STFT-based gate once
FFT lands.

**DSP shape.**  8 BPFs at log-spaced centres.  Per-band peak
envelope follower (~3 ms attack, user-controlled release
10..2000 ms).  Per-band smoothed gate state target = 1 when
env > thr, else 0; smoothing uses fast attack (~5 ms) and
the user release.  Output uses subtractive recombination —
`output = input - sum_i((1 - gate_i) * band_i)` — which
guarantees an exact passthrough when every gate is 1.0,
regardless of the BPF bank's reconstruction accuracy.

**Knobs.**
- `spec_thresh` 0..1 — linear amplitude threshold.  Default
  0 (every band stays open; FX is transparent).
- `spec_release` 0..1 → 10..2000 ms log-mapped.  Long
  values freeze low-level resonance after a transient hits
  (the "spectral hold" effect); short values give quick
  spectral gating.
- `spec_tilt` 0..1 — threshold skew across the spectrum.
  0.5 = uniform; <0.5 = high bands gate more aggressively;
  >0.5 = low bands gate more aggressively.  Skew is gentle
  (max ±2× of base) so the knob feels musical.
- `spec_mix` 0..1 with cheap-bypass fast path.

**Wiring.**  Standard FX add ritual — `FxStep::SpectralGate`
(idx 42, `FX_STEP_COUNT` bumped to 43),
`ModuleKind::FxSpectralGate` ("SPEC GATE" label, 2×1 grid,
sort-group 24 alongside `FxFreeze` — both spectral-domain
effects, both V1 approximations pending FFT machinery).
Aliases spectralgate / spectral_gate / specgate /
fxspectralgate.

**Tests.**  +5 DSP (mix=0 bypass, threshold=0 transparency
after warmup, high threshold silences a low-level signal,
loud signal passes when threshold is below the envelope,
output bounded), +6 state-side (defaults are transparent,
llm-apply all 4 knobs, lock honoured, FxStep mapping, label,
alias).  Suite **2108 → 2119**.

This entry closes out the FX wishlist; remaining wishlist
sections in PLAN.md are visualizations + modulation.

Files: new `src/audio/dsp/fx_spectral_gate.rs`, new
`src/tests/fx_spectral_gate_tests.rs`.

---

### Grain Delay FX (`FxGrainDelay`)

Granular feedback path from the FX wishlist.  Distinct from
`FxMultitap` (rhythmic taps), `FxFreeze` (held buffer), and
`FxDelay` (single tap): reads short Hann-windowed grains
scattered in time + pitch around a baseline delay, producing
a chorused, smeared, frequency-shifted echo cloud rather
than a clean delay tap.

**DSP shape.**  216 000-sample heap-allocated delay buffer
(2.25 s at 96 kHz, sized for max delay + grain length + 50 %
position scatter).  4 overlapping grains, each running an
independent fractional read at its own pitch_ratio with a
Hann window.  Trigger phase staggered by 1/N of the grain
length so the four grains always overlap in different
window stages.  On retrigger each grain picks a fresh random
position offset (±50 % of base delay × scatter) and pitch
ratio (±1 octave × scatter).  Output is windowed-sum / N for
roughly unity perceived gain.  Allocation-free; uses an LCG
(no rand crate dep) so two FX instances are deterministic
when scatter=0.

**Knobs.**
- `grain_delay` 0..1 → 50..1000 ms log-mapped (centre of the
  grain cloud).
- `grain_size` 0..1 → 20..200 ms grain length (short = chorus
  / verb cloud, long = smeared delay tap).
- `grain_scatter` 0..1.  0 = grains aligned (chorus around
  the baseline); 1 = wide pitch jitter + position scatter.
- `grain_mix` 0..1 with cheap-bypass fast path.

**Wiring.**  Standard FX add ritual — `FxStep::GrainDelay`
(idx 41, `FX_STEP_COUNT` bumped to 42),
`ModuleKind::FxGrainDelay` ("GRAIN DEL" label, 2×1 grid,
sort-group 11 alongside FxDelay / FxMultitap / FxRevDelay /
FxTapeEcho).  Aliases graindelay / grain_delay / fxgraindelay /
granulardelay / graincloud.  4 × Selector modulation jacks —
LFO on scatter gives a slowly-evolving jitter floor.

**Tests.**  +4 DSP (mix=0 bypass, audible output from steady
sine, output bounded, scatter=0 deterministic across two FX
instances), +6 state-side (defaults, llm-apply all 4 knobs,
lock honoured, FxStep mapping, label, alias).  Suite **2098
→ 2108**.

Files: new `src/audio/dsp/fx_grain_delay.rs`, new
`src/tests/fx_grain_delay_tests.rs`.

---

### Multiband compressor FX (`FxMultibandComp`)

Mastering-grade dynamics from the FX wishlist — 3-band split
+ 3 independent downward compressors.  Distinct from
`FxCompressor` (broadband single-band): each band has its own
threshold, so the user can tame a boomy low end without
flattening the air or vice versa.

**DSP shape.**  Two LP biquads (250 Hz, 2.5 kHz) split the
input into low / mid / high subtractively:
`low_band = LP_250(input)`,
`low_plus_mid = LP_2500(input)`,
`mid_band = low_plus_mid - low_band`,
`high_band = input - low_plus_mid`.  Sum of bands == input
exactly when no compression engages.  Each band runs through
its own peak-style envelope follower (~3 ms attack / ~80 ms
release) and a soft-knee compressor:
`gain = (env / thr)^(-AMOUNT)` clamped to (0, 1] when env >
thr.  AMOUNT is fixed at 0.6 (≈ 4:1 compression) for V1 so
the four UI knobs stay tight.  Allocation-free.

**Knobs.**
- `mb_low_thresh` 0..1 — low-band threshold (linear amplitude).
  Default 1.0 (above the signal's peak, so the band is
  uncompressed until the user dials it down).
- `mb_mid_thresh` 0..1.
- `mb_high_thresh` 0..1.
- `mb_mix` 0..1 wet/dry with cheap-bypass fast path.

**Wiring.**  Standard FX add ritual — `FxStep::MultibandComp`
(idx 40, `FX_STEP_COUNT` bumped to 41),
`ModuleKind::FxMultibandComp` ("MB COMP" label, 2×1 grid,
sort-group 28 next to the gate / single-band compressor /
vocoder dynamics cluster).  Aliases multiband / mbcomp /
mb_comp / mastercomp / mastercompressor / fxmultibandcomp.

**Tests.**  +5 DSP (mix=0 bypass, exact passthrough at unity
thresholds after warmup, bass compression independent of high
band, loud low band gets ducked under low threshold, output
bounded under full drive), +6 state-side (defaults are unity
passthrough, llm-apply all 4 knobs, lock honoured, FxStep
mapping, label, alias).  Suite **2087 → 2098**.

Files: new `src/audio/dsp/fx_mb_comp.rs`, new
`src/tests/fx_mb_comp_tests.rs`.

---

### Tape Echo FX (`FxTapeEcho`)

Dub-style delay with wow / flutter / saturation baked into
the feedback path.  Distinct from `FxDelay` (which can do
tape character but spreads it across five separate knobs),
`FxTapeSat` (no delay), and `FxMultitap` (rhythmic taps).
Single AGE knob folds wow + flutter + saturation + HF rolloff
together so users dial "more worn-out tape" with one gesture.

**DSP shape.**  160 000-sample heap-allocated delay buffer
(1.7 s at 96 kHz).  Two LFOs modulate the read tap (~0.5 Hz
wow + ~6 Hz flutter), depth scaling with AGE.  Linear-interp
fractional read.  Feedback path runs through a one-pole LPF
(cutoff sweeps with AGE — pristine at 0, ~1 kHz at 1) then a
tanh saturation (drive ramps 1× to 3.5× with AGE).

**Knobs.**
- `tape_echo_time` 0..1 → 25..1500 ms log-mapped (slap-back
  to dub-rhythm).  Free-running, not BPM-synced.
- `tape_echo_feedback` 0..1 → 0..0.95.  Caps sub-unity even
  at full knob to avoid runaway when AGE saturation is also
  engaged.  Default 0.4 (~3 audible repeats).
- `tape_echo_age` 0..1.  Single character knob — folds the
  four character parameters (wow / flutter / sat drive / HF
  rolloff) into one gesture.  Default 0.5.
- `tape_echo_mix` 0..1 with cheap-bypass fast path.

**Wiring.**  Standard FX add ritual — `FxStep::TapeEcho`
(idx 39, `FX_STEP_COUNT` bumped to 40),
`ModuleKind::FxTapeEcho` ("TAPE ECHO" label, 2×1 grid,
sort-group 11 next to FxDelay / FxMultitap / FxRevDelay).
Aliases tapeecho / tape_echo / fxtapeecho / dubecho /
spaceecho / echotape.  4 × Selector modulation jacks — LFO
on time gives a "warbling-pitch" patch when paired with low
feedback.

**Tests.**  +4 DSP (mix=0 bypass, audible repeats from
impulse, output bounded under max-feedback × max-age, age=0
delivers a clean digital echo near unity), +6 state-side
(defaults are dub slap-back at 250 ms / fb 0.4 / age 0.5,
llm-apply all 4 knobs, lock honoured, FxStep mapping, label,
alias parsing).  Suite **2077 → 2087**.

Files: new `src/audio/dsp/fx_tape_echo.rs`, new
`src/tests/fx_tape_echo_tests.rs`.

---

### Resonator bank FX (`FxResBank`)

Six tuned BPF biquads in parallel turn any input into a chord
layer.  Karplus-on-input character: noisy inputs ring at the
chord pitches, percussive transients pluck each pitch like a
string.  Distinct from `FxComb` (one tuned-delay resonator) —
six simultaneous pitches at once, chord-knob-selectable, with
pitch governed by the root knob rather than tracking the
input's fundamental.

**DSP shape.**  6 RBJ-cookbook constant-skirt-gain BPFs
(`b1=0`, `b2=-b0` baked into the recurrence).  Each tuned to
`root_midi + interval[i]` Hz at the same Q.  Output is the
sum of taps divided by `√(N · Q)` — gives consistent
perceived loudness across the resonance knob since the BPF's
peak gain is Q itself.  Coefficients refresh lazily on knob
movement (rare relative to audio rate).  Allocation-free.

**Knobs.**
- `resbank_root` 0..1 → MIDI 24..96 (C1..C7).  Default 0.5
  (≈ middle C).
- `resbank_chord` 0..1 quantised into 6 chord presets:
  - 0: minor 7 — root, m3, P5, m7, +oct root, +oct m3
  - 1: major triad spread — root, M3, P5, +oct root, +oct M3, +oct P5
  - 2: dominant 9 — root, M3, P5, m7, +oct root, M9
  - 3: open fifths — root, P5, +oct root, +oct P5, +2oct root, +2oct P5
  - 4: octave stack — root, +oct, +2oct, +3oct, +4oct, +5oct
  - 5: cluster — root, +M2, +P4, +P5, +M6, +oct
  Single f32 knob with quantisation inside the DSP — no u8
  mode field + cycle button, keeps the FX surface consistent
  with the other 4-knob 2×1 cards.
- `resbank_resonance` 0..1 → Q 1..50 log-mapped (1 = barely-
  resonant, 50 = singing).  Default 0.6.
- `resbank_mix` 0..1 wet/dry with cheap-bypass fast path.

**Wiring.**  Standard FX add ritual — `FxStep::ResBank`
(idx 38, `FX_STEP_COUNT` bumped to 39),
`ModuleKind::FxResBank` ("RES BANK" label, 2×1 grid,
sort-group 9 next to FxComb — same family of pitched-
resonance FX).  Aliases resbank / fxresbank / resonatorbank /
resonators / chordres / chordresonator.  4 × Selector
modulation jacks so an LFO on root drives slow chord
progressions.

**Tests.**  +4 DSP (mix=0 bypass, impulse → audible ring,
every chord preset rings, output bounded), +6 state-side
(defaults are middle-C minor7, llm-apply all 4 knobs, lock
honoured, FxStep mapping, label, alias).  Suite **2067 →
2077**.

Files: new `src/audio/dsp/fx_resbank.rs`, new
`src/tests/fx_resbank_tests.rs`.

---

### De-esser FX (`FxDeEsser`)

Specialist dynamics tool for vocal / hat material from the
FX wishlist.  Distinct from `FxCompressor` (broadband
dynamics): the de-esser only attenuates the sibilant band;
everything below the cutoff passes untouched, so the de-essed
signal sounds natural rather than dynamics-pumped.

**DSP shape.**  Complementary band split: one RBJ-cookbook
LP biquad at the cutoff produces the low band; the sibilant
band is `input - low`.  This is **phase-coherent** by
construction — `low + sibilant = input` exactly, so the FX
is bit-transparent when the ducker gain is 1.0.  An HP
biquad with subtractive complement was tried first but the
phase shift at the cutoff lets `input - sibilant` swing to
±2× the input amplitude — wrong shape for a de-esser.

**Envelope follower** runs on the sibilant band — peak-style
with ~3 ms attack / ~50 ms release.  When `env > threshold`,
the gain reduction is `(env / threshold)^(-amount)`, the
classic soft-knee compressor curve mapped through the amount
knob (amount=0 → exponent=0 → gain=1, no compression;
amount=1 → exponent=-1 → gain=thr/env, hard kill).

**Knobs.**
- `deess_freq` 0..1 → 3..12 kHz log-mapped.  Sibilant centre
  for typical vocal de-essing.  Default 0.5 (~6 kHz).
- `deess_threshold` 0..1 — linear amplitude.  When the
  sibilant-band envelope rises above this level, the ducker
  engages.  Lower = more aggressive de-essing.
- `deess_amount` 0..1.  0 = transparent; 1 = hard kill of
  the sibilant band on every over-threshold sample.
- `deess_mix` 0..1 wet/dry with cheap-bypass fast path.

**Wiring.**  Standard FX add ritual — `FxStep::DeEsser`
(idx 37, `FX_STEP_COUNT` bumped to 38), `ModuleKind::FxDeEsser`
("DE-ESSER" label, 2×1 grid, sort-group 28 next to the gate
/ compressor cluster).  Aliases deesser / deess / fxdeesser /
sibilance / sibilant.  4 × Selector modulation jacks so an
LFO on the threshold gives a "breathing" gating-style patch.

**LLM apply + schema.**  Standard `apply_fx_update` macro
expansion + flat schema entries describing the sibilant range
and the ducker behaviour.

**UI panel** lives in `rack_content_fx_lfo.rs` — repurposed
from "LFO cluster" to "spillover for new FX" once the file
naming gap stopped mattering vs the LOC cap on
`rack_content_fx_extras.rs`.  Header comment updated.

**Tests.**  +5 DSP (mix=0 bypass, amount=0 transparency
after warmup, 6 kHz sine gets ducked, 100 Hz sine passes
through, output bounded), +6 state-side (defaults, llm-apply
all 4 knobs, lock honoured, FxStep mapping, label, alias).
Suite **2056 → 2067**.

Caught a bug in the cached-coefficient pattern while at it:
initialising `cached_freq` to NaN means `(fc - NaN).abs() > 1.0`
returns FALSE (NaN compares as not-greater), so the biquad
never refreshes.  Fix is `!cached_freq.is_finite()` as a
first-time-init flag.

Files: new `src/audio/dsp/fx_deesser.rs`, new
`src/tests/fx_deesser_tests.rs`.

---

### 3-band ISO / kill EQ (`FxIsoEq`)

DJ-style hard-kill bands (LOW / MID / HIGH) at fixed
crossovers (~250 Hz, ~2.5 kHz).  Performance FX, distinct
from `FxEq` — that one is 3-band fixed-shelf with continuous
gain knobs; this one is unipolar 0..1 (kill / pass) with a
subtractive midband.

**DSP shape.**  Two RBJ-cookbook biquads — LP at 250 Hz for
the low band, HP at 2.5 kHz for the high band.  The mid band
is computed *subtractively*: `mid = dry - low - high`.  This
guarantees that when all three knobs are at 1.0, the bands
sum to the dry input EXACTLY, with no phase-cancellation
surprises as the user dials between extremes.  Allocation-
free; coefficients refresh lazily on sample-rate changes.

**Knobs.**
- `iso_low` 0..1 — gain on the band below ~250 Hz.  1 = full
  pass; 0 = silenced.  Default 1.0.
- `iso_mid` 0..1 — gain on the (subtractive) mid band.
  Default 1.0.
- `iso_high` 0..1 — gain on the band above ~2.5 kHz.
  Default 1.0.
- `iso_mix` 0..1 — wet/dry blend; 0 = bypass.  Default 0.0.
  All-bands-at-unity + mix=1 is a passthrough by design, so
  the user can A/B the FX engagement without dialling all
  three bands back to 1.

**Wiring.**  Standard FX add ritual — `FxStep::IsoEq` (idx
36, `FX_STEP_COUNT` bumped to 37), `ModuleKind::FxIsoEq`
("ISO EQ" label, 2×1 grid, sort-group 19 next to FxDjFilter
since both are DJ-style performance filters).  Aliases
iso / isoeq / iso_eq / killeq / kill / 3band / fxisoeq.
4 × Selector modulation jacks so the user can sequence kill
patterns by routing an LFO at a beat-aligned rate.

**LLM apply + schema.**  Standard `apply_fx_update` macro
expansion + flat schema entries describing the kill
behaviour and the subtractive midband (so the LLM can pitch
patches like "ISO out the low end on every other bar").

**Tests.**  +5 DSP (mix=0 bypass, exact passthrough at unity
gains after warmup, 50 Hz silenced when low killed, 8 kHz
silenced when high killed, output bounded), +6 state-side
(defaults, llm-apply all 4 knobs, lock honoured, FxStep
mapping, label, alias parsing).  Suite **2045 → 2056**.

Files: new `src/audio/dsp/fx_iso_eq.rs`, new
`src/tests/fx_iso_eq_tests.rs`.

---

### Vibrato FX (`FxVibrato`)

Pitch-modulation cousin of the shipped `FxTremolo`.  Single
delay-line tap whose read offset is modulated by an internal
LFO, producing pitch wobble without level swing.  Distinct
from `FxChorus` — chorus mixes multiple delay taps with the
dry to thicken; vibrato is one tap, no internal dry blend in
the wet path so the user hears pure pitch wobble.

**DSP shape.**  1024-sample static delay line (10 ms at 96 kHz —
the highest the engine ever runs at).  Write index advances
each sample; read offset = baseline (5 ms) + depth × LFO ×
max swing (5 ms), linearly interpolated.  Same sine→square
LFO morph as the tremolo for visual + sonic consistency.
No allocations.

**Knobs.**
- `vibrato_rate` 0..1 → 0.1..10 Hz log-mapped.  Caps lower
  than tremolo (12 Hz) — hyper-fast pitch wobble crosses into
  FM-sideband territory where the effect stops reading as a
  vibrato.
- `vibrato_depth` 0..1 → 0..±5 ms peak delay-time deviation
  (≈ 0..50 cents pitch swing at 5 Hz).
- `vibrato_shape` 0..1.  Sine (smooth pitch curve) → near-
  square (warbly two-pitch hop) via the same 16× tanh-clamped
  sine lerp the tremolo uses.
- `vibrato_mix` 0..1 wet/dry with cheap-bypass fast path.

**Wiring.**  Same FX add ritual as Tremolo — `FxStep::Vibrato`
(idx 35, `FX_STEP_COUNT` bumped to 36), `ModuleKind::FxVibrato`
("VIBRATO" label, 2×1 grid, sort-group 36 alongside Tremolo /
Pan).  `FxState::vibrato_*` flat fields with defaults
rate=0.45 / depth=0.5 / shape=0 / mix=0.  Aliases vibrato /
vib / fxvibrato / pitchmod / pitchwobble.  4 modulation
selectors so every knob can ride an LFO (slow rate sweep
yields a "warming-up vibrato" patch).

**LLM apply + schema.**  Standard `apply_fx_update` macro
expansion + flat schema entries.

**Tests.**  +4 DSP (mix=0 bypass, depth=0 transparency on a
warmed buffer, audible pitch modulation at full depth, output
bounded), +6 state-side (defaults, llm-apply, lock honoured,
FxStep mapping, label, alias parsing).  Suite **2035 → 2045**
lib tests passing.

Files: new `src/audio/dsp/fx_vibrato.rs`, new
`src/tests/fx_vibrato_tests.rs`.  ~14 files touched — same
short FX add ritual as Tremolo.

---

### Tremolo FX (`FxTremolo`)

First entry from the FX wishlist.  Internal-LFO amplitude
modulation — the dedicated module users reach for when they
type "add a tremolo".  Distinct from `FxPan` (left/right
balance LFO with no level swing) and from chorus / flanger
(delay-line modulation, not AM).

**DSP shape.**  One sample per phase advance + a sine eval +
a tanh + a single multiply per channel.  No buffers, no
lookups.  Phase is kept across `process` calls so the LFO
stays continuous when the user sweeps `rate` or toggles bypass.

**Knobs.**
- `tremolo_rate` 0..1 → 0.1..12 Hz log-mapped.  Slow swell
  at one end through helicopter-chop at the other.
- `tremolo_depth` 0..1.  At 0 the LFO has no effect; at 1 the
  gain swings between 0 and 2 (full chop on one side, +6 dB
  boost on the other).  Centre stays at unity regardless of
  the depth knob.
- `tremolo_shape` 0..1.  Lerps between pure sine (smooth
  swell) and a 16× tanh-clamped sine (near-square hard chop).
  Continuous across the whole knob — no mode flag.
- `tremolo_mix` 0..1 wet/dry.  0 triggers the cheap-bypass
  fast path so an inserted-but-disengaged tremolo costs ~zero.

**Wiring.**  Standard FX add ritual — `FxStep::Tremolo` (idx
34, `FX_STEP_COUNT` bumped to 35), `ModuleKind::FxTremolo`
("TREMOLO" label, 2×1 grid, sort-group 36 next to `FxPan`),
`FxState::tremolo_*` flat fields, `parse_module_kind`
aliases (`tremolo` / `trem` / `ampmod` / `ampl_mod`),
modulation jacks (4 × Selector — every knob can ride an LFO,
including rate for "speeding-up" effects).

**LLM apply + schema.**  `apply_fx_update` writes all four
knobs through the standard `unlocked_f32` macro; schema
entries describe the rate range and depth swing for the LLM.
While in here, plugged a pre-existing gap — `vinyl_*` knobs
were missing from the apply path; added them too so the
LLM can drive vinyl/cassette character via the `fx` object.

**Tests.**  +6 — DSP-side (mix=0 bypass, full-depth swing
near 0..2× input, depth=0 transparency, square shape dwells
at extremes, output bounded under full drive).  State-side
(defaults are engaged-neutral, llm apply writes all 4 knobs,
locked mix is honoured, ModuleKind ↔ FxStep mapping, label,
alias parsing).  Suite **2024 → 2035** lib tests passing.

Files: new `src/audio/dsp/fx_tremolo.rs`, new
`src/tests/fx_tremolo_tests.rs`.  ~13 files touched — FX
add ritual is much shorter than the voice ritual since FX
share the flat `FxState` rather than each having their own
struct + sequencer lane.

---

### Vocal formant synth voice

Last entry on the Voices wishlist — closes the "sing a vowel
without loading a phoneme model" gap that `NeuTts` deliberately
sidesteps.  Pure DSP: a sawtooth source through three parallel
RBJ-cookbook bandpass biquads tuned to the F1 / F2 / F3
formants of the played vowel.  The vowel character comes
entirely from the resonance pattern — no language, no model.

**DSP shape.**  Saw oscillator (broad harmonic content for
the formants to carve) → 1-pole brightness LP (low = hummed
vowel character, high = sung) → three formant biquads in
parallel summed at 1/3 weight.  Per-formant Q = 8 / 10 / 12
(higher Q on F3 keeps the upper resonance from washing into
F2).  Coefficients refresh lazily — only when vowel / morph /
formant_shift / sample-rate change, never per-sample.  Voice
ADSR matches the FM-ops / SAMPLER+ shape for consistent feel.

**Vowel table.**  Five presets — A / E / I / O / U — at
Peterson & Barney 1952 male averages, the canonical vowel-
formant reference.  The `morph` knob blends linearly from the
selected preset toward `(vowel + 1) % 5` so the user can hold
a vowel or sweep between adjacent ones; `formant_shift` then
applies a uniform multiplier across F1 / F2 / F3 (range
0.66×..1.51× — male → female / child / monster) so the
vocal-tract size moves without changing the played pitch.

**State.**  `VocalState` with enabled / volume / pan / vowel
(u8 0..=4) / morph / brightness / formant_shift + ADSR.
`VOCAL_VOWEL_PRESETS = 5` constant lives next to the table to
keep the apply-clamp and the DSP in sync.

**Sequencer integration.**  Full lane mirroring the other
melodic voices — `vocal_pattern`, `vocal_steps`,
`VocalTrigger` / `VocalGateOff` events, `gate_counter_vocal`,
`rack_vocal` derived flag.

**LLM apply + schema.**  `apply_vocal_update` (in
`llm_helpers_voices_v2`) clamps `vowel` to the preset range
defensively so a stale LLM number can't index past the
table; honours per-field locks.  Schema entry sits next to
the chiptune one with `vowel` declared as a 0..4 integer
plus `morph` / `brightness` / `formant_shift` / ADSR /
`vocal_steps` / `vocal_notes`.

**Alias hygiene.**  `voice` was deliberately NOT added as a
parse alias — it collides with the `NeuTts` voice (which
already owns that word) and would have rendered the
`parse_module_kind` arm unreachable.  Aliases are
`vocal` / `vocalvoice` / `vowel` / `formant` / `choir`.

**UI.**  5×3 panel.  Header row: ON/OFF + VOLUME + PAN +
VOWEL cycle button (A → E → I → O → U).  Glass-grouped
control row: MORPH + BRIGHT + SHIFT + ADSR.

**Tests.**  13 new — DSP-side: silence-before-trigger /
when-disabled, audible-after-trigger, every-vowel-preset-
produces-output (catches a regression where a future preset
with too-high F3 hits the Nyquist clamp and silences the
band), output-bounded-under-full-drive, release-eventually-
silences.  State-side: defaults are A / male-average,
apply-knobs, vowel preset clamp, locked-formant_shift, lane
plumbing, label, alias parsing (with `voice` deliberately
excluded), zone + audio-output.  Full suite **2011 → 2024**.

Files: new `src/state/vocal.rs`, `src/audio/dsp/vocal.rs`,
`src/ui/panels/vocal.rs`, `src/tests/vocal_tests.rs`.
~22 files touched — voice-add ritual unchanged from the
Modal / Additive / Chiptune ships.

This entry closes out the Voices wishlist; remaining wishlist
sections in PLAN.md are FX / visualizations / modulation.

---

### Chiptune (SID-flavoured) voice

3-oscillator chiptune voice modelled on the Commodore 64's
SID 6581/8580.  PLAN.md called for a "2× pulse + triangle +
LFSR noise" NES-style voice; user requested the SID flavour
instead, which the implementation prioritises.

**DSP shape.**  Three independent oscillators, each freely
selectable between four waveforms:
- **Saw** — bright + harmonic-rich (the SID-classic lead).
- **Triangle** — 16-step staircase quantisation reproducing
  the SID's defining triangle character (the straight-edge
  pieces make the wave more buzzy than a smooth analogue
  triangle).
- **Pulse** — variable duty cycle via the shared `pulse_width`
  knob.  0.5 = square; off-centre values produce the SID's
  PWM character.
- **Noise** — 23-bit-style LFSR with `x^23 + x^18 + 1` taps.
  Pitches with the played note (the LFSR clock advances at
  the played frequency, not the audio rate) — faithful to
  the SID's frequency-synced noise generator.

Per-osc ADSR (independent envelopes — what makes SID
percussion arrangements possible from a single voice).  Single
shared resonant filter (LP / BP / HP) reusing the standard
`Svf` from `fx_extras` — same SVF the FX module + SAMPLER+'s
per-voice filter use, so chiptune patches inherit familiar
filter character.  Filter is bypassed by default (mix = 0)
so a freshly-enabled patch sounds bright and raw before the
user dials it in.

**Ring-mod and hard-sync** — the SID's signature flags.  Ring
mod multiplies osc 1's output by `sign(osc 2)` for clangy /
metallic / bell timbres.  Hard sync resets osc 2's phase
when osc 1 wraps; combined with osc 2 at a non-integer ratio
(or different waveform / pulse width) this is the
sync-sweep lead Hubbard / Galway / Tel built so much of the
C64 catalogue around.

**State.**  `ChiptuneState` with 3× `SidOsc` (waveform u8,
level, ADSR — 6 fields per osc), shared `pulse_width`,
filter (cutoff, resonance, mode, mix), `ring_mod` and `sync`
flags, voice volume + pan + enabled.  Default = saw on osc 1
+ pulse on osc 2 (slight detune lead) + osc 3 silent.

**Sequencer integration.**  Full lane mirroring the other
melodic voices — `chiptune_pattern`, `chiptune_steps`,
`ChiptuneTrigger` / `ChiptuneGateOff` events,
`gate_counter_chiptune`, `rack_chiptune` derived flag.

**LLM apply + schema.**  `apply_chiptune_update` reads
nested per-osc objects (`chiptune.osc1.waveform`, etc.),
clamps waveform / filter_mode out-of-range values, honours
locks per-field.  Reusable `chiptune_osc_schema` cloned
across the 3 osc slots in the JSON schema (matches the
FM-ops / additive pattern).  Schema description prompts the
LLM toward SID-classic patches: "saw on osc 1 + slightly-
detuned (or pulse-mode + PWM) osc 2; engage `sync` for the
sync-sweep timbre, `ring_mod` for clangy bell timbres."

**UI.**  6×5 panel.  Header row: ON/OFF + VOLUME + PAN +
RING/SYNC toggle buttons (the SID-defining flags, exposed
upfront rather than buried in a menu).  3 oscillator rows,
each glass-grouped: OSC-N label + WAVE cycle button (SAW →
TRI → PULSE → NOISE) + LEVEL + ATTACK / DECAY / SUSTAIN /
RELEASE.  Filter row: CUTOFF + RESONANCE + MIX + MODE
cycle (LP → BP → HP) + PULSE WIDTH.

**Tests.**  17 new — DSP-side: silence-before-trigger /
when-disabled, audible-after-trigger, every-waveform-
produces-output (catches a regression where the noise
dispatch falls through to silence), output-bounded-under-
full-drive, release-eventually-silences, sync-resets-osc2-
phase-when-osc1-wraps.  State-side: defaults / apply-knobs
/ apply-per-osc / waveform clamp / filter mode clamp / lock
/ sequencer lane / label / alias parsing / zone +
audio-output.  Full suite **1993 → 2010**.

Files: new `src/state/chiptune.rs`, `src/audio/dsp/chiptune.rs`,
`src/ui/panels/chiptune.rs`, `src/tests/chiptune_tests.rs`.
~22 files touched — voice-add ritual unchanged from the
Modal / Additive ships.

---

### Modal / struck physical-model voice

Cheap N-mode resonator from the Voices wishlist.  Plugs the
marimba / bell / glass / metal-bar gap that the harmonic-stack
voices (Additive, AN1X) can't reach — modal synthesis models
each mode as a damped sinusoid, so the inharmonic ratios that
characterise real struck-percussion timbres come naturally.

**DSP shape.**  8 parallel two-pole resonant biquads excited by
a short LP-filtered noise burst on each trigger.  Each
resonator implements `y[n] = 2·r·cos(ω)·y[n-1] − r²·y[n-2] +
in_scale·x[n]`; the coefficients give an impulse response of
`A·exp(-t/τ)·sin(2πfₙt)` — exactly modal synthesis's damped
sinusoid.  `in_scale = √(1 − r²)` is the energy-preserving
input gain — without it, long-decay modes (r → 1) blow up
because their resonance gain scales with Q; the full `(1 − r²)`
form would clamp output too aggressively at long decays.  256-
sample noise burst (~5.3 ms) excites every mode in the bank
on each trigger.

**Ratio presets.**  The mode-frequency relationship picks the
character.  Four presets in V1, all sourced from acoustics
references for idealised resonators:
- **0 Harmonic**: integer multiples 1, 2, 3, … 8 — string- /
  pluck-like timbres.
- **1 Bell**: 1, 2, 2.4, 3, 4.5, 5.33, 6.66, 8 — idealised
  church bell with a strong "hum tone" feel.
- **2 Tubular**: 1, 2.756, 5.404, 8.933, … — narrower
  inharmonic spread than the bell.
- **3 Metal**: 1, 3.984, 9.933, 11.32, … — strong odd-mode
  emphasis with glassy overtones (uniform-thickness metal bar).
Default preset is Bell — the most musically distinctive of the
four, and the most interesting straight out of the box.

**State.**  `ModalState` with `[f32; 8] levels` (per-mode
amplitude), `brightness`, `decay_scale` (global ring time —
~5 ms to ~5 s on the fundamental, with each higher mode dying
~30 % faster per index step), `ratio_preset` (clamped
`0..=3`), plus voice volume + pan + enabled.

**Sequencer integration.**  Full lane mirroring Additive /
FM-ops — `modal_pattern`, `modal_steps`, `ModalTrigger` /
`ModalGateOff` events, `gate_counter_modal`, `rack_modal`
derived flag.  Gate-off dampens the bank's state ×½ as a
"hand on the bell" gesture rather than abrupt silence.

**LLM apply + schema.**  `apply_modal_update` with the same
shorter-array-keeps-trailing semantics as Additive.  Schema
prompts the LLM on bell / marimba / glass / tubular / metal
percussion requests.  `ratio_preset` clamps defensively at
apply time.

**UI.**  6×3 panel mirroring Additive's UX: header row
(ON/OFF + VOLUME + PAN + BRIGHTNESS + DECAY + preset cycle
button cycling HARMONIC → BELL → TUBULAR → METAL) above an
8-bar drawable mode-amplitude histogram.  Click or drag to set
per-mode amplitudes; fundamental column brighter than the rest;
all 8 columns labelled (vs. Additive's every-4 spacing — we
have room with only 8).

**Tests.**  16 new — DSP-side: silence-before-trigger,
silence-when-disabled, audible-after-trigger, fully-pegged
spectrum bounded, modes-die-after-short-decay, every-preset-
produces-audible-output (catches regressions where a too-high
ratio hits the Nyquist clamp).  State-side: defaults-are-bell-
preset, apply-knobs / apply-levels / shorter-array / lock /
preset clamp / sequencer lane / label / alias parsing / zone +
audio-output.  Full suite **1977 → 1993**.

Files: new `src/state/modal.rs`, `src/audio/dsp/modal.rs`,
`src/ui/panels/modal.rs`, `src/tests/modal_tests.rs`.  Edits
across the voice-add ritual: same ~22 files as the Additive
ship.

---

### Additive synth voice

16-partial harmonic-series voice from the wishlist.  Distinct
from `WavetableVoice` (which scans pre-baked frames): the user
draws the spectrum directly via per-partial level sliders.
Sequencer-driven like every other melodic voice.

**DSP shape.**  16 sine-oscillator phase accumulators, each at
`(i+1) × base_freq` where `base_freq` is set on trigger from
the played MIDI note.  Per-partial levels (0..1) weight the
sum; output is normalised by the sum of the levels so a fully-
pegged spectrum stays bounded at ~1.0 amplitude before
envelope.  A single voice-wide ADSR shapes the summed output —
per-partial decays deferred (a follow-up could add per-partial
decay rates to mimic real instruments where high partials die
first).  Allocation-free in `process()`.

**State.**  `AdditiveState` with `[f32; 16] levels`, voice
volume / pan / enabled, plus voice-wide ADSR.  Default partial
bank uses a 1/n falloff (sawtooth-like) so a freshly enabled
module produces a recognisable harmonic-rich tone rather than
a single flat sine.

**Sequencer integration.**  Full lane mirroring Pluck /
Wavetable / Sample / FM-ops — `additive_pattern: Vec<TB303Step>`,
`additive_steps: usize`, `AdditiveTrigger` / `AdditiveGateOff`
events, `gate_counter_additive` in `ClockState`,
`rack_additive` derived flag in `AudioParams`.

**LLM apply + schema.**  `apply_additive_update` accepts the
voice fields plus a `levels` array of up to 16 floats.
Shorter arrays leave the trailing partials untouched (LLM can
adjust just the fundamental + 2nd + 3rd without repeating the
1/n tail every time).  Schema entry prompts the LLM to reach
for additive on organ / drawbar / harmonic-rich / pure-sine-
stack requests.

**UI.**  6×3 panel.  Header row: ON/OFF + VOLUME + PAN +
ATTACK / DECAY / SUSTAIN / RELEASE.  Below: an interactive
16-bar harmonic histogram inside a glass pane.  Click a column
to set its partial level; drag horizontally to "draw" a curve
across the spectrum in a single sweep.  Fundamental (column 0)
gets a brighter shade so the user knows where the played-note
pitch sits; harmonic numbers labelled at columns 1, 5, 9, 13
to keep the strip readable without crowding every column.

**Tests.**  15 new — DSP-side: silence-before-trigger,
silence-when-disabled, audible-after-trigger, fully-pegged
spectrum bounded, release silences, single-partial drives at
correct frequency (zero-crossing count of 4th-only harmonic
matches 400 Hz when fundamental = 100 Hz).  State-side:
defaults / apply-knobs / apply-levels / shorter-array-keeps-
trailing / lock-honoured / sequencer-lane / label / alias
parsing / zone + audio-output.  Full suite **1962 → 1977**.

Files: new `src/state/additive.rs`,
`src/audio/dsp/additive.rs`, `src/ui/panels/additive.rs`,
`src/tests/additive_tests.rs`.  Edits across the usual
voice-add ritual: `src/state/mod.rs`,
`src/state/sequencer_state.rs`, `src/state/module_kind.rs`,
`src/state/modulation.rs`, `src/state/rack.rs`,
`src/state/rack_random.rs`, `src/state/rack_scope.rs`,
`src/state/rack_wiring.rs`, `src/state/llm_helpers.rs`,
`src/state/llm_apply.rs`, `src/sequencer/mod.rs`,
`src/audio/dsp/mod.rs`, `src/audio/dsp/params.rs` +
`params_from.rs`, `src/audio/dsp/process_block.rs`,
`src/audio/dsp/trigger_handler.rs`, `src/llm/schema.rs`,
`src/ui/rack_content.rs`, `src/ui/module_card.rs`,
`src/ui/panels/mod.rs`, `src/tests/mod.rs`.

---

### Refactor: split `fx_extras.rs` glitch family into a sibling

`src/audio/dsp/fx_extras.rs` was at 989 lines — 11 from the
1000-line cap, with the next FX add guaranteed to push it
over.  Extracted the glitch / time-domain performance family
(`TapeStop`, `Stutter`, `Freeze`) into a new
`src/audio/dsp/fx_glitch.rs` sibling.  Pure no-behaviour
refactor: `mod.rs` declares the new module + `use
fx_glitch::*;` so existing call sites keep working
unchanged.  Test imports updated from `fx_extras::TapeStop`
etc. to `fx_glitch::TapeStop`.

Before: `fx_extras.rs` 989 / `fx_glitch.rs` (does not
exist).  After: `fx_extras.rs` 628 / `fx_glitch.rs` 379.
1962 tests pass identically; cargo clippy clean.

The 7 surviving structs (Flanger, Limiter, Svf, CombRes,
Tilt, Transient, Exciter) plus Multitap + RevDelay all
remain in `fx_extras.rs` — both because they're shorter
than the glitch trio and because `Svf` is referenced by
`audio/dsp/sample_instrument.rs` as `super::fx_extras::Svf`,
so keeping it in place avoids touching unrelated code.

---

### Continuous Link bar-phase drift correction (V2.1)

V2 shipped the off→on bar-phase snap.  Even after that initial
snap, the local clock can drift from the network across a long
session — audio scheduling jitter, paused transports,
fine-grained phase adjustments Link makes that pure BPM
tracking misses.  V2.1 closes the loop with a continuous
re-snap pass.

**Policy.**  Pure helper `drift_resnap_target(local_bar_step,
expected_bar_step, bar_steps, tolerance, since_last_resnap,
min_interval)` returns `Some(target_step)` when the caller
should re-snap.  Two thresholds:

- **Tolerance** (`DRIFT_TOLERANCE_STEPS = 1`): drift of one
  step at the current grid is below the audible-jitter floor;
  ignored to prevent perpetual chasing on noisy sources.
- **Rate limit** (`DRIFT_CHECK_INTERVAL = 1 s`): consecutive
  re-snaps are spaced at least a second apart so a thrashing
  network can't cause the sequencer to glitch on every UI
  tick.  Catastrophic drift (more than `bar_steps / 4`)
  bypasses the rate limit because something dramatic
  happened (paused transport, late-joining peer with a
  wildly different clock).

Drift is computed with a shortest-path metric on the circular
bar — drifting forward 14 steps and backward 2 steps both
collapse to "off by 2," so the helper picks the cheaper
correction.

**Wiring.**  `tick_link_drift_correction` in `link_handler.rs`
runs every UI tick when Link is enabled and the sequencer is
running (no point chasing phase while the transport's
stopped — the local step is frozen and would always look
drifted).  Snapshots `current_step` + `step_division` from
`AppState`, asks the network for its phase, calls the pure
helper, and pushes `AudioCommand::SnapClock { step }` when
told to.  The off→on snap path is unchanged; it now stamps
`last_link_drift_resnap` so the drift loop doesn't double-
snap on the very next tick.

**Tests.**  7 new for the pure helper covering: zero drift,
drift at exact tolerance, moderate drift past tolerance,
moderate drift suppressed by rate limit, catastrophic drift
bypassing rate limit, shortest-path wrap-around, first-ever
check has no rate limit, defensive zero-bar-steps no-panic.
Full suite **1955 → 1962**.

Files: `src/ui/link_handler.rs` (helper + dispatch + tests),
`src/ui/mod.rs` (`last_link_drift_resnap` field + default).

---

### SampleInstrument V2 — Stage 7.5: drag-to-edit on the viz strip

The visualizer strip was read-only at Stage 7.  Stage 7.5
makes both modes interactive.

**Single-WAV mode — loop-marker drag.**  `draw_waveform` now
takes `&mut loop_start` / `&mut loop_end` and sense
`click_and_drag`.  Hovering within 6 px of either boundary
brightens + thickens that stem and switches the cursor to
`ResizeHorizontal`.  Click-and-drag updates the corresponding
fraction in real time, clamped so `loop_start + 0.001 ≤ loop_end`
(the markers can't cross).  Drag target latches into egui's
per-id memory at `drag_started` so the user can pull the
cursor away from the boundary mid-drag and the action still
resolves to the right stem.  Caller writes the new fractions
back to `state.sample_instrument` only when the helper
reports a change.

**SFZ mode — click-to-select region.**  `draw_zone_map` takes
`&mut Option<usize>` for the selected region.  Pre-computes
every band rect so the click hit-test reuses the exact paint
geometry; first band hit wins, clicking outside any band
clears the selection.  Selected band renders at `gray(220)`
with a `gray(255)` outline; its `pitch_keycenter` tick flips
dark so it stays visible against the brighter shade.  Stale
selection (index past the new region list after an SFZ
swap) is auto-cleared inside the helper so the inspector
never indexes a missing region.

**Per-zone inspector.**  When a region is selected,
`draw_zone_inspector` renders a 3-line read-only readout
beneath the zone map: line 1 the sample filename basename;
line 2 `lokey-hikey  vel L-H  root N` using a new `midi_label`
helper that converts MIDI numbers to scientific-pitch labels
(C4, A#3, etc.); line 3 the per-region opcodes (`±X.X dB`,
`tune ±Yc`, `transp ±Zst` only when non-zero, `RR P/L` only
when round-robin is active).  V1 is read-only — future
iteration can add inline edit.

**State.**  `ImpulseApp.sample_selected_region: Option<usize>`
holds the UI-only selection (not persisted to `AppState`).
Cleared when a fresh SFZ loads or a single-WAV swap empties
the region list.

Tests: 2 new for `midi_label` covering the well-known anchor
notes (C4 = 60, A4 = 69 = 440 Hz) and a chromatic walk
through C4–B4 confirming sharps land where expected.  Full
suite **1953 → 1955**.

Files: `src/ui/panels/sample_instrument_viz.rs` (interactive
helpers + `midi_label` + inspector + tests),
`src/ui/panels/sample_instrument.rs` (call sites),
`src/ui/mod.rs` (`sample_selected_region` field +
default).

---

### FM operator synth (kickoff #3)

4-op DX7-flavoured voice — closest gap to the existing AN1X
subtractive.  DX-flavoured bell / E-piano / FM-bass / metallic
stab tones that don't reproduce from any other voice.

**DSP shape.**  Four sine oscillators, each with its own ADSR
envelope, a frequency ratio (knob 0..1 → 0.5..8× the played
note via a log-symmetric `16^(k-0.5)` map so unison sits at
the 0.5 detent), and an output level.  Per-op envelopes are
the key to FM character — modulator decays shorter than
carriers gives the bright→mellow bell tail, etc.  Modulation
index `FM_INDEX_MAX = 8.0` (4× the standard DX7 unit), enough
for bell territory without breaking spectra.  2× oversampling
not needed because the SVF-style integrator pattern doesn't
apply — phase-modulation is implicitly band-limited by
`level * env`.

**Algorithms.**  V1 ships four (DX7 has 32 — extending the
list only requires growing the match arm in
`audio/dsp/fm_ops.rs`).  Picked to span the most common shapes:

- **0 — Stack**: 4→3→2→1.  Op 1 is the only carrier.  Rich
  harmonic cascade — the FM-bass / FM-lead workhorse.
- **1 — Multimod**: 4→1, 3→1, 2→1.  Op 1 is the carrier
  with three parallel modulators.  Bell / mallet timbres.
- **2 — Parallel pairs**: 4→3, 2→1.  Two stacks summed —
  ops 1 and 3 both carriers, each with one modulator.
  Layered two-tone patches.
- **3 — Additive**: all four ops are carriers, no FM.  Pure
  sine stack — organ / Hammond / clean leads.

Feedback applies to op 4 (the topmost modulator on chain
algorithms) — adds saw-like spectral richness; at extreme
settings op 4 self-oscillates into noise.

**Sequencer integration.**  Full sequencer lane mirroring
the Pluck / Wavetable / Sample lanes — `fm_ops_pattern: Vec<TB303Step>`,
`fm_ops_steps: usize`, `FmOpsTrigger` / `FmOpsGateOff` events,
`gate_counter_fm_ops` in `ClockState`, `rack_fm_ops` derived
flag in `AudioParams`, dispatch arm in `trigger_handler.rs`.
The voice plays from the sequencer like every other melodic
voice — drop the module in the rack and dial steps.

**State + LLM.**  `FmOpsState` with 4 nested `FmOp` sub-structs
(ratio + level + ADSR each).  29 fields total.  Defaults: 2-op
stack — op 1 carrier full, op 2 modulator 0.5, ops 3-4 silent.
`apply_llm_update` accepts nested per-op JSON
(`fm_ops.op1.ratio`, etc.) for readable schema; algorithm
clamps to valid range; locks honoured per field.  Schema entry
prompts the LLM to reach for FM ops on bell / E-piano / FM
bass / lead requests.

**UI.**  6×5 panel.  Header row: ON/OFF + 4 algorithm chips
(routing diagrams as labels: `4→3→2→1`, `4,3,2 → 1`,
`4→3 / 2→1`, `1+2+3+4`) + VOLUME / PAN / FEEDBACK.  Then four
op rows, each glass-grouped: OP-N label + RATIO (φ-bigger) +
LEVEL (φ-bigger) + ATTACK / DECAY / SUSTAIN / RELEASE.  Per-
op grouping makes the four ops read as distinct units.

**Tests.**  15 new — DSP-side: silent-before-trigger,
silent-when-disabled, audible-after-trigger, additive-sums,
release-eventually-silences, bounded-under-stress.  State-
side: defaults, apply (global + per-op), lock honouring,
algorithm clamp, sequencer-lane plumbing, label, alias
parsing, audio-output / Voice zone.  Full suite **1938 → 1953**.

Files: new `src/state/fm_ops.rs` (state),
`src/audio/dsp/fm_ops.rs` (DSP), `src/ui/panels/fm_ops.rs`
(UI), `src/tests/fm_ops_tests.rs` (state tests).  Edits
across `src/state/mod.rs` (AppState),
`src/state/sequencer_state.rs` (lane fields + defaults),
`src/state/module_kind.rs` (variant + 5 exhaustive matches),
`src/state/modulation.rs` (mod_inputs),
`src/state/rack.rs` (arrange order + zone matchset),
`src/state/rack_random.rs` (voice pool),
`src/state/rack_scope.rs` (label / parse / kind_matches),
`src/state/rack_wiring.rs` (sequencer-cabled voices),
`src/state/llm_helpers.rs` (apply helper),
`src/state/llm_apply.rs` (apply dispatch),
`src/sequencer/mod.rs` (TriggerEvent + emit + gate counter),
`src/audio/dsp/mod.rs` (DspState wiring),
`src/audio/dsp/params.rs` + `params_from.rs` (29 fields +
`rack_fm_ops`), `src/audio/dsp/process_block.rs` (per-frame
mix + sends + master pan), `src/audio/dsp/trigger_handler.rs`
(trigger dispatch), `src/llm/schema.rs` (schema +
`fm_op_schema` reusable),
`src/ui/rack_content.rs` (voice dispatch),
`src/ui/module_card.rs` (title_fill).  Roughly 22 files
touched — close to the wrap-up memory's "23-file Voice-add
ritual" estimate.

---

### DJ filter (kickoff #2)

Single-knob morph FX from the FX wishlist — sweep one
control to crossfade LP → BP → HP with a resonance peak at
the crossover.  Plugs the live-performance gap that
`FxFilter` doesn't fill: the static SVF needs separate
cutoff / mode / drive knobs and stays at one mode at a
time, while the DJ filter is meant for one-handed sweeps
where the cutoff and mode move together.

**DSP shape.**  One state-variable filter integrator
computes LP / BP / HP every sample; triangular crossfade
weights pick a pure mode at each end and a smooth blend in
between (`w_low = max(0, 1−2m)`, `w_band = 1 − |2m−1|`,
`w_high = max(0, 2m−1)`).  Cutoff sweeps log-symmetrically
80 Hz → 1 kHz → 8 kHz with morph so that morph=0 reads as
"low cut heavy", morph=1 as "high cut heavy", and morph=0.5
lands at the audibly-sensitive 1 kHz centre.  Resonance Q
is φ-emphasised at the morph midpoint
(`q = 0.5 + res * (4 + bp_emphasis * 12)`) so the peak is
narrow at the crossover and wider at the LP/HP edges, which
matches the audible "resonance peak emerging at the
midpoint" the user asked for.  2× oversampled SVF for
stability at the high-cutoff end.

**State + LLM.**  3 fields on `FxState`
(`dj_filter_morph`, `dj_filter_resonance`, `dj_filter_mix`).
Defaults: morph 0.5 (centre crossover, neutral starting
position), resonance 0.4 (audible peak without screaming),
mix 0.0 (bypass on insert).  Wired through
`apply_llm_update`, the JSON schema (`fx.dj_filter_*`),
locked-param honouring, and the rack-scope name parser
(aliases: `djfilter` / `dj_filter` / `dj filter` /
`fxdjfilter` / `morphfilter`).

**UI.**  2×1 card.  MORPH lives alone in a φ-bigger glass
group (it's the entire identity of the FX); RESONANCE + MIX
sit at default size beside it.  Three-knob XY pad available
when expanded.  Added to the random-rack pool.

**Tests.**  12 new — DSP-side: dry-when-mix-zero, morph=0
LP behaviour, morph=1 HP behaviour, BP peak at 1 kHz at
morph=0.5, BP narrows at high resonance, output bounded
under stress.  State-side: defaults, apply_llm_update, lock
honouring, kind→step mapping, label, alias parsing.  Full
suite 1926 → 1938.

Files: new `src/audio/dsp/fx_djfilter.rs` (DSP),
`src/state/fx.rs` (3 fields + defaults), `src/state/fx_types.rs`
(FxStep + FX_STEP_COUNT bump 33→34), `src/state/fx_plan.rs`
(kind→step), `src/state/module_kind.rs` (variant + 6
exhaustive matches), `src/state/modulation.rs` (mod_inputs),
`src/state/rack.rs` (arrange order), `src/state/rack_random.rs`
(pool), `src/state/rack_scope.rs` (label/parse/match),
`src/state/llm_helpers_fx.rs` (apply), `src/llm/schema.rs`
(JSON schema), `src/audio/dsp/mod.rs` + `fx_step.rs`
(DspState wiring), `src/audio/dsp/params.rs` +
`params_from.rs` (AudioParams), `src/ui/rack_content.rs` +
`rack_content_fx_extras.rs` (dispatch + render),
`src/ui/module_card.rs` (title_fill),
`src/tests/coverage_tests.rs` + `fx_step_idx_tests.rs`
(exhaustive walks), new `src/tests/fx_djfilter_tests.rs`.

---

### FX rack pass — empty cards fixed + layout cleanup

User-driven cleanup of the FX strip after the absurd-queue
ship.  Two distinct issues, plus a layout pass.

**Dispatch bug — five cards rendered empty.**  Multitap,
RevDelay, TapeStop, Stutter, and Freeze all have full
renderers in `rack_content_fx_extras.rs` (rows of knobs +
optional XY pad), but the *dispatch* arm in
`rack_content.rs` listed only twelve of the seventeen FX
kinds the extras helper accepts.  The missing five fell
through to `_ => {}` and rendered nothing.  Fixed by
extending the dispatch list and adding a comment that
documents the lockstep requirement (the helper's accept
list at the top of `try_draw_fx_extras_content` and the
caller's dispatch arm must stay in sync).

**Full preset — 6 LFO modules → 4.**  `wire_default_cables`
only patches the first 4 LFOs into voice-modulation slots;
preset shipped with two orphans hanging off the strip.
`lfo_count: 6 → 4` in both `Full` and `Full + Viz` presets.

**Layout pass.**  `grid_size` adjustments plus glass-pane
grouping and a primary/secondary knob hierarchy.  Sized
through two rounds of user feedback:

- **CONV REV** — `(3, 3) → (4, 2) → (3, 3)`.  Six knobs
  (MIX / SIZE / PREDELAY / DAMP / LOWCUT / WIDTH) in a
  glass-grouped 2-row bank (3+3); IR picker on row 3 in
  its own glass pane with LOAD IR promoted to a 96×24
  primary button, REV toggle + × clear flanking it,
  filename label trailing.  Spell-outs: PREDLY → PREDELAY,
  LOCUT → LOWCUT.
- **FREQ SHIFT** — `(2, 2) → (2, 1)`.  SHIFT + FEEDBACK
  glass-grouped; MIX at default (medium) size beside it
  — primary visual emphasis belongs to the SHIFT/FEEDBACK
  pair, not the wet/dry.  FBK → FEEDBACK.
- **PITCH SHIFT** — glass pane around row 1 (SHIFT / MIX
  / FEEDBACK); FEEDBACK and FINE both promoted to
  φ-bigger primary knobs because they're the card's
  character controls.  FBK → FEEDBACK.
- **LIMITER** — glass pane around all four knobs;
  THRESHOLD + CEILING φ-bigger (audio-shaping primaries),
  RELEASE + LOOKAHEAD at default (medium) size.
  Spell-outs: THRESH → THRESHOLD, CEIL → CEILING,
  REL → RELEASE, LOOK → LOOKAHEAD.
- **MULTITAP** — all 4 knobs (TIME / SPREAD / FEEDBACK /
  MIX) φ-bigger; performance/dub FX where every parameter
  is in active play, all share the same visual weight.
  FBK → FEEDBACK.
- **REV DELAY** — TIME + FEEDBACK φ-bigger (the delay's
  whole identity comes from how those interact); MIX at
  default size as the wet/dry trim.  FBK → FEEDBACK.
- **STUTTER** — all 3 knobs (RATE / SLICE / MIX) φ-bigger;
  hands-on performance chrome.
- **VOCODER** — `(3, 2) → (2, 1)`.  All four knobs (BANDS
  / CARRIER / SENSE / MIX) in one row.  CRR.MX → CARRIER.

Files touched: `state/module_kind.rs` (grid_size for
ConvReverb / FreqShift / Vocoder), `state/rack_presets.rs`
(LFO count), `ui/rack_content.rs` (dispatch fix +
ConvReverb repack), `ui/rack_content_fx_extras.rs`
(FreqShift / Limiter / Vocoder repack),
`ui/rack_content_pitch_shift.rs` (PitchShift repack).
1926 tests still passing.

---

### SampleInstrument polyphony meter

Live readout of the SampleInstrument poly pool — eight small
dots above the panel header, lit gray (`FOG`) when a slot is
active and dim (`IRON`) when free.  Surfaces the
voice-stealing path: users see how close they are to the cap
before they hit it, so chord stabs / release tails can be
dialled without surprise.

The voice already tracked active-slot count (gated to
`#[cfg(test)]` for the polyphony unit tests); the change
exposes `SampleInstrumentVoice::active_voice_count()` as a
plain method, adds `DspState::sample_instrument_active() ->
u8` for the audio thread, and pushes the count once per
callback into a shared `Arc<AtomicU8>`.  No locks, no rtrb
drain — single Relaxed store / Relaxed load is cheaper than a
ring buffer the UI would have to drain to find the latest
value.

The meter dot count is hard-coded at `POLY_DOTS = 8` in the
viz module and asserted against `SampleInstrumentVoice::POLY_VOICES`
by `poly_dots_matches_voice_pool` so the two stay in sync.
Tests: 2 new (initial-state zero count + the dot/pool
equivalence), full suite 1926.

Files: `src/audio/dsp/sample_instrument.rs` (un-gate
`active_voice_count`), `src/audio/dsp/mod.rs` (delegate
method), `src/audio/mod.rs` (atomic + per-callback store),
`src/main.rs` + `src/ui/mod.rs` (channel plumbing),
`src/ui/panels/sample_instrument.rs` (right-aligned in the
header row), `src/ui/panels/sample_instrument_viz.rs`
(`draw_poly_meter`).

---

### MIDI granuliser (absurd queue #8)

Granular but for triggers — scatter an existing sequencer
pattern with density / repeat / pitch-jitter knobs.  V1 ships
as an in-place transformation on the running session's
pattern (not a file-to-file converter) so the user sees the
result immediately.  File-to-file MIDI input/output deferred —
the live-session path was the actually-useful version of the
brief.

DSP / transformation (`sequencer::granuliser`):
* `granulise_tb303(&mut [TB303Step], opts)` — pure function over
  any TB303-style melodic pattern (bass / hoover / an1x /
  pluck / wavetable / sample all use `Vec<TB303Step>`).
* Per-step pipeline: density gate → pitch jitter → optional
  repeat into the next slot.
* Density 0..1 — drop probability per active step.  1.0 is
  pass-through; 0 silences the pattern.
* Repeat chance 0..1 — if the next slot is empty, half-accent
  clone of the current step lands there.  Skips already-active
  next slots so the user's existing notes survive.
* Pitch jitter 0..12 — random ±N semitones added to the kept
  step's note, clamped to MIDI 0..127.
* Tiny LCG seed — deterministic per `(pattern, opts)`, so an
  interesting roll can be replayed via `?seed=N`.
* No allocations.  No audio-thread touches — the transform
  runs on the UI / API thread, the audio thread reads the
  rewritten pattern on its next bar.

API: `POST /api/midi/granulise {"voice": "bass", "voice_index":
0, "density": 0.7, "repeat_chance": 0.2, "pitch_jitter_st": 5,
"seed": 42}`.  All fields except `voice` are optional with
sensible pass-through defaults; nanos seed when omitted.
Documented in CLAUDE.md.

7 new tests cover: density 1.0 pass-through, density 0.0 wipe,
density 0.5 statistical ratio across seeds, pitch jitter stays
in MIDI range under ±12 st, repeat populates empty next slots,
repeat doesn't overwrite existing active slots, deterministic
output for the same seed.  **1924 tests passing**; clippy clean.

Why "scatter the sequencer pattern" rather than "load a MIDI
file": the brief's framing ("MIDI granuliser") describes the
conceptual model (granularise trigger events).  Applying it to
the running pattern is the most concrete realisation — the user
hears the change on the next bar without round-tripping through
a file.  A file-to-file API can layer on top later if needed.

### Bird-song corpus (absurd queue #7)

Per the brief — "small CC0 corpus, granularised and pitch-mappable.
Pairs with samples/textures/" — V1 ships as a **curated source
pointer + dedicated corpus folder**, played through the existing
GRAN granular voice.  A dedicated `BIRD` module isn't justified:
GRAN already does the heavy lifting (grain extraction, pitch
scatter, density), so a new module would be GRAN with a different
default folder.  Curating the corpus location is the actual win.

What shipped:
* New `samples/birds/` directory (with `.gitkeep` so it survives
  fresh clones, ignored otherwise via the existing
  `samples/**/*.wav` rule).
* `download-samples.sh birds` (+ `.bat` mirror) — printer
  sub-command listing curated CC-licensed sources:
  - `https://xeno-canto.org` — community-curated bird-call
    archive
  - `https://archive.org/details/birdsong` — public-domain
    field recordings
  - `https://freesound.org` — search for: birds, calls, tweet,
    dawn
* Workflow doc: drop a clean clip (3–30 s) into the dir, load
  via the GRAN card's LOAD button, set DENSITY high +
  PITCH-SCATTER moderate for chirpy / chorus textures.

If demand surfaces for note-tracking pitch on bird grains
(triggered playback at a specific note), a follow-up could add
a `pitch_mappable` flag on GRAN — but that's a separate request
from "ship a bird corpus" and gets queued separately.

### Vinyl / cassette FX (absurd queue #6)

Steady-state analog-tape character — surface noise + dull EQ
shape — as a chain FX module.  Distinct from `FxTapeStop` (which
covers the brake transient); this one focuses on the
*continuous* spectral signature of analog playback.

DSP (`audio::dsp::fx_vinyl::VinylFx`):
* **Surface noise** — band-limited white noise from a per-FX LCG
  (no `rand` crate dep, no allocations).  Knob 0..1 → 0 to ≈
  -20 dBFS noise floor.
* **EQ shape** — fixed low-shelf boost (~+3 dB at 220 Hz) for
  warmth, plus a wear-driven high-shelf cut.  Wear knob 0..1
  sweeps the cutoff from 6 kHz / 0 dB (transparent) down to
  1 kHz / -10 dB (heavily dulled).  Coefficients only recompute
  when the knob actually moves — single Biquad refresh, not
  per-sample.
* **Mix** wet/dry blend.  At 0.0 the path is bypassed (early
  return — no Biquad work).

Plumbing:
* `ModuleKind::FxVinyl` joins the FX palette with full metadata
  (label `VINYL`, grid 2×1, FxMod zone, supports XY pad,
  `allows_multiple` true).
* New `FxStep::Vinyl` (idx 32) — `FX_STEP_COUNT` bumped 32 → 33.
  The `kind_to_fx_step` map gets a Vinyl arm so cables route
  through the standard `apply_fx_chain` path.
* `FxState.vinyl_noise / vinyl_wear / vinyl_mix` (defaults
  0.5 / 0.3 / 0.0 — bypass-on-load), `#[serde(default)]`.
* `AudioParams.vinyl_*` plumbed via `params_from`.
* UI: shipped through the existing `rack_content_fx_extras` Tier-1
  helper — NOISE / WEAR / MIX knobs in the standard glass row,
  XY-pad expansion supported.
* Added to the `random_layout` FX pool — Vinyl always makes
  sense as a random pick (steady-state character, no setup
  needed).

4 new DSP tests cover bypass-when-mix-zero, noise-floor-on-
silent-input, near-unity-passthrough at zero noise/wear, and
output stays bounded under full noise + wear + signal.  Two
existing test lists (`fx_step_idx_tests` + `coverage_tests`'s
exhaustive enum walk) updated for the new variant.  **1917
tests passing**; clippy clean.

Start / stop transient deferred — `FxTapeStop` already covers
the brake effect; building it again here would be redundant.
Vinyl focuses on the steady-state character.

### Pendulum voice (absurd queue #5)

Two near-tuned sine oscillators that beat acoustically.  As
`detune_hz` drifts from 0 the sound moves from chord (< 1 Hz
beat, inaudible) through tremolo (1–10 Hz, the iconic
Riley-style pulsing) to inharmonic drone (10+ Hz where the
difference becomes its own subaudible tone).  No sequencer
trigger — drone voice driven by knobs, same shape as the
Theremin.

DSP (`audio::dsp::pendulum`):
* Two sine oscillators with independent phase accumulators —
  the constructive / destructive interference between them
  *is* the beat, no separate amplitude modulator.
* Base pitch knob log-mapped 30–800 Hz (~4.7 octaves) — lower
  than the Theremin's range because the beating effect is most
  musical in the low-mid register where the difference tone
  lands inside the audible band.
* Detune knob 0..1 → 0–60 Hz separation.  At 0 the two are in
  phase (no beat); above ~10 Hz the difference becomes its own
  tone.
* Mix knob 0..1 → osc1 / osc2 balance; 0.5 (default) gives the
  deepest beat amplitude modulation.
* Pure additive — no allocations, peak amplitude bounded by
  ±0.5 even at full mix.

State + plumbing:
* `state::pendulum::PendulumState` (enabled / base_pitch /
  detune_hz / mix / volume / pan), `#[serde(default)]`.
* `ModuleKind::Pendulum` with full metadata (label PENDULUM,
  grid 3×2, `Zone::Voice`, `has_audio_output()` true).  All
  exhaustive matches updated.
* `wire_default_cables` adds Pendulum to the `audio_only_voices`
  list (no CV-in jack — knob-driven).

UI panel:
* Top row: ON/OFF + live beat-rate readout (`BEAT 3.0 Hz`)
  computed from the same `beat_rate_hz(detune_knob)` helper
  the DSP uses, so what you see is what you hear.
* Bottom row: PITCH / DETUNE / MIX / VOL / PAN knobs.

4 new tests cover disabled-is-silent, audible-when-enabled
with bounded amplitude, no-detune collapse stays in range,
and `beat_rate_hz` matches the DSP's 0..60 Hz scale (defends
against the readout lying about the actual beat rate).
**1913 tests passing**; clippy clean.

Excluded from `random_layout` pool — same reasoning as
Theremin: a knob-driven drone with no hand on the knobs is
silent, doesn't make sense as a random pick.

### AI patch morph (absurd queue #4)

Schedule a sequence of LLM "nudge" prompts that evolve the patch
along a textual prompt across N bars.  POST one prompt + a bar
count; the scheduler fires one LLM call per bar (configurable),
each tagged with progress context ("step 3 of 8") so the model
knows where in the arc it is.  Builds on the existing
`apply_llm_update` + `locked_params` machinery — every nudge
honours the user's locked params, so a morph can't trample
on knobs the user is actively touching.

State + plumbing:
* New `state::patch_morph::PatchMorphState` (active / prompt /
  total_calls / calls_done / start_global_step /
  last_step_fired / step_interval).  `#[serde(skip)]` on
  AppState — morph progress is ephemeral, reloading a session
  shouldn't resurrect a half-finished arc.  Distinct file from
  the existing `morph::ChainMorph` (pattern crossfade), so the
  two evolve independently.
* `next_nudge_prompt()` formats the user prompt with a "step N
  of M" header.
* `compute_step_interval(bars, step_division, total_calls)`
  is the pure mapping bar-count → `global_step_count` ticks.
  Defends against zero / overflow inputs.

Scheduler:
* `ui::patch_morph_handler::tick_patch_morph` runs once per UI
  tick alongside `tick_link_sync`.  Polls
  `state.global_step_count`; when it crosses
  `last_step_fired + step_interval`, sends the next nudge via
  the shared `send_llm_infer` helper (same path the LLM strip
  uses, so the morph nudge gets identical apply_llm_update
  treatment).  Decrements `calls_done`; deactivates when the
  total is reached.
* First nudge fires immediately on the next tick by seeding
  `last_step_fired = now - step_interval` at start time — no
  one-bar wait before the morph begins to act.

API:
* `POST /api/morph {"prompt": "...", "bars": 8, "calls": 8}` —
  `calls` defaults to `bars` (one nudge per bar), soft-capped
  at `bars * 4` to avoid LLM flood.  Documented in CLAUDE.md.

8 new tests: 3 over `PatchMorphState` (in_progress predicate,
nudge prompt format, divide-by-zero defense) + 5 over
`compute_step_interval` (default-grid one-bar-per-call, 2-bar
spacing, 8th-note grid, pathological inputs, more-calls-than-
steps).  1909 tests passing; clippy clean on stub +
--features link.

Dedicated UI dialog deferred — for V1 the morph is API-driven;
a panel-style dialog can come later if desired.

### Mellotron mode on SAMPLER+ (absurd queue #3)

Per the brief — "uses the SampleInstrument scaffolding" — V1
ships as a flag on the existing voice rather than a brand-new
module.  Toggle `MELLO` on the SAMPLER+ panel and the slot's
playback gains tape-loop character: per-note pitch flutter,
brief spin-up transient on attack, and gentle tanh saturation
on the output.

DSP additions (all gated by `sample_mellotron_mode`):
* **Per-slot flutter LFO** — triangle wave at 1–3 Hz with
  per-trigger random phase + rate jitter, so two simultaneous
  notes don't wobble in lockstep.  Modulates the read rate
  directly (no spectral processor needed) for ±0–40 cents of
  warble; the depth knob `mellotron_flutter` scales it 0..1.
* **Spin-up transient** — 80 ms exponential ramp on attack
  pulls the read rate from 0.94 to 1.0, so each note starts
  ~half a semitone flat and rises to nominal pitch.  Iconic
  "motor coming up to speed" gesture.
* **Tape saturation** — gentle tanh shaping (`tanh(x*1.4)*0.85`)
  compresses transients and adds even-order warmth; bounded so
  the path stays in `[-1, 1]`.
* Per-trigger init randomises `flutter_phase` and
  `flutter_rate_hz` from the slot's age counter — no per-process
  RNG state, no allocations.

Plumbing:
* `SampleInstrumentState.mellotron_mode` (bool) +
  `mellotron_flutter` (f32, 0..1, default 0.4 — pleasant
  warble), both `#[serde(default)]`.
* `AudioParams.sample_mellotron_mode` + `sample_mellotron_flutter`
  with the same clamp.
* LLM API: `sample.mellotron_mode` (boolean) +
  `sample.mellotron_flutter` (number, 0..1) added to the JSON
  schema.
* UI: 38-px `MELLO` toggle next to the FRMT button.

2 new tests cover output stays finite + bounded with full
flutter, and mellotron_mode = false is bit-identical to the
default path.  **1901 tests passing**; clippy clean.

A separate `ModuleKind::MellotronVoice` could come later if
discoverability matters; for V1 the flag-on-SAMPLER+ approach
keeps the rack uncluttered and avoids duplicating the SFZ /
polyphony / loop machinery.

### Theremin voice (absurd queue #2)

XY-pad-driven continuous-pitch oscillator with portamento glide.
Drag the pad: X is log-mapped to pitch (50–2000 Hz, ~5 octaves —
matches a real Theremin's antenna range), Y is the gain follower
(bottom = silent, top = loud).  Portamento smoothing on the
pitch target gives the defining glissando gesture; gain has its
own short fixed-tau smoother so quick volume articulations stay
crisp regardless of the portamento setting.

- New `state::theremin::ThereminState` (enabled / x / y /
  portamento / brightness / volume / pan), wired into
  `AppState` with `#[serde(default)]` so older sessions pick it
  up.
- `audio::dsp::theremin::ThereminVoice` — single sine + 3rd /
  5th odd harmonics scaled by the brightness knob (the "talking"
  overtones that approximate a real Theremin's heterodyne
  squeal at high volumes).  Pure additive — no allocations in
  process.  Auto-clamped target gain + smoothed pitch for
  click-free pad drags.
- `ModuleKind::Theremin` joins the voice palette with full
  metadata (label `THEREMIN`, grid 3×3, `Zone::Voice`,
  `has_audio_output()` true).
- `wire_default_cables` extended with an "audio-only voices"
  list — Theremin has no CV-in jack (it's pad-played, not
  sequencer-played), so it skips the seq → CV cable but still
  gets a master-audio cable so the card isn't silent the moment
  it's added.
- UI panel: ON/OFF toggle, big XY pad (square, dominates the
  card), and a glass row of PORTA / BRIGHT / VOL / PAN knobs.
  Pad-drag writes through to state and pushes audio params
  immediately.
- Excluded from the `random_layout` pool (same reasoning as
  `SampleInstrument` and `NeuTts` — a pad-played voice with no
  hand on the pad is silent, so it doesn't make sense as a
  random pick).
- 4 new tests cover: silent when disabled, silent at y=0,
  pitch convergence with short portamento, audible output with
  reasonable params + bounded amplitude.  Total: **1899 tests
  passing**.

### Eurorack patch generator (absurd queue #1)

Creative-seed tool — wipe the rack, drop in a random selection of
voices, FX, and LFOs, wire defaults so the result makes sound
immediately.  "Show me what could happen" button for sketching
new ideas.

- Pure `state::rack_random::random_layout(seed) -> RandomLayout`
  picks 2..=4 voices + 3..=7 FX + 1..=3 LFOs from curated pools.
  Deterministic per seed (tiny LCG, no `rand` crate dep) so an
  interesting roll can be replayed via
  `POST /api/rack/random {"seed": 42}`.  Voices that need
  user-loaded assets (`SampleInstrument`, `NeuTts`) stay out of
  the pool — random picks should always make sound, not require
  setup.
- `apply_random_layout(state, seed)` is the shared helper:
  wipes the rack to its persistent core (sequencer + master +
  console), drops the layout's modules, runs
  `wire_default_cables` + `arrange_canonical`.  Same code path
  for the API endpoint and the UI menu entry — bit-identical
  results for the same seed.
- UI: `Edit → Random Patch (Eurorack)` menu item.  Nanosecond
  seed → fresh roll on every click.
- API: `POST /api/rack/random {"seed": <u64>}` (seed optional —
  nanos by default).
- 9 new tests cover: determinism per seed, divergence across
  seeds, count ranges, distinct picks within a layout, pool
  membership, pool-cap behaviour, LCG mixing on seed=0, and the
  full apply round-trip (core preserved, every voice reaches
  master, repeatable across two AppStates).  Total: **1895 tests
  passing**.

### SampleInstrument time-stretch — continuous knob (UI follow-up)

The previous commit shipped time-stretch as a 4-preset cycle
button (1.0 / 0.5 / 0.75 / 2.0×).  This follow-up replaces it
with a continuous bipolar knob so the user can dial in arbitrary
multipliers without going through the LLM / API.

- Bipolar `param_control_bipolar` (range -1..+1) sits at knob
  centre = 1.0× rest; drag right makes playback faster, left
  slower.  Maps logarithmically: each ±0.5 of bipolar travel
  doubles or halves the multiplier (bipolar ±1 → 4.0× / 0.25×).
  The doubling-per-half-knob symmetry matches musicians' ear for
  octave relationships, so the control feels uniform across the
  range.
- Two pure helpers in `panels/sample_instrument.rs`:
  `bipolar_to_time_stretch` and its inverse
  `time_stretch_to_bipolar`.  Both clamp to safe ranges
  defensively (zero from a bad file load doesn't underflow log2;
  out-of-band bipolar values get clamped before exp).
- DSP path unchanged — knob writes through to
  `state.sample_instrument.time_stretch`, the spectral processor
  auto-engages when the value drifts off 1.0× by more than 0.001.
- 6 new tests cover: centre = unity, ±1 endpoints, octave
  symmetry at ±0.5, defensive clamping at out-of-range bipolar
  inputs, round-trip across a representative bipolar grid, and
  pathological `time_stretch` inputs (0 / 100×) clamping to the
  legal range.  Total: **1886 tests passing**.

### SampleInstrument time-stretch decoupled from pitch

V2 Stage 8 shipped formant-preserving pitch shift; this follow-up
adds the second axis of the same FFT scaffolding — playback speed
as a separate dimension from pitch.  Drop a sustained loop in
SAMPLER+, hit the 0.5× button on the front panel, and the loop
plays back at half tempo with the played note's pitch unchanged.

- New `SampleInstrumentState.time_stretch: f32` (default 1.0,
  range 0.25–4.0, persisted via `#[serde(default)]` so older
  sessions pick it up at default).
- Plumbed through `AudioParams.sample_time_stretch` and the
  `params_from` snapshot with the same clamp.
- DSP path in `process_slot`: when `time_stretch != 1.0`, the
  read-rate becomes `time_stretch` (not 1.0) and the
  `FormantShifter` ratio becomes `pitch_ratio / time_stretch` so
  the spectral shifter compensates for the read-rate's pitch
  change — net output pitch lands on the played note while
  duration scales by `1/time_stretch`.  Auto-engages the spectral
  processor when active so the cheap linear-resample path doesn't
  need a separate flip.
- UI: a 38-pixel cycle button next to FRMT, labelled with the
  current ratio (e.g. `0.50×`, dimmed when 1.0×).  Click cycles
  through 1.0 → 0.5 → 0.75 → 2.0; off-grid values from the LLM /
  API land on the next-greater preset in sorted order, then wrap.
- LLM API: `sample.time_stretch` (number, 0.25–4.0) added to the
  JSON schema with a description targeting the "sustained loop at
  a different tempo without retuning" use case.  `sample.formant_preserve`
  was also missing from the schema — added at the same time.
- 2 new tests over `next_time_stretch` cover the cycle order and
  the off-grid sorted-advance.  Total: **1882 tests passing**.
- Continuous-knob version (logarithmic 0.25–4.0) deferred until
  demand surfaces; the 4-preset cycle covers the original PLAN's
  "half / double speed" cases.

### LfoScope cable-driven LFO selection (V2)

The `LfoScope` rack module picked the *first enabled LFO slot* in
V1 — useful but ambiguous when multiple LFOs were running.  V2
makes selection explicit by following the rack cable graph: patch
a CV cable from any `LfoModule`'s CV-out to the scope's CV-in
jack and the scope renders that module's slot.

- `module_card_mod::has_cv_in` now returns `true` for `LfoScope`,
  so the back panel renders a "CV / Gate In" jack the user can
  drag a cable to.  Existing voice CV-in semantics untouched.
- New pure helper `viz::lfo_slot_from_cables(state, scope_id)`
  walks `state.rack.cables` for an incoming CV cable from any
  `LfoModule` and returns that source's slot index.  Slot index
  is the source's positional rank among `LfoModule` instances in
  rack order — same rule the rack canvas uses to publish the
  back-panel "LFO 1/2/3" label, so the two stay consistent.
- `draw_lfo_scope` calls the helper before falling back to the V1
  "first enabled" picker, so unwired scopes (and older sessions
  saved before this landed) keep displaying their LFO with no
  user-visible change.
- 5 new tests cover: no cable → fallback, single LFO patched,
  positional-rank slot resolution for the second LFO, non-LFO
  source ignored (StepSequencer's CV out doesn't accidentally
  steer the scope), and cable-insertion-order tie-break when
  multiple cables land on the same scope.  Total: **1880 tests
  passing**.

### Ableton Link bar-phase alignment (V2)

Tempo sync shipped in V1; V2 adds the bar-phase snap follow-up
flagged in PLAN.md.  When the user toggles Link on, the sequencer's
`current_step` is realigned to the network's current bar position
so our 16-step pattern starts from the correct sub-beat instead of
wherever it happened to be sitting.

- `LinkSync::pull_phase(quantum)` — wraps `rusty_link`'s
  `phase_at_time(clock_micros, quantum)`.  Returns `Some(beats)` in
  `[0, quantum)` when Link is enabled, `None` on the stub build or
  when disabled.  Mirrors the `pull_tempo` shape so callers don't
  need to gate on the cargo feature.
- `AudioCommand::SnapClock { step }` — new lock-free command from
  the UI thread.  The audio callback resets `clock.current_step`
  and `clock.sample_accumulator` on receipt, so the next block
  starts fresh on the snapped step instead of carrying over a
  partial sample count.
- `tick_link_sync` detects the off→on edge by comparing
  `was_enabled` against the user's `link_enabled` pref before
  calling `enable()`, then routes through `snap_clock_to_link_phase`
  when the transition fires.  Pure mapping
  `link_phase_to_step(phase, step_division)` — `phase * step_division`
  modulo bar length — is a free function so the math is testable
  without the audio thread or a Link session.
- 7 new tests (5 over the phase→step mapping covering the 16th /
  8th / 32nd grids, pathological phase wrapping, and zero
  `step_division`; 2 over `pull_phase` returning `None` when
  disabled or on the stub build).  Total: **1875 tests passing**.
- Continuous drift correction within a session (re-snap when the
  cumulative phase delta crosses a tolerance) is intentionally
  deferred — V2 stays one-shot to keep the UX surprise contained.

### `Full` rack preset audit + new `Full + Viz` showcase

The `Full` rack preset (`src/state/rack_presets.rs`) was authored
before the V2 module sprint and was missing every voice / FX added
since.  Caught up:

- **Voices** — added `PluckString`, `WavetableVoice`,
  `SampleInstrument` so all 13 voice modules are present.
- **FX** — added the 19 missing FX (`FxLimiter`, `FxFilter`,
  `FxComb`, `FxTilt`, `FxTransient`, `FxExciter`, `FxMultitap`,
  `FxRevDelay`, `FxTapeStop`, `FxStutter`, `FxFreeze`,
  `FxConvReverb`, `FxParamEq`, `FxPitchShift`, `FxFreqShift`,
  `FxWiden`, `FxGate`, `FxVocoder`, `FxPan`) and reordered the FX
  list by family (distortion → filter → modulation → time-domain
  → pitch → glitch → dynamics → stereo) so neighbouring cards on
  the rack share a sonic role.
- **LFO count** — bumped 4 → 6, since the larger FX pool justifies
  more modulation sources.
- **New `Full + Viz` preset** — a curated showcase that extends
  `Full` with one representative analysis module per family
  (`StereoMeter`, `SpectrumAnalyzer`, `BarOscilloscope`,
  `EventStream`, `LoudnessMeter`, `PhaseWheel`).  Kept separate
  from `Full` because the 12 viz modules together would clutter
  the rack and most overlap functionally; users who want the full
  diagnostic gallery now have a one-click path without burying the
  audio path in `Full`.
- **`wire_default_cables` fix** — the V2 voices weren't in the
  default-wiring voice list, so even when added to a preset they'd
  appear silent on the rack (no sequencer-CV trigger, no master
  audio cable).  Added them so the cards actually make sound out
  of the box.
- **Reachability test refactor** —
  `tests::rack_reach_tests::reaches_master_default_preset_voices_all_reach`
  was hand-enumerating FX kinds to exclude from "must reach
  master" assertions, which silently became incomplete every time
  a new FX module shipped.  Replaced with `default_zone() ==
  Zone::Voice`, the canonical answer to "is this a voice?" used
  elsewhere in the rack code.  Adding a new FX no longer requires
  touching this test.
- 1868 tests still passing.

### Sample-pack download helper

`scripts/download-samples.sh` (+ `download-samples.bat` on Windows)
mirrors the `download-models.sh` UX as a single umbrella entry point
for fetching CC-licensed audio packs.

- **Automated** (drops into `samples/instruments/<pack>/`):
  - `salamander` — Salamander Grand Piano V3, CC-BY 3.0 (~730 MB)
  - `sso` — Sonatina Symphonic Orchestra, free-use (~1.3 GB)
  - `vsco2` — VSCO 2 Community Edition, CC0 (~2.3 GB)
  - `instruments-all` — runs all three in sequence (~4.4 GB total)
  Uses `git clone --depth 1` when git is available; otherwise falls
  back to the GitHub zipball (`/archive/refs/heads/master.zip`) via
  curl/wget + unzip on Linux/macOS, or the built-in `curl` + `tar`
  on Windows 10/11 — so end-user binaries without git installed
  still work.  Each pack prompts before downloading and skips
  cleanly if the destination directory already exists.
- **Reference-only** (for libraries that don't ship a clean
  non-interactive archive — prints the curated source URLs from
  `samples/README.md` plus the **absolute** install path so the
  user can't get confused about where files belong):
  `amen`, `textures`, `wavetables`, `impulses`.
- After cloning, the SAMPLER+ card's LOAD button can navigate into
  the pack subfolder to pick a `.sfz`.  The `/api/sample
  {random:true}` picker only scans top-level `samples/instruments/`,
  so the file-dialog path is the canonical workflow for subfolder
  packs (a recursive scan is a future cleanup, not a blocker).

### Ableton Link bidirectional tempo sync

Optional `link` cargo feature (default off) pulls in `rusty_link`,
which wraps Ableton's official C++ Link library; default builds get
a no-op stub so users without `cmake` / a C++ toolchain stay
unblocked.  Build with `cargo build --features link`.

- `LinkSync` wrapper (`src/sync/link.rs`) exposes a narrow surface:
  `enable(bool)`, `pull_tempo(local_bpm) -> Option<f32>`, `push_tempo`,
  `num_peers`.  The `is_supported()` flag mirrors the cargo feature so
  the UI can render either the working toggle or an
  "Unavailable — rebuild with --features link" stub.
- Per-frame `ImpulseApp::tick_link_sync` (in `src/ui/link_handler.rs`)
  runs once per UI tick.  Pull: when network BPM differs from
  `state.sequencer.bpm` by > 0.01 BPM, write the network value into
  AppState + push audio params.  Push: when local BPM diverges from
  the last-pulled value by > 0.05 BPM, the user / LLM / MIDI clock
  changed it — broadcast our value to peers.  Tolerance bands
  prevent float-jitter feedback loops.
- UI: Preferences → Display gains an "ABLETON LINK" section with the
  network-sync toggle + peer-count readout.  `ui_prefs.link_enabled`
  persists across sessions so a Link-set user stays in sync after a
  relaunch.
- Verifiable with two impulse-instruct instances on the LAN, the
  free LinkHut tester from Ableton, or any other Link-aware app
  (Algoriddim djay, AUM, MOD devices, Live).
- Bar-phase alignment (snap our sequencer step counter to Link's
  quantum) is the planned follow-up — needs threading through the
  sequencer clock advance.
- 4 new tests over the LinkSync wrapper (constructs cleanly,
  pull-when-disabled returns None, peer count zero when disabled,
  is_supported matches the feature flag).  Total: **1868 tests
  passing**.

### SampleInstrument V2 — `.sfz` multisamples + polyphony + formant-preserving pitch

Nine-stage build-out of the V1 SAMPLER+ voice into a full pitched-
sample instrument that loads `.sfz` multisample banks (Salamander
Grand, Sonatina, VSCO 2 CE) alongside single `.wav`s.  The instrument
gains polyphony, multi-zone key mapping, velocity layers + round-
robin, a per-voice filter, an LFO routing surface, a zone-map / wave
visualizer strip, formant-preserving pitch shift, and curated
documentation for the starter pack.

**Stage 1 — SFZ parser** (`src/state/sfz.rs`).  Pure parser
`parse_sfz(text, base_dir) -> Vec<SfzRegion>`.  Handles `<global>` /
`<group>` / `<region>` headers with cascading opcode inheritance and
the PLAN-listed opcode subset: `sample`, `lokey`/`hikey`/
`pitch_keycenter`, `lovel`/`hivel`, `loop_mode`/`loop_start`/
`loop_end`, `volume`, `pan`, `seq_position`/`seq_length`,
`tune`, `transpose`, `ampeg_attack`/`decay`/`sustain`/`release`,
`cutoff`, `resonance`, `fil_type`.  Real-world quirks handled: spaces
in sample paths, Windows backslashes, `//` line + `/* */` block
comments, unknown opcodes / headers logged + skipped (partial-load
beats hard failure on malformed packs).  22 parser tests.

**Stage 2 — Load SFZ + single-zone playback**.  New audio command
`LoadSampleInstrumentSfz` carries pre-loaded `Vec<SfzRegionRuntime>`.
`audio/sfz_loader.rs` reads `.sfz`, parses, loads each referenced WAV
via `load_wav_to_44100` (de-duped via Arc cache), ships the runtime
list to the audio thread.  The voice's `load_sfz()` switches into
multisample mode; the trigger picks the first region whose
`lokey..=hikey` covers the played note, derives `root_freq` from
that region's `pitch_keycenter`, applies region `tune_cents` /
`transpose` / `volume_db` per trigger.  Single-WAV mode preserved
exactly (V1.1 behaviour) when no regions loaded.  UI's LOAD button
+ `/api/sample` path-poll both sniff extension and dispatch to the
right loader.

**Stage 3 — Polyphony + voice stealing**.  `SampleInstrumentVoice`
becomes an 8-slot pool (`POLY_VOICES = 8`).  Trigger picks the first
`Off` slot or steals the lowest-`age` slot when full; each slot
carries its own samples Arc, ADSR state, freq, region_gain.
`gate_off()` releases every gated slot — matches V1.1's monophonic
sequencer semantics, but release tails now overlap with the next
attack instead of being chopped.

**Stage 4 — Multi-zone overlap layering**.  `pick_regions(note)`
returns *every* region whose key range covers the note (was: first
match).  Overlapping SFZ regions (typical in orchestral patches that
layer close + room mics on the same key) all sound together on
parallel polyphony slots; non-overlapping mapping unchanged.

**Stage 5 — Velocity layers + round-robin**.  Trigger derives a
64..127 MIDI velocity from accent (0..1) and filters regions by
`lovel..=hivel`.  Regions with `seq_length > 0` only fire when
`(rr_counter % seq_length) + 1 == seq_position` (1-indexed per spec).
A single global RR counter keeps the audio thread allocation-free —
strict per-group counters would need a HashMap.

**Stage 6 — Per-voice filter + LFO routing**.  Each polyphony slot
carries an SVF (`fx_extras::Svf`, LP/BP/HP modes); cutoff /
resonance / mode / mix shared across slots, driven from
`SampleInstrumentState`.  Four new LfoTarget variants
(`SampleVolume`, `SamplePan`, `SamplePitch`, `SampleCutoff`) +
opcodes route LFO modulation to the voice.  Filter mix=0
shortcuts the SVF call so a bypassed filter is one cheap branch
per slot.

**Stage 7 — Zone-map + waveform visualizer**.  Bottom-strip viz on
the SAMPLER+ panel: SFZ mode shows a piano-keyboard zone map (each
region as a horizontal band across its `lokey..=hikey` range,
banded vertically by declaration order so close+room layered packs
render as parallel strips, with ticks at `pitch_keycenter`); single-
WAV mode shows a min/max waveform thumbnail with loop-window shading
+ bright stem lines at the loop boundaries.  UI-side
`sample_sfz_regions` + `sample_wave_cache` populated by the load
helper before the runtime list ships to the audio thread.

**Stage 8 — Formant-preserving phase vocoder**
(`src/audio/dsp/formant_shifter.rs`).  Per-slot phase-vocoder pitch
shifter: STFT (FFT 512, hop 128, 75 % Hann OLA), cepstral envelope
approximation via moving-average smoothed log-magnitude, whiten →
shift bins by ratio (with phase-vocoder coherence: track each input
bin's phase advance vs. its expected free-running advance, accumulate
ratio-scaled true freq advance into the synthesis phase) → re-multiply
by the *original* envelope so formants stay anchored while harmonics
move → ISTFT + Hann² OLA back into the output ring.  All FFT plans +
scratch buffers allocated once at construction; the realtime
`process()` is alloc-free.  ~12 KB state per slot, ~100 KB across
the 8-slot pool.  Engages when `sample_formant_preserve` is on; the
cheap linear-resample path stays the default.  UI toggle (`FRMT` /
`frmt`) lives in the FILTER row of the panel.

**Stage 9 — Starter pack scaffolding**.  `samples/instruments/` +
`samples/instruments/starter/` folders so the path-scan resolves on
a fresh checkout.  `samples/instruments/README.md` covers single-WAV
vs SFZ modes, the supported opcode subset, free CC-licensed pack
sources (Salamander Grand, Sonatina, VSCO 2 CE, sfzInstruments
collection), and a minimal authoring example.  `samples/README.md`
gains a "Pitched samples + multisamples" section pointing at the
new folder.

V1.1 (already shipped before this round): auto-detect-root via
`detect_pitch_hz` on load (≥ 0.5 confidence), full 4-stage ADSR,
loop start / end / `loop_enabled` toggle, sample lane in the
sequencer, `/api/sample` HTTP endpoint.

Tests: ~50 new across `sample_instrument_tests`, `sfz_parser_tests`,
`formant_shifter_tests` (defaults, ModuleKind metadata, single-WAV
DSP, ADSR release decay, loop-disabled one-shot silencing, SFZ
mode mode switch, region-by-note picking, out-of-range silencing,
volume_db scaling, overlap layering, velocity-layer filter,
round-robin cycle, polyphony overlap / stealing / gate-off all,
filter bypass + closed LP attenuation, LFO target routing, formant
shifter pass-through / no-divergence at ±octave / reset, viz
thumbnail builder, formant_preserve flag round-trip).

### Per-voice activity heatmap on EventStream

Optional bottom-strip overlay on the EventStream module showing per-
voice activity binned per sequencer step.  `MelodicLogEntry` gained
a `voice: MelodicVoice` enum (`Bass(u8)` / `An1x` / `Hoover`) so the
heatmap can split per source voice; drum hits already carried voice
identity.  Rows: BASS / AN1X / HOOV / KICK / SN / HAT / CLAP —
recent activity bright, fading with age.  Toggle via
`ui_prefs.stream_heatmap` (default off; Preferences → Display →
"Per-voice heatmap strip").  Render extracted to
`src/ui/widgets/event_stream_heatmap.rs` to keep `event_stream.rs`
under the LOC cap.  4 new tests.

### Master panel collapsed to a single row

`MasterOutput` card grew from a 2-row layout (master volume + voice
strip on top, M/S knobs below) into a single-row strip:
`MASTER VOL | MID G T S | SIDE G T S | voice-presence labels`.
Saves a grid cell of vertical space at the top of the rack without
dropping any controls.  Card grid_size went from `(grid_cols, 2)`
to `(grid_cols, 1)`.

### Frequency shifter (`FxFreqShift`) — single-sideband Hilbert pitch

Distinct from `FxPitchShift` (which preserves harmonic ratios):
`FxFreqShift` adds the same Hz to every spectral component, so
harmonics stop being integer multiples of the fundamental and the
timbre becomes inharmonic / metallic.  Classic radio-jamming, bell-
tine, and Sean-Costello-shimmer territory.

- Two parallel cascades of 4 second-order allpass sections produce
  the analytic-signal real / imaginary pair (`H(z) = (a + z⁻²) / (1 +
  a·z⁻²)` per section).  Hartmann-style coefficient pair (HBI
  flavour) gives ~1° phase error in 100 Hz – 20 kHz.
- Complex multiply with a `cos` / `sin` carrier at ±1000 Hz produces
  the SSB-shifted output; sign of `shift_hz` picks direction (subtract
  imaginary projection for upshift, add for downshift).
- Feedback path: capped at 0.85 with a `tanh`-clamped tap so the
  regen loop stays bounded under sustained input even at max
  feedback + max mix.
- 11 new tests covering DSP stability, FxState round-trip, ModuleKind
  metadata, panel XY-pad fan-out.

### Stereo widener (`FxWiden`) + M/S mode on `FxParamEq`

Both implemented via the master-stage latch pattern (mirroring
`FxPan` / `FxConvReverb`'s side-latch idiom): the chain step is a
mono passthrough that flips a state flag with the live knob values;
the master stage applies the actual stereo math after the existing
mid/side decomposition.  Avoids converting the whole FX chain to
stereo I/O while still unblocking the deferred items.

- **`FxWiden`**: knobs HAAS (0..30 ms delay on the L channel's mid
  component), SIDE (1..3× scaling on the existing mid/side
  decomposition), MIX.  Master stage maintains a small ring on the
  L-channel mid for the Haas delay; side scaling multiplies the
  decoded side signal before L/R recombination.
- **`FxParamEq` M/S mode**: when `param_eq_ms_mode` is on, the
  chain's ParamEq step is a passthrough; the master stage runs two
  extra `ParamEq` cascades (`param_eq_mid` + `param_eq_side`) on
  the decoded mid + side channels, using the same band list.  UI:
  `MN`/`M/S` toggle on the ParamEq band-readout strip.
- 11 new tests across the two surfaces.

### Tier-2 sidechain trio — `FxGate`, `FxVocoder`, sidechain `FxCompressor`

New `PortKind::SidechainIn` (5th port kind) + `connect_sidechain()`
helper on `RackState`.  Sidechain edges are taps, not part of the
forward signal chain — `compile_fx_plan` records the source in
`FxPlan.sidechain_routes: HashMap<FxStep, SidechainSource>` (where
`SidechainSource = Voice(ModuleKind) | Fx(FxStep)`), and the audio
thread reads the source via the previous-sample voice / FX cache
(one-sample delay, so cycles are safe by construction — the cycle
check skips `to.kind == SidechainIn` cables).

- **`FxGate`** — knobs THR (-60..0 dBFS) / ATK (0.5..50 ms) / REL
  (10..500 ms) / DEPTH / MIX.  Detector envelope on the sidechain
  drives threshold-gated gain reduction; falls back to the main
  signal as detector when the sidechain port isn't connected (gate
  becomes a noise gate then).
- **`FxVocoder`** — 16-band channel vocoder (log-spaced 100 Hz →
  8 kHz, fixed Q ≈ 3); per-band envelope follower on the modulator
  drives gain on the matching carrier band.  Knobs: BANDS (active
  fraction), CRR.MX (dry-carrier blend for talkbox flavour), SENSE
  (detector gain), MIX.  Pairs with `NeuTts` for talkbox patches.
- **Sidechain mode on `FxCompressor`** — `compressor_sidechain`
  bool flag; new `process_with_detector` reads the level detector
  from the sidechain cable but applies gain reduction to the input.
  Falls back gracefully to self-detect when no cable is connected.
  Multiband sidechain not in V1 (the 3-band path keeps self-
  detecting when the flag is on).

Audio thread carries a `SidechainSnap` snapshot alongside
`VoiceSendsSnap` / feedback array, refreshed each sample after
voices process.  16 new tests covering the rack plumbing + each
FX's DSP behaviour.

### Tier-3 heavy FX — Multitap / RevDelay / TapeStop / Stutter / Freeze

- **`FxMultitap`** — 4 fixed taps with knob-controlled spread.
  Per-tap pan + filter from the original spec deferred — the simpler
  4-tap mono variant covers the rhythmic-dub use case.
- **`FxRevDelay`** — ping-pong segment buffer (one fills while the
  other plays back reversed).
- **`FxTapeStop`** — mix knob doubles as ramp progress (0=normal,
  1=halted) with a darkening lowpass that tracks the slowing tape.
- **`FxStutter`** — BPM-synced glitch repeater (1/4, 1/8, 1/16, 1/32
  by quartiles of the rate knob).
- **`FxFreeze`** — spectral freezer.  Captures one FFT frame on
  rising-edge engage; resynths with random phases per hop via
  overlap-add (1024 FFT, 256 hop, Hann window).  xorshift32 phase
  randomization keeps successive frames decorrelated.
- **Cabinet IR mode** — flag on `FxConvReverb` (`conv_reverb_cabinet:
  bool`).  When true, caps `conv_reverb_size` internally at 0.1
  (10 % of loaded IR) and the file picker browses
  `samples/cabinets/` instead of `samples/impulses/`.

### Tier-1 FX modules — Flanger / Limiter / Filter / Comb / Tilt / Transient / Exciter

Seven small, well-defined FX modules to fill out the FX strip:

- **`FxFlanger`** — short modulated delay with bipolar feedback.
  Knobs: rate (0.05–4 Hz LFO), depth (sweep range up to ~9 ms
  around 1 ms base), feedback (-0.95..+0.95 centred at 0.5),
  mix.  Stack-allocated ring buffer.
- **`FxLimiter`** — brick-wall limiter with threshold / ceiling /
  release / lookahead knobs.
- **`FxFilter`** — state-variable filter with LP/BP/HP/Notch via
  mode selector.  Fields named `svf_*` to avoid collision with the
  per-voice bass filter knobs.
- **`FxComb`** — Karplus-style feedback comb tuned to a pitch in
  Hz; knobs: pitch (40 Hz–2 kHz), feedback, damp (lowpass on
  feedback path), mix.
- **`FxTilt`** — broad tilt EQ with tilt + pivot knobs.
- **`FxTransient`** — transient designer with attack + sustain
  shapers.
- **`FxExciter`** — saturation-based aural exciter with HP corner
  + amount + mix.

All allocation-free in `process()`; ring buffers live on the heap
(boxed) when too large for the stack.

### Visualization modules — Tier 1 + Tier 2

Tier 1 (cheap, reuse existing buffers):

- **`StereoVectorscope`** — XY plot of L vs R from the engine's
  interleaved stereo buffer (reads `app.stereo_buf`).  Square card so
  the lissajous lobes aren't squashed into an oval.
- **`LfoScope`** — phosphor-style waveform trace of the LFO module
  output.  V1 picks the first enabled LFO slot; CV-cable wiring of
  slot ↔ module is a follow-up.
- **`PitchTracker`** — autocorrelation pitch detect with cents-off
  needle, big note-name readout.  Reuses `detect_pitch_hz` from
  `audio/analysis.rs`.
- **`ChordDisplay`** — chroma-vector folding of the existing spectrum,
  matched against 24 major+minor triad templates.  Reuses
  `compute_spectrum` so the module is essentially free DSP-wise.

Tier 2 (heavier, but bounded):

- **`Spectrogram`** — rolling FFT-history waterfall rendered as a
  fresh `egui::ColorImage` per repaint with log-frequency Y axis.
  Uses the existing spectrum cache.
- **`LoudnessMeter`** — K-weighted (BS.1770 hard-coded 48 kHz
  coefficients) momentary + short-term LUFS EMAs.  Integrated LUFS
  (gated) deferred — momentary + short-term covers the meter use
  case.
- **`PhaseWheel`** — circular bar/beat indicator with beat-tick
  highlights.  Reads sequencer state directly.

### Refactor + coverage pass — pure logic isolated from impure shells

Session-long refactor following `docs/coding-guide.md`'s "pure
functions for core logic" rule.  Lifted testable kernels out of
the audio thread + UI dispatch shells, split files near the
1000-line cap, and added 62 new unit tests.

Pure helpers extracted (each one with a dedicated test module):

- `state/chain_advance.rs` — `LoopBoundaryAction` enum +
  `classify_loop_boundary()` + `build_advance_target()`.  Replaced
  ~150 lines of inline decision tree in the audio thread; the
  thread is now a thin dispatcher over the action.  17 tests cover
  every classifier branch (empty / repeat / one-shot stop / loop
  wrap / morph passthrough / BPM + style override precedence /
  defaults / determinism).
- `sequencer::step_count_delta(prev, curr) -> u64` — polymeter-
  aware saturating diff.  Removed an inline wrap-fallback that had
  silently dropped one slot per MAX_STEPS for any voice whose
  length didn't divide it.  5 tests.
- `TriggerEvent::is_gate_off(&self) -> bool` — gate-off classifier
  for the StopAtEnd filter.  Extracting it caught a latent bug:
  Pluck + Wavetable gate-offs were being dropped by the inline
  filter.  Now they survive cleanly.  3 tests.
- `midi::midi_clock_tick_interval_samples(bpm, sr)` —
  `midi::is_valid_clock_interval(secs)` —
  `midi::clock_interval_to_bpm(avg)` — three small MIDI clock
  helpers.  Round-trip locked (142 BPM in → 142 BPM back through
  both halves of the conversion).  11 tests covering boundary
  values + divide-by-zero guard.
- `ConversationMode::from_str_lossy(s) -> Self` — case-insensitive
  parser, whitespace trim, unknown → Producer fallback.  3 tests.
- `state::agent_matches_broadcast_scope(agent, scope) -> bool` +
  `state::push_pending_hint(agent, hint)` + `HINT_QUEUE_MAX = 5` —
  scope matcher (case-insensitive scope-list OR persona-name
  fallback ONLY when scope is empty) plus the cap helper.  Locks
  the "scoped agents don't fall through to persona match"
  contract.  9 tests.
- `StyleCatalog::resolve_style_id(query) -> Option<String>` —
  exact id → ci id → ci display name → None.  Locks the "no
  whitespace trim" contract.  7 tests.
- `ui::util::zoom_step_from_delta(delta)` +
  `next_module_scale(cur, step)` + `next_global_scale(cur, step)`
  — three pure pieces of `detect_ctrl_zoom`.  Saturation at half
  a notch's worth of zoom; module range 0.5..=2.0 / global
  0.5..=3.0 (wider for chrome).  7 tests.

Structural splits (no behaviour change):

- `state/llm_helpers.rs` 995→607 by extracting `apply_fx_update`
  (~390 lines) to a new `state/llm_helpers_fx.rs`.
- `state/mod.rs` 981→793 by extracting `LlmAgentState` +
  `AgentRole` + the agent-related constants to a new
  `state/llm_agent_state.rs`.

Test count: 1667 → 1729 (+62 across 7 new test modules).  Every
commit clean clippy + fmt + 1000-line cap + pre-commit hooks.
Largest source files now 996 (sequencer.rs UI), 924 (rack.rs),
921 (llm/mod.rs) — all well under cap.

### MPE DSP integration — bend / pressure / timbre drive the bass voice

Closes the loop on the MPE parser wiring.  `AppState.mpe` values now
flow through `AudioParams::from_app_state` into the Bass303 voice's
per-block modulation:

- **Pitch bend** maps to ±2 semitones (GM standard) and is added
  to the running pitch via the existing `freq_mod` factor.  Voice
  0 only for V1 — MPE controllers default to driving the lower-
  zone master, which is the natural target.
- **Channel pressure** boosts the output amplitude by up to 1.4×
  (`1 + pressure * 0.4`) so a hard press makes notes pop without
  saturating into clipping.
- **Timbre (CC74)** lifts the filter cutoff additively up to 0.3
  of the cutoff range, capped by the existing `clamp(0.0, 1.0)`
  so a Y-axis push opens the filter without overriding the user's
  set cutoff.

The `AppState.mpe` snapshot stays in place — it's the source of
truth that `from_app_state` reads each block.  The DSP fields
(`mpe_bend_st`, `mpe_pressure`, `mpe_timbre` on `AudioParams`)
are pre-clamped + pre-scaled so the audio thread does no
additional bounds checking per sample.

- 10 new tests: 5 over the AudioParams snapshot (default zero,
  bend ±1 → ±2 semitones, out-of-range clamps, pressure / timbre
  pass-through with clamp), 5 over the modulation math
  (replicated as pure helpers — full bend factor, timbre lifts
  cutoff 30%, timbre clamps at unit ceiling, pressure boost,
  zero-pressure pass-through).  Full suite at 1667 tests passing.

### Lane-score auto-tuner + per-lane few-shot examples + style obs

Three intelligence improvements bundled because they all loop the
LLM back on its own past output:

- **Lane-score auto-tuner**: long-term per-`(style, lane)` running
  average score on `LlmState.lane_avg_per_style`.  After each
  successful pipeline lane apply the score from `lane_eval` is
  pushed to the matching average.  The jam scheduler's
  `pick_jam_lane` multiplies its weight by `auto_tuner_bias(avg, n)`,
  which is centred at 1.0 and bounded to `[0.7, 1.3]` so a poor
  early run can't permanently disable a lane.  Trust ramps over
  ~5 observations; lanes with no history use the unmodified
  baseline / style dynamism.
- **Per-lane few-shot examples**: drop a JSON array of
  `{prompt, output}` pairs at `examples/<lane_slug>.json` and the
  pipeline injects up to 5 of them into that lane's system prompt
  as concrete reference outputs.  Slug map: `settings`,
  `bass1..4`, `kit_a`, `kit_b`, `amen`, `hoover`, `an1x`, `fx`,
  `modulation`, `rack`.  Missing / malformed files are silently
  skipped — best-effort enrichment, never a hard dependency.
  Lets the user steer a lane's style without touching the system
  prompt at all.
- **Agent personality evolution** (already shipped, now noted):
  `style_observations` accumulate per agent (capped at
  `STYLE_OBS_MAX = 10`) and inject into `build_system_prompt_full`
  under "Learned user preferences", so long-running agents
  develop a feel for what the user likes.

- 25 new tests: 6 over the running average (default empty / single
  score / arithmetic mean / clamp on out-of-range), 7 over the
  bias formula (neutral cases / boost / reduce / dampened-trust /
  bounded-output sweep), 5 integration over the active-style
  lookup (no style / no history / boost / cross-style isolation /
  serde safety), 10 over few-shot (slug map per LaneKind / loader
  missing-file / well-formed / malformed-no-panic / render
  empty / contents / 5-cap).  Full suite at 1657 tests passing.

### Persona library — save / load named agent configurations

- Captures the user-curated subset of an agent (persona name,
  role, conversation mode, instructions, system-prompt override,
  temperature, thinking flag) as a `PersonaPreset` JSON file
  under `~/.impulse_instruct/personas/<slug>.json`.  Pattern
  state, scope, model_path, and runtime counters are deliberately
  excluded — those are session-context, not personality.
- Agent card now exposes a tiny `preset:` row with a "save"
  button (capture this agent's knobs into a preset file) and a
  "load…" combobox listing every preset on disk.  Save names
  the file via `slugify(persona_name)`; load applies the preset
  onto the agent without disturbing scope / patterns / model
  routing.
- Public API: `PersonaPreset` struct, `save_preset` /
  `load_preset_from_path` / `list_presets`, plus `_to_dir` /
  `_in` overrides for callers that need an explicit directory
  (used by tests to avoid mutating `HOME`).  `slugify(name)`
  is exposed too so external tooling can predict filenames.
- 16 new tests: 6 over slugify (case / whitespace / punctuation
  / empty / unicode / trailing strip), 6 over agent ↔ preset
  round-trip + apply non-destructiveness + temperature clamp +
  missing-field defaults + idempotence, 4 over the FS path
  (creates dir if missing, save→load round-trip, list ignores
  non-JSON, missing dir returns empty).  Full suite at 1632
  tests passing.

### Cross-agent broadcast hints + per-agent budget + sleep mode

Three agent-tooling improvements bundled together because they all
touch `LlmAgentState`:

- **`broadcast_hint` action**: extends the existing single-target
  `send_hint` with a scope-fan-out variant.  LLM emits
  `broadcast_hint: { "scope": "bass", "hint": "half-time for 8 bars" }`
  and every agent whose `scope` contains the label (case-
  insensitive) — or whose persona name matches the scope when the
  agent is global — gets the hint queued.  Empty scope is rejected
  to keep "broadcast to everyone" admin-only.
- **Per-agent token-budget tracking**: new `total_prompt_tokens` /
  `total_completion_tokens` / `completed_cycles` (transient) on
  `LlmAgentState`.  LLM worker bumps these each cycle via
  `saturating_add`.  Agent card surfaces a `tok N+M=T (avg X/cycle)`
  line so the user can see which agents dominate throughput.
- **Sleep mode**: new `sleeping: bool` field (persisted) — the jam-
  scheduler's heartbeat picker skips sleeping agents, alongside
  the existing rack-disabled gate.  Toggle from the agent card
  (💤 / wake button); persists across sessions so a parked
  specialist stays parked.

- 10 new tests over budget + sleep: defaults, serde round-trip
  (sleeping persists, token counters skip), saturating
  arithmetic over many cycles + u64::MAX clamp, the round-robin
  filter (awake-picked / sleeping-skipped / all-sleeping-empty /
  disabled-skipped-even-when-awake).  1 new test for the
  json_repair extractor (broadcast_hint requires both scope and
  hint, rejects empty scope).  Full suite at 1616 passing.

### Pattern snapshot slots A/B/C/D + grouped shortcut overlay

- **Snapshot slots**: Shift+1..=4 instantly load pattern bank slots
  0..=3 via `bank_load(state, slot, keep_transport=true)` so a live
  performer can flip between four banked patterns at any time
  without touching the mouse.  Right-click on the bank strip cells
  (or the existing bank-write API) is still the way to capture
  into a slot.  Just keyboard wiring — no new state, leverages
  the pre-existing pattern_bank pipeline.
- **Shortcut overlay refresh**: F1 / ? overlay now reads from a
  canonical `SHORTCUT_GROUPS` const grouped into Transport / View /
  Editing.  Single source of truth so future handlers register
  there once and the help can't drift out of sync with what the
  app responds to.  Added the four snapshot slots, F2 performance
  mode, and the existing entries.
- 6 tests over `SHORTCUT_GROUPS`: at-least-one-group, every-group-
  has-rows, every-row-has-non-empty-key-and-desc, no-duplicate-keys
  across groups, snapshot slots listed under Transport,
  performance mode listed under View.  Full suite at 1605 tests
  passing.

### MPE input — pitch bend / channel pressure / per-note CC74 wiring

- The `midir` parser used to handle Note On/Off + ControlChange +
  PitchBend + clock messages.  ChannelPressure (0xD0) was missing,
  and PitchBend events were dropped entirely by the UI handler.
  Now all three MPE expression axes flow into `AppState.mpe`:
  - PitchBend on any channel → `mpe.pitch_bend` (-1..=1)
  - ChannelPressure → `mpe.pressure` (0..=1)
  - CC74 on a non-zero (per-note) channel → `mpe.timbre` (0..=1)
- `AppState.mpe: MpeExpression` is transient (skip-serialised) so
  controllers can drive expression without polluting session JSON.
  Surfaced via `/api/state` + the WebSocket push so external
  patches / OSC bridges can react to MPE controllers immediately.
- New helpers in `crate::midi`: `is_mpe_note_channel(ch)` (true for
  any non-zero channel — heuristic that works for the standard
  lower-zone layout), `pressure_to_unit(value)` (7-bit → 0..1
  with high-bit masking).
- CC74 routing now SHORT-CIRCUITS the static `cc_to_param_path`
  table when the CC came from a per-note channel — otherwise an
  MPE controller would wrench bass.cutoff on every Y-axis wiggle.
- DSP integration (per-note pitch bend / pressure → accent /
  timbre → cutoff) is a documented follow-up; V1 ships the wiring
  + state surface so users can patch the missing pieces externally.
- 13 new tests: parse_midi for ChannelPressure decoding, PitchBend
  centre/min, NoteOn unaffected; classifier helpers (master vs.
  note channels, pressure 0/127/64/0xFF mapping); AppState.mpe
  default state + clone/eq derives.  Full suite at 1599 passing.

### REC→CHOP — record master bus into AmenSampler with auto slices

- Lets the user sample their own jam back into the break rotation.
  Click REC→CHOP on the amen panel: the shared master-output ring
  buffer is frozen, run through `detect_onsets`, and loaded into
  the AmenSampler with `slice_positions` set to the detected
  transients.  Per-slice pitch / volume / reverse overrides are
  cleared (the new break has its own dynamics).
- Source ring buffer is now drained centrally in `app_update.rs`
  rather than inside the granular panel — both granular CAPTURE
  and amen REC→CHOP read from the same up-to-date tap, so the
  amen button works whether or not the granular panel is visible.
- New helpers in `panels/amen.rs`: `linearise_tap(tap, head) ->
  Vec<f32>` re-orders the ring so slot 0 is the oldest sample;
  `record_chop_into_amen(app)` runs the full freeze-detect-load
  flow.  Wave thumbnail rebuilds against the captured buffer so
  the panel waveform display matches what's loaded.
- 10 new tests: linearise round-trip (empty / head=0 / head mid /
  head=len wraps / length preservation) and the onset detector's
  contract on a synthetic pulse-train break (slice-0 anchor,
  sorted unit-range, silent-buffer safe default, short-buffer
  default, max_slices cap).  Full suite at 1587 tests passing.

### Undo timeline scrubber — slider over the past/future stacks

- The Ctrl+Z / Ctrl+Shift+Z stack always existed but was step-by-
  step only.  Now there's an opt-in window with a slider over
  the linearised timeline (`past + current + future`) so users
  can A/B compare past states visually without mashing Ctrl-Z
  blind.  Drag the slider to walk the history; Undo / Redo
  buttons nudge by one.  A "Mid-history — any new edit clears
  the future entries" hint surfaces when the slider sits past
  the latest mutation.
- Toggle in the header view menu → "Undo Timeline".  Off by
  default; primary keyboard shortcuts stay the main path.
- New `StateHistory::scrub_to(target, current)` helper does
  step-at-a-time walking so past/future stacks remain coherent.
  Out-of-range targets clamp to the timeline ends; same-slot
  targets short-circuit.  `current_index` / `total_slots`
  expose the linearised view for the slider.
- 7 tests cover the empty-history baseline, push + undo index
  walking, scrub-to-current no-op, scrub-back / scrub-forward
  state restoration, out-of-range clamping, and the regression
  invariant that a fresh push after mid-history scrub clears
  the future stack.  Full suite at 1577 tests passing.

### Rack mini-map — bird's-eye navigator with click-to-pan

- New optional overlay anchored bottom-right of the rack canvas
  showing every module as a thumbnail rectangle plus a viewport
  indicator that highlights what's currently in view.  Click /
  drag inside the mini-map to scroll there — the clicked y becomes
  the centre of the viewport.
- Useful for tall racks (Crew preset + many FX cards stacked) where
  the scroll bar is otherwise uninformative.  Off by default; toggle
  in Preferences → Display → "Rack mini-map".
- New module `ui/rack_minimap.rs` (~165 lines) reads the cached
  `module_card_rects` published by the rack canvas + the
  ScrollAreaOutput state; pure helpers
  `card_to_content_space` / `content_y_to_map_y` /
  `map_y_to_content_y` keep the coordinate math out of the painter.
- 7 tests cover viewport-relative card mapping, x-axis isolation
  from scroll, proportional y mapping, out-of-range clamps, the
  zero-content edge case, and inverse round-trip / pointer-out-of-
  range clamping for click-to-pan.  Full suite at 1570 tests
  passing.

### WebSocket state push — `/api/ws/state`

- New `GET /api/ws/state` upgrades the connection to a WebSocket
  and streams `AppState` JSON whenever the engine state changes.
  Mirrors `GET /api/state` for clients that want to react instead
  of poll — external dashboards, live-coding editors, OBS overlays.
- Protocol (V1): on connect the server sends a full snapshot, then
  pushes the latest snapshot at most every 250 ms (4 Hz) but only
  when the bytes have changed since the last push.  A FNV-1a 64-bit
  rolling hash gates "did anything change"; full diff payloads are
  out of V1 scope (clients can diff client-side cheaply).  Inbound
  frames are ignored — use the existing POST endpoints for writes.
- Connection lifetime: one tokio task per client.  Any send error
  (client closed, network gone) drops the task — abandoned
  connections don't leak.
- 7 tests cover the hash helper: empty-input invariant, identity,
  one-byte sensitivity, byte-order sensitivity, large-buffer
  termination, and that BPM change → different hash on a real
  AppState (the unchanged-skip gate depends on this).  Full suite
  at 1563 passing.

### Performance mode — stripped chrome for live use

- New `UiPrefs.performance_mode` toggle persists across sessions.
  When on, the layout hides the menu bar, the log + event-stream
  + scope strip, and the piano keyboard — leaving only the header
  transport, rack canvas, and footer.  Demos / live sets can
  launch straight into it because the flag is in `session.json`.
- Toggle from three places: the footer's "PERF" indicator (always
  visible — clickable even when chrome is hidden), F2 keyboard
  shortcut, and the underlying preference field.  No menu entry
  yet because the menu is itself one of the things that hides.
- 4 tests cover default-off, serde round-trip, missing-field-from-
  old-session graceful upgrade, and a regression guard that
  toggling performance mode doesn't disturb sibling prefs.

### OSC API mirror — TouchOSC-driven control surface

- The pre-existing OSC listener handled `/impulse/<section>/<param>`
  param updates, `/impulse/sequencer/play|stop`, and
  `/impulse/prompt`.  Now it also mirrors the live-performance
  surface of the HTTP API, so a TouchOSC layout / Max patch /
  hardware controller can drive the same things HTTP can without
  speaking JSON-over-TCP.
- New OSC address routes:
  - `/impulse/lock <path>` — lock a param dot-path (mirrors
    `POST /api/lock`).
  - `/impulse/unlock <path>` — clear a lock.
  - `/impulse/scroll <target>` — set `scroll_target` so the UI
    animates to the named zone / module ("voice", "fxmod",
    "AcidBass", …).
  - `/impulse/style <id>` — set `llm.active_style` and propagate
    to all unlocked agents.  Empty string clears the style
    (lets a single TouchOSC text widget toggle on/off).
  - `/impulse/preset <name>` — accepted-and-logged stub for V1;
    full preset application is non-trivial sync code so we'd
    rather route through HTTP than fork the logic.
- `OscAction` enum + `parse_osc_addr` are now `pub(crate)` so the
  parser is unit-testable without standing up a UDP listener.
- 14 OSC tests cover address rejection (no-prefix, unknown
  sub-addr), play/stop no-args, prompt with string + reject when
  missing, lock/unlock/scroll/preset/style routing, the empty-
  string clears style contract, param-update routing for both
  Float and Int args (so int-sending hardware works for BPM),
  and unsupported arg-type rejection.  Full suite at 1552 passing.

### Automation lane overlay — LFO sparkline under the step grid

- New optional sparkline row under each enabled bass voice's step
  pads showing where the per-voice LFO sits at every step.  Lets
  the user see the modulator aligned to the beat grid instead of
  inferring it from the LFO knob's rate / depth values.
- Pure helper `bass_lfo_curve_for_view(synth, bpm, step_division,
  page_start, visible_steps) -> Vec<f32>` lives in
  `state/automation_overlay.rs` so the math is unit-testable
  without an egui context.  Returns one bipolar (-1..1) value per
  step, scaled by the LFO's depth.  Phase advance is computed from
  `lfo_bpm_sync` / `lfo_sync_beats` (synced) or `lfo_rate` knob
  (free) using a quartic 0.01..20 Hz mapping.
- Six waveforms supported (Sine, Triangle, Saw, InvSaw, Square);
  Sample-and-Hold paints flat in V1 because faking it without the
  DSP's noise source would diverge from playback.  Off / zero-
  depth lanes elide the paint entirely.
- Toggle in Preferences → Display → "Automation overlay (LFO
  sparkline)".  Off by default so the grid stays compact for
  users who don't use modulation.
- 15 new tests cover the rate-knob mapping (floor at 0.01 Hz,
  cap at 20 Hz, quartic shape), synced phase math at a 16th grid
  for one-bar / one-quarter cycles, free-running phase at 120 BPM,
  curve sampling (off / zero-depth / synced sine cycle / depth
  scaling / square-wave alternation / S&H flat / page offset
  continuity / output clamp).  Full suite at 1538 passing.

### LLM writeback diff viewer

- The pipeline already filters every lane's JSON down to the keys
  it actually wrote, so each LaneApplied event carries an exact
  "what changed" payload.  Now those payloads survive past the
  log-line emit: a capped queue keeps the last 16 lane applies on
  hand for inspection.
- New `state::LaneApplyRecord { lane_label, update, ms, cycle }`
  + `LlmState.recent_lane_applies: VecDeque<LaneApplyRecord>` (cap
  `LANE_APPLY_LOG_MAX = 16`, transient, not serialised).  The
  pipeline-events handler pushes one record per LaneApplied,
  popping the oldest on overflow.
- New "Lane Diff" floating window (toggled from the header view
  menu, default off): newest-first list of recent lane applies,
  each row showing `lane · cycle · ms · N keys` in a
  `CollapsingHeader`.  Expanding a row dumps the JSON payload
  pretty-printed; the JSON IS the diff because the pipeline
  filter narrowed it.  "clear" button empties the queue.
- 9 lane-diff tests: default-empty, capacity cap drops oldest,
  newest pinned to back, clone round-trip, plus 5 tests for the
  `count_diff_keys` heuristic over flat / nested / mixed / non-
  object payloads + a real filter_lane_output bass payload.
  Full suite at 1523 tests passing.

### MIDI learn — user-defined CC → param bindings

- The MIDI input path used to consult only a static built-in CC
  table (`cc_to_param_path` — CC74→cutoff, CC91→reverb_mix, etc.).
  Anything outside that ten-entry list was ignored, and there was
  no way to point a hardware knob at a less-common parameter
  without editing source.
- New `UiPrefs.midi_cc_bindings: BTreeMap<u8, String>` persists
  user bindings across sessions.  The MIDI handler now consults
  user bindings first; the static table is the fallback.  All user
  bindings normalize 0..127 → 0..1 and apply via the existing
  `dot_path_to_json + apply_llm_update` pipeline, so any
  parameter reachable by the LLM apply path is bindable (bass /
  fx / drum kit / etc.).
- New Preferences → Controls → MIDI BINDINGS section: lists
  current bindings (delete with ✕), plus an "Add binding" form —
  type a dot-path, click "learn next CC", turn a knob on the
  controller, the next CC heard saves the pair.  A pending learn
  shows a "Waiting for CC… → path" banner with a cancel button.
- `ImpulseApp.midi_learn_target: Option<String>` is the transient
  cell holding the in-flight learn target; never persisted, so
  pending learns reset cleanly on app start.
- 8 new MIDI learn tests cover serde round-trip, default-empty,
  field-missing-from-old-session deserialization, removal, the
  apply-path normalize-to-0-1 invariant, multi-domain (bass + fx)
  routing, bogus-path no-panic, and the user-overrides-static
  precedence contract.  Full suite at 1514 passing.

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
