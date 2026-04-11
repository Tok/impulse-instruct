#!/usr/bin/env bash
# ─── Bach Demo ──────────────────────────────────────────────────────────────
# AN1X only. No drums, no bass synth. Classical counterpoint on a synthesizer.

scene_count 7

# ── Scene 1: Setup ──────────────────────────────────────────────────────────

scene "Setup"

reset_rack

add_instrument an1x
add_effect reverb

say "AN1X and reverb. No drums, no bass synth. Just a keyboard and a room."

add_agent BACH gemma
wait_for_model

# ── Scene 2: Simple melody ─────────────────────────────────────────────────

scene "Simple melody"

look_at sequencer

play
wait_seconds 1

ask "play a simple Bach-style melody in C major, slow tempo around 90 BPM, clean tone"

focus_on an1x

say "Single voice melody. The agent picks notes from the scale."
wait_seconds 4

# ── Scene 3: Tone shaping ─────────────────────────────────────────────────

scene "Tone shaping"

ask "warmer tone, more like a harpsichord, short decay, bright attack"

say "Filter and envelope adjusted. Closer to a plucked keyboard sound."
wait_seconds 4

# ── Scene 4: Counterpoint ─────────────────────────────────────────────────

scene "Adding voices"

ask "add a second voice, lower register, counterpoint style, moving in contrary motion"

say "Two voices. The agent tries to follow counterpoint rules from the prompt context."
wait_seconds 5

# ── Scene 5: Reverb as room ──────────────────────────────────────────────

scene "Room acoustics"

focus_on reverb

ask "cathedral reverb, long tail, like a stone church"

say "Reverb simulating a large acoustic space. Changes the character entirely."
wait_seconds 4

# ── Scene 6: Variation ───────────────────────────────────────────────────

scene "Variation"

show_all

ask "vary the melody, more ornamentation, keep the counterpoint"

wait_seconds 5

say "The agent reinterprets the pattern. Each generation is different."
wait_seconds 3

# ── Scene 7: End ────────────────────────────────────────────────────────────

scene "End"

say "Bach demo. Single instrument, no percussion, classical phrasing on a synth."

stop
