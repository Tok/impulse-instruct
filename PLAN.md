# Impulse Instruct — Vision & Roadmap

A synthesizer with a tiny LLM living inside it that actually understands music.
Bonsai listens, jams, evolves, and shouts at the crowd.

---

## Vision

You talk to it like a producer or a crowd. It responds like a machine that has listened
to everything and has taste. You can say "make an amen break", "go darker", "drop the
kick", "hit a Reese bass", "switch to jungle". It does it. It sounds right. It talks back.

---

## Phase 1 — Make It Sound Right  *(current sprint)*

The synth currently only has one real voice (303-style bass + drum kit).
It can nail acid. Everything else is a lie. Fix that.

### Sequencer
- [x] 16-step sequencer with per-step velocity
- [x] BPM locked to user by default — unlock for LLM control
- [x] Variable step count per pattern (8 / 16 / 32 / 64)
- [ ] Swing / shuffle per pattern
- [ ] Per-step accent and slide on bass sequencer
- [ ] Time signature selector (4/4, 3/4, 5/4, 6/8, 7/8…)
- [x] Amen break pattern preset — "make an amen break" should load a proper syncopated 170 BPM breakbeat

### Synthesis
- [x] Supersaw oscillator (detune + unison count) — needed for trance, rave stabs, Reese bass
- [ ] Sub-oscillator (one octave below, mix control)
- [ ] Oscillator detune (for that Reese bass detuned-saws feel)
- [ ] FM pair (simple 2-op FM for metallic/bell tones)
- [ ] Noise source (for snare body, hi-hat, wind textures)
- [ ] Portamento / glide time (separate from TB-303 slide)
- [ ] Second filter mode: highpass + bandpass (current is always lowpass)
- [ ] Waveshaper / soft clip (lighter distortion option)
- [ ] Chorus / ensemble effect (essential for 80s sounds, Reese bass)
- [ ] Proper 808 kick voice with pitch envelope (booming sub tail)
- [ ] Amen break sampler voice — load a WAV, pitch + stretch — the only real jungle solution

### FX Chain
- [x] Reverb (basic)
- [x] Delay
- [x] Distortion / drive
- [x] Bitcrush (bit depth + sample rate reduction — lo-fi, gabber, breakcore)
- [ ] Chorus / flanger as standalone FX slot
- [ ] EQ (3-band: low shelf, mid peak, high shelf)
- [ ] Compressor / limiter on master bus
- [ ] Tape saturation / warmth (warm distortion with subtle wow/flutter)
- [ ] FX routing: modular FX slots instead of fixed chain — each slot is an assignable module

### Styles
- [ ] Audit styles.json: remove or replace any artist reference that Bonsai doesn't understand (run llm_suite artist tests first)
- [ ] Add amen break template to jungle / DnB styles
- [ ] Rewrite dub techno description to make FX chain actually dominant
- [ ] Add genre-specific starter patterns (not just parameter suggestions)

---

## AN1X-Style Voice (BoC / Warm VA Aesthetic)

Target sound: Boards of Canada — warm, slightly detuned virtual analog. Nostalgic,
drifting, slightly wrong. The AN1X (Yamaha, 1997) was a VA synth whose specific
character — a digital engine with analog-flavoured imperfection — is central to that
palette. This is a separate voice type from the 303 bass, oriented toward pads, leads,
and melodic sequences.

### Oscillator layer
- [ ] Dual oscillator (OSC1 + OSC2), each with: Saw, Square (PWM), Triangle, Sine, Noise
- [ ] OSC2 coarse + fine detune relative to OSC1 (the detuned beating is the core BoC texture)
- [ ] OSC2 octave offset (−2 / −1 / 0 / +1 / +2)
- [ ] Oscillator mix (OSC1 level, OSC2 level, noise level)
- [ ] Hard sync: OSC2 resets its phase to OSC1 each cycle — with pitch sweep this produces the aggressive "screaming" harmonic sweep (more trance/techno than BoC, but part of the AN1X palette)
- [ ] Ring modulation: OSC1 × OSC2 output mixed into signal path
- [ ] Sub-oscillator: square wave one octave below OSC1, level control

