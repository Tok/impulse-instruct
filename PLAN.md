# Impulse Instruct — Vision & Roadmap

A synthesizer with a tiny LLM living inside it that actually understands music.
Bonsai listens, jams, evolves, and shouts at the crowd.

---

## The North Star

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
- [ ] Variable step count per pattern (8 / 16 / 32 / 64)
- [ ] Swing / shuffle per pattern
- [ ] Per-step accent and slide on bass sequencer
- [ ] Time signature selector (4/4, 3/4, 5/4, 6/8, 7/8…)
- [ ] Amen break pattern preset — "make an amen break" should load a proper syncopated 170 BPM breakbeat

### Synthesis
- [ ] Supersaw oscillator (detune + unison count) — needed for trance, rave stabs, Reese bass
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
- [ ] Bitcrush (bit depth + sample rate reduction — lo-fi, gabber, breakcore)
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

## Phase 2 — Make It Talk  *(TTS + MC mode)*

Bonsai generates the text. A TTS engine speaks it. The crowd goes wild.

- [ ] TTS backend — `espeak-ng` subprocess (simple, zero-dep, Linux)
- [ ] Alternative: Coqui TTS for higher quality voice (Python subprocess or REST)
- [ ] MC mode: when `_comment` is generated in MC/DJ mode, pass text to TTS
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

### Cable UI (Rack View)
- [ ] New "Rack" tab (opt-in, alongside current panels)
- [ ] Each node renders as a panel with named I/O port circles
- [ ] Drag from output port to input port draws a Bezier cable (egui painter)
- [ ] Delete cable by clicking on it
- [ ] Rack layout is resizable and scrollable
- [ ] Cable colors by signal type (audio = white, CV/mod = yellow, MIDI = blue)

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

## Immediate next steps

1. [ ] Variable step count (8/16/32/64) — unblocks amen breaks and polyrhythm
2. [ ] Amen break pattern preset + LLM instruction "make an amen break"
3. [ ] Supersaw oscillator — unblocks Reese bass, rave stabs, DnB
4. [ ] Bitcrush FX — unblocks gabber, breakcore, lo-fi sounds
5. [ ] TTS MC mode — highest fun-per-line-of-code ratio
6. [ ] Run artist reference LLM tests — audit styles.json, drop dead references
