#!/usr/bin/env bash
# ─── J.S. Bach — Italian Concerto III (MIDI score import) ───────────────────
# Plays a 2-voice piano MIDI of Bach's third movement on two 303 voices
# at its intended 240 BPM, start to end (~3:30 of playback).
#
# Deliberately simple: score playback + filter sweeps + FX, not a jam.
#
#   - Bass score + sequencer stay LOCKED; nobody writes to them.
#   - Pan is handled by a native LFO (state.lfo[0] targeting BassPan),
#     configured once via the API — no scripted Python loop, no agent
#     touching pan, no HTTP hammering during playback.
#   - One PAD agent drifts the 303 filter (cutoff + resonance) across
#     the movement.  That's its ONLY remit.
#   - 808 arrives mid-piece for a deterministic half-time pulse
#     (kicks on 0/8, closed hat on 4/12) written directly via
#     api_params — no KIT agent needed.
#   - One FX agent escalates reverb + bitcrush into the outro.
#   - Camera tracks each addition: focus_on the new module, linger a
#     few seconds, then return to the score.

scene_count 6

BACH_MIDI="${BACH_MIDI:-demo/scenarios/bach-italian-3rd.mid}"

# Helper: scroll to `$1`, pause `$2` seconds (default 4) so the viewer
# can see the module, then return to the sequencer view for continuity.
# Used after every add_instrument / add_effect so additions read as
# deliberate cuts instead of fly-bys.
show_then_return() {
    local target="$1"
    local linger="${2:-4}"
    focus_on "$target"
    wait_seconds "$linger"
    look_at sequencer
}

# ── Scene 1: Setup ──────────────────────────────────────────────────────────

scene "Setup"

reset_all
set_style ""

# Bach-specific Huth-per-component pinning: grayscale piano, Huth-
# coloured spectrum bars.  Sequencer step dots and event-stream
# history are Huth-coloured unconditionally.  `show_thinking_in_log`
# is on so the PAD / FX agents' reasoning traces are visible.
set_ui_prefs '{"huth_piano": false, "huth_bar_osc": false, "huth_ring_osc": false, "huth_spectrum": true, "show_thinking_in_log": true, "enable_thinking": true}'

# Master volume tuned down from the default 0.85 — the resonance
# sweeps + bitcrush outro push peak loudness into clip territory at
# the default gain.  0.60 leaves headroom for the FX escalation.
api_params '{"fx":{"master_volume":0.60}}'

add_instrument bass
look_at ai

say "Bach's Italian Concerto, third movement. Two acid basses, two hands."

if ! load_midi "$BACH_MIDI"; then
    say "The MIDI file didn't load. Check that the app is running and the file exists."
    stop
    exit 1
fi

PLAY_DURATION="$MIDI_DURATION"
PLAY_DURATION_INT=${PLAY_DURATION%.*}
echo "  [scenario] will play for ${PLAY_DURATION_INT}s at ${MIDI_BPM} BPM"

# Starting timbre.  Voice 0 (right hand) brighter and L-panned; voice
# 1 (left hand) darker and R-panned.  These are the static defaults —
# the LFO below modulates the global bass pan on top of voice 0's
# starting position (DSP applies one pan to the summed bass bus, so
# the LFO moves both voices together in the current engine; real
# anti-phase per-voice pan is a planned DSP refactor — see PLAN.md).
api_params '{
  "bass_voices": [
    { "enabled": true, "volume": 1.0, "cutoff": 0.55, "resonance": 0.35, "env_mod": 0.45, "decay": 0.40, "accent_level": 0.5, "distortion": 0.10, "pan": -0.25 },
    { "enabled": true, "volume": 1.0, "cutoff": 0.32, "resonance": 0.55, "env_mod": 0.35, "decay": 0.65, "accent_level": 0.4, "distortion": 0.05, "pan":  0.25 }
  ]
}'

# Native pan LFO: state.lfo[0] targets BassPan with a slow sine.  The
# audio thread runs it every block — zero HTTP traffic during
# playback.  Rate 0.03 ≈ ~0.2 Hz (5-second cycle at the LFO's
# internal scaling); depth 0.7 so pan swings ~±0.7 around the
# starting position.
api_params '{
  "lfo": [
    { "enabled": true, "target": "BassPan", "waveform": "Sine", "rate": 0.28, "depth": 0.7, "phase_offset": 0.0 }
  ]
}'

# Lock the score + the entire sequencer so nothing (agent or jam
# loop) can overwrite the imported notes.  The PAD agent is scoped
# to "bass" synth params only; these locks are belt-and-braces.
lock \
    "sequencer.bass_pattern" \
    "sequencer.bass_patterns" \
    "sequencer.bass_steps" \
    "sequencer.bass_voice_steps" \
    "sequencer.bpm" \
    "sequencer.steps" \
    "bass_voices.0.note" \
    "bass_voices.1.note" \
    "bass_voices.0.pan" \
    "bass_voices.1.pan" \
    "bass.pan" \
    "lfo[0]"

show_then_return bass 4

# ── Scene 2: Play + PAD agent ───────────────────────────────────────────────