### Modulation
- [ ] 2 LFOs, each with: Sine, Triangle, Saw, Square, Sample+Hold (random), Noise
- [ ] LFO rate: 0.01 Hz (glacial, BoC-style breathing) to ~20 Hz (vibrato)
- [ ] LFO delay + fade-in (starts slow, deepens over held note)
- [ ] LFO destinations per LFO: OSC1 pitch, OSC2 pitch, OSC1+2 pitch, filter cutoff, amplitude, PWM width
- [ ] LFO sync to BPM (rate snaps to musical divisions)
- [ ] Pitch drift: very low-depth random LFO on pitch — simulates tape instability and slight tuning imperfection (key BoC texture, subtly "analogue feeling")
- [ ] Free EG concept: one slow arbitrary-shape envelope that can be drawn and assigned to any target (filter, pitch, amp, detune) — enables long evolving patches

### Envelopes
- [ ] Filter envelope (ADSR) with amount + polarity (positive/negative modulation)
- [ ] Amplitude envelope (ADSR)
- [ ] Pitch envelope (Attack + Decay + amount) — for pluck-style pitch transients

### Filter
- [ ] Filter mode selector: Lowpass 24dB (Moog-style, current), Highpass 12dB, Bandpass 12dB
- [ ] Self-oscillation at high resonance (produces sine wave at resonant frequency)
- [ ] Filter key tracking (filter cutoff follows note pitch — 0%, 50%, 100%)

### Portamento
- [ ] Glide time (separate from 303 slide — smooth exponential pitch glide between notes)
- [ ] Glide mode: always on, legato only (only when notes overlap)

### Voice style in the LLM
- [ ] Add `an1x` as a separate controllable voice in JSON schema alongside `bass`
- [ ] Style briefs updated so BoC / IDM / ambient styles preferentially target this voice
- [ ] LLM can set: detune, lfo_rate, lfo_depth, lfo_target, drift, filter_mode, glide

### Known gap
The BoC aesthetic also relies heavily on tape saturation, lo-fi sampling, and pitch
manipulation of recorded material — none of which synthesis alone can fully replicate.
The AN1X voice gets the synthesis side. Bitcrush + tape saturation FX complete the picture.

---

## Phase 2 — Make It Talk  *(TTS + MC mode)*

Bonsai generates the text. A TTS engine speaks it. The crowd goes wild.

- [x] TTS backend — `espeak-ng` subprocess (simple, zero-dep, Linux)
- [ ] Alternative: Coqui TTS for higher quality voice (Python subprocess or REST)
- [x] MC mode: when `_comment` is generated in MC/DJ mode, pass text to TTS
- [ ] Optional: pitch-shift + distort TTS output for "vocoder MC" effect
- [ ] LLM generates `"mc_line"` JSON field alongside param updates (separate from `_comment`)
- [ ] Selectable MC voice character: Jungle MC, Rave Announcer, Robot, Smooth DJ
- [ ] TTS FX wiring: reverb / chorus / bitcrush on the TTS audio output
- [ ] Volume envelope on TTS so it ducks under the music

---

## Phase 3 — Make It Modular  *(architecture)*

Move from a fixed linear audio chain to a node graph.

### Signal Graph
- [ ] Define `AudioNode` trait: `process(inputs, outputs, params) -> ()`
- [ ] Node types: Oscillator, Filter, Envelope, FX slot, Mixer, Output
- [ ] `AudioGraph`: topologically sorted DAG, evaluated per block
- [ ] Patch connections stored as `Vec<Connection { from: NodeId, to: NodeId, port: u8 }>`
- [ ] Backward-compatible: default graph = current fixed chain

### Cable UI — Reason-style Rack Flip

Propellerhead Reason (2000) is the reference: pressing **Tab** flips all rack
modules to reveal their back panels, where the user patches virtual cables
between I/O ports freely — the same physical reality of hardware modular
synthesis. Goal: replicate this interaction exactly.

- [ ] **Tab toggle** — pressing Tab (or a button) flips the view from front panels
      to a "back of rack" cable view and back. Same modules, different face.
- [ ] Front view: knobs, buttons, sequencer (current UI panels, unchanged)
- [ ] Back view: each module renders as a flat back panel with labeled I/O
      port jacks (circles); no controls visible — only ports and cables
- [ ] Drag from output jack to input jack → spawn a Bezier cable
- [ ] Cable snaps to nearest valid port on release (within threshold pixels)
- [ ] Delete cable: click anywhere on the cable path, or right-click → remove
- [ ] Cable colors by signal type: audio = white, CV/modulation = yellow,
      MIDI = blue, gate/trigger = green
