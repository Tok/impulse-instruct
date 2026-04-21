#!/usr/bin/env bash
# ─── J.S. Bach — Italian Concerto III (MIDI score import) ───────────────────
# Plays a scrubbed MIDI of Bach's Italian Concerto 3rd movement on two 303
# voices, start to end (~3:30 at the file's 240 BPM / 8th-note grid).
# No narration during playback — the prompt console and the UI carry the
# story while the music plays.  Agents come in staggered:
#
#   t=0s        Setup + pre-play explanation
#   t=play      Bach starts; PAD agent drifts the 303 filter in the top-left
#               corner of the res/cutoff pad
#   t=play+60s  808 arrives with a KIT agent on very sparse 4otf (kick
#               every 4, CHH between, ghost variations)
#   t=end-45s   FX chaos — bitcrusher + FX agent escalates into the outro
#   t=end       chain_loop=false on the import stops the transport on
#               its own; we narrate a short outro

scene_count 6

BACH_MIDI="${BACH_MIDI:-demo/scenarios/bach-italian-3rd.mid}"

# ── Scene 1: Setup + explanation ────────────────────────────────────────────

scene "Setup"

reset_all
# Keep the global style empty — this is a score-driven scenario, the LLM
# shouldn't colour the prompts with a style brief unless we explicitly
# ask for one.  (Agents still run; they just take their instructions
# from our per-ask prompts.)
set_style ""

add_instrument bass

say "J.S. Bach — Italian Concerto, third movement. Two voices: right hand and left hand. Both played by a 303-style bass."

# Import the scrubbed score.  Populates pattern_bank[0..banks_used-1]
# (up to 64 slots), chain walks through them once (chain_loop=false) so
# playback stops at the end instead of looping.
if ! load_midi "$BACH_MIDI"; then
    say "The MIDI file didn't load. Check that the app is running and the file exists."
    stop
    exit 1
fi

say "The score runs about ${MIDI_DURATION%.*} seconds. During playback: first a filter agent drifts the 303, then drums arrive at sixty seconds, and a bitcrusher takes over at the end."

# Per-voice timbre.  Voice 0 (right hand) brighter and L-panned; voice
# 1 (left hand) darker and R-panned.  Volumes land at 1.0 each so we
# can later halve the drums to keep Bach in the centre of the mix.
api_params '{
  "bass_voices": [
    { "enabled": true, "volume": 1.0, "cutoff": 0.55, "resonance": 0.35, "env_mod": 0.45, "decay": 0.40, "accent_level": 0.5, "distortion": 0.10, "pan": -0.25 },
    { "enabled": true, "volume": 1.0, "cutoff": 0.32, "resonance": 0.55, "env_mod": 0.35, "decay": 0.65, "accent_level": 0.4, "distortion": 0.05, "pan":  0.25 }
  ]
}'

# Lock the per-step note fields so the PAD agent below can't overwrite
# the imported score via sequencer updates.  Cutoff/resonance stay
# unlocked — that's exactly what we want the agent drifting.
lock "bass_voices.0.note" "bass_voices.1.note" "sequencer.bass_pattern" "sequencer.bass_patterns"

wait_seconds 1

# ── Scene 2: Play + PAD agent ───────────────────────────────────────────────

scene "Play"

look_at sequencer
play

# PAD agent: scope "bass" so it writes into bass_voices only.  We ask
# for slow drifts in the top-left of the cutoff/resonance pad (low
# cutoff, high resonance) — the classic acid squelch corner applied to
# a baroque line.  Agent runs in jam mode (one_shot=false) so it keeps
# firing on every jam loop while the tune plays.
add_agent PAD gemma bass
wait_for_model

ask "PAD: over the next few bars, nudge both bass voices toward the top-left of the filter pad — cutoff around 0.15 to 0.30, resonance 0.85 to 0.98.  Keep them close to each other (within 0.1) so the two hands read as one instrument.  DO NOT touch notes, steps, gate, accent, slide, or pan — filter only.  Make very small moves every response, not big jumps." PAD 0

# ── Scene 3: Drums arrive (+60s) ───────────────────────────────────────────
# Sixty seconds into the piece we layer an 808 with an extremely sparse
# 4-on-the-floor pulse underneath.  Bank count is ~53; chain_advance
# for this import uses the preserve-non-bass path (set when
# chain_loop=false), so the KIT agent's drum writes survive each 4-second
# bank swap until playback ends.

scene "Drums arrive"

wait_seconds 60

add_instrument 808
# Bass twice as loud as drums — the whole point is Bach in the centre.
# volume is 0..1 per voice / module.
api_params '{"kit_a": {"volume": 0.4}}'

add_agent KIT gemma "kit_a"

# Very sparse prompt — 4otf basics with one or two creative ghosts.
# The kit scope includes every 808 voice (kick/snare/hats/toms), but
# we steer the agent to keep it minimal.
ask "KIT: extremely sparse four-on-the-floor under Bach.  Kit_a only.  Kick on every 4th step (0, 4, 8, 12 in a 16-step loop).  Closed hi-hat between kicks (steps 2, 6, 10, 14).  NO snare on 2 and 4 — this is not pop.  Add one or two ghost hits somewhere unusual for texture (a quiet hat on a weird step, or a single tom ping).  Everything quiet (velocity ~0.4 max).  Stay out of the bass's way." KIT 0

# ── Scene 4: Bitcrush takes over (near end) ─────────────────────────────────

scene "Bitcrush takeover"

# Sleep until there's ~45 s of piece left, then start the FX escalation.
# The total piece is MIDI_DURATION; we've already burned 60s + the
# wait for the agent calls above (negligible vs the wait_seconds).
# Use bc for float math so durations like 212.0 don't barf.
FX_START_OFFSET=$(echo "$MIDI_DURATION - 60 - 45" | bc)
wait_seconds "$FX_START_OFFSET"

add_effect bitcrush
add_agent FX gemma fx

# Escalate bitcrush wet over the tail.  The FX agent can touch any fx
# field; we focus it on bitcrush specifically.
ask "FX: over the next thirty seconds, slowly crank the bitcrush wet mix from almost nothing (0.1) to almost full (0.9).  Also raise bit_depth_reduction and sample_rate_reduction toward their nastier ends.  This is the outro — go crazy.  Leave reverb/delay alone." FX 0

wait_seconds 45

# ── Scene 5: End ────────────────────────────────────────────────────────────
# The chain was built with chain_loop=false so the audio thread stops
# on its own at the end of the last bank.  We stop() defensively in
# case we're a hair early, then fade out the narration.

scene "End"

stop

# Brief outro — the music's over, narration is welcome again.
say "Bach, full stop.  Scored in the eighteenth century, imported as MIDI, played by two acid basses, smeared by a bitcrusher, all driven by local AI agents."
wait_seconds 2
