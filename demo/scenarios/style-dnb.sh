#!/usr/bin/env bash
# ─── Drum & Bass (Jump-up) Demo ──────────────────────────────────────────────
# 174 BPM, amen-break sampler as the drum backbone, 303-style bass voice
# as the reese melody, AN1X as a background drone pad (atmosphere, not a
# lead).  Drum break is regenerated twice.  MC shout-out at the tail.

scene_count 10

# ── Scene 1: Intro ──────────────────────────────────────────────────────────

scene "Intro"

reset_all
set_style drum_and_bass

say "Impulse Instruct — a synthesizer with a local AI producer. Today: jump-up drum and bass at 174 BPM."
wait_seconds 1

# ── Scene 2: Setup ──────────────────────────────────────────────────────────
# D&B is sample-heavy, so AmenSampler is the drum backbone.  808 layers a
# punchy kick.  Bass voice (303-style) is the main reese.  AN1X sits in the
# back as a drone pad.

scene "Setup"

# Capture IDs so we can explicitly cable amen through reverb for a wet
# break tail — classic edit-era D&B treatment.  The auto-wire from each
# voice/FX to master still runs, so amen also has a dry path to master.
amen_id=$(add_instrument amen)
add_instrument 808
add_instrument bass
add_instrument an1x
reverb_id=$(add_effect reverb)
add_effect delay

# Send amen into reverb for the wet tail (parallel to its dry path).
if [ -n "$amen_id" ] && [ -n "$reverb_id" ]; then
    api_rack_cable "$amen_id" "$reverb_id" "audio"
fi

say "Amen break sampler, 808 kick layer, bass, AN1X drone pad, reverb, delay. Amen fed through the reverb."

add_agent DNB gemma
wait_for_model

set_bpm 174

# ── Scene 3: First break ────────────────────────────────────────────────────

scene "First break"

look_at sequencer

play
wait_seconds 1

ask "set up classic jump-up drums: amen sampler driving the break (write sequencer.amen_steps as a 32-step array hitting on the main backbeat positions), and kit_a (808) adding a punchy kick on beat 1, beat 3, and just past beat 3.5 with a big snare on 2 and 4. Rolling 16th hats across the bar with open-hat accents on the off-beats."

say "Amen break plus 808 kick layer. Rolling hats on top."
wait_seconds 3

# ── Scene 4: Reese bass ─────────────────────────────────────────────────────

scene "Reese bass"

focus_on bass

ask "BASS voice plays a reese melody — detuned supersaw (5 voices, detune around 0.55), ladder filter cutoff around 0.4 and resonance 0.6, some sub-osc for weight. A minor pentatonic, 5 or 6 distinct pitches across the 32-step bar, syncopated, singable but heavy. This is the main melodic line."

say "303-style bass voice carrying the reese. Syncopated, singable."
wait_seconds 3

# ── Scene 5: AN1X drone pad ─────────────────────────────────────────────────

scene "AN1X drone pad"

focus_on an1x

ask "AN1X as a BACKGROUND drone pad only — NOT a melody. Sparse: 2 to 4 LONG sustained notes across the 32-step bar, high sustain, long release, dark filter. Should sit UNDER the drums and bass, barely noticeable at first. Two pitches minimum (root and fifth). No dense patterns, no notes on every other step."

say "AN1X drops back to atmosphere. Two or three long notes, under the mix."
wait_seconds 3

# ── Scene 6: Slice chop ─────────────────────────────────────────────────────
# Break-chopping territory — the amen voice is divided into 8 slices and
# each step can pick which slice to fire.  The agent writes sequencer.
# amen_slices (a parallel array to amen_steps) for real edit-era patterns.

scene "Slice chop"

focus_on amen

ask "chop the amen break: keep amen at 8 slices, write sequencer.amen_slices as a 32-step array of slice indices (1..=8) that rearranges the break into a classic jump-up edit. Vary the slice picks so the break sounds chopped rather than linear. Also set amen.gate around 0.85 and amen.stutter 0 for clean hits." "" 12

say "Slice chopping the amen — each step picks a different slice of the break."
wait_seconds 4

# ── Scene 7: Fresh break ────────────────────────────────────────────────────

scene "Fresh break"

look_at sequencer

ask "rewrite the amen + kit_a drums for variety — keep the jump-up feel and rolling hats, but shift the kick placement, mutate the amen_slices into a different chop, and add a snare ghost hit or fill. same tempo, same energy." "" 10

say "The agent rewrites the break. Same style, different phrasing."
wait_seconds 4

# ── Scene 8: Drop ───────────────────────────────────────────────────────────

scene "Drop"

look_at console

# Deterministic FX levels — don't ask the agent for dB values.
api_params '{"fx": {"reverb_mix": 0.14, "reverb_size": 0.45, "delay_mix": 0.1, "delay_time": 0.1875, "stereo_width": 0.65}}'

show_all

ask "drop — rewrite amen + kit_a harder and denser: 32nd-note hat rolls on the off-beats, a proper amen-break-style snare fill before the bar loops, punchier kick velocities. Open the bass filter a bit for more bite. Keep AN1X sparse and subdued." "" 14

say "Second rewrite of the break. Denser, harder, fill-heavy."
wait_seconds 4

# ── Scene 9: MC ─────────────────────────────────────────────────────────────
# Spawn via API (reliable).  NeuTTS server is left running by record-demo.sh
# now, so in-app MC synthesis actually renders audio this time.

scene "MC on the mic"

add_agent MC gemma "" mc tts
wait_seconds 3

ask "drop a single short jump-up shout-out, one line, peak-time rave energy" MC 10

say "MC steps up. Jump-up energy on the mic."
wait_seconds 30

# ── Scene 10: End ───────────────────────────────────────────────────────────

scene "End"

say "Jump-up D&B at 174 — amen break, reese bass, AN1X pad, MC on top. Everything ran locally on one GPU."
wait_seconds 3

stop
