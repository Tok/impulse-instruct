#!/usr/bin/env bash
# ─── Drum & Bass Demo ───────────────────────────────────────────────────────
# 170 BPM, rolling hats, reese bass, filter sweeps.

scene_count 7

# ── Scene 1: Setup ──────────────────────────────────────────────────────────

scene "Setup"

reset_rack

add_instrument bass
add_instrument 808
add_instrument 909
add_effect reverb
add_effect delay

say "Bass, two drum kits, reverb, delay."

add_agent DNB gemma
wait_for_model

set_bpm 170

# ── Scene 2: Tempo and kick ────────────────────────────────────────────────

scene "170 BPM foundation"

look_at sequencer

play
wait_seconds 1

ask "Style is drum and bass at 170 BPM. Two-step kick and snare pattern, fast hats."

say "Two-step pattern at 170. The agent picks the step placement."
wait_seconds 3

# ── Scene 3: Rolling hats ──────────────────────────────────────────────────

scene "Rolling hats"

focus_on 808

ask "rolling hi-hats, sixteenths, add some open hats for syncopation"

say "Continuous hat rolls. Characteristic of the genre."
wait_seconds 3

# ── Scene 4: Bass ──────────────────────────────────────────────────────────

scene "Reese bass"

focus_on bass

ask "deep reese bass, dark, sub-heavy, sparse pattern"

say "Sub bass. The agent sets filter and note placement."
wait_seconds 3

# ── Scene 5: Effects ───────────────────────────────────────────────────────

scene "Effects and filter sweep"

look_at console

ask "subtle delay and reverb, then slowly open the bass filter over 4 bars" "" 14

say "Filter ramp running. Builds tension."
wait_seconds 3

# ── Scene 6: Full energy ──────────────────────────────────────────────────

scene "Drop"

show_all

ask "drop it, full energy, all drums, heavy bass" "" 12

wait_seconds 4

# ── Scene 7: End ────────────────────────────────────────────────────────────

scene "End"

say "DnB workflow. High tempo, rolling patterns, filter builds."

stop
