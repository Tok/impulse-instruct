#!/usr/bin/env bash
# ─── Odd Time Signatures Demo ───────────────────────────────────────────────
# 3/4, 5/4, 7/8 — non-standard grooves.

scene_count 7

# ── Scene 1: Setup ──────────────────────────────────────────────────────────

scene "Setup"

reset_rack
add_instrument bass
add_instrument 808
add_effect reverb
add_agent PULSE gemma
wait_for_model

# ── Scene 2: Waltz — 3/4 ───────────────────────────────────────────────────

scene "3/4 waltz time"

look_at sequencer
play
wait_seconds 1

ask "set time signature to 3/4, 100 BPM, waltz feel, bass on beat 1 and 3, kick on 1, hihat on 2 and 3"

wait_seconds 4

say "Three four time. The grid adjusts."
wait_seconds 3

# ── Scene 3: Progressive — 5/4 ────────────────────────────────────────────

scene "5/4 progressive"

ask "switch to 5/4 time signature, keep the pattern interesting, 110 BPM"

wait_seconds 4

say "Five four. Uneven but steady."
wait_seconds 3

# ── Scene 4: Math rock — 7/8 ─────────────────────────────────────────────

scene "7/8"

ask "7/8 time, faster at 130 BPM, syncopated bass, asymmetric kick pattern"

wait_seconds 4

say "Seven eight. The beat shifts."
wait_seconds 3

# ── Scene 5: Back to 4/4 ────────────────────────────────────────────────

scene "Return to 4/4"

ask "back to 4/4, 125 BPM, standard house groove"

wait_seconds 3

say "Four four. Home."
wait_seconds 2

# ── Scene 6: Mixed feel ──────────────────────────────────────────────────

scene "Polyrhythm"

ask "keep 4/4 but set hihat to 12 steps and kick to 16 for polyrhythm"

wait_seconds 5

say "Different loop lengths on the same grid. Polyrhythm."
wait_seconds 2

# ── Scene 7: End ─────────────────────────────────────────────────────────

scene "End"

say "Time signatures. Three four, five four, seven eight, polyrhythm. The sequencer handles all of them."

stop
