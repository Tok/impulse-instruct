#!/usr/bin/env bash
# ─── Ramp Demo ──────────────────────────────────────────────────────────────
# Parameter ramps for gradual changes over bars.

scene_count 7

# ── Scene 1: Setup ──────────────────────────────────────────────────────────

scene "Setup"

reset_rack

add_instrument bass
add_instrument 808
add_effect reverb
add_effect delay

add_agent RAMP gemma
wait_for_model

set_bpm 128

# ── Scene 2: Starting point ────────────────────────────────────────────────

scene "Starting pattern"

look_at sequencer

play
wait_seconds 1

ask "Style is acid at 128 BPM. Kick on steps 0,4,8,12,16,20,24,28. Bass line with 3-4 distinct scale pitches spread across both halves, cutoff low, resonance moderate."

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

# ── Scene 6: LFO modulation ─────────────────────────────────────────────────

scene "LFO modulation"

add_effect lfo
look_at lfo
wait_seconds 1

say "An LFO cycles a knob automatically. Any parameter is a target."

# Sine on the bass cutoff — classic acid breath.
api_params '{"lfo": [{"enabled": true, "target": "BassCutoff", "waveform": "Sine", "rate": 0.18, "depth": 0.55}]}'

focus_on bass
wait_seconds 5

# Release the LFO before the outro so the final state is clean.
api_params '{"lfo": [{"enabled": false}]}'
wait_seconds 1

# ── Scene 7: End ────────────────────────────────────────────────────────────

scene "End"

say "Ramps for one-shot moves over bars. LFOs for continuous cycling. Two tools, same targets."

stop
