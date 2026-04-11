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

say "Impulse Instruct. AI-controlled synthesizers. Let's build a rack."

add_instrument bass
add_instrument 808
add_instrument 909
add_effect reverb
add_effect delay

say "Bass, drums, reverb, delay. Adding an AI agent."

add_agent PULSE gemma
wait_for_model

# ── Scene 2: First sound ────────────────────────────────────────────────────

scene "First sound"

look_at sequencer

say "Asking the agent for a pattern."

# Send prompt BEFORE playing — pattern loads while sequencer is stopped.
ask "acid groove, kick and hats, short bass line with gaps. set pan positions for stereo width, add subtle chorus"

play
wait_seconds 3

# ── Scene 3: Tighten the beat ───────────────────────────────────────────────

scene "Tighten the beat"

ask "add a clap on two and four, a couple more bass notes, set an accent on beat one"

focus_on 808
wait_seconds 3

say "Drums refined. Now the filter."

# ── Scene 4: Acid pad — cutoff/resonance sweep ─────────────────────────────

scene "Acid filter sweep"

focus_on bass
wait_seconds 1

say "Cutoff and resonance."

sweep_pad 8

wait_seconds 1

# ── Scene 5: Ramp the filter via AI ────────────────────────────────────────

scene "AI-controlled ramp"

ask "slowly sweep the filter open over 4 bars" 14

say "The AI ramps parameters over bars. Smooth, tempo-synced."
wait_seconds 2

# ── Scene 6: Show cables — after wiring is visible ─────────────────────────

scene "Control cables"

# Scroll to console area where the agent sits, then flip
look_at console
wait_seconds 1
show_cables
wait_seconds 3
say "Back panel. Control cables from the agent to each instrument."
show_knobs
wait_seconds 1

# ── Scene 7: Parameter locking — scroll to bass first ──────────────────────

scene "Parameter lock"

focus_on bass
wait_seconds 1

lock "sequencer.bass_steps" "tb303.cutoff"

say "Locking bass cutoff and pattern."

look_at console
ask "strip it back, minimal techno, different drums"

focus_on bass
wait_seconds 2

say "Bass stayed locked. Only drums changed."

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
look_at bass
wait_seconds 0.5
add_instrument 808
add_instrument 909
look_at 808
wait_seconds 0.5
add_effect reverb
add_effect delay

# Add agents and scroll to console to show them appearing
look_at console
wait_seconds 0.5
add_agent BASS bonsai bass
wait_seconds 0.5
add_agent DRUMS bonsai "kit_a,kit_b"
wait_seconds 0.5
add_agent FX bonsai fx
wait_for_model

# Brief cable view — agents are now wired, so cables are visible
show_cables
wait_seconds 3
say "Each agent has its own control cables."
show_knobs
wait_seconds 1

# ── Scene 9: Band jam ─────────────────────────────────────────────────────

scene "Band jam"

play
wait_seconds 1

ask "acid bass, squelchy, cutoff low, resonance high, pan center" BASS
ask "kick on steps 0,4,8,12 pan center. hihat on 2,6,10,14 pan 0.3. clap on 4,12 pan -0.3. open hihat on 6,14" DRUMS
ask "reverb mix 0.12, reverb size 0.5, delay mix 0.08, chorus mix 0.1, stereo_width 0.6" FX

look_at console
wait_seconds 3

say "Three agents. Each handled its own part."

# ── Scene 10: Scoped control ─────────────────────────────────────────────

scene "Scoped control"

ask "more resonance, darker, add slide" BASS

focus_on bass
wait_seconds 2

say "Bass changed. Drums and FX untouched."

# ── Scene 11: Creative direction ──────────────────────────────────────────

scene "Full energy"

show_all

ask "more energy" BASS
ask "more energy, busier" DRUMS
ask "bigger space" FX 10

wait_seconds 4

# ── Scene 12: Live filter with the band ──────────────────────────────────

scene "Live filter"

focus_on bass
sweep_pad 8
wait_seconds 1

# ── Scene 13: Outro ───────────────────────────────────────────────────────

scene "End"

show_all

say "Impulse Instruct. Build your rack, wire AI agents, make music with words. Everything runs locally."

stop
