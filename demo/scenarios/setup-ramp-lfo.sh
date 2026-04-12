#!/usr/bin/env bash
# ─── Ramp Demo ──────────────────────────────────────────────────────────────
# Parameter ramps for gradual changes over bars.

scene_count 6

# ── Scene 1: Setup ──────────────────────────────────────────────────────────

scene "Setup"

reset_rack

add_instrument bass
add_instrument 808
add_effect reverb
add_effect delay

add_agent RAMP gemma
wait_for_model

# ── Scene 2: Starting point ────────────────────────────────────────────────

scene "Starting pattern"

look_at sequencer

play
wait_seconds 1

ask "basic acid pattern, kick on every beat, bass line, filter low"

focus_on bass

say "Static pattern. Filter is fixed. Ramps add movement."
wait_seconds 3

# ── Scene 3: Single ramp ──────────────────────────────────────────────────

scene "Single ramp"

ask "slowly open the filter over 4 bars" "" 14

say "Filter ramped over four bars. Visible on the knob."
wait_seconds 2

# ── Scene 4: Multiple ramps ──────────────────────────────────────────────

scene "Multiple simultaneous ramps"

ask "bring the filter back down over 4 bars while increasing resonance and fading in reverb" "" 16

say "Three parameters changing independently."
wait_seconds 2

# ── Scene 5: Build and drop ──────────────────────────────────────────────

scene "Build and drop"

show_all

ask "build up over 8 bars, open everything, more delay, more reverb" "" 18

say "Build complete."
wait_seconds 1

ask "drop, close the filter, punch it, strip the effects" "" 10

say "Instant contrast. Build-and-drop using ramps versus direct values."
wait_seconds 3

# ── Scene 6: End ────────────────────────────────────────────────────────────

scene "End"

say "Ramp system. Gradual changes, multiple parameters, build-drop dynamics."

stop