- [ ] Cables rendered as thick Bezier curves with a slight droop (gravity sim:
      midpoint offset proportional to distance, like a real cable hanging)
- [ ] Rack layout is scrollable horizontally; modules appear left-to-right in
      the same order as the front tabs
- [ ] Connections stored as `Vec<Connection { from: (NodeId, PortId), to: (NodeId, PortId) }>`
- [ ] Back-view patch state persists across Tab flips
- [ ] Default patch (no user cables) = current fixed audio chain, shown as
      pre-wired cables on first open (so new users see how things connect)

---

## Phase 4 — Multiple Voices & Multiple LLMs

- [ ] `Vec<SynthVoice>` — add/remove voice instances from UI
- [ ] Each voice has its own sequencer, oscillator, and filter
- [ ] Each voice can be independently targeted by LLM ("voice 2, more acid")
- [ ] Multiple LLM instances: spawn N Bonsai backends, assign to voices
- [ ] LLM routing: one LLM controls all voices (current), or one LLM per voice
- [ ] LLM can create new voice instances ("add a pad voice")

---

## Phase 5 — Break the Grid

- [ ] Polyrhythm: per-voice step counts that don't have to match (kick 16, hihat 12, bass 7)
- [ ] Euclidean rhythm generator (LLM can say "4-in-16 euclidean kick")
- [ ] Step probability per step (0–100% chance of firing)
- [ ] Ratcheting / note repeat per step
- [ ] Pattern chaining: define multiple patterns and sequence them (A → B → A → C)
- [ ] Live record: play keyboard, record directly into sequencer steps

---

## Phase 6 — Export & Integration

- [x] WAV export (32-bit float)
- [x] MP3 export (ffmpeg)
- [x] HTTP/MCP API
- [ ] MIDI clock out (sync external hardware / DAW)
- [ ] MIDI clock in (slave to external BPM)
- [ ] OSC support (for Max/MSP, Ableton, TouchOSC)
- [ ] Stem export (per-voice WAV files)
- [ ] Project versioning (auto-save snapshots, revert history)

---

## Known Gaps (styles vs synth reality)

| Style | What it promises | What's missing |
|-------|-----------------|----------------|
| Jungle | Amen break energy | Sampler voice, breakbeat patterns |
| DnB | Reese bass | Detuned supersaw, sub oscillator |
| IDM | Broken, polyrhythmic | Variable steps, step probability |
| Synthwave | Gated reverb snare | Reverb envelope / gate on drum channel |
| Vaporwave | Pitch-shifted down, woozy | Pitch control on output, pitch drift LFO |
| Dub Techno | FX IS the music | Modular FX routing (currently fixed chain) |
| Gabber | Kick IS the bass | Dedicated gabber kick voice (pitch env + clipper) |
| Ambient | Glacial filter sweeps | LFO / slow automation on filter |

---

## Model Options

Bonsai-8B is Qwen3-8B compressed to 1-bit (Q1_0_g128, PrismML format, 1.1 GB).
Quality ceiling is low. With 16 GB VRAM there is plenty of headroom for better quants
or larger models. The llama-server backend is model-agnostic — just swap the GGUF file.

### Recommended alternatives

| Model | Quant | Size | VRAM | Notes |
|-------|-------|------|------|-------|
| **Qwen3-8B Q4_K_M** | Q4_K_M | ~5 GB | ~7 GB | Same base as Bonsai, ~5× better quality |
| **Qwen3-8B Q8_0** | Q8_0 | ~8.7 GB | ~10 GB | Near-lossless; fits RTX 4070 Ti |
| **Qwen3-14B Q4_K_M** | Q4_K_M | ~9 GB | ~11 GB | Better musical reasoning, still fits |
| **Gemma 4 4B (E4B)** | Q4 | ~3 GB | ~5 GB | Fast, good structured output |
| **Llama 3.1-8B Q4_K_M** | Q4_K_M | ~5 GB | ~7 GB | Excellent JSON compliance |

HuggingFace sources (verify before downloading):
- `bartowski/Qwen_Qwen3-8B-GGUF` — Qwen3-8B quants
- `bartowski/Meta-Llama-3.1-8B-Instruct-GGUF` — Llama 3.1
- `unsloth/gemma-4-E4B-it-GGUF` — Gemma 4 (if available)

