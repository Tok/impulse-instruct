#!/usr/bin/env bash
# ─── Multi-Agent Demo ───────────────────────────────────────────────────────
# Three specialist agents with scoped control.

scene_count 7

# ── Scene 1: Setup ──────────────────────────────────────────────────────────

scene "Setup"

reset_rack

add_instrument bass
add_instrument 808
add_instrument 909
add_effect reverb
add_effect delay

wait_seconds 1

# ── Scene 2: Add agents ────────────────────────────────────────────────────

scene "Adding specialist agents"

say "Three agents, each scoped to specific instruments."

add_agent BASS bonsai bass
add_agent DRUMS bonsai "kit_a,kit_b"
add_agent FX bonsai fx

wait_for_model

say "Each runs Bonsai. They share one model server."

# ── Scene 3: Show wiring ──────────────────────────────────────────────────

scene "Control cables"

show_cables
look_at console
wait_seconds 2

say "Each agent has separate control cables to its scoped modules."

look_at bass
wait_seconds 2

show_knobs

# ── Scene 4: Independent control ──────────────────────────────────────────

scene "Independent prompts"

play
wait_seconds 1

ask "acid bass line, squelchy" BASS
ask "four on the floor, off-beat hats, clap on two and four" DRUMS
ask "subtle reverb and short delay" FX

say "Each agent only modified its own instruments."
wait_seconds 3

# ── Scene 5: Scoped changes ──────────────────────────────────────────────

scene "Scoped changes"

focus_on bass

ask "more resonance, darker" BASS

say "Bass changed. Drums and FX untouched."
wait_seconds 2

# ── Scene 6: Same direction, different results ────────────────────────────

scene "Unified direction"

show_all

ask "more energy" BASS
ask "more energy" DRUMS
ask "bigger space" FX 10

say "Same direction to all agents. Each interprets it for its domain."
wait_seconds 4

# ── Scene 7: End ────────────────────────────────────────────────────────────

scene "End"

say "Multi-agent mode. Scoped control, independent evolution, shared model server."

stop
