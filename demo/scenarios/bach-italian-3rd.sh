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
# drum / FX panels carry the story.  Agents come in staggered:
#
#   t=play         PAD agent drifts the filter across a wide swath of
#                  the res/cutoff pad; a background sine LFO pans voice
#                  0 and voice 1 in anti-phase
#   t=play+60s     808 arrives; KIT agent lays a half-density 4otf
#                  under the bass
#   t=end-90s..end Three staggered FX asks escalate bitcrush + reverb +
#                  delay into a final-30s freak-out
#   t=end          chain_loop=false stops the transport on its own; pan
#                  LFO killed in Scene 5 and a short outro line plays

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

# Per-voice starting timbre.  Voice 0 (right hand) slightly brighter;
# voice 1 (left hand) slightly darker.  Pan will be overwritten every
# 50ms by the background LFO below, so the static values here are just
# initial conditions.
api_params '{
  "bass_voices": [
    { "enabled": true, "volume": 1.0, "cutoff": 0.55, "resonance": 0.35, "env_mod": 0.45, "decay": 0.40, "accent_level": 0.5, "distortion": 0.10, "pan": 0.0 },
    { "enabled": true, "volume": 1.0, "cutoff": 0.32, "resonance": 0.55, "env_mod": 0.35, "decay": 0.65, "accent_level": 0.4, "distortion": 0.05, "pan": 0.0 }
  ]
}'

# Lock notes/patterns so the PAD agent can't overwrite the imported
# score via sequencer updates.  Pan is deliberately NOT locked — the
# 20 Hz sine LFO overwrites any occasional agent pan writes within a
# frame, and locking pan would also block our own api_params sine.
lock "bass_voices.0.note" "bass_voices.1.note" "sequencer.bass_pattern" "sequencer.bass_patterns"

wait_seconds 1

# ── Scene 2: Play + PAD agent + pan LFO ─────────────────────────────────────

scene "Play"

look_at sequencer
play

# Anti-phase pan LFO: voice 0 sweeps left-centre-right while voice 1
# does the opposite.  Rate 0.15 Hz ≈ 6.6-second cycle; depth 0.65 so
# the voices cross centre without hitting the hard edges.
start_opposing_pan_lfo 0.15 0.65

add_agent PAD gemma bass
wait_for_model

# Wider-than-before filter roam: cutoff 0.05..0.55 opens a lot more
# of the lowpass sweep, and the resonance range keeps the acid bite.
# We still keep the voices close to each other so the two hands read
# as one instrument, and explicitly block pan writes (the LFO owns pan).
ask "PAD: drift both bass voices across the filter pad freely — cutoff anywhere in 0.05 to 0.55, resonance 0.70 to 1.0.  Go as low as 0.05 on cutoff for squelchy dives, as high as 0.55 for opened-up breaths.  Move in bigger steps than usual (changes of 0.1-0.2 per response are fine — this is the outro of a Bach movement, not a meditation app).  Keep voice 0 and voice 1 within 0.12 of each other on both axes.  DO NOT touch notes, steps, gate, accent, slide, pan, or volume — filter axes only." PAD 0

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
# end of the last bank.  We stop defensively (in case timing drifted),
# kill the pan LFO, and narrate a short outro line.

scene "End"

stop
stop_pan_lfo

say "Bach, full stop. Eighteenth-century counterpoint, played by two acid basses, panned against each other, and smeared by a bitcrusher — all driven by local AI."
wait_seconds 2
