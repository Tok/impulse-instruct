#!/usr/bin/env bash
# ─── Techno Demo ────────────────────────────────────────────────────────────
# Minimal techno: kick-driven, sparse, hypnotic.

scene_count 7

# ── Scene 1: Setup ──────────────────────────────────────────────────────────

scene "Setup"

reset_rack

add_instrument bass
add_instrument 808
add_effect reverb
add_effect delay

say "Bass, eight oh eight, reverb, delay. Minimal setup."

add_agent TEKNO gemma
wait_for_model

# ── Scene 2: The kick ──────────────────────────────────────────────────────

scene "Kick foundation"

look_at sequencer

play
wait_seconds 1

ask "130 BPM, four on the floor kick, nothing else"

focus_on 808

say "Just the kick. Techno starts here."
wait_seconds 4

# ── Scene 3: Minimal percussion ────────────────────────────────────────────

scene "Minimal percussion"

ask "add sparse off-beat hats and a single clap, keep it minimal"

say "Minimal percussion. Only what's needed."
wait_seconds 3

# ── Scene 4: Bass ─────────────────────────────────────────────────────────

scene "Dark bass"

focus_on bass

ask "dark staccato bass, tight filter, aggressive"

say "Short, punchy bass notes under the kick."
wait_seconds 3

# ── Scene 5: Tension build ────────────────────────────────────────────────

scene "Building tension"

show_all

ask "build tension, slowly open the filter, add delay, stretch it over 8 bars" 16

say "Filter and delay rising over eight bars."
wait_seconds 3

screenshot "v${VERSION}-techno-tension"

# ── Scene 6: Release ─────────────────────────────────────────────────────

scene "Release"

ask "release, everything open, full drums, drive it" 12

wait_seconds 5

# ── Scene 7: End ────────────────────────────────────────────────────────────

scene "End"

say "Techno workflow. Kick-first, minimal, tension and release."

stop
