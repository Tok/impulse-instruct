#!/usr/bin/env bash
# ─── Jungle Demo ────────────────────────────────────────────────────────────
# 165 BPM, chopped amen break, sub bass, NeuTTS MC shouting selector lines.

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

# Main agent for the music
add_agent RAGGA gemma

# MC agent in MC mode with TTS — auto-adds a TTS module and wires it
add_agent SELECTA bonsai "" mc tts

wait_for_model

set_bpm 165

# ── Scene 2: Tempo and break ────────────────────────────────────────────────

scene "165 BPM amen feel"

look_at sequencer

play
wait_seconds 1

ask "Style is jungle at 165 BPM. Chopped amen-style break on kit_a: kick on steps 0,10,16,26, snare on 4,12,20,28, ghost snares on 7,15,23,31, hihat on 2,6,10,14,18,22,26,30. Pan hihat 0.3, clap -0.3, kick/snare center."

say "Jungle break. Fast chopped drums."
wait_seconds 3

# ── Scene 3: Sub bass ──────────────────────────────────────────────────────

scene "Sub bass"

focus_on bass

ask "deep sub bass, sine-heavy waveform, sparse pattern with gaps, low cutoff around 0.25, resonance low, slide on several steps. 4 distinct scale pitches spread across BOTH halves of the bank, not root-only"

say "Sub bass. Rolling and sparse."
wait_seconds 3

# ── Scene 4: The selector ──────────────────────────────────────────────────

scene "Selector on the mic"

look_at console

say "The SELECTA agent runs in MC mode with NeuTTS. Classic jungle calls."

ask "shout 'jungle selector massive' then 'rewind'" SELECTA
wait_seconds 6

ask "shout 'pull up selector' and 'big up the crew'" SELECTA
wait_seconds 6

# ── Scene 5: Effects build ─────────────────────────────────────────────────

scene "Delay and reverb"

look_at console

api_params '{"fx": {"delay_mix": 0.22, "delay_time": 0.375, "reverb_mix": 0.18, "reverb_size": 0.6}}'

say "Tape delay on the snares. Plate reverb on the hats."
wait_seconds 3

# ── Scene 6: Drop ──────────────────────────────────────────────────────────

scene "Drop"

show_all

ask "drop it, full energy, more bass, busier hats, open the filter over 4 bars" "" 12

ask "shout 'here comes the bass' then a long 'booyaka'" SELECTA

wait_seconds 8

# ── Scene 7: End ────────────────────────────────────────────────────────────

scene "End"

say "Jungle workflow. Chopped breaks, sub bass, and a selector on the mic."

stop
