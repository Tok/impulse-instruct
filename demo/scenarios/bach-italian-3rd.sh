#!/usr/bin/env bash
# ─── J.S. Bach — Italian Concerto III (MIDI score import) ───────────────────
# Plays a 2-voice piano MIDI of Bach's third movement on two 303 voices
# at its intended 240 BPM, start to end (~3:30 of playback).
#
# The 240 is brutal if the drums match it, so the KIT agent's prompt
# asks for half-density patterns: kicks only on steps 0 + 8 of a
# 16-step loop, hats on 4 + 12.  That puts the drums at effectively
# 120-BPM pulse under a 240-BPM bass, which is the intended feel.
#
# No narration during playback — the prompt console and the filter /
# drum / FX panels carry the story.  Agents come in staggered, and
# the scenario's role is to set up the scene and kick each agent off —
# the AI does the actual parameter motion (filter drift, pan moves,
# drum patterns, FX escalation).  No scripted sine loops from the
# scenario side.
#
#   t=play         PAD agent drifts cutoff / resonance / pan on both
#                  303 voices across the outro of the movement
#   t=play+60s     808 arrives; KIT agent lays a half-density 4otf
#                  under the bass
#   t=end-90s..end Three staggered FX asks escalate bitcrush + reverb +
#                  delay into a final-30s freak-out
#   t=end          chain_loop=false stops the transport on its own; a
#                  short outro line plays

scene_count 6

BACH_MIDI="${BACH_MIDI:-demo/scenarios/bach-italian-3rd.mid}"

# ── Scene 1: Setup + explanation ────────────────────────────────────────────

scene "Setup"

reset_all
set_style ""

# Bach-specific Huth-per-component pinning: grayscale piano (the 303 is
# the expressive voice, not the keyboard chrome), Huth-coloured
# spectrum bars so the harmonic make-up of the counterpoint reads as
# colour, oscilloscopes left grayscale.  Sequencer step dots and the
# event-stream history are Huth-coloured unconditionally.
set_ui_prefs '{"huth_piano": false, "huth_bar_osc": false, "huth_ring_osc": false, "huth_spectrum": true, "show_thinking_in_log": true, "enable_thinking": true}'

# Master volume tuned down from the default 0.85 — the Bach demo's
# resonance-heavy filter sweeps + bitcrush outro push peak loudness
# into clip territory at the default gain.  0.60 leaves headroom for
# the FX escalation without tripping the CLIPPING warning.
api_params '{"fx":{"master_volume":0.60}}'

add_instrument bass

# Scroll to the AI console AFTER the bass is added — otherwise the
# rack's zone layout shifts when the voice zone inflates for the bass
# module, and a pre-add look_at lands on the wrong y-position and the
# console can fall off-screen or sit under a zone boundary.
look_at ai

say "Bach's Italian Concerto, third movement. Two acid basses, two hands."

# Import the score.  Populates pattern_bank[0..banks_used-1], chain
# walks them once (chain_loop=false), bass voice 0 gets the right hand,
# voice 1 the left hand.  Sets shell vars MIDI_BPM / MIDI_DURATION /
# MIDI_BANKS_USED etc. from the import summary.
if ! load_midi "$BACH_MIDI"; then
    say "The MIDI file didn't load. Check that the app is running and the file exists."
    stop
    exit 1
fi

# Bach plays at the file's native 240 BPM — halfstepping the tempo
# down slowed the whole piece including the bass, so instead we keep
# the bass at 240 and sparsify the drums (KIT prompt below) so they
# feel half-time under the running bass line.
PLAY_DURATION="$MIDI_DURATION"
PLAY_DURATION_INT=${PLAY_DURATION%.*}
echo "  [scenario] will play for ${PLAY_DURATION_INT}s at ${MIDI_BPM} BPM"

# Per-voice starting timbre.  Voice 0 (right hand) slightly brighter
# and panned a touch left; voice 1 (left hand) darker and panned a
# touch right.  These are starting values only — the PAD agent will
# drift cutoff / resonance / pan from here.
api_params '{
  "bass_voices": [
    { "enabled": true, "volume": 1.0, "cutoff": 0.55, "resonance": 0.35, "env_mod": 0.45, "decay": 0.40, "accent_level": 0.5, "distortion": 0.10, "pan": -0.25 },
    { "enabled": true, "volume": 1.0, "cutoff": 0.32, "resonance": 0.55, "env_mod": 0.35, "decay": 0.65, "accent_level": 0.4, "distortion": 0.05, "pan":  0.25 }
  ]
}'

# Lock the imported score's notes / patterns so the PAD agent can't
# overwrite them via sequencer updates.  Everything else — cutoff,
# resonance, pan, volume, accent, distortion — stays unlocked and the
# agent is free to move it.  The scenario's job is setup; the motion
# is the agent's.
lock "bass_voices.0.note" "bass_voices.1.note" "sequencer.bass_pattern" "sequencer.bass_patterns"

wait_seconds 1

# ── Scene 2: Play + PAD agent + pan LFO ─────────────────────────────────────

scene "Play"

look_at sequencer
play

add_agent PAD gemma bass
wait_for_model

