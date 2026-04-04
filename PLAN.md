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
- [x] Swing / shuffle per pattern
- [x] Per-step accent and slide on bass sequencer
- [ ] Time signature selector (4/4, 3/4, 5/4, 6/8, 7/8…)
- [x] Amen break pattern preset — "make an amen break" should load a proper syncopated 170 BPM breakbeat

### Synthesis
- [x] Supersaw oscillator (detune + unison count) — needed for trance, rave stabs, Reese bass
- [x] Sub-oscillator (one octave below, mix control)
- [x] Oscillator detune (osc_detune ±1 semitone, shifts whole oscillator pitch)
- [ ] FM pair (simple 2-op FM for metallic/bell tones)
- [x] Noise source (noise_mix into bass oscillator before filter)
- [x] Portamento / glide time (separate from TB-303 slide)
- [x] Second filter mode: highpass + bandpass (current is always lowpass)
- [x] Waveshaper / soft clip (pre-FX tanh saturation insert)
- [x] Chorus / ensemble effect (essential for 80s sounds, Reese bass)
- [ ] Hoover lead — iconic early rave / hardcore sound: supersaw + aggressive highpass filter sweep
      triggered by pitch. Named after the vacuum cleaner drone on Human Resource "Dominator" (1991).
      Implementation: supersaw osc → highpass filter with fast attack env, slow cutoff sweep down,
      heavy resonance. Optionally add a pitch LFO for the "wailing" character. Expose as a voice
      preset or a dedicated `hoover` section in the synth with: `filter_start` (0–1), `sweep_time`
      (0.1–4 s), `resonance` (0–1), `detune` (0–1). LLM trigger: "add a hoover", "rave lead".
- [ ] Reese bass preset — detuned saws (supersaw_voices=2, supersaw_detune≈0.3) + sub_osc≈0.5
      + highpass filter removing sub mud + slight chorus. Expose as a one-shot LLM preset.
- [x] Proper 808 kick voice with pitch envelope (booming sub tail)
- [ ] Amen break sampler voice — load a WAV, pitch + stretch — the only real jungle solution

### LFO (global, wireable)

A standalone LFO engine separate from the AN1X voice — targets any parameter
in the synth. Think of it as a modulation matrix row, not a per-voice feature.

- [x] `LfoState` in `AppState`: up to 4 LFO slots (waveform, rate, depth, phase_offset, target, enabled)
- [x] Patchable sinks: BassCutoff, BassResonance, BassPitch, BassVolume, ReverbMix, DelayTime,
      DelayFeedback, ChorusMix, ChorusRate, Kick808Pitch
- [x] LFO runs in `process_block()` — one tick per audio block, modulates working params copy
- [x] LFO sync to sequencer transport: phase resets on sequencer start
- [x] UI: LFO panel — enable toggle, waveform buttons, rate/depth drag, target cycle button
- [x] LLM can set all LFO fields via JSON schema (`lfo[0..3].rate`, `.depth`, `.target`, etc.)
- [x] "Slow filter sweep" / "wobble bass" / "tremolo" → LLM maps to appropriate LFO config

### FX Chain
- [x] Reverb (basic)
- [x] Delay
- [x] Distortion / drive
- [x] Bitcrush (bit depth + sample rate reduction — lo-fi, gabber, breakcore)
- [x] Chorus / flanger as standalone FX slot
- [x] Phaser — all-pass filter chain with LFO-swept center frequency; classic psychedelic sweep
- [x] Ring modulator — multiply signal by a carrier sine; metallic/robotic character
- [x] EQ (3-band: low shelf 200Hz, mid peak 1kHz, high shelf 5kHz; biquad)
- [ ] Compressor / limiter on master bus
- [ ] Tape saturation / warmth (warm distortion with subtle wow/flutter)
- [ ] FX routing: modular FX slots instead of fixed chain — each slot is an assignable module

### Terminal / CLI
- [ ] Font configuration for Unicode box-drawing and special chars — the ✦ char (U+2726) logged
      on empty-prompt jam triggers renders as a replacement square □ in some terminals. Fix:
      document in README that the terminal font must include Unicode Block Elements and Geometric
      Shapes (e.g. JetBrains Mono, Fira Code, Nerd Fonts). Consider falling back to `*` when the
      locale is not UTF-8, or just replace ✦ with a plain `·` to avoid the issue entirely.

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

- [x] TTS backend — `espeak-ng` subprocess (zero-dep, Linux + Windows)
- [x] TTS only fires in MC / DJ mode — producer-mode explanations are never spoken (sounds wrong)
- [x] TTS output mirrored to CLI console (`log::info!("[TTS] …")`)
- [ ] TTS output mirrored to in-UI comment log (distinguish from LLM text log visually)
- [ ] TTS settings panel: pitch, speed, amplitude, voice character selector
      *(add TTS tab to prefs window now; full redesign deferred to Phase 3 modular UI)*
