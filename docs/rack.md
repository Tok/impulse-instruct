# Impulse Instruct - Modular Rack

The rack is the data model for every synthesis module, FX processor, and cable
connection in the app. It replaces the earlier fixed 5-tab layout with a
fully programmable routing graph.

Source: `src/state/rack.rs`

---

## Overview

The rack stores:

1. A list of **modules** — synthesizer voices, FX units, sequencer, master output.
2. A list of **cables** — directional connections between module ports.
3. A compiled **FxPlan** — the DSP-ready processing order, derived from the cable graph.

The rack itself is a field on `AppState` (`app_state.rack: RackState`). It serializes
to JSON with the rest of the session state.

When the rack topology changes (module added/removed, cable connected/disconnected),
the UI calls `compile_fx_plan(&rack)` to produce a new `FxPlan`, which is sent to the
audio thread via `AudioCommand::SetFxPlan`. The audio thread stores it in `DspState`
and uses it in the next `process_block()` call. The rack itself never touches the
audio thread directly.

---

## Data types

### ModuleKind

Every module type the rack can hold:

```rust
pub enum ModuleKind {
    // Voice modules (one DSP bus each)
    AcidBass,       // TB-303-style ladder filter bass
    DrumKit808,     // TR-808 kick, snare, hihat voices
    DrumKit909,     // TR-909 kick, snare voices
    HooverLead,     // Hoover/rave lead synth
    An1xVoice,      // Yamaha AN1X-style virtual analogue pad/lead
    AmenSampler,    // Amen break sampler / loop slicer
    NoiseVoice,     // White/pink noise generator

    // TTS voice module (NeuTTS Air)
    NeuTts,         // Neural TTS voice cloning via local GGUF model

    // Sequencer
    StepSequencer,  // Main 16/32/64-step clock and pattern store

    // FX processors (may have multiple instances)
    FxReverb, FxDelay, FxChorus, FxPhaser,
    FxRingMod, FxWaveshaper, FxBitcrush,
    FxEq, FxCompressor, FxTapeSat, FxDrive,

    // Modulation
    LfoModule,      // Low-frequency oscillator (up to 4 instances by default)

    // Utility
    MasterOutput,   // Final mix bus / master volume
}
```

**Key methods:**

| Method | Returns | Description |
|--------|---------|-------------|
| `kind.label()` | `&'static str` | Short display name for the module card title bar |
| `kind.default_zone()` | `Zone` | Which rack zone the module belongs to |
| `kind.allows_multiple()` | `bool` | Whether more than one instance can exist (all FX and LFO: yes) |

### Zone

Modules are grouped into three vertical zones in the rack UI:

| Variant | Contents |
|---------|----------|
| `Zone::Global` | StepSequencer, MasterOutput — full-width strip at the top |
| `Zone::Voice` | All voice and TTS modules |
| `Zone::FxMod` | All FX processors and LFO instances |

### RackModule

A single instantiated module in the rack:

```rust
pub struct RackModule {
    pub id: u32,        // Stable ID used by Cable references (never reused)
    pub kind: ModuleKind,
    pub enabled: bool,  // When false, module is bypassed in compile_fx_plan
    pub zone: Zone,     // Derived from kind.default_zone() at creation
    pub slot: u8,       // Position within zone (left-to-right order)
}
```

IDs start at 100. Values 0–99 are reserved for future system nodes.

### Ports and cables

A **PortRef** identifies a specific port on a specific module:

```rust
pub struct PortRef {
    pub module_id: u32,
    pub dir: PortDir,   // In or Out
    pub kind: PortKind, // Audio (stereo) or Cv (0–1 mono)
    pub index: u8,      // Port index within (dir, kind) group
}
```

A **Cable** connects one output port to one input port:

```rust
pub struct Cable {
    pub from: PortRef,
    pub to: PortRef,
    pub color: CableColor,  // Auto-assigned, cycles through 8 grayscale shades
}
```

**CableColor** cycles through: Gray, Slate, Silver, Ash, Stone, Iron, Pewter, Smoke.
All cable colors are strictly grayscale (R=G=B) per `docs/ui-design.md`.
`RackState::next_cable_color()` returns the next shade based on `cables.len() % 8`.

Audio cables are validated on insertion: `connect()` rejects cables that would
create a cycle in the signal graph (BFS reachability check). On session load,
`strip_audio_cycles()` removes any cyclic audio cables that slipped in from
older versions. `compile_fx_plan()` also handles cycles gracefully via Kahn's
topological sort (cyclic nodes produce empty output).

### RackState

The top-level rack container:

```rust
pub struct RackState {
    pub modules: Vec<RackModule>,
    pub cables: Vec<Cable>,
    pub next_id: u32,   // monotonically increasing ID counter
}
```

**Key methods:**

| Method | Description |
|--------|-------------|
| `rack.add_module(kind)` | Create a module, assign next ID and slot, return ID |
| `rack.remove_module(id)` | Remove module and all its cables |
| `rack.connect(from, to)` | Add a cable (no duplicate check) |
| `rack.disconnect(&from, &to)` | Remove cable matching those exact ports |
| `rack.module(id)` | Find a module by ID |
| `rack.zone_modules(zone)` | All modules in a zone, sorted by slot |
| `rack.next_cable_color()` | Next cable color in the rotation |

