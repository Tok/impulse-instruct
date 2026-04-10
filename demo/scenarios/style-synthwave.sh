#!/usr/bin/env bash
# ─── Synthwave Demo ─────────────────────────────────────────────────────────
# AN1X pads, driving bass, 808 drums, TTS MC spitting neon poetry.

scene_count 8

# ── Scene 1: Setup ──────────────────────────────────────────────────────────

scene "Setup"

reset_rack

add_instrument bass
add_instrument an1x
add_instrument 808
add_effect reverb
add_effect delay
add_effect chorus
add_instrument tts

say "Bass, AN1X for pads, eight oh eight, chorus, reverb, delay, and a TTS voice module."

# Main agent for the music
add_agent SYNTH gemma

# MC agent in MC mode with TTS — will generate cheesy synthwave lines
add_agent NEON bonsai "" mc tts

wait_for_model

# ── Scene 2: Arpeggiated bass ──────────────────────────────────────────────

scene "Driving bass"

look_at sequencer

play
wait_seconds 1

ask "synthwave, 108 BPM, driving arpeggiated bass line, minor key"

focus_on bass

say "Arpeggiated bass. The agent picks the pattern and filter settings."
wait_seconds 3

# ── Scene 3: AN1X pad ──────────────────────────────────────────────────────

scene "Pad layer"

focus_on an1x

ask "warm analog pad, detuned oscillators, slow attack, cinematic"

say "AN1X pad with detuned oscillators for width."
wait_seconds 4

# ── Scene 4: Drums ──────────────────────────────────────────────────────────

scene "808 drums"

focus_on 808

ask "gated snare on two and four, steady kick, eighth note hats"

say "Gated reverb snare is the signature sound."
wait_seconds 3

# ── Scene 5: Effects ───────────────────────────────────────────────────────

scene "Chorus and reverb"

ask "big chorus on the pad, long cinematic reverb, tempo synced delay"

say "Chorus, reverb, and delay. Wide stereo image."
wait_seconds 3

# ── Scene 6: The MC ────────────────────────────────────────────────────────

scene "Neon MC"

look_at console

say "The NEON agent runs in MC mode with TTS. It generates lines and speaks them."

ask "chrome sunset, neon grid, digital highway, palm trees in the rain" NEON

wait_seconds 8

say "The Bonsai model generates text, espeak renders it. Pitch snap quantises to the key."
wait_seconds 3

# ── Scene 7: Full scene ───────────────────────────────────────────────────

scene "Full scene"

show_all

ask "more movement, open the filter, build it up"
wait_seconds 5

ask "electric dreams, laser horizon, the city never sleeps" NEON

wait_seconds 8

# ── Scene 8: End ────────────────────────────────────────────────────────────

scene "End"

say "Synthwave demo. AN1X pads, arpeggiated bass, and a Bonsai MC narrating in TTS."

stop