- [ ] Per-character pitch/speed randomisation (±10%) — prevents robotic monotone
- [ ] Alternative: Coqui TTS for higher quality voice (Python subprocess or REST)
- [ ] LLM generates `"mc_line"` JSON field alongside param updates (separate from `_comment`)
- [ ] Selectable MC voice character: Jungle MC, Rave Announcer, Robot, Smooth DJ
- [ ] TTS FX wiring: TTS audio routed through a light reverb + optional bitcrush (hall MC sound)
- [ ] Volume envelope on TTS so it ducks under the music
- [ ] **Autotune / pitch-snap on TTS** — pitch-quantize the espeak-ng output to the synth's current
      key and scale, giving the MC voice a melodic "T-Pain" or jungle toaster character.
      Implementation approach (offline post-process, feasible with no new language deps):
        1. TTS renders to a temp WAV (espeak-ng already supports `-w outfile.wav`)
        2. Per-frame fundamental frequency tracked (YIN algorithm, ~50 lines of pure Rust)
        3. Each frame pitch-shifted to nearest scale degree via `rubberband-cli` subprocess
           (`rubberband --pitch N input.wav out.wav`) — same call on Linux and Windows
        4. Processed WAV decoded and played back through `cpal` (already in the project) —
           no `aplay` or platform-specific player needed
        5. Target scale = `AppState.sequencer.root_note` + `scale` (once key/scale state lands)
           — defaults to chromatic if not set
      Simpler fallback: shift the whole line to a fixed pitch (root note) with one
      `rubberband` call + no pitch tracking — instant T-Pain feel, trivial to implement first.
      **Dependencies** (both required, both cross-platform):
        - `espeak-ng` — Linux: `apt install espeak-ng`; Windows: installer from espeak-ng.github.io
        - `rubberband-cli` — Linux: `apt install rubberband-cli`; Windows: official builds at
          breakfastquay.com/rubberband, or via vcpkg / MSYS2 / winget
      Document both in README as required deps; the app degrades gracefully (no TTS/autotune)
      if either is absent — but they are not optional in spirit, just runtime deps not compile deps.
      Add checkbox in TTS settings: "Pitch snap" + "Snap to root only / full scale".

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

## LLM Music Theory Grounding  *(prompt engineering, do when tuning)*

The LLM should understand enough music theory to make intelligent harmonic choices —
not just timbre/rhythm. Currently it knows nothing about keys, chords, or scales, so
"make it more jazzy" affects only texture. Goal: it should be able to set bass notes and
step patterns that are tonally coherent.

### System prompt additions
- [ ] Embed a compact music theory reference in the system prompt (or a separate
      `system_prompt_music_theory` block injected alongside the existing prompt):
      - 12-note chromatic scale with semitone offsets
      - Major and natural minor scale formulas (W-W-H-W-W-W-H)
      - Common triad shapes: major (0,4,7), minor (0,3,7), diminished (0,3,6)
      - Common 7th chords: maj7 (0,4,7,11), dom7 (0,4,7,10), min7 (0,3,7,10)
      - Nashville number system shorthand (I ii iii IV V vi vii°) so the LLM
        can reason about chord progressions without needing to know the key

### Key / scale state
- [ ] Add `root_note: u8` (MIDI 0–11, default 0 = C) and `scale: Scale` enum
      (Major, NaturalMinor, Dorian, Phrygian, Lydian, Mixolydian, Locrian,
      Pentatonic, Blues, Chromatic) to `AppState` / sequencer state
- [ ] LLM can set `root_note` and `scale` via JSON schema
- [ ] Sequencer step notes snap to the current scale (optional, toggleable)
- [ ] UI: root note selector (piano key row) + scale selector dropdown in sequencer panel
- [ ] Huth note colors on root/chord tones — highlight tonic, 3rd, 5th in their
      respective colors so the grid is visually harmonic

### Style briefs
- [ ] Add `"suggested_root"` and `"suggested_scale"` fields to styles.json entries
      so genre styles carry their natural tonality (e.g. acid → minor/phrygian,
      BoC → dorian, jungle → minor, dub techno → minor/dorian)
- [ ] System prompt builder reads active style's `suggested_root/scale` and
      includes them as a strong hint to the LLM

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
9. [x] AI persona name — decouple from model file, editable in prefs
10. [x] Reasoning toggle (Qwen3 /think mode, greyed out for unsupported models)
11. [x] Download script: Qwen3-8B Q4_K_M option
12. [x] Run artist reference LLM tests — audit styles.json, drop dead references
13. [x] FX XY pads (reverb and delay panels)
