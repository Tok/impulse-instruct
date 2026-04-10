#!/usr/bin/env bash
# ─── Intro Demo ─────────────────────────────────────────────────────────────
# Quick overview of Impulse Instruct. Sound within 30 seconds.
# Two parts: single agent, then multi-agent band.

scene_count 13

# ═══════════════════════════════════════════════════════════════════════════════
# PART 1: Quick start — rack + agent + sound
# ═══════════════════════════════════════════════════════════════════════════════

scene "Setup"

reset_rack

say "Impulse Instruct. AI agents controlling synthesizers in real time."

add_instrument bass
add_instrument 808
add_instrument 909
add_effect reverb
add_effect delay
add_agent PULSE gemma
wait_for_model

# ── Scene 2: First sound ────────────────────────────────────────────────────

scene "First sound"

look_at sequencer
play
wait_seconds 1

ask "acid house groove, four on the floor, melodic bass line with at least 4 different notes, set the filter interesting"

wait_seconds 3

say "One sentence. The AI programmed a full pattern."

# ── Scene 3: Tighten the beat ───────────────────────────────────────────────

scene "Tighten the beat"

ask "add a clap on two and four, rearrange the hi-hats, more groove"

focus_on 808
wait_seconds 3

say "Drums refined. Now let's work the filter."

# ── Scene 4: Acid pad — cutoff/resonance sweep ─────────────────────────────

scene "Acid filter sweep"

focus_on bass
wait_seconds 1

say "The filter. Cutoff and resonance. This is the acid sound."

sweep_pad 6

say "That's what the three oh three does."
wait_seconds 1

# ── Scene 5: Ramp the filter via AI ────────────────────────────────────────

scene "AI-controlled ramp"

ask "slowly sweep the filter open over 4 bars" 14

say "The AI ramps parameters over bars. Smooth, tempo-synced."
wait_seconds 2

# ── Scene 6: Show cables — brief ───────────────────────────────────────────

scene "Control cables"

show_cables
wait_seconds 3
say "Back panel. The agent is wired to every instrument."
show_knobs
wait_seconds 1

# ── Scene 7: Parameter locking ──────────────────────────────────────────────

scene "Parameter lock"

show_all
look_at console

lock "sequencer.bass_steps" "tb303.cutoff"

ask "strip it back, minimal techno, different drums"

say "Bass is locked. Only drums changed."
wait_seconds 2

unlock "sequencer.bass_steps" "tb303.cutoff"

# ═══════════════════════════════════════════════════════════════════════════════
# PART 2: Multi-agent band
# ═══════════════════════════════════════════════════════════════════════════════

scene "Multi-agent setup"

stop
wait_seconds 1

say "Split the AI into specialists. Each one controls its own instruments."

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

# Brief cable view
show_cables
wait_seconds 2
show_knobs
wait_seconds 1

# ── Scene 9: Band jam ─────────────────────────────────────────────────────

scene "Band jam"

play
wait_seconds 1

ask "acid bass, squelchy, syncopated, cutoff low, resonance high" BASS
ask "kick on steps 0,4,8,12. hihat on 2,6,10,14. clap on 4,12. open hihat on 6,14" DRUMS
ask "reverb mix 0.12, reverb size 0.5, delay mix 0.08, delay time 0.375" FX

wait_seconds 3

say "Three agents. Each handled its own part."

# ── Scene 10: Scoped control ─────────────────────────────────────────────

scene "Scoped control"

ask "more resonance, darker" BASS

focus_on bass
wait_seconds 2

say "Bass changed. Drums and FX untouched. That's agent scoping."

# ── Scene 11: Creative direction ──────────────────────────────────────────

scene "Full energy"

show_all

ask "more energy" BASS
ask "more energy, busier" DRUMS
ask "bigger space" FX 10

wait_seconds 4

# ── Scene 12: Acid pad with band ──────────────────────────────────────────

scene "Live filter"

focus_on bass
sweep_pad 6
wait_seconds 1

# ── Scene 13: Outro ───────────────────────────────────────────────────────

scene "End"

show_all

say "Impulse Instruct. Build your rack, wire AI agents, make music with words. Everything runs locally."

stop
