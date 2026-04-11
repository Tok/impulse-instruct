#!/usr/bin/env bash
# ─── Raw Acid Demo ──────────────────────────────────────────────────────────
# 303 squelch, 808 percussion, filter sweeps, parameter locking.

scene_count 8

# ── Scene 1: Setup ──────────────────────────────────────────────────────────

scene "Setup"

reset_rack

say "Raw acid. Three oh three, eight oh eight, effects."

add_instrument bass
add_instrument 808
add_effect reverb
add_effect delay

add_agent ACID gemma
wait_for_model

# ── Scene 2: Basic pattern ──────────────────────────────────────────────────

scene "Basic pattern"

look_at sequencer

play
wait_seconds 1

ask "four on the floor kick, off-beat hats, simple bass root notes"

say "Foundation pattern. The agent chose the note placement and timing."
wait_seconds 3

# ── Scene 3: Make it acid ───────────────────────────────────────────────────

scene "Adding squelch"

focus_on bass

say "The three oh three filter creates the acid sound. We just describe what we want."

ask "make it acid and squelchy, syncopated bass line"

say "The agent adjusted cutoff, resonance, and accent. We didn't specify values."
wait_seconds 3

screenshot "v${VERSION}-acid-bass"

# ── Scene 4: Filter sweep ──────────────────────────────────────────────────

scene "Filter ramp"

say "Ramps change parameters gradually over bars instead of jumping."

ask "slowly open the filter over 4 bars" 14

say "The cutoff ramped from its current value. Visible on the knob in real time."
wait_seconds 2

# ── Scene 5: Percussion ────────────────────────────────────────────────────

scene "Percussion"

focus_on 808

ask "add open hats and a clap, more groove"

say "The agent filled in percussion. Each run produces different results."
wait_seconds 3

# ── Scene 6: Parameter locking ──────────────────────────────────────────────

scene "Parameter locking"

focus_on bass

say "Locking prevents the agent from changing specific parameters."

lock "sequencer.bass_steps" "tb303.cutoff" "tb303.resonance"

ask "strip the drums back, more minimal, add reverb"

say "Drums changed. Bass stayed locked."
wait_seconds 2

unlock "sequencer.bass_steps" "tb303.cutoff" "tb303.resonance"

# ── Scene 7: Push it ────────────────────────────────────────────────────────

scene "Full energy"

show_all
look_at sequencer

ask "raw acid, maximum squelch, driving, aggressive" 12

wait_seconds 4

# ── Scene 8: Outro ──────────────────────────────────────────────────────────

scene "End"

say "That covers the acid workflow. Filter control, ramps, and parameter locking."

stop
