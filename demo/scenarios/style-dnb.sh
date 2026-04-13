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

ask "Style is drum and bass at 170 BPM. Two-step on kit_a: kick on steps 0,20, snare on 8,24. Hihat on 2,6,10,14,18,22,26,30, pan 0.3. Kick/snare center."

say "Two-step pattern at 170. The agent picks the step placement."
wait_seconds 3

# ── Scene 3: Rolling hats ──────────────────────────────────────────────────

scene "Rolling hats"

focus_on 808

ask "rolling hi-hats, sixteenths across all 16 positions of kit_a, add open hats on 6,14,22,30 for syncopation, velocity variation between steps"

say "Continuous hat rolls. Characteristic of the genre."
wait_seconds 3

# ── Scene 4: Bass ──────────────────────────────────────────────────────────

scene "Reese bass"

focus_on bass

ask "deep reese bass, dark, sub-heavy, detuned supersaw, cutoff around 0.2, resonance 0.4, sparse pattern with 4 distinct scale pitches across both halves, slide between notes, pan center"

say "Sub bass. The agent sets filter and note placement."
wait_seconds 3

# ── Scene 5: Effects ───────────────────────────────────────────────────────

scene "Effects and filter sweep"

look_at console

# Deterministic FX levels via api_params — don't ask the agent to set dB values.
api_params '{"fx": {"reverb_mix": 0.18, "reverb_size": 0.55, "delay_mix": 0.14, "delay_time": 0.375, "stereo_width": 0.6}}'

ask "slowly open the bass filter over 4 bars, start near 0.2 and ramp to 0.75" "" 14

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
