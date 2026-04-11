#!/usr/bin/env bash
# ─── Ambient Demo ───────────────────────────────────────────────────────────
# Slow pads, long reverb, gentle filter movement.

scene_count 6

# ── Scene 1: Setup ──────────────────────────────────────────────────────────

scene "Setup"

reset_rack

add_instrument bass
add_instrument an1x
add_effect reverb
add_effect delay

say "Bass for drones, AN1X for pads, reverb and delay."

add_agent AMBIENT gemma
wait_for_model

# ── Scene 2: Sparse foundation ─────────────────────────────────────────────

scene "Sparse foundation"

look_at sequencer

play
wait_seconds 1

ask "80 BPM, very sparse, just a single bass drone note per bar"

say "One note per bar. Ambient needs space."
wait_seconds 3

# ── Scene 3: Pad layer ─────────────────────────────────────────────────────

scene "Pad texture"

focus_on an1x

ask "warm pad, slow attack, long release, gentle chords"

say "The AN1X pad fills the space between the bass notes."
wait_seconds 4

# ── Scene 4: Deep effects ──────────────────────────────────────────────────

scene "Deep reverb"

focus_on reverb

ask "push the reverb deep, long tail, lots of delay feedback, washy"

say "Heavy reverb and delay. Sounds blur into each other."
wait_seconds 4

# ── Scene 5: Evolution ────────────────────────────────────────────────────

scene "Filter evolution"

focus_on bass

ask "slowly evolve the bass, open the filter over 8 bars" 16

say "Gradual filter movement over eight bars."
wait_seconds 3

# ── Scene 6: End ────────────────────────────────────────────────────────────

scene "End"

say "Ambient workflow. Sparse patterns, deep effects, slow ramps."

stop