---

## Default layout

`RackState::default()` mirrors the original fixed 5-tab layout so pre-rack sessions
still render correctly:

**Global zone:** StepSequencer, MasterOutput

**Voice zone:** AcidBass, DrumKit808, DrumKit909, HooverLead, An1xVoice, AmenSampler, NeuTts

**FX zone (in processing order):**
Waveshaper → Reverb → Delay → Bitcrush → Chorus → Phaser → RingMod → Eq → Compressor → TapeSat → Drive

**LFO:** 4 LfoModule instances

**Default cables:**
- Voice modules (AcidBass through AmenSampler) → MasterOutput
- NeuTts → FxReverb (TTS bus bypasses the main voice mix)
- FX chain: each FX output → next FX input (serial chain)

---

## FX routing plan

### FxStep and FxPlan

`compile_fx_plan` converts the cable graph into a DSP-ready `FxPlan`:

```rust
pub enum FxStep {
    Waveshaper, Reverb, Delay, Bitcrush, Chorus,
    Phaser, RingMod, Eq, Compressor, TapeSat, Drive,
}

pub struct FxPlan {
    /// Global chain — applied to all voices without explicit per-voice routing.
    pub steps: Vec<FxStep>,

    /// Per-voice chains (key = ModuleKind of the voice module).
    /// When non-empty for a voice, that bus uses its own chain instead of global.
    pub voice_routes: HashMap<ModuleKind, Vec<FxStep>>,
}
```

### compile_fx_plan

`compile_fx_plan(rack: &RackState) -> FxPlan` builds the plan in two passes:

**Pass 1 — global chain (Kahn's topological sort):**
1. Collect all enabled FX modules (those with a `kind_to_fx_step` mapping).
2. Build an adjacency list over FX→FX audio cables only.
3. Run Kahn's algorithm with tie-breaking by module ID for deterministic ordering.
4. The result is `FxPlan::steps` — the ordered global chain.

**Pass 2 — per-voice routes:**
1. For each audio cable from a voice module to an FX module, record the entry point.
2. Walk the linear FX→FX adjacency from that entry to collect the sub-chain.
3. Store as `FxPlan::voice_routes[voice_kind] = chain`.

**Fast path:** When `voice_routes` is empty (no per-voice cables patched), the DSP
runs every voice through the single global chain with no branching.

### How the DSP uses FxPlan

In `DspState::process_block()` (`src/audio/dsp/mod.rs`):

1. Before the frame loop, copy `fx_plan.steps` into a stack array `[FxStep; 16]`.
   This snapshot releases the immutable borrow before mutable `&mut self` calls.
2. Check `have_voice_routes = !fx_plan.voice_routes.is_empty()`.
3. **Fast path:** Route the summed voice output through the global chain.
4. **Per-voice path:** Each of the 7 voice buses (AcidBass, DrumKit808, DrumKit909,
   HooverLead, An1xVoice, AmenSampler, NoiseVoice) is routed through its own chain
   if one exists in `voice_routes`, otherwise falls back to the global chain.
5. TTS bus is handled separately after the main mix.

The audio thread never reads `RackState` directly. It only reads the compiled `FxPlan`
that was sent via `AudioCommand::SetFxPlan`.

---

## Signal flow diagram

```
StepSequencer
    |
    +-- triggers --> AcidBass    ---[voice cable]--> FxReverb ---+
    +-- triggers --> DrumKit808  --|                             |
    +-- triggers --> DrumKit909  --+-> [global chain] ----------+-> MasterOutput
    +-- triggers --> HooverLead  --|   (Waveshaper→Reverb→...   |
    +-- triggers --> An1xVoice   --+    only when no per-voice   |
    +-- triggers --> AmenSampler --|    cable exists)            |
    +-- triggers --> NoiseVoice  --+                             |
                                                                 |
NeuTts -------[tts cable]-------> FxReverb ------[tts mix]-+
```

When AcidBass has an explicit cable to FxReverb, it routes through
`voice_routes[AcidBass]` instead of the global chain. All other buses
without explicit cables share the global chain.

---

## Adding a new module kind

1. Add a variant to `ModuleKind` in `src/state/rack.rs`.
2. Add a `label()` arm and a `default_zone()` arm.
3. If it is an FX processor, add an `FxStep` variant and a `kind_to_fx_step` arm.
4. If it has multiple instances, add it to `allows_multiple()`.
5. Add it to the default layout in `RackState::default()` if it should appear by default.
6. Add a render card in the rack UI panel (`src/ui/panels/`).
7. If it produces audio, add a DSP bus in `process_block()` and handle
   `AudioCommand` variants in `audio/mod.rs`.

---

## Current status

- Cable graph and module state fully implemented and persisted in session JSON.
- `compile_fx_plan` operational: global chain and per-voice routes both active in DSP.
- Default cables wire the serial FX chain and TTS bus at startup.
- Rack UI renders module cards; cable patching UI is in progress.
- TTS module cards (NeuTts, NeuTts) render in the voice zone; TTS audio routes
  through the reverb bus by default.
- CV routing (LFO → parameter target) is data-modeled but not yet wired into the DSP.
