#!/usr/bin/env bash
# ─── Drum & Bass (Jump-up) Demo ──────────────────────────────────────────────
# 174 BPM, syncopated two-step, rolling 16th hats, AN1X pitched bass stabs,
# the drum break is regenerated twice during the demo, and an MC rides the
# tail of the clip.

scene_count 9

# ── Scene 1: Intro ──────────────────────────────────────────────────────────
# Opens the clip with context for viewers landing on YouTube cold — what the
# app is, what style we're building, and the fact that the LLM writes the
# patterns live.

scene "Intro"

reset_all
set_style drum_and_bass

say "Impulse Instruct — a synthesizer with a local AI producer. Today: jump-up drum and bass at 174 BPM."
wait_seconds 1

# ── Scene 2: Setup ──────────────────────────────────────────────────────────

scene "Setup"

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

# ── Scene 3: First break ────────────────────────────────────────────────────

scene "First break"

look_at sequencer

play
wait_seconds 1

# Intent-driven prompt — style seed handles the exact step positions.
ask "set up classic jump-up drums on kit_a: kick on beat 1 and just before beat 3.5 (syncopated two-step), big snare on 2 and 4, rolling 16th hats the whole bar with open-hat accents on the off-beats. velocity variation so the hats breathe."

say "Syncopated two-step. Rolling hats drive the bar."
wait_seconds 3

# ── Scene 4: AN1X stab line ─────────────────────────────────────────────────

scene "AN1X bass stabs"

focus_on an1x

ask "AN1X as the lead-and-bass: tuned piano-like tone (detuned saws, NO hard sync, filter open around 0.6, resonance around 0.25 — warm, not abrasive), snappy amp envelope but notes still musical. A minor pentatonic, 5 or 6 distinct pitches across the 32-step loop, syncopated not on every downbeat. Write a singable melodic line, not a drone. Pan center."

say "AN1X tuned like a piano — melodic, not a reese drone."
wait_seconds 3

# ── Scene 5: Break regeneration #1 ──────────────────────────────────────────
# First of two explicit break regenerations — ask the agent for a fresh drum
# pattern that keeps the style but rewrites the hits.

scene "Fresh break"

look_at sequencer

ask "rewrite the kit_a drums for variety — keep the jump-up feel and rolling hats, but shift the kick placement and add a snare ghost hit or fill. same tempo, same energy." "" 10

say "The agent rewrites the break. Same style, different phrasing."
wait_seconds 4

# ── Scene 6: Build ──────────────────────────────────────────────────────────

scene "Build the pressure"

look_at console

# Deterministic FX levels — don't ask the agent for dB values.
api_params '{"fx": {"reverb_mix": 0.12, "reverb_size": 0.4, "delay_mix": 0.1, "delay_time": 0.1875, "stereo_width": 0.65}}'

ask "slowly open the AN1X filter over 4 bars, start near 0.3 and ramp to 0.8, keep resonance around 0.55 so it sings" "" 14

say "Filter ramp on the stabs. Tension builds into the drop."
wait_seconds 3

# ── Scene 7: Drop + break regeneration #2 ───────────────────────────────────
# The drop also regenerates the drum break — denser, harder, fill-heavy.

scene "Drop"

show_all

ask "drop — rewrite the kit_a drums harder and denser: 32nd-note hat rolls on the off-beats, a proper amen-break-style snare fill before the bar loops, punchier kick velocities, AN1X stabs with more octave jumps. Keep it musical, not cluttered." "" 12

say "Second rewrite of the break. Denser, harder, fill-heavy."
wait_seconds 4

# ── Scene 8: MC ─────────────────────────────────────────────────────────────
# Prompt PULSE to add + wire the MC rather than hard-coding it through the
# API — this is what the app's meant to feel like: tell the producer what
# you want, the producer sets it up.  PULSE emits a spawn_agent action with
# mode=mc and (implicitly) tts=true, which adds a NeuTts module, wires a
# control cable from the new MC agent to it, and auto-scrolls to the TTS.
#
# We split this into two asks:
#   1. PULSE spawns the MC agent + TTS module.
#   2. A second ask is sent directly to the MC, which emits the mc_line
#      field (only MC-mode agents produce those) and the in-app NeuTTS
#      pipeline synthesizes + plays it.
# Combining both into one ask to PULSE left the shout as a narration
# instead of an actual synthesized line — only MC-mode agents trigger TTS.

scene "MC on the mic"

look_at console

ask "spawn an MC agent, jump-up rave flavor, MC mode with TTS voice cloning" "" 8

# Give the spawn + TTS module creation a moment to apply, and for the UI
# to scroll to the new TTS module.
wait_seconds 4

# Now send the shout request directly to the MC agent — only MC-mode
# agents emit mc_line, which is what triggers NeuTTS playback.
ask "drop a single short jump-up shout-out, one line, peak-time rave energy" MC 10

say "PULSE spawned the MC. MC rides the track."

# In-app NeuTTS synthesis takes several seconds per line AFTER the LLM
# produces the MC text, then audio has to play through.  Give the pipeline
# at least 30s before stop() lands — cold-path NeuTTS is slow on the first
# line, and earlier cuts had stop() trim the shout.
wait_seconds 30

# ── Scene 9: End ────────────────────────────────────────────────────────────

scene "End"

say "Jump-up D&B at 174 — two break rewrites, AN1X stabs, MC on top. Everything ran locally on one GPU."
wait_seconds 3

stop