### TODO: model infrastructure
- [ ] `download-models.sh` — add options for Qwen3-8B Q4_K_M and Llama 3.1-8B
- [x] UI model selector: scan `models/` directory, show file list as radio group in prefs
- [x] Hot-swap: `LlmInput::SwitchModel(path)` restarts backend in LLM thread
- [ ] Reasoning / thinking mode toggle — **only relevant for models that support it**
      (Qwen3 supports thinking via `/no_think` token or system prompt toggle;
      Bonsai uses our `_thinking` JSON field hack instead of native `<think>` tags).
      When enabled: longer latency, better multi-step reasoning for complex prompts.
      When disabled: faster responses for simple parameter commands.
      Add checkbox in prefs panel, disabled/greyed out when model doesn't support it.
      Implement: set `enable_thinking: bool` on `LlmState`; in `infer()` append
      `/think` or `/no_think` suffix to user message for Qwen3, or add
      `"thinking": {"type": "enabled", "budget_tokens": 512}` to request body
      (depends on server/model capabilities).
- [ ] AI persona name — when stitching different models, decouple the persona from
      the model name. Current name "Bonsai" is confusing (Bonzai Records association,
      Belgian techno). Give the synth intelligence a snappy internal name that is
      model-agnostic. Something like **UNIT**, **GRID**, **IRIS**, **VERB**, **OSC**,
      **PULSE**, or **VOLT** — displayed in the UI and used in system prompt persona
      ("you are PULSE, the intelligence inside Impulse Instruct, a synthesizer...").
      Model file stays separate from persona. Add to prefs panel as a text field.

---

## UI Inspiration: Ableton Learning Synths

https://learningsynths.ableton.com/en/playground

Key elements to implement (in egui, respecting grayscale + Huth note colors only):

### XY Control Squares  ← **implement next**
- [x] `widgets::xy_pad(ui, label_x, label_y, x, y, size, locked)` — 2D parameter pad
- [x] Used in bass panel: CUT×RES pad and ENV×DEC pad
- [x] Use in FX panel: REVERB_MIX×REVERB_SIZE and DELAY_MIX×DELAY_FEEDBACK
- [ ] Use in 808 kick: PITCH×DECAY pad
- [ ] Generic: any two correlated params benefit from this (filter cutoff + resonance is the canonical case)
- [ ] Optional: show parameter name on cursor when dragging (tooltip-style)

### Oscilloscope / Waveform Display
- [ ] Ring buffer in audio thread → UI: `rtrb::RingBuffer<f32>` of ~2048 samples
- [ ] egui painter draws the waveform as a polyline (white stroke on PIT bg)
- [ ] Place above the piano in the main view, or as a narrow strip at the top of each voice panel
- [ ] No color — grayscale only: CHALK line on PIT bg with SLATE border

### Envelope Visualization
- [ ] Draw ADSR shape as a polyline given the 4 parameter values
- [ ] Used in AN1X voice panel and 303 panel (decay only, simplified)
- [ ] Interactive: drag the breakpoints to edit A/D/S/R directly on the shape

### Step Sequencer Improvements (inspired by Ableton's grid)
- [ ] Step length indicator: small marker showing 16th / 8th / dotted etc.
- [ ] Velocity lanes below each step row (small bar graph per step, drag to set)
- [ ] Mute/solo per row (M/S buttons on left edge of each row)
- [ ] Pattern copy/paste (right-click menu or keyboard shortcut)

---

## Immediate next steps

1. [x] Variable step count (8/16/32/64) — unblocks amen breaks and polyrhythm
2. [x] Amen break pattern preset + LLM instruction "make an amen break"
3. [x] Supersaw oscillator — unblocks Reese bass, rave stabs, DnB
4. [x] Bitcrush FX — unblocks gabber, breakcore, lo-fi sounds
5. [x] TTS MC mode — highest fun-per-line-of-code ratio
6. [x] XY control squares widget (CUT×RES, ENV×DEC pads in bass panel)
7. [x] Model selector UI (scan models/, hot-swap via LlmInput::SwitchModel)
8. [x] Oscilloscope strip (rtrb ring buffer → egui polyline)
9. [ ] AI persona name — decouple from model file, editable in prefs
10. [ ] Reasoning toggle (Qwen3 /think mode, greyed out for unsupported models)
11. [ ] Download script: Qwen3-8B Q4_K_M option
12. [ ] Run artist reference LLM tests — audit styles.json, drop dead references
13. [x] FX XY pads (reverb and delay panels)