# Wide roam across the filter pad + stereo pan: PAD owns cutoff,
# resonance, and pan for both voices.  The scenario is deliberately
# NOT scripting a sine LFO — the whole point of the demo is that the
# AI moves the knobs, so we give the agent explicit permission to
# drift pan too (and to do it in anti-phase between the two voices
# when it feels right).
ask "PAD: drift both bass voices across the filter pad and the stereo field over the movement.  Cutoff anywhere in 0.05 to 0.55, resonance 0.70 to 1.0.  Keep the two voices within 0.12 of each other on both filter axes so the counterpoint still reads as one instrument.  Pan range -0.8..+0.8, with voice 0 and voice 1 drifting in OPPOSITE directions (voice 0 left while voice 1 right, then swap).

Use \`ramp\` / \`ramps\` for the motion — NOT step-jump writes.  Each ramp should span 4 or 8 bars so the filter reads as evolving across multiple pattern cycles, not twitching per jam cycle.  Example shape:

  {\"ramps\":[
    {\"param\":\"bass.cutoff\",\"to\":0.18,\"bars\":8},
    {\"param\":\"bass.resonance\",\"to\":0.92,\"bars\":8},
    {\"param\":\"bass.pan\",\"to\":-0.6,\"bars\":8},
    {\"param\":\"bass_voices.1.cutoff\",\"to\":0.22,\"bars\":8},
    {\"param\":\"bass_voices.1.resonance\",\"to\":0.88,\"bars\":8},
    {\"param\":\"bass_voices.1.pan\",\"to\":0.6,\"bars\":8}
  ]}

On each next cycle, re-target to new values at the other corner of the pad (and swap pan directions again) so the motion reads as a continuous sweep.  DO NOT touch notes, steps, gate, accent, slide, volume, distortion, or env_mod." PAD 0

# ── Scene 3: Drums arrive (+60s) ───────────────────────────────────────────
# At +60s we layer an 808 with a very sparse pulse underneath.
# chain_advance_preserve_non_bass carries drum patterns across bank
# swaps when chain_loop=false, so the KIT agent's writes survive every
# bank transition until playback ends.

scene "Drums arrive"

wait_seconds 60

add_instrument 808
# Bass 2× drums — Bach stays centre-stage.  Tuned well below the intro
# mix because the filter sweep can get loud on resonance peaks.
api_params '{"kit_a": {"volume": 0.2}}'

add_agent KIT gemma "kit_a"

ask "KIT: half-time drums under a fast Bach at 240 BPM.  Kit_a only, 16-step loop.  Kick on steps 0 and 8 ONLY (not every 4 — that's too dense at this tempo).  Closed hi-hat on steps 4 and 12 for the between-kick accents.  NO snare on 2 and 4.  Add one or two ghost hits somewhere unusual for texture (a quiet hat on a weird step, or a single tom ping).  Everything quiet (velocity ~0.4 max).  The drums should feel half the speed of the bass — a slow pulse under a running melodic line, not a dance beat." KIT 0

# ── Scene 4: FX takeover (end - 90s) ────────────────────────────────────────
# Three staggered asks, ~30 seconds apart, so the escalation reads as
# a progression across the outro rather than a single step-jump.  The
# FX agent is allowed to touch anything — reverb and delay are fair
# game for the final chaos.

scene "FX takeover"

# Time until the FX section starts.  We've burned ~60s + the drums
# setup (negligible next to the waits) so the remaining playback is
# PLAY_DURATION - 60; we want 90 s of FX runway before the end.
FX_START_OFFSET=$(echo "$PLAY_DURATION - 60 - 90" | bc -l)
wait_seconds "$FX_START_OFFSET"

add_effect bitcrush
add_agent FX gemma fx

# Stage 1 (end - 90s): light bitcrush, open up a bit of reverb tail.
ask "FX: the outro starts now.  Turn bitcrush on at a low setting (wet around 0.2, bit_depth_reduction around 0.3, sample_rate_reduction around 0.3).  Nudge reverb_mix up a touch (to around 0.25) and lengthen reverb_size a bit.  Leave delay subtle.  This is stage one of three — keep room to escalate." FX 0

wait_seconds 30

# Stage 2 (end - 60s): dirtier — push bitcrush higher, bring in delay.
ask "FX: stage two.  Push bitcrush harder — wet around 0.55, bit_depth_reduction around 0.65, sample_rate_reduction around 0.6.  Raise delay_mix to around 0.3 with a short delay_time for slapback.  Keep reverb where it is or raise another small step.  Stay musical — this is a Bach outro losing its composure, not static." FX 0

wait_seconds 30

# Stage 3 (end - 30s): everything maxed — the tune's closing freak-out.
ask "FX: final stage.  Go crazy.  Bitcrush almost full (wet 0.85+, bit_depth_reduction and sample_rate_reduction near 0.9).  Reverb wet high (0.6+) with a long tail.  Delay mixed in thick (0.5+) with feedback if available.  Everything.  The last 30 seconds of a piece that's already decomposing." FX 0

wait_seconds 30

# ── Scene 5: End ────────────────────────────────────────────────────────────
# chain_loop=false on the import stops the transport on its own at the
# end of the last bank.  We stop defensively (in case timing drifted)
# and narrate a short outro line.

scene "End"

stop

say "Bach, full stop. Eighteenth-century counterpoint, played by two acid basses, panned against each other, and smeared by a bitcrusher — all driven by local AI."
wait_seconds 2
