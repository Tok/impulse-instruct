#!/usr/bin/env bash
# ─── Intro Demo ─────────────────────────────────────────────────────────────
# Quick overview of Impulse Instruct. Sound within 30 seconds.
# Two parts: single agent, then multi-agent band.

scene_count 12

# ═══════════════════════════════════════════════════════════════════════════════
# PART 1: Quick start — rack + agent + sound
# ═══════════════════════════════════════════════════════════════════════════════

scene "Setup"

reset_rack

say "Impulse Instruct. AI agents controlling synthesizers in real time."

# Add everything at once — no one-by-one ceremony
add_instrument bass
add_instrument 808
add_instrument 909
add_effect reverb
add_effect delay
add_agent PULSE gemma
wait_for_model

# ── Scene 2: Start playing immediately ──────────────────────────────────────

scene "First sound"

play
wait_seconds 1

ask "acid house groove, four on the floor, melodic bass line with at least 4 different notes, set the filter interesting"

look_at sequencer
wait_seconds 3

say "One sentence. The AI built a full pattern."

# ── Scene 3: Show the bass detail + filter movement ─────────────────────────

scene "Filter in motion"

focus_on bass
wait_seconds 1

ask "slowly sweep the filter open over 4 bars, ramp cutoff up" 14

say "The filter is ramping. Visible on the knob."
wait_seconds 2

# ── Scene 4: Flip to back — show cables ─────────────────────────────────────

scene "Control cables"

show_all
show_cables

say "The back panel shows how the agent is wired to each instrument."
wait_seconds 3

show_knobs

# ── Scene 5: Tweak via prompt ──────────────────────────────────────────────

scene "Natural language control"

ask "more acid, push the resonance, syncopate the bass"

focus_on bass
wait_seconds 3

say "Every parameter is addressable by description."

# ── Scene 6: Parameter locking ──────────────────────────────────────────────

scene "Parameter lock"

lock "sequencer.bass_steps" "tb303.cutoff"

ask "strip it back, minimal, different drums"

say "Bass locked. Drums changed. That's parameter locking."
wait_seconds 2

unlock "sequencer.bass_steps" "tb303.cutoff"

# ═══════════════════════════════════════════════════════════════════════════════
# PART 2: Multi-agent band
# ═══════════════════════════════════════════════════════════════════════════════

scene "Multi-agent setup"

stop
wait_seconds 1

say "Now split the AI into specialists. Each controls its own instruments."

# Remove single agent, add specialists
reset_rack
add_instrument bass
add_instrument 808
add_instrument 909
add_effect reverb
add_effect delay
add_agent BASS bonsai bass
add_agent DRUMS bonsai "kit_a,kit_b"
add_agent FX bonsai fx
wait_for_model

# ── Scene 8: Show wiring ───────────────────────────────────────────────────

scene "Agent wiring"

show_cables
look_at console
wait_seconds 2

say "Each agent has separate control cables. BASS only touches the three oh three."

show_knobs

# ── Scene 9: Band jam ─────────────────────────────────────────────────────

scene "Band jam"

play
wait_seconds 1

ask "acid bass, squelchy, syncopated" BASS
ask "four on the floor, off-beat hats, clap on two and four" DRUMS
ask "subtle reverb and short delay" FX

look_at console
wait_seconds 3

say "Three agents. Each handled its own part."

# ── Scene 10: Independent evolution ───────────────────────────────────────

scene "Scoped control"

ask "more resonance, darker, add slide" BASS

focus_on bass
wait_seconds 2

say "Bass changed. Drums and FX untouched."

# ── Scene 11: Creative direction ──────────────────────────────────────────

scene "Creative direction"

show_all

ask "more energy" BASS
ask "more energy, busier" DRUMS
ask "bigger space" FX 10

wait_seconds 4

# ── Scene 12: Outro ───────────────────────────────────────────────────────

scene "End"

say "Impulse Instruct. Build your rack, wire AI agents, make music with words. Everything runs locally."

stop
