#!/usr/bin/env bash
# ─── Screenshot Capture ─────────────────────────────────────────────────────
# Sets up various rack configurations and captures screenshots for README
# and documentation. Run with --skip-video --skip-narration.
#
# Usage: ./demo/record-demo.sh --scenario screenshots --skip-video --skip-narration

VERSION="v0.7.1"

scene_count 7

# ── Scene 1: Rack front — full acid setup ───────────────────────────────────

scene "Front panel — acid setup"

reset_rack
add_instrument bass
add_instrument 808
add_instrument 909
add_effect reverb
add_effect delay
add_agent ACID gemma
wait_for_model

play
wait_seconds 1
ask "classic acid groove, four on the floor, squelchy bass"

show_all
look_at sequencer
wait_seconds 2

screenshot "${VERSION}-front-sequencer"

focus_on bass
wait_seconds 1
screenshot "${VERSION}-front-bass-detail"

# ── Scene 2: Rack back — control cables ────────────────────────────────────

scene "Back panel — cables"

show_all
show_cables
look_at console
wait_seconds 1
screenshot "${VERSION}-back-cables-overview"

look_at bass
wait_seconds 1
screenshot "${VERSION}-back-cables-detail"

show_knobs

# ── Scene 3: Multi-agent band ──────────────────────────────────────────────

scene "Multi-agent setup"

stop
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

play
wait_seconds 1
ask "acid bass, squelchy" BASS
ask "four on the floor, off-beat hats" DRUMS
ask "subtle reverb and delay" FX

show_all
look_at console
wait_seconds 2
screenshot "${VERSION}-multi-agent-console"

show_cables
wait_seconds 1
screenshot "${VERSION}-multi-agent-cables"

show_knobs

# ── Scene 4: Effects zone ─────────────────────────────────────────────────

scene "Effects detail"

focus_on reverb
wait_seconds 1
screenshot "${VERSION}-fx-zone"

# ── Scene 5: 808 drums ───────────────────────────────────────────────────

scene "808 drum pattern"

focus_on 808
wait_seconds 1
screenshot "${VERSION}-808-pattern"

# ── Scene 6: AN1X + ambient ──────────────────────────────────────────────

scene "AN1X pad — ambient"

stop
reset_rack
add_instrument an1x
add_effect reverb
add_effect delay
add_agent AMBIENT gemma
wait_for_model

play
wait_seconds 1
ask "warm ambient pad, slow, dreamy"

focus_on an1x
wait_seconds 2
screenshot "${VERSION}-an1x-ambient"

# ── Scene 7: Synthwave with TTS MC ──────────────────────────────────────

scene "Synthwave + TTS"

stop
reset_rack
add_instrument bass
add_instrument an1x
add_instrument 808
add_effect reverb
add_effect chorus
add_agent SYNTH gemma
add_agent NEON bonsai "" mc tts
wait_for_model

play
wait_seconds 1
ask "synthwave, arpeggiated bass, warm pad"

show_all
look_at console
wait_seconds 2
screenshot "${VERSION}-synthwave-tts"

stop

echo ""
echo "  Screenshots saved to assets/screenshots/"
ls -1 assets/screenshots/${VERSION}-*.png 2>/dev/null | sed 's/^/    /'