scene "Play"

look_at sequencer
play

add_agent PAD gemma bass
wait_for_model

# PAD's ONLY remit: squelch the filter (cutoff + resonance).  No pan
# (LFO owns it), no notes, no patterns, no anything else.  Bar-based
# ramps so the motion develops across pattern cycles, not bumps.
ask "PAD: squelch the 303 filter across the Bach outro.  Move ONLY cutoff and resonance — everything else is locked or off-limits.  Cutoff in [0.05, 0.55], resonance in [0.70, 1.0].  Keep voice 0 and voice 1 within 0.12 of each other so the two hands read as one squelching instrument.

Use \`ramps\` with \`bars\`: 4 or 8.  Never step-jump.  Example per turn:

  {\"ramps\":[
    {\"param\":\"bass.cutoff\",\"to\":0.12,\"bars\":8},
    {\"param\":\"bass.resonance\",\"to\":0.95,\"bars\":8},
    {\"param\":\"bass_voices.1.cutoff\",\"to\":0.14,\"bars\":8},
    {\"param\":\"bass_voices.1.resonance\",\"to\":0.92,\"bars\":8}
  ]}

Each next cycle, re-target to new corners of the cutoff/resonance pad so the motion is a continuous drift.  DO NOT touch: notes, steps, patterns, gate, accent, slide, volume, pan, distortion, env_mod, decay.  DO NOT add ramps for any of those.  Cutoff + resonance on both voices — that is your entire scope." PAD 0

# ── Scene 3: Drums arrive (+60s) ────────────────────────────────────────────
# Deterministic half-time kick + hat pattern via direct api_params.
# No KIT agent — the pattern is fixed, the scene is just showing the
# 808 layering in.  chain_advance_preserve_non_bass carries the
# drums across bank swaps so they play for the rest of the piece.

scene "Drums arrive"

# 60 s of Bach + PAD-driven filter drift before the 808 layers in.
# Scroll around instead of staring at one place: filter pad → agent
# console → back-panel cable tour → front → sequencer, then the 808
# arrival reads as a deliberate reveal after the camera tour.
fill_wait 60

add_instrument 808
show_then_return 808 5

# Bass 2× drums — Bach stays centre-stage.
api_params '{"kit_a": {"kick": {"volume": 0.45}, "hihat_closed": {"volume": 0.30}}}'

# Half-time kick (steps 0, 8) + closed-hihat (steps 4, 12) on a
# 16-step loop.  `kick_a_steps` / `hihat_a_steps` accept an index
# list, which is 4× shorter on the wire than a bool array.
api_params '{
  "sequencer": {
    "kick_a_steps": [0, 8],
    "hihat_a_steps": [4, 12]
  }
}'

# ── Scene 4: FX outro ───────────────────────────────────────────────────────
# One FX agent, tight scope: reverb + bitcrush only.  Three asks
# ~30 s apart so the escalation reads as a progression, and the
# camera stays on the FX module after each ask so you can see the
# bitcrush wet knob swing.

scene "FX outro"

FX_START_OFFSET=$(echo "$PLAY_DURATION - 60 - 90" | bc -l)
# Long stretch between drums-arrive and FX-outro — fill it with
# camera motion instead of staring at the sequencer.
fill_wait "$FX_START_OFFSET"

add_effect bitcrush
show_then_return bitcrush 4

add_agent FX gemma fx

# Stage 1: gentle.
ask "FX: outro starts now.  Nudge reverb_mix up to about 0.30 and lengthen reverb_size a touch.  Bring bitcrush in light: wet around 0.20, bit_depth_reduction around 0.30, sample_rate_reduction around 0.25.  Leave delay / chorus / phaser / eq / compressor ALONE — this scenario only uses reverb and bitcrush.  Use ramps with bars: 4 for every move." FX 0

look_at fx
wait_seconds 30

# Stage 2: dirtier.
ask "FX: stage two.  Push bitcrush harder — wet 0.55, bit_depth_reduction 0.60, sample_rate_reduction 0.55.  Raise reverb_mix another step to about 0.45.  Still reverb + bitcrush only — nothing else.  Ramps with bars: 4." FX 0

look_at fx
wait_seconds 30

# Stage 3: full chaos.
ask "FX: final stage.  Bitcrush cranked: wet 0.85+, bit_depth_reduction 0.90, sample_rate_reduction 0.85.  Reverb_mix to 0.70 with reverb_size close to 1.0 for a cavernous tail.  Reverb + bitcrush ONLY.  Ramps with bars: 2 or 4 for a faster collapse into the final bars." FX 0

look_at fx
wait_seconds 30

# ── Scene 5: End ────────────────────────────────────────────────────────────
# chain_loop=false stops the transport on its own at the end of the
# last bank.  `stop` defensively; the End-scene cleanup (stop_pan_lfo)
# sweeps any stray python loops the scenario didn't start, just in
# case a prior run left one.

scene "End"

stop
stop_pan_lfo

say "Bach, full stop. Eighteenth-century counterpoint, played by two acid basses, smeared by a bitcrusher — all driven by local AI."
wait_seconds 2
