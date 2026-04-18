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

# All agents share a single Gemma 4 E4B server (ref-counted in LlamaServerPool),
# so a 3-agent band costs the same VRAM as a single Solo agent.
add_agent BASS gemma bass
add_agent DRUMS gemma "kit_a,kit_b"
add_agent FX gemma fx

wait_for_model

set_bpm 128

say "Bass, drums and FX all on Gemma. One server, ref-counted."

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

ask "acid bass line, squelchy, 4 distinct scale pitches spread across both halves of the bank, low cutoff, high resonance, pan center" BASS
ask "kick on steps 0,4,8,12,16,20,24,28 pan center, hihat on 2,6,10,14,18,22,26,30 pan 0.3, clap on 4,12,20,28 pan -0.3" DRUMS
ask "reverb mix 0.12, reverb size 0.5, delay mix 0.08, stereo_width 0.6" FX

say "Each agent only modified its own instruments."
wait_seconds 3

# ── Scene 5: Scoped changes ──────────────────────────────────────────────

scene "Scoped changes"

focus_on bass

ask "more resonance, darker, add slide on a few steps" BASS

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
