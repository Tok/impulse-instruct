#!/usr/bin/env bash
# ─── Drum & Bass (Jump-up) Demo ──────────────────────────────────────────────
# 174 BPM, syncopated two-step, rolling 16th hats, AN1X pitched bass stabs,
# MC shout-outs near the end.

scene_count 7

# ── Scene 1: Setup ──────────────────────────────────────────────────────────

scene "Setup"

reset_all
set_style drum_and_bass

# AN1X carries the bass line as pitched stabs — no 303 bass voice this time.
add_instrument an1x
add_instrument 808
add_instrument 909
add_effect reverb
add_effect delay

say "AN1X as the bass, two drum kits, reverb, delay."

add_agent DNB gemma
wait_for_model

set_bpm 174

# ── Scene 2: Two-step drums ────────────────────────────────────────────────

scene "Syncopated two-step"

look_at sequencer

play
wait_seconds 1

# No numeric step micromanagement — style seed covers it.  Keep the prompt
# intent-driven so the agent spreads activity across the full 32-step bar.
ask "set up classic jump-up drums on kit_a: kick on beat 1 and just before beat 3.5 (syncopated two-step), big snare on 2 and 4, rolling 16th hats the whole bar with open-hat accents on the off-beats. velocity variation so the hats breathe."

say "Syncopated two-step. Rolling hats drive the bar."
wait_seconds 3

# ── Scene 3: AN1X stab line ────────────────────────────────────────────────

scene "AN1X bass stabs"

focus_on an1x

ask "AN1X bass stabs, short punchy envelope (hard sync on, saw pair slightly detuned), A minor pentatonic, 5 or 6 distinct pitches across the 32-step loop, syncopated not on every downbeat. Think pitched figures that jump and talk — not a sustained drone. Pan center."

say "Pitched bass stabs instead of a sustained reese. Short and punchy."
wait_seconds 3

# ── Scene 4: Build ─────────────────────────────────────────────────────────

scene "Build the pressure"

look_at console

# Deterministic FX levels — don't ask the agent for dB values.
api_params '{"fx": {"reverb_mix": 0.12, "reverb_size": 0.4, "delay_mix": 0.1, "delay_time": 0.1875, "stereo_width": 0.65}}'

ask "slowly open the AN1X filter over 4 bars, start near 0.3 and ramp to 0.8, keep resonance around 0.55 so it sings" "" 14

say "Filter ramp on the stabs. Tension builds into the drop."
wait_seconds 3

# ── Scene 5: Drop ──────────────────────────────────────────────────────────

scene "Drop"

show_all

ask "full energy drop — denser hats with 32nd-note rolls on off-beats, punchier kick velocities, AN1X stabs with more octave jumps. Keep it musical, not cluttered." "" 12

wait_seconds 4

# ── Scene 6: MC ────────────────────────────────────────────────────────────

scene "MC on the mic"

# Dedicated MC agent with TTS enabled — gemma for coherent phrasing.  The
# style's mc_lines seed the flavor (jump-up, rewinds, big up the ravers).
add_agent MC gemma "" mc tts
wait_seconds 3

focus_on console

# In-app NeuTTS synthesis takes several seconds per line AFTER the LLM
# produces the MC text, then audio has to play through.  Give the pipeline
# at least 30s before moving on — cold-path NeuTTS synthesis can be slow
# on the first line, and the prior scenario cut stop() before the shout
# was audible.
ask "drop a short MC shout-out over the current pattern — one line, jump-up rave flavor" MC 8

say "MC rides the track."
wait_seconds 30

# ── Scene 7: End ───────────────────────────────────────────────────────────

scene "End"

say "Jump-up D&B at 174. Pitched stabs, rolling hats, MC on top."
wait_seconds 2

stop
